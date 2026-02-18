-- 注册令牌表
-- 支持邀请注册和注册令牌管理

-- 注册令牌主表
CREATE TABLE IF NOT EXISTS registration_tokens (
    id BIGSERIAL PRIMARY KEY,
    token VARCHAR(255) NOT NULL UNIQUE,
    token_type VARCHAR(50) DEFAULT 'single_use',
    description TEXT,
    max_uses INTEGER DEFAULT 1,
    current_uses INTEGER DEFAULT 0,
    is_used BOOLEAN DEFAULT FALSE,
    is_active BOOLEAN DEFAULT TRUE,
    expires_at BIGINT,
    created_by VARCHAR(255),
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    last_used_ts BIGINT,
    allowed_email_domains TEXT[],
    allowed_user_ids TEXT[],
    auto_join_rooms TEXT[],
    display_name VARCHAR(255),
    email VARCHAR(255),
    FOREIGN KEY (created_by) REFERENCES users(user_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_registration_tokens_token ON registration_tokens(token);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_is_active ON registration_tokens(is_active);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_is_used ON registration_tokens(is_used);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_expires_at ON registration_tokens(expires_at);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_created_ts ON registration_tokens(created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_registration_tokens_token_type ON registration_tokens(token_type);

-- 注册令牌使用记录表
CREATE TABLE IF NOT EXISTS registration_token_usage (
    id BIGSERIAL PRIMARY KEY,
    token_id BIGINT NOT NULL,
    token VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    username VARCHAR(255),
    email VARCHAR(255),
    ip_address VARCHAR(45),
    user_agent TEXT,
    used_ts BIGINT NOT NULL,
    success BOOLEAN DEFAULT TRUE,
    error_message TEXT,
    FOREIGN KEY (token_id) REFERENCES registration_tokens(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_registration_token_usage_token_id ON registration_token_usage(token_id);
CREATE INDEX IF NOT EXISTS idx_registration_token_usage_user_id ON registration_token_usage(user_id);
CREATE INDEX IF NOT EXISTS idx_registration_token_usage_used_ts ON registration_token_usage(used_ts DESC);

-- 邀请码表（用于房间邀请）
CREATE TABLE IF NOT EXISTS room_invites (
    id BIGSERIAL PRIMARY KEY,
    invite_code VARCHAR(255) NOT NULL UNIQUE,
    room_id VARCHAR(255) NOT NULL,
    inviter_user_id VARCHAR(255) NOT NULL,
    invitee_email VARCHAR(255),
    invitee_user_id VARCHAR(255),
    is_used BOOLEAN DEFAULT FALSE,
    is_revoked BOOLEAN DEFAULT FALSE,
    expires_at BIGINT,
    created_ts BIGINT NOT NULL,
    used_ts BIGINT,
    revoked_ts BIGINT,
    revoked_reason TEXT,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (inviter_user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (invitee_user_id) REFERENCES users(user_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_room_invites_invite_code ON room_invites(invite_code);
CREATE INDEX IF NOT EXISTS idx_room_invites_room_id ON room_invites(room_id);
CREATE INDEX IF NOT EXISTS idx_room_invites_inviter ON room_invites(inviter_user_id);
CREATE INDEX IF NOT EXISTS idx_room_invites_invitee ON room_invites(invitee_user_id);
CREATE INDEX IF NOT EXISTS idx_room_invites_is_used ON room_invites(is_used);
CREATE INDEX IF NOT EXISTS idx_room_invites_expires_at ON room_invites(expires_at);

-- 注册令牌批量创建表
CREATE TABLE IF NOT EXISTS registration_token_batches (
    id BIGSERIAL PRIMARY KEY,
    batch_id VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    token_count INTEGER NOT NULL,
    tokens_used INTEGER DEFAULT 0,
    created_by VARCHAR(255),
    created_ts BIGINT NOT NULL,
    expires_at BIGINT,
    is_active BOOLEAN DEFAULT TRUE,
    allowed_email_domains TEXT[],
    auto_join_rooms TEXT[],
    FOREIGN KEY (created_by) REFERENCES users(user_id) ON DELETE SET NULL
);

CREATE INDEX IF NOT EXISTS idx_registration_token_batches_batch_id ON registration_token_batches(batch_id);
CREATE INDEX IF NOT EXISTS idx_registration_token_batches_created_by ON registration_token_batches(created_by);
CREATE INDEX IF NOT EXISTS idx_registration_token_batches_is_active ON registration_token_batches(is_active);

-- 触发器：自动更新 updated_ts
CREATE OR REPLACE FUNCTION update_registration_token_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_registration_token_timestamp
    BEFORE UPDATE ON registration_tokens
    FOR EACH ROW
    EXECUTE FUNCTION update_registration_token_timestamp();

-- 函数：生成随机令牌
CREATE OR REPLACE FUNCTION generate_registration_token(length INTEGER DEFAULT 32)
RETURNS VARCHAR AS $$
DECLARE
    chars TEXT := 'ABCDEFGHJKLMNPQRSTUVWXYZabcdefghjkmnpqrstuvwxyz23456789';
    result VARCHAR := '';
    i INTEGER;
BEGIN
    FOR i IN 1..length LOOP
        result := result || substr(chars, floor(random() * length(chars))::INTEGER + 1, 1);
    END LOOP;
    RETURN result;
END;
$$ LANGUAGE plpgsql;

-- 函数：验证令牌有效性
CREATE OR REPLACE FUNCTION validate_registration_token(p_token VARCHAR)
RETURNS TABLE (
    is_valid BOOLEAN,
    token_id BIGINT,
    error_message TEXT
) AS $$
DECLARE
    v_token RECORD;
BEGIN
    SELECT * INTO v_token FROM registration_tokens WHERE token = p_token;
    
    IF NOT FOUND THEN
        RETURN QUERY SELECT FALSE, NULL::BIGINT, 'Token not found'::TEXT;
        RETURN;
    END IF;
    
    IF NOT v_token.is_active THEN
        RETURN QUERY SELECT FALSE, v_token.id, 'Token is not active'::TEXT;
        RETURN;
    END IF;
    
    IF v_token.is_used AND v_token.token_type = 'single_use' THEN
        RETURN QUERY SELECT FALSE, v_token.id, 'Token has already been used'::TEXT;
        RETURN;
    END IF;
    
    IF v_token.max_uses > 0 AND v_token.current_uses >= v_token.max_uses THEN
        RETURN QUERY SELECT FALSE, v_token.id, 'Token has reached maximum uses'::TEXT;
        RETURN;
    END IF;
    
    IF v_token.expires_at IS NOT NULL AND v_token.expires_at < EXTRACT(EPOCH FROM NOW()) * 1000 THEN
        RETURN QUERY SELECT FALSE, v_token.id, 'Token has expired'::TEXT;
        RETURN;
    END IF;
    
    RETURN QUERY SELECT TRUE, v_token.id, NULL::TEXT;
END;
$$ LANGUAGE plpgsql;

-- 函数：使用令牌
CREATE OR REPLACE FUNCTION use_registration_token(
    p_token VARCHAR,
    p_user_id VARCHAR,
    p_username VARCHAR,
    p_email VARCHAR,
    p_ip_address VARCHAR,
    p_user_agent TEXT
) RETURNS BOOLEAN AS $$
DECLARE
    v_token_id BIGINT;
    v_is_valid BOOLEAN;
    v_error TEXT;
BEGIN
    SELECT is_valid, token_id INTO v_is_valid, v_token_id
    FROM validate_registration_token(p_token);
    
    IF NOT v_is_valid THEN
        RETURN FALSE;
    END IF;
    
    UPDATE registration_tokens
    SET current_uses = current_uses + 1,
        is_used = CASE WHEN token_type = 'single_use' THEN TRUE ELSE is_used END,
        last_used_ts = EXTRACT(EPOCH FROM NOW()) * 1000
    WHERE id = v_token_id;
    
    INSERT INTO registration_token_usage (
        token_id, token, user_id, username, email, ip_address, user_agent, used_ts
    ) VALUES (
        v_token_id, p_token, p_user_id, p_username, p_email, p_ip_address, p_user_agent,
        EXTRACT(EPOCH FROM NOW()) * 1000
    );
    
    RETURN TRUE;
END;
$$ LANGUAGE plpgsql;

-- 函数：清理过期令牌
CREATE OR REPLACE FUNCTION cleanup_expired_registration_tokens()
RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    UPDATE registration_tokens
    SET is_active = FALSE
    WHERE expires_at IS NOT NULL
    AND expires_at < EXTRACT(EPOCH FROM NOW()) * 1000
    AND is_active = TRUE;
    
    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- 视图：活跃注册令牌
CREATE OR REPLACE VIEW v_active_registration_tokens AS
SELECT 
    rt.id,
    rt.token,
    rt.token_type,
    rt.description,
    rt.max_uses,
    rt.current_uses,
    rt.remaining_uses,
    rt.expires_at,
    rt.created_ts,
    rt.last_used_ts,
    rt.display_name,
    rt.email
FROM registration_tokens rt
WHERE rt.is_active = TRUE
AND (rt.expires_at IS NULL OR rt.expires_at > EXTRACT(EPOCH FROM NOW()) * 1000)
AND (rt.max_uses = 0 OR rt.current_uses < rt.max_uses);

-- 视图：令牌使用统计
CREATE OR REPLACE VIEW v_registration_token_stats AS
SELECT 
    rt.token_type,
    COUNT(*) as total_tokens,
    COUNT(*) FILTER (WHERE rt.is_active = TRUE) as active_tokens,
    COUNT(*) FILTER (WHERE rt.is_used = TRUE) as used_tokens,
    COUNT(*) FILTER (WHERE rt.expires_at < EXTRACT(EPOCH FROM NOW()) * 1000) as expired_tokens,
    SUM(rt.current_uses) as total_uses
FROM registration_tokens rt
GROUP BY rt.token_type;
