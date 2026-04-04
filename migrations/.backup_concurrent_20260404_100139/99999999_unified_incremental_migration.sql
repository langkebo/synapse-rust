-- ============================================================================
-- synapse-rust 历史综合增量兼容资产 v1.0.1
-- 创建日期: 2026-03-24
--
-- 说明: 该文件仅保留历史索引收敛能力，不代表当前完整增量迁移事实来源
--       新环境与升级环境的真实执行口径以 docker/db_migrate.sh、
--       db-migration-gate.yml 和离散迁移文件为准
-- 版本: 99999999 (兼容保留)
--
-- 历史用途:
--   - 汇总早期索引优化
--   - 为旧部署链提供兼容性保留
--   - 不应被视为“已覆盖所有增量迁移”的证明
-- ============================================================================

SET TIME ZONE 'UTC';

DO $$
BEGIN
    RAISE NOTICE '开始执行历史综合增量兼容脚本...';
END $$;

-- ============================================================================
-- 1. events 表复合索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_events_room_time
ON events(room_id, origin_server_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_events_sender_time
ON events(sender, origin_server_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_events_type_room
ON events(event_type, room_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_events_type_sender
ON events(event_type, sender);

-- ============================================================================
-- 2. room_memberships 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_memberships_user_room
ON room_memberships(user_id, room_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_memberships_room_membership
ON room_memberships(room_id, membership);

-- ============================================================================
-- 3. pushers 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_pushers_user
ON pushers(user_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_pushers_enabled
ON pushers(is_enabled)
WHERE is_enabled = TRUE;

-- ============================================================================
-- 4. devices 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_devices_user_id
ON devices(user_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_devices_last_seen
ON devices(last_seen_ts DESC);

-- ============================================================================
-- 5. user_threepids 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_user_threepids_user
ON user_threepids(user_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_threepids_medium_address
ON user_threepids(medium, address);

-- ============================================================================
-- 6. access_tokens 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_access_tokens_user_id
ON access_tokens(user_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_access_tokens_valid
ON access_tokens(is_revoked)
WHERE is_revoked = FALSE;

-- ============================================================================
-- 7. notifications 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_notifications_user_id
ON notifications(user_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_notifications_room
ON notifications(room_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_notifications_ts
ON notifications(ts DESC);

-- ============================================================================
-- 8. presence 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_presence_user_status
ON presence(user_id, presence);

-- ============================================================================
-- 9. event_receipts 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_event_receipts_event
ON event_receipts(event_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_event_receipts_room
ON event_receipts(room_id);

-- ============================================================================
-- 10. read_markers 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_read_markers_room_user
ON read_markers(room_id, user_id);

-- ============================================================================
-- 11. room_state_events 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_room_state_events_room
ON room_state_events(room_id);

-- ============================================================================
-- 12. room_events 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_room_events_room
ON room_events(room_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_room_events_event
ON room_events(event_id);

-- ============================================================================
-- 13. sliding_sync_rooms 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_sliding_sync_rooms_user_device
ON sliding_sync_rooms(user_id, device_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_sliding_sync_rooms_unique
ON sliding_sync_rooms(user_id, room_id);

-- ============================================================================
-- 14. key_backups 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_key_backups_user
ON key_backups(user_id);

-- ============================================================================
-- 15. backup_keys 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_backup_keys_backup
ON backup_keys(backup_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_backup_keys_room
ON backup_keys(room_id);

-- ============================================================================
-- 16. e2ee_key_requests 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_e2ee_key_requests_user
ON e2ee_key_requests(user_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_e2ee_key_requests_pending
ON e2ee_key_requests(is_fulfilled)
WHERE is_fulfilled = FALSE;

-- ============================================================================
-- 17. olm_sessions 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_olm_sessions_user_device
ON olm_sessions(user_id, device_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_olm_sessions_expires
ON olm_sessions(expires_at)
WHERE expires_at IS NOT NULL;

-- ============================================================================
-- 18. megolm_sessions 表索引优化
-- ============================================================================

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_megolm_sessions_room
ON megolm_sessions(room_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS CONCURRENTLY IF NOT EXISTS idx_megolm_sessions_session
ON megolm_sessions(session_id);

-- ============================================================================
-- 记录迁移执行
-- ============================================================================

DO $$
DECLARE
    index_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO index_count FROM pg_indexes
    WHERE schemaname = 'public' AND indexname LIKE 'idx_%';

    RAISE NOTICE '==========================================';
    RAISE NOTICE '历史综合增量兼容脚本执行完成';
    RAISE NOTICE '完成时间: %', NOW();
    RAISE NOTICE '索引数量: %', index_count;
    RAISE NOTICE '==========================================';

    -- 记录迁移执行
    INSERT INTO schema_migrations (version, name, success, applied_ts, execution_time_ms, description)
    VALUES ('99999999', 'unified_incremental_migration', true, FLOOR(EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT, 0, '历史综合增量兼容资产 v1.0.1，不代表全部离散迁移已收敛')
    ON CONFLICT (version) DO NOTHING;
END $$;
