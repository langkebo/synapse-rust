-- Rename schema_migrations.success to is_success on existing databases.
-- OPT-029 fixed the CREATE TABLE path for new databases; this migration
-- covers databases that already have the table with the old column name.
--
-- Robust against the "both columns exist" edge case: when a previous migrate
-- run re-added the legacy `success` column (via ADD COLUMN IF NOT EXISTS) while
-- the canonical `is_success` column already existed, a plain RENAME would fail
-- with "column is_success already exists". Drop the stale `success` column in
-- that case instead, keeping the canonical `is_success`.

DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'schema_migrations'
          AND column_name = 'success'
    ) AND NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'schema_migrations'
          AND column_name = 'is_success'
    ) THEN
        ALTER TABLE schema_migrations RENAME COLUMN success TO is_success;
    ELSIF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'schema_migrations'
          AND column_name = 'success'
    ) AND EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'schema_migrations'
          AND column_name = 'is_success'
    ) THEN
        ALTER TABLE schema_migrations DROP COLUMN success;
    END IF;
END $$;
