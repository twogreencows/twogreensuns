# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

`twogreensuns` is a home energy management system consisting of:
- **`server/`** — Rust async daemon (Tokio): polls a Fronius solar inverter and controls a Tesla via BLE
- **`client-ios/`** — SwiftUI iOS app (Swift Package Manager, XcodeGen)
- **`client-web/`** — Next.js 15 web dashboard (React, HeroUI, Tailwind)
- **`client-android/`** — Jetpack Compose Android app (Kotlin, Retrofit)

## Build & Run Commands

```bash
# Server
cd server && cargo build
cd server && cargo run
cd server && cargo test
cd server && cargo clippy

# Web client
cd client-web && npm install && npm run dev

# iOS — requires XcodeGen
cd client-ios && xcodegen generate   # generates TwoGreenSuns.xcodeproj
open client-ios/TwoGreenSuns.xcodeproj

# Android — open client-android/ in Android Studio
```

## Architecture

### Server (`server/`)

Single Tokio binary. `main.rs` initialises tracing, loads config, then runs:
```
tokio::join!(dispatcher_run, http_api_run, mdns_run)
```

**Modules:**
- `main.rs` — entry point, tracing init (`TWOGREENSUNS_LOG` env var, default DEBUG)
- `app_state.rs` — `AppState { config, inverter_state, tesla_state, tesla, log_level_setter }`
- `configuration.rs` — `TwoGreenSunsDispatcherConfiguration`, persists to `~/.twogreensuns/configurations.json`
- `fronius.rs` — `fronius_fetch_data()`, parses `/solar_api/v1/GetPowerFlowRealtimeData.fcgi`
- `dispatcher.rs` — `dispatcher_run` loop + pure `dispatcher_decide_command` (5 unit tests)
- `http_api.rs` — Axum 0.7, all routes under `/api/v1/`
- `mdns.rs` — raw UDP mDNS advertising `_twogreensuns._tcp.local.`
- `ble.rs` — `TeslaInterface` trait, `MockTesla`, `BtleplugTesla` stub
- `protocol.rs` — `TwoGreenSunsDispatcherCommand { Start, Level(f64), Stop }`
- `tesla.rs` — UUID constants + `tesla_encode_command` stubs

**HTTP API routes (all `/api/v1/`):**
```
GET  /api/v1/status
GET  /api/v1/config
PUT  /api/v1/config
PUT  /api/v1/log/level          body: { "level": "debug|info|warn|error" }
POST /api/v1/tesla/discover
POST /api/v1/tesla/charge/start
POST /api/v1/tesla/charge/stop
POST /api/v1/tesla/charge/amps/:n
```

**Key dependencies:** `tokio`, `reqwest`, `axum 0.7`, `btleplug`, `serde/serde_json`, `chrono`, `uuid`, `tracing`, `tracing-subscriber`

### Web client (`client-web/`)

Next.js 15 App Router. Call chain:
```
Browser → /api/v1/* (Next.js Route Handler) → TWOGREENSUNS_SERVER_URL/api/v1/* (Rust)
```

- `app/api/v1/[...path]/route.ts` — catch-all proxy to Rust server
- `lib/server-api.ts` — typed fetch wrappers for Server Components (reads `TWOGREENSUNS_SERVER_URL`)
- `lib/client-api.ts` — typed fetch wrappers for `"use client"` components (calls `/api/v1/`)
- `lib/types.ts` — shared TypeScript types matching Rust JSON shapes
- `app/dashboard/page.tsx` — Server Component, SSR
- `app/tesla/page.tsx` — Server Component with `TeslaCard` client island
- `app/settings/page.tsx` — `"use client"` form
- `components/` — `NavBar`, `InverterCard`, `TeslaCard`

Configure server URL in `client-web/.env.local`:
```
TWOGREENSUNS_SERVER_URL=http://twogreensuns.local:8080
```

### iOS client (`client-ios/`)

SwiftUI, iOS 17+, Swift Package Manager only (no CocoaPods).
Generate project with: `cd client-ios && xcodegen generate`

- `Network/APIClient.swift` — async/await HTTP client calling `/api/v1/`
- `Models/Models.swift` — Codable structs
- `Models/AppViewModel.swift` — `@MainActor ObservableObject`
- `Views/` — `DashboardView`, `TeslaView`, `SettingsView`

### Android client (`client-android/`)

Jetpack Compose, minSdk 26, Kotlin 2.0.
Open in Android Studio. Server URL set via `BuildConfig.SERVER_URL` in `app/build.gradle.kts`.

- `network/ApiClient.kt` — Retrofit + Moshi calling `/api/v1/`
- `network/Models.kt` — Moshi data classes
- `viewmodel/DashboardViewModel.kt` — single ViewModel for the app
- `ui/screens/` — `DashboardScreen`, `SettingsScreen`

## Configuration Storage

Config directory: `~/.twogreensuns/`
Config file: `~/.twogreensuns/configurations.json`

## Logging

Env var `TWOGREENSUNS_LOG` sets level (default: `debug`).
Runtime: `PUT /api/v1/log/level` with `{ "level": "info" }`.
Format: `[file:fn] Enter/Exit`, `  --> step`.

## Working Agreement

- **Never write or edit code without explicit approval first** — propose the change (describe it or show a diff) and wait for the user to review it before writing or editing any file. This applies even for small, obvious, or previously-discussed changes; approval for one step is never approval for the next one.
