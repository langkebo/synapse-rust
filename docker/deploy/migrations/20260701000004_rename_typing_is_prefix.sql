-- Align typing table boolean column with v10 is_ prefix naming convention.
-- v7: typing BOOLEAN  →  v10 / Rust: is_typing BOOLEAN
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'typing'
          AND column_name = 'typing'
    ) THEN
        ALTER TABLE typing RENAME COLUMN typing TO is_typing;
    END IF;
END $$;
