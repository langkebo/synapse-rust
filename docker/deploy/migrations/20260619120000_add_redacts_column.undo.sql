-- Rollback for 20260619120000_add_redacts_column.sql
-- Drops the redacts column and its partial index from the events table.

DROP INDEX IF EXISTS idx_events_redacts;
ALTER TABLE events DROP COLUMN IF EXISTS redacts;
