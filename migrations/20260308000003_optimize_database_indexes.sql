-- ============================================================================
-- Migration: Optimize database indexes
-- Version: 20260308000003
-- Description: 添加缺失的索引，优化查询性能
-- ============================================================================

-- ============================================================================
-- 用户认证相关索引
-- ============================================================================

-- users 表索引
CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_created_ts ON users(created_ts);
CREATE INDEX IF NOT EXISTS idx_users_is_deactivated ON users(is_deactivated) WHERE is_deactivated = false;
CREATE INDEX IF NOT EXISTS idx_users_appservice_id ON users(appservice_id) WHERE appservice_id IS NOT NULL;

-- devices 表索引
CREATE INDEX IF NOT EXISTS idx_devices_user_id ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen_ts ON devices(last_seen_ts DESC) WHERE last_seen_ts IS NOT NULL;

-- access_tokens 表索引
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_device ON access_tokens(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires_ts ON access_tokens(expires_ts) WHERE expires_ts IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_access_tokens_last_used ON access_tokens(last_used_ts DESC) WHERE last_used_ts IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_access_tokens_valid ON access_tokens(is_valid) WHERE is_valid = true;

-- refresh_tokens 表索引
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_device ON refresh_tokens(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires ON refresh_tokens(expires_at) WHERE expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_revoked ON refresh_tokens(is_revoked) WHERE is_revoked = false;

-- ============================================================================
-- 房间相关索引
-- ============================================================================

-- rooms 表索引
CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator);
CREATE INDEX IF NOT EXISTS idx_rooms_is_public ON rooms(is_public) WHERE is_public = true;
CREATE INDEX IF NOT EXISTS idx_rooms_created_ts ON rooms(created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_rooms_last_activity ON rooms(last_activity_ts DESC);

-- room_memberships 表复合索引
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_user ON room_memberships(room_id, user_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user_membership ON room_memberships(user_id, membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_room_membership ON room_memberships(room_id, membership);
CREATE INDEX IF NOT EXISTS idx_room_memberships_joined_ts ON room_memberships(joined_ts DESC) WHERE membership = 'join';

-- events 表复合索引
CREATE INDEX IF NOT EXISTS idx_events_room_ts ON events(room_id, origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_room_type ON events(room_id, event_type);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_type_state ON events(event_type, state_key) WHERE state_key IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_events_processed ON events(processed_ts) WHERE status = 'processed';
CREATE INDEX IF NOT EXISTS idx_events_depth ON events(depth);

-- room_summaries 表索引
CREATE INDEX IF NOT EXISTS idx_room_summaries_joined ON room_summaries(joined_members DESC);
CREATE INDEX IF NOT EXISTS idx_room_summaries_heroes ON room_summaries(heroes) WHERE heroes IS NOT NULL;

-- thread_statistics 表索引
CREATE INDEX IF NOT EXISTS idx_thread_statistics_room_thread ON thread_statistics(room_id, thread_id);
CREATE INDEX IF NOT EXISTS idx_thread_statistics_last_event ON thread_statistics(last_event_ts DESC);

-- ============================================================================
-- 加密相关索引
-- ============================================================================

-- device_keys 表索引
CREATE INDEX IF NOT EXISTS idx_device_keys_user_device ON device_keys(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_device_keys_algorithm ON device_keys(algorithm);
CREATE INDEX IF NOT EXISTS idx_device_keys_updated ON device_keys(ts_updated_ms DESC);

-- cross_signing_keys 表索引
CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user ON cross_signing_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_usage ON cross_signing_keys(key_usage);

-- megolm_sessions 表索引
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_room ON megolm_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_sender ON megolm_sessions(sender_key);
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_created ON megolm_sessions(created_ts DESC);

-- event_signatures 表索引
CREATE INDEX IF NOT EXISTS idx_event_signatures_event ON event_signatures(event_id);
CREATE INDEX IF NOT EXISTS idx_event_signatures_sender ON event_signatures(sender_id);

-- key_backups 表索引
CREATE INDEX IF NOT EXISTS idx_key_backups_user_version ON key_backups(user_id, version);
CREATE INDEX IF NOT EXISTS idx_key_backups_algorithm ON key_backups(algorithm);

-- ============================================================================
-- 推送通知相关索引
-- ============================================================================

-- push_devices 表索引
CREATE INDEX IF NOT EXISTS idx_push_devices_user ON push_devices(user_id);
CREATE INDEX IF NOT EXISTS idx_push_devices_enabled ON push_devices(is_enabled) WHERE is_enabled = true;
CREATE INDEX IF NOT EXISTS idx_push_devices_token ON push_devices(push_token);

-- push_rules 表索引
CREATE INDEX IF NOT EXISTS idx_push_rules_user_scope ON push_rules(user_id, scope);
CREATE INDEX IF NOT EXISTS idx_push_rules_user_kind ON push_rules(user_id, kind);
CREATE INDEX IF NOT EXISTS idx_push_rules_priority ON push_rules(priority);

-- ============================================================================
-- 联邦相关索引
-- ============================================================================

-- federation_servers 表索引
CREATE INDEX IF NOT EXISTS idx_federation_servers_name ON federation_servers(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_servers_last_success ON federation_servers(last_successful_connect_ts DESC);
CREATE INDEX IF NOT EXISTS idx_federation_servers_retry ON federation_servers(retry_count) WHERE is_active = true;

-- federation_queue 表索引
CREATE INDEX IF NOT EXISTS idx_federation_queue_destination ON federation_queue(destination);
CREATE INDEX IF NOT EXISTS idx_federation_queue_status ON federation_queue(status);
CREATE INDEX IF NOT EXISTS idx_federation_queue_created ON federation_queue(created_ts);

-- federation_blacklist 表索引
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_server ON federation_blacklist(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_blacklist_active ON federation_blacklist(is_active) WHERE is_active = true;

-- ============================================================================
-- 媒体相关索引
-- ============================================================================

-- media_metadata 表索引
CREATE INDEX IF NOT EXISTS idx_media_metadata_server ON media_metadata(server_name);
CREATE INDEX IF NOT EXISTS idx_media_metadata_uploader ON media_metadata(uploader_user_id);
CREATE INDEX IF NOT EXISTS idx_media_metadata_created ON media_metadata(created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_media_metadata_quarantine ON media_metadata(quarantine_status) WHERE quarantine_status IS NOT NULL;

-- thumbnails 表索引
CREATE INDEX IF NOT EXISTS idx_thumbnails_media ON thumbnails(media_id);
CREATE INDEX IF NOT EXISTS idx_thumbnails_dimensions ON thumbnails(width, height);

-- media_quota 表索引
CREATE INDEX IF NOT EXISTS idx_media_quota_user ON media_quota(user_id);
CREATE INDEX IF NOT EXISTS idx_media_quota_period ON media_quota(period_start, period_end);

-- ============================================================================
-- 新功能表索引
-- ============================================================================

-- beacon_info 表索引
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'beacon_info') THEN
        CREATE INDEX IF NOT EXISTS idx_beacon_info_room ON beacon_info(room_id);
        CREATE INDEX IF NOT EXISTS idx_beacon_info_sender ON beacon_info(sender);
        CREATE INDEX IF NOT EXISTS idx_beacon_info_live ON beacon_info(is_live) WHERE is_live = true;
        CREATE INDEX IF NOT EXISTS idx_beacon_info_expires ON beacon_info(expires_ts) WHERE expires_ts IS NOT NULL;
    END IF;
END $$;

-- beacon_locations 表索引
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'beacon_locations') THEN
        CREATE INDEX IF NOT EXISTS idx_beacon_locations_beacon_info ON beacon_locations(beacon_info_id);
        CREATE INDEX IF NOT EXISTS idx_beacon_locations_room ON beacon_locations(room_id);
        CREATE INDEX IF NOT EXISTS idx_beacon_locations_timestamp ON beacon_locations(timestamp DESC);
    END IF;
END $$;

-- matrixrtc_sessions 表索引
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'matrixrtc_sessions') THEN
        CREATE INDEX IF NOT EXISTS idx_matrixrtc_sessions_room ON matrixrtc_sessions(room_id);
        CREATE INDEX IF NOT EXISTS idx_matrixrtc_sessions_active ON matrixrtc_sessions(is_active) WHERE is_active = true;
        CREATE INDEX IF NOT EXISTS idx_matrixrtc_sessions_created ON matrixrtc_sessions(created_ts DESC);
    END IF;
END $$;

-- sliding_sync_tokens 表索引
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'sliding_sync_tokens') THEN
        CREATE INDEX IF NOT EXISTS idx_sliding_sync_tokens_user_device ON sliding_sync_tokens(user_id, device_id);
        CREATE INDEX IF NOT EXISTS idx_sliding_sync_tokens_conn ON sliding_sync_tokens(conn_id);
    END IF;
END $$;

-- dehydrated_devices 表索引
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'dehydrated_devices') THEN
        CREATE INDEX IF NOT EXISTS idx_dehydrated_devices_user ON dehydrated_devices(user_id);
        CREATE INDEX IF NOT EXISTS idx_dehydrated_devices_algorithm ON dehydrated_devices(algorithm);
        CREATE INDEX IF NOT EXISTS idx_dehydrated_devices_expires ON dehydrated_devices(expires_ts) WHERE expires_ts IS NOT NULL;
    END IF;
END $$;

-- moderation_rules 表索引
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'moderation_rules') THEN
        CREATE INDEX IF NOT EXISTS idx_moderation_rules_server ON moderation_rules(server_id) WHERE server_id IS NOT NULL;
        CREATE INDEX IF NOT EXISTS idx_moderation_rules_type ON moderation_rules(rule_type);
        CREATE INDEX IF NOT EXISTS idx_moderation_rules_active ON moderation_rules(is_active) WHERE is_active = true;
        CREATE INDEX IF NOT EXISTS idx_moderation_rules_priority ON moderation_rules(priority);
    END IF;
END $$;

-- ============================================================================
-- 账户数据相关索引
-- ============================================================================

-- filters 表索引
CREATE INDEX IF NOT EXISTS idx_filters_user ON filters(user_id);

-- account_data 表索引
CREATE INDEX IF NOT EXISTS idx_account_data_user_type ON account_data(user_id, data_type);

-- ============================================================================
-- 后台任务相关索引
-- ============================================================================

-- background_updates 表索引
CREATE INDEX IF NOT EXISTS idx_background_updates_status ON background_updates(status);
CREATE INDEX IF NOT EXISTS idx_background_updates_created ON background_updates(created_ts);

-- workers 表索引
CREATE INDEX IF NOT EXISTS idx_workers_status ON workers(status);
CREATE INDEX IF NOT EXISTS idx_workers_last_heartbeat ON workers(last_heartbeat_ts);

-- ============================================================================
-- 记录迁移完成
-- ============================================================================

INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES ('20260308000003', 'optimize_database_indexes', 
        EXTRACT(EPOCH FROM NOW()) * 1000, 
        '添加缺失的索引，优化查询性能')
ON CONFLICT (version) DO NOTHING;
