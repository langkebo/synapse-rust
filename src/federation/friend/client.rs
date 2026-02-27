use crate::common::federation_test_keys::{
    generate_federation_test_keypair, sign_federation_request, FederationTestKeypair,
};
use crate::common::{ApiError, ApiResult};
use reqwest::{Client, StatusCode};
use serde_json::Value;

pub struct FriendFederationClient {
    client: Client,
    server_name: String,
    keypair: FederationTestKeypair,
}

impl FriendFederationClient {
    pub fn new(server_name: String) -> Self {
        Self {
            client: Client::new(),
            server_name,
            keypair: generate_federation_test_keypair(),
        }
    }

    /// 发送好友邀请到远程服务器
    ///
    /// PUT /_matrix/federation/v1/send_join/{roomId}/{eventId}
    /// 这里简化为发送一个自定义的好友请求事件
    pub async fn send_invite(
        &self,
        destination: &str,
        _room_id: &str,
        content: &Value,
    ) -> ApiResult<()> {
        let path = format!("/_matrix/federation/v1/send/{}", uuid::Uuid::new_v4());
        let url = format!("https://{}{}", destination, path);

        // 1. 构造请求体
        let body_str = serde_json::to_string(content)
            .map_err(|e| ApiError::internal(format!("Failed to serialize body: {}", e)))?;

        // 2. 签名
        let auth_header = sign_federation_request(
            &self.keypair.secret_key,
            "PUT",
            &path,
            &self.server_name,
            destination,
            Some(&body_str),
        )
        .map_err(|e| ApiError::internal(format!("Failed to sign request: {}", e)))?;

        // 3. 发送请求
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

    /// 查询远程用户的好友列表
    ///
    /// GET /_matrix/federation/v1/user/friends/{userId}
    pub async fn query_remote_friends(
        &self,
        destination: &str,
        user_id: &str,
    ) -> ApiResult<Vec<String>> {
        let path = format!("/_matrix/federation/v1/user/friends/{}", user_id);
        let url = format!("https://{}{}", destination, path);

        // 1. 签名 (GET 请求无 Body)
        let auth_header = sign_federation_request(
            &self.keypair.secret_key,
            "GET",
            &path,
            &self.server_name,
            destination,
            None,
        )
        .map_err(|e| ApiError::internal(format!("Failed to sign request: {}", e)))?;

        // 2. 发送请求
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

        // 3. 解析响应
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
        let client = FriendFederationClient::new("example.com".to_string());
        assert_eq!(client.server_name, "example.com");
    }

    #[test]
    fn test_federation_client_keypair() {
        let client = FriendFederationClient::new("test.server".to_string());
        assert!(!client.keypair.public_key.is_empty());
        assert!(!client.keypair.secret_key.is_empty());
    }

    #[test]
    fn test_server_name_format() {
        let server_names = vec![
            "matrix.org",
            "example.com:8448",
            "server.local",
        ];

        for name in server_names {
            let client = FriendFederationClient::new(name.to_string());
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
