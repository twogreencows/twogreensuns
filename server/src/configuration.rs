use std::env;
use chrono::NaiveTime;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use tracing::{debug, error, info};

#[derive(Serialize, Deserialize)]
pub struct Preference {
    pub active_configuration: String,
    pub configurations: Vec<TwoGreenSunsDispatcherConfiguration>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct TwoGreenSunsDispatcherConfiguration {
    pub uuid: String,
    pub inverter_ipv4_address: String,
    pub installation_display_name: String,
    pub installation_latitude: f64,
    pub installation_longitude: f64,
    pub provider_costs: Vec<(NaiveTime, NaiveTime, f64)>,
    pub polling_interval_minutes: u16,
    pub low_cost_windows: Vec<(NaiveTime, NaiveTime)>,
    pub tesla_vin: Option<String>,
    pub tesla_max_amps: u8,
    pub http_port: u16,
    pub mdns_hostname: String,
    pub use_mock_ble: bool,
}

impl TwoGreenSunsDispatcherConfiguration {
    pub fn ensure_directory_configuration() {
        debug!("[configuration:ensure_directory_configuration] Enter");
        match env::home_dir() {
            Some(mut path) => {
                path.push(".twogreensuns/");
                debug!("  --> config directory path = {:?}", path);
                match std::fs::exists(&path) {
                    Ok(exists) => {
                        if !exists {
                            debug!("  --> directory not found, creating it");
                            let _ = std::fs::create_dir(&path);
                            info!("[configuration:ensure_directory_configuration] Created config directory {:?}", path);
                        } else {
                            debug!("  --> directory already exists");
                        }
                    }
                    _ => {}
                }
            }
            None => error!("[configuration:ensure_directory_configuration] Cannot determine home directory"),
        }
        debug!("[configuration:ensure_directory_configuration] Exit");
    }

    pub fn load_last_configuration() -> Self {
        debug!("[configuration:load_last_configuration] Enter");
        let config_file_path = env::home_dir()
            .expect("home directory should be present")
            .join(".twogreensuns/configurations.json");

        debug!("  --> config file path = {:?}", config_file_path);

        if config_file_path.exists() {
            debug!("  --> file found, reading");
            match std::fs::read_to_string(&config_file_path) {
                Ok(contents) => match serde_json::from_str(&contents) {
                    Ok(config) => {
                        info!("[configuration:load_last_configuration] Loaded configuration from {:?}", config_file_path);
                        debug!("[configuration:load_last_configuration] Exit");
                        return config;
                    }
                    Err(e) => error!("[configuration:load_last_configuration] Failed to parse config: {}", e),
                },
                Err(e) => error!("[configuration:load_last_configuration] Failed to read config file: {}", e),
            }
        } else {
            debug!("  --> file not found, using defaults");
        }

        info!("[configuration:load_last_configuration] Using default configuration");
        debug!("[configuration:load_last_configuration] Exit");
        Self::default()
    }

    pub fn save(&self) {
        debug!("[configuration:save] Enter");
        let config_file_path = env::home_dir()
            .expect("home directory should be present")
            .join(".twogreensuns/configurations.json");

        debug!("  --> saving to {:?}", config_file_path);
        match serde_json::to_string_pretty(self) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&config_file_path, json) {
                    error!("[configuration:save] Failed to write config: {}", e);
                } else {
                    info!("[configuration:save] Configuration saved to {:?}", config_file_path);
                }
            }
            Err(e) => error!("[configuration:save] Failed to serialize config: {}", e),
        }
        debug!("[configuration:save] Exit");
    }
}

impl Default for TwoGreenSunsDispatcherConfiguration {
    fn default() -> Self {
        TwoGreenSunsDispatcherConfiguration {
            uuid: Uuid::new_v4().to_string(),
            inverter_ipv4_address: "192.0.0.36".to_string(),
            installation_display_name: "Twogreensun Controller".to_string(),
            installation_latitude: 0.0,
            installation_longitude: 0.0,
            provider_costs: Vec::new(),
            polling_interval_minutes: 10,
            low_cost_windows: Vec::new(),
            tesla_vin: None,
            tesla_max_amps: 16,
            http_port: 8080,
            mdns_hostname: "twogreensuns".to_string(),
            use_mock_ble: true,
        }
    }
}
