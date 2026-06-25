use std::env;
use chrono::NaiveTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::{debug, error, info};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TGSDispatcherPreferences {
    pub active_configuration: Option<String>,
    pub configurations: Vec<TGSDispatcherConfiguration>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TGSDispatcherConfiguration {
    pub uuid: String,
    pub installation_display_name: String,
    pub installation_latitude: f64,
    pub installation_longitude: f64,
    pub provider_costs: Vec<(NaiveTime, NaiveTime, f64)>,
    pub polling_interval_seconds: u16,
    pub tesla_vin: Option<String>,
    pub tesla_max_charging_amps: u8,
}

impl TGSDispatcherConfiguration {
    pub fn ensure_directory_configuration() {
        match env::home_dir() {
            Some(mut path) => {
                path.push(".twogreensuns/");
                debug!("[configuration:ensure_directory_configuration] Ensuring config directory at {:?}", path);
                match std::fs::exists(&path) {
                    Ok(exists) => {
                        if !exists {
                            let _ = std::fs::create_dir(&path);
                        } 
                    }
                    _ => {}
                }
            }
            None => {},
        }
    }

    pub fn load_last_configuration() -> Self {
        let config_file_path = env::home_dir()
            .expect("home directory should be present")
            .join(".twogreensuns/configurations.json");
        debug!("[configuration:load_last_configuration] Load configuration");

        if !config_file_path.exists() {
            return Self::default();
        }

        match std::fs::read_to_string(&config_file_path) {
            Ok(contents) => {
              let preferences = serde_json::from_str::<TGSDispatcherPreferences>(&contents); 
              match preferences {
                Ok(preferences) => {
                    debug!(" --> Loaded configuration from {:?}", config_file_path);
                    let configuration = match  preferences.active_configuration  {
                        Some(configuration_uuid) => { 
                            debug!(" --> Configuration found : {}", configuration_uuid);
                            preferences.configurations.into_iter()
                                        .find(|configuration| configuration.uuid == configuration_uuid)
                                        .unwrap_or(Self::default()) 
                        }, 
                        None => Self::default()
                    };
                    return configuration;
                 }
                 Err(e) => error!(" -->  Failed to parse config: {}, using default", e),
              }
           },
           Err(e) => error!(" -->  Failed to read config file: {}, using default", e),
        }

        Self::default()
    }

    pub fn save(&self) {
        debug!("[configuration:save] Enter");
        let config_file_path = env::home_dir()
            .expect(" --> home directory should be present")
            .join(".twogreensuns/configurations.json");

        debug!("  --> saving to {:?}", config_file_path);
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&config_file_path, json) {
                    error!(" --> Failed to write config: {}", e);
                } else {
                    info!(" --> Configuration saved to {:?}", config_file_path);
                }
            }
            Err(e) => error!(" --> Failed to serialize config: {}", e),
        }
    }
}

impl Default for TGSDispatcherConfiguration {
    fn default() -> Self {
        TGSDispatcherConfiguration {
            uuid: Uuid::new_v4().to_string(),
            installation_display_name: "".to_string(),
            installation_latitude: 0.0,
            installation_longitude: 0.0,
            provider_costs: Vec::new(),
            polling_interval_seconds: 600,
            tesla_vin: None,
            tesla_max_charging_amps: 16,
        }
    }
}
