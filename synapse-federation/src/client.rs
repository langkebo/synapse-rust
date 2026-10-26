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

/// Maximum server key validity window per Matrix Server-Server API spec v1.6 §1.2
/// ("Server Discovery"): servers MUST advertise `valid_until_ts` of at most
/// `now + 7 days`; we apply the same cap when deriving our cache TTL so that a
/// peer publishing a `valid_until_ts` one year in the future still only gets
/// cached for up to 7 days. This bounds blast radius if a peer key is later
/// compromised (cached stale key would otherwise be served for the full year).
const MAX_SERVER_KEY_VALIDITY_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// Effective cache TTL (seconds) for a set of server keys.
///
/// The TTL is the minimum of:
///   - [`KEY_CACHE_TTL_SECS`] (1 hour, our own refresh budget)
///   - `valid_until_ts - now` (don't cache past the peer's own validity)
///   - `now + MAX_SERVER_KEY_VALIDITY_MS - now` (spec §1.2 7-day upper bound)
///
/// Spec reference: <https://spec.matrix.org/v1.6/server-server-api/#server-discovery>
/// "Servers MUST publish a `valid_until_ts` no more than 7 days in the future."
fn effective_cache_ttl_secs(keys: &ServerKeys, now_ms: i64) -> u64 {
    let remaining_secs = ((keys.valid_until_ts - now_ms) / 1000).max(0) as u64;
    let max_validity_secs = (MAX_SERVER_KEY_VALIDITY_MS / 1000) as u64;
    KEY_CACHE_TTL_SECS.min(remaining_secs).min(max_validity_secs)
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

    let self_sigs = keys.signatures.get(&keys.server_name).and_then(|v| v.as_object()).ok_or_else(|| {
        FederationClientError::Authentication(format!("server keys for {} lack a self-signature", keys.server_name))
    })?;

    // 待验签内容：完整响应去掉 signatures/unsigned 后的 canonical JSON
    let mut value = serde_json::to_value(keys).map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
    synapse_common::remove_signatures_and_unsigned(&mut value);
    let message = synapse_common::canonical_json_bytes(&value)
        .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;

    let mut any_valid = false;
    for (key_id, key_data) in verify_keys {
        if key_id.split(':').next() != Some("ed25519") {
            continue;
        }
        let Some(public_key_b64) = key_data.get("key").and_then(|v| v.as_str()).or_else(|| key_data.as_str()) else {
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

/// FED-02: the `server_name` inside a key document must be the server we asked
/// about — the document is a claim about *that* identity, nothing else.
///
/// Without this check a peer can hand back a key document that is internally
/// consistent (validly self-signed, non-empty `verify_keys`) but names a
/// *different* server. Trusting it would bind `destination -> wrong server's
/// keys` in [`FederationClient::key_cache`], i.e. cache poisoning across the
/// destination identity: a later signature check for `destination` would be
/// performed against an unrelated server's `verify_keys`, and every key
/// rotation on the real destination would silently invalidate.
///
/// AGENTS.md is explicit that `server_name` must be validated **before caching
/// remote keys**, which is why this runs ahead of
/// [`verify_server_keys_self_signature`]: the signature is looked up under
/// `keys.server_name`, so pinning the name first is what makes that lookup mean
/// "the destination signed this" rather than "whoever this document claims to
/// be signed this".
///
/// The comparison is intentionally strict (no case folding, no port stripping):
/// Matrix server names appear verbatim in IDs, in `signatures` object keys, and
/// in `m.server` delegation responses, so a peer that reports a different
/// spelling is either misconfigured or substituting identities. Fail closed.
fn verify_server_keys_server_name(keys: &ServerKeys, expected: &str) -> Result<(), FederationClientError> {
    if keys.server_name != expected {
        return Err(FederationClientError::Authentication(format!(
            "server keys name mismatch: requested {expected}, document claims {}",
            keys.server_name
        )));
    }
    Ok(())
}

/// Single trust gate for a remote server-key document.
///
/// Both key-fetching entry points funnel through here so the two checks can
/// never drift apart, and so there is exactly one place to audit when the trust
/// model changes:
///   1. the document names the server we asked for ([`verify_server_keys_server_name`]);
///   2. at least one `ed25519` `verify_key` carries a valid self-signature
///      ([`verify_server_keys_self_signature`]).
fn validate_remote_server_keys(keys: &ServerKeys, expected_server: &str) -> Result<(), FederationClientError> {
    verify_server_keys_server_name(keys, expected_server)?;
    verify_server_keys_self_signature(keys)
}

const DEFAULT_FEDERATION_PORT: u16 = 8448;

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `ServerKeys` type.
pub struct ServerKeys {
    /// The `server_name` field.
    /// The `verify_keys` field.
    /// The `old_verify_keys` field.
    /// The `signatures` field.
    /// The `valid_until_ts` field.
    pub server_name: String,
    /// The `verify_keys` field.
    /// The `old_verify_keys` field.
    /// The `signatures` field.
    /// The `valid_until_ts` field.
    pub verify_keys: serde_json::Value,
    /// The `old_verify_keys` field.
    /// The `signatures` field.
    /// The `valid_until_ts` field.
    pub old_verify_keys: serde_json::Value,
    /// The `signatures` field.
    /// The `valid_until_ts` field.
    pub signatures: serde_json::Value,
    /// The `valid_until_ts` field.
    pub valid_until_ts: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `FederationTransaction` type.
pub struct FederationTransaction {
    /// The `transaction_id` field.
    /// The `origin` field.
    /// The `origin_server_ts` field.
    /// The `destination` field.
    /// The `pdus` field.
    /// The `edus` field.
    pub transaction_id: String,
    /// The `origin` field.
    /// The `origin_server_ts` field.
    /// The `destination` field.
    /// The `pdus` field.
    /// The `edus` field.
    pub origin: String,
    /// The `origin_server_ts` field.
    /// The `destination` field.
    /// The `pdus` field.
    /// The `edus` field.
    pub origin_server_ts: i64,
    /// The `destination` field.
    /// The `pdus` field.
    /// The `edus` field.
    pub destination: String,
    /// The `pdus` field.
    /// The `edus` field.
    pub pdus: Vec<serde_json::Value>,
    /// The `edus` field.
    pub edus: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `MakeJoinResponse` type.
pub struct MakeJoinResponse {
    /// The `room_id` field.
    ///
    /// Not part of the `/make_join` response per the Matrix spec (which only
    /// returns `event` + `room_version`); kept optional so peers that do include
    /// it still deserialize, but our own server omits it.
    pub room_id: Option<String>,
    /// The `event` field.
    pub event: serde_json::Value,
    /// The `room_version` field.
    pub room_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SendJoinResponse` type.
pub struct SendJoinResponse {
    /// The `room_id` field.
    /// The `origin` field.
    /// The `state` field.
    /// The `auth_chain` field.
    /// The `event` field.
    pub room_id: String,
    /// The `origin` field.
    /// The `state` field.
    /// The `auth_chain` field.
    /// The `event` field.
    pub origin: String,
    /// The `state` field.
    /// The `auth_chain` field.
    /// The `event` field.
    pub state: Vec<serde_json::Value>,
    /// The `auth_chain` field.
    /// The `event` field.
    pub auth_chain: Vec<serde_json::Value>,
    /// The `event` field.
    pub event: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `MakeLeaveResponse` type.
pub struct MakeLeaveResponse {
    /// The `room_id` field.
    /// The `event` field.
    pub room_id: String,
    /// The `event` field.
    pub event: serde_json::Value,
    /// The `room_version` field (MSC1813 / spec `make_leave` response).
    ///
    /// `None` when the resident server did not state a version; the caller must
    /// not guess one, because the signature material is room-version dependent.
    pub room_version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `SendLeaveResponse` type.
pub struct SendLeaveResponse {
    /// The `room_id` field.
    pub room_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `InviteResponse` type.
pub struct InviteResponse {
    /// The `event` field.
    pub event: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `BackfillResponse` type.
pub struct BackfillResponse {
    /// The `origin` field.
    /// The `origin_server_ts` field.
    /// The `pdus` field.
    /// The `auth_chain` field.
    pub origin: String,
    /// The `origin_server_ts` field.
    /// The `pdus` field.
    /// The `auth_chain` field.
    pub origin_server_ts: i64,
    /// The `pdus` field.
    /// The `auth_chain` field.
    pub pdus: Vec<serde_json::Value>,
    /// The `auth_chain` field.
    pub auth_chain: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `StateResponse` type.
pub struct StateResponse {
    /// The `room_id` field.
    /// The `origin` field.
    /// The `pdus` field.
    /// The `auth_chain` field.
    pub room_id: String,
    /// The `origin` field.
    /// The `pdus` field.
    /// The `auth_chain` field.
    pub origin: String,
    /// The `pdus` field.
    /// The `auth_chain` field.
    pub pdus: Vec<serde_json::Value>,
    /// The `auth_chain` field.
    pub auth_chain: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `StateIdsResponse` type.
pub struct StateIdsResponse {
    /// The `room_id` field.
    /// The `origin` field.
    /// The `pdu_ids` field.
    /// The `auth_chain_ids` field.
    pub room_id: String,
    /// The `origin` field.
    /// The `pdu_ids` field.
    /// The `auth_chain_ids` field.
    pub origin: String,
    /// The `pdu_ids` field.
    /// The `auth_chain_ids` field.
    pub pdu_ids: Vec<String>,
    /// The `auth_chain_ids` field.
    pub auth_chain_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `UserDevicesResponse` type.
pub struct UserDevicesResponse {
    /// The `user_id` field.
    /// The `devices` field.
    /// The `master_key` field.
    /// The `self_signing_key` field.
    pub user_id: String,
    /// The `devices` field.
    /// The `master_key` field.
    /// The `self_signing_key` field.
    pub devices: Vec<serde_json::Value>,
    /// The `master_key` field.
    /// The `self_signing_key` field.
    pub master_key: Option<serde_json::Value>,
    /// The `self_signing_key` field.
    pub self_signing_key: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `ProfileResponse` type.
pub struct ProfileResponse {
    /// The `displayname` field.
    /// The `avatar_url` field.
    pub displayname: Option<String>,
    /// The `avatar_url` field.
    pub avatar_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `DirectoryResponse` type.
pub struct DirectoryResponse {
    /// The `room_id` field.
    /// The `servers` field.
    pub room_id: String,
    /// The `servers` field.
    pub servers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `VersionResponse` type.
pub struct VersionResponse {
    /// The `server` field.
    pub server: ServerInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `ServerInfo` type.
pub struct ServerInfo {
    /// The `name` field.
    /// The `version` field.
    pub name: String,
    /// The `version` field.
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// The `ResolvedServer` type.
pub struct ResolvedServer {
    /// The `server_name` field.
    /// The `host` field.
    /// The `port` field.
    pub server_name: String,
    /// The `host` field.
    /// The `port` field.
    pub host: String,
    /// The `port` field.
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

/// B9: Upper bound on the number of entries retained in each of the
/// per-`FederationClient` in-memory caches (`key_cache` and
/// `server_resolution_cache`).
///
/// Both maps are keyed by remote server name, which is attacker-influenced
/// (a message can reference arbitrarily many rooms/servers), so an uncapped map
/// grows without bound on a long-lived process. Concurrent peers number in the
/// dozens at most, so a few thousand entries is ample; inserts beyond the cap
/// evict the oldest entry (see [`enforce_cache_bound`]).
const MAX_CACHE_ENTRIES: usize = 4096;

/// B9: Bound an in-memory cache at [`MAX_CACHE_ENTRIES`] entries.
///
/// Called before every insert. While the map is at capacity, the entry with the
/// smallest `cached_at` (i.e. the oldest insert) is dropped, which keeps the
/// cap without pulling in an LRU dependency. The scan only runs once the map is
/// full, so the common path stays O(1).
fn enforce_cache_bound<T>(cache: &mut HashMap<String, T>, cached_at: impl Fn(&T) -> std::time::Instant) {
    while cache.len() >= MAX_CACHE_ENTRIES {
        let Some(oldest) = cache.iter().min_by_key(|(_, value)| cached_at(value)).map(|(key, _)| key.clone()) else {
            break;
        };
        cache.remove(&oldest);
    }
}

#[derive(Debug, thiserror::Error)]
/// The `FederationClientError` enum.
pub enum FederationClientError {
    /// The `Connection` variant.
    #[error("Connection error: {0}")]
    Connection(String),
    /// The `Authentication` variant.
    #[error("Authentication error: {0}")]
    Authentication(String),
    /// The `Remote` variant.
    #[error("Remote server error: {status} {body}")]
    Remote {
        /// The `status` field.
        status: u16,
        /// The `body` field.
        body: String,
    },
    /// The `NoSigningKey` variant.
    #[error("Signing key not available")]
    NoSigningKey,
    /// The `DiscoveryFailed` variant.
    #[error("Server discovery failed for {0}")]
    DiscoveryFailed(String),
    /// The `InvalidResponse` variant.
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    /// The `RateLimited` variant.
    #[error("Rate limited, retry after {0}ms")]
    RateLimited(u64),
    /// The `ServerBlocked` variant.
    #[error("Server blocked (possible SSRF): {0}")]
    ServerBlocked(String),
    /// The `Timeout` variant.
    #[error("Timeout")]
    Timeout,
}

/// Implementation of [`From`] methods.
impl From<FederationClientError> for ApiError {
    fn from(e: FederationClientError) -> Self {
        Self::internal(format!("Federation error: {e}"))
    }
}

// ---------------------------------------------------------------------------
// F-04: SSRF prevention for federation destinations
// ---------------------------------------------------------------------------

/// F-04: Reject IP literals as federation destinations to prevent SSRF.
///
/// Matrix federation is name-based — server_name must be a DNS name, not an IP
/// address.  This function rejects ALL IPs (private, loopback, multicast, and
/// public) because:
///  - Private/loopback: obvious internal-resource attack (port scanning, internal
///    service access, database queries, etc.).
///  - Public IPs: if an attacker can set `server_name` to a public IP, they can
///    hijack federation traffic regardless of what DNS says. Federation must go
///    through DNS so that TLS certificate validation secures transport.
pub(crate) fn validate_federation_host_not_ssrf(host: &str) -> Result<(), String> {
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        let kind = match ip {
            std::net::IpAddr::V4(v4) => {
                if v4.is_loopback() {
                    "loopback IPv4"
                } else if v4.is_private() {
                    "private IPv4"
                } else if v4.is_link_local() {
                    "link-local IPv4"
                } else if v4.is_multicast() {
                    "multicast IPv4"
                } else if v4.is_unspecified() {
                    "unspecified IPv4"
                } else {
                    "public IPv4"
                }
            }
            std::net::IpAddr::V6(v6) => {
                if v6.is_loopback() {
                    "loopback IPv6"
                } else if v6.is_multicast() {
                    "multicast IPv6"
                } else if v6.is_unspecified() {
                    "unspecified IPv6"
                } else {
                    "public IPv6"
                }
            }
        };
        return Err(format!("F-04: IP literal not allowed as federation destination ({kind} {ip})"));
    }
    Ok(())
}

/// The `FederationClient` type.
pub struct FederationClient {
    http_client: Client,
    server_name: String,
    key_rotation_manager: Arc<KeyRotationManager>,
    key_cache: Arc<RwLock<HashMap<String, CachedKeys>>>,
    server_resolution_cache: Arc<RwLock<HashMap<String, CachedResolvedServer>>>,
    /// FED-07: Optional dead letter queue for persisting failed transactions.
    dlq: Option<Arc<dyn DeadLetterQueueApi>>,
}

/// Implementation of [`std`] methods.
impl std::fmt::Debug for FederationClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FederationClient").field("server_name", &self.server_name).finish()
    }
}

/// Implementation of [`FederationClient`] methods.
impl FederationClient {
    /// See [`new`.
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

    /// See [`server_name`.
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
            "X-Matrix origin=\"{}\",destination=\"{}\",key=\"{}\",sig=\"{}\"",
            self.server_name, destination, key_id, sig_b64
        ))
    }

    /// See [`resolve_server`.
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

        // F-04: After resolving the server_name via well-known / fallback, the
        // resulting host may still be an IP literal (e.g. `server_name =
        // "192.168.1.1:8448"` or a delegated `m.server` of `10.0.0.1:443`).
        // Both must be rejected as potential SSRF targets — Matrix federation
        // is expected to use DNS-resolved hostnames, not IP literals, as
        // destination servers.
        if let Err(reason) = validate_federation_host_not_ssrf(&resolved.host) {
            return Err(FederationClientError::ServerBlocked(format!("{} (host={})", reason, resolved.host)));
        }

        {
            let mut cache = self.server_resolution_cache.write().await;
            enforce_cache_bound(&mut cache, |entry| entry.cached_at);
            cache.insert(
                server_name.to_string(),
                CachedResolvedServer { resolved: resolved.clone(), cached_at: std::time::Instant::now() },
            );
        }

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

    /// Signed federation request + metrics.
    ///
    /// This is the single choke point for outbound signed federation traffic
    /// (every `make_join` / `send_join` / `send_transaction` / `get_state` … call
    /// funnels through it), so it is where `federation_requests_total` and
    /// `federation_request_duration_ms` are observed.
    ///
    /// `success` means "the remote answered with a 2xx". A remote 4xx/5xx and a
    /// transport failure both count as a failed exchange and land on
    /// `federation_request_errors_total` — deliberately *not* on
    /// `federation_signature_errors`, which is reserved for signature
    /// verification (see `ServerMetrics::record_federation_request`).
    async fn send_signed_request(
        &self,
        method: &str,
        path: &str,
        destination: &str,
        body: Option<&str>,
    ) -> Result<reqwest::Response, FederationClientError> {
        let started = std::time::Instant::now();
        let result = self.send_signed_request_inner(method, path, destination, body).await;

        if let Some(metrics) = synapse_common::server_metrics::global_server_metrics() {
            let ok = result.as_ref().is_ok_and(|response| response.status().is_success());
            metrics.record_federation_request(started.elapsed().as_secs_f64() * 1000.0, ok);
        }

        result
    }

    async fn send_signed_request_inner(
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
                // MSC4512: the appservice `fed_proxy` endpoint forwards whatever
                // method the appservice chose, including DELETE.
                "DELETE" => self.http_client.delete(&url),
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

        // FED-01/FED-02: 验签与身份匹配通过前不得写入缓存
        self.admit_server_keys(destination, keys).await
    }

    /// Validate a remote key document and, only if it passes, admit it to the cache.
    ///
    /// Splitting validation from transport is what makes the security property
    /// directly testable: the invariant is not "we call a validator" but
    /// "**no unvalidated key ever reaches `key_cache`**". A test can assert the
    /// latter by handing this function a forged document and checking the cache
    /// is still empty — which a network-level test could not do without a live,
    /// TLS-speaking, non-IP-literal federation peer.
    ///
    /// `expected_server` is the cache key, so binding the document's
    /// `server_name` to it here is what stops cache poisoning across identities.
    async fn admit_server_keys(
        &self,
        expected_server: &str,
        keys: ServerKeys,
    ) -> Result<ServerKeys, FederationClientError> {
        validate_remote_server_keys(&keys, expected_server)?;

        {
            let mut cache = self.key_cache.write().await;
            enforce_cache_bound(&mut cache, |entry| entry.cached_at);
            cache.insert(
                expected_server.to_string(),
                CachedKeys { keys: keys.clone(), cached_at: std::time::Instant::now() },
            );
        }

        Ok(keys)
    }

    /// See [`query_server_keys`.
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
        let keys: ServerKeys = self.handle_response(response).await?;

        // S-1: 与 `get_server_keys` 走同一个信任门禁。此前这里直接 `return`，
        // 同一份文档经 `/key/v2/server` 会被拒、经 `/key/v2/query` 却会被接受 ——
        // 校验缺口不该取决于调用的是哪个端点。
        //
        // 期望值取 `server_name` 而非 `destination`：`destination` 是承载请求的
        // 传输对端，`server_name` 才是"我们要的是谁的密钥"。规范允许 `/query`
        // 的响应文档描述 `server_name` 这个身份。
        validate_remote_server_keys(&keys, server_name)?;

        Ok(keys)
    }

    /// See [`get_version`.
    pub async fn get_version(&self, destination: &str) -> Result<VersionResponse, FederationClientError> {
        let path = "/_matrix/federation/v1/version";
        let response = self.send_signed_request("GET", path, destination, None).await?;
        self.handle_response(response).await
    }

    /// See [`send_transaction`.
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

    /// See [`make_join`.
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

    /// See [`send_join`.
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

    /// See [`make_leave`.
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

    /// See [`send_leave`.
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

    /// See [`invite`.
    pub async fn invite(
        &self,
        destination: &str,
        room_id: &str,
        event_id: &str,
        room_version: &str,
        event: &serde_json::Value,
        invite_room_state: &[serde_json::Value],
    ) -> Result<InviteResponse, FederationClientError> {
        let path =
            format!("/_matrix/federation/v2/invite/{}/{}", urlencoding::encode(room_id), urlencoding::encode(event_id));
        // The body is NOT the bare event: the resident server reads
        // `room_version` from it (a v3+ PDU carries neither a version nor an
        // `event_id` of its own), so an unwrapped body is unprocessable
        // upstream (`FederationV2InviteServlet`).
        //
        // `invite_room_state` is the stripped state the invitee renders the
        // invite from. MSC4311 requires it to include `m.room.create`: an empty
        // list makes every conforming receiver log
        // `Stripped state must include m.room.create event` and leaves the
        // invitee without a room version — this used to be hardcoded `[]`.
        let body = serde_json::to_string(&serde_json::json!({
            "event": event,
            "room_version": room_version,
            "invite_room_state": invite_room_state,
        }))
        .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("PUT", &path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    /// See [`get_state`.
    pub async fn get_state(&self, destination: &str, room_id: &str) -> Result<StateResponse, FederationClientError> {
        let path = format!("/_matrix/federation/v1/state/{}", urlencoding::encode(room_id));
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    /// See [`get_state_ids`.
    pub async fn get_state_ids(
        &self,
        destination: &str,
        room_id: &str,
    ) -> Result<StateIdsResponse, FederationClientError> {
        let path = format!("/_matrix/federation/v1/state_ids/{}", urlencoding::encode(room_id));
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    /// See [`backfill`.
    pub async fn backfill(
        &self,
        destination: &str,
        room_id: &str,
        room_version: &str,
        from_event_id: &str,
        limit: u32,
    ) -> Result<BackfillResponse, FederationClientError> {
        // The spec's `from` is a single pagination cursor and `v` the room
        // version; sending event ids under `v` (pre-D-9) made spec-compliant
        // servers 400 the request.
        let path = format!(
            "/_matrix/federation/v1/backfill/{}?v={}&from={}&limit={}",
            urlencoding::encode(room_id),
            urlencoding::encode(room_version),
            urlencoding::encode(from_event_id),
            limit
        );
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    /// See [`get_missing_events`.
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

    /// See [`get_user_devices`.
    pub async fn get_user_devices(
        &self,
        destination: &str,
        user_id: &str,
    ) -> Result<UserDevicesResponse, FederationClientError> {
        let path = format!("/_matrix/federation/v1/user/devices/{}", urlencoding::encode(user_id));
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    /// See [`query_profile`.
    pub async fn query_profile(
        &self,
        destination: &str,
        user_id: &str,
    ) -> Result<ProfileResponse, FederationClientError> {
        let path = format!("/_matrix/federation/v1/query/profile?user_id={}", urlencoding::encode(user_id));
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    /// See [`query_directory`.
    pub async fn query_directory(
        &self,
        destination: &str,
        room_alias: &str,
    ) -> Result<DirectoryResponse, FederationClientError> {
        let path = format!("/_matrix/federation/v1/query/directory?room_alias={}", urlencoding::encode(room_alias));
        let response = self.send_signed_request("GET", &path, destination, None).await?;
        self.handle_response(response).await
    }

    /// See [`claim_keys`.
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

    /// See [`query_keys`.
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

    /// See [`get_account_status`].
    ///
    /// MSC3720 account status over federation. Unstable path only — the MSC has
    /// not been stabilised, matching upstream Synapse. Returns the raw response
    /// body: the domain layer (`AccountStatusService`) parses it and enforces
    /// the MSC's rule that a remote server may only report statuses for users
    /// on its own server.
    pub async fn get_account_status(
        &self,
        destination: &str,
        user_ids: &[String],
    ) -> Result<serde_json::Value, FederationClientError> {
        let path = "/_matrix/federation/unstable/org.matrix.msc3720/account_status";
        let body = serde_json::to_string(&serde_json::json!({ "user_ids": user_ids }))
            .map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response = self.send_signed_request("POST", path, destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    /// See [`timestamp_to_event`.
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

    /// See [`get_public_rooms`.
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

    /// See [`post_public_rooms`].
    pub async fn post_public_rooms(
        &self,
        destination: &str,
        body: &serde_json::Value,
    ) -> Result<serde_json::Value, FederationClientError> {
        let body = serde_json::to_string(body).map_err(|e| FederationClientError::InvalidResponse(e.to_string()))?;
        let response =
            self.send_signed_request("POST", "/_matrix/federation/v1/publicRooms", destination, Some(&body)).await?;
        self.handle_response(response).await
    }

    /// See [`knock_room`.
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

    /// See [`exchange_third_party_invite`.
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

    /// See [`media_download`.
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

    /// See [`media_thumbnail`].
    ///
    /// `animated` **必须透传**：它决定上游生成静图还是动图（WebP）。此前本函数没有这个参数，
    /// 而调用方 `routes/media/download.rs` 却按客户端的 `animated` 设置 `Content-Type` ——
    /// 于是 `animated=true` 的**远端**缩略图是 JPEG 字节却标着 `image/webp`（审计 NEW-01）。
    #[expect(clippy::too_many_arguments, reason = "同 trait 声明：联邦缩略图端点有 5 个查询参数，签名必须一一对应")]
    pub async fn media_thumbnail(
        &self,
        destination: &str,
        server_name: &str,
        media_id: &str,
        width: u32,
        height: u32,
        method: &str,
        animated: bool,
    ) -> Result<reqwest::Response, FederationClientError> {
        let path = media_thumbnail_path(server_name, media_id, width, height, method, animated);
        self.send_signed_request("GET", &path, destination, None).await
    }

    /// See [`invalidate_key_cache`.
    pub fn invalidate_key_cache(&self, server_name: &str) {
        let cache = self.key_cache.clone();
        let name = server_name.to_string();
        tokio::spawn(async move {
            cache.write().await.remove(&name);
        });
    }

    /// See [`get_cached_key`.
    pub async fn get_cached_key(&self, server_name: &str) -> Option<ServerKeys> {
        let cache = self.key_cache.read().await;
        cache.get(server_name).map(|c| c.keys.clone())
    }

    /// See [`health_check`.
    pub async fn health_check(&self, destination: &str) -> bool {
        self.get_version(destination).await.is_ok()
    }

    /// MSC4512: sign and send an arbitrary federation request on behalf of an
    /// appservice, returning the remote HTTP status code and the decoded JSON
    /// body (which becomes the `content` of the `fed_proxy` response).
    ///
    /// `path` is the full federation path **including** the query string, e.g.
    /// `/_matrix/federation/v1/version?foo=bar`. A non-2xx response from the
    /// remote is *not* an error: the caller relays the status back to the
    /// appservice, so only signing / transport failures surface as `Err`.
    pub async fn proxy_federation_request(
        &self,
        destination: &str,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<(u16, serde_json::Value), FederationClientError> {
        let response = self.send_signed_request(method, path, destination, body).await?;
        let status = response.status().as_u16();
        let text = response.text().await.map_err(|e| FederationClientError::Connection(e.to_string()))?;
        let content = if text.trim().is_empty() {
            serde_json::Value::Object(serde_json::Map::new())
        } else {
            match serde_json::from_str(&text) {
                Ok(value) => value,
                Err(_) => serde_json::Value::String(text),
            }
        };
        Ok((status, content))
    }
}

/// 联邦缩略图请求的路径（含 query）。
///
/// 抽成自由函数是为了**能被单测**：`media_thumbnail` 本身要发签名请求，没有远端就没法断言
/// URL。`animated` 是唯一一个"看起来不影响 URL、实际决定上游产物格式"的参数 ——
/// 审计 NEW-01 的缺陷正是它被漏传，所以这里用一条只断言字符串的用例把它钉住。
pub(crate) fn media_thumbnail_path(
    server_name: &str,
    media_id: &str,
    width: u32,
    height: u32,
    method: &str,
    animated: bool,
) -> String {
    let mut path = format!(
        "/_matrix/federation/v1/media/thumbnail/{}/{}?width={}&height={}&method={}",
        urlencoding::encode(server_name),
        urlencoding::encode(media_id),
        width,
        height,
        method
    );
    if animated {
        path.push_str("&animated=true");
    }
    path
}

#[cfg(test)]
mod tests;
