-- ============================================================================
-- 修复媒体配额相关表结构
-- 创建日期: 2026-03-11
-- 说明: 创建缺失的 user_media_quota 表
-- ============================================================================

-- 创建 user_media_quota 表
CREATE TABLE IF NOT EXISTS user_media_quota (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    quota_config_id BIGINT,
    custom_max_storage_bytes BIGINT,
    custom_max_file_size_bytes BIGINT,
    custom_max_files_count INTEGER,
    current_storage_bytes BIGINT NOT NULL DEFAULT 0,
    current_files_count INTEGER NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT uq_user_media_quota_user_id UNIQUE (user_id),
    CONSTRAINT fk_user_media_quota_config FOREIGN KEY (quota_config_id) REFERENCES server_media_quota(id) ON DELETE SET NULL
);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_user_media_quota_user_id ON user_media_quota(user_id);
CREATE INDEX IF NOT EXISTS idx_user_media_quota_config_id ON user_media_quota(quota_config_id);

-- 插入迁移记录
INSERT INTO schema_migrations (version, name, applied_ts, description)
VALUES (
    'v6.0.5', 
    'fix_media_quota_tables', 
    EXTRACT(EPOCH FROM NOW()) * 1000, 
    'Create user_media_quota table for media quota API'
) ON CONFLICT (version) DO NOTHING;
