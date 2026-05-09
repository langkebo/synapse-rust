-- ============================================================================
-- Consolidated Migration: Stream Ordering Online Fix v7
-- Version: 20260515000002
-- Created: 2026-05-15
--
-- Purpose:
--   - Finish any remaining events.stream_ordering backfill in bounded batches
--   - Re-sync the sequence after online repair
--   - Add the covering index used by sync/event pagination hot paths
-- ============================================================================
-- no-transaction

SET TIME ZONE 'UTC';

CREATE INDEX IF NOT EXISTS idx_events_sync_covering
    ON events(room_id, stream_ordering DESC)
    INCLUDE (event_id, sender, event_type, content, origin_server_ts);

DO $$
DECLARE
    batch_size INTEGER := 10000;
    rows_updated INTEGER := 0;
BEGIN
    LOOP
        WITH batch AS (
            SELECT event_id
            FROM events
            WHERE stream_ordering IS NULL
            ORDER BY origin_server_ts ASC, event_id ASC
            LIMIT batch_size
        )
        UPDATE events AS e
        SET stream_ordering = nextval('events_stream_ordering_seq'::regclass)
        FROM batch
        WHERE e.event_id = batch.event_id;

        GET DIAGNOSTICS rows_updated = ROW_COUNT;
        EXIT WHEN rows_updated = 0;

        PERFORM pg_sleep(0.05);
    END LOOP;

    PERFORM setval(
        'events_stream_ordering_seq',
        COALESCE((SELECT MAX(stream_ordering) FROM events), 0) + 1,
        false
    );
END $$;
