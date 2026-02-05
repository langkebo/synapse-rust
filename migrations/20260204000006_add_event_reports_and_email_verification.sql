-- Migration: Add event reports and email verification tables
-- Created: 2026-02-04
-- Purpose: Add tables for event reporting and email verification functionality

-- Event Reports Table
CREATE TABLE IF NOT EXISTS event_reports (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    reporter_user_id TEXT NOT NULL,
    reason TEXT,
    score INTEGER DEFAULT -100,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    FOREIGN KEY (event_id) REFERENCES events(event_id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    FOREIGN KEY (reporter_user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- Email Verification Tokens Table
CREATE TABLE IF NOT EXISTS email_verification_tokens (
    id BIGSERIAL PRIMARY KEY,
    email TEXT NOT NULL,
    token TEXT NOT NULL,
    expires_at BIGINT NOT NULL,
    created_ts BIGINT NOT NULL,
    used BOOLEAN DEFAULT FALSE,
    session_data JSONB
);

-- Create indexes for better query performance
CREATE INDEX IF NOT EXISTS idx_event_reports_event_id ON event_reports(event_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_room_id ON event_reports(room_id);
CREATE INDEX IF NOT EXISTS idx_event_reports_user_id ON event_reports(user_id);
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_email ON email_verification_tokens(email);
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_token ON email_verification_tokens(token);
CREATE INDEX IF NOT EXISTS idx_email_verification_tokens_expires ON email_verification_tokens(expires_at);
