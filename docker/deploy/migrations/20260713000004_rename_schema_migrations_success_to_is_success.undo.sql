-- Undo: revert schema_migrations.is_success back to success

DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'schema_migrations'
          AND column_name = 'is_success'
    ) THEN
        ALTER TABLE schema_migrations RENAME COLUMN is_success TO success;
    END IF;
END $$;
