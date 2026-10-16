//! Shared FHIR data types — small subset of R5 primitives and complex types.

use serde::{Deserialize, Serialize};

/// FHIR `code` — a string constrained to a value set. We model as a newtype
/// so we can attach validation later (e.g. ICD-10, LOINC, ATC).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Code(pub String);

impl Code {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }
}

/// FHIR `uri`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Uri(pub String);

/// FHIR `id` — alphanumeric + '-' + '.', 1..64 chars.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Id(pub String);

/// FHIR `instant` — ISO 8601 with timezone, second precision.
///
/// We store as `chrono::DateTime<Utc>` internally and serialize as RFC 3339.
pub type Instant = chrono::DateTime<chrono::Utc>;

/// FHIR `dateTime` — date or date+time, no timezone.
///
/// We always store a full date+time in Utc; partial dates get expanded to
/// midnight Utc on the given date. This is a simplification; FHIR allows
/// partial dates which we'd need to model properly in v0.2.
pub type DateTime = chrono::DateTime<chrono::Utc>;

/// FHIR `Identifier` — an identifier with a system + value, plus optional
/// use and type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identifier {
    /// Typical values: `http://patientspace.org/sid/smartcard`,
    /// `http://patientspace.org/sid/nrc-zm`, etc.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system: Option<String>,
    pub value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub use_: Option<String>,
}

/// FHIR `HumanName`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HumanName {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub given: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "use")]
    pub use_: Option<String>,
}

/// FHIR `ContactPoint` (phone, email, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContactPoint {
    /// `phone` | `email` | `sms` | ...
    #[serde(skip_serializing_if = "Option::is_none", rename = "system")]
    pub system: Option<String>,
    /// `home` | `work` | `mobile` | ...
    #[serde(skip_serializing_if = "Option::is_none", rename = "use")]
    pub use_: Option<String>,
    pub value: String,
}

/// FHIR `Address`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Address {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub line: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub city: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub district: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub postal_code: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "use")]
    pub use_: Option<String>,
}

/// FHIR `AdministrativeGender` — `male` | `female` | `other` | `unknown`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdministrativeGender {
    Male,
    Female,
    Other,
    Unknown,
}

/// FHIR `Extension` — used for PatientSpace-specific fields like
/// `biometric-template` and `provenance: migrated-from-smartcare`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Extension {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_string: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value_base64_binary: Option<String>,
}
