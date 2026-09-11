-- DB-06: Remove DEFERRABLE from room_summary_members foreign keys
-- See: artifacts/数据库架构诊断报告-2026-08-30.md §P0-2
--
-- ## Why
--
-- The two foreign keys on room_summary_members were defined as
-- `DEFERRABLE INITIALLY DEFERRED`, meaning the constraint check fires
-- only at COMMIT time, not at each row modification.
--
-- This was masking a data drift risk in the service layer: when a user
-- joins a room, MembershipService::add_member calls:
--   1. member_storage.add_member(room_memberships)    — auto-commit
--   2. summary_service.add_member(room_summary_members) — auto-commit
-- If step 2 fails (e.g. network blip, constraint violation), step 1 has
-- already committed. The DEFERRED FK would not notice until some future
-- transaction tried to COMMIT while the room was missing — creating a
-- silent, time-shifted failure mode.
--
-- ## Why the DDL below is built with format()/current_schema()
--
-- This migration previously used plain `ALTER TABLE room_summary_members
-- ... REFERENCES rooms(room_id)`. Postgres resolves BOTH the child table
-- and the referenced parent table through `search_path` at execution time.
-- The test harness applies migrations into a per-test/template schema while
-- `public` may *also* contain a leftover copy of these tables, so the FK was
-- silently bound to whichever schema won the search_path race. Observed
-- corruption (2026-09-12): `public.room_summary_members` carried
-- `fk_room_summary_members_room REFERENCES test_51027_403_...rooms(room_id)`
-- — a transient test schema — and `test_<pid>_<n>_<ts>.room_summary_members`
-- was left with NO FK at all.
--
-- Consequence: every test that reaches `room_summary_members` through
-- `public.rooms` (all `synapse-storage` suites that hand-roll
-- `test_pool()`) died with SQLSTATE 23503 for a room that demonstrably
-- existed, which reads as an application bug and is not one.
--
-- Building the identifiers from `current_schema()` pins every DDL statement
-- to the schema migrations are actually being applied to, no matter what
-- `search_path` contains. This mirrors the existing convention in this
-- repository (see `00000000_unified_schema_v11.sql:4880`,
-- `20260810120000_add_secret_key_to_verification_sas.sql:12`).
--
-- `format('%I.%I', ...)` is used for the table names (identifier quoting),
-- and `%I` for the table argument of `to_regclass()` so it is resolved as a
-- qualified relation name.
--
-- ## What this migration does
--
-- Step 1: Drop the two DEFERRED constraints (idempotent).
-- Step 2: Re-add them without DEFERRABLE (immediate check), NOT VALID.
-- Step 3: VALIDATE (non-blocking, ShareUpdateExclusiveLock).
-- Step 4: Comment for future maintainers.

-- Steps 1-4: all DDL is pinned to `current_schema()`.
DO $$
DECLARE
    target text := format('%I.%I', current_schema(), 'room_summary_members');
BEGIN
    -- The child table must exist in the schema migrations are being applied to.
    -- If it does not, this schema was never given the room-summary tables and
    -- there is nothing to constrain (the idempotency guard below would fail
    -- otherwise with `relation does not exist` rather than doing nothing).
    IF to_regclass(target) IS NULL THEN
        RAISE NOTICE 'room_summary_members not present in schema %, skipping DB-06 FK rewrite', current_schema();
        RETURN;
    END IF;

    -- Step 1: Drop the deferred FK constraints (idempotent).
    EXECUTE format('ALTER TABLE %s DROP CONSTRAINT IF EXISTS fk_room_summary_members_room', target);
    EXECUTE format('ALTER TABLE %s DROP CONSTRAINT IF EXISTS fk_room_summary_members_user', target);

    -- Step 2: Re-add as immediate (non-deferred) constraints, bound explicitly
    -- to the current schema so a leftover `public` copy can never capture them.
    EXECUTE format(
        'ALTER TABLE %s ADD CONSTRAINT fk_room_summary_members_room '
        'FOREIGN KEY (room_id) REFERENCES %I.rooms(room_id) ON DELETE CASCADE NOT VALID',
        target,
        current_schema()
    );

    EXECUTE format(
        'ALTER TABLE %s ADD CONSTRAINT fk_room_summary_members_user '
        'FOREIGN KEY (user_id) REFERENCES %I.users(user_id) ON DELETE CASCADE NOT VALID',
        target,
        current_schema()
    );

    -- Step 3: Validate (non-blocking). A validation failure must not abort the
    -- migration: `NOT VALID` already enforces the constraint for all future
    -- writes, which is the actual goal. Aborting here would make an otherwise
    -- healthy deployment un-migratable because of pre-existing orphan rows.
    BEGIN
        EXECUTE format('ALTER TABLE %s VALIDATE CONSTRAINT fk_room_summary_members_room', target);
    EXCEPTION WHEN foreign_key_violation THEN
        RAISE WARNING 'fk_room_summary_members_room left NOT VALID: pre-existing orphan rows in %.room_summary_members', current_schema();
    END;

    BEGIN
        EXECUTE format('ALTER TABLE %s VALIDATE CONSTRAINT fk_room_summary_members_user', target);
    EXCEPTION WHEN foreign_key_violation THEN
        RAISE WARNING 'fk_room_summary_members_user left NOT VALID: pre-existing orphan rows in %.room_summary_members', current_schema();
    END;

    -- Step 4: Comments.
    EXECUTE format(
        'COMMENT ON CONSTRAINT fk_room_summary_members_room ON %s IS '
        '%L',
        target,
        'DB-06: removed DEFERRABLE — FK violations surface immediately. Pairs with MembershipService::add_member atomic write path.'
    );

    EXECUTE format(
        'COMMENT ON CONSTRAINT fk_room_summary_members_user ON %s IS '
        '%L',
        target,
        'DB-06: removed DEFERRABLE — FK violations surface immediately. Pairs with MembershipService::add_member atomic write path.'
    );
END $$;
