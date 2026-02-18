-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260218000001
-- 描述: 统一修复所有API测试发现的问题
-- 问题来源: api-error.md 测试记录
-- =============================================================================

BEGIN;

-- =============================================================================
-- 第一部分: 修复语音消息表 (voice_messages)
-- 问题: #V1-#V4 - 缺少 processed_ts, mime_type, encryption 列
-- =============================================================================

-- 添加 processed_ts 列 (代码使用此名称而非 processed_at)
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS processed_ts BIGINT;

-- 添加 mime_type 列
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS mime_type VARCHAR(100);

-- 添加 encryption 列
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS encryption JSONB;

-- 添加 waveform_data 列 (如果不存在)
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS waveform_data TEXT;

-- 添加 transcribe_text 列 (如果不存在)
ALTER TABLE voice_messages 
ADD COLUMN IF NOT EXISTS transcribe_text TEXT;

-- 数据迁移: 从 processed_at 复制到 processed_ts
UPDATE voice_messages 
SET processed_ts = processed_at 
WHERE processed_ts IS NULL AND processed_at IS NOT NULL;

-- =============================================================================
-- 第二部分: 创建推送器表 (pushers)
-- 问题: #P1, #P2 - 缺少 pushers 表
-- =============================================================================

CREATE TABLE IF NOT EXISTS pushers (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    pushkey TEXT NOT NULL,
    kind VARCHAR(50) NOT NULL DEFAULT 'http',
    app_id VARCHAR(255) NOT NULL,
    app_display_name VARCHAR(255),
    device_display_name VARCHAR(255),
    profile_tag VARCHAR(255),
    lang VARCHAR(20) DEFAULT 'en',
    data JSONB DEFAULT '{}',
    enabled BOOLEAN DEFAULT true,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    last_updated_ts BIGINT,
    last_success_ts BIGINT,
    last_failure_ts BIGINT,
    failure_count INTEGER DEFAULT 0,
    CONSTRAINT pushers_user_pushkey_unique UNIQUE(user_id, pushkey)
);

CREATE INDEX IF NOT EXISTS idx_pushers_user ON pushers(user_id);
CREATE INDEX IF NOT EXISTS idx_pushers_kind ON pushers(kind);
CREATE INDEX IF NOT EXISTS idx_pushers_enabled ON pushers(enabled);

-- =============================================================================
-- 第三部分: 创建推送规则表 (push_rules)
-- 问题: #P3-#P8 - 缺少 push_rules 表
-- =============================================================================

CREATE TABLE IF NOT EXISTS push_rules (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    rule_id VARCHAR(255) NOT NULL,
    scope VARCHAR(50) NOT NULL DEFAULT 'global',
    kind VARCHAR(50) NOT NULL,
    priority INTEGER DEFAULT 0,
    conditions JSONB DEFAULT '[]',
    actions JSONB DEFAULT '[]',
    enabled BOOLEAN DEFAULT true,
    is_default BOOLEAN DEFAULT false,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT,
    CONSTRAINT push_rules_user_rule_unique UNIQUE(user_id, scope, kind, rule_id),
    CONSTRAINT push_rules_scope_check CHECK (scope IN ('global', 'device')),
    CONSTRAINT push_rules_kind_check CHECK (kind IN ('override', 'content', 'room', 'sender', 'underride'))
);

CREATE INDEX IF NOT EXISTS idx_push_rules_user ON push_rules(user_id);
CREATE INDEX IF NOT EXISTS idx_push_rules_scope ON push_rules(scope);
CREATE INDEX IF NOT EXISTS idx_push_rules_kind ON push_rules(kind);
CREATE INDEX IF NOT EXISTS idx_push_rules_enabled ON push_rules(enabled);

-- 插入默认推送规则
INSERT INTO push_rules (user_id, rule_id, scope, kind, priority, conditions, actions, enabled, is_default)
VALUES 
    ('.default', '.m.rule.master', 'global', 'override', 0, '[]', '["dont_notify"]', true, true),
    ('.default', '.m.rule.suppress_notices', 'global', 'override', 1, '[{"kind": "event_match", "key": "content.msgtype", "pattern": "m.notice"}]', '["dont_notify"]', true, true),
    ('.default', '.m.rule.invite_for_me', 'global', 'override', 2, '[{"kind": "event_match", "key": "type", "pattern": "m.room.member"}, {"kind": "event_match", "key": "content.membership", "pattern": "invite"}, {"kind": "event_match", "key": "state_key", "pattern": "_self"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true),
    ('.default', '.m.rule.member_event', 'global', 'override', 3, '[{"kind": "event_match", "key": "type", "pattern": "m.room.member"}]', '["dont_notify"]', true, true),
    ('.default', '.m.rule.contains_display_name', 'global', 'content', 4, '[{"kind": "contains_display_name"}]', '["notify", {"set_tweak": "sound", "value": "default"}, {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', '.m.rule.tombstone', 'global', 'override', 5, '[{"kind": "event_match", "key": "type", "pattern": "m.room.tombstone"}, {"kind": "event_match", "key": "state_key", "pattern": ""}]', '["notify", {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', '.m.rule.room_notif', 'global', 'content', 6, '[{"kind": "event_match", "key": "content.body", "pattern": "@room"}]', '["notify", {"set_tweak": "highlight", "value": true}]', true, true),
    ('.default', '.m.rule.message', 'global', 'underride', 7, '[{"kind": "event_match", "key": "type", "pattern": "m.room.message"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true),
    ('.default', '.m.rule.encrypted', 'global', 'underride', 8, '[{"kind": "event_match", "key": "type", "pattern": "m.room.encrypted"}]', '["notify", {"set_tweak": "sound", "value": "default"}]', true, true)
ON CONFLICT (user_id, scope, kind, rule_id) DO NOTHING;

-- =============================================================================
-- 第四部分: 创建房间成员表 (room_members)
-- 问题: #S4 - 缺少 room_members 表
-- =============================================================================

CREATE TABLE IF NOT EXISTS room_members (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    membership VARCHAR(50) NOT NULL DEFAULT 'join',
    displayname VARCHAR(255),
    avatar_url VARCHAR(512),
    reason TEXT,
    inviter_id VARCHAR(255),
    event_id VARCHAR(255),
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT,
    CONSTRAINT room_members_room_user_unique UNIQUE(room_id, user_id),
    CONSTRAINT room_members_membership_check CHECK (membership IN ('invite', 'join', 'knock', 'leave', 'ban'))
);

CREATE INDEX IF NOT EXISTS idx_room_members_room ON room_members(room_id);
CREATE INDEX IF NOT EXISTS idx_room_members_user ON room_members(user_id);
CREATE INDEX IF NOT EXISTS idx_room_members_membership ON room_members(membership);

-- 从 room_memberships 迁移数据到 room_members
INSERT INTO room_members (room_id, user_id, membership, displayname, avatar_url, event_id, created_ts)
SELECT DISTINCT ON (room_id, user_id) 
    room_id, user_id, membership, displayname, avatar_url, event_id, created_ts
FROM room_memberships
WHERE NOT EXISTS (
    SELECT 1 FROM room_members rm WHERE rm.room_id = room_memberships.room_id AND rm.user_id = room_memberships.user_id
)
ON CONFLICT (room_id, user_id) DO NOTHING;

-- =============================================================================
-- 第五部分: 修复搜索相关表
-- 问题: #S1 - events 表缺少 type 列
-- =============================================================================

-- 确保 events 表有 type 列 (event_type 的别名)
ALTER TABLE events ADD COLUMN IF NOT EXISTS type VARCHAR(255);
UPDATE events SET type = event_type WHERE type IS NULL AND event_type IS NOT NULL;

-- =============================================================================
-- 第六部分: 修复房间层级相关表
-- 问题: #S3 - rooms 表缺少 join_rules 列
-- =============================================================================

ALTER TABLE rooms ADD COLUMN IF NOT EXISTS join_rules VARCHAR(50) DEFAULT 'invite';
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS join_rules_event_id VARCHAR(255);

-- =============================================================================
-- 第七部分: 验证迁移
-- =============================================================================

DO $$
DECLARE
    col_count INTEGER;
    table_count INTEGER;
BEGIN
    -- 验证 voice_messages 新列
    SELECT COUNT(*) INTO col_count
    FROM information_schema.columns 
    WHERE table_name = 'voice_messages'
    AND column_name IN ('processed_ts', 'mime_type', 'encryption');
    
    IF col_count < 3 THEN
        RAISE EXCEPTION 'voice_messages migration failed: Expected 3 new columns, found %', col_count;
    END IF;
    
    -- 验证 pushers 表存在
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_name = 'pushers';
    
    IF table_count < 1 THEN
        RAISE EXCEPTION 'pushers table not created';
    END IF;
    
    -- 验证 push_rules 表存在
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_name = 'push_rules';
    
    IF table_count < 1 THEN
        RAISE EXCEPTION 'push_rules table not created';
    END IF;
    
    -- 验证 room_members 表存在
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_name = 'room_members';
    
    IF table_count < 1 THEN
        RAISE EXCEPTION 'room_members table not created';
    END IF;
    
    RAISE NOTICE 'All migrations completed successfully';
END $$;

COMMIT;

-- =============================================================================
-- 迁移完成
-- =============================================================================
