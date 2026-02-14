-- 联邦黑名单功能迁移脚本
-- 实现联邦服务器黑名单/白名单管理

-- 联邦黑名单表
CREATE TABLE IF NOT EXISTS federation_blacklist (
    id SERIAL PRIMARY KEY,
    server_name VARCHAR(255) NOT NULL UNIQUE,
    block_type VARCHAR(20) NOT NULL DEFAULT 'blacklist',
    reason TEXT,
    blocked_by VARCHAR(255) NOT NULL,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE,
    is_active BOOLEAN DEFAULT true,
    metadata JSONB DEFAULT '{}',
    CONSTRAINT federation_blacklist_server_unique UNIQUE(server_name),
    CONSTRAINT federation_blacklist_type_check CHECK (block_type IN ('blacklist', 'whitelist', 'quarantine'))
);

CREATE INDEX idx_federation_blacklist_server ON federation_blacklist(server_name);
CREATE INDEX idx_federation_blacklist_type ON federation_blacklist(block_type);
CREATE INDEX idx_federation_blacklist_active ON federation_blacklist(is_active);
CREATE INDEX idx_federation_blacklist_expires ON federation_blacklist(expires_at);

-- 联邦黑名单事件日志表
CREATE TABLE IF NOT EXISTS federation_blacklist_log (
    id SERIAL PRIMARY KEY,
    server_name VARCHAR(255) NOT NULL,
    action VARCHAR(50) NOT NULL,
    old_status VARCHAR(50),
    new_status VARCHAR(50),
    reason TEXT,
    performed_by VARCHAR(255) NOT NULL,
    performed_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    ip_address VARCHAR(45),
    user_agent TEXT,
    metadata JSONB DEFAULT '{}',
    CONSTRAINT federation_blacklist_log_action_check CHECK (action IN ('add', 'remove', 'update', 'expire', 'reactivate'))
);

CREATE INDEX idx_federation_blacklist_log_server ON federation_blacklist_log(server_name);
CREATE INDEX idx_federation_blacklist_log_action ON federation_blacklist_log(action);
CREATE INDEX idx_federation_blacklist_log_performed_at ON federation_blacklist_log(performed_at);

-- 联邦访问统计表
CREATE TABLE IF NOT EXISTS federation_access_stats (
    id SERIAL PRIMARY KEY,
    server_name VARCHAR(255) NOT NULL UNIQUE,
    total_requests BIGINT DEFAULT 0,
    successful_requests BIGINT DEFAULT 0,
    failed_requests BIGINT DEFAULT 0,
    last_request_at TIMESTAMP WITH TIME ZONE,
    last_success_at TIMESTAMP WITH TIME ZONE,
    last_failure_at TIMESTAMP WITH TIME ZONE,
    average_response_time_ms DOUBLE PRECISION DEFAULT 0,
    error_rate DOUBLE PRECISION DEFAULT 0,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT federation_access_stats_server_unique UNIQUE(server_name)
);

CREATE INDEX idx_federation_access_stats_server ON federation_access_stats(server_name);
CREATE INDEX idx_federation_access_stats_last_request ON federation_access_stats(last_request_at);

-- 联邦黑名单规则表
CREATE TABLE IF NOT EXISTS federation_blacklist_rule (
    id SERIAL PRIMARY KEY,
    rule_name VARCHAR(100) NOT NULL UNIQUE,
    rule_type VARCHAR(50) NOT NULL,
    pattern VARCHAR(500) NOT NULL,
    action VARCHAR(20) NOT NULL DEFAULT 'block',
    priority INTEGER DEFAULT 100,
    enabled BOOLEAN DEFAULT true,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    created_by VARCHAR(255) NOT NULL,
    CONSTRAINT federation_blacklist_rule_name_unique UNIQUE(rule_name),
    CONSTRAINT federation_blacklist_rule_type_check CHECK (rule_type IN ('domain', 'regex', 'wildcard', 'cidr')),
    CONSTRAINT federation_blacklist_rule_action_check CHECK (action IN ('block', 'allow', 'quarantine', 'rate_limit'))
);

CREATE INDEX idx_federation_blacklist_rule_type ON federation_blacklist_rule(rule_type);
CREATE INDEX idx_federation_blacklist_rule_enabled ON federation_blacklist_rule(enabled);
CREATE INDEX idx_federation_blacklist_rule_priority ON federation_blacklist_rule(priority);

-- 插入默认规则
INSERT INTO federation_blacklist_rule (rule_name, rule_type, pattern, action, priority, description, created_by)
VALUES 
    ('block_malicious_servers', 'domain', 'malicious.example.com', 'block', 1000, 'Block known malicious server', 'system'),
    ('block_spam_servers', 'regex', '.*spam\\..*', 'block', 900, 'Block spam servers', 'system'),
    ('quarantine_new_servers', 'wildcard', '*.new', 'quarantine', 100, 'Quarantine new servers for review', 'system')
ON CONFLICT (rule_name) DO NOTHING;

-- 联邦黑名单配置表
CREATE TABLE IF NOT EXISTS federation_blacklist_config (
    id SERIAL PRIMARY KEY,
    config_key VARCHAR(100) NOT NULL UNIQUE,
    config_value TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT federation_blacklist_config_key_unique UNIQUE(config_key)
);

CREATE INDEX idx_federation_blacklist_config_key ON federation_blacklist_config(config_key);

-- 插入默认配置
INSERT INTO federation_blacklist_config (config_key, config_value, description)
VALUES 
    ('default_action', 'block', 'Default action for servers not in list'),
    ('auto_expire_days', '30', 'Auto expire days for temporary blocks'),
    ('max_blacklist_size', '10000', 'Maximum number of entries in blacklist'),
    ('enable_auto_blacklist', 'true', 'Enable automatic blacklisting based on behavior'),
    ('auto_blacklist_threshold', '10', 'Number of failures before auto-blacklist'),
    ('auto_blacklist_window_minutes', '60', 'Time window for counting failures'),
    ('quarantine_duration_hours', '24', 'Duration for quarantine status'),
    ('enable_whitelist', 'true', 'Enable whitelist functionality'),
    ('log_all_requests', 'false', 'Log all federation requests')
ON CONFLICT (config_key) DO NOTHING;

COMMENT ON TABLE federation_blacklist IS '联邦服务器黑名单表';
COMMENT ON TABLE federation_blacklist_log IS '联邦黑名单事件日志表';
COMMENT ON TABLE federation_access_stats IS '联邦访问统计表';
COMMENT ON TABLE federation_blacklist_rule IS '联邦黑名单规则表';
COMMENT ON TABLE federation_blacklist_config IS '联邦黑名单配置表';
