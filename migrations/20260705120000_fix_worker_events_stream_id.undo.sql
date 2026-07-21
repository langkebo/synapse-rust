-- Rollback for 20260705120000_fix_worker_events_stream_id.sql
-- Removes the default from worker_events.stream_id and drops the sequence.
-- Note: backfilled stream_id values cannot be reverted to NULL.

ALTER TABLE worker_events ALTER COLUMN stream_id DROP DEFAULT;
DROP SEQUENCE IF EXISTS worker_events_stream_id_seq;
