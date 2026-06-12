pub use synapse_services::media_quota_service::*;

#[cfg(test)]
mod tests {
    use super::UserQuotaInfo;

    #[test]
    fn root_media_quota_service_reexport_keeps_user_quota_shape() {
        let info = UserQuotaInfo {
            current_storage_bytes: 1024,
            current_files_count: 4,
            max_storage_bytes: 4096,
            max_file_size_bytes: 2048,
            max_files_count: 16,
            usage_percent: 25.0,
        };

        assert_eq!(info.current_storage_bytes, 1024);
        assert_eq!(info.usage_percent, 25.0);
    }

    #[test]
    fn root_media_quota_service_reexport_keeps_user_quota_serde_round_trip() {
        let info = UserQuotaInfo {
            current_storage_bytes: 2048,
            current_files_count: 8,
            max_storage_bytes: 8192,
            max_file_size_bytes: 2048,
            max_files_count: 32,
            usage_percent: 25.0,
        };

        let json = serde_json::to_string(&info).expect("serialize user quota info");
        let parsed: UserQuotaInfo = serde_json::from_str(&json).expect("deserialize user quota info");

        assert_eq!(parsed.current_files_count, info.current_files_count);
        assert_eq!(parsed.max_storage_bytes, info.max_storage_bytes);
    }
}
