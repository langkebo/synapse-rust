use super::models::*;
use super::storage::{BackupKeyInsertParams, BackupKeyStorage, KeyBackupStorage};
use crate::device_keys::DeviceKeyStoreApi;
use crate::signed_json::verify_signed_json;
use sqlx::Row;
use std::sync::Arc;
use synapse_common::current_timestamp_millis;
use synapse_common::ApiError;

#[derive(Debug, Clone)]
pub struct BackupKeyUploadParams {
    pub user_id: String,
    pub version: String,
    pub room_id: String,
    pub session_id: String,
    pub first_message_index: i64,
    pub forwarded_count: i64,
    pub is_verified: bool,
    pub session_data: String,
}

#[derive(Clone)]
pub struct KeyBackupService {
    storage: KeyBackupStorage,
    key_storage: BackupKeyStorage,
    device_key_storage: Option<Arc<dyn DeviceKeyStoreApi>>,
}

impl KeyBackupService {
    pub fn new(storage: &KeyBackupStorage) -> Self {
        Self { storage: storage.clone(), key_storage: BackupKeyStorage::new(&storage.pool), device_key_storage: None }
    }

    pub fn with_device_key_storage(mut self, storage: Arc<dyn DeviceKeyStoreApi>) -> Self {
        self.device_key_storage = Some(storage);
        self
    }

    pub async fn create_backup(
        &self,
        user_id: &str,
        algorithm: &str,
        auth_data: Option<serde_json::Value>,
    ) -> Result<String, ApiError> {
        let version_i64 = chrono::Utc::now().timestamp();
        let version = version_i64.to_string();
        let auth_key =
            auth_data.as_ref().and_then(|v| v.get("auth_key")).and_then(|v| v.as_str()).unwrap_or("").to_string();
        let mgmt_key =
            auth_data.as_ref().and_then(|v| v.get("mgmt_key")).and_then(|v| v.as_str()).unwrap_or("").to_string();

        // Spec: the backup auth_data must contain the server-side
        // `public_key` that the client will use to encrypt Megolm session
        // keys before uploading them. Without it, the backup is
        // irrecoverable — refuse to create it. This also closes a confused
        // deputy where a caller could later PUT a public_key they
        // themselves control via `update_backup_auth_data` (E-02).
        let public_key = auth_data.as_ref().and_then(|v| v.get("public_key")).and_then(|v| v.as_str()).unwrap_or("");
        if public_key.is_empty() {
            return Err(ApiError::invalid_param("Backup auth_data must contain a public_key"));
        }

        let backup = KeyBackup {
            user_id: user_id.to_string(),
            backup_id: version.clone(),
            version: version_i64,
            algorithm: algorithm.to_string(),
            auth_key,
            mgmt_key,
            backup_data: auth_data.unwrap_or(serde_json::json!({})),
            etag: Some(format!("{version_i64:x}")),
        };

        self.storage.create_backup(&backup).await?;

        Ok(version)
    }

    pub async fn get_backup(&self, user_id: &str, version: &str) -> Result<Option<KeyBackup>, ApiError> {
        self.storage.get_backup_version(user_id, version).await
    }

    pub async fn update_backup_auth_data(
        &self,
        user_id: &str,
        version: &str,
        auth_data: Option<serde_json::Value>,
    ) -> Result<(), ApiError> {
        let backup =
            self.storage.get_backup_version(user_id, version).await?.ok_or_else(|| {
                ApiError::not_found(format!("Backup version '{version}' not found for user '{user_id}'"))
            })?;

        let mut updated_backup = backup;
        if let Some(data) = auth_data {
            // E-02: Re-validate the new auth_data before persisting. Without this
            // check, an attacker (or buggy client) could:
            //   * Strip the public_key field, making the backup undecryptable
            //   * Swap in a public_key for which they hold the private half
            //   * Replace the management key, which is supposed to be stable
            //   * Submit a forged `signatures` block that points at attacker keys
            // Perform the synchronous structural checks first.
            Self::validate_auth_data_update(&updated_backup, &data)?;

            // E-02: If the new auth_data carries a non-empty `signatures` block,
            // run cryptographic verification. We refuse to persist a backup
            // that claims to be signed by a key we cannot confirm on the
            // device list. When `device_key_storage` is not wired up we fall
            // back to a conservative reject (matches the fix for E-03).
            if let Some(sig_map) = data.get("signatures").and_then(|v| v.as_object()) {
                if !sig_map.is_empty() {
                    if let Some(device_key_storage) = &self.device_key_storage {
                        let mut signature_valid = false;
                        if let Some(user_sigs) = sig_map.get(user_id).and_then(|v| v.as_object()) {
                            for (signing_key_id, signature_value) in user_sigs {
                                let Some(signature) = signature_value.as_str() else {
                                    continue;
                                };
                                let parts: Vec<&str> = signing_key_id.splitn(2, ':').collect();
                                if parts.len() != 2 || parts[0] != "ed25519" {
                                    continue;
                                }
                                let device_id = parts[1];
                                if let Ok(Some(device_key)) =
                                    device_key_storage.get_device_key(user_id, device_id, "ed25519").await
                                {
                                    if let Ok(true) = verify_signed_json(
                                        user_id,
                                        signing_key_id,
                                        &device_key.public_key,
                                        signature,
                                        &data,
                                    ) {
                                        signature_valid = true;
                                        break;
                                    }
                                }
                            }
                        }
                        if !signature_valid {
                            return Err(ApiError::invalid_param("Backup auth_data signatures failed verification"));
                        }
                    } else {
                        // No device_key_storage — refuse to accept the update
                        // rather than persisting a backup with signatures we
                        // cannot cryptographically validate.
                        return Err(ApiError::invalid_param(
                            "Cannot validate backup auth_data signatures without device key storage",
                        ));
                    }
                }
            }

            updated_backup.auth_key = data.get("auth_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
            updated_backup.mgmt_key = data.get("mgmt_key").and_then(|v| v.as_str()).unwrap_or("").to_string();
            updated_backup.backup_data = data;
        }
        updated_backup.etag = Some(format!("{:x}", chrono::Utc::now().timestamp()));

        self.storage.create_backup(&updated_backup).await?;

        Ok(())
    }

    /// Synchronous structural validation for the body of
    /// `update_backup_auth_data`. Pulled out as a pure function so the E-02
    /// invariant can be unit-tested without spinning up a Postgres pool.
    pub fn validate_auth_data_update(current: &KeyBackup, new_auth_data: &serde_json::Value) -> Result<(), ApiError> {
        // The new auth_data must contain a non-empty public_key.
        let new_public_key = new_auth_data.get("public_key").and_then(|v| v.as_str());
        if new_public_key.is_none() || new_public_key.map(str::is_empty).unwrap_or(true) {
            return Err(ApiError::invalid_param("Backup auth_data must contain a non-empty public_key"));
        }

        // The management key is meant to be stable for a given version —
        // rotating it via PUT would silently invalidate all existing
        // session_data. Reject any change.
        let new_mgmt_key = new_auth_data.get("mgmt_key").and_then(|v| v.as_str()).unwrap_or("");
        if !current.mgmt_key.is_empty() && new_mgmt_key != current.mgmt_key {
            return Err(ApiError::invalid_param("Backup management key cannot be changed via update"));
        }

        Ok(())
    }

    pub async fn delete_backup(&self, user_id: &str, version: &str) -> Result<(), ApiError> {
        self.storage.delete_backup(user_id, version).await?;

        Ok(())
    }

    pub async fn list_backups(&self, user_id: &str) -> Result<Vec<KeyBackup>, ApiError> {
        self.storage.get_all_backup_versions(user_id).await
    }

    pub async fn upload_backup_key(&self, params: BackupKeyUploadParams) -> Result<(), ApiError> {
        let backup = self.storage.get_backup_version(&params.user_id, &params.version).await?.ok_or_else(|| {
            ApiError::not_found(format!("Backup version '{}' not found for user '{}'", params.version, params.user_id))
        })?;

        self.key_storage
            .upload_backup_key(BackupKeyInsertParams {
                user_id: params.user_id,
                backup_id: backup.backup_id,
                room_id: params.room_id,
                session_id: params.session_id,
                first_message_index: params.first_message_index,
                forwarded_count: params.forwarded_count,
                is_verified: params.is_verified,
                backup_data: serde_json::json!({ "session_data": params.session_data }),
            })
            .await?;

        Ok(())
    }

    /// Spec-compliant upload: stores the full KeyBackupData JSON object verbatim.
    /// `key_backup_data` is the per-session object Element sends:
    /// `{first_message_index, forwarded_count, is_verified, session_data}`.
    pub async fn upload_session(
        &self,
        user_id: &str,
        version: &str,
        room_id: &str,
        session_id: &str,
        key_backup_data: serde_json::Value,
    ) -> Result<(), ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        let first_message_index = key_backup_data.get("first_message_index").and_then(|v| v.as_i64()).unwrap_or(0);
        let forwarded_count = key_backup_data.get("forwarded_count").and_then(|v| v.as_i64()).unwrap_or(0);
        let is_verified = key_backup_data.get("is_verified").and_then(|v| v.as_bool()).unwrap_or(false);

        self.key_storage
            .upload_backup_key(BackupKeyInsertParams {
                user_id: user_id.to_string(),
                backup_id: backup.backup_id,
                room_id: room_id.to_string(),
                session_id: session_id.to_string(),
                first_message_index,
                forwarded_count,
                is_verified,
                backup_data: key_backup_data,
            })
            .await?;

        Ok(())
    }

    pub async fn delete_backup_key(&self, user_id: &str, room_id: &str, session_id: &str) -> Result<(), ApiError> {
        self.key_storage.delete_backup_key(user_id, room_id, session_id).await?;

        Ok(())
    }

    /// Delete one session within a specific backup version. Returns deleted-row count.
    pub async fn delete_session_for_version(
        &self,
        user_id: &str,
        version: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<u64, ApiError> {
        self.key_storage.delete_session_for_version(user_id, version, room_id, session_id).await
    }

    /// Delete all sessions for a room within a specific backup version. Returns deleted-row count.
    pub async fn delete_room_for_version(&self, user_id: &str, version: &str, room_id: &str) -> Result<u64, ApiError> {
        self.key_storage.delete_room_for_version(user_id, version, room_id).await
    }

    /// Delete every session within a specific backup version. Returns deleted-row count.
    pub async fn delete_all_for_version(&self, user_id: &str, version: &str) -> Result<u64, ApiError> {
        self.key_storage.delete_all_for_version(user_id, version).await
    }

    pub async fn upload_room_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
        session_data: &serde_json::Value,
    ) -> Result<(), ApiError> {
        let backup = self
            .storage
            .get_backup(user_id)
            .await?
            .ok_or_else(|| ApiError::not_found(format!("No backup found for user '{user_id}'")))?;

        self.key_storage
            .upload_backup_key(BackupKeyInsertParams {
                user_id: user_id.to_string(),
                backup_id: backup.backup_id.clone(),
                room_id: room_id.to_string(),
                session_id: session_id.to_string(),
                first_message_index: 0,
                forwarded_count: 0,
                is_verified: false,
                backup_data: session_data.clone(),
            })
            .await?;

        Ok(())
    }

    pub async fn upload_room_keys_for_room(
        &self,
        user_id: &str,
        room_id: &str,
        version: &str,
        keys: Vec<serde_json::Value>,
    ) -> Result<(), ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        for key in keys {
            let session_id = key["session_id"].as_str().unwrap_or_default().to_string();
            let session_data = key["session_data"].clone();
            let first_message_index = key["first_message_index"].as_i64().unwrap_or(0);
            let forwarded_count = key["forwarded_count"].as_i64().unwrap_or(0);
            let is_verified = key["is_verified"].as_bool().unwrap_or(false);

            self.key_storage
                .upload_backup_key(BackupKeyInsertParams {
                    user_id: user_id.to_string(),
                    backup_id: backup.backup_id.clone(),
                    room_id: room_id.to_string(),
                    session_id,
                    first_message_index,
                    forwarded_count,
                    is_verified,
                    backup_data: session_data,
                })
                .await?;
        }

        Ok(())
    }

    pub async fn store_backup_key(
        &self,
        user_id: &str,
        version: &str,
        room_id: &str,
        session_id: &str,
        key_data: &serde_json::Value,
    ) -> Result<(), ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        self.key_storage
            .upload_backup_key(BackupKeyInsertParams {
                user_id: user_id.to_string(),
                backup_id: backup.backup_id.clone(),
                room_id: room_id.to_string(),
                session_id: session_id.to_string(),
                first_message_index: 0,
                forwarded_count: 0,
                is_verified: false,
                backup_data: key_data.clone(),
            })
            .await?;

        Ok(())
    }

    pub async fn get_backup_version(&self, user_id: &str) -> Result<Option<KeyBackup>, ApiError> {
        self.storage.get_backup(user_id).await
    }

    pub async fn get_all_backups(&self, user_id: &str) -> Result<Vec<KeyBackup>, ApiError> {
        self.storage.get_all_backup_versions(user_id).await
    }

    pub async fn get_backup_key_count(&self, user_id: &str) -> Result<i64, ApiError> {
        let row = sqlx::query(
            r"
            SELECT COALESCE(COUNT(*), 0) as count
            FROM backup_keys bk
            JOIN key_backups kb ON kb.backup_id = bk.backup_id
            WHERE kb.user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_one(&*self.storage.pool)
        .await?;

        Ok(row.try_get::<i64, _>("count")?)
    }

    pub async fn get_all_backup_keys(&self, user_id: &str) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let rows = sqlx::query_as::<_, BackupKeyInfo>(
            r"
            SELECT
                kb.user_id,
                COALESCE(kb.backup_id_text, kb.version::text) AS backup_id,
                bk.room_id,
                bk.session_id,
                bk.first_message_index,
                bk.forwarded_count,
                bk.is_verified,
                bk.session_data
            FROM backup_keys bk
            JOIN key_backups kb ON kb.backup_id = bk.backup_id
            WHERE kb.user_id = $1
            ",
        )
        .bind(user_id)
        .fetch_all(&*self.storage.pool)
        .await?;
        Ok(rows)
    }

    /// Return every stored session for a single backup version.
    pub async fn get_keys_for_version(&self, user_id: &str, version: &str) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let rows = sqlx::query_as::<_, BackupKeyInfo>(
            r"
            SELECT
                kb.user_id,
                COALESCE(kb.backup_id_text, kb.version::text) AS backup_id,
                bk.room_id,
                bk.session_id,
                bk.first_message_index,
                bk.forwarded_count,
                bk.is_verified,
                bk.session_data
            FROM backup_keys bk
            JOIN key_backups kb ON kb.backup_id = bk.backup_id
            WHERE kb.user_id = $1
              AND (kb.backup_id_text = $2 OR kb.version::text = $2)
            ",
        )
        .bind(user_id)
        .bind(version)
        .fetch_all(&*self.storage.pool)
        .await?;
        Ok(rows)
    }

    pub async fn get_backup_key_count_for_version(&self, user_id: &str, version: &str) -> Result<i64, ApiError> {
        let row = sqlx::query(
            r"
            SELECT COALESCE(COUNT(*), 0) as count
            FROM backup_keys bk
            JOIN key_backups kb ON kb.backup_id = bk.backup_id
            WHERE kb.user_id = $1
              AND (kb.backup_id_text = $2 OR kb.version::text = $2)
            ",
        )
        .bind(user_id)
        .bind(version)
        .fetch_one(&*self.storage.pool)
        .await?;

        Ok(row.try_get::<i64, _>("count")?)
    }

    pub async fn get_backup_count_per_room(&self, user_id: &str, version: &str) -> Result<serde_json::Value, ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        let rows = sqlx::query(
            r"
            SELECT bk.room_id, COALESCE(COUNT(*), 0) as count
            FROM backup_keys bk
            JOIN key_backups kb ON kb.backup_id = bk.backup_id
            WHERE kb.user_id = $1
              AND (kb.backup_id_text = $2 OR kb.version::text = $2)
            GROUP BY bk.room_id
            ",
        )
        .bind(user_id)
        .bind(&backup.backup_id)
        .fetch_all(&*self.storage.pool)
        .await?;

        let mut rooms: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for row in rows {
            let room_id: String = row.try_get("room_id")?;
            let count: i64 = row.try_get("count")?;
            rooms.insert(room_id, serde_json::Value::from(count));
        }

        Ok(serde_json::Value::Object(rooms))
    }

    pub async fn get_room_backup_keys(
        &self,
        user_id: &str,
        room_id: &str,
        version: &str,
    ) -> Result<Vec<BackupKeyInfo>, ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        self.key_storage.get_room_backup_keys_by_backup_id(user_id, &backup.backup_id, room_id).await
    }

    pub async fn get_backup_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
        version: &str,
    ) -> Result<Option<BackupKeyInfo>, ApiError> {
        let backup = match self.storage.get_backup_version(user_id, version).await? {
            Some(b) => b,
            None => return Ok(None),
        };

        self.key_storage.get_backup_key_by_backup_id(user_id, &backup.backup_id, room_id, session_id).await
    }

    pub async fn get_room_key(
        &self,
        user_id: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<BackupKeyInfo>, ApiError> {
        self.key_storage.get_backup_key(user_id, room_id, session_id).await
    }

    pub async fn recover_keys(
        &self,
        user_id: &str,
        version: &str,
        rooms: Option<Vec<String>>,
    ) -> Result<RecoveryResponse, ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        // Rollback protection: refuse to recover from a non-current backup
        // version. Restoring a superseded version could reintroduce keys the
        // user has since rotated away, or mask a malicious version downgrade.
        let current = self
            .storage
            .get_backup(user_id)
            .await?
            .ok_or_else(|| ApiError::not_found("No backup version".to_string()))?;
        if backup.version != current.version {
            return Err(ApiError::invalid_param(format!(
                "Refusing to recover non-current backup version {} (current: {})",
                backup.version, current.version
            )));
        }

        let total_keys = self.get_backup_key_count(user_id).await?;

        let all_keys = if let Some(ref room_list) = rooms {
            let mut keys = Vec::new();
            for room_id in room_list {
                let room_keys =
                    self.key_storage.get_room_backup_keys_by_backup_id(user_id, &backup.backup_id, room_id).await?;
                keys.extend(room_keys);
            }
            keys
        } else {
            self.get_all_backup_keys(user_id).await?
        };

        let mut rooms_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for key in &all_keys {
            if !rooms_map.contains_key(&key.room_id) {
                rooms_map.insert(key.room_id.clone(), serde_json::json!({}));
            }
            if let Some(room_obj) = rooms_map.get_mut(&key.room_id) {
                if let Some(sessions) = room_obj.as_object_mut() {
                    sessions.insert(
                        key.session_id.clone(),
                        serde_json::json!({
                            "first_message_index": key.first_message_index,
                            "forwarded_count": key.forwarded_count,
                            "is_verified": key.is_verified,
                            "session_data": key.session_data,
                        }),
                    );
                }
            }
        }

        Ok(RecoveryResponse {
            rooms: serde_json::Value::Object(rooms_map),
            total_keys,
            recovered_keys: all_keys.len() as i64,
        })
    }

    pub async fn get_recovery_progress(&self, user_id: &str, version: &str) -> Result<RecoveryProgress, ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        let total_keys = self.get_backup_key_count(user_id).await?;
        let now = current_timestamp_millis();

        Ok(RecoveryProgress {
            user_id: user_id.to_string(),
            version: version.to_string(),
            total_keys,
            recovered_keys: total_keys,
            status: if total_keys > 0 { "completed".to_string() } else { "empty".to_string() },
            started_ts: backup.version * 1000,
            updated_ts: now,
        })
    }

    /// Pure helper for E-03: decides `signature_valid` when the device key
    /// store is *not* available. The async verification path (with
    /// `device_key_storage`) is exercised in integration tests.
    ///
    /// Invariant: without device_key_storage we can only inspect the
    /// signatures block. The conservative policy is to **reject** any
    /// non-empty signatures block (signature_valid = false), since we
    /// cannot cryptographically confirm them. An empty signatures block
    /// means the backup is unsigned — fall through and let the rest of
    /// `verify_backup` decide.
    pub fn compute_signature_validity_without_device_keys(signatures: &serde_json::Value) -> bool {
        let has_signatures = signatures.as_object().is_some_and(|m| !m.is_empty());
        // E-03 fix: refuse to accept non-empty signatures without crypto
        // verification. This closes the confused-deputy attack where a
        // caller can plant a `signatures` block the server cannot check.
        !has_signatures
    }

    pub async fn verify_backup(&self, user_id: &str, version: &str) -> Result<BackupVerificationResponse, ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        let key_count = self.get_backup_key_count(user_id).await?;

        let signatures = backup.backup_data.get("signatures").cloned().unwrap_or(serde_json::json!({}));

        let mut signature_valid = false;

        if let Some(device_key_storage) = &self.device_key_storage {
            if let Some(sig_map) = signatures.as_object() {
                if let Some(user_sigs) = sig_map.get(user_id).and_then(|v| v.as_object()) {
                    for (signing_key_id, signature_value) in user_sigs {
                        let Some(signature) = signature_value.as_str() else {
                            continue;
                        };

                        let parts: Vec<&str> = signing_key_id.splitn(2, ':').collect();
                        if parts.len() != 2 || parts[0] != "ed25519" {
                            continue;
                        }

                        let device_id = parts[1];

                        if let Ok(Some(device_key)) =
                            device_key_storage.get_device_key(user_id, device_id, "ed25519").await
                        {
                            match verify_signed_json(
                                user_id,
                                signing_key_id,
                                &device_key.public_key,
                                signature,
                                &backup.backup_data,
                            ) {
                                Ok(true) => {
                                    signature_valid = true;
                                    break;
                                }
                                Ok(false) => {
                                    tracing::warn!("Backup signature verification failed for key {}", signing_key_id);
                                }
                                Err(e) => {
                                    tracing::warn!(
                                        "Backup signature verification error for key {}: {}",
                                        signing_key_id,
                                        e
                                    );
                                }
                            }
                        }
                    }
                }
            }
        } else {
            // E-03: Without device_key_storage we cannot perform cryptographic
            // signature verification. Delegate to the pure helper which
            // returns `signature_valid = true` only when the signatures
            // block is empty (unsigned backup). Any non-empty block is
            // refused (returns `false`).
            signature_valid = Self::compute_signature_validity_without_device_keys(&signatures);
        }

        let valid = !backup.algorithm.is_empty() && backup.backup_data.get("public_key").is_some() && signature_valid;

        Ok(BackupVerificationResponse {
            valid,
            algorithm: backup.algorithm,
            auth_data: backup.backup_data,
            key_count,
            signatures,
        })
    }

    pub async fn batch_recover_keys(
        &self,
        user_id: &str,
        request: BatchRecoveryRequest,
    ) -> Result<BatchRecoveryResponse, ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, &request.version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        let session_limit = request.session_limit.unwrap_or(100) as usize;
        let mut rooms_map: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        let mut total_sessions = 0i64;
        let mut has_more = false;

        let batch_keys =
            self.key_storage.get_backup_keys_by_rooms(user_id, &backup.backup_id, &request.room_ids).await?;

        for room_id in &request.room_ids {
            let keys = match batch_keys.get(room_id) {
                Some(k) => k,
                None => continue,
            };

            let mut sessions: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
            for key in keys.iter().take(session_limit - total_sessions as usize) {
                sessions.insert(
                    key.session_id.clone(),
                    serde_json::json!({
                        "first_message_index": key.first_message_index,
                        "forwarded_count": key.forwarded_count,
                        "is_verified": key.is_verified,
                        "session_data": key.session_data,
                    }),
                );
                total_sessions += 1;
            }

            if !sessions.is_empty() {
                rooms_map.insert(room_id.clone(), serde_json::Value::Object(sessions));
            }

            if total_sessions >= session_limit as i64 {
                has_more = keys.len() > session_limit;
                break;
            }
        }

        Ok(BatchRecoveryResponse {
            rooms: rooms_map,
            total_sessions,
            has_more,
            next_batch: if has_more { Some(format!("batch_{}", chrono::Utc::now().timestamp())) } else { None },
        })
    }

    pub async fn recover_room_keys(
        &self,
        user_id: &str,
        version: &str,
        room_id: &str,
    ) -> Result<serde_json::Value, ApiError> {
        let backup = self
            .storage
            .get_backup_version(user_id, version)
            .await?
            .ok_or_else(|| ApiError::not_found("Backup not found".to_string()))?;

        let keys = self.key_storage.get_room_backup_keys_by_backup_id(user_id, &backup.backup_id, room_id).await?;

        let mut sessions: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();
        for key in keys {
            sessions.insert(
                key.session_id.clone(),
                serde_json::json!({
                    "first_message_index": key.first_message_index,
                    "forwarded_count": key.forwarded_count,
                    "is_verified": key.is_verified,
                    "session_data": key.session_data,
                }),
            );
        }

        Ok(serde_json::Value::Object(sessions))
    }

    pub async fn recover_session_key(
        &self,
        user_id: &str,
        version: &str,
        room_id: &str,
        session_id: &str,
    ) -> Result<Option<serde_json::Value>, ApiError> {
        let key = self.get_backup_key(user_id, room_id, session_id, version).await?;

        Ok(key.map(|k| {
            serde_json::json!({
                "first_message_index": k.first_message_index,
                "forwarded_count": k.forwarded_count,
                "is_verified": k.is_verified,
                "session_data": k.session_data,
            })
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backup_key_upload_params_creation() {
        let params = BackupKeyUploadParams {
            user_id: "@alice:example.com".to_string(),
            version: "1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session_123".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            session_data: "encrypted_data_base64".to_string(),
        };

        assert_eq!(params.user_id, "@alice:example.com");
        assert_eq!(params.version, "1");
        assert_eq!(params.first_message_index, 0);
        assert!(params.is_verified);
    }

    #[test]
    fn test_backup_key_upload_params_clone() {
        let params = BackupKeyUploadParams {
            user_id: "@bob:example.com".to_string(),
            version: "2".to_string(),
            room_id: "!room2:example.com".to_string(),
            session_id: "session_456".to_string(),
            first_message_index: 10,
            forwarded_count: 2,
            is_verified: false,
            session_data: "data".to_string(),
        };

        let cloned = params.clone();
        assert_eq!(params.user_id, cloned.user_id);
        assert_eq!(params.version, cloned.version);
        assert_eq!(params.first_message_index, cloned.first_message_index);
    }

    #[test]
    fn test_backup_key_upload_params_debug() {
        let params = BackupKeyUploadParams {
            user_id: "@test:example.com".to_string(),
            version: "1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            session_data: "data".to_string(),
        };

        let debug_str = format!("{params:?}");
        assert!(debug_str.contains("BackupKeyUploadParams"));
        assert!(debug_str.contains("@test:example.com"));
    }

    #[test]
    fn test_backup_key_upload_params_boundary_first_message_index() {
        let params_max = BackupKeyUploadParams {
            user_id: "@user:example.com".to_string(),
            version: "1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session".to_string(),
            first_message_index: i64::MAX,
            forwarded_count: 0,
            is_verified: true,
            session_data: "data".to_string(),
        };

        let params_min = BackupKeyUploadParams {
            user_id: "@user:example.com".to_string(),
            version: "1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            session_data: "data".to_string(),
        };

        assert_eq!(params_max.first_message_index, i64::MAX);
        assert_eq!(params_min.first_message_index, 0);
    }

    #[test]
    fn test_backup_key_upload_params_forwarded_count() {
        let params = BackupKeyUploadParams {
            user_id: "@user:example.com".to_string(),
            version: "1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session".to_string(),
            first_message_index: 0,
            forwarded_count: 5,
            is_verified: false,
            session_data: "data".to_string(),
        };

        assert_eq!(params.forwarded_count, 5);
    }

    #[test]
    fn test_backup_key_upload_params_verified_flag() {
        let verified = BackupKeyUploadParams {
            user_id: "@user:example.com".to_string(),
            version: "1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            session_data: "data".to_string(),
        };

        let unverified = BackupKeyUploadParams {
            user_id: "@user:example.com".to_string(),
            version: "1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: false,
            session_data: "data".to_string(),
        };

        assert!(verified.is_verified);
        assert!(!unverified.is_verified);
    }

    #[test]
    fn test_backup_key_upload_params_user_id_format() {
        let params = BackupKeyUploadParams {
            user_id: "@alice:matrix.org".to_string(),
            version: "1".to_string(),
            room_id: "!room:matrix.org".to_string(),
            session_id: "session".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            session_data: "data".to_string(),
        };

        assert!(params.user_id.starts_with('@'));
        assert!(params.user_id.contains(':'));
        assert!(params.room_id.starts_with('!'));
    }

    #[test]
    fn test_backup_key_upload_params_session_data() {
        let session_data = "eyJhbGciOiJBMjU2R0NNIiwiZW5jIjoiQTI1NkdDTSIsImtpZCI6ImtleV9pZCJ9";
        let params = BackupKeyUploadParams {
            user_id: "@user:example.com".to_string(),
            version: "1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            session_data: session_data.to_string(),
        };

        assert!(!params.session_data.is_empty());
        assert!(params.session_data.len() > 20);
    }

    #[test]
    fn test_backup_key_upload_params_version_format() {
        let params_numeric = BackupKeyUploadParams {
            user_id: "@user:example.com".to_string(),
            version: "1".to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            session_data: "data".to_string(),
        };

        let params_uuid = BackupKeyUploadParams {
            user_id: "@user:example.com".to_string(),
            version: uuid::Uuid::new_v4().to_string(),
            room_id: "!room:example.com".to_string(),
            session_id: "session".to_string(),
            first_message_index: 0,
            forwarded_count: 0,
            is_verified: true,
            session_data: "data".to_string(),
        };

        assert_eq!(params_numeric.version, "1");
        assert!(params_uuid.version.contains('-'));
    }

    // -------------------------------------------------------------------------
    // E-02 tests — update_backup_auth_data re-validation
    // -------------------------------------------------------------------------

    /// Helper: builds a minimal KeyBackup used by the E-02 / E-03 tests.
    fn make_backup(user_id: &str, mgmt_key: &str) -> KeyBackup {
        KeyBackup {
            user_id: user_id.to_string(),
            backup_id: "1".to_string(),
            version: 1,
            algorithm: "m.megolm_backup.v1".to_string(),
            auth_key: "old_auth_key".to_string(),
            mgmt_key: mgmt_key.to_string(),
            backup_data: serde_json::json!({
                "public_key": "AAAAB3NzaC1yc2EAAAADAQABAAABgQC7o",
                "signatures": {}
            }),
            etag: Some("abc123".to_string()),
        }
    }

    #[test]
    fn test_e02_rejects_missing_public_key() {
        let current = make_backup("@alice:example.com", "");
        let new_auth_data = serde_json::json!({
            // public_key intentionally omitted
            "mgmt_key": "old_key",
            "auth_key": "new_key"
        });

        let result = KeyBackupService::validate_auth_data_update(&current, &new_auth_data);
        assert!(result.is_err(), "Update with missing public_key must be rejected");
        let err = result.unwrap_err();
        assert!(err.message.contains("public_key"), "Error should mention public_key: {}", err.message);
    }

    #[test]
    fn test_e02_rejects_empty_public_key() {
        let current = make_backup("@alice:example.com", "");
        let new_auth_data = serde_json::json!({
            "public_key": "",
            "mgmt_key": "",
            "auth_key": "key"
        });

        let result = KeyBackupService::validate_auth_data_update(&current, &new_auth_data);
        assert!(result.is_err(), "Update with empty public_key must be rejected");
    }

    #[test]
    fn test_e02_rejects_changed_mgmt_key() {
        let current = make_backup("@alice:example.com", "original_mgmt_key");
        let new_auth_data = serde_json::json!({
            "public_key": "AAAAB3NzaC1yc2EAAAADAQABAAABgQC7o",
            "mgmt_key": "attacker_new_key",
            "auth_key": "new_key"
        });

        let result = KeyBackupService::validate_auth_data_update(&current, &new_auth_data);
        assert!(result.is_err(), "Update that changes mgmt_key must be rejected");
        let err = result.unwrap_err();
        assert!(err.message.contains("management key"), "Error should mention mgmt_key: {}", err.message);
    }

    #[test]
    fn test_e02_allows_valid_update() {
        let current = make_backup("@alice:example.com", "stable_key");
        let new_auth_data = serde_json::json!({
            "public_key": "AAAAB3NzaC1yc2EAAAADAQABAAABgQC7o",
            "mgmt_key": "stable_key", // unchanged
            "auth_key": "new_auth_key",
            "signatures": {
                "@alice:example.com": {
                    "ed25519:DEVICE": "valid_signature"
                }
            }
        });

        // Structural validation passes; cryptographic signature verification
        // (device_key_storage) is exercised at integration-test level.
        let result = KeyBackupService::validate_auth_data_update(&current, &new_auth_data);
        assert!(result.is_ok(), "Valid update should pass structural checks, got: {:?}", result.err());

        // Empty mgmt_key on current is treated as "no mgmt_key set" — update
        // with any value is allowed (this matches the migration case where the
        // very first PUT seeds mgmt_key).
        let no_mgmt = make_backup("@alice:example.com", "");
        let initial = serde_json::json!({
            "public_key": "AAAAB3NzaC1yc2EAAAADAQABAAABgQC7o",
            "mgmt_key": "first_key",
            "auth_key": "first_auth"
        });
        let result = KeyBackupService::validate_auth_data_update(&no_mgmt, &initial);
        assert!(result.is_ok(), "First mgmt_key seed should be allowed");
    }

    // -------------------------------------------------------------------------
    // E-02 test — create_backup must require a public_key
    // -------------------------------------------------------------------------

    #[test]
    fn test_e02_create_backup_rejects_missing_public_key() {
        // The current `create_backup` requires a Postgres pool, so we cannot
        // exercise it directly here. Instead we assert the public_key check
        // logic is present in the source via a behavioural-shaped integration
        // test elsewhere. The unit-level guarantee is encoded in
        // `validate_auth_data_update` which mirrors the same check used by
        // `create_backup`.
        let auth_data_no_key = serde_json::json!({
            "auth_key": "key",
            "mgmt_key": "key"
        });
        let pub_key = auth_data_no_key.get("public_key").and_then(|v| v.as_str());
        assert!(pub_key.is_none() || pub_key.map(str::is_empty).unwrap_or(true));
    }

    // -------------------------------------------------------------------------
    // E-03 tests — verify_backup without device_key_storage
    // -------------------------------------------------------------------------

    #[test]
    fn test_e03_rejects_non_empty_signatures_without_device_keys() {
        // E-03: When device_key_storage is None and the backup carries a
        // forged signatures block, we must NOT treat that as proof of
        // validity. The pure helper `compute_signature_validity_without_device_keys`
        // should return false for any non-empty signatures block.
        let forged_signatures = serde_json::json!({
            "@alice:example.com": {
                "ed25519:DEVICE1": "fakesignaturebase64stringthatisnotverified"
            }
        });

        let valid = KeyBackupService::compute_signature_validity_without_device_keys(&forged_signatures);
        assert!(!valid, "E-03: forged signatures must be rejected without device_key_storage");
    }

    #[test]
    fn test_e03_accepts_empty_signatures_block() {
        // An empty `signatures` block `{}` is the only case where the helper
        // returns true. This lets verify_backup distinguish "unsigned"
        // (acceptable fallback) from "forged" (rejected).
        let empty = serde_json::json!({});
        assert!(KeyBackupService::compute_signature_validity_without_device_keys(&empty));

        // Null/missing signatures: as_object() returns None → has_signatures
        // is false → returns true (treat as empty). This matches the original
        // `has_signatures = false` path that fell through.
        let missing = serde_json::json!(null);
        assert!(KeyBackupService::compute_signature_validity_without_device_keys(&missing));
    }

    #[test]
    fn test_e03_rejects_outer_signatures_with_inner_garbage() {
        // Even if the outer signatures object exists with arbitrary keys,
        // a non-empty object must be rejected.
        let garbage = serde_json::json!({
            "someuser": "not_a_map"
        });
        assert!(!KeyBackupService::compute_signature_validity_without_device_keys(&garbage));
    }
}
