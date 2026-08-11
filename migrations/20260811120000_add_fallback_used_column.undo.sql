-- Undo: remove fallback_used column
DROP INDEX IF EXISTS idx_device_keys_fallback;
CREATE INDEX idx_device_keys_fallback
    ON device_keys(user_id, device_id)
    WHERE is_fallback = TRUE;

ALTER TABLE device_keys DROP COLUMN IF EXISTS fallback_used;
