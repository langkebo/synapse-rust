-- Fix Push module issues
-- Created: 2026-03-15

-- Add missing column to pushers table
ALTER TABLE pushers ADD COLUMN IF NOT EXISTS last_updated_ts BIGINT DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT;

-- Add is_typing column to typing table (if exists)
-- ALTER TABLE typing ADD COLUMN IF NOT EXISTS is_typing BOOLEAN DEFAULT true;

-- Fix events table - add type column if missing
ALTER TABLE events ADD COLUMN IF NOT EXISTS type VARCHAR(100) DEFAULT '';
