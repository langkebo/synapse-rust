-- ============================================================================
-- 密码安全增强迁移
-- 版本: 20260309
-- 说明: 添加密码过期、首次登录强制修改密码、密码历史记录等功能
-- ============================================================================

-- 1. 添加密码安全相关字段到 users 表
ALTER TABLE users ADD COLUMN IF NOT EXISTS password_changed_at BIGINT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS must_change_password BOOLEAN DEFAULT FALSE;
ALTER TABLE users ADD COLUMN IF NOT EXISTS password_expires_at BIGINT;
ALTER TABLE users ADD COLUMN IF NOT EXISTS failed_login_attempts INTEGER DEFAULT 0;
ALTER TABLE users ADD COLUMN IF NOT EXISTS locked_until BIGINT;

-- 2. 创建密码历史记录表
-- 存储用户的历史密码哈希，防止重复使用
CREATE TABLE IF NOT EXISTS password_history (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_password_history_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_password_history_user ON password_history(user_id);
CREATE INDEX IF NOT EXISTS idx_password_history_created ON password_history(created_ts DESC);

-- 3. 创建密码策略配置表
-- 存储系统级密码策略配置
CREATE TABLE IF NOT EXISTS password_policy (
    id SERIAL PRIMARY KEY,
    name VARCHAR(100) NOT NULL UNIQUE,
    value TEXT NOT NULL,
    description TEXT,
    updated_ts BIGINT NOT NULL
);

-- 插入默认密码策略配置
INSERT INTO password_policy (name, value, description, updated_ts) VALUES
    ('min_length', '8', '最小密码长度', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('max_length', '128', '最大密码长度', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('require_uppercase', 'true', '是否需要大写字母', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('require_lowercase', 'true', '是否需要小写字母', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('require_digit', 'true', '是否需要数字', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('require_special', 'true', '是否需要特殊字符', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('max_age_days', '90', '密码最大有效期（天），0表示永不过期', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('history_count', '5', '密码历史记录数量，防止重复使用', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('max_failed_attempts', '5', '最大登录失败次数，超过后锁定账户', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('lockout_duration_minutes', '30', '账户锁定时长（分钟）', EXTRACT(EPOCH FROM NOW()) * 1000),
    ('force_first_login_change', 'true', '是否强制首次登录修改密码', EXTRACT(EPOCH FROM NOW()) * 1000)
ON CONFLICT (name) DO NOTHING;

-- 4. 设置默认管理员账户需要首次登录修改密码
UPDATE users SET must_change_password = TRUE WHERE username = 'admin';

-- 5. 创建索引优化密码相关查询
CREATE INDEX IF NOT EXISTS idx_users_must_change_password ON users(must_change_password) WHERE must_change_password = TRUE;
CREATE INDEX IF NOT EXISTS idx_users_password_expires ON users(password_expires_at) WHERE password_expires_at IS NOT NULL;
CREATE INDEX IF NOT EXISTS idx_users_locked ON users(locked_until) WHERE locked_until IS NOT NULL;

-- ============================================================================
-- 完成提示
-- ============================================================================
DO $$
BEGIN
    RAISE NOTICE '==========================================';
    RAISE NOTICE '密码安全增强迁移完成';
    RAISE NOTICE '添加字段:';
    RAISE NOTICE '  - password_changed_at (密码修改时间)';
    RAISE NOTICE '  - must_change_password (强制修改密码)';
    RAISE NOTICE '  - password_expires_at (密码过期时间)';
    RAISE NOTICE '  - failed_login_attempts (登录失败次数)';
    RAISE NOTICE '  - locked_until (账户锁定时间)';
    RAISE NOTICE '新增表:';
    RAISE NOTICE '  - password_history (密码历史记录)';
    RAISE NOTICE '  - password_policy (密码策略配置)';
    RAISE NOTICE '==========================================';
END $$;
