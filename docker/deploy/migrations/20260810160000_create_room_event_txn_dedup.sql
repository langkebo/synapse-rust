-- ISSUE-03: Durable transaction-id dedup for client-sent room events.
--
-- Previously, PUT /rooms/{roomId}/send/{eventType}/{txnId} deduplication
-- relied solely on a cache entry (key `txn:{user}:{room}:{txn_id}`, TTL
-- 3600s). Cache loss/eviction or expiry of a delayed retry produced
-- duplicate events. This table is the durable source of truth:
-- the PRIMARY KEY enforces uniqueness of (user_id, room_id, txn_id),
-- and the cache remains only as a fast path.
--
-- Rows are written after the event is durably created; concurrent
-- duplicate sends are resolved via INSERT ... ON CONFLICT DO NOTHING.

CREATE TABLE IF NOT EXISTS room_event_txn_dedup (
    user_id    TEXT   NOT NULL,
    room_id    TEXT   NOT NULL,
    txn_id     TEXT   NOT NULL,
    event_id   TEXT   NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (user_id, room_id, txn_id)
);

-- Lookup by event (e.g. cleanup / audit).
CREATE INDEX IF NOT EXISTS idx_room_event_txn_dedup_event
ON room_event_txn_dedup(event_id);

-- Retention-style cleanup by age.
CREATE INDEX IF NOT EXISTS idx_room_event_txn_dedup_created
ON room_event_txn_dedup(created_ts);
