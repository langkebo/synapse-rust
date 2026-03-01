-- Migration: Fix federation_blacklist table for API compatibility
-- Date: 2026-02-28
-- Description: Add created_ts column to federation_blacklist table

-- Add created_ts column to federation_blacklist table
ALTER TABLE federation_blacklist 
ADD COLUMN IF NOT EXISTS created_ts BIGINT NOT NULL DEFAULT (EXTRACT(epoch FROM now()) * 1000)::BIGINT;

-- Update existing records to use blocked_at timestamp
UPDATE federation_blacklist 
SET created_ts = (EXTRACT(epoch FROM blocked_at) * 1000)::BIGINT 
WHERE created_ts IS NULL AND blocked_at IS NOT NULL;

-- Add index on created_ts for ordering
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_created_ts ON federation_blacklist(created_ts);
