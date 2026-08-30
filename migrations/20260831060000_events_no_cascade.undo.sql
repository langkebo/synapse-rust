-- DB-04-b undo: Restore events.room_id ON DELETE CASCADE
--
-- IMPORTANT: Before running this undo, verify there are NO orphan rows in
-- events that reference deleted rooms. If the Rust layer's batch DELETE
-- failed mid-way, orphan rows may exist. Run:
--
--   SELECT COUNT(*) FROM events e
--   LEFT JOIN rooms r ON r.room_id = e.room_id
--   WHERE r.room_id IS NULL;
--
-- If > 0, manually delete them before running this undo:
--
--   DELETE FROM events WHERE event_id IN (
--       SELECT e.event_id FROM events e
--       LEFT JOIN rooms r ON r.room_id = e.room_id
--       WHERE r.room_id IS NULL
--       LIMIT 1000
--   );  -- repeat until 0
--
-- Then:
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_room_no_action;
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_room;

ALTER TABLE events
    ADD CONSTRAINT fk_events_room
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    NOT VALID;

ALTER TABLE events VALIDATE CONSTRAINT fk_events_room;

-- Also restore the secondary CASCADE constraint from v10 baseline (line 4272)
-- to keep the schema byte-identical to pre-DB-04-b state.
ALTER TABLE events
    ADD CONSTRAINT fk_events_room_id
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    NOT VALID;

ALTER TABLE events VALIDATE CONSTRAINT fk_events_room_id;