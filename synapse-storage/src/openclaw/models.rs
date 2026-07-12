use serde::{Deserialize, Serialize};
use sqlx::FromRow;

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
