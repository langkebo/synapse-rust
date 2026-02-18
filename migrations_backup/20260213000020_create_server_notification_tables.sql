-- Server Notification Tables
-- Implements server-wide notification system

-- Server notifications table
CREATE TABLE IF NOT EXISTS server_notifications (
    id SERIAL PRIMARY KEY,
    title VARCHAR(255) NOT NULL,
    content TEXT NOT NULL,
    notification_type VARCHAR(50) NOT NULL DEFAULT 'info',
    priority INTEGER NOT NULL DEFAULT 0,
    target_audience VARCHAR(50) NOT NULL DEFAULT 'all',
    target_user_ids JSONB DEFAULT '[]',
    starts_at TIMESTAMPTZ,
    expires_at TIMESTAMPTZ,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    is_dismissible BOOLEAN NOT NULL DEFAULT TRUE,
    action_url TEXT,
    action_text VARCHAR(255),
    created_by VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_server_notifications_is_active ON server_notifications(is_active);
CREATE INDEX idx_server_notifications_starts_at ON server_notifications(starts_at);
CREATE INDEX idx_server_notifications_expires_at ON server_notifications(expires_at);
CREATE INDEX idx_server_notifications_type ON server_notifications(notification_type);
CREATE INDEX idx_server_notifications_priority ON server_notifications(priority);

-- User notification read status table
CREATE TABLE IF NOT EXISTS user_notification_status (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    notification_id INTEGER NOT NULL REFERENCES server_notifications(id) ON DELETE CASCADE,
    is_read BOOLEAN NOT NULL DEFAULT FALSE,
    is_dismissed BOOLEAN NOT NULL DEFAULT FALSE,
    read_at TIMESTAMPTZ,
    dismissed_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT uq_user_notification UNIQUE (user_id, notification_id),
    CONSTRAINT fk_user_notification_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX idx_user_notification_status_user_id ON user_notification_status(user_id);
CREATE INDEX idx_user_notification_status_notification_id ON user_notification_status(notification_id);
CREATE INDEX idx_user_notification_status_is_read ON user_notification_status(is_read);

-- Notification templates table
CREATE TABLE IF NOT EXISTS notification_templates (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL UNIQUE,
    title_template VARCHAR(255) NOT NULL,
    content_template TEXT NOT NULL,
    notification_type VARCHAR(50) NOT NULL DEFAULT 'info',
    variables JSONB DEFAULT '[]',
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_notification_templates_name ON notification_templates(name);
CREATE INDEX idx_notification_templates_is_active ON notification_templates(is_active);

-- Notification delivery log table
CREATE TABLE IF NOT EXISTS notification_delivery_log (
    id SERIAL PRIMARY KEY,
    notification_id INTEGER NOT NULL REFERENCES server_notifications(id) ON DELETE CASCADE,
    user_id VARCHAR(255),
    delivery_method VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL,
    error_message TEXT,
    delivered_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT fk_notification_delivery_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX idx_notification_delivery_notification_id ON notification_delivery_log(notification_id);
CREATE INDEX idx_notification_delivery_user_id ON notification_delivery_log(user_id);
CREATE INDEX idx_notification_delivery_status ON notification_delivery_log(status);

-- Scheduled notifications table
CREATE TABLE IF NOT EXISTS scheduled_notifications (
    id SERIAL PRIMARY KEY,
    notification_id INTEGER NOT NULL REFERENCES server_notifications(id) ON DELETE CASCADE,
    scheduled_for TIMESTAMPTZ NOT NULL,
    is_sent BOOLEAN NOT NULL DEFAULT FALSE,
    sent_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_scheduled_notifications_scheduled_for ON scheduled_notifications(scheduled_for);
CREATE INDEX idx_scheduled_notifications_is_sent ON scheduled_notifications(is_sent);

-- Insert comment
COMMENT ON TABLE server_notifications IS 'Server-wide notifications';
COMMENT ON TABLE user_notification_status IS 'User notification read/dismiss status';
COMMENT ON TABLE notification_templates IS 'Reusable notification templates';
COMMENT ON TABLE notification_delivery_log IS 'Notification delivery tracking';
COMMENT ON TABLE scheduled_notifications IS 'Scheduled notification jobs';
