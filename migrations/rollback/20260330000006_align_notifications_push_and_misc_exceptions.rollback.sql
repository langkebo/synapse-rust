DROP INDEX IF EXISTS idx_registration_token_batches_enabled_created;
DROP INDEX IF EXISTS idx_registration_token_batches_created;
DROP INDEX IF EXISTS idx_reaction_aggregations_room_relates_origin;
DROP INDEX IF EXISTS idx_qr_login_transactions_user_created;
DROP INDEX IF EXISTS idx_qr_login_transactions_expires;
DROP INDEX IF EXISTS idx_user_notification_settings_updated;
DROP INDEX IF EXISTS idx_server_notices_sent;
DROP INDEX IF EXISTS idx_rate_limits_updated;
DROP INDEX IF EXISTS idx_push_device_user_enabled;

DROP TABLE IF EXISTS registration_token_batches;
DROP TABLE IF EXISTS reaction_aggregations;
DROP TABLE IF EXISTS qr_login_transactions;
DROP TABLE IF EXISTS server_notices;
DROP TABLE IF EXISTS user_notification_settings;
DROP TABLE IF EXISTS rate_limits;
DROP TABLE IF EXISTS push_device;
