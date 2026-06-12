pub use synapse_storage::feature_flags::*;

#[cfg(test)]
mod tests {
    use super::{
        CreateFeatureFlagRequest, FeatureFlag, FeatureFlagFilters, FeatureFlagTargetInput, FeatureFlagTargetRecord,
    };

    #[test]
    fn root_feature_flags_reexport_keeps_create_request_targets_default() {
        let parsed: CreateFeatureFlagRequest = serde_json::from_value(serde_json::json!({
            "flag_key": "demo.feature",
            "target_scope": "tenant",
            "rollout_percent": 20,
            "expires_at": null,
            "reason": "enable for smoke test",
            "status": "draft"
        }))
        .expect("deserialize create request");

        assert!(parsed.targets.is_empty());
        assert_eq!(parsed.flag_key, "demo.feature");
        assert_eq!(parsed.target_scope, "tenant");
    }

    #[test]
    fn root_feature_flags_reexport_keeps_public_type_shapes() {
        let filters = FeatureFlagFilters {
            target_scope: Some("user".to_string()),
            status: Some("active".to_string()),
            limit: 10,
            cursor_updated_ts: Some(1_700_000_000_000),
            cursor_flag_key: Some("demo.feature".to_string()),
        };

        let flag = FeatureFlag {
            flag_key: "demo.feature".to_string(),
            target_scope: "user".to_string(),
            rollout_percent: 50,
            expires_at: None,
            reason: "shape smoke".to_string(),
            status: "active".to_string(),
            created_by: "@admin:example.com".to_string(),
            created_ts: 1,
            updated_ts: 2,
            targets: vec![FeatureFlagTargetRecord {
                id: 7,
                flag_key: "demo.feature".to_string(),
                subject_type: "user".to_string(),
                subject_id: "@alice:example.com".to_string(),
                created_ts: 3,
            }],
        };

        let request_target = FeatureFlagTargetInput {
            subject_type: "room".to_string(),
            subject_id: "!room:example.com".to_string(),
        };

        assert_eq!(filters.limit, 10);
        assert_eq!(filters.cursor_flag_key.as_deref(), Some("demo.feature"));
        assert_eq!(flag.targets[0].subject_id, "@alice:example.com");
        assert_eq!(request_target.subject_type, "room");
    }
}
