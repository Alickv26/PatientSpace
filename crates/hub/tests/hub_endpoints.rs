//! Integration tests for the PSEP §7 hub endpoints.
//!
//! These tests boot the hub in-process on an ephemeral port and exercise the
//! handshake / push / pull / pull-ack endpoints over real HTTP (via `reqwest`).
//! CBOR encoding/decoding is done in-test so we exercise the wire format end
//! to end.

use std::collections::HashSet;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use patientspace_hub::endpoints;
use patientspace_hub::registry::InMemoryFacilityRegistry;
use patientspace_hub::state::HubState;
use patientspace_shared::{ClinicianId, FacilityId, ResourceId, ResourceKind, VersionId};
use patientspace_sync::{
    detect_conflicts, Envelope, EnvelopeBuilder, EventLog, FacilityKeypair, HandshakeRequest,
    HandshakeResponse, PullAckRequest, PullRequest, PushResponse, VectorClock,
};

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

struct HubHandle {
    url: String,
    _state: Arc<HubState>,
}

async fn start_hub(registry: InMemoryFacilityRegistry) -> HubHandle {
    let log = EventLog::open_in_memory().await.expect("open log");
    let state = HubState::new(log, Arc::new(registry));
    let app = Router::new()
        .route("/health", get(health))
        .route("/psep/v1/ping", get(ping))
        .route("/psep/v1/handshake", post(endpoints::handshake))
        .route("/psep/v1/push", post(endpoints::push))
        .route("/psep/v1/pull", post(endpoints::pull))
        .route("/psep/v1/pull-ack", post(endpoints::pull_ack))
        .with_state(state.clone());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    HubHandle {
        url,
        _state: Arc::new(state),
    }
}

async fn health() -> &'static str {
    "ok"
}

async fn ping() -> &'static str {
    "pong"
}

/// Build a sealed envelope signed by `author`, with the given vector clock +
/// parents. `other` is included as an additional recipient (per §5.5 access
/// policy) so cross-facility decryption works.
fn build_envelope(
    author_kp: &FacilityKeypair,
    author_id: &FacilityId,
    resource_id: &ResourceId,
    version_id: &VersionId,
    vector_clock: VectorClock,
    parents: Vec<VersionId>,
    plaintext: Vec<u8>,
    other: Option<(&FacilityId, &patientspace_sync::FacilityPublicKeys)>,
) -> Envelope {
    let author_pub = author_kp.public_keys();
    let mut recipients: Vec<(&FacilityId, &patientspace_sync::FacilityPublicKeys)> =
        vec![(author_id, &author_pub)];
    if let Some((other_id, other_keys)) = other {
        recipients.push((other_id, other_keys));
    }
    EnvelopeBuilder {
        version_id: version_id.clone(),
        resource_id: resource_id.clone(),
        resource_type: "Patient".to_string(),
        vector_clock,
        parent_version_ids: parents,
        author_facility_id: author_id.clone(),
        author_clinician_id: ClinicianId::new(),
        created_at_unix_ms: 1_729_000_000_000,
        kind: ResourceKind::Clinical,
        plaintext,
    }
    .seal(author_kp, &recipients)
    .expect("seal")
}

fn cbor_encode<T: serde::Serialize>(value: &T) -> Vec<u8> {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(value, &mut buf).expect("cbor encode");
    buf
}

fn cbor_decode<T: for<'de> serde::Deserialize<'de>>(bytes: &[u8]) -> T {
    ciborium::de::from_reader(bytes).expect("cbor decode")
}

fn cbor_seq_encode<T: serde::Serialize>(items: &[T]) -> Vec<u8> {
    let mut buf = Vec::new();
    for item in items {
        ciborium::ser::into_writer(item, &mut buf).expect("cbor encode");
    }
    buf
}

fn cbor_seq_decode<T: for<'de> serde::Deserialize<'de>>(bytes: &[u8]) -> Vec<T> {
    let mut out = Vec::new();
    let mut cursor = std::io::Cursor::new(bytes);
    while cursor.position() < bytes.len() as u64 {
        let item: T = ciborium::de::from_reader(&mut cursor).expect("cbor decode");
        out.push(item);
    }
    out
}

async fn post_handshake(
    client: &reqwest::Client,
    url: &str,
    facility_clock: VectorClock,
) -> HandshakeResponse {
    let req = HandshakeRequest {
        facility_id: "F-TEST".to_string(),
        facility_clock,
        resource_watermarks: Default::default(),
        capabilities: vec!["psep-v1".to_string()],
    };
    let body = cbor_encode(&req);
    let resp = client
        .post(format!("{url}/psep/v1/handshake"))
        .header("content-type", "application/cbor")
        .body(body)
        .send()
        .await
        .expect("handshake");
    assert_eq!(resp.status(), 200);
    let bytes = resp.bytes().await.unwrap();
    cbor_decode(&bytes)
}

async fn post_push(client: &reqwest::Client, url: &str, envelopes: &[Envelope]) -> PushResponse {
    let body = cbor_seq_encode(envelopes);
    let resp = client
        .post(format!("{url}/psep/v1/push"))
        .header("content-type", "application/cbor-seq")
        .body(body)
        .send()
        .await
        .expect("push");
    assert_eq!(resp.status(), 200);
    let bytes = resp.bytes().await.unwrap();
    cbor_decode(&bytes)
}

async fn post_pull(
    client: &reqwest::Client,
    url: &str,
    facility_clock: VectorClock,
) -> Vec<Envelope> {
    let req = PullRequest {
        facility_clock,
    };
    let body = cbor_encode(&req);
    let resp = client
        .post(format!("{url}/psep/v1/pull"))
        .header("content-type", "application/cbor")
        .body(body)
        .send()
        .await
        .expect("pull");
    assert_eq!(resp.status(), 200);
    let bytes = resp.bytes().await.unwrap();
    cbor_seq_decode(&bytes)
}

async fn post_pull_ack(client: &reqwest::Client, url: &str, received: Vec<uuid::Uuid>) {
    let req = PullAckRequest { received };
    let body = cbor_encode(&req);
    let resp = client
        .post(format!("{url}/psep/v1/pull-ack"))
        .header("content-type", "application/cbor")
        .body(body)
        .send()
        .await
        .expect("pull-ack");
    assert_eq!(resp.status(), 200);
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[tokio::test]
async fn push_and_pull_round_trip() {
    let kp = FacilityKeypair::generate();
    let fid = FacilityId::new("F-A");
    let pub_keys = kp.public_keys();

    let mut registry = InMemoryFacilityRegistry::new();
    registry.register(fid.as_str().to_string(), pub_keys);
    let hub = start_hub(registry).await;
    let client = reqwest::Client::new();

    // 3 genesis + 2 linear edits on the same resource.
    let rid = ResourceId::new();
    let mut envelopes = Vec::new();
    let mut last_parent: Option<VersionId> = None;
    for i in 1..=3 {
        let vid = VersionId::new();
        let env = build_envelope(
            &kp,
            &fid,
            &rid,
            &vid,
            VectorClock::from_facility(&fid, i),
            vec![],
            format!(r#"{{"resourceType":"Patient","i":{i}}}"#).into_bytes(),
            None,
        );
        last_parent = Some(vid.clone());
        envelopes.push(env);
    }
    // Two linear edits, each a child of the previous genesis version.
    for i in 4..=5 {
        let vid = VersionId::new();
        let parent = last_parent.clone().unwrap();
        let env = build_envelope(
            &kp,
            &fid,
            &rid,
            &vid,
            VectorClock::from_facility(&fid, i),
            vec![parent.clone()],
            format!(r#"{{"resourceType":"Patient","i":{i}}}"#).into_bytes(),
            None,
        );
        last_parent = Some(vid.clone());
        envelopes.push(env);
    }
    assert_eq!(envelopes.len(), 5);

    // Push all 5 envelopes.
    let resp = post_push(&client, &hub.url, &envelopes).await;
    assert_eq!(resp.accepted.len(), 5);
    assert_eq!(resp.rejected.len(), 0);

    // Pull with an empty clock → hub has all 5.
    let pulled = post_pull(&client, &hub.url, VectorClock::new()).await;
    assert_eq!(pulled.len(), 5);

    // Verify causal-DAG invariant: every parent appears before its children.
    let mut seen: HashSet<uuid::Uuid> = HashSet::new();
    for env in &pulled {
        for parent in &env.parent_version_ids {
            assert!(
                seen.contains(parent),
                "parent {} must appear before child {}",
                parent,
                env.version_id
            );
        }
        seen.insert(env.version_id);
    }

    // Pull again with the full clock (F-A: 5) → 0 envelopes.
    let full_clock = VectorClock::from_facility(&fid, 5);
    let pulled_again = post_pull(&client, &hub.url, full_clock).await;
    assert_eq!(pulled_again.len(), 0);
}

#[tokio::test]
async fn push_rejects_unknown_facility() {
    // Empty registry — every facility is unknown.
    let hub = start_hub(InMemoryFacilityRegistry::new()).await;
    let client = reqwest::Client::new();

    let kp = FacilityKeypair::generate();
    let fid = FacilityId::new("F-A");
    let env = build_envelope(
        &kp,
        &fid,
        &ResourceId::new(),
        &VersionId::new(),
        VectorClock::from_facility(&fid, 1),
        vec![],
        b"{}".to_vec(),
        None,
    );

    let resp = post_push(&client, &hub.url, &[env]).await;
    assert_eq!(resp.accepted.len(), 0);
    assert_eq!(resp.rejected.len(), 1);
    assert_eq!(resp.rejected[0].reason, "unknown_facility");
}

#[tokio::test]
async fn push_rejects_bad_signature() {
    // Register key A, but sign with key B.
    let kp_a = FacilityKeypair::generate();
    let kp_b = FacilityKeypair::generate();
    let fid_a = FacilityId::new("F-A");

    let mut registry = InMemoryFacilityRegistry::new();
    // Author is F-A; registry knows F-A's keys.
    registry.register(fid_a.as_str().to_string(), kp_a.public_keys());
    let hub = start_hub(registry).await;
    let client = reqwest::Client::new();

    // Envelope authored by F-A but signed by F-B's keypair.
    let env = build_envelope(
        &kp_b,
        &fid_a,
        &ResourceId::new(),
        &VersionId::new(),
        VectorClock::from_facility(&fid_a, 1),
        vec![],
        b"{}".to_vec(),
        None,
    );

    let resp = post_push(&client, &hub.url, &[env]).await;
    assert_eq!(resp.accepted.len(), 0);
    assert_eq!(resp.rejected.len(), 1);
    assert_eq!(resp.rejected[0].reason, "bad_signature");
}

#[tokio::test]
async fn push_is_idempotent() {
    let kp = FacilityKeypair::generate();
    let fid = FacilityId::new("F-A");

    let mut registry = InMemoryFacilityRegistry::new();
    registry.register(fid.as_str().to_string(), kp.public_keys());
    let hub = start_hub(registry).await;
    let client = reqwest::Client::new();

    let env = build_envelope(
        &kp,
        &fid,
        &ResourceId::new(),
        &VersionId::new(),
        VectorClock::from_facility(&fid, 1),
        vec![],
        b"{}".to_vec(),
        None,
    );

    // Push the same envelope 3 times.
    for _ in 0..3 {
        let resp = post_push(&client, &hub.url, &[env.clone()]).await;
        assert_eq!(resp.rejected.len(), 0);
    }

    // Log must have exactly 1 envelope.
    let count = hub._state.log.count().await.unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn handshake_returns_correct_hints() {
    let kp = FacilityKeypair::generate();
    let fid = FacilityId::new("F-A");

    let mut registry = InMemoryFacilityRegistry::new();
    registry.register(fid.as_str().to_string(), kp.public_keys());
    let hub = start_hub(registry).await;
    let client = reqwest::Client::new();

    // Push 3 envelopes with VC {F-A: 1}, {F-A: 2}, {F-A: 3}.
    let mut envelopes = Vec::new();
    for i in 1..=3 {
        envelopes.push(build_envelope(
            &kp,
            &fid,
            &ResourceId::new(),
            &VersionId::new(),
            VectorClock::from_facility(&fid, i),
            vec![],
            format!(r#"{{"i":{i}}}"#).into_bytes(),
            None,
        ));
    }
    let resp = post_push(&client, &hub.url, &envelopes).await;
    assert_eq!(resp.accepted.len(), 3);

    // Handshake with an empty facility clock.
    // Expected: facility has nothing newer than hub → pull_required = false.
    // Hub has F-A:3 > 0 → push_required = true.
    let hs = post_handshake(&client, &hub.url, VectorClock::new()).await;
    assert!(!hs.pull_required, "facility has nothing the hub lacks");
    assert!(hs.push_required, "hub has 3 versions the facility lacks");
    assert!(hs.push_size_hint >= 3);
    assert_eq!(hs.pull_size_hint, 0);
    // session_id is a Uuid (v7 / SortRand).
    assert!(hs.session_id.get_version().is_some());
    assert!(hs.hub_clock.get(&fid) >= 3);

    // Now handshake with the facility having seen everything.
    let full = VectorClock::from_facility(&fid, 3);
    let hs2 = post_handshake(&client, &hub.url, full).await;
    assert!(!hs2.pull_required);
    assert!(!hs2.push_required);
}

#[tokio::test]
async fn pull_ack_is_noop() {
    let kp = FacilityKeypair::generate();
    let fid = FacilityId::new("F-A");
    let mut registry = InMemoryFacilityRegistry::new();
    registry.register(fid.as_str().to_string(), kp.public_keys());
    let hub = start_hub(registry).await;
    let client = reqwest::Client::new();

    // Just verify pull-ack returns 200 with a non-empty received list.
    post_pull_ack(&client, &hub.url, vec![uuid::Uuid::now_v7()]).await;

    // Sanity: detect_conflicts on an empty set returns nothing.
    let conflicts = detect_conflicts(&[]);
    assert!(conflicts.is_empty());
}
