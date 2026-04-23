DROP INDEX IF EXISTS idx_federation_servers_status;
ALTER TABLE federation_servers DROP COLUMN IF EXISTS status;
ALTER TABLE federation_servers DROP COLUMN IF EXISTS updated_ts;
