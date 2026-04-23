-- =============================================================================
-- Extension: Voice Messages (feature: voice-extended)
-- Extracted from 00000000_unified_schema_v6.sql
-- Tables: voice_messages, voice_usage_stats
-- =============================================================================

CREATE TABLE IF NOT EXISTS voice_messages (
    id BIGSERIAL,
    event_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    room_id TEXT,
    media_id TEXT,
    duration_ms INT NOT NULL,
    file_size BIGINT,
    file_path TEXT,
    mime_type TEXT DEFAULT 'audio/ogg',
    waveform JSONB,
    transcription TEXT,
    transcription_language TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_voice_messages PRIMARY KEY (id),
    CONSTRAINT uq_voice_messages_event UNIQUE (event_id)
);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user ON voice_messages(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_room ON voice_messages(room_id);

CREATE TABLE IF NOT EXISTS voice_usage_stats (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    date DATE NOT NULL,
    messages_sent INTEGER DEFAULT 0,
    total_duration_ms BIGINT DEFAULT 0,
    total_size_bytes BIGINT DEFAULT 0,
    CONSTRAINT pk_voice_usage_stats PRIMARY KEY (id),
    CONSTRAINT uq_voice_usage_stats_user_date UNIQUE (user_id, date)
);
CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_user ON voice_usage_stats(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_date ON voice_usage_stats(date);
