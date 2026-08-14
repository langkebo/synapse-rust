-- E2EE-02: Add secret_key column to verification_sas table.
-- The Curve25519 private key is generated during accept_sas and stored
-- (base64-encoded) so that generate_sas can compute the real ECDH shared
-- secret with the peer's public key.  Previously the private key was
-- discarded, causing SAS codes to be derived from random bytes instead of
-- the shared secret — a critical security vulnerability.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = current_schema() AND table_name = 'verification_sas' AND column_name = 'secret_key'
    ) THEN
        ALTER TABLE verification_sas ADD COLUMN secret_key TEXT;
    END IF;
END $$;
