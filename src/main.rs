mod config;
mod printer;
mod system_printer;

use axum::{
    Router,
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
};
use serde_json::{Value, json};
use std::sync::Arc;
use tower_http::cors::{Any, CorsLayer};
use tracing::info;

use config::AgentConfig;
use printer::PrinterManager;

#[derive(Clone)]
struct AppState {
    printer: Arc<PrinterManager>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "efact_printer_agent=info".into()),
        )
        .init();

    let config = AgentConfig::load();
    let port = config.port;

    let printer = Arc::new(PrinterManager::new(config));
    let state = AppState { printer };

    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/health", get(health))
        .route("/printers", get(list_printers))
        .route("/print", post(print_raw))
        .layer(cors)
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    info!("efact-printer-agent listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}

async fn list_printers(State(state): State<AppState>) -> Json<Value> {
    let printers = state.printer.list();
    Json(json!({ "printers": printers }))
}

async fn print_raw(
    State(state): State<AppState>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, Json<Value>)> {
    if body.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": "empty body" })),
        ));
    }

    state.printer.print(&body).map_err(|e| {
        tracing::error!("Print error: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({ "error": e.to_string() })),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}
