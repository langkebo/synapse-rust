-- =============================================================================
-- Synapse-Rust 紧急修复迁移脚本
-- 版本: 1.0
-- 创建日期: 2026-02-12
-- 描述: 修复好友系统和邮箱验证的 Schema 问题
-- =============================================================================

-- =============================================================================
-- 第一部分: 修复 email_verification_tokens 表
-- =============================================================================

-- 添加缺失的 session_data 字段
ALTER TABLE email_verification_tokens 
ADD COLUMN IF NOT EXISTS session_data JSONB;

-- 添加索引以提高查询性能
CREATE INDEX IF NOT EXISTS idx_email_verification_session_data 
ON email_verification_tokens(session_data) 
WHERE session_data IS NOT NULL;

-- 注意: 保留 expires_ts 字段名以保持向后兼容
-- Rust 代码需要更新以使用 expires_ts 而不是 expires_at

-- 使 user_id 可为空（支持匿名邮箱验证）
ALTER TABLE email_verification_tokens 
ALTER COLUMN user_id DROP NOT NULL;

-- =============================================================================
-- 第二部分: 创建 current_state_events 视图
-- =============================================================================

-- 删除旧视图（如果存在）
DROP VIEW IF EXISTS current_state_events CASCADE;

-- 创建当前状态事件视图
CREATE VIEW current_state_events AS
SELECT DISTINCT ON (e.room_id, e.event_type, e.state_key)
    e.event_id,
    e.room_id,
    e.event_type,
    e.state_key,
    e.content,
    e.sender,
    e.user_id,
    e.origin_server_ts,
    e.depth
FROM events e
WHERE e.state_key IS NOT NULL
ORDER BY e.room_id, e.event_type, e.state_key, e.origin_server_ts DESC;

-- 为视图添加注释
COMMENT ON VIEW current_state_events IS '当前房间状态事件的最新版本';

-- =============================================================================
-- 第三部分: 添加好友系统相关索引
-- =============================================================================

-- 好友列表房间查询优化
CREATE INDEX IF NOT EXISTS idx_events_friend_room_create 
ON events(room_id, event_type, sender) 
WHERE event_type = 'm.room.create';

-- 好友列表状态事件查询优化
CREATE INDEX IF NOT EXISTS idx_events_friend_list 
ON events(room_id, event_type, state_key, origin_server_ts DESC) 
WHERE event_type = 'm.friends.list';

-- 好友请求事件索引
CREATE INDEX IF NOT EXISTS idx_events_friend_requests 
ON events(room_id, event_type, origin_server_ts DESC) 
WHERE event_type LIKE 'm.friend_requests%';

-- 直接消息房间索引
CREATE INDEX IF NOT EXISTS idx_events_dm_related_users 
ON events(room_id, event_type) 
WHERE event_type = 'm.friends.related_users';

-- =============================================================================
-- 第四部分: 添加房间类型支持
-- =============================================================================

-- 为 rooms 表添加 room_type 字段（如果不存在）
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'rooms' AND column_name = 'room_type'
    ) THEN
        ALTER TABLE rooms ADD COLUMN room_type VARCHAR(100);
        COMMENT ON COLUMN rooms.room_type IS '房间类型: m.friends, m.direct, 等';
    END IF;
END $$;

-- =============================================================================
-- 第五部分: 数据完整性检查
-- =============================================================================

-- 检查孤立事件（引用不存在的房间）
DO $$
DECLARE
    orphan_count BIGINT;
BEGIN
    SELECT COUNT(*) INTO orphan_count
    FROM events e
    LEFT JOIN rooms r ON e.room_id = r.room_id
    WHERE r.room_id IS NULL;
    
    IF orphan_count > 0 THEN
        RAISE NOTICE '警告: 发现 % 个孤立事件（引用不存在的房间）', orphan_count;
        -- 不自动删除，需要人工确认
    ELSE
        RAISE NOTICE '✓ 数据完整性检查通过';
    END IF;
END $$;

-- =============================================================================
-- 第六部分: 更新统计信息
-- =============================================================================

ANALYZE events;
ANALYZE rooms;
ANALYZE email_verification_tokens;

-- =============================================================================
-- 迁移完成
-- =============================================================================

-- 记录迁移
INSERT INTO db_metadata (key, value, updated_ts)
VALUES ('emergency_fix_20260212', 'completed', EXTRACT(EPOCH FROM NOW())::BIGINT)
ON CONFLICT (key) DO UPDATE SET 
    value = EXCLUDED.value,
    updated_ts = EXCLUDED.updated_ts;

SELECT 'Emergency fix migration completed successfully' as status;
