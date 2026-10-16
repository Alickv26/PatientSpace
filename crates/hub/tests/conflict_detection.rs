//! Conflict-detection integration tests.
//!
//! These tests run two or three facilities through the PSEP hub, create
//! concurrent or linear edits of the same resource, sync to convergence, and
//! verify that [`patientspace_sync::detect_conflicts`] surfaces the right
//! number of pairwise conflicts at every facility.
//!
//! Conflict pairs are compared as `HashSet<(Uuid, Uuid)>` (with the smaller
//! version_id first) because the iteration order may differ between
//! facilities' logs.

use std::collections::HashSet;
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use patientspace_hub::endpoints;
use patientspace_hub::registry::InMemoryFacilityRegistry;
use patientspace_hub::state::HubState;
use patientspace_shared::{ClinicianId, FacilityId, ResourceId, ResourceKind, VersionId};
use patientspace_sync::{
    detect_conflicts, Envelope, EnvelopeBuilder, EventLog, FacilityKeypair, FacilityPublicKeys,
    HandshakeRequest, PullAckRequest, PullRequest, VectorClock,
};

// ---------------------------------------------------------------------
// Test hub bootstrap
// ---------------------------------------------------------------------

async fn start_hub(registry: InMemoryFacilityRegistry) -> String {
    let log = EventLog::open_in_memory().await.expect("open log");
    let state = HubState::new(log, Arc::new(registry));
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/psep/v1/ping", get(|| async { "pong" }))
        .route("/psep/v1/handshake", post(endpoints::handshake))
        .route("/psep/v1/push", post(endpoints::push))
        .route("/psep/v1/pull", post(endpoints::pull))
        .route("/psep/v1/pull-ack", post(endpoints::pull_ack))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let url = format!("http://{addr}");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    url
}

// ---------------------------------------------------------------------
// FacilityInstance helper (mirrors two_instance.rs)
// ---------------------------------------------------------------------

struct FacilityInstance {
    id: FacilityId,
    keypair: Arc<FacilityKeypair>,
    log: EventLog,
    hub_url: String,
    counter: u64,
}

impl FacilityInstance {
    async fn with_keypair(
        id: FacilityId,
        keypair: FacilityKeypair,
        log: EventLog,
        hub_url: String,
    ) -> Self {
        Self {
            id,
            keypair: Arc::new(keypair),
            log,
            hub_url,
            counter: 0,
        }
    }

    async fn next_clock(&mut self) -> VectorClock {
        let mut vc = self.log.global_vector_clock().await.expect("global vc");
        let my_observed = vc.get(&self.id);
        self.counter = self.counter.max(my_observed) + 1;
        vc.observe(&self.id, self.counter);
        vc
    }

    /// Author and ingest a version of an existing resource (genesis or child).
    /// `other_recipients` is the list of additional recipient facilities
    /// (per §5.5 access policy).
    async fn create_version(
        &mut self,
        resource_type: &str,
        resource_id: ResourceId,
        plaintext: Vec<u8>,
        parents: Vec<VersionId>,
        other_recipients: &[(&FacilityId, &FacilityPublicKeys)],
    ) -> VersionId {
        let vid = VersionId::new();
        let vc = self.next_clock().await;
        let author_pub = self.keypair.public_keys();
        let mut recipients: Vec<(&FacilityId, &FacilityPublicKeys)> =
            vec![(&self.id, &author_pub)];
        for (fid, keys) in other_recipients {
            recipients.push((*fid, *keys));
        }
        let env = EnvelopeBuilder {
            version_id: vid.clone(),
            resource_id,
            resource_type: resource_type.to_string(),
            vector_clock: vc,
            parent_version_ids: parents,
            author_facility_id: self.id.clone(),
            author_clinician_id: ClinicianId::new(),
            created_at_unix_ms: now_unix_ms(),
            kind: ResourceKind::Clinical,
            plaintext,
        }
        .seal(&self.keypair, &recipients)
        .expect("seal");
        self.log.ingest(&env).await.expect("ingest");
        vid
    }

    /// Run a full sync cycle (handshake → push → pull → ack).
    async fn sync(&mut self) {
        let client = reqwest::Client::new();

        let local_clock = self.log.global_vector_clock().await.expect("global vc");
        let hs_req = HandshakeRequest {
            facility_id: self.id.as_str().to_string(),
            facility_clock: local_clock.clone(),
            resource_watermarks: serde_json::json!({}),
            capabilities: vec!["psep-v1".to_string()],
        };
        let mut hs_body = Vec::new();
        ciborium::ser::into_writer(&hs_req, &mut hs_body).unwrap();
        let _ = client
            .post(format!("{}/psep/v1/handshake", self.hub_url))
            .header("content-type", "application/cbor")
            .body(hs_body)
            .send()
            .await
            .expect("handshake");

        // Push every local envelope.
        let locals = self.log.list_all(1_000_000).await.expect("list_all");
        if !locals.is_empty() {
            let mut push_body = Vec::new();
            for env in &locals {
                ciborium::ser::into_writer(env, &mut push_body).unwrap();
            }
            let _ = client
                .post(format!("{}/psep/v1/push", self.hub_url))
                .header("content-type", "application/cbor-seq")
                .body(push_body)
                .send()
                .await
                .expect("push");
        }

        // Pull anything we're missing.
        let pull_req = PullRequest {
            facility_clock: local_clock,
        };
        let mut pull_body = Vec::new();
        ciborium::ser::into_writer(&pull_req, &mut pull_body).unwrap();
        let pull_resp = client
            .post(format!("{}/psep/v1/pull", self.hub_url))
            .header("content-type", "application/cbor")
            .body(pull_body)
            .send()
            .await
            .expect("pull")
            .bytes()
            .await
            .unwrap();

        let mut received: Vec<uuid::Uuid> = Vec::new();
        let mut cursor = std::io::Cursor::new(&pull_resp[..]);
        while cursor.position() < pull_resp.len() as u64 {
            let env: Envelope =
                ciborium::de::from_reader(&mut cursor).expect("decode pulled envelope");
            received.push(env.version_id);
            self.log.ingest(&env).await.expect("ingest pulled");
        }

        // Pull-ack.
        let ack_req = PullAckRequest { received };
        let mut ack_body = Vec::new();
        ciborium::ser::into_writer(&ack_req, &mut ack_body).unwrap();
        let _ = client
            .post(format!("{}/psep/v1/pull-ack", self.hub_url))
            .header("content-type", "application/cbor")
            .body(ack_body)
            .send()
            .await
            .expect("pull-ack");
    }

    /// Every envelope this facility currently holds — the input to
    /// [`detect_conflicts`].
    async fn all_envelopes(&self) -> Vec<Envelope> {
        self.log.list_all(1_000_000).await.expect("list_all")
    }
}

fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Normalize a `ConflictPair` to a `(min, max)` tuple of version IDs so the
/// same conflict is represented identically regardless of which order
/// `detect_conflicts` happened to enumerate it in.
fn pair_key(
    p: &patientspace_sync::ConflictPair,
) -> (uuid::Uuid, uuid::Uuid) {
    if p.version_a <= p.version_b {
        (p.version_a, p.version_b)
    } else {
        (p.version_b, p.version_a)
    }
}

fn pair_set(
    conflicts: &[patientspace_sync::ConflictPair],
) -> HashSet<(uuid::Uuid, uuid::Uuid)> {
    conflicts.iter().map(pair_key).collect()
}

/// Spin up a hub with N facilities pre-registered.
async fn setup_hub_with_facilities(names: &[&str]) -> (String, Vec<FacilityInstance>) {
    let mut keypairs = Vec::new();
    let mut pubs = Vec::new();
    let mut fids = Vec::new();
    for name in names {
        let kp = FacilityKeypair::generate();
        let pub_keys = kp.public_keys();
        let fid = FacilityId::new(*name);
        keypairs.push(kp);
        pubs.push(pub_keys);
        fids.push(fid);
    }

    let mut registry = InMemoryFacilityRegistry::new();
    for (fid, pub_keys) in fids.iter().zip(pubs.iter()) {
        registry.register(fid.as_str().to_string(), pub_keys.clone());
    }
    let hub_url = start_hub(registry).await;

    let mut instances = Vec::new();
    for (fid, kp) in fids.into_iter().zip(keypairs.into_iter()) {
        let log = EventLog::open_in_memory().await.unwrap();
        let inst = FacilityInstance::with_keypair(fid, kp, log, hub_url.clone()).await;
        instances.push(inst);
    }
    (hub_url, instances)
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[tokio::test]
async fn concurrent_edits_produce_conflict() {
    // Two facilities A + B both create the same resource concurrently.
    let (_hub_url, mut facilities) = setup_hub_with_facilities(&["F-A", "F-B"]).await;

    // Take A's data out, create the concurrent version, then put it back.
    let a_id = facilities[0].id.clone();
    let a_pub = facilities[0].keypair.public_keys();
    let b_id = facilities[1].id.clone();
    let b_pub = facilities[1].keypair.public_keys();

    let shared_rid = ResourceId::new();
    let plaintext = br#"{"resourceType":"Patient"}"#.to_vec();

    // A creates v1 (no knowledge of B).
    let _vid_a = facilities[0]
        .create_version(
            "Patient",
            shared_rid.clone(),
            plaintext.clone(),
            vec![],
            &[(&b_id, &b_pub)],
        )
        .await;
    // B creates v2 (no knowledge of A) — same resource_id, concurrent.
    let _vid_b = facilities[1]
        .create_version(
            "Patient",
            shared_rid.clone(),
            plaintext.clone(),
            vec![],
            &[(&a_id, &a_pub)],
        )
        .await;

    // Sync to convergence: A.push, B.pull+push, A.pull.
    facilities[0].sync().await;
    facilities[1].sync().await;
    facilities[0].sync().await;

    // Both facilities now have both envelopes. Each should detect exactly
    // one conflict (the A↔B pair).
    let envs_a = facilities[0].all_envelopes().await;
    let envs_b = facilities[1].all_envelopes().await;
    assert_eq!(envs_a.len(), 2, "A should have both versions");
    assert_eq!(envs_b.len(), 2, "B should have both versions");

    let conflicts_a = detect_conflicts(&envs_a);
    let conflicts_b = detect_conflicts(&envs_b);
    assert_eq!(conflicts_a.len(), 1, "A should detect 1 conflict");
    assert_eq!(conflicts_b.len(), 1, "B should detect 1 conflict");

    // Same conflict pair at both facilities (as a set).
    let set_a = pair_set(&conflicts_a);
    let set_b = pair_set(&conflicts_b);
    assert_eq!(set_a, set_b, "both facilities detect the same conflict pair");
}

#[tokio::test]
async fn linear_edits_produce_no_conflict() {
    // A creates v1; B syncs to get v1; B creates v2 as a linear child of v1.
    let (_hub_url, mut facilities) = setup_hub_with_facilities(&["F-A", "F-B"]).await;

    let a_id = facilities[0].id.clone();
    let a_pub = facilities[0].keypair.public_keys();
    let b_id = facilities[1].id.clone();
    let b_pub = facilities[1].keypair.public_keys();

    let shared_rid = ResourceId::new();
    let plaintext = br#"{"resourceType":"Patient"}"#.to_vec();

    // A creates v1.
    let vid_a = facilities[0]
        .create_version(
            "Patient",
            shared_rid.clone(),
            plaintext.clone(),
            vec![],
            &[(&b_id, &b_pub)],
        )
        .await;

    // Sync: A → hub → B.
    facilities[0].sync().await;
    facilities[1].sync().await;

    // B creates v2 as a child of v1 (linear, not concurrent).
    let _vid_b = facilities[1]
        .create_version(
            "Patient",
            shared_rid.clone(),
            plaintext.clone(),
            vec![vid_a.clone()],
            &[(&a_id, &a_pub)],
        )
        .await;

    // Sync: B → hub → A.
    facilities[1].sync().await;
    facilities[0].sync().await;

    // Both facilities have both versions, but they are NOT concurrent
    // (v2 descends from v1) → 0 conflicts.
    let envs_a = facilities[0].all_envelopes().await;
    let envs_b = facilities[1].all_envelopes().await;
    assert_eq!(envs_a.len(), 2);
    assert_eq!(envs_b.len(), 2);

    let conflicts_a = detect_conflicts(&envs_a);
    let conflicts_b = detect_conflicts(&envs_b);
    assert!(conflicts_a.is_empty(), "A should detect 0 conflicts, got {conflicts_a:?}");
    assert!(conflicts_b.is_empty(), "B should detect 0 conflicts, got {conflicts_b:?}");
}

#[tokio::test]
async fn three_way_concurrent_conflict() {
    // Three facilities A, B, C each create the same resource concurrently.
    // → 3 pairwise conflicts (A↔B, A↔C, B↔C).
    let (_hub_url, mut facilities) =
        setup_hub_with_facilities(&["F-A", "F-B", "F-C"]).await;

    let a_id = facilities[0].id.clone();
    let a_pub = facilities[0].keypair.public_keys();
    let b_id = facilities[1].id.clone();
    let b_pub = facilities[1].keypair.public_keys();
    let c_id = facilities[2].id.clone();
    let c_pub = facilities[2].keypair.public_keys();

    let shared_rid = ResourceId::new();
    let plaintext = br#"{"resourceType":"Patient"}"#.to_vec();

    // Each facility creates its own concurrent version of the same resource.
    // Each envelope is sealed to all three facilities as recipients.
    let _vid_a = facilities[0]
        .create_version(
            "Patient",
            shared_rid.clone(),
            plaintext.clone(),
            vec![],
            &[(&b_id, &b_pub), (&c_id, &c_pub)],
        )
        .await;
    let _vid_b = facilities[1]
        .create_version(
            "Patient",
            shared_rid.clone(),
            plaintext.clone(),
            vec![],
            &[(&a_id, &a_pub), (&c_id, &c_pub)],
        )
        .await;
    let _vid_c = facilities[2]
        .create_version(
            "Patient",
            shared_rid.clone(),
            plaintext.clone(),
            vec![],
            &[(&a_id, &a_pub), (&b_id, &b_pub)],
        )
        .await;

    // Sync to convergence: each facility needs to learn about the other two.
    // Three sync rounds is sufficient for any topology to converge.
    for _ in 0..3 {
        for f in facilities.iter_mut() {
            f.sync().await;
        }
    }

    // Each facility should have all 3 versions and detect 3 pairwise conflicts.
    for (i, f) in facilities.iter().enumerate().take(3) {
        let envs = f.all_envelopes().await;
        assert_eq!(envs.len(), 3, "facility {i} should have all 3 versions");
        let conflicts = detect_conflicts(&envs);
        assert_eq!(
            conflicts.len(),
            3,
            "facility {i} should detect 3 conflicts, got {}",
            conflicts.len()
        );
    }

    // The 3 conflict pairs should be identical across all facilities (as sets).
    let set_a = pair_set(&detect_conflicts(&facilities[0].all_envelopes().await));
    let set_b = pair_set(&detect_conflicts(&facilities[1].all_envelopes().await));
    let set_c = pair_set(&detect_conflicts(&facilities[2].all_envelopes().await));
    assert_eq!(set_a, set_b);
    assert_eq!(set_a, set_c);
    assert_eq!(set_a.len(), 3);
}
