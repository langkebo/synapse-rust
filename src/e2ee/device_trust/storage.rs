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
                id,
                user_id,
                device_id,
                trust_level,
                verified_by_device_id,
                CASE WHEN verified_at IS NOT NULL THEN CAST(EXTRACT(EPOCH FROM verified_at) * 1000 AS BIGINT) ELSE 0 END AS "verified_at!",
                created_ts,
                COALESCE(updated_ts, created_ts) AS "updated_ts!"
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
        let trust_level = status.trust_level.to_string();
        let verified_by = status.verified_by_device_id.as_deref();
        let verified_at_millis = status.verified_at.unwrap_or(0) as f64;
        sqlx::query!(
            r#"INSERT INTO device_trust_status (user_id, device_id, trust_level,
             verified_by_device_id, verified_at, created_ts, updated_ts)
             VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0), $6, $7)
             ON CONFLICT (user_id, device_id) DO UPDATE SET
             trust_level = EXCLUDED.trust_level,
             verified_by_device_id = EXCLUDED.verified_by_device_id,
             verified_at = EXCLUDED.verified_at,
             updated_ts = EXCLUDED.updated_ts"#,
            &status.user_id,
            &status.device_id,
            &trust_level,
            verified_by,
            verified_at_millis,
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
        let now_ts = chrono::Utc::now().timestamp_millis();

        let level_str = level.to_string();
        let verified_at_millis =
            if matches!(level, DeviceTrustLevel::Verified) { Some(now_ts) } else { None }.unwrap_or(0) as f64;

        sqlx::query!(
            r#"INSERT INTO device_trust_status (user_id, device_id, trust_level,
             verified_by_device_id, verified_at, created_ts, updated_ts)
             VALUES ($1, $2, $3, $4, to_timestamp($5::double precision / 1000.0), $6, $7)
             ON CONFLICT (user_id, device_id) DO UPDATE SET
             trust_level = EXCLUDED.trust_level,
             verified_by_device_id = EXCLUDED.verified_by_device_id,
             verified_at = CASE WHEN EXCLUDED.trust_level = 'verified' THEN EXCLUDED.verified_at ELSE device_trust_status.verified_at END,
             updated_ts = EXCLUDED.updated_ts"#,
            user_id,
            device_id,
            &level_str,
            verified_by,
            verified_at_millis,
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
                id,
                user_id,
                device_id,
                trust_level,
                verified_by_device_id,
                CASE WHEN verified_at IS NOT NULL THEN CAST(EXTRACT(EPOCH FROM verified_at) * 1000 AS BIGINT) ELSE 0 END AS "verified_at!",
                created_ts,
                COALESCE(updated_ts, created_ts) AS "updated_ts!"
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
                id,
                user_id,
                device_id,
                trust_level,
                verified_by_device_id,
                CASE WHEN verified_at IS NOT NULL THEN CAST(EXTRACT(EPOCH FROM verified_at) * 1000 AS BIGINT) ELSE 0 END AS "verified_at!",
                created_ts,
                COALESCE(updated_ts, created_ts) AS "updated_ts!"
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
        let row = sqlx::query_as!(
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
        let method = request.verification_method.to_string();
        let status_str = request.status.to_string();
        let requesting_device = request.requesting_device_id.as_deref();
        let commitment = request.commitment.as_deref();
        let pubkey = request.pubkey.as_deref();
        sqlx::query!(
            r#"INSERT INTO device_verification_request
             (user_id, new_device_id, requesting_device_id, verification_method,
              status, request_token, commitment, pubkey, created_ts, expires_at, completed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)"#,
            &request.user_id,
            &request.new_device_id,
            requesting_device,
            &method,
            &status_str,
            &request.request_token,
            commitment,
            pubkey,
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
                id,
                user_id,
                new_device_id,
                requesting_device_id,
                verification_method,
                status,
                request_token,
                commitment,
                pubkey,
                created_ts,
                expires_at,
                completed_at
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
                id,
                user_id,
                new_device_id,
                requesting_device_id,
                verification_method,
                status,
                request_token,
                commitment,
                pubkey,
                created_ts,
                expires_at,
                completed_at
            FROM device_verification_request
            WHERE user_id = $1 AND new_device_id = $2 AND status = 'pending' AND expires_at > (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)"#,
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
        let now = chrono::Utc::now().timestamp_millis();

        let status_str = status.to_string();

        sqlx::query!(
            r#"UPDATE device_verification_request
             SET status = $1, completed_at = $2
             WHERE request_token = $3"#,
            &status_str,
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
             SET status = 'expired', completed_at = (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
             WHERE status = 'pending' AND expires_at < (EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)"#
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
            &log.user_id,
            &log.device_id,
            log.room_id.as_deref(),
            &log.rotation_type,
            log.old_key_id.as_deref(),
            log.new_key_id.as_deref(),
            log.reason.as_deref(),
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
        let event_data_str = event.event_data.as_ref().map(|v| v.to_string());
        sqlx::query!(
            r#"INSERT INTO e2ee_security_events
             (user_id, device_id, event_type, event_data, ip_address, user_agent, created_ts)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"#,
            &event.user_id,
            event.device_id.as_deref(),
            &event.event_type,
            event_data_str.as_deref(),
            event.ip_address.as_deref(),
            event.user_agent.as_deref(),
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
                id,
                user_id,
                device_id,
                event_type,
                event_data,
                ip_address,
                user_agent,
                created_ts
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

        let trusted_at_millis = if is_trusted { Some(now) } else { None }.unwrap_or(0) as f64;

        sqlx::query!(
            r#"INSERT INTO cross_signing_trust
             (user_id, target_user_id, master_key_id, is_trusted, trusted_at, created_ts, updated_ts)
             VALUES ($1, $2, (SELECT key_data FROM cross_signing_keys WHERE user_id = $2 AND key_type = 'master' LIMIT 1), $3, to_timestamp($4::double precision / 1000.0), $5, $6)
             ON CONFLICT (user_id, target_user_id) DO UPDATE SET
             is_trusted = EXCLUDED.is_trusted,
             master_key_id = COALESCE(EXCLUDED.master_key_id, cross_signing_trust.master_key_id),
             trusted_at = CASE WHEN EXCLUDED.is_trusted = TRUE THEN EXCLUDED.trusted_at ELSE cross_signing_trust.trusted_at END,
             updated_ts = EXCLUDED.updated_ts"#,
            user_id,
            target_user_id,
            is_trusted,
            trusted_at_millis,
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
            r#"SELECT COUNT(*) AS "count!: i64" FROM cross_signing_keys WHERE user_id = $1 AND key_type = 'master'"#,
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
    verified_at: i64,
    created_ts: i64,
    updated_ts: i64,
}

/// Wrapper for count_devices_by_trust — 3 COUNT columns.
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
            verified_at: if row.verified_at == 0 { None } else { Some(row.verified_at) },
            created_ts: row.created_ts,
            updated_ts: row.updated_ts,
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
    expires_at: i64,
    completed_at: Option<i64>,
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
