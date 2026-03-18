-- Application Services table migration
-- Create application_services table for App Service support
-- 注意：此迁移脚本与现有表结构兼容

-- 主应用服务表 (已存在，只需确保所有列都存在)
DO $$ 
BEGIN
    -- 添加可能缺失的列
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'application_services' AND column_name = 'sender') THEN
        ALTER TABLE application_services ADD COLUMN sender VARCHAR(255);
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'application_services' AND column_name = 'name') THEN
        ALTER TABLE application_services ADD COLUMN name TEXT;
    END IF;
    
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'application_services' AND column_name = 'last_seen_ts') THEN
        ALTER TABLE application_services ADD COLUMN last_seen_ts BIGINT;
    END IF;
    
    -- 确保sender_localpart可以为null
    ALTER TABLE application_services ALTER COLUMN sender_localpart DROP NOT NULL;
END $$;

-- 确保 protocols 列是 JSONB 类型
DO $$
DECLARE
    col_type text;
BEGIN
    SELECT data_type INTO col_type 
    FROM information_schema.columns 
    WHERE table_name = 'application_services' AND column_name = 'protocols';
    
    IF col_type = 'ARRAY' THEN
        -- 添加新列
        ALTER TABLE application_services ADD COLUMN protocols_new JSONB;
        -- 复制数据
        UPDATE application_services SET protocols_new = '[]'::jsonb;
        -- 删除旧列
        ALTER TABLE application_services DROP COLUMN protocols;
        -- 重命名
        ALTER TABLE application_services RENAME COLUMN protocols_new TO protocols;
    END IF;
END $$;

-- 应用服务状态表
CREATE TABLE IF NOT EXISTS application_service_state (
    as_id VARCHAR(255) NOT NULL,
    state_key VARCHAR(255) NOT NULL,
    state_value TEXT NOT NULL,
    updated_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (as_id, state_key)
);

-- 应用服务事件表
CREATE TABLE IF NOT EXISTS application_service_events (
    event_id VARCHAR(255) NOT NULL,
    as_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    state_key TEXT,
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    transaction_id VARCHAR(255),
    PRIMARY KEY (event_id, as_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_events_as_id ON application_service_events(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_events_room ON application_service_events(room_id);

-- 应用服务事务表
CREATE TABLE IF NOT EXISTS application_service_transactions (
    id BIGSERIAL PRIMARY KEY,
    as_id VARCHAR(255) NOT NULL,
    txn_id VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    UNIQUE(as_id, txn_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_transactions_as_id ON application_service_transactions(as_id);

-- 应用服务用户表 (虚拟用户)
CREATE TABLE IF NOT EXISTS application_service_users (
    as_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    displayname TEXT,
    avatar_url TEXT,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (as_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_users_as_id ON application_service_users(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_users_user_id ON application_service_users(user_id);

-- 应用服务房间表
CREATE TABLE IF NOT EXISTS application_service_rooms (
    as_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    creator_as_id VARCHAR(255) NOT NULL,
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    PRIMARY KEY (as_id, room_id)
);

CREATE INDEX IF NOT EXISTS idx_application_service_rooms_as_id ON application_service_rooms(as_id);
CREATE INDEX IF NOT EXISTS idx_application_service_rooms_room_id ON application_service_rooms(room_id);

-- 用户命名空间表
CREATE TABLE IF NOT EXISTS application_service_user_namespaces (
    as_id VARCHAR(255) NOT NULL,
    exclusive BOOLEAN NOT NULL DEFAULT true,
    regex VARCHAR(255) NOT NULL,
    PRIMARY KEY (as_id, regex),
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

-- 房间命名空间表
CREATE TABLE IF NOT EXISTS application_service_room_namespaces (
    as_id VARCHAR(255) NOT NULL,
    exclusive BOOLEAN NOT NULL DEFAULT false,
    regex VARCHAR(255) NOT NULL,
    PRIMARY KEY (as_id, regex),
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

-- 别名命名空间表
CREATE TABLE IF NOT EXISTS application_service_room_alias_namespaces (
    as_id VARCHAR(255) NOT NULL,
    exclusive BOOLEAN NOT NULL DEFAULT false,
    regex VARCHAR(255) NOT NULL,
    PRIMARY KEY (as_id, regex),
    FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);
