-- Undo: restore original ON DELETE CASCADE FK on events.room_id
-- (idempotent drop + rebuild; will FAIL if orphaned events exist — intentional safety check)
--
-- DDL is pinned to `current_schema()` instead of `search_path` for the same
-- reason as the forward migration: a leftover copy of `events`/`rooms` in
-- `public` must never capture the constraint. See the header of
-- 20260831070000_room_summary_members_fk_not_deferred.sql.
DO $$
DECLARE
    events_tbl text := format('%I.%I', current_schema(), 'events');
BEGIN
    IF to_regclass(events_tbl) IS NULL THEN
        RAISE NOTICE 'events not present in schema %, skipping DB-04-b undo', current_schema();
        RETURN;
    END IF;

    EXECUTE format('ALTER TABLE %s DROP CONSTRAINT IF EXISTS fk_events_room_no_action', events_tbl);
    EXECUTE format('ALTER TABLE %s DROP CONSTRAINT IF EXISTS fk_events_room_id', events_tbl);
    EXECUTE format('ALTER TABLE %s DROP CONSTRAINT IF EXISTS fk_events_room', events_tbl);

    EXECUTE format(
        'ALTER TABLE %s ADD CONSTRAINT fk_events_room '
        'FOREIGN KEY (room_id) REFERENCES %I.rooms(room_id) ON DELETE CASCADE NOT DEFERRED',
        events_tbl,
        current_schema()
    );
END $$;
