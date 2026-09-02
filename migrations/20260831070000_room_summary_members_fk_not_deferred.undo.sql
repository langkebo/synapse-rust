-- Undo: restore original DEFERRED FK constraints on room_summary_members
-- (DB-06 reversed: immediate → DEFERRED INITIALLY DEFERRED)
-- Step 1: drop immediate constraints
ALTER TABLE room_summary_members DROP CONSTRAINT IF EXISTS fk_room_summary_members_room;
ALTER TABLE room_summary_members DROP CONSTRAINT IF EXISTS fk_room_summary_members_user;
-- Step 2: restore as DEFERRED INITIALLY DEFERRED
ALTER TABLE room_summary_members
    ADD CONSTRAINT fk_room_summary_members_room
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    DEFERRABLE INITIALLY DEFERRED;
ALTER TABLE room_summary_members
    ADD CONSTRAINT fk_room_summary_members_user
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    DEFERRABLE INITIALLY DEFERRED;
