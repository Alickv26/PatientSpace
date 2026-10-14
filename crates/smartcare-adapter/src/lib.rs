//! SmartCare ingest adapters — used to migrate a facility from SmartCare to
//! PatientSpace.
//!
//! See `docs/sync-protocol.md` §9 for the design.
//!
//! Three adapters will ship with v0.1:
//!   - `SmartCareFhirAdapter` — preferred. Reads from SmartCare Pro's HIE FHIR endpoint.
//!   - `SmartCareDbAdapter`   — direct read-only MySQL connection.
//!   - `SmartCareCsvAdapter`  — fallback for SmartCare Legacy/Plus CSV/XML exports.
//!
//! v0.1 ships the trait + a stub of the FHIR adapter. DB and CSV adapters
//! come once we know what access Petauke IT will give us.

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
}

/// A pluggable SmartCare ingest adapter. Implementations produce a stream of
/// FHIR resources that the migration pipeline converts into PSEP genesis
/// versions (see `docs/sync-protocol.md` §9.2).
pub trait SmartCareIngestAdapter {
    fn iter_patients(&mut self) -> Box<dyn Iterator<Item = Result<Patient, AdapterError>> + '_>;
}
