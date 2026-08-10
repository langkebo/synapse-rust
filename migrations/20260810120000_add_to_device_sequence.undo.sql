-- Rollback: remove the ordered retrieval index from to_device_messages.
DROP INDEX IF EXISTS idx_to_device_ordered;
