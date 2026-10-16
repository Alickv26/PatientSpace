//! Shared state injected into every hub endpoint via `axum::extract::State`.
//!
//! Cheaply cloneable — both fields are themselves cheap clones (`EventLog`
//! wraps an `Arc<SqlitePool>`; `SharedRegistry` is an `Arc<dyn ...>`).

use crate::registry::SharedRegistry;
use patientspace_sync::EventLog;

/// The shared state of the hub: the event log + the facility registry.
#[derive(Clone)]
pub struct HubState {
    pub log: EventLog,
    pub registry: SharedRegistry,
}

impl HubState {
    pub fn new(log: EventLog, registry: SharedRegistry) -> Self {
        Self { log, registry }
    }
}
