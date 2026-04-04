use axum::body::Body;
use hyper::{Request, StatusCode};
use serde_json::json;
use tower::ServiceExt;

use crate::{get_admin_token, setup_test_app};

/// P2-1: 密钥备份创建与恢复完整闭环
/// 验证：用户可以创建密钥备份、存储会话密钥、恢复备份
#[tokio::test]
async fn test_e2ee_key_backup_lifecycle() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (_admin_token, _) = get_admin_token(&app).await;

    // 1. 创建测试用户
    let username = format!("backup_user_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let user_token = json["access_token"].as_str().unwrap().to_string();

    // 2. 创建密钥备份
    let passphrase = format!("secure_passphrase_{}", rand::random::<u32>());
    let create_backup_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/backup/secure")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "passphrase": passphrase
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(create_backup_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Key backup creation should return 200 OK"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let backup_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(backup_json["backup_id"].is_string());
    let backup_id = backup_json["backup_id"].as_str().unwrap().to_string();
    assert!(backup_json["version"].is_string());
    assert!(backup_json["algorithm"].is_string());

    // 3. 查询备份信息
    let get_backup_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/keys/backup/secure/{}",
            backup_id
        ))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(get_backup_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Get backup info should return 200 OK"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let info_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(info_json["backup_id"], backup_id);

    // 4. 存储会话密钥到备份
    let store_keys_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/keys/backup/secure/{}/keys",
            backup_id
        ))
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "passphrase": passphrase,
                "session_keys": [
                    {
                        "room_id": "!test_room:localhost",
                        "session_id": "session_123",
                        "first_message_index": 0,
                        "forwarded_count": 0,
                        "is_verified": true,
                        "session_data": {
                            "algorithm": "m.megolm.v1.aes-sha2",
                            "sender_key": "test_sender_key",
                            "session_key": "test_session_key_data"
                        }
                    }
                ]
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(store_keys_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Store session keys should return 200 OK"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let store_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        store_json["count"].as_i64().unwrap(),
        1,
        "Should store 1 session key"
    );

    // 5. 验证备份密码
    let verify_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/keys/backup/secure/{}/verify",
            backup_id
        ))
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "passphrase": passphrase
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(verify_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Verify passphrase should return 200 OK"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let verify_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        verify_json["valid"], true,
        "Correct passphrase should be valid"
    );

    // 6. 恢复备份
    let restore_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/keys/backup/secure/{}/restore",
            backup_id
        ))
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "passphrase": passphrase
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(restore_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Restore backup should return 200 OK"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let restore_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        restore_json["restored_keys"].as_i64().unwrap() >= 1,
        "Should restore at least 1 key"
    );

    // 7. 删除备份
    let delete_request = Request::builder()
        .method("DELETE")
        .uri(format!(
            "/_matrix/client/v3/keys/backup/secure/{}",
            backup_id
        ))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(delete_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Delete backup should return 200 OK"
    );

    // 8. 验证备份已删除
    let verify_deleted_request = Request::builder()
        .method("GET")
        .uri(format!(
            "/_matrix/client/v3/keys/backup/secure/{}",
            backup_id
        ))
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(verify_deleted_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Deleted backup should return 404"
    );
}

/// P2-2: 交叉签名完整流程
/// 验证：用户可以上传 master/self-signing/user-signing keys，并进行交叉签名
#[tokio::test]
async fn test_e2ee_cross_signing_flow() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (_admin_token, _) = get_admin_token(&app).await;

    // 1. 创建测试用户
    let username = format!("cross_sign_user_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let user_token = json["access_token"].as_str().unwrap().to_string();
    let user_id = json["user_id"].as_str().unwrap().to_string();

    // 2. 上传设备密钥（交叉签名的前提）
    let upload_device_keys_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/upload")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": {
                    "user_id": user_id,
                    "device_id": "DEVICE_CROSS",
                    "algorithms": ["m.olm.v1.curve25519-aes-sha2", "m.megolm.v1.aes-sha2"],
                    "keys": {
                        "curve25519:DEVICE_CROSS": "device_curve_key",
                        "ed25519:DEVICE_CROSS": "device_ed_key"
                    },
                    "signatures": {
                        user_id.clone(): {
                            "ed25519:DEVICE_CROSS": "device_signature"
                        }
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(upload_device_keys_request)
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    // 3. 上传交叉签名密钥（master, self_signing, user_signing）
    let upload_cross_signing_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/device_signing/upload")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "master_key": {
                    "user_id": user_id,
                    "usage": ["master"],
                    "keys": {
                        "ed25519:master_key_id": "master_public_key"
                    }
                },
                "self_signing_key": {
                    "user_id": user_id,
                    "usage": ["self_signing"],
                    "keys": {
                        "ed25519:self_signing_key_id": "self_signing_public_key"
                    },
                    "signatures": {
                        user_id.clone(): {
                            "ed25519:master_key_id": "master_signs_self_signing"
                        }
                    }
                },
                "user_signing_key": {
                    "user_id": user_id,
                    "usage": ["user_signing"],
                    "keys": {
                        "ed25519:user_signing_key_id": "user_signing_public_key"
                    },
                    "signatures": {
                        user_id.clone(): {
                            "ed25519:master_key_id": "master_signs_user_signing"
                        }
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(upload_cross_signing_request)
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Upload cross-signing keys should return 200 OK"
    );

    // 4. 上传签名（设备自签名）
    let upload_signatures_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/signatures/upload")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                user_id.clone(): {
                    "DEVICE_CROSS": {
                        "user_id": user_id,
                        "device_id": "DEVICE_CROSS",
                        "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
                        "keys": {
                            "ed25519:DEVICE_CROSS": "device_ed_key"
                        },
                        "signatures": {
                            user_id.clone(): {
                                "ed25519:self_signing_key_id": "self_signing_signs_device"
                            }
                        }
                    }
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app
        .clone()
        .oneshot(upload_signatures_request)
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Upload signatures should return 200 OK"
    );

    // 5. 查询密钥以验证交叉签名已生效
    let query_keys_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/keys/query")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "device_keys": {
                    user_id.clone(): []
                }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(query_keys_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Query keys should return 200 OK"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let query_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // 验证返回的密钥包含交叉签名信息
    assert!(
        query_json["device_keys"].is_object(),
        "Should return device keys"
    );
    if let Some(user_keys) = query_json["device_keys"].get(&user_id) {
        assert!(
            user_keys.is_object(),
            "Should have keys for the queried user"
        );
    }

    // 6. 查询安全摘要（验证交叉签名状态）
    let security_summary_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/security/summary")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = app.clone().oneshot(security_summary_request).await.unwrap();
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Security summary should return 200 OK"
    );

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let summary_json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // 验证安全摘要包含交叉签名信息
    assert!(
        summary_json["has_cross_signing_master"].is_boolean(),
        "Should indicate if cross-signing master key exists"
    );
}

/// P2-3: 密钥备份错误处理
/// 验证：错误的密码被拒绝，不存在的备份返回 404
#[tokio::test]
async fn test_e2ee_key_backup_error_handling() {
    let Some(app) = setup_test_app().await else {
        return;
    };

    let (_admin_token, _) = get_admin_token(&app).await;

    // 1. 创建测试用户
    let username = format!("backup_error_user_{}", rand::random::<u32>());
    let register_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/r0/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": username,
                "password": "Password123!",
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(register_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let user_token = json["access_token"].as_str().unwrap().to_string();

    // 2. 创建密钥备份
    let passphrase = format!("correct_passphrase_{}", rand::random::<u32>());
    let create_backup_request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/backup/secure")
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "passphrase": passphrase
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(create_backup_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let backup_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let backup_id = backup_json["backup_id"].as_str().unwrap().to_string();

    // 3. 测试错误的密码
    let wrong_verify_request = Request::builder()
        .method("POST")
        .uri(format!(
            "/_matrix/client/v3/keys/backup/secure/{}/verify",
            backup_id
        ))
        .header("Authorization", format!("Bearer {}", user_token))
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "passphrase": "wrong_passphrase"
            })
            .to_string(),
        ))
        .unwrap();

    let response = app.clone().oneshot(wrong_verify_request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let body = axum::body::to_bytes(response.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let verify_json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(
        verify_json["valid"], false,
        "Wrong passphrase should be invalid"
    );

    // 4. 测试不存在的备份 ID
    let nonexistent_backup_request = Request::builder()
        .method("GET")
        .uri("/_matrix/client/v3/keys/backup/secure/nonexistent_backup_id_12345")
        .header("Authorization", format!("Bearer {}", user_token))
        .body(Body::empty())
        .unwrap();

    let response = app
        .clone()
        .oneshot(nonexistent_backup_request)
        .await
        .unwrap();
    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Nonexistent backup should return 404"
    );
}
