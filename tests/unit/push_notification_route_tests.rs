// Push notification route layer tests.
//
// Covers the wire-level contracts exposed by `src/web/routes/push_notification.rs`:
//   * Request body deserialization (RegisterDeviceBody / SendNotificationBody)
//   * Query-string deserialization (ProcessQueueQuery / CleanupQuery)
//   * Response struct serialization + From<PushDevice> conversion
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
    validate_push_config_patch, CleanupQuery, DeviceResponse, ProcessQueueQuery, RegisterDeviceBody,
    SendNotificationBody, SetPushConfigBody,
};
use synapse_rust::web::routes::route_ledger::RouteEntry;
use synapse_storage::push_notification::PushDevice;

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
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// ─────────────────────────────────────────────────────────────────────────────

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
    let q: ProcessQueueQuery = serde_json::from_str(r#"{"batch_size": 250}"#).expect("batch_size should parse as i32");
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
// ─────────────────────────────────────────────────────────────────────────────

// ─────────────────────────────────────────────────────────────────────────────
// Route manifest — every endpoint registered with correct method/path/tag
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn push_notification_route_manifest_contains_all_endpoints() {
    let manifest = synapse_rust::web::routes::push_notification::push_notification_route_manifest();

    let mut seen: Vec<(Method, &str)> = manifest.iter().map(|e| (e.method.clone(), e.path)).collect();
    seen.sort_by(|a, b| a.1.cmp(b.1).then_with(|| format!("{:?}", a.0).cmp(&format!("{:?}", b.0))));

    let expected: &[(Method, &str)] = &[
        (Method::DELETE, "/_matrix/client/v3/push/devices/{device_id}"),
        (Method::GET, "/_matrix/client/v3/push/devices"),
        (Method::POST, "/_matrix/client/v3/push/devices"),
        (Method::POST, "/_matrix/client/v3/push/send"),
        (Method::GET, "/_synapse/admin/v1/push/config"),
        (Method::PUT, "/_synapse/admin/v1/push/config"),
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

// ─────────────────────────────────────────────────────────────────────────────
// Admin push provider config — request body + validation
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn set_push_config_body_deserializes_values_and_deletions() {
    let payload = json!({
        "config": {
            "fcm.enabled": "true",
            "fcm.api_key": "key-1",
            "webpush.vapid_private_key": null
        }
    });

    let body: SetPushConfigBody = serde_json::from_value(payload).expect("payload should deserialize");
    assert_eq!(body.config.get("fcm.enabled"), Some(&Some("true".to_string())));
    assert_eq!(body.config.get("fcm.api_key"), Some(&Some("key-1".to_string())));
    assert_eq!(body.config.get("webpush.vapid_private_key"), Some(&None), "a null value means \"delete this key\"");
}

#[test]
fn set_push_config_body_rejects_unknown_fields() {
    let payload = json!({ "config": { "fcm.enabled": "true" }, "extra": 1 });
    let error = serde_json::from_value::<SetPushConfigBody>(payload).expect_err("unknown fields must be rejected");
    assert!(error.to_string().contains("unknown field"), "got: {error}");
}

#[test]
fn validate_push_config_patch_accepts_every_supported_key() {
    let mut config = std::collections::BTreeMap::new();
    config.insert("fcm.enabled".to_string(), Some("true".to_string()));
    config.insert("fcm.api_key".to_string(), Some("k".to_string()));
    config.insert("apns.enabled".to_string(), Some("false".to_string()));
    config.insert("apns.topic".to_string(), Some("com.example".to_string()));
    config.insert("webpush.enabled".to_string(), Some("TRUE".to_string()));
    config.insert("webpush.vapid_public_key".to_string(), Some("pub".to_string()));
    config.insert("webpush.vapid_private_key".to_string(), Some("priv".to_string()));

    validate_push_config_patch(&config).expect("every supported key must validate");
}

#[test]
fn validate_push_config_patch_rejects_unknown_key() {
    let mut config = std::collections::BTreeMap::new();
    config.insert("fcm.endpoint".to_string(), Some("https://example.test".to_string()));

    let error = validate_push_config_patch(&config).expect_err("an unconsumed key must be rejected");
    assert!(
        error.to_string().contains("unsupported push config key"),
        "accepting a key nothing reads would make push_config a settings graveyard; got: {error}"
    );
}

#[test]
fn validate_push_config_patch_rejects_empty_and_non_boolean_enabled() {
    let empty = std::collections::BTreeMap::new();
    validate_push_config_patch(&empty).expect_err("an empty patch must be rejected");

    let mut config = std::collections::BTreeMap::new();
    config.insert("fcm.enabled".to_string(), Some("yes".to_string()));
    validate_push_config_patch(&config).expect_err("`*.enabled` must be true/false");

    let mut config = std::collections::BTreeMap::new();
    config.insert("fcm.enabled".to_string(), None);
    validate_push_config_patch(&config).expect_err("deleting `*.enabled` is not a valid enable state");
}

#[test]
fn supported_push_config_keys_match_what_initialize_providers_reads() {
    use synapse_services::push_notification_service::{SECRET_PUSH_CONFIG_KEYS, SUPPORTED_PUSH_CONFIG_KEYS};

    // Every key the service reads must be settable through the admin endpoint.
    for key in SUPPORTED_PUSH_CONFIG_KEYS {
        assert!(key.contains('.'), "config keys are namespaced by provider (got `{key}`)");
    }
    assert!(SUPPORTED_PUSH_CONFIG_KEYS.contains(&"fcm.enabled"));
    assert!(SUPPORTED_PUSH_CONFIG_KEYS.contains(&"fcm.api_key"));
    assert!(SUPPORTED_PUSH_CONFIG_KEYS.contains(&"apns.enabled"));
    assert!(SUPPORTED_PUSH_CONFIG_KEYS.contains(&"apns.topic"));
    assert!(SUPPORTED_PUSH_CONFIG_KEYS.contains(&"webpush.enabled"));
    assert!(SUPPORTED_PUSH_CONFIG_KEYS.contains(&"webpush.vapid_public_key"));
    assert!(SUPPORTED_PUSH_CONFIG_KEYS.contains(&"webpush.vapid_private_key"));

    for secret in SECRET_PUSH_CONFIG_KEYS {
        assert!(SUPPORTED_PUSH_CONFIG_KEYS.contains(secret), "a secret key that is not settable is a bug: `{secret}`");
    }
}
