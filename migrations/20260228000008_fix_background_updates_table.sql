-- Migration: Fix background_updates table for API compatibility
-- Date: 2026-02-28
-- Description: Add missing columns to background_updates table

-- Add missing columns to background_updates table
ALTER TABLE background_updates 
ADD COLUMN IF NOT EXISTS job_type VARCHAR(50) DEFAULT 'custom',
ADD COLUMN IF NOT EXISTS description TEXT,
ADD COLUMN IF NOT EXISTS table_name VARCHAR(255),
ADD COLUMN IF NOT EXISTS column_name VARCHAR(255),
ADD COLUMN IF NOT EXISTS status VARCHAR(50) DEFAULT 'pending',
ADD COLUMN IF NOT EXISTS progress INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS total_items INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS processed_items INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS created_ts BIGINT NOT NULL DEFAULT (EXTRACT(epoch FROM now()) * 1000)::BIGINT,
ADD COLUMN IF NOT EXISTS started_ts BIGINT,
ADD COLUMN IF NOT EXISTS completed_ts BIGINT,
ADD COLUMN IF NOT EXISTS last_updated_ts BIGINT,
ADD COLUMN IF NOT EXISTS error_message TEXT,
ADD COLUMN IF NOT EXISTS retry_count INTEGER DEFAULT 0,
ADD COLUMN IF NOT EXISTS max_retries INTEGER DEFAULT 3,
ADD COLUMN IF NOT EXISTS batch_size INTEGER DEFAULT 100,
ADD COLUMN IF NOT EXISTS sleep_ms INTEGER DEFAULT 100,
ADD COLUMN IF NOT EXISTS metadata JSONB;

-- Add index on status for faster queries
CREATE INDEX IF NOT EXISTS idx_background_updates_status ON background_updates(status);

-- Add index on created_ts for ordering
CREATE INDEX IF NOT EXISTS idx_background_updates_created_ts ON background_updates(created_ts);
