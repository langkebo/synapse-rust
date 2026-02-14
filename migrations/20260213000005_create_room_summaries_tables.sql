-- 房间摘要表 (MSC3266)
-- 存储房间的摘要信息，用于快速访问和展示

-- 房间摘要表
CREATE TABLE IF NOT EXISTS room_summaries (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL UNIQUE,
    room_type VARCHAR(50),
    name VARCHAR(255),
    topic TEXT,
    avatar_url TEXT,
    canonical_alias VARCHAR(255),
    join_rules VARCHAR(50) DEFAULT 'invite',
    history_visibility VARCHAR(50) DEFAULT 'shared',
    guest_access VARCHAR(50) DEFAULT 'forbidden',
    is_direct BOOLEAN DEFAULT FALSE,
    is_space BOOLEAN DEFAULT FALSE,
    is_encrypted BOOLEAN DEFAULT FALSE,
    member_count INTEGER DEFAULT 0,
    joined_member_count INTEGER DEFAULT 0,
    invited_member_count INTEGER DEFAULT 0,
    hero_users JSONB DEFAULT '[]'::jsonb,
    last_event_id VARCHAR(255),
    last_event_ts BIGINT,
    last_message_ts BIGINT,
    unread_notifications INTEGER DEFAULT 0,
    unread_highlight INTEGER DEFAULT 0,
    updated_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_summaries_room_id ON room_summaries(room_id);
CREATE INDEX IF NOT EXISTS idx_room_summaries_room_type ON room_summaries(room_type);
CREATE INDEX IF NOT EXISTS idx_room_summaries_is_space ON room_summaries(is_space);
CREATE INDEX IF NOT EXISTS idx_room_summaries_is_encrypted ON room_summaries(is_encrypted);
CREATE INDEX IF NOT EXISTS idx_room_summaries_last_event_ts ON room_summaries(last_event_ts DESC);
CREATE INDEX IF NOT EXISTS idx_room_summaries_member_count ON room_summaries(member_count DESC);

-- 房间摘要成员表
CREATE TABLE IF NOT EXISTS room_summary_members (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    display_name VARCHAR(255),
    avatar_url TEXT,
    membership VARCHAR(50) NOT NULL DEFAULT 'join',
    is_hero BOOLEAN DEFAULT FALSE,
    last_active_ts BIGINT,
    updated_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    UNIQUE (room_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_room_summary_members_room_id ON room_summary_members(room_id);
CREATE INDEX IF NOT EXISTS idx_room_summary_members_user_id ON room_summary_members(user_id);
CREATE INDEX IF NOT EXISTS idx_room_summary_members_membership ON room_summary_members(membership);
CREATE INDEX IF NOT EXISTS idx_room_summary_members_is_hero ON room_summary_members(is_hero);

-- 房间摘要状态表
CREATE TABLE IF NOT EXISTS room_summary_state (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    state_key VARCHAR(255) NOT NULL,
    event_id VARCHAR(255),
    content JSONB DEFAULT '{}'::jsonb,
    updated_ts BIGINT NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    UNIQUE (room_id, event_type, state_key)
);

CREATE INDEX IF NOT EXISTS idx_room_summary_state_room_id ON room_summary_state(room_id);
CREATE INDEX IF NOT EXISTS idx_room_summary_state_event_type ON room_summary_state(event_type);
CREATE INDEX IF NOT EXISTS idx_room_summary_state_state_key ON room_summary_state(state_key);

-- 房间摘要更新队列表
CREATE TABLE IF NOT EXISTS room_summary_update_queue (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    event_id VARCHAR(255) NOT NULL,
    event_type VARCHAR(255) NOT NULL,
    state_key VARCHAR(255),
    priority INTEGER DEFAULT 0,
    status VARCHAR(50) DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    processed_ts BIGINT,
    error_message TEXT,
    retry_count INTEGER DEFAULT 0,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_summary_update_queue_room_id ON room_summary_update_queue(room_id);
CREATE INDEX IF NOT EXISTS idx_room_summary_update_queue_status ON room_summary_update_queue(status);
CREATE INDEX IF NOT EXISTS idx_room_summary_update_queue_priority ON room_summary_update_queue(priority DESC);
CREATE INDEX IF NOT EXISTS idx_room_summary_update_queue_created_ts ON room_summary_update_queue(created_ts);

-- 房间摘要统计表
CREATE TABLE IF NOT EXISTS room_summary_stats (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL UNIQUE,
    total_events BIGINT DEFAULT 0,
    total_state_events BIGINT DEFAULT 0,
    total_messages BIGINT DEFAULT 0,
    total_media BIGINT DEFAULT 0,
    storage_size BIGINT DEFAULT 0,
    last_updated_ts BIGINT NOT NULL,
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_room_summary_stats_room_id ON room_summary_stats(room_id);
CREATE INDEX IF NOT EXISTS idx_room_summary_stats_total_events ON room_summary_stats(total_events DESC);
CREATE INDEX IF NOT EXISTS idx_room_summary_stats_total_messages ON room_summary_stats(total_messages DESC);

-- 触发器：自动更新 updated_ts
CREATE OR REPLACE FUNCTION update_room_summary_timestamp()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_ts = EXTRACT(EPOCH FROM NOW()) * 1000;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_room_summary_timestamp
    BEFORE UPDATE ON room_summaries
    FOR EACH ROW
    EXECUTE FUNCTION update_room_summary_timestamp();

CREATE TRIGGER trigger_update_room_summary_member_timestamp
    BEFORE UPDATE ON room_summary_members
    FOR EACH ROW
    EXECUTE FUNCTION update_room_summary_timestamp();

CREATE TRIGGER trigger_update_room_summary_state_timestamp
    BEFORE UPDATE ON room_summary_state
    FOR EACH ROW
    EXECUTE FUNCTION update_room_summary_timestamp();

-- 触发器：自动更新成员计数
CREATE OR REPLACE FUNCTION update_room_summary_member_count()
RETURNS TRIGGER AS $$
BEGIN
    IF TG_OP = 'INSERT' THEN
        UPDATE room_summaries
        SET member_count = member_count + 1,
            joined_member_count = CASE WHEN NEW.membership = 'join' THEN joined_member_count + 1 ELSE joined_member_count END,
            invited_member_count = CASE WHEN NEW.membership = 'invite' THEN invited_member_count + 1 ELSE invited_member_count END
        WHERE room_id = NEW.room_id;
    ELSIF TG_OP = 'DELETE' THEN
        UPDATE room_summaries
        SET member_count = GREATEST(member_count - 1, 0),
            joined_member_count = CASE WHEN OLD.membership = 'join' THEN GREATEST(joined_member_count - 1, 0) ELSE joined_member_count END,
            invited_member_count = CASE WHEN OLD.membership = 'invite' THEN GREATEST(invited_member_count - 1, 0) ELSE invited_member_count END
        WHERE room_id = OLD.room_id;
    ELSIF TG_OP = 'UPDATE' THEN
        IF OLD.membership != NEW.membership THEN
            UPDATE room_summaries
            SET joined_member_count = CASE
                WHEN OLD.membership = 'join' AND NEW.membership != 'join' THEN GREATEST(joined_member_count - 1, 0)
                WHEN OLD.membership != 'join' AND NEW.membership = 'join' THEN joined_member_count + 1
                ELSE joined_member_count
            END,
            invited_member_count = CASE
                WHEN OLD.membership = 'invite' AND NEW.membership != 'invite' THEN GREATEST(invited_member_count - 1, 0)
                WHEN OLD.membership != 'invite' AND NEW.membership = 'invite' THEN invited_member_count + 1
                ELSE invited_member_count
            END
            WHERE room_id = NEW.room_id;
        END IF;
    END IF;
    RETURN NULL;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trigger_update_room_summary_member_count
    AFTER INSERT OR UPDATE OR DELETE ON room_summary_members
    FOR EACH ROW
    EXECUTE FUNCTION update_room_summary_member_count();

-- 初始数据迁移：从现有房间创建摘要
INSERT INTO room_summaries (room_id, room_type, name, topic, avatar_url, canonical_alias, join_rules, history_visibility, guest_access, is_direct, is_space, is_encrypted, member_count, joined_member_count, invited_member_count, last_event_ts, updated_ts, created_ts)
SELECT 
    r.room_id,
    NULL as room_type,
    (SELECT content->>'name' FROM events WHERE room_id = r.room_id AND event_type = 'm.room.name' AND state_key = '' ORDER BY stream_id DESC LIMIT 1),
    (SELECT content->>'topic' FROM events WHERE room_id = r.room_id AND event_type = 'm.room.topic' AND state_key = '' ORDER BY stream_id DESC LIMIT 1),
    (SELECT content->>'url' FROM events WHERE room_id = r.room_id AND event_type = 'm.room.avatar' AND state_key = '' ORDER BY stream_id DESC LIMIT 1),
    (SELECT content->>'alias' FROM events WHERE room_id = r.room_id AND event_type = 'm.room.canonical_alias' AND state_key = '' ORDER BY stream_id DESC LIMIT 1),
    COALESCE((SELECT content->>'join_rule' FROM events WHERE room_id = r.room_id AND event_type = 'm.room.join_rules' AND state_key = '' ORDER BY stream_id DESC LIMIT 1), 'invite'),
    COALESCE((SELECT content->>'history_visibility' FROM events WHERE room_id = r.room_id AND event_type = 'm.room.history_visibility' AND state_key = '' ORDER BY stream_id DESC LIMIT 1), 'shared'),
    COALESCE((SELECT content->>'guest_access' FROM events WHERE room_id = r.room_id AND event_type = 'm.room.guest_access' AND state_key = '' ORDER BY stream_id DESC LIMIT 1), 'forbidden'),
    FALSE as is_direct,
    FALSE as is_space,
    EXISTS (SELECT 1 FROM events WHERE room_id = r.room_id AND event_type = 'm.room.encryption' AND state_key = ''),
    (SELECT COUNT(*) FROM room_members WHERE room_id = r.room_id),
    (SELECT COUNT(*) FROM room_members WHERE room_id = r.room_id AND membership = 'join'),
    (SELECT COUNT(*) FROM room_members WHERE room_id = r.room_id AND membership = 'invite'),
    (SELECT origin_server_ts FROM events WHERE room_id = r.room_id ORDER BY stream_id DESC LIMIT 1),
    EXTRACT(EPOCH FROM NOW()) * 1000,
    EXTRACT(EPOCH FROM NOW()) * 1000
FROM rooms r
WHERE NOT EXISTS (SELECT 1 FROM room_summaries WHERE room_id = r.room_id)
ON CONFLICT (room_id) DO NOTHING;

-- 初始数据迁移：从现有成员创建摘要成员
INSERT INTO room_summary_members (room_id, user_id, display_name, avatar_url, membership, is_hero, last_active_ts, updated_ts, created_ts)
SELECT 
    rm.room_id,
    rm.user_id,
    rm.display_name,
    rm.avatar_url,
    rm.membership,
    FALSE as is_hero,
    rm.last_active_ts,
    EXTRACT(EPOCH FROM NOW()) * 1000,
    EXTRACT(EPOCH FROM NOW()) * 1000
FROM room_members rm
WHERE NOT EXISTS (SELECT 1 FROM room_summary_members WHERE room_id = rm.room_id AND user_id = rm.user_id)
ON CONFLICT (room_id, user_id) DO NOTHING;
