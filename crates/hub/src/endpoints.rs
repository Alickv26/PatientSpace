//! PSEP §7 endpoints — handshake, push, pull, pull-ack.
//!
//! All request/response bodies are CBOR-encoded. Push and pull bodies use
//! **CBOR sequence** framing (RFC 8742): concatenated CBOR items with no
//! length prefix.
//!
//! # Endpoint summary
//!
//! | Method | Path                  | Body in         | Body out          |
//! |--------|-----------------------|-----------------|-------------------|
//! | POST   | `/psep/v1/handshake`  | HandshakeRequest| HandshakeResponse |
//! | POST   | `/psep/v1/push`       | CBOR-seq envs   | PushResponse      |
//! | POST   | `/psep/v1/pull`       | PullRequest     | CBOR-seq envs     |
//! | POST   | `/psep/v1/pull-ack`   | PullAckRequest  | (empty, 200)      |

use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Cursor;

use axum::body::{Body, Bytes};
use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use patientspace_shared::VersionId;
use patientspace_sync::{
    Envelope, HandshakeRequest, HandshakeResponse, LogError, PullAckRequest, PullRequest,
    PushResponse, RejectedEnvelope, VectorClock,
};
use serde::Deserialize;
use uuid::Uuid;

use crate::state::HubState;

// ---------------------------------------------------------------------
// Query-param types
// ---------------------------------------------------------------------

/// `?session_id=...` for the push endpoint. v0.1 ignores the value (sessions
/// are not yet enforced); the field exists so we can wire resumability in
/// later without changing the URL shape.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PushQuery {
    pub session_id: Option<String>,
}

/// `?session_id=...` for the pull endpoint (see [`PushQuery`]).
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PullQuery {
    pub session_id: Option<String>,
}

/// `?session_id=...` for the pull-ack endpoint.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PullAckQuery {
    pub session_id: Option<String>,
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

/// True iff `a` has any facility counter strictly greater than `b`'s.
///
/// Used by the handshake to decide direction:
/// - `clock_has_newer(&facility_clock, &hub_clock)` → facility is ahead, hub needs to pull.
/// - `clock_has_newer(&hub_clock, &facility_clock)` → hub is ahead, facility needs to pull.
fn clock_has_newer(a: &VectorClock, b: &VectorClock) -> bool {
    for (k, &v) in &a.0 {
        let b_v = b.0.get(k).copied().unwrap_or(0);
        if v > b_v {
            return true;
        }
    }
    false
}

/// Sum of the per-facility counter differences where `a` is ahead of `b`.
/// Used to populate `push_size_hint` / `pull_size_hint` in the handshake.
fn count_newer(a: &VectorClock, b: &VectorClock) -> u64 {
    let mut total = 0u64;
    for (k, &v) in &a.0 {
        let b_v = b.0.get(k).copied().unwrap_or(0);
        if v > b_v {
            total += v - b_v;
        }
    }
    total
}

/// Topologically sort the given envelopes so that parents appear before
/// children (Kahn's algorithm).
///
/// Only parents that are *also in the set* count towards a child's in-degree.
/// Parents outside the set (e.g., genesis envelopes the facility already has)
/// are not blockers. Ties are broken by ascending `version_id` for
/// deterministic output.
fn topological_sort(envelopes: Vec<Envelope>) -> Vec<Envelope> {
    let id_set: HashSet<Uuid> = envelopes.iter().map(|e| e.version_id).collect();
    let mut by_id: HashMap<Uuid, Envelope> =
        envelopes.into_iter().map(|e| (e.version_id, e)).collect();

    // in_degree[vid] = number of parents that are also in the set.
    let mut in_degree: HashMap<Uuid, usize> = HashMap::new();
    let mut children: HashMap<Uuid, Vec<Uuid>> = HashMap::new();

    for (vid, env) in &by_id {
        let mut deg = 0usize;
        for parent in &env.parent_version_ids {
            if id_set.contains(parent) {
                deg += 1;
                children.entry(*parent).or_default().push(*vid);
            }
        }
        in_degree.insert(*vid, deg);
    }

    // BTreeSet for deterministic tie-breaking by version_id.
    let mut queue: BTreeSet<Uuid> = in_degree
        .iter()
        .filter(|(_, &d)| d == 0)
        .map(|(k, _)| *k)
        .collect();

    let mut result = Vec::with_capacity(by_id.len());
    while let Some(&vid) = queue.iter().next() {
        queue.remove(&vid);
        let env = by_id.remove(&vid).expect("present in queue");
        if let Some(child_list) = children.get(&vid) {
            for child in child_list {
                if let Some(d) = in_degree.get_mut(child) {
                    *d = d.saturating_sub(1);
                    if *d == 0 {
                        queue.insert(*child);
                    }
                }
            }
        }
        result.push(env);
    }

    // The DAG is guaranteed acyclic by PSEP's parent-linking rules, but if a
    // cycle somehow appears we still want to return everything (don't drop
    // envelopes silently).
    for (_, env) in by_id {
        result.push(env);
    }
    result
}

/// Parse a CBOR sequence (RFC 8742) — concatenated CBOR items, no length
/// prefix — into a `Vec<T>`.
fn parse_cbor_seq<T: for<'de> serde::Deserialize<'de>>(bytes: &[u8]) -> Result<Vec<T>, String> {
    let mut result = Vec::new();
    let mut cursor = Cursor::new(bytes);
    while cursor.position() < bytes.len() as u64 {
        let item: T = ciborium::de::from_reader(&mut cursor)
            .map_err(|e| format!("cbor decode: {e}"))?;
        result.push(item);
    }
    Ok(result)
}

/// CBOR-encode a single value to a `Vec<u8>`.
fn cbor_encode<T: serde::Serialize>(value: &T) -> Result<Vec<u8>, String> {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(value, &mut buf).map_err(|e| format!("cbor encode: {e}"))?;
    Ok(buf)
}

fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn cbor_response(body: Vec<u8>, content_type: &str) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .body(Body::from(body))
        .expect("response builder")
}

fn err_response(status: StatusCode, msg: impl Into<String>) -> Response<Body> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(msg.into()))
        .expect("response builder")
}

// ---------------------------------------------------------------------
// Endpoints
// ---------------------------------------------------------------------

/// `POST /psep/v1/handshake` — §7.2.
///
/// The facility sends its view of the global vector clock; the hub replies
/// with its own view + hints about which direction sync needs to flow.
pub async fn handshake(
    State(state): State<HubState>,
    body: Bytes,
) -> Response<Body> {
    let req: HandshakeRequest = match ciborium::de::from_reader(&mut Cursor::new(&body[..])) {
        Ok(v) => v,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, format!("cbor decode: {e}")),
    };

    let hub_clock = match state.log.global_vector_clock().await {
        Ok(c) => c,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    // pull_required: facility has versions the hub doesn't (facility is ahead).
    let pull_required = clock_has_newer(&req.facility_clock, &hub_clock);
    // push_required: hub has versions the facility doesn't (hub is ahead).
    let push_required = clock_has_newer(&hub_clock, &req.facility_clock);

    let pull_size_hint = count_newer(&req.facility_clock, &hub_clock);
    let push_size_hint = count_newer(&hub_clock, &req.facility_clock);

    let resp = HandshakeResponse {
        hub_clock,
        push_size_hint,
        pull_size_hint,
        pull_required,
        push_required,
        session_id: Uuid::now_v7(),
        session_expiry_unix_ms: now_unix_ms().saturating_add(3600 * 1000),
    };

    match cbor_encode(&resp) {
        Ok(buf) => cbor_response(buf, "application/cbor"),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

/// `POST /psep/v1/push` — §7.3.
///
/// Body is a CBOR-sequence of envelopes. The hub verifies each envelope's
/// signature against the registry, then ingests it (idempotently) into the
/// event log. Returns a PushResponse listing accepted + rejected version IDs.
pub async fn push(
    State(state): State<HubState>,
    Query(_query): Query<PushQuery>,
    body: Bytes,
) -> Response<Body> {
    let bytes_received = body.len() as u64;

    let envelopes: Vec<Envelope> = match parse_cbor_seq(&body) {
        Ok(v) => v,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, e),
    };

    let mut accepted = Vec::new();
    let mut rejected = Vec::new();

    for env in envelopes {
        // 1. Look up the author facility's public keys.
        let author_keys = match state.registry.get(&env.author_facility_id) {
            Some(k) => k,
            None => {
                rejected.push(RejectedEnvelope {
                    version_id: env.version_id,
                    reason: "unknown_facility".to_string(),
                });
                continue;
            }
        };

        // 2. Verify the Ed25519 signature.
        if env.verify_signature(&author_keys.verifying).is_err() {
            rejected.push(RejectedEnvelope {
                version_id: env.version_id,
                reason: "bad_signature".to_string(),
            });
            continue;
        }

        // 3. Idempotent ingest. If the version is already in the log, this is
        //    a no-op and we do NOT add it to `accepted` (per spec: second push
        //    of the same envelope yields `accepted: []`).
        let already_present = state
            .log
            .get(&VersionId(env.version_id))
            .await
            .is_ok();
        if already_present {
            continue;
        }

        match state.log.ingest(&env).await {
            Ok(()) => accepted.push(env.version_id),
            Err(LogError::Duplicate(_)) => {
                // Race: someone else inserted between our get and ingest. Treat as no-op.
            }
            Err(e) => {
                rejected.push(RejectedEnvelope {
                    version_id: env.version_id,
                    reason: format!("internal: {e}"),
                });
            }
        }
    }

    let resp = PushResponse {
        accepted,
        rejected,
        bytes_received,
    };

    match cbor_encode(&resp) {
        Ok(buf) => cbor_response(buf, "application/cbor"),
        Err(e) => err_response(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}

/// `POST /psep/v1/pull` — §7.4.
///
/// The facility sends its view of the global vector clock; the hub returns
/// (as a CBOR-seq) every envelope the facility is missing, in causal-DAG
/// order (parents before children).
pub async fn pull(
    State(state): State<HubState>,
    Query(_query): Query<PullQuery>,
    body: Bytes,
) -> Response<Body> {
    let req: PullRequest = match ciborium::de::from_reader(&mut Cursor::new(&body[..])) {
        Ok(v) => v,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, format!("cbor decode: {e}")),
    };

    // Load every envelope the hub has. The hub is honest-but-curious: it
    // cannot decrypt the payloads, but it can see author + vector clock and
    // so can decide which envelopes the facility is missing.
    let all = match state.log.list_all(1_000_000).await {
        Ok(v) => v,
        Err(e) => return err_response(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    // Filter: keep envelopes where the author's counter in the envelope's VC
    // strictly exceeds the facility's counter for that author. (If they're
    // equal, the facility has already seen this exact envelope.)
    let filtered: Vec<Envelope> = all
        .into_iter()
        .filter(|env| {
            let env_counter = env
                .vector_clock
                .0
                .get(&env.author_facility_id)
                .copied()
                .unwrap_or(0);
            let facility_counter = req
                .facility_clock
                .0
                .get(&env.author_facility_id)
                .copied()
                .unwrap_or(0);
            env_counter > facility_counter
        })
        .collect();

    let sorted = topological_sort(filtered);

    let mut buf = Vec::new();
    for env in &sorted {
        if let Err(e) = ciborium::ser::into_writer(env, &mut buf) {
            return err_response(StatusCode::INTERNAL_SERVER_ERROR, format!("cbor encode: {e}"));
        }
    }

    cbor_response(buf, "application/cbor-seq")
}

/// `POST /psep/v1/pull-ack` — §7.4.
///
/// v0.1 is a no-op: the hub acknowledges receipt of the facility's pull-ack
/// with 200 OK. A future version may use this to advance the facility's
/// session checkpoint for resumable transfers.
pub async fn pull_ack(
    State(_state): State<HubState>,
    Query(_query): Query<PullAckQuery>,
    body: Bytes,
) -> Response<Body> {
    let _req: PullAckRequest = match ciborium::de::from_reader(&mut Cursor::new(&body[..])) {
        Ok(v) => v,
        Err(e) => return err_response(StatusCode::BAD_REQUEST, format!("cbor decode: {e}")),
    };

    Response::builder()
        .status(StatusCode::OK)
        .body(Body::empty())
        .expect("response builder")
}

#[cfg(test)]
mod tests {
    use super::*;
    use patientspace_shared::FacilityId;

    #[test]
    fn clock_has_newer_detects_ahead_counter() {
        let a = VectorClock::from_facility(&FacilityId::new("F-A"), 5);
        let b = VectorClock::from_facility(&FacilityId::new("F-A"), 3);
        assert!(clock_has_newer(&a, &b));
        assert!(!clock_has_newer(&b, &a));
    }

    #[test]
    fn clock_has_newer_with_disjoint_facilities() {
        let mut a = VectorClock::from_facility(&FacilityId::new("F-A"), 5);
        a.observe(&FacilityId::new("F-B"), 0);
        let b = VectorClock::from_facility(&FacilityId::new("F-B"), 3);
        // A is ahead on F-A; B is ahead on F-B → both directions true.
        assert!(clock_has_newer(&a, &b));
        assert!(clock_has_newer(&b, &a));
    }

    #[test]
    fn count_newer_sums_differences() {
        let mut a = VectorClock::new();
        a.observe(&FacilityId::new("F-A"), 5);
        a.observe(&FacilityId::new("F-B"), 2);
        let mut b = VectorClock::new();
        b.observe(&FacilityId::new("F-A"), 3);
        b.observe(&FacilityId::new("F-B"), 2);
        // (5-3) + (2-2) = 2
        assert_eq!(count_newer(&a, &b), 2);
    }

    #[test]
    fn topological_sort_puts_parents_before_children() {
        use patientspace_shared::{ClinicianId, ResourceId, ResourceKind};
        use patientspace_sync::{EnvelopeBuilder, FacilityKeypair};

        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-A");
        let rid = ResourceId::new();
        let author_pub = kp.public_keys();

        let v0 = VersionId::new();
        let env0 = EnvelopeBuilder {
            version_id: v0.clone(),
            resource_id: rid.clone(),
            resource_type: "Patient".to_string(),
            vector_clock: VectorClock::from_facility(&fid, 1),
            parent_version_ids: vec![],
            author_facility_id: fid.clone(),
            author_clinician_id: ClinicianId::new(),
            created_at_unix_ms: 1_000,
            kind: ResourceKind::Clinical,
            plaintext: b"p0".to_vec(),
        }
        .seal(&kp, &[(&fid, &author_pub)])
        .unwrap();

        let v1 = VersionId::new();
        let env1 = EnvelopeBuilder {
            version_id: v1.clone(),
            resource_id: rid.clone(),
            resource_type: "Patient".to_string(),
            vector_clock: VectorClock::from_facility(&fid, 2),
            parent_version_ids: vec![v0.clone()],
            author_facility_id: fid.clone(),
            author_clinician_id: ClinicianId::new(),
            created_at_unix_ms: 2_000,
            kind: ResourceKind::Clinical,
            plaintext: b"p1".to_vec(),
        }
        .seal(&kp, &[(&fid, &author_pub)])
        .unwrap();

        let v2 = VersionId::new();
        let env2 = EnvelopeBuilder {
            version_id: v2.clone(),
            resource_id: rid,
            resource_type: "Patient".to_string(),
            vector_clock: VectorClock::from_facility(&fid, 3),
            parent_version_ids: vec![v1.clone()],
            author_facility_id: fid.clone(),
            author_clinician_id: ClinicianId::new(),
            created_at_unix_ms: 3_000,
            kind: ResourceKind::Clinical,
            plaintext: b"p2".to_vec(),
        }
        .seal(&kp, &[(&fid, &author_pub)])
        .unwrap();

        // Pass them in reverse order to make sure sort reorders.
        let sorted = topological_sort(vec![env2.clone(), env1.clone(), env0.clone()]);
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].version_id, v0.0);
        assert_eq!(sorted[1].version_id, v1.0);
        assert_eq!(sorted[2].version_id, v2.0);
    }
}
