//! Sneakernet (USB-stick) export and import.
//!
//! Implements the PSEP sneakernet file format specified in
//! `docs/sync-protocol.md` §8.
//!
//! # File format
//!
//! The file is a CBOR sequence: `[header][payload...][trailer]`.
//!
//! - The **header** carries magic, protocol version, origin facility,
//!   export timestamp, payload envelope count, Blake3 payload hash, and an
//!   Ed25519 signature over `canonical_cbor(unsigned_header) || payload_bytes`.
//! - The **payload** is a CBOR-sequence of zero or more PSEP envelopes.
//! - The **trailer** carries the Blake3 payload hash and an Ed25519 signature
//!   over `canonical_cbor(unsigned_trailer) || payload_bytes`.
//!
//! Both signatures are produced by the exporting facility's Ed25519 signing
//! key. Importers verify both signatures against the facility's verifying key
//! before accepting any envelopes, and they recompute the Blake3 hash to
//! detect tampering.

use std::path::Path;

use ed25519_dalek::{Signature, Signer, Verifier, VerifyingKey};
use patientspace_shared::FacilityId;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::envelope::Envelope;
use crate::keys::FacilityKeypair;

/// Magic string at the start of every sneakernet file.
pub const SNEAKERNET_MAGIC: &str = "psep-usb";

/// Sneakernet file format protocol version.
pub const SNEAKERNET_PROTOCOL_VERSION: u32 = 1;

/// Length of an Ed25519 signature, in bytes.
const SIG_LEN: usize = 64;

/// Errors that can arise during sneakernet export/import.
#[derive(Debug, Error)]
pub enum SneakernetError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("cbor encode error: {0}")]
    CborEncode(String),

    #[error("cbor decode error: {0}")]
    CborDecode(String),

    #[error("bad magic: expected {expected}, got {got}")]
    BadMagic { expected: String, got: String },

    #[error("bad protocol version: expected {expected}, got {got}")]
    BadVersion { expected: u32, got: u32 },

    #[error("signature verification failed")]
    BadSignature,

    #[error("payload hash mismatch — file has been tampered with")]
    HashMismatch,

    #[error("envelope count mismatch: header says {expected}, found {actual}")]
    CountMismatch { expected: u64, actual: u64 },

    #[error("envelope error: {0}")]
    Envelope(String),
}

/// The sneakernet file header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SneakernetHeader {
    pub magic: String,
    pub protocol_version: u32,
    pub origin_facility_id: String,
    pub exported_at_unix_ms: u64,
    pub payload_envelope_count: u64,
    pub payload_hash: Vec<u8>,
    pub signature: Vec<u8>,
}

/// The sneakernet file trailer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SneakernetTrailer {
    pub payload_hash: Vec<u8>,
    pub signature: Vec<u8>,
}

/// Internal: the header minus the signature — the bytes that get signed.
#[derive(Debug, Serialize)]
struct UnsignedHeader {
    magic: String,
    protocol_version: u32,
    origin_facility_id: String,
    exported_at_unix_ms: u64,
    payload_envelope_count: u64,
    payload_hash: Vec<u8>,
}

/// Internal: the trailer minus the signature — the bytes that get signed.
#[derive(Debug, Serialize)]
struct UnsignedTrailer {
    payload_hash: Vec<u8>,
}

/// Export a list of envelopes to a sneakernet file at `path`.
///
/// The file is signed by the exporting facility's Ed25519 signing key.
/// See module docs for the file format.
pub fn export_envelopes(
    path: &Path,
    facility_id: &FacilityId,
    keypair: &FacilityKeypair,
    envelopes: &[Envelope],
) -> Result<(), SneakernetError> {
    // 1. Serialize envelopes as CBOR-seq → payload_bytes.
    let mut payload_bytes = Vec::new();
    for env in envelopes {
        ciborium::ser::into_writer(env, &mut payload_bytes)
            .map_err(|e| SneakernetError::CborEncode(e.to_string()))?;
    }

    // 2. Blake3 hash of the payload.
    let payload_hash = blake3::hash(&payload_bytes).as_bytes().to_vec();

    // 3. Build the unsigned header and sign it.
    let unsigned_header = UnsignedHeader {
        magic: SNEAKERNET_MAGIC.to_string(),
        protocol_version: SNEAKERNET_PROTOCOL_VERSION,
        origin_facility_id: facility_id.as_str().to_string(),
        exported_at_unix_ms: now_unix_ms(),
        payload_envelope_count: envelopes.len() as u64,
        payload_hash: payload_hash.clone(),
    };
    let mut header_sig_input = canonical_cbor(&unsigned_header)?;
    header_sig_input.extend_from_slice(&payload_bytes);
    let header_signature = keypair.signing.sign(&header_sig_input).to_bytes().to_vec();

    let header = SneakernetHeader {
        magic: unsigned_header.magic,
        protocol_version: unsigned_header.protocol_version,
        origin_facility_id: unsigned_header.origin_facility_id,
        exported_at_unix_ms: unsigned_header.exported_at_unix_ms,
        payload_envelope_count: unsigned_header.payload_envelope_count,
        payload_hash: unsigned_header.payload_hash,
        signature: header_signature,
    };

    // 4. Build the unsigned trailer and sign it.
    let unsigned_trailer = UnsignedTrailer {
        payload_hash: payload_hash.clone(),
    };
    let mut trailer_sig_input = canonical_cbor(&unsigned_trailer)?;
    trailer_sig_input.extend_from_slice(&payload_bytes);
    let trailer_signature = keypair.signing.sign(&trailer_sig_input).to_bytes().to_vec();

    let trailer = SneakernetTrailer {
        payload_hash,
        signature: trailer_signature,
    };

    // 5. Write: [header CBOR][payload CBOR-seq][trailer CBOR].
    let mut file_bytes = Vec::new();
    ciborium::ser::into_writer(&header, &mut file_bytes)
        .map_err(|e| SneakernetError::CborEncode(e.to_string()))?;
    file_bytes.extend_from_slice(&payload_bytes);
    ciborium::ser::into_writer(&trailer, &mut file_bytes)
        .map_err(|e| SneakernetError::CborEncode(e.to_string()))?;

    std::fs::write(path, &file_bytes)?;
    Ok(())
}

/// Import envelopes from a sneakernet file at `path`.
///
/// Verifies magic, protocol version, Blake3 payload hash, envelope count,
/// header signature, and trailer signature before returning any envelopes.
/// All checks must pass; the file is treated as untrusted.
pub fn import_envelopes(
    path: &Path,
    verifying_key: &VerifyingKey,
) -> Result<Vec<Envelope>, SneakernetError> {
    let file_bytes = std::fs::read(path)?;

    // Parse the header (first CBOR item).
    let mut cursor = std::io::Cursor::new(&file_bytes[..]);
    let header: SneakernetHeader = ciborium::de::from_reader(&mut cursor)
        .map_err(|e| SneakernetError::CborDecode(e.to_string()))?;
    let header_end = cursor.position() as usize;

    // Verify magic and protocol version.
    if header.magic != SNEAKERNET_MAGIC {
        return Err(SneakernetError::BadMagic {
            expected: SNEAKERNET_MAGIC.to_string(),
            got: header.magic,
        });
    }
    if header.protocol_version != SNEAKERNET_PROTOCOL_VERSION {
        return Err(SneakernetError::BadVersion {
            expected: SNEAKERNET_PROTOCOL_VERSION,
            got: header.protocol_version,
        });
    }

    // Parse the remaining CBOR items to locate the trailer.
    let remaining = &file_bytes[header_end..];
    let mut item_offsets: Vec<(usize, usize)> = Vec::new();
    let mut offset = 0;
    while offset < remaining.len() {
        let slice = &remaining[offset..];
        let mut c = std::io::Cursor::new(slice);
        let _: ciborium::Value = ciborium::de::from_reader(&mut c)
            .map_err(|e| SneakernetError::CborDecode(e.to_string()))?;
        let consumed = c.position() as usize;
        item_offsets.push((offset, offset + consumed));
        offset += consumed;
    }

    if item_offsets.is_empty() {
        return Err(SneakernetError::CborDecode(
            "no trailer found in sneakernet file".to_string(),
        ));
    }

    let n = item_offsets.len();
    let (trailer_rel_start, _trailer_rel_end) = item_offsets[n - 1];
    let trailer_bytes = &remaining[trailer_rel_start..];
    let trailer: SneakernetTrailer = ciborium::de::from_reader(trailer_bytes)
        .map_err(|e| SneakernetError::CborDecode(e.to_string()))?;

    // The payload is everything between the header and the trailer.
    let payload_start = header_end;
    let payload_end = header_end + trailer_rel_start;
    let payload_bytes = &file_bytes[payload_start..payload_end];

    // Verify Blake3 hash (header).
    let computed_hash = blake3::hash(payload_bytes).as_bytes().to_vec();
    if computed_hash != header.payload_hash {
        return Err(SneakernetError::HashMismatch);
    }
    // Verify Blake3 hash (trailer).
    if computed_hash != trailer.payload_hash {
        return Err(SneakernetError::HashMismatch);
    }

    // Verify envelope count.
    let envelope_count = (n - 1) as u64; // all items except the trailer
    if envelope_count != header.payload_envelope_count {
        return Err(SneakernetError::CountMismatch {
            expected: header.payload_envelope_count,
            actual: envelope_count,
        });
    }

    // Verify header signature: Ed25519 over canonical_cbor(unsigned_header) || payload_bytes.
    let unsigned_header = UnsignedHeader {
        magic: header.magic.clone(),
        protocol_version: header.protocol_version,
        origin_facility_id: header.origin_facility_id.clone(),
        exported_at_unix_ms: header.exported_at_unix_ms,
        payload_envelope_count: header.payload_envelope_count,
        payload_hash: header.payload_hash.clone(),
    };
    let mut header_sig_input = canonical_cbor(&unsigned_header)?;
    header_sig_input.extend_from_slice(payload_bytes);
    verify_ed25519(verifying_key, &header_sig_input, &header.signature)?;

    // Verify trailer signature: Ed25519 over canonical_cbor(unsigned_trailer) || payload_bytes.
    let unsigned_trailer = UnsignedTrailer {
        payload_hash: trailer.payload_hash.clone(),
    };
    let mut trailer_sig_input = canonical_cbor(&unsigned_trailer)?;
    trailer_sig_input.extend_from_slice(payload_bytes);
    verify_ed25519(verifying_key, &trailer_sig_input, &trailer.signature)?;

    // Parse the payload envelopes.
    let mut envelopes = Vec::with_capacity(envelope_count as usize);
    for i in 0..envelope_count as usize {
        let (rel_start, rel_end) = item_offsets[i];
        let env_bytes = &remaining[rel_start..rel_end];
        let env: Envelope = ciborium::de::from_reader(env_bytes)
            .map_err(|e| SneakernetError::CborDecode(e.to_string()))?;
        envelopes.push(env);
    }

    Ok(envelopes)
}

// -----------------------------------------------------------------
// Helpers
// -----------------------------------------------------------------

/// Canonical CBOR encoding of a value, returned as a `Vec<u8>`.
fn canonical_cbor<T: Serialize>(value: &T) -> Result<Vec<u8>, SneakernetError> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes)
        .map_err(|e| SneakernetError::CborEncode(e.to_string()))?;
    Ok(bytes)
}

/// Verify an Ed25519 signature. Returns `BadSignature` on any failure.
fn verify_ed25519(
    verifying: &VerifyingKey,
    msg: &[u8],
    sig: &[u8],
) -> Result<(), SneakernetError> {
    if sig.len() != SIG_LEN {
        return Err(SneakernetError::BadSignature);
    }
    let sig_bytes: [u8; SIG_LEN] = sig
        .try_into()
        .map_err(|_| SneakernetError::BadSignature)?;
    let signature = Signature::from_bytes(&sig_bytes);
    verifying
        .verify(msg, &signature)
        .map_err(|_| SneakernetError::BadSignature)
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
    use crate::keys::FacilityKeypair;
    use crate::vector_clock::VectorClock;
    use patientspace_shared::{ClinicianId, FacilityId, ResourceId, ResourceKind, VersionId};

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
            created_at_unix_ms: 1_729_000_000_000,
            kind: ResourceKind::Clinical,
            plaintext,
        };
        let recipients: Vec<(&FacilityId, &crate::keys::FacilityPublicKeys)> =
            vec![(fid, &author_pub)];
        builder.seal(kp, &recipients).expect("seal")
    }

    #[test]
    fn export_and_import_round_trip() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path();
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-PETAUKE1");

        let envs: Vec<Envelope> = (0..5).map(|_| make_envelope(&kp, &fid)).collect();
        export_envelopes(path, &fid, &kp, &envs).expect("export");

        let imported = import_envelopes(path, &kp.public_keys().verifying).expect("import");
        assert_eq!(imported.len(), envs.len());
        for (a, b) in envs.iter().zip(imported.iter()) {
            assert_eq!(a.version_id, b.version_id);
            assert_eq!(a.signature, b.signature);
            assert_eq!(a.payload_hash, b.payload_hash);
        }
    }

    #[test]
    fn import_rejects_tampered_payload() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path();
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-PETAUKE1");

        let env = make_envelope(&kp, &fid);
        export_envelopes(path, &fid, &kp, &[env]).expect("export");

        // Read the file, flip a bit in the payload region, and re-write.
        let mut bytes = std::fs::read(path).unwrap();

        // Parse all CBOR items to find the boundary between payload and trailer.
        let mut offset = 0;
        let mut trailer_start = 0;
        while offset < bytes.len() {
            trailer_start = offset;
            let mut c = std::io::Cursor::new(&bytes[offset..]);
            let _: ciborium::Value = ciborium::de::from_reader(&mut c).unwrap();
            offset += c.position() as usize;
        }
        // `trailer_start` is now the start of the trailer (last CBOR item).
        // Flip the last byte of the payload (just before the trailer).
        // This byte is inside the envelope's signature field (a byte string),
        // so it won't break CBOR parsing.
        bytes[trailer_start - 1] ^= 0x01;
        std::fs::write(path, &bytes).unwrap();

        let err = import_envelopes(path, &kp.public_keys().verifying).unwrap_err();
        assert!(
            matches!(err, SneakernetError::HashMismatch | SneakernetError::BadSignature),
            "expected HashMismatch or BadSignature, got {err:?}"
        );
    }

    #[test]
    fn import_rejects_wrong_signing_key() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path();
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-PETAUKE1");

        let env = make_envelope(&kp, &fid);
        export_envelopes(path, &fid, &kp, &[env]).expect("export");

        let stranger = FacilityKeypair::generate();
        let err = import_envelopes(path, &stranger.public_keys().verifying).unwrap_err();
        assert!(matches!(err, SneakernetError::BadSignature));
    }

    #[test]
    fn export_empty_envelope_list() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path();
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-PETAUKE1");

        export_envelopes(path, &fid, &kp, &[]).expect("export");
        let imported = import_envelopes(path, &kp.public_keys().verifying).expect("import");
        assert!(imported.is_empty());
    }

    #[test]
    fn import_nonexistent_file() {
        let kp = FacilityKeypair::generate();
        let path = Path::new("/tmp/psep-sneakernet-does-not-exist-xyz.psep");
        let err = import_envelopes(path, &kp.public_keys().verifying).unwrap_err();
        assert!(matches!(err, SneakernetError::Io(_)));
    }

    #[test]
    fn export_large_batch() {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path();
        let kp = FacilityKeypair::generate();
        let fid = FacilityId::new("F-PETAUKE1");

        let envs: Vec<Envelope> = (0..100).map(|_| make_envelope(&kp, &fid)).collect();
        export_envelopes(path, &fid, &kp, &envs).expect("export");

        let imported = import_envelopes(path, &kp.public_keys().verifying).expect("import");
        assert_eq!(imported.len(), 100);

        // Verify all version IDs match.
        let original_ids: std::collections::HashSet<_> =
            envs.iter().map(|e| e.version_id).collect();
        let imported_ids: std::collections::HashSet<_> =
            imported.iter().map(|e| e.version_id).collect();
        assert_eq!(original_ids, imported_ids);
    }
}
