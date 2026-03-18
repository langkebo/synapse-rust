-- 修复字段命名不一致问题
-- 创建日期: 2026-03-15
-- 遵循 project_rules.md 规范

-- 1. 修复 expires_ts -> expires_at (可选时间戳使用 _at 后缀)
ALTER TABLE IF EXISTS olm_sessions RENAME COLUMN expires_ts TO expires_at;
ALTER TABLE IF EXISTS captcha_send_log RENAME COLUMN expires_ts TO expires_at;
ALTER TABLE IF EXISTS email_verification_codes RENAME COLUMN expires_ts TO expires_at;
ALTER TABLE IF EXISTS password_reset_tokens RENAME COLUMN expires_ts TO expires_at;
ALTER TABLE IF EXISTS openid_tokens RENAME COLUMN expires_ts TO expires_at;
ALTER TABLE IF EXISTS cas_tickets RENAME COLUMN expires_ts TO expires_at;
ALTER TABLE IF EXISTS rendezvous_session RENAME COLUMN expires_ts TO expires_at;
ALTER TABLE IF EXISTS background_update_locks RENAME COLUMN expires_ts TO expires_at;
ALTER TABLE IF EXISTS room_ephemeral RENAME COLUMN expires_ts TO expires_at;

-- 2. 修复 revoked_ts -> revoked_at
ALTER TABLE IF EXISTS token_blacklist RENAME COLUMN revoked_ts TO revoked_at;

-- 3. 修复 validated_ts -> validated_at
ALTER TABLE IF EXISTS user_threepids RENAME COLUMN validated_ts TO validated_at;

-- 4. 修复 invalidated_ts -> revoked_at (语义更清晰)
ALTER TABLE IF EXISTS access_tokens RENAME COLUMN invalidated_ts TO revoked_at WHERE invalidated_ts IS NOT NULL;

-- 5. 修复布尔字段缺少 is_ 前缀
ALTER TABLE IF EXISTS device_keys RENAME COLUMN verified TO is_verified;
ALTER TABLE IF EXISTS device_keys RENAME COLUMN blocked TO is_blocked;

-- 6. 修复 created_at -> created_ts (统一使用 _ts 后缀)
-- 注意：这些表可能使用 TIMESTAMP 类型，需要同时转换类型
DO $$
BEGIN
    -- 检查表是否存在且字段类型为 TIMESTAMP
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'pushers' AND column_name = 'created_at' 
        AND data_type = 'timestamp with time zone'
    ) THEN
        ALTER TABLE pushers RENAME COLUMN created_at TO created_ts;
        ALTER TABLE pushers ALTER COLUMN created_ts TYPE BIGINT 
        USING (EXTRACT(EPOCH FROM created_ts) * 1000)::BIGINT;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns 
        WHERE table_name = 'push_rules' AND column_name = 'created_at'
    ) THEN
        ALTER TABLE push_rules RENAME COLUMN created_at TO created_ts;
    END IF;
END $$;

-- 7. 更新相关索引
DROP INDEX IF EXISTS idx_registration_tokens_expires_ts;
CREATE INDEX IF NOT EXISTS idx_registration_tokens_expires_at 
ON registration_tokens(expires_at) WHERE expires_at IS NOT NULL;

-- 8. 添加缺失的 created_ts 字段
ALTER TABLE IF EXISTS shadow_bans ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE IF EXISTS rate_limits ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE IF EXISTS server_notices ADD COLUMN IF NOT EXISTS created_ts BIGINT;

-- 9. 更新现有记录的 created_ts
UPDATE shadow_bans SET created_ts = banned_at WHERE created_ts IS NULL;
UPDATE rate_limits SET created_ts = (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT WHERE created_ts IS NULL;
UPDATE server_notices SET created_ts = sent_ts WHERE created_ts IS NULL;
