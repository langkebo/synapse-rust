-- Align user_notification_settings boolean column with v10 is_ prefix convention.
-- v7: enabled BOOLEAN  →  v10 / Rust: is_enabled BOOLEAN
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'user_notification_settings'
          AND column_name = 'enabled'
    ) THEN
        ALTER TABLE user_notification_settings RENAME COLUMN enabled TO is_enabled;
    END IF;
END $$;
