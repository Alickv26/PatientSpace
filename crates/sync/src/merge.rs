//! Conflict detection and merge policy for PSEP.
//!
//! See `docs/sync-protocol.md` §6 for the design rationale.
//!
//! # Conflict detection
//!
//! Two versions of the same resource are **concurrent** (in conflict) if
//! neither version's vector clock happens-before the other. This is the
//! standard vector-clock concurrency test (§3.2).
//!
//! # Merge policy
//!
//! For each FHIR field, one of three merge classes applies:
//!
//! | Class             | Rule                                            | Fields (Patient)              |
//! |-------------------|-------------------------------------------------|-------------------------------|
//! | `lww_scalar`      | Last-writer-wins by `created_at_unix_ms`        | name, telecom, birthDate, ... |
//! | `union_set`       | Union of all values, dedup by natural key       | identifier, address           |
//! | `clinical_review` | Surface to clinician; never auto-merge          | all other fields              |
//!
//! `compute_merged_view` produces a deterministic merged resource given the
//! same set of input versions (invariant §14.1.1).

use std::collections::BTreeSet;

use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::envelope::Envelope;

/// A pair of concurrent versions of the same resource (a conflict).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConflictPair {
    pub version_a: Uuid,
    pub version_b: Uuid,
    pub resource_id: Uuid,
}

/// A per-field conflict between two versions.
#[derive(Debug, Clone, Serialize)]
pub struct FieldConflict {
    pub field_path: String,
    pub version_a: Uuid,
    pub value_a: Value,
    pub version_b: Uuid,
    pub value_b: Value,
}

/// The merged view of a resource across multiple versions.
#[derive(Debug, Clone)]
pub struct MergedView {
    pub resource_id: Uuid,
    pub merged_resource: Value,
    pub has_conflicts: bool,
    pub field_conflicts: Vec<FieldConflict>,
    pub contributing_versions: Vec<Uuid>,
}

/// Fields merged last-writer-wins (by `created_at_unix_ms`, tie-break by `version_id`).
const LWW_SCALAR_FIELDS: &[&str] = &["name", "telecom", "birthDate", "gender", "active"];

/// Fields merged as a union set (dedup by canonical JSON).
const UNION_SET_FIELDS: &[&str] = &["identifier", "address"];

/// FHIR extension URL for PatientSpace conflict markers.
const CONFLICT_EXTENSION_URL: &str =
    "https://patientspace.org/fhir/StructureDefinition/conflict";

/// Detect all pairs of concurrent versions across the given envelopes.
///
/// Two versions are concurrent if they share the same `resource_id` and
/// neither vector clock happens-before the other. Returns one `ConflictPair`
/// per concurrent pair (unordered).
pub fn detect_conflicts(versions: &[Envelope]) -> Vec<ConflictPair> {
    let mut conflicts = Vec::new();
    for i in 0..versions.len() {
        for j in (i + 1)..versions.len() {
            let a = &versions[i];
            let b = &versions[j];
            if a.resource_id == b.resource_id
                && a.vector_clock.is_concurrent_with(&b.vector_clock)
            {
                conflicts.push(ConflictPair {
                    version_a: a.version_id,
                    version_b: b.version_id,
                    resource_id: a.resource_id,
                });
            }
        }
    }
    conflicts
}

/// Compute a deterministic merged view of a resource from its versions.
///
/// `decrypt_fn` is called on each envelope to recover the plaintext FHIR
/// resource. Envelopes that fail to decrypt are silently skipped.
///
/// The merge applies the per-field policy:
/// - `lww_scalar` fields: value from the version with the highest
///   `created_at_unix_ms` (tie-break: highest `version_id`).
/// - `union_set` fields: union of all arrays across versions, dedup by
///   canonical JSON string.
/// - All other fields: if all versions agree, use that value; otherwise
///   flag as a `clinical_review` conflict (set to null, add a conflict
///   extension).
pub fn compute_merged_view(
    resource_id: Uuid,
    versions: &[Envelope],
    decrypt_fn: Box<dyn Fn(&Envelope) -> Result<Value, String>>,
) -> MergedView {
    if versions.is_empty() {
        return MergedView {
            resource_id,
            merged_resource: Value::Null,
            has_conflicts: false,
            field_conflicts: vec![],
            contributing_versions: vec![],
        };
    }

    // Decrypt all versions, skipping failures.
    let mut decrypted: Vec<(Envelope, Value)> = Vec::with_capacity(versions.len());
    for env in versions {
        match decrypt_fn(env) {
            Ok(resource) => decrypted.push((env.clone(), resource)),
            Err(_) => continue,
        }
    }

    let contributing_versions: Vec<Uuid> =
        decrypted.iter().map(|(e, _)| e.version_id).collect();

    if decrypted.is_empty() {
        return MergedView {
            resource_id,
            merged_resource: Value::Null,
            has_conflicts: false,
            field_conflicts: vec![],
            contributing_versions: versions.iter().map(|e| e.version_id).collect(),
        };
    }

    // Single version: no conflicts, return it directly.
    if decrypted.len() == 1 {
        return MergedView {
            resource_id,
            merged_resource: decrypted[0].1.clone(),
            has_conflicts: false,
            field_conflicts: vec![],
            contributing_versions,
        };
    }

    // Multi-version merge.
    let mut merged = serde_json::Map::new();
    let mut field_conflicts: Vec<FieldConflict> = Vec::new();

    // Copy resourceType and id from the first version.
    if let Some(obj) = decrypted[0].1.as_object() {
        if let Some(rt) = obj.get("resourceType") {
            merged.insert("resourceType".to_string(), rt.clone());
        }
        if let Some(id) = obj.get("id") {
            merged.insert("id".to_string(), id.clone());
        }
    }

    // --- lww_scalar fields ---
    for field in LWW_SCALAR_FIELDS {
        let winner = decrypted
            .iter()
            .filter(|(_, r)| r.get(*field).is_some())
            .max_by(|(a, _), (b, _)| {
                a.created_at_unix_ms
                    .cmp(&b.created_at_unix_ms)
                    .then(a.version_id.cmp(&b.version_id))
            });
        if let Some((_, resource)) = winner {
            if let Some(val) = resource.get(*field) {
                merged.insert(field.to_string(), val.clone());
            }
        }
    }

    // --- union_set fields ---
    for field in UNION_SET_FIELDS {
        let mut seen: Vec<Value> = Vec::new();
        let mut seen_keys: BTreeSet<String> = BTreeSet::new();
        for (_, resource) in &decrypted {
            if let Some(arr) = resource.get(*field).and_then(|v| v.as_array()) {
                for item in arr {
                    let key = serde_json::to_string(item).unwrap_or_default();
                    if seen_keys.insert(key) {
                        seen.push(item.clone());
                    }
                }
            }
        }
        if !seen.is_empty() {
            merged.insert(field.to_string(), Value::Array(seen));
        }
    }

    // --- clinical_review fields (all other fields) ---
    let mut other_fields: BTreeSet<String> = BTreeSet::new();
    for (_, resource) in &decrypted {
        if let Some(obj) = resource.as_object() {
            for key in obj.keys() {
                if key != "resourceType"
                    && key != "id"
                    && !LWW_SCALAR_FIELDS.contains(&key.as_str())
                    && !UNION_SET_FIELDS.contains(&key.as_str())
                {
                    other_fields.insert(key.clone());
                }
            }
        }
    }

    for field in &other_fields {
        let values: Vec<(Uuid, Value)> = decrypted
            .iter()
            .filter_map(|(e, r)| r.get(field).map(|v| (e.version_id, v.clone())))
            .collect();

        if values.is_empty() {
            continue;
        }

        let all_same = values.windows(2).all(|w| w[0].1 == w[1].1);
        if all_same {
            merged.insert(field.clone(), values[0].1.clone());
        } else {
            // Conflict: set to null and record all differing pairs.
            merged.insert(field.clone(), Value::Null);
            for i in 0..values.len() {
                for j in (i + 1)..values.len() {
                    if values[i].1 != values[j].1 {
                        field_conflicts.push(FieldConflict {
                            field_path: field.clone(),
                            version_a: values[i].0,
                            value_a: values[i].1.clone(),
                            version_b: values[j].0,
                            value_b: values[j].1.clone(),
                        });
                    }
                }
            }
        }
    }

    // Add FHIR conflict extension if any conflicts were detected.
    if !field_conflicts.is_empty() {
        let conflicts_ext: Vec<Value> = field_conflicts
            .iter()
            .map(|c| {
                serde_json::json!({
                    "url": CONFLICT_EXTENSION_URL,
                    "extension": [
                        {"url": "field", "valueString": c.field_path},
                        {"url": "versionA", "valueString": c.version_a.to_string()},
                        {"url": "versionB", "valueString": c.version_b.to_string()},
                    ]
                })
            })
            .collect();
        merged.insert(
            "extension".to_string(),
            Value::Array(conflicts_ext),
        );
    }

    MergedView {
        resource_id,
        merged_resource: Value::Object(merged),
        has_conflicts: !field_conflicts.is_empty(),
        field_conflicts,
        contributing_versions,
    }
}

// -----------------------------------------------------------------
// Tests
// -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::EnvelopeBuilder;
    use crate::keys::FacilityKeypair;
    use crate::vector_clock::VectorClock;
    use patientspace_shared::{
        ClinicianId, FacilityId, ResourceId, ResourceKind, VersionId,
    };

    fn make_envelope(
        kp: &FacilityKeypair,
        fid: &FacilityId,
        rid: &ResourceId,
        vc: VectorClock,
        created_at: u64,
    ) -> Envelope {
        let plaintext = br#"{"resourceType":"Patient","id":"U-test"}"#.to_vec();
        let author_pub = kp.public_keys();
        let builder = EnvelopeBuilder {
            version_id: VersionId::new(),
            resource_id: rid.clone(),
            resource_type: "Patient".to_string(),
            vector_clock: vc,
            parent_version_ids: vec![],
            author_facility_id: fid.clone(),
            author_clinician_id: ClinicianId::new(),
            created_at_unix_ms: created_at,
            kind: ResourceKind::Clinical,
            plaintext,
        };
        let recipients: Vec<(&FacilityId, &crate::keys::FacilityPublicKeys)> =
            vec![(fid, &author_pub)];
        builder.seal(kp, &recipients).unwrap()
    }

    #[test]
    fn detect_conflicts_finds_concurrent_versions() {
        let kp = FacilityKeypair::generate();
        let fid_a = FacilityId::new("F-A");
        let fid_b = FacilityId::new("F-B");
        let rid = ResourceId::new();

        let env_a = make_envelope(
            &kp,
            &fid_a,
            &rid,
            VectorClock::from_facility(&fid_a, 1),
            1_700_000_000_000,
        );
        let env_b = make_envelope(
            &kp,
            &fid_b,
            &rid,
            VectorClock::from_facility(&fid_b, 1),
            1_700_000_000_001,
        );

        let conflicts = detect_conflicts(&[env_a, env_b]);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].resource_id, rid.0);
    }

    #[test]
    fn detect_conflicts_returns_empty_for_linear_history() {
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-A");
        let rid = ResourceId::new();

        let mut vc1 = VectorClock::from_facility(&fid, 1);
        let env1 = make_envelope(&kp, &fid, &rid, vc1.clone(), 1_700_000_000_000);

        vc1.increment(&fid);
        let env2 = make_envelope(&kp, &fid, &rid, vc1, 1_700_000_000_001);

        let conflicts = detect_conflicts(&[env1, env2]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detect_conflicts_with_three_way_concurrent() {
        let kp = FacilityKeypair::generate();
        let fid_a = FacilityId::new("F-A");
        let fid_b = FacilityId::new("F-B");
        let fid_c = FacilityId::new("F-C");
        let rid = ResourceId::new();

        let env_a = make_envelope(
            &kp,
            &fid_a,
            &rid,
            VectorClock::from_facility(&fid_a, 1),
            1_700_000_000_000,
        );
        let env_b = make_envelope(
            &kp,
            &fid_b,
            &rid,
            VectorClock::from_facility(&fid_b, 1),
            1_700_000_000_001,
        );
        let env_c = make_envelope(
            &kp,
            &fid_c,
            &rid,
            VectorClock::from_facility(&fid_c, 1),
            1_700_000_000_002,
        );

        // C(3,2) = 3 concurrent pairs.
        let conflicts = detect_conflicts(&[env_a, env_b, env_c]);
        assert_eq!(conflicts.len(), 3);
    }

    #[test]
    fn detect_conflicts_mixed_concurrent_and_linear() {
        let kp = FacilityKeypair::generate();
        let fid_a = FacilityId::new("F-A");
        let fid_b = FacilityId::new("F-B");
        let rid = ResourceId::new();

        // env_a: VC = {A:1} — genesis at A.
        let env_a = make_envelope(
            &kp,
            &fid_a,
            &rid,
            VectorClock::from_facility(&fid_a, 1),
            1_700_000_000_000,
        );

        // env_b: VC = {B:1} — concurrent with env_a (B hasn't seen A).
        let env_b = make_envelope(
            &kp,
            &fid_b,
            &rid,
            VectorClock::from_facility(&fid_b, 1),
            1_700_000_000_001,
        );

        // env_c: VC = {A:1, B:1} — linear descendant of both env_a and env_b.
        let mut vc_c = VectorClock::from_facility(&fid_a, 1);
        vc_c.observe(&fid_b, 1);
        let env_c = make_envelope(&kp, &fid_a, &rid, vc_c, 1_700_000_000_002);

        // Only env_a || env_b is concurrent. env_c happens-after both.
        let conflicts = detect_conflicts(&[env_a, env_b, env_c]);
        assert_eq!(conflicts.len(), 1);
    }

    #[test]
    fn compute_merged_view_empty_returns_null() {
        let rid = Uuid::now_v7();
        let decrypt_fn: Box<dyn Fn(&Envelope) -> Result<Value, String>> =
            Box::new(|_| Ok(Value::Null));
        let view = compute_merged_view(rid, &[], decrypt_fn);
        assert_eq!(view.merged_resource, Value::Null);
        assert!(!view.has_conflicts);
        assert!(view.field_conflicts.is_empty());
        assert!(view.contributing_versions.is_empty());
    }

    #[test]
    fn compute_merged_view_single_version_no_conflict() {
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-A");
        let rid = ResourceId::new();

        let env = make_envelope(
            &kp,
            &fid,
            &rid,
            VectorClock::from_facility(&fid, 1),
            1_700_000_000_000,
        );

        let payload = serde_json::json!({
            "resourceType": "Patient",
            "id": rid.to_string(),
            "name": {"text": "John Banda"},
            "active": true,
        });

        let env_version_id = env.version_id;
        let decrypt_fn: Box<dyn Fn(&Envelope) -> Result<Value, String>> = Box::new({
            let payload = payload.clone();
            move |_| Ok(payload.clone())
        });

        let view = compute_merged_view(rid.0, &[env], decrypt_fn);
        assert!(!view.has_conflicts);
        assert!(view.field_conflicts.is_empty());
        assert_eq!(view.contributing_versions, vec![env_version_id]);
        assert_eq!(view.merged_resource, payload);
    }
}
