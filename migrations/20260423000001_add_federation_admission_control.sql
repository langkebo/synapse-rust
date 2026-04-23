ALTER TABLE federation_servers ADD COLUMN IF NOT EXISTS status TEXT NOT NULL DEFAULT 'active';
ALTER TABLE federation_servers ADD COLUMN IF NOT EXISTS updated_ts BIGINT;

CREATE INDEX IF NOT EXISTS idx_federation_servers_status ON federation_servers(status);

COMMENT ON COLUMN federation_servers.status IS 'Federation admission status: pending, active, rejected';
COMMENT ON COLUMN federation_servers.updated_ts IS 'Timestamp of last status update in milliseconds';
