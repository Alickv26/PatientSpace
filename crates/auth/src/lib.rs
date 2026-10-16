//! Authentication, RBAC, and (eventually) biometric matching.
//!
//! v0.1: roles + permissions only. MFA, biometrics, clinician key issuance
//! come later.

use serde::{Deserialize, Serialize};

/// A clinician role. Drives RBAC permission checks in `core` and `hub`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    /// Front-desk / registration clerk. Can create and edit Patient identity resources only.
    Clerk,
    /// Nurse / clinical officer. Can author encounters, observations, medications.
    Nurse,
    /// Doctor. Full clinical authorship including diagnoses and care plans.
    Doctor,
    /// Pharmacist. Can author MedicationDispense, view MedicationRequest.
    Pharmacist,
    /// Lab technician. Can author Observations linked to lab orders.
    LabTech,
    /// Facility administrator. Can manage clinicians, view audit log.
    Admin,
    /// Read-only auditor (e.g. for Medico-Legal Council reviews).
    Auditor,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Clerk => "clerk",
            Role::Nurse => "nurse",
            Role::Doctor => "doctor",
            Role::Pharmacist => "pharmacist",
            Role::LabTech => "lab_tech",
            Role::Admin => "admin",
            Role::Auditor => "auditor",
        }
    }
}

/// A permission. Resources + actions are intentionally coarse in v0.1; we'll
/// refine to FHIR compartments in v0.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    ReadPatient,
    WritePatient,
    ReadEncounter,
    WriteEncounter,
    ReadObservation,
    WriteObservation,
    ReadMedication,
    WriteMedication,
    ViewAuditLog,
    ManageClinicians,
}

impl Role {
    pub fn has_permission(self, perm: Permission) -> bool {
        use Permission::*;
        match self {
            Role::Clerk => matches!(perm, ReadPatient | WritePatient | ReadEncounter),
            Role::Nurse => matches!(
                perm,
                ReadPatient | WritePatient | ReadEncounter | WriteEncounter
                    | ReadObservation | WriteObservation | ReadMedication
            ),
            Role::Doctor => matches!(
                perm,
                ReadPatient | WritePatient | ReadEncounter | WriteEncounter
                    | ReadObservation | WriteObservation
                    | ReadMedication | WriteMedication
            ),
            Role::Pharmacist => matches!(
                perm,
                ReadPatient | ReadMedication | WriteMedication | ReadEncounter
            ),
            Role::LabTech => matches!(
                perm,
                ReadPatient | ReadEncounter | ReadObservation | WriteObservation
            ),
            Role::Admin => matches!(perm, ViewAuditLog | ManageClinicians | ReadPatient),
            Role::Auditor => matches!(
                perm,
                ReadPatient | ReadEncounter | ReadObservation | ReadMedication | ViewAuditLog
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn doctor_can_write_medication_clerk_cannot() {
        assert!(Role::Doctor.has_permission(Permission::WriteMedication));
        assert!(!Role::Clerk.has_permission(Permission::WriteMedication));
    }

    #[test]
    fn auditor_is_read_only() {
        assert!(Role::Auditor.has_permission(Permission::ReadPatient));
        assert!(!Role::Auditor.has_permission(Permission::WritePatient));
    }
}
