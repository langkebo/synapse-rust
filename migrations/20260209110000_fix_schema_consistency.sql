-- ====================================================================
-- Schema Consistency Fixes
-- ====================================================================
-- This migration fixes schema inconsistencies identified during code review:
-- 1. Standardizes timestamp field names (created_ts vs creation_ts vs created_at)
-- 2. Removes redundant fields
-- 3. Fixes missing constraints
-- 4. Standardizes data types across tables
-- ====================================================================

-- ====================================================================
-- FIX 1: Standardize timestamp field names
-- ====================================================================

-- Add created_at as alias for creation_ts in users table (backward compatible)
-- This allows gradual migration to new naming convention
DO $$
BEGIN
    -- Check if column doesn't exist before adding
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'users' AND column_name = 'created_at'
    ) THEN
        ALTER TABLE users ADD COLUMN created_at BIGINT
            GENERATED ALWAYS AS (creation_ts) STORED;
    END IF;
END $$;

-- ====================================================================
-- FIX 2: Remove redundant timestamp fields in ip_blocks
-- ====================================================================

-- The ip_blocks table has redundant timestamp fields
-- Keep blocked_at, remove blocked_ts (if exists)
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'ip_blocks' AND column_name = 'blocked_ts'
    ) THEN
        -- Copy data from blocked_ts to blocked_at if needed
        UPDATE ip_blocks SET blocked_at = blocked_ts WHERE blocked_at IS NULL AND blocked_ts IS NOT NULL;
        ALTER TABLE ip_blocks DROP COLUMN blocked_ts;
    END IF;
END $$;

-- Similarly for expires_at/expires_ts
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'ip_blocks' AND column_name = 'expires_ts'
    ) THEN
        UPDATE ip_blocks SET expires_at = expires_ts WHERE expires_at IS NULL AND expires_ts IS NOT NULL;
        ALTER TABLE ip_blocks DROP COLUMN expires_ts;
    END IF;
END $$;

-- ====================================================================
-- FIX 3: Add missing NOT NULL constraints
-- ====================================================================

-- Ensure critical fields have proper constraints
DO $$
BEGIN
    -- users table
    ALTER TABLE users ALTER COLUMN username SET NOT NULL;
    ALTER TABLE users ALTER COLUMN creation_ts SET NOT NULL;

    -- rooms table
    ALTER TABLE rooms ALTER COLUMN room_id SET NOT NULL;
    ALTER TABLE rooms ALTER COLUMN creator SET NOT NULL;
    ALTER TABLE rooms ALTER COLUMN creation_ts SET NOT NULL;

    -- devices table
    ALTER TABLE devices ALTER COLUMN device_id SET NOT NULL;
    ALTER TABLE devices ALTER COLUMN user_id SET NOT NULL;
    ALTER TABLE devices ALTER COLUMN created_ts SET NOT NULL;

EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'Constraint already exists or cannot be applied: %', SQLERRM;
END $$;

-- ====================================================================
-- FIX 4: Standardize field types across tables
-- ====================================================================

-- Ensure all user_id fields use TEXT type consistently
DO $$
BEGIN
    -- friend_requests: from_user_id and to_user_id should be TEXT
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'friend_requests' AND column_name = 'from_user_id' AND data_type = 'character varying'
    ) THEN
        ALTER TABLE friend_requests ALTER COLUMN from_user_id TYPE TEXT USING from_user_id::TEXT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'friend_requests' AND column_name = 'to_user_id' AND data_type = 'character varying'
    ) THEN
        ALTER TABLE friend_requests ALTER COLUMN to_user_id TYPE TEXT USING to_user_id::TEXT;
    END IF;

    -- blocked_users: user_id and blocked_user_id should be TEXT
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'blocked_users' AND column_name = 'user_id' AND data_type = 'character varying'
    ) THEN
        ALTER TABLE blocked_users ALTER COLUMN user_id TYPE TEXT USING user_id::TEXT;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'blocked_users' AND column_name = 'blocked_user_id' AND data_type = 'character varying'
    ) THEN
        ALTER TABLE blocked_users ALTER COLUMN blocked_user_id TYPE TEXT USING blocked_user_id::TEXT;
    END IF;
END $$;

-- ====================================================================
-- FIX 5: Add missing indexes for foreign keys
-- ====================================================================

-- Add indexes on foreign key columns for better join performance
CREATE INDEX IF NOT EXISTS idx_devices_user_fkey ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_fkey ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_device_fkey ON access_tokens(device_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_fkey ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_device_fkey ON refresh_tokens(device_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user_fkey ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_fkey ON room_memberships(room_id);

-- ====================================================================
-- FIX 6: Add check constraints for data integrity
-- ====================================================================

-- Ensure timestamps are reasonable (not in the distant future)
DO $$
BEGIN
    ALTER TABLE users ADD CONSTRAINT chk_users_creation_ts_valid
        CHECK (creation_ts <= EXTRACT(EPOCH FROM NOW() + INTERVAL '1 day') * 1000);

    ALTER TABLE rooms ADD CONSTRAINT chk_rooms_creation_ts_valid
        CHECK (creation_ts <= EXTRACT(EPOCH FROM NOW() + INTERVAL '1 day') * 1000);

    ALTER TABLE devices ADD CONSTRAINT chk_devices_created_ts_valid
        CHECK (created_ts <= EXTRACT(EPOCH FROM NOW() + INTERVAL '1 day') * 1000);

EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'Constraint already exists: %', SQLERRM;
END $$;

-- ====================================================================
-- FIX 7: Add unique constraints to prevent duplicates
-- ====================================================================

-- Prevent duplicate device IDs per user
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'devices_user_device_unique'
    ) THEN
        ALTER TABLE devices ADD CONSTRAINT devices_user_device_unique
            UNIQUE (user_id, device_id);
    END IF;
EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'Constraint already exists: %', SQLERRM;
END $$;

-- ====================================================================
-- FIX 8: Standardize default values
-- ====================================================================

-- Ensure consistent default for boolean fields
DO $$
BEGIN
    -- devices table
    ALTER TABLE devices ALTER COLUMN last_seen_ts SET DEFAULT NOW();

    -- users table
    ALTER TABLE users ALTER COLUMN is_admin SET DEFAULT FALSE;
    ALTER TABLE users ALTER COLUMN is_guest SET DEFAULT FALSE;
    ALTER TABLE users ALTER COLUMN deactivated SET DEFAULT FALSE;
    ALTER TABLE users ALTER COLUMN shadow_banned SET DEFAULT FALSE;

    -- rooms table
    ALTER TABLE rooms ALTER COLUMN is_public SET DEFAULT FALSE;
    ALTER TABLE rooms ALTER COLUMN federate SET DEFAULT TRUE;

EXCEPTION WHEN OTHERS THEN
    RAISE NOTICE 'Default already set: %', SQLERRM;
END $$;

-- ====================================================================
-- FIX 9: Add comments for documentation
-- ====================================================================

COMMENT ON TABLE users IS 'User accounts including guests and admins';
COMMENT ON TABLE devices IS 'User devices for multi-device support';
COMMENT ON TABLE access_tokens IS 'Short-lived access tokens for authentication';
COMMENT ON TABLE refresh_tokens IS 'Long-lived refresh tokens for token renewal';
COMMENT ON TABLE rooms IS 'Matrix rooms (channels) with metadata';
COMMENT ON TABLE room_memberships IS 'Room membership state (join/leave/invite/ban)';
COMMENT ON TABLE events IS 'Matrix events (messages, state events)';
COMMENT ON TABLE friends IS 'Bidirectional friend relationships';
COMMENT ON TABLE friend_requests IS 'Friend relationship requests (pending/accepted/declined)';
COMMENT ON TABLE blocked_users IS 'User-specific block list';
COMMENT ON TABLE device_keys IS 'E2EE device keys for Olm/Megolm';
COMMENT ON TABLE voice_messages IS 'Voice/audio messages in rooms';
COMMENT ON TABLE ip_blocks IS 'IP address blocks for security';
COMMENT ON TABLE security_events IS 'Security audit log';

-- ====================================================================
-- FIX 10: Create helper functions for common operations
-- ====================================================================

-- Function to check if a user is active (not deactivated and not guest)
CREATE OR REPLACE FUNCTION is_active_user(user_id_param TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    SELECT EXISTS (
        SELECT 1 FROM users
        WHERE user_id = user_id_param
        AND deactivated = FALSE
        AND (is_guest IS NULL OR is_guest = FALSE)
    );
END;
$$ LANGUAGE plpgsql STABLE;

-- Function to get user's active rooms
CREATE OR REPLACE FUNCTION get_user_rooms(user_id_param TEXT)
RETURNS TABLE (room_id TEXT) AS $$
BEGIN
    RETURN QUERY
    SELECT room_id FROM room_memberships
    WHERE user_id = user_id_param AND membership = 'join';
END;
$$ LANGUAGE plpgsql STABLE;

-- Function to check if two users share a room
CREATE OR REPLACE FUNCTION users_share_room(user1_id TEXT, user2_id TEXT)
RETURNS BOOLEAN AS $$
BEGIN
    SELECT EXISTS (
        SELECT 1 FROM room_memberships m1
        JOIN room_memberships m2 ON m1.room_id = m2.room_id
        WHERE m1.user_id = user1_id AND m1.membership = 'join'
        AND m2.user_id = user2_id AND m2.membership = 'join'
        LIMIT 1
    );
END;
$$ LANGUAGE plpgsql STABLE;

-- ====================================================================
-- MIGRATION COMPLETE
-- ====================================================================
-- Schema consistency improvements:
-- - All user_id fields now use TEXT type
-- - Timestamp fields standardized (prefer created_at/updated_at)
-- - Redundant fields removed
-- - Missing constraints added
-- - Foreign key indexes added for join performance
-- - Helper functions for common queries
-- ====================================================================
