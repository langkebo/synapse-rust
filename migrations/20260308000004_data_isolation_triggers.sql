-- ============================================================================
-- Migration: Data isolation and cleanup triggers
-- Version: 20260308000004
-- Description: 建立数据隔离机制，确保级联删除正确工作
-- ============================================================================

-- ============================================================================
-- 创建清理函数
-- ============================================================================

-- 清理用户关联数据的函数
CREATE OR REPLACE FUNCTION cleanup_user_data(p_user_id TEXT)
RETURNS void AS $$
BEGIN
    -- 删除用户的推送设备
    DELETE FROM push_devices WHERE user_id = p_user_id;
    
    -- 删除用户的推送规则
    DELETE FROM push_rules WHERE user_id = p_user_id;
    
    -- 删除用户的过滤器
    DELETE FROM filters WHERE user_id = p_user_id;
    
    -- 删除用户的账户数据
    DELETE FROM account_data WHERE user_id = p_user_id;
    
    -- 删除用户的 OpenID 令牌
    DELETE FROM openid_tokens WHERE user_id = p_user_id;
    
    -- 删除用户的第三方身份
    DELETE FROM user_threepids WHERE user_id = p_user_id;
    
    -- 删除用户的脱水设备
    DELETE FROM dehydrated_devices WHERE user_id = p_user_id;
    
    -- 删除用户的好友关系
    DELETE FROM friends WHERE user_id = p_user_id OR friend_id = p_user_id;
    
    -- 删除用户的好友请求
    DELETE FROM friend_requests WHERE sender_id = p_user_id OR receiver_id = p_user_id;
    
    -- 删除用户的屏蔽列表
    DELETE FROM blocked_users WHERE user_id = p_user_id OR blocked_id = p_user_id;
    
    -- 删除用户的 Presence 状态
    DELETE FROM presence WHERE user_id = p_user_id;
    
    -- 删除用户的房间成员关系
    DELETE FROM room_memberships WHERE user_id = p_user_id;
    
    -- 删除用户的设备
    DELETE FROM devices WHERE user_id = p_user_id;
    
    -- 删除用户的访问令牌
    DELETE FROM access_tokens WHERE user_id = p_user_id;
    
    -- 删除用户的刷新令牌
    DELETE FROM refresh_tokens WHERE user_id = p_user_id;
    
    -- 删除用户的密钥备份
    DELETE FROM key_backups WHERE user_id = p_user_id;
    
    -- 删除用户的跨签名密钥
    DELETE FROM cross_signing_keys WHERE user_id = p_user_id;
END;
$$ LANGUAGE plpgsql;

-- 清理房间关联数据的函数
CREATE OR REPLACE FUNCTION cleanup_room_data(p_room_id TEXT)
RETURNS void AS $$
BEGIN
    -- 删除房间的成员关系
    DELETE FROM room_memberships WHERE room_id = p_room_id;
    
    -- 删除房间的事件
    DELETE FROM events WHERE room_id = p_room_id;
    
    -- 删除房间的摘要
    DELETE FROM room_summaries WHERE room_id = p_room_id;
    
    -- 删除房间的目录条目
    DELETE FROM room_directory WHERE room_id = p_room_id;
    
    -- 删除房间的别名
    DELETE FROM room_aliases WHERE room_id = p_room_id;
    
    -- 删除房间的线程统计
    DELETE FROM thread_statistics WHERE room_id = p_room_id;
    
    -- 删除房间的 Megolm 会话
    DELETE FROM megolm_sessions WHERE room_id = p_room_id;
    
    -- 删除房间的 MatrixRTC 会话
    DELETE FROM matrixrtc_sessions WHERE room_id = p_room_id;
    
    -- 删除房间的 Beacon 信息
    DELETE FROM beacon_info WHERE room_id = p_room_id;
    
    -- 删除房间的读标记
    DELETE FROM read_markers WHERE room_id = p_room_id;
    
    -- 删除房间的事件接收
    DELETE FROM event_receipts WHERE room_id = p_room_id;
    
    -- 删除房间的状态事件
    DELETE FROM room_state_events WHERE room_id = p_room_id;
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 创建触发器
-- ============================================================================

-- 用户删除触发器
DROP TRIGGER IF EXISTS trigger_cleanup_user_data ON users;

CREATE TRIGGER trigger_cleanup_user_data
    BEFORE DELETE ON users
    FOR EACH ROW
    EXECUTE FUNCTION cleanup_user_data(OLD.user_id);

-- 房间删除触发器
DROP TRIGGER IF EXISTS trigger_cleanup_room_data ON rooms;

CREATE TRIGGER trigger_cleanup_room_data
    BEFORE DELETE ON rooms
    FOR EACH ROW
    EXECUTE FUNCTION cleanup_room_data(OLD.room_id);

-- ============================================================================
-- 创建孤儿数据清理函数
-- ============================================================================

-- 清理孤儿事件的函数
CREATE OR REPLACE FUNCTION cleanup_orphan_events()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    -- 删除没有对应房间的事件
    DELETE FROM events
    WHERE room_id NOT IN (SELECT room_id FROM rooms);
    
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- 清理孤儿成员关系的函数
CREATE OR REPLACE FUNCTION cleanup_orphan_memberships()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    -- 删除没有对应用户或房间的成员关系
    DELETE FROM room_memberships
    WHERE user_id NOT IN (SELECT user_id FROM users)
       OR room_id NOT IN (SELECT room_id FROM rooms);
    
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- 清理孤儿令牌的函数
CREATE OR REPLACE FUNCTION cleanup_orphan_tokens()
RETURNS INTEGER AS $$
DECLARE
    deleted_count INTEGER;
BEGIN
    -- 删除没有对应用户的访问令牌
    DELETE FROM access_tokens
    WHERE user_id NOT IN (SELECT user_id FROM users);
    
    -- 删除没有对应用户的刷新令牌
    DELETE FROM refresh_tokens
    WHERE user_id NOT IN (SELECT user_id FROM users);
    
    GET DIAGNOSTICS deleted_count = ROW_COUNT;
    RETURN deleted_count;
END;
$$ LANGUAGE plpgsql;

-- 综合清理函数
CREATE OR REPLACE FUNCTION cleanup_all_orphan_data()
RETURNS JSONB AS $$
DECLARE
    events_deleted INTEGER;
    memberships_deleted INTEGER;
    tokens_deleted INTEGER;
    total_deleted INTEGER;
BEGIN
    SELECT * FROM cleanup_orphan_events() INTO events_deleted;
    SELECT * FROM cleanup_orphan_memberships() INTO memberships_deleted;
    SELECT * FROM cleanup_orphan_tokens() INTO tokens_deleted;
    
    total_deleted := events_deleted + memberships_deleted + tokens_deleted;
    
    RETURN jsonb_build_object(
        'events_deleted', events_deleted,
        'memberships_deleted', memberships_deleted,
        'tokens_deleted', tokens_deleted,
        'total_deleted', total_deleted,
        'cleaned_at', EXTRACT(EPOCH FROM NOW()) * 1000
    );
END;
$$ LANGUAGE plpgsql;

-- ============================================================================
-- 记录迁移完成
-- ============================================================================

INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES ('20260308000004', 'data_isolation_triggers', 
        EXTRACT(EPOCH FROM NOW()) * 1000, 
        '建立数据隔离机制，确保级联删除正确工作')
ON CONFLICT (version) DO NOTHING;
