-- Rename schema_migrations.success to is_success on existing databases.
-- OPT-029 fixed the CREATE TABLE path for new databases; this migration
-- covers databases that already have the table with the old column name.

DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'schema_migrations'
          AND column_name = 'success'
    ) THEN
        ALTER TABLE schema_migrations RENAME COLUMN success TO is_success;
    END IF;
END $$;
