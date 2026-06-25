use super::models::*;
use super::storage::IdentityStorage;
use crate::ApiResult;
use reqwest::Client;
use synapse_common::error::ApiError;

pub struct IdentityService {
    storage: IdentityStorage,
    http_client: Client,
    trusted_servers: Vec<String>,
}

impl IdentityService {
    pub fn new(storage: IdentityStorage, trusted_servers: Vec<String>) -> Self {
        Self { storage, http_client: Client::new(), trusted_servers }
    }

    pub async fn get_user_three_pids(&self, user_id: &str) -> ApiResult<Vec<ThirdPartyId>> {
        self.storage.get_user_three_pids(user_id).await
    }

    pub async fn add_three_pid(&self, address: &str, medium: &str, user_id: &str) -> ApiResult<()> {
        let three_pid = ThirdPartyId::new(address, medium, user_id);
        self.storage.add_three_pid(&three_pid).await
    }

    pub async fn remove_three_pid(&self, address: &str, medium: &str, user_id: &str) -> ApiResult<()> {
        self.storage.remove_three_pid(address, medium, user_id).await
    }

    pub async fn bind_three_pid(
        &self,
        id_server: &str,
        id_access_token: &str,
        sid: &str,
        client_secret: &str,
        user_id: &str,
    ) -> ApiResult<()> {
        self.validate_id_server(id_server)?;
        let url = format!("https://{id_server}/_matrix/identity/v3/3pid/bind");

        let body = serde_json::json!({
            "sid": sid,
            "client_secret": client_secret,
            "mxid": user_id,
            "token": id_access_token
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to bind 3PID", &e))?;

        if !response.status().is_success() {
            return Err(ApiError::internal_with_log("Identity server returned error", &response.status()));
        }

        // Parse the bind response to extract the real address and medium.
        // Per MSC2133 / Matrix Identity Service v2, the response contains
        // `address`, `medium`, and `ts`. Fall back to the request parameters
        // if the response body is missing or malformed.
        let response_json: serde_json::Value = response
            .json()
            .await
            .unwrap_or_else(|_| serde_json::json!({}));

        let address = response_json
            .get("address")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let medium = response_json
            .get("medium")
            .and_then(|v| v.as_str())
            .unwrap_or("email");

        if address.is_empty() {
            return Err(ApiError::internal(
                "Identity server bind response did not contain a valid address".to_string(),
            ));
        }

        let three_pid = ThirdPartyId::new(address, medium, user_id);
        self.storage.add_three_pid(&three_pid).await?;

        Ok(())
    }

    pub async fn unbind_three_pid(
        &self,
        id_server: &str,
        id_access_token: &str,
        address: &str,
        medium: &str,
    ) -> ApiResult<()> {
        self.validate_id_server(id_server)?;
        let url = format!("https://{id_server}/_matrix/identity/v3/3pid/unbind");

        let body = serde_json::json!({
            "address": address,
            "medium": medium,
            "id_server": id_server,
            "id_access_token": id_access_token
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to unbind 3PID", &e))?;

        if !response.status().is_success() && response.status().as_u16() != 404 {
            return Err(ApiError::internal_with_log("Identity server returned error", &response.status()));
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
        self.validate_id_server(id_server)?;
        let url = format!("https://{id_server}/_matrix/identity/v3/3pid/requestAuth");

        let body = serde_json::json!({
            "medium": medium,
            "address": address,
            "client_secret": "synapse_rust",
            "send_attempt": 1,
            "mxid": user_id,
            "token": id_access_token
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to request verification", &e))?;

        if !response.status().is_success() {
            return Err(ApiError::internal_with_log("Identity server returned error", &response.status()));
        }

        let json: serde_json::Value =
            response.json().await.map_err(|e| ApiError::internal_with_log("Failed to parse response", &e))?;

        let sid = json
            .get("sid")
            .and_then(|v| v.as_str())
            .map(String::from)
            .ok_or_else(|| ApiError::internal("Missing sid in response".to_string()))?;

        Ok(sid)
    }

    pub async fn check_3pid_validity(&self, id_server: &str, sid: &str, client_secret: &str) -> ApiResult<bool> {
        self.validate_id_server(id_server)?;
        let url = format!("https://{id_server}/_matrix/identity/v3/3pid/getValidationStatus");

        let body = serde_json::json!({
            "sid": sid,
            "client_secret": client_secret
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to check validity", &e))?;

        if !response.status().is_success() {
            return Ok(false);
        }

        let json: serde_json::Value =
            response.json().await.map_err(|e| ApiError::internal_with_log("Failed to parse response", &e))?;

        Ok(json.get("valid").and_then(|v| v.as_bool()).unwrap_or(false))
    }

    pub async fn lookup_3pid(&self, medium: &str, address: &str) -> ApiResult<Option<String>> {
        self.storage.get_three_pid_user(address, medium).await
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
        self.validate_id_server(id_server)?;
        let url = format!("https://{id_server}/_matrix/identity/v1/invite");

        let body = serde_json::json!({
            "room_id": room_id,
            "sender": inviter,
            "medium": medium,
            "address": address,
            "id_access_token": id_access_token
        });

        let response = self
            .http_client
            .post(&url)
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::internal_with_log("Failed to invite", &e))?;

        if !response.status().is_success() {
            let status = response.status();
            if status.as_u16() == 404 {
                return Ok(InvitationResponse { user_id: None, signed: None });
            }
            return Err(ApiError::internal_with_log("Identity server returned error", &status));
        }

        let json: serde_json::Value =
            response.json().await.map_err(|e| ApiError::internal_with_log("Failed to parse response", &e))?;

        let user_id = json.get("user_id").and_then(|v| v.as_str()).map(String::from);
        let signed = json.get("signed").cloned();

        Ok(InvitationResponse { user_id, signed })
    }

    pub fn get_trusted_servers(&self) -> &[String] {
        &self.trusted_servers
    }

    pub fn validate_id_server(&self, id_server: &str) -> ApiResult<()> {
        if id_server.is_empty() {
            return Err(ApiError::bad_request("id_server cannot be empty".to_string()));
        }

        if id_server.contains('/') || id_server.contains('\\') {
            return Err(ApiError::bad_request("id_server must be a hostname only".to_string()));
        }

        if id_server.starts_with('.') || id_server.ends_with('.') {
            return Err(ApiError::bad_request("id_server has invalid format".to_string()));
        }

        let host = id_server.split(':').next().unwrap_or("");
        if host.is_empty() {
            return Err(ApiError::bad_request("id_server has empty hostname".to_string()));
        }

        if host == "localhost"
            || host.starts_with("127.")
            || host.starts_with("10.")
            || host.starts_with("192.168.")
            || host.starts_with("169.254.")
        {
            return Err(ApiError::bad_request("id_server must not be a private/local address".to_string()));
        }

        if host.starts_with("0.") || host == "0.0.0.0" {
            return Err(ApiError::bad_request("id_server must not be a broadcast address".to_string()));
        }

        if !self.trusted_servers.is_empty() && !self.trusted_servers.iter().any(|s| s == id_server) {
            return Err(ApiError::bad_request(format!("id_server '{id_server}' is not in the trusted servers list")));
        }

        Ok(())
    }
}
