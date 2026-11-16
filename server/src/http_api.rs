use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put},
    Router,
};
use chrono::{DateTime, Duration, NaiveDate, NaiveTime, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{debug, error, info, warn, Span};
use tracing_subscriber::filter::LevelFilter;

use tokio::sync::oneshot;

use crate::application::TGSApplication;
use crate::blecar::{TGSTeslaCarCommand, TGSBleCarRequest, NEXT_BLE_REQUEST_ID};
use crate::configuration::TGSDispatcherConfiguration;
use crate::storage::TGSEvent;

pub async fn http_api_run(state: Arc<TGSApplication>, port: u16) {
    debug!("[http_api:http_api_run] Enter");
    debug!("  --> binding on port {}", port);

    let app = Router::new()
        .route("/api/v1/runtime", get(handle_get_runtime))
        .route("/api/v1/config", get(handle_get_config))
        .route("/api/v1/config", put(handle_put_config))
        .route("/api/v1/events", get(handle_get_events))
        .route("/api/v1/car/command", post(handle_car_command))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind HTTP port");

    info!("[http_api:http_api_run] HTTP API listening on port {}", port);
    axum::serve(listener, app).await.expect("HTTP server error");
}

async fn handle_get_runtime(State(state): State<Arc<TGSApplication>>) -> Json<Value> {
    info!("[http_api:handle_get_runtime] Enter");
    let inverter = state.inverter_state.read().await.clone();
    info!("{:?}", inverter);
    let tesla = state.tesla_state.read().await.clone();
    info!("{:?}", tesla);
    debug!("[http_api:handle_status] Exit");
    Json(json!({
        "inverter": inverter,
        "tesla": tesla,
    }))
}

async fn handle_get_config(
    State(state): State<Arc<TGSApplication>>,
) -> Json<TGSDispatcherConfiguration> {
    info!("[http_api:handle_get_config] Enter");
    let configuration = state.runtime.read().await.config.clone();
    info!("{:?}", configuration);
    info!("[http_api:handle_get_config] Exit");
    Json(configuration)
}

/// Replaces the configuration. `bypass` and `log_level` are optional in the body: when absent the
/// current values are kept. A `log_level` that is present is applied to the running logger as well as stored.
async fn handle_put_config(State(state): State<Arc<TGSApplication>>, Json(body): Json<Value>) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    debug!("[http_api:handle_put_config] Writing new configuration");
    let mut updated_configuration: TGSDispatcherConfiguration = serde_json::from_value(body.clone())
        .map_err(|e| (StatusCode::UNPROCESSABLE_ENTITY, Json(json!({ "error": e.to_string() }))))?;

    // The deserializer turns an unknown level into debug: reject it here instead.
    if let Some(level) = body.get("log_level") {
        if !level.as_str().is_some_and(|text| text.parse::<LevelFilter>().is_ok()) {
            return Err((StatusCode::BAD_REQUEST, Json(json!({ "error": format!("invalid log level: {}", level) }))));
        }
    }

    let mut runtime = state.runtime.write().await;
    if body.get("bypass").is_none() {
        debug!("  --> no bypass in the request, keeping {}", runtime.config.bypass);
        updated_configuration.bypass = runtime.config.bypass;
    }
    if body.get("log_level").is_none() {
        debug!("  --> no log_level in the request, keeping {}", runtime.config.log_level);
        updated_configuration.log_level = runtime.config.log_level;
    } else {
        debug!("  --> applying log level {}", updated_configuration.log_level);
        (state.log_level_setter)(&updated_configuration.log_level.to_string())
            .map_err(|e| (StatusCode::BAD_REQUEST, Json(json!({ "error": e }))))?;
    }
    runtime.config = updated_configuration;
    runtime.config.save();
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct TGSEventsQuery {
    from: Option<String>,
    to: Option<String>,
    date: Option<String>,
}

/// RFC3339 timestamp, or a plain `YYYY-MM-DD` date meaning 00:00:00 UTC.
fn parse_time_bound(text: &str) -> Option<DateTime<Utc>> {
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(text) {
        return Some(timestamp.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(text, "%Y-%m-%d").ok().map(|d| d.and_time(NaiveTime::MIN).and_utc())
}

async fn handle_get_events(
    State(state): State<Arc<TGSApplication>>,
    Query(query): Query<TGSEventsQuery>,
) -> Result<Json<Vec<TGSEvent>>, StatusCode> {
    debug!("[http_api:handle_get_events] Enter from={:?} to={:?} date={:?}", query.from, query.to, query.date);

    let (from, to) = match &query.date {
        Some(day) => {
            if query.from.is_some() || query.to.is_some() {
                return Err(StatusCode::BAD_REQUEST);
            }
            let start = NaiveDate::parse_from_str(day, "%Y-%m-%d")
                .map_err(|_| StatusCode::BAD_REQUEST)?
                .and_time(NaiveTime::MIN)
                .and_utc();
            (Some(start), Some(start + Duration::days(1)))
        }
        None => {
            let parse = |value: &Option<String>| match value {
                None => Ok(None),
                Some(text) => parse_time_bound(text).map(Some).ok_or(StatusCode::BAD_REQUEST),
            };
            (parse(&query.from)?, parse(&query.to)?)
        }
    };

    // The query can return a lot of rows, so keep it off the async worker threads.
    let events = tokio::task::spawn_blocking(move || state.storage.query_events(from, to))
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .map_err(|e| {
            error!("[http_api:handle_get_events] query failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    debug!("[http_api:handle_get_events] Exit ({} events)", events.len());
    Ok(Json(events))
}

/// Forward a request (a state read or a charging command) to the BLE task and return the
/// resulting Tesla state.
///
/// Runs in a `ble_request` span (unique `id`, `source = "http"`) that is sent along with the
/// request, so the BLE task logs the whole trace of this request under the same id.
#[tracing::instrument(name = "ble_request", skip_all, fields(id = NEXT_BLE_REQUEST_ID.fetch_add(1, Ordering::Relaxed), source = "http"))]
async fn send_car_request(
    state: &Arc<TGSApplication>,
    request: TGSBleCarRequest,
) -> (StatusCode, Json<Value>) {
    debug!("[http_api:send_car_request] Enter {:?}", request);
    let started = Instant::now();
    let (reply_tx, reply_rx) = oneshot::channel();
    if state
        .blecar_commandchannel_tx
        .send((request, reply_tx, Span::current()))
        .await
        .is_err()
    {
        warn!("[http_api:send_car_request] BLE task unavailable, request not sent");
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "BLE task unavailable (no VIN configured?)" })),
        );
    }
    debug!("  --> request queued, waiting for the BLE reply");
    match reply_rx.await {
        Ok(Ok(tesla_state)) => {
            debug!("[http_api:send_car_request] Exit OK after {:?}, state {:?}", started.elapsed(), tesla_state);
            (
                StatusCode::OK,
                Json(serde_json::to_value(tesla_state).unwrap_or(Value::Null)),
            )
        }
        Ok(Err(e)) => {
            warn!("[http_api:send_car_request] Exit with BLE error after {:?}: {}", started.elapsed(), e);
            (
                StatusCode::BAD_GATEWAY,
                Json(json!({ "error": e.to_string() })),
            )
        }
        Err(_) => {
            warn!("[http_api:send_car_request] Exit, BLE task dropped the reply after {:?}", started.elapsed());
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": "no reply from BLE task" })),
            )
        }
    }
}

#[derive(Deserialize)]
struct CarCommandQuery {
    command: String,
    value: Option<u8>,
}

/// `POST /api/v1/car/command?command=chargestate|start|stop|amps|limit[&value=N]`
///
/// `chargestate` only reads the car state (`TGSBleCarRequest::ReadState`), no action is sent.
async fn handle_car_command(
    State(state): State<Arc<TGSApplication>>,
    Query(params): Query<CarCommandQuery>,
) -> (StatusCode, Json<Value>) {
    debug!("[http_api:handle_car_command] Enter (command={}, value={:?})", params.command, params.value);
    let request = match (params.command.as_str(), params.value) {
        ("chargestate", _) => TGSBleCarRequest::ReadState,
        ("start", _) => TGSBleCarRequest::Action(TGSTeslaCarCommand::StartCharging),
        ("stop", _) => TGSBleCarRequest::Action(TGSTeslaCarCommand::StopCharging),
        ("amps", Some(v)) => TGSBleCarRequest::Action(TGSTeslaCarCommand::SetChargingAmps(v)),
        ("limit", Some(v)) => TGSBleCarRequest::Action(TGSTeslaCarCommand::SetChargeLimit(v)),
        ("amps", None) | ("limit", None) => {
            warn!("[http_api:handle_car_command] Exit, command '{}' needs a 'value' query param", params.command);
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": "missing 'value' query param" })));
        }
        (other, _) => {
            warn!("[http_api:handle_car_command] Exit, unknown command '{}'", other);
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("unknown command '{}'", other) })));
        }
    };
    debug!("  --> resolved request {:?}", request);
    send_car_request(&state, request).await
}
