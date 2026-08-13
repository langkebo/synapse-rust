-- Rename push_notification_log.success to is_success for v10 is_ prefix alignment
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'push_notification_log'
          AND column_name = 'success'
    ) THEN
        ALTER TABLE push_notification_log RENAME COLUMN success TO is_success;
    END IF;
END $$;
