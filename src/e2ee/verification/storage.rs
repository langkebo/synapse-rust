//! Verification storage

use crate::e2ee::verification::models::{
    QrState, SasState, VerificationMethod as VMethod, VerificationRequest, VerificationState,
};
use crate::error::ApiError;
use sqlx::PgPool;
use std::sync::Arc;

pub struct VerificationStorage {
    pool: Arc<PgPool>,
}

impl VerificationStorage {
    pub fn new(pool: &Arc<PgPool>) -> Self {
        Self { pool: pool.clone() }
    }

    /// Create a new verification request
    pub async fn create_request(&self, request: &VerificationRequest) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            INSERT INTO verification_requests
            (transaction_id, from_user, from_device, to_user, to_device, method, state, created_ts, updated_ts)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (transaction_id) DO NOTHING
            ",
            &request.transaction_id,
            &request.from_user,
            &request.from_device,
            &request.to_user,
            request.to_device.as_deref(),
            serialize_method(&request.method),
            serialize_state(&request.state),
            request.created_ts,
            request.updated_ts,
        )
        .execute(self.pool.as_ref())
        .await
        .map_err(|e| {
            tracing::error!("Failed to create verification request: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    /// Get verification request by transaction ID
    pub async fn get_request(&self, transaction_id: &str) -> Result<Option<VerificationRequest>, ApiError> {
        let row = sqlx::query!(
            r#"
            SELECT
                transaction_id AS "transaction_id!",
                from_user AS "from_user!",
                from_device AS "from_device!",
                to_user AS "to_user!",
                to_device,
                method AS "method!",
                state AS "state!",
                created_ts AS "created_ts!",
                updated_ts
            FROM verification_requests WHERE transaction_id = $1
            "#,
            transaction_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| { tracing::error!("Failed to get verification request: {e}"); ApiError::database("A database error occurred".to_string()) })?;

        if let Some(r) = row
        {
            Ok(Some(VerificationRequest {
                transaction_id: r.transaction_id,
                from_user: r.from_user,
                from_device: r.from_device,
                to_user: r.to_user,
                to_device: r.to_device,
                method: parse_method(&r.method),
                state: parse_state(&r.state),
                created_ts: r.created_ts,
                updated_ts: r.updated_ts,
            }))
        } else {
            Ok(None)
        }
    }

    /// Update verification state
    pub async fn update_state(&self, transaction_id: &str, state: VerificationState) -> Result<(), ApiError> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query!("UPDATE verification_requests SET state = $1, updated_ts = $2 WHERE transaction_id = $3",
            serialize_state(&state),
            now,
            transaction_id,
        )
            .execute(&*self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to update verification state: {e}");
                ApiError::database("A database error occurred".to_string())
            })?;

        Ok(())
    }

    /// Store SAS state
    pub async fn store_sas_state(&self, sas: &SasState) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            INSERT INTO verification_sas
            (tx_id, from_device, to_device, method, state, exchange_hashes, commitment, pubkey, sas_bytes, mac)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            ON CONFLICT (tx_id) DO UPDATE SET
                to_device = $3, state = $5, exchange_hashes = $6, commitment = $7, pubkey = $8, sas_bytes = $9, mac = $10
            ",
            &sas.tx_id,
            &sas.from_device,
            sas.to_device.as_deref(),
            serialize_method(&sas.method),
            serialize_state(&sas.state),
            serde_json::to_value(&sas.exchange_hashes).unwrap_or_default(),
            sas.commitment.as_deref(),
            sas.pubkey.as_deref(),
            sas.sas_bytes.as_deref(),
            sas.mac.as_deref(),
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| { tracing::error!("Failed to store SAS state: {e}"); ApiError::database("A database error occurred".to_string()) })?;

        Ok(())
    }

    /// Store QR state
    pub async fn store_qr_state(&self, qr: &QrState) -> Result<(), ApiError> {
        sqlx::query!(
            r"
            INSERT INTO verification_qr
            (tx_id, from_device, to_device, state, qr_code_data, scanned_data)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (tx_id) DO UPDATE SET
                to_device = $3, state = $4, qr_code_data = $5, scanned_data = $6
            ",
            &qr.tx_id,
            &qr.from_device,
            qr.to_device.as_deref(),
            serialize_state(&qr.state),
            qr.qr_code_data.as_deref(),
            qr.scanned_data.as_deref(),
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to store QR state: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(())
    }

    /// Get pending verifications for a user
    pub async fn get_pending_verifications(&self, user_id: &str) -> Result<Vec<VerificationRequest>, ApiError> {
        let rows = sqlx::query!(
            r#"
            SELECT
                transaction_id AS "transaction_id!",
                from_user AS "from_user!",
                from_device AS "from_device!",
                to_user AS "to_user!",
                to_device,
                method AS "method!",
                state AS "state!",
                created_ts AS "created_ts!",
                updated_ts
            FROM verification_requests
            WHERE to_user = $1 AND state IN ('requested', 'ready', 'pending')
            "#,
            user_id,
        )
        .fetch_all(&*self.pool)
        .await
        .map_err(|e| {
            tracing::error!("Failed to get pending verifications: {e}");
            ApiError::database("A database error occurred".to_string())
        })?;

        Ok(rows
            .into_iter()
            .map(|r| {
                VerificationRequest {
                    transaction_id: r.transaction_id,
                    from_user: r.from_user,
                    from_device: r.from_device,
                    to_user: r.to_user,
                    to_device: r.to_device,
                    method: parse_method(&r.method),
                    state: parse_state(&r.state),
                    created_ts: r.created_ts,
                    updated_ts: r.updated_ts,
                }
            })
            .collect())
    }

    pub async fn get_sas_state(&self, transaction_id: &str) -> Result<Option<SasState>, ApiError> {
        let row = sqlx::query!(
            r#"
            SELECT
                tx_id AS "tx_id!",
                from_device AS "from_device!",
                to_device,
                method AS "method!",
                state AS "state!",
                exchange_hashes AS "exchange_hashes!",
                commitment,
                pubkey,
                sas_bytes,
                mac
            FROM verification_sas WHERE tx_id = $1
            "#,
            transaction_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| { tracing::error!("Failed to get SAS state: {e}"); ApiError::database("A database error occurred".to_string()) })?;

        if let Some(r) = row
        {
            Ok(Some(SasState {
                tx_id: r.tx_id,
                from_device: r.from_device,
                to_device: r.to_device,
                method: parse_method(&r.method),
                state: parse_state(&r.state),
                exchange_hashes: serde_json::from_value(r.exchange_hashes).unwrap_or_default(),
                commitment: r.commitment,
                pubkey: r.pubkey,
                sas_bytes: r.sas_bytes,
                mac: r.mac,
            }))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_request(&self, transaction_id: &str) -> Result<(), ApiError> {
        sqlx::query!("DELETE FROM verification_requests WHERE transaction_id = $1",
            transaction_id,
        )
            .execute(&*self.pool)
            .await
            .map_err(|e| {
                tracing::error!("Failed to delete verification request: {e}");
                ApiError::database("A database error occurred".to_string())
            })?;

        Ok(())
    }
}

fn serialize_method(method: &VMethod) -> &'static str {
    match method {
        VMethod::Sas => "sas",
        VMethod::Qr => "qr",
        VMethod::Emoji => "emoji",
        VMethod::Decimal => "decimal",
    }
}

fn serialize_state(state: &VerificationState) -> &'static str {
    match state {
        VerificationState::Requested => "requested",
        VerificationState::Ready => "ready",
        VerificationState::Pending => "pending",
        VerificationState::Done => "done",
        VerificationState::Cancelled => "cancelled",
    }
}

fn parse_method(value: &str) -> VMethod {
    match value.trim_matches('"') {
        "sas" => VMethod::Sas,
        "qr" => VMethod::Qr,
        "emoji" => VMethod::Emoji,
        "decimal" => VMethod::Decimal,
        _ => serde_json::from_str(value).unwrap_or(VMethod::Sas),
    }
}

fn parse_state(value: &str) -> VerificationState {
    match value.trim_matches('"') {
        "requested" => VerificationState::Requested,
        "ready" => VerificationState::Ready,
        "pending" => VerificationState::Pending,
        "done" => VerificationState::Done,
        "cancelled" | "canceled" => VerificationState::Cancelled,
        _ => serde_json::from_str(value).unwrap_or(VerificationState::Requested),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_method, parse_state, serialize_method, serialize_state};
    use crate::e2ee::verification::models::{VerificationMethod, VerificationState};

    #[test]
    fn serializes_plain_enum_values() {
        assert_eq!(serialize_method(&VerificationMethod::Sas), "sas");
        assert_eq!(serialize_state(&VerificationState::Requested), "requested");
    }

    #[test]
    fn parses_plain_and_json_encoded_values() {
        assert_eq!(parse_method("sas"), VerificationMethod::Sas);
        assert_eq!(parse_method("\"qr\""), VerificationMethod::Qr);
        assert_eq!(parse_state("pending"), VerificationState::Pending);
        assert_eq!(parse_state("\"cancelled\""), VerificationState::Cancelled);
    }
}
