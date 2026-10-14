//! PSEP envelope — the immutable wire unit of the sync protocol.
//!
//! See `docs/sync-protocol.md` §4 for the schema and §5 for the encryption
//! procedure.
//!
//! # Lifecycle
//!
//! ```text
//! Author:
//!   1. Build EnvelopeBuilder with plaintext FHIR payload + recipient list.
//!   2. Call .seal(&author_keypair, &recipient_public_keys) -> Envelope.
//!   3. Serialize envelope to CBOR bytes for the wire.
//!
//! Recipient:
//!   1. Parse CBOR bytes back into Envelope.
//!   2. Call .verify_signature(&author_verifying_key) -> Result.
//!   3. Call .open(&recipient_keypair) -> Plaintext (FHIR resource bytes).
//! ```

use aes_gcm::{Aes256Gcm, Key, Nonce};
use aes_gcm::aead::{Aead, KeyInit};
use ed25519_dalek::Signer;
use patientspace_shared::{ClinicianId, FacilityId, ResourceId, ResourceKind, VersionId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::keys::{FacilityKeypair, FacilityPublicKeys};
use crate::vector_clock::VectorClock;

/// PSEP protocol version. Bumped on incompatible wire format changes.
pub const PROTOCOL_MAJOR: u8 = 1;
pub const PROTOCOL_MINOR: u8 = 0;

/// Magic prefix on every envelope, used for fast sniffing.
pub const MAGIC: &str = "psep";

/// Length of an Ed25519 signature, in bytes.
pub const SIG_LEN: usize = 64;
/// Length of a SHA-256 hash, in bytes.
pub const HASH_LEN: usize = 32;
/// Length of an AES-GCM-256 key, in bytes.
pub const AES_KEY_LEN: usize = 32;
/// Length of an AES-GCM nonce, in bytes.
pub const AES_NONCE_LEN: usize = 12;
/// Length of an X25519 public key, in bytes.
pub const X25519_PUBKEY_LEN: usize = 32;

#[derive(Debug, Error)]
pub enum EnvelopeError {
    #[error("envelope magic mismatch: expected {expected}, got {got}")]
    BadMagic { expected: String, got: String },

    #[error("envelope protocol version mismatch: expected {expected}, got {got}")]
    BadVersion { expected: u8, got: u8 },

    #[error("cbor encode error: {0}")]
    CborEncode(String),

    #[error("cbor decode error: {0}")]
    CborDecode(String),

    #[error("signature verification failed")]
    BadSignature,

    #[error("payload hash mismatch — decrypted payload is corrupt or has been tampered with")]
    PayloadHashMismatch,

    #[error("recipient key not found in envelope (facility_id={0})")]
    RecipientNotFound(String),

    #[error("decryption failed: {0}")]
    DecryptionFailed(String),

    #[error("encryption failed: {0}")]
    EncryptionFailed(String),

    #[error("invalid parent count: expected 0, 1, or 2, got {0}")]
    BadParentCount(usize),
}

/// The PSEP envelope — one immutable, signed, encrypted resource version.
///
/// Wire format note: byte arrays (`signature`, `payload_hash`, `payload_nonce`,
/// `eph_pubkey`) are serialized as CBOR byte strings. We use `Vec<u8>` rather
/// than fixed-size arrays because serde's blanket `Serialize` impl only covers
/// arrays up to length 32.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    pub magic: String,
    pub protocol_major: u8,
    pub protocol_minor: u8,

    pub version_id: Uuid,
    pub resource_id: Uuid,
    pub resource_type: String,
    pub vector_clock: VectorClock,
    pub parent_version_ids: Vec<Uuid>,
    pub author_facility_id: String,
    pub author_clinician_id: Uuid,
    pub created_at_unix_ms: u64,
    pub kind: String,

    /// The encrypted FHIR payload (AES-GCM-256 ciphertext).
    pub payload_ciphertext: Vec<u8>,
    /// SHA-256 of the plaintext payload (32 bytes).
    pub payload_hash: Vec<u8>,
    /// AES-GCM nonce (12 bytes).
    pub payload_nonce: Vec<u8>,

    /// Per-recipient sealed key boxes — one per authorized reader facility.
    /// Each entry contains the recipient's facility_id, the ephemeral X25519
    /// pubkey used to derive the shared secret, and the sealed-box-encrypted
    /// AES key.
    pub recipient_keys: Vec<RecipientKeyBox>,

    /// Ed25519 signature by the author facility over the canonical CBOR of
    /// the envelope (excluding this field). 64 bytes.
    pub signature: Vec<u8>,
}

/// A sealed-box-encrypted copy of the AES-GCM payload key, addressed to one
/// specific recipient facility.
///
/// Construction (see `seal_aes_key_to_recipient`):
///   1. Author generates an ephemeral X25519 keypair.
///   2. ECDH(eph_secret, recipient_pub) -> shared_secret (32 bytes).
///   3. Derive AES key + nonce from shared_secret via HKDF-SHA256.
///   4. AES-GCM-encrypt the per-event AES key under the derived key+nonce.
///   5. Output = eph_pubkey || derived_nonce || ciphertext.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipientKeyBox {
    pub reader_facility_id: String,
    /// Ephemeral X25519 pubkey used by the author for this recipient (32 bytes).
    pub eph_pubkey: Vec<u8>,
    /// Sealed-box: eph_pubkey || derived_nonce (12B) || ciphertext (48B = 32B key + 16B tag).
    pub sealed_key: Vec<u8>,
}

/// Builder for an envelope before sealing.
pub struct EnvelopeBuilder {
    pub version_id: VersionId,
    pub resource_id: ResourceId,
    pub resource_type: String,
    pub vector_clock: VectorClock,
    pub parent_version_ids: Vec<VersionId>,
    pub author_facility_id: FacilityId,
    pub author_clinician_id: ClinicianId,
    pub created_at_unix_ms: u64,
    pub kind: ResourceKind,
    /// Plaintext FHIR resource, CBOR-serialized.
    pub plaintext: Vec<u8>,
}

impl EnvelopeBuilder {
    /// Seal this envelope: encrypt the payload, sign it, produce the final
    /// immutable envelope.
    ///
    /// `recipients` is the full list of facilities authorized to decrypt this
    /// envelope (including the author itself).
    pub fn seal(
        self,
        author_keypair: &FacilityKeypair,
        recipients: &[(&FacilityId, &FacilityPublicKeys)],
    ) -> Result<Envelope, EnvelopeError> {
        if self.parent_version_ids.len() > 2 {
            return Err(EnvelopeError::BadParentCount(self.parent_version_ids.len()));
        }

        // 1. Compute payload hash for integrity verification post-decrypt.
        let mut hasher = Sha256::new();
        hasher.update(&self.plaintext);
        let payload_hash: Vec<u8> = hasher.finalize().to_vec();

        // 2. Generate a random AES-GCM key + nonce for this envelope.
        let mut aes_key_bytes = [0u8; AES_KEY_LEN];
        let mut nonce_bytes = [0u8; AES_NONCE_LEN];
        rand::fill(&mut aes_key_bytes);
        rand::fill(&mut nonce_bytes);
        let aes_key = Key::<Aes256Gcm>::from_slice(&aes_key_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let cipher = Aes256Gcm::new(aes_key);

        let payload_ciphertext = cipher
            .encrypt(nonce, self.plaintext.as_ref())
            .map_err(|e| EnvelopeError::EncryptionFailed(e.to_string()))?;

        // 3. For each recipient (including the author itself), seal-box the AES key.
        let mut recipient_keys = Vec::with_capacity(recipients.len());
        for (reader_facility_id, reader_public_keys) in recipients {
            let sealed = seal_aes_key_to_recipient(&aes_key_bytes, &reader_public_keys.encryption)?;
            recipient_keys.push(RecipientKeyBox {
                reader_facility_id: reader_facility_id.as_str().to_string(),
                eph_pubkey: sealed.eph_pubkey,
                sealed_key: sealed.sealed_key,
            });
        }

        // 4. Build the unsigned envelope.
        let unsigned = UnsignedEnvelope {
            magic: MAGIC.to_string(),
            protocol_major: PROTOCOL_MAJOR,
            protocol_minor: PROTOCOL_MINOR,
            version_id: self.version_id.0,
            resource_id: self.resource_id.0,
            resource_type: self.resource_type,
            vector_clock: self.vector_clock,
            parent_version_ids: self.parent_version_ids.iter().map(|v| v.0).collect(),
            author_facility_id: self.author_facility_id.as_str().to_string(),
            author_clinician_id: self.author_clinician_id.0,
            created_at_unix_ms: self.created_at_unix_ms,
            kind: self.kind.as_str().to_string(),
            payload_ciphertext,
            payload_hash,
            payload_nonce: nonce_bytes.to_vec(),
            recipient_keys,
        };

        // 5. CBOR-encode the unsigned envelope (canonical form) and sign it.
        let unsigned_bytes = canonical_cbor_encode(&unsigned)
            .map_err(|e| EnvelopeError::CborEncode(e.to_string()))?;
        let sig = author_keypair.signing.sign(&unsigned_bytes);
        let signature = sig.to_bytes().to_vec();

        Ok(Envelope {
            magic: unsigned.magic,
            protocol_major: unsigned.protocol_major,
            protocol_minor: unsigned.protocol_minor,
            version_id: unsigned.version_id,
            resource_id: unsigned.resource_id,
            resource_type: unsigned.resource_type,
            vector_clock: unsigned.vector_clock,
            parent_version_ids: unsigned.parent_version_ids,
            author_facility_id: unsigned.author_facility_id,
            author_clinician_id: unsigned.author_clinician_id,
            created_at_unix_ms: unsigned.created_at_unix_ms,
            kind: unsigned.kind,
            payload_ciphertext: unsigned.payload_ciphertext,
            payload_hash: unsigned.payload_hash,
            payload_nonce: unsigned.payload_nonce,
            recipient_keys: unsigned.recipient_keys,
            signature,
        })
    }
}

/// Internal helper: the envelope minus the signature, used for signing.
#[derive(Debug, Serialize, Deserialize)]
struct UnsignedEnvelope {
    magic: String,
    protocol_major: u8,
    protocol_minor: u8,
    version_id: Uuid,
    resource_id: Uuid,
    resource_type: String,
    vector_clock: VectorClock,
    parent_version_ids: Vec<Uuid>,
    author_facility_id: String,
    author_clinician_id: Uuid,
    created_at_unix_ms: u64,
    kind: String,
    payload_ciphertext: Vec<u8>,
    payload_hash: Vec<u8>,
    payload_nonce: Vec<u8>,
    recipient_keys: Vec<RecipientKeyBox>,
}

/// Internal helper: the result of sealing an AES key to a recipient.
struct SealedKey {
    eph_pubkey: Vec<u8>,
    sealed_key: Vec<u8>,
}

/// Encrypt a 32-byte AES key to a recipient's X25519 public key.
///
/// Construction:
///   1. Generate an ephemeral X25519 keypair.
///   2. ECDH(eph_secret, recipient_pub) -> shared_secret (32 bytes).
///   3. HKDF-SHA256(shared_secret, info=b"psep-sealed-key") -> 44 bytes
///      (32 bytes AES key + 12 bytes AES nonce).
///   4. AES-GCM-256 encrypt the per-event AES key under the derived key+nonce.
///   5. Output = eph_pubkey (32) || derived_nonce (12) || ciphertext (48).
fn seal_aes_key_to_recipient(
    aes_key: &[u8; AES_KEY_LEN],
    recipient_x25519_pub: &x25519_dalek::PublicKey,
) -> Result<SealedKey, EnvelopeError> {
    use x25519_dalek::{PublicKey, StaticSecret};

    // 1. Ephemeral keypair.
    let mut rng = rand::rng();
    let eph_secret = StaticSecret::random_from_rng(&mut rng);
    let eph_pub = PublicKey::from(&eph_secret);

    // 2. ECDH.
    let shared = eph_secret.diffie_hellman(recipient_x25519_pub);

    // 3. HKDF-SHA256 → 44 bytes (32 key + 12 nonce).
    let derived = hkdf_sha256(shared.as_bytes(), b"psep-sealed-key-v1", 44);
    let derived_key = &derived[..32];
    let derived_nonce = &derived[32..44];

    // 4. AES-GCM encrypt the AES key.
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(derived_key));
    let nonce = Nonce::from_slice(derived_nonce);
    let ciphertext = cipher
        .encrypt(nonce, aes_key.as_ref())
        .map_err(|e| EnvelopeError::EncryptionFailed(e.to_string()))?;

    // 5. Output: eph_pubkey || derived_nonce || ciphertext.
    let mut sealed = Vec::with_capacity(X25519_PUBKEY_LEN + AES_NONCE_LEN + ciphertext.len());
    sealed.extend_from_slice(&eph_pub.to_bytes());
    sealed.extend_from_slice(derived_nonce);
    sealed.extend_from_slice(&ciphertext);

    Ok(SealedKey {
        eph_pubkey: eph_pub.to_bytes().to_vec(),
        sealed_key: sealed,
    })
}

/// Open a sealed-box-encrypted AES key using the recipient's secret key.
fn open_sealed_aes_key(
    sealed: &RecipientKeyBox,
    recipient_secret: &x25519_dalek::StaticSecret,
) -> Result<[u8; AES_KEY_LEN], EnvelopeError> {
    if sealed.sealed_key.len() < X25519_PUBKEY_LEN + AES_NONCE_LEN {
        return Err(EnvelopeError::DecryptionFailed(
            "sealed key too short".to_string(),
        ));
    }
    let eph_pub_bytes: [u8; X25519_PUBKEY_LEN] = sealed.sealed_key[..X25519_PUBKEY_LEN]
        .try_into()
        .map_err(|_| EnvelopeError::DecryptionFailed("bad eph pubkey".to_string()))?;
    let eph_pub = x25519_dalek::PublicKey::from(eph_pub_bytes);
    let derived_nonce: &[u8] = &sealed.sealed_key[X25519_PUBKEY_LEN..X25519_PUBKEY_LEN + AES_NONCE_LEN];
    let ciphertext = &sealed.sealed_key[X25519_PUBKEY_LEN + AES_NONCE_LEN..];

    // ECDH(recipient_secret, eph_pub).
    let shared = recipient_secret.diffie_hellman(&eph_pub);

    // Same HKDF as the author used.
    let derived = hkdf_sha256(shared.as_bytes(), b"psep-sealed-key-v1", 44);
    let derived_key = &derived[..32];

    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(derived_key));
    let nonce = Nonce::from_slice(derived_nonce);
    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| EnvelopeError::DecryptionFailed(e.to_string()))?;

    if plaintext.len() != AES_KEY_LEN {
        return Err(EnvelopeError::DecryptionFailed(format!(
            "decrypted AES key has wrong length: {}",
            plaintext.len()
        )));
    }
    let mut key = [0u8; AES_KEY_LEN];
    key.copy_from_slice(&plaintext);
    Ok(key)
}

/// Minimal HKDF-SHA256 (RFC 5869) — extract+expand to `out_len` bytes.
fn hkdf_sha256(ikm: &[u8], info: &[u8], out_len: usize) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    // Extract.
    let mut hmac = <HmacSha256 as Mac>::new_from_slice(b"").expect("hmac key");
    hmac.update(ikm);
    let prk = hmac.finalize().into_bytes();

    // Expand.
    let mut out = Vec::with_capacity(out_len);
    let mut t: Vec<u8> = Vec::new();
    let mut counter: u8 = 1;
    while out.len() < out_len {
        let mut hmac = <HmacSha256 as Mac>::new_from_slice(&prk).expect("hmac key");
        hmac.update(&t);
        hmac.update(info);
        hmac.update(&[counter]);
        t = hmac.finalize().into_bytes().to_vec();
        let take = (out_len - out.len()).min(t.len());
        out.extend_from_slice(&t[..take]);
        counter = counter.wrapping_add(1);
        if counter == 0 {
            break; // safety: cap at 255 iterations
        }
    }
    out.truncate(out_len);
    out
}

impl Envelope {
    /// Verify the envelope signature against the author's verifying key.
    pub fn verify_signature(
        &self,
        author_verifying: &ed25519_dalek::VerifyingKey,
    ) -> Result<(), EnvelopeError> {
        if self.magic != MAGIC {
            return Err(EnvelopeError::BadMagic {
                expected: MAGIC.to_string(),
                got: self.magic.clone(),
            });
        }
        if self.protocol_major != PROTOCOL_MAJOR {
            return Err(EnvelopeError::BadVersion {
                expected: PROTOCOL_MAJOR,
                got: self.protocol_major,
            });
        }
        if self.parent_version_ids.len() > 2 {
            return Err(EnvelopeError::BadParentCount(self.parent_version_ids.len()));
        }
        if self.signature.len() != SIG_LEN {
            return Err(EnvelopeError::BadSignature);
        }

        let unsigned = UnsignedEnvelope {
            magic: self.magic.clone(),
            protocol_major: self.protocol_major,
            protocol_minor: self.protocol_minor,
            version_id: self.version_id,
            resource_id: self.resource_id,
            resource_type: self.resource_type.clone(),
            vector_clock: self.vector_clock.clone(),
            parent_version_ids: self.parent_version_ids.clone(),
            author_facility_id: self.author_facility_id.clone(),
            author_clinician_id: self.author_clinician_id,
            created_at_unix_ms: self.created_at_unix_ms,
            kind: self.kind.clone(),
            payload_ciphertext: self.payload_ciphertext.clone(),
            payload_hash: self.payload_hash.clone(),
            payload_nonce: self.payload_nonce.clone(),
            recipient_keys: self.recipient_keys.clone(),
        };
        let unsigned_bytes = canonical_cbor_encode(&unsigned)
            .map_err(|e| EnvelopeError::CborEncode(e.to_string()))?;

        let sig_bytes: [u8; SIG_LEN] = self.signature.as_slice().try_into().map_err(|_| EnvelopeError::BadSignature)?;
        let sig = ed25519_dalek::Signature::from_bytes(&sig_bytes);
        use ed25519_dalek::Verifier;
        author_verifying
            .verify(&unsigned_bytes, &sig)
            .map_err(|_| EnvelopeError::BadSignature)
    }

    /// Decrypt the payload as a specific recipient.
    pub fn open(
        &self,
        reader_facility_id: &FacilityId,
        reader_keypair: &FacilityKeypair,
    ) -> Result<Vec<u8>, EnvelopeError> {
        let sealed = self
            .recipient_keys
            .iter()
            .find(|r| r.reader_facility_id == reader_facility_id.as_str())
            .ok_or_else(|| EnvelopeError::RecipientNotFound(reader_facility_id.to_string()))?;

        let aes_key_bytes = open_sealed_aes_key(sealed, &reader_keypair.encryption_secret)?;
        let aes_key = Key::<Aes256Gcm>::from_slice(&aes_key_bytes);
        let nonce_bytes: [u8; AES_NONCE_LEN] = self.payload_nonce.as_slice().try_into().map_err(|_| {
            EnvelopeError::DecryptionFailed("bad payload nonce length".to_string())
        })?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let cipher = Aes256Gcm::new(aes_key);

        let plaintext = cipher
            .decrypt(nonce, self.payload_ciphertext.as_ref())
            .map_err(|e| EnvelopeError::DecryptionFailed(e.to_string()))?;

        // Verify payload hash to detect tampering or corruption.
        let mut hasher = Sha256::new();
        hasher.update(&plaintext);
        let computed: Vec<u8> = hasher.finalize().to_vec();
        if computed != self.payload_hash {
            return Err(EnvelopeError::PayloadHashMismatch);
        }

        Ok(plaintext)
    }

    /// Serialize this envelope to canonical CBOR bytes for the wire.
    pub fn to_cbor(&self) -> Result<Vec<u8>, EnvelopeError> {
        canonical_cbor_encode(self).map_err(|e| EnvelopeError::CborEncode(e.to_string()))
    }

    /// Deserialize an envelope from canonical CBOR bytes.
    pub fn from_cbor(bytes: &[u8]) -> Result<Self, EnvelopeError> {
        ciborium::de::from_reader(bytes).map_err(|e| EnvelopeError::CborDecode(e.to_string()))
    }

    pub fn version(&self) -> EnvelopeVersion {
        EnvelopeVersion {
            major: self.protocol_major,
            minor: self.protocol_minor,
        }
    }
}

/// Marker type for the envelope's protocol version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnvelopeVersion {
    pub major: u8,
    pub minor: u8,
}

/// Encode a value to canonical CBOR bytes.
///
/// `ciborium` always emits canonical (deterministic) CBOR when serializing —
/// map keys are emitted in sorted order, integers in shortest form, etc.
fn canonical_cbor_encode<T: Serialize>(
    value: &T,
) -> Result<Vec<u8>, ciborium::ser::Error<std::io::Error>> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes)?;
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use patientspace_shared::{ClinicianId, FacilityId, ResourceId, ResourceKind, VersionId};

    fn build_envelope(author: &FacilityKeypair, author_id: &FacilityId) -> Envelope {
        let plaintext = br#"{"resourceType":"Patient","id":"U-test"}"#.to_vec();
        let author_pub = author.public_keys();

        let builder = EnvelopeBuilder {
            version_id: VersionId::new(),
            resource_id: ResourceId::new(),
            resource_type: "Patient".to_string(),
            vector_clock: VectorClock::from_facility(author_id, 1),
            parent_version_ids: vec![],
            author_facility_id: author_id.clone(),
            author_clinician_id: ClinicianId::new(),
            created_at_unix_ms: 1_729_000_000_000,
            kind: ResourceKind::Clinical,
            plaintext,
        };

        let recipients: Vec<(&FacilityId, &FacilityPublicKeys)> = vec![(author_id, &author_pub)];
        builder.seal(author, &recipients).expect("seal")
    }

    #[test]
    fn envelope_roundtrip_signs_and_decrypts() {
        let author = FacilityKeypair::generate();
        let author_id = FacilityId::new("F-PETAUKE1");
        let author_pub = author.public_keys();

        let env = build_envelope(&author, &author_id);

        env.verify_signature(&author_pub.verifying)
            .expect("signature should verify");

        let plaintext = env.open(&author_id, &author).expect("open");
        assert!(plaintext.starts_with(b"{\"resourceType\":\"Patient\""));

        let wire = env.to_cbor().expect("encode");
        let back = Envelope::from_cbor(&wire).expect("decode");
        assert_eq!(env.version_id, back.version_id);
        assert_eq!(env.signature, back.signature);
        assert_eq!(env.payload_hash, back.payload_hash);
    }

    #[test]
    fn envelope_rejects_tampered_signature() {
        let author = FacilityKeypair::generate();
        let author_id = FacilityId::new("F-PETAUKE1");
        let author_pub = author.public_keys();

        let mut env = build_envelope(&author, &author_id);
        env.signature[0] ^= 0x01;

        let err = env.verify_signature(&author_pub.verifying).unwrap_err();
        assert!(matches!(err, EnvelopeError::BadSignature));
    }

    #[test]
    fn envelope_rejects_wrong_recipient() {
        let author = FacilityKeypair::generate();
        let author_id = FacilityId::new("F-PETAUKE1");
        let env = build_envelope(&author, &author_id);

        let stranger = FacilityKeypair::generate();
        let stranger_id = FacilityId::new("F-LUSAKA2");

        let err = env.open(&stranger_id, &stranger).unwrap_err();
        assert!(matches!(err, EnvelopeError::RecipientNotFound(_)));
    }

    #[test]
    fn envelope_supports_multiple_recipients() {
        let author = FacilityKeypair::generate();
        let author_id = FacilityId::new("F-PETAUKE1");
        let reader = FacilityKeypair::generate();
        let reader_id = FacilityId::new("F-LUSAKA2");
        let author_pub = author.public_keys();
        let reader_pub = reader.public_keys();

        let plaintext = br#"{"resourceType":"Patient","id":"U-multi"}"#.to_vec();
        let builder = EnvelopeBuilder {
            version_id: VersionId::new(),
            resource_id: ResourceId::new(),
            resource_type: "Patient".to_string(),
            vector_clock: VectorClock::from_facility(&author_id, 1),
            parent_version_ids: vec![],
            author_facility_id: author_id.clone(),
            author_clinician_id: ClinicianId::new(),
            created_at_unix_ms: 1_729_000_000_000,
            kind: ResourceKind::Clinical,
            plaintext,
        };

        let recipients: Vec<(&FacilityId, &FacilityPublicKeys)> = vec![
            (&author_id, &author_pub),
            (&reader_id, &reader_pub),
        ];
        let env = builder.seal(&author, &recipients).expect("seal");

        env.open(&author_id, &author).expect("author opens");
        env.open(&reader_id, &reader).expect("reader opens");

        env.verify_signature(&author_pub.verifying)
            .expect("signature verifies");
    }

    #[test]
    fn envelope_rejects_tampered_ciphertext() {
        let author = FacilityKeypair::generate();
        let author_id = FacilityId::new("F-PETAUKE1");
        let mut env = build_envelope(&author, &author_id);

        env.payload_ciphertext[0] ^= 0x01;
        let err = env.verify_signature(&author.public_keys().verifying).unwrap_err();
        assert!(matches!(err, EnvelopeError::BadSignature));
    }
}
