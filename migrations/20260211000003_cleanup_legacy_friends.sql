-- Phase 4: Final Cleanup of Legacy Friends System
-- Execution time: 2026-02-11
-- Description: Remove legacy friends tables after successful migration to room system
--
-- IMPORTANT: Only run this migration AFTER validation passes!
-- Run 20260211000002_validate_friend_migration.sql first.

-- ==============================================================================
-- PRE-CHECKS
-- ==============================================================================

-- Ensure validation was run
DO $$
BEGIN
    -- Check if there are any users without friend list rooms
    IF EXISTS (
        SELECT 1 FROM friends f
        WHERE NOT EXISTS (
            SELECT 1 FROM events e
            WHERE e.event_type = 'm.friends.list'
              AND e.state_key = ''
              AND e.room_id = '!friends:' || substring(f.user_id from 2)
        )
        LIMIT 1
    ) THEN
        RAISE EXCEPTION 'Migration validation failed! Some users do not have friend list rooms. Run 20260211000002_validate_friend_migration.sql first.';
    END IF;

    RAISE NOTICE '✓ Pre-checks passed. Proceeding with cleanup...';
END $$;

-- ==============================================================================
-- CREATE BACKUP TABLES (for safety)
-- ==============================================================================

-- Create backup of friends table before dropping
CREATE TABLE IF NOT EXISTS friends_backup_20260211 AS
SELECT * FROM friends;

-- Create backup of friend_requests table before dropping
CREATE TABLE IF NOT EXISTS friend_requests_backup_20260211 AS
SELECT * FROM friend_requests;

-- Create backup of friend_categories table before dropping
CREATE TABLE IF NOT EXISTS friend_categories_backup_20260211 AS
SELECT * FROM friend_categories;

-- Create backup of blocked_users table before dropping
CREATE TABLE IF NOT EXISTS blocked_users_backup_20260211 AS
SELECT * FROM blocked_users;

RAISE NOTICE '✓ Backup tables created';

-- ==============================================================================
-- DROP LEGACY TABLES
-- ==============================================================================

-- Drop foreign key constraints first
DO $$
BEGIN
    -- Drop the friend_requests table
    DROP TABLE IF EXISTS friend_requests CASCADE;

    -- Drop the friend_categories table
    DROP TABLE IF EXISTS friend_categories CASCADE;

    -- Drop the blocked_users table
    DROP TABLE IF EXISTS blocked_users CASCADE;

    -- Drop the friends table
    DROP TABLE IF EXISTS friends CASCADE;

    RAISE NOTICE '✓ Legacy friends tables dropped';
END $$;

-- ==============================================================================
-- MARKER FOR COMPLETED MIGRATION
-- ==============================================================================

-- Insert a marker record to track migration completion
INSERT INTO migrations (name, executed_at)
VALUES ('friends_to_rooms_migration_completed', NOW())
ON CONFLICT (name) DO UPDATE SET executed_at = NOW();

RAISE NOTICE '✓ Migration marker inserted';

-- ==============================================================================
-- POST-CLEANUP VALIDATION
-- ==============================================================================

DO $$
DECLARE
    v_friend_list_rooms BIGINT;
    v_dm_rooms BIGINT;
BEGIN
    RAISE NOTICE '';
    RAISE NOTICE '=== Post-Cleanup Validation ===';

    -- Verify friend list rooms still exist
    SELECT COUNT(*) INTO v_friend_list_rooms
    FROM events
    WHERE event_type = 'm.friends.list'
      AND state_key = '';

    RAISE NOTICE 'Friend list rooms preserved: %', v_friend_list_rooms;

    -- Verify DM rooms still exist
    SELECT COUNT(*) INTO v_dm_rooms
    FROM events
    WHERE event_type = 'm.friends.related_users'
      AND state_key = '';

    RAISE NOTICE 'DM rooms preserved: %', v_dm_rooms;

    RAISE NOTICE '';
    RAISE NOTICE '=== Cleanup Complete ===';
    RAISE NOTICE 'Legacy friends system has been removed.';
    RAISE NOTICE 'All friend relationships now use Matrix room mechanism.';
    RAISE NOTICE 'Backup tables are available for recovery if needed.';
END $$;

-- ==============================================================================
-- UPDATE DOCUMENTATION
-- ==============================================================================

COMMENT ON TABLE friends_backup_20260211 IS 'Backup of friends table before migration to room system. Safe to drop after verification period.';
COMMENT ON TABLE friend_requests_backup_20260211 IS 'Backup of friend_requests table before migration to room system. Safe to drop after verification period.';
COMMENT ON TABLE friend_categories_backup_20260211 IS 'Backup of friend_categories table before migration to room system. Safe to drop after verification period.';
COMMENT ON TABLE blocked_users_backup_20260211 IS 'Backup of blocked_users table before migration to room system. Safe to drop after verification period.';

-- ==============================================================================
-- NOTES
-- ==============================================================================

/*
MIGRATION COMPLETE

The friend system has been fully migrated to the Matrix room mechanism.

Old system (REMOVED):
- friends table
- friend_requests table
- friend_categories table
- blocked_users table

New system (ACTIVE):
- Friend list rooms: !friends:@user:server.com
- DM rooms: m.friends.related_users events
- Friend state events: m.friends.list

API endpoints:
- OLD: /_synapse/enhanced/friends/* (removed)
- NEW: /_matrix/client/v1/friends/* (active)
- COMPAT: /_matrix/client/unstable/friends/* (compatibility layer, can be removed after client migration)

Backup tables available:
- friends_backup_20260211
- friend_requests_backup_20260211
- friend_categories_backup_20260211
- blocked_users_backup_20260211

These can be safely dropped 30 days after successful verification.
*/
