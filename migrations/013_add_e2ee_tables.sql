-- Add missing E2EE tables

-- Create one_time_keys table
CREATE TABLE IF NOT EXISTS one_time_keys (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    key_id VARCHAR(255) NOT NULL,
    algorithm VARCHAR(50) NOT NULL,
    key_data JSONB NOT NULL,
    signature JSONB,
    claimed BOOLEAN NOT NULL DEFAULT FALSE,
    claimed_at BIGINT,
    created_at BIGINT NOT NULL,
    CONSTRAINT fk_one_time_keys_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_one_time_keys_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE,
    CONSTRAINT uk_one_time_keys UNIQUE (user_id, device_id, key_id)
);

CREATE INDEX IF NOT EXISTS idx_one_time_keys_user ON one_time_keys(user_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_device ON one_time_keys(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_claimed ON one_time_keys(claimed);
CREATE INDEX IF NOT EXISTS idx_one_time_keys_algorithm ON one_time_keys(algorithm);

-- Create key_changes table
CREATE TABLE IF NOT EXISTS key_changes (
    id BIGSERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    change_type VARCHAR(50) NOT NULL,
    key_id VARCHAR(255),
    old_key_data JSONB,
    new_key_data JSONB,
    changed_at BIGINT NOT NULL,
    CONSTRAINT fk_key_changes_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_key_changes_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_key_changes_user ON key_changes(user_id);
CREATE INDEX IF NOT EXISTS idx_key_changes_device ON key_changes(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_key_changes_type ON key_changes(change_type);
CREATE INDEX IF NOT EXISTS idx_key_changes_changed_at ON key_changes(changed_at DESC);

-- Create room_key_distributions table
CREATE TABLE IF NOT EXISTS room_key_distributions (
    id BIGSERIAL PRIMARY KEY,
    room_id VARCHAR(255) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    device_id VARCHAR(255) NOT NULL,
    session_id VARCHAR(255) NOT NULL,
    session_key TEXT NOT NULL,
    algorithm VARCHAR(100) NOT NULL,
    key_data JSONB NOT NULL,
    distributed_at BIGINT NOT NULL,
    CONSTRAINT fk_room_key_distributions_room FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_key_distributions_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_room_key_distributions_device FOREIGN KEY (device_id) REFERENCES devices(device_id) ON DELETE CASCADE,
    CONSTRAINT uk_room_key_distributions UNIQUE (room_id, user_id, device_id, session_id)
);

CREATE INDEX IF NOT EXISTS idx_room_key_distributions_room ON room_key_distributions(room_id);
CREATE INDEX IF NOT EXISTS idx_room_key_distributions_user ON room_key_distributions(user_id);
CREATE INDEX IF NOT EXISTS idx_room_key_distributions_device ON room_key_distributions(user_id, device_id);
CREATE INDEX IF NOT EXISTS idx_room_key_distributions_session ON room_key_distributions(session_id);
CREATE INDEX IF NOT EXISTS idx_room_key_distributions_distributed_at ON room_key_distributions(distributed_at DESC);

-- Add missing columns to device_keys table if they don't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'device_keys' AND column_name = 'key_data'
    ) THEN
        ALTER TABLE device_keys ADD COLUMN key_data JSONB;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'device_keys' AND column_name = 'signatures'
    ) THEN
        ALTER TABLE device_keys ADD COLUMN signatures JSONB;
    END IF;
END $$;

-- Add missing columns to key_backups table if they don't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'key_backups' AND column_name = 'count'
    ) THEN
        ALTER TABLE key_backups ADD COLUMN count INTEGER NOT NULL DEFAULT 0;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'key_backups' AND column_name = 'etag'
    ) THEN
        ALTER TABLE key_backups ADD COLUMN etag VARCHAR(255);
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'key_backups' AND column_name = 'deleted'
    ) THEN
        ALTER TABLE key_backups ADD COLUMN deleted BOOLEAN NOT NULL DEFAULT FALSE;
    END IF;
END $$;

-- Add missing columns to backup_keys table if they don't exist
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'backup_keys' AND column_name = 'session_key_data'
    ) THEN
        ALTER TABLE backup_keys ADD COLUMN session_key_data JSONB;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'backup_keys' AND column_name = 'algorithm'
    ) THEN
        ALTER TABLE backup_keys ADD COLUMN algorithm VARCHAR(100);
    END IF;
END $$;

\echo '✅ E2EE tables added successfully!'
