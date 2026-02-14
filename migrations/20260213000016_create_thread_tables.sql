-- Room Threading Support (MSC3440)
-- This migration adds support for threaded conversations in rooms

-- Thread root events table - stores the root event of each thread
CREATE TABLE IF NOT EXISTS thread_roots (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    root_event_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    thread_id VARCHAR(255) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    origin_server_ts BIGINT NOT NULL,
    last_reply_event_id VARCHAR(255),
    last_reply_sender VARCHAR(255),
    last_reply_ts BIGINT,
    reply_count INTEGER NOT NULL DEFAULT 0,
    is_frozen BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    UNIQUE(room_id, root_event_id)
);

CREATE INDEX idx_thread_roots_room_id ON thread_roots(room_id);
CREATE INDEX idx_thread_roots_thread_id ON thread_roots(thread_id);
CREATE INDEX idx_thread_roots_sender ON thread_roots(sender);
CREATE INDEX idx_thread_roots_last_reply_ts ON thread_roots(last_reply_ts DESC);
CREATE INDEX idx_thread_roots_room_thread ON thread_roots(room_id, thread_id);

-- Thread replies table - stores all replies in threads
CREATE TABLE IF NOT EXISTS thread_replies (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    thread_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    root_event_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    in_reply_to_event_id VARCHAR(255),
    content JSONB NOT NULL DEFAULT '{}',
    origin_server_ts BIGINT NOT NULL,
    is_edited BOOLEAN NOT NULL DEFAULT FALSE,
    is_redacted BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    UNIQUE(room_id, event_id)
);

CREATE INDEX idx_thread_replies_room_id ON thread_replies(room_id);
CREATE INDEX idx_thread_replies_thread_id ON thread_replies(thread_id);
CREATE INDEX idx_thread_replies_root_event ON thread_replies(root_event_id);
CREATE INDEX idx_thread_replies_sender ON thread_replies(sender);
CREATE INDEX idx_thread_replies_in_reply_to ON thread_replies(in_reply_to_event_id);
CREATE INDEX idx_thread_replies_origin_ts ON thread_replies(origin_server_ts DESC);

-- Thread subscriptions - users subscribed to threads for notifications
CREATE TABLE IF NOT EXISTS thread_subscriptions (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    thread_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    notification_level VARCHAR(50) NOT NULL DEFAULT 'all',
    is_muted BOOLEAN NOT NULL DEFAULT FALSE,
    subscribed_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    UNIQUE(room_id, thread_id, user_id)
);

CREATE INDEX idx_thread_subscriptions_room_thread ON thread_subscriptions(room_id, thread_id);
CREATE INDEX idx_thread_subscriptions_user ON thread_subscriptions(user_id);

-- Thread read receipts - track which threads users have read
CREATE TABLE IF NOT EXISTS thread_read_receipts (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    thread_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    last_read_event_id VARCHAR(255),
    last_read_ts BIGINT NOT NULL,
    unread_count INTEGER NOT NULL DEFAULT 0,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    UNIQUE(room_id, thread_id, user_id)
);

CREATE INDEX idx_thread_read_receipts_user ON thread_read_receipts(user_id);
CREATE INDEX idx_thread_read_receipts_room_thread ON thread_read_receipts(room_id, thread_id);

-- Thread relations - stores relations between events in threads
CREATE TABLE IF NOT EXISTS thread_relations (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    relates_to_event_id VARCHAR(255) NOT NULL,
    relation_type VARCHAR(50) NOT NULL,
    thread_id VARCHAR(255),
    is_falling_back BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    UNIQUE(room_id, event_id, relates_to_event_id)
);

CREATE INDEX idx_thread_relations_event ON thread_relations(event_id);
CREATE INDEX idx_thread_relations_relates_to ON thread_relations(relates_to_event_id);
CREATE INDEX idx_thread_relations_type ON thread_relations(relation_type);
CREATE INDEX idx_thread_relations_thread ON thread_relations(thread_id);

-- Thread summaries - cached thread summaries for quick access
CREATE TABLE IF NOT EXISTS thread_summaries (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    thread_id VARCHAR(255) NOT NULL,
    root_event_id VARCHAR(255) NOT NULL,
    root_sender VARCHAR(255) NOT NULL,
    root_content JSONB NOT NULL DEFAULT '{}',
    root_origin_server_ts BIGINT NOT NULL,
    latest_event_id VARCHAR(255),
    latest_sender VARCHAR(255),
    latest_content JSONB,
    latest_origin_server_ts BIGINT,
    reply_count INTEGER NOT NULL DEFAULT 0,
    participants JSONB NOT NULL DEFAULT '[]',
    is_frozen BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    UNIQUE(room_id, thread_id)
);

CREATE INDEX idx_thread_summaries_room ON thread_summaries(room_id);
CREATE INDEX idx_thread_summaries_thread ON thread_summaries(thread_id);
CREATE INDEX idx_thread_summaries_latest_ts ON thread_summaries(latest_origin_server_ts DESC);

-- Thread notifications queue - pending notifications for thread activity
CREATE TABLE IF NOT EXISTS thread_notification_queue (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    thread_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    sender VARCHAR(255) NOT NULL,
    notification_type VARCHAR(50) NOT NULL,
    content JSONB NOT NULL DEFAULT '{}',
    is_processed BOOLEAN NOT NULL DEFAULT FALSE,
    processed_ts BIGINT,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000
);

CREATE INDEX idx_thread_notification_queue_processed ON thread_notification_queue(is_processed, created_ts);
CREATE INDEX idx_thread_notification_queue_thread ON thread_notification_queue(room_id, thread_id);

-- Thread statistics - aggregate statistics for threads
CREATE TABLE IF NOT EXISTS thread_statistics (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    thread_id VARCHAR(255) NOT NULL,
    total_replies INTEGER NOT NULL DEFAULT 0,
    total_participants INTEGER NOT NULL DEFAULT 0,
    total_edits INTEGER NOT NULL DEFAULT 0,
    total_redactions INTEGER NOT NULL DEFAULT 0,
    first_reply_ts BIGINT,
    last_reply_ts BIGINT,
    avg_reply_time_ms BIGINT,
    created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    updated_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW()) * 1000,
    UNIQUE(room_id, thread_id)
);

CREATE INDEX idx_thread_statistics_room ON thread_statistics(room_id);

-- Insert triggers for thread statistics
CREATE OR REPLACE FUNCTION update_thread_root_on_reply()
RETURNS TRIGGER AS $$
BEGIN
    UPDATE thread_roots
    SET 
        last_reply_event_id = NEW.event_id,
        last_reply_sender = NEW.sender,
        last_reply_ts = NEW.origin_server_ts,
        reply_count = reply_count + 1,
        updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000
    WHERE room_id = NEW.room_id AND thread_id = NEW.thread_id;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_thread_root_on_reply
    AFTER INSERT ON thread_replies
    FOR EACH ROW
    EXECUTE FUNCTION update_thread_root_on_reply();

-- Function to update thread summary
CREATE OR REPLACE FUNCTION update_thread_summary()
RETURNS TRIGGER AS $$
BEGIN
    INSERT INTO thread_summaries (
        room_id, thread_id, root_event_id, root_sender, root_content,
        root_origin_server_ts, latest_event_id, latest_sender, latest_content,
        latest_origin_server_ts, reply_count, participants
    )
    SELECT 
        tr.room_id, tr.thread_id, tr.root_event_id, tr.sender, tr.content,
        tr.origin_server_ts, tr.last_reply_event_id, tr.last_reply_sender,
        (SELECT content FROM thread_replies WHERE event_id = tr.last_reply_event_id),
        tr.last_reply_ts, tr.reply_count,
        (SELECT jsonb_agg(DISTINCT sender) FROM thread_replies WHERE thread_id = tr.thread_id)
    FROM thread_roots tr
    WHERE tr.room_id = NEW.room_id AND tr.thread_id = NEW.thread_id
    ON CONFLICT (room_id, thread_id) DO UPDATE SET
        latest_event_id = EXCLUDED.latest_event_id,
        latest_sender = EXCLUDED.latest_sender,
        latest_content = EXCLUDED.latest_content,
        latest_origin_server_ts = EXCLUDED.latest_origin_server_ts,
        reply_count = EXCLUDED.reply_count,
        participants = EXCLUDED.participants,
        updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000;
    
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_thread_summary
    AFTER INSERT OR UPDATE ON thread_roots
    FOR EACH ROW
    EXECUTE FUNCTION update_thread_summary();

-- Comments
COMMENT ON TABLE thread_roots IS 'Stores the root events of message threads (MSC3440)';
COMMENT ON TABLE thread_replies IS 'Stores all replies within message threads';
COMMENT ON TABLE thread_subscriptions IS 'User subscriptions to threads for notifications';
COMMENT ON TABLE thread_read_receipts IS 'Tracks which threads users have read';
COMMENT ON TABLE thread_relations IS 'Stores relations between events in threads';
COMMENT ON TABLE thread_summaries IS 'Cached thread summaries for quick access';
COMMENT ON TABLE thread_notification_queue IS 'Pending notifications for thread activity';
COMMENT ON TABLE thread_statistics IS 'Aggregate statistics for threads';
