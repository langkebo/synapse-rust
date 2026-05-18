CREATE TABLE IF NOT EXISTS voice_usage_stats (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    room_id TEXT,
    media_id TEXT NOT NULL,
    content_type TEXT NOT NULL,
    duration_ms INTEGER NOT NULL DEFAULT 0,
    size_bytes BIGINT NOT NULL DEFAULT 0,
    created_ts BIGINT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_user ON voice_usage_stats(user_id, created_ts DESC);
CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_room ON voice_usage_stats(room_id, created_ts DESC);
