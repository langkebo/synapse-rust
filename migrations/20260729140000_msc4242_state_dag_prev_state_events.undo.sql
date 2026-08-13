-- Rollback for 20260729140000_msc4242_state_dag_prev_state_events.sql
-- Drops the prev_state_events column and its two state-DAG indexes from the
-- events table.

DROP INDEX IF EXISTS idx_events_state_dag_room;
DROP INDEX IF EXISTS idx_events_prev_state_events;
ALTER TABLE events DROP COLUMN IF EXISTS prev_state_events;
