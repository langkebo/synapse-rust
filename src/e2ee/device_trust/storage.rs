// Device Trust Storage Layer
// E2EE Phase 1: Database operations for device trust and verification

use crate::e2ee::device_trust::models::*;
use crate::error::ApiError;
use sqlx::PgPool;
use std::sync::Arc;

pub struct DeviceTrustStorage {
    pool: Arc<PgPool>,
}

impl DeviceTrustStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    // =====================================================
    // Device Trust Status Operations
    // =====================================================

    /// Get device trust status
    pub async fn get_device_trust(
        &self,
        user_id: &str,
        device_id: &str,
    ) -> Result<Option<DeviceTrustStatus>, ApiError> {
        let result = sqlx::query_as!(
            SqlxDeviceTrustStatus,
            r#"
            SELECT
                id AS "id!",
                user_id AS "user_id!",
                device_id AS "device_id!",
                trust_level AS "trust_level!",
                verified_by_device_id AS "verified_by_device_id?",
                verified_at AS "verified_at?",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?"
            FROM device_trust_status
            WHERE user_id = $1 AND device_id = $2
            "#,
            user_id,
            device_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result.map(Into::into))
    }

    /// Create or update device trust status
    pub async fn upsert_device_trust(&self, status: &DeviceTrustStatus) -> Result<(), ApiError> {
        sqlx::query!(
            r#"INSERT INTO device_trust_status (user_id, device_id, trust_level,
             verified_by_device_id, verified_at, created_ts, updated_ts)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (user_id, device_id) DO UPDATE SET
             trust_level = EXCLUDED.trust_level,
             verified_by_device_id = EXCLUDED.verified_by_device_id,
             verified_at = EXCLUDED.verified_at,
             updated_ts = EXCLUDED.updated_ts"#,
            status.user_id,
            status.device_id,
            status.trust_level.to_string(),
            status.verified_by_device_id,
            status.verified_at,
            status.created_ts,
            status.updated_ts,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    /// Set device trust level
    pub async fn set_device_trust(
        &self,
        user_id: &str,
        device_id: &str,
        level: DeviceTrustLevel,
        verified_by: Option<&str>,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now();
        let now_ts = now.timestamp_millis();

        sqlx::query!(
            r#"INSERT INTO device_trust_status (user_id, device_id, trust_level,
             verified_by_device_id, verified_at, created_ts, updated_ts)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (user_id, device_id) DO UPDATE SET
             trust_level = EXCLUDED.trust_level,
             verified_by_device_id = EXCLUDED.verified_by_device_id,
             verified_at = CASE WHEN EXCLUDED.trust_level = 'verified' THEN EXCLUDED.verified_at ELSE device_trust_status.verified_at END,
             updated_ts = EXCLUDED.updated_ts"#,
            user_id,
            device_id,
            level.to_string(),
            verified_by,
            if matches!(level, DeviceTrustLevel::Verified) { Some(now) } else { None },
            now_ts,
            now_ts,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

        Ok(())
    }

    /// Get all devices for a user with trust status
    pub async fn get_all_devices_with_trust(&self, user_id: &str) -> Result<Vec<DeviceTrustStatus>, ApiError> {
        let results = sqlx::query_as!(
            SqlxDeviceTrustStatus,
            r#"
            SELECT
                id AS "id!",
                user_id AS "user_id!",
                device_id AS "device_id!",
                trust_level AS "trust_level!",
                verified_by_device_id AS "verified_by_device_id?",
                verified_at AS "verified_at?",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?"
            FROM device_trust_status
            WHERE user_id = $1
            "#,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(results.into_iter().map(Into::into).collect())
    }

    /// Get verified devices only
    pub async fn get_verified_devices(&self, user_id: &str) -> Result<Vec<DeviceTrustStatus>, ApiError> {
        let results = sqlx::query_as!(
            SqlxDeviceTrustStatus,
            r#"
            SELECT
                id AS "id!",
                user_id AS "user_id!",
                device_id AS "device_id!",
                trust_level AS "trust_level!",
                verified_by_device_id AS "verified_by_device_id?",
                verified_at AS "verified_at?",
                created_ts AS "created_ts!",
                updated_ts AS "updated_ts?"
            FROM device_trust_status
            WHERE user_id = $1 AND trust_level = 'verified'
            "#,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(results.into_iter().map(Into::into).collect())
    }

    /// Count devices by trust level
    pub async fn count_devices_by_trust(&self, user_id: &str) -> Result<(i64, i64, i64), ApiError> {
        let row: DeviceTrustCount = sqlx::query_as!(
            DeviceTrustCount,
            r#"SELECT
                COUNT(CASE WHEN trust_level = 'verified' THEN 1 END) AS "verified!",
                COUNT(CASE WHEN trust_level = 'unverified' THEN 1 END) AS "unverified!",
                COUNT(CASE WHEN trust_level = 'blocked' THEN 1 END) AS "blocked!"
            FROM device_trust_status
            WHERE user_id = $1"#,
            user_id,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok((row.verified, row.unverified, row.blocked))
    }

    // =====================================================
    // Verification Request Operations
    // =====================================================

    /// Create a new verification request
    pub async fn create_verification_request(&self, request: &DeviceVerificationRequest) -> Result<(), ApiError> {
        sqlx::query!(
            r#"INSERT INTO device_verification_request
             (user_id, new_device_id, requesting_device_id, verification_method,
              status, request_token, commitment, pubkey, created_ts, expires_at, completed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
            request.user_id,
            request.new_device_id,
            request.requesting_device_id,
            request.verification_method.to_string(),
            request.status.to_string(),
            request.request_token,
            request.commitment,
            request.pubkey,
            request.created_ts,
            request.expires_at,
            request.completed_at,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    /// Get verification request by token
    pub async fn get_request_by_token(&self, token: &str) -> Result<Option<DeviceVerificationRequest>, ApiError> {
        let result = sqlx::query_as!(
            SqlxVerificationRequest,
            r#"SELECT
                id AS "id!",
                user_id AS "user_id!",
                new_device_id AS "new_device_id!",
                requesting_device_id AS "requesting_device_id?",
                verification_method AS "verification_method!",
                status AS "status!",
                request_token AS "request_token!",
                commitment AS "commitment?",
                pubkey AS "pubkey?",
                created_ts AS "created_ts!",
                expires_at AS "expires_at!",
                completed_at AS "completed_at?"
            FROM device_verification_request
            WHERE request_token = $1"#,
            token,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result.map(Into::into))
    }

    /// Get pending verification request for user and device
    pub async fn get_pending_request(
        &self,
        user_id: &str,
        new_device_id: &str,
    ) -> Result<Option<DeviceVerificationRequest>, ApiError> {
        let result = sqlx::query_as!(
            SqlxVerificationRequest,
            r#"SELECT
                id AS "id!",
                user_id AS "user_id!",
                new_device_id AS "new_device_id!",
                requesting_device_id AS "requesting_device_id?",
                verification_method AS "verification_method!",
                status AS "status!",
                request_token AS "request_token!",
                commitment AS "commitment?",
                pubkey AS "pubkey?",
                created_ts AS "created_ts!",
                expires_at AS "expires_at!",
                completed_at AS "completed_at?"
            FROM device_verification_request
            WHERE user_id = $1 AND new_device_id = $2 AND status = 'pending' AND expires_at > NOW()"#,
            user_id,
            new_device_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result.map(Into::into))
    }

    /// Update verification request status
    pub async fn update_request_status(&self, token: &str, status: VerificationRequestStatus) -> Result<(), ApiError> {
        let now = chrono::Utc::now();

        sqlx::query!(
            r#"UPDATE device_verification_request
             SET status = $1, completed_at = $2
             WHERE request_token = $3"#,
            status.to_string(),
            now,
            token,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    /// Update verification request with verification data
    pub async fn update_request_with_data(&self, token: &str, commitment: &str, pubkey: &str) -> Result<(), ApiError> {
        sqlx::query!(
            r#"UPDATE device_verification_request
             SET commitment = $1, pubkey = $2
             WHERE request_token = $3"#,
            commitment,
            pubkey,
            token,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    /// Clean up expired verification requests
    pub async fn cleanup_expired_requests(&self) -> Result<i64, ApiError> {
        let result = sqlx::query!(
            r#"UPDATE device_verification_request
             SET status = 'expired', completed_at = NOW()
             WHERE status = 'pending' AND expires_at < NOW()"#,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result.rows_affected() as i64)
    }

    // =====================================================
    // Key Rotation Log Operations
    // =====================================================

    /// Log a key rotation event
    pub async fn log_key_rotation(&self, log: &KeyRotationLog) -> Result<(), ApiError> {
        sqlx::query!(
            r#"INSERT INTO key_rotation_log
             (user_id, device_id, room_id, rotation_type, old_key_id, new_key_id, reason, rotated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
            log.user_id,
            log.device_id,
            log.room_id,
            log.rotation_type,
            log.old_key_id,
            log.new_key_id,
            log.reason,
            log.rotated_at,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    // =====================================================
    // Security Event Operations
    // =====================================================

    /// Log a security event
    pub async fn log_security_event(&self, event: &E2eeSecurityEvent) -> Result<(), ApiError> {
        sqlx::query!(
            r#"INSERT INTO e2ee_security_events
             (user_id, device_id, event_type, event_data, ip_address, user_agent, created_ts)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            event.user_id,
            event.device_id,
            event.event_type,
            event.event_data.as_ref().map(|v| v.to_string()),
            event.ip_address,
            event.user_agent,
            event.created_ts,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    /// Get recent security events for a user
    pub async fn get_recent_security_events(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<E2eeSecurityEvent>, ApiError> {
        let results = sqlx::query_as!(
            SqlxSecurityEvent,
            r#"SELECT
                id AS "id!",
                user_id AS "user_id!",
                device_id AS "device_id?",
                event_type AS "event_type!",
                event_data AS "event_data?",
                ip_address AS "ip_address?",
                user_agent AS "user_agent?",
                created_ts AS "created_ts!"
            FROM e2ee_security_events
            WHERE user_id = $1
            ORDER BY created_ts DESC
            LIMIT $2"#,
            user_id,
            limit,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(results.into_iter().map(Into::into).collect())
    }

    // =====================================================
    // Cross-Signing Trust Operations
    // =====================================================

    /// Set cross-signing trust for a user
    pub async fn set_cross_signing_trust(
        &self,
        user_id: &str,
        target_user_id: &str,
        is_trusted: bool,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();

        sqlx::query!(
            r#"INSERT INTO cross_signing_trust
             (user_id, target_user_id, master_key_id, is_trusted, trusted_at, created_ts, updated_ts)
             VALUES ($1, $2, (SELECT key_data FROM cross_signing_keys WHERE user_id = $2 AND key_type = 'master' LIMIT 1), $3, $4, $5, $6)
             ON CONFLICT (user_id, target_user_id) DO UPDATE SET
             is_trusted = EXCLUDED.is_trusted,
             master_key_id = COALESCE(EXCLUDED.master_key_id, cross_signing_trust.master_key_id),
             trusted_at = CASE WHEN EXCLUDED.is_trusted = TRUE THEN EXCLUDED.trusted_at ELSE cross_signing_trust.trusted_at END,
             updated_ts = EXCLUDED.updated_ts"#,
            user_id,
            target_user_id,
            is_trusted,
            if is_trusted { Some(chrono::Utc::now()) } else { None },
            now,
            now,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| { tracing::error!("Database error: {e}"); ApiError::database("A database error occurred".to_string()) })?;

        Ok(())
    }

    /// Check if user has cross-signing master key
    pub async fn has_cross_signing_master_key(&self, user_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query_scalar!(
            r#"SELECT COUNT(*) AS "count!"
            FROM cross_signing_keys
            WHERE user_id = $1 AND key_type = 'master'"#,
            user_id,
        )
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Database error: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(result > 0)
    }
}

// =====================================================
// SQLx Row types (for internal mapping)
// =====================================================

#[derive(sqlx::FromRow)]
struct SqlxDeviceTrustStatus {
    id: i64,
    user_id: String,
    device_id: String,
    trust_level: String,
    verified_by_device_id: Option<String>,
    verified_at: Option<chrono::DateTime<chrono::Utc>>,
    created_ts: i64,
    updated_ts: Option<i64>,
}

/// Wrapper for `query_as!` count_devices_by_trust — 3 COUNT columns.
#[derive(sqlx::FromRow)]
struct DeviceTrustCount {
    verified: i64,
    unverified: i64,
    blocked: i64,
}

impl From<SqlxDeviceTrustStatus> for DeviceTrustStatus {
    fn from(row: SqlxDeviceTrustStatus) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            device_id: row.device_id,
            trust_level: row.trust_level.parse().unwrap_or_default(),
            verified_by_device_id: row.verified_by_device_id,
            verified_at: row.verified_at,
            created_ts: row.created_ts,
            updated_ts: row.updated_ts.unwrap_or(row.created_ts),
        }
    }
}

#[derive(sqlx::FromRow)]
struct SqlxVerificationRequest {
    id: i64,
    user_id: String,
    new_device_id: String,
    requesting_device_id: Option<String>,
    verification_method: String,
    status: String,
    request_token: String,
    commitment: Option<String>,
    pubkey: Option<String>,
    created_ts: i64,
    expires_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<SqlxVerificationRequest> for DeviceVerificationRequest {
    fn from(row: SqlxVerificationRequest) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            new_device_id: row.new_device_id,
            requesting_device_id: row.requesting_device_id,
            verification_method: row.verification_method.parse().unwrap_or_default(),
            status: row.status.parse().unwrap_or_default(),
            request_token: row.request_token,
            commitment: row.commitment,
            pubkey: row.pubkey,
            created_ts: row.created_ts,
            expires_at: row.expires_at,
            completed_at: row.completed_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct SqlxSecurityEvent {
    id: i64,
    user_id: String,
    device_id: Option<String>,
    event_type: String,
    event_data: Option<String>,
    ip_address: Option<String>,
    user_agent: Option<String>,
    created_ts: i64,
}

impl From<SqlxSecurityEvent> for E2eeSecurityEvent {
    fn from(row: SqlxSecurityEvent) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            device_id: row.device_id,
            event_type: row.event_type,
            event_data: row.event_data.and_then(|s| serde_json::from_str(&s).ok()),
            ip_address: row.ip_address,
            user_agent: row.user_agent,
            created_ts: row.created_ts,
        }
    }
}
