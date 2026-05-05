-- Drop indexes created in the up migration
DROP INDEX IF EXISTS idx_users_lower_username;
DROP INDEX IF EXISTS idx_users_lower_displayname;
DROP INDEX IF EXISTS idx_users_lower_email;
DROP INDEX IF EXISTS idx_users_created_ts;
DROP INDEX IF EXISTS idx_presence_user_id;
