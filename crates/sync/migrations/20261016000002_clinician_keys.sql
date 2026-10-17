-- Add delegated signing key to clinicians table.
-- Each clinician gets an Ed25519 keypair for co-signing PSEP envelopes.
ALTER TABLE clinicians ADD COLUMN signing_key_pub TEXT;
ALTER TABLE clinicians ADD COLUMN signing_key_priv TEXT;

-- Add clinician_id to sessions table so we can look up who created each
-- envelope without a separate DB round-trip on every write.
-- (Already exists via clinician_id FK, but we also want the signing key
-- cached in the session for performance.)
ALTER TABLE sessions ADD COLUMN signing_key_pub TEXT;
ALTER TABLE sessions ADD COLUMN signing_key_priv TEXT;
