-- Undo migration: drop ai_connections table

DROP INDEX IF EXISTS idx_ai_connections_user_id;
DROP INDEX IF EXISTS idx_ai_connections_provider;
DROP TABLE IF EXISTS ai_connections;
