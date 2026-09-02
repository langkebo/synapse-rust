-- DB-04-b: Replace events.room_id ON DELETE CASCADE with ON DELETE NO ACTION
-- See: artifacts/数据库架构诊断报告-2026-08-30.md §P0-3
-- Ticket: .scratch/db-schema-optimization/04-remove-cascade-fix-admin.md
--
-- ## Why
--
-- The previous schema had `events.room_id REFERENCES rooms(room_id) ON DELETE CASCADE`.
-- When a room with millions of events is deleted, PostgreSQL acquires an
-- AccessExclusiveLock on the `events` table for the entire duration of the
-- cascading delete, blocking ALL concurrent reads and writes to events.
-- This causes visible stalls on hot paths: messages can't be sent, sync
-- streams can't advance, server notification pipelines block.
--
-- Replacing CASCADE with NO ACTION (the default) defers the constraint
-- check to COMMIT, but it still requires that all rows referencing the
-- deleted room are gone first — which is exactly what we want, enforced
-- at the Rust layer (`RoomStorage::delete_room`) with explicit batched
-- DELETE in 1000-row chunks.
--
-- ## Why a batch DELETE in Rust (not just NO ACTION)
--
-- NO ACTION alone would still trigger the same lock storm at COMMIT time
-- when the constraint check fires. The Rust batch DELETE breaks the work
-- into small transactions, each only locking 1000 rows for milliseconds,
-- so the table remains available throughout the room deletion.
--
-- ## What this migration does NOT change
--
-- The other 32 tables that reference `rooms(room_id) ON DELETE CASCADE`
-- (room_memberships, room_aliases, room_summaries, etc.) are intentionally
-- LEFT AS-IS. Their row counts are small (typically <100 per room), so
-- the lock cost is negligible. Removing all 33 CASCADE constraints would
-- be a much larger migration with high regression risk.
--
-- ## Rollback
--
-- The `.undo.sql` reverses this change by re-adding the CASCADE constraint.
-- Existing orphan rows in `events` (created if the migration is applied
-- but the Rust layer is not yet updated) MUST be cleaned up before
-- rollback, or the CASCADE re-add will fail.

-- Step 1: Drop the existing CASCADE FKs on events.room_id.
-- There are two constraints covering the same relationship in the v10 baseline:
--   - fk_events_room (inline CREATE TABLE, line 338): already NO ACTION in v10
--   - fk_events_room_id (IF NOT EXISTS block, line 4272): still CASCADE — THIS is the problem
-- Use IF EXISTS for idempotency in case this is re-run.
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_room;
ALTER TABLE events DROP CONSTRAINT IF EXISTS fk_events_room_id;

-- Step 2: Re-add the same constraint as NO ACTION (the default).
-- We name it `fk_events_room_no_action` to make the change traceable.
-- NOT VALID is used to skip the existing-row check at constraint-creation
-- time (which would itself be an expensive AccessShareLock scan of
-- events). The constraint will be validated by `VALIDATE CONSTRAINT`
-- in a separate step that doesn't block writes.
--
-- Note: even with NOT VALID, this constraint enforces future row-level
-- operations immediately. The only thing skipped is the historical scan.
ALTER TABLE events
    ADD CONSTRAINT fk_events_room_no_action
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE NO ACTION
    NOT VALID;

-- Step 3: VALIDATE the constraint separately.
-- This acquires only a ShareUpdateExclusiveLock on events, which DOES
-- NOT block reads or normal writes — only schema-altering operations.
-- It scans events once to verify that no rows reference a non-existent
-- room. In a healthy database this should pass in a few seconds.
ALTER TABLE events VALIDATE CONSTRAINT fk_events_room_no_action;

-- Step 4: Comment the constraint for future maintainers.
COMMENT ON CONSTRAINT fk_events_room_no_action ON events IS
    'DB-04-b: replaced CASCADE with NO ACTION to avoid AccessExclusiveLock on events during room deletion. Rust layer RoomStorage::delete_room is now responsible for batched cleanup.';
