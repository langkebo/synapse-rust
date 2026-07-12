-- Align sliding_sync_rooms boolean column with v10 is_ prefix convention.
-- v10 / Rust: is_invited BOOLEAN  ←  old: invited BOOLEAN

DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'sliding_sync_rooms'
          AND column_name = 'invited'
    ) THEN
        ALTER TABLE sliding_sync_rooms RENAME COLUMN invited TO is_invited;
    END IF;
END $$;
