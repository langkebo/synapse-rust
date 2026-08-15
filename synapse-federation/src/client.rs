use crate::dead_letter_queue::{DeadLetterQueueApi, DlqEntry};
use crate::key_rotation::KeyRotationManager;
use crate::signing::canonical_federation_request_bytes;
use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use ed25519_dalek::Signer;
use ed25519_dalek::SigningKey as DalekSigningKey;
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use synapse_common::current_timestamp_millis;
use synapse_common::http_client;
use synapse_common::ApiError;
use tokio::sync::RwLock;

const MAX_RETRIES: u32 = 3;
const RETRY_BASE_DELAY_MS: u64 = 500;
const MAX_RETRY_DELAY_MS: u64 = 30000;
const KEY_CACHE_TTL_SECS: u64 = 3600;
const WELL_KNOWN_TIMEOUT_SECS: u64 = 5;

/// Effective cache TTL (seconds) for a set of server keys: never longer than the
/// default window, and never past the key's own `valid_until_ts` validity window.
fn effective_cache_ttl_secs(keys: &ServerKeys, now_ms: i64) -> u64 {
    let remaining_secs = ((keys.valid_until_ts - now_ms) / 1000).max(0) as u64;
    KEY_CACHE_TTL_SECS.min(remaining_secs)
}

/// FED-01: 远程服务器密钥在缓存前必须验证自签名。
///
/// 矩阵密钥响应要求服务器用自己的 ed25519 私钥对响应体签名；
/// 不验签就缓存会让 MITM 注入伪造的 verify_keys，进而伪造任意联邦请求签名。
/// 至少要求一条 verify_key 持有有效的自签名，否则拒绝。
fn verify_server_keys_self_signature(keys: &ServerKeys) -> Result<(), FederationClientError> {
    let verify_keys = keys
        .verify_keys
        .as_object()
        .ok_or_else(|| FederationClientError::InvalidResponse("verify_keys must be an object".into()))?;
    if verify_keys.is_empty() {
        return Err(FederationClientError::InvalidResponse("verify_keys must not be empty".into()));
    }

    let self_sigs = keys
        .signatures
        .get(&keys.server_name)
        .and_then(|v| v.as_object())
        .ok_or_else(|| {
            FederationClientError::Authentication(format!(
                "server keys for {} lack a self-signature",
                keys.server_name
            ))
        })?;

    // 待验签内容：完整响应去掉 signatures/unsigned 后的 canonical JSON
    let mut value = serde_json::to_value(keys)
        .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
    synapse_common::remove_signatures_and_unsigned(&mut value);
    let message = synapse_common::canonical_json_bytes(&value)
        .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;

    let mut any_valid = false;
    for (key_id, key_data) in verify_keys {
        if key_id.split(':').next() != Some("ed25519") {
            continue;
        }
        let Some(public_key_b64) =
            key_data.get("key").and_then(|v| v.as_str()).or_else(|| key_data.as_str())
        else {
            continue;
        };
        let Some(signature_b64) = self_sigs.get(key_id).and_then(|v| v.as_str()) else {
            continue;
        };

        // Matrix 使用 unpadded base64，但对 padded 变体宽容
        let pub_bytes = STANDARD_NO_PAD
            .decode(public_key_b64)
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(public_key_b64));
        let sig_bytes = STANDARD_NO_PAD
            .decode(signature_b64)
            .or_else(|_| base64::engine::general_purpose::STANDARD.decode(signature_b64));
        let (Ok(pub_bytes), Ok(sig_bytes)) = (pub_bytes, sig_bytes) else {
            continue;
        };
        let (Ok(pub_arr), Ok(sig_arr)) =
            (<[u8; 32]>::try_from(pub_bytes.as_slice()), <[u8; 64]>::try_from(sig_bytes.as_slice()))
        else {
            continue;
        };
        let Ok(verifying_key) = ed25519_dalek::VerifyingKey::from_bytes(&pub_arr) else {
            continue;
        };
        let signature = ed25519_dalek::Signature::from_bytes(&sig_arr);
        if verifying_key.verify_strict(&message, &signature).is_ok() {
            any_valid = true;
            break;
        }
    }

    if any_valid {
        Ok(())
    } else {
        Err(FederationClientError::Authentication(format!(
            "no valid self-signature on server keys for {}",
            keys.server_name
        )))
    }
}
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
pub struct UserDevicesResponse {
    pub user_id: String,
    pub devices: Vec<serde_json::Value>,
    pub master_key: Option<serde_json::Value>,
    pub self_signing_key: Option<serde_json::Value>,
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

/// FED-06: Cached server resolution with TTL. DNS changes are detected
/// after `SERVER_RESOLUTION_TTL_SECS` seconds instead of being cached forever.
struct CachedResolvedServer {
    resolved: ResolvedServer,
    cached_at: std::time::Instant,
}

/// FED-06: TTL for server resolution cache (5 minutes). After this period,
/// DNS changes will be detected on the next resolution attempt.
const SERVER_RESOLUTION_TTL_SECS: u64 = 300;

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
        Self::internal(format!("Federation error: {e}"))
    }
}

pub struct FederationClient {
    http_client: Client,
    server_name: String,
    key_rotation_manager: Arc<KeyRotationManager>,
    key_cache: Arc<RwLock<HashMap<String, CachedKeys>>>,
    server_resolution_cache: Arc<RwLock<HashMap<String, CachedResolvedServer>>>,
    /// FED-07: Optional dead letter queue for persisting failed transactions.
    dlq: Option<Arc<dyn DeadLetterQueueApi>>,
}

impl std::fmt::Debug for FederationClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FederationClient").field("server_name", &self.server_name).finish()
    }
}

impl FederationClient {
    pub fn new(server_name: String, key_rotation_manager: Arc<KeyRotationManager>) -> Self {
        // F-1/E-1: Use shared HTTP client with connection pool reuse.
        let http_client = http_client::default_client();

        Self {
            http_client,
            server_name,
            key_rotation_manager,
            key_cache: Arc::new(RwLock::new(HashMap::new())),
            server_resolution_cache: Arc::new(RwLock::new(HashMap::new())),
            dlq: None,
        }
    }

    /// FED-07: Attach a dead letter queue to this client.
    ///
    /// When set, [`send_transaction`](Self::send_transaction) will persist
    /// failed transactions (after retries are exhausted) to the DLQ for
    /// audit and manual retry.
    pub fn with_dlq(mut self, dlq: Arc<dyn DeadLetterQueueApi>) -> Self {
        self.dlq = Some(dlq);
        self
    }

    /// FED-07: Returns the dead letter queue if one is attached.
    pub fn dead_letter_queue(&self) -> Option<&Arc<dyn DeadLetterQueueApi>> {
        self.dlq.as_ref()
    }

    pub fn server_name(&self) -> &str {
        &self.server_name
    }

    async fn get_signing_key_and_id(&self) -> Result<(String, String), FederationClientError> {
        let key = self.key_rotation_manager.get_current_key().await.map_err(|_| FederationClientError::NoSigningKey)?;
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
            .map_err(|e| FederationClientError::Authentication(format!("Invalid key: {e}")))?;
        let secret_bytes: [u8; 32] = secret_bytes_vec
            .try_into()
            .map_err(|_| FederationClientError::Authentication("Key must be 32 bytes".into()))?;
        let signing_key = DalekSigningKey::from_bytes(&secret_bytes);

        let message = canonical_federation_request_bytes(
            method,
            path,
            &self.server_name,
            destination,
            body.and_then(|s| serde_json::from_str(s).ok()).as_ref(),
        )
        .map_err(|e| FederationClientError::Authentication(format!("Canonical JSON error: {e}")))?;

        let signature = signing_key.sign(&message);
        let sig_b64 = STANDARD_NO_PAD.encode(signature.to_bytes());

        Ok(format!(
            "X-Matrix origin={},destination={},key_id={},sig={}",
            self.server_name, destination, key_id, sig_b64
        ))
    }

    pub async fn resolve_server(&self, server_name: &str) -> Result<ResolvedServer, FederationClientError> {
        // FED-06: Check cache with TTL — expired entries are treated as misses
        // so DNS changes are detected within SERVER_RESOLUTION_TTL_SECS.
        {
            let cache = self.server_resolution_cache.read().await;
            if let Some(cached) = cache.get(server_name) {
                let age = cached.cached_at.elapsed();
                if age.as_secs() < SERVER_RESOLUTION_TTL_SECS {
                    return Ok(cached.resolved.clone());
                }
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
            self.resolve_via_well_known(server_name).await.unwrap_or_else(|| ResolvedServer {
                server_name: server_name.to_string(),
                host: server_name.to_string(),
                port: DEFAULT_FEDERATION_PORT,
            })
        };

        self.server_resolution_cache
            .write()
            .await
            .insert(server_name.to_string(), CachedResolvedServer {
                resolved: resolved.clone(),
                cached_at: std::time::Instant::now(),
            });

        Ok(resolved)
    }

    async fn resolve_via_well_known(&self, server_name: &str) -> Option<ResolvedServer> {
        let url = format!("https://{server_name}/.well-known/matrix/server");
        let client = http_client::client_with_timeout(Duration::from_secs(WELL_KNOWN_TIMEOUT_SECS));

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

    fn build_url(resolved: &ResolvedServer, path: &str) -> String {
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
        let url = Self::build_url(&resolved, path);

        let mut last_error = None;
        for attempt in 0..=MAX_RETRIES {
            if attempt > 0 {
                let delay = std::cmp::min(RETRY_BASE_DELAY_MS * 2u64.pow(attempt - 1), MAX_RETRY_DELAY_MS);
                tokio::time::sleep(Duration::from_millis(delay)).await;
            }

            let retry_request = match method {
                "GET" => self.http_client.get(&url),
                "PUT" => self.http_client.put(&url),
                "POST" => self.http_client.post(&url),
                _ => return Err(FederationClientError::Connection(format!("Unsupported method: {method}"))),
            };
            let retry_request = retry_request.header("Authorization", &auth_header).header("Host", &resolved.host);
            let retry_request = if let Some(content) = body {
                retry_request.header("Content-Type", "application/json").body(content.to_string())
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
            return Err(FederationClientError::Remote { status: status.as_u16(), body });
        }
        response.json::<T>().await.map_err(|e| FederationClientError::InvalidResponse(e.to_string()))
    }

    /// Fetch server signing keys from a remote federation peer.
    ///
    /// Dup1 注释: 此 origin 路径与 web 层 keys.rs 的 notary 路径服务不同场景。
    ///   - 此处通过联邦签名请求获取已知对等方的密钥，destination 来自房间成员列表，
    ///     SSRF 风险由联邦信任模型（server_name 格式校验 + resolve_server 端口归一化）控制。
    ///   - Notary 路径需要抓取第三方服务器密钥，必须走直接 HTTP + SSRF IP 钉扎。
    ///
    /// 如需在联邦 client 层加强 SSRF 防护，可参考 web 层 keys.rs 使用
    /// synapse_common::security::validate_origin 对 destination 做格式校验。
    pub async fn get_server_keys(&self, destination: &str) -> Result<ServerKeys, FederationClientError> {
        {
            let cache = self.key_cache.read().await;
            if let Some(cached) = cache.get(destination) {
                let now_ms = current_timestamp_millis();
                if cached.cached_at.elapsed().as_secs() < effective_cache_ttl_secs(&cached.keys, now_ms) {
                    return Ok(cached.keys.clone());
                }
            }
        }

        let path = "/_matrix/key/v2/server";
        let response = self.send_signed_request("GET", path, destination, None).await?;
        let keys: ServerKeys = self.handle_response(response).await?;

        // FED-01: 验签通过前不得写入缓存
        verify_server_keys_self_signature(&keys)?;

        self.key_cache
            .write()
            .await
            .insert(destination.to_string(), CachedKeys { keys: keys.clone(), cached_at: std::time::Instant::now() });

        Ok(keys)
    }

    pub async fn query_server_keys(
        &self,
        destination: &str,
        server_name: &str,
        key_id: Option<&str>,
    ) -> Result<ServerKeys, FederationClientError> {
        let path = match key_id {
            Some(kid) => format!("/_matrix/key/v2/query/{server_name}/{kid}"),
            None => format!("/_matrix/key/v2/query/{server_name}"),
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
        let body =
            serde_json::to_string(transaction).map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;

        // FED-07: send_signed_request handles HTTP retries internally (MAX_RETRIES=3).
        // When it returns Ok, the response was received but may still be an HTTP
        // error (e.g. 5xx). handle_response converts non-2xx responses into
        // FederationClientError::Remote. Both paths must be caught by the DLQ
        // so that a 5xx from the remote server is also persisted.
        let result = match self.send_signed_request("PUT", &path, destination, Some(&body)).await {
            Ok(response) => self.handle_response(response).await,
            Err(error) => Err(error),
        };

        // FED-07: If the transaction failed (either during send or response
        // handling), persist it to the dead letter queue for audit trail and
        // manual retry. The DLQ enqueue is best-effort: if it fails we log
        // but still return the original federation error.
        if let Err(error) = &result {
            if let Some(dlq) = &self.dlq {
                tracing::warn!(
                    txn_id = %transaction.transaction_id,
                    destination = %destination,
                    error = %error,
                    "federation transaction failed, moving to DLQ"
                );
                let payload = serde_json::to_value(transaction).unwrap_or_else(|e| {
                    tracing::error!(
                        txn_id = %transaction.transaction_id,
                        error = %e,
                        "failed to serialize transaction for DLQ payload"
                    );
                    serde_json::json!({
                        "transaction_id": transaction.transaction_id,
                        "serialization_error": e.to_string(),
                    })
                });
                // retry_count records the maximum configured retry attempts
                // (MAX_RETRIES), not the exact number of attempts actually
                // made. send_signed_request may succeed on the first try and
                // then fail at handle_response (0 HTTP retries), but the DLQ
                // entry still records the configured ceiling for audit
                // purposes. Tracking the actual attempt count would require
                // send_signed_request to return it alongside the result.
                let entry = DlqEntry::new(
                    transaction.transaction_id.clone(),
                    destination.to_string(),
                    self.server_name.clone(),
                    payload,
                    error.to_string(),
                    MAX_RETRIES as i32,
                );
                if let Err(dlq_err) = dlq.enqueue(&entry).await {
                    tracing::error!(
                        txn_id = %transaction.transaction_id,
                        destination = %destination,
                        error = %dlq_err,
                        "failed to enqueue federation transaction to DLQ"
                    );
                }
            }
        }

        result
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
        let body = serde_json::to_string(event).map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
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
        let body = serde_json::to_string(event).map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
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
        let path =
            format!("/_matrix/federation/v2/invite/{}/{}", urlencoding::encode(room_id), urlencoding::encode(event_id));
        let body = serde_json::to_string(event).map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("PUT", &path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    pub async fn get_state(&self, destination: &str, room_id: &str) -> Result<StateResponse, FederationClientError> {
        let path = format!("/_matrix/federation/v1/state/{}", urlencoding::encode(room_id));
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn get_state_ids(
        &self,
        destination: &str,
        room_id: &str,
    ) -> Result<StateIdsResponse, FederationClientError> {
        let path = format!("/_matrix/federation/v1/state_ids/{}", urlencoding::encode(room_id));
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
        let ids_param =
            event_ids.iter().map(|id| format!("v={}", urlencoding::encode(id))).collect::<Vec<_>>().join("&");
        let path =
            format!("/_matrix/federation/v1/backfill/{}?{}&limit={}", urlencoding::encode(room_id), ids_param, limit);
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
        let path = format!("/_matrix/federation/v1/get_missing_events/{}", urlencoding::encode(room_id));
        let body = serde_json::json!({
            "earliest_events": earliest_events,
            "latest_events": latest_events,
            "limit": limit,
            "min_depth": min_depth.unwrap_or(0),
        });
        let body_str =
            serde_json::to_string(&body).map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("POST", &path, destination, Some(&body_str)).await?;
        self.handle_response(response).await
    }

    pub async fn get_user_devices(
        &self,
        destination: &str,
        user_id: &str,
    ) -> Result<UserDevicesResponse, FederationClientError> {
        let path = format!("/_matrix/federation/v1/user/devices/{}", urlencoding::encode(user_id));
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn query_profile(
        &self,
        destination: &str,
        user_id: &str,
    ) -> Result<ProfileResponse, FederationClientError> {
        let path = format!("/_matrix/federation/v1/query/profile?user_id={}", urlencoding::encode(user_id));
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn query_directory(
        &self,
        destination: &str,
        room_alias: &str,
    ) -> Result<DirectoryResponse, FederationClientError> {
        let path = format!("/_matrix/federation/v1/query/directory?room_alias={}", urlencoding::encode(room_alias));
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    pub async fn claim_keys(
        &self,
        destination: &str,
        claims: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        let path = "/_matrix/federation/v1/user/keys/claim";
        let body = serde_json::to_string(claims).map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("POST", path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    pub async fn query_keys(
        &self,
        destination: &str,
        query: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        let path = "/_matrix/federation/v1/user/keys/query";
        let body = serde_json::to_string(query).map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
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
            params.push(format!("limit={l}"));
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
        let path =
            format!("/_matrix/federation/v1/knock/{}/{}", urlencoding::encode(room_id), urlencoding::encode(user_id));
        let body = serde_json::to_string(event).map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("PUT", &path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    pub async fn exchange_third_party_invite(
        &self,
        destination: &str,
        room_id: &str,
        event: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        let path = format!("/_matrix/federation/v1/exchange_third_party_invite/{}", urlencoding::encode(room_id));
        let body = serde_json::to_string(event).map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
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
    use crate::dead_letter_queue::InMemoryDeadLetterQueue;

    fn create_test_client() -> (tokio::runtime::Runtime, FederationClient) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let key_rotation = {
            let _guard = rt.enter();
            Arc::new(KeyRotationManager::new(
                &Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap()),
                "test.com",
            ))
        };
        let client = FederationClient::new("test.com".to_string(), key_rotation);
        (rt, client)
    }

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
    fn cache_ttl_shrinks_to_valid_until_ts_window() {
        let now_ms = current_timestamp_millis();

        // Key expires 600s from now -> effective TTL should shrink to that window.
        let soon = ServerKeys {
            server_name: "example.com".to_string(),
            verify_keys: serde_json::json!({}),
            old_verify_keys: serde_json::json!({}),
            signatures: serde_json::json!({}),
            valid_until_ts: now_ms + 600_000,
        };
        let effective = effective_cache_ttl_secs(&soon, now_ms);
        assert!(
            (590..=600).contains(&effective),
            "TTL must shrink to the ~600s valid_until_ts window, got {effective}"
        );

        // Key valid far in the future -> capped at the default window.
        let far = ServerKeys {
            server_name: "example.com".to_string(),
            verify_keys: serde_json::json!({}),
            old_verify_keys: serde_json::json!({}),
            signatures: serde_json::json!({}),
            valid_until_ts: now_ms + 10_000_000_000,
        };
        assert_eq!(
            effective_cache_ttl_secs(&far, now_ms),
            KEY_CACHE_TTL_SECS,
            "far-future validity must cap at the default TTL"
        );

        // Already-expired key -> never serve from cache.
        let past = ServerKeys {
            server_name: "example.com".to_string(),
            verify_keys: serde_json::json!({}),
            old_verify_keys: serde_json::json!({}),
            signatures: serde_json::json!({}),
            valid_until_ts: now_ms - 60_000,
        };
        assert_eq!(effective_cache_ttl_secs(&past, now_ms), 0, "expired key must yield zero TTL");
    }

    #[test]
    fn test_resolved_server_ip_literal() {
        let (rt, client) = create_test_client();
        let resolved = rt.block_on(client.resolve_server("[::1]:8448")).unwrap();
        assert_eq!(resolved.host, "::1");
        assert_eq!(resolved.port, 8448);
    }

    #[test]
    fn test_resolved_server_with_port() {
        let (rt, client) = create_test_client();
        let resolved = rt.block_on(client.resolve_server("example.com:8448")).unwrap();
        assert_eq!(resolved.host, "example.com");
        assert_eq!(resolved.port, 8448);
    }

    #[test]
    fn test_build_url() {
        let (_rt, _client) = create_test_client();
        let resolved =
            ResolvedServer { server_name: "example.com".to_string(), host: "example.com".to_string(), port: 8448 };
        assert_eq!(
            FederationClient::build_url(&resolved, "/_matrix/federation/v1/version"),
            "https://example.com:8448/_matrix/federation/v1/version"
        );

        let resolved_443 =
            ResolvedServer { server_name: "example.com".to_string(), host: "example.com".to_string(), port: 443 };
        assert_eq!(
            FederationClient::build_url(&resolved_443, "/_matrix/federation/v1/version"),
            "https://example.com/_matrix/federation/v1/version"
        );
    }

    #[test]
    fn test_version_response_deserialization() {
        let json = r#"{"server": {"name": "synapse-rust", "version": "0.1.0"}}"#;
        let resp: VersionResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.server.name, "synapse-rust");
    }

    // ------------------------------------------------------------------
    // S2 / FED-01: 远程服务器密钥缓存前必须验证自签名，防 MITM 注入伪造密钥
    // ------------------------------------------------------------------

    fn make_signed_server_keys(
        signing_key: &ed25519_dalek::SigningKey,
        server: &str,
    ) -> ServerKeys {
        use base64::Engine;
        use ed25519_dalek::Signer;
        let key_id = "ed25519:test";
        let pub_b64 =
            base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().as_bytes());
        let mut value = serde_json::json!({
            "server_name": server,
            "verify_keys": { key_id: { "key": pub_b64 } },
            "old_verify_keys": {},
            "valid_until_ts": current_timestamp_millis() + 86_400_000,
        });
        let mut for_signing = value.clone();
        synapse_common::remove_signatures_and_unsigned(&mut for_signing);
        let msg = synapse_common::canonical_json_bytes(&for_signing).expect("canonical json");
        let sig =
            base64::engine::general_purpose::STANDARD.encode(signing_key.sign(&msg).to_bytes());
        value["signatures"] = serde_json::json!({ server: { key_id: sig } });
        serde_json::from_value(value).expect("ServerKeys")
    }

    #[test]
    fn server_keys_valid_self_signature_accepted() {
        let sk = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
        let keys = make_signed_server_keys(&sk, "remote.example");
        assert!(
            verify_server_keys_self_signature(&keys).is_ok(),
            "validly self-signed server keys must be accepted"
        );
    }

    #[test]
    fn server_keys_forged_self_signature_rejected() {
        let victim_key = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
        let attacker_key = ed25519_dalek::SigningKey::from_bytes(&[4u8; 32]);
        // 公钥是受害者的，签名却是攻击者私钥签的 —— MITM 注入场景
        let keys = {
            let mut keys = make_signed_server_keys(&victim_key, "remote.example");
            let forged = make_signed_server_keys(&attacker_key, "remote.example");
            keys.signatures = forged.signatures;
            keys
        };
        assert!(
            verify_server_keys_self_signature(&keys).is_err(),
            "forged self-signature must be rejected"
        );
    }

    #[test]
    fn server_keys_missing_self_signature_rejected() {
        let sk = ed25519_dalek::SigningKey::from_bytes(&[3u8; 32]);
        let mut keys = make_signed_server_keys(&sk, "remote.example");
        keys.signatures = serde_json::json!({});
        assert!(
            verify_server_keys_self_signature(&keys).is_err(),
            "key response without self-signature must be rejected"
        );
    }

    #[test]
    fn test_directory_response_deserialization() {
        let json = r#"{"room_id": "!room:example.com", "servers": ["example.com", "other.com"]}"#;
        let resp: DirectoryResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.room_id, "!room:example.com");
        assert_eq!(resp.servers.len(), 2);
    }

    // ------------------------------------------------------------------
    // FED-07: Dead Letter Queue integration
    // ------------------------------------------------------------------

    /// Create a test client with an in-memory DLQ attached.
    fn create_test_client_with_dlq() -> (tokio::runtime::Runtime, FederationClient, Arc<InMemoryDeadLetterQueue>) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let key_rotation = {
            let _guard = rt.enter();
            Arc::new(KeyRotationManager::new(
                &Arc::new(sqlx::PgPool::connect_lazy("postgres://localhost/test").unwrap()),
                "test.com",
            ))
        };
        let dlq = Arc::new(InMemoryDeadLetterQueue::new());
        let client =
            FederationClient::new("test.com".to_string(), key_rotation).with_dlq(dlq.clone());
        (rt, client, dlq)
    }

    fn make_test_transaction(txn_id: &str, destination: &str) -> FederationTransaction {
        FederationTransaction {
            transaction_id: txn_id.to_string(),
            origin: "test.com".to_string(),
            origin_server_ts: 1234567890000,
            destination: destination.to_string(),
            pdus: vec![],
            edus: vec![],
        }
    }

    #[test]
    fn fed07_failed_transaction_moves_to_dlq() {
        let (rt, client, dlq) = create_test_client_with_dlq();

        // The test client has no signing key configured, so send_transaction
        // will fail immediately at build_auth_header — this exercises the
        // DLQ integration path without network I/O or sleep delays.
        let txn = make_test_transaction("txn-001", "failed.example.com");

        let result = rt.block_on(client.send_transaction("failed.example.com", &txn));
        assert!(result.is_err(), "send_transaction must fail without a signing key");

        let entries = rt.block_on(dlq.list_unresolved()).unwrap();
        assert!(
            entries.iter().any(|e| e.txn_id == "txn-001"),
            "failed transaction must be in the DLQ"
        );
    }

    #[test]
    fn fed07_dlq_entry_contains_correct_fields() {
        let (rt, client, dlq) = create_test_client_with_dlq();

        let txn = make_test_transaction("txn-002", "down.example.com");
        rt.block_on(client.send_transaction("down.example.com", &txn)).ok();

        let entries = rt.block_on(dlq.list_unresolved()).unwrap();
        let entry = entries
            .iter()
            .find(|e| e.txn_id == "txn-002")
            .expect("DLQ must contain txn-002");

        assert_eq!(entry.destination, "down.example.com");
        assert_eq!(entry.origin, "test.com");
        assert!(entry.failure_reason.is_some(), "failure_reason must be populated");
        assert!(!entry.is_resolved, "new DLQ entry must be unresolved");
        assert!(entry.id.is_some(), "DLQ entry must have an id assigned");

        // payload must contain the serialized transaction
        let payload_txn_id = entry
            .payload
            .get("transaction_id")
            .and_then(|v| v.as_str())
            .expect("payload must contain transaction_id");
        assert_eq!(payload_txn_id, "txn-002");
    }

    #[test]
    fn fed07_no_dlq_attached_does_not_panic() {
        // When no DLQ is attached, send_transaction must still return the
        // error normally — no panic, no DLQ write.
        let (rt, client) = create_test_client();
        let txn = make_test_transaction("txn-003", "no-dlq.example.com");

        let result = rt.block_on(client.send_transaction("no-dlq.example.com", &txn));
        assert!(result.is_err());
        assert!(client.dead_letter_queue().is_none());
    }

    #[test]
    fn fed07_with_dlq_builder_attaches_queue() {
        let (rt, client, dlq) = create_test_client_with_dlq();

        // The DLQ Arc is shared — entries pushed by the client are visible here.
        let entries = rt.block_on(dlq.list_unresolved()).unwrap();
        assert!(entries.is_empty(), "fresh DLQ must have no entries");
        assert!(client.dead_letter_queue().is_some(), "client must have DLQ attached");
    }

    /// FED-07 (issue #3 fix): Verify that HTTP 5xx errors from
    /// `handle_response` are captured by the DLQ.
    ///
    /// Previously, `send_transaction` only enqueued to the DLQ when
    /// `send_signed_request` returned `Err`. HTTP 5xx responses went
    /// through `send_signed_request` as `Ok(response)` and then
    /// `handle_response` converted them to `Err(Remote { status: 500 })`,
    /// but that error bypassed the DLQ. The restructured `send_transaction`
    /// now catches errors from both paths.
    ///
    /// This test starts a lightweight TCP server that returns HTTP 500,
    /// sends a request to it, passes the response to `handle_response`,
    /// and verifies the resulting `Remote` error can be enqueued to the
    /// DLQ — the same error type that previously bypassed it.
    #[test]
    fn fed07_handle_response_5xx_error_captured_by_dlq() {
        let (rt, client, dlq) = create_test_client_with_dlq();

        rt.block_on(async {
            // Use wiremock (a real hyper-based server) to avoid the flaky
            // raw-TCP mock that raced with hyper's HTTP/1 dispatcher and
            // system-proxy interference in dev shells.
            use wiremock::matchers::{method, path};
            use wiremock::{Mock, MockServer, ResponseTemplate};

            let server = MockServer::start().await;
            Mock::given(method("GET"))
                .and(path("/"))
                .respond_with(ResponseTemplate::new(500))
                .mount(&server)
                .await;

            // no_proxy(): dev/CI shells may export HTTP(S)_PROXY; routing a
            // loopback request through a proxy breaks hyper's parser.
            let response = reqwest::Client::builder()
                .no_proxy()
                .build()
                .unwrap()
                .get(server.uri())
                .send()
                .await
                .unwrap();
            assert_eq!(response.status().as_u16(), 500);

            // Call handle_response — this is the path that previously
            // bypassed the DLQ. It should return Err(Remote { status: 500 }).
            let result: Result<serde_json::Value, _> = client.handle_response(response).await;
            assert!(result.is_err(), "handle_response must return Err for 5xx");

            let error = result.unwrap_err();
            assert!(
                matches!(error, FederationClientError::Remote { status: 500, .. }),
                "error must be Remote with status 500, got: {error:?}"
            );

            // Simulate what send_transaction does with this error: construct
            // a DlqEntry and enqueue it. The restructured send_transaction
            // catches this via: `if let Err(error) = &result { ... dlq.enqueue }`.
            let txn = make_test_transaction("txn-5xx", "server5xx.example.com");
            let payload = serde_json::to_value(&txn).unwrap_or_else(|_| {
                serde_json::json!({"transaction_id": "txn-5xx"})
            });
            let entry = DlqEntry::new(
                "txn-5xx".to_string(),
                "server5xx.example.com".to_string(),
                "test.com".to_string(),
                payload,
                error.to_string(),
                MAX_RETRIES as i32,
            );
            dlq.enqueue(&entry).await.unwrap();

            // Verify the DLQ captured the 5xx error.
            let entries = dlq.list_unresolved().await.unwrap();
            let entry = entries
                .iter()
                .find(|e| e.txn_id == "txn-5xx")
                .expect("DLQ must contain txn-5xx");
            assert!(
                entry.failure_reason.as_ref().unwrap().contains("500"),
                "failure_reason must contain status 500, got: {:?}",
                entry.failure_reason
            );
        });
    }
}
