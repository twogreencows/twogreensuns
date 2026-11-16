use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use chrono::{Local, NaiveTime, Utc};
use reqwest::Client;
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info, Span};

use crate::application::TGSApplication;
use crate::blecar::{TGSTeslaCarCommand, TGSTeslaState, TGSBleCarRequest, NEXT_BLE_REQUEST_ID};
use crate::configuration::TGSDispatcherConfiguration;
use crate::fronius::{fronius_fetch_data, TGSInverterDataEntry};

pub async fn dispatcher_run(state: Arc<TGSApplication>) {
    debug!("[dispatcher:dispatcher_run] Starting dispatcher");

    // Block until the Fronius inverter address is known.  The Notify fires as
    // soon as fronius_discovery_run writes a non-empty address.  If discovery
    // already succeeded before we reach this point the stored permit means
    // notified() returns immediately without blocking.
    if state.runtime.read().await.tgsdevice_inverter_ipv4_address.is_empty() {
        info!(" ==> No inverter address yet — waiting for mDNS discovery...");
        state.inverter_address_notify.notified().await;
        info!(" ==> Inverter address received, starting poll loop");
    }

    let client = Client::new();
    loop {
        let (base_url, interval_seconds) = {
            let runtime = state.runtime.read().await;
            let base_url = format!("http://{}", runtime.tgsdevice_inverter_ipv4_address);
            let interval_seconds = runtime.config.polling_interval_seconds;
            (base_url, interval_seconds)
        };

        // Fetch the current car state and the inverter data in parallel.
        let (tesla, fronius) = tokio::join!(
            request_tesla(&state, TGSBleCarRequest::ReadState),
            fronius_fetch_data(&client, &base_url),
        );
        let now = Utc::now();

        match fronius {
            Ok(data) => {
                info!(
                    " --> [{}] Polling inverter with result AC_VOLTAGE={} P_PV={:.0}W P_Load={:.0}W P_Grid={:.0}W P_Akku={:.0}W",
                    now.format("%Y-%m-%d %H:%M:%S UTC"), data.ac_voltage, data.p_pv, data.p_load, data.p_grid, data.p_akku
                );

                // While bypass is on, the dispatcher decides and records its actions but never sends them to the car.
                let (actions, bypass) = {
                    let runtime = state.runtime.read().await;
                    (dispatcher_decide_actions(&data, tesla.as_ref(), &runtime.config, now.with_timezone(&Local).time()), runtime.config.bypass)
                };

                if let Err(e) = state.storage.write_event(now, &data, tesla.as_ref(), &actions) {
                    error!("[dispatcher:dispatcher_run] storage write failed: {}", e);
                }

                debug!("  --> updating inverter state");
                *state.inverter_state.write().await = Some(data);

                if actions.is_empty() {
                    debug!("  --> no Tesla action required");
                } else if bypass {
                    info!("  --> BYPASS, would send Tesla commands {:?}", actions);
                } else {
                    for action in actions {
                        debug!("  --> sending Tesla command {:?}", action);
                        // The BLE task mirrors `tesla_state` on every request.
                        let _ = request_tesla(&state, TGSBleCarRequest::Action(action)).await;
                    }
                }
            }
            Err(e) => error!("[dispatcher:dispatcher_run] Inverter fetch error: {}", e),
        }

        let sleep_duration = if interval_seconds == 300 || interval_seconds == 600 {
            // Align on the wall clock (:00, :05, :10... or :00, :10, :20...) so the logs and events are easier to read.
            let interval_millis = interval_seconds as i64 * 1000;
            Duration::from_millis((interval_millis - Utc::now().timestamp_millis() % interval_millis) as u64)
        } else {
            Duration::from_secs(interval_seconds as u64)
        };
        debug!("  --> sleeping {:?} before next poll (inverter = {})", sleep_duration, base_url);
        sleep(sleep_duration).await;
    }
}

/// Send a request to the BLE task and wait for the resulting Tesla state.
/// Returns `None` when the BLE task is unavailable (e.g. no VIN configured) or
/// the request failed — the caller treats that as "no car".
///
/// Runs in a `ble_request` span (unique `id`, `source = "dispatcher"`) that is sent along with the
/// request, so the BLE task logs the whole trace of this request under the same id.
#[tracing::instrument(name = "ble_request", skip_all, fields(id = NEXT_BLE_REQUEST_ID.fetch_add(1, Ordering::Relaxed), source = "dispatcher"))]
async fn request_tesla(state: &Arc<TGSApplication>, req: TGSBleCarRequest) -> Option<TGSTeslaState> {
    debug!("[dispatcher:request_tesla] Enter {:?}", req);
    let started = Instant::now();
    let (reply_tx, reply_rx) = oneshot::channel();
    if state.blecar_commandchannel_tx.send((req, reply_tx, Span::current())).await.is_err() {
        debug!("[dispatcher:request_tesla] BLE task unavailable, request not sent");
        return None;
    }
    debug!("  --> request queued, waiting for the BLE reply");
    match reply_rx.await {
        Ok(Ok(tesla_state)) => {
            debug!("[dispatcher:request_tesla] Exit OK after {:?}, state {:?}", started.elapsed(), tesla_state);
            Some(tesla_state)
        }
        Ok(Err(e)) => {
            debug!("[dispatcher:request_tesla] Exit with Tesla error after {:?}: {}", started.elapsed(), e);
            None
        }
        Err(_) => {
            debug!("[dispatcher:request_tesla] Exit, BLE task dropped the reply after {:?}", started.elapsed());
            None
        }
    }
}

/// Charging amps to request, from the solar power and the battery state.
///
/// STUB: returns a fixed 5A placeholder (capped by `max_amps`) whatever the sun and the battery.
/// The real computation will use the solar excess (`data.p_pv`, `data.p_load`, `data.ac_voltage`)
/// and the battery state of charge (`data.battery_soc`), for example lower amps while the battery
/// is still low.
fn dispatcher_compute_target_amps(_data: &TGSInverterDataEntry, max_amps: f64) -> f64 {
    // TODO: placeholder value, replace with the solar and battery based computation.
    5.0_f64.min(max_amps)
}

/// Decide which charging commands (if any) to send to the car this cycle.
///
/// `tesla` is the freshly-read car state, or `None` when the car is unreachable.
///
/// Algorithm (the car's own charging state is the memory of the hysteresis, nothing is stored):
///
/// 1. Battery hysteresis, on the state of charge (0.0 - 1.0):
///    - car charging and soc < 0.15: stop charging, whatever the solar production.
///    - car idle and soc <= 0.20: do nothing, the battery has not recovered enough to start.
///    - otherwise (charging with soc >= 0.15, or idle with soc > 0.20): go to rule 2.
/// 2. Solar: the amps come from `dispatcher_compute_target_amps` (solar power and battery state).
///    If amps >= 1: set the charging amps (truncated to a whole number) and start charging if the
///    car is idle.
/// 3. Otherwise (no solar, or less than 1A): stop if charging, else do nothing.
pub fn dispatcher_decide_actions( inverter_data: &TGSInverterDataEntry, tesla: Option<&TGSTeslaState>, 
            cfg: &TGSDispatcherConfiguration, now: NaiveTime) -> Vec<TGSTeslaCarCommand> {

    debug!("[dispatcher:dispatcher_decide_actions] Enter {:?} {:?}", now, inverter_data);
    let max_amps = cfg.tesla_max_charging_amps as f64;
    let charging_now = tesla.map(|t| t.charging).unwrap_or(false);

    let mut actions = Vec::new();

    // Protect the home battery: below 15% SoC (fraction 0.15), only charge the car inside the night window.
    if inverter_data.battery_soc < 15.0 {
        // TEMPORARY: hard-coded night window (local time), edit these two values to test without waiting.
        let night_start = NaiveTime::from_hms_opt(1, 6, 0).unwrap();
        let night_end = NaiveTime::from_hms_opt(7, 6, 0).unwrap();
        //let night_start = NaiveTime::from_hms_opt(21, 30, 0).unwrap();
        //let night_end = NaiveTime::from_hms_opt(22, 12, 0).unwrap();
        let in_window = now >= night_start && now < night_end;
        debug!("  --> battery SoC {:.2} < 0.15, night window active: {}", inverter_data.battery_soc, in_window);

        let below_limit = tesla.map(|t| t.current_charge_level < t.charge_limit_percent).unwrap_or(false);
        if in_window && !charging_now && below_limit {
            actions.push(TGSTeslaCarCommand::StartCharging);
        } else if !in_window && charging_now {
            actions.push(TGSTeslaCarCommand::StopCharging);
        }
        debug!("[dispatcher:dispatcher_decide_actions] Exit");
        return actions;
    }

    if inverter_data.p_pv > 0.0 {
        let target_amps = dispatcher_compute_target_amps(inverter_data, max_amps);
        debug!("  --> solar active: target={:.2}A max={:.0}A", target_amps, max_amps);


        if target_amps > 0.0 {
            actions.push(TGSTeslaCarCommand::SetChargingAmps(target_amps as u8));
            if !charging_now {
                actions.push(TGSTeslaCarCommand::StartCharging);
            }
            debug!("[dispatcher:dispatcher_decide_actions] Exit");
            return actions;
        } else {
            if charging_now {
                actions.push(TGSTeslaCarCommand::StopCharging);
            }
        }
    }

    debug!("[dispatcher:dispatcher_decide_actions] Exit");
    actions
}

#[cfg(test)]
mod tests {
    //! Rules of `dispatcher_decide_actions`, evaluated in this order:
    //!
    //! 1. Battery protection: state of charge (0.0 - 1.0) below 0.15 means the car must never
    //!    charge. Stop it if it is charging, otherwise do nothing, whatever the solar production.
    //! 2. Solar excess: when the panels produce, excess = p_pv - |p_load| and
    //!    amps = excess / ac_voltage, clamped between 0 and the configured maximum.
    //!    If amps >= 1: set the charging amps (truncated to a whole number) and start charging
    //!    if the car is idle.
    //! 3. Otherwise (no solar, or less than 1A of excess): stop if charging, else do nothing.

    use super::*;
    use chrono::Utc;

    fn build_inverter_data(p_pv: f64, p_load: f64, battery_soc: f64) -> TGSInverterDataEntry {
        TGSInverterDataEntry {
            ac_voltage: 230.0,
            p_pv,
            p_load,
            p_grid: 0.0,
            p_akku: 0.0,
            battery_soc,
            timestamp: Utc::now(),
        }
    }

    // Rule 1: battery at 10% (below 0.15) while the car charges. Solar excess (3000W - 500W) is
    // ignored, the only action is to stop the car.
    #[test]
    fn low_battery_stops_charging_even_with_solar_excess() {
        let inverter_data = build_inverter_data(3000.0, 500.0, 0.10);
        let tesla_state = TGSTeslaState { charging: true, ..Default::default() };
        let configuration = TGSDispatcherConfiguration { tesla_max_charging_amps: 16, ..Default::default() };
        let current_time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        let actions = dispatcher_decide_actions(&inverter_data, Some(&tesla_state), &configuration, current_time);

        assert_eq!(actions, vec![TGSTeslaCarCommand::StopCharging]);
    }

    // Rule 1: battery at 10% while the car is idle. Nothing to stop, and solar excess must not
    // start the charge either.
    #[test]
    fn low_battery_does_not_start_charging_even_with_solar_excess() {
        let inverter_data = build_inverter_data(3000.0, 500.0, 0.10);
        let tesla_state = TGSTeslaState { charging: false, ..Default::default() };
        let configuration = TGSDispatcherConfiguration { tesla_max_charging_amps: 16, ..Default::default() };
        let current_time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        let actions = dispatcher_decide_actions(&inverter_data, Some(&tesla_state), &configuration, current_time);

        assert!(actions.is_empty());
    }

    // Rule 1 boundary: at exactly 0.15 the battery is NOT considered low (the test is strictly
    // `< 0.15`), so the solar rule applies: 2500W / 230V = 10.9A, truncated to 10A.
    #[test]
    fn battery_at_exactly_threshold_does_not_stop_charging() {
        let inverter_data = build_inverter_data(3000.0, 500.0, 0.15);
        let tesla_state = TGSTeslaState { charging: true, ..Default::default() };
        let configuration = TGSDispatcherConfiguration { tesla_max_charging_amps: 16, ..Default::default() };
        let current_time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        let actions = dispatcher_decide_actions(&inverter_data, Some(&tesla_state), &configuration, current_time);

        assert_eq!(actions, vec![TGSTeslaCarCommand::SetChargingAmps(10)]);
    }

    // Rule 1 hysteresis, not implemented yet: the car is charging and the battery is at 17%, inside
    // the 0.15 - 0.20 band. Charging must continue (only a drop below 0.15 stops it), with the
    // amps coming from `dispatcher_compute_target_amps`.
    #[test]
    #[ignore = "hysteresis not implemented yet"]
    fn battery_inside_band_keeps_charging_car_charging() {
        todo!("charging, soc 0.17, solar excess: expect SetChargingAmps and no StopCharging");
    }

    // Rule 1 hysteresis, not implemented yet: the car is idle and the battery is at 17%, inside the
    // band. The battery has not recovered above 0.20, so the car must not start, even with solar
    // excess.
    #[test]
    #[ignore = "hysteresis not implemented yet"]
    fn battery_inside_band_does_not_start_idle_car() {
        todo!("idle, soc 0.17, solar excess: expect no action");
    }

    // Rule 1 hysteresis, not implemented yet: the car is idle and the battery has recovered to 25%
    // (above 0.20), so the solar rule applies and the car is started.
    #[test]
    #[ignore = "hysteresis not implemented yet"]
    fn battery_above_start_threshold_starts_idle_car() {
        todo!("idle, soc 0.25, solar excess: expect SetChargingAmps then StartCharging");
    }

    // Rule 1 boundary, not implemented yet: the idle car at exactly 0.20 must not start (the start
    // needs soc strictly above 0.20), while the same car at 0.21 must start.
    #[test]
    #[ignore = "hysteresis not implemented yet"]
    fn battery_at_exactly_start_threshold_does_not_start_idle_car() {
        todo!("idle, soc 0.20 then 0.21, solar excess: expect no action then start");
    }

    // Rule 2, not implemented yet: the amps depend on the battery state as well as the sun, so the
    // same solar excess must give fewer amps with a low battery than with a full one. The exact
    // curve is still to be defined in `dispatcher_compute_target_amps`.
    #[test]
    #[ignore = "amps depending on the battery state not implemented yet"]
    fn target_amps_are_lower_with_a_low_battery_than_with_a_full_one() {
        todo!("same solar excess, soc 0.25 vs soc 0.90: expect fewer amps at 0.25");
    }

    // Rule 2: 3000W produced, 500W consumed by the house, so 2500W / 230V = 10.9A, truncated to
    // 10A. The car is idle, so it must also be started, after its amps are set.
    #[test]
    fn solar_excess_sets_amps_then_starts_idle_car() {
        let inverter_data = build_inverter_data(3000.0, 500.0, 0.50);
        let tesla_state = TGSTeslaState { charging: false, ..Default::default() };
        let configuration = TGSDispatcherConfiguration { tesla_max_charging_amps: 16, ..Default::default() };
        let current_time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        let actions = dispatcher_decide_actions(&inverter_data, Some(&tesla_state), &configuration, current_time);

        assert_eq!(actions, vec![TGSTeslaCarCommand::SetChargingAmps(10), TGSTeslaCarCommand::StartCharging]);
    }

    // Rule 2: same excess as above, but the car already charges, so only the amps are adjusted.
    #[test]
    fn solar_excess_only_adjusts_amps_when_car_already_charging() {
        let inverter_data = build_inverter_data(3000.0, 500.0, 0.50);
        let tesla_state = TGSTeslaState { charging: true, ..Default::default() };
        let configuration = TGSDispatcherConfiguration { tesla_max_charging_amps: 16, ..Default::default() };
        let current_time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        let actions = dispatcher_decide_actions(&inverter_data, Some(&tesla_state), &configuration, current_time);

        assert_eq!(actions, vec![TGSTeslaCarCommand::SetChargingAmps(10)]);
    }

    // Rule 2: 10000W - 500W = 9500W / 230V = 41A, but the configured maximum is 8A.
    #[test]
    fn solar_excess_is_clamped_to_configured_max_amps() {
        let inverter_data = build_inverter_data(10000.0, 500.0, 0.50);
        let tesla_state = TGSTeslaState { charging: true, ..Default::default() };
        let configuration = TGSDispatcherConfiguration { tesla_max_charging_amps: 8, ..Default::default() };
        let current_time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        let actions = dispatcher_decide_actions(&inverter_data, Some(&tesla_state), &configuration, current_time);

        assert_eq!(actions, vec![TGSTeslaCarCommand::SetChargingAmps(8)]);
    }

    // Rule 3: no solar production at all while the car charges, so it must be stopped.
    #[test]
    fn no_solar_stops_charging_car() {
        let inverter_data = build_inverter_data(0.0, 500.0, 0.50);
        let tesla_state = TGSTeslaState { charging: true, ..Default::default() };
        let configuration = TGSDispatcherConfiguration { tesla_max_charging_amps: 16, ..Default::default() };
        let current_time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        let actions = dispatcher_decide_actions(&inverter_data, Some(&tesla_state), &configuration, current_time);

        assert_eq!(actions, vec![TGSTeslaCarCommand::StopCharging]);
    }

    // Rule 3: no solar production and the car is idle, so there is nothing to do.
    #[test]
    fn no_solar_does_nothing_when_car_idle() {
        let inverter_data = build_inverter_data(0.0, 500.0, 0.50);
        let tesla_state = TGSTeslaState { charging: false, ..Default::default() };
        let configuration = TGSDispatcherConfiguration { tesla_max_charging_amps: 16, ..Default::default() };
        let current_time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        let actions = dispatcher_decide_actions(&inverter_data, Some(&tesla_state), &configuration, current_time);

        assert!(actions.is_empty());
    }

    // Rule 3: panels produce 100W but the house consumes 500W, so the excess is negative
    // (clamped to 0A, below the 1A minimum) and the charging car must be stopped.
    #[test]
    fn solar_below_house_load_stops_charging_car() {
        let inverter_data = build_inverter_data(100.0, 500.0, 0.50);
        let tesla_state = TGSTeslaState { charging: true, ..Default::default() };
        let configuration = TGSDispatcherConfiguration { tesla_max_charging_amps: 16, ..Default::default() };
        let current_time = NaiveTime::from_hms_opt(12, 0, 0).unwrap();

        let actions = dispatcher_decide_actions(&inverter_data, Some(&tesla_state), &configuration, current_time);

        assert_eq!(actions, vec![TGSTeslaCarCommand::StopCharging]);
    }
}
