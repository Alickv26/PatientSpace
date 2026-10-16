//! patientspace-core library — the facility binary's reusable interior.
//!
//! The binary entrypoint (`src/main.rs`) wires up tracing, config, and the
//! axum router; everything that is genuinely reusable lives here so that
//! integration tests under `tests/` can build an `AppState` and start a
//! server on a random port without re-implementing the wiring.
//!
//! # Modules
//!
//! - [`config`] — loads configuration from `PATIENTSPACE_*` env vars.
//! - [`http`] — axum handlers for the FHIR REST facade (`/health`,
//!   `/fhir/Patient` POST/GET, `/fhir/Patient/{id}` GET) and the shared
//!   [`AppState`][http::AppState] they run against.

pub mod analytics;
pub mod auth;
pub mod config;
pub mod conflicts;
pub mod http;
