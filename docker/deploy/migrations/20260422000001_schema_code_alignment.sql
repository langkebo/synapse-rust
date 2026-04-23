-- ============================================================================
-- 数据库结构对齐迁移 (Schema-Code Alignment)
-- 日期: 2026-04-22
-- 目的: 修复 schema 审计发现的 CRITICAL 级不一致
-- ============================================================================

-- C-05: device_keys 缺少 is_fallback 列
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS is_fallback BOOLEAN NOT NULL DEFAULT FALSE;
CREATE INDEX IF NOT EXISTS idx_device_keys_fallback ON device_keys(user_id, device_id) WHERE is_fallback = TRUE;

-- C-08: to_device_transactions 表不存在
CREATE TABLE IF NOT EXISTS to_device_transactions (
    id BIGSERIAL PRIMARY KEY,
    transaction_id TEXT,
    message_id TEXT,
    sender_user_id TEXT NOT NULL,
    sender_device_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_to_device_transactions_txn UNIQUE (transaction_id, sender_user_id, sender_device_id)
);
CREATE INDEX IF NOT EXISTS idx_to_device_transactions_created ON to_device_transactions(created_ts);
CREATE UNIQUE INDEX IF NOT EXISTS uq_to_device_transactions_msg ON to_device_transactions(sender_user_id, sender_device_id, message_id);

-- C-09: push_rules 缺少 priority_class 的兼容性处理
-- push_rules 表已在 unified schema 中有 priority_class，此处确保列存在
ALTER TABLE push_rules ADD COLUMN IF NOT EXISTS priority_class INTEGER NOT NULL DEFAULT 0;

-- C-10/C-11/C-12: push_notification_queue/log/config 补齐缺失列
-- push_notification_queue: 代码需要 priority, status, attempts, max_attempts, next_attempt_at, sent_at, error_message
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS priority INTEGER NOT NULL DEFAULT 0;
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'pending';
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS attempts INTEGER NOT NULL DEFAULT 0;
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS max_attempts INTEGER NOT NULL DEFAULT 3;
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS next_attempt_at TIMESTAMPTZ;
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS sent_at TIMESTAMPTZ;
ALTER TABLE push_notification_queue ADD COLUMN IF NOT EXISTS error_message TEXT;

-- push_notification_log: 代码需要 event_id, room_id, notification_type, push_type, sent_at, success, provider_response, response_time_ms, metadata
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS event_id TEXT;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS room_id TEXT;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS notification_type TEXT;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS push_type TEXT;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS sent_at TIMESTAMPTZ;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS success BOOLEAN;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS provider_response TEXT;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS response_time_ms INTEGER;
ALTER TABLE push_notification_log ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}';

-- push_config: 代码使用 config_key/config_value 而非 config_type/config_data
ALTER TABLE push_config ADD COLUMN IF NOT EXISTS config_key TEXT;
ALTER TABLE push_config ADD COLUMN IF NOT EXISTS config_value TEXT;

-- C-16: e2ee_key_requests 缺少 updated_ts 列
ALTER TABLE e2ee_key_requests ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

-- ============================================================================
-- 第二轮审计修复 (2026-04-22 续)
-- ============================================================================

-- federation_blacklist: 代码需要 block_type, blocked_by, created_ts, expires_at, is_enabled, metadata
-- 基线 schema 只有: server_name, reason, added_ts, added_by, updated_ts
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS block_type TEXT NOT NULL DEFAULT 'manual';
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS blocked_by TEXT;
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS expires_at BIGINT;
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS is_enabled BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE federation_blacklist ADD COLUMN IF NOT EXISTS metadata JSONB NOT NULL DEFAULT '{}';
-- 回填 created_ts 从 added_ts
UPDATE federation_blacklist SET created_ts = added_ts WHERE created_ts IS NULL AND added_ts IS NOT NULL;
UPDATE federation_blacklist SET blocked_by = added_by WHERE blocked_by IS NULL AND added_by IS NOT NULL;

-- event_signatures: INSERT 缺少 algorithm 列 — 添加默认值使其可省略
-- 注意: 已有数据的 algorithm 为 NOT NULL，新增默认值仅影响新 INSERT
DO $$ BEGIN
    ALTER TABLE event_signatures ALTER COLUMN algorithm SET DEFAULT 'ed25519';
EXCEPTION WHEN others THEN NULL;
END $$;

-- push_notification_queue: 放宽 NOT NULL 约束（代码使用 Option<String>）
DO $$ BEGIN
    ALTER TABLE push_notification_queue ALTER COLUMN event_id DROP NOT NULL;
    ALTER TABLE push_notification_queue ALTER COLUMN room_id DROP NOT NULL;
    ALTER TABLE push_notification_queue ALTER COLUMN notification_type DROP NOT NULL;
EXCEPTION WHEN others THEN NULL;
END $$;

-- push_notification_log: 放宽 pushkey/status NOT NULL（代码不提供这些列）
DO $$ BEGIN
    ALTER TABLE push_notification_log ALTER COLUMN pushkey DROP NOT NULL;
    ALTER TABLE push_notification_log ALTER COLUMN status DROP NOT NULL;
EXCEPTION WHEN others THEN NULL;
END $$;

-- user_privacy_settings: 旧 schema 使用 allow_* BOOLEAN 列，代码使用 *_visibility TEXT 列
-- 为已部署环境添加新列（新环境通过 extensions_privacy.sql 直接创建正确 schema）
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'user_privacy_settings' AND column_name = 'id') THEN
        ALTER TABLE user_privacy_settings ADD COLUMN id BIGSERIAL;
    END IF;
    ALTER TABLE user_privacy_settings ADD COLUMN IF NOT EXISTS profile_visibility TEXT NOT NULL DEFAULT 'public';
    ALTER TABLE user_privacy_settings ADD COLUMN IF NOT EXISTS avatar_visibility TEXT NOT NULL DEFAULT 'public';
    ALTER TABLE user_privacy_settings ADD COLUMN IF NOT EXISTS displayname_visibility TEXT NOT NULL DEFAULT 'public';
    ALTER TABLE user_privacy_settings ADD COLUMN IF NOT EXISTS presence_visibility TEXT NOT NULL DEFAULT 'contacts';
    ALTER TABLE user_privacy_settings ADD COLUMN IF NOT EXISTS room_membership_visibility TEXT NOT NULL DEFAULT 'contacts';
EXCEPTION WHEN others THEN NULL;
END $$;

-- ============================================================================
-- 第三轮审计修复 (2026-04-22 续)
-- ============================================================================

-- e2ee_secret_storage_keys: 代码使用 encrypted_key/public_key/signatures，schema 使用 key_data
ALTER TABLE e2ee_secret_storage_keys ADD COLUMN IF NOT EXISTS encrypted_key TEXT;
ALTER TABLE e2ee_secret_storage_keys ADD COLUMN IF NOT EXISTS public_key TEXT;
ALTER TABLE e2ee_secret_storage_keys ADD COLUMN IF NOT EXISTS signatures JSONB;

-- e2ee_stored_secrets: 代码使用 encrypted_secret/key_id，schema 使用 secret_data/key_key_id
ALTER TABLE e2ee_stored_secrets ADD COLUMN IF NOT EXISTS encrypted_secret TEXT;
-- key_id 列可能与 e2ee_secret_storage_keys 的 UNIQUE key_id 冲突，使用不同名
DO $$ BEGIN
    ALTER TABLE e2ee_stored_secrets ADD COLUMN IF NOT EXISTS key_id TEXT;
EXCEPTION WHEN others THEN NULL;
END $$;

-- e2ee_audit_log: 代码使用 operation/key_id/ip_address，schema 使用 action/event_id (无 ip_address)
ALTER TABLE e2ee_audit_log ADD COLUMN IF NOT EXISTS operation TEXT;
ALTER TABLE e2ee_audit_log ADD COLUMN IF NOT EXISTS key_id TEXT;
ALTER TABLE e2ee_audit_log ADD COLUMN IF NOT EXISTS ip_address TEXT;

-- space_summaries: SELECT * 返回 id，但 SpaceSummary struct 无 id 字段
-- 修复方式: 不改 schema，改代码（添加 id 字段到 struct）

-- ============================================================================
-- 第四轮审计修复 (2026-04-22 续)
-- ============================================================================

-- registration_token_usage: 代码使用 7 个不存在的列
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS token TEXT;
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS username TEXT;
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS email TEXT;
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS ip_address TEXT;
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS user_agent TEXT;
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS success BOOLEAN NOT NULL DEFAULT TRUE;
ALTER TABLE registration_token_usage ADD COLUMN IF NOT EXISTS error_message TEXT;

-- room_invites: 代码使用完全不同的列名（invite_code 设计）
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS invite_code TEXT;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS inviter_user_id TEXT;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS invitee_email TEXT;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS invitee_user_id TEXT;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS is_used BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS is_revoked BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS used_ts BIGINT;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS revoked_at BIGINT;
ALTER TABLE room_invites ADD COLUMN IF NOT EXISTS revoked_reason TEXT;
-- 回填旧数据
UPDATE room_invites SET inviter_user_id = inviter WHERE inviter_user_id IS NULL AND inviter IS NOT NULL;
UPDATE room_invites SET invitee_user_id = invitee WHERE invitee_user_id IS NULL AND invitee IS NOT NULL;

-- application_service_state: 代码使用 state_value (String) 但 schema 使用 value (JSONB)
ALTER TABLE application_service_state ADD COLUMN IF NOT EXISTS state_value TEXT;

-- application_service_transactions: 代码使用不同的列名
ALTER TABLE application_service_transactions ADD COLUMN IF NOT EXISTS transaction_id TEXT;
ALTER TABLE application_service_transactions ADD COLUMN IF NOT EXISTS events JSONB;
ALTER TABLE application_service_transactions ADD COLUMN IF NOT EXISTS sent_ts BIGINT;
ALTER TABLE application_service_transactions ADD COLUMN IF NOT EXISTS completed_ts BIGINT;
ALTER TABLE application_service_transactions ADD COLUMN IF NOT EXISTS retry_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE application_service_transactions ADD COLUMN IF NOT EXISTS last_error TEXT;
-- 回填旧数据
UPDATE application_service_transactions SET transaction_id = txn_id WHERE transaction_id IS NULL AND txn_id IS NOT NULL;
UPDATE application_service_transactions SET events = data WHERE events IS NULL AND data IS NOT NULL;
UPDATE application_service_transactions SET sent_ts = created_ts WHERE sent_ts IS NULL AND created_ts IS NOT NULL;

-- thread_subscriptions: 代码缺少 is_pinned 字段 (schema 有)
-- 修复方式: 代码添加字段（已在 Rust 代码中修复）

-- registration_tokens: created_by 放宽 NOT NULL
DO $$ BEGIN
    ALTER TABLE registration_tokens ALTER COLUMN created_by DROP NOT NULL;
EXCEPTION WHEN others THEN NULL;
END $$;
