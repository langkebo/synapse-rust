-- Rename leak_alerts.acknowledged to is_acknowledged for v10 is_ prefix alignment
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'leak_alerts'
          AND column_name = 'acknowledged'
    ) THEN
        ALTER TABLE leak_alerts RENAME COLUMN acknowledged TO is_acknowledged;
    END IF;
END $$;
