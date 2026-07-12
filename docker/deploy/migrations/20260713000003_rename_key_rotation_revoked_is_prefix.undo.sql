-- Undo: revert key_rotation_history.is_revoked back to revoked

DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'key_rotation_history'
          AND column_name = 'is_revoked'
    ) THEN
        ALTER TABLE key_rotation_history RENAME COLUMN is_revoked TO revoked;
    END IF;
END $$;
