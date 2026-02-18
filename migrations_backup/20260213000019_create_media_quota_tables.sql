-- Media Quota Tables
-- Implements media storage quota management

-- Media quota configuration table
CREATE TABLE IF NOT EXISTS media_quota_config (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    description TEXT,
    max_storage_bytes BIGINT NOT NULL DEFAULT 0,
    max_file_size_bytes BIGINT NOT NULL DEFAULT 104857600,
    max_files_count INTEGER NOT NULL DEFAULT 0,
    allowed_mime_types JSONB DEFAULT '[]',
    blocked_mime_types JSONB DEFAULT '[]',
    is_default BOOLEAN NOT NULL DEFAULT FALSE,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_media_quota_config_name ON media_quota_config(name);
CREATE INDEX idx_media_quota_config_is_default ON media_quota_config(is_default);
CREATE INDEX idx_media_quota_config_is_active ON media_quota_config(is_active);

-- User media quota assignments table
CREATE TABLE IF NOT EXISTS user_media_quota (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL UNIQUE,
    quota_config_id INTEGER REFERENCES media_quota_config(id) ON DELETE SET NULL,
    custom_max_storage_bytes BIGINT,
    custom_max_file_size_bytes BIGINT,
    custom_max_files_count INTEGER,
    current_storage_bytes BIGINT NOT NULL DEFAULT 0,
    current_files_count INTEGER NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT fk_user_media_quota_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX idx_user_media_quota_user_id ON user_media_quota(user_id);
CREATE INDEX idx_user_media_quota_quota_config ON user_media_quota(quota_config_id);

-- Media usage tracking table
CREATE TABLE IF NOT EXISTS media_usage_log (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    media_id VARCHAR(255) NOT NULL,
    file_size_bytes BIGINT NOT NULL,
    mime_type VARCHAR(255),
    operation VARCHAR(50) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT fk_media_usage_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX idx_media_usage_user_id ON media_usage_log(user_id);
CREATE INDEX idx_media_usage_timestamp ON media_usage_log(timestamp);
CREATE INDEX idx_media_usage_operation ON media_usage_log(operation);

-- Media quota alerts table
CREATE TABLE IF NOT EXISTS media_quota_alerts (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    alert_type VARCHAR(50) NOT NULL,
    threshold_percent INTEGER NOT NULL,
    current_usage_bytes BIGINT NOT NULL,
    quota_limit_bytes BIGINT NOT NULL,
    message TEXT,
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT fk_media_quota_alert_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX idx_media_quota_alerts_user_id ON media_quota_alerts(user_id);
CREATE INDEX idx_media_quota_alerts_is_read ON media_quota_alerts(is_read);
CREATE INDEX idx_media_quota_alerts_created_at ON media_quota_alerts(created_at);

-- Server-wide media quota table
CREATE TABLE IF NOT EXISTS server_media_quota (
    id SERIAL PRIMARY KEY,
    max_storage_bytes BIGINT NOT NULL DEFAULT 0,
    max_file_size_bytes BIGINT NOT NULL DEFAULT 104857600,
    max_files_count INTEGER NOT NULL DEFAULT 0,
    current_storage_bytes BIGINT NOT NULL DEFAULT 0,
    current_files_count INTEGER NOT NULL DEFAULT 0,
    alert_threshold_percent INTEGER NOT NULL DEFAULT 80,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_server_media_quota_updated_at ON server_media_quota(updated_at);

-- Insert default server quota
INSERT INTO server_media_quota (max_storage_bytes, max_file_size_bytes, max_files_count)
VALUES (0, 104857600, 0)
ON CONFLICT DO NOTHING;

-- Insert default quota config
INSERT INTO media_quota_config (name, description, max_storage_bytes, max_file_size_bytes, max_files_count, is_default)
VALUES ('default', 'Default media quota configuration', 10737418240, 104857600, 1000, TRUE)
ON CONFLICT DO NOTHING;

-- Insert comment
COMMENT ON TABLE media_quota_config IS 'Media quota configuration templates';
COMMENT ON TABLE user_media_quota IS 'User-specific media quota assignments';
COMMENT ON TABLE media_usage_log IS 'Media usage tracking log';
COMMENT ON TABLE media_quota_alerts IS 'Media quota alert notifications';
COMMENT ON TABLE server_media_quota IS 'Server-wide media quota settings';
