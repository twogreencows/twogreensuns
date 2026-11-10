use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};
use serde::Serialize;
use tracing::{debug, error, info};

use crate::blecar::{TGSTeslaCarCommand, TGSTeslaState};
use crate::fronius::TGSInverterDataEntry;

/// One poll cycle: what we read (inverter + car) and what we decided (actions).
#[derive(Debug, Clone, Serialize)]
pub struct TGSEvent {
    pub id: i64,
    pub timestamp: DateTime<Utc>,
    pub inverter: TGSInverterDataEntry,
    pub tesla: Option<TGSTeslaState>, // None when the car was unreachable
    pub actions: Vec<TGSTeslaCarCommand>,
}

pub struct TGSStorage {
    conn: Mutex<Connection>,
}

impl TGSStorage {
    pub fn new(path: &Path) -> rusqlite::Result<Self> {
        debug!("[storage:new] Enter");
        info!("  --> database path: {:?}", path);
        info!("  --> sqlite library version: {}", rusqlite::version());
        debug!("  --> file exists: {}", path.exists());
        match path.parent() {
            Some(dir) => debug!("  --> parent directory {:?} exists: {}", dir, dir.exists()),
            None => error!("  --> database path has no parent directory"),
        }

        let conn = match Connection::open(path) {
            Ok(conn) => {
                info!("  --> database opened (file now exists: {})", path.exists());
                conn
            }
            Err(e) => {
                error!("  --> failed to open database {:?}: {}", path, e);
                return Err(e);
            }
        };

        debug!("  --> creating events table and index if missing");
        if let Err(e) = conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS events (
                 id                          INTEGER PRIMARY KEY AUTOINCREMENT,
                 timestamp                   INTEGER NOT NULL,  -- UTC unix ms
                 inverter_ac_voltage         REAL    NOT NULL,
                 inverter_photovoltaic_power REAL    NOT NULL,
                 inverter_load_power         REAL    NOT NULL,
                 inverter_grid_power         REAL    NOT NULL,
                 inverter_battery_power      REAL    NOT NULL,
                 inverter_battery_soc        REAL    NOT NULL,
                 car_present                 INTEGER,           -- NULL = car unreachable
                 car_connected               INTEGER,
                 car_plugged                 INTEGER,
                 car_charging                INTEGER,
                 car_charging_amps           INTEGER,
                 car_charge_limit_percent    INTEGER,
                 actions                     TEXT    NOT NULL   -- JSON array, '[]' if none
             );
             CREATE INDEX IF NOT EXISTS idx_events_timestamp ON events(timestamp);",
        ) {
            error!("  --> failed to create schema: {}", e);
            return Err(e);
        }
        info!("  --> schema ready");
        debug!("[storage:new] Exit");
        Ok(Self { conn: Mutex::new(conn) })
    }

    /// Insert one event with the given UTC timestamp.
    pub fn write_event(
        &self,
        timestamp: DateTime<Utc>,
        inv: &TGSInverterDataEntry,
        car: Option<&TGSTeslaState>,
        actions: &[TGSTeslaCarCommand],
    ) -> rusqlite::Result<i64> {
        let actions_json = serde_json::to_string(actions)
            .map_err(|e| rusqlite::Error::ToSqlConversionFailure(Box::new(e)))?;
        let conn = self.conn.lock().unwrap();
        let result = conn.execute(
            "INSERT INTO events (timestamp,
                 inverter_ac_voltage, inverter_photovoltaic_power, inverter_load_power,
                 inverter_grid_power, inverter_battery_power, inverter_battery_soc,
                 car_present, car_connected, car_plugged, car_charging,
                 car_charging_amps, car_charge_limit_percent, actions)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
            params![
                timestamp.timestamp_millis(),
                inv.ac_voltage, inv.p_pv, inv.p_load, inv.p_grid, inv.p_akku, inv.battery_soc,
                car.map(|c| c.present), car.map(|c| c.connected), car.map(|c| c.plugged),
                car.map(|c| c.charging), car.map(|c| c.charging_amps),
                car.map(|c| c.charge_limit_percent),
                actions_json,
            ],
        );
        if let Err(e) = result {
            error!("[storage:write_event] insert failed: {}", e);
            return Err(e);
        }
        Ok(conn.last_insert_rowid())
    }

    /// Events with `from <= timestamp < to`, oldest first. A `None` bound is open-ended.
    pub fn query_events(&self, from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> rusqlite::Result<Vec<TGSEvent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, timestamp,
                    inverter_ac_voltage, inverter_photovoltaic_power, inverter_load_power,
                    inverter_grid_power, inverter_battery_power, inverter_battery_soc,
                    car_present, car_connected, car_plugged, car_charging,
                    car_charging_amps, car_charge_limit_percent, actions
             FROM events
             WHERE (?1 IS NULL OR timestamp >= ?1) AND (?2 IS NULL OR timestamp < ?2)
             ORDER BY timestamp ASC",
        )?;
        let rows = stmt.query_map(params![from.map(|t| t.timestamp_millis()), to.map(|t| t.timestamp_millis())], |r| {
            let ts = DateTime::from_timestamp_millis(r.get(1)?).unwrap_or_default();
            let tesla = match r.get::<_, Option<bool>>(8)? {
                None => None,
                Some(present) => Some(TGSTeslaState {
                    present,
                    connected: r.get(9)?,
                    plugged: r.get(10)?,
                    charging: r.get(11)?,
                    charging_amps: r.get(12)?,
                    charge_limit_percent: r.get(13)?,
                }),
            };
            let actions_json: String = r.get(14)?;
            Ok(TGSEvent {
                id: r.get(0)?,
                timestamp: ts,
                inverter: TGSInverterDataEntry {
                    ac_voltage: r.get(2)?,
                    p_pv: r.get(3)?,
                    p_load: r.get(4)?,
                    p_grid: r.get(5)?,
                    p_akku: r.get(6)?,
                    battery_soc: r.get(7)?,
                    timestamp: ts,
                },
                tesla,
                actions: serde_json::from_str(&actions_json).unwrap_or_default(),
            })
        })?;
        let events: rusqlite::Result<Vec<TGSEvent>> = rows.collect();
        match &events {
            Ok(list) => debug!("[storage:query_events] returned {} events", list.len()),
            Err(e) => error!("[storage:query_events] query failed: {}", e),
        }
        events
    }
}
