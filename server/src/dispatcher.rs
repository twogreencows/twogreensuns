use std::sync::Arc;

use chrono::{NaiveTime, Utc};
use reqwest::Client;
use tokio::time::{sleep, Duration};
use tracing::{debug, error, info};

use crate::app_state::AppState;
use crate::configuration::TwoGreenSunsDispatcherConfiguration;
use crate::fronius::{fronius_fetch_data, InverterData};
use crate::protocol::TwoGreenSunsDispatcherCommand;

pub async fn dispatcher_run(state: Arc<AppState>) {
    debug!("[dispatcher:dispatcher_run] Enter");
    let client = Client::new();
    loop {
        let (base_url, interval_minutes) = {
            let cfg = state.config.read().await;
            let base_url = format!("http://{}", cfg.inverter_ipv4_address);
            let interval_minutes = cfg.polling_interval_minutes;
            (base_url, interval_minutes)
        };

        debug!("  --> sleeping {}m before next poll (inverter = {})", interval_minutes, base_url);
        sleep(Duration::from_secs(interval_minutes as u64 * 60)).await;

        debug!("  --> polling inverter at {}", base_url);
        match fronius_fetch_data(&client, &base_url).await {
            Ok(data) => {
                info!(
                    "[dispatcher:dispatcher_run] P_PV={:.0}W P_Load={:.0}W P_Grid={:.0}W P_Akku={:.0}W",
                    data.p_pv, data.p_load, data.p_grid, data.p_akku
                );

                let now = Utc::now().time();
                let cmd = {
                    let cfg = state.config.read().await;
                    dispatcher_decide_command(&data, &cfg, now)
                };

                debug!("  --> updating inverter state");
                *state.inverter_state.write().await = Some(data);

                debug!("  --> sending command to Tesla");
                let tesla = state.tesla.lock().await;
                let result = match cmd {
                    TwoGreenSunsDispatcherCommand::Start => tesla.start_charging().await,
                    TwoGreenSunsDispatcherCommand::Stop => tesla.stop_charging().await,
                    TwoGreenSunsDispatcherCommand::Level(amps) => {
                        tesla.set_charging_amps(amps as u8).await
                    }
                };
                if let Err(e) = result {
                    error!("[dispatcher:dispatcher_run] Tesla command error: {}", e);
                }

                debug!("  --> refreshing Tesla state");
                *state.tesla_state.write().await = tesla.status().await;
            }
            Err(e) => error!("[dispatcher:dispatcher_run] Inverter fetch error: {}", e),
        }
    }
}

pub fn dispatcher_decide_command(
    data: &InverterData,
    cfg: &TwoGreenSunsDispatcherConfiguration,
    now: NaiveTime,
) -> TwoGreenSunsDispatcherCommand {
    debug!("[dispatcher:dispatcher_decide_command] Enter");
    let max_amps = cfg.tesla_max_amps as f64;

    let cmd = if data.p_pv > 0.0 {
        let excess_watts = data.p_pv - data.p_load.abs();
        let target_amps = (excess_watts / 230.0).clamp(0.0, max_amps);
        debug!("  --> solar active: excess={:.0}W target={:.2}A max={:.0}A", excess_watts, target_amps, max_amps);
        if target_amps > 0.0 {
            TwoGreenSunsDispatcherCommand::Level(target_amps)
        } else {
            TwoGreenSunsDispatcherCommand::Stop
        }
    } else {
        let in_low_cost_window = cfg.low_cost_windows.iter().any(|(start, end)| {
            if start <= end {
                now >= *start && now <= *end
            } else {
                // Window wraps midnight
                now >= *start || now <= *end
            }
        });
        debug!("  --> no solar: in_low_cost_window={}", in_low_cost_window);
        if in_low_cost_window {
            TwoGreenSunsDispatcherCommand::Level(max_amps)
        } else {
            TwoGreenSunsDispatcherCommand::Stop
        }
    };

    debug!("[dispatcher:dispatcher_decide_command] Exit");
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_cfg(
        max_amps: u8,
        low_cost_windows: Vec<(NaiveTime, NaiveTime)>,
    ) -> TwoGreenSunsDispatcherConfiguration {
        TwoGreenSunsDispatcherConfiguration {
            tesla_max_amps: max_amps,
            low_cost_windows,
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
        let cfg = make_cfg(16, vec![]);
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
    fn test_no_solar_no_window_stops() {
        let cfg = make_cfg(16, vec![]);
        let data = make_data(0.0, 500.0);
        let now = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let cmd = dispatcher_decide_command(&data, &cfg, now);
        assert!(matches!(cmd, TwoGreenSunsDispatcherCommand::Stop));
    }

    #[test]
    fn test_no_solar_in_low_cost_window_charges_max() {
        let start = NaiveTime::from_hms_opt(0, 0, 0).unwrap();
        let end = NaiveTime::from_hms_opt(6, 0, 0).unwrap();
        let cfg = make_cfg(16, vec![(start, end)]);
        let data = make_data(0.0, 500.0);
        let now = NaiveTime::from_hms_opt(3, 0, 0).unwrap();
        let cmd = dispatcher_decide_command(&data, &cfg, now);
        match cmd {
            TwoGreenSunsDispatcherCommand::Level(amps) => {
                assert!((amps - 16.0).abs() < 0.01, "Expected 16A, got {}", amps);
            }
            _ => panic!("Expected Level command at max amps"),
        }
    }

    #[test]
    fn test_solar_less_than_load_stops() {
        let cfg = make_cfg(16, vec![]);
        let data = make_data(100.0, 500.0); // excess = -400W → clamped to 0 → Stop
        let now = NaiveTime::from_hms_opt(12, 0, 0).unwrap();
        let cmd = dispatcher_decide_command(&data, &cfg, now);
        assert!(matches!(cmd, TwoGreenSunsDispatcherCommand::Stop));
    }

    #[test]
    fn test_solar_clamped_to_max_amps() {
        let cfg = make_cfg(8, vec![]);
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
