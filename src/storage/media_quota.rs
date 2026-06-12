pub use synapse_storage::media_quota::*;

#[cfg(test)]
mod tests {
    use super::{CreateQuotaConfigRequest, MediaQuotaAlert, QuotaCheckResult, UserMediaQuota};

    #[test]
    fn root_media_quota_storage_reexport_keeps_request_and_quota_shape() {
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
        let quota = UserMediaQuota {
            id: 1,
            user_id: "@user:example.com".to_string(),
            quota_config_id: Some(1),
            custom_max_storage_bytes: None,
            custom_max_file_size_bytes: None,
            custom_max_files_count: None,
            current_storage_bytes: 524288000,
            current_files_count: 50,
            created_ts: 1234567890,
            updated_ts: Some(1234567891),
        };

        assert_eq!(request.name, "premium");
        assert_eq!(quota.current_files_count, 50);
    }

    #[test]
    fn root_media_quota_storage_reexport_keeps_alert_and_check_result_shape() {
        let alert = MediaQuotaAlert {
            id: 1,
            user_id: "@user:example.com".to_string(),
            alert_type: "warning".to_string(),
            threshold_percent: 80,
            current_usage_bytes: 858993459,
            quota_limit_bytes: 1073741824,
            message: Some("Storage usage at 80%".to_string()),
            is_read: false,
            created_ts: 1234567890,
        };
        let result = QuotaCheckResult {
            is_allowed: true,
            reason: None,
            current_usage: 524288000,
            quota_limit: 1073741824,
            usage_percent: 48.8,
        };

        assert_eq!(alert.threshold_percent, 80);
        assert!(result.is_allowed);
        assert!(result.usage_percent < 100.0);
    }
}
