use std::fmt;
use uuid::Uuid;
use crate::configuration::TGSDispatcherConfiguration;
use crate::blecar::TGSTeslaCarCommand;
use crate::mdns::mdns_get_hostname;
use crate::fronius::TGSInverterDataEntry;

use serde::{Serialize,Deserialize};

// ──────────────────────────────────────────────────────────────────────────────
// Platform
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TGSRuntimeDataEntry{
    pub uuid:Uuid,
    //pub inverter_entry:TGSInverterDataEntry,
    pub blecar_commands:Vec<TGSTeslaCarCommand> 
}


#[derive(Debug, Clone, PartialEq)]
pub enum TGSPlatform {
    MacOS,
    Linux,
    Windows,
}

impl TGSPlatform {
    fn detect() -> Self {
        #[cfg(target_os = "macos")]
        {
            TGSPlatform::MacOS
        }
        #[cfg(target_os = "windows")]
        {
            TGSPlatform::Windows
        }
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        {
            TGSPlatform::Linux
        }
    }
}

impl fmt::Display for TGSPlatform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            TGSPlatform::MacOS => "twogreensuns.platform.macos",
            TGSPlatform::Linux => "twogreensuns.platform.linux",
            TGSPlatform::Windows => "twogreensuns.platform.windows",
        };
        write!(f, "{}", s)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Runtime
// ──────────────────────────────────────────────────────────────────────────────

/// Runtime state built at startup from the persisted configuration plus
/// environment-specific parameters.  Not serialised to disk.
///
/// Fields that are not persisted in `TGSDispatcherConfiguration`:
/// * `tgsdevice_inverter_ipv4_address` — active address (mDNS-discovered, or a fallback)
/// * `tgsbox_server_hostname`          — mDNS service name: `installation_display_name` if set,
///                                       otherwise the machine hostname
/// * `tgsbox_server_httport`           — TCP port the HTTP API listens on
/// * `tgsbox_platform`                 — detected at compile time
pub struct TGSDispatcherRuntime {
    pub config: TGSDispatcherConfiguration,
    pub tgsdevice_inverter_ipv4_address: String,
    //this is not point this should be data entry
    pub tgsdevice_inverter_points: Vec<TGSInverterDataEntry>,

    pub tgsbox_server_hostname: String,
    pub tgsbox_server_httport: u16,
    pub tgsbox_platform: TGSPlatform,
}

impl TGSDispatcherRuntime {
    pub fn new( config: TGSDispatcherConfiguration, inverter_ipv4_address: String) -> Self {
        let platform = TGSPlatform::detect();
        let hostname = if config.installation_display_name.is_empty() {
            mdns_get_hostname()
        } else {
            config.installation_display_name.clone()
        };
        TGSDispatcherRuntime {
            config,
            tgsdevice_inverter_ipv4_address: inverter_ipv4_address,
            tgsdevice_inverter_points: Vec::new(),
            tgsbox_server_hostname: hostname,
            tgsbox_server_httport: 8080,
            tgsbox_platform: platform,
        }
    }
}
