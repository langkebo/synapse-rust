-- ============================================================================
-- Migration: 20260302000004_comprehensive_db_optimization.sql
-- Created: 2026-03-02
-- Purpose: Comprehensive database optimization based on pg-aiguide best practices
-- Tasks: 
--   1. Merge duplicate tables (push_rule/push_rules, room_members/room_memberships)
--   2. Remove redundant tables and columns
--   3. Add missing indexes for performance
--   4. Add foreign key constraints for data integrity
--   5. Create unified schema
-- ============================================================================

-- ============================================================================
-- SECTION 1: Pre-optimization Analysis
-- ============================================================================

-- Create optimization log table
CREATE TABLE IF NOT EXISTS optimization_log (
    id SERIAL PRIMARY KEY,
    operation TEXT NOT NULL,
    table_name TEXT,
    details TEXT,
    executed_at TIMESTAMP DEFAULT NOW()
);

-- Log start of optimization
INSERT INTO optimization_log (operation, details) 
VALUES ('START', 'Beginning comprehensive database optimization');

-- ============================================================================
-- SECTION 2: Merge Duplicate Tables
-- ============================================================================

-- 2.1 Merge push_rule into push_rules (they have identical structure)
DO $$
DECLARE
    push_rule_count INTEGER;
    push_rules_count INTEGER;
BEGIN
    -- Check if both tables exist
    SELECT COUNT(*) INTO push_rule_count FROM push_rule;
    SELECT COUNT(*) INTO push_rules_count FROM push_rules;
    
    -- If push_rule has data, migrate to push_rules
    IF push_rule_count > 0 THEN
        -- Insert data from push_rule to push_rules (avoiding duplicates)
        INSERT INTO push_rules (rule_id, user_id, kind, priority, pattern, conditions, actions, is_default, is_enabled, created_ts, updated_ts)
        SELECT 
            rule_id,
            user_id,
            kind,
            priority,
            pattern,
            conditions,
            actions,
            is_default,
            is_enabled,
            created_ts,
            updated_ts
        FROM push_rule
        WHERE NOT EXISTS (
            SELECT 1 FROM push_rules WHERE push_rules.rule_id = push_rule.rule_id
        );
        
        INSERT INTO optimization_log (operation, table_name, details)
        VALUES ('MERGE', 'push_rule', 'Migrated ' || push_rule_count || ' rows to push_rules');
    END IF;
END $$;

-- 2.2 Merge room_members into room_memberships
DO $$
DECLARE
    room_members_count INTEGER;
    room_memberships_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO room_members_count FROM room_members;
    SELECT COUNT(*) INTO room_memberships_count FROM room_memberships;
    
    IF room_members_count > 0 THEN
        -- Migrate data from room_members to room_memberships
        INSERT INTO room_memberships (room_id, user_id, membership, event_id, display_name, avatar_url, updated_ts, joined_ts, left_ts, reason, sender, event_type, join_reason, invite_token, is_banned, banned_by, ban_reason, ban_ts)
        SELECT 
            room_id,
            user_id,
            membership,
            event_id,
            displayname,
            avatar_url,
            updated_ts,
            NULL, -- joined_ts (not in room_members)
            NULL, -- left_ts
            reason,
            NULL, -- sender
            NULL, -- event_type
            NULL, -- join_reason
            NULL, -- invite_token
            FALSE, -- is_banned
            NULL, -- banned_by
            NULL, -- ban_reason
            NULL -- ban_ts
        FROM room_members
        WHERE NOT EXISTS (
            SELECT 1 FROM room_memberships WHERE room_memberships.room_id = room_members.room_id AND room_memberships.user_id = room_members.user_id
        );
        
        INSERT INTO optimization_log (operation, table_name, details)
        VALUES ('MERGE', 'room_members', 'Migrated ' || room_members_count || ' rows to room_memberships');
    END IF;
END $$;

-- ============================================================================
-- SECTION 3: Drop Redundant Tables (after data migration)
-- ============================================================================

-- Drop push_rule (data migrated to push_rules)
DROP TABLE IF EXISTS push_rule;

-- Drop room_members (data migrated to room_memberships)
DROP TABLE IF EXISTS room_members;

-- Drop retention_policies (using room_retention_policies and server_retention_policy)
DROP TABLE IF EXISTS retention_policies;

-- ============================================================================
-- SECTION 4: Add Missing Indexes for Performance
-- ============================================================================

-- 4.1 Events table indexes
CREATE INDEX IF NOT EXISTS idx_events_room_sender ON events(room_id, sender);
CREATE INDEX IF NOT EXISTS idx_events_type_ts ON events(type, origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_state_key ON events(state_key) WHERE state_key IS NOT NULL;

-- 4.2 Users table indexes
CREATE INDEX IF NOT EXISTS idx_users_creation_ts ON users(creation_ts DESC);
CREATE INDEX IF NOT EXISTS idx_users_deactivated ON users(is_deactivated) WHERE is_deactivated = TRUE;

-- 4.3 Rooms table indexes
CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator);
CREATE INDEX IF NOT EXISTS idx_rooms_public ON rooms(is_public) WHERE is_public = TRUE;
CREATE INDEX IF NOT EXISTS idx_rooms_member_count ON rooms(member_count DESC);

-- 4.4 Device keys indexes
CREATE INDEX IF NOT EXISTS idx_device_keys_algorithm ON device_keys(algorithm);
CREATE INDEX IF NOT EXISTS idx_device_keys_created ON device_keys(created_at DESC);

-- 4.5 Room memberships indexes
CREATE INDEX IF NOT EXISTS idx_room_memberships_membership ON room_memberships(membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_joined ON room_memberships(joined_ts DESC) WHERE joined_ts IS NOT NULL;

-- 4.6 Federation signing keys indexes
CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_valid ON federation_signing_keys(valid_until_ts) WHERE valid_until_ts > EXTRACT(EPOCH FROM NOW()) * 1000;

-- 4.7 Push rules indexes
CREATE INDEX IF NOT EXISTS idx_push_rules_user_kind ON push_rules(user_id, kind);
CREATE INDEX IF NOT EXISTS idx_push_rules_priority ON push_rules(priority DESC);

-- 4.8 Space tables indexes
CREATE INDEX IF NOT EXISTS idx_spaces_creator ON spaces(creator);
CREATE INDEX IF NOT EXISTS idx_space_members_membership ON space_members(membership);

-- 4.9 Voice messages indexes
CREATE INDEX IF NOT EXISTS idx_voice_messages_pending ON voice_messages(created_ts) WHERE is_processed = FALSE;

-- 4.10 Thread tables indexes
CREATE INDEX IF NOT EXISTS idx_thread_roots_room ON thread_roots(room_id, last_reply_ts DESC);
CREATE INDEX IF NOT EXISTS idx_thread_replies_thread ON thread_replies(thread_id, created_ts DESC);

-- ============================================================================
-- SECTION 5: Add Foreign Key Constraints (with ON DELETE CASCADE)
-- ============================================================================

-- 5.1 Events -> Rooms
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_room;
ALTER TABLE events ADD CONSTRAINT fk_events_room 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;

-- 5.2 Events -> Users (sender)
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_sender;
ALTER TABLE events ADD CONSTRAINT fk_events_sender 
    FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE;

-- 5.3 Room Memberships -> Rooms
ALTER TABLE room_memberships DROP CONSTRAINT IF EXISTS fk_room_memberships_room;
ALTER TABLE room_memberships ADD CONSTRAINT fk_room_memberships_room 
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE;

-- 5.4 Room Memberships -> Users
ALTER TABLE room_memberships DROP CONSTRAINT IF EXISTS fk_room_memberships_user;
ALTER TABLE room_memberships ADD CONSTRAINT fk_room_memberships_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 5.5 Device Keys -> Users
ALTER TABLE device_keys DROP CONSTRAINT IF EXISTS fk_device_keys_user;
ALTER TABLE device_keys ADD CONSTRAINT fk_device_keys_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 5.6 Access Tokens -> Users
ALTER TABLE access_tokens DROP CONSTRAINT IF EXISTS fk_access_tokens_user;
ALTER TABLE access_tokens ADD CONSTRAINT fk_access_tokens_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 5.7 Refresh Tokens -> Users
ALTER TABLE refresh_tokens DROP CONSTRAINT IF EXISTS fk_refresh_tokens_user;
ALTER TABLE refresh_tokens ADD CONSTRAINT fk_refresh_tokens_user 
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE;

-- 5.8 Space Members -> Spaces
ALTER TABLE space_members DROP CONSTRAINT IF EXISTS fk_space_members_space;
ALTER TABLE space_members ADD CONSTRAINT fk_space_members_space 
    FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE;

-- 5.9 Space Children -> Spaces
ALTER TABLE space_children DROP CONSTRAINT IF EXISTS fk_space_children_space;
ALTER TABLE space_children ADD CONSTRAINT fk_space_children_space 
    FOREIGN KEY (space_id) REFERENCES spaces(space_id) ON DELETE CASCADE;

-- ============================================================================
-- SECTION 6: Data Integrity Checks
-- ============================================================================

-- 6.1 Check for orphaned events (events without valid rooms)
DO $$
DECLARE
    orphan_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO orphan_count
    FROM events e
    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = e.room_id);
    
    IF orphan_count > 0 THEN
        INSERT INTO optimization_log (operation, table_name, details)
        VALUES ('WARNING', 'events', 'Found ' || orphan_count || ' orphaned events without valid rooms');
    END IF;
END $$;

-- 6.2 Check for orphaned room memberships
DO $$
DECLARE
    orphan_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO orphan_count
    FROM room_memberships rm
    WHERE NOT EXISTS (SELECT 1 FROM rooms r WHERE r.room_id = rm.room_id);
    
    IF orphan_count > 0 THEN
        INSERT INTO optimization_log (operation, table_name, details)
        VALUES ('WARNING', 'room_memberships', 'Found ' || orphan_count || ' orphaned memberships without valid rooms');
    END IF;
END $$;

-- 6.3 Check for orphaned device keys
DO $$
DECLARE
    orphan_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO orphan_count
    FROM device_keys dk
    WHERE NOT EXISTS (SELECT 1 FROM users u WHERE u.user_id = dk.user_id);
    
    IF orphan_count > 0 THEN
        INSERT INTO optimization_log (operation, table_name, details)
        VALUES ('WARNING', 'device_keys', 'Found ' || orphan_count || ' orphaned device keys without valid users');
    END IF;
END $$;

-- ============================================================================
-- SECTION 7: Vacuum and Analyze
-- ============================================================================

-- Run VACUUM to reclaim space and update statistics
VACUUM FULL;

-- Analyze tables for query planning
ANALYZE events;
ANALYZE users;
ANALYZE rooms;
ANALYZE room_memberships;
ANALYZE device_keys;

-- ============================================================================
-- SECTION 8: Create Useful Views
-- ============================================================================

-- View: Active users with their device count
CREATE OR REPLACE VIEW v_active_users AS
SELECT 
    u.user_id,
    u.username,
    u.displayname,
    u.email,
    u.creation_ts,
    COUNT(DISTINCT d.device_id) as device_count,
    MAX(d.last_seen_ts) as last_active_ts
FROM users u
LEFT JOIN devices d ON d.user_id = u.user_id
WHERE u.is_deactivated = FALSE OR u.is_deactivated IS NULL
GROUP BY u.user_id, u.username, u.displayname, u.email, u.creation_ts;

-- View: Room statistics
CREATE OR REPLACE VIEW v_room_statistics AS
SELECT 
    r.room_id,
    r.name,
    r.is_public,
    r.creation_ts,
    COUNT(rm.user_id) FILTER (WHERE rm.membership = 'join') as joined_members,
    COUNT(rm.user_id) FILTER (WHERE rm.membership = 'invite') as invited_members,
    COUNT(rm.user_id) FILTER (WHERE rm.membership = 'leave') as left_members,
    COUNT(e.event_id) as total_events
FROM rooms r
LEFT JOIN room_memberships rm ON rm.room_id = r.room_id
LEFT JOIN events e ON e.room_id = r.room_id
GROUP BY r.room_id, r.name, r.is_public, r.creation_ts;

-- View: Federation health
CREATE OR REPLACE VIEW v_federation_health AS
SELECT 
    server_name,
    COUNT(*) as total_requests,
    AVG(response_time_ms) as avg_response_time,
    MAX(last_success_ts) as last_success,
    MAX(last_failure_ts) as last_failure
FROM federation_access_stats
GROUP BY server_name;

-- ============================================================================
-- SECTION 9: Optimization Complete
-- ============================================================================

-- Log completion
INSERT INTO optimization_log (operation, details) 
VALUES ('COMPLETE', 'Database optimization completed successfully');

-- Record migration
INSERT INTO migrations (name, applied_at) 
VALUES ('20260302000004_comprehensive_db_optimization', NOW())
ON CONFLICT (name) DO NOTHING;

-- Display optimization summary
SELECT * FROM optimization_log ORDER BY executed_at;

-- Display final statistics
SELECT 
    'Total Tables' as metric,
    COUNT(*)::TEXT as value
FROM information_schema.tables 
WHERE table_schema = 'public'
UNION ALL
SELECT 
    'Total Indexes' as metric,
    COUNT(*)::TEXT as value
FROM pg_indexes 
WHERE schemaname = 'public'
UNION ALL
SELECT 
    'Database Size' as metric,
    pg_size_pretty(pg_database_size('synapse_test')) as value;
