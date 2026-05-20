use crate::common::ApiError;
use crate::storage::openclaw::{
    decode_conversation_cursor, decode_generation_cursor, AiChatRole, AiConversation,
    AiGeneration, AiMessage, OpenClawConnection, OpenClawStorage,
};
use sha2::{Digest, Sha256};
use std::net::IpAddr;
use std::sync::Arc;
use url::Url;

/// Business logic service for the OpenClaw AI integration.
///
/// Encapsulates all OpenClaw domain logic (encryption, SSRF validation,
/// ownership checks, health probing) so that the route layer remains a
/// thin HTTP adapter.
pub struct OpenClawService {
    storage: Arc<OpenClawStorage>,
    api_key_encryption_key: Option<[u8; 32]>,
}

impl OpenClawService {
    pub fn new(storage: Arc<OpenClawStorage>, api_key_encryption_key: Option<[u8; 32]>) -> Self {
        Self {
            storage,
            api_key_encryption_key,
        }
    }

    /// Resolve the API key encryption key from environment / config.
    pub fn resolve_encryption_key(
        macaroon_secret_key: Option<&str>,
        security_secret: &str,
    ) -> Option<[u8; 32]> {
        let explicit = std::env::var("API_KEY_ENCRYPTION_KEY").ok();
        let config_secret = macaroon_secret_key
            .map(|s| s.to_string())
            .or_else(|| Some(security_secret.to_string()))
            .filter(|value| !value.trim().is_empty());

        explicit
            .filter(|value| !value.trim().is_empty())
            .or(config_secret)
            .map(|secret| derive_api_key_encryption_key(secret.trim()))
    }

    // -----------------------------------------------------------------------
    // Auth / ownership helpers
    // -----------------------------------------------------------------------

    pub fn ensure_user_allowed(&self, is_guest: bool) -> Result<(), ApiError> {
        if is_guest {
            return Err(ApiError::forbidden(
                "Guest access to OpenClaw routes is disabled",
            ));
        }
        Ok(())
    }

    pub fn ensure_resource_owner(
        &self,
        owner_user_id: &str,
        auth_user_id: &str,
        not_found_message: &'static str,
    ) -> Result<(), ApiError> {
        if owner_user_id != auth_user_id {
            return Err(ApiError::not_found(not_found_message));
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Validation helpers
    // -----------------------------------------------------------------------

    /// SSRF protection: reject localhost / private IPs / non-HTTP schemes.
    pub fn validate_base_url(&self, base_url: &str) -> Result<(), ApiError> {
        let url = Url::parse(base_url)
            .map_err(|e| ApiError::bad_request(format!("Invalid base_url: {}", e)))?;

        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(ApiError::bad_request(
                "OpenClaw base_url must use http or https".to_string(),
            ));
        }

        let host = url.host_str().ok_or_else(|| {
            ApiError::bad_request("OpenClaw base_url must include a host".to_string())
        })?;

        let is_forbidden_host = host.eq_ignore_ascii_case("localhost")
            || host.eq_ignore_ascii_case("localhost.")
            || host.ends_with(".localhost");

        if is_forbidden_host {
            return Err(ApiError::bad_request(
                "OpenClaw base_url cannot target localhost".to_string(),
            ));
        }

        if let Ok(ip) = host.parse::<IpAddr>() {
            let forbidden_ip = match ip {
                IpAddr::V4(ip) => {
                    ip.is_private()
                        || ip.is_loopback()
                        || ip.is_link_local()
                        || ip.is_broadcast()
                        || ip.is_multicast()
                        || ip.is_unspecified()
                }
                IpAddr::V6(ip) => {
                    ip.is_loopback()
                        || ip.is_multicast()
                        || ip.is_unspecified()
                        || ip.is_unique_local()
                        || ip.is_unicast_link_local()
                }
            };

            if forbidden_ip {
                return Err(ApiError::bad_request(
                    "OpenClaw base_url cannot target local or private IP ranges".to_string(),
                ));
            }
        }

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Encryption helpers
    // -----------------------------------------------------------------------

    pub fn encrypt_optional_api_key(
        &self,
        api_key: Option<String>,
    ) -> Result<Option<String>, ApiError> {
        match api_key {
            Some(api_key) => {
                let key = self.api_key_encryption_key.ok_or_else(|| {
                    ApiError::internal(
                        "OpenClaw API key encryption is not configured".to_string(),
                    )
                })?;
                Ok(Some(encrypt_api_key(&api_key, &key)?))
            }
            None => Ok(None),
        }
    }

    // -----------------------------------------------------------------------
    // Health check
    // -----------------------------------------------------------------------

    pub async fn test_connection_health(&self, base_url: &str) -> bool {
        use reqwest::Client;
        use std::time::Duration;

        let client = match Client::builder().timeout(Duration::from_secs(5)).build() {
            Ok(c) => c,
            Err(_) => return false,
        };

        let health_url = format!("{}/health", base_url.trim_end_matches('/'));

        match client.get(&health_url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    // -----------------------------------------------------------------------
    // Connection CRUD
    // -----------------------------------------------------------------------

    pub async fn list_connections(
        &self,
        user_id: &str,
    ) -> Result<Vec<OpenClawConnection>, ApiError> {
        self.storage
            .get_user_connections(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get connections: {}", e)))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_connection(
        &self,
        user_id: &str,
        name: &str,
        provider: &str,
        base_url: &str,
        api_key: Option<String>,
        config: Option<serde_json::Value>,
        is_default: bool,
    ) -> Result<OpenClawConnection, ApiError> {
        self.validate_base_url(base_url)?;
        let encrypted_key = self.encrypt_optional_api_key(api_key)?;

        self.storage
            .create_connection(crate::storage::openclaw::CreateConnectionParams {
                user_id,
                name,
                provider,
                base_url,
                encrypted_api_key: encrypted_key.as_deref(),
                config,
                is_default,
            })
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create connection: {}", e)))
    }

    pub async fn get_connection_for_user(
        &self,
        id: i64,
        auth_user_id: &str,
    ) -> Result<OpenClawConnection, ApiError> {
        let conn = self
            .storage
            .get_connection(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get connection: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Connection not found"))?;

        self.ensure_resource_owner(&conn.user_id, auth_user_id, "Connection not found")?;
        Ok(conn)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_connection(
        &self,
        id: i64,
        auth_user_id: &str,
        name: Option<String>,
        base_url: Option<String>,
        api_key: Option<String>,
        config: Option<serde_json::Value>,
        is_default: Option<bool>,
        is_active: Option<bool>,
    ) -> Result<OpenClawConnection, ApiError> {
        if let Some(ref base_url) = base_url {
            self.validate_base_url(base_url)?;
        }

        // Ownership check
        let _ = self.get_connection_for_user(id, auth_user_id).await?;

        let encrypted_key = self.encrypt_optional_api_key(api_key)?;

        self.storage
            .update_connection(crate::storage::openclaw::UpdateConnectionParams {
                id,
                name: name.as_deref(),
                base_url: base_url.as_deref(),
                encrypted_api_key: encrypted_key.as_deref(),
                config,
                is_default,
                is_active,
            })
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update connection: {}", e)))
    }

    pub async fn delete_connection(
        &self,
        id: i64,
        auth_user_id: &str,
    ) -> Result<(), ApiError> {
        // Ownership check
        let _ = self.get_connection_for_user(id, auth_user_id).await?;

        self.storage
            .delete_connection(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete connection: {}", e)))
    }

    pub async fn test_connection(
        &self,
        id: i64,
        auth_user_id: &str,
    ) -> Result<(OpenClawConnection, bool, i64), ApiError> {
        let conn = self.get_connection_for_user(id, auth_user_id).await?;
        self.validate_base_url(&conn.base_url)?;

        let start = std::time::Instant::now();
        let is_healthy = self.test_connection_health(&conn.base_url).await;
        let latency_ms = start.elapsed().as_millis() as i64;

        Ok((conn, is_healthy, latency_ms))
    }

    // -----------------------------------------------------------------------
    // Conversation CRUD
    // -----------------------------------------------------------------------

    pub async fn list_conversations(
        &self,
        user_id: &str,
        limit: i64,
        from: Option<String>,
    ) -> Result<(Vec<AiConversation>, Option<String>), ApiError> {
        let cursor = decode_conversation_cursor(from.as_deref());
        if from.is_some() && cursor.is_none() {
            return Err(ApiError::bad_request("Invalid from cursor"));
        }

        self.storage
            .get_user_conversations(user_id, limit, cursor)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get conversations: {}", e)))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_conversation(
        &self,
        user_id: &str,
        connection_id: Option<i64>,
        title: Option<&str>,
        model_id: Option<&str>,
        system_prompt: Option<&str>,
        temperature: Option<f32>,
        max_tokens: Option<i32>,
    ) -> Result<AiConversation, ApiError> {
        // Validate connection ownership if specified
        if let Some(conn_id) = connection_id {
            let _ = self.get_connection_for_user(conn_id, user_id).await?;
        }

        self.storage
            .create_conversation(crate::storage::openclaw::CreateConversationParams {
                user_id,
                connection_id,
                title,
                model_id,
                system_prompt,
                temperature,
                max_tokens,
            })
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create conversation: {}", e)))
    }

    pub async fn get_conversation_for_user(
        &self,
        id: i64,
        auth_user_id: &str,
    ) -> Result<AiConversation, ApiError> {
        let conv = self
            .storage
            .get_conversation(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get conversation: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Conversation not found"))?;

        self.ensure_resource_owner(&conv.user_id, auth_user_id, "Conversation not found")?;
        Ok(conv)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_conversation(
        &self,
        id: i64,
        auth_user_id: &str,
        title: Option<&str>,
        system_prompt: Option<&str>,
        temperature: Option<f32>,
        max_tokens: Option<i32>,
        is_pinned: Option<bool>,
    ) -> Result<AiConversation, ApiError> {
        // Ownership check
        let _ = self.get_conversation_for_user(id, auth_user_id).await?;

        self.storage
            .update_conversation(id, title, system_prompt, temperature, max_tokens, is_pinned)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update conversation: {}", e)))
    }

    pub async fn delete_conversation(
        &self,
        id: i64,
        auth_user_id: &str,
    ) -> Result<(), ApiError> {
        // Ownership check
        let _ = self.get_conversation_for_user(id, auth_user_id).await?;

        self.storage
            .delete_conversation(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete conversation: {}", e)))
    }

    // -----------------------------------------------------------------------
    // Message CRUD
    // -----------------------------------------------------------------------

    pub async fn list_messages(
        &self,
        conversation_id: i64,
        auth_user_id: &str,
        limit: i64,
        before: Option<i64>,
    ) -> Result<Vec<AiMessage>, ApiError> {
        // Ownership check via conversation
        let _ = self
            .get_conversation_for_user(conversation_id, auth_user_id)
            .await?;

        self.storage
            .get_conversation_messages(conversation_id, limit, before)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get messages: {}", e)))
    }

    pub async fn send_message(
        &self,
        conversation_id: i64,
        auth_user_id: &str,
        content: &str,
        role: Option<&str>,
        tool_calls: Option<serde_json::Value>,
        tool_call_id: Option<&str>,
    ) -> Result<AiMessage, ApiError> {
        // Ownership check via conversation
        let _ = self
            .get_conversation_for_user(conversation_id, auth_user_id)
            .await?;

        let role = role.unwrap_or("user");

        self.storage
            .create_message(conversation_id, role, content, None, tool_calls, tool_call_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create message: {}", e)))
    }

    pub async fn delete_message(
        &self,
        id: i64,
        auth_user_id: &str,
    ) -> Result<(), ApiError> {
        let msg = self
            .storage
            .get_message(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get message: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Message not found"))?;

        // Ownership check via conversation
        let _ = self
            .get_conversation_for_user(msg.conversation_id, auth_user_id)
            .await?;

        self.storage
            .delete_message(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete message: {}", e)))
    }

    // -----------------------------------------------------------------------
    // Generation CRUD
    // -----------------------------------------------------------------------

    pub async fn list_generations(
        &self,
        user_id: &str,
        gen_type: Option<&str>,
        limit: i64,
        from: Option<String>,
    ) -> Result<(Vec<AiGeneration>, Option<String>), ApiError> {
        let cursor = decode_generation_cursor(from.as_deref());
        if from.is_some() && cursor.is_none() {
            return Err(ApiError::bad_request("Invalid from cursor"));
        }

        self.storage
            .get_user_generations(user_id, gen_type, limit, cursor)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get generations: {}", e)))
    }

    pub async fn create_generation(
        &self,
        user_id: &str,
        conversation_id: Option<i64>,
        gen_type: &str,
        prompt: &str,
    ) -> Result<AiGeneration, ApiError> {
        // Validate conversation ownership if specified
        if let Some(conv_id) = conversation_id {
            let _ = self.get_conversation_for_user(conv_id, user_id).await?;
        }

        self.storage
            .create_generation(user_id, conversation_id, gen_type, prompt)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create generation: {}", e)))
    }

    pub async fn get_generation_for_user(
        &self,
        id: i64,
        auth_user_id: &str,
    ) -> Result<AiGeneration, ApiError> {
        let gen = self
            .storage
            .get_generation(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get generation: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Generation not found"))?;

        self.ensure_resource_owner(&gen.user_id, auth_user_id, "Generation not found")?;
        Ok(gen)
    }

    pub async fn delete_generation(
        &self,
        id: i64,
        auth_user_id: &str,
    ) -> Result<(), ApiError> {
        // Ownership check
        let _ = self.get_generation_for_user(id, auth_user_id).await?;

        self.storage
            .delete_generation(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete generation: {}", e)))
    }

    // -----------------------------------------------------------------------
    // Chat Role CRUD
    // -----------------------------------------------------------------------

    pub async fn list_chat_roles(
        &self,
        user_id: &str,
    ) -> Result<Vec<AiChatRole>, ApiError> {
        self.storage
            .get_user_chat_roles(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get chat roles: {}", e)))
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn create_chat_role(
        &self,
        user_id: &str,
        name: &str,
        description: Option<&str>,
        system_message: &str,
        model_id: Option<&str>,
        avatar_url: Option<&str>,
        category: Option<&str>,
        temperature: Option<f32>,
        max_tokens: Option<i32>,
        is_public: bool,
    ) -> Result<AiChatRole, ApiError> {
        self.storage
            .create_chat_role(crate::storage::openclaw::CreateChatRoleParams {
                user_id,
                name,
                description,
                system_message,
                model_id,
                avatar_url,
                category,
                temperature,
                max_tokens,
                is_public,
            })
            .await
            .map_err(|e| ApiError::internal(format!("Failed to create chat role: {}", e)))
    }

    pub async fn get_chat_role_for_user(
        &self,
        id: i64,
        auth_user_id: &str,
    ) -> Result<AiChatRole, ApiError> {
        let role = self
            .storage
            .get_chat_role(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get chat role: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Chat role not found"))?;

        // Public roles are visible to everyone; private roles require ownership
        if !role.is_public {
            self.ensure_resource_owner(&role.user_id, auth_user_id, "Chat role not found")?;
        }
        Ok(role)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_chat_role(
        &self,
        id: i64,
        auth_user_id: &str,
        name: Option<&str>,
        description: Option<&str>,
        system_message: Option<&str>,
        model_id: Option<&str>,
        avatar_url: Option<&str>,
        category: Option<&str>,
        temperature: Option<f32>,
        max_tokens: Option<i32>,
        is_public: Option<bool>,
    ) -> Result<AiChatRole, ApiError> {
        // Ownership check — must be owner to update (even public roles)
        let existing = self
            .storage
            .get_chat_role(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get chat role: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Chat role not found"))?;

        self.ensure_resource_owner(&existing.user_id, auth_user_id, "Chat role not found")?;

        self.storage
            .update_chat_role(crate::storage::openclaw::UpdateChatRoleParams {
                id,
                name,
                description,
                system_message,
                model_id,
                avatar_url,
                category,
                temperature,
                max_tokens,
                is_public,
            })
            .await
            .map_err(|e| ApiError::internal(format!("Failed to update chat role: {}", e)))
    }

    pub async fn delete_chat_role(
        &self,
        id: i64,
        auth_user_id: &str,
    ) -> Result<(), ApiError> {
        // Ownership check
        let existing = self
            .storage
            .get_chat_role(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get chat role: {}", e)))?
            .ok_or_else(|| ApiError::not_found("Chat role not found"))?;

        self.ensure_resource_owner(&existing.user_id, auth_user_id, "Chat role not found")?;

        self.storage
            .delete_chat_role(id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to delete chat role: {}", e)))
    }
}

// ---------------------------------------------------------------------------
// Private encryption helpers
// ---------------------------------------------------------------------------

fn derive_api_key_encryption_key(secret: &str) -> [u8; 32] {
    let digest = Sha256::digest(secret.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(&digest);
    key
}

fn encrypt_api_key(key: &str, encryption_key: &[u8; 32]) -> Result<String, ApiError> {
    use aes_gcm::aead::Aead;
    use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
    use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
    use rand::RngCore;

    let cipher = Aes256Gcm::new_from_slice(encryption_key)
        .map_err(|_| ApiError::internal("Invalid encryption key length".to_string()))?;
    let mut nonce_bytes = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext = cipher
        .encrypt(nonce, key.as_bytes())
        .map_err(|e| ApiError::internal(format!("Encryption failed: {}", e)))?;

    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);

    Ok(BASE64_STANDARD.encode(&combined))
}

#[cfg(test)]
mod tests {
    use super::derive_api_key_encryption_key;

    #[test]
    fn test_derive_api_key_encryption_key_is_deterministic_and_hashed() {
        let first = derive_api_key_encryption_key("short-secret");
        let second = derive_api_key_encryption_key("short-secret");
        let different = derive_api_key_encryption_key("different-secret");

        assert_eq!(first, second);
        assert_ne!(first, different);
        assert_ne!(&first[..12], b"short-secret");
    }
}
