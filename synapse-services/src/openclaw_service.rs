use sha2::{Digest, Sha256};
use std::net::IpAddr;
use std::sync::Arc;
use synapse_common::ApiError;
use synapse_storage::openclaw::{
    decode_conversation_cursor, decode_generation_cursor, decode_message_cursor, AiChatRole, AiConversation,
    AiGeneration, AiMessage, MessageCursor, OpenClawConnection, OpenClawStoreApi,
};
use url::Url;

/// Business logic service for the OpenClaw AI integration.
///
/// Encapsulates all OpenClaw domain logic (encryption, SSRF validation,
/// ownership checks, health probing) so that the route layer remains a
/// thin HTTP adapter.
pub struct OpenClawService {
    storage: Arc<dyn OpenClawStoreApi>,
    api_key_encryption_key: Option<[u8; 32]>,
}

impl OpenClawService {
    pub fn new(storage: Arc<dyn OpenClawStoreApi>, api_key_encryption_key: Option<[u8; 32]>) -> Self {
        Self { storage, api_key_encryption_key }
    }

    /// Resolve the API key encryption key from environment / config.
    pub fn resolve_encryption_key(macaroon_secret_key: Option<&str>, security_secret: &str) -> Option<[u8; 32]> {
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
            return Err(ApiError::forbidden("Guest access to OpenClaw routes is disabled"));
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
        let url = Url::parse(base_url).map_err(|e| ApiError::bad_request(format!("Invalid base_url: {}", e)))?;

        if url.scheme() != "http" && url.scheme() != "https" {
            return Err(ApiError::bad_request("OpenClaw base_url must use http or https".to_string()));
        }

        let host =
            url.host_str().ok_or_else(|| ApiError::bad_request("OpenClaw base_url must include a host".to_string()))?;

        let is_forbidden_host = host.eq_ignore_ascii_case("localhost")
            || host.eq_ignore_ascii_case("localhost.")
            || host.ends_with(".localhost");

        if is_forbidden_host {
            return Err(ApiError::bad_request("OpenClaw base_url cannot target localhost".to_string()));
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

    pub fn encrypt_optional_api_key(&self, api_key: Option<String>) -> Result<Option<String>, ApiError> {
        match api_key {
            Some(api_key) => {
                let key = self
                    .api_key_encryption_key
                    .ok_or_else(|| ApiError::internal("OpenClaw API key encryption is not configured".to_string()))?;
                Ok(Some(encrypt_api_key(&api_key, &key)?))
            }
            None => Ok(None),
        }
    }

    // -----------------------------------------------------------------------
    // Health check
    // -----------------------------------------------------------------------

    #[::tracing::instrument(skip_all, fields(base_url = %base_url))]
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

    #[::tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn list_connections(&self, user_id: &str) -> Result<Vec<OpenClawConnection>, ApiError> {
        self.storage
            .get_user_connections(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get connections", &e))
    }

    #[::tracing::instrument(
        skip_all,
        fields(
            user_id = %user_id,
            name = %name,
            provider = %provider,
            base_url = %base_url,
            has_api_key = api_key.is_some(),
            has_config = config.is_some(),
            is_default = is_default
        )
    )]
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
            .create_connection(synapse_storage::openclaw::CreateConnectionParams {
                user_id,
                name,
                provider,
                base_url,
                encrypted_api_key: encrypted_key.as_deref(),
                config,
                is_default,
            })
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create connection", &e))
    }

    #[::tracing::instrument(skip_all, fields(id = id, auth_user_id = %auth_user_id))]
    pub async fn get_connection_for_user(&self, id: i64, auth_user_id: &str) -> Result<OpenClawConnection, ApiError> {
        let conn = self
            .storage
            .get_connection(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get connection", &e))?
            .ok_or_else(|| ApiError::not_found("Connection not found"))?;

        self.ensure_resource_owner(&conn.user_id, auth_user_id, "Connection not found")?;
        Ok(conn)
    }

    #[::tracing::instrument(
        skip_all,
        fields(
            id = id,
            auth_user_id = %auth_user_id,
            has_name = name.is_some(),
            has_base_url = base_url.is_some(),
            has_api_key = api_key.is_some(),
            has_config = config.is_some(),
            has_is_default = is_default.is_some(),
            has_is_active = is_active.is_some()
        )
    )]
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
            .update_connection(synapse_storage::openclaw::UpdateConnectionParams {
                id,
                name: name.as_deref(),
                base_url: base_url.as_deref(),
                encrypted_api_key: encrypted_key.as_deref(),
                config,
                is_default,
                is_active,
            })
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to update connection", &e))
    }

    #[::tracing::instrument(skip_all, fields(id = id, auth_user_id = %auth_user_id))]
    pub async fn delete_connection(&self, id: i64, auth_user_id: &str) -> Result<(), ApiError> {
        // Ownership check
        let _ = self.get_connection_for_user(id, auth_user_id).await?;

        self.storage
            .delete_connection(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete connection", &e))
    }

    #[::tracing::instrument(skip_all, fields(id = id, auth_user_id = %auth_user_id))]
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

    #[::tracing::instrument(skip_all, fields(user_id = %user_id, limit = limit, has_from = from.is_some()))]
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
            .map_err(|e| ApiError::internal_with_log("Failed to get conversations", &e))
    }

    #[::tracing::instrument(
        skip_all,
        fields(
            user_id = %user_id,
            has_connection_id = connection_id.is_some(),
            has_title = title.is_some(),
            has_model_id = model_id.is_some(),
            has_system_prompt = system_prompt.is_some(),
            has_temperature = temperature.is_some(),
            has_max_tokens = max_tokens.is_some()
        )
    )]
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
            .create_conversation(synapse_storage::openclaw::CreateConversationParams {
                user_id,
                connection_id,
                title,
                model_id,
                system_prompt,
                temperature,
                max_tokens,
            })
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create conversation", &e))
    }

    #[::tracing::instrument(skip_all, fields(id = id, auth_user_id = %auth_user_id))]
    pub async fn get_conversation_for_user(&self, id: i64, auth_user_id: &str) -> Result<AiConversation, ApiError> {
        let conv = self
            .storage
            .get_conversation(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get conversation", &e))?
            .ok_or_else(|| ApiError::not_found("Conversation not found"))?;

        self.ensure_resource_owner(&conv.user_id, auth_user_id, "Conversation not found")?;
        Ok(conv)
    }

    #[::tracing::instrument(
        skip_all,
        fields(
            id = id,
            auth_user_id = %auth_user_id,
            has_title = title.is_some(),
            has_system_prompt = system_prompt.is_some(),
            has_temperature = temperature.is_some(),
            has_max_tokens = max_tokens.is_some(),
            has_is_pinned = is_pinned.is_some()
        )
    )]
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
            .map_err(|e| ApiError::internal_with_log("Failed to update conversation", &e))
    }

    #[::tracing::instrument(skip_all, fields(id = id, auth_user_id = %auth_user_id))]
    pub async fn delete_conversation(&self, id: i64, auth_user_id: &str) -> Result<(), ApiError> {
        // Ownership check
        let _ = self.get_conversation_for_user(id, auth_user_id).await?;

        self.storage
            .delete_conversation(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete conversation", &e))
    }

    // -----------------------------------------------------------------------
    // Message CRUD
    // -----------------------------------------------------------------------

    #[::tracing::instrument(
        skip_all,
        fields(
            conversation_id = conversation_id,
            auth_user_id = %auth_user_id,
            limit = limit,
            has_from = from.is_some(),
            has_before = before.is_some()
        )
    )]
    pub async fn list_messages(
        &self,
        conversation_id: i64,
        auth_user_id: &str,
        limit: i64,
        from: Option<String>,
        before: Option<i64>,
    ) -> Result<(Vec<AiMessage>, Option<String>), ApiError> {
        // Ownership check via conversation
        let _ = self.get_conversation_for_user(conversation_id, auth_user_id).await?;

        let cursor = match (from, before) {
            (Some(from), _) => {
                Some(decode_message_cursor(Some(&from)).ok_or_else(|| ApiError::bad_request("Invalid from cursor"))?)
            }
            (None, Some(before_id)) => {
                let message = self
                    .storage
                    .get_message(before_id)
                    .await
                    .map_err(|e| ApiError::internal_with_log("Failed to resolve legacy before cursor", &e))?
                    .ok_or_else(|| ApiError::not_found("Message not found"))?;

                if message.conversation_id != conversation_id {
                    return Err(ApiError::bad_request("Legacy before cursor does not belong to this conversation"));
                }

                Some(MessageCursor { created_ts: message.created_ts, id: message.id })
            }
            (None, None) => None,
        };

        self.storage
            .get_conversation_messages(conversation_id, limit, cursor)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get messages", &e))
    }

    #[::tracing::instrument(
        skip_all,
        fields(
            conversation_id = conversation_id,
            auth_user_id = %auth_user_id,
            content_len = content.len(),
            has_role = role.is_some(),
            has_tool_calls = tool_calls.is_some(),
            has_tool_call_id = tool_call_id.is_some()
        )
    )]
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
        let _ = self.get_conversation_for_user(conversation_id, auth_user_id).await?;

        let role = role.unwrap_or("user");

        self.storage
            .create_message(conversation_id, role, content, None, tool_calls, tool_call_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to create message", &e))
    }

    #[::tracing::instrument(skip_all, fields(id = id, auth_user_id = %auth_user_id))]
    pub async fn delete_message(&self, id: i64, auth_user_id: &str) -> Result<(), ApiError> {
        let msg = self
            .storage
            .get_message(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get message", &e))?
            .ok_or_else(|| ApiError::not_found("Message not found"))?;

        // Ownership check via conversation
        let _ = self.get_conversation_for_user(msg.conversation_id, auth_user_id).await?;

        self.storage.delete_message(id).await.map_err(|e| ApiError::internal_with_log("Failed to delete message", &e))
    }

    // -----------------------------------------------------------------------
    // Generation CRUD
    // -----------------------------------------------------------------------

    #[::tracing::instrument(
        skip_all,
        fields(
            user_id = %user_id,
            has_gen_type = gen_type.is_some(),
            limit = limit,
            has_from = from.is_some()
        )
    )]
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
            .map_err(|e| ApiError::internal_with_log("Failed to get generations", &e))
    }

    #[::tracing::instrument(
        skip_all,
        fields(
            user_id = %user_id,
            has_conversation_id = conversation_id.is_some(),
            gen_type = %gen_type,
            prompt_len = prompt.len()
        )
    )]
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
            .map_err(|e| ApiError::internal_with_log("Failed to create generation", &e))
    }

    #[::tracing::instrument(skip_all, fields(id = id, auth_user_id = %auth_user_id))]
    pub async fn get_generation_for_user(&self, id: i64, auth_user_id: &str) -> Result<AiGeneration, ApiError> {
        let gen = self
            .storage
            .get_generation(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get generation", &e))?
            .ok_or_else(|| ApiError::not_found("Generation not found"))?;

        self.ensure_resource_owner(&gen.user_id, auth_user_id, "Generation not found")?;
        Ok(gen)
    }

    #[::tracing::instrument(skip_all, fields(id = id, auth_user_id = %auth_user_id))]
    pub async fn delete_generation(&self, id: i64, auth_user_id: &str) -> Result<(), ApiError> {
        // Ownership check
        let _ = self.get_generation_for_user(id, auth_user_id).await?;

        self.storage
            .delete_generation(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete generation", &e))
    }

    // -----------------------------------------------------------------------
    // Chat Role CRUD
    // -----------------------------------------------------------------------

    #[::tracing::instrument(skip_all, fields(user_id = %user_id))]
    pub async fn list_chat_roles(&self, user_id: &str) -> Result<Vec<AiChatRole>, ApiError> {
        self.storage
            .get_user_chat_roles(user_id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get chat roles", &e))
    }

    #[::tracing::instrument(
        skip_all,
        fields(
            user_id = %user_id,
            name = %name,
            has_description = description.is_some(),
            has_model_id = model_id.is_some(),
            has_avatar_url = avatar_url.is_some(),
            has_category = category.is_some(),
            has_temperature = temperature.is_some(),
            has_max_tokens = max_tokens.is_some(),
            is_public = is_public
        )
    )]
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
            .create_chat_role(synapse_storage::openclaw::CreateChatRoleParams {
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
            .map_err(|e| ApiError::internal_with_log("Failed to create chat role", &e))
    }

    #[::tracing::instrument(skip_all, fields(id = id, auth_user_id = %auth_user_id))]
    pub async fn get_chat_role_for_user(&self, id: i64, auth_user_id: &str) -> Result<AiChatRole, ApiError> {
        let role = self
            .storage
            .get_chat_role(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get chat role", &e))?
            .ok_or_else(|| ApiError::not_found("Chat role not found"))?;

        // Public roles are visible to everyone; private roles require ownership
        if !role.is_public {
            self.ensure_resource_owner(&role.user_id, auth_user_id, "Chat role not found")?;
        }
        Ok(role)
    }

    #[::tracing::instrument(
        skip_all,
        fields(
            id = id,
            auth_user_id = %auth_user_id,
            has_name = name.is_some(),
            has_description = description.is_some(),
            has_system_message = system_message.is_some(),
            has_model_id = model_id.is_some(),
            has_avatar_url = avatar_url.is_some(),
            has_category = category.is_some(),
            has_temperature = temperature.is_some(),
            has_max_tokens = max_tokens.is_some(),
            has_is_public = is_public.is_some()
        )
    )]
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
            .map_err(|e| ApiError::internal_with_log("Failed to get chat role", &e))?
            .ok_or_else(|| ApiError::not_found("Chat role not found"))?;

        self.ensure_resource_owner(&existing.user_id, auth_user_id, "Chat role not found")?;

        self.storage
            .update_chat_role(synapse_storage::openclaw::UpdateChatRoleParams {
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
            .map_err(|e| ApiError::internal_with_log("Failed to update chat role", &e))
    }

    #[::tracing::instrument(skip_all, fields(id = id, auth_user_id = %auth_user_id))]
    pub async fn delete_chat_role(&self, id: i64, auth_user_id: &str) -> Result<(), ApiError> {
        // Ownership check
        let existing = self
            .storage
            .get_chat_role(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to get chat role", &e))?
            .ok_or_else(|| ApiError::not_found("Chat role not found"))?;

        self.ensure_resource_owner(&existing.user_id, auth_user_id, "Chat role not found")?;

        self.storage
            .delete_chat_role(id)
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to delete chat role", &e))
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
        .map_err(|e| ApiError::internal_with_log("Invalid encryption key length", &e))?;
    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let ciphertext =
        cipher.encrypt(nonce, key.as_bytes()).map_err(|e| ApiError::internal_with_log("Encryption failed", &e))?;

    let mut combined = nonce_bytes.to_vec();
    combined.extend_from_slice(&ciphertext);

    Ok(BASE64_STANDARD.encode(&combined))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_api_key_encryption_key_is_deterministic_and_hashed() {
        let first = derive_api_key_encryption_key("short-secret");
        let second = derive_api_key_encryption_key("short-secret");
        let different = derive_api_key_encryption_key("different-secret");

        assert_eq!(first, second);
        assert_ne!(first, different);
        assert_ne!(&first[..12], b"short-secret");
    }

    // -----------------------------------------------------------------------
    // ensure_user_allowed
    // -----------------------------------------------------------------------

    #[test]
    fn test_ensure_user_allowed_when_not_guest() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        assert!(service.ensure_user_allowed(false).is_ok());
    }

    #[test]
    fn test_ensure_user_allowed_when_guest() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        let result = service.ensure_user_allowed(true);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Guest access"));
    }

    // -----------------------------------------------------------------------
    // ensure_resource_owner
    // -----------------------------------------------------------------------

    #[test]
    fn test_ensure_resource_owner_when_same_user() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        assert!(service.ensure_resource_owner("@alice:example.com", "@alice:example.com", "Not found").is_ok());
    }

    #[test]
    fn test_ensure_resource_owner_when_different_user() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        let result = service.ensure_resource_owner("@alice:example.com", "@bob:example.com", "Not found");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Not found"));
    }

    // -----------------------------------------------------------------------
    // validate_base_url
    // -----------------------------------------------------------------------

    #[test]
    fn test_validate_base_url_accepts_https() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        assert!(service.validate_base_url("https://api.example.com").is_ok());
    }

    #[test]
    fn test_validate_base_url_accepts_http() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        assert!(service.validate_base_url("http://api.example.com").is_ok());
    }

    #[test]
    fn test_validate_base_url_rejects_localhost() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        let result = service.validate_base_url("https://localhost:8080");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("localhost"));
    }

    #[test]
    fn test_validate_base_url_rejects_localhost_subdomain() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        let result = service.validate_base_url("https://app.localhost");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("localhost"));
    }

    #[test]
    fn test_validate_base_url_rejects_private_ip() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        let result = service.validate_base_url("https://192.168.1.1");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("private"));
    }

    #[test]
    fn test_validate_base_url_rejects_loopback_ip() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        let result = service.validate_base_url("https://127.0.0.1");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("private"));
    }

    #[test]
    fn test_validate_base_url_rejects_ftp_scheme() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        let result = service.validate_base_url("ftp://api.example.com");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("http or https"));
    }

    #[test]
    fn test_validate_base_url_rejects_invalid_url() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        let result = service.validate_base_url("not-a-url");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid"));
    }

    #[test]
    fn test_validate_base_url_rejects_ipv6_loopback() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        let result = service.validate_base_url("https://[::1]");
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // encrypt_optional_api_key
    // -----------------------------------------------------------------------

    #[test]
    fn test_encrypt_optional_api_key_with_none() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        let result = service.encrypt_optional_api_key(None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn test_encrypt_optional_api_key_without_encryption_key() {
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), None);
        let result = service.encrypt_optional_api_key(Some("my-api-key".to_string()));
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not configured"));
    }

    #[test]
    fn test_encrypt_optional_api_key_with_encryption_key() {
        let key = derive_api_key_encryption_key("test-secret-12345678901234567890");
        let service = OpenClawService::new(Arc::new(FakeOpenClawStore), Some(key));
        let result = service.encrypt_optional_api_key(Some("my-api-key".to_string()));
        assert!(result.is_ok());
        let encrypted = result.unwrap();
        assert!(encrypted.is_some());
        // Encrypted value should be base64 and different from original
        let encrypted_str = encrypted.unwrap();
        assert_ne!(encrypted_str, "my-api-key");
        // Should be valid base64 (no padding issues)
        assert!(!encrypted_str.is_empty());
    }

    // -----------------------------------------------------------------------
    // resolve_encryption_key
    // -----------------------------------------------------------------------

    #[test]
    fn test_resolve_encryption_key_from_macaroon_secret() {
        let key = OpenClawService::resolve_encryption_key(Some("my-secret-key"), "fallback");
        assert!(key.is_some());
        // Same secret should produce same key
        let key2 = OpenClawService::resolve_encryption_key(Some("my-secret-key"), "fallback");
        assert_eq!(key.unwrap(), key2.unwrap());
    }

    #[test]
    fn test_resolve_encryption_key_from_security_secret() {
        let key = OpenClawService::resolve_encryption_key(None, "security-secret");
        assert!(key.is_some());
    }

    #[test]
    fn test_resolve_encryption_key_empty_secret_falls_back() {
        // Empty macaroon_secret falls back to security_secret
        let key = OpenClawService::resolve_encryption_key(Some(""), "fallback-secret");
        assert!(key.is_some());
    }

    // Fake mock storage for tests that need it
    struct FakeOpenClawStore;

    #[async_trait::async_trait]
    impl OpenClawStoreApi for FakeOpenClawStore {
        async fn create_connection(&self, _params: synapse_storage::openclaw::CreateConnectionParams<'_>) -> Result<synapse_storage::openclaw::OpenClawConnection, sqlx::Error> {
            unimplemented!()
        }
        async fn get_connection(&self, _id: i64) -> Result<Option<synapse_storage::openclaw::OpenClawConnection>, sqlx::Error> {
            unimplemented!()
        }
        async fn get_user_connections(&self, _user_id: &str) -> Result<Vec<synapse_storage::openclaw::OpenClawConnection>, sqlx::Error> {
            unimplemented!()
        }
        async fn get_default_connection(&self, _user_id: &str) -> Result<Option<synapse_storage::openclaw::OpenClawConnection>, sqlx::Error> {
            unimplemented!()
        }
        async fn update_connection(&self, _params: synapse_storage::openclaw::UpdateConnectionParams<'_>) -> Result<synapse_storage::openclaw::OpenClawConnection, sqlx::Error> {
            unimplemented!()
        }
        async fn delete_connection(&self, _id: i64) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn create_conversation(&self, _params: synapse_storage::openclaw::CreateConversationParams<'_>) -> Result<synapse_storage::openclaw::AiConversation, sqlx::Error> {
            unimplemented!()
        }
        async fn get_conversation(&self, _id: i64) -> Result<Option<synapse_storage::openclaw::AiConversation>, sqlx::Error> {
            unimplemented!()
        }
        async fn get_user_conversations(&self, _user_id: &str, _limit: i64, _from: Option<synapse_storage::openclaw::ConversationCursor>) -> Result<(Vec<synapse_storage::openclaw::AiConversation>, Option<String>), sqlx::Error> {
            unimplemented!()
        }
        async fn update_conversation(&self, _id: i64, _title: Option<&str>, _system_prompt: Option<&str>, _temperature: Option<f32>, _max_tokens: Option<i32>, _is_pinned: Option<bool>) -> Result<synapse_storage::openclaw::AiConversation, sqlx::Error> {
            unimplemented!()
        }
        async fn delete_conversation(&self, _id: i64) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn create_message(&self, _conversation_id: i64, _role: &str, _content: &str, _token_count: Option<i32>, _tool_calls: Option<serde_json::Value>, _tool_call_id: Option<&str>) -> Result<synapse_storage::openclaw::AiMessage, sqlx::Error> {
            unimplemented!()
        }
        async fn get_conversation_messages(&self, _conversation_id: i64, _limit: i64, _from: Option<synapse_storage::openclaw::MessageCursor>) -> Result<(Vec<synapse_storage::openclaw::AiMessage>, Option<String>), sqlx::Error> {
            unimplemented!()
        }
        async fn get_message(&self, _id: i64) -> Result<Option<synapse_storage::openclaw::AiMessage>, sqlx::Error> {
            unimplemented!()
        }
        async fn delete_message(&self, _id: i64) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn create_generation(&self, _user_id: &str, _conversation_id: Option<i64>, _gen_type: &str, _prompt: &str) -> Result<synapse_storage::openclaw::AiGeneration, sqlx::Error> {
            unimplemented!()
        }
        async fn update_generation_status(&self, _id: i64, _status: &str, _result_url: Option<&str>, _result_mxc: Option<&str>, _error_message: Option<&str>) -> Result<synapse_storage::openclaw::AiGeneration, sqlx::Error> {
            unimplemented!()
        }
        async fn get_generation(&self, _id: i64) -> Result<Option<synapse_storage::openclaw::AiGeneration>, sqlx::Error> {
            unimplemented!()
        }
        async fn get_user_generations(&self, _user_id: &str, _gen_type: Option<&str>, _limit: i64, _from: Option<synapse_storage::openclaw::GenerationCursor>) -> Result<(Vec<synapse_storage::openclaw::AiGeneration>, Option<String>), sqlx::Error> {
            unimplemented!()
        }
        async fn delete_generation(&self, _id: i64) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
        async fn create_chat_role(&self, _params: synapse_storage::openclaw::CreateChatRoleParams<'_>) -> Result<synapse_storage::openclaw::AiChatRole, sqlx::Error> {
            unimplemented!()
        }
        async fn get_chat_role(&self, _id: i64) -> Result<Option<synapse_storage::openclaw::AiChatRole>, sqlx::Error> {
            unimplemented!()
        }
        async fn get_user_chat_roles(&self, _user_id: &str) -> Result<Vec<synapse_storage::openclaw::AiChatRole>, sqlx::Error> {
            unimplemented!()
        }
        async fn update_chat_role(&self, _params: synapse_storage::openclaw::UpdateChatRoleParams<'_>) -> Result<synapse_storage::openclaw::AiChatRole, sqlx::Error> {
            unimplemented!()
        }
        async fn delete_chat_role(&self, _id: i64) -> Result<(), sqlx::Error> {
            unimplemented!()
        }
    }
}
