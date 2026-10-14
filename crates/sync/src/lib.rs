//! PSEP (PatientSpace Sync Protocol) — v0.1.
//!
//! Implementation of the protocol specified in `docs/sync-protocol.md`.
//!
//! # Layout
//!
//! - [`vector_clock`] — Lamport-style vector clocks for causality.
//! - [`envelope`] — PSEP envelope: encode, decode, sign, verify, encrypt, decrypt.
//! - [`keys`] — facility keypairs (Ed25519 + X25519), key generation, sealing.
//!
//! # Status
//!
//! This crate is a work in progress. The envelope and vector clock types are
//! defined; the HTTPS transport agent and SQLite log will land next.

pub mod envelope;
pub mod keys;
pub mod vector_clock;

pub use envelope::{Envelope, EnvelopeError, EnvelopeVersion};
pub use keys::{FacilityKeypair, FacilityPublicKeys};
pub use vector_clock::{VectorClock, VectorClockError};
