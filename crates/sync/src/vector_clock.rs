//! Lamport-style vector clocks for PSEP causality tracking.
//!
//! See `docs/sync-protocol.md` §3 for the design rationale.

use std::collections::BTreeMap;

use patientspace_shared::FacilityId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum VectorClockError {
    #[error("facility id missing from clock")]
    MissingFacility,
}

/// A vector clock: `{ facility_id -> counter }`.
///
/// Counters are monotonically increasing per facility. Used to detect
/// causality between two versions of a resource (happened-before, concurrent).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock(pub BTreeMap<String, u64>);

impl VectorClock {
    /// Create an empty clock.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a clock with a single facility's counter set.
    pub fn from_facility(facility_id: &FacilityId, counter: u64) -> Self {
        let mut map = BTreeMap::new();
        map.insert(facility_id.as_str().to_string(), counter);
        Self(map)
    }

    /// Get the counter for a facility. Returns 0 if absent.
    pub fn get(&self, facility_id: &FacilityId) -> u64 {
        self.0.get(facility_id.as_str()).copied().unwrap_or(0)
    }

    /// Increment the counter for a facility. Returns the new counter value.
    pub fn increment(&mut self, facility_id: &FacilityId) -> u64 {
        let counter = self.0.entry(facility_id.as_str().to_string()).or_insert(0);
        *counter += 1;
        *counter
    }

    /// Set the counter for a facility to a specific value, if it's higher
    /// than the current value. Otherwise no-op.
    pub fn observe(&mut self, facility_id: &FacilityId, counter: u64) {
        let entry = self.0.entry(facility_id.as_str().to_string()).or_insert(0);
        if counter > *entry {
            *entry = counter;
        }
    }

    /// Merge another clock into this one (component-wise max).
    pub fn merge(&mut self, other: &Self) {
        for (k, v) in &other.0 {
            self.observe(&FacilityId::new(k.clone()), *v);
        }
    }

    /// Returns true if this clock happens-before `other`.
    ///
    /// `A < B` ⟺ `∀k: A[k] ≤ B[k]` and `∃k: A[k] < B[k]`.
    pub fn happens_before(&self, other: &Self) -> bool {
        let mut strictly_less = false;
        // All keys in either clock
        let keys: std::collections::BTreeSet<_> = self.0.keys().chain(other.0.keys()).collect();
        for k in keys {
            let a = self.0.get(k).copied().unwrap_or(0);
            let b = other.0.get(k).copied().unwrap_or(0);
            if a > b {
                return false;
            }
            if a < b {
                strictly_less = true;
            }
        }
        strictly_less
    }

    /// Returns true if the two clocks are concurrent (neither happens-before
    /// the other). This is the conflict condition.
    pub fn is_concurrent_with(&self, other: &Self) -> bool {
        !self.happens_before(other) && !other.happens_before(self) && self != other
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fid(s: &str) -> FacilityId {
        FacilityId::new(s)
    }

    #[test]
    fn empty_clocks_are_equal() {
        assert_eq!(VectorClock::new(), VectorClock::new());
    }

    #[test]
    fn happens_before_basic() {
        let mut a = VectorClock::new();
        a.increment(&fid("F-A"));

        let mut b = VectorClock::new();
        b.increment(&fid("F-A"));
        b.increment(&fid("F-A"));
        b.increment(&fid("F-B"));

        assert!(a.happens_before(&b));
        assert!(!b.happens_before(&a));
    }

    #[test]
    fn concurrent_edits_are_detected() {
        let mut a = VectorClock::new();
        a.increment(&fid("F-A"));

        let mut b = VectorClock::new();
        b.increment(&fid("F-B"));

        assert!(a.is_concurrent_with(&b));
        assert!(b.is_concurrent_with(&a));
    }

    #[test]
    fn merge_takes_componentwise_max() {
        let mut a = VectorClock::new();
        a.increment(&fid("F-A"));
        a.increment(&fid("F-A"));
        a.increment(&fid("F-B"));

        let mut b = VectorClock::new();
        b.increment(&fid("F-A"));
        b.increment(&fid("F-A"));
        b.increment(&fid("F-A"));
        b.increment(&fid("F-C"));

        let mut merged = a.clone();
        merged.merge(&b);

        assert_eq!(merged.get(&fid("F-A")), 3);
        assert_eq!(merged.get(&fid("F-B")), 1);
        assert_eq!(merged.get(&fid("F-C")), 1);
    }

    #[test]
    fn equal_clocks_neither_happens_before() {
        let mut a = VectorClock::new();
        a.increment(&fid("F-A"));
        let mut b = VectorClock::new();
        b.increment(&fid("F-A"));

        assert!(!a.happens_before(&b));
        assert!(!b.happens_before(&a));
        assert!(!a.is_concurrent_with(&b));
    }
}
