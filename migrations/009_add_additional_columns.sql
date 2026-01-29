-- Add additional missing columns for synapse-rust

-- Add missing columns to typing table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'typing' AND column_name = 'last_active_ts'
    ) THEN
        ALTER TABLE typing ADD COLUMN last_active_ts TIMESTAMPTZ NOT NULL DEFAULT NOW();
    END IF;
END $$;

-- Add missing columns to friend_categories table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'friend_categories' AND column_name = 'color'
    ) THEN
        ALTER TABLE friend_categories ADD COLUMN color VARCHAR(7);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'friend_categories' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE friend_categories ADD COLUMN created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT;
    END IF;
END $$;

-- Add missing columns to private_sessions table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'private_sessions' AND column_name = 'user_id_1'
    ) THEN
        ALTER TABLE private_sessions ADD COLUMN user_id_1 VARCHAR(255);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'private_sessions' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE private_sessions ADD COLUMN created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'private_sessions' AND column_name = 'last_message_ts'
    ) THEN
        ALTER TABLE private_sessions ADD COLUMN last_message_ts BIGINT;
    END IF;
END $$;

-- Add missing columns to private_messages table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'private_messages' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE private_messages ADD COLUMN created_ts BIGINT NOT NULL DEFAULT EXTRACT(EPOCH FROM NOW())::BIGINT;
    END IF;
END $$;

-- Add missing columns to voice_messages table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'voice_messages' AND column_name = 'waveform_data'
    ) THEN
        ALTER TABLE voice_messages ADD COLUMN waveform_data JSONB;
    END IF;
END $$;

-- Add missing columns to room_account_data table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'room_account_data' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE room_account_data ADD COLUMN created_ts TIMESTAMPTZ NOT NULL DEFAULT NOW();
    END IF;
END $$;

-- Add missing columns to read_markers table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'read_markers' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE read_markers ADD COLUMN created_ts TIMESTAMPTZ NOT NULL DEFAULT NOW();
    END IF;
END $$;

-- Add missing columns to user_account_data table
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'user_account_data' AND column_name = 'created_ts'
    ) THEN
        ALTER TABLE user_account_data ADD COLUMN created_ts TIMESTAMPTZ NOT NULL DEFAULT NOW();
    END IF;
END $$;

\echo '✅ Additional missing columns added successfully!'
