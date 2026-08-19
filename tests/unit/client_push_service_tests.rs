// Client push service unit tests.
//
// Exercises `synapse_services::client_push_service::ClientPushService` against
// in-memory mocks of its two storage dependencies
// (`AccountDataStoreApi` + `PushStoreApi`). Covers:
//   * Happy path for non-row-returning storage methods (returns Ok + value
//     mapping for `delete_push_rule`, `get_push_rule_enabled`, ...).
//   * Happy path for row-returning methods when storage is empty (verifies
//     the service maps empty `Vec<PgRow>` → empty `Vec<Value>` without error).
//   * Error path: every storage method that returns `Err` is mapped to an
//     `ApiError::internal` (verified via `ApiError::is_internal()`).
//   * Request DTO construction / cloning.
//
// Row-shape happy paths (PgRow with real columns) are exercised by the
// storage-layer integration tests in `synapse-storage/src/push/mod.rs::db_tests`
// against a real Postgres — they are intentionally NOT duplicated here, since
// `sqlx::postgres::PgRow` cannot be constructed outside a live connection.

use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::postgres::PgRow;
use std::sync::{Arc, Mutex};
use synapse_common::ApiError;
use synapse_services::client_push_service::{ClientPushService, UpsertPushRuleRequest, UpsertPusherRequest};
use synapse_storage::account_data::{AccountDataRecord, AccountDataStoreApi};
use synapse_storage::push::PushStoreApi;

// ─────────────────────────────────────────────────────────────────────────────
// Mock: AccountDataStoreApi
// ─────────────────────────────────────────────────────────────────────────────

/// In-memory fake for `AccountDataStoreApi`. Only `get_account_data_content`
/// is exercised by `ClientPushService`; the remaining trait methods return
/// their default empty/zero values so the trait object compiles.
#[derive(Debug, Default)]
struct MockAccountDataStore {
    state: Mutex<MockAccountDataState>,
}

#[derive(Debug, Default, Clone)]
struct MockAccountDataState {
    /// When set, every method returns `Err`.
    fail_all: bool,
    /// Configured return for `get_account_data_content`.
    get_content: Option<Option<Value>>,
}

impl MockAccountDataStore {
    fn new() -> Self {
        Self::default()
    }

    fn with_failure() -> Self {
        let store = Self::new();
        *store.state.lock().unwrap() = MockAccountDataState { fail_all: true, get_content: None };
        store
    }

    fn with_content(content: Option<Value>) -> Self {
        let store = Self::new();
        *store.state.lock().unwrap() = MockAccountDataState { fail_all: false, get_content: Some(content) };
        store
    }
}

#[async_trait]
impl AccountDataStoreApi for MockAccountDataStore {
    async fn get_account_data_content(&self, _user_id: &str, _data_type: &str) -> Result<Option<Value>, ApiError> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(ApiError::internal_with_context("mock: forced failure", &"mock-error"));
        }
        Ok(state.get_content.unwrap_or(None))
    }

    async fn list_account_data(&self, _user_id: &str) -> Result<Vec<AccountDataRecord>, ApiError> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(ApiError::internal_with_context("mock: forced failure", &"mock-error"));
        }
        Ok(Vec::new())
    }

    async fn delete_account_data(&self, _user_id: &str, _data_type: &str) -> Result<bool, ApiError> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(ApiError::internal_with_context("mock: forced failure", &"mock-error"));
        }
        Ok(false)
    }

    async fn upsert_account_data(&self, _user_id: &str, _data_type: &str, _content: Value) -> Result<(), ApiError> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(ApiError::internal_with_context("mock: forced failure", &"mock-error"));
        }
        Ok(())
    }
}

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
    /// Configured return value for `ack_notification` — `true` here signals
    /// that a fake "some row" result should be returned. We can't build a
    /// real `PgRow`, so we instead test the `Ok(None)` branch (which the
    /// service maps to `Ok(false)`) and rely on the storage-layer db_tests
    /// for the `Ok(Some(row))` → `Ok(true)` branch.
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
    async fn get_pushers(&self, _user_id: &str, _device_id: Option<&str>) -> Result<Vec<PgRow>, sqlx::Error> {
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

    async fn get_user_push_rules(&self, _user_id: &str, _scope: &str, _kind: &str) -> Result<Vec<PgRow>, sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(Vec::new())
    }

    async fn get_notifications(&self, _user_id: &str, _limit: i64) -> Result<Vec<PgRow>, sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        Ok(Vec::new())
    }

    async fn ack_notification(&self, _id: i64, _user_id: &str, _now: i64) -> Result<Option<PgRow>, sqlx::Error> {
        let state = self.state.lock().unwrap().clone();
        if state.fail_all {
            return Err(storage_error());
        }
        // We only exercise the `Ok(None)` branch here — `Ok(Some(row))`
        // requires a live PgRow, covered by storage-layer db_tests.
        let _ = state.ack_returns_some;
        Ok(None)
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn build_service(account: MockAccountDataStore, push: MockPushStore) -> ClientPushService {
    ClientPushService::new(Arc::new(account) as Arc<dyn AccountDataStoreApi>, Arc::new(push) as Arc<dyn PushStoreApi>)
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
// get_push_rules_content — happy path (only touches account_data_storage)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_push_rules_content_returns_some_when_storage_has_data() {
    let content = json!({"global": {"override": []}});
    let service = build_service(MockAccountDataStore::with_content(Some(content.clone())), MockPushStore::new());

    let result = service.get_push_rules_content("@alice:localhost").await.expect("should succeed");
    assert_eq!(result, Some(content));
}

#[tokio::test]
async fn get_push_rules_content_returns_none_when_storage_empty() {
    let service = build_service(MockAccountDataStore::with_content(None), MockPushStore::new());

    let result = service.get_push_rules_content("@alice:localhost").await.expect("should succeed");
    assert_eq!(result, None);
}

#[tokio::test]
async fn get_push_rules_content_maps_storage_error_to_internal() {
    let service = build_service(MockAccountDataStore::with_failure(), MockPushStore::new());

    let err = service.get_push_rules_content("@alice:localhost").await.expect_err("should propagate storage error");
    assert!(err.is_internal(), "storage error must surface as ApiError::internal");
}

// ─────────────────────────────────────────────────────────────────────────────
// Row-returning methods — empty happy path (service maps empty Vec → empty Vec)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_pushers_returns_empty_vec_when_storage_empty() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::new());

    let result = service.get_pushers("@alice:localhost", None).await.expect("should succeed");
    assert!(result.is_empty(), "empty pusher storage should yield empty JSON array");
}

#[tokio::test]
async fn get_pushers_with_device_filter_returns_empty() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::new());

    let result = service.get_pushers("@alice:localhost", Some("DEV-1")).await.expect("should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn get_user_push_rules_returns_empty_vec_when_storage_empty() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::new());

    let result = service.get_user_push_rules("@alice:localhost", "global", "override").await.expect("should succeed");
    assert!(result.is_empty(), "empty rule storage should yield empty JSON array");
}

#[tokio::test]
async fn get_notifications_returns_empty_vec_when_storage_empty() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::new());

    let result = service.get_notifications("@alice:localhost", 20).await.expect("should succeed");
    assert!(result.is_empty(), "empty notification storage should yield empty JSON array");
}

// ─────────────────────────────────────────────────────────────────────────────
// Primitive-returning methods — happy path (value mapping)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn upsert_pusher_returns_timestamp_on_success() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::new());
    let before = synapse_common::current_timestamp_millis();

    let ts = service.upsert_pusher(sample_upsert_pusher_request()).await.expect("should succeed");

    assert!(ts >= before, "upsert_pusher should return the current timestamp");
    assert!(ts <= synapse_common::current_timestamp_millis() + 5_000);
}

#[tokio::test]
async fn delete_pusher_returns_ok_on_success() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::new());

    service.delete_pusher("@alice:localhost", "DEV-1", "pk-abc").await.expect("should succeed on empty delete");
}

#[tokio::test]
async fn upsert_push_rule_returns_timestamp_on_success() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::new());
    let before = synapse_common::current_timestamp_millis();

    let ts = service.upsert_push_rule(sample_upsert_push_rule_request()).await.expect("should succeed");

    assert!(ts >= before, "upsert_push_rule should return the current timestamp");
}

#[tokio::test]
async fn delete_push_rule_returns_false_when_zero_rows_affected() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_delete_rows(0));

    let deleted =
        service.delete_push_rule("@alice:localhost", "global", "override", "rule_x").await.expect("should succeed");
    assert!(!deleted, "zero rows affected should map to false");
}

#[tokio::test]
async fn delete_push_rule_returns_true_when_rows_affected() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_delete_rows(1));

    let deleted =
        service.delete_push_rule("@alice:localhost", "global", "override", "rule_x").await.expect("should succeed");
    assert!(deleted, "non-zero rows affected should map to true");
}

#[tokio::test]
async fn set_push_rule_actions_returns_ok_on_success() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::new());
    let actions = json!(["dont_notify"]);

    service
        .set_push_rule_actions("@alice:localhost", "global", "override", "rule_1", &actions)
        .await
        .expect("should succeed");
}

#[tokio::test]
async fn get_push_rule_enabled_returns_none_when_storage_returns_none() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_enabled(None));

    let result = service
        .get_push_rule_enabled("@alice:localhost", "global", "override", "rule_1")
        .await
        .expect("should succeed");
    assert_eq!(result, None, "missing rule should yield None (not an error)");
}

#[tokio::test]
async fn get_push_rule_enabled_returns_some_when_storage_returns_some() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_enabled(Some(false)));

    let result = service
        .get_push_rule_enabled("@alice:localhost", "global", "override", "rule_1")
        .await
        .expect("should succeed");
    assert_eq!(result, Some(false));
}

#[tokio::test]
async fn set_push_rule_enabled_returns_ok_on_success() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::new());

    service
        .set_push_rule_enabled("@alice:localhost", "global", "override", "rule_1", true)
        .await
        .expect("should succeed");
}

#[tokio::test]
async fn ack_notification_returns_false_when_storage_returns_none() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::new());

    let acked = service.ack_notification(42, "@alice:localhost").await.expect("should succeed");
    assert!(!acked, "storage returning Ok(None) must map to Ok(false)");
}

// ─────────────────────────────────────────────────────────────────────────────
// Error path — every storage error surfaces as ApiError::internal
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_pushers_maps_storage_error_to_internal() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_failure());
    let err = service.get_pushers("@alice:localhost", None).await.expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn upsert_pusher_maps_storage_error_to_internal() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_failure());
    let err = service.upsert_pusher(sample_upsert_pusher_request()).await.expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn delete_pusher_maps_storage_error_to_internal() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_failure());
    let err = service.delete_pusher("@alice:localhost", "DEV-1", "pk-abc").await.expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn get_user_push_rules_maps_storage_error_to_internal() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_failure());
    let err = service
        .get_user_push_rules("@alice:localhost", "global", "override")
        .await
        .expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn upsert_push_rule_maps_storage_error_to_internal() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_failure());
    let err = service.upsert_push_rule(sample_upsert_push_rule_request()).await.expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn delete_push_rule_maps_storage_error_to_internal() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_failure());
    let err = service
        .delete_push_rule("@alice:localhost", "global", "override", "rule_1")
        .await
        .expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn set_push_rule_actions_maps_storage_error_to_internal() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_failure());
    let actions = json!(["notify"]);
    let err = service
        .set_push_rule_actions("@alice:localhost", "global", "override", "rule_1", &actions)
        .await
        .expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn get_push_rule_enabled_maps_storage_error_to_internal() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_failure());
    let err = service
        .get_push_rule_enabled("@alice:localhost", "global", "override", "rule_1")
        .await
        .expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn set_push_rule_enabled_maps_storage_error_to_internal() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_failure());
    let err = service
        .set_push_rule_enabled("@alice:localhost", "global", "override", "rule_1", true)
        .await
        .expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn get_notifications_maps_storage_error_to_internal() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_failure());
    let err = service.get_notifications("@alice:localhost", 10).await.expect_err("should propagate error");
    assert!(err.is_internal());
}

#[tokio::test]
async fn ack_notification_maps_storage_error_to_internal() {
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_failure());
    let err = service.ack_notification(42, "@alice:localhost").await.expect_err("should propagate error");
    assert!(err.is_internal());
}

// ─────────────────────────────────────────────────────────────────────────────
// Boundary / argument-passthrough smoke tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn get_notifications_passes_limit_to_storage_without_error() {
    // limit=0 is a degenerate but valid call; storage mock returns empty vec.
    let service = build_service(MockAccountDataStore::new(), MockPushStore::new());
    let result = service.get_notifications("@alice:localhost", 0).await.expect("should succeed");
    assert!(result.is_empty());
}

#[tokio::test]
async fn delete_push_rule_for_nonexistent_user_returns_false() {
    // Mirrors the storage db_tests invariant: deleting a non-existent rule
    // is not an error, it just returns Ok(false).
    let service = build_service(MockAccountDataStore::new(), MockPushStore::with_delete_rows(0));
    let deleted = service
        .delete_push_rule("@nobody:localhost", "global", "override", "missing")
        .await
        .expect("should succeed for non-existent rule");
    assert!(!deleted);
}
