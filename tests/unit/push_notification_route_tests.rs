// Push notification route layer tests.
//
// Covers the wire-level contracts exposed by `src/web/routes/push_notification.rs`:
//   * Request body deserialization (RegisterDeviceBody / SendNotificationBody / CreateRuleBody)
//   * Query-string deserialization (ProcessQueueQuery / CleanupQuery)
//   * Path-parameter deserialization (RulePath)
//   * Response struct serialization + From<PushDevice> / From<PushRule> conversions
//   * Route manifest contents (methods + paths + registered_by tag)
//   * batch_size / days default + clamping semantics documented in handlers
//
// HTTP status-level coverage (200 / 401 / 400 / 404 per endpoint) is exercised
// through the integration `TestContext` harness (real router + admin auth
// middleware + isolated DB), which lives under `tests/integration/`. Unit
// tests here follow the same pattern as the existing `push_api_tests.rs`
// (wire-contract focused, no database required) so they run hermetically.

use axum::http::Method;
use serde_json::json;
use synapse_rust::web::routes::push_notification::{
    CleanupQuery, CreateRuleBody, DeviceResponse, ProcessQueueQuery, RegisterDeviceBody, RulePath, RuleResponse,
    SendNotificationBody,
};
use synapse_rust::web::routes::route_ledger::RouteEntry;
use synapse_storage::push_notification::{PushDevice, PushRule};

// ─────────────────────────────────────────────────────────────────────────────
// Request body deserialization — RegisterDeviceBody
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn register_device_body_deserializes_full_payload() {
    let payload = json!({
        "device_id": "DEV-001",
        "push_token": "tok-abc",
        "push_type": "fcm",
        "app_id": "com.example.app",
        "platform": "ios",
        "platform_version": "17.0",
        "app_version": "1.2.3",
        "locale": "zh-CN",
        "timezone": "Asia/Shanghai"
    });

    let body: RegisterDeviceBody = serde_json::from_value(payload).expect("full payload should deserialize");
    assert_eq!(body.device_id, "DEV-001");
    assert_eq!(body.push_token, "tok-abc");
    assert_eq!(body.push_type, "fcm");
    assert_eq!(body.app_id.as_deref(), Some("com.example.app"));
    assert_eq!(body.platform.as_deref(), Some("ios"));
    assert_eq!(body.platform_version.as_deref(), Some("17.0"));
    assert_eq!(body.app_version.as_deref(), Some("1.2.3"));
    assert_eq!(body.locale.as_deref(), Some("zh-CN"));
    assert_eq!(body.timezone.as_deref(), Some("Asia/Shanghai"));
}

#[test]
fn register_device_body_accepts_minimal_required_fields() {
    let payload = json!({
        "device_id": "DEV-002",
        "push_token": "tok-min",
        "push_type": "apns"
    });

    let body: RegisterDeviceBody = serde_json::from_value(payload).expect("minimal payload should deserialize");
    assert_eq!(body.device_id, "DEV-002");
    assert_eq!(body.push_token, "tok-min");
    assert_eq!(body.push_type, "apns");
    assert!(body.app_id.is_none());
    assert!(body.platform.is_none());
    assert!(body.platform_version.is_none());
    assert!(body.app_version.is_none());
    assert!(body.locale.is_none());
    assert!(body.timezone.is_none());
}

#[test]
fn register_device_body_rejects_missing_device_id() {
    let payload = json!({
        "push_token": "tok",
        "push_type": "fcm"
    });

    let err = serde_json::from_value::<RegisterDeviceBody>(payload);
    assert!(err.is_err(), "missing device_id must fail deserialization");
}

#[test]
fn register_device_body_rejects_missing_push_token() {
    let payload = json!({
        "device_id": "DEV",
        "push_type": "fcm"
    });

    let err = serde_json::from_value::<RegisterDeviceBody>(payload);
    assert!(err.is_err(), "missing push_token must fail deserialization");
}

#[test]
fn register_device_body_rejects_missing_push_type() {
    let payload = json!({
        "device_id": "DEV",
        "push_token": "tok"
    });

    let err = serde_json::from_value::<RegisterDeviceBody>(payload);
    assert!(err.is_err(), "missing push_type must fail deserialization");
}

// ─────────────────────────────────────────────────────────────────────────────
// Request body deserialization — SendNotificationBody
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn send_notification_body_deserializes_full_payload() {
    let payload = json!({
        "device_id": "DEV-001",
        "event_id": "$ev:server",
        "room_id": "!room:server",
        "notification_type": "message",
        "title": "Hello",
        "body": "World",
        "data": {"badge": 3, "sound": "default"},
        "priority": 5
    });

    let body: SendNotificationBody = serde_json::from_value(payload).expect("full payload should deserialize");
    assert_eq!(body.device_id.as_deref(), Some("DEV-001"));
    assert_eq!(body.event_id.as_deref(), Some("$ev:server"));
    assert_eq!(body.room_id.as_deref(), Some("!room:server"));
    assert_eq!(body.notification_type.as_deref(), Some("message"));
    assert_eq!(body.title, "Hello");
    assert_eq!(body.body, "World");
    assert_eq!(body.data, Some(json!({"badge": 3, "sound": "default"})));
    assert_eq!(body.priority, Some(5));
}

#[test]
fn send_notification_body_accepts_minimal_required_fields() {
    let payload = json!({
        "title": "t",
        "body": "b"
    });

    let body: SendNotificationBody = serde_json::from_value(payload).expect("minimal payload should deserialize");
    assert_eq!(body.title, "t");
    assert_eq!(body.body, "b");
    assert!(body.device_id.is_none());
    assert!(body.event_id.is_none());
    assert!(body.room_id.is_none());
    assert!(body.notification_type.is_none());
    assert!(body.data.is_none());
    assert!(body.priority.is_none());
}

#[test]
fn send_notification_body_rejects_missing_title() {
    let payload = json!({"body": "b"});

    let err = serde_json::from_value::<SendNotificationBody>(payload);
    assert!(err.is_err(), "missing title must fail deserialization");
}

#[test]
fn send_notification_body_rejects_missing_body() {
    let payload = json!({"title": "t"});

    let err = serde_json::from_value::<SendNotificationBody>(payload);
    assert!(err.is_err(), "missing body must fail deserialization");
}

// ─────────────────────────────────────────────────────────────────────────────
// Request body deserialization — CreateRuleBody
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn create_rule_body_deserializes_full_payload() {
    let payload = json!({
        "rule_id": "rule_1",
        "scope": "global",
        "kind": "override",
        "priority": 5,
        "conditions": [{"kind": "event_match", "key": "content.body", "pattern": "spam"}],
        "actions": ["notify", {"set_tweak": "highlight", "value": true}],
        "enabled": true
    });

    let body: CreateRuleBody = serde_json::from_value(payload).expect("full payload should deserialize");
    assert_eq!(body.rule_id, "rule_1");
    assert_eq!(body.scope, "global");
    assert_eq!(body.kind, "override");
    assert_eq!(body.priority, 5);
    assert!(body.conditions.is_array());
    assert!(body.actions.is_array());
    assert!(body.enabled);
}

#[test]
fn create_rule_body_rejects_missing_rule_id() {
    let payload = json!({
        "scope": "global",
        "kind": "override",
        "priority": 5,
        "conditions": [],
        "actions": [],
        "enabled": true
    });

    let err = serde_json::from_value::<CreateRuleBody>(payload);
    assert!(err.is_err(), "missing rule_id must fail deserialization");
}

#[test]
fn create_rule_body_rejects_missing_scope() {
    let payload = json!({
        "rule_id": "r",
        "kind": "override",
        "priority": 5,
        "conditions": [],
        "actions": [],
        "enabled": true
    });

    let err = serde_json::from_value::<CreateRuleBody>(payload);
    assert!(err.is_err(), "missing scope must fail deserialization");
}

#[test]
fn create_rule_body_rejects_missing_kind() {
    let payload = json!({
        "rule_id": "r",
        "scope": "global",
        "priority": 5,
        "conditions": [],
        "actions": [],
        "enabled": true
    });

    let err = serde_json::from_value::<CreateRuleBody>(payload);
    assert!(err.is_err(), "missing kind must fail deserialization");
}

#[test]
fn create_rule_body_rejects_non_boolean_enabled() {
    let payload = json!({
        "rule_id": "r",
        "scope": "global",
        "kind": "override",
        "priority": 5,
        "conditions": [],
        "actions": [],
        "enabled": "yes"
    });

    let err = serde_json::from_value::<CreateRuleBody>(payload);
    assert!(err.is_err(), "non-boolean enabled must fail deserialization");
}

// ─────────────────────────────────────────────────────────────────────────────
// Path parameter deserialization — RulePath
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn rule_path_deserializes_three_segments() {
    // Simulates axum's Path extractor capturing /{scope}/{kind}/{rule_id}
    let raw = json!(["global", "override", "my.rule.v1"]);
    let path: RulePath = serde_json::from_value(raw).expect("three-segment path should deserialize");
    assert_eq!(path.scope, "global");
    assert_eq!(path.kind, "override");
    assert_eq!(path.rule_id, "my.rule.v1");
}

#[test]
fn rule_path_rejects_missing_segments() {
    let raw = json!(["global", "override"]);
    let err = serde_json::from_value::<RulePath>(raw);
    assert!(err.is_err(), "two-segment path must fail RulePath deserialization");
}

// ─────────────────────────────────────────────────────────────────────────────
// Query parameter deserialization — ProcessQueueQuery / CleanupQuery
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn process_queue_query_defaults_batch_size_to_none() {
    // serde_qs-style or axum Query: empty query string → all fields None.
    let q: ProcessQueueQuery =
        serde_json::from_str("{}").expect("empty object should deserialize with None batch_size");
    assert!(q.batch_size.is_none());
}

#[test]
fn process_queue_query_parses_batch_size() {
    let q: ProcessQueueQuery =
        serde_json::from_str(r#"{"batch_size": 250}"#).expect("batch_size should parse as i32");
    assert_eq!(q.batch_size, Some(250));
}

#[test]
fn process_queue_query_rejects_non_numeric_batch_size() {
    let err = serde_json::from_str::<ProcessQueueQuery>(r#"{"batch_size": "lots"}"#);
    assert!(err.is_err(), "non-numeric batch_size must fail deserialization");
}

#[test]
fn cleanup_query_defaults_days_to_none() {
    let q: CleanupQuery = serde_json::from_str("{}").expect("empty object should deserialize with None days");
    assert!(q.days.is_none());
}

#[test]
fn cleanup_query_parses_days() {
    let q: CleanupQuery = serde_json::from_str(r#"{"days": 14}"#).expect("days should parse as i32");
    assert_eq!(q.days, Some(14));
}

// ─────────────────────────────────────────────────────────────────────────────
// batch_size / days resolution semantics (mirrors handler logic).
//
// The handler inlines `query.batch_size.unwrap_or(100).clamp(1, 500)` and
// `query.days.unwrap_or(30).clamp(1, 200)`. We replicate that expression here
// to document and lock the contract — if the source changes these bounds or
// defaults, this test must be updated in lockstep.
// ─────────────────────────────────────────────────────────────────────────────

fn resolve_batch_size(query: &ProcessQueueQuery) -> i32 {
    query.batch_size.unwrap_or(100).clamp(1, 500)
}

fn resolve_days(query: &CleanupQuery) -> i32 {
    query.days.unwrap_or(30).clamp(1, 200)
}

#[test]
fn batch_size_uses_default_when_absent() {
    let q = ProcessQueueQuery { batch_size: None };
    assert_eq!(resolve_batch_size(&q), 100);
}

#[test]
fn batch_size_clamps_to_minimum() {
    let q = ProcessQueueQuery { batch_size: Some(0) };
    assert_eq!(resolve_batch_size(&q), 1);
}

#[test]
fn batch_size_clamps_to_maximum() {
    let q = ProcessQueueQuery { batch_size: Some(10_000) };
    assert_eq!(resolve_batch_size(&q), 500);
}

#[test]
fn batch_size_passes_through_in_range() {
    let q = ProcessQueueQuery { batch_size: Some(250) };
    assert_eq!(resolve_batch_size(&q), 250);
}

#[test]
fn days_uses_default_when_absent() {
    let q = CleanupQuery { days: None };
    assert_eq!(resolve_days(&q), 30);
}

#[test]
fn days_clamps_to_minimum() {
    let q = CleanupQuery { days: Some(0) };
    assert_eq!(resolve_days(&q), 1);
}

#[test]
fn days_clamps_to_maximum() {
    let q = CleanupQuery { days: Some(365) };
    assert_eq!(resolve_days(&q), 200);
}

#[test]
fn days_passes_through_in_range() {
    let q = CleanupQuery { days: Some(45) };
    assert_eq!(resolve_days(&q), 45);
}

// ─────────────────────────────────────────────────────────────────────────────
// Response struct — DeviceResponse + From<PushDevice>
// ─────────────────────────────────────────────────────────────────────────────

fn sample_push_device() -> PushDevice {
    PushDevice {
        id: 42,
        user_id: "@alice:localhost".into(),
        device_id: "DEV-001".into(),
        push_token: "tok-abc".into(),
        push_type: "fcm".into(),
        app_id: Some("com.example.app".into()),
        platform: Some("android".into()),
        platform_version: Some("14.0".into()),
        app_version: Some("1.2.3".into()),
        locale: Some("en-US".into()),
        timezone: Some("UTC".into()),
        is_enabled: true,
        created_ts: 1_700_000_000_000,
        updated_ts: Some(1_700_000_500_000),
        last_used_ts: Some(1_700_000_400_000),
        last_error: None,
        error_count: 0,
        metadata: json!({}),
    }
}

#[test]
fn device_response_from_push_device_maps_relevant_fields() {
    let device = sample_push_device();
    let resp = DeviceResponse::from(device);

    assert_eq!(resp.device_id, "DEV-001");
    assert_eq!(resp.push_type, "fcm");
    assert_eq!(resp.platform.as_deref(), Some("android"));
    assert!(resp.enabled);
    assert_eq!(resp.created_ts, 1_700_000_000_000);
    assert_eq!(resp.last_used_ts, Some(1_700_000_400_000));
}

#[test]
fn device_response_handles_null_last_used_ts() {
    let mut device = sample_push_device();
    device.last_used_ts = None;
    let resp = DeviceResponse::from(device);
    assert!(resp.last_used_ts.is_none());
}

#[test]
fn device_response_serializes_expected_json_shape() {
    let device = sample_push_device();
    let resp = DeviceResponse::from(device);
    let json_value = serde_json::to_value(&resp).expect("DeviceResponse should serialize");

    assert_eq!(json_value["device_id"], "DEV-001");
    assert_eq!(json_value["push_type"], "fcm");
    assert_eq!(json_value["platform"], "android");
    assert_eq!(json_value["enabled"], true);
    assert_eq!(json_value["created_ts"], 1_700_000_000_000_i64);
    assert_eq!(json_value["last_used_ts"], 1_700_000_400_000_i64);
}

#[test]
fn device_response_ignores_internal_only_fields() {
    // Internal-only fields (push_token, error_count, last_error, metadata, ...)
    // must NOT be surfaced in the public response.
    let device = sample_push_device();
    let resp = DeviceResponse::from(device);
    let json_value = serde_json::to_value(&resp).expect("DeviceResponse should serialize");
    let obj = json_value.as_object().expect("serialized value should be an object");
    assert!(!obj.contains_key("push_token"), "push_token must not leak into response");
    assert!(!obj.contains_key("error_count"), "error_count must not leak into response");
    assert!(!obj.contains_key("last_error"), "last_error must not leak into response");
    assert!(!obj.contains_key("metadata"), "metadata must not leak into response");
    assert!(!obj.contains_key("user_id"), "user_id must not leak into response");
}

// ─────────────────────────────────────────────────────────────────────────────
// Response struct — RuleResponse + From<PushRule>
// ─────────────────────────────────────────────────────────────────────────────

fn sample_push_rule() -> PushRule {
    PushRule {
        id: 7,
        user_id: "@alice:localhost".into(),
        rule_id: "rule_1".into(),
        scope: "global".into(),
        kind: "override".into(),
        priority: 5,
        priority_class: 5,
        conditions: json!([{"kind": "event_match"}]),
        actions: json!(["notify", {"set_tweak": "highlight"}]),
        is_enabled: true,
        is_default: false,
        created_ts: 1_700_000_000_000,
        updated_ts: None,
        pattern: None,
    }
}

#[test]
fn rule_response_from_push_rule_maps_relevant_fields() {
    let rule = sample_push_rule();
    let resp = RuleResponse::from(rule);

    assert_eq!(resp.rule_id, "rule_1");
    assert_eq!(resp.scope, "global");
    assert_eq!(resp.kind, "override");
    assert_eq!(resp.priority, 5);
    assert!(resp.conditions.is_array());
    assert!(resp.actions.is_array());
    assert!(resp.enabled);
}

#[test]
fn rule_response_serializes_expected_json_shape() {
    let rule = sample_push_rule();
    let resp = RuleResponse::from(rule);
    let json_value = serde_json::to_value(&resp).expect("RuleResponse should serialize");

    assert_eq!(json_value["rule_id"], "rule_1");
    assert_eq!(json_value["scope"], "global");
    assert_eq!(json_value["kind"], "override");
    assert_eq!(json_value["priority"], 5);
    assert_eq!(json_value["enabled"], true);
    assert!(json_value["conditions"].is_array());
    assert!(json_value["actions"].is_array());
}

#[test]
fn rule_response_renames_is_enabled_to_enabled() {
    // Storage exposes `is_enabled`; the wire contract uses `enabled`.
    let rule = sample_push_rule();
    let resp = RuleResponse::from(rule);
    let json_value = serde_json::to_value(&resp).expect("RuleResponse should serialize");
    let obj = json_value.as_object().expect("serialized value should be an object");
    assert!(obj.contains_key("enabled"), "rule response should expose `enabled`");
    assert!(!obj.contains_key("is_enabled"), "rule response must not expose `is_enabled`");
    assert!(!obj.contains_key("is_default"), "is_default must not leak into response");
    assert!(!obj.contains_key("user_id"), "user_id must not leak into response");
}

// ─────────────────────────────────────────────────────────────────────────────
// Route manifest — every endpoint registered with correct method/path/tag
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn push_notification_route_manifest_contains_all_endpoints() {
    let manifest = synapse_rust::web::routes::push_notification::push_notification_route_manifest();

    let mut seen: Vec<(Method, &str)> = manifest.iter().map(|e| (e.method.clone(), e.path)).collect();
    seen.sort_by(|a, b| a.1.cmp(b.1).then_with(|| format!("{:?}", a.0).cmp(&format!("{:?}", b.0))));

    let expected: &[(Method, &str)] = &[
        (Method::DELETE, "/_matrix/client/r0/push/devices/{device_id}"),
        (Method::DELETE, "/_matrix/client/r0/push/rules/{scope}/{kind}/{rule_id}"),
        (Method::GET, "/_matrix/client/r0/push/devices"),
        (Method::GET, "/_matrix/client/r0/push/rules"),
        (Method::POST, "/_matrix/client/r0/push/devices"),
        (Method::POST, "/_matrix/client/r0/push/rules"),
        (Method::POST, "/_matrix/client/r0/push/send"),
        (Method::POST, "/_synapse/admin/v1/push/cleanup"),
        (Method::POST, "/_synapse/admin/v1/push/process"),
    ];

    let mut expected_sorted: Vec<(Method, &str)> = expected.to_vec();
    expected_sorted.sort_by(|a, b| a.1.cmp(b.1).then_with(|| format!("{:?}", a.0).cmp(&format!("{:?}", b.0))));

    assert_eq!(seen.len(), expected_sorted.len(), "manifest entry count mismatch");
    for (got, want) in seen.iter().zip(expected_sorted.iter()) {
        assert_eq!(got.0, want.0, "method mismatch for path {}", want.1);
        assert_eq!(got.1, want.1, "path mismatch");
    }
}

#[test]
fn push_notification_route_manifest_tags_all_entries_push_notification() {
    let manifest = synapse_rust::web::routes::push_notification::push_notification_route_manifest();

    assert!(!manifest.is_empty(), "manifest should not be empty");
    for entry in &manifest {
        assert_eq!(
            entry.registered_by, "push_notification",
            "every manifest entry must be tagged registered_by=push_notification (path={})",
            entry.path
        );
    }
}

#[test]
fn push_notification_route_manifest_entries_are_unique() {
    // No accidental duplicate (method, path) registrations.
    let manifest = synapse_rust::web::routes::push_notification::push_notification_route_manifest();
    let mut keys: Vec<(String, &str)> = manifest.iter().map(|e| (format!("{:?}", e.method), e.path)).collect();
    let total = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), total, "duplicate (method, path) entries detected in manifest");
}

#[test]
fn push_notification_route_manifest_entries_are_route_entry_type() {
    // Smoke-test that the manifest returns RouteEntry values whose fields are
    // publicly accessible (the route_ledger surface contract).
    let manifest = synapse_rust::web::routes::push_notification::push_notification_route_manifest();
    let _: Vec<&RouteEntry> = manifest.iter().collect();
    for entry in &manifest {
        assert!(!entry.path.is_empty());
        assert!(!entry.registered_by.is_empty());
    }
}
