-- B-07 ticket: burn_after_read 重试上限与死信隔离
--
-- 问题：
-- - mark_processed_batch 失败 → redaction 事件已发出但 row 仍是 is_processed=FALSE
--   → 下轮 sweep 重新 redact（重复 redaction 事件）
-- - 无重试上限 → 反复失败的 row 永远重试
--
-- 变更：
-- 1. retry_count: 成功完成处理时 +1；记录"该 row 被处理了多少次"（包括成功的）
-- 2. last_error: 最近一次错误信息（可观测性）
-- 3. is_dead_letter: TRUE 时扫描器跳过该 row
--
-- 死信触发时机：redact + create + mark_processed_batch 三步全部成功，
-- 但 log_burned_event_batch 失败时 → 本轮仍视为"处理成功"（redaction 已在 timeline），
-- 只记录 last_error；下一轮重复时 retry_count 再 +1。
--
-- 死信阈值：5 次（重试 5 次后仍无法完成日志写入则放弃）。
-- 这个阈值同时覆盖了 redact/create 失败的场景（两种失败均会导致本轮
-- mark_processed_batch 不会执行，所以 retry_count 不会 +1）。
--
-- 零停机：
-- 1. 添加可空列（立即返回）
-- 2. 添加 NOT NULL 约束 + 默认值（pg 对已存在行用 DEFAULT 填充）
-- 3. 建立索引（CONCURRENTLY）

-- Step 1: 添加可空列
ALTER TABLE burn_after_read_pending
    ADD COLUMN IF NOT EXISTS retry_count INTEGER,
    ADD COLUMN IF NOT EXISTS last_error TEXT,
    ADD COLUMN IF NOT EXISTS is_dead_letter BOOLEAN;

-- Step 2: 用默认值填充已存在行，再改为 NOT NULL
UPDATE burn_after_read_pending SET retry_count = 0 WHERE retry_count IS NULL;
UPDATE burn_after_read_pending SET last_error = NULL WHERE last_error IS NULL;
UPDATE burn_after_read_pending SET is_dead_letter = FALSE WHERE is_dead_letter IS NULL;

ALTER TABLE burn_after_read_pending
    ALTER COLUMN retry_count SET NOT NULL,
    ALTER COLUMN retry_count SET DEFAULT 0,
    ALTER COLUMN is_dead_letter SET NOT NULL,
    ALTER COLUMN is_dead_letter SET DEFAULT FALSE;

-- Step 3: 修改部分索引，排除死信记录（减少扫描行数）
DROP INDEX IF EXISTS idx_burn_pending_delete_ts;
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_burn_pending_delete_ts
    ON burn_after_read_pending(delete_ts)
    WHERE is_processed = FALSE AND is_dead_letter = FALSE;
