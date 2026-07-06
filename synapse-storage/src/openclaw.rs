use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationCursor {
    pub is_pinned: bool,
    pub updated_ts: i64,
    pub id: i64,
}

pub fn encode_conversation_cursor(cursor: &ConversationCursor) -> String {
    format!("{}|{}|{}", if cursor.is_pinned { 1 } else { 0 }, cursor.updated_ts, cursor.id)
}

pub fn decode_conversation_cursor(cursor: Option<&str>) -> Option<ConversationCursor> {
    let cursor = cursor?;
    let mut parts = cursor.split('|');
    let is_pinned = parts.next()?.parse::<u8>().ok()? == 1;
    let updated_ts = parts.next()?.parse::<i64>().ok()?;
    let id = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(ConversationCursor { is_pinned, updated_ts, id })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenerationCursor {
    pub created_ts: i64,
    pub id: i64,
}

pub fn encode_generation_cursor(cursor: &GenerationCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.id)
}

pub fn decode_generation_cursor(cursor: Option<&str>) -> Option<GenerationCursor> {
    let cursor = cursor?;
    let mut parts = cursor.split('|');
    let created_ts = parts.next()?.parse::<i64>().ok()?;
    let id = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(GenerationCursor { created_ts, id })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageCursor {
    pub created_ts: i64,
    pub id: i64,
}

pub fn encode_message_cursor(cursor: &MessageCursor) -> String {
    format!("{}|{}", cursor.created_ts, cursor.id)
}

pub fn decode_message_cursor(cursor: Option<&str>) -> Option<MessageCursor> {
    let cursor = cursor?;
    let mut parts = cursor.split('|');
    let created_ts = parts.next()?.parse::<i64>().ok()?;
    let id = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some(MessageCursor { created_ts, id })
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct OpenClawConnection {
    pub id: i64,
    pub user_id: String,
    pub name: String,
    pub provider: String,
    pub base_url: String,
    pub encrypted_api_key: Option<String>,
    pub config: Option<serde_json::Value>,
    pub is_default: bool,
    pub is_active: bool,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiConversation {
    pub id: i64,
    pub user_id: String,
    pub connection_id: Option<i64>,
    pub title: Option<String>,
    pub model_id: Option<String>,
    pub system_prompt: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub is_pinned: bool,
    pub metadata: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiMessage {
    pub id: i64,
    pub conversation_id: i64,
    pub role: String,
    pub content: String,
    pub token_count: Option<i32>,
    pub tool_calls: Option<serde_json::Value>,
    pub tool_call_id: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiGeneration {
    pub id: i64,
    pub user_id: String,
    pub conversation_id: Option<i64>,
    pub r#type: String,
    pub prompt: String,
    pub result_url: Option<String>,
    pub result_mxc: Option<String>,
    pub status: String,
    pub error_message: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_ts: i64,
    pub completed_ts: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct AiChatRole {
    pub id: i64,
    pub user_id: String,
    pub name: String,
    pub description: Option<String>,
    pub system_message: String,
    pub model_id: Option<String>,
    pub avatar_url: Option<String>,
    pub category: Option<String>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub is_public: bool,
    pub metadata: Option<serde_json::Value>,
    pub created_ts: i64,
    pub updated_ts: i64,
}

pub struct CreateConnectionParams<'a> {
    pub user_id: &'a str,
    pub name: &'a str,
    pub provider: &'a str,
    pub base_url: &'a str,
    pub encrypted_api_key: Option<&'a str>,
    pub config: Option<serde_json::Value>,
    pub is_default: bool,
}

pub struct UpdateConnectionParams<'a> {
    pub id: i64,
    pub name: Option<&'a str>,
    pub base_url: Option<&'a str>,
    pub encrypted_api_key: Option<&'a str>,
    pub config: Option<serde_json::Value>,
    pub is_default: Option<bool>,
    pub is_active: Option<bool>,
}

pub struct CreateConversationParams<'a> {
    pub user_id: &'a str,
    pub connection_id: Option<i64>,
    pub title: Option<&'a str>,
    pub model_id: Option<&'a str>,
    pub system_prompt: Option<&'a str>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
}

pub struct CreateChatRoleParams<'a> {
    pub user_id: &'a str,
    pub name: &'a str,
    pub description: Option<&'a str>,
    pub system_message: &'a str,
    pub model_id: Option<&'a str>,
    pub avatar_url: Option<&'a str>,
    pub category: Option<&'a str>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub is_public: bool,
}

pub struct UpdateChatRoleParams<'a> {
    pub id: i64,
    pub name: Option<&'a str>,
    pub description: Option<&'a str>,
    pub system_message: Option<&'a str>,
    pub model_id: Option<&'a str>,
    pub avatar_url: Option<&'a str>,
    pub category: Option<&'a str>,
    pub temperature: Option<f32>,
    pub max_tokens: Option<i32>,
    pub is_public: Option<bool>,
}

#[async_trait]
pub trait OpenClawStoreApi: Send + Sync {
    async fn create_connection(&self, params: CreateConnectionParams<'_>) -> Result<OpenClawConnection, sqlx::Error>;

    async fn get_connection(&self, id: i64) -> Result<Option<OpenClawConnection>, sqlx::Error>;

    async fn get_user_connections(&self, user_id: &str) -> Result<Vec<OpenClawConnection>, sqlx::Error>;

    async fn get_default_connection(&self, user_id: &str) -> Result<Option<OpenClawConnection>, sqlx::Error>;

    async fn update_connection(&self, params: UpdateConnectionParams<'_>) -> Result<OpenClawConnection, sqlx::Error>;

    async fn delete_connection(&self, id: i64) -> Result<(), sqlx::Error>;

    async fn create_conversation(&self, params: CreateConversationParams<'_>) -> Result<AiConversation, sqlx::Error>;

    async fn get_conversation(&self, id: i64) -> Result<Option<AiConversation>, sqlx::Error>;

    async fn get_user_conversations(
        &self,
        user_id: &str,
        limit: i64,
        from: Option<ConversationCursor>,
    ) -> Result<(Vec<AiConversation>, Option<String>), sqlx::Error>;

    async fn update_conversation(
        &self,
        id: i64,
        title: Option<&str>,
        system_prompt: Option<&str>,
        temperature: Option<f32>,
        max_tokens: Option<i32>,
        is_pinned: Option<bool>,
    ) -> Result<AiConversation, sqlx::Error>;

    async fn delete_conversation(&self, id: i64) -> Result<(), sqlx::Error>;

    async fn create_message(
        &self,
        conversation_id: i64,
        role: &str,
        content: &str,
        token_count: Option<i32>,
        tool_calls: Option<serde_json::Value>,
        tool_call_id: Option<&str>,
    ) -> Result<AiMessage, sqlx::Error>;

    async fn get_conversation_messages(
        &self,
        conversation_id: i64,
        limit: i64,
        from: Option<MessageCursor>,
    ) -> Result<(Vec<AiMessage>, Option<String>), sqlx::Error>;

    async fn get_message(&self, id: i64) -> Result<Option<AiMessage>, sqlx::Error>;

    async fn delete_message(&self, id: i64) -> Result<(), sqlx::Error>;

    async fn create_generation(
        &self,
        user_id: &str,
        conversation_id: Option<i64>,
        gen_type: &str,
        prompt: &str,
    ) -> Result<AiGeneration, sqlx::Error>;

    async fn update_generation_status(
        &self,
        id: i64,
        status: &str,
        result_url: Option<&str>,
        result_mxc: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<AiGeneration, sqlx::Error>;

    async fn get_generation(&self, id: i64) -> Result<Option<AiGeneration>, sqlx::Error>;

    async fn get_user_generations(
        &self,
        user_id: &str,
        gen_type: Option<&str>,
        limit: i64,
        from: Option<GenerationCursor>,
    ) -> Result<(Vec<AiGeneration>, Option<String>), sqlx::Error>;

    async fn delete_generation(&self, id: i64) -> Result<(), sqlx::Error>;

    async fn create_chat_role(&self, params: CreateChatRoleParams<'_>) -> Result<AiChatRole, sqlx::Error>;

    async fn get_chat_role(&self, id: i64) -> Result<Option<AiChatRole>, sqlx::Error>;

    async fn get_user_chat_roles(&self, user_id: &str) -> Result<Vec<AiChatRole>, sqlx::Error>;

    async fn update_chat_role(&self, params: UpdateChatRoleParams<'_>) -> Result<AiChatRole, sqlx::Error>;

    async fn delete_chat_role(&self, id: i64) -> Result<(), sqlx::Error>;
}

pub struct OpenClawStorage {
    db: Arc<PgPool>,
}

impl OpenClawStorage {
    pub fn new(db: Arc<PgPool>) -> Self {
        Self { db }
    }

    pub async fn create_connection(
        &self,
        params: CreateConnectionParams<'_>,
    ) -> Result<OpenClawConnection, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        if params.is_default {
            sqlx::query(
                r#"
                UPDATE openclaw_connections
                SET is_default = false, updated_ts = $1
                WHERE user_id = $2 AND is_default = true
                "#,
            )
            .bind(now)
            .bind(params.user_id)
            .execute(&*self.db)
            .await?;
        }

        let conn = sqlx::query_as::<_, OpenClawConnection>(
            r#"
            INSERT INTO openclaw_connections
                (user_id, name, provider, base_url, encrypted_api_key, config, is_default, is_active, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8, $8)
            RETURNING *
            "#,
        )
        .bind(params.user_id)
        .bind(params.name)
        .bind(params.provider)
        .bind(params.base_url)
        .bind(params.encrypted_api_key)
        .bind(&params.config)
        .bind(params.is_default)
        .bind(now)
        .fetch_one(&*self.db)
        .await?;

        Ok(conn)
    }

    pub async fn get_connection(&self, id: i64) -> Result<Option<OpenClawConnection>, sqlx::Error> {
        sqlx::query_as::<_, OpenClawConnection>(
            r#"
            SELECT id, user_id, name, provider, base_url, encrypted_api_key, config, is_default, is_active, created_ts, updated_ts FROM openclaw_connections WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&*self.db)
        .await
    }

    pub async fn get_user_connections(&self, user_id: &str) -> Result<Vec<OpenClawConnection>, sqlx::Error> {
        sqlx::query_as::<_, OpenClawConnection>(
            r#"
            SELECT id, user_id, name, provider, base_url, encrypted_api_key, config, is_default, is_active, created_ts, updated_ts FROM openclaw_connections
            WHERE user_id = $1
            ORDER BY is_default DESC, created_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.db)
        .await
    }

    pub async fn get_default_connection(&self, user_id: &str) -> Result<Option<OpenClawConnection>, sqlx::Error> {
        sqlx::query_as::<_, OpenClawConnection>(
            r#"
            SELECT id, user_id, name, provider, base_url, encrypted_api_key, config, is_default, is_active, created_ts, updated_ts FROM openclaw_connections
            WHERE user_id = $1 AND is_default = true AND is_active = true
            LIMIT 1
            "#,
        )
        .bind(user_id)
        .fetch_optional(&*self.db)
        .await
    }

    pub async fn update_connection(
        &self,
        params: UpdateConnectionParams<'_>,
    ) -> Result<OpenClawConnection, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        if let Some(true) = params.is_default {
            let conn = self.get_connection(params.id).await?;
            if let Some(c) = conn {
                sqlx::query(
                    r#"
                    UPDATE openclaw_connections
                    SET is_default = false, updated_ts = $1
                    WHERE user_id = $2 AND is_default = true
                    "#,
                )
                .bind(now)
                .bind(&c.user_id)
                .execute(&*self.db)
                .await?;
            }
        }

        let conn = sqlx::query_as::<_, OpenClawConnection>(
            r#"
            UPDATE openclaw_connections
            SET
                name = COALESCE($1, name),
                base_url = COALESCE($2, base_url),
                encrypted_api_key = COALESCE($3, encrypted_api_key),
                config = COALESCE($4, config),
                is_default = COALESCE($5, is_default),
                is_active = COALESCE($6, is_active),
                updated_ts = $7
            WHERE id = $8
            RETURNING *
            "#,
        )
        .bind(params.name)
        .bind(params.base_url)
        .bind(params.encrypted_api_key)
        .bind(&params.config)
        .bind(params.is_default)
        .bind(params.is_active)
        .bind(now)
        .bind(params.id)
        .fetch_one(&*self.db)
        .await?;

        Ok(conn)
    }

    pub async fn delete_connection(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM openclaw_connections WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&*self.db)
        .await?;

        Ok(())
    }

    pub async fn create_conversation(
        &self,
        params: CreateConversationParams<'_>,
    ) -> Result<AiConversation, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let conv = sqlx::query_as::<_, AiConversation>(
            r#"
            INSERT INTO ai_conversations
                (user_id, connection_id, title, model_id, system_prompt, temperature, max_tokens, is_pinned, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, false, $8, $8)
            RETURNING *
            "#,
        )
        .bind(params.user_id)
        .bind(params.connection_id)
        .bind(params.title)
        .bind(params.model_id)
        .bind(params.system_prompt)
        .bind(params.temperature)
        .bind(params.max_tokens)
        .bind(now)
        .fetch_one(&*self.db)
        .await?;

        Ok(conv)
    }

    pub async fn get_conversation(&self, id: i64) -> Result<Option<AiConversation>, sqlx::Error> {
        sqlx::query_as::<_, AiConversation>(
            r#"
            SELECT id, user_id, connection_id, title, model_id, system_prompt, temperature, max_tokens, is_pinned, metadata, created_ts, updated_ts FROM ai_conversations WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&*self.db)
        .await
    }

    pub async fn get_user_conversations(
        &self,
        user_id: &str,
        limit: i64,
        from: Option<ConversationCursor>,
    ) -> Result<(Vec<AiConversation>, Option<String>), sqlx::Error> {
        let conversations = if let Some(cursor) = from {
            sqlx::query_as::<_, AiConversation>(
                r#"
                SELECT id, user_id, connection_id, title, model_id, system_prompt, temperature, max_tokens, is_pinned, metadata, created_ts, updated_ts FROM ai_conversations
                WHERE user_id = $1
                  AND (is_pinned, updated_ts, id) < ($2, $3, $4)
                ORDER BY is_pinned DESC, updated_ts DESC, id DESC
                LIMIT $5
                "#,
            )
            .bind(user_id)
            .bind(cursor.is_pinned)
            .bind(cursor.updated_ts)
            .bind(cursor.id)
            .bind(limit + 1)
            .fetch_all(&*self.db)
            .await?
        } else {
            sqlx::query_as::<_, AiConversation>(
                r#"
                SELECT id, user_id, connection_id, title, model_id, system_prompt, temperature, max_tokens, is_pinned, metadata, created_ts, updated_ts FROM ai_conversations
                WHERE user_id = $1
                ORDER BY is_pinned DESC, updated_ts DESC, id DESC
                LIMIT $2
                "#,
            )
            .bind(user_id)
            .bind(limit + 1)
            .fetch_all(&*self.db)
            .await?
        };

        let next_batch = if conversations.len() > limit as usize {
            conversations.get(limit as usize).map(|conversation| {
                encode_conversation_cursor(&ConversationCursor {
                    is_pinned: conversation.is_pinned,
                    updated_ts: conversation.updated_ts,
                    id: conversation.id,
                })
            })
        } else {
            None
        };

        Ok((conversations.into_iter().take(limit as usize).collect(), next_batch))
    }

    pub async fn update_conversation(
        &self,
        id: i64,
        title: Option<&str>,
        system_prompt: Option<&str>,
        temperature: Option<f32>,
        max_tokens: Option<i32>,
        is_pinned: Option<bool>,
    ) -> Result<AiConversation, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let conv = sqlx::query_as::<_, AiConversation>(
            r#"
            UPDATE ai_conversations
            SET
                title = COALESCE($1, title),
                system_prompt = COALESCE($2, system_prompt),
                temperature = COALESCE($3, temperature),
                max_tokens = COALESCE($4, max_tokens),
                is_pinned = COALESCE($5, is_pinned),
                updated_ts = $6
            WHERE id = $7
            RETURNING *
            "#,
        )
        .bind(title)
        .bind(system_prompt)
        .bind(temperature)
        .bind(max_tokens)
        .bind(is_pinned)
        .bind(now)
        .bind(id)
        .fetch_one(&*self.db)
        .await?;

        Ok(conv)
    }

    pub async fn delete_conversation(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM ai_conversations WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&*self.db)
        .await?;

        Ok(())
    }

    pub async fn create_message(
        &self,
        conversation_id: i64,
        role: &str,
        content: &str,
        token_count: Option<i32>,
        tool_calls: Option<serde_json::Value>,
        tool_call_id: Option<&str>,
    ) -> Result<AiMessage, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let msg = sqlx::query_as::<_, AiMessage>(
            r#"
            INSERT INTO ai_messages
                (conversation_id, role, content, token_count, tool_calls, tool_call_id, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING *
            "#,
        )
        .bind(conversation_id)
        .bind(role)
        .bind(content)
        .bind(token_count)
        .bind(&tool_calls)
        .bind(tool_call_id)
        .bind(now)
        .fetch_one(&*self.db)
        .await?;

        sqlx::query(
            r#"
            UPDATE ai_conversations SET updated_ts = $1 WHERE id = $2
            "#,
        )
        .bind(now)
        .bind(conversation_id)
        .execute(&*self.db)
        .await?;

        Ok(msg)
    }

    pub async fn get_conversation_messages(
        &self,
        conversation_id: i64,
        limit: i64,
        from: Option<MessageCursor>,
    ) -> Result<(Vec<AiMessage>, Option<String>), sqlx::Error> {
        let messages = match from {
            Some(cursor) => {
                sqlx::query_as::<_, AiMessage>(
                    r#"
                    SELECT id, conversation_id, role, content, token_count, tool_calls, tool_call_id, metadata, created_ts
                    FROM ai_messages
                    WHERE conversation_id = $1 AND (created_ts, id) < ($2, $3)
                    ORDER BY created_ts DESC, id DESC
                    LIMIT $4
                    "#,
                )
                .bind(conversation_id)
                .bind(cursor.created_ts)
                .bind(cursor.id)
                .bind(limit + 1)
                .fetch_all(&*self.db)
                .await?
            }
            None => {
                sqlx::query_as::<_, AiMessage>(
                    r#"
                    SELECT id, conversation_id, role, content, token_count, tool_calls, tool_call_id, metadata, created_ts
                    FROM ai_messages
                    WHERE conversation_id = $1
                    ORDER BY created_ts DESC, id DESC
                    LIMIT $2
                    "#,
                )
                .bind(conversation_id)
                .bind(limit + 1)
                .fetch_all(&*self.db)
                .await?
            }
        };

        let next_batch = if messages.len() > limit as usize {
            messages
                .get(limit as usize)
                .map(|message| encode_message_cursor(&MessageCursor { created_ts: message.created_ts, id: message.id }))
        } else {
            None
        };

        Ok((messages.into_iter().take(limit as usize).collect(), next_batch))
    }

    pub async fn get_message(&self, id: i64) -> Result<Option<AiMessage>, sqlx::Error> {
        sqlx::query_as::<_, AiMessage>(
            r#"
            SELECT id, conversation_id, role, content, token_count, tool_calls, tool_call_id, metadata, created_ts FROM ai_messages WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&*self.db)
        .await
    }

    pub async fn delete_message(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM ai_messages WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&*self.db)
        .await?;

        Ok(())
    }

    pub async fn create_generation(
        &self,
        user_id: &str,
        conversation_id: Option<i64>,
        gen_type: &str,
        prompt: &str,
    ) -> Result<AiGeneration, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let gen = sqlx::query_as::<_, AiGeneration>(
            r#"
            INSERT INTO ai_generations
                (user_id, conversation_id, type, prompt, status, created_ts)
            VALUES ($1, $2, $3, $4, 'pending', $5)
            RETURNING *
            "#,
        )
        .bind(user_id)
        .bind(conversation_id)
        .bind(gen_type)
        .bind(prompt)
        .bind(now)
        .fetch_one(&*self.db)
        .await?;

        Ok(gen)
    }

    pub async fn update_generation_status(
        &self,
        id: i64,
        status: &str,
        result_url: Option<&str>,
        result_mxc: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<AiGeneration, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let gen = sqlx::query_as::<_, AiGeneration>(
            r#"
            UPDATE ai_generations
            SET
                status = $1,
                result_url = COALESCE($2, result_url),
                result_mxc = COALESCE($3, result_mxc),
                error_message = COALESCE($4, error_message),
                completed_ts = CASE WHEN $1 = 'completed' THEN $5 ELSE completed_ts END
            WHERE id = $6
            RETURNING *
            "#,
        )
        .bind(status)
        .bind(result_url)
        .bind(result_mxc)
        .bind(error_message)
        .bind(now)
        .bind(id)
        .fetch_one(&*self.db)
        .await?;

        Ok(gen)
    }

    pub async fn get_generation(&self, id: i64) -> Result<Option<AiGeneration>, sqlx::Error> {
        sqlx::query_as::<_, AiGeneration>(
            r#"
            SELECT id, user_id, conversation_id, type, prompt, result_url, result_mxc, status, error_message, metadata, created_ts, completed_ts FROM ai_generations WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&*self.db)
        .await
    }

    pub async fn get_user_generations(
        &self,
        user_id: &str,
        gen_type: Option<&str>,
        limit: i64,
        from: Option<GenerationCursor>,
    ) -> Result<(Vec<AiGeneration>, Option<String>), sqlx::Error> {
        let generations = match (gen_type, from) {
            (Some(t), Some(cursor)) => {
                sqlx::query_as::<_, AiGeneration>(
                    r#"
                    SELECT id, user_id, conversation_id, type, prompt, result_url, result_mxc, status, error_message, metadata, created_ts, completed_ts FROM ai_generations
                    WHERE user_id = $1 AND type = $2
                      AND (created_ts, id) < ($3, $4)
                    ORDER BY created_ts DESC, id DESC
                    LIMIT $5
                    "#,
                )
                .bind(user_id)
                .bind(t)
                .bind(cursor.created_ts)
                .bind(cursor.id)
                .bind(limit + 1)
                .fetch_all(&*self.db)
                .await?
            }
            (Some(t), None) => {
                sqlx::query_as::<_, AiGeneration>(
                    r#"
                    SELECT id, user_id, conversation_id, type, prompt, result_url, result_mxc, status, error_message, metadata, created_ts, completed_ts FROM ai_generations
                    WHERE user_id = $1 AND type = $2
                    ORDER BY created_ts DESC, id DESC
                    LIMIT $3
                    "#,
                )
                .bind(user_id)
                .bind(t)
                .bind(limit + 1)
                .fetch_all(&*self.db)
                .await?
            }
            (None, Some(cursor)) => {
                sqlx::query_as::<_, AiGeneration>(
                    r#"
                    SELECT id, user_id, conversation_id, type, prompt, result_url, result_mxc, status, error_message, metadata, created_ts, completed_ts FROM ai_generations
                    WHERE user_id = $1
                      AND (created_ts, id) < ($2, $3)
                    ORDER BY created_ts DESC, id DESC
                    LIMIT $4
                    "#,
                )
                .bind(user_id)
                .bind(cursor.created_ts)
                .bind(cursor.id)
                .bind(limit + 1)
                .fetch_all(&*self.db)
                .await?
            }
            (None, None) => {
                sqlx::query_as::<_, AiGeneration>(
                    r#"
                    SELECT id, user_id, conversation_id, type, prompt, result_url, result_mxc, status, error_message, metadata, created_ts, completed_ts FROM ai_generations
                    WHERE user_id = $1
                    ORDER BY created_ts DESC, id DESC
                    LIMIT $2
                    "#,
                )
                .bind(user_id)
                .bind(limit + 1)
                .fetch_all(&*self.db)
                .await?
            }
        };

        let next_batch = if generations.len() > limit as usize {
            generations.get(limit as usize).map(|generation| {
                encode_generation_cursor(&GenerationCursor { created_ts: generation.created_ts, id: generation.id })
            })
        } else {
            None
        };

        Ok((generations.into_iter().take(limit as usize).collect(), next_batch))
    }

    pub async fn delete_generation(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM ai_generations WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&*self.db)
        .await?;

        Ok(())
    }

    pub async fn create_chat_role(&self, params: CreateChatRoleParams<'_>) -> Result<AiChatRole, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let role = sqlx::query_as::<_, AiChatRole>(
            r#"
            INSERT INTO ai_chat_roles
                (user_id, name, description, system_message, model_id, avatar_url, category, temperature, max_tokens, is_public, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
            RETURNING *
            "#,
        )
        .bind(params.user_id)
        .bind(params.name)
        .bind(params.description)
        .bind(params.system_message)
        .bind(params.model_id)
        .bind(params.avatar_url)
        .bind(params.category)
        .bind(params.temperature)
        .bind(params.max_tokens)
        .bind(params.is_public)
        .bind(now)
        .fetch_one(&*self.db)
        .await?;

        Ok(role)
    }

    pub async fn get_chat_role(&self, id: i64) -> Result<Option<AiChatRole>, sqlx::Error> {
        sqlx::query_as::<_, AiChatRole>(
            r#"
            SELECT id, user_id, name, description, system_message, model_id, avatar_url, category, temperature, max_tokens, is_public, metadata, created_ts, updated_ts FROM ai_chat_roles WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&*self.db)
        .await
    }

    pub async fn get_user_chat_roles(&self, user_id: &str) -> Result<Vec<AiChatRole>, sqlx::Error> {
        sqlx::query_as::<_, AiChatRole>(
            r#"
            SELECT id, user_id, name, description, system_message, model_id, avatar_url, category, temperature, max_tokens, is_public, metadata, created_ts, updated_ts FROM ai_chat_roles
            WHERE user_id = $1 OR is_public = true
            ORDER BY is_public, created_ts DESC
            "#,
        )
        .bind(user_id)
        .fetch_all(&*self.db)
        .await
    }

    pub async fn update_chat_role(&self, params: UpdateChatRoleParams<'_>) -> Result<AiChatRole, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let role = sqlx::query_as::<_, AiChatRole>(
            r#"
            UPDATE ai_chat_roles
            SET
                name = COALESCE($1, name),
                description = COALESCE($2, description),
                system_message = COALESCE($3, system_message),
                model_id = COALESCE($4, model_id),
                avatar_url = COALESCE($5, avatar_url),
                category = COALESCE($6, category),
                temperature = COALESCE($7, temperature),
                max_tokens = COALESCE($8, max_tokens),
                is_public = COALESCE($9, is_public),
                updated_ts = $10
            WHERE id = $11
            RETURNING *
            "#,
        )
        .bind(params.name)
        .bind(params.description)
        .bind(params.system_message)
        .bind(params.model_id)
        .bind(params.avatar_url)
        .bind(params.category)
        .bind(params.temperature)
        .bind(params.max_tokens)
        .bind(params.is_public)
        .bind(now)
        .bind(params.id)
        .fetch_one(&*self.db)
        .await?;

        Ok(role)
    }

    pub async fn delete_chat_role(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM ai_chat_roles WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&*self.db)
        .await?;

        Ok(())
    }
}

#[async_trait]
impl OpenClawStoreApi for OpenClawStorage {
    async fn create_connection(&self, params: CreateConnectionParams<'_>) -> Result<OpenClawConnection, sqlx::Error> {
        self.create_connection(params).await
    }

    async fn get_connection(&self, id: i64) -> Result<Option<OpenClawConnection>, sqlx::Error> {
        self.get_connection(id).await
    }

    async fn get_user_connections(&self, user_id: &str) -> Result<Vec<OpenClawConnection>, sqlx::Error> {
        self.get_user_connections(user_id).await
    }

    async fn get_default_connection(&self, user_id: &str) -> Result<Option<OpenClawConnection>, sqlx::Error> {
        self.get_default_connection(user_id).await
    }

    async fn update_connection(&self, params: UpdateConnectionParams<'_>) -> Result<OpenClawConnection, sqlx::Error> {
        self.update_connection(params).await
    }

    async fn delete_connection(&self, id: i64) -> Result<(), sqlx::Error> {
        self.delete_connection(id).await
    }

    async fn create_conversation(&self, params: CreateConversationParams<'_>) -> Result<AiConversation, sqlx::Error> {
        self.create_conversation(params).await
    }

    async fn get_conversation(&self, id: i64) -> Result<Option<AiConversation>, sqlx::Error> {
        self.get_conversation(id).await
    }

    async fn get_user_conversations(
        &self,
        user_id: &str,
        limit: i64,
        from: Option<ConversationCursor>,
    ) -> Result<(Vec<AiConversation>, Option<String>), sqlx::Error> {
        self.get_user_conversations(user_id, limit, from).await
    }

    async fn update_conversation(
        &self,
        id: i64,
        title: Option<&str>,
        system_prompt: Option<&str>,
        temperature: Option<f32>,
        max_tokens: Option<i32>,
        is_pinned: Option<bool>,
    ) -> Result<AiConversation, sqlx::Error> {
        self.update_conversation(id, title, system_prompt, temperature, max_tokens, is_pinned).await
    }

    async fn delete_conversation(&self, id: i64) -> Result<(), sqlx::Error> {
        self.delete_conversation(id).await
    }

    async fn create_message(
        &self,
        conversation_id: i64,
        role: &str,
        content: &str,
        token_count: Option<i32>,
        tool_calls: Option<serde_json::Value>,
        tool_call_id: Option<&str>,
    ) -> Result<AiMessage, sqlx::Error> {
        self.create_message(conversation_id, role, content, token_count, tool_calls, tool_call_id).await
    }

    async fn get_conversation_messages(
        &self,
        conversation_id: i64,
        limit: i64,
        from: Option<MessageCursor>,
    ) -> Result<(Vec<AiMessage>, Option<String>), sqlx::Error> {
        self.get_conversation_messages(conversation_id, limit, from).await
    }

    async fn get_message(&self, id: i64) -> Result<Option<AiMessage>, sqlx::Error> {
        self.get_message(id).await
    }

    async fn delete_message(&self, id: i64) -> Result<(), sqlx::Error> {
        self.delete_message(id).await
    }

    async fn create_generation(
        &self,
        user_id: &str,
        conversation_id: Option<i64>,
        gen_type: &str,
        prompt: &str,
    ) -> Result<AiGeneration, sqlx::Error> {
        self.create_generation(user_id, conversation_id, gen_type, prompt).await
    }

    async fn update_generation_status(
        &self,
        id: i64,
        status: &str,
        result_url: Option<&str>,
        result_mxc: Option<&str>,
        error_message: Option<&str>,
    ) -> Result<AiGeneration, sqlx::Error> {
        self.update_generation_status(id, status, result_url, result_mxc, error_message).await
    }

    async fn get_generation(&self, id: i64) -> Result<Option<AiGeneration>, sqlx::Error> {
        self.get_generation(id).await
    }

    async fn get_user_generations(
        &self,
        user_id: &str,
        gen_type: Option<&str>,
        limit: i64,
        from: Option<GenerationCursor>,
    ) -> Result<(Vec<AiGeneration>, Option<String>), sqlx::Error> {
        self.get_user_generations(user_id, gen_type, limit, from).await
    }

    async fn delete_generation(&self, id: i64) -> Result<(), sqlx::Error> {
        self.delete_generation(id).await
    }

    async fn create_chat_role(&self, params: CreateChatRoleParams<'_>) -> Result<AiChatRole, sqlx::Error> {
        self.create_chat_role(params).await
    }

    async fn get_chat_role(&self, id: i64) -> Result<Option<AiChatRole>, sqlx::Error> {
        self.get_chat_role(id).await
    }

    async fn get_user_chat_roles(&self, user_id: &str) -> Result<Vec<AiChatRole>, sqlx::Error> {
        self.get_user_chat_roles(user_id).await
    }

    async fn update_chat_role(&self, params: UpdateChatRoleParams<'_>) -> Result<AiChatRole, sqlx::Error> {
        self.update_chat_role(params).await
    }

    async fn delete_chat_role(&self, id: i64) -> Result<(), sqlx::Error> {
        self.delete_chat_role(id).await
    }
}

#[cfg(test)]
mod cursor_tests {
    use super::{
        decode_conversation_cursor, decode_generation_cursor, decode_message_cursor, encode_conversation_cursor,
        encode_generation_cursor, encode_message_cursor, ConversationCursor, GenerationCursor, MessageCursor,
    };

    #[test]
    fn conversation_cursor_round_trip() {
        let cursor = ConversationCursor { is_pinned: true, updated_ts: 1_746_700_000_000, id: 42 };
        let encoded = encode_conversation_cursor(&cursor);
        assert_eq!(decode_conversation_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn generation_cursor_round_trip() {
        let cursor = GenerationCursor { created_ts: 1_746_700_000_000, id: 42 };
        let encoded = encode_generation_cursor(&cursor);
        assert_eq!(decode_generation_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn message_cursor_round_trip() {
        let cursor = MessageCursor { created_ts: 1_746_700_000_000, id: 42 };
        let encoded = encode_message_cursor(&cursor);
        assert_eq!(decode_message_cursor(Some(&encoded)), Some(cursor));
    }

    #[test]
    fn openclaw_cursor_rejects_invalid_values() {
        assert_eq!(decode_conversation_cursor(Some("bad")), None);
        assert_eq!(decode_generation_cursor(Some("bad")), None);
        assert_eq!(decode_message_cursor(Some("bad")), None);
        assert_eq!(decode_generation_cursor(Some("123|")), None);
    }
}

#[cfg(test)]
mod db_tests {
    use super::{
        decode_conversation_cursor, decode_generation_cursor, decode_message_cursor, CreateChatRoleParams,
        CreateConnectionParams, CreateConversationParams, OpenClawStorage, UpdateChatRoleParams,
        UpdateConnectionParams,
    };
    use sqlx::postgres::PgPoolOptions;
    use std::env;
    use std::sync::Arc;

    async fn test_pool() -> Arc<sqlx::PgPool> {
        let db_url = env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://synapse:synapse@localhost:15432/synapse".to_string());
        let pool =
            PgPoolOptions::new().max_connections(2).connect(&db_url).await.expect("Failed to connect to test database");
        Arc::new(pool)
    }

    async fn ensure_test_user(pool: &sqlx::PgPool, user_id: &str) {
        let username = user_id.strip_prefix('@').and_then(|u| u.split(':').next()).unwrap_or("testuser");
        sqlx::query(
            "INSERT INTO users (user_id, username, created_ts) VALUES ($1, $2, EXTRACT(EPOCH FROM NOW()) * 1000) ON CONFLICT (user_id) DO NOTHING",
        )
        .bind(user_id)
        .bind(username)
        .execute(pool)
        .await
        .ok();
    }

    async fn clean_test_data(pool: &sqlx::PgPool, suffix: &str) {
        let pattern = format!("%{}%", suffix);
        // Must delete child-table rows first due to FK constraints
        sqlx::query(
            "DELETE FROM ai_messages WHERE conversation_id IN (SELECT id FROM ai_conversations WHERE user_id LIKE $1)",
        )
        .bind(&pattern)
        .execute(pool)
        .await
        .ok();
        sqlx::query("DELETE FROM ai_conversations WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
        sqlx::query("DELETE FROM ai_generations WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
        sqlx::query("DELETE FROM ai_chat_roles WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
        sqlx::query("DELETE FROM openclaw_connections WHERE user_id LIKE $1").bind(&pattern).execute(pool).await.ok();
    }

    fn unique_suffix() -> String {
        uuid::Uuid::new_v4().to_string().replace('-', "")
    }

    fn build_user_id(suffix: &str) -> String {
        format!("@oc_test_{suffix}:localhost")
    }

    // ========================================================================
    // Connection tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_connection() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let config = serde_json::json!({"temperature": 0.7, "max_tokens": 4096});
        let conn = storage
            .create_connection(CreateConnectionParams {
                user_id: &user_id,
                name: "test-conn",
                provider: "openai",
                base_url: "https://api.openai.com",
                encrypted_api_key: Some("enc-key-123"),
                config: Some(config.clone()),
                is_default: true,
            })
            .await
            .expect("create_connection failed");

        assert_eq!(conn.user_id, user_id);
        assert_eq!(conn.name, "test-conn");
        assert_eq!(conn.provider, "openai");
        assert_eq!(conn.base_url, "https://api.openai.com");
        assert_eq!(conn.encrypted_api_key.as_deref(), Some("enc-key-123"));
        assert_eq!(conn.config.as_ref(), Some(&config));
        assert!(conn.is_default);
        assert!(conn.is_active);
        assert!(conn.created_ts > 0);
        assert_eq!(conn.created_ts, conn.updated_ts);

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_create_connection_nullable_fields_none() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let conn = storage
            .create_connection(CreateConnectionParams {
                user_id: &user_id,
                name: "minimal-conn",
                provider: "ollama",
                base_url: "http://localhost:11434",
                encrypted_api_key: None,
                config: None,
                is_default: false,
            })
            .await
            .expect("create_connection should succeed with nullable fields as None");

        assert_eq!(conn.name, "minimal-conn");
        assert!(conn.encrypted_api_key.is_none());
        assert!(conn.config.is_none());
        assert!(!conn.is_default);

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_connection_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let created = storage
            .create_connection(CreateConnectionParams {
                user_id: &user_id,
                name: "get-test",
                provider: "anthropic",
                base_url: "https://api.anthropic.com",
                encrypted_api_key: None,
                config: None,
                is_default: false,
            })
            .await
            .expect("create_connection failed");

        let found =
            storage.get_connection(created.id).await.expect("get_connection failed").expect("connection should exist");
        assert_eq!(found.id, created.id);
        assert_eq!(found.name, "get-test");

        let missing = storage.get_connection(-1).await.expect("get_connection should not error for missing id");
        assert!(missing.is_none());

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_user_connections_multiple_and_ordered() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        // Create two non-default connections first, then a default one
        storage
            .create_connection(CreateConnectionParams {
                user_id: &user_id,
                name: "conn-a",
                provider: "openai",
                base_url: "https://a.example.com",
                encrypted_api_key: None,
                config: None,
                is_default: false,
            })
            .await
            .expect("create conn-a");
        // Small sleep to ensure different created_ts
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        storage
            .create_connection(CreateConnectionParams {
                user_id: &user_id,
                name: "conn-b",
                provider: "anthropic",
                base_url: "https://b.example.com",
                encrypted_api_key: None,
                config: None,
                is_default: true,
            })
            .await
            .expect("create conn-b");
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        storage
            .create_connection(CreateConnectionParams {
                user_id: &user_id,
                name: "conn-c",
                provider: "ollama",
                base_url: "https://c.example.com",
                encrypted_api_key: None,
                config: None,
                is_default: false,
            })
            .await
            .expect("create conn-c");

        let connections = storage.get_user_connections(&user_id).await.expect("get_user_connections failed");
        assert_eq!(connections.len(), 3);
        // Default connection should be first (ordered by is_default DESC, created_ts DESC)
        assert!(connections[0].is_default);
        assert_eq!(connections[0].name, "conn-b");

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_user_connections_empty() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let connections = storage.get_user_connections(&user_id).await.expect("get_user_connections failed");
        assert!(connections.is_empty());

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_default_connection_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        // No default should exist yet
        let none = storage.get_default_connection(&user_id).await.expect("get_default_connection failed");
        assert!(none.is_none());

        // Create a default (is_active=true by default in create_connection)
        let created = storage
            .create_connection(CreateConnectionParams {
                user_id: &user_id,
                name: "default-conn",
                provider: "openai",
                base_url: "https://api.openai.com",
                encrypted_api_key: None,
                config: None,
                is_default: true,
            })
            .await
            .expect("create_connection failed");

        let found = storage
            .get_default_connection(&user_id)
            .await
            .expect("get_default_connection failed")
            .expect("should find default connection");
        assert_eq!(found.id, created.id);
        assert!(found.is_default);
        assert!(found.is_active);

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_update_connection_fields() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let created = storage
            .create_connection(CreateConnectionParams {
                user_id: &user_id,
                name: "to-update",
                provider: "openai",
                base_url: "https://a.example.com",
                encrypted_api_key: None,
                config: None,
                is_default: false,
            })
            .await
            .expect("create_connection");

        let new_config = serde_json::json!({"temperature": 0.5});
        let updated = storage
            .update_connection(UpdateConnectionParams {
                id: created.id,
                name: Some("updated-name"),
                base_url: Some("https://b.example.com"),
                encrypted_api_key: Some("new-key"),
                config: Some(new_config.clone()),
                is_default: None,
                is_active: Some(false),
            })
            .await
            .expect("update_connection failed");

        assert_eq!(updated.id, created.id);
        assert_eq!(updated.name, "updated-name");
        assert_eq!(updated.base_url, "https://b.example.com");
        assert_eq!(updated.encrypted_api_key.as_deref(), Some("new-key"));
        assert_eq!(updated.config.as_ref(), Some(&new_config));
        assert!(!updated.is_active);

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_update_connection_set_default() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let conn1 = storage
            .create_connection(CreateConnectionParams {
                user_id: &user_id,
                name: "default-conn",
                provider: "openai",
                base_url: "https://a.example.com",
                encrypted_api_key: None,
                config: None,
                is_default: true,
            })
            .await
            .expect("create conn1");

        let conn2 = storage
            .create_connection(CreateConnectionParams {
                user_id: &user_id,
                name: "new-default",
                provider: "anthropic",
                base_url: "https://b.example.com",
                encrypted_api_key: None,
                config: None,
                is_default: false,
            })
            .await
            .expect("create conn2");

        // Make conn2 the new default
        let updated = storage
            .update_connection(UpdateConnectionParams {
                id: conn2.id,
                name: None,
                base_url: None,
                encrypted_api_key: None,
                config: None,
                is_default: Some(true),
                is_active: None,
            })
            .await
            .expect("update_connection to set default");
        assert!(updated.is_default);

        // conn1 should no longer be default
        let conn1_after = storage.get_connection(conn1.id).await.expect("get").expect("exists");
        assert!(!conn1_after.is_default);

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_delete_connection() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let created = storage
            .create_connection(CreateConnectionParams {
                user_id: &user_id,
                name: "to-delete",
                provider: "openai",
                base_url: "https://example.com",
                encrypted_api_key: None,
                config: None,
                is_default: false,
            })
            .await
            .expect("create_connection");

        storage.delete_connection(created.id).await.expect("delete_connection failed");

        let after = storage.get_connection(created.id).await.expect("get_connection");
        assert!(after.is_none());

        // Idempotent: deleting again should not error
        storage.delete_connection(created.id).await.expect("delete_connection should be idempotent");

        clean_test_data(&pool, &suffix).await;
    }

    // ========================================================================
    // Conversation tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_conversation() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let conv = storage
            .create_conversation(CreateConversationParams {
                user_id: &user_id,
                connection_id: None,
                title: Some("Test Conversation"),
                model_id: Some("gpt-4"),
                system_prompt: Some("You are helpful."),
                temperature: Some(0.8),
                max_tokens: Some(2048),
            })
            .await
            .expect("create_conversation failed");

        assert_eq!(conv.user_id, user_id);
        assert_eq!(conv.title.as_deref(), Some("Test Conversation"));
        assert_eq!(conv.model_id.as_deref(), Some("gpt-4"));
        assert_eq!(conv.system_prompt.as_deref(), Some("You are helpful."));
        assert!((conv.temperature.unwrap() - 0.8).abs() < f32::EPSILON);
        assert_eq!(conv.max_tokens, Some(2048));
        assert!(!conv.is_pinned);
        assert!(conv.connection_id.is_none());
        assert!(conv.created_ts > 0);

        // Also test creating with only required fields (title=None)
        let conv2 = storage
            .create_conversation(CreateConversationParams {
                user_id: &user_id,
                connection_id: None,
                title: None,
                model_id: None,
                system_prompt: None,
                temperature: None,
                max_tokens: None,
            })
            .await
            .expect("create_conversation with None fields");
        assert!(conv2.title.is_none());
        assert!(conv2.id != conv.id);

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_conversation_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let created = storage
            .create_conversation(CreateConversationParams {
                user_id: &user_id,
                connection_id: None,
                title: Some("My Conversation"),
                model_id: None,
                system_prompt: None,
                temperature: None,
                max_tokens: None,
            })
            .await
            .expect("create_conversation");

        let found = storage
            .get_conversation(created.id)
            .await
            .expect("get_conversation failed")
            .expect("should find conversation");
        assert_eq!(found.id, created.id);
        assert_eq!(found.title.as_deref(), Some("My Conversation"));

        let missing = storage.get_conversation(-1).await.expect("get_conversation should not error");
        assert!(missing.is_none());

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_user_conversations_pagination_and_empty() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        // Empty case first
        let (empty, next) =
            storage.get_user_conversations(&user_id, 10, None).await.expect("get_user_conversations failed");
        assert!(empty.is_empty());
        assert!(next.is_none());

        // Create 5 conversations
        for i in 0..5 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            storage
                .create_conversation(CreateConversationParams {
                    user_id: &user_id,
                    connection_id: None,
                    title: Some(&format!("Conv {i}")),
                    model_id: None,
                    system_prompt: None,
                    temperature: None,
                    max_tokens: None,
                })
                .await
                .expect("create_conversation");
        }

        // Fetch with limit=3, should return 3 + next_batch cursor
        let (convs, cursor) =
            storage.get_user_conversations(&user_id, 3, None).await.expect("get_user_conversations failed");
        assert_eq!(convs.len(), 3);
        assert!(cursor.is_some(), "should have next-batch cursor");

        // Use cursor to get next page (limit=3 again)
        let decoded = decode_conversation_cursor(cursor.as_deref()).expect("cursor should decode");
        let (convs2, cursor2) = storage
            .get_user_conversations(&user_id, 3, Some(decoded))
            .await
            .expect("get_user_conversations with cursor failed");
        // With limit=3 and 5 total, we fetched 4 rows (limit+1) on page1,
        // returned 3, and the cursor points to the 4th item. On page2, only
        // the 5th (oldest) item remains.
        assert_eq!(convs2.len(), 1);
        assert!(cursor2.is_none(), "no more pages");

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_update_conversation() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let created = storage
            .create_conversation(CreateConversationParams {
                user_id: &user_id,
                connection_id: None,
                title: Some("Original"),
                model_id: None,
                system_prompt: None,
                temperature: None,
                max_tokens: None,
            })
            .await
            .expect("create_conversation");

        let updated = storage
            .update_conversation(
                created.id,
                Some("Updated Title"),
                Some("New system prompt"),
                Some(0.3),
                Some(1024),
                Some(true),
            )
            .await
            .expect("update_conversation failed");

        assert_eq!(updated.id, created.id);
        assert_eq!(updated.title.as_deref(), Some("Updated Title"));
        assert_eq!(updated.system_prompt.as_deref(), Some("New system prompt"));
        assert!((updated.temperature.unwrap() - 0.3).abs() < f32::EPSILON);
        assert_eq!(updated.max_tokens, Some(1024));
        assert!(updated.is_pinned);

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_delete_conversation() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let created = storage
            .create_conversation(CreateConversationParams {
                user_id: &user_id,
                connection_id: None,
                title: Some("To Delete"),
                model_id: None,
                system_prompt: None,
                temperature: None,
                max_tokens: None,
            })
            .await
            .expect("create_conversation");

        storage.delete_conversation(created.id).await.expect("delete_conversation failed");

        let after = storage.get_conversation(created.id).await.expect("get_conversation");
        assert!(after.is_none());

        // Idempotent: deleting again does not error
        storage.delete_conversation(created.id).await.expect("delete_conversation idempotent");

        clean_test_data(&pool, &suffix).await;
    }

    // ========================================================================
    // Message tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_message_user_and_assistant_roles() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let conv = storage
            .create_conversation(CreateConversationParams {
                user_id: &user_id,
                connection_id: None,
                title: None,
                model_id: None,
                system_prompt: None,
                temperature: None,
                max_tokens: None,
            })
            .await
            .expect("create_conversation");

        let tool_calls = serde_json::json!([{"name": "search", "args": {"q": "weather"}}]);
        let msg_user = storage
            .create_message(conv.id, "user", "Hello, world!", Some(50), None, None)
            .await
            .expect("create user message");

        assert_eq!(msg_user.conversation_id, conv.id);
        assert_eq!(msg_user.role, "user");
        assert_eq!(msg_user.content, "Hello, world!");
        assert_eq!(msg_user.token_count, Some(50));
        assert!(msg_user.tool_calls.is_none());
        assert!(msg_user.created_ts > 0);

        let msg_assistant = storage
            .create_message(conv.id, "assistant", "Hi there!", Some(30), Some(tool_calls.clone()), Some("call_123"))
            .await
            .expect("create assistant message");

        assert_eq!(msg_assistant.role, "assistant");
        assert_eq!(msg_assistant.content, "Hi there!");
        assert_eq!(msg_assistant.tool_calls.as_ref(), Some(&tool_calls));
        assert_eq!(msg_assistant.tool_call_id.as_deref(), Some("call_123"));

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_conversation_messages_ordered_and_pagination() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let conv = storage
            .create_conversation(CreateConversationParams {
                user_id: &user_id,
                connection_id: None,
                title: None,
                model_id: None,
                system_prompt: None,
                temperature: None,
                max_tokens: None,
            })
            .await
            .expect("create_conversation");

        // Create 4 messages (sleep to guarantee different timestamps)
        for i in 0..4 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            storage
                .create_message(conv.id, "user", &format!("Message {i}"), None, None, None)
                .await
                .expect("create message");
        }

        // Fetch all 4
        let (msgs, next) =
            storage.get_conversation_messages(conv.id, 10, None).await.expect("get_conversation_messages failed");
        assert_eq!(msgs.len(), 4);
        assert!(next.is_none());

        // Messages should be in descending order by created_ts (newest first)
        for i in 1..msgs.len() {
            assert!(msgs[i - 1].created_ts >= msgs[i].created_ts, "messages should be ordered by created_ts DESC");
        }

        // Pagination: limit=2
        let (page1, cursor1) = storage.get_conversation_messages(conv.id, 2, None).await.expect("first page");
        assert_eq!(page1.len(), 2);
        assert!(cursor1.is_some());

        let decoded = decode_message_cursor(cursor1.as_deref()).expect("message cursor should decode");
        let (page2, cursor2) = storage.get_conversation_messages(conv.id, 2, Some(decoded)).await.expect("second page");
        assert_eq!(page2.len(), 1);
        assert!(cursor2.is_none(), "no more pages");

        // Ensure pages don't overlap
        let page1_ids: Vec<i64> = page1.iter().map(|m| m.id).collect();
        let page2_ids: Vec<i64> = page2.iter().map(|m| m.id).collect();
        for id in &page1_ids {
            assert!(!page2_ids.contains(id), "pages must not overlap");
        }

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_message_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let conv = storage
            .create_conversation(CreateConversationParams {
                user_id: &user_id,
                connection_id: None,
                title: None,
                model_id: None,
                system_prompt: None,
                temperature: None,
                max_tokens: None,
            })
            .await
            .expect("create_conversation");

        let msg = storage.create_message(conv.id, "user", "Hello", None, None, None).await.expect("create message");

        let found = storage.get_message(msg.id).await.expect("get_message failed").expect("should find message");
        assert_eq!(found.id, msg.id);
        assert_eq!(found.content, "Hello");

        let missing = storage.get_message(-1).await.expect("get_message for missing id");
        assert!(missing.is_none());

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_delete_message() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let conv = storage
            .create_conversation(CreateConversationParams {
                user_id: &user_id,
                connection_id: None,
                title: None,
                model_id: None,
                system_prompt: None,
                temperature: None,
                max_tokens: None,
            })
            .await
            .expect("create_conversation");

        let msg = storage.create_message(conv.id, "user", "Delete me", None, None, None).await.expect("create message");

        storage.delete_message(msg.id).await.expect("delete_message failed");

        let after = storage.get_message(msg.id).await.expect("get_message");
        assert!(after.is_none());

        // Idempotent
        storage.delete_message(msg.id).await.expect("delete_message idempotent");

        clean_test_data(&pool, &suffix).await;
    }

    // ========================================================================
    // Generation tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_and_update_generation() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let gen = storage
            .create_generation(&user_id, None, "image", "A beautiful sunset")
            .await
            .expect("create_generation failed");

        assert_eq!(gen.user_id, user_id);
        assert_eq!(gen.r#type, "image");
        assert_eq!(gen.prompt, "A beautiful sunset");
        assert_eq!(gen.status, "pending");
        assert!(gen.created_ts > 0);
        assert!(gen.completed_ts.is_none());
        assert!(gen.conversation_id.is_none());

        // Update to completed
        let updated = storage
            .update_generation_status(
                gen.id,
                "completed",
                Some("https://cdn.example.com/img.png"),
                Some("mxc://localhost/img"),
                None,
            )
            .await
            .expect("update_generation_status failed");

        assert_eq!(updated.status, "completed");
        assert_eq!(updated.result_url.as_deref(), Some("https://cdn.example.com/img.png"));
        assert_eq!(updated.result_mxc.as_deref(), Some("mxc://localhost/img"));
        assert!(updated.completed_ts.is_some());

        // Update to failed with error message
        let gen2 = storage
            .create_generation(&user_id, None, "video", "Generate a video")
            .await
            .expect("create second generation");

        let failed = storage
            .update_generation_status(gen2.id, "failed", None, None, Some("API quota exceeded"))
            .await
            .expect("update to failed");

        assert_eq!(failed.status, "failed");
        assert_eq!(failed.error_message.as_deref(), Some("API quota exceeded"));

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_generation_found_and_not_found() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let gen = storage.create_generation(&user_id, None, "image", "Prompt").await.expect("create_generation");

        let found =
            storage.get_generation(gen.id).await.expect("get_generation failed").expect("should find generation");
        assert_eq!(found.id, gen.id);

        let missing = storage.get_generation(-1).await.expect("get_generation for missing");
        assert!(missing.is_none());

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_user_generations_filter_and_pagination() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        // Create 3 image and 2 audio generations
        for i in 0..3 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            storage
                .create_generation(&user_id, None, "image", &format!("Image prompt {i}"))
                .await
                .expect("create image gen");
        }
        for i in 0..2 {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            storage
                .create_generation(&user_id, None, "audio", &format!("Audio prompt {i}"))
                .await
                .expect("create audio gen");
        }

        // All generations without type filter
        let (all, _) = storage.get_user_generations(&user_id, None, 10, None).await.expect("get_user_generations all");
        assert_eq!(all.len(), 5);

        // Filter by type "image" → 3
        let (images, _) = storage
            .get_user_generations(&user_id, Some("image"), 10, None)
            .await
            .expect("get_user_generations image filter");
        assert_eq!(images.len(), 3);
        for img in &images {
            assert_eq!(img.r#type, "image");
        }

        // Filter by type "audio" → 2
        let (audios, _) = storage
            .get_user_generations(&user_id, Some("audio"), 10, None)
            .await
            .expect("get_user_generations audio filter");
        assert_eq!(audios.len(), 2);
        for a in &audios {
            assert_eq!(a.r#type, "audio");
        }

        // Pagination with type filter: limit=2
        let (page1, cursor1) =
            storage.get_user_generations(&user_id, Some("image"), 2, None).await.expect("get_user_generations page1");
        assert_eq!(page1.len(), 2);
        assert!(cursor1.is_some());

        let decoded = decode_generation_cursor(cursor1.as_deref()).expect("cursor should decode");
        let (page2, cursor2) = storage
            .get_user_generations(&user_id, Some("image"), 2, Some(decoded))
            .await
            .expect("get_user_generations page2");
        assert_eq!(page2.len(), 0);
        assert!(cursor2.is_none());

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_delete_generation() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let gen = storage.create_generation(&user_id, None, "image", "Delete me").await.expect("create_generation");

        storage.delete_generation(gen.id).await.expect("delete_generation failed");

        let after = storage.get_generation(gen.id).await.expect("get_generation");
        assert!(after.is_none());

        // Idempotent
        storage.delete_generation(gen.id).await.expect("delete_generation idempotent");

        clean_test_data(&pool, &suffix).await;
    }

    // ========================================================================
    // Chat role tests
    // ========================================================================

    #[tokio::test]
    async fn test_create_and_get_chat_role() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let role = storage
            .create_chat_role(CreateChatRoleParams {
                user_id: &user_id,
                name: "My Role",
                description: Some("A test role"),
                system_message: "You are a helpful assistant.",
                model_id: Some("gpt-4"),
                avatar_url: Some("mxc://localhost/avatar"),
                category: Some("general"),
                temperature: Some(0.7),
                max_tokens: Some(2048),
                is_public: true,
            })
            .await
            .expect("create_chat_role failed");

        assert_eq!(role.user_id, user_id);
        assert_eq!(role.name, "My Role");
        assert_eq!(role.description.as_deref(), Some("A test role"));
        assert_eq!(role.system_message, "You are a helpful assistant.");
        assert!(role.is_public);
        assert!(role.created_ts > 0);

        // Get by id
        let found = storage.get_chat_role(role.id).await.expect("get_chat_role failed").expect("should find role");
        assert_eq!(found.id, role.id);

        let missing = storage.get_chat_role(-1).await.expect("get_chat_role missing");
        assert!(missing.is_none());

        clean_test_data(&pool, &suffix).await;
    }

    #[tokio::test]
    async fn test_get_user_chat_roles_includes_public() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        let suffix2 = unique_suffix();
        let user_id2 = build_user_id(&suffix2);
        clean_test_data(&pool, &suffix).await;
        clean_test_data(&pool, &suffix2).await;
        // Also clean any residual test chat roles with oc_test_ pattern
        sqlx::query("DELETE FROM ai_chat_roles WHERE user_id LIKE '%oc_test_%'").execute(pool.as_ref()).await.ok();
        ensure_test_user(&pool, &user_id).await;
        ensure_test_user(&pool, &user_id2).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        // User 1 creates a role
        storage
            .create_chat_role(CreateChatRoleParams {
                user_id: &user_id,
                name: "Private Role",
                description: None,
                system_message: "Private",
                model_id: None,
                avatar_url: None,
                category: None,
                temperature: None,
                max_tokens: None,
                is_public: false,
            })
            .await
            .expect("create private role");

        // User 2 creates a public role
        storage
            .create_chat_role(CreateChatRoleParams {
                user_id: &user_id2,
                name: "Public Role",
                description: None,
                system_message: "Public",
                model_id: None,
                avatar_url: None,
                category: None,
                temperature: None,
                max_tokens: None,
                is_public: true,
            })
            .await
            .expect("create public role");

        // User 1 sees own private role + public role of user 2
        // (may also see other public roles from residual test data)
        let roles = storage.get_user_chat_roles(&user_id).await.expect("get_user_chat_roles failed");
        assert!(roles.len() >= 2, "should see at least own private role + public role from user 2");
        // Verify that our private role is present
        let private = roles.iter().find(|r| r.name == "Private Role");
        assert!(private.is_some(), "should find own private role");
        assert!(!private.unwrap().is_public);
        // Verify that user2's public role is present
        let public = roles.iter().find(|r| r.name == "Public Role");
        assert!(public.is_some(), "should find user2's public role");
        assert!(public.unwrap().is_public);
        // Own private roles should come before public roles (ORDER BY is_public)
        let private_pos = roles.iter().position(|r| r.name == "Private Role").unwrap();
        let public_pos = roles.iter().position(|r| r.name == "Public Role").unwrap();
        assert!(private_pos < public_pos, "private roles should be ordered before public roles");

        clean_test_data(&pool, &suffix).await;
        clean_test_data(&pool, &suffix2).await;
    }

    #[tokio::test]
    async fn test_update_and_delete_chat_role() {
        let pool = test_pool().await;
        let suffix = unique_suffix();
        let user_id = build_user_id(&suffix);
        clean_test_data(&pool, &suffix).await;
        ensure_test_user(&pool, &user_id).await;

        let storage = OpenClawStorage::new(Arc::clone(&pool));

        let role = storage
            .create_chat_role(CreateChatRoleParams {
                user_id: &user_id,
                name: "Original",
                description: None,
                system_message: "Original prompt",
                model_id: None,
                avatar_url: None,
                category: None,
                temperature: None,
                max_tokens: None,
                is_public: false,
            })
            .await
            .expect("create_chat_role");

        let updated = storage
            .update_chat_role(UpdateChatRoleParams {
                id: role.id,
                name: Some("Updated Role"),
                description: Some("New description"),
                system_message: Some("Updated prompt"),
                model_id: Some("claude-4"),
                avatar_url: None,
                category: None,
                temperature: Some(0.5),
                max_tokens: Some(4096),
                is_public: Some(true),
            })
            .await
            .expect("update_chat_role failed");

        assert_eq!(updated.name, "Updated Role");
        assert_eq!(updated.description.as_deref(), Some("New description"));
        assert_eq!(updated.system_message, "Updated prompt");
        assert_eq!(updated.model_id.as_deref(), Some("claude-4"));
        assert!(updated.is_public);

        // Delete
        storage.delete_chat_role(role.id).await.expect("delete_chat_role failed");
        let after = storage.get_chat_role(role.id).await.expect("get_chat_role");
        assert!(after.is_none());

        // Idempotent
        storage.delete_chat_role(role.id).await.expect("delete_chat_role idempotent");

        clean_test_data(&pool, &suffix).await;
    }
}
