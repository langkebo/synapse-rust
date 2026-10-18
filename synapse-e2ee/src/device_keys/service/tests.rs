//! Tests for device-key service helpers (moved out of `service.rs`
//! on 2026-10-10 to bring that file back under its size ceiling).

use super::is_local_user_id;

#[test]
fn local_user_id_matches_server_name() {
    assert!(is_local_user_id("@alice:example.com", "example.com"));
}

#[test]
fn remote_user_id_is_filtered_out() {
    assert!(!is_local_user_id("@alice:remote.test", "example.com"));
}

#[test]
fn invalid_user_id_is_not_local() {
    assert!(!is_local_user_id("alice", "example.com"));
}

// ------------------------------------------------------------------
// Mock-based tests — verify DeviceKeyService works with InMemoryDeviceKeyStore
// (validates the Arc<dyn DeviceKeyStoreApi> trait object injection).
// ------------------------------------------------------------------

use super::{DeviceKeyService, KeyQueryRequest};
use crate::device_keys::models::DeviceKey;
use crate::device_keys::storage::DeviceKeyStoreApi;
use crate::test_mocks::InMemoryDeviceKeyStore;
use chrono::Utc;
use std::sync::Arc;
use synapse_cache::{CacheConfig, CacheManager};

fn make_test_cache() -> Arc<CacheManager> {
    Arc::new(CacheManager::new(&CacheConfig::default()))
}

fn make_device_key(user_id: &str, device_id: &str, algorithm: &str) -> DeviceKey {
    let now = Utc::now();
    DeviceKey {
        user_id: user_id.to_string(),
        device_id: device_id.to_string(),
        display_name: None,
        algorithm: algorithm.to_string(),
        key_id: format!("{algorithm}:{device_id}"),
        public_key: format!("pk-{algorithm}-{device_id}"),
        signatures: serde_json::json!({}),
        created_ts: now,
        updated_ts: now,
    }
}

#[tokio::test]
async fn mock_store_roundtrips_device_key_via_service_query() {
    // Seed the in-memory store with an ed25519 device key for @alice.
    let store = InMemoryDeviceKeyStore::new();
    store.seed_key(make_device_key("@alice:example.com", "DEVICE_A", "ed25519")).await;

    // Wire it into DeviceKeyService through the trait object.
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store);
    let service = DeviceKeyService::new(storage, make_test_cache());

    // Query the key for @alice (no device filter — should return all keys).
    let request =
        KeyQueryRequest { timeout: None, device_keys: serde_json::json!({ "@alice:example.com": [] }), token: None };
    let response = service.query_keys(request).await.expect("query_keys must succeed with mock store");

    // The response should contain @alice in device_keys.
    let device_keys_map = response.device_keys.as_object().expect("device_keys should be a JSON object");
    assert!(
        device_keys_map.contains_key("@alice:example.com"),
        "expected @alice:example.com in device_keys response, got: {device_keys_map:?}"
    );
}

#[tokio::test]
async fn mock_store_returns_empty_for_unknown_user() {
    let store = InMemoryDeviceKeyStore::new();
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store);
    let service = DeviceKeyService::new(storage, make_test_cache());

    let request =
        KeyQueryRequest { timeout: None, device_keys: serde_json::json!({ "@nobody:example.com": [] }), token: None };
    let response = service.query_keys(request).await.expect("query_keys must succeed");

    // @nobody has no keys — the entry should map to an empty object.
    let device_keys_map = response.device_keys.as_object().expect("device_keys should be object");
    let alice_entry = device_keys_map
        .get("@nobody:example.com")
        .and_then(|v| v.as_object())
        .expect("entry for queried user should exist (possibly empty)");
    assert!(alice_entry.is_empty(), "expected empty device_keys for unknown user, got: {alice_entry:?}");
}

// ------------------------------------------------------------------
// S1 / E2EE-05/06: 验签失败的密钥必须被拒绝，而不是 warn 后继续存储
// ------------------------------------------------------------------

use super::{KeyUploadRequest, KeyUploadResponse};
use crate::device_keys::models::DeviceKeys;
use base64::Engine;
use ed25519_dalek::{Signer, SigningKey};
use synapse_common::MatrixErrorCode;

fn matrix_sign(value: &mut serde_json::Value, user_id: &str, key_id: &str, signing_key: &SigningKey) {
    let mut for_signing = value.clone();
    crate::signed_json::remove_signatures_and_unsigned(&mut for_signing);
    let msg = crate::signed_json::canonical_json_bytes(&for_signing).expect("canonical json");
    let sig = base64::engine::general_purpose::STANDARD.encode(signing_key.sign(&msg).to_bytes());
    value["signatures"] = serde_json::json!({ user_id: { key_id: sig } });
}

fn device_keys_request(device_keys: DeviceKeys) -> KeyUploadRequest {
    KeyUploadRequest { device_keys: Some(device_keys), one_time_keys: None, fallback_keys: None }
}

#[tokio::test]
async fn upload_keys_rejects_invalid_device_key_signature() {
    let store = InMemoryDeviceKeyStore::new();
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store);
    let service = DeviceKeyService::new(storage, make_test_cache());

    let user_id = "@alice:example.com";
    let device_id = "DEVICE_A";
    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let attacker_key = SigningKey::from_bytes(&[9u8; 32]);
    let ed_key_id = format!("ed25519:{device_id}");
    let ed_pub = base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().as_bytes());

    // 用攻击者的私钥签名 —— 与设备公钥不匹配，验签必失败
    let mut dk = serde_json::json!({
        "user_id": user_id,
        "device_id": device_id,
        "algorithms": ["m.olm.v1.curve25519-aes-sha2", "m.megolm.v1.aes-sha2"],
        "keys": { ed_key_id.clone(): ed_pub }
    });
    matrix_sign(&mut dk, user_id, &ed_key_id, &attacker_key);

    let device_keys: DeviceKeys = serde_json::from_value(dk).expect("deserialize");
    let result = service.upload_keys(device_keys_request(device_keys), user_id, device_id).await;

    assert!(result.is_err(), "invalid device key signature must be rejected, got: {result:?}");
}

#[tokio::test]
async fn upload_keys_accepts_valid_device_key_signature() {
    let store = InMemoryDeviceKeyStore::new();
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store.clone());
    let service = DeviceKeyService::new(storage, make_test_cache());

    let user_id = "@alice:example.com";
    let device_id = "DEVICE_A";
    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let ed_key_id = format!("ed25519:{device_id}");
    let ed_pub = base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().as_bytes());

    let mut dk = serde_json::json!({
        "user_id": user_id,
        "device_id": device_id,
        "algorithms": ["m.olm.v1.curve25519-aes-sha2", "m.megolm.v1.aes-sha2"],
        "keys": { ed_key_id.clone(): ed_pub }
    });
    matrix_sign(&mut dk, user_id, &ed_key_id, &signing_key);

    let device_keys: DeviceKeys = serde_json::from_value(dk).expect("deserialize");
    let result: Result<KeyUploadResponse, _> =
        service.upload_keys(device_keys_request(device_keys), user_id, device_id).await;

    assert!(result.is_ok(), "valid device key signature must be accepted, got: {result:?}");
    let stored = store.get_device_key(user_id, device_id, "ed25519").await.expect("query");
    assert!(stored.is_some(), "validly signed device key must be stored");
}

#[tokio::test]
async fn upload_keys_rejects_invalid_signed_one_time_key() {
    let store = InMemoryDeviceKeyStore::new();
    let user_id = "@alice:example.com";
    let device_id = "DEVICE_A";
    let signing_key = SigningKey::from_bytes(&[7u8; 32]);
    let ed_pub = base64::engine::general_purpose::STANDARD.encode(signing_key.verifying_key().as_bytes());

    // 预置设备 ed25519 密钥，使 OTK 验签路径可执行
    let mut device_key = make_device_key(user_id, device_id, "ed25519");
    device_key.public_key = ed_pub;
    store.seed_key(device_key).await;

    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store.clone());
    let service = DeviceKeyService::new(storage, make_test_cache());

    let bad_sig = base64::engine::general_purpose::STANDARD.encode([0xFFu8; 64]);
    let request = KeyUploadRequest {
        device_keys: None,
        one_time_keys: Some(serde_json::json!({
            "signed_curve25519:AAAA": {
                "key": "curve_public_key",
                "signatures": { user_id: { format!("ed25519:{device_id}"): bad_sig } }
            }
        })),
        fallback_keys: None,
    };

    let result = service.upload_keys(request, user_id, device_id).await;
    assert!(result.is_err(), "invalid OTK signature must be rejected, got: {result:?}");
    let stored = store.get_device_key(user_id, device_id, "signed_curve25519").await.expect("query");
    assert!(stored.is_none(), "invalidly signed OTK must NOT be stored");
}

#[tokio::test]
async fn upload_signatures_rejects_signatures_from_other_users() {
    let store = InMemoryDeviceKeyStore::new();
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store.clone());
    let service = DeviceKeyService::new(storage, make_test_cache());

    // 已认证用户 @alice 上传"以 @mallory 名义"的签名 —— 必须进入 failures 且不得存储
    let body = serde_json::json!({
        "@bob:example.com": {
            "ed25519:BOB_DEVICE": {
                "@mallory:example.com": { "ed25519:MALLORY_KEY": "forged_signature" }
            }
        }
    });

    let response = service.upload_signatures("@alice:example.com", body).await.expect("upload_signatures");
    let failures = response.get("failures").and_then(|f| f.as_object()).expect("failures object");
    assert!(!failures.is_empty(), "forged third-party signature must appear in failures");
    assert_eq!(store.signature_count().await, 0, "forged signature must NOT be stored");
}

#[tokio::test]
async fn upload_signatures_accepts_own_signatures() {
    let store = InMemoryDeviceKeyStore::new();
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store.clone());
    let service = DeviceKeyService::new(storage, make_test_cache());

    let body = serde_json::json!({
        "@bob:example.com": {
            "ed25519:BOB_DEVICE": {
                "@alice:example.com": { "ed25519:ALICE_KEY": "alice_signature" }
            }
        }
    });

    let response = service.upload_signatures("@alice:example.com", body).await.expect("upload_signatures");
    let failures = response.get("failures").and_then(|f| f.as_object()).expect("failures object");
    assert!(failures.is_empty(), "own signature must not fail, got: {failures:?}");
    assert_eq!(store.signature_count().await, 1, "own signature must be stored");
}

#[tokio::test]
async fn upload_signatures_rejects_non_object_body() {
    let store = InMemoryDeviceKeyStore::new();
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store.clone());
    let service = DeviceKeyService::new(storage, make_test_cache());

    // 非对象 body（数组/字符串/null）必须显式报错（400 M_BAD_JSON），
    // 而不是静默返回 200 `{"failures":{}}` —— 后者会把调用方的格式错误
    // 伪装成“无失败”。规范要求 body 是对象映射。
    for body in [serde_json::json!(["not", "an", "object"]), serde_json::json!("oops"), serde_json::json!(null)] {
        let err = service
            .upload_signatures("@alice:example.com", body.clone())
            .await
            .expect_err(&format!("non-object body {body} must be rejected"));
        assert!(err.code_is(MatrixErrorCode::BadJson), "expected M_BAD_JSON, got {}", err.code_str());
        assert_eq!(store.signature_count().await, 0, "non-object body must store nothing");
    }
}

// ------------------------------------------------------------------
// H1 (Synapse PR #20162): per-device, per-algorithm one-time key cap of 500
// ------------------------------------------------------------------

/// Build an upload carrying `count` legacy (string-valued) one-time keys.
/// A string value makes `one_time_key_algorithm` derive `curve25519`.
fn string_valued_otk_upload(count: usize) -> KeyUploadRequest {
    let mut keys = serde_json::Map::new();
    for i in 0..count {
        keys.insert(format!("curve25519:K{i}"), serde_json::Value::String("curve_public_key".to_string()));
    }
    KeyUploadRequest { device_keys: None, one_time_keys: Some(serde_json::Value::Object(keys)), fallback_keys: None }
}

#[tokio::test]
async fn upload_keys_allows_exactly_the_per_algorithm_one_time_key_limit() {
    let store = InMemoryDeviceKeyStore::new();
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store.clone());
    let service = DeviceKeyService::new(storage, make_test_cache());

    let result = service.upload_keys(string_valued_otk_upload(500), "@alice:example.com", "DEVICE_A").await;

    assert!(result.is_ok(), "exactly 500 one-time keys must be accepted, got: {result:?}");
}

#[tokio::test]
async fn upload_keys_rejects_one_time_keys_over_the_per_algorithm_limit() {
    let store = InMemoryDeviceKeyStore::new();
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store.clone());
    let service = DeviceKeyService::new(storage, make_test_cache());

    let err = service
        .upload_keys(string_valued_otk_upload(501), "@alice:example.com", "DEVICE_A")
        .await
        .expect_err("501 one-time keys must be rejected");

    assert!(err.code_is(MatrixErrorCode::TooLarge), "expected M_TOO_LARGE, got {}", err.code_str());
    assert_eq!(err.code_str(), "M_TOO_LARGE");
    // The cap is a client error, so the API surfaces it as HTTP 400 even
    // though the raw `M_TOO_LARGE` errcode canonically maps to 413.
    assert_eq!(err.http_status().as_u16(), 400, "over-limit upload must map to HTTP 400");
    // The whole upload is rejected atomically — nothing is persisted.
    let stored = store.get_device_key("@alice:example.com", "DEVICE_A", "curve25519").await.expect("query");
    assert!(stored.is_none(), "a rejected upload must not persist any one-time key");
}

#[tokio::test]
async fn upload_keys_rejects_when_existing_keys_would_exceed_the_limit() {
    let store = InMemoryDeviceKeyStore::new();
    // The device already holds 500 `signed_curve25519` keys.
    for i in 0..500 {
        let mut key = make_device_key("@alice:example.com", "DEVICE_A", "signed_curve25519");
        key.key_id = format!("signed_curve25519:seed{i}");
        key.public_key = format!("pk-seed-{i}");
        store.seed_key(key).await;
    }
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store);
    let service = DeviceKeyService::new(storage, make_test_cache());

    // One more key of the same algorithm would push the total to 501.
    let request = KeyUploadRequest {
        device_keys: None,
        one_time_keys: Some(serde_json::json!({
            "signed_curve25519:NEW": { "key": "curve_public_key" }
        })),
        fallback_keys: None,
    };

    let err = service
        .upload_keys(request, "@alice:example.com", "DEVICE_A")
        .await
        .expect_err("existing + new keys over the limit must be rejected");

    assert!(err.code_is(MatrixErrorCode::TooLarge));
    assert_eq!(err.code_str(), "M_TOO_LARGE");
}

// ------------------------------------------------------------------
// B-4：设备下线 / 登出必须清掉密钥材料，且不能让缓存继续返回已删设备
// ------------------------------------------------------------------

#[tokio::test]
async fn delete_keys_purges_only_the_target_device() {
    let store = InMemoryDeviceKeyStore::new();
    store.seed_key(make_device_key("@alice:example.com", "DEVICE_A", "ed25519")).await;
    store.seed_key(make_device_key("@alice:example.com", "DEVICE_B", "ed25519")).await;
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store.clone());
    let service = DeviceKeyService::new(storage, make_test_cache());

    service.delete_keys("@alice:example.com", "DEVICE_A").await.expect("delete_keys");

    assert!(
        store.get_device_key("@alice:example.com", "DEVICE_A", "ed25519").await.expect("query").is_none(),
        "target device key must be gone"
    );
    assert!(
        store.get_device_key("@alice:example.com", "DEVICE_B", "ed25519").await.expect("query").is_some(),
        "other devices must not be touched"
    );
}

/// 回归点：`/keys/query` 的整用户结果有 300s 的 bulk 缓存（`device_keys_bulk:{user}`）。
/// 只删行不清 bulk 缓存时，删完设备后 5 分钟内查询仍会返回它 ——
/// 等于「设备已删除但协议层依然可见」。
#[tokio::test]
async fn delete_keys_invalidates_the_bulk_query_cache() {
    let store = InMemoryDeviceKeyStore::new();
    store.seed_key(make_device_key("@alice:example.com", "DEVICE_A", "ed25519")).await;
    store.seed_key(make_device_key("@alice:example.com", "DEVICE_B", "ed25519")).await;
    let caches = make_test_cache();
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store.clone());
    let service = DeviceKeyService::new(storage, caches.clone());

    // 第一次查询：把两台设备写进 bulk 缓存
    let request =
        || KeyQueryRequest { timeout: None, device_keys: serde_json::json!({ "@alice:example.com": [] }), token: None };
    let first = service.query_keys(request()).await.expect("first query");
    let devices =
        first.device_keys["@alice:example.com"].as_object().expect("device map").keys().cloned().collect::<Vec<_>>();
    assert!(devices.contains(&"DEVICE_A".to_string()), "precondition: DEVICE_A visible, got {devices:?}");

    service.delete_keys("@alice:example.com", "DEVICE_A").await.expect("delete_keys");

    let second = service.query_keys(request()).await.expect("second query");
    let after =
        second.device_keys["@alice:example.com"].as_object().expect("device map").keys().cloned().collect::<Vec<_>>();
    assert!(
        !after.contains(&"DEVICE_A".to_string()),
        "deleted device must not be served from the bulk cache; got {after:?}"
    );
    assert!(after.contains(&"DEVICE_B".to_string()), "surviving device must still be listed");
}

#[tokio::test]
async fn delete_all_keys_purges_every_device_of_the_user() {
    let store = InMemoryDeviceKeyStore::new();
    store.seed_key(make_device_key("@alice:example.com", "DEVICE_A", "ed25519")).await;
    store.seed_key(make_device_key("@alice:example.com", "DEVICE_B", "ed25519")).await;
    store.seed_key(make_device_key("@bob:example.com", "DEVICE_C", "ed25519")).await;
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store.clone());
    let service = DeviceKeyService::new(storage, make_test_cache());

    let purged = service.delete_all_keys("@alice:example.com").await.expect("delete_all_keys");

    assert_eq!(purged, 2, "should report the two devices it cleaned");
    assert!(store.get_all_device_keys("@alice:example.com").await.expect("query").is_empty());
    assert!(
        !store.get_all_device_keys("@bob:example.com").await.expect("query").is_empty(),
        "other users must not be touched"
    );
}

#[tokio::test]
async fn delete_all_keys_on_user_without_keys_is_a_noop() {
    let store = InMemoryDeviceKeyStore::new();
    let storage: Arc<dyn super::DeviceKeyStoreApi> = Arc::new(store);
    let service = DeviceKeyService::new(storage, make_test_cache());

    let purged = service.delete_all_keys("@nobody:example.com").await.expect("delete_all_keys");
    assert_eq!(purged, 0);
}
