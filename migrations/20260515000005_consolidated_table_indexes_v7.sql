-- ============================================================================
-- Forward Script: 20260515000005_consolidated_table_indexes_v7.sql
-- Description: Consolidated table indexes for performance optimization.
--   - events: sync covering, time range, content GIN, type/state partial, sender
--   - users: LOWER function indexes, trigram indexes
--   - room_memberships: user status, room status
--   - access_tokens: token unique, user valid, expires partial
--   - federation_queue: pending partial, destination status
--   - background_updates: running partial, pending partial
--   - Additional trigram: users(user_id,email), search_index(content)
--   - spaces: name/topic trigram
--   - thread_replies: content->>'body' trigram
-- Merged from:
--   - 20260520000002_add_events_table_indexes.sql
--   - 20260520000003_add_users_table_indexes.sql
--   - 20260520000004_add_room_memberships_table_indexes.sql
--   - 20260520000005_add_access_tokens_table_indexes.sql
--   - 20260520000006_add_federation_queue_table_indexes.sql
--   - 20260520000007_add_background_updates_table_indexes.sql
--   - 20260520000008_add_additional_trigram_indexes.sql
--   - 20260520000009_add_spaces_table_search_indexes.sql
--   - 20260520000010_add_thread_content_search_indexes.sql
-- Created: 2026-05-09
-- Risk: LOW - All operations use CREATE INDEX IF NOT EXISTS for idempotency.
-- ============================================================================

SET TIME ZONE 'UTC';

-- Ensure required extensions are available
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- ============================================================================
-- Section A: Events table indexes
-- ============================================================================

-- A1. Covering index for Sync API core path (avoids table lookups)
CREATE INDEX IF NOT EXISTS idx_events_sync_covering ON events(room_id, stream_ordering DESC)
    INCLUDE (event_id, sender, event_type, content, origin_server_ts);

-- A2. Time range query composite index
CREATE INDEX IF NOT EXISTS idx_events_room_time ON events(room_id, origin_server_ts DESC);

-- A3. Content search GIN index (JSONB path ops)
CREATE INDEX IF NOT EXISTS idx_events_content_gin ON events USING GIN (content jsonb_path_ops);

-- A4. Event type state partial index
CREATE INDEX IF NOT EXISTS idx_events_type_state ON events(room_id, event_type, state_key)
    WHERE event_type LIKE 'm.room.%';

-- A5. Sender query index (federation + moderation)
CREATE INDEX IF NOT EXISTS idx_events_sender_time ON events(sender, origin_server_ts DESC);

-- ============================================================================
-- Section B: Users table indexes
-- ============================================================================

-- B1. Function indexes for exact prefix matching
CREATE INDEX IF NOT EXISTS idx_users_lower_username ON users(LOWER(username));
CREATE INDEX IF NOT EXISTS idx_users_lower_displayname ON users(LOWER(displayname));

-- B2. Trigram indexes for fuzzy search
CREATE INDEX IF NOT EXISTS idx_users_username_trgm ON users USING GIN (username gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_users_displayname_trgm ON users USING GIN (displayname gin_trgm_ops);

-- ============================================================================
-- Section C: Room memberships table indexes
-- ============================================================================

-- C1. User room list (sync core path)
CREATE INDEX IF NOT EXISTS idx_room_memberships_user_status ON room_memberships(user_id, membership, joined_ts DESC);

-- C2. Room member statistics (admin)
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_status ON room_memberships(room_id, membership);

-- ============================================================================
-- Section D: Access tokens table indexes
-- ============================================================================

-- D1. Token lookup unique index (authentication hotspot)
CREATE UNIQUE INDEX IF NOT EXISTS idx_access_tokens_token ON access_tokens(token);

-- D2. User token list (device management)
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_valid ON access_tokens(user_id, is_revoked);

-- D3. Expired token cleanup partial index
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires ON access_tokens(expires_at) WHERE is_revoked = false;

-- ============================================================================
-- Section E: Federation queue table indexes
-- ============================================================================

-- E1. Pending queue partial index
CREATE INDEX IF NOT EXISTS idx_federation_queue_pending ON federation_queue(destination, created_ts)
    WHERE status = 'pending';

-- E2. Destination + status composite index
CREATE INDEX IF NOT EXISTS idx_federation_queue_dest_status ON federation_queue(destination, status, created_ts);

-- ============================================================================
-- Section F: Background updates table indexes
-- ============================================================================

-- F1. Running tasks partial index
CREATE INDEX IF NOT EXISTS idx_background_updates_running ON background_updates(job_name, started_ts)
    WHERE status = 'running';

-- F2. Pending/scheduled tasks partial index
CREATE INDEX IF NOT EXISTS idx_background_updates_pending ON background_updates(status, job_type, created_ts)
    WHERE status IN ('pending', 'scheduled');

-- ============================================================================
-- Section G: Additional trigram indexes
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_users_user_id_trgm ON users USING GIN (user_id gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_users_email_trgm ON users USING GIN (email gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_search_index_content_trgm ON search_index USING GIN (content gin_trgm_ops);

-- ============================================================================
-- Section H: Spaces table search indexes
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_spaces_name_trgm ON spaces USING GIN (name gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_spaces_topic_trgm ON spaces USING GIN (topic gin_trgm_ops);

-- ============================================================================
-- Section I: Thread content search indexes
-- ============================================================================

CREATE INDEX IF NOT EXISTS idx_thread_replies_content_trgm ON thread_replies USING GIN ((content->>'body') gin_trgm_ops);

-- ============================================================================
-- Migration records
-- ============================================================================
INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES ('20260515000005', 'consolidated_table_indexes_v7', TRUE, 'Consolidated table indexes for events, users, room_memberships, access_tokens, federation_queue, background_updates, trigram, spaces, threads', (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT)
ON CONFLICT (version) DO NOTHING;
