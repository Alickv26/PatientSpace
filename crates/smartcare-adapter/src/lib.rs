//! SmartCare ingest adapters — used to migrate a facility from SmartCare to
//! PatientSpace.
//!
//! See `docs/sync-protocol.md` §9 for the design.
//!
//! Three adapters will ship with v0.1:
//!   - `SmartCareFhirAdapter` — preferred. Reads from SmartCare Pro's HIE FHIR endpoint.
//!   - `SmartCareDbAdapter`   — direct read-only MySQL connection.
//!   - `SmartCareCsvAdapter`  — fallback for SmartCare Legacy/Plus CSV/XML exports.

use patientspace_fhir::Patient;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AdapterError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("source connection error: {0}")]
    Connection(String),

    #[error("source query error: {0}")]
    Query(String),

    #[error("mapping error: {0}")]
    Mapping(String),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("http error: {0}")]
    Http(String),
}

/// A pluggable SmartCare ingest adapter. Implementations produce a stream of
/// FHIR resources that the migration pipeline converts into PSEP genesis
/// versions (see `docs/sync-protocol.md` §9.2).
pub trait SmartCareIngestAdapter {
    fn iter_patients(&mut self) -> Box<dyn Iterator<Item = Result<Patient, AdapterError>> + '_>;
}

/// FHIR adapter — reads from a SmartCare Pro HIE FHIR R4 endpoint.
pub mod fhir_adapter;

/// Mock SmartCare HIE server for testing.
pub mod mock_server;

/// Migration validation report.
pub mod validation;

pub use fhir_adapter::{FhirAdapterConfig, SmartCareFhirAdapter};
pub use validation::{ValidationReport, ValidationStatus};
