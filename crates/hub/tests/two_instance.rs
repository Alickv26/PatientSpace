//! Two-facility integration tests for the PSEP hub.
//!
//! Each test spins up an in-process hub, creates two `FacilityInstance`s
//! (each with its own EventLog + keypair), and exercises cross-facility sync
//! through the hub's HTTP endpoints.
//!
//! Per §5.5 access policy, every envelope is sealed to *both* facilities as
//! recipients — the `other` parameter on `create_*` makes this explicit.

use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use patientspace_hub::endpoints;
use patientspace_hub::registry::InMemoryFacilityRegistry;
use patientspace_hub::state::HubState;
use patientspace_shared::{ClinicianId, FacilityId, ResourceId, ResourceKind, VersionId};
use patientspace_sync::{
    Envelope, EnvelopeBuilder, EventLog, FacilityKeypair, FacilityPublicKeys, HandshakeRequest,
    PullAckRequest, PullRequest, VectorClock,
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

/// Spin up a hub with two facilities (A and B) pre-registered, and return
/// both `FacilityInstance`s + the hub URL.
async fn setup_two_facilities(
    name_a: &str,
    name_b: &str,
) -> (FacilityInstance, FacilityInstance, String) {
    let kp_a = FacilityKeypair::generate();
    let kp_b = FacilityKeypair::generate();
    let pub_a = kp_a.public_keys();
    let pub_b = kp_b.public_keys();

    let fid_a = FacilityId::new(name_a);
    let fid_b = FacilityId::new(name_b);

    let mut registry = InMemoryFacilityRegistry::new();
    registry.register(fid_a.as_str().to_string(), pub_a);
    registry.register(fid_b.as_str().to_string(), pub_b);

    let hub_url = start_hub(registry).await;

    let log_a = EventLog::open_in_memory().await.unwrap();
    let log_b = EventLog::open_in_memory().await.unwrap();

    let inst_a =
        FacilityInstance::with_keypair(fid_a, kp_a, log_a, hub_url.clone()).await;
    let inst_b =
        FacilityInstance::with_keypair(fid_b, kp_b, log_b, hub_url.clone()).await;

    (inst_a, inst_b, hub_url)
}

// ---------------------------------------------------------------------
// FacilityInstance helper
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

    /// Compute the next vector clock for this facility: merge the local
    /// global clock, then bump our own counter to strictly greater than
    /// anything we've observed.
    async fn next_clock(&mut self) -> VectorClock {
        let mut vc = self.log.global_vector_clock().await.expect("global vc");
        let my_observed = vc.get(&self.id);
        self.counter = self.counter.max(my_observed) + 1;
        vc.observe(&self.id, self.counter);
        vc
    }

    /// Author and ingest a genesis Patient envelope.
    /// `other` is included as an additional recipient (per §5.5).
    async fn create_patient(
        &mut self,
        plaintext: Vec<u8>,
        other: Option<(&FacilityId, &FacilityPublicKeys)>,
    ) -> (VersionId, ResourceId) {
        let rid = ResourceId::new();
        let vid = self
            .create_resource_version("Patient", rid.clone(), plaintext, vec![], other)
            .await;
        (vid, rid)
    }

    /// Author and ingest a generic resource version (genesis or child).
    async fn create_resource_version(
        &mut self,
        resource_type: &str,
        resource_id: ResourceId,
        plaintext: Vec<u8>,
        parents: Vec<VersionId>,
        other: Option<(&FacilityId, &FacilityPublicKeys)>,
    ) -> VersionId {
        let vid = VersionId::new();
        let vc = self.next_clock().await;
        let author_pub = self.keypair.public_keys();
        let mut recipients: Vec<(&FacilityId, &FacilityPublicKeys)> =
            vec![(&self.id, &author_pub)];
        if let Some((oid, okeys)) = other {
            recipients.push((oid, okeys));
        }
        let env = EnvelopeBuilder {
            version_id: vid.clone(),
            resource_id: resource_id.clone(),
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

    /// Run a full sync cycle: handshake, push all local envelopes, pull, ingest, ack.
    async fn sync(&mut self) {
        let client = reqwest::Client::new();

        // 1. Handshake (informational — we always push and pull).
        let local_clock = self.log.global_vector_clock().await.expect("global vc");
        let hs_req = HandshakeRequest {
            facility_id: self.id.as_str().to_string(),
            facility_clock: local_clock.clone(),
            resource_watermarks: serde_json::json!({}),
            capabilities: vec!["psep-v1".to_string()],
        };
        let mut hs_body = Vec::new();
        ciborium::ser::into_writer(&hs_req, &mut hs_body).unwrap();
        let _hs_resp = client
            .post(format!("{}/psep/v1/handshake", self.hub_url))
            .header("content-type", "application/cbor")
            .body(hs_body)
            .send()
            .await
            .expect("handshake")
            .bytes()
            .await
            .unwrap();

        // 2. Push every local envelope (hub is idempotent).
        let locals = self.log.list_all(1_000_000).await.expect("list_all");
        if !locals.is_empty() {
            let mut push_body = Vec::new();
            for env in &locals {
                ciborium::ser::into_writer(env, &mut push_body).unwrap();
            }
            let _push_resp = client
                .post(format!("{}/psep/v1/push", self.hub_url))
                .header("content-type", "application/cbor-seq")
                .body(push_body)
                .send()
                .await
                .expect("push");
        }

        // 3. Pull anything we're missing.
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

        // Parse CBOR-seq and ingest each envelope.
        let mut received: Vec<uuid::Uuid> = Vec::new();
        let mut cursor = std::io::Cursor::new(&pull_resp[..]);
        while cursor.position() < pull_resp.len() as u64 {
            let env: Envelope =
                ciborium::de::from_reader(&mut cursor).expect("decode pulled envelope");
            received.push(env.version_id);
            // Idempotent — ingest is a no-op if we already have it.
            self.log.ingest(&env).await.expect("ingest pulled");
        }

        // 4. Pull-ack.
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

    async fn has_envelope(&self, version_id: VersionId) -> bool {
        self.log.get(&version_id).await.is_ok()
    }

    async fn get_and_decrypt(&self, version_id: VersionId) -> Vec<u8> {
        let env = self.log.get(&version_id).await.expect("envelope exists");
        env.open(&self.id, &self.keypair).expect("decrypt")
    }

    async fn local_count(&self) -> u64 {
        self.log.count().await.unwrap()
    }
}

fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[tokio::test]
async fn basic_cross_facility_sync() {
    let (mut a, mut b, _hub_url) = setup_two_facilities("F-A", "F-B").await;
    let b_id = b.id.clone();
    let b_pub = b.keypair.public_keys();

    // A creates a Patient, with B as a recipient.
    let plaintext = br#"{"resourceType":"Patient","id":"P-1","name":{"text":"John"}}"#.to_vec();
    let (vid_a, _rid) = a.create_patient(plaintext, Some((&b_id, &b_pub))).await;

    // A pushes to the hub.
    a.sync().await;

    // B pulls from the hub.
    b.sync().await;

    // B has the envelope and can decrypt it.
    assert!(b.has_envelope(vid_a.clone()).await);
    let decrypted = b.get_and_decrypt(vid_a).await;
    let decrypted_str = String::from_utf8(decrypted).unwrap();
    assert!(decrypted_str.contains("John"));
}

#[tokio::test]
async fn partition_simulation() {
    let (mut a, mut b, _hub_url) = setup_two_facilities("F-A", "F-B").await;
    let b_id = b.id.clone();
    let b_pub = b.keypair.public_keys();

    // Simulate a partition: A creates the Patient but does NOT sync yet.
    let plaintext = br#"{"resourceType":"Patient","id":"P-2"}"#.to_vec();
    let (vid_a, _rid) = a.create_patient(plaintext, Some((&b_id, &b_pub))).await;

    // Partition heals: A syncs (pushes), then B syncs (pulls).
    a.sync().await;
    b.sync().await;

    // B received the late-arriving envelope.
    assert!(b.has_envelope(vid_a).await);
}

#[tokio::test]
async fn multi_resource_sync() {
    let (mut a, mut b, _hub_url) = setup_two_facilities("F-A", "F-B").await;
    let b_id = b.id.clone();
    let b_pub = b.keypair.public_keys();

    // A creates Patient + Encounter + Observation (3 different resources).
    let patient_pt = br#"{"resourceType":"Patient"}"#.to_vec();
    let encounter_pt = br#"{"resourceType":"Encounter"}"#.to_vec();
    let observation_pt = br#"{"resourceType":"Observation"}"#.to_vec();

    let vid_p = a
        .create_resource_version("Patient", ResourceId::new(), patient_pt, vec![], Some((&b_id, &b_pub)))
        .await;
    let vid_e = a
        .create_resource_version("Encounter", ResourceId::new(), encounter_pt, vec![], Some((&b_id, &b_pub)))
        .await;
    let vid_o = a
        .create_resource_version("Observation", ResourceId::new(), observation_pt, vec![], Some((&b_id, &b_pub)))
        .await;

    // Sync A → hub → B.
    a.sync().await;
    b.sync().await;

    // B has all 3 envelopes and can decrypt each.
    for vid in [&vid_p, &vid_e, &vid_o] {
        assert!(b.has_envelope(vid.clone()).await, "B should have {vid}");
        let _pt = b.get_and_decrypt(vid.clone()).await;
    }

    assert_eq!(b.local_count().await, 3);
}

#[tokio::test]
async fn bidirectional_sync() {
    let (mut a, mut b, _hub_url) = setup_two_facilities("F-A", "F-B").await;
    let a_id = a.id.clone();
    let a_pub = a.keypair.public_keys();
    let b_id = b.id.clone();
    let b_pub = b.keypair.public_keys();

    // A creates P1 (with B as recipient), B creates P2 (with A as recipient)
    // — both in the same cycle, before either syncs.
    let (vid_a, _rid_a) = a
        .create_patient(
            br#"{"resourceType":"Patient","name":"A"}"#.to_vec(),
            Some((&b_id, &b_pub)),
        )
        .await;
    let (vid_b, _rid_b) = b
        .create_patient(
            br#"{"resourceType":"Patient","name":"B"}"#.to_vec(),
            Some((&a_id, &a_pub)),
        )
        .await;

    // A.sync: pushes P1 to hub, pulls (hub has nothing new yet).
    a.sync().await;
    // B.sync: pushes P2 to hub, pulls P1.
    b.sync().await;
    // A.sync: pulls P2.
    a.sync().await;

    // Both facilities have both envelopes.
    assert!(a.has_envelope(vid_a.clone()).await, "A has its own");
    assert!(a.has_envelope(vid_b.clone()).await, "A has B's");
    assert!(b.has_envelope(vid_a.clone()).await, "B has A's");
    assert!(b.has_envelope(vid_b.clone()).await, "B has its own");

    // And both can decrypt each other's.
    let a_decrypts_b = a.get_and_decrypt(vid_b.clone()).await;
    assert!(String::from_utf8(a_decrypts_b).unwrap().contains("\"B\""));
    let b_decrypts_a = b.get_and_decrypt(vid_a.clone()).await;
    assert!(String::from_utf8(b_decrypts_a).unwrap().contains("\"A\""));
}

#[tokio::test]
async fn multiple_sync_cycles_convergence() {
    let (mut a, mut b, _hub_url) = setup_two_facilities("F-A", "F-B").await;
    let a_id = a.id.clone();
    let a_pub = a.keypair.public_keys();
    let b_id = b.id.clone();
    let b_pub = b.keypair.public_keys();

    // Interleaved creates: A creates P1, B creates P2, A creates P3.
    let (vid1, _rid1) = a
        .create_patient(
            br#"{"resourceType":"Patient","n":1}"#.to_vec(),
            Some((&b_id, &b_pub)),
        )
        .await;
    let (vid2, _rid2) = b
        .create_patient(
            br#"{"resourceType":"Patient","n":2}"#.to_vec(),
            Some((&a_id, &a_pub)),
        )
        .await;
    let (vid3, _rid3) = a
        .create_patient(
            br#"{"resourceType":"Patient","n":3}"#.to_vec(),
            Some((&b_id, &b_pub)),
        )
        .await;

    // Run a few sync cycles until both converge.
    for _ in 0..5 {
        a.sync().await;
        b.sync().await;
    }

    // Both facilities have all 3 envelopes.
    for vid in [&vid1, &vid2, &vid3] {
        assert!(a.has_envelope(vid.clone()).await, "A missing {vid}");
        assert!(b.has_envelope(vid.clone()).await, "B missing {vid}");
    }
    assert_eq!(a.local_count().await, 3);
    assert_eq!(b.local_count().await, 3);
}
