-- Rollback: OpenClaw Integration Tables
-- Version: 1.0.0
-- Date: 2026-04-03

-- 删除触发器
DROP TRIGGER IF EXISTS update_openclaw_connections_updated_ts ON openclaw_connections;
DROP TRIGGER IF EXISTS update_ai_conversations_updated_ts ON ai_conversations;
DROP TRIGGER IF EXISTS update_ai_chat_roles_updated_ts ON ai_chat_roles;

-- 删除函数
DROP FUNCTION IF EXISTS update_updated_ts_column();

-- 删除索引
DROP INDEX IF EXISTS idx_openclaw_connections_user;
DROP INDEX IF EXISTS idx_openclaw_connections_provider;
DROP INDEX IF EXISTS idx_openclaw_connections_active;
DROP INDEX IF EXISTS idx_ai_conversations_user;
DROP INDEX IF EXISTS idx_ai_conversations_connection;
DROP INDEX IF EXISTS idx_ai_conversations_pinned;
DROP INDEX IF EXISTS idx_ai_conversations_updated;
DROP INDEX IF EXISTS idx_ai_messages_conversation;
DROP INDEX IF EXISTS idx_ai_messages_created;
DROP INDEX IF EXISTS idx_ai_messages_role;
DROP INDEX IF EXISTS idx_ai_generations_user;
DROP INDEX IF EXISTS idx_ai_generations_conversation;
DROP INDEX IF EXISTS idx_ai_generations_type;
DROP INDEX IF EXISTS idx_ai_generations_status;
DROP INDEX IF EXISTS idx_ai_chat_roles_user;
DROP INDEX IF EXISTS idx_ai_chat_roles_public;
DROP INDEX IF EXISTS idx_ai_chat_roles_category;

-- 删除表（按依赖顺序）
DROP TABLE IF EXISTS ai_chat_roles;
DROP TABLE IF EXISTS ai_generations;
DROP TABLE IF EXISTS ai_messages;
DROP TABLE IF EXISTS ai_conversations;
DROP TABLE IF EXISTS openclaw_connections;
