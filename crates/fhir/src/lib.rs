//! FHIR R5 resource types and serialization for PatientSpace.
//!
//! We model only the FHIR resources we actually need for v0.1, not the entire
//! R5 specification. The full R5 spec is ~150 resources; we'll need ~20 for
//! the Petauke pilot. Each resource is added here when its crate needs it.
//!
//! All resources implement `serde::Serialize` / `Deserialize` so they can be
//! round-tripped through CBOR (for PSEP envelopes) and JSON (for the FHIR
//! REST API facade).

pub mod patient;
pub mod types;

pub use patient::Patient;
