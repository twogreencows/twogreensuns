// Mirrors the Rust structs serialized by the server (snake_case).

export interface InverterData {
  p_pv: number;
  p_load: number;
  p_grid: number;
  p_akku: number;
  timestamp: string;
}

export interface TeslaState {
  connected: boolean;
  charging: boolean;
  charging_amps: number;
}

export interface StatusResponse {
  inverter: InverterData | null;
  tesla: TeslaState | null;
}

export interface AppConfig {
  uuid: string;
  inverter_ipv4_address: string;
  installation_display_name: string;
  installation_latitude: number;
  installation_longitude: number;
  polling_interval_minutes: number;
  tesla_vin: string | null;
  tesla_max_amps: number;
  http_port: number;
  mdns_hostname: string;
  use_mock_ble: boolean;
}
