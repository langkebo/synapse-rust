use async_trait::async_trait;

use super::models::*;
use super::repository::OpenClawStorage;

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
