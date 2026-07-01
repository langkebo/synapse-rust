-- Drop the duplicate 'success' column from module_execution_logs.
-- The canonical column is 'is_success', which the Rust storage layer uses.
-- 'success' was added by 20260529000001_module_schema_alignment.sql but
-- duplicates the semantic of the existing 'is_success' column.

ALTER TABLE module_execution_logs
    DROP COLUMN IF EXISTS success;
