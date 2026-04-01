CREATE TABLE IF NOT EXISTS audit_events (
    event_id TEXT PRIMARY KEY,
    actor_id TEXT NOT NULL,
    action TEXT NOT NULL,
    resource_type TEXT NOT NULL,
    resource_id TEXT NOT NULL,
    result TEXT NOT NULL,
    request_id TEXT NOT NULL,
    details JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_audit_events_actor_created
ON audit_events(actor_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_audit_events_resource_created
ON audit_events(resource_type, resource_id, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_audit_events_request_created
ON audit_events(request_id, created_ts DESC);
