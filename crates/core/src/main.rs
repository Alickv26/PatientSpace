//! patientspace-core — the facility binary.
//!
//! Runs on a NUC at the facility (e.g., Petauke District Hospital). Exposes
//! a localhost HTTP API for the Svelte frontend, serves FHIR REST endpoints,
//! runs the PSEP sync agent in the background, embeds DuckDB for analytics.
//!
//! v0.1: boots an axum server with `/health`, `POST /fhir/Patient`,
//! `GET /fhir/Patient`, and `GET /fhir/Patient/{id}` wired up to a real
//! PSEP-backed [`patientspace_core::http::AppState`].

use std::sync::Arc;

use axum::{routing::get, Router};
use std::net::SocketAddr;
use tracing_subscriber::EnvFilter;

use patientspace_core::config;
use patientspace_core::http::AppState;
use patientspace_shared::FacilityId;
use patientspace_sync::{EventLog, FacilityKeypair};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = config::Config::load()?;
    tracing::info!(
        facility_id = %cfg.facility_id,
        bind = %cfg.bind,
        "patientspace-core starting"
    );

    // Open the local PSEP event log. For sqlite::memory: this is per-process;
    // production deploys should set PATIENTSPACE_DATABASE_URL to a file path.
    let log = EventLog::open(&cfg.database_url).await?;
    let keypair = Arc::new(FacilityKeypair::generate());
    let facility_id = FacilityId::new(cfg.facility_id.clone());

    let state = AppState {
        facility_id,
        keypair,
        log,
    };

    // Create default admin user on first boot.
    if let Err(e) = patientspace_core::auth::ensure_default_admin(state.log.pool()).await {
        tracing::warn!(error = %e, "failed to create default admin");
    }

    let app = Router::new()
        .route("/health", get(patientspace_core::http::health))
        // Auth
        .route("/api/auth/login", axum::routing::post(patientspace_core::auth::login))
        .route("/api/auth/logout", axum::routing::post(patientspace_core::auth::logout))
        .route("/api/auth/me", get(patientspace_core::auth::me))
        // FHIR Patient
        .route(
            "/fhir/Patient",
            get(patientspace_core::http::list_patients)
                .post(patientspace_core::http::create_patient),
        )
        .route("/fhir/Patient/{id}", get(patientspace_core::http::get_patient))
        // Generic FHIR resource routes
        .route(
            "/fhir/{resource_type}",
            get(patientspace_core::http::list_resources)
                .post(patientspace_core::http::create_resource),
        )
        .route(
            "/fhir/{resource_type}/{id}",
            get(patientspace_core::http::get_resource),
        )
        // Analytics
        .route("/api/dashboards", get(patientspace_core::analytics::get_dashboards))
        .route("/api/dashboards/{id}/run", get(patientspace_core::analytics::run_dashboard))
        // Conflicts
        .route("/api/conflicts", get(patientspace_core::conflicts::list_conflicts))
        .route("/api/conflicts/{resource_id}", get(patientspace_core::conflicts::get_conflict))
        .with_state(state);

    let addr: SocketAddr = cfg.bind.parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!(%addr, "listening");
    axum::serve(listener, app).await?;

    Ok(())
}
