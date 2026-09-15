-- =============================================================================
-- 数据库初始化脚本
-- =============================================================================
-- 此脚本在 PostgreSQL 容器首次启动时执行，并运行在 POSTGRES_DB 指定的数据库中。
-- 这里只做幂等且与数据库名无关的初始化，避免硬编码用户或库名。
-- =============================================================================

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";
CREATE EXTENSION IF NOT EXISTS "pgcrypto";
CREATE EXTENSION IF NOT EXISTS "pg_stat_statements";

-- 列定义必须与 canonical 迁移 `migrations/00000000_unified_schema_v*.sql`、
-- `docker/db_migrate.sh` 和 `docker/deploy/scripts/container-migrate.sh` 完全一致。
--
-- 这里曾经把成功标记写成 `success`，且把 executed_at 声明为 timestamptz：
-- 本脚本是 PostgreSQL 容器首次启动时最先执行的，`CREATE TABLE IF NOT EXISTS`
-- 会先建表，后续所有 `CREATE TABLE IF NOT EXISTS`（基线迁移 / 迁移器 / Rust
-- 运行时初始化）全部变成空操作，于是写入方引用的 `is_success` 列**不存在**、
-- executed_at 类型也错，迁移记录一条都写不进去（2026-09-15 实测：migrator
-- exit=1、schema_migrations 0 行、deploy.sh 版本一致性门禁必然失败）。
CREATE TABLE IF NOT EXISTS schema_migrations (
    id BIGSERIAL PRIMARY KEY,
    version TEXT NOT NULL,
    name TEXT,
    checksum TEXT,
    applied_ts BIGINT,
    execution_time_ms BIGINT,
    is_success BOOLEAN NOT NULL DEFAULT TRUE,
    description TEXT,
    executed_at BIGINT DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    CONSTRAINT uq_schema_migrations_version UNIQUE (version)
);

CREATE INDEX IF NOT EXISTS idx_schema_migrations_version ON schema_migrations(version);

INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES ('0', 'init-db', EXTRACT(EPOCH FROM NOW()) * 1000, 'Database initialization')
ON CONFLICT (version) DO NOTHING;
