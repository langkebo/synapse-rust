-- Fix worker_events.stream_id: previously BIGINT NOT NULL without default,
-- which broke add_event() since the storage layer relies on the DB to assign
-- monotonically increasing stream positions. Introduce a dedicated sequence
-- and bind it as the column default so INSERTs without an explicit stream_id
-- get a unique ascending value.
-- This mirrors the v10 baseline schema fix in 00000000_unified_schema_v10.sql.

CREATE SEQUENCE IF NOT EXISTS worker_events_stream_id_seq;

-- Always set the default (idempotent). SET DEFAULT does not error if the
-- default is already set to the same value.
ALTER TABLE worker_events
    ALTER COLUMN stream_id SET DEFAULT nextval('worker_events_stream_id_seq');

-- Backfill any NULL stream_id values (should be none in practice, but guard
-- the NOT NULL constraint when the default is applied on existing rows).
UPDATE worker_events
SET stream_id = nextval('worker_events_stream_id_seq')
WHERE stream_id IS NULL;
