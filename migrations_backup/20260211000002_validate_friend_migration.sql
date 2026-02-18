-- Phase 4: Data Validation Before Friends Table Cleanup
-- Execution time: 2026-02-11
-- Description: Validates that all friend data has been migrated to room system

-- This script performs validation checks to ensure data is ready for cleanup.
-- Run this before executing the final cleanup migration.

-- ==============================================================================
-- Validation Functions
-- ==============================================================================

-- Count users with friend list rooms
DO $$
DECLARE
    v_users_with_rooms BIGINT;
    v_users_with_legacy_friends BIGINT;
    v_users_fully_synced BIGINT;
    v_unsynced_users TEXT[];
BEGIN
    RAISE NOTICE '=== Friend System Migration Validation ===';

    -- Count users with friend list rooms
    SELECT COUNT(DISTINCT user_id) INTO v_users_with_rooms
    FROM events
    WHERE event_type = 'm.friends.list'
      AND state_key = '';

    RAISE NOTICE 'Users with friend list rooms: %', v_users_with_rooms;

    -- Count users with legacy friends table data
    SELECT COUNT(DISTINCT user_id) INTO v_users_with_legacy_friends
    FROM friends;

    RAISE NOTICE 'Users with legacy friends data: %', v_users_with_legacy_friends;

    -- Check which users have both systems
    CREATE TEMPORARY TABLE IF NOT EXISTS temp_users_both AS
    SELECT DISTINCT f.user_id
    FROM friends f
    WHERE EXISTS (
        SELECT 1 FROM events e
        WHERE e.event_type = 'm.friends.list'
          AND e.state_key = ''
          AND e.room_id = '!friends:' || substring(f.user_id from 2)
    );

    SELECT COUNT(*) INTO v_users_fully_synced FROM temp_users_both;
    RAISE NOTICE 'Users with both systems (ready for cleanup): %', v_users_fully_synced;

    -- Find users with legacy data but no friend list room
    CREATE TEMPORARY TABLE IF NOT EXISTS temp_users_needing_migration AS
    SELECT DISTINCT f.user_id
    FROM friends f
    WHERE NOT EXISTS (
        SELECT 1 FROM events e
        WHERE e.event_type = 'm.friends.list'
          AND e.state_key = ''
          AND e.room_id = '!friends:' || substring(f.user_id from 2)
    );

    -- Store users needing migration
    SELECT ARRAY_AGG(user_id ORDER BY user_id) INTO v_unsynced_users
    FROM temp_users_needing_migration
    LIMIT 10;

    IF v_unsynced_users IS NOT NULL AND array_length(v_unsynced_users, 1) > 0 THEN
        RAISE NOTICE 'Users needing migration (first 10): %', v_unsynced_users;
    END IF;

    -- Validation result
    IF (SELECT COUNT(*) FROM temp_users_needing_migration) = 0 THEN
        RAISE NOTICE '✓ VALIDATION PASSED: All users have friend list rooms';
        RAISE NOTICE 'Safe to proceed with cleanup migration.';
    ELSE
        RAISE NOTICE '✗ VALIDATION FAILED: Some users need migration first';
        RAISE NOTICE 'Please run migration for users listed above.';
    END IF;

    DROP TABLE IF EXISTS temp_users_both;
    DROP TABLE IF EXISTS temp_users_needing_migration;
END $$;

-- ==============================================================================
-- Detailed Data Comparison
-- ==============================================================================

-- Compare friend counts between systems for each user
DO $$
DECLARE
    v_record RECORD;
    v_mismatch_count INTEGER := 0;
BEGIN
    RAISE NOTICE '';
    RAISE NOTICE '=== Friend Count Comparison ===';

    CREATE TEMPORARY TABLE IF NOT EXISTS temp_friend_counts AS
    SELECT
        f.user_id,
        COUNT(DISTINCT f.friend_id) AS legacy_count,
        COALESCE(
            (SELECT jsonb_array_length(e.content->'friends')
             FROM events e
             WHERE e.event_type = 'm.friends.list'
               AND e.state_key = ''
               AND e.room_id = '!friends:' || substring(f.user_id from 2)
             ORDER BY e.origin_server_ts DESC
             LIMIT 1),
            0
        ) AS room_count
    FROM friends f
    GROUP BY f.user_id;

    -- Find mismatches
    FOR v_record IN
        SELECT user_id, legacy_count, room_count
        FROM temp_friend_counts
        WHERE legacy_count != room_count
        LIMIT 20
    LOOP
        v_mismatch_count := v_mismatch_count + 1;
        RAISE NOTICE 'Mismatch: % - legacy: %, room: %',
            v_record.user_id, v_record.legacy_count, v_record.room_count;
    END LOOP;

    IF v_mismatch_count = 0 THEN
        RAISE NOTICE '✓ All friend counts match between systems';
    ELSE
        RAISE NOTICE '✗ Found % friend count mismatches', v_mismatch_count;
    END IF;

    DROP TABLE IF EXISTS temp_friend_counts;
END $$;

-- ==============================================================================
-- Friend Request Validation
-- ==============================================================================

-- Check if friend requests are handled properly
DO $$
DECLARE
    v_pending_requests BIGINT;
    v_accepted_with_dm BIGINT;
BEGIN
    RAISE NOTICE '';
    RAISE NOTICE '=== Friend Request Validation ===';

    -- Count pending requests
    SELECT COUNT(*) INTO v_pending_requests
    FROM friend_requests
    WHERE status = 'pending';

    RAISE NOTICE 'Pending friend requests: %', v_pending_requests;

    -- Check if accepted requests have corresponding DM rooms
    SELECT COUNT(*) INTO v_accepted_with_dm
    FROM friend_requests fr
    WHERE fr.status = 'accepted'
      AND EXISTS (
          SELECT 1 FROM events e
          JOIN events ev ON e.room_id = ev.room_id
          WHERE ev.event_type = 'm.friends.related_users'
            AND ev.state_key = ''
            AND ev.content->'related_users' @> jsonb_build_array(fr.from_user_id, fr.to_user_id)
      );

    RAISE NOTICE 'Accepted requests with DM rooms: %', v_accepted_with_dm;
END $$;

-- ==============================================================================
-- Summary Report
-- ==============================================================================

DO $$
DECLARE
    v_total_friends_legacy BIGINT;
    v_total_friends_rooms BIGINT;
    v_total_dm_rooms BIGINT;
BEGIN
    RAISE NOTICE '';
    RAISE NOTICE '=== Summary Report ===';

    SELECT COUNT(*) INTO v_total_friends_legacy FROM friends;

    SELECT COUNT(*) INTO v_total_friends_rooms
    FROM events e
    WHERE e.event_type = 'm.friends.list'
      AND e.state_key = '';

    SELECT COUNT(*) INTO v_total_dm_rooms
    FROM events
    WHERE event_type = 'm.friends.related_users'
      AND state_key = '';

    RAISE NOTICE 'Total friend relationships (legacy): %', v_total_friends_legacy;
    RAISE NOTICE 'Total friend list rooms: %', v_total_friends_rooms;
    RAISE NOTICE 'Total DM rooms marked: %', v_total_dm_rooms;

    RAISE NOTICE '';
    RAISE NOTICE '=== Next Steps ===';
    RAISE NOTICE '1. If validation passed, run the cleanup migration';
    RAISE NOTICE '2. If validation failed, investigate and fix issues';
    RAISE NOTICE '3. After cleanup, old friends table will be dropped';
END $$;
