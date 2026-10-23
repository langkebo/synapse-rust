ALTER TABLE users ADD COLUMN IF NOT EXISTS email TEXT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS phone TEXT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS generation BIGINT DEFAULT 0;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS consent_version TEXT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS appservice_id TEXT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS user_type TEXT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS invalid_update_at BIGINT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS migration_state TEXT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS password_changed_ts BIGINT;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS failed_login_attempts INTEGER DEFAULT 0;
        ALTER TABLE users ADD COLUMN IF NOT EXISTS locked_until BIGINT;

        ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS token_hash TEXT;
        ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS token TEXT;
        ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS last_used_ts BIGINT;
        ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS user_agent TEXT;
        ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS ip_address TEXT;
        ALTER TABLE access_tokens ADD COLUMN IF NOT EXISTS is_revoked BOOLEAN DEFAULT FALSE;
        ALTER TABLE access_tokens ALTER COLUMN token DROP NOT NULL;

        ALTER TABLE events ADD COLUMN IF NOT EXISTS signatures JSONB DEFAULT '{}'::jsonb;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS hashes JSONB DEFAULT '{}'::jsonb;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS unsigned JSONB DEFAULT '{}'::jsonb;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS processed_at BIGINT;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS not_before BIGINT DEFAULT 0;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS status TEXT DEFAULT 'persisted';
        ALTER TABLE events ADD COLUMN IF NOT EXISTS origin TEXT DEFAULT 'self';
        ALTER TABLE events ADD COLUMN IF NOT EXISTS user_id TEXT;
        ALTER TABLE events ADD COLUMN IF NOT EXISTS stream_ordering BIGINT;
