use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post, put},
    Router,
};
use serde_json::{json, Value};
use tracing::{debug, info};

use crate::app_state::AppState;
use crate::configuration::TwoGreenSunsDispatcherConfiguration;

pub async fn http_api_run(state: Arc<AppState>, port: u16) {
    debug!("[http_api:http_api_run] Enter");
    debug!("  --> binding on port {}", port);

    let app = Router::new()
        .route("/api/status", get(handle_status))
        .route("/api/config", get(handle_get_config))
        .route("/api/config", put(handle_put_config))
        .route("/api/log/level", put(handle_put_log_level))
        .route("/api/tesla/discover", post(handle_tesla_discover))
        .route("/api/tesla/charge/start", post(handle_tesla_start))
        .route("/api/tesla/charge/stop", post(handle_tesla_stop))
        .route("/api/tesla/charge/amps/:n", post(handle_tesla_set_amps))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", port))
        .await
        .expect("Failed to bind HTTP port");

    info!("[http_api:http_api_run] HTTP API listening on port {}", port);
    axum::serve(listener, app).await.expect("HTTP server error");
}

async fn handle_status(State(state): State<Arc<AppState>>) -> Json<Value> {
    debug!("[http_api:handle_status] Enter");
    let inverter = state.inverter_state.read().await.clone();
    let tesla = state.tesla_state.read().await.clone();
    debug!("[http_api:handle_status] Exit");
    Json(json!({
        "inverter": inverter,
        "tesla": tesla,
    }))
}

async fn handle_get_config(
    State(state): State<Arc<AppState>>,
) -> Json<TwoGreenSunsDispatcherConfiguration> {
    debug!("[http_api:handle_get_config] Enter");
    let cfg = state.config.read().await.clone();
    debug!("[http_api:handle_get_config] Exit");
    Json(cfg)
}

async fn handle_put_config(
    State(state): State<Arc<AppState>>,
    Json(new_cfg): Json<TwoGreenSunsDispatcherConfiguration>,
) -> StatusCode {
    debug!("[http_api:handle_put_config] Enter");
    debug!("  --> writing new configuration");
    let mut cfg = state.config.write().await;
    *cfg = new_cfg;
    cfg.save();
    info!("[http_api:handle_put_config] Configuration updated and saved");
    debug!("[http_api:handle_put_config] Exit");
    StatusCode::OK
}

async fn handle_put_log_level(
    State(state): State<Arc<AppState>>,
    Json(body): Json<Value>,
) -> (StatusCode, Json<Value>) {
    debug!("[http_api:handle_put_log_level] Enter");
    let level = match body["level"].as_str() {
        Some(l) => l.to_string(),
        None => {
            debug!("[http_api:handle_put_log_level] Exit (missing 'level' field)");
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({ "error": "missing 'level' field" })),
            );
        }
    };
    debug!("  --> requested level = '{}'", level);
    match (state.log_level_setter)(&level) {
        Ok(()) => {
            info!("[http_api:handle_put_log_level] Log level set to '{}'", level);
            debug!("[http_api:handle_put_log_level] Exit");
            (StatusCode::OK, Json(json!({ "level": level })))
        }
        Err(e) => {
            debug!("[http_api:handle_put_log_level] Exit (error: {})", e);
            (StatusCode::BAD_REQUEST, Json(json!({ "error": e })))
        }
    }
}

async fn handle_tesla_discover(State(state): State<Arc<AppState>>) -> StatusCode {
    debug!("[http_api:handle_tesla_discover] Enter");
    let vin = {
        let cfg = state.config.read().await;
        cfg.tesla_vin.clone().unwrap_or_default()
    };
    debug!("  --> discovering VIN '{}'", vin);
    let mut tesla = state.tesla.lock().await;
    let result = match tesla.discover(&vin).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    debug!("[http_api:handle_tesla_discover] Exit");
    result
}

async fn handle_tesla_start(State(state): State<Arc<AppState>>) -> StatusCode {
    debug!("[http_api:handle_tesla_start] Enter");
    let tesla = state.tesla.lock().await;
    let result = match tesla.start_charging().await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    debug!("[http_api:handle_tesla_start] Exit");
    result
}

async fn handle_tesla_stop(State(state): State<Arc<AppState>>) -> StatusCode {
    debug!("[http_api:handle_tesla_stop] Enter");
    let tesla = state.tesla.lock().await;
    let result = match tesla.stop_charging().await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    debug!("[http_api:handle_tesla_stop] Exit");
    result
}

async fn handle_tesla_set_amps(
    State(state): State<Arc<AppState>>,
    Path(n): Path<u8>,
) -> StatusCode {
    debug!("[http_api:handle_tesla_set_amps] Enter");
    debug!("  --> requested amps = {}", n);
    let tesla = state.tesla.lock().await;
    let result = match tesla.set_charging_amps(n).await {
        Ok(_) => StatusCode::OK,
        Err(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    debug!("[http_api:handle_tesla_set_amps] Exit");
    result
}
