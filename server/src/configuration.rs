// ---------------------------------------------------------------------------
// TODO(refactor): separate concerns between Preferences and Configuration.
//
// Today `TGSDispatcherConfiguration` owns filesystem paths, directory creation,
// and multi-config persistence — none of which are its job. `save()` and
// `load_last_configuration()` are also asymmetric in spirit (one config in,
// a whole wrapper file out).
//
// Proposed split:
//
// TGSDispatcherPreferences  — owns the file + the collection
//   - config_dir() / config_file_path()   path helpers
//   - ensure_directory()                  (was ensure_directory_configuration)
//   - load() -> Self                      read + parse file; empty on any failure
//   - save(&self)                         serialize the whole wrapper
//   - active(&self) -> TGSDispatcherConfiguration
//                                         resolve active_configuration -> clone,
//                                         or default
//   - upsert(&mut self, cfg)              replace-by-uuid or push, then set
//                                         active_configuration
//   - impl Default
//
// TGSDispatcherConfiguration — just the data + its own defaults
//   - fields + impl Default only; no `env`, no `std::fs`
//
// Call sites:
//   - main.rs: TGSDispatcherPreferences::ensure_directory();
//              let prefs = TGSDispatcherPreferences::load();
//              let config = prefs.active();
//   - http_api.rs handle_put_config: load prefs, prefs.upsert(updated),
//              prefs.save(), then set runtime.config.
//
// Open question: does `runtime` hold just the active TGSDispatcherConfiguration
// (simpler, current shape) or the full TGSDispatcherPreferences (needed to
// switch/list configs over the API)? Leaning: keep the active config in runtime
// for now and let http_api reload prefs on write.
// ---------------------------------------------------------------------------

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

        // Load existing preferences (or start fresh), then upsert this configuration.
        let mut preferences = match std::fs::read_to_string(&config_file_path) {
            Ok(contents) => serde_json::from_str::<TGSDispatcherPreferences>(&contents)
                .unwrap_or_else(|e| {
                    error!(" --> Failed to parse existing config: {}, starting fresh", e);
                    TGSDispatcherPreferences {
                        active_configuration: None,
                        configurations: Vec::new(),
                    }
                }),
            Err(_) => TGSDispatcherPreferences {
                active_configuration: None,
                configurations: Vec::new(),
            },
        };

        match preferences
            .configurations
            .iter_mut()
            .find(|configuration| configuration.uuid == self.uuid)
        {
            Some(existing) => {
                debug!("  --> replacing existing configuration {}", self.uuid);
                *existing = self.clone();
            }
            None => {
                debug!("  --> appending new configuration {}", self.uuid);
                preferences.configurations.push(self.clone());
            }
        }
        preferences.active_configuration = Some(self.uuid.clone());

        match serde_json::to_string_pretty(&preferences) {
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
