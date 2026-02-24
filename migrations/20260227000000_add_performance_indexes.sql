-- =============================================================================
-- Synapse-Rust 性能索引迁移脚本
-- 版本: 1.0.0
-- 创建日期: 2026-02-27
-- 描述: 为高频查询添加复合索引，优化数据库性能
-- 
-- 执行方式:
--   docker exec -i synapse-postgres psql -U synapse -d synapse_test < 20260227000000_add_performance_indexes.sql
--
-- 回滚方式:
--   docker exec -i synapse-postgres psql -U synapse -d synapse_test < 20260227000000_rollback_performance_indexes.sql
-- =============================================================================

SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;

-- =============================================================================
-- 安装必要扩展
-- =============================================================================

-- pg_trgm 扩展用于支持 ILIKE 模糊搜索索引
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- =============================================================================
-- 第一部分: events 表索引
-- =============================================================================

-- 索引1: 房间事件按时间排序查询 (高频)
-- 查询模式: WHERE room_id = $1 ORDER BY origin_server_ts DESC
-- 场景: 获取房间消息列表、分页加载
CREATE INDEX IF NOT EXISTS idx_events_room_origin_ts 
    ON events(room_id, origin_server_ts DESC);

-- 索引2: 房间事件按类型和时间查询 (高频)
-- 查询模式: WHERE room_id = $1 AND event_type = $2 ORDER BY origin_server_ts DESC
-- 场景: 获取特定类型消息（如 m.room.message）
CREATE INDEX IF NOT EXISTS idx_events_room_type_origin_ts 
    ON events(room_id, event_type, origin_server_ts DESC);

-- 索引3: 房间状态事件查询 (高频)
-- 查询模式: WHERE room_id = $1 AND state_key IS NOT NULL ORDER BY origin_server_ts DESC
-- 场景: 获取房间状态事件
CREATE INDEX IF NOT EXISTS idx_events_room_state_origin_ts 
    ON events(room_id, origin_server_ts DESC) 
    WHERE state_key IS NOT NULL;

-- 索引4: 房间状态事件按类型查询 (高频)
-- 查询模式: WHERE room_id = $1 AND event_type = $2 AND state_key IS NOT NULL
-- 场景: 获取特定类型的状态事件
CREATE INDEX IF NOT EXISTS idx_events_room_type_state 
    ON events(room_id, event_type, state_key) 
    WHERE state_key IS NOT NULL;

-- 索引5: 增量同步查询 (高频)
-- 查询模式: WHERE room_id = $1 AND origin_server_ts > $2 ORDER BY origin_server_ts ASC
-- 场景: 增量同步、获取新消息
CREATE INDEX IF NOT EXISTS idx_events_room_since_ts 
    ON events(room_id, origin_server_ts ASC) 
    WHERE origin_server_ts > 0;

-- 索引6: 用户发送的事件查询
-- 查询模式: WHERE sender = $1 ORDER BY origin_server_ts DESC
-- 场景: 获取用户发送的所有事件
CREATE INDEX IF NOT EXISTS idx_events_sender_origin_ts 
    ON events(sender, origin_server_ts DESC);

-- 索引7: 消息统计查询
-- 查询模式: WHERE room_id = $1 AND event_type = 'm.room.message'
-- 场景: 统计房间消息数量
CREATE INDEX IF NOT EXISTS idx_events_room_messages 
    ON events(room_id) 
    WHERE event_type = 'm.room.message';

-- 索引8: 批量获取多个房间事件
-- 查询模式: WHERE room_id = ANY($1) ORDER BY room_id, origin_server_ts DESC
-- 场景: 同步多个房间的消息
CREATE INDEX IF NOT EXISTS idx_events_batch_rooms 
    ON events(room_id, origin_server_ts DESC);

-- 索引9: 增量同步批量查询
-- 查询模式: WHERE room_id = ANY($1) AND origin_server_ts > $2
-- 场景: 批量增量同步
CREATE INDEX IF NOT EXISTS idx_events_batch_since 
    ON events(room_id, origin_server_ts) 
    WHERE origin_server_ts > 0;

-- =============================================================================
-- 第二部分: users 表索引
-- =============================================================================

-- 索引1: 用户名查询 (已存在 idx_users_username，确保存在)
-- 查询模式: WHERE username = $1
-- 场景: 登录验证、用户查找
CREATE INDEX IF NOT EXISTS idx_users_username 
    ON users(username);

-- 索引2: 用户ID查询 (主键已覆盖，但添加用于批量查询优化)
-- 查询模式: WHERE user_id = ANY($1)
-- 场景: 批量获取用户信息
CREATE INDEX IF NOT EXISTS idx_users_id_batch 
    ON users(user_id);

-- 索引3: 用户搜索索引 (支持 ILIKE 搜索)
-- 查询模式: WHERE username ILIKE $1 OR user_id ILIKE $1 OR displayname ILIKE $1
-- 场景: 用户搜索功能
-- 注意: 使用 pg_trgm 扩展支持模糊搜索
CREATE INDEX IF NOT EXISTS idx_users_username_trgm ON users USING gin(username gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_users_userid_trgm ON users USING gin(user_id gin_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_users_displayname_trgm ON users USING gin(COALESCE(displayname, '') gin_trgm_ops);

-- 索引4: 活跃用户过滤
-- 查询模式: WHERE is_deactivated = FALSE
-- 场景: 过滤已停用用户
CREATE INDEX IF NOT EXISTS idx_users_active 
    ON users(user_id) 
    WHERE COALESCE(is_deactivated, FALSE) = FALSE;

-- 索引5: 用户创建时间排序
-- 查询模式: ORDER BY creation_ts DESC
-- 场景: 用户列表分页
CREATE INDEX IF NOT EXISTS idx_users_creation_ts 
    ON users(creation_ts DESC);

-- 索引6: 用户邮箱查询
-- 查询模式: WHERE email = $1
-- 场景: 邮箱验证、找回密码
CREATE INDEX IF NOT EXISTS idx_users_email 
    ON users(email) 
    WHERE email IS NOT NULL;

-- =============================================================================
-- 第三部分: room_members 表索引
-- =============================================================================

-- 索引1: 房间成员主键查询 (唯一约束已覆盖)
-- 查询模式: WHERE room_id = $1 AND user_id = $2
-- 场景: 获取特定成员信息

-- 索引2: 房间成员按状态查询 (高频)
-- 查询模式: WHERE room_id = $1 AND membership = $2
-- 场景: 获取房间加入成员、邀请列表
CREATE INDEX IF NOT EXISTS idx_room_members_room_status 
    ON room_members(room_id, membership);

-- 索引3: 用户加入的房间 (高频)
-- 查询模式: WHERE user_id = $1 AND membership = 'join'
-- 场景: 获取用户房间列表
CREATE INDEX IF NOT EXISTS idx_room_members_user_joined 
    ON room_members(user_id, room_id) 
    WHERE membership = 'join';

-- 索引4: 成员更新时间排序
-- 查询模式: WHERE room_id = $1 ORDER BY updated_ts DESC
-- 场景: 获取成员变更历史
CREATE INDEX IF NOT EXISTS idx_room_members_room_updated 
    ON room_members(room_id, updated_ts DESC);

-- 索引5: 共享房间查询优化 (用于判断两用户是否共享房间)
-- 查询模式: JOIN 查询判断两用户是否在同一房间
-- 场景: 好友关系验证、权限检查
CREATE INDEX IF NOT EXISTS idx_room_members_user_membership 
    ON room_members(user_id, membership, room_id);

-- =============================================================================
-- 第四部分: rooms 表索引
-- =============================================================================

-- 索引1: 公开房间列表 (高频)
-- 查询模式: WHERE is_public = TRUE ORDER BY created_ts DESC
-- 场景: 公开房间目录
CREATE INDEX IF NOT EXISTS idx_rooms_public 
    ON rooms(created_ts DESC) 
    WHERE is_public = TRUE;

-- 索引2: 房间创建者查询 (已存在 idx_rooms_creator)
-- 查询模式: WHERE creator = $1
-- 场景: 获取用户创建的房间

-- 索引3: 房间版本查询
-- 查询模式: WHERE room_version = $1
-- 场景: 按版本筛选房间
CREATE INDEX IF NOT EXISTS idx_rooms_version 
    ON rooms(room_version);

-- =============================================================================
-- 第五部分: devices 表索引
-- =============================================================================

-- 索引1: 用户设备列表 (高频)
-- 查询模式: WHERE user_id = $1 ORDER BY last_seen_ts DESC
-- 场景: 获取用户设备列表
CREATE INDEX IF NOT EXISTS idx_devices_user_last_seen 
    ON devices(user_id, last_seen_ts DESC);

-- 索引2: 设备主键查询 (复合主键已覆盖)
-- 查询模式: WHERE device_id = $1 AND user_id = $2
-- 场景: 获取特定设备

-- 索引3: 活跃设备查询
-- 查询模式: WHERE last_seen_ts > $1
-- 场景: 获取最近活跃的设备
CREATE INDEX IF NOT EXISTS idx_devices_active 
    ON devices(last_seen_ts DESC) 
    WHERE last_seen_ts IS NOT NULL;

-- =============================================================================
-- 第六部分: access_tokens 表索引
-- =============================================================================

-- 索引1: 令牌验证 (高频)
-- 查询模式: WHERE token = $1
-- 场景: 每次API请求验证
CREATE INDEX IF NOT EXISTS idx_access_tokens_token 
    ON access_tokens(token);

-- 索引2: 用户令牌列表
-- 查询模式: WHERE user_id = $1
-- 场景: 获取用户所有令牌
CREATE INDEX IF NOT EXISTS idx_access_tokens_user 
    ON access_tokens(user_id);

-- 索引3: 有效令牌查询
-- 查询模式: WHERE user_id = $1 AND is_valid = TRUE
-- 场景: 获取用户有效令牌
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_valid 
    ON access_tokens(user_id, token) 
    WHERE is_valid = TRUE;

-- 索引4: 令牌过期清理
-- 查询模式: WHERE expires_ts < $1
-- 场景: 清理过期令牌
CREATE INDEX IF NOT EXISTS idx_access_tokens_expires 
    ON access_tokens(expires_ts) 
    WHERE expires_ts IS NOT NULL;

-- =============================================================================
-- 第七部分: refresh_tokens 表索引
-- =============================================================================

-- 索引1: 刷新令牌哈希查询 (高频)
-- 查询模式: WHERE token_hash = $1
-- 场景: 刷新令牌验证
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_hash 
    ON refresh_tokens(token_hash);

-- 索引2: 用户刷新令牌列表
-- 查询模式: WHERE user_id = $1
-- 场景: 获取用户刷新令牌
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user 
    ON refresh_tokens(user_id);

-- 索引3: 有效刷新令牌
-- 查询模式: WHERE user_id = $1 AND is_revoked = FALSE
-- 场景: 获取用户有效刷新令牌
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_valid 
    ON refresh_tokens(user_id, token_hash) 
    WHERE is_revoked = FALSE;

-- =============================================================================
-- 第八部分: push_notification_queue 表索引
-- =============================================================================

-- 索引1: 待发送推送队列
-- 查询模式: WHERE status = 'pending' ORDER BY next_attempt_at
-- 场景: 推送发送队列处理
CREATE INDEX IF NOT EXISTS idx_push_queue_pending 
    ON push_notification_queue(status, next_attempt_at) 
    WHERE status = 'pending';

-- 索引2: 用户推送队列
-- 查询模式: WHERE user_id = $1 AND device_id = $2
-- 场景: 获取用户设备推送队列
CREATE INDEX IF NOT EXISTS idx_push_queue_user_device 
    ON push_notification_queue(user_id, device_id);

-- =============================================================================
-- 第九部分: pushers 表索引
-- =============================================================================

-- 索引1: 用户推送器列表
-- 查询模式: WHERE user_id = $1
-- 场景: 获取用户推送配置
CREATE INDEX IF NOT EXISTS idx_pushers_user 
    ON pushers(user_id);

-- 索引2: 启用的推送器
-- 查询模式: WHERE user_id = $1 AND is_enabled = TRUE
-- 场景: 获取用户启用的推送器
CREATE INDEX IF NOT EXISTS idx_pushers_user_enabled 
    ON pushers(user_id, pushkey) 
    WHERE is_enabled = TRUE;

-- =============================================================================
-- 第十部分: event_reports 表索引
-- =============================================================================

-- 索引1: 事件举报查询
-- 查询模式: WHERE event_id = $1 ORDER BY received_ts DESC
-- 场景: 获取事件的举报记录
CREATE INDEX IF NOT EXISTS idx_event_reports_event_ts 
    ON event_reports(event_id, received_ts DESC);

-- 索引2: 房间举报列表
-- 查询模式: WHERE room_id = $1 AND status = 'open'
-- 场景: 获取房间待处理举报
CREATE INDEX IF NOT EXISTS idx_event_reports_room_status 
    ON event_reports(room_id, status) 
    WHERE status = 'open';

-- 索引3: 举报状态统计
-- 查询模式: WHERE status = $1
-- 场景: 按状态统计举报
CREATE INDEX IF NOT EXISTS idx_event_reports_status 
    ON event_reports(status, received_ts DESC);

-- =============================================================================
-- 第十一部分: thread_roots 和 thread_replies 表索引
-- =============================================================================

-- 索引1: 房间线程列表
-- 查询模式: WHERE room_id = $1 ORDER BY last_reply_ts DESC
-- 场景: 获取房间线程列表
CREATE INDEX IF NOT EXISTS idx_thread_rooms_room 
    ON thread_roots(room_id, last_reply_ts DESC);

-- 索引2: 线程回复列表
-- 查询模式: WHERE thread_id = $1 ORDER BY created_ts ASC
-- 场景: 获取线程回复
CREATE INDEX IF NOT EXISTS idx_thread_replies_thread_ts 
    ON thread_replies(thread_id, created_ts ASC);

-- =============================================================================
-- 第十二部分: media_repository 表索引
-- =============================================================================

-- 索引1: 用户媒体列表
-- 查询模式: WHERE user_id = $1 ORDER BY created_ts DESC
-- 场景: 获取用户上传的媒体
CREATE INDEX IF NOT EXISTS idx_media_user 
    ON media_repository(user_id, created_ts DESC);

-- 索引2: 媒体来源查询
-- 查询模式: WHERE media_origin = $1
-- 场景: 按来源筛选媒体
CREATE INDEX IF NOT EXISTS idx_media_origin 
    ON media_repository(media_origin);

-- 索引3: 隔离媒体查询
-- 查询模式: WHERE is_quarantined = TRUE
-- 场景: 获取隔离媒体
CREATE INDEX IF NOT EXISTS idx_media_quarantined 
    ON media_repository(created_ts DESC) 
    WHERE is_quarantined = TRUE;

-- =============================================================================
-- 第十三部分: federation_signing_keys 表索引
-- =============================================================================

-- 索引1: 服务器签名密钥
-- 查询模式: WHERE server_name = $1 AND expires_at > $2
-- 场景: 获取有效的联邦签名密钥
CREATE INDEX IF NOT EXISTS idx_federation_keys_valid 
    ON federation_signing_keys(server_name, expires_at DESC);

-- =============================================================================
-- 第十四部分: registration_tokens 表索引
-- =============================================================================

-- 索引1: 有效注册令牌
-- 查询模式: WHERE token = $1 AND is_enabled = TRUE
-- 场景: 验证注册令牌
CREATE INDEX IF NOT EXISTS idx_registration_tokens_valid 
    ON registration_tokens(token) 
    WHERE is_enabled = TRUE;

-- 索引2: 未过期的注册令牌
-- 查询模式: WHERE is_enabled = TRUE AND (expires_at IS NULL OR expires_at > $1)
-- 场景: 获取有效注册令牌列表
CREATE INDEX IF NOT EXISTS idx_registration_tokens_active 
    ON registration_tokens(created_ts DESC) 
    WHERE is_enabled = TRUE;

-- =============================================================================
-- 第十五部分: security_events 表索引
-- =============================================================================

-- 索引1: 用户安全事件
-- 查询模式: WHERE user_id = $1 ORDER BY created_at DESC
-- 场景: 获取用户安全事件日志
CREATE INDEX IF NOT EXISTS idx_security_events_user_ts 
    ON security_events(user_id, created_at DESC);

-- 索引2: 安全事件类型统计
-- 查询模式: WHERE event_type = $1 AND created_at > $2
-- 场景: 按类型统计安全事件
CREATE INDEX IF NOT EXISTS idx_security_events_type_ts 
    ON security_events(event_type, created_at DESC);

-- =============================================================================
-- 第十六部分: ip_reputation 和 ip_blocks 表索引
-- =============================================================================

-- 索引1: 封禁IP查询
-- 查询模式: WHERE is_blocked = TRUE AND (blocked_until_ts IS NULL OR blocked_until_ts > $1)
-- 场景: 检查IP是否被封禁
CREATE INDEX IF NOT EXISTS idx_ip_reputation_blocked 
    ON ip_reputation(ip_address) 
    WHERE is_blocked = TRUE;

-- 索引2: 启用的IP封禁
-- 查询模式: WHERE is_enabled = TRUE
-- 场景: 获取所有启用的IP封禁规则
CREATE INDEX IF NOT EXISTS idx_ip_blocks_enabled 
    ON ip_blocks(ip_address) 
    WHERE is_enabled = TRUE;

-- =============================================================================
-- 第十七部分: saml_sessions 和 cas_tickets 表索引
-- =============================================================================

-- 索引1: 活跃SAML会话
-- 查询模式: WHERE session_id = $1 AND expires_at > $2
-- 场景: 验证SAML会话
CREATE INDEX IF NOT EXISTS idx_saml_sessions_valid 
    ON saml_sessions(session_id, expires_at) 
    WHERE status = 'active';

-- 索引2: 活跃CAS票据
-- 查询模式: WHERE ticket = $1 AND expires_at > $2 AND used_at IS NULL
-- 场景: 验证CAS票据
CREATE INDEX IF NOT EXISTS idx_cas_tickets_valid 
    ON cas_tickets(ticket, expires_at) 
    WHERE used_at IS NULL;

-- =============================================================================
-- 第十八部分: voice_messages 表索引
-- =============================================================================

-- 索引1: 房间语音消息
-- 查询模式: WHERE room_id = $1 ORDER BY created_ts DESC
-- 场景: 获取房间语音消息
CREATE INDEX IF NOT EXISTS idx_voice_messages_room_ts 
    ON voice_messages(room_id, created_ts DESC);

-- 索引2: 用户语音消息
-- 查询模式: WHERE user_id = $1 ORDER BY created_ts DESC
-- 场景: 获取用户语音消息
CREATE INDEX IF NOT EXISTS idx_voice_messages_user_ts 
    ON voice_messages(user_id, created_ts DESC);

-- 索引3: 未处理的语音消息
-- 查询模式: WHERE is_processed = FALSE
-- 场景: 获取待处理语音消息
CREATE INDEX IF NOT EXISTS idx_voice_messages_pending 
    ON voice_messages(created_ts) 
    WHERE is_processed = FALSE;

-- =============================================================================
-- 记录迁移
-- =============================================================================

INSERT INTO schema_migrations (version, description, success)
VALUES ('20260227000000', 'Add performance indexes for high-frequency queries', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE, executed_at = NOW();

-- =============================================================================
-- 验证索引创建
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
    RAISE NOTICE 'Performance indexes migration completed!';
    RAISE NOTICE 'Total custom indexes created: %', index_count;
    RAISE NOTICE '==========================================';
END $$;
