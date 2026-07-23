//! AI storage domain group.
//!
//! Re-exports AI-related storage modules (`ai_connection`, `openclaw`) under a
//! single namespace. Feature-gated behind `openclaw-routes`.
//!
//! Consumers should prefer `synapse_storage::ai::OpenClawStorage` over the
//! flat `synapse_storage::OpenClawStorage`.

pub use crate::ai_connection::{AiConnection, AiConnectionStorage, AiConnectionStoreApi};
pub use crate::openclaw::{
    decode_conversation_cursor, decode_generation_cursor, decode_message_cursor, encode_conversation_cursor,
    encode_generation_cursor, encode_message_cursor, AiChatRole, AiConversation, AiGeneration, AiMessage,
    ConversationCursor, CreateChatRoleParams, CreateConnectionParams, CreateConversationParams, GenerationCursor,
    MessageCursor, OpenClawConnection, OpenClawStorage, OpenClawStoreApi, UpdateChatRoleParams, UpdateConnectionParams,
};
