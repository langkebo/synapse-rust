use super::models::*;
use super::storage::IdentityStorage;
use crate::error::ApiError;
use crate::ApiResult;
use reqwest::Client;

pub struct IdentityService {
    storage: IdentityStorage,
    http_client: Client,
    trusted_servers: Vec<String>,
}

impl IdentityService {
    pub fn new(storage: IdentityStorage, trusted_servers: Vec<String>) -> Self {
        Self {
            storage,
            http_client: Client::new(),
            trusted_servers,
        }
    }

    pub async fn get_user_three_pids(&self, user_id: &str) -> ApiResult<Vec<ThirdPartyId>> {
        self.storage
            .get_user_three_pids(user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to get 3PIDs: {}", e)))
    }

    pub async fn add_three_pid(&self, address: &str, medium: &str, user_id: &str) -> ApiResult<()> {
        let three_pid = ThirdPartyId::new(address, medium, user_id);
        self.storage
            .add_three_pid(&three_pid)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to add 3PID: {}", e)))
    }

    pub async fn remove_three_pid(&self, address: &str, medium: &str, user_id: &str) -> ApiResult<()> {
        self.storage
            .remove_three_pid(address, medium, user_id)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to remove 3PID: {}", e)))
    }

    pub async fn bind_three_pid(
        &self,
        id_server: &str,
        id_access_token: &str,
        sid: &str,
        client_secret: &str,
        user_id: &str,
    ) -> ApiResult<()> {
        let url = format!("https://{}/_matrix/identity/v3/3pid/bind", id_server);

        let body = serde_json::json!({
            "sid": sid,
            "client_secret": client_secret,
            "mxid": user_id,
            "token": id_access_token
        });

        let response = self.http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to bind 3PID: {}", e)))?;

        if !response.status().is_success() {
            return Err(ApiError::internal(format!("Identity server returned error: {}", response.status())));
        }

        let three_pid = ThirdPartyId::new(
            &format!("{}:{}", id_server, sid),
            "unknown",
            user_id,
        );
        self.storage
            .add_three_pid(&three_pid)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to save 3PID: {}", e)))?;

        Ok(())
    }

    pub async fn unbind_three_pid(
        &self,
        id_server: &str,
        id_access_token: &str,
        address: &str,
        medium: &str,
    ) -> ApiResult<()> {
        let url = format!("https://{}/_matrix/identity/v3/3pid/unbind", id_server);

        let body = serde_json::json!({
            "address": address,
            "medium": medium,
            "id_server": id_server,
            "id_access_token": id_access_token
        });

        let response = self.http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to unbind 3PID: {}", e)))?;

        if !response.status().is_success() && response.status().as_u16() != 404 {
            return Err(ApiError::internal(format!("Identity server returned error: {}", response.status())));
        }

        Ok(())
    }

    pub async fn request_3pid_verification(
        &self,
        id_server: &str,
        id_access_token: &str,
        medium: &str,
        address: &str,
        user_id: &str,
    ) -> ApiResult<String> {
        let url = format!("https://{}/_matrix/identity/v3/3pid/requestAuth", id_server);

        let body = serde_json::json!({
            "medium": medium,
            "address": address,
            "client_secret": "synapse_rust",
            "send_attempt": 1,
            "mxid": user_id,
            "token": id_access_token
        });

        let response = self.http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to request verification: {}", e)))?;

        if !response.status().is_success() {
            return Err(ApiError::internal(format!("Identity server returned error: {}", response.status())));
        }

        let json: serde_json::Value = response.json().await
            .map_err(|e| ApiError::internal(format!("Failed to parse response: {}", e)))?;

        let sid = json.get("sid")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| ApiError::internal("Missing sid in response".to_string()))?;

        Ok(sid)
    }

    pub async fn check_3pid_validity(
        &self,
        id_server: &str,
        sid: &str,
        client_secret: &str,
    ) -> ApiResult<bool> {
        let url = format!("https://{}/_matrix/identity/v3/3pid/getValidationStatus", id_server);

        let body = serde_json::json!({
            "sid": sid,
            "client_secret": client_secret
        });

        let response = self.http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to check validity: {}", e)))?;

        if !response.status().is_success() {
            return Ok(false);
        }

        let json: serde_json::Value = response.json().await
            .map_err(|e| ApiError::internal(format!("Failed to parse response: {}", e)))?;

        Ok(json.get("valid").and_then(|v| v.as_bool()).unwrap_or(false))
    }

    pub async fn lookup_3pid(&self, medium: &str, address: &str) -> ApiResult<Option<String>> {
        self.storage
            .get_three_pid_user(address, medium)
            .await
            .map_err(|e| ApiError::internal(format!("Failed to lookup 3PID: {}", e)))
    }

    pub async fn hash_lookup(&self, addresses: &[String], mediums: &[String]) -> ApiResult<Vec<serde_json::Value>> {
        let mut results = Vec::new();

        for address in addresses {
            for medium in mediums {
                if let Ok(Some(_user_id)) = self.lookup_3pid(medium, address).await {
                    results.push(serde_json::json!({
                        "address": address,
                        "medium": medium,
                    }));
                }
            }
        }

        Ok(results)
    }

    pub async fn invite_3pid(
        &self,
        room_id: &str,
        inviter: &str,
        medium: &str,
        address: &str,
        id_server: &str,
        id_access_token: &str,
    ) -> ApiResult<InvitationResponse> {
        let url = format!("https://{}/_matrix/identity/v1/invite", id_server);

        let body = serde_json::json!({
            "room_id": room_id,
            "sender": inviter,
            "medium": medium,
            "address": address,
            "id_access_token": id_access_token
        });

        let response = self.http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal(format!("Failed to invite: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 404 {
                return Ok(InvitationResponse {
                    user_id: None,
                    signed: None,
                });
            }
            return Err(ApiError::internal(format!("Identity server returned error: {}", status)));
        }

        let json: serde_json::Value = response.json().await
            .map_err(|e| ApiError::internal(format!("Failed to parse response: {}", e)))?;

        let user_id = json.get("user_id").and_then(|v| v.as_str()).map(String::from);
        let signed = json.get("signed").cloned();

        Ok(InvitationResponse {
            user_id,
            signed,
        })
    }

    pub fn get_trusted_servers(&self) -> &[String] {
        &self.trusted_servers
    }
}
