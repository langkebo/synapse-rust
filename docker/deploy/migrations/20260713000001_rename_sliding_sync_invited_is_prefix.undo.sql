-- Undo: revert sliding_sync_rooms.is_invited back to invited

DO $$ BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = 'sliding_sync_rooms'
          AND column_name = 'is_invited'
    ) THEN
        ALTER TABLE sliding_sync_rooms RENAME COLUMN is_invited TO invited;
    END IF;
END $$;
