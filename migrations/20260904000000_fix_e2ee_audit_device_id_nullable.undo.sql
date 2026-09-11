-- =====================================================================
-- Undo Migration: 20260904000000_fix_e2ee_audit_device_id_nullable.undo.sql
--
-- Forward 迁移: ALTER TABLE e2ee_audit_log ALTER COLUMN device_id DROP NOT NULL;
-- 目的: CrossSigningVerificationService::verify_all_devices 会写入一条
--       device_id = None 的汇总事件（不属于任何具体设备），而该列原为 NOT NULL，
--       导致每次 verify_user_devices 都报 "Failed to log key operation"。
--
-- 本 undo 恢复 NOT NULL 约束，但**先检查是否存在 NULL 行**：
-- 一旦 forward 迁移生效后写入过汇总事件，列中就存在 NULL，
-- 直接 ADD NOT NULL 会失败（或若用默认值填充则会静默改写审计数据）。
-- 因此这里选择 fail-loud：检测到 NULL 就抛错，由运维决定如何处理，
-- 而不是静默吞掉或篡改 e2ee 审计记录。
--
-- 如需强行回滚，请先自行处理 NULL 行，例如：
--   DELETE FROM e2ee_audit_log WHERE device_id IS NULL;   -- 会丢失审计记录
--   -- 或先归档到临时表再删除
-- 然后重新执行本 undo。
-- =====================================================================

DO $$
DECLARE
    null_rows BIGINT;
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public' AND table_name = 'e2ee_audit_log'
    ) THEN
        RAISE NOTICE 'e2ee_audit_log 不存在，跳过 undo';
        RETURN;
    END IF;

    -- 已是 NOT NULL 则幂等返回
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'e2ee_audit_log'
          AND column_name = 'device_id'
          AND is_nullable = 'NO'
    ) THEN
        RAISE NOTICE 'e2ee_audit_log.device_id 已是 NOT NULL，无需 undo';
        RETURN;
    END IF;

    EXECUTE 'SELECT count(*) FROM e2ee_audit_log WHERE device_id IS NULL' INTO null_rows;

    IF null_rows > 0 THEN
        RAISE EXCEPTION
            'e2ee_audit_log.device_id 存在 % 行 NULL（forward 迁移后写入的无设备汇总事件）；拒绝静默恢复 NOT NULL。请先处理这些行后重试。',
            null_rows;
    END IF;

    EXECUTE 'ALTER TABLE e2ee_audit_log ALTER COLUMN device_id SET NOT NULL';
    RAISE NOTICE 'e2ee_audit_log.device_id 已恢复 NOT NULL';
END $$;
