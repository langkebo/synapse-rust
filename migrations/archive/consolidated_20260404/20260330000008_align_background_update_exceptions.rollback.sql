DROP INDEX IF EXISTS idx_background_update_stats_created;
DROP INDEX IF EXISTS idx_background_update_history_job_start;
DROP INDEX IF EXISTS idx_background_update_locks_expires;

DROP TABLE IF EXISTS background_update_stats;
DROP TABLE IF EXISTS background_update_history;
DROP TABLE IF EXISTS background_update_locks;
