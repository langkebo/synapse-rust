-- +----------------------------------------------------------------------------+
-- | Migration: V260329_000__SYS_0001__create_migration_audit_table
-- | Jira: SYS-0001
-- | Author: synapse-rust team
-- | Date: 2026-03-29
-- | Description: 创建 migration_audit 表用于记录迁移执行指标
-- | Checksum: a1b2c3d4e5f6g7h8
-- +----------------------------------------------------------------------------+

BEGIN;

-- Migration Audit Table - 记录每次迁移执行的指标
CREATE TABLE IF NOT EXISTS migration_audit (
    id BIGSERIAL PRIMARY KEY,
    version VARCHAR(50) NOT NULL,
    description TEXT,
    duration_ms BIGINT NOT NULL,
    rows_affected BIGINT DEFAULT 0,
    executed_by VARCHAR(100) NOT NULL DEFAULT CURRENT_USER,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    status VARCHAR(20) NOT NULL DEFAULT 'SUCCESS',
    error_message TEXT,
    checksum VARCHAR(64),
    migration_file VARCHAR(255),
    created_ts BIGINT NOT NULL DEFAULT (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_migration_audit_version ON migration_audit (version);
CREATE INDEX IF NOT EXISTS idx_migration_audit_executed_at ON migration_audit (executed_at);
CREATE INDEX IF NOT EXISTS idx_migration_audit_status ON migration_audit (status);

-- 添加注释
COMMENT ON TABLE migration_audit IS '记录每次数据库迁移执行的指标，用于性能监控和问题排查';
COMMENT ON COLUMN migration_audit.duration_ms IS '迁移执行耗时（毫秒）';
COMMENT ON COLUMN migration_audit.rows_affected IS '影响的行数';
COMMENT ON COLUMN migration_audit.status IS '执行状态：SUCCESS, FAILED, ROLLED_BACK';
COMMENT ON COLUMN migration_audit.checksum IS '迁移脚本的 SHA-256 校验和';
COMMENT ON COLUMN migration_audit.migration_file IS '迁移脚本文件名';

COMMIT;
