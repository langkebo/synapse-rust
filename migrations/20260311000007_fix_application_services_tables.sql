-- ============================================================================
-- 迁移: 20260311000007_fix_application_services_tables.sql
-- 描述: 修复应用服务相关表结构和添加缺失表
-- 日期: 2026-03-11
-- ============================================================================

-- 1. 修复 application_services 表
-- 添加缺失字段并重命名不一致字段

-- 添加 name 字段
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_services' AND column_name = 'name'
    ) THEN
        ALTER TABLE application_services ADD COLUMN name TEXT;
        RAISE NOTICE 'Added column: application_services.name';
    END IF;
END $$;

-- 添加 sender 字段 (如果不存在)
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_services' AND column_name = 'sender'
    ) THEN
        ALTER TABLE application_services ADD COLUMN sender TEXT;
        RAISE NOTICE 'Added column: application_services.sender';
    END IF;
END $$;

-- 迁移 sender_localpart 数据到 sender
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_services' AND column_name = 'sender_localpart'
    ) AND EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_services' AND column_name = 'sender'
    ) THEN
        UPDATE application_services SET sender = sender_localpart WHERE sender IS NULL;
        RAISE NOTICE 'Migrated sender_localpart to sender';
    END IF;
END $$;

-- 添加 last_seen_ts 字段
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_services' AND column_name = 'last_seen_ts'
    ) THEN
        ALTER TABLE application_services ADD COLUMN last_seen_ts BIGINT;
        RAISE NOTICE 'Added column: application_services.last_seen_ts';
    END IF;
END $$;

-- 重命名 updated_at 为 updated_ts
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_services' AND column_name = 'updated_at'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_services' AND column_name = 'updated_ts'
    ) THEN
        ALTER TABLE application_services RENAME COLUMN updated_at TO updated_ts;
        RAISE NOTICE 'Renamed updated_at to updated_ts';
    END IF;
END $$;

-- 修改 is_enabled 默认值为 TRUE
ALTER TABLE application_services ALTER COLUMN is_enabled SET DEFAULT TRUE;

-- 2. 创建 application_service_state 表
CREATE TABLE IF NOT EXISTS application_service_state (
    as_id TEXT NOT NULL,
    state_key TEXT NOT NULL,
    state_value TEXT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_state PRIMARY KEY (as_id, state_key),
    CONSTRAINT fk_app_service_state_as_id FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

-- 3. 创建 application_service_events 表
CREATE TABLE IF NOT EXISTS application_service_events (
    event_id TEXT NOT NULL,
    as_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    sender TEXT NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    state_key TEXT,
    origin_server_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    transaction_id TEXT,
    CONSTRAINT pk_application_service_events PRIMARY KEY (event_id),
    CONSTRAINT fk_app_service_events_as_id FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_app_service_events_as_id ON application_service_events(as_id);
CREATE INDEX IF NOT EXISTS idx_app_service_events_processed ON application_service_events(as_id, processed_ts) WHERE processed_ts IS NULL;

-- 4. 创建 application_service_transactions 表
CREATE TABLE IF NOT EXISTS application_service_transactions (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    transaction_id TEXT NOT NULL,
    events JSONB NOT NULL DEFAULT '[]',
    sent_ts BIGINT NOT NULL,
    completed_ts BIGINT,
    retry_count INTEGER DEFAULT 0,
    last_error TEXT,
    CONSTRAINT pk_application_service_transactions PRIMARY KEY (id),
    CONSTRAINT uq_app_service_transactions UNIQUE (as_id, transaction_id),
    CONSTRAINT fk_app_service_transactions_as_id FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_app_service_transactions_pending ON application_service_transactions(as_id, completed_ts) WHERE completed_ts IS NULL;

-- 5. 创建 application_service_users 表
CREATE TABLE IF NOT EXISTS application_service_users (
    as_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    displayname TEXT,
    avatar_url TEXT,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_users PRIMARY KEY (as_id, user_id),
    CONSTRAINT fk_app_service_users_as_id FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_app_service_users_user_id ON application_service_users(user_id);

-- 6. 创建 application_service_user_namespaces 表
CREATE TABLE IF NOT EXISTS application_service_user_namespaces (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    namespace_pattern TEXT NOT NULL,
    exclusive BOOLEAN DEFAULT FALSE,
    regex TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_user_namespaces PRIMARY KEY (id),
    CONSTRAINT fk_app_service_user_ns_as_id FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_app_service_user_ns_as_id ON application_service_user_namespaces(as_id);

-- 7. 创建 application_service_room_alias_namespaces 表
CREATE TABLE IF NOT EXISTS application_service_room_alias_namespaces (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    namespace_pattern TEXT NOT NULL,
    exclusive BOOLEAN DEFAULT FALSE,
    regex TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_room_alias_ns PRIMARY KEY (id),
    CONSTRAINT fk_app_service_room_alias_ns_as_id FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_app_service_room_alias_ns_as_id ON application_service_room_alias_namespaces(as_id);

-- 8. 创建 application_service_room_namespaces 表
CREATE TABLE IF NOT EXISTS application_service_room_namespaces (
    id BIGSERIAL,
    as_id TEXT NOT NULL,
    namespace_pattern TEXT NOT NULL,
    exclusive BOOLEAN DEFAULT FALSE,
    regex TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_application_service_room_ns PRIMARY KEY (id),
    CONSTRAINT fk_app_service_room_ns_as_id FOREIGN KEY (as_id) REFERENCES application_services(as_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_app_service_room_ns_as_id ON application_service_room_namespaces(as_id);

-- 9. 创建 application_service_statistics 视图
CREATE OR REPLACE VIEW application_service_statistics AS
SELECT
    asv.id,
    asv.as_id,
    asv.name,
    asv.is_enabled,
    asv.rate_limited,
    COALESCE(u.user_count, 0) AS virtual_user_count,
    COALESCE(e.event_count, 0) AS pending_event_count,
    COALESCE(t.txn_count, 0) AS pending_transaction_count,
    asv.last_seen_ts,
    asv.created_ts
FROM application_services asv
LEFT JOIN (
    SELECT as_id, COUNT(*) AS user_count
    FROM application_service_users
    GROUP BY as_id
) u ON asv.as_id = u.as_id
LEFT JOIN (
    SELECT as_id, COUNT(*) AS event_count
    FROM application_service_events
    WHERE processed_ts IS NULL
    GROUP BY as_id
) e ON asv.as_id = e.as_id
LEFT JOIN (
    SELECT as_id, COUNT(*) AS txn_count
    FROM application_service_transactions
    WHERE completed_ts IS NULL
    GROUP BY as_id
) t ON asv.as_id = t.as_id;

-- ============================================================================
-- 验证
-- ============================================================================
DO $$
DECLARE
    col_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO col_count
    FROM information_schema.columns
    WHERE table_name = 'application_services'
    AND column_name IN ('name', 'sender', 'last_seen_ts', 'updated_ts');
    
    IF col_count >= 3 THEN
        RAISE NOTICE 'Application services table fixed successfully';
    ELSE
        RAISE WARNING 'Application services table may have missing columns';
    END IF;
END $$;
