-- Undo: restore the duplicate success column.

ALTER TABLE module_execution_logs
    ADD COLUMN IF NOT EXISTS success BOOLEAN NOT NULL DEFAULT TRUE;
