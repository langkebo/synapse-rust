-- Align room_sticky_events boolean column with v10 is_ prefix naming convention.
-- v7: sticky BOOLEAN  →  v10 / Rust: is_sticky BOOLEAN
DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'room_sticky_events'
          AND column_name = 'sticky'
    ) THEN
        ALTER TABLE room_sticky_events RENAME COLUMN sticky TO is_sticky;
    END IF;
END $$;
