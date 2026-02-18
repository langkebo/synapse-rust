-- =============================================================================
-- Synapse-Rust 数据库迁移脚本
-- 版本: 20260219000001
-- 描述: 修复安全管理相关表结构与代码不匹配问题
-- 问题来源: 4.19.4 安全管理 API 测试报告
-- =============================================================================

-- =============================================================================
-- 第一部分: 修复 ip_blocks 表
-- 问题: 代码使用CIDR类型，数据库使用VARCHAR(50)
-- 解决: 将ip_range列转换为CIDR类型
-- =============================================================================

-- 备份现有数据到临时表
CREATE TABLE IF NOT EXISTS ip_blocks_backup AS SELECT * FROM ip_blocks;

-- 删除现有约束
ALTER TABLE ip_blocks DROP CONSTRAINT IF EXISTS ip_blocks_ip_range_key;

-- 添加新列（使用CIDR类型）
ALTER TABLE ip_blocks ADD COLUMN IF NOT EXISTS ip_range_cidr CIDR;

-- 迁移数据（将VARCHAR转换为CIDR）
UPDATE ip_blocks 
SET ip_range_cidr = ip_range::CIDR 
WHERE ip_range IS NOT NULL AND ip_range != '';

-- 删除旧列
ALTER TABLE ip_blocks DROP COLUMN IF EXISTS ip_range;

-- 重命名新列
ALTER TABLE ip_blocks RENAME COLUMN ip_range_cidr TO ip_range;

-- 添加约束
ALTER TABLE ip_blocks ALTER COLUMN ip_range SET NOT NULL;
ALTER TABLE ip_blocks ADD CONSTRAINT ip_blocks_ip_range_key UNIQUE (ip_range);

-- 添加缺失的列（代码期望的列）
ALTER TABLE ip_blocks ADD COLUMN IF NOT EXISTS blocked_at BIGINT;
ALTER TABLE ip_blocks ADD COLUMN IF NOT EXISTS expires_at BIGINT;

-- 同步数据（从blocked_ts/expires_ts复制到blocked_at/expires_at）
UPDATE ip_blocks SET blocked_at = blocked_ts WHERE blocked_at IS NULL;
UPDATE ip_blocks SET expires_at = expires_ts WHERE expires_at IS NULL;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_ip_blocks_range ON ip_blocks(ip_range);
CREATE INDEX IF NOT EXISTS idx_ip_blocks_expires ON ip_blocks(expires_ts) WHERE expires_ts IS NOT NULL;

-- =============================================================================
-- 第二部分: 修复 ip_reputation 表
-- 问题: 数据库列名为ip，代码期望ip_address，且缺少多个列
-- 解决: 重命名列并添加缺失的列
-- =============================================================================

-- 重命名主键列
ALTER TABLE ip_reputation RENAME COLUMN ip TO ip_address;

-- 添加缺失的列
ALTER TABLE ip_reputation ADD COLUMN IF NOT EXISTS score INTEGER DEFAULT 50;
ALTER TABLE ip_reputation ADD COLUMN IF NOT EXISTS last_seen_at BIGINT;
ALTER TABLE ip_reputation ADD COLUMN IF NOT EXISTS updated_at BIGINT;
ALTER TABLE ip_reputation ADD COLUMN IF NOT EXISTS details TEXT;
ALTER TABLE ip_reputation ADD COLUMN IF NOT EXISTS threat_level VARCHAR(50) DEFAULT 'none';
ALTER TABLE ip_reputation ADD COLUMN IF NOT EXISTS created_ts BIGINT;

-- 同步数据（使用现有reputation_score作为score）
UPDATE ip_reputation SET score = reputation_score WHERE score = 50 AND reputation_score IS NOT NULL;
UPDATE ip_reputation SET updated_at = last_updated_ts WHERE updated_at IS NULL;
UPDATE ip_reputation SET created_ts = last_updated_ts WHERE created_ts IS NULL;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_ip_reputation_score ON ip_reputation(score);
CREATE INDEX IF NOT EXISTS idx_ip_reputation_abuse ON ip_reputation(abuse_detected) WHERE abuse_detected = TRUE;

-- =============================================================================
-- 第三部分: 修复 security_events 表
-- 问题: 代码期望IpNetwork类型，数据库使用VARCHAR
-- 解决: 添加INET类型的ip_address_inet列用于代码查询
-- =============================================================================

-- 添加INET类型的IP地址列
ALTER TABLE security_events ADD COLUMN IF NOT EXISTS ip_address_inet INET;

-- 迁移数据（将VARCHAR转换为INET）
UPDATE security_events 
SET ip_address_inet = ip_address::INET 
WHERE ip_address IS NOT NULL AND ip_address != '';

-- 添加缺失的列（代码期望的列）
ALTER TABLE security_events ADD COLUMN IF NOT EXISTS description TEXT;
ALTER TABLE security_events ADD COLUMN IF NOT EXISTS created_ts BIGINT;

-- 同步数据
UPDATE security_events SET created_ts = created_at WHERE created_ts IS NULL;

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_security_events_created ON security_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_security_events_type ON security_events(event_type);
CREATE INDEX IF NOT EXISTS idx_security_events_unresolved ON security_events(resolved) WHERE resolved = false;
CREATE INDEX IF NOT EXISTS idx_security_events_user ON security_events(user_id);

-- =============================================================================
-- 第四部分: 创建 schema_migrations 表（如果不存在）
-- =============================================================================

CREATE TABLE IF NOT EXISTS schema_migrations (
    version VARCHAR(20) PRIMARY KEY,
    success BOOLEAN NOT NULL,
    executed_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    description TEXT
);

-- =============================================================================
-- 最后部分: 记录迁移版本
-- =============================================================================

INSERT INTO schema_migrations (version, success, executed_at, description)
VALUES ('20260219000001', true, NOW(), '修复安全管理相关表结构与代码不匹配问题')
ON CONFLICT (version) DO NOTHING;

-- =============================================================================
-- 回滚脚本 (如需回滚，请手动执行以下语句)
-- =============================================================================
-- -- 回滚 ip_blocks 表
-- ALTER TABLE ip_blocks RENAME COLUMN ip_range TO ip_range_cidr;
-- ALTER TABLE ip_blocks ADD COLUMN ip_range VARCHAR(50);
-- UPDATE ip_blocks SET ip_range = ip_range_cidr::TEXT;
-- ALTER TABLE ip_blocks DROP COLUMN ip_range_cidr;
-- ALTER TABLE ip_blocks DROP COLUMN IF EXISTS blocked_at;
-- ALTER TABLE ip_blocks DROP COLUMN IF EXISTS expires_at;
-- 
-- -- 回滚 ip_reputation 表
-- ALTER TABLE ip_reputation RENAME COLUMN ip_address TO ip;
-- ALTER TABLE ip_reputation DROP COLUMN IF EXISTS score;
-- ALTER TABLE ip_reputation DROP COLUMN IF EXISTS last_seen_at;
-- ALTER TABLE ip_reputation DROP COLUMN IF EXISTS updated_at;
-- ALTER TABLE ip_reputation DROP COLUMN IF EXISTS details;
-- ALTER TABLE ip_reputation DROP COLUMN IF EXISTS threat_level;
-- ALTER TABLE ip_reputation DROP COLUMN IF EXISTS created_ts;
-- 
-- -- 回滚 security_events 表
-- ALTER TABLE security_events DROP COLUMN IF EXISTS ip_address_inet;
-- ALTER TABLE security_events DROP COLUMN IF EXISTS description;
-- ALTER TABLE security_events DROP COLUMN IF EXISTS created_ts;
-- 
-- -- 删除迁移记录
-- DELETE FROM schema_migrations WHERE version = '20260219000001';
