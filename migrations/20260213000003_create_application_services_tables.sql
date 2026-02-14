-- =============================================================================
-- Synapse-Rust 应用服务 (Application Service) 数据库迁移脚本
-- 版本: 1.0
-- 创建日期: 2026-02-13
-- PostgreSQL版本: 15.x 兼容
-- 描述: 实现 Matrix Application Service 功能，支持第三方服务集成
-- =============================================================================

-- 应用服务表: 存储已注册的应用服务信息
CREATE TABLE IF NOT EXISTS application_services (
    id BIGSERIAL PRIMARY KEY,
    as_id VARCHAR(255) NOT NULL UNIQUE,
    url VARCHAR(1024) NOT NULL,
    as_token VARCHAR(255) NOT NULL UNIQUE,
    hs_token VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    name VARCHAR(255),
    description TEXT,
    rate_limited BOOLEAN DEFAULT FALSE,
    protocols TEXT[],
    namespaces JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    last_seen_ts BIGINT,
    is_active BOOLEAN DEFAULT TRUE,
    FOREIGN KEY (sender) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 应用服务索引
CREATE INDEX IF NOT EXISTS idx_application_services_as_id ON application_services(as_id);
CREATE INDEX IF NOT EXISTS idx_application_services_as_token ON application_services(as_token);
CREATE INDEX IF NOT EXISTS idx_application_services_sender ON application_services(sender);
CREATE INDEX IF NOT EXISTS idx_application_services_active ON application_services(is_active) WHERE is_active = TRUE;
CREATE INDEX IF NOT EXISTS idx_application_services_protocols ON application_services USING GIN(protocols);

-- 应用服务状态表: 存储应用服务的状态信息
CREATE TABLE IF NOT EXISTS application_service_state (
    as_id VARCHAR(255) NOT NULL,
    state_key VARCHAR(255) NOT NULL,
    state_value TEXT NOT NULL,
    updated_ts BIGINT NOT NULL,
    PRIMARY KEY (as_id, state_key),
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

-- 应用服务状态索引
CREATE INDEX IF NOT EXISTS idx_application_service_state_as_id ON application_service_state(as_id);

-- 应用服务事件流表: 存储需要推送给应用服务的事件
CREATE TABLE IF NOT EXISTS application_service_events (
    event_id VARCHAR(255) NOT NULL PRIMARY KEY,
    as_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}'::jsonb,
    state_key VARCHAR(255),
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    transaction_id VARCHAR(255),
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

-- 应用服务事件索引
CREATE INDEX IF NOT EXISTS idx_application_service_events_as_id ON application_service_events(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_events_room ON application_service_events(room_id);
CREATE INDEX IF NOT EXISTS idx_application_service_events_type ON application_service_events(event_type);
CREATE INDEX IF NOT EXISTS idx_application_service_events_processed ON application_service_events(as_id, processed_ts) WHERE processed_ts IS NULL;
CREATE INDEX IF NOT EXISTS idx_application_service_events_ts ON application_service_events(origin_server_ts DESC);

-- 应用服务事务表: 跟踪发送给应用服务的事务
CREATE TABLE IF NOT EXISTS application_service_transactions (
    id BIGSERIAL PRIMARY KEY,
    as_id VARCHAR(255) NOT NULL,
    transaction_id VARCHAR(255) NOT NULL,
    events JSONB NOT NULL DEFAULT '[]'::jsonb,
    sent_ts BIGINT NOT NULL,
    completed_ts BIGINT,
    retry_count INTEGER DEFAULT 0,
    last_error TEXT,
    UNIQUE(as_id, transaction_id),
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

-- 应用服务事务索引
CREATE INDEX IF NOT EXISTS idx_application_service_transactions_as_id ON application_service_transactions(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_transactions_pending ON application_service_transactions(as_id, completed_ts) WHERE completed_ts IS NULL;
CREATE INDEX IF NOT EXISTS idx_application_service_transactions_sent ON application_service_transactions(sent_ts DESC);

-- 应用服务用户命名空间表: 存储用户命名空间匹配规则
CREATE TABLE IF NOT EXISTS application_service_user_namespaces (
    id BIGSERIAL PRIMARY KEY,
    as_id VARCHAR(255) NOT NULL,
    namespace_pattern VARCHAR(255) NOT NULL,
    exclusive BOOLEAN DEFAULT FALSE,
    regex VARCHAR(512) NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

-- 应用服务用户命名空间索引
CREATE INDEX IF NOT EXISTS idx_application_service_user_ns_as_id ON application_service_user_namespaces(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_user_ns_pattern ON application_service_user_namespaces(namespace_pattern);

-- 应用服务房间别名命名空间表: 存储房间别名命名空间匹配规则
CREATE TABLE IF NOT EXISTS application_service_room_alias_namespaces (
    id BIGSERIAL PRIMARY KEY,
    as_id VARCHAR(255) NOT NULL,
    namespace_pattern VARCHAR(255) NOT NULL,
    exclusive BOOLEAN DEFAULT FALSE,
    regex VARCHAR(512) NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

-- 应用服务房间别名命名空间索引
CREATE INDEX IF NOT EXISTS idx_application_service_room_ns_as_id ON application_service_room_alias_namespaces(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_room_ns_pattern ON application_service_room_alias_namespaces(namespace_pattern);

-- 应用服务房间命名空间表: 存储房间ID命名空间匹配规则
CREATE TABLE IF NOT EXISTS application_service_room_namespaces (
    id BIGSERIAL PRIMARY KEY,
    as_id VARCHAR(255) NOT NULL,
    namespace_pattern VARCHAR(255) NOT NULL,
    exclusive BOOLEAN DEFAULT FALSE,
    regex VARCHAR(512) NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

-- 应用服务房间命名空间索引
CREATE INDEX IF NOT EXISTS idx_application_service_room_id_ns_as_id ON application_service_room_namespaces(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_room_id_ns_pattern ON application_service_room_namespaces(namespace_pattern);

-- 应用服务虚拟用户表: 存储由应用服务创建的虚拟用户
CREATE TABLE IF NOT EXISTS application_service_users (
    as_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    displayname VARCHAR(255),
    avatar_url VARCHAR(512),
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (as_id, user_id),
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 应用服务虚拟用户索引
CREATE INDEX IF NOT EXISTS idx_application_service_users_as_id ON application_service_users(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_users_user ON application_service_users(user_id);

-- 添加注释
COMMENT ON TABLE application_services IS 'Matrix Application Services - 已注册的第三方服务';
COMMENT ON TABLE application_service_state IS '应用服务状态表，存储键值对状态信息';
COMMENT ON TABLE application_service_events IS '应用服务事件流，存储需要推送的事件';
COMMENT ON TABLE application_service_transactions IS '应用服务事务表，跟踪发送的事务';
COMMENT ON TABLE application_service_user_namespaces IS '用户命名空间匹配规则';
COMMENT ON TABLE application_service_room_alias_namespaces IS '房间别名命名空间匹配规则';
COMMENT ON TABLE application_service_room_namespaces IS '房间ID命名空间匹配规则';
COMMENT ON TABLE application_service_users IS '应用服务创建的虚拟用户';

-- 创建更新时间戳触发器函数
CREATE OR REPLACE FUNCTION update_application_service_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- 为 application_services 表添加触发器
DROP TRIGGER IF EXISTS update_application_services_timestamp ON application_services;
CREATE TRIGGER update_application_services_timestamp
    BEFORE UPDATE ON application_services
    FOR EACH ROW
    EXECUTE FUNCTION update_application_service_timestamp();

-- 创建应用服务统计视图
CREATE OR REPLACE VIEW application_service_statistics AS
SELECT 
    asv.id,
    asv.as_id,
    asv.name,
    asv.is_active,
    asv.rate_limited,
    COUNT(DISTINCT asu.user_id) AS virtual_user_count,
    COUNT(DISTINCT ase.event_id) AS pending_event_count,
    COUNT(DISTINCT ast.id) FILTER (WHERE ast.completed_ts IS NULL) AS pending_transaction_count,
    asv.last_seen_ts,
    asv.created_ts
FROM application_services asv
LEFT JOIN application_service_users asu ON asv.as_id = asu.as_id
LEFT JOIN application_service_events ase ON asv.as_id = ase.as_id AND ase.processed_ts IS NULL
LEFT JOIN application_service_transactions ast ON asv.as_id = ast.as_id
GROUP BY asv.id, asv.as_id, asv.name, asv.is_active, asv.rate_limited, asv.last_seen_ts, asv.created_ts;

COMMENT ON VIEW application_service_statistics IS '应用服务统计视图，提供服务的基本统计信息';
