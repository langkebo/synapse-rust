-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260222120000
-- 描述: 统一时间戳字段命名规范
-- 问题来源: DATABASE_FIELD_STANDARDS.md 规范要求
-- =============================================================================

-- =============================================================================
-- 第一部分: push_device 表字段重命名
-- 问题: created_at/updated_at 应改为 created_ts/updated_ts
-- =============================================================================

BEGIN;

-- 1. 添加新字段
ALTER TABLE push_device ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE push_device ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

-- 2. 迁移数据
UPDATE push_device SET created_ts = created_at WHERE created_at IS NOT NULL;
UPDATE push_device SET updated_ts = updated_at WHERE updated_at IS NOT NULL;

-- 3. 设置默认值和非空约束
ALTER TABLE push_device ALTER COLUMN created_ts SET DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000;
ALTER TABLE push_device ALTER COLUMN created_ts SET NOT NULL;

-- 4. 删除旧字段（保留一段时间后再删除）
-- ALTER TABLE push_device DROP COLUMN IF EXISTS created_at;
-- ALTER TABLE push_device DROP COLUMN IF EXISTS updated_at;

-- =============================================================================
-- 第二部分: push_rule 表字段重命名
-- 问题: created_at/updated_at 应改为 created_ts/updated_ts
-- =============================================================================

-- 1. 添加新字段
ALTER TABLE push_rule ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE push_rule ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

-- 2. 迁移数据
UPDATE push_rule SET created_ts = created_at WHERE created_at IS NOT NULL;
UPDATE push_rule SET updated_ts = updated_at WHERE updated_at IS NOT NULL;

-- 3. 设置默认值和非空约束
ALTER TABLE push_rule ALTER COLUMN created_ts SET DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000;
ALTER TABLE push_rule ALTER COLUMN created_ts SET NOT NULL;

-- 4. 删除旧字段（保留一段时间后再删除）
-- ALTER TABLE push_rule DROP COLUMN IF EXISTS created_at;
-- ALTER TABLE push_rule DROP COLUMN IF EXISTS updated_at;

-- =============================================================================
-- 第三部分: push_notification_queue 表字段重命名
-- 问题: created_at 应改为 created_ts
-- =============================================================================

-- 1. 添加新字段
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS created_ts BIGINT;

-- 2. 迁移数据
UPDATE push_notification_queue SET created_ts = created_at WHERE created_at IS NOT NULL;

-- 3. 设置默认值和非空约束
ALTER TABLE push_notification_queue ALTER COLUMN created_ts SET DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000;
ALTER TABLE push_notification_queue ALTER COLUMN created_ts SET NOT NULL;

-- 4. 删除旧字段（保留一段时间后再删除）
-- ALTER TABLE push_notification_queue DROP COLUMN IF EXISTS created_at;

-- =============================================================================
-- 第四部分: 记录迁移版本
-- =============================================================================

INSERT INTO schema_migrations (version, success, executed_at, description)
VALUES ('20260222120000', true, NOW(), '统一时间戳字段命名规范')
ON CONFLICT (version) DO NOTHING;

COMMIT;

-- =============================================================================
-- 回滚脚本 (如需回滚，请手动执行以下语句)
-- =============================================================================
-- BEGIN;
-- ALTER TABLE push_device DROP COLUMN IF EXISTS created_ts;
-- ALTER TABLE push_device DROP COLUMN IF EXISTS updated_ts;
-- ALTER TABLE push_rule DROP COLUMN IF EXISTS created_ts;
-- ALTER TABLE push_rule DROP COLUMN IF EXISTS updated_ts;
-- ALTER TABLE push_notification_queue DROP COLUMN IF EXISTS created_ts;
-- DELETE FROM schema_migrations WHERE version = '20260222120000';
-- COMMIT;
