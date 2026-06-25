use std::sync::Arc;

use chrono::{DateTime, Utc};
use reqwest::Error;
use serde::{Deserialize, Serialize};
use tokio::time::Duration;
use tracing::{debug, error, info};

use crate::application::TGSApplication;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TGSInverterDataEntry {
    pub ac_voltage:f64,
    pub p_pv: f64,
    pub p_load: f64,
    pub p_grid: f64,
    pub p_akku: f64,
    pub timestamp: DateTime<Utc>,
}

/// Discovers the Fronius inverter on the local network via mDNS
/// (`_Fronius-SE-Inverter._tcp`) and returns its IPv4 address, or `None` if
/// no inverter responds within 5 seconds.
pub async fn fronius_discover_address() -> Option<String> {
    use tracing::{debug, info};
    //debug!("[fronius:fronius_discover_address] Enter");
    let addr = crate::mdns::mdns_discover_service_address("_Fronius-SE-Inverter._tcp", 5).await;
    match &addr {
        Some(a) => info!("[fronius:fronius_discover_address] Inverter found at {}", a),
        None => debug!("[fronius:fronius_discover_address] Inverter not found via mDNS"),
    }
    addr
}

/// Continuously browses for the Fronius inverter via mDNS.  When an address
/// is found it is written into the runtime and `inverter_address_notify` is
/// fired so the dispatcher can unblock.  Retries every 30 seconds until the
/// program exits.
pub async fn fronius_discovery_run(state: Arc<TGSApplication>) {
    loop {
        debug!("[fronius:fronius_discovery_run] Browsing for Fronius inverter...");
        if let Some(discovered_address) = fronius_discover_address().await {
            {
                let mut runtime = state.runtime.write().await;
                if runtime.tgsdevice_inverter_ipv4_address != discovered_address {
                    info!(
                        "[fronius:fronius_discovery_run] Inverter address updated: {} → {}",
                        runtime.tgsdevice_inverter_ipv4_address, discovered_address
                    );
                    runtime.tgsdevice_inverter_ipv4_address = discovered_address;
                }
            } // write lock released here
            state.inverter_address_notify.notify_one();
        }
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}

pub async fn fronius_fetch_data( client: &reqwest::Client, base_url: &str) -> Result<TGSInverterDataEntry, Error> {
    debug!("[fronius:fronius_fetch_data] Enter");

    let url_meter = format!( "{}/solar_api/v1/GetMeterRealtimeData.cgi", base_url);
    let url_power = format!( "{}/solar_api/v1/GetPowerFlowRealtimeData.fcgi", base_url);
    debug!("  --> GET {} + {} (parallel)", url_meter, url_power);

    let (result_meter, result_power) = tokio::join!(
        async { client.get(&url_meter).send().await?.json::<serde_json::Value>().await },
        async { client.get(&url_power).send().await?.json::<serde_json::Value>().await }
    );
    let data_meter = result_meter?;
    println!("data_meter is {:#?}", data_meter);

    let data_power = result_power?;
    println!("data_power is {:#?}", data_power);

    let inverter = &data_power["Body"]["Data"]["Inverters"]["1"];
    let site = &data_power["Body"]["Data"]["Site"];

    let ac_voltage = data_meter["Body"]["Data"]["0"]["Voltage_AC_Phase_1"].as_f64().unwrap_or(0.0);
    //println!("{:#?}" , data_meter["Body"]["Data"]["0"]);
    let p_pv = site["P_PV"].as_f64().unwrap_or(0.0);
    let p_load = site["P_Load"].as_f64().unwrap_or(0.0);
    let p_grid = site["P_Grid"].as_f64().unwrap_or(0.0);
    let p_akku = site["P_Akku"].as_f64().unwrap_or(0.0);

    debug!("  --> parsed: AC_VOLTAGE = {:.00} P_PV={:.0} P_Load={:.0} P_Grid={:.0} P_Akku={:.0}", ac_voltage, p_pv, p_load, p_grid, p_akku);

    if p_pv == 0.0 && p_load == 0.0 && p_grid == 0.0 && p_akku == 0.0 {
        error!("[fronius:fronius_fetch_data] All values zero — inverter may be unreachable or off");
    }

    Ok(TGSInverterDataEntry{ ac_voltage, p_pv, p_load, p_grid, p_akku, timestamp: Utc::now() })
}
