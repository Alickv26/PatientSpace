//! patientspace-hub — the regional/national hub for the PatientSpace sync protocol.
//!
//! This crate provides both a library (used by integration tests and any
//! in-process hub consumer) and a binary (`patientspace-hub`) that boots an
//! axum server implementing the PSEP §7 endpoints.
//!
//! # Layout
//!
//! - [`endpoints`] — the four PSEP endpoints (handshake / push / pull / pull-ack).
//! - [`registry`] — the [`FacilityRegistry`] trait + an in-memory impl.
//! - [`state`] — the [`HubState`] shared by all endpoints.

pub mod endpoints;
pub mod registry;
pub mod state;

pub use registry::{FacilityRegistry, InMemoryFacilityRegistry, SharedRegistry};
pub use state::HubState;
