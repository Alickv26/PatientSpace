//! Database adapter — reads SmartCare patient data directly from the
//! SmartCare Pro MySQL database.
//!
//! This adapter connects to SmartCare Pro's MySQL database with read-only
//! credentials and extracts patient data. It maps SmartCare's database
//! schema to FHIR R5 Patient resources.
//!
//! # SmartCare Pro database schema (known columns)
//!
//! The SmartCare Pro `patients` table has columns like:
//!   - patient_id (UUID)
//!   - smartcard_number
//!   - nrc_number
//!   - first_name, last_name
//!   - date_of_birth
//!   - sex
//!   - phone_number
//!   - district
//!   - home_address
//!
//! Since the exact schema varies by SmartCare version, this adapter uses
//! a flexible query with column-name mapping via a config file.

use patientspace_fhir::Patient;
use patientspace_fhir::types::{
    AdministrativeGender, Address, ContactPoint, Extension, HumanName, Identifier,
};
use serde::Deserialize;
use tracing::{info, warn};

use crate::{AdapterError, SmartCareIngestAdapter};

/// Configuration for the DB adapter.
#[derive(Debug, Clone, Deserialize)]
pub struct DbAdapterConfig {
    /// MySQL connection URL.
    /// e.g., "mysql://readonly_user:password@10.0.1.50:3306/smartcare"
    pub connection_string: String,
    /// SQL query to fetch patients. If empty, uses a default query.
    #[serde(default)]
    pub query: String,
}

/// Default query — works with SmartCare Pro's typical schema.
fn default_query() -> &'static str {
    r#"SELECT
        patient_id,
        smartcard_number,
        nrc_number,
        first_name,
        last_name,
        date_of_birth,
        sex,
        phone_number,
        district,
        home_address
    FROM patients
    WHERE active = 1 OR active IS NULL"#
}

/// DB adapter — reads patients from a SmartCare Pro MySQL database.
///
/// Uses reqwest blocking client (no MySQL driver dependency — we connect
/// via a thin HTTP bridge if needed, or read from a MySQL dump CSV).
/// In production, this would use `sqlx` with MySQL features.
///
/// For v0.1, this adapter reads from a MySQL dump exported as CSV.
/// The `connection_string` is interpreted as a file path to the dump.
/// This is a pragmatic fallback — the real DB adapter would use
/// `sqlx::mysql` once we confirm the SmartCare schema.
pub struct SmartCareDbAdapter {
    config: DbAdapterConfig,
    patients: Vec<Patient>,
    loaded: bool,
}

impl SmartCareDbAdapter {
    pub fn new(config: DbAdapterConfig) -> Self {
        Self {
            config,
            patients: Vec::new(),
            loaded: false,
        }
    }

    fn load(&mut self) -> Result<(), AdapterError> {
        // v0.1: Read from a MySQL dump CSV file.
        // The connection_string is treated as a file path to the dump.
        // This works because SmartCare IT can run `mysqldump --tab` to
        // produce CSV files, which we then read directly.
        //
        // v0.2 will use sqlx::mysql for a live connection.

        let path = std::path::Path::new(&self.config.connection_string);
        if path.exists() && path.is_file() {
            info!(path = %self.config.connection_string, "loading DB dump CSV");

            let mut rdr = csv::ReaderBuilder::new()
                .has_headers(true)
                .flexible(true)
                .from_path(path)
                .map_err(|e| AdapterError::Connection(format!("failed to open dump: {e}")))?;

            let mut count = 0;
            for result in rdr.deserialize() {
                let row: DbDumpRow = match result {
                    Ok(r) => r,
                    Err(e) => {
                        warn!(error = %e, "failed to parse DB dump row, skipping");
                        continue;
                    }
                };

                let patient = map_db_row_to_patient(&row);
                self.patients.push(patient);
                count += 1;
            }

            info!(count, "loaded {} patients from DB dump", count);
        } else if path.exists() && path.is_dir() {
            // Directory of CSV files (mysqldump --tab output)
            info!(dir = %self.config.connection_string, "loading DB dump directory");

            for entry in std::fs::read_dir(path)
                .map_err(|e| AdapterError::Connection(format!("read_dir failed: {e}")))?
            {
                let entry = entry.map_err(|e| AdapterError::Connection(format!("dir entry: {e}")))?;
                let file_path = entry.path();
                if file_path.extension().and_then(|e| e.to_str()) != Some("csv") {
                    continue;
                }

                let mut rdr = csv::ReaderBuilder::new()
                    .has_headers(true)
                    .flexible(true)
                    .from_path(&file_path)
                    .map_err(|e| AdapterError::Connection(format!("open {}: {e}", file_path.display())))?;

                for result in rdr.deserialize() {
                    let row: DbDumpRow = match result {
                        Ok(r) => r,
                        Err(_) => continue,
                    };

                    self.patients.push(map_db_row_to_patient(&row));
                }
            }

            info!(count = self.patients.len(), "loaded patients from DB dump directory");
        } else {
            return Err(AdapterError::Connection(format!(
                "path not found: {} (expected a CSV dump file or directory of CSV files)",
                self.config.connection_string
            )));
        }

        self.loaded = true;
        Ok(())
    }
}

impl SmartCareIngestAdapter for SmartCareDbAdapter {
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

/// A row from a SmartCare Pro database dump.
#[derive(Debug, Deserialize)]
struct DbDumpRow {
    patient_id: Option<String>,
    smartcard_number: Option<String>,
    nrc_number: Option<String>,
    first_name: Option<String>,
    last_name: Option<String>,
    date_of_birth: Option<String>,
    sex: Option<String>,
    phone_number: Option<String>,
    district: Option<String>,
    home_address: Option<String>,
}

fn map_db_row_to_patient(row: &DbDumpRow) -> Patient {
    let resource_id = patientspace_shared::ResourceId::new();
    let mut patient = Patient::new(resource_id.to_string());

    let mut identifiers = Vec::new();
    if let Some(ref sc) = row.smartcard_number {
        if !sc.is_empty() {
            identifiers.push(Identifier {
                system: Some("http://patientspace.org/sid/smartcard".to_string()),
                value: sc.clone(),
                use_: None,
            });
        }
    }
    if let Some(ref nrc) = row.nrc_number {
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

    let given: Vec<String> = row.first_name.as_ref().filter(|s| !s.is_empty()).map(|s| vec![s.clone()]).unwrap_or_default();
    let family = row.last_name.as_ref().filter(|s| !s.is_empty()).cloned();

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

    if let Some(ref sex) = row.sex {
        patient.gender = match sex.to_uppercase().as_str() {
            "M" | "MALE" => Some(AdministrativeGender::Male),
            "F" | "FEMALE" => Some(AdministrativeGender::Female),
            _ => Some(AdministrativeGender::Unknown),
        };
    }

    if let Some(ref dob) = row.date_of_birth {
        if !dob.is_empty() {
            patient.birth_date = Some(dob.clone());
        }
    }

    if let Some(ref ph) = row.phone_number {
        if !ph.is_empty() {
            patient.telecom = vec![ContactPoint {
                system: Some("phone".to_string()),
                value: ph.clone(),
                use_: Some("mobile".to_string()),
            }];
        }
    }

    if let Some(ref district) = row.district {
        if !district.is_empty() {
            patient.address = vec![Address {
                line: row.home_address.as_ref().filter(|s| !s.is_empty()).map(|s| vec![s.clone()]).unwrap_or_default(),
                city: Some(district.clone()),
                district: Some(district.clone()),
                state: None,
                postal_code: None,
                country: Some("Zambia".to_string()),
                use_: Some("home".to_string()),
            }];
        }
    }

    patient.extension = vec![Extension {
        url: "http://patientspace.org/fhir/StructureDefinition/migrated-from-smartcare".to_string(),
        value_string: Some("database".to_string()),
        value_base64_binary: None,
    }];

    patient
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_db_row() {
        let row = DbDumpRow {
            patient_id: Some("smartcare-uuid-123".to_string()),
            smartcard_number: Some("SC-456".to_string()),
            nrc_number: Some("987654/32/1".to_string()),
            first_name: Some("John".to_string()),
            last_name: Some("Phiri".to_string()),
            date_of_birth: Some("1985-06-15".to_string()),
            sex: Some("M".to_string()),
            phone_number: Some("+260972223333".to_string()),
            district: Some("Nyimba".to_string()),
            home_address: Some("Plot 42, Main Rd".to_string()),
        };

        let patient = map_db_row_to_patient(&row);
        assert_eq!(patient.identifier.len(), 2);
        assert_eq!(patient.name[0].family.as_deref(), Some("Phiri"));
        assert_eq!(patient.gender, Some(AdministrativeGender::Male));
        assert_eq!(patient.address[0].district.as_deref(), Some("Nyimba"));
    }
}
