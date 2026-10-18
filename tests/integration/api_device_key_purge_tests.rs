//! B-4：设备下线 / 登出必须清掉密钥材料。
//!
//! 这条用例覆盖的是**路由接线**，而不是 `DeviceKeyService::delete_keys` 本身
//! （后者已有单测）。历史缺陷是：`delete_keys` 实现了但**全仓 0 调用点**，
//! 于是删设备 / 登出只撤销了 token，`device_keys`（含 OTK / fallback）仍留在库里，
//! 可被 `/keys/query` 与 `/keys/claim` 继续使用 —— 等于留下一个能建立 Olm 会话的幽灵设备。
//!
//! 因此这里一律走真实 HTTP：注册 → 上传设备密钥 → 登出 → 再查询，验证设备确实消失。

use axum::body::Body;
use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ed25519_dalek::{Signer, SigningKey};
use hyper::{Request, StatusCode};
use serde_json::{json, Value};
use synapse_rust::e2ee::signed_json::{canonical_json_bytes, remove_signatures_and_unsigned};
use tower::ServiceExt;

use crate::setup_fresh_test_app;

const PASSWORD: &str = "Password123!";

fn attach_matrix_signature(value: &mut Value, signer_user_id: &str, key_id: &str, signing_key: &SigningKey) {
    let mut json_for_signing = value.clone();
    remove_signatures_and_unsigned(&mut json_for_signing);
    let message = canonical_json_bytes(&json_for_signing).unwrap();
    let signature = STANDARD.encode(signing_key.sign(&message).to_bytes());
    value["signatures"] = json!({ signer_user_id: { key_id: signature } });
}

/// 注册一个用户，返回 `(access_token, user_id, device_id, username)`。
async fn register_user(app: &axum::Router, username: &str) -> (String, String, String, String) {
    let suffix = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let unique_username = format!("{username}_{suffix}");
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/register")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "username": unique_username.clone(),
                "password": PASSWORD,
                "auth": { "type": "m.login.dummy" }
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (
        json["access_token"].as_str().unwrap().to_string(),
        json["user_id"].as_str().unwrap().to_string(),
        json["device_id"].as_str().unwrap().to_string(),
        unique_username,
    )
}

/// 用同一账号再登录一次，拿到第二台设备（登出全部设备需要多台设备做对照）。
async fn login_new_device(app: &axum::Router, username: &str) -> (String, String) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/login")
        .header("Content-Type", "application/json")
        .body(Body::from(
            json!({
                "type": "m.login.password",
                "identifier": { "type": "m.id.user", "user": username },
                "password": PASSWORD,
            })
            .to_string(),
        ))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "second login must succeed");
    let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();
    (json["access_token"].as_str().unwrap().to_string(), json["device_id"].as_str().unwrap().to_string())
}

/// 给某台设备上传一份（已正确签名的）设备身份密钥。
async fn upload_device_keys(app: &axum::Router, token: &str, user_id: &str, device_id: &str, seed: u8) {
    let signing_key = SigningKey::from_bytes(&[seed; 32]);
    let ed25519_key_id = format!("ed25519:{device_id}");

    let mut device_keys = json!({
        "user_id": user_id,
        "device_id": device_id,
        "algorithms": ["m.olm.v1.curve25519-aes-sha2", "m.megolm.v1.aes-sha2"],
        "keys": {
            format!("curve25519:{device_id}"): STANDARD.encode([seed; 32]),
            ed25519_key_id.clone(): STANDARD.encode(signing_key.verifying_key().as_bytes()),
        }
    });
    attach_matrix_signature(&mut device_keys, user_id, &ed25519_key_id, &signing_key);

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/upload")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "device_keys": device_keys }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "keys/upload must succeed");
}

/// 查询某用户当前可见的设备 id 列表。
async fn query_device_ids(app: &axum::Router, token: &str, user_id: &str) -> Vec<String> {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/keys/query")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from(json!({ "device_keys": { user_id: [] } }).to_string()))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), 1 << 20).await.unwrap();
    let json: Value = serde_json::from_slice(&body).unwrap();

    json["device_keys"][user_id].as_object().map(|map| map.keys().cloned().collect()).unwrap_or_default()
}

async fn logout(app: &axum::Router, token: &str) {
    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/logout")
        .header("Authorization", format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();

    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "logout must succeed");
}

#[tokio::test]
async fn logout_purges_the_devices_key_material() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (token_a, user_id, device_a, username) = register_user(&app, "purge_logout").await;
    upload_device_keys(&app, &token_a, &user_id, &device_a, 11).await;

    // 第二台设备只用来「旁观」查询（登出 A 之后 A 的 token 不能再用）
    let (token_b, device_b) = login_new_device(&app, &username).await;
    upload_device_keys(&app, &token_b, &user_id, &device_b, 12).await;

    let before = query_device_ids(&app, &token_b, &user_id).await;
    assert!(before.contains(&device_a), "precondition: uploaded device must be visible, got {before:?}");

    logout(&app, &token_a).await;

    let after = query_device_ids(&app, &token_b, &user_id).await;
    assert!(
        !after.contains(&device_a),
        "logged-out device must no longer be queryable (B-4: ghost device), got {after:?}"
    );
    assert!(after.contains(&device_b), "the still-logged-in device must remain visible, got {after:?}");
}

#[tokio::test]
async fn logout_all_purges_every_devices_key_material() {
    let Some(app) = setup_fresh_test_app().await else {
        return;
    };

    let (token_a, user_id, device_a, username) = register_user(&app, "purge_logout_all").await;
    upload_device_keys(&app, &token_a, &user_id, &device_a, 21).await;

    // 第二台设备：登出全部设备需要多台设备做对照
    let (token_b, device_b) = login_new_device(&app, &username).await;
    upload_device_keys(&app, &token_b, &user_id, &device_b, 22).await;

    let request = Request::builder()
        .method("POST")
        .uri("/_matrix/client/v3/logout/all")
        .header("Authorization", format!("Bearer {token_a}"))
        .header("Content-Type", "application/json")
        .body(Body::from("{}"))
        .unwrap();
    let response = ServiceExt::<Request<Body>>::oneshot(app.clone(), request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK, "logout/all must succeed");

    // logout/all 会吊销所有既有 token，所以必须**重新登录**一台设备来旁观结果
    let (token_fresh, _device_fresh) = login_new_device(&app, &username).await;
    let after = query_device_ids(&app, &token_fresh, &user_id).await;
    assert!(
        !after.contains(&device_a) && !after.contains(&device_b),
        "every device's keys must be purged by logout/all, got {after:?}"
    );
}
