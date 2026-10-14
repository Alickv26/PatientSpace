//! FHIR R5 `Patient` resource.
//!
//! Spec: https://hl7.org/fhir/R5/patient.html
//!
//! Only the fields used by PatientSpace v0.1 are modeled. Missing fields can
//! be added without breaking the wire format (FHIR is forward-compatible via
//! the `extension` array).

use serde::{Deserialize, Serialize};

use crate::types::{AdministrativeGender, Address, ContactPoint, Extension, HumanName, Identifier};

/// FHIR R5 Patient.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Patient {
    /// FHIR resource type discriminator — always `"Patient"`.
    #[serde(rename = "resourceType")]
    pub resource_type: String,

    /// PatientSpace `resource_id` (UUIDv7). Becomes `Patient.id` on the wire.
    pub id: String,

    /// Patient identifiers — Smartcard No., NRC, etc.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub identifier: Vec<Identifier>,

    /// Whether this patient record is in active use.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,

    /// Patient names. First entry is the "official" name; others may be
    /// "maiden", "nickname", etc.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub name: Vec<HumanName>,

    /// Contact details — phone, email.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub telecom: Vec<ContactPoint>,

    /// Administrative gender.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gender: Option<AdministrativeGender>,

    /// Date of birth — `YYYY-MM-DD`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub birth_date: Option<String>,

    /// Addresses.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub address: Vec<Address>,

    /// Extensions — used for biometric template, provenance, etc.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub extension: Vec<Extension>,
}

impl Patient {
    /// Create a new Patient with the given PatientSpace resource id.
    pub fn new(resource_id: impl Into<String>) -> Self {
        Self {
            resource_type: "Patient".to_string(),
            id: resource_id.into(),
            identifier: vec![],
            active: Some(true),
            name: vec![],
            telecom: vec![],
            gender: None,
            birth_date: None,
            address: vec![],
            extension: vec![],
        }
    }

    /// Find the first identifier matching a given system URL.
    pub fn identifier_for_system(&self, system: &str) -> Option<&str> {
        self.identifier
            .iter()
            .find(|i| i.system.as_deref() == Some(system))
            .map(|i| i.value.as_str())
    }

    /// Convenience: get the Smartcard number (PatientSpace system URL).
    pub fn smartcard_no(&self) -> Option<&str> {
        self.identifier_for_system("http://patientspace.org/sid/smartcard")
    }

    /// Convenience: get the Zambian NRC.
    pub fn nrc(&self) -> Option<&str> {
        self.identifier_for_system("http://patientspace.org/sid/nrc-zm")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patient_serializes_to_fhir_json() {
        let mut p = Patient::new("U-0190abcd-1234-7abc-8def-000000000001");
        p.identifier.push(Identifier {
            system: Some("http://patientspace.org/sid/smartcard".to_string()),
            value: "SC-12345678".to_string(),
            use_: None,
        });
        p.name.push(HumanName {
            text: Some("Banda, Chipo".to_string()),
            given: vec!["Chipo".to_string()],
            family: Some("Banda".to_string()),
            use_: Some("official".to_string()),
        });
        p.gender = Some(AdministrativeGender::Female);
        p.birth_date = Some("1990-04-12".to_string());

        let json = serde_json::to_string_pretty(&p).unwrap();
        assert!(json.contains("\"resourceType\""));
        assert!(json.contains("Patient"));
        assert!(json.contains("SC-12345678"));
        assert!(json.contains("female"));

        let back: Patient = serde_json::from_str(&json).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn patient_smartcard_lookup_works() {
        let mut p = Patient::new("U-test");
        p.identifier.push(Identifier {
            system: Some("http://patientspace.org/sid/nrc-zm".to_string()),
            value: "123456/78/1".to_string(),
            use_: None,
        });
        p.identifier.push(Identifier {
            system: Some("http://patientspace.org/sid/smartcard".to_string()),
            value: "SC-99".to_string(),
            use_: None,
        });

        assert_eq!(p.smartcard_no(), Some("SC-99"));
        assert_eq!(p.nrc(), Some("123456/78/1"));
    }
}
