-- Rename registration_token_usage.success to is_success for v10 is_ prefix alignment
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'registration_token_usage'
          AND column_name = 'success'
    ) THEN
        ALTER TABLE registration_token_usage RENAME COLUMN success TO is_success;
    END IF;
END $$;
