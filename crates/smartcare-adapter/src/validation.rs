//! Migration validation report.
//!
//! After ingesting SmartCare data into PatientSpace, this module produces
//! a report comparing counts and detecting data quality issues.

use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct ValidationReport {
    pub smartcare_patient_count: u32,
    pub patientspace_patient_count: u32,
    pub missing: u32,
    pub duplicates_resolved: u32,
    pub status: ValidationStatus,
    pub details: Vec<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub enum ValidationStatus {
    Pass,
    Warn,
    Fail,
}

impl ValidationReport {
    pub fn new(
        smartcare_count: u32,
        patientspace_count: u32,
        duplicates: u32,
    ) -> Self {
        let missing = smartcare_count.saturating_sub(patientspace_count + duplicates);
        let (status, mut details) = if missing > 0 {
            (
                ValidationStatus::Fail,
                vec![format!(
                    "FAIL: {} patients from SmartCare are missing in PatientSpace",
                    missing
                )],
            )
        } else if duplicates > 0 {
            (
                ValidationStatus::Warn,
                vec![format!(
                    "WARN: {} duplicate patients resolved via Smartcard + NRC dedup",
                    duplicates
                )],
            )
        } else {
            (ValidationStatus::Pass, vec!["PASS: all patients migrated successfully".to_string()])
        };

        details.push(format!(
            "SmartCare source: {} patients | PatientSpace: {} patients | duplicates: {}",
            smartcare_count, patientspace_count, duplicates
        ));

        ValidationReport {
            smartcare_patient_count: smartcare_count,
            patientspace_patient_count: patientspace_count,
            missing,
            duplicates_resolved: duplicates,
            status,
            details,
        }
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_when_counts_match() {
        let report = ValidationReport::new(100, 100, 0);
        assert_eq!(report.status, ValidationStatus::Pass);
        assert_eq!(report.missing, 0);
    }

    #[test]
    fn warn_when_duplicates_resolved() {
        let report = ValidationReport::new(100, 98, 2);
        assert_eq!(report.status, ValidationStatus::Warn);
        assert_eq!(report.duplicates_resolved, 2);
    }

    #[test]
    fn fail_when_patients_missing() {
        let report = ValidationReport::new(100, 95, 0);
        assert_eq!(report.status, ValidationStatus::Fail);
        assert_eq!(report.missing, 5);
    }
}
