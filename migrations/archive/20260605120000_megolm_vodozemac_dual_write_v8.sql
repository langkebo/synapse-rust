-- =============================================================================
-- Phase 2 (E2EE vodozemac 双写): megolm_sessions.pickle_format / vodozemac_pickle
-- =============================================================================
-- 时间: 2026-06-05
-- 目的:
--   1. 为 vodozemac 双写路径引入 pickle 格式标识。
--   2. 新增 vodozemac_pickle 列存储 vodozemac 0.9 pickle 副本。
--   3. 存量数据全部回填为 'legacy'（保持向后兼容）。
--   4. 引入 lazy 迁移部分索引，加速存量 legacy → dual 转换查询。
--
-- 设计原则:
--   - 完全幂等（IF NOT EXISTS 模式）。
--   - 零停机：DEFAULT 'legacy' 保证存量行立即合规。
--   - 兼容既有读路径：MegolmProvider 优先读 pickle_format，再 fallback。
--
-- 关联:
--   - docs/synapse-rust/E2EE_VODOZEMAC_MIGRATION.md Phase 2
--   - src/e2ee/megolm/storage.rs: create_session / get_session / update_session
--   - src/e2ee/megolm/service.rs: MegolmProvider::from_env
-- =============================================================================

-- 1) 新增列：pickle_format 标识（'legacy' / 'vodozemac' / 'dual'）
ALTER TABLE megolm_sessions
    ADD COLUMN IF NOT EXISTS pickle_format TEXT NOT NULL DEFAULT 'legacy';

-- 2) 新增列：vodozemac_pickle 副本（nullable，未启用 vodozemac 时为 NULL）
ALTER TABLE megolm_sessions
    ADD COLUMN IF NOT EXISTS vodozemac_pickle TEXT;

-- 3) CHECK 约束：pickle_format 取值合法性
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM information_schema.constraint_column_usage
        WHERE table_name = 'megolm_sessions'
          AND constraint_name = 'chk_megolm_sessions_pickle_format'
    ) THEN
        ALTER TABLE megolm_sessions
            ADD CONSTRAINT chk_megolm_sessions_pickle_format
            CHECK (pickle_format IN ('legacy', 'vodozemac', 'dual'));
    END IF;
END $$;

-- 4) 部分索引：仅索引存量 legacy 记录，加速懒迁移扫描
CREATE INDEX IF NOT EXISTS idx_megolm_sessions_pickle_format_legacy
    ON megolm_sessions(pickle_format)
    WHERE pickle_format = 'legacy';

-- 5) 兼容性 comment（运维查询友好）
COMMENT ON COLUMN megolm_sessions.pickle_format IS
    'Pickle 格式: legacy=自研AES-256-GCM, vodozemac=vodozemac 0.9 pickle, dual=同时持有两种 (Phase 2 引入)';
COMMENT ON COLUMN megolm_sessions.vodozemac_pickle IS
    'vodozemac 0.9 pickle 副本（base64 编码 JSON）。当 pickle_format=dual 或 vodozemac 时非空';
