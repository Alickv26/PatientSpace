//! Conflict detection for PSEP.
//!
//! See `docs/sync-protocol.md` §6. A conflict exists between two versions of
//! the same resource when their vector clocks are concurrent (neither
//! happened-before the other). This is the condition under which both
//! facilities edited the same record without observing each other's edit.
//!
//! `detect_conflicts` runs over the full version history of a single resource
//! and returns every concurrent pair it finds. The caller (typically the FHIR
//! read path) uses the result to annotate the served resource with a
//! `conflict` extension so the clinician UI can surface it for review.

use patientspace_shared::VersionId;

use crate::envelope::Envelope;

/// A pair of concurrent versions of the same resource — a conflict.
///
/// Order of `version_a` / `version_b` is not significant; the pair is
/// unordered. Each pair appears only once in the output of `detect_conflicts`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    pub version_a: VersionId,
    pub version_b: VersionId,
}

/// Detect every concurrent (conflicting) pair among `versions`.
///
/// `versions` is typically the output of `EventLog::list_by_resource(resource_id)`
/// — every version of a single resource that the local facility has observed.
/// Passing envelopes for multiple resources at once is allowed but produces
/// meaningless cross-resource pairs; callers should scope the slice to one
/// resource.
///
/// Runs in O(n²) on the number of versions. For typical PatientSpace usage
/// (a handful of versions per resource, occasionally a few dozen after a long
/// partition) this is fine. If a resource ever accumulates thousands of
/// versions, switch to a sweep-line over the partial order.
pub fn detect_conflicts(versions: &[Envelope]) -> Vec<Conflict> {
    let mut out = Vec::new();
    for i in 0..versions.len() {
        for j in (i + 1)..versions.len() {
            let a = &versions[i];
            let b = &versions[j];
            // Only same-resource pairs are meaningful conflicts.
            if a.resource_id != b.resource_id {
                continue;
            }
            if a.vector_clock.is_concurrent_with(&b.vector_clock) {
                out.push(Conflict {
                    version_a: VersionId(a.version_id),
                    version_b: VersionId(b.version_id),
                });
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::EnvelopeBuilder;
    use crate::keys::FacilityKeypair;
    use crate::vector_clock::VectorClock;
    use patientspace_shared::{ClinicianId, FacilityId, ResourceId, ResourceKind, VersionId};

    fn make_envelope(
        kp: &FacilityKeypair,
        fid: &FacilityId,
        rid: &ResourceId,
        vid: &VersionId,
        vc: VectorClock,
    ) -> Envelope {
        let plaintext = br#"{"resourceType":"Patient"}"#.to_vec();
        let author_pub = kp.public_keys();
        let builder = EnvelopeBuilder {
            version_id: vid.clone(),
            resource_id: rid.clone(),
            resource_type: "Patient".to_string(),
            vector_clock: vc,
            parent_version_ids: vec![],
            author_facility_id: fid.clone(),
            author_clinician_id: ClinicianId::new(),
            created_at_unix_ms: 1_700_000_000_000,
            kind: ResourceKind::Clinical,
            plaintext,
        };
        builder.seal(kp, &[(fid, &author_pub)]).expect("seal")
    }

    #[test]
    fn no_conflicts_for_linear_history() {
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-A");
        let rid = ResourceId::new();

        let v1 = make_envelope(
            &kp,
            &fid,
            &rid,
            &VersionId::new(),
            VectorClock::from_facility(&fid, 1),
        );
        let mut vc2 = VectorClock::from_facility(&fid, 1);
        vc2.observe(&fid, 2);
        let v2 = make_envelope(&kp, &fid, &rid, &VersionId::new(), vc2);

        assert!(detect_conflicts(&[v1, v2]).is_empty());
    }

    #[test]
    fn detects_concurrent_pair() {
        let kp = FacilityKeypair::generate();
        let fid_a = FacilityId::new("F-A");
        let fid_b = FacilityId::new("F-B");
        let rid = ResourceId::new();

        let v1 = make_envelope(
            &kp,
            &fid_a,
            &rid,
            &VersionId::new(),
            VectorClock::from_facility(&fid_a, 1),
        );
        let v2 = make_envelope(
            &kp,
            &fid_b,
            &rid,
            &VersionId::new(),
            VectorClock::from_facility(&fid_b, 1),
        );

        let conflicts = detect_conflicts(&[v1, v2]);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].version_a.0, v1.version_id);
        assert_eq!(conflicts[0].version_b.0, v2.version_id);
    }
}
