-- Align user_notification_settings boolean column with v10 is_ prefix convention.
-- v7: enabled BOOLEAN  →  v10 / Rust: is_enabled BOOLEAN

ALTER TABLE user_notification_settings
    RENAME COLUMN enabled TO is_enabled;
