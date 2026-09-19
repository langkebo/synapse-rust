use super::models::*;
use super::storage::MegolmSessionStorage;
use crate::crypto::key_at_rest::KeyAtRest;
use crate::vodozemac_megolm::MegolmVodozemacService;
use std::sync::Arc;
use synapse_cache::CacheManager;
use synapse_common::ApiError;

/// Megolm 服务 — 强制使用 vodozemac 实现（Phase 4 清理后）
///
/// 自研 AES-256-GCM 路径和 MegolmBackend 运行时分支已被移除。
/// `MegolmProvider` 直接封装 `MegolmVodozemacService`，
/// 提供与 Element 客户端完全互操作的 Megolm 加密。
#[derive(Clone)]
pub struct MegolmProvider {
    inner: MegolmVodozemacService,
}

/// Implementation of [`MegolmProvider`] methods.
impl MegolmProvider {
    /// See [`from_env`].
    pub fn from_env(storage: MegolmSessionStorage, cache: Arc<CacheManager>, at_rest: KeyAtRest) -> Self {
        Self { inner: MegolmVodozemacService::new(storage, cache, at_rest) }
    }

    /// See [`create_session`].
    pub async fn create_session(&self, room_id: &str, sender_key: &str) -> Result<MegolmSession, ApiError> {
        self.inner.create_session(room_id, sender_key).await
    }

    /// See [`encrypt`].
    pub async fn encrypt(&self, session_id: &str, plaintext: &[u8]) -> Result<Vec<u8>, ApiError> {
        self.inner.encrypt(session_id, plaintext).await
    }

    /// See [`decrypt`].
    pub async fn decrypt(&self, session_id: &str, ciphertext: &[u8]) -> Result<Vec<u8>, ApiError> {
        self.inner.decrypt(session_id, ciphertext).await
    }

    /// See [`rotate_session`].
    pub async fn rotate_session(&self, session_id: &str) -> Result<(), ApiError> {
        self.inner.rotate_session(session_id).await
    }

    /// See [`share_session`].
    pub async fn share_session(&self, session_id: &str, user_ids: &[String]) -> Result<(), ApiError> {
        self.inner.share_session(session_id, user_ids).await
    }

    /// See [`get_room_sessions`].
    pub async fn get_room_sessions(&self, room_id: &str) -> Result<Vec<MegolmSession>, ApiError> {
        self.inner.get_room_sessions(room_id).await
    }

    /// See [`delete_session`].
    pub async fn delete_session(&self, session_id: &str) -> Result<(), ApiError> {
        self.inner.delete_session(session_id).await
    }

    /// See [`cleanup_expired_sessions`].
    pub async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError> {
        self.inner.cleanup_expired_sessions().await
    }

    /// See [`get_outbound_session`].
    pub async fn get_outbound_session(&self, room_id: &str) -> Result<Option<RoomKeyDistributionData>, ApiError> {
        self.inner.get_outbound_session(room_id).await
    }

    /// See [`get_room_key_distribution`].
    pub async fn get_room_key_distribution(&self, room_id: &str) -> Result<Option<RoomKeyDistributionData>, ApiError> {
        self.inner.get_room_key_distribution(room_id).await
    }
}
