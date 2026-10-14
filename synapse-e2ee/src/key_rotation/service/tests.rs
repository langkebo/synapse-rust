//! key_rotation 测试（P0 安全关键路径，此前 0 覆盖）。
//!
//! 2026-10-10 从 `service.rs` 外置，使该文件回到体积天花板以内；
//! `include_str!` 的相对路径已由 `service.rs` 调整为 `../service.rs`。

use super::*;
use crate::megolm::MegolmSessionStorage;
use chrono::Duration;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use synapse_cache::CacheConfig;

fn build_service(config: KeyRotationConfig) -> KeyRotationService {
    // 懒连接池：should_rotate 只用 config，不真正触达 olm/megolm/storage 的 DB。
    let pool = Arc::new(PgPoolOptions::new().connect_lazy_with(PgConnectOptions::new()));
    let cache = Arc::new(synapse_cache::CacheManager::new(&CacheConfig::default()));
    let at_rest = crate::crypto::key_at_rest::KeyAtRest::new([0u8; 32]);
    let megolm =
        Arc::new(MegolmProvider::from_env(MegolmSessionStorage::new(&pool, at_rest.clone()), cache.clone(), at_rest));
    let storage = Arc::new(KeyRotationStorage::new(pool));
    KeyRotationService::new(megolm, storage, config)
}

fn make_session(
    message_index: i64,
    last_used_ts: chrono::DateTime<Utc>,
    expires_at: Option<chrono::DateTime<Utc>>,
) -> MegolmSession {
    MegolmSession {
        id: uuid::Uuid::new_v4(),
        session_id: "session-1".to_string(),
        room_id: "!room:test".to_string(),
        sender_key: "sender-key".to_string(),
        session_key: "session-key".to_string(),
        algorithm: "m.megolm.v1.aes-sha2".to_string(),
        message_index,
        created_ts: last_used_ts,
        last_used_ts,
        expires_at,
    }
}

#[test]
fn test_key_rotation_config_defaults() {
    let cfg = KeyRotationConfig::default();
    assert_eq!(cfg.olm_rotation_days, 7);
    assert_eq!(cfg.megolm_rotation_messages, 100);
    assert_eq!(cfg.max_session_age_days, 90);
    assert!(cfg.enable_auto_rotation);
}

#[tokio::test]
async fn test_should_rotate_false_when_fresh() {
    let svc = build_service(KeyRotationConfig::default());
    let session = make_session(0, Utc::now(), None);
    assert!(!svc.should_rotate(&session).await.unwrap(), "fresh session must not rotate");
}

#[tokio::test]
async fn test_should_rotate_when_age_exceeds_olm_days() {
    let svc = build_service(KeyRotationConfig::default());
    // 10 天前最后使用，超过 olm_rotation_days = 7。
    let session = make_session(0, Utc::now() - Duration::days(10), None);
    assert!(svc.should_rotate(&session).await.unwrap(), "aged session must rotate");
}

#[tokio::test]
async fn test_should_rotate_when_message_index_exceeds_limit() {
    let svc = build_service(KeyRotationConfig::default());
    // message_index >= megolm_rotation_messages = 100。
    let session = make_session(150, Utc::now(), None);
    assert!(svc.should_rotate(&session).await.unwrap(), "high message index must rotate");
}

#[tokio::test]
async fn test_should_rotate_when_expired() {
    let svc = build_service(KeyRotationConfig::default());
    let session = make_session(0, Utc::now(), Some(Utc::now() - Duration::hours(1)));
    assert!(svc.should_rotate(&session).await.unwrap(), "expired session must rotate");
}

#[tokio::test]
async fn test_get_config_returns_defaults() {
    let svc = build_service(KeyRotationConfig::default());
    let cfg = svc.get_config().await;
    assert_eq!(cfg.olm_rotation_days, 7);
    assert!(cfg.enable_auto_rotation);
}

// -------------------------------------------------------------------------
// E-07 dedup invariant test
// -------------------------------------------------------------------------

/// E-07 invariant: `forward_keys_for_new_member` must skip a session
/// that already appears in `megolm_key_shares` for the SAME RECIPIENT.
/// We exercise this by asserting the storage-level dedup helper is the
/// gating function (return value contract) and the SQL it executes is
/// correct — now with a three-tuple (`room_id`, `session_id`,
/// `recipient_user_id`) primary key.
#[test]
fn test_e07_dedup_helper_query() {
    // The query MUST filter by room_id, session_id, AND recipient_user_id —
    // without the recipient dimension a prior share to user A would
    // suppress the key for user B. This test asserts the SQL contract
    // by static introspection: we look for the exact substrings that
    // make the check per-recipient.
    let src = include_str!("../service.rs");
    let helper_section = src.split("pub async fn key_share_exists").nth(1).expect("key_share_exists should be defined");
    let body = helper_section.split("}\n    }").next().expect("helper should have a body");
    assert!(body.contains("room_id = $1"));
    assert!(body.contains("session_id = $2"));
    assert!(body.contains("recipient_user_id = $3"), "key_share_exists must filter by recipient_user_id (E-07 fix)");
}

#[test]
fn test_e07_forward_keys_calls_dedup_first() {
    // E-07: the dedup check must happen *before* `share_session`.
    // Source-level contract: if the order is ever reversed, this
    // assertion will fail and we'll know to fix it.
    let src = include_str!("../service.rs");
    let fn_section = src
        .split("pub async fn forward_keys_for_new_member")
        .nth(1)
        .expect("forward_keys_for_new_member should be defined");
    let body = fn_section.split("\n    }\n").next().expect("body");

    let dedup_pos = body.find("key_share_exists").expect("should call key_share_exists");
    let share_pos = body.find("share_session(").expect("should call share_session");

    assert!(dedup_pos < share_pos, "E-07 invariant: key_share_exists (dedup) must run before share_session");

    let continue_pos = body.find("continue;").expect("dedup branch should skip with continue");
    assert!(continue_pos > dedup_pos && continue_pos < share_pos, "continue should sit between dedup and share");
}

// -----------------------------------------------------------
// KeyRotationConfig edge cases (non-async)
// -----------------------------------------------------------

#[test]
fn test_key_rotation_custom_config_values() {
    let cfg = KeyRotationConfig {
        olm_rotation_days: 30,
        megolm_rotation_messages: 500,
        max_session_age_days: 180,
        enable_auto_rotation: false,
    };

    assert_eq!(cfg.olm_rotation_days, 30);
    assert_eq!(cfg.megolm_rotation_messages, 500);
    assert_eq!(cfg.max_session_age_days, 180);
    assert!(!cfg.enable_auto_rotation);
}

#[test]
fn test_key_rotation_config_from_values() {
    let cfg = KeyRotationConfig {
        olm_rotation_days: 14,
        megolm_rotation_messages: 200,
        max_session_age_days: 60,
        enable_auto_rotation: true,
    };

    assert_eq!(cfg.olm_rotation_days, 14);
    assert_eq!(cfg.megolm_rotation_messages, 200);
    assert_eq!(cfg.max_session_age_days, 60);
    assert!(cfg.enable_auto_rotation);
}

// -----------------------------------------------------------
// should_rotate edge cases (async)
// -----------------------------------------------------------

#[tokio::test]
async fn test_should_rotate_false_when_message_index_at_limit() {
    // message_index == megolm_rotation_messages means rotate.
    // message_index < megolm_rotation_messages means do not rotate.
    let svc = build_service(KeyRotationConfig::default());
    let session = make_session(99, Utc::now(), None);
    assert!(!svc.should_rotate(&session).await.unwrap(), "message_index at boundary (99 < 100) should not rotate");
}

#[tokio::test]
async fn test_should_rotate_false_when_not_expired() {
    let svc = build_service(KeyRotationConfig::default());
    // Session with 1 day age (under 7-day default) and not expired.
    let session = make_session(0, Utc::now() - Duration::days(1), Some(Utc::now() + Duration::days(30)));
    assert!(!svc.should_rotate(&session).await.unwrap(), "recent session should not rotate");
}

// -----------------------------------------------------------
// 模型守卫（M-05 / 审计 §11.15）：key_rotation_log 属 megolm 域，不是联邦签名密钥域
// -----------------------------------------------------------

/// 取出 `CREATE TABLE [IF NOT EXISTS] <name> (...);` 的整段 DDL（小写，便于大小写无关断言）。
///
/// 找不到时返回空串 —— 调用方据此断言"表必须存在"，而不是悄悄跳过。
fn ddl_block(sql: &str, table: &str) -> String {
    let needle = format!("create table if not exists {table} (");
    let Some(start) = sql.to_lowercase().find(&needle) else {
        return String::new();
    };
    let rest = &sql[start..];
    let end = rest.find(");").map_or(rest.len(), |i| i + 2);
    rest[..end].to_lowercase()
}

/// `key_rotation_log` 记的是 **megolm 会话轮换**，作用域 per `(user_id, device_id, room_id)`。
/// 它**不是**联邦签名密钥轮换 —— 后者是 per `(server_name, key_id)`，落在**另一张表**
/// `federation_signing_keys` 上（上游 Synapse 对应 `old_signing_keys` + `signing.key`，
/// 经 `/_matrix/key/v2/server` 的 `old_verify_keys` 发布）。
///
/// 这条守卫防的是把两个域**再并回一起**：一旦有人为了"让 `/history` 不再恒空"而给这张表加
/// `server_name` 之类的列，它就不再是它的写者（`log_rotation(user, device, room, "megolm")`）
/// 与读者（`GET …/keys/rotation/history/{device_id}`）所说的那个东西了。
///
/// 为什么用**文本级**断言而非查 `information_schema`：schema 的唯一真源就是这份基线 SQL
/// （`IsolatedTestPool::new(BASELINE_SQL)` 逐字节施加它），所以文本级判据既等价又不需要数据库。
/// 另有一处棘轮副作用要避开：`#[cfg(test)]` 模块里的**任何** `sqlx::query*` 都会被
/// `sqlx_ratio_gate` 计入"测试基础设施动态 SQL"，而 `#[cfg(test)]` 内的 sqlx **宏**又进不了
/// `cargo sqlx prepare`（`--all-targets` 会 E0432）；纯断言两者都不触碰。
#[test]
fn key_rotation_log_stays_user_device_scoped_not_server_scoped() {
    let baseline = include_str!("../../../../migrations/00000000_unified_schema_v12.sql");

    let log_ddl = ddl_block(baseline, "key_rotation_log");
    assert!(!log_ddl.is_empty(), "baseline 里必须存在 `CREATE TABLE IF NOT EXISTS key_rotation_log`");
    for required in ["user_id", "device_id", "room_id", "rotation_type", "old_key_id", "new_key_id", "rotated_at"] {
        assert!(
            log_ddl.contains(required),
            "key_rotation_log 必须保留 `{required}` —— 它是 per-(user, device, room) 的 megolm 轮换日志:\n{log_ddl}"
        );
    }
    for forbidden in ["server_name", "verify_key", "public_key", "secret_key"] {
        assert!(
            !log_ddl.contains(forbidden),
            "key_rotation_log 必须是 user/device 作用域：出现 server 作用域列 `{forbidden}` 说明联邦签名密钥域\
             又被并回了 megolm 日志（M-05 的混装复发，见审计 §11.15）:\n{log_ddl}"
        );
    }

    // 对照：per-server 的签名密钥历史在另一张表上，键是 (server_name, key_id)。
    let signing_keys_ddl = ddl_block(baseline, "federation_signing_keys");
    assert!(!signing_keys_ddl.is_empty(), "baseline 里必须存在 `CREATE TABLE IF NOT EXISTS federation_signing_keys`");
    for required in ["server_name", "key_id"] {
        assert!(
            signing_keys_ddl.contains(required),
            "federation_signing_keys 必须保留 `{required}`:\n{signing_keys_ddl}"
        );
    }
    assert!(!signing_keys_ddl.contains("user_id"), "联邦签名密钥是 per-server、不是 per-user:\n{signing_keys_ddl}");
}
