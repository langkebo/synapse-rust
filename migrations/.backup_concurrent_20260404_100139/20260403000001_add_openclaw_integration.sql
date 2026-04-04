-- OpenClaw Integration Tables
-- Version: 1.0.0
-- Date: 2026-04-03
-- Description: 创建 OpenClaw 集成所需的数据库表

-- ============================================
-- 1. OpenClaw 连接配置表
-- ============================================
CREATE TABLE IF NOT EXISTS openclaw_connections (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    provider TEXT NOT NULL,
    base_url TEXT NOT NULL,
    encrypted_api_key TEXT,
    config JSONB DEFAULT '{}',
    is_default BOOLEAN DEFAULT false,
    is_active BOOLEAN DEFAULT true,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    UNIQUE(user_id, name)
);

COMMENT ON TABLE openclaw_connections IS 'OpenClaw 连接配置表';
COMMENT ON COLUMN openclaw_connections.user_id IS '用户 ID';
COMMENT ON COLUMN openclaw_connections.name IS '连接名称';
COMMENT ON COLUMN openclaw_connections.provider IS '提供商: openai, anthropic, ollama, openclaw, custom';
COMMENT ON COLUMN openclaw_connections.base_url IS 'API 端点 URL';
COMMENT ON COLUMN openclaw_connections.encrypted_api_key IS '加密存储的 API Key';
COMMENT ON COLUMN openclaw_connections.config IS '其他配置 (temperature, maxTokens 等)';
COMMENT ON COLUMN openclaw_connections.is_default IS '是否为默认连接';
COMMENT ON COLUMN openclaw_connections.is_active IS '是否激活';

-- ============================================
-- 2. AI 对话记录表
-- ============================================
CREATE TABLE IF NOT EXISTS ai_conversations (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    connection_id BIGINT REFERENCES openclaw_connections(id) ON DELETE SET NULL,
    title TEXT,
    model_id TEXT,
    system_prompt TEXT,
    temperature REAL DEFAULT 0.7,
    max_tokens INTEGER DEFAULT 4096,
    is_pinned BOOLEAN DEFAULT false,
    metadata JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

COMMENT ON TABLE ai_conversations IS 'AI 对话记录表';
COMMENT ON COLUMN ai_conversations.user_id IS '用户 ID';
COMMENT ON COLUMN ai_conversations.connection_id IS '关联的 OpenClaw 连接';
COMMENT ON COLUMN ai_conversations.title IS '对话标题';
COMMENT ON COLUMN ai_conversations.model_id IS '使用的模型 ID';
COMMENT ON COLUMN ai_conversations.system_prompt IS '系统提示词';
COMMENT ON COLUMN ai_conversations.temperature IS '温度参数';
COMMENT ON COLUMN ai_conversations.max_tokens IS '最大 Token 数';
COMMENT ON COLUMN ai_conversations.is_pinned IS '是否置顶';
COMMENT ON COLUMN ai_conversations.metadata IS '其他元数据';

-- ============================================
-- 3. AI 消息记录表
-- ============================================
CREATE TABLE IF NOT EXISTS ai_messages (
    id BIGSERIAL PRIMARY KEY,
    conversation_id BIGINT NOT NULL REFERENCES ai_conversations(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system', 'tool')),
    content TEXT NOT NULL,
    token_count INTEGER,
    tool_calls JSONB,
    tool_call_id TEXT,
    metadata JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL
);

COMMENT ON TABLE ai_messages IS 'AI 消息记录表';
COMMENT ON COLUMN ai_messages.conversation_id IS '关联的对话 ID';
COMMENT ON COLUMN ai_messages.role IS '消息角色: user, assistant, system, tool';
COMMENT ON COLUMN ai_messages.content IS '消息内容';
COMMENT ON COLUMN ai_messages.token_count IS 'Token 数量';
COMMENT ON COLUMN ai_messages.tool_calls IS 'Function Calling 工具调用记录';
COMMENT ON COLUMN ai_messages.tool_call_id IS '工具调用 ID (用于关联工具响应)';
COMMENT ON COLUMN ai_messages.metadata IS '其他元数据';

-- ============================================
-- 4. AI 生成记录表 (图片/视频/音频)
-- ============================================
CREATE TABLE IF NOT EXISTS ai_generations (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    conversation_id BIGINT REFERENCES ai_conversations(id) ON DELETE SET NULL,
    type TEXT NOT NULL CHECK (type IN ('image', 'video', 'audio')),
    prompt TEXT NOT NULL,
    result_url TEXT,
    result_mxc TEXT,
    status TEXT DEFAULT 'pending' CHECK (status IN ('pending', 'processing', 'completed', 'failed')),
    error_message TEXT,
    metadata JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    completed_ts BIGINT
);

COMMENT ON TABLE ai_generations IS 'AI 生成记录表 (图片/视频/音频)';
COMMENT ON COLUMN ai_generations.user_id IS '用户 ID';
COMMENT ON COLUMN ai_generations.conversation_id IS '关联的对话 ID';
COMMENT ON COLUMN ai_generations.type IS '生成类型: image, video, audio';
COMMENT ON COLUMN ai_generations.prompt IS '提示词';
COMMENT ON COLUMN ai_generations.result_url IS '结果 URL';
COMMENT ON COLUMN ai_generations.result_mxc IS 'Matrix MXC URL';
COMMENT ON COLUMN ai_generations.status IS '状态: pending, processing, completed, failed';
COMMENT ON COLUMN ai_generations.error_message IS '错误信息';
COMMENT ON COLUMN ai_generations.metadata IS '其他元数据 (尺寸、时长等)';

-- ============================================
-- 5. AI 聊天角色表
-- ============================================
CREATE TABLE IF NOT EXISTS ai_chat_roles (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    system_message TEXT NOT NULL,
    model_id TEXT,
    avatar_url TEXT,
    category TEXT,
    temperature REAL DEFAULT 0.7,
    max_tokens INTEGER DEFAULT 4096,
    is_public BOOLEAN DEFAULT false,
    metadata JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

COMMENT ON TABLE ai_chat_roles IS 'AI 聊天角色表';
COMMENT ON COLUMN ai_chat_roles.user_id IS '用户 ID';
COMMENT ON COLUMN ai_chat_roles.name IS '角色名称';
COMMENT ON COLUMN ai_chat_roles.description IS '角色描述';
COMMENT ON COLUMN ai_chat_roles.system_message IS '系统提示词';
COMMENT ON COLUMN ai_chat_roles.model_id IS '默认模型 ID';
COMMENT ON COLUMN ai_chat_roles.avatar_url IS '头像 URL';
COMMENT ON COLUMN ai_chat_roles.category IS '分类';
COMMENT ON COLUMN ai_chat_roles.temperature IS '默认温度参数';
COMMENT ON COLUMN ai_chat_roles.max_tokens IS '默认最大 Token 数';
COMMENT ON COLUMN ai_chat_roles.is_public IS '是否公开';

-- ============================================
-- 6. 索引
-- ============================================
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_openclaw_connections_user ON openclaw_connections(user_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_openclaw_connections_provider ON openclaw_connections(provider);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_openclaw_connections_active ON openclaw_connections(is_active) WHERE is_active = true;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_conversations_user ON ai_conversations(user_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_conversations_connection ON ai_conversations(connection_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_conversations_pinned ON ai_conversations(user_id, is_pinned) WHERE is_pinned = true;
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_conversations_updated ON ai_conversations(updated_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_messages_conversation ON ai_messages(conversation_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_messages_created ON ai_messages(conversation_id, created_ts DESC);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_messages_role ON ai_messages(conversation_id, role);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_generations_user ON ai_generations(user_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_generations_conversation ON ai_generations(conversation_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_generations_type ON ai_generations(user_id, type);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_generations_status ON ai_generations(status) WHERE status IN ('pending', 'processing');

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_chat_roles_user ON ai_chat_roles(user_id);
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_chat_roles_public ON ai_chat_roles(is_public) WHERE is_public = true;
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_ai_chat_roles_category ON ai_chat_roles(category);

-- ============================================
-- 7. 触发器：自动更新 updated_ts
-- ============================================
CREATE OR REPLACE FUNCTION update_updated_ts_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_ts = EXTRACT(EPOCH FROM NOW())::BIGINT * 1000;
    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER update_openclaw_connections_updated_ts
    BEFORE UPDATE ON openclaw_connections
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_ts_column();

CREATE TRIGGER update_ai_conversations_updated_ts
    BEFORE UPDATE ON ai_conversations
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_ts_column();

CREATE TRIGGER update_ai_chat_roles_updated_ts
    BEFORE UPDATE ON ai_chat_roles
    FOR EACH ROW
    EXECUTE FUNCTION update_updated_ts_column();
