-- Undo: restore original ON DELETE CASCADE FK on events.room_id
-- (idempotent drop + rebuild; will FAIL if orphaned events exist — intentional safety check)
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_room_no_action;
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_room_id;
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_room;
ALTER TABLE events
    ADD CONSTRAINT fk_events_room
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE NOT DEFERRED;
