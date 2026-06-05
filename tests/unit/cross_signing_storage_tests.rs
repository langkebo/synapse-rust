#![cfg(test)]

use chrono::Utc;
use serde_json::json;
use sqlx::{Pool, Postgres};
use std::sync::Arc;
use synapse_rust::e2ee::cross_signing::models::CrossSigningUpload;
use synapse_rust::e2ee::cross_signing::models::{CrossSigningKey, DeviceSignature};
use synapse_rust::e2ee::cross_signing::service::CrossSigningService;
use synapse_rust::e2ee::cross_signing::storage::CrossSigningStorage;
use tokio::runtime::Runtime;

async fn setup_test_database() -> Arc<Pool<Postgres>> {
    let pool = synapse_rust::test_utils::prepare_empty_isolated_test_pool().await.expect("Failed to prepare test pool");

    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS cross_signing_keys (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                key_type TEXT NOT NULL,
                key_data TEXT NOT NULL,
                signatures JSONB,
                added_ts BIGINT NOT NULL,
                CONSTRAINT uq_cross_signing_keys_user_type UNIQUE (user_id, key_type)
            )
            "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create cross_signing_keys table");

    sqlx::query(
        r#"
            CREATE TABLE IF NOT EXISTS device_signatures (
                id BIGSERIAL PRIMARY KEY,
                user_id TEXT NOT NULL,
                device_id TEXT NOT NULL,
                target_user_id TEXT NOT NULL,
                target_device_id TEXT NOT NULL,
                algorithm TEXT NOT NULL,
                signature TEXT NOT NULL,
                created_ts BIGINT NOT NULL,
                CONSTRAINT uq_device_signatures_unique UNIQUE (
                    user_id,
                    device_id,
                    target_user_id,
                    target_device_id,
                    algorithm
                )
            )
            "#,
    )
    .execute(&*pool)
    .await
    .expect("Failed to create device_signatures table");

    pool
}

#[test]
fn test_cross_signing_storage_round_trip_preserves_millis_timestamps() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = CrossSigningStorage::new(&pool);

        let key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: "@alice:localhost".to_string(),
            key_type: "master".to_string(),
            public_key: "master_public_key".to_string(),
            usage: vec!["master".to_string()],
            signatures: json!({
                "@alice:localhost": {
                    "ed25519:MASTER": "sig"
                }
            }),
            key_json: Some(json!({
                "user_id": "@alice:localhost",
                "usage": ["master"],
                "keys": {
                    "ed25519:MASTER": "master_public_key"
                },
                "signatures": {
                    "@alice:localhost": {
                        "ed25519:MASTER": "sig"
                    }
                }
            })),
            created_ts: Utc::now(),
            updated_ts: Utc::now(),
            binding_token: None,
            binding_ts: None,
        };

        storage.create_cross_signing_key(&key).await.unwrap();

        let fetched = storage.get_cross_signing_key("@alice:localhost", "master").await.unwrap().unwrap();
        assert_eq!(fetched.public_key, "master_public_key");
        assert!(fetched.created_ts.timestamp_millis() > 1_700_000_000_000);
        assert!(fetched.updated_ts.timestamp_millis() > 1_700_000_000_000);

        let signature = DeviceSignature {
            user_id: "@alice:localhost".to_string(),
            device_id: "ALICEDEVICE".to_string(),
            signing_key_id: "ed25519:MASTER".to_string(),
            target_user_id: "@bob:localhost".to_string(),
            target_device_id: "BOBDEVICE".to_string(),
            target_key_id: "ed25519:BOBDEVICE".to_string(),
            signature: "device_sig".to_string(),
            created_ts: Utc::now(),
        };

        storage.save_device_signature(&signature).await.unwrap();

        let user_signatures = storage.get_user_signatures("@alice:localhost").await.unwrap();
        assert_eq!(user_signatures.len(), 1);
        assert_eq!(user_signatures[0].signature, "device_sig");
        assert!(user_signatures[0].created_ts.timestamp_millis() > 1_700_000_000_000);

        let fetched_signature =
            storage.get_signature("@alice:localhost", "ed25519:MASTER", "ALICEDEVICE").await.unwrap().unwrap();
        assert_eq!(fetched_signature.signature, "device_sig");
        assert!(fetched_signature.created_ts.timestamp_millis() > 1_700_000_000_000);
    });
}

#[test]
fn test_upload_cross_signing_keys_accepts_dynamic_ed25519_key_ids() {
    let rt = Runtime::new().unwrap();
    rt.block_on(async {
        let pool = setup_test_database().await;
        let storage = CrossSigningStorage::new(&pool);
        let service = CrossSigningService::new(storage.clone());

        let upload = CrossSigningUpload {
            master_key: json!({
                "user_id": "@alice:localhost",
                "usage": ["master"],
                "keys": {
                    "ed25519:alice-master-key": "master_public_key"
                },
                "signatures": {}
            }),
            self_signing_key: json!({
                "user_id": "@alice:localhost",
                "usage": ["self_signing"],
                "keys": {
                    "ed25519:alice-self-signing-key": "self_signing_public_key"
                },
                "signatures": {
                    "@alice:localhost": {
                        "ed25519:alice-master-key": "sig"
                    }
                }
            }),
            user_signing_key: json!({
                "user_id": "@alice:localhost",
                "usage": ["user_signing"],
                "keys": {
                    "ed25519:alice-user-signing-key": "user_signing_public_key"
                },
                "signatures": {
                    "@alice:localhost": {
                        "ed25519:alice-master-key": "sig"
                    }
                }
            }),
        };

        service.upload_cross_signing_keys(upload).await.unwrap();

        let master = storage.get_cross_signing_key("@alice:localhost", "master").await.unwrap().unwrap();
        let self_signing = storage.get_cross_signing_key("@alice:localhost", "self_signing").await.unwrap().unwrap();
        let user_signing = storage.get_cross_signing_key("@alice:localhost", "user_signing").await.unwrap().unwrap();

        assert_eq!(master.public_key, "master_public_key");
        assert_eq!(self_signing.public_key, "self_signing_public_key");
        assert_eq!(user_signing.public_key, "user_signing_public_key");
    });
}
