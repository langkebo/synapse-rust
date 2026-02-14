-- 可插拔模块系统表
-- 用于支持垃圾信息检查、第三方规则等扩展功能

-- 模块注册表
CREATE TABLE IF NOT EXISTS modules (
    id SERIAL PRIMARY KEY,
    module_name VARCHAR(255) NOT NULL UNIQUE,
    module_type VARCHAR(50) NOT NULL,
    version VARCHAR(50) NOT NULL,
    description TEXT,
    enabled BOOLEAN DEFAULT true,
    priority INTEGER DEFAULT 100,
    config JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    last_executed_ts BIGINT,
    execution_count INTEGER DEFAULT 0,
    error_count INTEGER DEFAULT 0,
    last_error TEXT
);

CREATE INDEX idx_modules_type ON modules(module_type);
CREATE INDEX idx_modules_enabled ON modules(enabled);
CREATE INDEX idx_modules_priority ON modules(priority);

-- 垃圾信息检查结果表
CREATE TABLE IF NOT EXISTS spam_check_results (
    id SERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    content JSONB NOT NULL,
    result VARCHAR(50) NOT NULL,
    score INTEGER DEFAULT 0,
    reason TEXT,
    checker_module VARCHAR(255) NOT NULL,
    checked_ts BIGINT NOT NULL,
    action_taken VARCHAR(50),
    UNIQUE(event_id, checker_module)
);

CREATE INDEX idx_spam_check_event ON spam_check_results(event_id);
CREATE INDEX idx_spam_check_sender ON spam_check_results(sender);
CREATE INDEX idx_spam_check_result ON spam_check_results(result);
CREATE INDEX idx_spam_check_ts ON spam_check_results(checked_ts);

-- 第三方规则检查结果表
CREATE TABLE IF NOT EXISTS third_party_rule_results (
    id SERIAL PRIMARY KEY,
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    rule_name VARCHAR(255) NOT NULL,
    allowed BOOLEAN NOT NULL,
    reason TEXT,
    modified_content JSONB,
    checked_ts BIGINT NOT NULL,
    UNIQUE(event_id, rule_name)
);

CREATE INDEX idx_third_party_event ON third_party_rule_results(event_id);
CREATE INDEX idx_third_party_room ON third_party_rule_results(room_id);
CREATE INDEX idx_third_party_sender ON third_party_rule_results(sender);
CREATE INDEX idx_third_party_allowed ON third_party_rule_results(allowed);

-- 模块执行日志表
CREATE TABLE IF NOT EXISTS module_execution_logs (
    id SERIAL PRIMARY KEY,
    module_name VARCHAR(255) NOT NULL,
    module_type VARCHAR(50) NOT NULL,
    event_id VARCHAR(255),
    room_id VARCHAR(255),
    execution_time_ms BIGINT NOT NULL,
    success BOOLEAN NOT NULL,
    error_message TEXT,
    metadata JSONB,
    executed_ts BIGINT NOT NULL
);

CREATE INDEX idx_module_logs_module ON module_execution_logs(module_name);
CREATE INDEX idx_module_logs_ts ON module_execution_logs(executed_ts);
CREATE INDEX idx_module_logs_success ON module_execution_logs(success);

-- 账户有效性检查表
CREATE TABLE IF NOT EXISTS account_validity (
    user_id VARCHAR(255) PRIMARY KEY,
    expiration_ts BIGINT NOT NULL,
    email_sent_ts BIGINT,
    renewal_token VARCHAR(255),
    renewal_token_ts BIGINT,
    is_valid BOOLEAN DEFAULT true,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX idx_account_validity_expiration ON account_validity(expiration_ts);
CREATE INDEX idx_account_validity_valid ON account_validity(is_valid);
CREATE INDEX idx_account_validity_token ON account_validity(renewal_token);

-- 密码认证提供者配置表
CREATE TABLE IF NOT EXISTS password_auth_providers (
    id SERIAL PRIMARY KEY,
    provider_name VARCHAR(255) NOT NULL UNIQUE,
    provider_type VARCHAR(50) NOT NULL,
    config JSONB NOT NULL,
    enabled BOOLEAN DEFAULT true,
    priority INTEGER DEFAULT 100,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX idx_password_auth_enabled ON password_auth_providers(enabled);
CREATE INDEX idx_password_auth_priority ON password_auth_providers(priority);

-- Presence 路由配置表
CREATE TABLE IF NOT EXISTS presence_routes (
    id SERIAL PRIMARY KEY,
    route_name VARCHAR(255) NOT NULL UNIQUE,
    route_type VARCHAR(50) NOT NULL,
    config JSONB NOT NULL,
    enabled BOOLEAN DEFAULT true,
    priority INTEGER DEFAULT 100,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX idx_presence_routes_enabled ON presence_routes(enabled);
CREATE INDEX idx_presence_routes_priority ON presence_routes(priority);

-- 媒体仓库回调表
CREATE TABLE IF NOT EXISTS media_callbacks (
    id SERIAL PRIMARY KEY,
    callback_name VARCHAR(255) NOT NULL UNIQUE,
    callback_type VARCHAR(50) NOT NULL,
    url VARCHAR(500) NOT NULL,
    method VARCHAR(10) DEFAULT 'POST',
    headers JSONB DEFAULT '{}',
    enabled BOOLEAN DEFAULT true,
    timeout_ms INTEGER DEFAULT 5000,
    retry_count INTEGER DEFAULT 3,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX idx_media_callbacks_enabled ON media_callbacks(enabled);
CREATE INDEX idx_media_callbacks_type ON media_callbacks(callback_type);

-- 速率限制回调表
CREATE TABLE IF NOT EXISTS rate_limit_callbacks (
    id SERIAL PRIMARY KEY,
    callback_name VARCHAR(255) NOT NULL UNIQUE,
    callback_type VARCHAR(50) NOT NULL,
    config JSONB NOT NULL,
    enabled BOOLEAN DEFAULT true,
    priority INTEGER DEFAULT 100,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX idx_rate_limit_callbacks_enabled ON rate_limit_callbacks(enabled);
CREATE INDEX idx_rate_limit_callbacks_priority ON rate_limit_callbacks(priority);

-- 账户数据回调表
CREATE TABLE IF NOT EXISTS account_data_callbacks (
    id SERIAL PRIMARY KEY,
    callback_name VARCHAR(255) NOT NULL UNIQUE,
    callback_type VARCHAR(50) NOT NULL,
    config JSONB NOT NULL,
    enabled BOOLEAN DEFAULT true,
    priority INTEGER DEFAULT 100,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE INDEX idx_account_data_callbacks_enabled ON account_data_callbacks(enabled);
CREATE INDEX idx_account_data_callbacks_priority ON account_data_callbacks(priority);

-- 触发器：自动更新 updated_ts
CREATE OR REPLACE FUNCTION update_module_updated_ts()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_modules_updated_ts
    BEFORE UPDATE ON modules
    FOR EACH ROW
    EXECUTE FUNCTION update_module_updated_ts();

CREATE TRIGGER trigger_password_auth_updated_ts
    BEFORE UPDATE ON password_auth_providers
    FOR EACH ROW
    EXECUTE FUNCTION update_module_updated_ts();

CREATE TRIGGER trigger_presence_routes_updated_ts
    BEFORE UPDATE ON presence_routes
    FOR EACH ROW
    EXECUTE FUNCTION update_module_updated_ts();

CREATE TRIGGER trigger_media_callbacks_updated_ts
    BEFORE UPDATE ON media_callbacks
    FOR EACH ROW
    EXECUTE FUNCTION update_module_updated_ts();

CREATE TRIGGER trigger_rate_limit_callbacks_updated_ts
    BEFORE UPDATE ON rate_limit_callbacks
    FOR EACH ROW
    EXECUTE FUNCTION update_module_updated_ts();

CREATE TRIGGER trigger_account_data_callbacks_updated_ts
    BEFORE UPDATE ON account_data_callbacks
    FOR EACH ROW
    EXECUTE FUNCTION update_module_updated_ts();

CREATE TRIGGER trigger_account_validity_updated_ts
    BEFORE UPDATE ON account_validity
    FOR EACH ROW
    EXECUTE FUNCTION update_module_updated_ts();

-- 注释
COMMENT ON TABLE modules IS '可插拔模块注册表';
COMMENT ON TABLE spam_check_results IS '垃圾信息检查结果';
COMMENT ON TABLE third_party_rule_results IS '第三方规则检查结果';
COMMENT ON TABLE module_execution_logs IS '模块执行日志';
COMMENT ON TABLE account_validity IS '账户有效性管理';
COMMENT ON TABLE password_auth_providers IS '密码认证提供者配置';
COMMENT ON TABLE presence_routes IS 'Presence路由配置';
COMMENT ON TABLE media_callbacks IS '媒体仓库回调配置';
COMMENT ON TABLE rate_limit_callbacks IS '速率限制回调配置';
COMMENT ON TABLE account_data_callbacks IS '账户数据回调配置';
