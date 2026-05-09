-- ============================================================================
-- Rollback Script: 20260515000005_consolidated_table_indexes_v7.undo.sql
-- Forward Script: 20260515000005_consolidated_table_indexes_v7.sql
-- Created: 2026-05-09
-- Risk: LOW - Only drops indexes, no data loss.
-- Rollback RTO: < 5 minutes
-- ============================================================================

-- Section A: Events
DROP INDEX IF EXISTS idx_events_sync_covering;
DROP INDEX IF EXISTS idx_events_room_time;
DROP INDEX IF EXISTS idx_events_content_gin;
DROP INDEX IF EXISTS idx_events_type_state;
DROP INDEX IF EXISTS idx_events_sender_time;

-- Section B: Users
DROP INDEX IF EXISTS idx_users_lower_username;
DROP INDEX IF EXISTS idx_users_lower_displayname;
DROP INDEX IF EXISTS idx_users_username_trgm;
DROP INDEX IF EXISTS idx_users_displayname_trgm;

-- Section C: Room memberships
DROP INDEX IF EXISTS idx_room_memberships_user_status;
DROP INDEX IF EXISTS idx_room_memberships_room_status;

-- Section D: Access tokens
DROP INDEX IF EXISTS idx_access_tokens_token;
DROP INDEX IF EXISTS idx_access_tokens_user_valid;
DROP INDEX IF EXISTS idx_access_tokens_expires;

-- Section E: Federation queue
DROP INDEX IF EXISTS idx_federation_queue_pending;
DROP INDEX IF EXISTS idx_federation_queue_dest_status;

-- Section F: Background updates
DROP INDEX IF EXISTS idx_background_updates_running;
DROP INDEX IF EXISTS idx_background_updates_pending;

-- Section G: Additional trigram
DROP INDEX IF EXISTS idx_users_user_id_trgm;
DROP INDEX IF EXISTS idx_users_email_trgm;
DROP INDEX IF EXISTS idx_search_index_content_trgm;

-- Section H: Spaces
DROP INDEX IF EXISTS idx_spaces_name_trgm;
DROP INDEX IF EXISTS idx_spaces_topic_trgm;

-- Section I: Thread replies
DROP INDEX IF EXISTS idx_thread_replies_content_trgm;