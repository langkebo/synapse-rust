-- Comprehensive schema fix to add all missing columns
-- This script adds all columns that are expected by the application code

-- Fix private_sessions table
ALTER TABLE private_sessions ADD COLUMN IF NOT EXISTS session_type VARCHAR(50) DEFAULT 'direct';
ALTER TABLE private_sessions ADD COLUMN IF NOT EXISTS encryption_key VARCHAR(255);
ALTER TABLE private_sessions ADD COLUMN IF NOT EXISTS last_activity_ts BIGINT;
ALTER TABLE private_sessions ADD COLUMN IF NOT EXISTS updated_ts BIGINT;
ALTER TABLE private_sessions ADD COLUMN IF NOT EXISTS unread_count INTEGER DEFAULT 0;

-- Fix private_messages table
ALTER TABLE private_messages ADD COLUMN IF NOT EXISTS encrypted_content TEXT;
ALTER TABLE private_messages ADD COLUMN IF NOT EXISTS read_by_receiver BOOLEAN DEFAULT FALSE;

-- Fix room_memberships table
ALTER TABLE room_memberships ADD COLUMN IF NOT EXISTS event_type VARCHAR(255);
ALTER TABLE room_memberships ADD COLUMN IF NOT EXISTS display_name VARCHAR(255);

-- Fix voice_usage_stats table
ALTER TABLE voice_usage_stats ADD COLUMN IF NOT EXISTS total_file_size BIGINT DEFAULT 0;
ALTER TABLE voice_usage_stats ADD COLUMN IF NOT EXISTS last_active_ts BIGINT;

-- Fix ip_blocks table
ALTER TABLE ip_blocks ADD COLUMN IF NOT EXISTS blocked_at BIGINT;
ALTER TABLE ip_blocks ADD COLUMN IF NOT EXISTS expires_at BIGINT;

-- Fix blocked_users table
ALTER TABLE blocked_users ADD COLUMN IF NOT EXISTS created_ts BIGINT;
ALTER TABLE blocked_users ADD COLUMN IF NOT EXISTS blocked_user_id VARCHAR(255);
ALTER TABLE blocked_users ADD COLUMN IF NOT EXISTS user_id VARCHAR(255);

-- Ensure blocked_users conflict target exists for ON CONFLICT (user_id, blocked_user_id)
CREATE UNIQUE INDEX IF NOT EXISTS idx_blocked_users_user_blocked_user
    ON blocked_users(user_id, blocked_user_id);

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'blocked_users'
          AND column_name = 'blocked_id'
    ) THEN
        EXECUTE 'ALTER TABLE blocked_users ALTER COLUMN blocked_id DROP NOT NULL';
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'blocked_users'
          AND column_name = 'blocker_id'
    ) THEN
        EXECUTE 'ALTER TABLE blocked_users ALTER COLUMN blocker_id DROP NOT NULL';
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM information_schema.columns
        WHERE table_name = 'blocked_users'
          AND column_name = 'blocked_ts'
    ) THEN
        EXECUTE 'ALTER TABLE blocked_users ALTER COLUMN blocked_ts DROP NOT NULL';
    END IF;
END $$;

-- Fix room_aliases table
ALTER TABLE room_aliases ADD COLUMN IF NOT EXISTS alias VARCHAR(255);

-- Fix access_tokens table
ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS invalidated_ts BIGINT;

-- Create indexes for private_sessions
CREATE INDEX IF NOT EXISTS idx_private_sessions_user1 ON private_sessions(user_id_1);
CREATE INDEX IF NOT EXISTS idx_private_sessions_user2 ON private_sessions(user_id_2);
CREATE INDEX IF NOT EXISTS idx_private_sessions_activity ON private_sessions(last_activity_ts DESC);

-- Create indexes for private_messages
CREATE INDEX IF NOT EXISTS idx_private_messages_session ON private_messages(session_id);
CREATE INDEX IF NOT EXISTS idx_private_messages_sender ON private_messages(sender_id);

-- Create indexes for friends
CREATE INDEX IF NOT EXISTS idx_friends_user ON friends(user_id);
CREATE INDEX IF NOT EXISTS idx_friends_friend ON friends(friend_id);

-- Create indexes for friend_requests
CREATE INDEX IF NOT EXISTS idx_friend_requests_target ON friend_requests(to_user_id);
