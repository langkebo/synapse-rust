-- Add ts_updated_ms to device_keys for tracking changes
ALTER TABLE device_keys ADD COLUMN IF NOT EXISTS ts_updated_ms BIGINT;
UPDATE device_keys SET ts_updated_ms = ts_added_ms WHERE ts_updated_ms IS NULL;
ALTER TABLE device_keys ALTER COLUMN ts_updated_ms SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_device_keys_updated ON device_keys(ts_updated_ms);
