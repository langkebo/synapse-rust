-- =============================================================================
-- 数据库初始化脚本
-- =============================================================================
-- 此脚本在 PostgreSQL 容器首次启动时执行，并运行在 POSTGRES_DB 指定的数据库中。
-- 这里只做幂等且与数据库名无关的初始化，避免硬编码用户或库名。
-- =============================================================================

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
CREATE EXTENSION IF NOT EXISTS "pg_stat_statements";

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

INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES ('0', 'init-db', EXTRACT(EPOCH FROM NOW()) * 1000, 'Database initialization')
ON CONFLICT (version) DO NOTHING;
