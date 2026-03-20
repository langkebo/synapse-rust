-- Widget Support (MSC4261)
-- 实现嵌入式应用支持

-- Widget 表
CREATE TABLE IF NOT EXISTS widgets (
    id BIGSERIAL PRIMARY KEY,
    widget_id TEXT NOT NULL UNIQUE,
    room_id TEXT,
    user_id TEXT NOT NULL,
    widget_type TEXT NOT NULL,
    url TEXT NOT NULL,
    name TEXT NOT NULL,
    data JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_widgets_room ON widgets(room_id) WHERE is_active = TRUE;
CREATE INDEX IF NOT EXISTS idx_widgets_user ON widgets(user_id);
CREATE INDEX IF NOT EXISTS idx_widgets_type ON widgets(widget_type);

-- Widget 权限表
CREATE TABLE IF NOT EXISTS widget_permissions (
    id BIGSERIAL PRIMARY KEY,
    widget_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    permissions JSONB DEFAULT '[]',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    UNIQUE(widget_id, user_id)
);

CREATE INDEX IF NOT EXISTS idx_widget_permissions_widget ON widget_permissions(widget_id);
CREATE INDEX IF NOT EXISTS idx_widget_permissions_user ON widget_permissions(user_id);

-- Widget 会话表
CREATE TABLE IF NOT EXISTS widget_sessions (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    widget_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    device_id TEXT,
    created_ts BIGINT NOT NULL,
    last_active_ts BIGINT,
    expires_at BIGINT,
    is_active BOOLEAN DEFAULT TRUE
);

CREATE INDEX IF NOT EXISTS idx_widget_sessions_widget ON widget_sessions(widget_id);
CREATE INDEX IF NOT EXISTS idx_widget_sessions_user ON widget_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_widget_sessions_active ON widget_sessions(is_active, expires_at);
