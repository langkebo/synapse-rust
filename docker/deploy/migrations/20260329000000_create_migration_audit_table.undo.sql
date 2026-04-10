-- +----------------------------------------------------------------------------+
-- | Rollback: V260329_000__SYS_0001__create_migration_audit_table
-- | Jira: SYS-0001
-- | Author: synapse-rust team
-- | Date: 2026-03-29
-- | Description: 回滚创建 migration_audit 表
-- +----------------------------------------------------------------------------+

BEGIN;

DROP TABLE IF EXISTS migration_audit;

COMMIT;
