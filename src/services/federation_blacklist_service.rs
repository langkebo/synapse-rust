pub use synapse_services::federation_blacklist_service::*;

#[cfg(test)]
mod tests {
    use super::{AddBlacklistRequest, CheckResult, CheckServerRequest};

    #[test]
    fn root_federation_blacklist_service_reexport_keeps_check_result_shape() {
        let result = CheckResult {
            is_blocked: true,
            is_whitelisted: false,
            is_quarantined: false,
            reason: Some("Spam server".to_string()),
            matched_rule: Some("direct_block".to_string()),
        };

        let json = serde_json::to_value(&result).expect("serialize check result");
        assert_eq!(json.get("is_blocked").and_then(serde_json::Value::as_bool), Some(true));
        assert_eq!(json.get("matched_rule").and_then(serde_json::Value::as_str), Some("direct_block"));
    }

    #[test]
    fn root_federation_blacklist_service_reexport_keeps_request_types() {
        let add_request = AddBlacklistRequest {
            server_name: "evil.example.com".to_string(),
            block_type: "blacklist".to_string(),
            reason: Some("Sending spam".to_string()),
            expires_in_days: Some(30),
        };
        let check_request = CheckServerRequest { server_name: "matrix.example.com".to_string() };

        assert_eq!(add_request.block_type, "blacklist");
        assert_eq!(check_request.server_name, "matrix.example.com");
    }
}
