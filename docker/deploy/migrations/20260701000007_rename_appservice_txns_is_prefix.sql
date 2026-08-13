-- Align application_service_transactions boolean column with v10 is_ prefix convention.
-- v7: processed BOOLEAN  →  v10 / Rust: is_processed BOOLEAN
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'application_service_transactions'
          AND column_name = 'processed'
    ) THEN
        ALTER TABLE application_service_transactions RENAME COLUMN processed TO is_processed;
    END IF;
END $$;
