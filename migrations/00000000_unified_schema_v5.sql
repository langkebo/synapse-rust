-- ============================================================================
-- synapse-rust 统一数据库架构 v5.0.0
-- 创建日期: 2026-03-02
-- 说明: 修复所有字段命名不一致问题，完全符合 DATABASE_FIELD_STANDARDS.md 规范
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- 第一部分：核心用户表
-- ============================================================================

-- 用户表
CREATE TABLE IF NOT EXISTS users (
    user_id TEXT NOT NULL PRIMARY KEY,
    username TEXT NOT NULL UNIQUE,
    password_hash TEXT,
    is_admin BOOLEAN DEFAULT FALSE,
    is_guest BOOLEAN DEFAULT FALSE,
    is_shadow_banned BOOLEAN DEFAULT FALSE,
    is_deactivated BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    displayname TEXT,
    avatar_url TEXT,
    email TEXT,
    phone TEXT,
    generation BIGINT DEFAULT 0,
    consent_version TEXT,
    appservice_id TEXT,
    user_type TEXT,
    invalid_update_ts BIGINT,
    migration_state TEXT
);

CREATE INDEX IF NOT EXISTS idx_users_username ON users(username);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);
CREATE INDEX IF NOT EXISTS idx_users_is_admin ON users(is_admin);

-- 用户第三方身份表 (Third-party IDs)
CREATE TABLE IF NOT EXISTS user_threepids (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    medium TEXT NOT NULL,
    address TEXT NOT NULL,
    validated_ts BIGINT,
    added_ts BIGINT NOT NULL,
    is_verified BOOLEAN DEFAULT FALSE,
    verification_token TEXT,
    verification_expires_ts BIGINT,
    UNIQUE(medium, address),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_user_threepids_user ON user_threepids(user_id);
CREATE INDEX IF NOT EXISTS idx_user_threepids_medium_address ON user_threepids(medium, address);

-- 设备表
CREATE TABLE IF NOT EXISTS devices (
    device_id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL,
    display_name TEXT,
    device_key JSONB,
    last_seen_ts BIGINT,
    last_seen_ip TEXT,
    created_ts BIGINT NOT NULL,
    first_seen_ts BIGINT NOT NULL,
    user_agent TEXT,
    appservice_id TEXT,
    ignored_user_list TEXT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_devices_user_id ON devices(user_id);
CREATE INDEX IF NOT EXISTS idx_devices_last_seen ON devices(last_seen_ts DESC);

-- 访问令牌表
CREATE TABLE IF NOT EXISTS access_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT,
    last_used_ts BIGINT,
    user_agent TEXT,
    ip_address TEXT,
    is_valid BOOLEAN DEFAULT TRUE,
    revoked_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_access_tokens_token ON access_tokens(token);
CREATE INDEX IF NOT EXISTS idx_access_tokens_user_id ON access_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_access_tokens_valid ON access_tokens(is_valid) WHERE is_valid = TRUE;

-- 刷新令牌表
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    access_token_id TEXT,
    scope TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    last_used_ts BIGINT,
    use_count INTEGER DEFAULT 0,
    is_revoked BOOLEAN DEFAULT FALSE,
    revoked_ts BIGINT,
    revoked_reason TEXT,
    client_info JSONB,
    ip_address TEXT,
    user_agent TEXT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_revoked ON refresh_tokens(is_revoked) WHERE is_revoked = FALSE;

-- Token 黑名单表
CREATE TABLE IF NOT EXISTS token_blacklist (
    id BIGSERIAL PRIMARY KEY,
    token_hash TEXT NOT NULL UNIQUE,
    token TEXT,
    token_type TEXT DEFAULT 'access',
    user_id TEXT,
    revoked_ts BIGINT NOT NULL,
    reason TEXT,
    expires_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_token_blacklist_hash ON token_blacklist(token_hash);

-- ============================================================================
-- 第二部分：房间相关表
-- ============================================================================

-- 房间表
CREATE TABLE IF NOT EXISTS rooms (
    room_id TEXT NOT NULL PRIMARY KEY,
    creator TEXT,
    is_public BOOLEAN DEFAULT FALSE,
    room_version TEXT DEFAULT '6',
    created_ts BIGINT NOT NULL,
    last_activity_ts BIGINT,
    is_federated BOOLEAN DEFAULT TRUE,
    has_guest_access BOOLEAN DEFAULT FALSE,
    join_rules TEXT DEFAULT 'invite',
    history_visibility TEXT DEFAULT 'shared',
    name TEXT,
    topic TEXT,
    avatar_url TEXT,
    canonical_alias TEXT,
    member_count INTEGER DEFAULT 0,
    visibility TEXT DEFAULT 'private'
);

CREATE INDEX IF NOT EXISTS idx_rooms_creator ON rooms(creator);
CREATE INDEX IF NOT EXISTS idx_rooms_is_public ON rooms(is_public) WHERE is_public = TRUE;

-- 房间成员表
CREATE TABLE IF NOT EXISTS room_memberships (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    membership TEXT NOT NULL,
    joined_ts BIGINT,
    invited_ts BIGINT,
    left_ts BIGINT,
    banned_ts BIGINT,
    sender TEXT,
    reason TEXT,
    event_id TEXT,
    event_type TEXT,
    display_name TEXT,
    avatar_url TEXT,
    is_banned BOOLEAN DEFAULT FALSE,
    invite_token TEXT,
    updated_ts BIGINT,
    join_reason TEXT,
    banned_by TEXT,
    ban_reason TEXT,
    UNIQUE(room_id, user_id),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_memberships_room ON room_memberships(room_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_user ON room_memberships(user_id);
CREATE INDEX IF NOT EXISTS idx_room_memberships_membership ON room_memberships(membership);

-- 事件表
CREATE TABLE IF NOT EXISTS events (
    event_id TEXT NOT NULL PRIMARY KEY,
    room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content JSONB NOT NULL,
    origin_server_ts BIGINT NOT NULL,
    state_key TEXT,
    is_redacted BOOLEAN DEFAULT FALSE,
    redacted_ts BIGINT,
    redacted_by TEXT,
    transaction_id TEXT,
    depth BIGINT,
    prev_events JSONB,
    auth_events JSONB,
    signatures JSONB,
    hashes JSONB,
    unsigned JSONB DEFAULT '{}',
    processed_ts BIGINT,
    not_before BIGINT DEFAULT 0,
    status TEXT,
    reference_image TEXT,
    origin TEXT,
    user_id TEXT,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_events_room_id ON events(room_id);
CREATE INDEX IF NOT EXISTS idx_events_sender ON events(sender);
CREATE INDEX IF NOT EXISTS idx_events_type ON events(event_type);
CREATE INDEX IF NOT EXISTS idx_events_origin_server_ts ON events(origin_server_ts DESC);
CREATE INDEX IF NOT EXISTS idx_events_not_redacted ON events(room_id, origin_server_ts DESC) WHERE is_redacted = FALSE;

-- 房间摘要表
CREATE TABLE IF NOT EXISTS room_summaries (
    room_id TEXT NOT NULL PRIMARY KEY,
    name TEXT,
    topic TEXT,
    canonical_alias TEXT,
    joined_members BIGINT DEFAULT 0,
    invited_members BIGINT DEFAULT 0,
    hero_users JSONB,
    is_world_readable BOOLEAN DEFAULT FALSE,
    can_guest_join BOOLEAN DEFAULT FALSE,
    is_federated BOOLEAN DEFAULT TRUE,
    encryption_state TEXT,
    updated_ts BIGINT
);

-- 房间目录表
CREATE TABLE IF NOT EXISTS room_directory (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL UNIQUE,
    is_public BOOLEAN DEFAULT TRUE,
    is_searchable BOOLEAN DEFAULT TRUE,
    app_service_id TEXT,
    added_ts BIGINT NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_directory_public ON room_directory(is_public) WHERE is_public = TRUE;

-- 房间别名表
CREATE TABLE IF NOT EXISTS room_aliases (
    room_alias TEXT NOT NULL PRIMARY KEY,
    room_id TEXT NOT NULL,
    server_name TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_aliases_room_id ON room_aliases(room_id);

-- 线程统计表 (Thread Statistics)
CREATE TABLE IF NOT EXISTS thread_statistics (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    thread_root_event_id TEXT NOT NULL,
    reply_count BIGINT DEFAULT 0,
    last_reply_event_id TEXT,
    last_reply_sender TEXT,
    last_reply_ts BIGINT,
    participants JSONB DEFAULT '[]',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    UNIQUE(room_id, thread_root_event_id),
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_thread_statistics_room ON thread_statistics(room_id);
CREATE INDEX IF NOT EXISTS idx_thread_statistics_root ON thread_statistics(thread_root_event_id);
CREATE INDEX IF NOT EXISTS idx_thread_statistics_last_reply ON thread_statistics(last_reply_ts DESC);

-- ============================================================================
-- 第三部分：E2EE 加密相关表
-- ============================================================================

-- 设备密钥表
CREATE TABLE IF NOT EXISTS device_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    key_id TEXT NOT NULL,
    public_key TEXT NOT NULL,
    key_data TEXT,
    signatures JSONB,
    added_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    is_verified BOOLEAN DEFAULT FALSE,
    is_blocked BOOLEAN DEFAULT FALSE,
    display_name TEXT,
    UNIQUE(user_id, device_id, key_id)
);

CREATE INDEX IF NOT EXISTS idx_device_keys_user_device ON device_keys(user_id, device_id);

-- 跨签名密钥表
CREATE TABLE IF NOT EXISTS cross_signing_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    key_type TEXT NOT NULL,
    key_data TEXT NOT NULL,
    signatures JSONB,
    added_ts BIGINT NOT NULL,
    UNIQUE(user_id, key_type)
);

CREATE INDEX IF NOT EXISTS idx_cross_signing_keys_user ON cross_signing_keys(user_id);

-- Megolm 会话表
CREATE TABLE IF NOT EXISTS megolm_sessions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id TEXT NOT NULL UNIQUE,
    room_id TEXT NOT NULL,
    sender_key TEXT NOT NULL,
    session_key TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    message_index BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    last_used_ts BIGINT,
    expires_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_megolm_sessions_room ON megolm_sessions(room_id);
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_session ON megolm_sessions(session_id);

-- 事件签名表
CREATE TABLE IF NOT EXISTS event_signatures (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    signature TEXT NOT NULL,
    key_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    UNIQUE(event_id, user_id, device_id, key_id)
);

CREATE INDEX IF NOT EXISTS idx_event_signatures_event ON event_signatures(event_id);

-- 设备签名表
CREATE TABLE IF NOT EXISTS device_signatures (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    target_user_id TEXT NOT NULL,
    target_device_id TEXT NOT NULL,
    algorithm TEXT NOT NULL,
    signature TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    UNIQUE(user_id, device_id, target_user_id, target_device_id, algorithm)
);

-- 密钥备份表
CREATE TABLE IF NOT EXISTS key_backups (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL UNIQUE,
    algorithm TEXT NOT NULL,
    auth_data JSONB NOT NULL,
    version BIGINT DEFAULT 1,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_key_backups_user ON key_backups(user_id);

-- 密钥备份数据表
CREATE TABLE IF NOT EXISTS backup_keys (
    id BIGSERIAL PRIMARY KEY,
    backup_id BIGINT NOT NULL,
    room_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    session_data JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (backup_id) REFERENCES key_backups(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_backup_keys_backup ON backup_keys(backup_id);
CREATE INDEX IF NOT EXISTS idx_backup_keys_room ON backup_keys(room_id);

-- ============================================================================
-- 第四部分：媒体存储表
-- ============================================================================

-- 媒体元数据表
CREATE TABLE IF NOT EXISTS media_metadata (
    media_id TEXT NOT NULL PRIMARY KEY,
    server_name TEXT NOT NULL,
    content_type TEXT NOT NULL,
    file_name TEXT,
    size BIGINT NOT NULL,
    uploader_user_id TEXT,
    created_ts BIGINT NOT NULL,
    last_accessed_ts BIGINT,
    quarantine_status TEXT
);

CREATE INDEX IF NOT EXISTS idx_media_uploader ON media_metadata(uploader_user_id);
CREATE INDEX IF NOT EXISTS idx_media_server ON media_metadata(server_name);

-- 缩略图表
CREATE TABLE IF NOT EXISTS thumbnails (
    id BIGSERIAL PRIMARY KEY,
    media_id TEXT NOT NULL,
    width INTEGER NOT NULL,
    height INTEGER NOT NULL,
    method TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (media_id) REFERENCES media_metadata(media_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_thumbnails_media ON thumbnails(media_id);

-- 媒体配额表
CREATE TABLE IF NOT EXISTS media_quota (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL UNIQUE,
    max_bytes BIGINT DEFAULT 1073741824,
    used_bytes BIGINT DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- ============================================================================
-- 第五部分：认证相关表 (CAS/SAML)
-- ============================================================================

-- CAS 票据表
CREATE TABLE IF NOT EXISTS cas_tickets (
    id BIGSERIAL PRIMARY KEY,
    ticket_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    service_url TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL,
    consumed_ts BIGINT,
    consumed_by TEXT,
    is_valid BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_cas_tickets_ticket ON cas_tickets(ticket_id);
CREATE INDEX IF NOT EXISTS idx_cas_tickets_user ON cas_tickets(user_id);

-- CAS 代理票据表
CREATE TABLE IF NOT EXISTS cas_proxy_tickets (
    id BIGSERIAL PRIMARY KEY,
    proxy_ticket_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    service_url TEXT NOT NULL,
    pgt_url TEXT,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL,
    consumed_ts BIGINT,
    is_valid BOOLEAN DEFAULT TRUE
);

-- CAS 代理授予票据表
CREATE TABLE IF NOT EXISTS cas_proxy_granting_tickets (
    id BIGSERIAL PRIMARY KEY,
    pgt_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    service_url TEXT NOT NULL,
    iou TEXT,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL,
    is_valid BOOLEAN DEFAULT TRUE
);

-- CAS 服务表
CREATE TABLE IF NOT EXISTS cas_services (
    id BIGSERIAL PRIMARY KEY,
    service_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    service_url_pattern TEXT NOT NULL,
    allowed_attributes JSONB DEFAULT '[]',
    allowed_proxy_callbacks JSONB DEFAULT '[]',
    is_enabled BOOLEAN DEFAULT TRUE,
    require_secure BOOLEAN DEFAULT TRUE,
    single_logout BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

-- CAS 用户属性表
CREATE TABLE IF NOT EXISTS cas_user_attributes (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    attribute_name TEXT NOT NULL,
    attribute_value TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    UNIQUE(user_id, attribute_name)
);

-- CAS 单点登出会话表
CREATE TABLE IF NOT EXISTS cas_slo_sessions (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    service_url TEXT NOT NULL,
    ticket_id TEXT,
    created_ts BIGINT NOT NULL,
    logout_sent_ts BIGINT
);

-- SAML 会话表
CREATE TABLE IF NOT EXISTS saml_sessions (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    name_id TEXT,
    issuer TEXT,
    session_index TEXT,
    attributes JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL,
    last_used_ts BIGINT NOT NULL,
    status TEXT DEFAULT 'active'
);

CREATE INDEX IF NOT EXISTS idx_saml_sessions_user ON saml_sessions(user_id);

-- SAML 用户映射表
CREATE TABLE IF NOT EXISTS saml_user_mapping (
    id BIGSERIAL PRIMARY KEY,
    name_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    issuer TEXT NOT NULL,
    first_seen_ts BIGINT NOT NULL,
    last_authenticated_ts BIGINT NOT NULL,
    authentication_count INTEGER DEFAULT 1,
    attributes JSONB DEFAULT '{}',
    UNIQUE(name_id, issuer)
);

-- SAML 身份提供商表
CREATE TABLE IF NOT EXISTS saml_identity_providers (
    id BIGSERIAL PRIMARY KEY,
    entity_id TEXT NOT NULL UNIQUE,
    display_name TEXT,
    description TEXT,
    metadata_url TEXT,
    metadata_xml TEXT,
    is_enabled BOOLEAN DEFAULT TRUE,
    priority INTEGER DEFAULT 100,
    attribute_mapping JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    last_metadata_refresh_ts BIGINT,
    metadata_valid_until_ts BIGINT
);

-- SAML 认证事件表
CREATE TABLE IF NOT EXISTS saml_auth_events (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT,
    user_id TEXT,
    name_id TEXT,
    issuer TEXT,
    event_type TEXT NOT NULL,
    status TEXT NOT NULL,
    error_message TEXT,
    ip_address TEXT,
    user_agent TEXT,
    request_id TEXT,
    attributes JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL
);

-- SAML 登出请求表
CREATE TABLE IF NOT EXISTS saml_logout_requests (
    id BIGSERIAL PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE,
    session_id TEXT,
    user_id TEXT,
    name_id TEXT,
    issuer TEXT,
    reason TEXT,
    status TEXT DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    processed_ts BIGINT
);

-- ============================================================================
-- 第六部分：验证码相关表
-- ============================================================================

-- 注册验证码表
CREATE TABLE IF NOT EXISTS registration_captcha (
    id BIGSERIAL PRIMARY KEY,
    captcha_id TEXT NOT NULL UNIQUE,
    captcha_type TEXT NOT NULL,
    target TEXT NOT NULL,
    code TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL,
    used_ts BIGINT,
    verified_ts BIGINT,
    ip_address TEXT,
    user_agent TEXT,
    attempt_count INTEGER DEFAULT 0,
    max_attempts INTEGER DEFAULT 3,
    status TEXT DEFAULT 'pending',
    metadata JSONB DEFAULT '{}'
);

CREATE INDEX IF NOT EXISTS idx_captcha_target ON registration_captcha(target);
CREATE INDEX IF NOT EXISTS idx_captcha_status ON registration_captcha(status);

-- 验证码发送日志表
CREATE TABLE IF NOT EXISTS captcha_send_log (
    id BIGSERIAL PRIMARY KEY,
    captcha_id TEXT,
    captcha_type TEXT NOT NULL,
    target TEXT NOT NULL,
    sent_ts BIGINT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    is_success BOOLEAN DEFAULT TRUE,
    error_message TEXT,
    provider TEXT,
    provider_response TEXT
);

CREATE INDEX IF NOT EXISTS idx_captcha_send_target ON captcha_send_log(target);

-- 验证码模板表
CREATE TABLE IF NOT EXISTS captcha_template (
    id BIGSERIAL PRIMARY KEY,
    template_name TEXT NOT NULL UNIQUE,
    captcha_type TEXT NOT NULL,
    subject TEXT,
    content TEXT NOT NULL,
    variables JSONB DEFAULT '{}',
    is_default BOOLEAN DEFAULT FALSE,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

-- 验证码配置表
CREATE TABLE IF NOT EXISTS captcha_config (
    id BIGSERIAL PRIMARY KEY,
    config_key TEXT NOT NULL UNIQUE,
    config_value TEXT NOT NULL,
    description TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

-- ============================================================================
-- 第七部分：推送通知表
-- ============================================================================

-- 推送设备表
CREATE TABLE IF NOT EXISTS push_devices (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    push_kind TEXT NOT NULL,
    app_id TEXT NOT NULL,
    app_display_name TEXT,
    device_display_name TEXT,
    profile_tag TEXT,
    pushkey TEXT NOT NULL,
    lang TEXT DEFAULT 'en',
    data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    is_enabled BOOLEAN DEFAULT TRUE,
    UNIQUE(user_id, device_id, pushkey)
);

CREATE INDEX IF NOT EXISTS idx_push_devices_user ON push_devices(user_id);

-- 推送规则表
CREATE TABLE IF NOT EXISTS push_rules (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    rule_id TEXT NOT NULL,
    priority_class INTEGER NOT NULL,
    priority INTEGER DEFAULT 0,
    conditions JSONB DEFAULT '[]',
    actions JSONB DEFAULT '[]',
    pattern TEXT,
    is_default BOOLEAN DEFAULT FALSE,
    is_enabled BOOLEAN DEFAULT TRUE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    UNIQUE(user_id, scope, rule_id)
);

CREATE INDEX IF NOT EXISTS idx_push_rules_user ON push_rules(user_id);

-- ============================================================================
-- 第八部分：Space 相关表
-- ============================================================================

-- Space 子房间表
CREATE TABLE IF NOT EXISTS space_children (
    id BIGSERIAL PRIMARY KEY,
    space_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    is_suggested BOOLEAN DEFAULT FALSE,
    via_servers JSONB DEFAULT '[]',
    added_ts BIGINT NOT NULL,
    UNIQUE(space_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_space_children_space ON space_children(space_id);
CREATE INDEX IF NOT EXISTS idx_space_children_room ON space_children(room_id);

-- ============================================================================
-- 第九部分：联邦相关表
-- ============================================================================

-- 联邦服务器表
CREATE TABLE IF NOT EXISTS federation_servers (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL UNIQUE,
    is_blocked BOOLEAN DEFAULT FALSE,
    blocked_ts BIGINT,
    blocked_reason TEXT,
    last_successful_connect_ts BIGINT,
    last_failed_connect_ts BIGINT,
    failure_count INTEGER DEFAULT 0
);

-- 联邦黑名单表
CREATE TABLE IF NOT EXISTS federation_blacklist (
    id BIGSERIAL PRIMARY KEY,
    server_name TEXT NOT NULL UNIQUE,
    reason TEXT,
    added_ts BIGINT NOT NULL,
    added_by TEXT,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_federation_blacklist_server ON federation_blacklist(server_name);

-- 联邦队列表
CREATE TABLE IF NOT EXISTS federation_queue (
    id BIGSERIAL PRIMARY KEY,
    destination TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    room_id TEXT,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    sent_ts BIGINT,
    retry_count INTEGER DEFAULT 0,
    status TEXT DEFAULT 'pending'
);

CREATE INDEX IF NOT EXISTS idx_federation_queue_destination ON federation_queue(destination);
CREATE INDEX IF NOT EXISTS idx_federation_queue_status ON federation_queue(status);

-- ============================================================================
-- 第十部分：账户数据表
-- ============================================================================

-- 用户过滤器表
CREATE TABLE IF NOT EXISTS filters (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    filter_id TEXT NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    UNIQUE(user_id, filter_id)
);

CREATE INDEX IF NOT EXISTS idx_filters_user ON filters(user_id);
CREATE INDEX IF NOT EXISTS idx_filters_filter_id ON filters(filter_id);

-- OpenID 令牌表
CREATE TABLE IF NOT EXISTS openid_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT NOT NULL,
    is_valid BOOLEAN DEFAULT TRUE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_openid_tokens_token ON openid_tokens(token);
CREATE INDEX IF NOT EXISTS idx_openid_tokens_user ON openid_tokens(user_id);

-- 账户数据表
CREATE TABLE IF NOT EXISTS account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    data_type TEXT NOT NULL,
    data JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(user_id, data_type)
);

CREATE INDEX IF NOT EXISTS idx_account_data_user ON account_data(user_id);

-- 房间账户数据表
CREATE TABLE IF NOT EXISTS room_account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    data_type TEXT NOT NULL,
    data JSONB NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(user_id, room_id, data_type)
);

-- 用户账户数据表
CREATE TABLE IF NOT EXISTS user_account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    UNIQUE(user_id, event_type)
);

-- ============================================================================
-- 第十一部分：后台任务表
-- ============================================================================

-- 后台更新表
CREATE TABLE IF NOT EXISTS background_updates (
    id BIGSERIAL PRIMARY KEY,
    update_name TEXT NOT NULL UNIQUE,
    is_running BOOLEAN DEFAULT FALSE,
    progress JSONB DEFAULT '{}',
    started_ts BIGINT,
    completed_ts BIGINT,
    error_message TEXT
);

-- 工作进程表
CREATE TABLE IF NOT EXISTS workers (
    id BIGSERIAL PRIMARY KEY,
    worker_id TEXT NOT NULL UNIQUE,
    worker_type TEXT NOT NULL,
    last_heartbeat_ts BIGINT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    metadata JSONB DEFAULT '{}'
);

-- ============================================================================
-- 模块管理表
-- ============================================================================

CREATE TABLE IF NOT EXISTS modules (
    id BIGSERIAL PRIMARY KEY,
    module_name TEXT NOT NULL UNIQUE,
    module_type TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    config JSONB DEFAULT '{}',
    priority INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    description TEXT
);

CREATE INDEX IF NOT EXISTS idx_modules_name ON modules(module_name);
CREATE INDEX IF NOT EXISTS idx_modules_enabled ON modules(is_enabled);

-- ============================================================================
-- 模块执行日志表
-- ============================================================================

CREATE TABLE IF NOT EXISTS module_execution_logs (
    id BIGSERIAL PRIMARY KEY,
    module_id BIGINT REFERENCES modules(id) ON DELETE CASCADE,
    execution_type TEXT NOT NULL,
    input_data JSONB,
    output_data JSONB,
    is_success BOOLEAN DEFAULT TRUE,
    error_message TEXT,
    execution_time_ms BIGINT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_module_logs_module ON module_execution_logs(module_id);
CREATE INDEX IF NOT EXISTS idx_module_logs_created ON module_execution_logs(created_ts);

-- ============================================================================
-- 垃圾信息检查结果表
-- ============================================================================

CREATE TABLE IF NOT EXISTS spam_check_results (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    spam_score REAL DEFAULT 0,
    is_spam BOOLEAN DEFAULT FALSE,
    check_details JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_spam_results_event ON spam_check_results(event_id);
CREATE INDEX IF NOT EXISTS idx_spam_results_room ON spam_check_results(room_id);

-- ============================================================================
-- 第三方规则结果表
-- ============================================================================

CREATE TABLE IF NOT EXISTS third_party_rule_results (
    id BIGSERIAL PRIMARY KEY,
    rule_type TEXT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    user_id TEXT,
    is_allowed BOOLEAN DEFAULT TRUE,
    rule_details JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_third_party_rule_type ON third_party_rule_results(rule_type);

-- ============================================================================
-- 账户有效性检查表
-- ============================================================================

CREATE TABLE IF NOT EXISTS account_validity (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL UNIQUE,
    is_valid BOOLEAN DEFAULT TRUE,
    last_check_ts BIGINT,
    expiration_ts BIGINT,
    renewal_token TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_account_validity_user ON account_validity(user_id);

-- ============================================================================
-- 密码认证提供者表
-- ============================================================================

CREATE TABLE IF NOT EXISTS password_auth_providers (
    id BIGSERIAL PRIMARY KEY,
    provider_name TEXT NOT NULL UNIQUE,
    provider_type TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    config JSONB DEFAULT '{}',
    priority INTEGER DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_password_auth_name ON password_auth_providers(provider_name);

-- ============================================================================
-- Presence 路由表
-- ============================================================================

CREATE TABLE IF NOT EXISTS presence_routes (
    id BIGSERIAL PRIMARY KEY,
    route_name TEXT NOT NULL UNIQUE,
    route_type TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    config JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_presence_routes_name ON presence_routes(route_name);

-- ============================================================================
-- 媒体回调表
-- ============================================================================

CREATE TABLE IF NOT EXISTS media_callbacks (
    id BIGSERIAL PRIMARY KEY,
    callback_name TEXT NOT NULL UNIQUE,
    callback_type TEXT NOT NULL,
    is_enabled BOOLEAN DEFAULT TRUE,
    url TEXT NOT NULL,
    headers JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_media_callbacks_name ON media_callbacks(callback_name);

-- ============================================================================
-- 速率限制回调表
-- ============================================================================

CREATE TABLE IF NOT EXISTS rate_limit_callbacks (
    id BIGSERIAL PRIMARY KEY,
    callback_name TEXT NOT NULL UNIQUE,
    is_enabled BOOLEAN DEFAULT TRUE,
    config JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rate_limit_callbacks_name ON rate_limit_callbacks(callback_name);

-- ============================================================================
-- 账户数据回调表
-- ============================================================================

CREATE TABLE IF NOT EXISTS account_data_callbacks (
    id BIGSERIAL PRIMARY KEY,
    callback_name TEXT NOT NULL UNIQUE,
    is_enabled BOOLEAN DEFAULT TRUE,
    data_types TEXT[] DEFAULT '{}',
    config JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_account_data_callbacks_name ON account_data_callbacks(callback_name);

-- ============================================================================
-- 注册令牌表
-- ============================================================================

CREATE TABLE IF NOT EXISTS registration_tokens (
    id BIGSERIAL PRIMARY KEY,
    token TEXT NOT NULL UNIQUE,
    token_type TEXT DEFAULT 'single_use',
    description TEXT,
    max_uses INTEGER DEFAULT 0,
    uses_count INTEGER DEFAULT 0,
    is_used BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    expires_ts BIGINT,
    created_by TEXT NOT NULL,
    allowed_email_domains TEXT[],
    allowed_user_ids TEXT[],
    auto_join_rooms TEXT[],
    display_name TEXT,
    email TEXT
);

CREATE INDEX IF NOT EXISTS idx_registration_tokens_token ON registration_tokens(token);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_type ON registration_tokens(token_type);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_expires ON registration_tokens(expires_ts) WHERE expires_ts IS NOT NULL;

-- ============================================================================
-- 注册令牌使用记录表
-- ============================================================================

CREATE TABLE IF NOT EXISTS registration_token_usage (
    id BIGSERIAL PRIMARY KEY,
    token_id BIGINT REFERENCES registration_tokens(id) ON DELETE CASCADE,
    user_id TEXT NOT NULL,
    used_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_reg_token_usage_token ON registration_token_usage(token_id);
CREATE INDEX IF NOT EXISTS idx_reg_token_usage_user ON registration_token_usage(user_id);

-- ============================================================================
-- 事件举报表
-- ============================================================================

CREATE TABLE IF NOT EXISTS event_reports (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    reporter_user_id TEXT NOT NULL,
    reported_user_id TEXT,
    event_json JSONB,
    reason TEXT,
    description TEXT,
    status TEXT DEFAULT 'open',
    score INTEGER DEFAULT 0,
    received_ts BIGINT NOT NULL,
    resolved_ts BIGINT,
    resolved_by TEXT,
    resolution_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_event_reports_event ON event_reports(event_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_room ON event_reports(room_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_reporter ON event_reports(reporter_user_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_status ON event_reports(status);
CREATE INDEX IF NOT EXISTS idx_event_reports_received ON event_reports(received_ts DESC);

-- ============================================================================
-- 事件举报历史表
-- ============================================================================

CREATE TABLE IF NOT EXISTS event_report_history (
    id BIGSERIAL PRIMARY KEY,
    report_id BIGINT NOT NULL,
    action TEXT NOT NULL,
    actor_user_id TEXT,
    actor_role TEXT,
    old_status TEXT,
    new_status TEXT,
    reason TEXT,
    created_ts BIGINT NOT NULL,
    metadata JSONB,
    FOREIGN KEY (report_id) REFERENCES event_reports(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_event_report_history_report ON event_report_history(report_id);

-- ============================================================================
-- 举报速率限制表
-- ============================================================================

CREATE TABLE IF NOT EXISTS report_rate_limits (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL UNIQUE,
    report_count INTEGER DEFAULT 0,
    is_blocked BOOLEAN DEFAULT FALSE,
    blocked_until BIGINT,
    last_report_ts BIGINT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_report_rate_limits_user ON report_rate_limits(user_id);

-- ============================================================================
-- 举报统计表
-- ============================================================================

CREATE TABLE IF NOT EXISTS event_report_stats (
    id BIGSERIAL PRIMARY KEY,
    stat_date DATE NOT NULL UNIQUE,
    total_reports INTEGER DEFAULT 0,
    open_reports INTEGER DEFAULT 0,
    resolved_reports INTEGER DEFAULT 0,
    dismissed_reports INTEGER DEFAULT 0,
    escalated_reports INTEGER DEFAULT 0,
    avg_resolution_time_ms BIGINT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_event_report_stats_date ON event_report_stats(stat_date);

-- ============================================================================
-- 房间邀请表
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_invites (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    inviter TEXT NOT NULL,
    invitee TEXT NOT NULL,
    is_accepted BOOLEAN DEFAULT FALSE,
    accepted_ts BIGINT,
    created_ts BIGINT NOT NULL,
    expires_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_room_invites_room ON room_invites(room_id);
CREATE INDEX IF NOT EXISTS idx_room_invites_invitee ON room_invites(invitee);

-- ============================================================================
-- 推送通知队列表
-- ============================================================================

CREATE TABLE IF NOT EXISTS push_notification_queue (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    notification_type TEXT NOT NULL,
    content JSONB DEFAULT '{}',
    is_processed BOOLEAN DEFAULT FALSE,
    processed_ts BIGINT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_push_queue_user ON push_notification_queue(user_id);
CREATE INDEX IF NOT EXISTS idx_push_queue_processed ON push_notification_queue(is_processed);

-- ============================================================================
-- 推送通知日志表
-- ============================================================================

CREATE TABLE IF NOT EXISTS push_notification_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    pushkey TEXT NOT NULL,
    status TEXT NOT NULL,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    last_attempt_ts BIGINT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_push_log_user ON push_notification_log(user_id);
CREATE INDEX IF NOT EXISTS idx_push_log_status ON push_notification_log(status);

-- ============================================================================
-- 推送配置表
-- ============================================================================

CREATE TABLE IF NOT EXISTS push_config (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    device_id TEXT NOT NULL,
    config_type TEXT NOT NULL,
    config_data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    UNIQUE(user_id, device_id, config_type)
);

CREATE INDEX IF NOT EXISTS idx_push_config_user ON push_config(user_id);

-- ============================================================================
-- 通知表 (Matrix 标准通知)
-- ============================================================================

CREATE TABLE IF NOT EXISTS notifications (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    ts BIGINT NOT NULL,
    notification_type VARCHAR(50) DEFAULT 'message',
    profile_tag VARCHAR(255),
    is_read BOOLEAN DEFAULT FALSE,
    read BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_notifications_user_id ON notifications(user_id);
CREATE INDEX IF NOT EXISTS idx_notifications_ts ON notifications(ts DESC);
CREATE INDEX IF NOT EXISTS idx_notifications_room ON notifications(room_id);

-- ============================================================================
-- 语音消息表 (Voice Messages)
-- ============================================================================

CREATE TABLE IF NOT EXISTS voice_messages (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    room_id TEXT,
    media_id TEXT,
    duration_ms INT NOT NULL,
    waveform TEXT,
    mime_type VARCHAR(100),
    file_size BIGINT,
    transcription TEXT,
    encryption JSONB,
    is_processed BOOLEAN DEFAULT FALSE,
    processed_ts BIGINT,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_voice_messages_room ON voice_messages(room_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user ON voice_messages(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_processed ON voice_messages(is_processed);
CREATE INDEX IF NOT EXISTS idx_voice_messages_room_ts ON voice_messages(room_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user_ts ON voice_messages(user_id, created_ts DESC);

-- ============================================================================
-- 语音使用统计表 (Voice Usage Stats)
-- ============================================================================

CREATE TABLE IF NOT EXISTS voice_usage_stats (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT,
    date DATE NOT NULL,
    period_start TIMESTAMP,
    period_end TIMESTAMP,
    total_duration_ms BIGINT DEFAULT 0,
    total_file_size BIGINT DEFAULT 0,
    message_count BIGINT DEFAULT 0,
    last_activity_ts BIGINT,
    last_active_ts BIGINT,
    created_ts BIGINT,
    updated_ts BIGINT,
    UNIQUE(user_id, room_id, period_start)
);

CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_user ON voice_usage_stats(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_date ON voice_usage_stats(date);

-- ============================================================================
-- Presence 表
-- ============================================================================

CREATE TABLE IF NOT EXISTS presence (
    user_id TEXT NOT NULL PRIMARY KEY,
    status_msg TEXT,
    presence TEXT NOT NULL DEFAULT 'offline',
    last_active_ts BIGINT NOT NULL DEFAULT 0,
    status_from TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- ============================================================================
-- 用户目录表
-- ============================================================================

CREATE TABLE IF NOT EXISTS user_directory (
    user_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    added_by TEXT,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, room_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- ============================================================================
-- 好友相关表
-- ============================================================================

CREATE TABLE IF NOT EXISTS friends (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    friend_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    UNIQUE(user_id, friend_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS friend_requests (
    id BIGSERIAL PRIMARY KEY,
    sender_id TEXT NOT NULL,
    receiver_id TEXT NOT NULL,
    message TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    UNIQUE(sender_id, receiver_id),
    FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (receiver_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS friend_categories (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#000000',
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS blocked_users (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    blocked_id TEXT NOT NULL,
    reason TEXT,
    created_ts BIGINT NOT NULL,
    UNIQUE(user_id, blocked_id),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (blocked_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_friends_user_id ON friends(user_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_sender ON friend_requests(sender_id);
CREATE INDEX IF NOT EXISTS idx_friend_requests_receiver ON friend_requests(receiver_id);
CREATE INDEX IF NOT EXISTS idx_blocked_users_user_id ON blocked_users(user_id);

-- ============================================================================
-- 私密会话表
-- ============================================================================

CREATE TABLE IF NOT EXISTS private_sessions (
    id VARCHAR(255) NOT NULL PRIMARY KEY,
    user_id_1 VARCHAR(255) NOT NULL,
    user_id_2 VARCHAR(255) NOT NULL,
    session_type VARCHAR(50) DEFAULT 'direct',
    encryption_key VARCHAR(255),
    created_ts BIGINT NOT NULL,
    last_activity_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    unread_count INTEGER DEFAULT 0,
    encrypted_content TEXT,
    FOREIGN KEY (user_id_1) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id_2) REFERENCES users(user_id) ON DELETE CASCADE,
    UNIQUE (user_id_1, user_id_2)
);

CREATE TABLE IF NOT EXISTS private_messages (
    id BIGSERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL,
    sender_id VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    encrypted_content TEXT,
    created_ts BIGINT NOT NULL,
    message_type VARCHAR(50) DEFAULT 'm.text',
    is_read BOOLEAN DEFAULT FALSE,
    read_by_receiver BOOLEAN DEFAULT FALSE,
    read_ts BIGINT,
    edit_history JSONB,
    is_deleted BOOLEAN DEFAULT FALSE,
    deleted_ts BIGINT,
    is_edited BOOLEAN DEFAULT FALSE,
    unread_count INTEGER DEFAULT 0,
    FOREIGN KEY (session_id) REFERENCES private_sessions(id) ON DELETE CASCADE,
    FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_private_sessions_user ON private_sessions(user_id_1, user_id_2);
CREATE INDEX IF NOT EXISTS idx_private_messages_session ON private_messages(session_id);

-- ============================================================================
-- 安全事件表
-- ============================================================================

CREATE TABLE IF NOT EXISTS security_events (
    id BIGSERIAL PRIMARY KEY,
    event_type TEXT NOT NULL,
    user_id TEXT,
    ip_address TEXT,
    user_agent TEXT,
    details JSONB,
    created_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS ip_blocks (
    id BIGSERIAL PRIMARY KEY,
    ip_address TEXT UNIQUE NOT NULL,
    reason TEXT,
    blocked_ts BIGINT NOT NULL,
    expires_ts BIGINT
);

CREATE TABLE IF NOT EXISTS ip_reputation (
    id BIGSERIAL PRIMARY KEY,
    ip_address TEXT UNIQUE NOT NULL,
    score INTEGER DEFAULT 0,
    last_seen_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    details JSONB
);

CREATE INDEX IF NOT EXISTS idx_security_events_user_id ON security_events(user_id);
CREATE INDEX IF NOT EXISTS idx_security_events_created_ts ON security_events(created_ts);
CREATE INDEX IF NOT EXISTS idx_ip_blocks_blocked_ts ON ip_blocks(blocked_ts);
CREATE INDEX IF NOT EXISTS idx_ip_reputation_score ON ip_reputation(score);

-- ============================================================================
-- 读标记表
-- ============================================================================

CREATE TABLE IF NOT EXISTS read_markers (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    marker_type TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(room_id, user_id, marker_type)
);

CREATE INDEX IF NOT EXISTS idx_read_markers_room_user ON read_markers(room_id, user_id);

-- ============================================================================
-- 事件接收表
-- ============================================================================

CREATE TABLE IF NOT EXISTS event_receipts (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    receipt_type TEXT NOT NULL,
    ts BIGINT NOT NULL,
    data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(event_id, room_id, user_id, receipt_type)
);

CREATE INDEX IF NOT EXISTS idx_event_receipts_event ON event_receipts(event_id);
CREATE INDEX IF NOT EXISTS idx_event_receipts_room ON event_receipts(room_id);

-- ============================================================================
-- 房间状态事件表
-- ============================================================================

CREATE TABLE IF NOT EXISTS room_state_events (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    type TEXT NOT NULL,
    state_key TEXT NOT NULL,
    content JSONB NOT NULL,
    sender TEXT NOT NULL,
    origin_server_ts BIGINT NOT NULL,
    UNIQUE(room_id, type, state_key)
);

CREATE INDEX IF NOT EXISTS idx_room_state_events_room ON room_state_events(room_id);

-- ============================================================================
-- 刷新令牌使用记录表
-- ============================================================================

CREATE TABLE IF NOT EXISTS refresh_token_usage (
    id BIGSERIAL PRIMARY KEY,
    refresh_token_id BIGINT NOT NULL,
    user_id TEXT NOT NULL,
    old_access_token_id TEXT,
    new_access_token_id TEXT,
    used_ts BIGINT NOT NULL,
    ip_address TEXT,
    user_agent TEXT,
    is_success BOOLEAN DEFAULT TRUE,
    error_message TEXT
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_token ON refresh_token_usage(refresh_token_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_user ON refresh_token_usage(user_id);

-- ============================================================================
-- 刷新令牌家族表
-- ============================================================================

CREATE TABLE IF NOT EXISTS refresh_token_families (
    id BIGSERIAL PRIMARY KEY,
    family_id TEXT NOT NULL UNIQUE,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    last_refresh_ts BIGINT,
    refresh_count INTEGER DEFAULT 0,
    is_compromised BOOLEAN DEFAULT FALSE,
    compromised_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_families_user ON refresh_token_families(user_id);

-- ============================================================================
-- 刷新令牌轮换表
-- ============================================================================

CREATE TABLE IF NOT EXISTS refresh_token_rotations (
    id BIGSERIAL PRIMARY KEY,
    family_id TEXT NOT NULL,
    old_token_hash TEXT,
    new_token_hash TEXT NOT NULL,
    rotated_ts BIGINT NOT NULL,
    rotation_reason TEXT
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_rotations_family ON refresh_token_rotations(family_id);

-- ============================================================================
-- 第十二部分：迁移版本控制表
-- ============================================================================

-- 迁移记录表
CREATE TABLE IF NOT EXISTS schema_migrations (
    id BIGSERIAL PRIMARY KEY,
    version TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    applied_ts BIGINT NOT NULL,
    execution_time_ms BIGINT,
    checksum TEXT,
    description TEXT
);

CREATE INDEX IF NOT EXISTS idx_schema_migrations_version ON schema_migrations(version);

-- 插入初始版本记录
INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES ('v5.0.0', 'unified_schema_v5', EXTRACT(EPOCH FROM NOW()) * 1000, 'unified_schema_v5 - fixed all field naming inconsistencies')
ON CONFLICT (version) DO NOTHING;

-- ============================================================================
-- 完成提示
-- ============================================================================

DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE 'synapse-rust 统一数据库架构 v5.0.0 初始化完成';
    RAISE NOTICE '创建时间: %', NOW();
    RAISE NOTICE '修复: 所有字段命名符合 DATABASE_FIELD_STANDARDS.md 规范';
    RAISE NOTICE '==========================================';
END $$;
