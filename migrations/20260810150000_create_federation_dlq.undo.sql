-- Undo: remove federation dead letter queue table

DROP INDEX IF EXISTS idx_fed_dlq_created;
DROP INDEX IF EXISTS idx_fed_dlq_destination;
DROP TABLE IF EXISTS federation_dead_letter_queue;
