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
-- ## What this migration does
--
-- Step 1: Drop the two DEFERRED constraints (idempotent).
-- Step 2: Re-add them without DEFERRABLE (immediate check), NOT VALID.
-- Step 3: VALIDATE (non-blocking, ShareUpdateExclusiveLock).
-- Step 4: Comment for future maintainers.

-- Step 1: Drop the deferred FK constraints.
ALTER TABLE room_summary_members DROP CONSTRAINT IF EXISTS fk_room_summary_members_room;
ALTER TABLE room_summary_members DROP CONSTRAINT IF EXISTS fk_room_summary_members_user;

-- Step 2: Re-add as immediate (non-deferred) constraints.
ALTER TABLE room_summary_members
    ADD CONSTRAINT fk_room_summary_members_room
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
    NOT VALID;

ALTER TABLE room_summary_members
    ADD CONSTRAINT fk_room_summary_members_user
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
    NOT VALID;

-- Step 3: Validate (non-blocking).
ALTER TABLE room_summary_members VALIDATE CONSTRAINT fk_room_summary_members_room;
ALTER TABLE room_summary_members VALIDATE CONSTRAINT fk_room_summary_members_user;

-- Step 4: Comments.
COMMENT ON CONSTRAINT fk_room_summary_members_room ON room_summary_members IS
    'DB-06: removed DEFERRABLE — FK violations surface immediately. Pairs with MembershipService::add_member atomic write path.';

COMMENT ON CONSTRAINT fk_room_summary_members_user ON room_summary_members IS
    'DB-06: removed DEFERRABLE — FK violations surface immediately. Pairs with MembershipService::add_member atomic write path.';
