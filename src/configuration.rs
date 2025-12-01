use std::fs;
use std::path::Path;
use std::env;
use chrono::NaiveTime;

pub struct TwoGreenSunsDispatcherConfiguration {
    inverter_ipv4_address:String,
    installation_display_name:String,    
    installation_latitude: f64,
    installation_longitude :f64,
    provider_costs:Vec::<(NaiveTime, NaiveTime,f64)>, //this is not good this should be      
}

impl TwoGreenSunsDispatcherConfiguration {

    pub fn ensure_directory_configuration() {
        match env::home_dir() {
            Some(mut path) => {
                println!("Your home directory, probably: {}", path.display());
                path.push(".twogreensuns/");
                println!("{path:?}");

                match  fs::exists(&path) {
                    Ok(exists) => {
                        if false == exists {
                            println!("We need to create the directory");
                            let _ = fs::create_dir(&path);
                        } else {
                            println!("Directory is here");
                        }
                    },
                    _ => {},
                }
            },
            None => println!("Impossible to get your home dir!"),
        }

    }

}
