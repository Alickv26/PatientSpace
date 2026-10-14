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
//!
//! # Status
//!
//! Envelope, vector clock, keys, and the SQLite log are implemented.
//! The HTTPS transport agent and sync agent will land next (P1-T03, P1-T04).

pub mod envelope;
pub mod keys;
pub mod log;
pub mod vector_clock;

pub use envelope::{Envelope, EnvelopeBuilder, EnvelopeError, EnvelopeVersion};
pub use keys::{FacilityKeypair, FacilityPublicKeys};
pub use log::{EventLog, LogError, LogResult};
pub use vector_clock::{VectorClock, VectorClockError};
