
use chrono::NaiveTime;

struct TwoGreenSunsDispatcherConfiguration {
    inverter_ipv4_address:String,
    installation_display_name:String,    
    installation_latitude: f64,
    installation_longitude :f64,
    provider_costs:Vec::<(NaiveTime, NaiveTime,f64)>, //this is not good this should be      
}

impl TwoGreenSunsDispatcherConfiguration {


}
