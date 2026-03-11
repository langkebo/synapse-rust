-- ============================================================================
-- 修复 ip_reputation 表结构以匹配代码实现
-- 创建日期: 2026-03-11
-- 说明: 添加缺失的字段以匹配 admin.rs 中的代码期望
-- 参考: DATABASE_FIELD_STANDARDS.md
-- ============================================================================

-- 添加缺失的列
ALTER TABLE ip_reputation 
ADD COLUMN IF NOT EXISTS reputation_score INTEGER DEFAULT 50;

ALTER TABLE ip_reputation 
ADD COLUMN IF NOT EXISTS failed_attempts INTEGER DEFAULT 0;

ALTER TABLE ip_reputation 
ADD COLUMN IF NOT EXISTS successful_attempts INTEGER DEFAULT 0;

ALTER TABLE ip_reputation 
ADD COLUMN IF NOT EXISTS last_failed_ts BIGINT;

ALTER TABLE ip_reputation 
ADD COLUMN IF NOT EXISTS last_success_ts BIGINT;

ALTER TABLE ip_reputation 
ADD COLUMN IF NOT EXISTS is_blocked BOOLEAN DEFAULT FALSE;

ALTER TABLE ip_reputation 
ADD COLUMN IF NOT EXISTS blocked_ts BIGINT;

ALTER TABLE ip_reputation 
ADD COLUMN IF NOT EXISTS blocked_until_ts BIGINT;

ALTER TABLE ip_reputation 
ADD COLUMN IF NOT EXISTS block_reason TEXT;

ALTER TABLE ip_reputation 
ADD COLUMN IF NOT EXISTS risk_level VARCHAR(50) DEFAULT 'none';

-- 重命名 score 为 threat_score (如果存在)
-- 注意: 保留原有的 score 列作为 threat_score 的别名

-- 更新数据：将 reputation_score 设置为 score 的值（如果 reputation_score 为 NULL）
UPDATE ip_reputation 
SET reputation_score = COALESCE(reputation_score, score, 50)
WHERE reputation_score IS NULL;

-- 添加索引
CREATE INDEX IF NOT EXISTS idx_ip_reputation_reputation ON ip_reputation(reputation_score);
CREATE INDEX IF NOT EXISTS idx_ip_reputation_blocked ON ip_reputation(is_blocked) WHERE is_blocked = TRUE;

-- 插入迁移记录
INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES (
    'v6.0.4', 
    'fix_ip_reputation_table', 
    EXTRACT(EPOCH FROM NOW()) * 1000, 
    'Add missing columns to ip_reputation table to match code expectations'
) ON CONFLICT (version) DO NOTHING;
