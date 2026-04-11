use crate::common::ApiError;
use crate::federation::key_rotation::KeyRotationManager;
use crate::federation::signing::canonical_federation_request_bytes;
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey as DalekSigningKey;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

const MAX_RETRIES: u32 = 3;
const RETRY_BASE_DELAY_MS: u64 = 500;
const MAX_RETRY_DELAY_MS: u64 = 30000;
const KEY_CACHE_TTL_SECS: u64 = 3600;
const WELL_KNOWN_TIMEOUT_SECS: u64 = 5;
const DEFAULT_FEDERATION_PORT: u16 = 8448;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerKeys {
    pub server_name: String,
    pub verify_keys: serde_json::Value,
    pub old_verify_keys: serde_json::Value,
    pub signatures: serde_json::Value,
    pub valid_until_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationTransaction {
    pub transaction_id: String,
    pub origin: String,
    pub origin_server_ts: i64,
    pub destination: String,
    pub pdus: Vec<serde_json::Value>,
    pub edus: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MakeJoinResponse {
    pub room_id: String,
    pub event: serde_json::Value,
    pub room_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendJoinResponse {
    pub room_id: String,
    pub origin: String,
    pub state: Vec<serde_json::Value>,
    pub auth_chain: Vec<serde_json::Value>,
    pub event: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MakeLeaveResponse {
    pub room_id: String,
    pub event: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendLeaveResponse {
    pub room_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InviteResponse {
    pub event: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackfillResponse {
    pub origin: String,
    pub origin_server_ts: i64,
    pub pdus: Vec<serde_json::Value>,
    pub auth_chain: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateResponse {
    pub room_id: String,
    pub origin: String,
    pub pdus: Vec<serde_json::Value>,
    pub auth_chain: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateIdsResponse {
    pub room_id: String,
    pub origin: String,
    pub pdu_ids: Vec<String>,
    pub auth_chain_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventResponse {
    pub origin: String,
    pub origin_server_ts: i64,
    pub pdu: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserDevicesResponse {
    pub user_id: String,
    pub devices: Vec<DeviceKeys>,
    pub master_key: Option<serde_json::Value>,
    pub self_signing_key: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKeys {
    pub device_id: String,
    pub keys: serde_json::Value,
    pub algorithms: Vec<String>,
    pub signatures: serde_json::Value,
    pub user_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileResponse {
    pub displayname: Option<String>,
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryResponse {
    pub room_id: String,
    pub servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionResponse {
    pub server: ServerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedServer {
    pub server_name: String,
    pub host: String,
    pub port: u16,
}

struct CachedKeys {
    keys: ServerKeys,
    cached_at: std::time::Instant,
}

#[derive(Debug, thiserror::Error)]
pub enum FederationClientError {
    #[error("Connection error: {0}")]
    Connection(String),
    #[error("Authentication error: {0}")]
    Authentication(String),
    #[error("Remote server error: {status} {body}")]
    Remote { status: u16, body: String },
    #[error("Signing key not available")]
    NoSigningKey,
    #[error("Server discovery failed for {0}")]
    DiscoveryFailed(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Rate limited, retry after {0}ms")]
    RateLimited(u64),
    #[error("Timeout")]
    Timeout,
}

impl From<FederationClientError> for ApiError {
    fn from(e: FederationClientError) -> Self {
        ApiError::internal(format!("Federation error: {}", e))
    }
}

pub struct FederationClient {
    http_client: Client,
    server_name: String,
    key_rotation_manager: Arc<KeyRotationManager>,
    key_cache: Arc<RwLock<HashMap<String, CachedKeys>>>,
    server_resolution_cache: Arc<RwLock<HashMap<String, ResolvedServer>>>,
}

impl std::fmt::Debug for FederationClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FederationClient")
            .field("server_name", &self.server_name)
            .finish()
    }
}

impl FederationClient {
    pub fn new(server_name: String, key_rotation_manager: Arc<KeyRotationManager>) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .danger_accept_invalid_certs(true)
            .build()
            .expect("Failed to build HTTP client");

        Self {
            http_client,
            server_name,
            key_rotation_manager,
            key_cache: Arc::new(RwLock::new(HashMap::new())),
            server_resolution_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    async fn get_signing_key_and_id(&self) -> Result<(String, String), FederationClientError> {
        let key = self.key_rotation_manager.get_current_key().await
            .map_err(|_| FederationClientError::NoSigningKey)?;
        match key {
            Some(k) => Ok((k.secret_key, k.key_id)),
            None => Err(FederationClientError::NoSigningKey),
        }
    }

    async fn build_auth_header(
        &self,
        method: &str,
        path: &str,
        destination: &str,
        body: Option<&str>,
    ) -> Result<String, FederationClientError> {
        let (secret_key, key_id) = self.get_signing_key_and_id().await?;

        let secret_bytes_vec = STANDARD_NO_PAD
            .decode(&secret_key)
            .map_err(|e| FederationClientError::Authentication(format!("Invalid key: {}", e)))?;
        let secret_bytes: [u8; 32] = secret_bytes_vec
            .try_into()
            .map_err(|_| FederationClientError::Authentication("Key must be 32 bytes".into()))?;
        let signing_key = DalekSigningKey::from_bytes(&secret_bytes);

        let message = canonical_federation_request_bytes(
            method,
            path,
            &self.server_name,
            destination,
            body.map(|s| serde_json::from_str(s).unwrap_or(serde_json::Value::Null)).as_ref(),
        );

        let signature = signing_key.sign(&message);
        let sig_b64 = STANDARD_NO_PAD.encode(signature.to_bytes());

        Ok(format!(
            "X-Matrix origin={},destination={},key_id={},sig={}",
            self.server_name, destination, key_id, sig_b64
        ))
    }

    pub async fn resolve_server(
        &self,
        server_name: &str,
    ) -> Result<ResolvedServer, FederationClientError> {
        {
            let cache = self.server_resolution_cache.read().await;
            if let Some(resolved) = cache.get(server_name) {
                return Ok(resolved.clone());
            }
        }

        let resolved = if server_name.starts_with('[') {
            if let Some(close) = server_name.find(']') {
                let host = server_name[1..close].to_string();
                let port = if close + 1 < server_name.len() && server_name[close + 1..].starts_with(':') {
                    server_name[close + 2..].parse().unwrap_or(DEFAULT_FEDERATION_PORT)
                } else {
                    DEFAULT_FEDERATION_PORT
                };
                ResolvedServer { server_name: server_name.to_string(), host, port }
            } else {
                ResolvedServer {
                    server_name: server_name.to_string(),
                    host: server_name.to_string(),
                    port: DEFAULT_FEDERATION_PORT,
                }
            }
        } else if let Some(colon_pos) = server_name.rfind(':') {
            let host = server_name[..colon_pos].to_string();
            let port = server_name[colon_pos + 1..].parse().unwrap_or(DEFAULT_FEDERATION_PORT);
            ResolvedServer { server_name: server_name.to_string(), host, port }
        } else {
            self.resolve_via_well_known(server_name).await.unwrap_or_else(|| {
                ResolvedServer {
                    server_name: server_name.to_string(),
                    host: server_name.to_string(),
                    port: DEFAULT_FEDERATION_PORT,
                }
            })
        };

        self.server_resolution_cache
            .write()
            .await
            .insert(server_name.to_string(), resolved.clone());

        Ok(resolved)
    }

    async fn resolve_via_well_known(&self, server_name: &str) -> Option<ResolvedServer> {
        let url = format!("https://{}/.well-known/matrix/server", server_name);
        let client = Client::builder()
            .timeout(Duration::from_secs(WELL_KNOWN_TIMEOUT_SECS))
            .danger_accept_invalid_certs(true)
            .build()
            .ok()?;

        let response = client.get(&url).send().await.ok()?;
        if !response.status().is_success() {
            return None;
        }

        let body: serde_json::Value = response.json().await.ok()?;
        let delegated = body.get("m.server")?.as_str()?.to_string();

        if let Some(colon_pos) = delegated.rfind(':') {
            let host = delegated[..colon_pos].to_string();
            let port = delegated[colon_pos + 1..].parse().ok()?;
            Some(ResolvedServer { server_name: server_name.to_string(), host, port })
        } else {
            Some(ResolvedServer {
                server_name: server_name.to_string(),
                host: delegated,
                port: DEFAULT_FEDERATION_PORT,
            })
        }
    }

    fn build_url(&self, resolved: &ResolvedServer, path: &str) -> String {
        if resolved.port == 443 {
            format!("https://{}{}", resolved.host, path)
        } else {
            format!("https://{}:{}{}", resolved.host, resolved.port, path)
        }
    }

    async fn send_signed_request(
        &self,
        method: &str,
        path: &str,
        destination: &str,
        body: Option<&str>,
    ) -> Result<reqwest::Response, FederationClientError> {
        let auth_header = self.build_auth_header(method, path, destination, body).await?;
        let resolved = self.resolve_server(destination).await?;
        let url = self.build_url(&resolved, path);

        let mut last_error = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = std::cmp::min(
                    RETRY_BASE_DELAY_MS * 2u64.pow(attempt - 1),
                    MAX_RETRY_DELAY_MS,
                );
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }

            let retry_request = match method {
                "GET" => self.http_client.get(&url),
                "PUT" => self.http_client.put(&url),
                "POST" => self.http_client.post(&url),
                _ => return Err(FederationClientError::Connection(format!("Unsupported method: {}", method))),
            };
            let retry_request = retry_request
                .header("Authorization", &auth_header)
                .header("Host", &resolved.host);
            let retry_request = if let Some(content) = body {
                retry_request
                    .header("Content-Type", "application/json")
                    .body(content.to_string())
            } else {
                retry_request
            };

            match retry_request.send().await {
                Ok(response) => {
                    let status = response.status();
                    if status == StatusCode::TOO_MANY_REQUESTS {
                        let retry_after = response
                            .headers()
                            .get("Retry-After")
                            .and_then(|v| v.to_str().ok())
                            .and_then(|v| v.parse::<u64>().ok())
                            .unwrap_or(5000);
                        last_error = Some(FederationClientError::RateLimited(retry_after));
                        continue;
                    }
                    return Ok(response);
                }
                Err(e) => {
                    if e.is_timeout() {
                        last_error = Some(FederationClientError::Timeout);
                    } else {
                        last_error = Some(FederationClientError::Connection(e.to_string()));
                    }
                    continue;
                }
            }
        }

        Err(last_error.unwrap_or(FederationClientError::Connection("Max retries exceeded".into())))
    }

    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        response: reqwest::Response,
    ) -> Result<T, FederationClientError> {
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(FederationClientError::Remote {
                status: status.as_u16(),
                body,
            });
        }
        response
            .json::<T>()
            .await
            .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))
    }

    pub async fn get_server_keys(&self, destination: &str) -> Result<ServerKeys, FederationClientError> {
        {
            let cache = self.key_cache.read().await;
            if let Some(cached) = cache.get(destination) {
                if cached.cached_at.elapsed().as_secs() < KEY_CACHE_TTL_SECS {
                    return Ok(cached.keys.clone());
                }
            }
        }

        let path = "/_matrix/key/v2/server";
        let response = self.send_signed_request("GET", path, destination, None).await?;
        let keys: ServerKeys = self.handle_response(response).await?;

        self.key_cache
            .write()
            .await
            .insert(destination.to_string(), CachedKeys {
                keys: keys.clone(),
                cached_at: std::time::Instant::now(),
            });

        Ok(keys)
    }

    pub async fn query_server_keys(
        &self,
        destination: &str,
        server_name: &str,
        key_id: Option<&str>,
    ) -> Result<ServerKeys, FederationClientError> {
        let path = match key_id {
            Some(kid) => format!("/_matrix/key/v2/query/{}/{}", server_name, kid),
            None => format!("/_matrix/key/v2/query/{}", server_name),
        };
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn get_version(&self, destination: &str) -> Result<VersionResponse, FederationClientError> {
        let path = "/_matrix/federation/v1/version";
        let response = self.send_signed_request("GET", path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn send_transaction(
        &self,
        destination: &str,
        transaction: &FederationTransaction,
    ) -> Result<serde_json::Value, FederationClientError> {
        let path = format!("/_matrix/federation/v1/send/{}", transaction.transaction_id);
        let body = serde_json::to_string(transaction)
            .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("PUT", &path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    pub async fn make_join(
        &self,
        destination: &str,
        room_id: &str,
        user_id: &str,
    ) -> Result<MakeJoinResponse, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/make_join/{}/{}?ver=v10",
            urlencoding::encode(room_id),
            urlencoding::encode(user_id)
        );
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn send_join(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
        event: &serde_json::Value,
    ) -> Result<SendJoinResponse, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v2/send_join/{}/{}",
            urlencoding::encode(room_id),
            urlencoding::encode(event_id)
        );
        let body = serde_json::to_string(event)
            .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("PUT", &path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    pub async fn make_leave(
        &self,
        destination: &str,
        room_id: &str,
        user_id: &str,
    ) -> Result<MakeLeaveResponse, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/make_leave/{}/{}",
            urlencoding::encode(room_id),
            urlencoding::encode(user_id)
        );
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn send_leave(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
        event: &serde_json::Value,
    ) -> Result<SendLeaveResponse, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v2/send_leave/{}/{}",
            urlencoding::encode(room_id),
            urlencoding::encode(event_id)
        );
        let body = serde_json::to_string(event)
            .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("PUT", &path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    pub async fn invite(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
        event: &serde_json::Value,
    ) -> Result<InviteResponse, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v2/invite/{}/{}",
            urlencoding::encode(room_id),
            urlencoding::encode(event_id)
        );
        let body = serde_json::to_string(event)
            .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("PUT", &path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    pub async fn get_event(
        &self,
        destination: &str,
        event_id: &str,
    ) -> Result<EventResponse, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/event/{}",
            urlencoding::encode(event_id)
        );
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn get_state(
        &self,
        destination: &str,
        room_id: &str,
    ) -> Result<StateResponse, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/state/{}",
            urlencoding::encode(room_id)
        );
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn get_state_ids(
        &self,
        destination: &str,
        room_id: &str,
    ) -> Result<StateIdsResponse, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/state_ids/{}",
            urlencoding::encode(room_id)
        );
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn backfill(
        &self,
        destination: &str,
        room_id: &str,
        event_ids: &[String],
        limit: u32,
    ) -> Result<BackfillResponse, FederationClientError> {
        let ids_param = event_ids.iter()
            .map(|id| format!("v={}", urlencoding::encode(id)))
            .collect::<Vec<_>>()
            .join("&");
        let path = format!(
            "/_matrix/federation/v1/backfill/{}?{}&limit={}",
            urlencoding::encode(room_id),
            ids_param,
            limit
        );
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn get_missing_events(
        &self,
        destination: &str,
        room_id: &str,
        earliest_events: &[String],
        latest_events: &[String],
        limit: u32,
        min_depth: Option<i64>,
    ) -> Result<serde_json::Value, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/get_missing_events/{}",
            urlencoding::encode(room_id)
        );
        let body = serde_json::json!({
            "earliest_events": earliest_events,
            "latest_events": latest_events,
            "limit": limit,
            "min_depth": min_depth.unwrap_or(0),
        });
        let body_str = serde_json::to_string(&body)
            .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("POST", &path, destination, Some(&body_str)).await?;
        self.handle_response(response).await
    }

    pub async fn get_event_auth(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
    ) -> Result<serde_json::Value, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/get_event_auth/{}/{}",
            urlencoding::encode(room_id),
            urlencoding::encode(event_id)
        );
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn get_user_devices(
        &self,
        destination: &str,
        user_id: &str,
    ) -> Result<UserDevicesResponse, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/user/devices/{}",
            urlencoding::encode(user_id)
        );
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn query_profile(
        &self,
        destination: &str,
        user_id: &str,
    ) -> Result<ProfileResponse, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/query/profile?user_id={}",
            urlencoding::encode(user_id)
        );
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn query_directory(
        &self,
        destination: &str,
        room_alias: &str,
    ) -> Result<DirectoryResponse, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/query/directory?room_alias={}",
            urlencoding::encode(room_alias)
        );
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn claim_keys(
        &self,
        destination: &str,
        claims: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        let path = "/_matrix/federation/v1/keys/claim";
        let body = serde_json::to_string(claims)
            .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("POST", path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    pub async fn query_keys(
        &self,
        destination: &str,
        query: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        let path = "/_matrix/federation/v1/keys/query";
        let body = serde_json::to_string(query)
            .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("POST", path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    pub async fn timestamp_to_event(
        &self,
        destination: &str,
        room_id: &str,
        timestamp: i64,
        direction: &str,
    ) -> Result<serde_json::Value, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/timestamp_to_event/{}?timestamp={}&direction={}",
            urlencoding::encode(room_id),
            timestamp,
            direction
        );
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn get_public_rooms(
        &self,
        destination: &str,
        limit: Option<u32>,
        since: Option<&str>,
    ) -> Result<serde_json::Value, FederationClientError> {
        let mut path = "/_matrix/federation/v1/publicRooms".to_string();
        let mut params = Vec::new();
        if let Some(l) = limit {
            params.push(format!("limit={}", l));
        }
        if let Some(s) = since {
            params.push(format!("since={}", urlencoding::encode(s)));
        }
        if !params.is_empty() {
            path = format!("{}?{}", path, params.join("&"));
        }
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn knock_room(
        &self,
        destination: &str,
        room_id: &str,
        user_id: &str,
        event: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/knock/{}/{}",
            urlencoding::encode(room_id),
            urlencoding::encode(user_id)
        );
        let body = serde_json::to_string(event)
            .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("PUT", &path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    pub async fn exchange_third_party_invite(
        &self,
        destination: &str,
        room_id: &str,
        event: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/exchange_third_party_invite/{}",
            urlencoding::encode(room_id)
        );
        let body = serde_json::to_string(event)
            .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("PUT", &path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    pub async fn media_download(
        &self,
        destination: &str,
        server_name: &str,
        media_id: &str,
    ) -> Result<reqwest::Response, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/media/download/{}/{}",
            urlencoding::encode(server_name),
            urlencoding::encode(media_id)
        );
        self.send_signed_request("GET", &path, destination, None).await
    }

    pub async fn media_thumbnail(
        &self,
        destination: &str,
        server_name: &str,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
    ) -> Result<reqwest::Response, FederationClientError> {
        let path = format!(
            "/_matrix/federation/v1/media/thumbnail/{}/{}?width={}&height={}&method={}",
            urlencoding::encode(server_name),
            urlencoding::encode(media_id),
            width,
            height,
            method
        );
        self.send_signed_request("GET", &path, destination, None).await
    }

    pub fn invalidate_key_cache(&self, server_name: &str) {
        let cache = self.key_cache.clone();
        let name = server_name.to_string();
        tokio::spawn(async move {
            cache.write().await.remove(&name);
        });
    }

    pub async fn get_cached_key(&self, server_name: &str) -> Option<ServerKeys> {
        let cache = self.key_cache.read().await;
        cache.get(server_name).map(|c| c.keys.clone())
    }

    pub async fn health_check(&self, destination: &str) -> bool {
        self.get_version(destination).await.is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_federation_transaction_serialization() {
        let txn = FederationTransaction {
            transaction_id: "txn_123".to_string(),
            origin: "example.com".to_string(),
            origin_server_ts: 1234567890000,
            destination: "remote.com".to_string(),
            pdus: vec![],
            edus: vec![],
        };
        let json = serde_json::to_string(&txn).unwrap();
        assert!(json.contains("txn_123"));
        assert!(json.contains("example.com"));
    }

    #[test]
    fn test_make_join_response_deserialization() {
        let json = r#"{
            "room_id": "!room:example.com",
            "event": {"type": "m.room.member"},
            "room_version": "10"
        }"#;
        let resp: MakeJoinResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.room_id, "!room:example.com");
        assert_eq!(resp.room_version, Some("10".to_string()));
    }

    #[test]
    fn test_server_keys_deserialization() {
        let json = r#"{
            "server_name": "example.com",
            "verify_keys": {},
            "old_verify_keys": {},
            "signatures": {},
            "valid_until_ts": 1234567890000
        }"#;
        let keys: ServerKeys = serde_json::from_str(json).unwrap();
        assert_eq!(keys.server_name, "example.com");
    }

    #[test]
    fn test_resolved_server_ip_literal() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let key_rotation = Arc::new(KeyRotationManager::new(
            &Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap()),
            "test.com",
        ));
        let client = FederationClient::new("test.com".to_string(), key_rotation);

        let resolved = rt.block_on(client.resolve_server("[::1]:8448")).unwrap();
        assert_eq!(resolved.host, "::1");
        assert_eq!(resolved.port, 8448);
    }

    #[test]
    fn test_resolved_server_with_port() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let key_rotation = Arc::new(KeyRotationManager::new(
            &Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap()),
            "test.com",
        ));
        let client = FederationClient::new("test.com".to_string(), key_rotation);

        let resolved = rt.block_on(client.resolve_server("example.com:8448")).unwrap();
        assert_eq!(resolved.host, "example.com");
        assert_eq!(resolved.port, 8448);
    }

    #[test]
    fn test_build_url() {
        let key_rotation = Arc::new(KeyRotationManager::new(
            &Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap()),
            "test.com",
        ));
        let client = FederationClient::new("test.com".to_string(), key_rotation);

        let resolved = ResolvedServer {
            server_name: "example.com".to_string(),
            host: "example.com".to_string(),
            port: 8448,
        };
        assert_eq!(
            client.build_url(&resolved, "/_matrix/federation/v1/version"),
            "https://example.com:8448/_matrix/federation/v1/version"
        );

        let resolved_443 = ResolvedServer {
            server_name: "example.com".to_string(),
            host: "example.com".to_string(),
            port: 443,
        };
        assert_eq!(
            client.build_url(&resolved_443, "/_matrix/federation/v1/version"),
            "https://example.com/_matrix/federation/v1/version"
        );
    }

    #[test]
    fn test_version_response_deserialization() {
        let json = r#"{"server": {"name": "synapse-rust", "version": "0.1.0"}}"#;
        let resp: VersionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.server.name, "synapse-rust");
    }

    #[test]
    fn test_directory_response_deserialization() {
        let json = r#"{"room_id": "!room:example.com", "servers": ["example.com", "other.com"]}"#;
        let resp: DirectoryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.room_id, "!room:example.com");
        assert_eq!(resp.servers.len(), 2);
    }
}
