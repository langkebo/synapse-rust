-- Undo: restore original DEFERRED FK constraints on room_summary_members
-- (DB-06 reversed: immediate → DEFERRED INITIALLY DEFERRED)
--
-- DDL is pinned to `current_schema()` for the same reason as the forward
-- migration: unqualified `room_summary_members` / `rooms` / `users` resolve
-- through `search_path`, so a leftover copy of these tables in `public` can
-- silently capture the constraint (see the header of
-- 20260831070000_room_summary_members_fk_not_deferred.sql for the full
-- incident write-up).
DO $$
DECLARE
    target text := format('%I.%I', current_schema(), 'room_summary_members');
BEGIN
    IF to_regclass(target) IS NULL THEN
        RAISE NOTICE 'room_summary_members not present in schema %, skipping DB-06 undo', current_schema();
        RETURN;
    END IF;

    -- Step 1: drop immediate constraints.
    EXECUTE format('ALTER TABLE %s DROP CONSTRAINT IF EXISTS fk_room_summary_members_room', target);
    EXECUTE format('ALTER TABLE %s DROP CONSTRAINT IF EXISTS fk_room_summary_members_user', target);

    -- Step 2: restore as DEFERRED INITIALLY DEFERRED, bound to the current schema.
    EXECUTE format(
        'ALTER TABLE %s ADD CONSTRAINT fk_room_summary_members_room '
        'FOREIGN KEY (room_id) REFERENCES %I.rooms(room_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED',
        target,
        current_schema()
    );

    EXECUTE format(
        'ALTER TABLE %s ADD CONSTRAINT fk_room_summary_members_user '
        'FOREIGN KEY (user_id) REFERENCES %I.users(user_id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED',
        target,
        current_schema()
    );
END $$;
