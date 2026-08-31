-- DB-06 undo: Restore DEFERRABLE INITIALLY DEFERRED on room_summary_members FKs
--
-- IMPORTANT: Verify there are no orphan rows before running this undo.
-- Run:
--
--   SELECT COUNT(*) FROM room_summary_members m
--   LEFT JOIN rooms r ON r.room_id = m.room_id
--   WHERE r.room_id IS NULL;
--
--   SELECT COUNT(*) FROM room_summary_members m
--   LEFT JOIN users u ON u.user_id = m.user_id
--   WHERE u.user_id IS NULL;
--
-- If either returns > 0, investigate and clean up before proceeding.
-- Then:

ALTER TABLE room_summary_members DROP CONSTRAINT IF EXISTS fk_room_summary_members_room;
ALTER TABLE room_summary_members DROP CONSTRAINT IF EXISTS fk_room_summary_members_user;

ALTER TABLE room_summary_members
    ADD CONSTRAINT fk_room_summary_members_room
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    DEFERRABLE INITIALLY DEFERRED
    NOT VALID;

ALTER TABLE room_summary_members
    ADD CONSTRAINT fk_room_summary_members_user
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    DEFERRABLE INITIALLY DEFERRED
    NOT VALID;

ALTER TABLE room_summary_members VALIDATE CONSTRAINT fk_room_summary_members_room;
ALTER TABLE room_summary_members VALIDATE CONSTRAINT fk_room_summary_members_user;

COMMENT ON CONSTRAINT fk_room_summary_members_room ON room_summary_members IS
    'Restored DEFERRABLE INITIALLY DEFERRED (DB-06 undo)';

COMMENT ON CONSTRAINT fk_room_summary_members_user ON room_summary_members IS
    'Restored DEFERRABLE INITIALLY DEFERRED (DB-06 undo)';
