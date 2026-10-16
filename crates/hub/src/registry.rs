//! Facility registry — maps `facility_id` to that facility's public keys.
//!
//! The hub uses the registry to:
//!   1. Verify Ed25519 signatures on incoming push envelopes.
//!   2. (Future) authorize which facilities may push at all.
//!
//! v0.1 ships an [`InMemoryFacilityRegistry`] — sufficient for tests and
//! single-process hubs. A persistent SQL-backed registry is on the roadmap
//! (P2 — see implementation-plan.md).

use std::collections::HashMap;
use std::sync::Arc;

use patientspace_sync::FacilityPublicKeys;

/// Read-only lookup from facility ID to that facility's published public keys.
///
/// Implementations must be cheap to call (the hub does one lookup per pushed
/// envelope for signature verification).
pub trait FacilityRegistry: Send + Sync {
    /// Returns the public keys for `facility_id`, or `None` if the facility
    /// is not registered with this hub.
    fn get(&self, facility_id: &str) -> Option<FacilityPublicKeys>;
}

/// A simple in-memory registry. Not persistent — data is lost on restart.
///
/// Appropriate for tests and for single-process hubs that load their registry
/// from a config file at boot.
pub struct InMemoryFacilityRegistry {
    facilities: HashMap<String, FacilityPublicKeys>,
}

impl InMemoryFacilityRegistry {
    pub fn new() -> Self {
        Self {
            facilities: HashMap::new(),
        }
    }

    /// Register a facility's public keys.
    pub fn register(&mut self, facility_id: impl Into<String>, keys: FacilityPublicKeys) {
        self.facilities.insert(facility_id.into(), keys);
    }
}

impl Default for InMemoryFacilityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl FacilityRegistry for InMemoryFacilityRegistry {
    fn get(&self, facility_id: &str) -> Option<FacilityPublicKeys> {
        self.facilities.get(facility_id).cloned()
    }
}

/// Type alias for the most common way to share a registry across tasks.
pub type SharedRegistry = Arc<dyn FacilityRegistry>;

#[cfg(test)]
mod tests {
    use super::*;
    use patientspace_sync::FacilityKeypair;

    #[test]
    fn register_then_get_returns_keys() {
        let kp = FacilityKeypair::generate();
        let pub_keys = kp.public_keys();
        let mut reg = InMemoryFacilityRegistry::new();
        reg.register("F-A", pub_keys.clone());
        let got = reg.get("F-A");
        assert!(got.is_some());
        assert_eq!(got.unwrap().verifying, pub_keys.verifying);
    }

    #[test]
    fn get_missing_facility_returns_none() {
        let reg = InMemoryFacilityRegistry::new();
        assert!(reg.get("F-NOPE").is_none());
    }
}
