-- 修复媒体配额表结构
-- 添加缺失的列和表

-- 1. 添加 media_usage_log 表
CREATE TABLE IF NOT EXISTS media_usage_log (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    media_id TEXT NOT NULL,
    file_size_bytes BIGINT NOT NULL,
    mime_type TEXT,
    operation TEXT NOT NULL,
    timestamp BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_media_usage_log_user ON media_usage_log(user_id);
CREATE INDEX IF NOT EXISTS idx_media_usage_log_timestamp ON media_usage_log(timestamp);

-- 2. 添加 media_quota_alerts 表
CREATE TABLE IF NOT EXISTS media_quota_alerts (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    alert_type TEXT NOT NULL,
    threshold_percent INTEGER NOT NULL,
    current_usage_bytes BIGINT NOT NULL,
    quota_limit_bytes BIGINT NOT NULL,
    message TEXT,
    is_read BOOLEAN DEFAULT FALSE,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_media_quota_alerts_user ON media_quota_alerts(user_id) WHERE is_read = FALSE;

-- 3. 添加 media_quota_config 的缺失列
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_quota_config' AND column_name = 'max_storage_bytes') THEN
        ALTER TABLE media_quota_config ADD COLUMN max_storage_bytes BIGINT DEFAULT 10737418240;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_quota_config' AND column_name = 'max_files_count') THEN
        ALTER TABLE media_quota_config ADD COLUMN max_files_count INTEGER DEFAULT 10000;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_quota_config' AND column_name = 'allowed_mime_types') THEN
        ALTER TABLE media_quota_config ADD COLUMN allowed_mime_types JSONB DEFAULT '["image/jpeg", "image/png", "image/gif", "video/mp4", "audio/ogg"]';
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'media_quota_config' AND column_name = 'blocked_mime_types') THEN
        ALTER TABLE media_quota_config ADD COLUMN blocked_mime_types JSONB DEFAULT '[]';
    END IF;
END $$;

-- 4. 更新 user_media_quota 表结构
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_media_quota' AND column_name = 'quota_config_id') THEN
        ALTER TABLE user_media_quota ADD COLUMN quota_config_id BIGINT;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_media_quota' AND column_name = 'custom_max_storage_bytes') THEN
        ALTER TABLE user_media_quota ADD COLUMN custom_max_storage_bytes BIGINT;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_media_quota' AND column_name = 'custom_max_file_size_bytes') THEN
        ALTER TABLE user_media_quota ADD COLUMN custom_max_file_size_bytes BIGINT;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_media_quota' AND column_name = 'custom_max_files_count') THEN
        ALTER TABLE user_media_quota ADD COLUMN custom_max_files_count INTEGER;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_media_quota' AND column_name = 'current_storage_bytes') THEN
        ALTER TABLE user_media_quota ADD COLUMN current_storage_bytes BIGINT DEFAULT 0;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_media_quota' AND column_name = 'current_files_count') THEN
        ALTER TABLE user_media_quota ADD COLUMN current_files_count INTEGER DEFAULT 0;
    END IF;
END $$;

-- 5. 添加 server_media_quota 表
CREATE TABLE IF NOT EXISTS server_media_quota (
    id BIGSERIAL PRIMARY KEY,
    max_storage_bytes BIGINT,
    max_file_size_bytes BIGINT,
    max_files_count INTEGER,
    current_storage_bytes BIGINT DEFAULT 0,
    current_files_count INTEGER DEFAULT 0,
    alert_threshold_percent INTEGER DEFAULT 80,
    updated_ts BIGINT NOT NULL
);

-- 插入默认服务器配额记录（如果不存在）
INSERT INTO server_media_quota (id, max_storage_bytes, max_file_size_bytes, max_files_count, current_storage_bytes, current_files_count, alert_threshold_percent, updated_ts)
SELECT 1, 10995116277760, 1073741824, 1000000, 0, 0, 80, EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
WHERE NOT EXISTS (SELECT 1 FROM server_media_quota WHERE id = 1);

-- 6. 修复 key_backups 表
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'key_backups' AND column_name = 'backup_id_text') THEN
        ALTER TABLE key_backups ADD COLUMN backup_id_text TEXT;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'key_backups' AND column_name = 'mgmt_key') THEN
        ALTER TABLE key_backups ADD COLUMN mgmt_key TEXT;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'key_backups' AND column_name = 'backup_data') THEN
        ALTER TABLE key_backups ADD COLUMN backup_data JSONB;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'key_backups' AND column_name = 'etag') THEN
        ALTER TABLE key_backups ADD COLUMN etag TEXT;
    END IF;
END $$;

-- 7. 修复 rendezvous_session 表
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rendezvous_session' AND column_name = 'intent') THEN
        ALTER TABLE rendezvous_session ADD COLUMN intent TEXT;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rendezvous_session' AND column_name = 'transport') THEN
        ALTER TABLE rendezvous_session ADD COLUMN transport TEXT;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rendezvous_session' AND column_name = 'transport_data') THEN
        ALTER TABLE rendezvous_session ADD COLUMN transport_data JSONB;
    END IF;
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'rendezvous_session' AND column_name = 'key') THEN
        ALTER TABLE rendezvous_session ADD COLUMN key TEXT;
    END IF;
END $$;

-- 修复 rendezvous_messages 表
CREATE TABLE IF NOT EXISTS rendezvous_messages (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL,
    direction TEXT NOT NULL,
    message_type TEXT NOT NULL,
    content JSONB NOT NULL,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_rendezvous_messages_session ON rendezvous_messages(session_id);
