-- Optimize private message search with pg_trgm
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Add GIST index for trigram-based search on message content
CREATE INDEX IF NOT EXISTS idx_private_messages_content_trgm ON private_messages USING gist (content gist_trgm_ops);
CREATE INDEX IF NOT EXISTS idx_private_messages_encrypted_content_trgm ON private_messages USING gist (encrypted_content gist_trgm_ops);

-- Add indexes for common filter columns if they don't exist
CREATE INDEX IF NOT EXISTS idx_private_messages_session_id ON private_messages(session_id);
CREATE INDEX IF NOT EXISTS idx_private_messages_created_ts ON private_messages(created_ts DESC);
