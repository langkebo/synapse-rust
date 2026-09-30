// Client push service unit tests.
//
// Exercises `synapse_services::client_push_service::ClientPushService` against
// an in-memory mock of its single storage dependency (`PushStoreApi`). Covers:
//   * Happy path for non-row-returning storage methods (returns Ok + value
//     mapping for `delete_push_rule`, `get_push_rule_enabled`, ...).
//   * Happy path for row-returning methods when storage is empty (verifies
//     the service maps an empty typed-row `Vec` → empty `Vec<Value>` without error).
//   * Error path: every storage method that returns `Err` is mapped to an
//     `ApiError::internal` (verified via `ApiError::is_internal()`).
//   * Request DTO construction / cloning.
//
// Row-shape happy paths are covered in two places since C33 (when these methods
// stopped returning `sqlx::postgres::PgRow` in favour of `PusherRow` /
// `PushRuleRow` / `NotificationRow`): the column→JSON mapping by
// `synapse-services/src/client_push_service.rs`'s own unit tests (which can now
// feed an `InMemoryPushStore`), and the real-SQL round trips by
// `synapse-storage/src/push/mod.rs::db_tests` against a real Postgres.
//
// The `push_rules` table is the single authority for push rules (I.2): this
// suite therefore has **no** account-data mock — the former `m.push_rules`
// account-data reader is gone.

use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex};
use synapse_services::client_push_service::{ClientPushService, UpsertPushRuleRequest, UpsertPusherRequest};
use synapse_storage::push::PushStoreApi;
use synapse_storage::push::{NotificationRow, PushRuleRow, PushRuleScopedRow, PusherRow};

// ─────────────────────────────────────────────────────────────────────────────
// Mock: PushStoreApi
// ─────────────────────────────────────────────────────────────────────────────

/// In-memory fake for `PushStoreApi`. Configurable per-call return values for
/// the primitives (`delete_push_rule`, `get_push_rule_enabled`,
/// `ack_notification`); row-returning methods yield empty vectors / `None`
/// unless `fail_all` is set.
#[derive(Debug, Default)]
struct MockPushStore {
    state: Mutex<MockPushStoreState>,
}

#[derive(Debug, Default, Clone)]
struct MockPushStoreState {
    /// When set, every method returns `Err(sqlx::Error::PoolClosed)`.
    fail_all: bool,
    /// Configured return value for `delete_push_rule` (rows affected).
    delete_rule_rows: u64,
    /// Configured return value for `get_push_rule_enabled`.
    enabled_value: Option<bool>,
    /// Configured return value for `ack_notification` — `true` makes the mock
    /// report a successful ack (the service maps `Some(id)` → `Ok(true)`).
    ack_returns_some: bool,
}

impl MockPushStore {
    fn new() -> Self {
        Self::default()
    }

    fn with_failure() -> Self {
        let store = Self::new();
        *store.state.lock().unwrap() = MockPushStoreState { fail_all: true, ..Default::default() };
        store
    }

    fn with_delete_rows(rows: u64) -> Self {
        let store = Self::new();
        *store.state.lock().unwrap() = MockPushStoreState { delete_rule_rows: rows, ..Default::default() };
        store
    }

    fn with_ack_some() -> Self {
        let store = Self::new();
        *store.state.lock().unwrap() = MockPushStoreState { ack_returns_some: true, ..Default::default() };
        store
    }

    fn with_enabled(value: Option<bool>) -> Self {
        let store = Self::new();
        *store.state.lock().unwrap() = MockPushStoreState { enabled_value: value, ..Default::default() };
        store
    }
}

fn storage_error() -> sqlx::Error {
    // `PoolClosed` is a unit variant — cheapest `sqlx::Error` to construct
    // without a live connection. The service must wrap it in `ApiError`.
    sqlx::Error::PoolClosed
}

#[async_trait]
impl PushStoreApi for MockPushStore {
    async fn get_pushers(&self, _user_id: &str, _device_id: Option<&str>) -> Result<Vec<PusherRow>, sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(Vec::new())
    }

    async fn upsert_pusher(
        &self,
        _user_id: &str,
        _device_id: &str,
        _pushkey: &str,
        _kind: &str,
        _app_id: &str,
        _app_display_name: &str,
        _device_display_name: &str,
        _profile_tag: &Option<String>,
        _lang: &str,
        _data: &Option<Value>,
        _now: i64,
    ) -> Result<(), sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(())
    }

    async fn delete_pusher(&self, _user_id: &str, _device_id: &str, _pushkey: &str) -> Result<(), sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(())
    }

    async fn upsert_push_rule(
        &self,
        _user_id: &str,
        _scope: &str,
        _kind: &str,
        _rule_id: &str,
        _pattern: &Option<String>,
        _conditions: &Option<Value>,
        _actions: &Value,
        _now: i64,
    ) -> Result<(), sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(())
    }

    async fn delete_push_rule(
        &self,
        _user_id: &str,
        _scope: &str,
        _kind: &str,
        _rule_id: &str,
    ) -> Result<u64, sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(state.delete_rule_rows)
    }

    async fn update_push_rule_actions(
        &self,
        _user_id: &str,
        _scope: &str,
        _kind: &str,
        _rule_id: &str,
        _actions: &Value,
    ) -> Result<(), sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(())
    }

    async fn get_push_rule_enabled(
        &self,
        _user_id: &str,
        _scope: &str,
        _kind: &str,
        _rule_id: &str,
    ) -> Result<Option<bool>, sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(state.enabled_value)
    }

    async fn set_push_rule_enabled(
        &self,
        _user_id: &str,
        _scope: &str,
        _kind: &str,
        _rule_id: &str,
        _enabled: bool,
    ) -> Result<(), sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(())
    }

    async fn get_user_push_rules(
        &self,
        _user_id: &str,
        _scope: &str,
        _kind: &str,
    ) -> Result<Vec<PushRuleRow>, sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(Vec::new())
    }

    async fn get_all_push_rules(&self, _user_id: &str) -> Result<Vec<PushRuleScopedRow>, sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(Vec::new())
    }

    async fn get_notifications(&self, _user_id: &str, _limit: i64) -> Result<Vec<NotificationRow>, sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(Vec::new())
    }

    async fn record_notification(
        &self,
        _user_id: &str,
        _event_id: Option<&str>,
        _room_id: Option<&str>,
        _notification_type: &str,
        _ts: i64,
    ) -> Result<(), sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(())
    }

    async fn ack_notification(&self, id: i64, _user_id: &str, _now: i64) -> Result<Option<i64>, sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        // C33: the return type is `Option<i64>` (the acked id) instead of
        // `Option<PgRow>`, so the `Ok(Some(..))` branch is finally constructible
        // in memory — `ack_returns_some` is honoured instead of discarded.
        Ok(if state.ack_returns_some { Some(id) } else { None })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn build_service(push: MockPushStore) -> ClientPushService {
    ClientPushService::new(Arc::new(push) as Arc<dyn PushStoreApi>)
}

fn sample_upsert_pusher_request() -> UpsertPusherRequest {
    UpsertPusherRequest {
        user_id: "@alice:localhost".to_string(),
        device_id: "DEV-001".to_string(),
        pushkey: "pk-abc".to_string(),
        kind: "http".to_string(),
        app_id: "com.example.app".to_string(),
        app_display_name: "My App".to_string(),
        device_display_name: "My Device".to_string(),
        profile_tag: Some("tag1".to_string()),
        lang: "en".to_string(),
        data: Some(json!({"url": "https://push.example.com"})),
    }
}

fn sample_upsert_push_rule_request() -> UpsertPushRuleRequest {
    UpsertPushRuleRequest {
        user_id: "@alice:localhost".to_string(),
        scope: "global".to_string(),
        kind: "override".to_string(),
        rule_id: "rule_1".to_string(),
        pattern: Some("spam".to_string()),
        conditions: Some(json!([{"kind": "event_match"}])),
        actions: json!(["notify"]),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Request DTO construction / cloning
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn upsert_pusher_request_construct_and_clone() {
    let req = sample_upsert_pusher_request();
    let cloned = req.clone();

    assert_eq!(req.user_id, cloned.user_id);
    assert_eq!(req.device_id, cloned.device_id);
    assert_eq!(req.pushkey, cloned.pushkey);
    assert_eq!(req.kind, cloned.kind);
    assert_eq!(req.app_id, cloned.app_id);
    assert_eq!(req.app_display_name, cloned.app_display_name);
    assert_eq!(req.device_display_name, cloned.device_display_name);
    assert_eq!(req.profile_tag, cloned.profile_tag);
    assert_eq!(req.lang, cloned.lang);
    assert_eq!(req.data, cloned.data);
}

#[test]
fn upsert_push_rule_request_construct_and_clone() {
    let req = sample_upsert_push_rule_request();
    let cloned = req.clone();

    assert_eq!(req.user_id, cloned.user_id);
    assert_eq!(req.scope, cloned.scope);
    assert_eq!(req.kind, cloned.kind);
    assert_eq!(req.rule_id, cloned.rule_id);
    assert_eq!(req.pattern, cloned.pattern);
    assert_eq!(req.conditions, cloned.conditions);
    assert_eq!(req.actions, cloned.actions);
}

// ─────────────────────────────────────────────────────────────────────────────
// get_push_rules_content — the `push_rules` table is the only input (I.2)
// ─────────────────────────────────────────────────────────────────────────────

/// The non-empty case (rows → grouped JSON document) is asserted as a whole
/// document by `client_push_service.rs`'s own unit test over
/// `InMemoryPushStore`; this mock cannot hold rows.
#[tokio::test]
async fn get_push_rules_content_returns_none_when_table_empty() {
    let service = build_service(MockPushStore::new());

    let result = service.get_push_rules_content("@alice:localhost").await.expect("should succeed");
    assert_eq!(result, None, "an empty push_rules table must yield None so callers fall back to defaults");
}

#[tokio::test]
async fn get_push_rules_content_maps_storage_error_to_internal() {
    let service = build_service(MockPushStore::with_failure());

    let err = service.get_push_rules_content("@alice:localhost").await.expect_err("should propagate storage error");
    assert!(err.is_internal(), "storage error must surface as ApiError::internal");
}

// ─────────────────────────────────────────────────────────────────────────────
// Row-returning methods — empty happy path (service maps empty Vec → empty Vec)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_pushers_returns_empty_vec_when_storage_empty() {
    let service = build_service(MockPushStore::new());

    let result = service.get_pushers("@alice:localhost", None).await.expect("should succeed");
    assert!(result.is_empty(), "empty pusher storage should yield empty JSON array");
}

#[tokio::test]
async fn get_pushers_with_device_filter_returns_empty() {
    let service = build_service(MockPushStore::new());

    let result = service.get_pushers("@alice:localhost", Some("DEV-1")).await.expect("should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn get_user_push_rules_returns_empty_vec_when_storage_empty() {
    let service = build_service(MockPushStore::new());

    let result = service.get_user_push_rules("@alice:localhost", "global", "override").await.expect("should succeed");
    assert!(result.is_empty(), "empty rule storage should yield empty JSON array");
}

#[tokio::test]
async fn get_notifications_returns_empty_vec_when_storage_empty() {
    let service = build_service(MockPushStore::new());

    let result = service.get_notifications("@alice:localhost", 20).await.expect("should succeed");
    assert!(result.is_empty(), "empty notification storage should yield empty JSON array");
}

// ─────────────────────────────────────────────────────────────────────────────
// Primitive-returning methods — happy path (value mapping)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_pusher_returns_timestamp_on_success() {
    let service = build_service(MockPushStore::new());
    let before = synapse_common::current_timestamp_millis();

    let ts = service.upsert_pusher(sample_upsert_pusher_request()).await.expect("should succeed");

    assert!(ts >= before, "upsert_pusher should return the current timestamp");
    assert!(ts <= synapse_common::current_timestamp_millis() + 5_000);
}

#[tokio::test]
async fn delete_pusher_returns_ok_on_success() {
    let service = build_service(MockPushStore::new());

    service.delete_pusher("@alice:localhost", "DEV-1", "pk-abc").await.expect("should succeed on empty delete");
}

#[tokio::test]
async fn upsert_push_rule_returns_timestamp_on_success() {
    let service = build_service(MockPushStore::new());
    let before = synapse_common::current_timestamp_millis();

    let ts = service.upsert_push_rule(sample_upsert_push_rule_request()).await.expect("should succeed");

    assert!(ts >= before, "upsert_push_rule should return the current timestamp");
}

#[tokio::test]
async fn delete_push_rule_returns_false_when_zero_rows_affected() {
    let service = build_service(MockPushStore::with_delete_rows(0));

    let deleted =
        service.delete_push_rule("@alice:localhost", "global", "override", "rule_x").await.expect("should succeed");
    assert!(!deleted, "zero rows affected should map to false");
}

#[tokio::test]
async fn delete_push_rule_returns_true_when_rows_affected() {
    let service = build_service(MockPushStore::with_delete_rows(1));

    let deleted =
        service.delete_push_rule("@alice:localhost", "global", "override", "rule_x").await.expect("should succeed");
    assert!(deleted, "non-zero rows affected should map to true");
}

#[tokio::test]
async fn set_push_rule_actions_returns_ok_on_success() {
    let service = build_service(MockPushStore::new());
    let actions = json!(["dont_notify"]);

    service
        .set_push_rule_actions("@alice:localhost", "global", "override", "rule_1", &actions)
        .await
        .expect("should succeed");
}

#[tokio::test]
async fn get_push_rule_enabled_returns_none_when_storage_returns_none() {
    let service = build_service(MockPushStore::with_enabled(None));

    let result = service
        .get_push_rule_enabled("@alice:localhost", "global", "override", "rule_1")
        .await
        .expect("should succeed");
    assert_eq!(result, None, "missing rule should yield None (not an error)");
}

#[tokio::test]
async fn get_push_rule_enabled_returns_some_when_storage_returns_some() {
    let service = build_service(MockPushStore::with_enabled(Some(false)));

    let result = service
        .get_push_rule_enabled("@alice:localhost", "global", "override", "rule_1")
        .await
        .expect("should succeed");
    assert_eq!(result, Some(false));
}

#[tokio::test]
async fn set_push_rule_enabled_returns_ok_on_success() {
    let service = build_service(MockPushStore::new());

    service
        .set_push_rule_enabled("@alice:localhost", "global", "override", "rule_1", true)
        .await
        .expect("should succeed");
}

#[tokio::test]
async fn ack_notification_returns_true_when_storage_acks_a_row() {
    // C33：`ack_notification` 现在返回 `Option<i64>`，成功分支可以造出来
    // （此前是 `Option<PgRow>`，内存 mock 只能走 `None` 分支）。
    let service = build_service(MockPushStore::with_ack_some());
    let acked = service.ack_notification(42, "@alice:localhost").await.expect("should succeed");
    assert!(acked, "storage returning Some(id) must map to Ok(true)");
}

#[tokio::test]
async fn ack_notification_returns_false_when_storage_returns_none() {
    let service = build_service(MockPushStore::new());

    let acked = service.ack_notification(42, "@alice:localhost").await.expect("should succeed");
    assert!(!acked, "storage returning Ok(None) must map to Ok(false)");
}

// ─────────────────────────────────────────────────────────────────────────────
// Error path — every storage error surfaces as ApiError::internal
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_pushers_maps_storage_error_to_internal() {
    let service = build_service(MockPushStore::with_failure());
    let err = service.get_pushers("@alice:localhost", None).await.expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn upsert_pusher_maps_storage_error_to_internal() {
    let service = build_service(MockPushStore::with_failure());
    let err = service.upsert_pusher(sample_upsert_pusher_request()).await.expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn delete_pusher_maps_storage_error_to_internal() {
    let service = build_service(MockPushStore::with_failure());
    let err = service.delete_pusher("@alice:localhost", "DEV-1", "pk-abc").await.expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn get_user_push_rules_maps_storage_error_to_internal() {
    let service = build_service(MockPushStore::with_failure());
    let err = service
        .get_user_push_rules("@alice:localhost", "global", "override")
        .await
        .expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn upsert_push_rule_maps_storage_error_to_internal() {
    let service = build_service(MockPushStore::with_failure());
    let err = service.upsert_push_rule(sample_upsert_push_rule_request()).await.expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn delete_push_rule_maps_storage_error_to_internal() {
    let service = build_service(MockPushStore::with_failure());
    let err = service
        .delete_push_rule("@alice:localhost", "global", "override", "rule_1")
        .await
        .expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn set_push_rule_actions_maps_storage_error_to_internal() {
    let service = build_service(MockPushStore::with_failure());
    let actions = json!(["notify"]);
    let err = service
        .set_push_rule_actions("@alice:localhost", "global", "override", "rule_1", &actions)
        .await
        .expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn get_push_rule_enabled_maps_storage_error_to_internal() {
    let service = build_service(MockPushStore::with_failure());
    let err = service
        .get_push_rule_enabled("@alice:localhost", "global", "override", "rule_1")
        .await
        .expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn set_push_rule_enabled_maps_storage_error_to_internal() {
    let service = build_service(MockPushStore::with_failure());
    let err = service
        .set_push_rule_enabled("@alice:localhost", "global", "override", "rule_1", true)
        .await
        .expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn get_notifications_maps_storage_error_to_internal() {
    let service = build_service(MockPushStore::with_failure());
    let err = service.get_notifications("@alice:localhost", 10).await.expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn ack_notification_maps_storage_error_to_internal() {
    let service = build_service(MockPushStore::with_failure());
    let err = service.ack_notification(42, "@alice:localhost").await.expect_err("should propagate error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// Boundary / argument-passthrough smoke tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_notifications_passes_limit_to_storage_without_error() {
    // limit=0 is a degenerate but valid call; storage mock returns empty vec.
    let service = build_service(MockPushStore::new());
    let result = service.get_notifications("@alice:localhost", 0).await.expect("should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn delete_push_rule_for_nonexistent_user_returns_false() {
    // Mirrors the storage db_tests invariant: deleting a non-existent rule
    // is not an error, it just returns Ok(false).
    let service = build_service(MockPushStore::with_delete_rows(0));
    let deleted = service
        .delete_push_rule("@nobody:localhost", "global", "override", "missing")
        .await
        .expect("should succeed for non-existent rule");
    assert!(!deleted);
}
