//! Shared types, identifiers, and error handling for PatientSpace.
//!
//! Everything that needs to be visible across multiple crates lives here.
//! Domain-specific types belong in their owning crate (e.g. FHIR resources
//! in `patientspace-fhir`, sync envelopes in `patientspace-sync`).

use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;
use uuid::Uuid;

/// A PatientSpace facility identifier.
///
/// Format: `F-XXXXXXXX` where the 8 chars are base32 (Crockford, no I/L/O/U).
/// Generated at first boot, never reused.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FacilityId(pub String);

impl FacilityId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FacilityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A PatientSpace clinician identifier.
///
/// Format: `C-UUIDv7`. Clinician identities are issued by their facility
/// (delegated signing key) and recorded for audit in every authored version.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ClinicianId(pub Uuid);

impl ClinicianId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl fmt::Display for ClinicianId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "C-{}", self.0)
    }
}

impl Default for ClinicianId {
    fn default() -> Self {
        Self::new()
    }
}

/// A PatientSpace resource identifier.
///
/// Format: `U-UUIDv7`. Globally unique, assigned at first creation of the
/// resource, never reused or reassigned.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResourceId(pub Uuid);

impl ResourceId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl fmt::Display for ResourceId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "U-{}", self.0)
    }
}

impl Default for ResourceId {
    fn default() -> Self {
        Self::new()
    }
}

/// A PatientSpace version identifier.
///
/// Format: `V-UUIDv7`. Each version of a resource gets a new id. Versions are
/// immutable once authored.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VersionId(pub Uuid);

impl VersionId {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl fmt::Display for VersionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "V-{}", self.0)
    }
}

impl Default for VersionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Resource kind. Affects merge rules and retention policy (see PSEP §2.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Clinical,
    Identity,
    Audit,
    System,
}

impl ResourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clinical => "clinical",
            Self::Identity => "identity",
            Self::Audit => "audit",
            Self::System => "system",
        }
    }
}

/// The top-level error type used across PatientSpace crates.
#[derive(Debug, Error)]
pub enum PSError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("crypto error: {0}")]
    Crypto(String),

    #[error("database error: {0}")]
    Db(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("internal: {0}")]
    Internal(#[from] anyhow::Error),
}

pub type PSResult<T> = Result<T, PSError>;
