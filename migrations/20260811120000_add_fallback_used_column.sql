-- ISSUE-02: Track fallback key usage for device_unused_fallback_key_types
--
-- Per Matrix spec, when a fallback key is claimed (OTK stock exhausted),
-- the server must:
--   1. NOT delete the fallback key (it can be reused by other sessions)
--   2. Mark it as "used" so it disappears from device_unused_fallback_key_types
--   3. The client sees the algorithm disappear and uploads a new fallback key
--
-- This migration adds the fallback_used column to track that state.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'device_keys' AND column_name = 'fallback_used'
    ) THEN
        ALTER TABLE device_keys ADD COLUMN fallback_used BOOLEAN NOT NULL DEFAULT FALSE;
    END IF;
END $$;

-- Update the fallback index to also filter on fallback_used for efficient
-- queries by get_unused_fallback_key_types.
DROP INDEX IF EXISTS idx_device_keys_fallback;
CREATE INDEX idx_device_keys_fallback
    ON device_keys(user_id, device_id)
    WHERE is_fallback = TRUE AND fallback_used = FALSE;
