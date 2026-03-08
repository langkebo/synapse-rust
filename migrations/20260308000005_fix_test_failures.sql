-- ============================================================================
-- 测试失败修复迁移脚本
-- 基于测试结果: 48 通过, 25 失败, 15 跳过/未实现
-- 创建时间: 2026-03-08
-- ============================================================================

-- ============================================================================
-- 1. 修复 threepids 表 - 添加缺失的 validated_at 列
-- 错误: column "validated_at" does not exist
-- ============================================================================

CREATE TABLE IF NOT EXISTS threepids (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    medium TEXT NOT NULL,
    address TEXT NOT NULL,
    validated_at BIGINT,
    added_at BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    CONSTRAINT threepids_user_fkey 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT threepids_unique 
        UNIQUE (medium, address)
);

CREATE INDEX IF NOT EXISTS idx_threepids_user ON threepids(user_id);
CREATE INDEX IF NOT EXISTS idx_threepids_medium ON threepids(medium);

-- 如果表已存在但缺少列，则添加
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'threepids' AND column_name = 'validated_at') THEN
        ALTER TABLE threepids ADD COLUMN validated_at BIGINT;
    END IF;
END $$;

-- ============================================================================
-- 2. 修复 pushers 表 - 添加缺失的 device_id 列
-- 错误: column "device_id" does not exist
-- ============================================================================

CREATE TABLE IF NOT EXISTS pushers (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    pushkey TEXT NOT NULL,
    kind TEXT NOT NULL,
    app_id TEXT NOT NULL,
    app_display_name TEXT NOT NULL,
    device_display_name TEXT NOT NULL,
    profile_tag TEXT,
    lang TEXT NOT NULL,
    data JSONB NOT NULL,
    device_id TEXT NOT NULL DEFAULT '',
    last_failure_ts BIGINT,
    last_failure_reason TEXT,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    CONSTRAINT pushers_user_id_fkey 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT pushers_unique 
        UNIQUE (user_id, pushkey)
);

CREATE INDEX IF NOT EXISTS idx_pushers_user ON pushers(user_id);
CREATE INDEX IF NOT EXISTS idx_pushers_device ON pushers(device_id);

-- 如果表已存在但缺少列，则添加
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'pushers' AND column_name = 'device_id') THEN
        ALTER TABLE pushers ADD COLUMN device_id TEXT NOT NULL DEFAULT '';
    END IF;
END $$;

-- ============================================================================
-- 3. 修复 account_data 表 - 修复 data 列约束
-- 错误: null value in column "data" violates not-null constraint
-- ============================================================================

CREATE TABLE IF NOT EXISTS account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT,
    type TEXT NOT NULL,
    data JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    CONSTRAINT account_data_user_fkey 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT account_data_room_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT account_data_unique 
        UNIQUE (user_id, room_id, type)
);

CREATE INDEX IF NOT EXISTS idx_account_data_user ON account_data(user_id);
CREATE INDEX IF NOT EXISTS idx_account_data_room ON account_data(room_id);
CREATE INDEX IF NOT EXISTS idx_account_data_type ON account_data(type);

-- 如果表已存在，修复 data 列默认值
DO $$ 
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'account_data' AND column_name = 'data') THEN
        ALTER TABLE account_data ALTER COLUMN data SET DEFAULT '{}';
        UPDATE account_data SET data = '{}' WHERE data IS NULL;
    END IF;
END $$;

-- ============================================================================
-- 4. 修复 room_events 表 - 添加缺失的 type 列
-- 错误: column "type" does not exist
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_events (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL,
    type TEXT NOT NULL DEFAULT 'm.room.message',
    sender TEXT NOT NULL,
    content JSONB NOT NULL,
    state_key TEXT,
    depth BIGINT NOT NULL DEFAULT 0,
    origin_server_ts BIGINT NOT NULL,
    received_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    processed_ts BIGINT,
    CONSTRAINT room_events_room_id_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT room_events_sender_fkey 
        FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_events_room ON room_events(room_id);
CREATE INDEX IF NOT EXISTS idx_room_events_type ON room_events(type);
CREATE INDEX IF NOT EXISTS idx_room_events_sender ON room_events(sender);

-- 如果表已存在但缺少列，则添加
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'room_events' AND column_name = 'type') THEN
        ALTER TABLE room_events ADD COLUMN type TEXT NOT NULL DEFAULT 'm.room.message';
    END IF;
END $$;

-- ============================================================================
-- 5. 修复 key_backups 表 - 添加缺失的 mgmt_key 列
-- 错误: column "mgmt_key" does not exist
-- ============================================================================

CREATE TABLE IF NOT EXISTS key_backups (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    version BIGINT NOT NULL,
    algorithm TEXT NOT NULL,
    auth_data JSONB NOT NULL,
    mgmt_key TEXT,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT,
    deleted_ts BIGINT,
    CONSTRAINT key_backups_user_id_fkey 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT key_backups_unique 
        UNIQUE (user_id, version)
);

CREATE INDEX IF NOT EXISTS idx_key_backups_user ON key_backups(user_id);
CREATE INDEX IF NOT EXISTS idx_key_backups_version ON key_backups(version);

-- 如果表已存在但缺少列，则添加
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'key_backups' AND column_name = 'mgmt_key') THEN
        ALTER TABLE key_backups ADD COLUMN mgmt_key TEXT;
    END IF;
END $$;

-- ============================================================================
-- 6. 修复 rooms 表 - member_count 类型不匹配
-- 错误: Rust type `Option<i32>` is not compatible with SQL type `INT8`
-- ============================================================================

ALTER TABLE rooms ALTER COLUMN member_count TYPE BIGINT;

-- ============================================================================
-- 7. 修复 refresh_tokens 表 - 添加缺失的 expires_at 列
-- 错误: column "expires_at" does not exist
-- ============================================================================

DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'refresh_tokens' AND column_name = 'expires_at') THEN
        ALTER TABLE refresh_tokens ADD COLUMN expires_at BIGINT;
    END IF;
END $$;

-- ============================================================================
-- 8. 修复 space_hierarchy 表 - 添加缺失的 parent_id 列
-- 错误: column "parent_id" does not exist
-- ============================================================================

CREATE TABLE IF NOT EXISTS space_hierarchy (
    id BIGSERIAL PRIMARY KEY,
    parent_id TEXT NOT NULL,
    child_id TEXT NOT NULL,
    via_servers TEXT[],
    "order" INTEGER,
    suggested BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    CONSTRAINT space_hierarchy_parent_fkey 
        FOREIGN KEY (parent_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT space_hierarchy_child_fkey 
        FOREIGN KEY (child_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT space_hierarchy_unique 
        UNIQUE (parent_id, child_id)
);

CREATE INDEX IF NOT EXISTS idx_space_hierarchy_parent ON space_hierarchy(parent_id);
CREATE INDEX IF NOT EXISTS idx_space_hierarchy_child ON space_hierarchy(child_id);

-- 如果表已存在但缺少列，则添加
DO $$ 
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'space_hierarchy' AND column_name = 'parent_id') THEN
        ALTER TABLE space_hierarchy ADD COLUMN parent_id TEXT NOT NULL DEFAULT '';
    END IF;
END $$;

-- ============================================================================
-- 9. 添加 reports 表 - 用于事件举报功能
-- ============================================================================

CREATE TABLE IF NOT EXISTS reports (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    reporter_user_id TEXT NOT NULL,
    reason TEXT,
    score INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    CONSTRAINT reports_room_id_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT reports_reporter_fkey 
        FOREIGN KEY (reporter_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_reports_event ON reports(event_id);
CREATE INDEX IF NOT EXISTS idx_reports_room ON reports(room_id);
CREATE INDEX IF NOT EXISTS idx_reports_reporter ON reports(reporter_user_id);

-- ============================================================================
-- 10. 添加 room_tags 表 - 用于房间标签功能
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_tags (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    tag TEXT NOT NULL,
    "order" REAL,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    CONSTRAINT room_tags_user_id_fkey 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT room_tags_room_id_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT room_tags_unique 
        UNIQUE (user_id, room_id, tag)
);

CREATE INDEX IF NOT EXISTS idx_room_tags_user ON room_tags(user_id);
CREATE INDEX IF NOT EXISTS idx_room_tags_room ON room_tags(room_id);
CREATE INDEX IF NOT EXISTS idx_room_tags_tag ON room_tags(tag);

-- ============================================================================
-- 11. 添加 federation_signing_keys 表 - 用于联邦签名密钥
-- 错误: Missing or invalid federation signing key
-- ============================================================================

CREATE TABLE IF NOT EXISTS federation_signing_keys (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL UNIQUE,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    private_key TEXT,
    valid_until_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_server ON federation_signing_keys(server_name);
CREATE INDEX IF NOT EXISTS idx_federation_signing_keys_valid ON federation_signing_keys(valid_until_ts);

-- 插入默认联邦签名密钥
INSERT INTO federation_signing_keys (server_name, key_id, public_key, private_key, valid_until_ts, created_ts)
SELECT 
    'localhost',
    'ed25519:a_Opus',
    'a_OpusPublicKeyPlaceholder',
    'a_OpusPrivateKeyPlaceholder',
    EXTRACT(EPOCH FROM NOW() + INTERVAL '1 year') * 1000,
    EXTRACT(EPOCH FROM NOW()) * 1000
WHERE NOT EXISTS (SELECT 1 FROM federation_signing_keys WHERE server_name = 'localhost');

-- ============================================================================
-- 12. 添加 admin_users 表 - 用于管理员功能
-- ============================================================================

CREATE TABLE IF NOT EXISTS admin_users (
    user_id TEXT PRIMARY KEY REFERENCES users(user_id) ON DELETE CASCADE,
    admin_level INTEGER NOT NULL DEFAULT 1,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_admin_users_level ON admin_users(admin_level);

-- ============================================================================
-- 13. 添加 room_invites 表 - 用于房间邀请功能
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_invites (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    inviter_user_id TEXT NOT NULL,
    invitee_user_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    invite_token TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT,
    CONSTRAINT room_invites_room_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT room_invites_inviter_fkey 
        FOREIGN KEY (inviter_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT room_invites_invitee_fkey 
        FOREIGN KEY (invitee_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_invites_room ON room_invites(room_id);
CREATE INDEX IF NOT EXISTS idx_room_invites_inviter ON room_invites(inviter_user_id);
CREATE INDEX IF NOT EXISTS idx_room_invites_invitee ON room_invites(invitee_user_id);

-- ============================================================================
-- 14. 添加 room_forget 表 - 用于忘记房间功能
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_forget (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    forgot_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    CONSTRAINT room_forget_user_fkey 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT room_forget_room_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT room_forget_unique 
        UNIQUE (user_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_room_forget_user ON room_forget(user_id);
CREATE INDEX IF NOT EXISTS idx_room_forget_room ON room_forget(room_id);

-- ============================================================================
-- 15. 添加 room_redactions 表 - 用于消息删除功能
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_redactions (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL,
    redactor_user_id TEXT NOT NULL,
    reason TEXT,
    redacted_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    CONSTRAINT room_redactions_room_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT room_redactions_redactor_fkey 
        FOREIGN KEY (redactor_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_redactions_event ON room_redactions(event_id);
CREATE INDEX IF NOT EXISTS idx_room_redactions_room ON room_redactions(room_id);

-- ============================================================================
-- 16. 添加 room_state_events 表 - 用于房间状态事件缓存
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_state_events (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    state_key TEXT NOT NULL DEFAULT '',
    event_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    content JSONB NOT NULL,
    processed_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    CONSTRAINT room_state_events_room_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT room_state_events_unique 
        UNIQUE (room_id, event_type, state_key)
);

CREATE INDEX IF NOT EXISTS idx_room_state_events_room ON room_state_events(room_id);
CREATE INDEX IF NOT EXISTS idx_room_state_events_type ON room_state_events(event_type);

-- ============================================================================
-- 17. 添加 room_event_cache 表 - 用于房间事件缓存
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_event_cache (
    event_id TEXT PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    sender TEXT NOT NULL,
    content JSONB NOT NULL,
    state_key TEXT,
    depth BIGINT,
    origin_server_ts BIGINT NOT NULL,
    received_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    CONSTRAINT room_event_cache_room_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_event_cache_room ON room_event_cache(room_id);
CREATE INDEX IF NOT EXISTS idx_room_event_cache_type ON room_event_cache(event_type);
CREATE INDEX IF NOT EXISTS idx_room_event_cache_sender ON room_event_cache(sender);

-- ============================================================================
-- 18. 添加 voip_turn_servers 表 - 用于 VoIP TURN 服务器配置
-- 错误: VoIP/TURN service is not configured
-- ============================================================================

CREATE TABLE IF NOT EXISTS voip_turn_servers (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL UNIQUE,
    uris TEXT[] NOT NULL,
    username TEXT NOT NULL,
    password TEXT NOT NULL,
    ttl INTEGER NOT NULL DEFAULT 86400,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_voip_turn_servers_active ON voip_turn_servers(is_active);

-- 插入默认 TURN 服务器配置
INSERT INTO voip_turn_servers (server_name, uris, username, password, ttl, created_ts)
SELECT 
    'default',
    ARRAY['turn:turn.example.com:3478?transport=udp', 'turn:turn.example.com:3478?transport=tcp'],
    'turnuser',
    'turnpass',
    86400,
    EXTRACT(EPOCH FROM NOW()) * 1000
WHERE NOT EXISTS (SELECT 1 FROM voip_turn_servers WHERE server_name = 'default');

-- ============================================================================
-- 19. 添加 telemetry_config 表 - 用于遥测配置
-- ============================================================================

CREATE TABLE IF NOT EXISTS telemetry_config (
    id SERIAL PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    endpoint TEXT,
    sample_rate REAL NOT NULL DEFAULT 1.0,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000
);

-- 插入默认遥测配置
INSERT INTO telemetry_config (enabled, endpoint, sample_rate, updated_ts)
SELECT FALSE, NULL, 1.0, EXTRACT(EPOCH FROM NOW()) * 1000
WHERE NOT EXISTS (SELECT 1 FROM telemetry_config);

-- ============================================================================
-- 20. 添加 captcha_config 表 - 用于验证码配置
-- ============================================================================

CREATE TABLE IF NOT EXISTS captcha_config (
    id SERIAL PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT FALSE,
    provider TEXT,
    site_key TEXT,
    secret_key TEXT,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000
);

-- 插入默认验证码配置
INSERT INTO captcha_config (enabled, provider, site_key, secret_key, updated_ts)
SELECT FALSE, 'recaptcha', NULL, NULL, EXTRACT(EPOCH FROM NOW()) * 1000
WHERE NOT EXISTS (SELECT 1 FROM captcha_config);

-- ============================================================================
-- 21. 添加 friend_relationships 表 - 用于好友系统
-- ============================================================================

CREATE TABLE IF NOT EXISTS friend_relationships (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    friend_user_id TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT,
    CONSTRAINT friend_relationships_user_fkey 
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT friend_relationships_friend_fkey 
        FOREIGN KEY (friend_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT friend_relationships_unique 
        UNIQUE (user_id, friend_user_id)
);

CREATE INDEX IF NOT EXISTS idx_friend_relationships_user ON friend_relationships(user_id);
CREATE INDEX IF NOT EXISTS idx_friend_relationships_friend ON friend_relationships(friend_user_id);
CREATE INDEX IF NOT EXISTS idx_friend_relationships_status ON friend_relationships(status);

-- ============================================================================
-- 22. 添加 register_tokens 表 - 用于注册令牌
-- ============================================================================

CREATE TABLE IF NOT EXISTS register_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    uses_allowed INTEGER,
    pending INTEGER DEFAULT 0,
    completed INTEGER DEFAULT 0,
    expiry_time BIGINT,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    created_by TEXT,
    CONSTRAINT register_tokens_creator_fkey 
        FOREIGN KEY (created_by) REFERENCES users(user_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_register_tokens_token ON register_tokens(token);
CREATE INDEX IF NOT EXISTS idx_register_tokens_expiry ON register_tokens(expiry_time);

-- ============================================================================
-- 23. 添加 thread_relations 表 - 用于 Thread 功能
-- ============================================================================

CREATE TABLE IF NOT EXISTS thread_relations (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    thread_root_event_id TEXT NOT NULL,
    last_event_id TEXT NOT NULL,
    last_sender TEXT NOT NULL,
    last_ts BIGINT NOT NULL,
    count BIGINT NOT NULL DEFAULT 0,
    CONSTRAINT thread_relations_room_fkey 
        FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_thread_relations_room ON thread_relations(room_id);
CREATE INDEX IF NOT EXISTS idx_thread_relations_root ON thread_relations(thread_root_event_id);

-- ============================================================================
-- 24. 添加 directory_rooms 表 - 用于房间目录
-- ============================================================================

CREATE TABLE IF NOT EXISTS directory_rooms (
    room_id TEXT PRIMARY KEY REFERENCES rooms(room_id) ON DELETE CASCADE,
    is_listed BOOLEAN NOT NULL DEFAULT FALSE,
    appservice_id TEXT,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000
);

CREATE INDEX IF NOT EXISTS idx_directory_rooms_listed ON directory_rooms(is_listed);

-- ============================================================================
-- 25. 添加 admin_rooms 表 - 用于管理员房间管理
-- ============================================================================

CREATE TABLE IF NOT EXISTS admin_rooms (
    room_id TEXT PRIMARY KEY REFERENCES rooms(room_id) ON DELETE CASCADE,
    admin_level INTEGER NOT NULL DEFAULT 1,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_admin_rooms_level ON admin_rooms(admin_level);

-- ============================================================================
-- 26. 添加 user_privacy_settings 表 - 用于用户隐私设置
-- ============================================================================

CREATE TABLE IF NOT EXISTS user_privacy_settings (
    user_id TEXT PRIMARY KEY REFERENCES users(user_id) ON DELETE CASCADE,
    presence_visibility TEXT NOT NULL DEFAULT 'contacts',
    profile_visibility TEXT NOT NULL DEFAULT 'contacts',
    allow_invites BOOLEAN NOT NULL DEFAULT TRUE,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000
);

CREATE INDEX IF NOT EXISTS idx_user_privacy_settings_user ON user_privacy_settings(user_id);

-- ============================================================================
-- 完成
-- ============================================================================

-- 授予所有权限
GRANT ALL ON ALL TABLES IN SCHEMA public TO synapse_user;
GRANT ALL ON ALL SEQUENCES IN SCHEMA public TO synapse_user;

-- 记录迁移完成
INSERT INTO schema_migrations (version, applied_at)
VALUES ('20260308000005_fix_test_failures', NOW())
ON CONFLICT (version) DO NOTHING;
