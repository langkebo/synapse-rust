DO $$
BEGIN
    DROP TABLE IF EXISTS matrixrtc_encryption_keys;
    DROP TABLE IF EXISTS matrixrtc_memberships;
    DROP TABLE IF EXISTS matrixrtc_sessions;
    DROP TABLE IF EXISTS call_candidates;
    DROP TABLE IF EXISTS call_sessions;
    DROP TABLE IF EXISTS beacon_locations;
    DROP TABLE IF EXISTS beacon_info;
END $$;
