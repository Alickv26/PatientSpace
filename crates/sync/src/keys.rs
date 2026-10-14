//! Facility keypairs: Ed25519 (signing) + X25519 (encryption).
//!
//! Each facility generates a keypair at first boot. The signing key signs every
//! PSEP envelope; the encryption key is used for sealed-box encryption of
//! envelope payloads (see `docs/sync-protocol.md` §5).

use ed25519_dalek::{SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret as X25519Secret};

/// A facility's complete keypair — signing + encryption.
///
/// The signing keypair is Ed25519; the encryption keypair is X25519.
/// Both are generated from independent random sources at first boot.
///
/// Store this securely: OS keychain, TPM, or an encrypted keystore on disk.
/// If lost, the facility cannot decrypt its own historical events (see §5.6).
pub struct FacilityKeypair {
    pub signing: SigningKey,
    pub encryption_secret: X25519Secret,
}

impl FacilityKeypair {
    /// Generate a fresh keypair using the OS CSPRNG.
    pub fn generate() -> Self {
        let mut rng = rand::rng();
        let signing = SigningKey::generate(&mut rng);
        let encryption_secret = X25519Secret::random_from_rng(&mut rng);
        Self {
            signing,
            encryption_secret,
        }
    }

    /// The public counterpart of this keypair — safe to share with the hub
    /// and other facilities.
    pub fn public_keys(&self) -> FacilityPublicKeys {
        FacilityPublicKeys {
            verifying: self.signing.verifying_key(),
            encryption: X25519PublicKey::from(&self.encryption_secret),
        }
    }

    /// Convenience: the X25519 public key (32 bytes).
    pub fn encryption_public(&self) -> X25519PublicKey {
        X25519PublicKey::from(&self.encryption_secret)
    }
}

/// A facility's public keys — the shareable counterpart to `FacilityKeypair`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FacilityPublicKeys {
    /// Ed25519 verifying key (32 bytes) — used to verify envelope signatures.
    pub verifying: VerifyingKey,

    /// X25519 public key (32 bytes) — used by other facilities to encrypt
    /// sealed-box payloads to us.
    pub encryption: X25519PublicKey,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_keypair_is_unique() {
        let a = FacilityKeypair::generate();
        let b = FacilityKeypair::generate();
        assert_ne!(a.public_keys().verifying, b.public_keys().verifying);
        assert_ne!(a.public_keys().encryption, b.public_keys().encryption);
    }

    #[test]
    fn public_keys_are_consistent_with_secret() {
        let kp = FacilityKeypair::generate();
        let pub_keys = kp.public_keys();
        let rederived = X25519PublicKey::from(&kp.encryption_secret);
        assert_eq!(pub_keys.encryption, rederived);
    }
}
