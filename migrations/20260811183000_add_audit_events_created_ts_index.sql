-- Add standalone index on audit_events.created_ts for the retention
-- cleanup DELETE query:  DELETE FROM audit_events WHERE created_ts < $1
--
-- The existing composite indexes (actor_id, resource_type, request_id)
-- all lead with a different column, so PostgreSQL cannot use them for
-- a bare created_ts range scan.  This standalone index allows the
-- cleanup job to use an index-only scan instead of a sequential scan.

CREATE INDEX IF NOT EXISTS idx_audit_events_created_ts
    ON audit_events (created_ts);
