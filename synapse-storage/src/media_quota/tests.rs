use synapse_common::current_timestamp_millis;

use super::*;

fn create_test_quota_config() -> MediaQuotaConfig {
    MediaQuotaConfig {
        id: 1,
        name: "default".to_string(),
        description: Some("Default quota".to_string()),
        max_storage_bytes: 1073741824,
        max_file_size_bytes: 104857600,
        max_files_count: 1000,
        allowed_mime_types: serde_json::json!(["image/*", "video/*"]),
        blocked_mime_types: serde_json::json!(["application/exe"]),
        is_default: true,
        is_enabled: true,
        created_ts: current_timestamp_millis(),
        updated_ts: Some(current_timestamp_millis()),
    }
}

fn create_test_user_quota() -> UserMediaQuota {
    UserMediaQuota {
        id: 1,
        user_id: "@user:example.com".to_string(),
        quota_config_id: Some(1),
        custom_max_storage_bytes: None,
        custom_max_file_size_bytes: None,
        custom_max_files_count: None,
        current_storage_bytes: 524288000,
        current_files_count: 50,
        created_ts: current_timestamp_millis(),
        updated_ts: Some(current_timestamp_millis()),
    }
}

fn create_test_media_alert() -> MediaQuotaAlert {
    MediaQuotaAlert {
        id: 1,
        user_id: "@user:example.com".to_string(),
        alert_type: "warning".to_string(),
        threshold_percent: 80,
        current_usage_bytes: 858993459,
        quota_limit_bytes: 1073741824,
        message: Some("Storage usage at 80%".to_string()),
        is_read: false,
        created_ts: current_timestamp_millis(),
    }
}

#[test]
fn test_quota_config_creation() {
    let config = create_test_quota_config();
    assert_eq!(config.name, "default");
    assert_eq!(config.max_storage_bytes, 1073741824);
    assert!(config.is_default);
    assert!(config.is_enabled);
}

#[test]
fn test_user_quota_creation() {
    let quota = create_test_user_quota();
    assert_eq!(quota.user_id, "@user:example.com");
    assert_eq!(quota.current_storage_bytes, 524288000);
    assert_eq!(quota.current_files_count, 50);
}

#[test]
fn test_media_alert_creation() {
    let alert = create_test_media_alert();
    assert_eq!(alert.alert_type, "warning");
    assert_eq!(alert.threshold_percent, 80);
    assert!(!alert.is_read);
}

#[test]
fn test_create_quota_config_request() {
    let request = CreateQuotaConfigRequest {
        name: "premium".to_string(),
        description: Some("Premium quota".to_string()),
        max_storage_bytes: 10737418240,
        max_file_size_bytes: 524288000,
        max_files_count: 5000,
        allowed_mime_types: Some(vec!["*".to_string()]),
        blocked_mime_types: None,
        is_default: Some(false),
    };
    assert_eq!(request.name, "premium");
    assert_eq!(request.max_storage_bytes, 10737418240);
}

#[test]
fn test_set_user_quota_request() {
    let request = SetUserQuotaRequest {
        user_id: "@user:example.com".to_string(),
        quota_config_id: Some(1),
        custom_max_storage_bytes: Some(2147483648),
        custom_max_file_size_bytes: None,
        custom_max_files_count: None,
    };
    assert_eq!(request.user_id, "@user:example.com");
    assert!(request.custom_max_storage_bytes.is_some());
}

#[test]
fn test_update_usage_request() {
    let request = UpdateUsageRequest {
        user_id: "@user:example.com".to_string(),
        media_id: "media123".to_string(),
        file_size_bytes: 1048576,
        mime_type: Some("image/png".to_string()),
        operation: "upload".to_string(),
    };
    assert_eq!(request.operation, "upload");
    assert_eq!(request.file_size_bytes, 1048576);
}

#[test]
fn test_quota_check_result() {
    let result = QuotaCheckResult {
        is_allowed: true,
        reason: None,
        current_usage: 524288000,
        quota_limit: 1073741824,
        usage_percent: 48.8,
    };
    assert!(result.is_allowed);
    assert!(result.reason.is_none());
    assert!(result.usage_percent < 100.0);
}

#[test]
fn test_quota_check_result_exceeded() {
    let result = QuotaCheckResult {
        is_allowed: false,
        reason: Some("Quota exceeded".to_string()),
        current_usage: 1073741824,
        quota_limit: 1073741824,
        usage_percent: 100.0,
    };
    assert!(!result.is_allowed);
    assert!(result.reason.is_some());
}

#[test]
fn test_server_media_quota() {
    let quota = ServerMediaQuota {
        id: 1,
        max_storage_bytes: Some(1099511627776),
        max_file_size_bytes: Some(1073741824),
        max_files_count: Some(100000),
        current_storage_bytes: 549755813888,
        current_files_count: 25000,
        alert_threshold_percent: 90,
        updated_ts: current_timestamp_millis(),
    };
    assert_eq!(quota.max_storage_bytes, Some(1099511627776));
    assert_eq!(quota.alert_threshold_percent, 90);
}

#[test]
fn test_usage_percent_calculation() {
    let current: i64 = 524288000;
    let limit: i64 = 1073741824;
    let percent = (current as f64 / limit as f64) * 100.0;
    assert!(percent > 48.0 && percent < 49.0);
}

#[test]
fn test_mime_type_validation() {
    let allowed = ["image/*", "video/*", "application/pdf"];
    let blocked = ["application/exe", "application/bat"];

    assert!(allowed.contains(&"image/*"));
    assert!(blocked.contains(&"application/exe"));
}
