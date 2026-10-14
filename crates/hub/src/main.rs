//! patientspace-hub — the regional/national hub binary.
//!
//! Receives PSEP envelopes from facilities, stores them immutably, replays
//! them to other facilities. The hub CANNOT decrypt envelope payloads (only
//! facilities hold the private keys).
//!
//! v0.1: boots an axum server with `/health` and a stub `/psep/v1/handshake`.
//! Real PSEP endpoints land incrementally.

use axum::{routing::{get, post}, Router};
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let bind = std::env::var("PATIENTSPACE_HUB_BIND")
        .unwrap_or_else(|_| "0.0.0.0:8080".to_string());

    let app = Router::new()
        .route("/health", get(health))
        .route("/psep/v1/ping", get(ping))
        .route("/psep/v1/handshake", post(handshake));

    let addr: SocketAddr = bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "patientspace-hub listening");
    axum::serve(listener, app).await?;

    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "patientspace-hub",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}

async fn ping() -> &'static str {
    "pong"
}

async fn handshake() -> Json<serde_json::Value> {
    // v0.1 stub — real implementation in PSEP §7.2.
    Json(serde_json::json!({
        "hub_clock": {},
        "push_size_hint": 0,
        "pull_size_hint": 0,
        "pull_required": false,
        "push_required": false,
        "session_id": "S-stub",
        "session_expiry_unix_ms": 0,
    }))
}

use axum::response::Json;
