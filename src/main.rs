


use std::time::Instant;
use reqwest::Error;
use serde_json::Value;
use tokio::time;


const INVERTER_ADDRESS:&str = "192.168.1.36";

use chrono::{DateTime, NaiveDate, NaiveDateTime, Months};

async fn inverter_task() -> Result<(), Error> {
    let url1 = "http://192.168.1.36/solar_api/v1/GetInverterRealtimeData.cgi?Scope=Device&DeviceId=1&DataCollection=CommonInverterData";
    let url2 = "http://192.168.1.36/solar_api/v1/GetStorageRealtimeData.cgi?Scope=Device&DeviceId=0&DataCollection=CommonInverterData";
    let url3 = "http://192.168.1.36/solar_api/v1/GetPowerFlowRealtimeData.fcgi?Scope=Device&DeviceId=1&DataCollection=CommonInverterData";
    let url4 = "http://192.168.1.36/status/powerflow";
    let client = reqwest::Client::new();

    let response =  client.get(url1).send().await?;
    let datajson: serde_json::Value = response.json().await?;
    //println!("=====\n {}",  serde_json::to_string_pretty(&datajson).unwrap() );

    let response =  client.get(url2).send().await?;
    let datajson: serde_json::Value = response.json().await?;
    //println!("=====\n {}",  serde_json::to_string_pretty(&datajson).unwrap() );
    
    let response =  client.get(url3).send().await?;
    let datajson: serde_json::Value = response.json().await?;
    let power_pv = &datajson["Body"]["Data"]["Site"]["P_PV"];
    let power_akku = &datajson["Body"]["Data"]["Site"]["P_Akku"];
    let power_grid = &datajson["Body"]["Data"]["Site"]["P_Grid"];
    let power_load = &datajson["Body"]["Data"]["Site"]["P_Load"];
    //println!("=====\n {}",  serde_json::to_string_pretty(&datajson).unwrap() );
    println!("P_Load {}, P_PV {}, P_grid {}, P_Akku {}", power_load, power_pv, power_grid, power_akku);

    Ok(())

}


#[tokio::main]
async fn main() -> Result< (), Error>{
    let mut interval = time::interval(time::Duration::from_secs(10));
    loop {
        interval.tick().await;
        inverter_task().await;
    } 


    Ok(())

}
