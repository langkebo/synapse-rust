-- Migration: add ai_connections table
-- Created at: 2026-04-09
-- Description: AI connection configuration table for MCP tool integrations

CREATE TABLE IF NOT EXISTS ai_connections (
    id VARCHAR(36) PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    provider VARCHAR(50) NOT NULL,  -- 'openclaw', 'trendradar', 'hula'
    config JSONB,                   -- 连接配置（如 mcp_url: http://127.0.0.1:3333）
    is_active BOOLEAN DEFAULT true,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

CREATE INDEX IF NOT EXISTS idx_ai_connections_user_id ON ai_connections(user_id);
CREATE INDEX IF NOT EXISTS idx_ai_connections_provider ON ai_connections(provider);
