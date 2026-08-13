-- Rename application_services.rate_limited to is_rate_limited for v10 is_ prefix alignment
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'application_services'
          AND column_name = 'rate_limited'
    ) THEN
        ALTER TABLE application_services RENAME COLUMN rate_limited TO is_rate_limited;
    END IF;
END $$;
