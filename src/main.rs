#![cfg_attr(
    all(target_os = "windows", not(debug_assertions)),
    windows_subsystem = "windows"
)]

mod config;
mod printer;
mod system_printer;
mod tray;

use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderValue, Method, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use std::{future::Future, sync::Arc};
use tracing::info;

use config::AgentConfig;
use printer::PrinterManager;

#[derive(Clone)]
struct AppState {
    printer: Arc<PrinterManager>,
}

fn main() {
    init_tracing();

    let config = AgentConfig::load();
    let state = AppState {
        printer: Arc::new(PrinterManager::new(config.clone())),
    };

    // The tray event loop must run on the main thread on all platforms.
    tray::run(config, state);
}

fn init_tracing() {
    let filter = std::env::var("RUST_LOG").unwrap_or_else(|_| "efact_printer_agent=info".into());
    let log_dir = tray::log_dir();
    let _ = std::fs::create_dir_all(&log_dir);

    let file_appender = tracing_appender::rolling::never(log_dir, "agent.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_ansi(false)
        .with_writer(non_blocking)
        .init();

    // Keep the guard alive for the process lifetime.
    std::mem::forget(guard);
}

pub(crate) async fn run_server(
    state: AppState,
    port: u16,
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> std::io::Result<()> {
    let app = Router::new()
        .route("/health", get(health))
        .route("/printers", get(list_printers))
        .route("/print", post(print_raw))
        .layer(axum::middleware::from_fn(cors_and_pna))
        .with_state(state);

    let addr = format!("127.0.0.1:{port}");
    info!("efact-printer-agent listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown)
        .await
        .unwrap();

    Ok(())
}

/// Unified CORS + Chrome Private Network Access middleware.
/// CorsLayer from tower_http short-circuits OPTIONS responses internally,
/// making it impossible to inject PNA headers from an outer middleware.
/// This middleware handles both concerns in one place.
async fn cors_and_pna(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let is_preflight = req.method() == Method::OPTIONS;

    // For preflights, skip the handler entirely and build the response here.
    if is_preflight {
        let mut res = StatusCode::NO_CONTENT.into_response();
        let h = res.headers_mut();
        h.insert("Access-Control-Allow-Origin",          HeaderValue::from_static("*"));
        h.insert("Access-Control-Allow-Methods",         HeaderValue::from_static("GET, POST, OPTIONS"));
        h.insert("Access-Control-Allow-Headers",         HeaderValue::from_static("*"));
        h.insert("Access-Control-Allow-Private-Network", HeaderValue::from_static("true"));
        return res;
    }

    let mut response = next.run(req).await;
    let h = response.headers_mut();
    h.insert("Access-Control-Allow-Origin",          HeaderValue::from_static("*"));
    h.insert("Access-Control-Allow-Private-Network", HeaderValue::from_static("true"));
    response
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
