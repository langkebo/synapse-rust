-- 账户数据相关表修复迁移
-- 修复账户数据、房间账户数据、过滤器和OpenID令牌表
-- 注意: 不使用 BEGIN/COMMIT，因为应用程序按语句分割执行

-- ============================================
-- 1. 修复 account_data 表
-- ============================================

-- 重命名现有表（如果存在旧名称）
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'user_account_data') THEN
        -- 备份数据
        CREATE TABLE IF NOT EXISTS account_data_backup AS SELECT * FROM user_account_data;
        
        -- 删除旧表
        DROP TABLE IF EXISTS user_account_data CASCADE;
    END IF;
END $$;

-- 创建符合代码预期的 account_data 表
CREATE TABLE IF NOT EXISTS account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    data_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    updated_at BIGINT NOT NULL,
    created_at BIGINT,
    UNIQUE(user_id, data_type),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_account_data_user ON account_data(user_id);
CREATE INDEX IF NOT EXISTS idx_account_data_type ON account_data(data_type);

-- 恢复备份数据（如果有）
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'account_data_backup') THEN
        INSERT INTO account_data (user_id, data_type, content, updated_at, created_at)
        SELECT user_id, account_data_type, content, COALESCE(updated_ts, created_ts), created_ts
        FROM account_data_backup
        ON CONFLICT (user_id, data_type) DO NOTHING;
        
        DROP TABLE account_data_backup;
    END IF;
END $$;

-- ============================================
-- 2. 修复 room_account_data 表
-- ============================================

-- 备份现有数据
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'room_account_data') THEN
        CREATE TABLE IF NOT EXISTS room_account_data_backup AS SELECT * FROM room_account_data;
        DROP TABLE IF EXISTS room_account_data CASCADE;
    END IF;
END $$;

-- 创建符合代码预期的 room_account_data 表
CREATE TABLE IF NOT EXISTS room_account_data (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    data_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    updated_at BIGINT NOT NULL,
    created_at BIGINT,
    UNIQUE(user_id, room_id, data_type),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_room_account_data_user ON room_account_data(user_id);
CREATE INDEX IF NOT EXISTS idx_room_account_data_room ON room_account_data(room_id);
CREATE INDEX IF NOT EXISTS idx_room_account_data_user_room ON room_account_data(user_id, room_id);

-- ============================================
-- 3. 创建 filters 表
-- ============================================

CREATE TABLE IF NOT EXISTS filters (
    id BIGSERIAL PRIMARY KEY,
    filter_id VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    created_at BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_filters_user ON filters(user_id);
CREATE INDEX IF NOT EXISTS idx_filters_filter_id ON filters(filter_id);

-- ============================================
-- 4. 创建 openid_tokens 表
-- ============================================

CREATE TABLE IF NOT EXISTS openid_tokens (
    id BIGSERIAL PRIMARY KEY,
    token VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_openid_tokens_token ON openid_tokens(token);
CREATE INDEX IF NOT EXISTS idx_openid_tokens_user ON openid_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_openid_tokens_expires ON openid_tokens(expires_at);

-- ============================================
-- 5. 清理过期数据
-- ============================================

-- 定期清理过期的 OpenID 令牌
DELETE FROM openid_tokens WHERE expires_at < EXTRACT(EPOCH FROM NOW())::BIGINT;

-- ============================================
-- 6. 添加注释
-- ============================================

COMMENT ON TABLE account_data IS '用户账户数据存储';
COMMENT ON TABLE room_account_data IS '用户房间账户数据存储';
COMMENT ON TABLE filters IS '用户过滤器存储';
COMMENT ON TABLE openid_tokens IS 'OpenID 令牌存储';

COMMENT ON COLUMN account_data.user_id IS '用户ID';
COMMENT ON COLUMN account_data.data_type IS '数据类型（如 m.push_rules, m.direct 等）';
COMMENT ON COLUMN account_data.content IS '数据内容（JSON格式）';

COMMENT ON COLUMN room_account_data.user_id IS '用户ID';
COMMENT ON COLUMN room_account_data.room_id IS '房间ID';
COMMENT ON COLUMN room_account_data.data_type IS '数据类型';
COMMENT ON COLUMN room_account_data.content IS '数据内容（JSON格式）';

COMMENT ON COLUMN filters.filter_id IS '过滤器唯一标识';
COMMENT ON COLUMN filters.user_id IS '所属用户ID';
COMMENT ON COLUMN filters.content IS '过滤器定义（JSON格式）';

COMMENT ON COLUMN openid_tokens.token IS 'OpenID 令牌';
COMMENT ON COLUMN openid_tokens.user_id IS '所属用户ID';
COMMENT ON COLUMN openid_tokens.expires_at IS '过期时间戳';
