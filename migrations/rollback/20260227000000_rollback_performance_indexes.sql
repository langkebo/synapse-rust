-- =============================================================================
-- Synapse-Rust 性能索引回滚脚本
-- 版本: 1.0.0
-- 创建日期: 2026-02-27
-- 描述: 回滚性能索引迁移
-- 
-- 执行方式:
--   docker exec -i synapse-postgres psql -U synapse -d synapse_test < 20260227000000_rollback_performance_indexes.sql
-- =============================================================================

SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;

-- =============================================================================
-- 回滚 events 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_events_room_origin_ts;
DROP INDEX IF EXISTS idx_events_room_type_origin_ts;
DROP INDEX IF EXISTS idx_events_room_state_origin_ts;
DROP INDEX IF EXISTS idx_events_room_type_state;
DROP INDEX IF EXISTS idx_events_room_since_ts;
DROP INDEX IF EXISTS idx_events_sender_origin_ts;
DROP INDEX IF EXISTS idx_events_room_messages;
DROP INDEX IF EXISTS idx_events_batch_rooms;
DROP INDEX IF EXISTS idx_events_batch_since;

-- =============================================================================
-- 回滚 users 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_users_id_batch;
DROP INDEX IF EXISTS idx_users_username_trgm;
DROP INDEX IF EXISTS idx_users_userid_trgm;
DROP INDEX IF EXISTS idx_users_displayname_trgm;
DROP INDEX IF EXISTS idx_users_active;
DROP INDEX IF EXISTS idx_users_email;
-- 注意: idx_users_username 和 idx_users_creation_ts 是基础索引，保留

-- =============================================================================
-- 回滚 room_members 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_room_members_room_status;
DROP INDEX IF EXISTS idx_room_members_user_joined;
DROP INDEX IF EXISTS idx_room_members_room_updated;
DROP INDEX IF EXISTS idx_room_members_user_membership;

-- =============================================================================
-- 回滚 rooms 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_rooms_public;
DROP INDEX IF EXISTS idx_rooms_version;
-- 注意: idx_rooms_creator 是基础索引，保留

-- =============================================================================
-- 回滚 devices 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_devices_user_last_seen;
DROP INDEX IF EXISTS idx_devices_active;

-- =============================================================================
-- 回滚 access_tokens 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_access_tokens_user_valid;
DROP INDEX IF EXISTS idx_access_tokens_expires;
-- 注意: idx_access_tokens_token 和 idx_access_tokens_user 是基础索引，保留

-- =============================================================================
-- 回滚 refresh_tokens 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_refresh_tokens_user_valid;
-- 注意: idx_refresh_tokens_hash 和 idx_refresh_tokens_user 是基础索引，保留

-- =============================================================================
-- 回滚 push_notification_queue 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_push_queue_pending;
DROP INDEX IF EXISTS idx_push_queue_user_device;

-- =============================================================================
-- 回滚 pushers 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_pushers_user_enabled;
-- 注意: idx_pushers_user 是基础索引，保留

-- =============================================================================
-- 回滚 event_reports 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_event_reports_event_ts;
DROP INDEX IF EXISTS idx_event_reports_room_status;
DROP INDEX IF EXISTS idx_event_reports_status;

-- =============================================================================
-- 回滚 thread_roots 和 thread_replies 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_thread_rooms_room;
DROP INDEX IF EXISTS idx_thread_replies_thread_ts;

-- =============================================================================
-- 回滚 media_repository 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_media_user;
DROP INDEX IF EXISTS idx_media_origin;
DROP INDEX IF EXISTS idx_media_quarantined;

-- =============================================================================
-- 回滚 federation_signing_keys 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_federation_keys_valid;

-- =============================================================================
-- 回滚 registration_tokens 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_registration_tokens_valid;
DROP INDEX IF EXISTS idx_registration_tokens_active;

-- =============================================================================
-- 回滚 security_events 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_security_events_user_ts;
DROP INDEX IF EXISTS idx_security_events_type_ts;

-- =============================================================================
-- 回滚 ip_reputation 和 ip_blocks 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_ip_reputation_blocked;
DROP INDEX IF EXISTS idx_ip_blocks_enabled;

-- =============================================================================
-- 回滚 saml_sessions 和 cas_tickets 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_saml_sessions_valid;
DROP INDEX IF EXISTS idx_cas_tickets_valid;

-- =============================================================================
-- 回滚 voice_messages 表索引
-- =============================================================================

DROP INDEX IF EXISTS idx_voice_messages_room_ts;
DROP INDEX IF EXISTS idx_voice_messages_user_ts;
DROP INDEX IF EXISTS idx_voice_messages_pending;

-- =============================================================================
-- 删除迁移记录
-- =============================================================================

DELETE FROM schema_migrations WHERE version = '20260227000000';

-- =============================================================================
-- 验证回滚
-- =============================================================================

DO $$
DECLARE
    index_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO index_count
    FROM pg_indexes 
    WHERE schemaname = 'public' 
    AND indexname LIKE 'idx_%';
    
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'Performance indexes rollback completed!';
    RAISE NOTICE 'Remaining custom indexes: %', index_count;
    RAISE NOTICE '==========================================';
END $$;
