//! patientspace-hub binary entrypoint.
//!
//! Boots an axum server that implements the PSEP §7 endpoints. The hub holds
//! an [`EventLog`] (SQLite) of every envelope it has ever received, plus a
//! [`FacilityRegistry`] for verifying envelope signatures.
//!
//! The hub **cannot** decrypt envelope payloads — only facilities hold the
//! private keys (see §5).

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post};
use axum::Router;
use tracing_subscriber::EnvFilter;

use patientspace_hub::endpoints;
use patientspace_hub::registry::{FacilityRegistry, InMemoryFacilityRegistry};
use patientspace_hub::state::HubState;
use patientspace_sync::EventLog;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    // Open the hub's event log.
    let database_url = std::env::var("PATIENTSPACE_HUB_DATABASE_URL")
        .unwrap_or_else(|_| "sqlite:patientspace-hub.db?mode=rwc".to_string());
    let log = EventLog::open(&database_url).await?;

    // v0.1: an empty in-memory registry. Real facilities will be added via a
    // future admin endpoint (or loaded from a config file at boot).
    let registry: Arc<dyn FacilityRegistry> = Arc::new(InMemoryFacilityRegistry::new());
    let state = HubState::new(log, registry);

    let app = Router::new()
        .route("/health", get(health))
        .route("/psep/v1/ping", get(ping))
        .route("/psep/v1/handshake", post(endpoints::handshake))
        .route("/psep/v1/push", post(endpoints::push))
        .route("/psep/v1/pull", post(endpoints::pull))
        .route("/psep/v1/pull-ack", post(endpoints::pull_ack))
        .layer(DefaultBodyLimit::disable())
        .with_state(state);

    let bind = std::env::var("PATIENTSPACE_HUB_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    let addr: SocketAddr = bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "patientspace-hub listening");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "status": "ok",
        "service": "patientspace-hub",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn ping() -> &'static str {
    "pong"
}
