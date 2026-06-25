use std::sync::Arc;

use chrono::{Local, NaiveTime, Utc};
use reqwest::Client;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info};

use crate::application::TGSApplication;
use crate::configuration::TGSDispatcherConfiguration;
use crate::fronius::{fronius_fetch_data, TGSInverterDataEntry};
use crate::protocol::TwoGreenSunsDispatcherCommand;

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

        match fronius_fetch_data(&client, &base_url).await {
            Ok(data) => {
                info!(
                    " --> [{}] Polling inverter with result AC_VOLTAGE={} P_PV={:.0}W P_Load={:.0}W P_Grid={:.0}W P_Akku={:.0}W",
                    Local::now().format("%Y-%m-%d %H:%M:%S"), data.ac_voltage, data.p_pv, data.p_load, data.p_grid, data.p_akku
                );

                let now = Utc::now().time();
                let cmd = {
                    let runtime = state.runtime.read().await;
                    dispatcher_decide_command(&data, &runtime.config, now)
                };

                debug!("  --> updating inverter state");
                *state.inverter_state.write().await = Some(data);

                debug!("  --> sending command to Tesla");

                debug!("  --> refreshing Tesla state");
                //*state.tesla_state.write().await = tesla.status().await;
            }
            Err(e) => error!("[dispatcher:dispatcher_run] Inverter fetch error: {}", e),
        }

        debug!("  --> sleeping {}s before next poll (inverter = {})", interval_seconds, base_url);
        sleep(Duration::from_secs(interval_seconds as u64)).await;
    }
}

pub fn dispatcher_decide_command( data: &TGSInverterDataEntry, cfg: &TGSDispatcherConfiguration, _now: NaiveTime) -> TwoGreenSunsDispatcherCommand {
    debug!("[dispatcher:dispatcher_decide_command] Enter");
    let max_amps = cfg.tesla_max_charging_amps as f64;

    let cmd = if data.p_pv > 0.0 {
        let excess_watts = data.p_pv - data.p_load.abs();
        let target_amps = (excess_watts / data.ac_voltage).clamp(0.0, max_amps);

        debug!("  --> solar active: excess={:.0}W target={:.2}A max={:.0}A", excess_watts, target_amps, max_amps);
        if target_amps > 0.0 {
            TwoGreenSunsDispatcherCommand::Level(target_amps)
        } else {
            TwoGreenSunsDispatcherCommand::Stop
        }
    } else {
        TwoGreenSunsDispatcherCommand::Stop
    };

    debug!("[dispatcher:dispatcher_decide_command] Exit");
    cmd
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

    fn make_data(p_pv: f64, p_load: f64) -> InverterData {
        InverterData {
            p_pv,
            p_load,
            p_grid: 0.0,
            p_akku: 0.0,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_solar_excess_charges() {
        let cfg = make_cfg(16);
        let data = make_data(3000.0, 500.0); // 2500W excess → ~10.87A
        let now = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let cmd = dispatcher_decide_command(&data, &cfg, now);
        match cmd {
            TwoGreenSunsDispatcherCommand::Level(amps) => {
                assert!((amps - (2500.0 / 230.0)).abs() < 0.01, "Unexpected amps: {}", amps);
            }
            _ => panic!("Expected Level command"),
        }
    }

    #[test]
    fn test_no_solar_stops() {
        let cfg = make_cfg(16);
        let data = make_data(0.0, 500.0);
        let now = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let cmd = dispatcher_decide_command(&data, &cfg, now);
        assert!(matches!(cmd, TwoGreenSunsDispatcherCommand::Stop));
    }

    #[test]
    fn test_solar_less_than_load_stops() {
        let cfg = make_cfg(16);
        let data = make_data(100.0, 500.0); // excess = -400W → clamped to 0 → Stop
        let now = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let cmd = dispatcher_decide_command(&data, &cfg, now);
        assert!(matches!(cmd, TwoGreenSunsDispatcherCommand::Stop));
    }

    #[test]
    fn test_solar_clamped_to_max_amps() {
        let cfg = make_cfg(8);
        let data = make_data(10000.0, 500.0); // huge excess → capped at 8A
        let now = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let cmd = dispatcher_decide_command(&data, &cfg, now);
        match cmd {
            TwoGreenSunsDispatcherCommand::Level(amps) => {
                assert!((amps - 8.0).abs() < 0.01, "Expected 8A, got {}", amps);
            }
            _ => panic!("Expected Level command"),
        }
    }
}
