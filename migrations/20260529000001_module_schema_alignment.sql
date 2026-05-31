-- Align module-management tables with the storage layer fields used at runtime.

ALTER TABLE modules
    ADD COLUMN IF NOT EXISTS version TEXT NOT NULL DEFAULT '1.0.0',
    ADD COLUMN IF NOT EXISTS last_executed_ts BIGINT,
    ADD COLUMN IF NOT EXISTS execution_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS error_count INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS last_error TEXT;

UPDATE modules
SET updated_ts = created_ts
WHERE updated_ts IS NULL;

ALTER TABLE modules
    ALTER COLUMN updated_ts SET NOT NULL;

ALTER TABLE module_execution_logs
    ADD COLUMN IF NOT EXISTS module_name TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS module_type TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS event_id TEXT,
    ADD COLUMN IF NOT EXISTS room_id TEXT,
    ADD COLUMN IF NOT EXISTS success BOOLEAN NOT NULL DEFAULT TRUE,
    ADD COLUMN IF NOT EXISTS metadata JSONB,
    ADD COLUMN IF NOT EXISTS executed_ts BIGINT;

UPDATE module_execution_logs
SET executed_ts = created_ts
WHERE executed_ts IS NULL;

ALTER TABLE module_execution_logs
    ALTER COLUMN execution_type SET DEFAULT 'module_execution',
    ALTER COLUMN created_ts SET DEFAULT ((EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT),
    ALTER COLUMN executed_ts SET NOT NULL;

ALTER TABLE media_callbacks
    ADD COLUMN IF NOT EXISTS method TEXT NOT NULL DEFAULT 'POST',
    ADD COLUMN IF NOT EXISTS timeout_ms INTEGER NOT NULL DEFAULT 5000,
    ADD COLUMN IF NOT EXISTS retry_count INTEGER NOT NULL DEFAULT 3,
    ADD COLUMN IF NOT EXISTS updated_ts BIGINT,
    ADD COLUMN IF NOT EXISTS media_id TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS user_id TEXT NOT NULL DEFAULT '',
    ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'pending',
    ADD COLUMN IF NOT EXISTS result JSONB,
    ADD COLUMN IF NOT EXISTS completed_ts BIGINT;

UPDATE media_callbacks
SET updated_ts = created_ts
WHERE updated_ts IS NULL;

ALTER TABLE media_callbacks
    ALTER COLUMN updated_ts SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_modules_type_enabled_priority
    ON modules(module_type, is_enabled, priority);

CREATE INDEX IF NOT EXISTS idx_module_execution_logs_module_name_executed
    ON module_execution_logs(module_name, executed_ts DESC);

CREATE INDEX IF NOT EXISTS idx_media_callbacks_type_enabled
    ON media_callbacks(callback_type, is_enabled);
