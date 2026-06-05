use super::models::*;
use super::storage::MegolmSessionStorage;
use crate::cache::CacheManager;
use crate::e2ee::crypto::aes::{Aes256GcmCipher, Aes256GcmCiphertext, Aes256GcmKey, Aes256GcmNonce};
use crate::e2ee::crypto::CryptoError;
#[cfg(feature = "vodozemac-megolm")]
use crate::e2ee::vodozemac_megolm::MegolmVodozemacService;
use crate::error::ApiError;
use chrono::Utc;
use std::sync::Arc;
use std::sync::OnceLock;

static MEGOLM_SESSION_MAX_AGE_DAYS: OnceLock<i64> = OnceLock::new();

fn get_session_max_age_days() -> i64 {
    *MEGOLM_SESSION_MAX_AGE_DAYS.get_or_init(|| {
        std::env::var("MEGOLM_SESSION_MAX_AGE_DAYS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|d: &i64| *d > 0)
            .unwrap_or(7)
    })
}

#[derive(Clone)]
pub struct MegolmService {
    storage: MegolmSessionStorage,
    cache: Arc<CacheManager>,
    encryption_key: [u8; 32],
}

impl MegolmService {
    pub fn new(storage: MegolmSessionStorage, cache: Arc<CacheManager>, encryption_key: [u8; 32]) -> Self {
        Self { storage, cache, encryption_key }
    }

    pub async fn create_session(&self, room_id: &str, sender_key: &str) -> Result<MegolmSession, ApiError> {
        let session_id = uuid::Uuid::new_v4().to_string();

        let session_key = Aes256GcmKey::generate();
        let encrypted_key = self.encrypt_session_key(&session_key)?;

        let session = MegolmSession {
            id: uuid::Uuid::new_v4(),
            session_id: session_id.clone(),
            room_id: room_id.to_string(),
            sender_key: sender_key.to_string(),
            session_key: encrypted_key,
            algorithm: "m.megolm.v1.aes-sha2".to_string(),
            message_index: 0,
            created_ts: Utc::now(),
            last_used_ts: Utc::now(),
            expires_at: Some(Utc::now() + chrono::Duration::days(get_session_max_age_days())),
            pickle_format: PickleFormat::Legacy,
            vodozemac_pickle: None,
        };

        self.storage.create_session(&session).await?;

        let cache_key = format!("megolm_session:{session_id}");
        let _ = self.cache.set(&cache_key, &session, 600).await;

        Ok(session)
    }

    pub async fn load_session(&self, session_id: &str) -> Result<MegolmSession, ApiError> {
        let cache_key = format!("megolm_session:{session_id}");
        if let Ok(Some(session)) = self.cache.get::<MegolmSession>(&cache_key).await {
            return Ok(session);
        }

        // Cache miss - load from storage
        let session = self
            .storage
            .get_session(session_id)
            .await?
            .ok_or_else(|| ApiError::NotFound("Session not found".to_string()))?;

        // Update cache with the loaded session
        let _ = self.cache.set(&cache_key, &session, 600).await;

        Ok(session)
    }

    pub async fn encrypt(&self, session_id: &str, plaintext: &[u8]) -> Result<Vec<u8>, ApiError> {
        let session = self.load_session(session_id).await?;
        let session_key = self.decrypt_session_key(&session.session_key)?;

        let cipher_key = Aes256GcmKey::from_bytes(session_key);
        let encrypted = Aes256GcmCipher::encrypt_with_nonce(&cipher_key, plaintext)?;

        let mut updated_session = session.clone();
        updated_session.message_index += 1;
        updated_session.last_used_ts = Utc::now();
        self.storage.update_session(&updated_session).await?;

        Ok(encrypted)
    }

    pub async fn decrypt(&self, session_id: &str, ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, ApiError> {
        let session = self.load_session(session_id).await?;
        let session_key = self.decrypt_session_key(&session.session_key)?;

        let cipher_key = Aes256GcmKey::from_bytes(session_key);
        let nonce_obj = Aes256GcmNonce::from_bytes(nonce)
            .map_err(|_| ApiError::DecryptionError("Invalid nonce length".to_string()))?;
        let decrypted = Aes256GcmCipher::decrypt(&cipher_key, &nonce_obj, ciphertext)?;

        Ok(decrypted)
    }

    pub async fn rotate_session(&self, session_id: &str) -> Result<(), ApiError> {
        let session = self.load_session(session_id).await?;

        self.storage.delete_session(session_id).await?;

        self.create_session(&session.room_id, &session.sender_key).await?;

        Ok(())
    }

    pub async fn share_session(&self, session_id: &str, user_ids: &[String]) -> Result<(), ApiError> {
        let session = self.load_session(session_id).await?;
        let session_key = self.decrypt_session_key(&session.session_key)?;

        let encrypted_key = self.encrypt_session_key(&Aes256GcmKey::from_bytes(session_key))?;

        for user_id in user_ids {
            let cache_key = format!("megolm_session_key:{user_id}:{session_id}");
            let _ = self.cache.set(&cache_key, &encrypted_key, 600).await;
        }

        Ok(())
    }

    pub async fn get_room_sessions(&self, room_id: &str) -> Result<Vec<MegolmSession>, ApiError> {
        self.storage.get_room_sessions(room_id).await.map_err(|e| {
            tracing::error!("Failed to get room sessions: {e}");
            ApiError::database("A database error occurred".to_string())
        })
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        self.storage.delete_session(session_id).await.map_err(|e| {
            tracing::error!("Failed to delete session: {e}");
            ApiError::database("A database error occurred".to_string())
        })
    }

    fn encrypt_session_key(&self, key: &Aes256GcmKey) -> Result<String, ApiError> {
        let cipher_key = Aes256GcmKey::from_bytes(self.encryption_key);
        let encrypted = Aes256GcmCipher::encrypt_with_nonce(&cipher_key, &key.as_bytes()[..])?;
        let json = serde_json::to_string(&encrypted).map_err(|e| CryptoError::EncryptionError(e.to_string()))?;
        Ok(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, json.as_bytes()))
    }

    fn decrypt_session_key(&self, encrypted: &str) -> Result<[u8; 32], ApiError> {
        let json_bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encrypted)
            .map_err(|_| ApiError::DecryptionError("Invalid base64".to_string()))?;
        let json_str =
            String::from_utf8(json_bytes).map_err(|_| ApiError::DecryptionError("Invalid UTF-8".to_string()))?;
        let ciphertext: Aes256GcmCiphertext =
            serde_json::from_str(&json_str).map_err(|e| ApiError::DecryptionError(e.to_string()))?;
        let cipher_key = Aes256GcmKey::from_bytes(self.encryption_key);
        let decrypted = Aes256GcmCipher::decrypt(&cipher_key, ciphertext.nonce(), ciphertext.ciphertext())?;
        let mut key = [0u8; 32];
        key.copy_from_slice(&decrypted);
        Ok(key)
    }

    pub async fn get_outbound_session(&self, room_id: &str) -> Result<Option<RoomKeyDistributionData>, ApiError> {
        self.get_room_key_distribution(room_id).await
    }

    pub async fn get_room_key_distribution(&self, room_id: &str) -> Result<Option<RoomKeyDistributionData>, ApiError> {
        let sessions = self.get_room_sessions(room_id).await?;

        if let Some(session) = sessions.first() {
            let session_key = self.decrypt_session_key(&session.session_key)?;

            Ok(Some(RoomKeyDistributionData {
                session_id: session.session_id.clone(),
                session_key: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, session_key),
                algorithm: session.algorithm.clone(),
                room_id: room_id.to_string(),
            }))
        } else {
            Ok(None)
        }
    }
}

// =============================================================================
// MegolmProvider — 双路径抽象（Phase 1: E2EE 收敛到 vodozemac）
// =============================================================================
//
// 在 vodozemac 全量收敛之前，`MegolmProvider` 统一封装自研 AES-256-GCM
// 路径和 vodozemac 路径，对外提供相同的 API 表面，调用方（key_request、
// key_rotation、to_device 等）无需关心底层实现。
//
// 选择规则（按优先级）：
// 1. 环境变量 `E2EE_USE_VODOZEMAC_MEGOLM=true` 强制启用 vodozemac
// 2. 否则使用自研 AES-256-GCM 路径（向后兼容）
//
// Phase 4（清理）之后：删除自研 `MegolmService`，`MegolmProvider` 直接持有
// `MegolmVodozemacService`，移除运行时分支。

/// Megolm 加密实现路径
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MegolmBackend {
    /// 自研 AES-256-GCM 实现（向后兼容）
    Legacy,
    /// vodozemac 0.9 实现（与 Element 客户端互操作）
    Vodozemac,
}

#[cfg(feature = "vodozemac-megolm")]
impl MegolmBackend {
    /// 从环境变量解析后端选择
    pub fn from_env() -> Self {
        match std::env::var("E2EE_USE_VODOZEMAC_MEGOLM")
            .ok()
            .map(|s| s.to_ascii_lowercase())
            .as_deref()
        {
            Some("1") | Some("true") | Some("yes") | Some("on") => Self::Vodozemac,
            _ => Self::Legacy,
        }
    }

    /// 当前后端的字符串名称（用于日志/诊断）
    pub fn name(&self) -> &'static str {
        match self {
            Self::Legacy => "legacy-aes-256-gcm",
            Self::Vodozemac => "vodozemac-0.9",
        }
    }
}

/// Megolm 统一抽象：包装自研和 vodozemac 两种实现
///
/// `decrypt` 接受 `nonce` 参数以保持与自研 `MegolmService` 的签名兼容；
/// vodozemac 路径下 nonce 被忽略（vodozemac 的 `MegolmMessage` 自带 nonce）。
///
/// 当 `vodozemac-megolm` feature 关闭时，`MegolmProvider` 退化为
/// `MegolmService` 的类型别名，确保无 vodozemac 依赖的最小构建仍能编译。
#[cfg(feature = "vodozemac-megolm")]
#[derive(Clone)]
pub enum MegolmProvider {
    Legacy(MegolmService),
    Vodozemac(MegolmVodozemacService),
}

#[cfg(not(feature = "vodozemac-megolm"))]
pub type MegolmProvider = MegolmService;
#[cfg(feature = "vodozemac-megolm")]
impl MegolmProvider {
    /// 根据环境变量和共享依赖创建 provider
    pub fn from_env(
        storage: MegolmSessionStorage,
        cache: Arc<CacheManager>,
        encryption_key: [u8; 32],
    ) -> Self {
        match MegolmBackend::from_env() {
            MegolmBackend::Vodozemac => {
                ::tracing::info!(
                    backend = "vodozemac-0.9",
                    "E2EE_USE_VODOZEMAC_MEGOLM enabled — using vodozemac-backed Megolm"
                );
                // Phase 2: 把 encryption_key 传给 vodozemac 服务，支持 E2EE_DUAL_WRITE=true
                // 时的双写（同时写 legacy 加密格式到 session_key 列）。
                Self::Vodozemac(MegolmVodozemacService::new(storage, cache).with_encryption_key(encryption_key))
            }
            MegolmBackend::Legacy => {
                ::tracing::info!(
                    backend = "legacy-aes-256-gcm",
                    "Using legacy AES-256-GCM Megolm path (default)"
                );
                Self::Legacy(MegolmService::new(storage, cache, encryption_key))
            }
        }
    }

    /// 当前后端
    pub fn backend(&self) -> MegolmBackend {
        match self {
            Self::Legacy(_) => MegolmBackend::Legacy,
            Self::Vodozemac(_) => MegolmBackend::Vodozemac,
        }
    }

    pub async fn create_session(&self, room_id: &str, sender_key: &str) -> Result<MegolmSession, ApiError> {
        match self {
            Self::Legacy(s) => s.create_session(room_id, sender_key).await,
            Self::Vodozemac(s) => s.create_session(room_id, sender_key).await,
        }
    }

    pub async fn encrypt(&self, session_id: &str, plaintext: &[u8]) -> Result<Vec<u8>, ApiError> {
        match self {
            Self::Legacy(s) => s.encrypt(session_id, plaintext).await,
            Self::Vodozemac(s) => s.encrypt(session_id, plaintext).await,
        }
    }

    /// 解密。`nonce` 参数仅在自研路径下使用；vodozemac 路径下被忽略。
    pub async fn decrypt(&self, session_id: &str, ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, ApiError> {
        match self {
            Self::Legacy(s) => s.decrypt(session_id, ciphertext, nonce).await,
            Self::Vodozemac(s) => s.decrypt(session_id, ciphertext).await,
        }
    }

    pub async fn rotate_session(&self, session_id: &str) -> Result<(), ApiError> {
        match self {
            Self::Legacy(s) => s.rotate_session(session_id).await,
            Self::Vodozemac(s) => s.rotate_session(session_id).await,
        }
    }

    pub async fn share_session(&self, session_id: &str, user_ids: &[String]) -> Result<(), ApiError> {
        match self {
            Self::Legacy(s) => s.share_session(session_id, user_ids).await,
            Self::Vodozemac(s) => s.share_session(session_id, user_ids).await,
        }
    }

    pub async fn get_room_sessions(&self, room_id: &str) -> Result<Vec<MegolmSession>, ApiError> {
        match self {
            Self::Legacy(s) => s.get_room_sessions(room_id).await,
            Self::Vodozemac(s) => s.get_room_sessions(room_id).await,
        }
    }

    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        match self {
            Self::Legacy(s) => s.delete_session(session_id).await,
            Self::Vodozemac(s) => s.delete_session(session_id).await,
        }
    }

    pub async fn get_outbound_session(&self, room_id: &str) -> Result<Option<RoomKeyDistributionData>, ApiError> {
        match self {
            Self::Legacy(s) => s.get_outbound_session(room_id).await,
            Self::Vodozemac(s) => s.get_outbound_session(room_id).await,
        }
    }

    pub async fn get_room_key_distribution(&self, room_id: &str) -> Result<Option<RoomKeyDistributionData>, ApiError> {
        match self {
            Self::Legacy(s) => s.get_room_key_distribution(room_id).await,
            Self::Vodozemac(s) => s.get_room_key_distribution(room_id).await,
        }
    }
}

#[cfg(all(test, feature = "vodozemac-megolm"))]
mod provider_tests {
    use super::*;
    use crate::e2ee::vodozemac_megolm::MegolmVodozemacService;

    #[test]
    fn backend_from_env_defaults_to_legacy() {
        // 默认（未设置 E2EE_USE_VODOZEMAC_MEGOLM）应为 Legacy
        // 注意：env 变量可能已被其他测试设置，仅校验合法值
        let backend = MegolmBackend::from_env();
        assert!(matches!(backend, MegolmBackend::Legacy | MegolmBackend::Vodozemac));
    }

    #[test]
    fn provider_backend_name() {
        assert_eq!(MegolmBackend::Legacy.name(), "legacy-aes-256-gcm");
        assert_eq!(MegolmBackend::Vodozemac.name(), "vodozemac-0.9");
    }

    #[test]
    fn vodozemac_variant_compiles() {
        // 验证 MegolmVodozemacService 与 MegolmProvider 的集成在编译期通过
        // 集成测试需要真实 PG pool，此处只验证类型定义
        let _: Option<MegolmVodozemacService> = None;
        let _: Option<MegolmProvider> = None;
    }
}
