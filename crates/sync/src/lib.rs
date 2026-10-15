//! PSEP (PatientSpace Sync Protocol) — v0.1.
//!
//! Implementation of the protocol specified in `docs/sync-protocol.md`.
//!
//! # Layout
//!
//! - [`vector_clock`] — Lamport-style vector clocks for causality.
//! - [`envelope`] — PSEP envelope: encode, decode, sign, verify, encrypt, decrypt.
//! - [`keys`] — facility keypairs (Ed25519 + X25519), key generation, sealing.
//! - [`log`] — SQLite-backed event log: append-only envelopes, vector clock cache, causal DAG.
//! - [`agent`] — HTTPS sync agent (handshake/push/pull/pull-ack).
//! - [`merge`] — conflict detection and per-field merge policy.
//! - [`sneakernet`] — USB-stick export/import with Ed25519-signed header/trailer.

pub mod agent;
pub mod envelope;
pub mod keys;
pub mod log;
pub mod merge;
pub mod sneakernet;
pub mod vector_clock;

pub use agent::{
    AgentError, HandshakeRequest, HandshakeResponse, PullAckRequest, PullRequest, PushResponse,
    RejectedEnvelope, SyncAgent, SyncStats,
};
pub use envelope::{Envelope, EnvelopeBuilder, EnvelopeError, EnvelopeVersion};
pub use keys::{FacilityKeypair, FacilityPublicKeys};
pub use log::{EventLog, LogError, LogResult};
pub use merge::{compute_merged_view, detect_conflicts, ConflictPair, FieldConflict, MergedView};
pub use sneakernet::{
    export_envelopes, import_envelopes, SneakernetError, SneakernetHeader, SneakernetTrailer,
};
pub use vector_clock::{VectorClock, VectorClockError};
