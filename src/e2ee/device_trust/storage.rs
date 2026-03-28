// Device Trust Storage Layer
// E2EE Phase 1: Database operations for device trust and verification

use crate::e2ee::device_trust::models::*;
use crate::error::ApiError;
use sqlx::{PgPool, Row};
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
        let result = sqlx::query_as::<_, SqlxDeviceTrustStatus>(
            "SELECT id, user_id, device_id, trust_level, verified_by_device_id, 
             verified_at, created_at, updated_at 
             FROM device_trust_status 
             WHERE user_id = $1 AND device_id = $2",
        )
        .bind(user_id)
        .bind(device_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(result.map(|r| r.into()))
    }

    /// Create or update device trust status
    pub async fn upsert_device_trust(&self, status: &DeviceTrustStatus) -> Result<(), ApiError> {
        sqlx::query(
            "INSERT INTO device_trust_status (user_id, device_id, trust_level,
             verified_by_device_id, verified_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (user_id, device_id) DO UPDATE SET
             trust_level = EXCLUDED.trust_level,
             verified_by_device_id = EXCLUDED.verified_by_device_id,
             verified_at = EXCLUDED.verified_at,
             updated_at = EXCLUDED.updated_at",
        )
        .bind(&status.user_id)
        .bind(&status.device_id)
        .bind(status.trust_level.to_string())
        .bind(&status.verified_by_device_id)
        .bind(status.verified_at)
        .bind(status.created_at)
        .bind(status.updated_at)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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

        sqlx::query(
            "INSERT INTO device_trust_status (user_id, device_id, trust_level,
             verified_by_device_id, verified_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (user_id, device_id) DO UPDATE SET
             trust_level = EXCLUDED.trust_level,
             verified_by_device_id = EXCLUDED.verified_by_device_id,
             verified_at = CASE WHEN EXCLUDED.trust_level = 'verified' THEN EXCLUDED.verified_at ELSE device_trust_status.verified_at END,
             updated_at = EXCLUDED.updated_at"
        )
        .bind(user_id)
        .bind(device_id)
        .bind(level.to_string())
        .bind(verified_by)
        .bind(if matches!(level, DeviceTrustLevel::Verified) { Some(now) } else { None })
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    /// Get all devices for a user with trust status
    pub async fn get_all_devices_with_trust(
        &self,
        user_id: &str,
    ) -> Result<Vec<DeviceTrustStatus>, ApiError> {
        let results = sqlx::query_as::<_, SqlxDeviceTrustStatus>(
            "SELECT id, user_id, device_id, trust_level, verified_by_device_id,
             verified_at, created_at, updated_at 
             FROM device_trust_status 
             WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(results.into_iter().map(|r| r.into()).collect())
    }

    /// Get verified devices only
    pub async fn get_verified_devices(
        &self,
        user_id: &str,
    ) -> Result<Vec<DeviceTrustStatus>, ApiError> {
        let results = sqlx::query_as::<_, SqlxDeviceTrustStatus>(
            "SELECT id, user_id, device_id, trust_level, verified_by_device_id,
             verified_at, created_at, updated_at 
             FROM device_trust_status 
             WHERE user_id = $1 AND trust_level = 'verified'",
        )
        .bind(user_id)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(results.into_iter().map(|r| r.into()).collect())
    }

    /// Count devices by trust level
    pub async fn count_devices_by_trust(&self, user_id: &str) -> Result<(i64, i64, i64), ApiError> {
        let row = sqlx::query(
            "SELECT 
             COUNT(CASE WHEN trust_level = 'verified' THEN 1 END) as verified,
             COUNT(CASE WHEN trust_level = 'unverified' THEN 1 END) as unverified,
             COUNT(CASE WHEN trust_level = 'blocked' THEN 1 END) as blocked
             FROM device_trust_status 
             WHERE user_id = $1",
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        let verified: i64 = row.get("verified");
        let unverified: i64 = row.get("unverified");
        let blocked: i64 = row.get("blocked");

        Ok((verified, unverified, blocked))
    }

    // =====================================================
    // Verification Request Operations
    // =====================================================

    /// Create a new verification request
    pub async fn create_verification_request(
        &self,
        request: &DeviceVerificationRequest,
    ) -> Result<(), ApiError> {
        sqlx::query(
            "INSERT INTO device_verification_request 
             (user_id, new_device_id, requesting_device_id, verification_method, 
              status, request_token, commitment, pubkey, created_at, expires_at, completed_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)",
        )
        .bind(&request.user_id)
        .bind(&request.new_device_id)
        .bind(&request.requesting_device_id)
        .bind(request.verification_method.to_string())
        .bind(request.status.to_string())
        .bind(&request.request_token)
        .bind(&request.commitment)
        .bind(&request.pubkey)
        .bind(request.created_at)
        .bind(request.expires_at)
        .bind(request.completed_at)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    /// Get verification request by token
    pub async fn get_request_by_token(
        &self,
        token: &str,
    ) -> Result<Option<DeviceVerificationRequest>, ApiError> {
        let result = sqlx::query_as::<_, SqlxVerificationRequest>(
            "SELECT id, user_id, new_device_id, requesting_device_id, verification_method,
             status, request_token, commitment, pubkey, created_at, expires_at, completed_at
             FROM device_verification_request 
             WHERE request_token = $1",
        )
        .bind(token)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(result.map(|r| r.into()))
    }

    /// Get pending verification request for user and device
    pub async fn get_pending_request(
        &self,
        user_id: &str,
        new_device_id: &str,
    ) -> Result<Option<DeviceVerificationRequest>, ApiError> {
        let result = sqlx::query_as::<_, SqlxVerificationRequest>(
            "SELECT id, user_id, new_device_id, requesting_device_id, verification_method,
             status, request_token, commitment, pubkey, created_at, expires_at, completed_at
             FROM device_verification_request 
             WHERE user_id = $1 AND new_device_id = $2 AND status = 'pending' AND expires_at > NOW()"
        )
        .bind(user_id)
        .bind(new_device_id)
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(result.map(|r| r.into()))
    }

    /// Update verification request status
    pub async fn update_request_status(
        &self,
        token: &str,
        status: VerificationRequestStatus,
    ) -> Result<(), ApiError> {
        let now = chrono::Utc::now();

        sqlx::query(
            "UPDATE device_verification_request 
             SET status = $1, completed_at = $2
             WHERE request_token = $3",
        )
        .bind(status.to_string())
        .bind(now)
        .bind(token)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    /// Update verification request with verification data
    pub async fn update_request_with_data(
        &self,
        token: &str,
        commitment: &str,
        pubkey: &str,
    ) -> Result<(), ApiError> {
        sqlx::query(
            "UPDATE device_verification_request 
             SET commitment = $1, pubkey = $2
             WHERE request_token = $3",
        )
        .bind(commitment)
        .bind(pubkey)
        .bind(token)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    /// Clean up expired verification requests
    pub async fn cleanup_expired_requests(&self) -> Result<i64, ApiError> {
        let result = sqlx::query(
            "UPDATE device_verification_request 
             SET status = 'expired', completed_at = NOW()
             WHERE status = 'pending' AND expires_at < NOW()",
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(result.rows_affected() as i64)
    }

    // =====================================================
    // Key Rotation Log Operations
    // =====================================================

    /// Log a key rotation event
    pub async fn log_key_rotation(&self, log: &KeyRotationLog) -> Result<(), ApiError> {
        sqlx::query(
            "INSERT INTO key_rotation_log 
             (user_id, device_id, room_id, rotation_type, old_key_id, new_key_id, reason, rotated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"
        )
        .bind(&log.user_id)
        .bind(&log.device_id)
        .bind(&log.room_id)
        .bind(&log.rotation_type)
        .bind(&log.old_key_id)
        .bind(&log.new_key_id)
        .bind(&log.reason)
        .bind(log.rotated_at)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    // =====================================================
    // Security Event Operations
    // =====================================================

    /// Log a security event
    pub async fn log_security_event(&self, event: &E2eeSecurityEvent) -> Result<(), ApiError> {
        sqlx::query(
            "INSERT INTO e2ee_security_events 
             (user_id, device_id, event_type, event_data, ip_address, user_agent, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(&event.user_id)
        .bind(&event.device_id)
        .bind(&event.event_type)
        .bind(event.event_data.as_ref().map(|v| v.to_string()))
        .bind(&event.ip_address)
        .bind(&event.user_agent)
        .bind(event.created_at)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    /// Get recent security events for a user
    pub async fn get_recent_security_events(
        &self,
        user_id: &str,
        limit: i64,
    ) -> Result<Vec<E2eeSecurityEvent>, ApiError> {
        let results = sqlx::query_as::<_, SqlxSecurityEvent>(
            "SELECT id, user_id, device_id, event_type, event_data, ip_address, user_agent, created_at
             FROM e2ee_security_events 
             WHERE user_id = $1
             ORDER BY created_at DESC
             LIMIT $2"
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(results.into_iter().map(|r| r.into()).collect())
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
        let now = chrono::Utc::now();

        sqlx::query(
            "INSERT INTO cross_signing_trust
             (user_id, target_user_id, is_trusted, trusted_at, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (user_id, target_user_id) DO UPDATE SET
             is_trusted = EXCLUDED.is_trusted,
             trusted_at = CASE WHEN EXCLUDED.is_trusted = TRUE THEN EXCLUDED.trusted_at ELSE cross_signing_trust.trusted_at END,
             updated_at = EXCLUDED.updated_at"
        )
        .bind(user_id)
        .bind(target_user_id)
        .bind(is_trusted)
        .bind(if is_trusted { Some(now) } else { None })
        .bind(now)
        .bind(now)
        .execute(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

        Ok(())
    }

    /// Check if user has cross-signing master key
    pub async fn has_cross_signing_master_key(&self, user_id: &str) -> Result<bool, ApiError> {
        let result = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM cross_signing_keys 
             WHERE user_id = $1 AND key_type = 'master'",
        )
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await
        .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

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
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<SqlxDeviceTrustStatus> for DeviceTrustStatus {
    fn from(row: SqlxDeviceTrustStatus) -> Self {
        DeviceTrustStatus {
            id: row.id,
            user_id: row.user_id,
            device_id: row.device_id,
            trust_level: row.trust_level.parse().unwrap_or_default(),
            verified_by_device_id: row.verified_by_device_id,
            verified_at: row.verified_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
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
    created_at: chrono::DateTime<chrono::Utc>,
    expires_at: chrono::DateTime<chrono::Utc>,
    completed_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl From<SqlxVerificationRequest> for DeviceVerificationRequest {
    fn from(row: SqlxVerificationRequest) -> Self {
        DeviceVerificationRequest {
            id: row.id,
            user_id: row.user_id,
            new_device_id: row.new_device_id,
            requesting_device_id: row.requesting_device_id,
            verification_method: row.verification_method.parse().unwrap_or_default(),
            status: row.status.parse().unwrap_or_default(),
            request_token: row.request_token,
            commitment: row.commitment,
            pubkey: row.pubkey,
            created_at: row.created_at,
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
    created_at: chrono::DateTime<chrono::Utc>,
}

impl From<SqlxSecurityEvent> for E2eeSecurityEvent {
    fn from(row: SqlxSecurityEvent) -> Self {
        E2eeSecurityEvent {
            id: row.id,
            user_id: row.user_id,
            device_id: row.device_id,
            event_type: row.event_type,
            event_data: row.event_data.and_then(|s| serde_json::from_str(&s).ok()),
            ip_address: row.ip_address,
            user_agent: row.user_agent,
            created_at: row.created_at,
        }
    }
}
