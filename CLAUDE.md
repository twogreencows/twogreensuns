# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`twogreensuns` is a Rust-based home energy management dispatcher. It polls a Fronius solar inverter over HTTP and is being extended to control a Tesla vehicle via Bluetooth LE. The goal is to coordinate solar production, battery storage, and EV charging based on configurable rules and energy provider pricing.

## Build & Run Commands

```bash
cargo build          # compile
cargo run            # run the dispatcher
cargo test           # run tests
cargo clippy         # lint
```

The inverter IP address is currently hardcoded in `main.rs` as `INVERTER_ADDRESS` (`192.168.1.36`). Running requires local network access to that device.

## Architecture

The project is a single Tokio async binary (`#[tokio::main]`) that loops on a 10-second interval polling the Fronius inverter REST API.

### Modules

- **`main.rs`** — Entry point. Calls `ensure_directory_configuration()`, loads config, then runs an infinite `tokio::time::interval` loop calling `inverter_task()`. The inverter IP and API URLs are hardcoded here (not yet read from config).
- **`configuration.rs`** — `TwoGreenSunsDispatcherConfiguration` struct: holds inverter IPv4, installation name/coordinates, and electricity tariff schedule (`Vec<(NaiveTime, NaiveTime, f64)>`). Persists to `~/.twogreensuns/configurations.json`. `Preference` wraps multiple named configurations.
- **`fronius.rs`** — Stub `InverterData` struct (generated power, house consumption, battery level) and an unimplemented `fetch_inverter_data()`. The actual fetching is currently in `main.rs::inverter_task()`.
- **`protocol.rs`** — `TwoGreenSunsDispatcherCommand` enum: `Start`, `Level(f64)`, `Stop`. Intended as the dispatch command type sent to controlled devices (Tesla, etc.).
- **`tesla.rs`** — Empty stub for Tesla integration.

### Key Dependencies

| Crate | Purpose |
|---|---|
| `tokio` (full) | Async runtime |
| `reqwest` (json) | HTTP calls to Fronius Solar API |
| `btleplug` | Bluetooth LE (cross-platform) for Tesla/device control |
| `serde` / `serde_json` | JSON serialization for config and API responses |
| `chrono` | Time-based tariff scheduling |
| `uuid` | Per-configuration unique IDs |

### Fronius Solar API Endpoints (v1)

The inverter exposes a local REST API:
- `/solar_api/v1/GetInverterRealtimeData.cgi` — AC power output
- `/solar_api/v1/GetStorageRealtimeData.cgi` — Battery state
- `/solar_api/v1/GetPowerFlowRealtimeData.fcgi` — Site-level flows: `P_PV`, `P_Akku`, `P_Grid`, `P_Load`
- `/status/powerflow` — Alternative power flow endpoint

### Configuration Storage

Config directory: `~/.twogreensuns/`
Config file: `~/.twogreensuns/configurations.json`

The directory is created on first run by `ensure_directory_configuration()`. Config loading (`load_last_configuration`) is stubbed and returns `Default` for now.
