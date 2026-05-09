-- ============================================================================
-- Rollback Script: 20260515000007_rooms_summaries_materialized_view_v7.undo.sql
-- Forward Script: 20260515000007_rooms_summaries_materialized_view_v7.sql
-- Created: 2026-05-09
-- Risk: LOW — Only drops materialized views, no data loss.
-- Rollback RTO: < 1 minute
-- ============================================================================

DROP MATERIALIZED VIEW IF EXISTS public_room_directory CASCADE;
DROP MATERIALIZED VIEW IF EXISTS rooms_summaries CASCADE;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'pg_cron'
    ) THEN
        PERFORM cron.unschedule('refresh-public-room-directory');
        PERFORM cron.unschedule('refresh-rooms-summaries');
    END IF;
END $$;