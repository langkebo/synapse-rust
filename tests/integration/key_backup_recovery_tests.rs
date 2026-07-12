use super::TestContext;
use synapse_e2ee::backup::{KeyBackup, KeyBackupService, KeyBackupStorage};
use synapse_rust::common::error::MatrixErrorCode;

#[tokio::test]
async fn recover_rejects_stale_backup_version() {
    let Some(ctx) = TestContext::new().await else {
        return;
    };
    let storage = KeyBackupStorage::new(&ctx.pool);
    let user_id = "@opt007_stale:localhost";

    for v in [1_i64, 2_i64] {
        storage
            .create_backup(&KeyBackup {
                user_id: user_id.to_string(),
                backup_id: v.to_string(),
                version: v,
                algorithm: "m.megolm_backup.v1.curve25519-aes-sha2".to_string(),
                auth_key: String::new(),
                mgmt_key: String::new(),
                backup_data: serde_json::json!({}),
                etag: Some(format!("{v:x}")),
            })
            .await
            .expect("seed backup version");
    }

    let service = KeyBackupService::new(&storage);

    // Requesting the stale version (1) while current is 2 must be rejected.
    let err = service.recover_keys(user_id, "1", None).await.expect_err("stale version must be rejected");
    assert_eq!(err.code(), &MatrixErrorCode::InvalidParam);

    // Requesting the current version (2) recovers fine.
    service.recover_keys(user_id, "2", None).await.expect("current version recovers");
}
