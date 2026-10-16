//! PSEP sync agent — the HTTPS transport client.
//!
//! Implements the sync protocol specified in `docs/sync-protocol.md` §7.
//! The agent runs as a background tokio task, performing periodic
//! handshake → pull → push cycles against the hub.
//!
//! # Lifecycle
//!
//! 1. **Handshake**: the agent sends its global vector clock to the hub;
//!    the hub responds with what it has that the agent doesn't (and vice
//!    versa).
//! 2. **Pull** (if `push_required`): the agent pulls missing versions from
//!    the hub. Pull-first reduces conflict rate (§7.6).
//! 3. **Push** (if `pull_required`): the agent pushes its locally-authored
//!    versions to the hub, in batches of `max_envelopes_per_push`.
//!
//! # Wire format
//!
//! All HTTP bodies are `application/cbor-seq` (RFC 8742). The handshake,
//! pull-request, pull-ack, and push-response bodies are single CBOR items.
//! The push-request and pull-response bodies are CBOR sequences (concatenated
//! envelopes).
//!
//! # Authentication
//!
//! Every request carries an `Authorization: PSEP-Sig v1` header containing
//! the facility's Ed25519 signature over the request body. The hub verifies
//! this signature before processing the request.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use ed25519_dalek::Signer;
use patientspace_shared::FacilityId;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::envelope::Envelope;
use crate::keys::FacilityKeypair;
use crate::log::{EventLog, LogError};
use crate::vector_clock::VectorClock;

/// Sync-state key for the last-push watermark (little-endian u64 millis).
const SYNC_STATE_LAST_PUSH_MS: &str = "last_push_ms";

/// Default maximum envelopes per push request (§7.5: ~4000 per 8MB body).
const DEFAULT_MAX_ENVELOPES_PER_PUSH: usize = 4000;

/// PSEP handshake request body (§7.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeRequest {
    pub facility_id: String,
    pub facility_clock: VectorClock,
    pub resource_watermarks: serde_json::Value,
    pub capabilities: Vec<String>,
}

/// PSEP handshake response body (§7.2).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeResponse {
    pub hub_clock: VectorClock,
    pub push_size_hint: u64,
    pub pull_size_hint: u64,
    /// True = facility needs to pull from hub (hub has versions facility doesn't).
    pub push_required: bool,
    /// True = facility needs to push to hub (hub is missing facility's versions).
    pub pull_required: bool,
    pub session_id: Uuid,
    pub session_expiry_unix_ms: u64,
}

/// PSEP push response body (§7.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PushResponse {
    pub accepted: Vec<Uuid>,
    pub rejected: Vec<RejectedEnvelope>,
    pub bytes_received: u64,
}

/// A rejected envelope in a push response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RejectedEnvelope {
    pub version_id: Uuid,
    pub reason: String,
}

/// PSEP pull request body (§7.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequest {
    pub facility_clock: VectorClock,
}

/// PSEP pull-ack request body (§7.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullAckRequest {
    pub received: Vec<Uuid>,
}

/// Per-sync-cycle statistics.
#[derive(Debug, Clone, Default)]
pub struct SyncStats {
    pub envelopes_pushed: u64,
    pub bytes_pushed: u64,
    pub envelopes_rejected: u64,
    pub envelopes_pulled: u64,
    pub bytes_pulled: u64,
    pub sync_duration_ms: u64,
}

/// Errors that can arise from the sync agent.
#[derive(Debug, Error)]
pub enum AgentError {
    #[error("bad hub URL: {0}")]
    BadHubUrl(String),

    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("handshake failed: {0}")]
    HandshakeFailed(String),

    #[error("push failed: {0}")]
    PushFailed(String),

    #[error("pull failed: {0}")]
    PullFailed(String),

    #[error("CBOR encode error: {0}")]
    CborEncode(String),

    #[error("CBOR decode error: {0}")]
    CborDecode(String),

    #[error("envelope encode error: {0}")]
    EnvelopeEncode(String),

    #[error("database error: {0}")]
    Db(#[from] LogError),

    #[error("base64 error: {0}")]
    Base64(String),
}

/// The PSEP sync agent.
///
/// Owns the HTTP client, facility keypair, and event log handle. Cheap to
/// clone (all fields are `Arc` or `Clone`).
#[derive(Clone)]
pub struct SyncAgent {
    hub_url: reqwest::Url,
    facility_id: FacilityId,
    facility_keypair: Arc<FacilityKeypair>,
    log: EventLog,
    http: reqwest::Client,
    max_envelopes_per_push: usize,
}

impl SyncAgent {
    /// Create a new sync agent.
    ///
    /// The HTTP client is configured with a 30-second timeout.
    pub fn new(
        hub_url: reqwest::Url,
        facility_id: FacilityId,
        facility_keypair: Arc<FacilityKeypair>,
        log: EventLog,
    ) -> Self {
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .expect("reqwest client build");
        Self {
            hub_url,
            facility_id,
            facility_keypair,
            log,
            http,
            max_envelopes_per_push: DEFAULT_MAX_ENVELOPES_PER_PUSH,
        }
    }

    /// Test helper: set the max envelopes per push batch.
    #[cfg(test)]
    pub fn with_max_envelopes_per_push(mut self, max: usize) -> Self {
        self.max_envelopes_per_push = max;
        self
    }

    // -----------------------------------------------------------------
    // Wire-level methods
    // -----------------------------------------------------------------

    /// Perform the PSEP handshake.
    ///
    /// Sends the facility's global vector clock to the hub and receives
    /// the hub's view of what needs to sync.
    pub async fn handshake(&self) -> Result<HandshakeResponse, AgentError> {
        let facility_clock = self.log.global_vector_clock().await?;
        let request = HandshakeRequest {
            facility_id: self.facility_id.as_str().to_string(),
            facility_clock,
            resource_watermarks: serde_json::json!({}),
            capabilities: vec![
                "psep-v1".to_string(),
                "cbor".to_string(),
                "sealed-box-v1".to_string(),
            ],
        };

        let body = cbor_encode(&request)?;
        let auth = self.sign_authorization(&body);

        let url = self
            .hub_url
            .join("psep/v1/handshake")
            .map_err(|e| AgentError::BadHubUrl(e.to_string()))?;

        let response = self
            .http
            .post(url)
            .header("Authorization", auth)
            .header("Content-Type", "application/cbor")
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AgentError::HandshakeFailed(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let bytes = response.bytes().await?;
        let hs: HandshakeResponse = cbor_decode(&bytes)?;
        Ok(hs)
    }

    /// Push a batch of envelopes to the hub.
    ///
    /// The body is a CBOR sequence of envelopes. The hub acknowledges each
    /// envelope by `version_id`.
    pub async fn push(
        &self,
        session_id: Uuid,
        envelopes: &[Envelope],
    ) -> Result<PushResponse, AgentError> {
        let body = cbor_seq_encode(envelopes)?;
        let auth = self.sign_authorization(&body);

        let url = self
            .hub_url
            .join(&format!("psep/v1/push?session_id={session_id}"))
            .map_err(|e| AgentError::BadHubUrl(e.to_string()))?;

        let response = self
            .http
            .post(url)
            .header("Authorization", auth)
            .header("Content-Type", "application/cbor-seq")
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AgentError::PushFailed(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let bytes = response.bytes().await?;
        let push_response: PushResponse = cbor_decode(&bytes)?;
        Ok(push_response)
    }

    /// Pull missing envelopes from the hub.
    ///
    /// Sends the facility's vector clock; the hub responds with a CBOR-seq
    /// of envelopes the facility doesn't have. Each envelope is ingested
    /// into the local log, then a pull-ack is sent.
    ///
    /// Returns the list of ingested `version_id`s.
    pub async fn pull(&self, session_id: Uuid) -> Result<Vec<Uuid>, AgentError> {
        let facility_clock = self.log.global_vector_clock().await?;
        let request = PullRequest { facility_clock };
        let body = cbor_encode(&request)?;
        let auth = self.sign_authorization(&body);

        let url = self
            .hub_url
            .join(&format!("psep/v1/pull?session_id={session_id}"))
            .map_err(|e| AgentError::BadHubUrl(e.to_string()))?;

        let response = self
            .http
            .post(url)
            .header("Authorization", auth)
            .header("Content-Type", "application/cbor")
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AgentError::PullFailed(format!(
                "HTTP {}",
                response.status()
            )));
        }

        let bytes = response.bytes().await?;
        let envelopes: Vec<Envelope> = cbor_seq_decode(&bytes)?;

        let mut ingested = Vec::with_capacity(envelopes.len());
        for env in &envelopes {
            // Ingest is idempotent (INSERT OR IGNORE), so duplicates are safe.
            self.log.ingest(env).await?;
            ingested.push(env.version_id);
        }

        // Acknowledge receipt (non-fatal on failure).
        if !ingested.is_empty() {
            let _ = self.pull_ack(session_id, &ingested).await;
        }

        Ok(ingested)
    }

    /// Send a pull-ack to the hub.
    ///
    /// Non-fatal: errors are logged but do not fail the sync cycle.
    pub async fn pull_ack(
        &self,
        session_id: Uuid,
        version_ids: &[Uuid],
    ) -> Result<(), AgentError> {
        let request = PullAckRequest {
            received: version_ids.to_vec(),
        };
        let body = cbor_encode(&request)?;
        let auth = self.sign_authorization(&body);

        let url = self
            .hub_url
            .join(&format!("psep/v1/pull-ack?session_id={session_id}"))
            .map_err(|e| AgentError::BadHubUrl(e.to_string()))?;

        let response = self
            .http
            .post(url)
            .header("Authorization", auth)
            .header("Content-Type", "application/cbor")
            .body(body)
            .send()
            .await?;

        if !response.status().is_success() {
            tracing::warn!(
                "pull-ack failed with HTTP {} (non-fatal)",
                response.status()
            );
        }
        Ok(())
    }

    // -----------------------------------------------------------------
    // Orchestration
    // -----------------------------------------------------------------

    /// Run one complete sync cycle: handshake → pull (if push_required) →
    /// push (if pull_required).
    ///
    /// Pull-first reduces conflict rate (§7.6): the facility gets the latest
    /// global state before pushing its own edits.
    pub async fn run_once(&self) -> Result<SyncStats, AgentError> {
        let start = std::time::Instant::now();
        let mut stats = SyncStats::default();

        let hs = self.handshake().await?;
        let session_id = hs.session_id;

        // Pull first (if the facility needs hub's versions).
        if hs.push_required {
            let pulled = self.pull(session_id).await?;
            stats.envelopes_pulled = pulled.len() as u64;
        }

        // Push second (if the hub needs facility's versions).
        if hs.pull_required {
            self.push_phase(session_id, &mut stats).await?;
        }

        stats.sync_duration_ms = start.elapsed().as_millis() as u64;
        Ok(stats)
    }

    /// Run the push phase: fetch all envelopes from the log since the last
    /// watermark, then push them in batches of `max_envelopes_per_push`.
    ///
    /// The watermark is updated once at the end of the phase (not per batch)
    /// to avoid skipping envelopes that were ingested before the fetch time.
    async fn push_phase(
        &self,
        session_id: Uuid,
        stats: &mut SyncStats,
    ) -> Result<(), AgentError> {
        let watermark = self.get_last_push_ms().await?;
        let fetch_time = now_unix_ms();

        // Fetch all pending envelopes in one query. The limit is set high
        // enough to cover a full sync cycle; anything beyond that is caught
        // on the next cycle.
        let envelopes = self
            .log
            .list_by_facility(&self.facility_id, watermark, u32::MAX)
            .await?;

        if envelopes.is_empty() {
            // Advance the watermark past the current time so we don't
            // re-query the same (empty) range on the next cycle.
            self.set_last_push_ms(fetch_time).await?;
            return Ok(());
        }

        // Push in batches of `max_envelopes_per_push`.
        for chunk in envelopes.chunks(self.max_envelopes_per_push) {
            let response = self.push(session_id, chunk).await?;
            stats.envelopes_pushed += response.accepted.len() as u64;
            stats.envelopes_rejected += response.rejected.len() as u64;
            stats.bytes_pushed += response.bytes_received;
        }

        // Advance the watermark to the fetch time. Any envelope ingested
        // after `fetch_time` will be caught on the next cycle.
        self.set_last_push_ms(fetch_time).await?;

        Ok(())
    }

    /// Run the sync agent forever, with additive backoff on failure.
    ///
    /// Backoff schedule: 5s → 10s → 30s → 60s → 5min cap. Resets to 5s on
    /// success. Shuts down cleanly when the `shutdown` token is cancelled.
    pub async fn run_forever(self, shutdown: CancellationToken) {
        let mut backoff = Duration::from_secs(5);
        loop {
            tokio::select! {
                _ = shutdown.cancelled() => break,
                _ = tokio::time::sleep(backoff) => {}
            }
            if shutdown.is_cancelled() {
                break;
            }

            match self.run_once().await {
                Ok(stats) => {
                    tracing::info!(
                        envelopes_pushed = stats.envelopes_pushed,
                        envelopes_pulled = stats.envelopes_pulled,
                        duration_ms = stats.sync_duration_ms,
                        "sync cycle completed"
                    );
                    backoff = Duration::from_secs(5);
                }
                Err(e) => {
                    tracing::warn!(error = %e, backoff_secs = backoff.as_secs(), "sync cycle failed");
                    backoff = match backoff.as_secs() {
                        5 => Duration::from_secs(10),
                        10 => Duration::from_secs(30),
                        30 => Duration::from_secs(60),
                        _ => Duration::from_secs(300),
                    };
                }
            }
        }
    }

    // -----------------------------------------------------------------
    // Internal helpers
    // -----------------------------------------------------------------

    /// Build the `Authorization: PSEP-Sig v1` header value for a request body.
    fn sign_authorization(&self, body: &[u8]) -> String {
        let sig = self.facility_keypair.signing.sign(body);
        let sig_b64 = base64::engine::general_purpose::STANDARD.encode(sig.to_bytes());
        format!(
            "PSEP-Sig v1 facility_id={},signature={}",
            self.facility_id.as_str(),
            sig_b64
        )
    }

    async fn get_last_push_ms(&self) -> Result<u64, AgentError> {
        let val = self
            .log
            .get_sync_state(&self.facility_id, SYNC_STATE_LAST_PUSH_MS)
            .await?;
        match val {
            Some(bytes) if bytes.len() == 8 => {
                let arr: [u8; 8] = bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| AgentError::Db(LogError::Invalid("bad sync_state value".to_string())))?;
                Ok(u64::from_le_bytes(arr))
            }
            _ => Ok(0),
        }
    }

    async fn set_last_push_ms(&self, ms: u64) -> Result<(), AgentError> {
        self.log
            .set_sync_state(
                &self.facility_id,
                SYNC_STATE_LAST_PUSH_MS,
                &ms.to_le_bytes(),
            )
            .await?;
        Ok(())
    }
}

// -----------------------------------------------------------------
// CBOR helpers
// -----------------------------------------------------------------

fn cbor_encode<T: Serialize>(value: &T) -> Result<Vec<u8>, AgentError> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes)
        .map_err(|e| AgentError::CborEncode(e.to_string()))?;
    Ok(bytes)
}

fn cbor_decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, AgentError> {
    ciborium::de::from_reader(bytes).map_err(|e| AgentError::CborDecode(e.to_string()))
}

fn cbor_seq_encode(envelopes: &[Envelope]) -> Result<Vec<u8>, AgentError> {
    let mut bytes = Vec::new();
    for env in envelopes {
        ciborium::ser::into_writer(env, &mut bytes)
            .map_err(|e| AgentError::CborEncode(e.to_string()))?;
    }
    Ok(bytes)
}

fn cbor_seq_decode<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<Vec<T>, AgentError> {
    let mut items = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() {
        let mut cursor = std::io::Cursor::new(&bytes[offset..]);
        let item: T = ciborium::de::from_reader(&mut cursor)
            .map_err(|e| AgentError::CborDecode(e.to_string()))?;
        offset += cursor.position() as usize;
        items.push(item);
    }
    Ok(items)
}

fn now_unix_ms() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// -----------------------------------------------------------------
// Tests
// -----------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::envelope::EnvelopeBuilder;
    use crate::vector_clock::VectorClock;
    use axum::extract::{DefaultBodyLimit, State};
    use axum::routing::post;
    use axum::Router;
    use patientspace_shared::{
        ClinicianId, FacilityId, ResourceId, ResourceKind, VersionId,
    };
    use std::net::SocketAddr;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::Mutex;

    // --- Mock hub ---

    #[derive(Clone, Default)]
    struct MockHubState {
        handshake_count: Arc<AtomicU32>,
        push_count: Arc<AtomicU32>,
        pull_count: Arc<AtomicU32>,
        pull_ack_count: Arc<AtomicU32>,
        pull_envelopes: Arc<Mutex<Vec<Vec<u8>>>>,
        received_envelopes: Arc<Mutex<Vec<Uuid>>>,
        push_required: Arc<AtomicBool>,
        pull_required: Arc<AtomicBool>,
        handshake_should_fail: Arc<AtomicBool>,
        events: Arc<Mutex<Vec<String>>>,
    }

    impl MockHubState {
        fn record_event(&self, name: &str) {
            self.events.lock().unwrap().push(name.to_string());
        }
    }

    async fn handle_handshake(
        State(state): State<MockHubState>,
        body: axum::body::Bytes,
    ) -> Result<(axum::http::StatusCode, Vec<u8>), axum::http::StatusCode> {
        state.handshake_count.fetch_add(1, Ordering::SeqCst);
        state.record_event("handshake");

        if state.handshake_should_fail.load(Ordering::SeqCst) {
            return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR);
        }

        let _req: HandshakeRequest =
            ciborium::de::from_reader(body.as_ref()).map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

        let push_required = state.push_required.load(Ordering::SeqCst);
        let pull_required = state.pull_required.load(Ordering::SeqCst);

        let response = HandshakeResponse {
            hub_clock: VectorClock::new(),
            push_size_hint: if push_required { 10 } else { 0 },
            pull_size_hint: if pull_required { 10 } else { 0 },
            push_required,
            pull_required,
            session_id: Uuid::now_v7(),
            session_expiry_unix_ms: now_unix_ms() + 300_000,
        };

        let mut bytes = Vec::new();
        ciborium::ser::into_writer(&response, &mut bytes)
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok((axum::http::StatusCode::OK, bytes))
    }

    async fn handle_push(
        State(state): State<MockHubState>,
        body: axum::body::Bytes,
    ) -> Result<(axum::http::StatusCode, Vec<u8>), axum::http::StatusCode> {
        state.push_count.fetch_add(1, Ordering::SeqCst);
        state.record_event("push");

        let envelopes: Vec<Envelope> = cbor_seq_decode(body.as_ref())
            .map_err(|_| axum::http::StatusCode::BAD_REQUEST)?;

        let mut accepted = Vec::new();
        let mut rejected = Vec::new();
        let mut received = state.received_envelopes.lock().unwrap();
        for env in envelopes {
            if received.contains(&env.version_id) {
                rejected.push(RejectedEnvelope {
                    version_id: env.version_id,
                    reason: "duplicate".to_string(),
                });
            } else {
                received.push(env.version_id);
                accepted.push(env.version_id);
            }
        }

        let response = PushResponse {
            accepted,
            rejected,
            bytes_received: body.len() as u64,
        };

        let mut bytes = Vec::new();
        ciborium::ser::into_writer(&response, &mut bytes)
            .map_err(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR)?;
        Ok((axum::http::StatusCode::OK, bytes))
    }

    async fn handle_pull(
        State(state): State<MockHubState>,
        _body: axum::body::Bytes,
    ) -> Result<(axum::http::StatusCode, Vec<u8>), axum::http::StatusCode> {
        state.pull_count.fetch_add(1, Ordering::SeqCst);
        state.record_event("pull");

        let envelopes = state.pull_envelopes.lock().unwrap();
        let mut bytes = Vec::new();
        for env_bytes in envelopes.iter() {
            bytes.extend_from_slice(env_bytes);
        }
        Ok((axum::http::StatusCode::OK, bytes))
    }

    async fn handle_pull_ack(
        State(state): State<MockHubState>,
        body: axum::body::Bytes,
    ) -> axum::http::StatusCode {
        state.pull_ack_count.fetch_add(1, Ordering::SeqCst);
        state.record_event("pull_ack");

        let _req: PullAckRequest = ciborium::de::from_reader(body.as_ref()).unwrap();
        axum::http::StatusCode::OK
    }

    async fn start_mock_hub(state: MockHubState) -> SocketAddr {
        let app = Router::new()
            .route("/psep/v1/handshake", post(handle_handshake))
            .route("/psep/v1/push", post(handle_push))
            .route("/psep/v1/pull", post(handle_pull))
            .route("/psep/v1/pull-ack", post(handle_pull_ack))
            .layer(DefaultBodyLimit::disable())
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        addr
    }

    // --- Test helpers ---

    fn make_envelope(kp: &FacilityKeypair, fid: &FacilityId) -> Envelope {
        let plaintext = br#"{"resourceType":"Patient","id":"U-test"}"#.to_vec();
        let author_pub = kp.public_keys();
        let builder = EnvelopeBuilder {
            version_id: VersionId::new(),
            resource_id: ResourceId::new(),
            resource_type: "Patient".to_string(),
            vector_clock: VectorClock::from_facility(fid, 1),
            parent_version_ids: vec![],
            author_facility_id: fid.clone(),
            author_clinician_id: ClinicianId::new(),
            created_at_unix_ms: now_unix_ms(),
            kind: ResourceKind::Clinical,
            plaintext,
        };
        let recipients: Vec<(&FacilityId, &crate::keys::FacilityPublicKeys)> =
            vec![(&fid, &author_pub)];
        builder.seal(kp, &recipients).unwrap()
    }

    async fn make_agent(
        state: &MockHubState,
    ) -> (SyncAgent, MockHubState) {
        let addr = start_mock_hub(state.clone()).await;
        let url = reqwest::Url::parse(&format!("http://{addr}")).unwrap();
        let log = EventLog::open_in_memory().await.unwrap();
        let kp = Arc::new(FacilityKeypair::generate());
        let fid = FacilityId::new("F-TEST");
        let agent = SyncAgent::new(url, fid, kp, log);
        (agent, state.clone())
    }

    // --- Tests ---

    #[tokio::test]
    async fn handshake_and_push_round_trip() {
        let state = MockHubState::default();
        state.pull_required.store(true, Ordering::SeqCst);

        let (agent, state) = make_agent(&state).await;

        // Put 3 envelopes in the log.
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-TEST");
        let envs: Vec<Envelope> = (0..3).map(|_| make_envelope(&kp, &fid)).collect();
        for env in &envs {
            agent.log.ingest(env).await.unwrap();
        }

        let hs = agent.handshake().await.expect("handshake");
        let response = agent.push(hs.session_id, &envs).await.expect("push");

        assert_eq!(response.accepted.len(), 3);
        assert!(response.rejected.is_empty());
        assert_eq!(state.push_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn run_once_with_empty_log_pushes_nothing() {
        let state = MockHubState::default();
        state.pull_required.store(true, Ordering::SeqCst);

        let (agent, state) = make_agent(&state).await;

        let stats = agent.run_once().await.expect("run_once");
        assert_eq!(stats.envelopes_pushed, 0);
        assert_eq!(state.push_count.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn handshake_failure_returns_error() {
        let state = MockHubState::default();
        state.handshake_should_fail.store(true, Ordering::SeqCst);

        let (agent, _state) = make_agent(&state).await;

        let err = agent.handshake().await.unwrap_err();
        assert!(matches!(err, AgentError::HandshakeFailed(_)));
    }

    #[tokio::test]
    async fn push_respects_max_envelopes_per_push() {
        let state = MockHubState::default();
        state.pull_required.store(true, Ordering::SeqCst);

        let addr = start_mock_hub(state.clone()).await;
        let url = reqwest::Url::parse(&format!("http://{addr}")).unwrap();
        let log = EventLog::open_in_memory().await.unwrap();
        let kp = Arc::new(FacilityKeypair::generate());
        let fid = FacilityId::new("F-TEST");
        let agent = SyncAgent::new(url, fid.clone(), kp, log)
            .with_max_envelopes_per_push(2);

        // Put 5 envelopes in the log.
        let kp2 = FacilityKeypair::generate();
        for _ in 0..5 {
            let env = make_envelope(&kp2, &fid);
            agent.log.ingest(&env).await.unwrap();
        }

        let _stats = agent.run_once().await.expect("run_once");

        // 5 envelopes / 2 per batch = 3 push requests.
        assert_eq!(state.push_count.load(Ordering::SeqCst), 3);
        assert_eq!(
            state.received_envelopes.lock().unwrap().len(),
            5
        );
    }

    #[tokio::test]
    async fn run_forever_shuts_down_cleanly() {
        let state = MockHubState::default();
        let (agent, _state) = make_agent(&state).await;

        let shutdown = CancellationToken::new();
        let shutdown_clone = shutdown.clone();

        let handle = tokio::spawn(async move {
            agent.run_forever(shutdown_clone).await;
        });

        // Give it a moment to start, then shut down.
        tokio::time::sleep(Duration::from_millis(100)).await;
        shutdown.cancel();

        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("run_forever should shut down within 2s")
            .expect("task should not panic");
    }

    #[tokio::test]
    async fn second_run_only_pushes_new_envelopes() {
        let state = MockHubState::default();
        state.pull_required.store(true, Ordering::SeqCst);

        let addr = start_mock_hub(state.clone()).await;
        let url = reqwest::Url::parse(&format!("http://{addr}")).unwrap();
        let log = EventLog::open_in_memory().await.unwrap();
        let kp = Arc::new(FacilityKeypair::generate());
        let fid = FacilityId::new("F-TEST");
        let agent = SyncAgent::new(url, fid.clone(), kp, log);

        let kp2 = FacilityKeypair::generate();

        // First run: push 3 envelopes.
        for _ in 0..3 {
            let env = make_envelope(&kp2, &fid);
            agent.log.ingest(&env).await.unwrap();
        }
        agent.run_once().await.expect("first run_once");
        assert_eq!(state.received_envelopes.lock().unwrap().len(), 3);

        // Second run: add 2 more, should only push 2.
        for _ in 0..2 {
            let env = make_envelope(&kp2, &fid);
            agent.log.ingest(&env).await.unwrap();
        }
        agent.run_once().await.expect("second run_once");
        assert_eq!(state.received_envelopes.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn pull_ingests_envelopes_from_hub() {
        let state = MockHubState::default();
        state.push_required.store(true, Ordering::SeqCst);

        // Create envelopes to serve on pull.
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-HUB");
        let hub_envs: Vec<Envelope> = (0..3).map(|_| make_envelope(&kp, &fid)).collect();
        let hub_cbor: Vec<Vec<u8>> = hub_envs.iter().map(|e| e.to_cbor().unwrap()).collect();
        state.pull_envelopes.lock().unwrap().extend(hub_cbor);

        let (agent, _state) = make_agent(&state).await;

        assert_eq!(agent.log.count().await.unwrap(), 0);

        let _stats = agent.run_once().await.expect("run_once");

        // All 3 envelopes should have been ingested.
        assert_eq!(agent.log.count().await.unwrap(), 3);
    }

    #[tokio::test]
    async fn pull_is_idempotent() {
        let state = MockHubState::default();
        state.push_required.store(true, Ordering::SeqCst);

        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-HUB");
        let hub_envs: Vec<Envelope> = (0..3).map(|_| make_envelope(&kp, &fid)).collect();
        let hub_cbor: Vec<Vec<u8>> = hub_envs.iter().map(|e| e.to_cbor().unwrap()).collect();
        state.pull_envelopes.lock().unwrap().extend(hub_cbor);

        let (agent, _state) = make_agent(&state).await;

        // First pull.
        let hs = agent.handshake().await.expect("handshake");
        let pulled1 = agent.pull(hs.session_id).await.expect("first pull");
        assert_eq!(pulled1.len(), 3);
        assert_eq!(agent.log.count().await.unwrap(), 3);

        // Second pull (same envelopes) — log should deduplicate.
        let hs2 = agent.handshake().await.expect("handshake 2");
        let pulled2 = agent.pull(hs2.session_id).await.expect("second pull");
        assert_eq!(pulled2.len(), 3);
        assert_eq!(agent.log.count().await.unwrap(), 3); // still 3, not 6
    }

    #[tokio::test]
    async fn run_once_pulls_then_pushes() {
        let state = MockHubState::default();
        state.push_required.store(true, Ordering::SeqCst);
        state.pull_required.store(true, Ordering::SeqCst);

        // Envelopes for the hub to serve on pull.
        let hub_kp = FacilityKeypair::generate();
        let hub_fid = FacilityId::new("F-HUB");
        let hub_envs: Vec<Envelope> = (0..2).map(|_| make_envelope(&hub_kp, &hub_fid)).collect();
        let hub_cbor: Vec<Vec<u8>> = hub_envs.iter().map(|e| e.to_cbor().unwrap()).collect();
        state.pull_envelopes.lock().unwrap().extend(hub_cbor);

        let (agent, state) = make_agent(&state).await;

        // Put some local envelopes to push.
        let local_kp = FacilityKeypair::generate();
        let local_fid = FacilityId::new("F-TEST");
        for _ in 0..2 {
            let env = make_envelope(&local_kp, &local_fid);
            agent.log.ingest(&env).await.unwrap();
        }

        let _stats = agent.run_once().await.expect("run_once");

        // Both pull and push should have happened.
        assert!(state.pull_count.load(Ordering::SeqCst) > 0, "pull should have happened");
        assert!(state.push_count.load(Ordering::SeqCst) > 0, "push should have happened");

        // Verify the order: pull before push.
        let events = state.events.lock().unwrap();
        let pull_idx = events.iter().position(|e| e == "pull");
        let push_idx = events.iter().position(|e| e == "push");
        assert!(pull_idx.is_some(), "pull event should be recorded");
        assert!(push_idx.is_some(), "push event should be recorded");
        assert!(
            pull_idx.unwrap() < push_idx.unwrap(),
            "pull should happen before push"
        );
    }
}
