-- Migration: Fix token tables field names for consistency
-- Date: 2026-03-13
-- Description: Rename fields to comply with naming standards
--   - expires_ts → expires_at (nullable timestamp)
--   - last_used_at → last_used_ts (activity timestamp, nullable)
--   - revoked_ts → revoked_at (nullable timestamp)

-- Rename columns in access_tokens table
ALTER TABLE access_tokens RENAME COLUMN expires_ts TO expires_at;
ALTER TABLE access_tokens RENAME COLUMN last_used_at TO last_used_ts;
ALTER TABLE access_tokens RENAME COLUMN revoked_ts TO revoked_at;

-- Rename columns in refresh_tokens table
ALTER TABLE refresh_tokens RENAME COLUMN expires_ts TO expires_at;
ALTER TABLE refresh_tokens RENAME COLUMN last_used_at TO last_used_ts;
ALTER TABLE refresh_tokens RENAME COLUMN revoked_ts TO revoked_at;

-- Update comments
COMMENT ON COLUMN access_tokens.expires_at IS 'Token expiration timestamp (nullable)';
COMMENT ON COLUMN access_tokens.last_used_ts IS 'Last usage timestamp (nullable)';
COMMENT ON COLUMN access_tokens.revoked_at IS 'Revocation timestamp (nullable)';
COMMENT ON COLUMN refresh_tokens.expires_at IS 'Token expiration timestamp (nullable)';
COMMENT ON COLUMN refresh_tokens.last_used_ts IS 'Last usage timestamp (nullable)';
COMMENT ON COLUMN refresh_tokens.revoked_at IS 'Revocation timestamp (nullable)';
