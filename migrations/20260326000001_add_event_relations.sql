-- Migration: Add event_relations table for Matrix Relations API
-- Date: 20260326
-- Description: Create event_relations table to support Matrix Relations (annotations, references, replacements, threads)
-- Spec: https://spec.matrix.org/v1.8/client-server-api/#relationship-types

CREATE TABLE IF NOT EXISTS event_relations (
    id BIGSERIAL PRIMARY KEY,
    room_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    relates_to_event_id TEXT NOT NULL,
    relation_type TEXT NOT NULL CHECK (relation_type IN ('m.annotation', 'm.reference', 'm.replace', 'm.thread')),
    sender TEXT NOT NULL,
    origin_server_ts BIGINT NOT NULL,
    content JSONB DEFAULT '{}',
    is_redacted BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    UNIQUE(event_id, relation_type, sender)
);

CREATE INDEX IF NOT EXISTS idx_event_relations_room_event ON event_relations(room_id, relates_to_event_id, relation_type);
CREATE INDEX IF NOT EXISTS idx_event_relations_sender ON event_relations(sender, relation_type);
CREATE INDEX IF NOT EXISTS idx_event_relations_origin_ts ON event_relations(room_id, origin_server_ts DESC);

COMMENT ON TABLE event_relations IS 'Stores Matrix event relations (annotations, references, replacements, threads)';
COMMENT ON COLUMN event_relations.event_id IS 'The event that is relating to another event';
COMMENT ON COLUMN event_relations.relates_to_event_id IS 'The event_id being related to';
COMMENT ON COLUMN event_relations.relation_type IS 'Relation type: m.annotation (reactions), m.reference, m.replace (edits), m.thread';
COMMENT ON COLUMN event_relations.content IS 'Additional content for the relation (e.g., reaction emoji, edit content)';