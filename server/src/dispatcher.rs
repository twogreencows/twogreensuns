use std::sync::Arc;

use chrono::{Local, NaiveTime, Utc};
use reqwest::Client;
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info};

use crate::application::TGSApplication;
use crate::blecar::{TGSTeslaCarCommand, TGSTeslaState, TGSBleCarRequest};
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

        match fronius {
            Ok(data) => {
                info!(
                    " --> [{}] Polling inverter with result AC_VOLTAGE={} P_PV={:.0}W P_Load={:.0}W P_Grid={:.0}W P_Akku={:.0}W",
                    Local::now().format("%Y-%m-%d %H:%M:%S"), data.ac_voltage, data.p_pv, data.p_load, data.p_grid, data.p_akku
                );

                let now = Utc::now().time();
                let actions = {
                    let runtime = state.runtime.read().await;
                    dispatcher_decide_actions(&data, tesla.as_ref(), &runtime.config, now)
                };

                debug!("  --> updating inverter state");
                *state.inverter_state.write().await = Some(data);

                if actions.is_empty() {
                    debug!("  --> no Tesla action required");
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

        debug!("  --> sleeping {}s before next poll (inverter = {})", interval_seconds, base_url);
        sleep(Duration::from_secs(interval_seconds as u64)).await;
    }
}

/// Send a request to the BLE task and wait for the resulting Tesla state.
/// Returns `None` when the BLE task is unavailable (e.g. no VIN configured) or
/// the request failed — the caller treats that as "no car".
async fn request_tesla(state: &Arc<TGSApplication>, req: TGSBleCarRequest) -> Option<TGSTeslaState> {
    let (reply_tx, reply_rx) = oneshot::channel();
    if state.blecar_commandchannel_tx.send((req, reply_tx)).await.is_err() {
        debug!("[dispatcher:request_tesla] BLE task unavailable");
        return None;
    }
    match reply_rx.await {
        Ok(Ok(tesla_state)) => Some(tesla_state),
        Ok(Err(e)) => {
            debug!("[dispatcher:request_tesla] Tesla request failed: {}", e);
            None
        }
        Err(_) => {
            debug!("[dispatcher:request_tesla] BLE task dropped the reply");
            None
        }
    }
}

/// Decide which charging commands (if any) to send to the car this cycle.
///
/// `tesla` is the freshly-read car state, or `None` when the car is unreachable.
pub fn dispatcher_decide_actions(
    data: &TGSInverterDataEntry,
    tesla: Option<&TGSTeslaState>,
    cfg: &TGSDispatcherConfiguration,
    _now: NaiveTime,
) -> Vec<TGSTeslaCarCommand> {
    debug!("[dispatcher:dispatcher_decide_actions] Enter");
    let max_amps = cfg.tesla_max_charging_amps as f64;
    let charging_now = tesla.map(|t| t.charging).unwrap_or(false);

    let mut actions = Vec::new();

    if data.p_pv > 0.0 {
        let excess_watts = data.p_pv - data.p_load.abs();
        let target_amps = (excess_watts / data.ac_voltage).clamp(0.0, max_amps);
        debug!("  --> solar active: excess={:.0}W target={:.2}A max={:.0}A", excess_watts, target_amps, max_amps);

        if target_amps >= 1.0 {
            actions.push(TGSTeslaCarCommand::SetChargingAmps(target_amps as u8));
            if !charging_now {
                actions.push(TGSTeslaCarCommand::StartCharging);
            }
            debug!("[dispatcher:dispatcher_decide_actions] Exit");
            return actions;
        }
    }

    if charging_now {
        actions.push(TGSTeslaCarCommand::StopCharging);
    }
    debug!("[dispatcher:dispatcher_decide_actions] Exit");
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_cfg(max_amps: u8) -> TGSDispatcherConfiguration {
        TGSDispatcherConfiguration {
            tesla_max_charging_amps: max_amps,
            ..Default::default()
        }
    }

    fn make_data(p_pv: f64, p_load: f64) -> TGSInverterDataEntry {
        TGSInverterDataEntry {
            ac_voltage: 230.0,
            p_pv,
            p_load,
            p_grid: 0.0,
            p_akku: 0.0,
            battery_soc: 0.0,
            timestamp: Utc::now(),
        }
    }

    fn tesla(charging: bool) -> TGSTeslaState {
        TGSTeslaState { charging, ..Default::default() }
    }

    fn noon() -> NaiveTime {
        NaiveTime::from_hms_opt(12, 0, 0).unwrap()
    }

    #[test]
    fn solar_excess_sets_amps_and_starts_when_idle() {
        let cfg = make_cfg(16);
        let data = make_data(3000.0, 500.0); // 2500W / 230V ≈ 10.8A
        let actions = dispatcher_decide_actions(&data, Some(&tesla(false)), &cfg, noon());
        assert_eq!(
            actions,
            vec![
                TGSTeslaCarCommand::SetChargingAmps(10),
                TGSTeslaCarCommand::StartCharging,
            ]
        );
    }

    #[test]
    fn solar_excess_only_sets_amps_when_already_charging() {
        let cfg = make_cfg(16);
        let data = make_data(3000.0, 500.0);
        let actions = dispatcher_decide_actions(&data, Some(&tesla(true)), &cfg, noon());
        assert_eq!(actions, vec![TGSTeslaCarCommand::SetChargingAmps(10)]);
    }

    #[test]
    fn no_solar_stops_when_charging() {
        let cfg = make_cfg(16);
        let data = make_data(0.0, 500.0);
        let actions = dispatcher_decide_actions(&data, Some(&tesla(true)), &cfg, noon());
        assert_eq!(actions, vec![TGSTeslaCarCommand::StopCharging]);
    }

    #[test]
    fn no_solar_does_nothing_when_idle() {
        let cfg = make_cfg(16);
        let data = make_data(0.0, 500.0);
        let actions = dispatcher_decide_actions(&data, Some(&tesla(false)), &cfg, noon());
        assert!(actions.is_empty());
    }

    #[test]
    fn solar_less_than_load_stops() {
        let cfg = make_cfg(16);
        let data = make_data(100.0, 500.0); // excess negative → clamped to 0
        let actions = dispatcher_decide_actions(&data, Some(&tesla(true)), &cfg, noon());
        assert_eq!(actions, vec![TGSTeslaCarCommand::StopCharging]);
    }

    #[test]
    fn solar_clamped_to_max_amps() {
        let cfg = make_cfg(8);
        let data = make_data(10000.0, 500.0); // huge excess → capped at 8A
        let actions = dispatcher_decide_actions(&data, Some(&tesla(true)), &cfg, noon());
        assert_eq!(actions, vec![TGSTeslaCarCommand::SetChargingAmps(8)]);
    }
}
