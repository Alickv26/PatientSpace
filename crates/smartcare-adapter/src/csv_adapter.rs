//! CSV adapter — reads SmartCare patient data from CSV export files.
//!
//! SmartCare Legacy and SmartCare Plus can export patient data as CSV.
//! This adapter reads those CSV files and converts them to FHIR Patients.
//!
//! Expected CSV columns (flexible — missing columns are tolerated):
//!   - smartcard_no, nrc, surname, firstname, othernames,
//!     gender, birthdate, phone, district, address

use std::path::Path;

use patientspace_fhir::Patient;
use patientspace_fhir::types::{
    AdministrativeGender, Address, ContactPoint, Extension, HumanName, Identifier,
};
use serde::Deserialize;
use tracing::{info, warn};

use crate::{AdapterError, SmartCareIngestAdapter};

/// Configuration for the CSV adapter.
#[derive(Debug, Clone, Deserialize)]
pub struct CsvAdapterConfig {
    /// Path to the CSV file containing patient data.
    pub file_path: String,
    /// Whether the CSV has a header row.
    #[serde(default = "default_has_header")]
    pub has_header: bool,
}

fn default_has_header() -> bool {
    true
}

/// A row in the SmartCare CSV export.
#[derive(Debug, Deserialize)]
struct CsvPatient {
    #[serde(rename = "smartcard_no")]
    smartcard_no: Option<String>,
    #[serde(rename = "nrc")]
    nrc: Option<String>,
    #[serde(rename = "surname")]
    surname: Option<String>,
    #[serde(rename = "firstname")]
    firstname: Option<String>,
    #[serde(rename = "othernames")]
    othernames: Option<String>,
    #[serde(rename = "gender")]
    gender: Option<String>,
    #[serde(rename = "birthdate")]
    birthdate: Option<String>,
    #[serde(rename = "phone")]
    phone: Option<String>,
    #[serde(rename = "district")]
    district: Option<String>,
    #[serde(rename = "address")]
    address: Option<String>,
}

/// CSV adapter — reads patients from a SmartCare CSV export.
pub struct SmartCareCsvAdapter {
    config: CsvAdapterConfig,
    patients: Vec<Patient>,
    loaded: bool,
}

impl SmartCareCsvAdapter {
    pub fn new(config: CsvAdapterConfig) -> Self {
        Self {
            config,
            patients: Vec::new(),
            loaded: false,
        }
    }

    fn load(&mut self) -> Result<(), AdapterError> {
        let path = Path::new(&self.config.file_path);
        if !path.exists() {
            return Err(AdapterError::Connection(format!(
                "CSV file not found: {}",
                self.config.file_path
            )));
        }

        info!(file = %self.config.file_path, "loading CSV file");

        let mut rdr = csv::ReaderBuilder::new()
            .has_headers(self.config.has_header)
            .flexible(true)
            .from_path(path)
            .map_err(|e| AdapterError::Connection(format!("failed to open CSV: {e}")))?;

        let mut count = 0;
        for result in rdr.deserialize() {
            let row: CsvPatient = match result {
                Ok(r) => r,
                Err(e) => {
                    warn!(error = %e, "failed to parse CSV row, skipping");
                    continue;
                }
            };

            let patient = map_csv_to_patient(&row);
            self.patients.push(patient);
            count += 1;
        }

        info!(count, "loaded {} patients from CSV", count);
        self.loaded = true;
        Ok(())
    }
}

impl SmartCareIngestAdapter for SmartCareCsvAdapter {
    fn iter_patients(&mut self) -> Box<dyn Iterator<Item = Result<Patient, AdapterError>> + '_> {
        if !self.loaded {
            if let Err(e) = self.load() {
                return Box::new(std::iter::once(Err(e)));
            }
        }
        let patients = std::mem::take(&mut self.patients);
        Box::new(patients.into_iter().map(Ok))
    }
}

fn map_csv_to_patient(row: &CsvPatient) -> Patient {
    let resource_id = patientspace_shared::ResourceId::new();
    let mut patient = Patient::new(resource_id.to_string());

    let mut identifiers = Vec::new();
    if let Some(ref sc) = row.smartcard_no {
        if !sc.is_empty() {
            identifiers.push(Identifier {
                system: Some("http://patientspace.org/sid/smartcard".to_string()),
                value: sc.clone(),
                use_: None,
            });
        }
    }
    if let Some(ref nrc) = row.nrc {
        if !nrc.is_empty() {
            identifiers.push(Identifier {
                system: Some("http://patientspace.org/sid/nrc-zm".to_string()),
                value: nrc.clone(),
                use_: None,
            });
        }
    }
    if !identifiers.is_empty() {
        patient.identifier = identifiers;
    }

    // Build name.
    let given: Vec<String> = row
        .firstname
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(|s| {
            let mut names = vec![s.clone()];
            if let Some(ref other) = row.othernames {
                if !other.is_empty() {
                    names.extend(other.split_whitespace().map(String::from));
                }
            }
            names
        })
        .unwrap_or_default();

    let family = row.surname.as_ref().filter(|s| !s.is_empty()).cloned();

    if !given.is_empty() || family.is_some() {
        let text = [family.as_deref(), given.first().map(|s| s.as_str())]
            .iter()
            .filter_map(|s| *s)
            .collect::<Vec<_>>()
            .join(", ");
        patient.name = vec![HumanName {
            text: if text.is_empty() { None } else { Some(text) },
            given,
            family,
            use_: Some("official".to_string()),
        }];
    }

    // Gender.
    if let Some(ref g) = row.gender {
        patient.gender = match g.to_lowercase().as_str() {
            "m" | "male" => Some(AdministrativeGender::Male),
            "f" | "female" => Some(AdministrativeGender::Female),
            _ => Some(AdministrativeGender::Unknown),
        };
    }

    // Birth date.
    if let Some(ref bd) = row.birthdate {
        if !bd.is_empty() {
            patient.birth_date = Some(bd.clone());
        }
    }

    // Phone.
    if let Some(ref ph) = row.phone {
        if !ph.is_empty() {
            patient.telecom = vec![ContactPoint {
                system: Some("phone".to_string()),
                value: ph.clone(),
                use_: Some("mobile".to_string()),
            }];
        }
    }

    // Address.
    if let Some(ref district) = row.district {
        if !district.is_empty() {
            patient.address = vec![Address {
                line: row.address.as_ref().filter(|s| !s.is_empty()).map(|s| vec![s.clone()]).unwrap_or_default(),
                city: Some(district.clone()),
                district: Some(district.clone()),
                state: None,
                postal_code: None,
                country: Some("Zambia".to_string()),
                use_: Some("home".to_string()),
            }];
        }
    }

    // Provenance extension.
    patient.extension = vec![Extension {
        url: "http://patientspace.org/fhir/StructureDefinition/migrated-from-smartcare".to_string(),
        value_string: Some("csv".to_string()),
        value_base64_binary: None,
    }];

    patient
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_csv_row() {
        let row = CsvPatient {
            smartcard_no: Some("SC-123".to_string()),
            nrc: Some("123456/78/1".to_string()),
            surname: Some("Banda".to_string()),
            firstname: Some("Chipo".to_string()),
            othernames: None,
            gender: Some("female".to_string()),
            birthdate: Some("1990-04-12".to_string()),
            phone: Some("+260971234567".to_string()),
            district: Some("Petauke".to_string()),
            address: None,
        };

        let patient = map_csv_to_patient(&row);
        assert_eq!(patient.resource_type, "Patient");
        assert_eq!(patient.identifier.len(), 2);
        assert_eq!(patient.name[0].family.as_deref(), Some("Banda"));
        assert_eq!(patient.gender, Some(AdministrativeGender::Female));
        assert_eq!(patient.birth_date.as_deref(), Some("1990-04-12"));
    }
}
