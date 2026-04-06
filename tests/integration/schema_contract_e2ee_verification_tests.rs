#![cfg(test)]

#[path = "../common/mod.rs"]
mod common;

use sqlx::Row;
use std::sync::Arc;
use synapse_rust::e2ee::device_trust::{
    DeviceTrustLevel, DeviceTrustStorage, DeviceVerificationRequest, VerificationMethod,
    VerificationRequestStatus,
};
use synapse_rust::e2ee::verification::{
    QrState, SasState, VerificationMethod as VerificationFlowMethod, VerificationRequest,
    VerificationState, VerificationStorage,
};

async fn connect_pool() -> Option<Arc<sqlx::PgPool>> {
    match common::get_test_pool_async().await {
        Ok(pool) => Some(pool),
        Err(error) => {
            eprintln!(
                "Skipping e2ee schema contract integration tests because test database is unavailable: {}",
                error
            );
            None
        }
    }
}

async fn has_unique_constraint_on(pool: &sqlx::PgPool, table_name: &str, columns: &[&str]) -> bool {
    let rows = sqlx::query(
        r#"
        SELECT tc.constraint_name, kcu.column_name, kcu.ordinal_position
        FROM information_schema.table_constraints tc
        JOIN information_schema.key_column_usage kcu
          ON tc.constraint_name = kcu.constraint_name
         AND tc.table_schema = kcu.table_schema
        WHERE tc.table_schema = 'public'
          AND tc.table_name = $1
          AND tc.constraint_type = 'UNIQUE'
        ORDER BY tc.constraint_name, kcu.ordinal_position
        "#,
    )
    .bind(table_name)
    .fetch_all(pool)
    .await
    .expect("Failed to query unique constraints");

    let mut current_name: Option<String> = None;
    let mut current_columns: Vec<String> = Vec::new();
    for row in rows {
        let name = row.get::<String, _>("constraint_name");
        let column = row.get::<String, _>("column_name");
        if current_name.as_deref() != Some(name.as_str()) {
            if current_columns == columns.iter().map(|c| (*c).to_string()).collect::<Vec<_>>() {
                return true;
            }
            current_name = Some(name);
            current_columns.clear();
        }
        current_columns.push(column);
    }

    current_columns == columns.iter().map(|c| (*c).to_string()).collect::<Vec<_>>()
}

async fn has_index_named(pool: &sqlx::PgPool, index_name: &str) -> bool {
    sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM pg_indexes
            WHERE schemaname = 'public' AND indexname = $1
        )
        "#,
    )
    .bind(index_name)
    .fetch_one(pool)
    .await
    .expect("Failed to query pg_indexes")
}

async fn assert_column(
    pool: &sqlx::PgPool,
    table_name: &str,
    column_name: &str,
    expected_types: &[&str],
    expected_nullable: bool,
    expected_default_contains: Option<&str>,
) {
    let row = sqlx::query(
        r#"
        SELECT data_type, is_nullable, column_default
        FROM information_schema.columns
        WHERE table_schema = 'public' AND table_name = $1 AND column_name = $2
        "#,
    )
    .bind(table_name)
    .bind(column_name)
    .fetch_one(pool)
    .await
    .unwrap_or_else(|_| panic!("Expected column {}.{} to exist", table_name, column_name));

    let data_type = row.get::<String, _>("data_type");
    assert!(
        expected_types
            .iter()
            .any(|ty| data_type.eq_ignore_ascii_case(ty)),
        "Expected {}.{} type in {:?}, got {}",
        table_name,
        column_name,
        expected_types,
        data_type
    );

    let is_nullable = row.get::<String, _>("is_nullable");
    assert_eq!(
        is_nullable.eq_ignore_ascii_case("YES"),
        expected_nullable,
        "Unexpected nullable flag for {}.{}",
        table_name,
        column_name
    );

    if let Some(expected_default_fragment) = expected_default_contains {
        let column_default = row
            .get::<Option<String>, _>("column_default")
            .unwrap_or_default();
        assert!(
            column_default.contains(expected_default_fragment),
            "Expected {}.{} default to contain {:?}, got {:?}",
            table_name,
            column_name,
            expected_default_fragment,
            column_default
        );
    }
}

async fn cleanup_e2ee_fixtures(
    pool: &sqlx::PgPool,
    user_id: &str,
    other_user_id: &str,
    token: &str,
    tx_id: &str,
) {
    sqlx::query("DELETE FROM verification_qr WHERE tx_id = $1")
        .bind(tx_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup verification_qr");
    sqlx::query("DELETE FROM verification_sas WHERE tx_id = $1")
        .bind(tx_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup verification_sas");
    sqlx::query("DELETE FROM verification_requests WHERE transaction_id = $1")
        .bind(tx_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup verification_requests");
    sqlx::query("DELETE FROM device_verification_request WHERE request_token = $1")
        .bind(token)
        .execute(pool)
        .await
        .expect("Failed to cleanup device_verification_request");
    sqlx::query("DELETE FROM cross_signing_trust WHERE user_id = $1 AND target_user_id = $2")
        .bind(user_id)
        .bind(other_user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup cross_signing_trust");
    sqlx::query("DELETE FROM device_trust_status WHERE user_id = $1")
        .bind(user_id)
        .execute(pool)
        .await
        .expect("Failed to cleanup device_trust_status");
}

#[tokio::test]
async fn test_schema_contract_e2ee_tables_shape() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    assert_column(
        &pool,
        "device_trust_status",
        "user_id",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "device_trust_status",
        "device_id",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "device_trust_status",
        "trust_level",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "device_trust_status",
        "verified_at",
        &["timestamp with time zone"],
        true,
        None,
    )
    .await;
    assert_column(
        &pool,
        "device_trust_status",
        "created_ts",
        &["bigint"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "device_trust_status",
        "updated_ts",
        &["bigint"],
        true,
        None,
    )
    .await;
    assert!(
        has_unique_constraint_on(&pool, "device_trust_status", &["user_id", "device_id"]).await,
        "Expected device_trust_status UNIQUE(user_id,device_id)"
    );
    assert!(
        has_index_named(&pool, "idx_device_trust_status_user_level").await,
        "Expected idx_device_trust_status_user_level"
    );

    assert_column(
        &pool,
        "cross_signing_trust",
        "user_id",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "cross_signing_trust",
        "target_user_id",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "cross_signing_trust",
        "is_trusted",
        &["boolean"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "cross_signing_trust",
        "trusted_at",
        &["timestamp with time zone"],
        true,
        None,
    )
    .await;
    assert!(
        has_unique_constraint_on(&pool, "cross_signing_trust", &["user_id", "target_user_id"])
            .await,
        "Expected cross_signing_trust UNIQUE(user_id,target_user_id)"
    );
    assert!(
        has_index_named(&pool, "idx_cross_signing_trust_user_trusted").await,
        "Expected idx_cross_signing_trust_user_trusted"
    );

    assert_column(
        &pool,
        "device_verification_request",
        "request_token",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "device_verification_request",
        "status",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "device_verification_request",
        "expires_at",
        &["timestamp with time zone"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "device_verification_request",
        "completed_at",
        &["timestamp with time zone"],
        true,
        None,
    )
    .await;
    assert!(
        has_index_named(&pool, "idx_device_verification_request_user_device_pending").await,
        "Expected idx_device_verification_request_user_device_pending"
    );
    assert!(
        has_index_named(&pool, "idx_device_verification_request_expires_pending").await,
        "Expected idx_device_verification_request_expires_pending"
    );

    for column in [
        "transaction_id",
        "from_user",
        "from_device",
        "to_user",
        "method",
        "state",
        "created_ts",
        "updated_ts",
    ] {
        assert_column(
            &pool,
            "verification_requests",
            column,
            match column {
                "created_ts" | "updated_ts" => &["bigint"],
                _ => &["text", "character varying"],
            },
            column == "updated_ts",
            None,
        )
        .await;
    }
    assert!(
        has_index_named(&pool, "idx_verification_requests_to_user_state").await,
        "Expected idx_verification_requests_to_user_state"
    );
    assert_column(
        &pool,
        "verification_sas",
        "tx_id",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "verification_sas",
        "exchange_hashes",
        &["jsonb"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "verification_qr",
        "tx_id",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
    assert_column(
        &pool,
        "verification_qr",
        "state",
        &["text", "character varying"],
        false,
        None,
    )
    .await;
}

#[tokio::test]
async fn test_schema_contract_device_trust_and_request_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = DeviceTrustStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string();
    let user_id = format!("@schema-e2ee-user-{suffix}:localhost");
    let other_user_id = format!("@schema-e2ee-target-{suffix}:localhost");
    let device_verified = "DEVICE_VERIFIED";
    let device_unverified = "DEVICE_UNVERIFIED";
    let token = format!("token-{suffix}");

    storage
        .set_device_trust(
            &user_id,
            device_unverified,
            DeviceTrustLevel::Unverified,
            None,
        )
        .await
        .expect("Failed to set unverified device trust");
    storage
        .set_device_trust(
            &user_id,
            device_verified,
            DeviceTrustLevel::Verified,
            Some(device_unverified),
        )
        .await
        .expect("Failed to set verified device trust");

    let all_devices = storage
        .get_all_devices_with_trust(&user_id)
        .await
        .expect("Failed to query device trust rows");
    assert_eq!(all_devices.len(), 2);

    let verified_devices = storage
        .get_verified_devices(&user_id)
        .await
        .expect("Failed to query verified devices");
    assert_eq!(verified_devices.len(), 1);
    assert_eq!(verified_devices[0].device_id, device_verified);

    let counts = storage
        .count_devices_by_trust(&user_id)
        .await
        .expect("Failed to count devices by trust");
    assert_eq!(counts, (1, 1, 0));

    storage
        .set_cross_signing_trust(&user_id, &other_user_id, true)
        .await
        .expect("Failed to set cross signing trust");
    let trust_row = sqlx::query(
        "SELECT is_trusted, trusted_at FROM cross_signing_trust WHERE user_id = $1 AND target_user_id = $2",
    )
    .bind(&user_id)
    .bind(&other_user_id)
    .fetch_one(&*pool)
    .await
    .expect("Failed to query cross_signing_trust");
    assert!(trust_row.get::<bool, _>("is_trusted"));
    assert!(trust_row
        .get::<Option<chrono::DateTime<chrono::Utc>>, _>("trusted_at")
        .is_some());

    let mut request =
        DeviceVerificationRequest::new(&user_id, "DEVICE_NEW", VerificationMethod::Sas, &token, 15);
    request.requesting_device_id = Some(device_verified.to_string());
    storage
        .create_verification_request(&request)
        .await
        .expect("Failed to create device verification request");

    let fetched_request = storage
        .get_request_by_token(&token)
        .await
        .expect("Failed to get request by token")
        .expect("Expected verification request to exist");
    assert_eq!(fetched_request.status, VerificationRequestStatus::Pending);
    assert_eq!(fetched_request.new_device_id, "DEVICE_NEW");

    let pending_request = storage
        .get_pending_request(&user_id, "DEVICE_NEW")
        .await
        .expect("Failed to query pending request");
    assert!(pending_request.is_some(), "Expected pending request");

    storage
        .update_request_with_data(&token, "commitment-value", "pubkey-value")
        .await
        .expect("Failed to update verification request with data");
    storage
        .update_request_status(&token, VerificationRequestStatus::Approved)
        .await
        .expect("Failed to approve verification request");

    let approved_request = storage
        .get_request_by_token(&token)
        .await
        .expect("Failed to reload approved request")
        .expect("Expected approved verification request");
    assert_eq!(approved_request.status, VerificationRequestStatus::Approved);
    assert_eq!(
        approved_request.commitment.as_deref(),
        Some("commitment-value")
    );
    assert_eq!(approved_request.pubkey.as_deref(), Some("pubkey-value"));
    assert!(approved_request.completed_at.is_some());

    cleanup_e2ee_fixtures(&pool, &user_id, &other_user_id, &token, "unused-tx-id").await;
}

#[tokio::test]
async fn test_schema_contract_verification_storage_closure() {
    let pool = match connect_pool().await {
        Some(pool) => pool,
        None => return,
    };

    let storage = VerificationStorage::new(&pool);
    let suffix = uuid::Uuid::new_v4().to_string();
    let tx_id = format!("tx-{suffix}");
    let token = format!("cleanup-token-{suffix}");
    let user_id = format!("@schema-verification-user-{suffix}:localhost");
    let other_user_id = format!("@schema-verification-target-{suffix}:localhost");

    let request = VerificationRequest {
        transaction_id: tx_id.clone(),
        from_user: user_id.clone(),
        from_device: "DEVICE_A".to_string(),
        to_user: other_user_id.clone(),
        to_device: Some("DEVICE_B".to_string()),
        method: VerificationFlowMethod::Sas,
        state: VerificationState::Requested,
        created_ts: chrono::Utc::now().timestamp_millis(),
        updated_ts: chrono::Utc::now().timestamp_millis(),
    };
    storage
        .create_request(&request)
        .await
        .expect("Failed to create verification request");

    let fetched = storage
        .get_request(&tx_id)
        .await
        .expect("Failed to get verification request")
        .expect("Expected verification request to exist");
    assert_eq!(fetched.transaction_id, tx_id);
    assert_eq!(fetched.state, VerificationState::Requested);

    let pending = storage
        .get_pending_verifications(&other_user_id)
        .await
        .expect("Failed to query pending verifications");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].transaction_id, tx_id);

    storage
        .store_sas_state(&SasState {
            tx_id: tx_id.clone(),
            from_device: "DEVICE_A".to_string(),
            to_device: Some("DEVICE_B".to_string()),
            method: VerificationFlowMethod::Sas,
            state: VerificationState::Ready,
            exchange_hashes: vec!["sha256".to_string()],
            commitment: Some("commitment".to_string()),
            pubkey: Some("pubkey".to_string()),
            sas_bytes: Some(vec![1, 2, 3, 4]),
            mac: Some("mac".to_string()),
        })
        .await
        .expect("Failed to store sas state");
    storage
        .store_qr_state(&QrState {
            tx_id: tx_id.clone(),
            from_device: "DEVICE_A".to_string(),
            to_device: Some("DEVICE_B".to_string()),
            state: VerificationState::Ready,
            qr_code_data: Some("qr-data".to_string()),
            scanned_data: Some("scanned-data".to_string()),
        })
        .await
        .expect("Failed to store qr state");

    let sas_row =
        sqlx::query("SELECT state, commitment, pubkey FROM verification_sas WHERE tx_id = $1")
            .bind(&tx_id)
            .fetch_one(&*pool)
            .await
            .expect("Failed to query verification_sas");
    assert_eq!(sas_row.get::<String, _>("state"), "ready");
    assert_eq!(
        sas_row.get::<Option<String>, _>("commitment").as_deref(),
        Some("commitment")
    );
    assert_eq!(
        sas_row.get::<Option<String>, _>("pubkey").as_deref(),
        Some("pubkey")
    );

    let qr_row = sqlx::query(
        "SELECT state, qr_code_data, scanned_data FROM verification_qr WHERE tx_id = $1",
    )
    .bind(&tx_id)
    .fetch_one(&*pool)
    .await
    .expect("Failed to query verification_qr");
    assert_eq!(qr_row.get::<String, _>("state"), "ready");
    assert_eq!(
        qr_row.get::<Option<String>, _>("qr_code_data").as_deref(),
        Some("qr-data")
    );
    assert_eq!(
        qr_row.get::<Option<String>, _>("scanned_data").as_deref(),
        Some("scanned-data")
    );

    storage
        .update_state(&tx_id, VerificationState::Done)
        .await
        .expect("Failed to update verification state");
    let pending_after_done = storage
        .get_pending_verifications(&other_user_id)
        .await
        .expect("Failed to query pending verifications after done");
    assert!(pending_after_done.is_empty());

    storage
        .delete_request(&tx_id)
        .await
        .expect("Failed to delete verification request");
    let deleted = storage
        .get_request(&tx_id)
        .await
        .expect("Failed to reload deleted verification request");
    assert!(deleted.is_none());

    cleanup_e2ee_fixtures(&pool, &user_id, &other_user_id, &token, &tx_id).await;
}
