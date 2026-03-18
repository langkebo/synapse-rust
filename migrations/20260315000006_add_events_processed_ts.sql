-- Add processed_ts column to events table if not exists
-- This column is used for tracking when events were processed

-- Add processed_ts column
ALTER TABLE events ADD COLUMN IF NOT EXISTS processed_ts BIGINT;

-- Copy data from processed_at to processed_ts if needed
UPDATE events SET processed_ts = processed_at 
WHERE processed_ts IS NULL AND processed_at IS NOT NULL;

-- Create index for processed_ts
CREATE INDEX IF NOT EXISTS idx_events_processed_ts ON events(processed_ts) WHERE processed_ts IS NOT NULL;
