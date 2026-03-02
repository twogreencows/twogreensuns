use chrono::{DateTime, Utc};
use reqwest::Error;
use serde::{Deserialize, Serialize};
use tracing::{debug, error};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct InverterData {
    pub p_pv: f64,
    pub p_load: f64,
    pub p_grid: f64,
    pub p_akku: f64,
    pub timestamp: DateTime<Utc>,
}

pub async fn fronius_fetch_data(
    client: &reqwest::Client,
    base_url: &str,
) -> Result<InverterData, Error> {
    debug!("[fronius:fronius_fetch_data] Enter");

    let url = format!(
        "{}/solar_api/v1/GetPowerFlowRealtimeData.fcgi",
        base_url
    );
    debug!("  --> GET {}", url);

    let response = client.get(&url).send().await?;
    debug!("  --> response received, parsing JSON");

    let data: serde_json::Value = response.json().await?;

    let site = &data["Body"]["Data"]["Site"];

    let p_pv = site["P_PV"].as_f64().unwrap_or(0.0);
    let p_load = site["P_Load"].as_f64().unwrap_or(0.0);
    let p_grid = site["P_Grid"].as_f64().unwrap_or(0.0);
    let p_akku = site["P_Akku"].as_f64().unwrap_or(0.0);

    debug!("  --> parsed: P_PV={:.0} P_Load={:.0} P_Grid={:.0} P_Akku={:.0}", p_pv, p_load, p_grid, p_akku);

    if p_pv == 0.0 && p_load == 0.0 && p_grid == 0.0 && p_akku == 0.0 {
        error!("[fronius:fronius_fetch_data] All values zero — inverter may be unreachable or off");
    }

    debug!("[fronius:fronius_fetch_data] Exit");
    Ok(InverterData {
        p_pv,
        p_load,
        p_grid,
        p_akku,
        timestamp: Utc::now(),
    })
}
