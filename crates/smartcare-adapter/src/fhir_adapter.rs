//! FHIR adapter — reads from a SmartCare Pro HIE FHIR R4 endpoint.
//!
//! This is the preferred migration path. Petauke IT gives us an OAuth client
//! and we pull FHIR R4 Patient resources from the SmartCare HIE.
//!
//! The adapter converts FHIR R4 Patient to PatientSpace's internal Patient
//! (which is FHIR R5-compatible). The main difference is `resourceType` field
//! naming and some R4 → R5 field migrations.

use patientspace_fhir::Patient;
use patientspace_fhir::types::{
    AdministrativeGender, Address, ContactPoint, Extension, HumanName, Identifier,
};
use serde::Deserialize;
use tracing::{info, warn};

use crate::{AdapterError, SmartCareIngestAdapter};

/// Configuration for the FHIR adapter.
#[derive(Debug, Clone, Deserialize)]
pub struct FhirAdapterConfig {
    /// Base URL of the SmartCare HIE FHIR endpoint.
    /// e.g., "http://127.0.0.1:9090/fhir"
    pub base_url: String,
    /// OAuth bearer token (if required). Empty = no auth.
    #[serde(default)]
    pub bearer_token: String,
    /// Maximum number of patients to fetch per page.
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page_size() -> u32 {
    100
}

/// FHIR R4 Bundle (simplified — we only need entry[].resource).
#[derive(Debug, Deserialize)]
struct FhirBundle {
    entry: Vec<BundleEntry>,
    total: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct BundleEntry {
    resource: serde_json::Value,
}

/// SmartCare FHIR adapter — reads FHIR R4 Patients from a SmartCare HIE.
pub struct SmartCareFhirAdapter {
    config: FhirAdapterConfig,
    http: reqwest::blocking::Client,
    /// Buffered patients from the last fetch.
    buffer: Vec<Patient>,
    /// Whether we've fetched the first page.
    started: bool,
    /// Total count from the source.
    total: u32,
    /// How many we've returned so far.
    returned: u32,
}

impl SmartCareFhirAdapter {
    pub fn new(config: FhirAdapterConfig) -> Self {
        let http = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .unwrap();
        Self {
            config,
            http,
            buffer: Vec::new(),
            started: false,
            total: 0,
            returned: 0,
        }
    }

    fn fetch_page(&mut self, offset: u32) -> Result<(), AdapterError> {
        let url = format!(
            "{}/Patient?_count={}&_offset={}",
            self.config.base_url, self.config.page_size, offset
        );

        info!(url = %url, "fetching FHIR page");

        let mut req = self.http.get(&url);
        if !self.config.bearer_token.is_empty() {
            req = req.bearer_auth(&self.config.bearer_token);
        }

        let resp = req
            .send()
            .map_err(|e| AdapterError::Connection(e.to_string()))?;

        if !resp.status().is_success() {
            return Err(AdapterError::Connection(format!(
                "HTTP {} from {}",
                resp.status(),
                url
            )));
        }

        let bundle: FhirBundle = resp
            .json()
            .map_err(|e| AdapterError::Query(format!("parse FHIR Bundle: {e}")))?;

        if !self.started {
            self.total = bundle.total.unwrap_or(0);
            self.started = true;
            info!(total = self.total, "SmartCare source has {} patients", self.total);
        }

        for entry in bundle.entry {
            match map_r4_to_patient(entry.resource) {
                Ok(p) => self.buffer.push(p),
                Err(e) => warn!(error = %e, "failed to map patient, skipping"),
            }
        }

        Ok(())
    }
}

impl SmartCareIngestAdapter for SmartCareFhirAdapter {
    fn iter_patients(&mut self) -> Box<dyn Iterator<Item = Result<Patient, AdapterError>> + '_> {
        // We need to return an iterator. Since we do blocking HTTP, we'll
        // use a custom iterator that fetches pages lazily.
        Box::new(SmartCareFhirIterator { adapter: self })
    }
}

/// Iterator that lazily fetches pages from the FHIR endpoint.
struct SmartCareFhirIterator<'a> {
    adapter: &'a mut SmartCareFhirAdapter,
}

impl<'a> Iterator for SmartCareFhirIterator<'a> {
    type Item = Result<Patient, AdapterError>;

    fn next(&mut self) -> Option<Self::Item> {
        // If buffer is empty, fetch next page.
        if self.adapter.buffer.is_empty() {
            if self.adapter.started && self.adapter.returned >= self.adapter.total && self.adapter.total > 0 {
                return None; // done
            }
            let offset = self.adapter.returned;
            match self.adapter.fetch_page(offset) {
                Ok(()) => {}
                Err(e) => return Some(Err(e)),
            }
            if self.adapter.buffer.is_empty() {
                return None; // no more data
            }
        }

        // Pop from buffer.
        let patient = self.adapter.buffer.remove(0);
        self.adapter.returned += 1;
        Some(Ok(patient))
    }
}

/// Map a FHIR R4 Patient JSON to PatientSpace's internal Patient.
///
/// R4 → R5 differences we handle:
/// - `resourceType` stays "Patient"
/// - `id` — we generate a new PatientSpace resource_id (UUIDv7)
/// - `identifier` — same structure
/// - `name` — R4 uses `family` + `given[]`, same as our model
/// - `gender` — same enum
/// - `birthDate` — same string format
fn map_r4_to_patient(r4: serde_json::Value) -> Result<Patient, AdapterError> {
    let obj = r4
        .as_object()
        .ok_or_else(|| AdapterError::Mapping("expected JSON object".to_string()))?;

    // Generate a new PatientSpace resource_id.
    let resource_id = patientspace_shared::ResourceId::new();

    let mut patient = Patient::new(resource_id.to_string());

    // Map identifiers.
    if let Some(identifiers) = obj.get("identifier").and_then(|v| v.as_array()) {
        for id in identifiers {
            let system = id
                .get("system")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let value = id
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let use_ = id
                .get("use")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            patient.identifier.push(Identifier { system, value, use_ });
        }
    }

    // Map names.
    if let Some(names) = obj.get("name").and_then(|v| v.as_array()) {
        for name in names {
            let text = name
                .get("text")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let given: Vec<String> = name
                .get("given")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let family = name
                .get("family")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let use_ = name
                .get("use")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            patient.name.push(HumanName { text, given, family, use_ });
        }
    }

    // Map telecom.
    if let Some(telecom) = obj.get("telecom").and_then(|v| v.as_array()) {
        for tc in telecom {
            let system = tc
                .get("system")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let value = tc
                .get("value")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let use_ = tc
                .get("use")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            patient.telecom.push(ContactPoint { system, value, use_ });
        }
    }

    // Map gender.
    if let Some(gender) = obj.get("gender").and_then(|v| v.as_str()) {
        patient.gender = match gender {
            "male" => Some(AdministrativeGender::Male),
            "female" => Some(AdministrativeGender::Female),
            "other" => Some(AdministrativeGender::Other),
            _ => Some(AdministrativeGender::Unknown),
        };
    }

    // Map birthDate.
    patient.birth_date = obj
        .get("birthDate")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Map address.
    if let Some(addresses) = obj.get("address").and_then(|v| v.as_array()) {
        for addr in addresses {
            let line: Vec<String> = addr
                .get("line")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_string()))
                        .collect()
                })
                .unwrap_or_default();
            let city = addr
                .get("city")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let district = addr
                .get("district")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let state = addr
                .get("state")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let country = addr
                .get("country")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let use_ = addr
                .get("use")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            patient.address.push(Address {
                line,
                city,
                district,
                state,
                postal_code: None,
                country,
                use_,
            });
        }
    }

    // Add provenance extension.
    patient.extension.push(Extension {
        url: "http://patientspace.org/fhir/StructureDefinition/migrated-from-smartcare".to_string(),
        value_string: Some(chrono::Utc::now().to_rfc3339()),
        value_base64_binary: None,
    });

    Ok(patient)
}
