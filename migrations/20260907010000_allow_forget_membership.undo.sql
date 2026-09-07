ALTER TABLE room_memberships DROP CONSTRAINT IF EXISTS ck_room_memberships_valid;
ALTER TABLE room_memberships
    ADD CONSTRAINT ck_room_memberships_valid
    CHECK (membership IN ('invite', 'join', 'knock', 'leave', 'ban'));
