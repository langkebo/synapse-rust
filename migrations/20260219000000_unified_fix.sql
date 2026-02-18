-- =============================================================================
-- Synapse-Rust 数据库统一迁移脚本
-- 版本: 20260219000000
-- 描述: 统一修复所有数据库架构问题
-- 问题来源: api-error.md 测试记录 + 数据库初始化日志分析
-- 注意: 不使用 BEGIN/COMMIT，因为应用程序按语句分割执行
-- =============================================================================

-- =============================================================================
-- 第一部分: 修复 voice_messages 表
-- 问题: #V1-#V4 - 缺少 processed_ts, mime_type, encryption 列
-- =============================================================================

ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS processed_ts BIGINT;
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS mime_type VARCHAR(100);
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS encryption JSONB;
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS waveform_data TEXT;
ALTER TABLE voice_messages ADD COLUMN IF NOT EXISTS transcribe_text TEXT;

UPDATE voice_messages SET processed_ts = created_ts WHERE processed_ts IS NULL AND created_ts IS NOT NULL;

-- =============================================================================
-- 第二部分: 创建 pushers 表
-- 问题: #P1, #P2 - 缺少 pushers 表
-- =============================================================================

CREATE TABLE IF NOT EXISTS pushers (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
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
    created_at BIGINT,
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
-- 第三部分: 创建 push_rules 表
-- 问题: #P3-#P8 - 缺少 push_rules 表
-- =============================================================================

CREATE TABLE IF NOT EXISTS push_rules (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    rule_id VARCHAR(255) NOT NULL,
    scope VARCHAR(50) NOT NULL DEFAULT 'global',
    kind VARCHAR(50) NOT NULL,
    priority INTEGER DEFAULT 0,
    pattern VARCHAR(255),
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

INSERT INTO push_rules (user_id, rule_id, scope, kind, priority, conditions, actions, enabled, is_default)
VALUES 
    ('.default', '.m.rule.master', 'global', 'override', 0, '[]'::jsonb, '["dont_notify"]'::jsonb, true, true),
    ('.default', '.m.rule.suppress_notices', 'global', 'override', 1, '[{"kind": "event_match", "key": "content.msgtype", "pattern": "m.notice"}]'::jsonb, '["dont_notify"]'::jsonb, true, true),
    ('.default', '.m.rule.invite_for_me', 'global', 'override', 2, '[{"kind": "event_match", "key": "type", "pattern": "m.room.member"}, {"kind": "event_match", "key": "content.membership", "pattern": "invite"}, {"kind": "event_match", "key": "state_key", "pattern": "_self"}]'::jsonb, '["notify", {"set_tweak": "sound", "value": "default"}]'::jsonb, true, true),
    ('.default', '.m.rule.member_event', 'global', 'override', 3, '[{"kind": "event_match", "key": "type", "pattern": "m.room.member"}]'::jsonb, '["dont_notify"]'::jsonb, true, true),
    ('.default', '.m.rule.contains_display_name', 'global', 'content', 4, '[{"kind": "contains_display_name"}]'::jsonb, '["notify", {"set_tweak": "sound", "value": "default"}, {"set_tweak": "highlight", "value": true}]'::jsonb, true, true),
    ('.default', '.m.rule.tombstone', 'global', 'override', 5, '[{"kind": "event_match", "key": "type", "pattern": "m.room.tombstone"}, {"kind": "event_match", "key": "state_key", "pattern": ""}]'::jsonb, '["notify", {"set_tweak": "highlight", "value": true}]'::jsonb, true, true),
    ('.default', '.m.rule.room_notif', 'global', 'content', 6, '[{"kind": "event_match", "key": "content.body", "pattern": "@room"}]'::jsonb, '["notify", {"set_tweak": "highlight", "value": true}]'::jsonb, true, true),
    ('.default', '.m.rule.message', 'global', 'underride', 7, '[{"kind": "event_match", "key": "type", "pattern": "m.room.message"}]'::jsonb, '["notify", {"set_tweak": "sound", "value": "default"}]'::jsonb, true, true),
    ('.default', '.m.rule.encrypted', 'global', 'underride', 8, '[{"kind": "event_match", "key": "type", "pattern": "m.room.encrypted"}]'::jsonb, '["notify", {"set_tweak": "sound", "value": "default"}]'::jsonb, true, true)
ON CONFLICT (user_id, scope, kind, rule_id) DO NOTHING;

-- =============================================================================
-- 第四部分: 创建 room_members 表
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

INSERT INTO room_members (room_id, user_id, membership, displayname, avatar_url, event_id, created_ts)
SELECT DISTINCT ON (room_id, user_id) 
    room_id, user_id, membership, displayname, avatar_url, event_id, joined_ts
FROM room_memberships
WHERE NOT EXISTS (
    SELECT 1 FROM room_members rm WHERE rm.room_id = room_memberships.room_id AND rm.user_id = room_memberships.user_id
)
ON CONFLICT (room_id, user_id) DO NOTHING;

-- =============================================================================
-- 第五部分: 修复 events 表
-- 问题: #S1 - events 表缺少 type 列
-- =============================================================================

ALTER TABLE events ADD COLUMN IF NOT EXISTS type VARCHAR(255);
UPDATE events SET type = event_type WHERE type IS NULL AND event_type IS NOT NULL;

-- =============================================================================
-- 第六部分: 修复 rooms 表
-- 问题: #S3 - rooms 表缺少 join_rules 和 guest_access 列
-- =============================================================================

ALTER TABLE rooms ADD COLUMN IF NOT EXISTS join_rules VARCHAR(50) DEFAULT 'invite';
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS join_rules_event_id VARCHAR(255);
ALTER TABLE rooms ADD COLUMN IF NOT EXISTS guest_access VARCHAR(50) DEFAULT 'forbidden';

-- =============================================================================
-- 第七部分: 修复 refresh_tokens 表
-- 问题: 迁移脚本引用不存在的列
-- =============================================================================

ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS token_hash VARCHAR(255);
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS expires_at BIGINT;
ALTER TABLE refresh_tokens ADD COLUMN IF NOT EXISTS is_revoked BOOLEAN DEFAULT FALSE;

UPDATE refresh_tokens SET token_hash = token WHERE token_hash IS NULL;
UPDATE refresh_tokens SET expires_at = expires_ts WHERE expires_at IS NULL AND expires_ts IS NOT NULL;

-- =============================================================================
-- 第八部分: 修复 event_reports 表
-- 问题: 迁移脚本引用不存在的列
-- =============================================================================

ALTER TABLE event_reports ADD COLUMN IF NOT EXISTS reporter_user_id VARCHAR(255);
ALTER TABLE event_reports ADD COLUMN IF NOT EXISTS reported_user_id VARCHAR(255);
ALTER TABLE event_reports ADD COLUMN IF NOT EXISTS status VARCHAR(50) DEFAULT 'open';
ALTER TABLE event_reports ADD COLUMN IF NOT EXISTS received_ts BIGINT;

-- =============================================================================
-- 第九部分: 修复 ip_blocks 表
-- 问题: 迁移脚本引用不存在的列
-- =============================================================================

ALTER TABLE ip_blocks ADD COLUMN IF NOT EXISTS ip_address VARCHAR(45);

-- =============================================================================
-- 第九部分: 修复 room_summaries 表 (如果存在)
-- 问题: 迁移脚本引用不存在的列
-- =============================================================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'room_summaries') THEN
        ALTER TABLE room_summaries ADD COLUMN IF NOT EXISTS stream_id BIGSERIAL;
    END IF;
END $$;

-- =============================================================================
-- 第十一部分: 修复 registration_tokens 表 (如果存在)
-- 问题: 迁移脚本引用不存在的列
-- =============================================================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'registration_tokens') THEN
        ALTER TABLE registration_tokens ADD COLUMN IF NOT EXISTS remaining_uses INTEGER DEFAULT 1;
    END IF;
END $$;

-- =============================================================================
-- 第十一部分: 验证迁移 (宽松模式)
-- =============================================================================

DO $$
DECLARE
    col_count INTEGER;
    table_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO col_count
    FROM information_schema.columns 
    WHERE table_name = 'voice_messages'
    AND column_name IN ('processed_ts', 'mime_type', 'encryption');
    
    SELECT COUNT(*) INTO table_count
    FROM information_schema.tables 
    WHERE table_name IN ('pushers', 'push_rules', 'room_members');
    
    RAISE NOTICE 'Migration validation: voice_messages columns=%, required tables=%', col_count, table_count;
END $$;

-- =============================================================================
-- 第十二部分: 记录迁移版本
-- =============================================================================

INSERT INTO schema_migrations (version, success, executed_at)
VALUES ('20260219000000', true, NOW())
ON CONFLICT (version) DO NOTHING;
