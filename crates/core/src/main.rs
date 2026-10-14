//! patientspace-core — the facility binary.
//!
//! Runs on a NUC at the facility (e.g., Petauke District Hospital). Exposes
//! a localhost HTTP API for the Svelte frontend, serves FHIR REST endpoints,
//! runs the PSEP sync agent in the background, embeds DuckDB for analytics.
//!
//! v0.1: boots an axum server with a `/health` endpoint and a stub `/fhir/Patient`
//! GET. Real implementation lands incrementally.

use axum::{routing::get, Router};
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

mod config;
mod http;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")))
        .init();

    let cfg = config::Config::load()?;
    tracing::info!(
        facility_id = %cfg.facility_id,
        bind = %cfg.bind,
        "patientspace-core starting"
    );

    let app = Router::new()
        .route("/health", get(http::health))
        .route("/fhir/Patient", get(http::list_patients).post(http::create_patient))
        .route("/fhir/Patient/{id}", get(http::get_patient));

    let addr: SocketAddr = cfg.bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, app).await?;

    Ok(())
}
