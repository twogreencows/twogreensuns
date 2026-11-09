use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use tracing::{debug, info};
use tracing_subscriber::filter::LevelFilter;

use tokio::sync::oneshot;

use crate::application::TGSApplication;
use crate::blecar::{TGSTeslaCarCommand, TGSBleCarRequest};
use crate::configuration::TGSDispatcherConfiguration;

pub async fn http_api_run(state: Arc<TGSApplication>, port: u16) {
    debug!("[http_api:http_api_run] Enter");
    debug!("  --> binding on port {}", port);

    let app = Router::new()
        .route("/api/v1/runtime", get(handle_get_runtime).put(handle_put_runtime))
        .route("/api/v1/config", get(handle_get_config))
        .route("/api/v1/config", put(handle_put_config))
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

async fn handle_put_config( State(state): State<Arc<TGSApplication>>, Json(updated_configuration): Json<TGSDispatcherConfiguration>) -> StatusCode {
    debug!("[http_api:handle_put_config] Writing new configuration");
    debug!("  --> writing new configuration");
    let mut runtime = state.runtime.write().await;
    runtime.config = updated_configuration;
    runtime.config.save();
    StatusCode::OK
}

#[derive(Deserialize)]
struct TGSRuntimeLogLevelUpdate {
    log_level: String,
}

async fn handle_put_runtime(
    State(state): State<Arc<TGSApplication>>,
    Json(updated_log_level): Json<TGSRuntimeLogLevelUpdate>,
) -> (StatusCode, Json<Value>) {
    debug!("[http_api:handle_put_runtime] Writing new log level");
    debug!("  --> new log level = '{}'", updated_log_level.log_level);
    let new_level: LevelFilter = match updated_log_level.log_level.parse() {
        Ok(l) => l,
        Err(_) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": format!("invalid log level: '{}'", updated_log_level.log_level) })),
            );
        }
    };
    if let Err(e) = (state.log_level_setter)(&updated_log_level.log_level) {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": e })));
    }
    state.runtime.write().await.tgsbox_log_level = new_level;
    (StatusCode::OK, Json(json!({ "log_level": updated_log_level.log_level })))
}
/// Forward a charging command to the BLE task and return the resulting Tesla state.
async fn send_car_command(
    state: &Arc<TGSApplication>,
    command: TGSTeslaCarCommand,
) -> (StatusCode, Json<Value>) {
    let (reply_tx, reply_rx) = oneshot::channel();
    if state
        .blecar_commandchannel_tx
        .send((TGSBleCarRequest::Action(command), reply_tx))
        .await
        .is_err()
    {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "error": "BLE task unavailable (no VIN configured?)" })),
        );
    }
    match reply_rx.await {
        Ok(Ok(tesla_state)) => (
            StatusCode::OK,
            Json(serde_json::to_value(tesla_state).unwrap_or(Value::Null)),
        ),
        Ok(Err(e)) => (
            StatusCode::BAD_GATEWAY,
            Json(json!({ "error": e.to_string() })),
        ),
        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": "no reply from BLE task" })),
        ),
    }
}

#[derive(Deserialize)]
struct CarCommandQuery {
    command: String,
    value: Option<u8>,
}

/// `POST /api/v1/car/command?command=start|stop|amps|limit[&value=N]`
async fn handle_car_command(
    State(state): State<Arc<TGSApplication>>,
    Query(params): Query<CarCommandQuery>,
) -> (StatusCode, Json<Value>) {
    debug!("[http_api:handle_car_command] Enter (command={}, value={:?})", params.command, params.value);
    let command = match (params.command.as_str(), params.value) {
        ("start", _) => TGSTeslaCarCommand::StartCharging,
        ("stop", _) => TGSTeslaCarCommand::StopCharging,
        ("amps", Some(v)) => TGSTeslaCarCommand::SetChargingAmps(v),
        ("limit", Some(v)) => TGSTeslaCarCommand::SetChargeLimit(v),
        ("amps", None) | ("limit", None) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": "missing 'value' query param" })));
        }
        (other, _) => {
            return (StatusCode::BAD_REQUEST, Json(json!({ "error": format!("unknown command '{}'", other) })));
        }
    };
    send_car_command(&state, command).await
}
