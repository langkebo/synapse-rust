-- 撤销: 20260907000000_burn_idempotent_retry_cap.sql

-- 恢复原始部分索引（去掉 WHERE 条件）
DROP INDEX IF EXISTS idx_burn_pending_delete_ts;
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_burn_pending_delete_ts
    ON burn_after_read_pending(delete_ts)
    WHERE is_processed = FALSE;

-- 移除列（pg 支持 DROP COLUMN）
ALTER TABLE burn_after_read_pending DROP COLUMN IF EXISTS retry_count;
ALTER TABLE burn_after_read_pending DROP COLUMN IF EXISTS last_error;
ALTER TABLE burn_after_read_pending DROP COLUMN IF EXISTS is_dead_letter;
