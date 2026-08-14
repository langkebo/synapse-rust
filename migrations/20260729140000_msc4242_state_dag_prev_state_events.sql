-- P2-14: MSC4242 State DAGs — add prev_state_events column to events table.
--
-- MSC4242 introduces a state DAG for room state events, where edges are
-- defined by `prev_state_events` instead of `prev_events`. This forms a
-- partial order on state events only, distinct from the room DAG.
--
-- Key differences from the room DAG:
-- - `prev_events`: links ALL events (state + message) into the room DAG
-- - `prev_state_events`: links ONLY state events into the state DAG
--
-- The state DAG enables:
-- - Calculated `auth_events` (server-computed, not sender-specified)
-- - Faster state convergence across federation
-- - Mandated `/get_missing_events` backfill for unknown prev_state_events
--
-- This migration adds the `prev_state_events` JSONB column to store the
-- state DAG edges. It is nullable: existing events and non-state events
-- have NULL prev_state_events; only MSC4242 room versions populate it.
--
-- Safety: idempotent — uses DO $$ ... ADD COLUMN IF NOT EXISTS.
-- Backward compatible: NULL by default, no impact on existing queries.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = current_schema() AND table_name = 'events' AND column_name = 'prev_state_events'
    ) THEN
        ALTER TABLE events ADD COLUMN prev_state_events JSONB;
    END IF;
END $$;

-- Index to support state DAG traversal queries (e.g. "find all events
-- whose prev_state_events contains X"). GIN index is optimal for JSONB
-- array containment checks: `prev_state_events @> '["$event_id"]'`
CREATE INDEX IF NOT EXISTS idx_events_prev_state_events
ON events USING GIN (prev_state_events)
WHERE prev_state_events IS NOT NULL;

-- Index for fetching prev_state_events by event_id (the common read path).
CREATE INDEX IF NOT EXISTS idx_events_state_dag_room
ON events(room_id, event_id)
WHERE prev_state_events IS NOT NULL;
