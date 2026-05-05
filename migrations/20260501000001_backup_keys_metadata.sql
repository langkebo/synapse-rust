-- Promote per-session KeyBackupData metadata to real columns so we can
-- index/query them and stop wrapping them inside the session_data jsonb
-- payload. See docs/synapse-rust/SPEC_ALIGNMENT_PLAN_2026-05-01.md §4.

ALTER TABLE backup_keys
    ADD COLUMN IF NOT EXISTS first_message_index BIGINT  NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS forwarded_count     BIGINT  NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS is_verified         BOOLEAN NOT NULL DEFAULT FALSE;

-- Backfill from any existing JSON payload that already carries these fields.
UPDATE backup_keys
SET    first_message_index = COALESCE((session_data ->> 'first_message_index')::BIGINT,  first_message_index),
       forwarded_count     = COALESCE((session_data ->> 'forwarded_count')::BIGINT,      forwarded_count),
       is_verified         = COALESCE((session_data ->> 'is_verified')::BOOLEAN,         is_verified)
WHERE  jsonb_typeof(session_data) = 'object';
