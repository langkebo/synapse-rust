-- Align key_rotation_history boolean column with v10 is_ prefix convention.
-- v10 / Rust: is_revoked BOOLEAN  ←  old: revoked BOOLEAN

DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'key_rotation_history'
          AND column_name = 'revoked'
    ) THEN
        ALTER TABLE key_rotation_history RENAME COLUMN revoked TO is_revoked;
    END IF;
END $$;
