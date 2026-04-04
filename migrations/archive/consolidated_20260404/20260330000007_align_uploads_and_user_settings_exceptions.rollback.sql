DROP INDEX IF EXISTS idx_upload_chunks_upload_order;
DROP INDEX IF EXISTS idx_upload_progress_user_created_active;
DROP INDEX IF EXISTS idx_upload_progress_expires;

DROP TABLE IF EXISTS upload_chunks;
DROP TABLE IF EXISTS upload_progress;
DROP TABLE IF EXISTS user_settings;
