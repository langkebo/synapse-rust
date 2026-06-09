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
            sqlx::query!(
                r#"
                UPDATE openclaw_connections
                SET is_default = false, updated_ts = $1
                WHERE user_id = $2 AND is_default = true
                "#,
                now,
                params.user_id
            )
            .execute(&*self.db)
            .await?;
        }

        let conn = sqlx::query_as!(OpenClawConnection,
            r#"
            INSERT INTO openclaw_connections
                (user_id, name, provider, base_url, encrypted_api_key, config, is_default, is_active, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, true, $8, $8)
            RETURNING id as "id!", user_id as "user_id!", name as "name!", provider as "provider!",
                      base_url as "base_url!", encrypted_api_key as "encrypted_api_key?",
                      config as "config?", is_default as "is_default!", is_active as "is_active!",
                      created_ts as "created_ts!", updated_ts as "updated_ts!"
            "#,
            params.user_id,
            params.name,
            params.provider,
            params.base_url,
            params.encrypted_api_key,
            params.config.as_ref(),
            params.is_default,
            now
        )
        .fetch_one(&*self.db)
        .await?;

        Ok(conn)
    }

    pub async fn get_connection(&self, id: i64) -> Result<Option<OpenClawConnection>, sqlx::Error> {
        sqlx::query_as!(OpenClawConnection,
            r#"SELECT id as "id!", user_id as "user_id!", name as "name!", provider as "provider!",
                      base_url as "base_url!", encrypted_api_key as "encrypted_api_key?",
                      config as "config?", is_default as "is_default!", is_active as "is_active!",
                      created_ts as "created_ts!", updated_ts as "updated_ts!"
               FROM openclaw_connections WHERE id = $1"#,
            id
        )
        .fetch_optional(&*self.db)
        .await
    }

    pub async fn get_user_connections(&self, user_id: &str) -> Result<Vec<OpenClawConnection>, sqlx::Error> {
        sqlx::query_as!(OpenClawConnection,
            r#"SELECT id as "id!", user_id as "user_id!", name as "name!", provider as "provider!",
                      base_url as "base_url!", encrypted_api_key as "encrypted_api_key?",
                      config as "config?", is_default as "is_default!", is_active as "is_active!",
                      created_ts as "created_ts!", updated_ts as "updated_ts!"
               FROM openclaw_connections
               WHERE user_id = $1
               ORDER BY is_default DESC, created_ts DESC"#,
            user_id
        )
        .fetch_all(&*self.db)
        .await
    }

    pub async fn get_default_connection(&self, user_id: &str) -> Result<Option<OpenClawConnection>, sqlx::Error> {
        sqlx::query_as!(OpenClawConnection,
            r#"SELECT id as "id!", user_id as "user_id!", name as "name!", provider as "provider!",
                      base_url as "base_url!", encrypted_api_key as "encrypted_api_key?",
                      config as "config?", is_default as "is_default!", is_active as "is_active!",
                      created_ts as "created_ts!", updated_ts as "updated_ts!"
               FROM openclaw_connections
               WHERE user_id = $1 AND is_default = true AND is_active = true
               LIMIT 1"#,
            user_id
        )
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
                sqlx::query!(
                    r#"
                    UPDATE openclaw_connections
                    SET is_default = false, updated_ts = $1
                    WHERE user_id = $2 AND is_default = true
                    "#,
                    now,
                    &c.user_id
                )
                .execute(&*self.db)
                .await?;
            }
        }

        let conn = sqlx::query_as!(OpenClawConnection,
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
            RETURNING id as "id!", user_id as "user_id!", name as "name!", provider as "provider!",
                      base_url as "base_url!", encrypted_api_key as "encrypted_api_key?",
                      config as "config?", is_default as "is_default!", is_active as "is_active!",
                      created_ts as "created_ts!", updated_ts as "updated_ts!"
            "#,
            params.name,
            params.base_url,
            params.encrypted_api_key,
            params.config.as_ref(),
            params.is_default,
            params.is_active,
            now,
            params.id
        )
        .fetch_one(&*self.db)
        .await?;

        Ok(conn)
    }

    pub async fn delete_connection(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM openclaw_connections WHERE id = $1
            "#,
            id
        )
        .execute(&*self.db)
        .await?;

        Ok(())
    }

    pub async fn create_conversation(
        &self,
        params: CreateConversationParams<'_>,
    ) -> Result<AiConversation, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let conv = sqlx::query_as!(AiConversation,
            r#"
            INSERT INTO ai_conversations
                (user_id, connection_id, title, model_id, system_prompt, temperature, max_tokens, is_pinned, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, false, $8, $8)
            RETURNING id as "id!", user_id as "user_id!", connection_id as "connection_id?",
                      title as "title?", model_id as "model_id?", system_prompt as "system_prompt?",
                      temperature as "temperature?", max_tokens as "max_tokens?",
                      is_pinned as "is_pinned!", metadata as "metadata?",
                      created_ts as "created_ts!", updated_ts as "updated_ts!"
            "#,
            params.user_id,
            params.connection_id,
            params.title,
            params.model_id,
            params.system_prompt,
            params.temperature,
            params.max_tokens,
            now
        )
        .fetch_one(&*self.db)
        .await?;

        Ok(conv)
    }

    pub async fn get_conversation(&self, id: i64) -> Result<Option<AiConversation>, sqlx::Error> {
        sqlx::query_as!(AiConversation,
            r#"SELECT id as "id!", user_id as "user_id!", connection_id as "connection_id?",
                      title as "title?", model_id as "model_id?", system_prompt as "system_prompt?",
                      temperature as "temperature?", max_tokens as "max_tokens?",
                      is_pinned as "is_pinned!", metadata as "metadata?",
                      created_ts as "created_ts!", updated_ts as "updated_ts!"
               FROM ai_conversations WHERE id = $1"#,
            id
        )
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
            sqlx::query_as!(AiConversation,
                r#"SELECT id as "id!", user_id as "user_id!", connection_id as "connection_id?",
                          title as "title?", model_id as "model_id?", system_prompt as "system_prompt?",
                          temperature as "temperature?", max_tokens as "max_tokens?",
                          is_pinned as "is_pinned!", metadata as "metadata?",
                          created_ts as "created_ts!", updated_ts as "updated_ts!"
                   FROM ai_conversations
                   WHERE user_id = $1
                     AND (is_pinned, updated_ts, id) < ($2, $3, $4)
                   ORDER BY is_pinned DESC, updated_ts DESC, id DESC
                   LIMIT $5"#,
                user_id,
                cursor.is_pinned,
                cursor.updated_ts,
                cursor.id,
                limit + 1
            )
            .fetch_all(&*self.db)
            .await?
        } else {
            sqlx::query_as!(AiConversation,
                r#"SELECT id as "id!", user_id as "user_id!", connection_id as "connection_id?",
                          title as "title?", model_id as "model_id?", system_prompt as "system_prompt?",
                          temperature as "temperature?", max_tokens as "max_tokens?",
                          is_pinned as "is_pinned!", metadata as "metadata?",
                          created_ts as "created_ts!", updated_ts as "updated_ts!"
                   FROM ai_conversations
                   WHERE user_id = $1
                   ORDER BY is_pinned DESC, updated_ts DESC, id DESC
                   LIMIT $2"#,
                user_id,
                limit + 1
            )
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

        let conv = sqlx::query_as!(AiConversation,
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
            RETURNING id as "id!", user_id as "user_id!", connection_id as "connection_id?",
                      title as "title?", model_id as "model_id?", system_prompt as "system_prompt?",
                      temperature as "temperature?", max_tokens as "max_tokens?",
                      is_pinned as "is_pinned!", metadata as "metadata?",
                      created_ts as "created_ts!", updated_ts as "updated_ts!"
            "#,
            title,
            system_prompt,
            temperature,
            max_tokens,
            is_pinned,
            now,
            id
        )
        .fetch_one(&*self.db)
        .await?;

        Ok(conv)
    }

    pub async fn delete_conversation(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM ai_conversations WHERE id = $1
            "#,
            id
        )
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

        let msg = sqlx::query_as!(AiMessage,
            r#"
            INSERT INTO ai_messages
                (conversation_id, role, content, token_count, tool_calls, tool_call_id, created_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            RETURNING id as "id!", conversation_id as "conversation_id!", role as "role!",
                      content as "content!", token_count as "token_count?",
                      tool_calls as "tool_calls?", tool_call_id as "tool_call_id?",
                      metadata as "metadata?", created_ts as "created_ts!"
            "#,
            conversation_id,
            role,
            content,
            token_count,
            tool_calls.as_ref(),
            tool_call_id,
            now
        )
        .fetch_one(&*self.db)
        .await?;

        sqlx::query!(
            r#"
            UPDATE ai_conversations SET updated_ts = $1 WHERE id = $2
            "#,
            now,
            conversation_id
        )
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
                sqlx::query_as!(AiMessage,
                    r#"SELECT id as "id!", conversation_id as "conversation_id!", role as "role!",
                              content as "content!", token_count as "token_count?",
                              tool_calls as "tool_calls?", tool_call_id as "tool_call_id?",
                              metadata as "metadata?", created_ts as "created_ts!"
                       FROM ai_messages
                       WHERE conversation_id = $1
                         AND (created_ts, id) < ($2, $3)
                       ORDER BY created_ts DESC, id DESC
                       LIMIT $4"#,
                    conversation_id,
                    cursor.created_ts,
                    cursor.id,
                    limit + 1
                )
                .fetch_all(&*self.db)
                .await?
            }
            None => {
                sqlx::query_as!(AiMessage,
                    r#"SELECT id as "id!", conversation_id as "conversation_id!", role as "role!",
                              content as "content!", token_count as "token_count?",
                              tool_calls as "tool_calls?", tool_call_id as "tool_call_id?",
                              metadata as "metadata?", created_ts as "created_ts!"
                       FROM ai_messages
                       WHERE conversation_id = $1
                       ORDER BY created_ts DESC, id DESC
                       LIMIT $2"#,
                    conversation_id,
                    limit + 1
                )
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
        sqlx::query_as!(AiMessage,
            r#"SELECT id as "id!", conversation_id as "conversation_id!", role as "role!",
                      content as "content!", token_count as "token_count?",
                      tool_calls as "tool_calls?", tool_call_id as "tool_call_id?",
                      metadata as "metadata?", created_ts as "created_ts!"
               FROM ai_messages WHERE id = $1"#,
            id
        )
        .fetch_optional(&*self.db)
        .await
    }

    pub async fn delete_message(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM ai_messages WHERE id = $1
            "#,
            id
        )
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

        let gen = sqlx::query_as!(AiGeneration,
            r#"
            INSERT INTO ai_generations
                (user_id, conversation_id, type, prompt, status, created_ts)
            VALUES ($1, $2, $3, $4, 'pending', $5)
            RETURNING id as "id!", user_id as "user_id!", conversation_id as "conversation_id?",
                      type as "type!", prompt as "prompt!", result_url as "result_url?",
                      result_mxc as "result_mxc?", COALESCE(status, 'pending') as "status!",
                      error_message as "error_message?", metadata as "metadata?",
                      created_ts as "created_ts!", completed_ts as "completed_ts?"
            "#,
            user_id,
            conversation_id,
            gen_type,
            prompt,
            now
        )
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

        let gen = sqlx::query_as!(AiGeneration,
            r#"
            UPDATE ai_generations
            SET
                status = $1,
                result_url = COALESCE($2, result_url),
                result_mxc = COALESCE($3, result_mxc),
                error_message = COALESCE($4, error_message),
                completed_ts = CASE WHEN $1 = 'completed' THEN $5 ELSE completed_ts END
            WHERE id = $6
            RETURNING id as "id!", user_id as "user_id!", conversation_id as "conversation_id?",
                      type as "type!", prompt as "prompt!", result_url as "result_url?",
                      result_mxc as "result_mxc?", COALESCE(status, 'pending') as "status!",
                      error_message as "error_message?", metadata as "metadata?",
                      created_ts as "created_ts!", completed_ts as "completed_ts?"
            "#,
            status,
            result_url,
            result_mxc,
            error_message,
            now,
            id
        )
        .fetch_one(&*self.db)
        .await?;

        Ok(gen)
    }

    pub async fn get_generation(&self, id: i64) -> Result<Option<AiGeneration>, sqlx::Error> {
        sqlx::query_as!(AiGeneration,
            r#"SELECT id as "id!", user_id as "user_id!", conversation_id as "conversation_id?",
                      type as "type!", prompt as "prompt!", result_url as "result_url?",
                      result_mxc as "result_mxc?", COALESCE(status, 'pending') as "status!",
                      error_message as "error_message?", metadata as "metadata?",
                      created_ts as "created_ts!", completed_ts as "completed_ts?"
               FROM ai_generations WHERE id = $1"#,
            id
        )
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
                sqlx::query_as!(AiGeneration,
                    r#"SELECT id as "id!", user_id as "user_id!", conversation_id as "conversation_id?",
                              type as "type!", prompt as "prompt!", result_url as "result_url?",
                              result_mxc as "result_mxc?", COALESCE(status, 'pending') as "status!",
                              error_message as "error_message?", metadata as "metadata?",
                              created_ts as "created_ts!", completed_ts as "completed_ts?"
                       FROM ai_generations
                       WHERE user_id = $1 AND type = $2
                         AND (created_ts, id) < ($3, $4)
                       ORDER BY created_ts DESC, id DESC
                       LIMIT $5"#,
                    user_id,
                    t,
                    cursor.created_ts,
                    cursor.id,
                    limit + 1
                )
                .fetch_all(&*self.db)
                .await?
            }
            (Some(t), None) => {
                sqlx::query_as!(AiGeneration,
                    r#"SELECT id as "id!", user_id as "user_id!", conversation_id as "conversation_id?",
                              type as "type!", prompt as "prompt!", result_url as "result_url?",
                              result_mxc as "result_mxc?", COALESCE(status, 'pending') as "status!",
                              error_message as "error_message?", metadata as "metadata?",
                              created_ts as "created_ts!", completed_ts as "completed_ts?"
                       FROM ai_generations
                       WHERE user_id = $1 AND type = $2
                       ORDER BY created_ts DESC, id DESC
                       LIMIT $3"#,
                    user_id,
                    t,
                    limit + 1
                )
                .fetch_all(&*self.db)
                .await?
            }
            (None, Some(cursor)) => {
                sqlx::query_as!(AiGeneration,
                    r#"SELECT id as "id!", user_id as "user_id!", conversation_id as "conversation_id?",
                              type as "type!", prompt as "prompt!", result_url as "result_url?",
                              result_mxc as "result_mxc?", COALESCE(status, 'pending') as "status!",
                              error_message as "error_message?", metadata as "metadata?",
                              created_ts as "created_ts!", completed_ts as "completed_ts?"
                       FROM ai_generations
                       WHERE user_id = $1
                         AND (created_ts, id) < ($2, $3)
                       ORDER BY created_ts DESC, id DESC
                       LIMIT $4"#,
                    user_id,
                    cursor.created_ts,
                    cursor.id,
                    limit + 1
                )
                .fetch_all(&*self.db)
                .await?
            }
            (None, None) => {
                sqlx::query_as!(AiGeneration,
                    r#"SELECT id as "id!", user_id as "user_id!", conversation_id as "conversation_id?",
                              type as "type!", prompt as "prompt!", result_url as "result_url?",
                              result_mxc as "result_mxc?", COALESCE(status, 'pending') as "status!",
                              error_message as "error_message?", metadata as "metadata?",
                              created_ts as "created_ts!", completed_ts as "completed_ts?"
                       FROM ai_generations
                       WHERE user_id = $1
                       ORDER BY created_ts DESC, id DESC
                       LIMIT $2"#,
                    user_id,
                    limit + 1
                )
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
        sqlx::query!(
            r#"
            DELETE FROM ai_generations WHERE id = $1
            "#,
            id
        )
        .execute(&*self.db)
        .await?;

        Ok(())
    }

    pub async fn create_chat_role(&self, params: CreateChatRoleParams<'_>) -> Result<AiChatRole, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let role = sqlx::query_as!(AiChatRole,
            r#"
            INSERT INTO ai_chat_roles
                (user_id, name, description, system_message, model_id, avatar_url, category, temperature, max_tokens, is_public, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)
            RETURNING id as "id!", user_id as "user_id!", name as "name!", description as "description?",
                      system_message as "system_message!", model_id as "model_id?", avatar_url as "avatar_url?",
                      category as "category?", temperature as "temperature?", max_tokens as "max_tokens?",
                      is_public as "is_public!", metadata as "metadata?",
                      created_ts as "created_ts!", updated_ts as "updated_ts!"
            "#,
            params.user_id,
            params.name,
            params.description,
            params.system_message,
            params.model_id,
            params.avatar_url,
            params.category,
            params.temperature,
            params.max_tokens,
            params.is_public,
            now
        )
        .fetch_one(&*self.db)
        .await?;

        Ok(role)
    }

    pub async fn get_chat_role(&self, id: i64) -> Result<Option<AiChatRole>, sqlx::Error> {
        sqlx::query_as!(AiChatRole,
            r#"SELECT id as "id!", user_id as "user_id!", name as "name!", description as "description?",
                      system_message as "system_message!", model_id as "model_id?", avatar_url as "avatar_url?",
                      category as "category?", temperature as "temperature?", max_tokens as "max_tokens?",
                      is_public as "is_public!", metadata as "metadata?",
                      created_ts as "created_ts!", updated_ts as "updated_ts!"
               FROM ai_chat_roles WHERE id = $1"#,
            id
        )
        .fetch_optional(&*self.db)
        .await
    }

    pub async fn get_user_chat_roles(&self, user_id: &str) -> Result<Vec<AiChatRole>, sqlx::Error> {
        sqlx::query_as!(AiChatRole,
            r#"SELECT id as "id!", user_id as "user_id!", name as "name!", description as "description?",
                      system_message as "system_message!", model_id as "model_id?", avatar_url as "avatar_url?",
                      category as "category?", temperature as "temperature?", max_tokens as "max_tokens?",
                      is_public as "is_public!", metadata as "metadata?",
                      created_ts as "created_ts!", updated_ts as "updated_ts!"
               FROM ai_chat_roles
               WHERE user_id = $1 OR is_public = true
               ORDER BY is_public, created_ts DESC"#,
            user_id
        )
        .fetch_all(&*self.db)
        .await
    }

    pub async fn update_chat_role(&self, params: UpdateChatRoleParams<'_>) -> Result<AiChatRole, sqlx::Error> {
        let now = chrono::Utc::now().timestamp_millis();

        let role = sqlx::query_as!(AiChatRole,
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
            RETURNING id as "id!", user_id as "user_id!", name as "name!", description as "description?",
                      system_message as "system_message!", model_id as "model_id?", avatar_url as "avatar_url?",
                      category as "category?", temperature as "temperature?", max_tokens as "max_tokens?",
                      is_public as "is_public!", metadata as "metadata?",
                      created_ts as "created_ts!", updated_ts as "updated_ts!"
            "#,
            params.name,
            params.description,
            params.system_message,
            params.model_id,
            params.avatar_url,
            params.category,
            params.temperature,
            params.max_tokens,
            params.is_public,
            now,
            params.id
        )
        .fetch_one(&*self.db)
        .await?;

        Ok(role)
    }

    pub async fn delete_chat_role(&self, id: i64) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            DELETE FROM ai_chat_roles WHERE id = $1
            "#,
            id
        )
        .execute(&*self.db)
        .await?;

        Ok(())
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
