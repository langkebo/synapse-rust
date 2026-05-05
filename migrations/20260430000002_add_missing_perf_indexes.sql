-- =============================================================================
-- Restore the two performance indexes that schema_health_check expects but
-- which are not present in the consolidated schema:
--   - idx_memberships_user_room  on room_memberships(user_id, room_id)
--   - idx_user_threepids_medium_address on user_threepids(medium, address)
-- Both speed up extremely hot lookups (room membership joins on the per-user
-- side, and 3PID resolution on login / password reset). Synapse upstream has
-- analogous indexes; we previously archived them but never re-applied.
-- =============================================================================

DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.tables
               WHERE table_schema = 'public' AND table_name = 'room_memberships') THEN
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_memberships_user_room
                 ON room_memberships(user_id, room_id)';
    END IF;

    IF EXISTS (SELECT 1 FROM information_schema.tables
               WHERE table_schema = 'public' AND table_name = 'user_threepids') THEN
        EXECUTE 'CREATE INDEX IF NOT EXISTS idx_user_threepids_medium_address
                 ON user_threepids(medium, address)';
    END IF;
END
$$;
