-- Undo module-management schema alignment.

DROP INDEX IF EXISTS idx_media_callbacks_type_enabled;
DROP INDEX IF EXISTS idx_module_execution_logs_module_name_executed;
DROP INDEX IF EXISTS idx_modules_type_enabled_priority;

ALTER TABLE media_callbacks
    DROP COLUMN IF EXISTS completed_ts,
    DROP COLUMN IF EXISTS result,
    DROP COLUMN IF EXISTS status,
    DROP COLUMN IF EXISTS user_id,
    DROP COLUMN IF EXISTS media_id,
    DROP COLUMN IF EXISTS updated_ts,
    DROP COLUMN IF EXISTS retry_count,
    DROP COLUMN IF EXISTS timeout_ms,
    DROP COLUMN IF EXISTS method;

ALTER TABLE module_execution_logs
    ALTER COLUMN executed_ts DROP NOT NULL,
    ALTER COLUMN created_ts DROP DEFAULT,
    ALTER COLUMN execution_type DROP DEFAULT,
    DROP COLUMN IF EXISTS executed_ts,
    DROP COLUMN IF EXISTS metadata,
    DROP COLUMN IF EXISTS success,
    DROP COLUMN IF EXISTS room_id,
    DROP COLUMN IF EXISTS event_id,
    DROP COLUMN IF EXISTS module_type,
    DROP COLUMN IF EXISTS module_name;

ALTER TABLE modules
    ALTER COLUMN updated_ts DROP NOT NULL,
    DROP COLUMN IF EXISTS last_error,
    DROP COLUMN IF EXISTS error_count,
    DROP COLUMN IF EXISTS execution_count,
    DROP COLUMN IF EXISTS last_executed_ts,
    DROP COLUMN IF EXISTS version;
