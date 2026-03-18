-- Event Relations Tables for Matrix Relations API
-- Created: 2026-03-15

-- Event relations table
CREATE TABLE IF NOT EXISTS event_relations (
    event_id VARCHAR(255) NOT NULL,
    room_id VARCHAR(255) NOT NULL,
    relation_type VARCHAR(50) NOT NULL,
    relates_to_event_id VARCHAR(255) NOT NULL,
    content JSONB DEFAULT '{}',
    origin_server_ts BIGINT NOT NULL,
    PRIMARY KEY (event_id, relates_to_event_id),
    INDEX idx_relations_event_id (event_id),
    INDEX idx_relations_relates_to (relates_to_event_id),
    INDEX idx_relations_room_id (room_id),
    INDEX idx_relations_type (relation_type),
    INDEX idx_relations_room_type (room_id, relation_type)
);

-- Reaction aggregations (denormalized for performance)
CREATE TABLE IF NOT EXISTS reaction_aggregations (
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    relates_to_event_id VARCHAR(255) NOT NULL,
    reaction_key VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    count BIGINT DEFAULT 1,
    PRIMARY KEY (room_id, event_id, reaction_key, sender),
    INDEX idx_reaction_aggregations_target (relates_to_event_id)
);
