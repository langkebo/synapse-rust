-- 注册验证码功能迁移脚本
-- 实现邮箱/手机验证码验证，防止恶意注册

-- 验证码表
CREATE TABLE IF NOT EXISTS registration_captcha (
    id SERIAL PRIMARY KEY,
    captcha_id VARCHAR(64) NOT NULL UNIQUE,
    captcha_type VARCHAR(20) NOT NULL,
    target VARCHAR(255) NOT NULL,
    code VARCHAR(20) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    used_at TIMESTAMP WITH TIME ZONE,
    verified_at TIMESTAMP WITH TIME ZONE,
    ip_address VARCHAR(45),
    user_agent TEXT,
    attempt_count INTEGER DEFAULT 0,
    max_attempts INTEGER DEFAULT 5,
    status VARCHAR(20) DEFAULT 'pending',
    metadata JSONB DEFAULT '{}',
    CONSTRAINT registration_captcha_captcha_id_unique UNIQUE(captcha_id),
    CONSTRAINT registration_captcha_type_check CHECK (captcha_type IN ('email', 'sms', 'image'))
);

CREATE INDEX idx_registration_captcha_target ON registration_captcha(target);
CREATE INDEX idx_registration_captcha_status ON registration_captcha(status);
CREATE INDEX idx_registration_captcha_expires_at ON registration_captcha(expires_at);
CREATE INDEX idx_registration_captcha_created_at ON registration_captcha(created_at);

-- 验证码发送记录表
CREATE TABLE IF NOT EXISTS captcha_send_log (
    id SERIAL PRIMARY KEY,
    captcha_id VARCHAR(64),
    captcha_type VARCHAR(20) NOT NULL,
    target VARCHAR(255) NOT NULL,
    sent_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    ip_address VARCHAR(45),
    user_agent TEXT,
    success BOOLEAN DEFAULT true,
    error_message TEXT,
    provider VARCHAR(50),
    provider_response TEXT,
    CONSTRAINT captcha_send_log_captcha_type_check CHECK (captcha_type IN ('email', 'sms', 'image'))
);

CREATE INDEX idx_captcha_send_log_target ON captcha_send_log(target);
CREATE INDEX idx_captcha_send_log_sent_at ON captcha_send_log(sent_at);
CREATE INDEX idx_captcha_send_log_captcha_id ON captcha_send_log(captcha_id);

-- 验证码频率限制表
CREATE TABLE IF NOT EXISTS captcha_rate_limit (
    id SERIAL PRIMARY KEY,
    target VARCHAR(255) NOT NULL,
    ip_address VARCHAR(45),
    captcha_type VARCHAR(20) NOT NULL,
    request_count INTEGER DEFAULT 1,
    first_request_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    last_request_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    blocked_until TIMESTAMP WITH TIME ZONE,
    CONSTRAINT captcha_rate_limit_captcha_type_check CHECK (captcha_type IN ('email', 'sms', 'image')),
    CONSTRAINT captcha_rate_limit_unique UNIQUE(target, captcha_type)
);

CREATE INDEX idx_captcha_rate_limit_target ON captcha_rate_limit(target);
CREATE INDEX idx_captcha_rate_limit_ip ON captcha_rate_limit(ip_address);
CREATE INDEX idx_captcha_rate_limit_blocked ON captcha_rate_limit(blocked_until);

-- 验证码模板表
CREATE TABLE IF NOT EXISTS captcha_template (
    id SERIAL PRIMARY KEY,
    template_name VARCHAR(100) NOT NULL UNIQUE,
    captcha_type VARCHAR(20) NOT NULL,
    subject VARCHAR(255),
    content TEXT NOT NULL,
    variables JSONB DEFAULT '[]',
    is_default BOOLEAN DEFAULT false,
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT captcha_template_captcha_type_check CHECK (captcha_type IN ('email', 'sms', 'image')),
    CONSTRAINT captcha_template_name_unique UNIQUE(template_name)
);

CREATE INDEX idx_captcha_template_type ON captcha_template(captcha_type);
CREATE INDEX idx_captcha_template_enabled ON captcha_template(enabled);

-- 插入默认模板
INSERT INTO captcha_template (template_name, captcha_type, subject, content, variables, is_default, enabled)
VALUES 
    ('default_email', 'email', '您的注册验证码', '您的注册验证码是：{{code}}，有效期{{expiry_minutes}}分钟。如非本人操作，请忽略此邮件。', '["code", "expiry_minutes"]', true, true),
    ('default_sms', 'sms', NULL, '您的注册验证码：{{code}}，有效期{{expiry_minutes}}分钟。', '["code", "expiry_minutes"]', true, true)
ON CONFLICT (template_name) DO NOTHING;

-- 验证码配置表
CREATE TABLE IF NOT EXISTS captcha_config (
    id SERIAL PRIMARY KEY,
    config_key VARCHAR(100) NOT NULL UNIQUE,
    config_value TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT captcha_config_key_unique UNIQUE(config_key)
);

CREATE INDEX idx_captcha_config_key ON captcha_config(config_key);

-- 插入默认配置
INSERT INTO captcha_config (config_key, config_value, description)
VALUES 
    ('email.code_length', '6', '邮箱验证码长度'),
    ('email.code_expiry_minutes', '10', '邮箱验证码有效期（分钟）'),
    ('email.max_attempts', '5', '邮箱验证码最大尝试次数'),
    ('email.rate_limit_per_hour', '5', '每小时发送次数限制'),
    ('email.rate_limit_per_day', '20', '每天发送次数限制'),
    ('sms.code_length', '6', '短信验证码长度'),
    ('sms.code_expiry_minutes', '5', '短信验证码有效期（分钟）'),
    ('sms.max_attempts', '5', '短信验证码最大尝试次数'),
    ('sms.rate_limit_per_hour', '5', '每小时发送次数限制'),
    ('sms.rate_limit_per_day', '10', '每天发送次数限制'),
    ('image.code_length', '4', '图片验证码长度'),
    ('image.code_expiry_minutes', '5', '图片验证码有效期（分钟）'),
    ('image.max_attempts', '3', '图片验证码最大尝试次数'),
    ('global.block_duration_minutes', '30', '触发限制后的封禁时长（分钟）'),
    ('global.ip_rate_limit_per_hour', '20', '同一IP每小时请求限制')
ON CONFLICT (config_key) DO NOTHING;

COMMENT ON TABLE registration_captcha IS '注册验证码表';
COMMENT ON TABLE captcha_send_log IS '验证码发送记录表';
COMMENT ON TABLE captcha_rate_limit IS '验证码频率限制表';
COMMENT ON TABLE captcha_template IS '验证码模板表';
COMMENT ON TABLE captcha_config IS '验证码配置表';
