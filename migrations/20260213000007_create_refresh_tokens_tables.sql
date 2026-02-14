-- 刷新令牌表
-- 支持长期会话和令牌刷新机制

-- 刷新令牌主表
CREATE TABLE IF NOT EXISTS refresh_tokens (
    id BIGSERIAL PRIMARY KEY,
    token_hash VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    access_token_id VARCHAR(255),
    scope TEXT,
    expires_at BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    last_used_ts BIGINT,
    use_count INTEGER DEFAULT 0,
    is_revoked BOOLEAN DEFAULT FALSE,
    revoked_ts BIGINT,
    revoked_reason VARCHAR(255),
    client_info JSONB,
    ip_address VARCHAR(45),
    user_agent TEXT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refresh_tokens_token_hash ON refresh_tokens(token_hash);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_user_id ON refresh_tokens(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_device_id ON refresh_tokens(device_id);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_expires_at ON refresh_tokens(expires_at);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_is_revoked ON refresh_tokens(is_revoked);
CREATE INDEX IF NOT EXISTS idx_refresh_tokens_created_ts ON refresh_tokens(created_ts DESC);

-- 刷新令牌使用历史表
CREATE TABLE IF NOT EXISTS refresh_token_usage (
    id BIGSERIAL PRIMARY KEY,
    refresh_token_id BIGINT NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    old_access_token_id VARCHAR(255),
    new_access_token_id VARCHAR(255),
    used_ts BIGINT NOT NULL,
    ip_address VARCHAR(45),
    user_agent TEXT,
    success BOOLEAN DEFAULT TRUE,
    error_message TEXT,
    FOREIGN KEY (refresh_token_id) REFERENCES refresh_tokens(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_token_id ON refresh_token_usage(refresh_token_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_user_id ON refresh_token_usage(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_usage_used_ts ON refresh_token_usage(used_ts DESC);

-- 刷新令牌族（Token Family）- 用于检测令牌重放攻击
CREATE TABLE IF NOT EXISTS refresh_token_families (
    id BIGSERIAL PRIMARY KEY,
    family_id VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255),
    created_ts BIGINT NOT NULL,
    last_refresh_ts BIGINT,
    refresh_count INTEGER DEFAULT 0,
    is_compromised BOOLEAN DEFAULT FALSE,
    compromised_ts BIGINT,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_families_family_id ON refresh_token_families(family_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_families_user_id ON refresh_token_families(user_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_families_is_compromised ON refresh_token_families(is_compromised);

-- 刷新令牌轮换记录表
CREATE TABLE IF NOT EXISTS refresh_token_rotations (
    id BIGSERIAL PRIMARY KEY,
    family_id VARCHAR(255) NOT NULL,
    old_token_hash VARCHAR(255),
    new_token_hash VARCHAR(255) NOT NULL,
    rotated_ts BIGINT NOT NULL,
    rotation_reason VARCHAR(50) DEFAULT 'refresh',
    FOREIGN KEY (family_id) REFERENCES refresh_token_families(family_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_refresh_token_rotations_family_id ON refresh_token_rotations(family_id);
CREATE INDEX IF NOT EXISTS idx_refresh_token_rotations_old_token ON refresh_token_rotations(old_token_hash);
CREATE INDEX IF NOT EXISTS idx_refresh_token_rotations_new_token ON refresh_token_rotations(new_token_hash);
CREATE INDEX IF NOT EXISTS idx_refresh_token_rotations_rotated_ts ON refresh_token_rotations(rotated_ts DESC);

-- 令牌黑名单表（用于撤销的访问令牌）
CREATE TABLE IF NOT EXISTS token_blacklist (
    id BIGSERIAL PRIMARY KEY,
    token_hash VARCHAR(255) NOT NULL UNIQUE,
    token_type VARCHAR(50) NOT NULL DEFAULT 'access',
    user_id VARCHAR(255) NOT NULL,
    revoked_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    reason VARCHAR(255),
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_token_blacklist_token_hash ON token_blacklist(token_hash);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_user_id ON token_blacklist(user_id);
CREATE INDEX IF NOT EXISTS idx_token_blacklist_expires_at ON token_blacklist(expires_at);

-- 触发器：自动清理过期的黑名单令牌
CREATE OR REPLACE FUNCTION cleanup_expired_blacklist_tokens()
RETURNS VOID AS $$
BEGIN
    DELETE FROM token_blacklist WHERE expires_at < EXTRACT(EPOCH FROM NOW()) * 1000;
END;
$$ LANGUAGE plpgsql;

-- 触发器：自动更新 last_used_ts
CREATE OR REPLACE FUNCTION update_refresh_token_last_used()
RETURNS TRIGGER AS $$
BEGIN
    NEW.last_used_ts = EXTRACT(EPOCH FROM NOW()) * 1000;
    NEW.use_count = OLD.use_count + 1;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_refresh_token_last_used
    BEFORE UPDATE ON refresh_tokens
    FOR EACH ROW
    EXECUTE FUNCTION update_refresh_token_last_used();

-- 函数：检查令牌是否被撤销
CREATE OR REPLACE FUNCTION is_token_revoked(p_token_hash VARCHAR)
RETURNS BOOLEAN AS $$
DECLARE
    v_revoked BOOLEAN;
BEGIN
    SELECT is_revoked INTO v_revoked
    FROM refresh_tokens
    WHERE token_hash = p_token_hash;
    
    RETURN COALESCE(v_revoked, TRUE);
END;
$$ LANGUAGE plpgsql;

-- 函数：检查令牌是否在黑名单中
CREATE OR REPLACE FUNCTION is_token_blacklisted(p_token_hash VARCHAR)
RETURNS BOOLEAN AS $$
DECLARE
    v_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO v_count
    FROM token_blacklist
    WHERE token_hash = p_token_hash
    AND expires_at > EXTRACT(EPOCH FROM NOW()) * 1000;
    
    RETURN v_count > 0;
END;
$$ LANGUAGE plpgsql;

-- 函数：撤销用户所有刷新令牌
CREATE OR REPLACE FUNCTION revoke_all_user_refresh_tokens(p_user_id VARCHAR, p_reason VARCHAR)
RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    UPDATE refresh_tokens
    SET is_revoked = TRUE,
        revoked_ts = EXTRACT(EPOCH FROM NOW()) * 1000,
        revoked_reason = p_reason
    WHERE user_id = p_user_id
    AND is_revoked = FALSE;
    
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- 函数：清理过期刷新令牌
CREATE OR REPLACE FUNCTION cleanup_expired_refresh_tokens()
RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    DELETE FROM refresh_tokens
    WHERE expires_at < EXTRACT(EPOCH FROM NOW()) * 1000
    AND is_revoked = FALSE;
    
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- 视图：活跃刷新令牌
CREATE OR REPLACE VIEW v_active_refresh_tokens AS
SELECT 
    rt.id,
    rt.token_hash,
    rt.user_id,
    rt.device_id,
    rt.scope,
    rt.expires_at,
    rt.created_ts,
    rt.last_used_ts,
    rt.use_count,
    rt.client_info,
    rt.ip_address,
    u.username,
    u.display_name
FROM refresh_tokens rt
JOIN users u ON u.user_id = rt.user_id
WHERE rt.is_revoked = FALSE
AND rt.expires_at > EXTRACT(EPOCH FROM NOW()) * 1000;

-- 视图：令牌使用统计
CREATE OR REPLACE VIEW v_refresh_token_stats AS
SELECT 
    rt.user_id,
    COUNT(*) as total_tokens,
    COUNT(*) FILTER (WHERE rt.is_revoked = FALSE AND rt.expires_at > EXTRACT(EPOCH FROM NOW()) * 1000) as active_tokens,
    COUNT(*) FILTER (WHERE rt.is_revoked = TRUE) as revoked_tokens,
    COUNT(*) FILTER (WHERE rt.expires_at <= EXTRACT(EPOCH FROM NOW()) * 1000) as expired_tokens,
    SUM(rt.use_count) as total_uses
FROM refresh_tokens rt
GROUP BY rt.user_id;
