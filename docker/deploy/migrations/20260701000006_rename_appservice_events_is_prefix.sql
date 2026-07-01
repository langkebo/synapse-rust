-- Align application_service_events boolean column with v10 is_ prefix convention.
-- v7: processed BOOLEAN  →  v10 / Rust: is_processed BOOLEAN

ALTER TABLE application_service_events
    RENAME COLUMN processed TO is_processed;
