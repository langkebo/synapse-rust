CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS idx_thread_roots_room_thread_unique
ON thread_roots(room_id, thread_id)
WHERE thread_id IS NOT NULL;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_thread_roots_room_last_reply_created
ON thread_roots(room_id, last_reply_ts DESC, created_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_thread_replies_room_thread_event
ON thread_replies(room_id, thread_id, event_id);

CREATE TABLE IF NOT EXISTS thread_relations (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    relates_to_event_id TEXT NOT NULL,
    relation_type TEXT NOT NULL,
    thread_id TEXT,
    is_falling_back BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    CONSTRAINT uq_thread_relations_room_event_type UNIQUE (room_id, event_id, relation_type),
    CONSTRAINT fk_thread_relations_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_thread_relations_room_event
ON thread_relations(room_id, event_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_thread_relations_room_relates_to
ON thread_relations(room_id, relates_to_event_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_thread_relations_room_thread
ON thread_relations(room_id, thread_id)
WHERE thread_id IS NOT NULL;
