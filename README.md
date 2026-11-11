# TwoGreenSuns

Home energy management system. Polls a Fronius solar inverter, controls a Tesla charger via BLE, and exposes everything through a local HTTP API. Clients connect over the local network using mDNS (Bonjour) discovery.

## Components

```
server/          Rust async daemon — the brain
client-ios/      SwiftUI iOS app
client-web/      Next.js 15 web dashboard
client-android/  Jetpack Compose Android app
```

---

## Server

Rust / Tokio daemon. Runs three concurrent tasks: Fronius poller, HTTP API, mDNS advertiser.
rustup target add aarch64-unknown-linux-gnu   # or armv7-unknown-linux-gnueabihf for 32-bit
cargo build --release --target aarch64-unknown-linux-gnu

**Requirements:** Rust toolchain (`rustup`)

```bash
cd server

# Build
cargo build

# Run (default port 8080, log level DEBUG)
cargo run

# Run with a custom log level
TWOGREENSUNS_LOG=info cargo run

# Run without mDNS advertising
cargo run -- --no-bonjour

# Tests
cargo test

# Lint
cargo clippy
```

Config is stored in `~/.twogreensuns/configurations.json` and created with defaults on first run.

---

## iOS client

SwiftUI app, iOS 17+. Discovers the server on the local network via Bonjour automatically.

**Requirements:** Xcode 15+, [XcodeGen](https://github.com/yonaskolb/XcodeGen)

```bash
# Install XcodeGen once
brew install xcodegen

cd client-ios

# Generate the Xcode project (re-run whenever project.yml changes)
xcodegen generate

# Open in Xcode
open TwoGreenSuns.xcodeproj
```

Then build and run on a simulator or device from Xcode.

---

## Web client

Next.js 15 dashboard. Proxies all API calls to the Rust server.

**Requirements:** Node.js, npm

```bash
cd client-web

# Install dependencies
npm install

# Configure the server URL
echo "TWOGREENSUNS_SERVER_URL=http://twogreensuns.local:8080" > .env.local

# Run in development
npm run dev
```

Open [http://localhost:3000](http://localhost:3000).

---

## Android client

Jetpack Compose app, minSdk 26. Server URL is set at build time via `BuildConfig`.

**Requirements:** Android Studio, JDK 17+

1. Open the `client-android/` folder in Android Studio.
2. Edit `app/build.gradle.kts` and set `SERVER_URL` to your server address.
3. Build and run on an emulator or device.


## Run on py with tmux
ssh rasp-beau1
tmux attach -t tgs        # or: tmux a
tmux ls  
We exit with crtl-B d

for a new tmux session
tmux new -s tgs 
