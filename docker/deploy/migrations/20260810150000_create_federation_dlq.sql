-- FED-07: Federation Dead Letter Queue
--
-- Failed federation transactions are persisted to this table after
-- exhausting retries, enabling manual retry and audit trails.
-- The DLQ is append-only for unresolved entries; resolution is a
-- separate UPDATE that sets is_resolved = TRUE.

CREATE TABLE IF NOT EXISTS federation_dead_letter_queue (
    id BIGSERIAL PRIMARY KEY,
    txn_id TEXT NOT NULL,
    destination TEXT NOT NULL,
    origin TEXT NOT NULL,
    payload JSONB NOT NULL,
    failure_reason TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL,
    last_attempt_ts BIGINT,
    is_resolved BOOLEAN NOT NULL DEFAULT FALSE
);

-- Partial index for unresolved entries by destination (most common query).
CREATE INDEX IF NOT EXISTS idx_fed_dlq_destination
ON federation_dead_letter_queue(destination, is_resolved)
WHERE is_resolved = FALSE;

-- Index for chronological listing / cleanup.
CREATE INDEX IF NOT EXISTS idx_fed_dlq_created
ON federation_dead_letter_queue(created_ts DESC);
