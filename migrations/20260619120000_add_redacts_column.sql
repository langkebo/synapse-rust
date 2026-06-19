-- P0-05: Add redacts column to events table for Matrix redaction event support.
-- The redacts column stores the target event_id that a m.room.redaction event
-- refers to.  For room versions 1-10 this is populated from the top-level
-- `redacts` field of the PDU; for v11+ (MSC2174/MSC3820) it is populated from
-- `content.redacts`.  The column is nullable because non-redaction events do
-- not have a redacts target.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'events' AND column_name = 'redacts'
    ) THEN
        ALTER TABLE events ADD COLUMN redacts TEXT;
    END IF;
END $$;

-- Index to quickly look up redaction events targeting a specific event.
CREATE INDEX IF NOT EXISTS idx_events_redacts ON events(redacts) WHERE redacts IS NOT NULL;
