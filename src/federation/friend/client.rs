use crate::common::{ApiError, ApiResult};
use crate::federation::signing::canonical_federation_request_bytes;
use crate::federation::KeyRotationManager;
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine};
use ed25519_dalek::{Signer, SigningKey};
use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct FriendFederationClient {
    client: Client,
    server_name: String,
    signing_key_id: String,
    signing_key: Option<SigningKey>,
    key_rotation_manager: Option<Arc<KeyRotationManager>>,
    missing_signing_key_logged: AtomicBool,
}

impl FriendFederationClient {
    pub fn new(server_name: String, key_rotation_manager: Option<Arc<KeyRotationManager>>) -> Self {
        let signing_key_id =
            std::env::var("FEDERATION_SIGNING_KEY_ID").unwrap_or_else(|_| "ed25519:0".to_string());

        let signing_key = std::env::var("FEDERATION_SIGNING_KEY")
            .ok()
            .and_then(|key_b64| Self::decode_signing_key(&key_b64));

        Self {
            client: Client::new(),
            server_name,
            signing_key_id,
            signing_key,
            key_rotation_manager,
            missing_signing_key_logged: AtomicBool::new(false),
        }
    }

    fn decode_signing_key(key_b64: &str) -> Option<SigningKey> {
        STANDARD_NO_PAD.decode(key_b64).ok().and_then(|bytes| {
            if bytes.len() == 32 {
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                Some(SigningKey::from_bytes(&arr))
            } else {
                None
            }
        })
    }

    fn build_auth_header(
        &self,
        key_id: &str,
        signing_key: &SigningKey,
        method: &str,
        path: &str,
        destination: &str,
        content: Option<&Value>,
    ) -> Result<String, ApiError> {
        let message = canonical_federation_request_bytes(
            method,
            path,
            &self.server_name,
            destination,
            content,
        );

        let signature = signing_key.sign(&message);
        let sig_b64 = STANDARD_NO_PAD.encode(signature.to_bytes());

        Ok(format!(
            "X-Matrix origin={},destination={},key=\"{}\",sig=\"{}\"",
            self.server_name, destination, key_id, sig_b64
        ))
    }

    async fn sign_request(
        &self,
        method: &str,
        path: &str,
        destination: &str,
        content: Option<&Value>,
    ) -> Result<String, ApiError> {
        if let Some(signing_key) = self.signing_key.as_ref() {
            return self.build_auth_header(
                &self.signing_key_id,
                signing_key,
                method,
                path,
                destination,
                content,
            );
        }

        if let Some(key_rotation_manager) = &self.key_rotation_manager {
            if let Some(current_key) =
                key_rotation_manager.get_current_key().await.map_err(|e| {
                    ApiError::internal(format!("Failed to load federation signing key: {}", e))
                })?
            {
                if let Some(signing_key) = Self::decode_signing_key(&current_key.secret_key) {
                    return self.build_auth_header(
                        &current_key.key_id,
                        &signing_key,
                        method,
                        path,
                        destination,
                        content,
                    );
                }
            }
        }

        if !self
            .missing_signing_key_logged
            .swap(true, Ordering::Relaxed)
        {
            tracing::warn!(
                "Friend federation signing key unavailable; checked FEDERATION_SIGNING_KEY and database-managed federation keys"
            );
        }

        Err(ApiError::internal(
            "Federation signing key not configured".to_string(),
        ))
    }

    pub async fn send_invite(
        &self,
        destination: &str,
        _room_id: &str,
        content: &Value,
    ) -> ApiResult<()> {
        let path = format!("/_matrix/federation/v1/send/{}", uuid::Uuid::new_v4());
        let url = format!("https://{}{}", destination, path);

        let body_str = serde_json::to_string(content)
            .map_err(|e| ApiError::internal(format!("Failed to serialize body: {}", e)))?;

        let auth_header = self
            .sign_request("PUT", &path, destination, Some(content))
            .await?;

        tracing::info!("Sending federation invite to {}", url);
        let response = self
            .client
            .put(&url)
            .header("Authorization", auth_header)
            .header("Content-Type", "application/json")
            .body(body_str)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Federation request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(ApiError::internal(format!(
                "Remote server returned error: {}",
                response.status()
            )));
        }

        Ok(())
    }

    pub async fn query_remote_friends(
        &self,
        destination: &str,
        user_id: &str,
    ) -> ApiResult<Vec<String>> {
        let path = format!("/_matrix/federation/v1/user/friends/{}", user_id);
        let url = format!("https://{}{}", destination, path);

        let auth_header = self.sign_request("GET", &path, destination, None).await?;

        tracing::info!("Querying remote friends from {}", url);
        let response = self
            .client
            .get(&url)
            .header("Authorization", auth_header)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Federation request failed: {}", e)))?;

        if response.status() == StatusCode::NOT_FOUND {
            return Ok(vec![]);
        }

        if !response.status().is_success() {
            return Err(ApiError::internal(format!(
                "Remote server returned error: {}",
                response.status()
            )));
        }

        let body: Value = response
            .json()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to parse response: {}", e)))?;

        let friends = body
            .get("friends")
            .and_then(|v| v.as_array())
            .ok_or_else(|| ApiError::internal("Invalid response format"))?
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        Ok(friends)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_friend_federation_client_creation() {
        let client = FriendFederationClient::new("example.com".to_string(), None);
        assert_eq!(client.server_name, "example.com");
    }

    #[test]
    fn test_server_name_format() {
        let server_names = vec!["matrix.org", "example.com:8448", "server.local"];

        for name in server_names {
            let client = FriendFederationClient::new(name.to_string(), None);
            assert_eq!(client.server_name, name);
        }
    }

    #[test]
    fn test_federation_path_format() {
        let user_id = "@alice:example.com";
        let path = format!("/_matrix/federation/v1/user/friends/{}", user_id);

        assert!(path.starts_with("/_matrix/federation/"));
        assert!(path.contains(user_id));
    }

    #[test]
    fn test_invite_path_format() {
        let event_id = uuid::Uuid::new_v4();
        let path = format!("/_matrix/federation/v1/send/{}", event_id);

        assert!(path.starts_with("/_matrix/federation/v1/send/"));
    }

    #[test]
    fn test_friends_response_parsing() {
        let response = serde_json::json!({
            "friends": ["@alice:example.com", "@bob:example.com"]
        });

        let friends: Vec<String> = response
            .get("friends")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        assert_eq!(friends.len(), 2);
        assert!(friends.contains(&"@alice:example.com".to_string()));
    }

    #[test]
    fn test_empty_friends_response() {
        let response = serde_json::json!({
            "friends": []
        });

        let friends: Vec<String> = response
            .get("friends")
            .and_then(|v| v.as_array())
            .unwrap()
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        assert!(friends.is_empty());
    }
}
