use super::*;
use crate::refresh_token::{
    CreateRefreshTokenRequest, RecordUsageRequest, RefreshToken, RefreshTokenFamily, RefreshTokenRotation,
    RefreshTokenStats, RefreshTokenStoreApi, RefreshTokenUsage,
};
use synapse_common::current_timestamp_millis;

/// In-memory test double for [`RefreshTokenStoreApi`].
#[derive(Clone, Default)]
pub struct InMemoryRefreshTokenStore {
    tokens: Arc<tokio::sync::RwLock<HashMap<i64, RefreshToken>>>,
    token_hash_index: Arc<tokio::sync::RwLock<HashMap<String, i64>>>,
    next_id: Arc<tokio::sync::Mutex<i64>>,
    blacklist: Arc<tokio::sync::RwLock<HashMap<String, i64>>>,
    /// 族的登记表。**必须真的记录**：refresh 的重放检测要先用
    /// `get_family_id(user_id, device_id)` 找到族，再去 `get_rotations(family_id)` 比对
    /// 最新一次轮换。若这里不记录，检测路径在单测里永远走不到
    /// （见 `docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §2.5）。
    families: Arc<tokio::sync::RwLock<Vec<RefreshTokenFamily>>>,
    /// `family_id → 轮换序列`（未排序，读取时按 `rotated_ts DESC` 排）。
    rotations: Arc<tokio::sync::RwLock<HashMap<String, Vec<RefreshTokenRotation>>>>,
}

impl InMemoryRefreshTokenStore {
    /// See [`new`].
    pub fn new() -> Self {
        Self::default()
    }

    /// 测试辅助：把该族的轮换时间戳整体往前挪 `by_ms`，用于模拟
    /// "重放发生在并发窗口之外"（窗口内属良性并发，不撤族）。
    pub async fn age_rotations(&self, family_id: &str, by_ms: i64) {
        let mut rotations = self.rotations.write().await;
        if let Some(rows) = rotations.get_mut(family_id) {
            for row in rows.iter_mut() {
                row.rotated_ts -= by_ms;
            }
        }
    }

    /// See [`seed_token`].
    pub async fn seed_token(&self, user_id: &str, token_id: i64, token_hash: &str, device_id: Option<&str>) {
        let mut tokens = self.tokens.write().await;
        tokens.insert(
            token_id,
            RefreshToken {
                id: token_id,
                token_hash: token_hash.to_string(),
                user_id: user_id.to_string(),
                device_id: device_id.map(|d| d.to_string()),
                access_token_id: None,
                scope: None,
                created_ts: 1_700_000_000_000,
                expires_at: None,
                last_used_ts: None,
                use_count: 0,
                is_revoked: false,
                revoked_reason: None,
                client_info: None,
                ip_address: None,
                user_agent: None,
            },
        );
        self.token_hash_index.write().await.insert(token_hash.to_string(), token_id);
    }

    async fn next_id(&self) -> i64 {
        let mut guard = self.next_id.lock().await;
        *guard += 1;
        *guard
    }
}

#[async_trait::async_trait]
impl RefreshTokenStoreApi for InMemoryRefreshTokenStore {
    async fn get_user_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error> {
        let tokens = self.tokens.read().await;
        Ok(tokens.values().filter(|t| t.user_id == user_id).cloned().collect())
    }

    async fn get_token_by_id(&self, id: i64) -> Result<Option<RefreshToken>, sqlx::Error> {
        Ok(self.tokens.read().await.get(&id).cloned())
    }

    async fn delete_token(&self, token_hash: &str) -> Result<(), sqlx::Error> {
        if let Some(id) = self.token_hash_index.read().await.get(token_hash) {
            self.tokens.write().await.remove(id);
        }
        Ok(())
    }

    async fn create_token(&self, request: CreateRefreshTokenRequest) -> Result<RefreshToken, sqlx::Error> {
        let id = self.next_id().await;
        let now = current_timestamp_millis();
        let token = RefreshToken {
            id,
            token_hash: request.token_hash.clone(),
            user_id: request.user_id.clone(),
            device_id: request.device_id.clone(),
            access_token_id: request.access_token_id.clone(),
            scope: request.scope.clone(),
            created_ts: now,
            expires_at: Some(request.expires_at),
            last_used_ts: None,
            use_count: 0,
            is_revoked: false,
            revoked_reason: None,
            client_info: request.client_info.clone(),
            ip_address: request.ip_address.clone(),
            user_agent: request.user_agent.clone(),
        };
        self.token_hash_index.write().await.insert(token.token_hash.clone(), id);
        self.tokens.write().await.insert(id, token.clone());
        Ok(token)
    }

    async fn get_token(&self, token_hash: &str) -> Result<Option<RefreshToken>, sqlx::Error> {
        let index = self.token_hash_index.read().await;
        if let Some(id) = index.get(token_hash) {
            Ok(self.tokens.read().await.get(id).cloned())
        } else {
            Ok(None)
        }
    }

    async fn get_active_tokens(&self, user_id: &str) -> Result<Vec<RefreshToken>, sqlx::Error> {
        let now = current_timestamp_millis();
        let tokens = self.tokens.read().await;
        Ok(tokens
            .values()
            .filter(|t| t.user_id == user_id && !t.is_revoked && t.expires_at.is_none_or(|exp| exp > now))
            .cloned()
            .collect())
    }

    async fn revoke_token(&self, token_hash: &str, reason: &str) -> Result<(), sqlx::Error> {
        let index = self.token_hash_index.read().await;
        if let Some(id) = index.get(token_hash) {
            if let Some(token) = self.tokens.write().await.get_mut(id) {
                token.is_revoked = true;
                token.revoked_reason = Some(reason.to_string());
            }
        }
        Ok(())
    }

    async fn revoke_token_cas(&self, token_hash: &str, reason: &str) -> Result<bool, sqlx::Error> {
        let index = self.token_hash_index.read().await;
        if let Some(id) = index.get(token_hash) {
            if let Some(token) = self.tokens.write().await.get_mut(id) {
                if token.is_revoked {
                    return Ok(false);
                }
                token.is_revoked = true;
                token.revoked_reason = Some(reason.to_string());
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn revoke_token_by_id(&self, id: i64, reason: &str) -> Result<(), sqlx::Error> {
        if let Some(token) = self.tokens.write().await.get_mut(&id) {
            token.is_revoked = true;
            token.revoked_reason = Some(reason.to_string());
        }
        Ok(())
    }

    async fn revoke_all_user_tokens(&self, user_id: &str, reason: &str) -> Result<i64, sqlx::Error> {
        let mut tokens = self.tokens.write().await;
        let mut count = 0i64;
        for token in tokens.values_mut() {
            if token.user_id == user_id && !token.is_revoked {
                token.is_revoked = true;
                token.revoked_reason = Some(reason.to_string());
                count += 1;
            }
        }
        Ok(count)
    }

    async fn record_usage(&self, _request: &RecordUsageRequest) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn create_family(
        &self,
        family_id: &str,
        user_id: &str,
        device_id: Option<&str>,
    ) -> Result<RefreshTokenFamily, sqlx::Error> {
        let now = current_timestamp_millis();
        let family = RefreshTokenFamily {
            id: self.next_id().await,
            family_id: family_id.to_string(),
            user_id: user_id.to_string(),
            device_id: device_id.map(|d| d.to_string()),
            created_ts: now,
            last_refresh_ts: None,
            refresh_count: 0,
            is_compromised: false,
            compromised_ts: None,
        };
        // 真存：`get_family_id` 要靠它找到族，否则重放检测在单测里走不到。
        self.families.write().await.push(family.clone());
        Ok(family)
    }

    async fn mark_family_compromised(&self, _family_id: &str) -> Result<(), sqlx::Error> {
        Ok(())
    }

    async fn record_rotation(
        &self,
        family_id: &str,
        old_token_hash: Option<&str>,
        new_token_hash: &str,
        reason: &str,
    ) -> Result<(), sqlx::Error> {
        let rotation = RefreshTokenRotation {
            id: self.next_id().await,
            family_id: family_id.to_string(),
            old_token_hash: old_token_hash.map(str::to_string),
            new_token_hash: new_token_hash.to_string(),
            rotated_ts: current_timestamp_millis(),
            rotation_reason: Some(reason.to_string()),
        };
        self.rotations.write().await.entry(family_id.to_string()).or_default().push(rotation);
        Ok(())
    }

    async fn rotate_token_atomic(
        &self,
        old_token_hash: &str,
        revoke_reason: &str,
        new_token: CreateRefreshTokenRequest,
        family_id: &str,
        rotation_reason: &str,
    ) -> Result<Option<RefreshToken>, sqlx::Error> {
        // CAS: revoke the old token only if it is still active. Mirror the
        // SQL path's `WHERE token_hash = $1 AND is_revoked = FALSE`.
        let old_id = match self.token_hash_index.read().await.get(old_token_hash).copied() {
            Some(id) => id,
            None => return Ok(None),
        };
        {
            let mut tokens = self.tokens.write().await;
            match tokens.get_mut(&old_id) {
                Some(token) if !token.is_revoked => {
                    token.is_revoked = true;
                    token.revoked_reason = Some(revoke_reason.to_string());
                }
                // Already revoked (or vanished) → CAS miss.
                _ => return Ok(None),
            }
        }
        // Insert the new token (mirrors `insert_refresh_token`).
        let new_token_hash = new_token.token_hash.clone();
        let token = RefreshTokenStoreApi::create_token(self, new_token).await?;
        // 与真实实现一致：CAS 成功即写一条轮换记录（真实实现与 CAS 在同一事务里）。
        RefreshTokenStoreApi::record_rotation(self, family_id, Some(old_token_hash), &new_token_hash, rotation_reason)
            .await?;
        Ok(Some(token))
    }

    async fn get_rotations(&self, family_id: &str) -> Result<Vec<RefreshTokenRotation>, sqlx::Error> {
        let rotations = self.rotations.read().await;
        let mut rows = rotations.get(family_id).cloned().unwrap_or_default();
        rows.sort_by(|a, b| b.rotated_ts.cmp(&a.rotated_ts).then_with(|| b.id.cmp(&a.id)));
        Ok(rows)
    }

    async fn get_family_id(&self, user_id: &str, device_id: Option<&str>) -> Result<Option<String>, sqlx::Error> {
        let families = self.families.read().await;
        Ok(families
            .iter()
            .filter(|family| family.user_id == user_id && family.device_id.as_deref() == device_id)
            .max_by_key(|family| (family.created_ts, family.id))
            .map(|family| family.family_id.clone()))
    }

    async fn add_to_blacklist(
        &self,
        token_hash: &str,
        _token_type: &str,
        _user_id: &str,
        expires_at: i64,
        _reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        self.blacklist.write().await.insert(token_hash.to_string(), expires_at);
        Ok(())
    }

    async fn is_blacklisted(&self, token_hash: &str) -> Result<bool, sqlx::Error> {
        let now = current_timestamp_millis();
        let blacklist = self.blacklist.read().await;
        Ok(blacklist.get(token_hash).is_some_and(|exp| *exp > now))
    }

    async fn cleanup_expired_tokens(&self) -> Result<i64, sqlx::Error> {
        let now = current_timestamp_millis();
        let mut tokens = self.tokens.write().await;
        let mut hash_index = self.token_hash_index.write().await;
        let mut removed = 0i64;
        let expired_ids: Vec<i64> = tokens
            .iter()
            .filter(|(_, t)| !t.is_revoked && t.expires_at.is_some_and(|exp| exp < now))
            .map(|(id, _)| *id)
            .collect();
        for id in expired_ids {
            if let Some(token) = tokens.remove(&id) {
                hash_index.remove(&token.token_hash);
                removed += 1;
            }
        }
        Ok(removed)
    }

    async fn cleanup_blacklist(&self) -> Result<i64, sqlx::Error> {
        let now = current_timestamp_millis();
        let mut blacklist = self.blacklist.write().await;
        let expired: Vec<String> = blacklist.iter().filter(|(_, exp)| **exp < now).map(|(k, _)| k.clone()).collect();
        let count = expired.len() as i64;
        for key in expired {
            blacklist.remove(&key);
        }
        Ok(count)
    }

    async fn get_user_stats(&self, _user_id: &str) -> Result<Option<RefreshTokenStats>, sqlx::Error> {
        Ok(None)
    }

    async fn get_usage_history(&self, _user_id: &str, _limit: i64) -> Result<Vec<RefreshTokenUsage>, sqlx::Error> {
        Ok(Vec::new())
    }

    async fn revoke_device_tokens(&self, user_id: &str, device_id: &str, reason: &str) -> Result<i64, sqlx::Error> {
        let mut tokens = self.tokens.write().await;
        let mut count = 0i64;
        for token in tokens.values_mut() {
            if token.user_id == user_id && token.device_id.as_deref() == Some(device_id) && !token.is_revoked {
                token.is_revoked = true;
                token.revoked_reason = Some(reason.to_string());
                count += 1;
            }
        }
        Ok(count)
    }

    async fn revoke_all_user_tokens_except_device(
        &self,
        user_id: &str,
        device_id: &str,
        reason: &str,
    ) -> Result<i64, sqlx::Error> {
        let mut tokens = self.tokens.write().await;
        let mut count = 0i64;
        for token in tokens.values_mut() {
            if token.user_id == user_id && token.device_id.as_deref() != Some(device_id) && !token.is_revoked {
                token.is_revoked = true;
                token.revoked_reason = Some(reason.to_string());
                count += 1;
            }
        }
        Ok(count)
    }
}
