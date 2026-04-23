-- ============================================================================
-- 数据库结构对齐迁移 — 回滚
-- ============================================================================

ALTER TABLE device_keys DROP COLUMN IF EXISTS is_fallback;
DROP INDEX IF EXISTS idx_device_keys_fallback;

DROP TABLE IF EXISTS to_device_transactions CASCADE;

ALTER TABLE push_rules DROP COLUMN IF EXISTS priority_class;

ALTER TABLE push_notification_queue DROP COLUMN IF EXISTS priority;
ALTER TABLE push_notification_queue DROP COLUMN IF EXISTS status;
ALTER TABLE push_notification_queue DROP COLUMN IF EXISTS attempts;
ALTER TABLE push_notification_queue DROP COLUMN IF EXISTS max_attempts;
ALTER TABLE push_notification_queue DROP COLUMN IF EXISTS next_attempt_at;
ALTER TABLE push_notification_queue DROP COLUMN IF EXISTS sent_at;
ALTER TABLE push_notification_queue DROP COLUMN IF EXISTS error_message;

ALTER TABLE push_notification_log DROP COLUMN IF EXISTS event_id;
ALTER TABLE push_notification_log DROP COLUMN IF EXISTS room_id;
ALTER TABLE push_notification_log DROP COLUMN IF EXISTS notification_type;
ALTER TABLE push_notification_log DROP COLUMN IF EXISTS push_type;
ALTER TABLE push_notification_log DROP COLUMN IF EXISTS sent_at;
ALTER TABLE push_notification_log DROP COLUMN IF EXISTS success;
ALTER TABLE push_notification_log DROP COLUMN IF EXISTS provider_response;
ALTER TABLE push_notification_log DROP COLUMN IF EXISTS response_time_ms;
ALTER TABLE push_notification_log DROP COLUMN IF EXISTS metadata;

ALTER TABLE push_config DROP COLUMN IF EXISTS config_key;
ALTER TABLE push_config DROP COLUMN IF EXISTS config_value;

ALTER TABLE e2ee_key_requests DROP COLUMN IF EXISTS updated_ts;

-- 第二轮回滚
ALTER TABLE federation_blacklist DROP COLUMN IF EXISTS block_type;
ALTER TABLE federation_blacklist DROP COLUMN IF EXISTS blocked_by;
ALTER TABLE federation_blacklist DROP COLUMN IF EXISTS created_ts;
ALTER TABLE federation_blacklist DROP COLUMN IF EXISTS expires_at;
ALTER TABLE federation_blacklist DROP COLUMN IF EXISTS is_enabled;
ALTER TABLE federation_blacklist DROP COLUMN IF EXISTS metadata;

ALTER TABLE event_signatures ALTER COLUMN algorithm DROP DEFAULT;

ALTER TABLE push_notification_queue ALTER COLUMN event_id SET NOT NULL;
ALTER TABLE push_notification_queue ALTER COLUMN room_id SET NOT NULL;
ALTER TABLE push_notification_queue ALTER COLUMN notification_type SET NOT NULL;

ALTER TABLE push_notification_log ALTER COLUMN pushkey SET NOT NULL;
ALTER TABLE push_notification_log ALTER COLUMN status SET NOT NULL;

ALTER TABLE user_privacy_settings DROP COLUMN IF EXISTS profile_visibility;
ALTER TABLE user_privacy_settings DROP COLUMN IF EXISTS avatar_visibility;
ALTER TABLE user_privacy_settings DROP COLUMN IF EXISTS displayname_visibility;
ALTER TABLE user_privacy_settings DROP COLUMN IF EXISTS presence_visibility;
ALTER TABLE user_privacy_settings DROP COLUMN IF EXISTS room_membership_visibility;

-- 第三轮回滚
ALTER TABLE e2ee_secret_storage_keys DROP COLUMN IF EXISTS encrypted_key;
ALTER TABLE e2ee_secret_storage_keys DROP COLUMN IF EXISTS public_key;
ALTER TABLE e2ee_secret_storage_keys DROP COLUMN IF EXISTS signatures;

ALTER TABLE e2ee_stored_secrets DROP COLUMN IF EXISTS encrypted_secret;
ALTER TABLE e2ee_stored_secrets DROP COLUMN IF EXISTS key_id;

ALTER TABLE e2ee_audit_log DROP COLUMN IF EXISTS operation;
ALTER TABLE e2ee_audit_log DROP COLUMN IF EXISTS key_id;
ALTER TABLE e2ee_audit_log DROP COLUMN IF EXISTS ip_address;

-- 第四轮回滚
ALTER TABLE registration_token_usage DROP COLUMN IF EXISTS token;
ALTER TABLE registration_token_usage DROP COLUMN IF EXISTS username;
ALTER TABLE registration_token_usage DROP COLUMN IF EXISTS email;
ALTER TABLE registration_token_usage DROP COLUMN IF EXISTS ip_address;
ALTER TABLE registration_token_usage DROP COLUMN IF EXISTS user_agent;
ALTER TABLE registration_token_usage DROP COLUMN IF EXISTS success;
ALTER TABLE registration_token_usage DROP COLUMN IF EXISTS error_message;

ALTER TABLE room_invites DROP COLUMN IF EXISTS invite_code;
ALTER TABLE room_invites DROP COLUMN IF EXISTS inviter_user_id;
ALTER TABLE room_invites DROP COLUMN IF EXISTS invitee_email;
ALTER TABLE room_invites DROP COLUMN IF EXISTS invitee_user_id;
ALTER TABLE room_invites DROP COLUMN IF EXISTS is_used;
ALTER TABLE room_invites DROP COLUMN IF EXISTS is_revoked;
ALTER TABLE room_invites DROP COLUMN IF EXISTS used_ts;
ALTER TABLE room_invites DROP COLUMN IF EXISTS revoked_at;
ALTER TABLE room_invites DROP COLUMN IF EXISTS revoked_reason;

ALTER TABLE application_service_state DROP COLUMN IF EXISTS state_value;

ALTER TABLE application_service_transactions DROP COLUMN IF EXISTS transaction_id;
ALTER TABLE application_service_transactions DROP COLUMN IF EXISTS events;
ALTER TABLE application_service_transactions DROP COLUMN IF EXISTS sent_ts;
ALTER TABLE application_service_transactions DROP COLUMN IF EXISTS completed_ts;
ALTER TABLE application_service_transactions DROP COLUMN IF EXISTS retry_count;
ALTER TABLE application_service_transactions DROP COLUMN IF EXISTS last_error;

ALTER TABLE registration_tokens ALTER COLUMN created_by SET NOT NULL;
