-- =============================================================================
-- 数据库初始化脚本
-- =============================================================================
-- 此脚本在 PostgreSQL 容器首次启动时执行
-- =============================================================================

-- 创建扩展
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";

-- 确保数据库用户有正确的权限
GRANT ALL PRIVILEGES ON DATABASE synapse TO postgres;

-- 创建 schema 版本表 (用于跟踪迁移状态)
CREATE TABLE IF NOT EXISTS schema_migrations (
    id BIGSERIAL PRIMARY KEY,
    version TEXT NOT NULL,
    name TEXT,
    checksum TEXT,
    applied_ts BIGINT,
    execution_time_ms BIGINT,
    success BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT,
    executed_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT uq_schema_migrations_version UNIQUE (version)
);

CREATE INDEX IF NOT EXISTS idx_schema_migrations_version ON schema_migrations(version);

-- 记录初始化
INSERT INTO schema_migrations (version, name, applied_ts, description) 
VALUES ('0', 'init-db', EXTRACT(EPOCH FROM NOW()) * 1000, 'Database initialization') 
ON CONFLICT (version) DO NOTHING;
