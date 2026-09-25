#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use synapse_common::test_isolation::IsolatedTestPool;
use synapse_e2ee::cross_signing::models::{CrossSigningKey, DeviceSignature};
use synapse_e2ee::cross_signing::storage::CrossSigningStorage;

/// 工作区迁移 baseline；模板名是其内容指纹，必须与其它 crate 传同一份字节。
const BASELINE_SQL: &str = include_str!("../../migrations/00000000_unified_schema_v12.sql");

/// 每个用例一个从 v12 模板克隆的 schema（D-36/D-47 口径）。
///
/// 该文件原先用 `prepare_empty_isolated_test_pool()` + 自建两张表 —— 自建 schema
/// **没有** baseline 里的 `cross_signing_keys.user_id → users(user_id)` 外键，
/// 于是"未 seed 用户也能写交叉签名密钥"这一假象一直存在。迁到模板后该 FK 生效，
/// 故先 seed 用户（`ensure_test_user`）。
async fn setup_test_database() -> (IsolatedTestPool, Arc<sqlx::PgPool>) {
    let isolated = IsolatedTestPool::new(BASELINE_SQL).await.expect("isolated test pool");
    let pool = isolated.pool();
    crate::ensure_test_user(&pool, "@alice:localhost").await;
    (isolated, pool)
}

#[tokio::test]
async fn test_cross_signing_storage_round_trip_preserves_millis_timestamps() {
    let (_isolated, pool) = setup_test_database().await;
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
}

#[tokio::test]
async fn test_cross_signing_storage_accepts_dynamic_ed25519_key_ids() {
    let (_isolated, pool) = setup_test_database().await;
    let storage = CrossSigningStorage::new(&pool);

    // Key IDs carry dynamic suffixes (e.g. `ed25519:alice-master-key`) rather
    // than fixed names; storage must persist them verbatim.
    for (key_type, key_id, public_key) in [
        ("master", "ed25519:alice-master-key", "master_public_key"),
        ("self_signing", "ed25519:alice-self-signing-key", "self_signing_public_key"),
        ("user_signing", "ed25519:alice-user-signing-key", "user_signing_public_key"),
    ] {
        let key = CrossSigningKey {
            id: uuid::Uuid::new_v4(),
            user_id: "@alice:localhost".to_string(),
            key_type: key_type.to_string(),
            public_key: public_key.to_string(),
            usage: vec![key_type.to_string()],
            signatures: json!({}),
            key_json: Some(json!({
                "user_id": "@alice:localhost",
                "usage": [key_type],
                "keys": { key_id: public_key }
            })),
            created_ts: Utc::now(),
            updated_ts: Utc::now(),
        };
        storage.create_cross_signing_key(&key).await.unwrap();
    }

    let master = storage.get_cross_signing_key("@alice:localhost", "master").await.unwrap().unwrap();
    let self_signing = storage.get_cross_signing_key("@alice:localhost", "self_signing").await.unwrap().unwrap();
    let user_signing = storage.get_cross_signing_key("@alice:localhost", "user_signing").await.unwrap().unwrap();

    assert_eq!(master.public_key, "master_public_key");
    assert_eq!(self_signing.public_key, "self_signing_public_key");
    assert_eq!(user_signing.public_key, "user_signing_public_key");
    assert!(master.key_json.unwrap()["keys"].get("ed25519:alice-master-key").is_some());
}

/// C27 补覆盖：`cross_signing/storage.rs` 转换后的 12 处语句里，上面两条用例覆盖了 5 处
/// （`create_cross_signing_key` / `get_cross_signing_key` / `save_device_signature` /
/// `get_user_signatures` / `get_signature`）。本用例覆盖**余下 7 处**：两条列表读、
/// 两条 `ANY($1)` 批量读、`update_cross_signing_key`，以及 `delete_cross_signing_keys`
/// 的**两条** DELETE（同一事务）。
///
/// 为什么值得补：这 7 处在 C27 之前是动态 `sqlx::query*`，列名/谓词写错只有真 schema 才能
/// 证伪（D-31/D-33/D-34 同型）。其中最易写错的是**批量读的 `ANY($1)`** 与**删除的
/// `WHERE user_id = $1` 是否收窄**——后者若漏写谓词会静默清空全表。
#[tokio::test]
async fn test_cross_signing_storage_list_batch_update_delete_paths() {
    let (_isolated, pool) = setup_test_database().await;
    crate::ensure_test_user(&pool, "@bob:localhost").await;
    let storage = CrossSigningStorage::new(&pool);

    // 两个用户各两把钥匙：用于区分「按用户列表读」与「ANY($1) 批量读」。
    for user in ["@alice:localhost", "@bob:localhost"] {
        for key_type in ["master", "self_signing"] {
            let key = CrossSigningKey {
                id: uuid::Uuid::new_v4(),
                user_id: user.to_string(),
                key_type: key_type.to_string(),
                public_key: format!("{user}-{key_type}-pub"),
                usage: vec![key_type.to_string()],
                signatures: json!({}),
                key_json: Some(json!({ "user_id": user, "usage": [key_type] })),
                created_ts: Utc::now(),
                updated_ts: Utc::now(),
            };
            storage.create_cross_signing_key(&key).await.unwrap();
        }
    }

    // --- get_cross_signing_keys（按用户列表读）---
    let alice_keys = storage.get_cross_signing_keys("@alice:localhost").await.unwrap();
    assert_eq!(alice_keys.len(), 2, "alice 有两把钥匙");
    let mut types: Vec<&str> = alice_keys.iter().map(|k| k.key_type.as_str()).collect();
    types.sort_unstable();
    assert_eq!(types, vec!["master", "self_signing"]);
    assert!(storage.get_cross_signing_keys("@nobody:localhost").await.unwrap().is_empty());

    // --- get_cross_signing_keys_batch（`WHERE user_id = ANY($1)`）---
    let batch = storage
        .get_cross_signing_keys_batch(&["@alice:localhost".to_string(), "@bob:localhost".to_string()])
        .await
        .unwrap();
    assert_eq!(batch.len(), 2);
    assert_eq!(batch["@alice:localhost"].len(), 2);
    assert_eq!(batch["@bob:localhost"].len(), 2);
    // 空输入在工作区层短路（不查库），返回空 map
    assert!(storage.get_cross_signing_keys_batch(&[]).await.unwrap().is_empty());

    // --- update_cross_signing_key ---
    let mut master = storage.get_cross_signing_key("@alice:localhost", "master").await.unwrap().unwrap();
    master.public_key = "rotated-master-pub".to_string();
    master.key_json = Some(json!({ "keys": { "ed25519:MASTER": "rotated-master-pub" } }));
    storage.update_cross_signing_key(&master).await.unwrap();

    let reread = storage.get_cross_signing_key("@alice:localhost", "master").await.unwrap().unwrap();
    assert_eq!(reread.public_key, "rotated-master-pub");
    // UPDATE 按 (user_id, key_type) 收窄：key_type 不变，也不新增行
    assert_eq!(reread.key_type, "master");
    assert_eq!(storage.get_cross_signing_keys("@alice:localhost").await.unwrap().len(), 2);

    // --- save_device_signature + get_device_signatures + get_device_signatures_batch ---
    for device in ["ALICEDEVICE", "ALICEDEVICE2"] {
        let signature = DeviceSignature {
            user_id: "@alice:localhost".to_string(),
            device_id: device.to_string(),
            signing_key_id: "ed25519:MASTER".to_string(),
            target_user_id: "@bob:localhost".to_string(),
            target_device_id: "BOBDEVICE".to_string(),
            target_key_id: "ed25519:BOBDEVICE".to_string(),
            signature: format!("sig-{device}"),
            created_ts: Utc::now(),
        };
        storage.save_device_signature(&signature).await.unwrap();
    }

    // get_device_signatures 按 (user_id, target_device_id) 过滤；两条都指向 BOBDEVICE
    let by_target = storage.get_device_signatures("@alice:localhost", "BOBDEVICE").await.unwrap();
    assert_eq!(by_target.len(), 2, "两条签名都指向 BOBDEVICE");
    assert!(storage.get_device_signatures("@alice:localhost", "OTHERDEVICE").await.unwrap().is_empty());

    let sig_batch = storage
        .get_device_signatures_batch(&["@alice:localhost".to_string(), "@bob:localhost".to_string()])
        .await
        .unwrap();
    assert_eq!(sig_batch["@alice:localhost"].len(), 2);
    // bob 只是 target、不是签名者 ⇒ 批量读**不得**凭空给他一个条目
    assert!(!sig_batch.contains_key("@bob:localhost"));
    assert!(storage.get_device_signatures_batch(&[]).await.unwrap().is_empty());

    // --- delete_cross_signing_keys（同一事务里的两条 DELETE）---
    storage.delete_cross_signing_keys("@alice:localhost").await.unwrap();
    assert!(storage.get_cross_signing_keys("@alice:localhost").await.unwrap().is_empty());
    assert!(storage.get_user_signatures("@alice:localhost").await.unwrap().is_empty());
    assert!(storage.get_signature("@alice:localhost", "ed25519:MASTER", "ALICEDEVICE").await.unwrap().is_none());
    // 删除必须按 user_id 收窄：bob 的两把钥匙还在
    assert_eq!(
        storage.get_cross_signing_keys("@bob:localhost").await.unwrap().len(),
        2,
        "delete_cross_signing_keys 若漏写 user_id 谓词会静默清空全表"
    );
}
