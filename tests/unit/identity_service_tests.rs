#[cfg(test)]
mod tests {
    use synapse_rust::services::identity::models::*;

    #[test]
    fn test_third_party_id_new() {
        let three_pid = ThirdPartyId::new("test@example.com", "email", "@test:localhost");
        assert_eq!(three_pid.address, "test@example.com");
        assert_eq!(three_pid.medium, "email");
        assert_eq!(three_pid.user_id, "@test:localhost");
        assert!(three_pid.validated_ts > 0);
        assert!(three_pid.added_ts > 0);
    }

    #[test]
    fn test_third_party_id_msisdn() {
        let three_pid = ThirdPartyId::new("+1234567890", "msisdn", "@phone:localhost");
        assert_eq!(three_pid.address, "+1234567890");
        assert_eq!(three_pid.medium, "msisdn");
        assert_eq!(three_pid.user_id, "@phone:localhost");
    }

    #[test]
    fn test_binding_request_serialization() {
        let request = BindingRequest {
            sid: "test_sid".to_string(),
            client_secret: "test_secret".to_string(),
            id_server: "vector.im".to_string(),
            id_access_token: "test_token".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("test_sid"));
        assert!(json.contains("test_secret"));
        assert!(json.contains("vector.im"));
        assert!(json.contains("test_token"));
    }

    #[test]
    fn test_binding_request_deserialization() {
        let json = r#"{"sid":"test_sid","client_secret":"test_secret","id_server":"vector.im","id_access_token":"test_token"}"#;
        let request: BindingRequest = serde_json::from_str(json).unwrap();
        assert_eq!(request.sid, "test_sid");
        assert_eq!(request.client_secret, "test_secret");
        assert_eq!(request.id_server, "vector.im");
        assert_eq!(request.id_access_token, "test_token");
    }

    #[test]
    fn test_unbinding_request_serialization() {
        let request = UnbindingRequest {
            id_server: "vector.im".to_string(),
            id_access_token: "test_token".to_string(),
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("vector.im"));
        assert!(json.contains("test_token"));
    }

    #[test]
    fn test_invitation_structure() {
        let invitation = Invitation {
            room_id: "!room:localhost".to_string(),
            sender: "@sender:localhost".to_string(),
            medium: "email".to_string(),
            address: "invite@example.com".to_string(),
            id_server: "vector.im".to_string(),
            display_name: Some("Invitee Name".to_string()),
        };

        assert_eq!(invitation.room_id, "!room:localhost");
        assert_eq!(invitation.sender, "@sender:localhost");
        assert_eq!(invitation.medium, "email");
        assert_eq!(invitation.address, "invite@example.com");
        assert!(invitation.display_name.is_some());
    }

    #[test]
    fn test_invitation_response_structure() {
        let response = InvitationResponse {
            user_id: Some("@user:localhost".to_string()),
            signed: Some(serde_json::json!({"mxid": "@user:localhost", "token": "test"})),
        };

        assert!(response.user_id.is_some());
        assert!(response.signed.is_some());
    }

    #[test]
    fn test_invitation_response_empty() {
        let response = InvitationResponse {
            user_id: None,
            signed: None,
        };

        assert!(response.user_id.is_none());
        assert!(response.signed.is_none());
    }

    #[test]
    fn test_lookup_request_structure() {
        let request = LookupRequest {
            medium: "email".to_string(),
            address: "test@example.com".to_string(),
        };

        assert_eq!(request.medium, "email");
        assert_eq!(request.address, "test@example.com");
    }

    #[test]
    fn test_hash_lookup_request_structure() {
        let request = HashLookupRequest {
            algorithm: "sha256".to_string(),
            addresses: vec!["test@example.com".to_string()],
            mediums: vec!["email".to_string()],
        };

        assert_eq!(request.algorithm, "sha256");
        assert_eq!(request.addresses.len(), 1);
        assert_eq!(request.mediums.len(), 1);
    }

    #[test]
    fn test_hash_lookup_response_structure() {
        let response = HashLookupResponse {
            chunk: vec![serde_json::json!({"address": "test@example.com", "medium": "email"})],
        };

        assert_eq!(response.chunk.len(), 1);
    }

    #[test]
    fn test_invite_3pid_structure() {
        let invite = Invite3pid {
            id_server: "vector.im".to_string(),
            id_access_token: "test_token".to_string(),
            medium: "email".to_string(),
            address: "test@example.com".to_string(),
            signer: "@sender:localhost".to_string(),
            signature: None,
        };

        assert_eq!(invite.id_server, "vector.im");
        assert_eq!(invite.medium, "email");
        assert!(invite.signature.is_none());
    }

    #[test]
    fn test_identity_server_info_structure() {
        let info = IdentityServerInfo {
            trusted_servers: vec!["vector.im".to_string(), "matrix.org".to_string()],
            api_endpoint: "https://vector.im".to_string(),
        };

        assert_eq!(info.trusted_servers.len(), 2);
        assert!(info.trusted_servers.contains(&"vector.im".to_string()));
    }

    #[test]
    fn test_third_party_id_validation_structure() {
        let validation = ThirdPartyIdValidation {
            sid: "test_sid".to_string(),
            client_secret: "test_secret".to_string(),
            medium: "email".to_string(),
            address: "test@example.com".to_string(),
        };

        assert_eq!(validation.sid, "test_sid");
        assert_eq!(validation.client_secret, "test_secret");
        assert_eq!(validation.medium, "email");
        assert_eq!(validation.address, "test@example.com");
    }

    #[test]
    fn test_identity_validate_id_server_empty() {
        let result =
            validate_id_server_standalone("", &["vector.im".to_string(), "matrix.org".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_validate_id_server_with_path() {
        let result = validate_id_server_standalone(
            "vector.im/evil",
            &["vector.im".to_string(), "matrix.org".to_string()],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_validate_id_server_private_ip() {
        let result = validate_id_server_standalone(
            "127.0.0.1",
            &["vector.im".to_string(), "matrix.org".to_string()],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_validate_id_server_localhost() {
        let result = validate_id_server_standalone(
            "localhost",
            &["vector.im".to_string(), "matrix.org".to_string()],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_validate_id_server_private_10() {
        let result = validate_id_server_standalone(
            "10.0.0.1",
            &["vector.im".to_string(), "matrix.org".to_string()],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_validate_id_server_private_192_168() {
        let result = validate_id_server_standalone(
            "192.168.1.1",
            &["vector.im".to_string(), "matrix.org".to_string()],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_validate_id_server_link_local() {
        let result = validate_id_server_standalone(
            "169.254.1.1",
            &["vector.im".to_string(), "matrix.org".to_string()],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_validate_id_server_untrusted() {
        let result = validate_id_server_standalone(
            "evil.example.com",
            &["vector.im".to_string(), "matrix.org".to_string()],
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_validate_id_server_trusted() {
        let result = validate_id_server_standalone(
            "vector.im",
            &["vector.im".to_string(), "matrix.org".to_string()],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_identity_validate_id_server_trusted_matrix_org() {
        let result = validate_id_server_standalone(
            "matrix.org",
            &["vector.im".to_string(), "matrix.org".to_string()],
        );
        assert!(result.is_ok());
    }

    #[test]
    fn test_identity_validate_id_server_starts_with_dot() {
        let result = validate_id_server_standalone(".vector.im", &["vector.im".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_validate_id_server_ends_with_dot() {
        let result = validate_id_server_standalone("vector.im.", &["vector.im".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_validate_id_server_zero_address() {
        let result = validate_id_server_standalone("0.0.0.0", &["vector.im".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_validate_id_server_backslash() {
        let result = validate_id_server_standalone("vector.im\\evil", &["vector.im".to_string()]);
        assert!(result.is_err());
    }

    #[test]
    fn test_identity_validate_empty_trusted_servers() {
        let result = validate_id_server_standalone("any.example.com", &[]);
        assert!(result.is_ok());
    }

    fn validate_id_server_standalone(
        id_server: &str,
        trusted_servers: &[String],
    ) -> Result<(), String> {
        if id_server.is_empty() {
            return Err("id_server cannot be empty".to_string());
        }

        if id_server.contains('/') || id_server.contains('\\') {
            return Err("id_server must be a hostname only".to_string());
        }

        if id_server.starts_with('.') || id_server.ends_with('.') {
            return Err("id_server has invalid format".to_string());
        }

        let host = id_server.split(':').next().unwrap_or("");
        if host.is_empty() {
            return Err("id_server has empty hostname".to_string());
        }

        if host == "localhost"
            || host.starts_with("127.")
            || host.starts_with("10.")
            || host.starts_with("192.168.")
            || host.starts_with("169.254.")
        {
            return Err("id_server must not be a private/local address".to_string());
        }

        if host.starts_with("0.") || host == "0.0.0.0" {
            return Err("id_server must not be a broadcast address".to_string());
        }

        if !trusted_servers.is_empty() && !trusted_servers.iter().any(|s| s == id_server) {
            return Err(format!(
                "id_server '{}' is not in the trusted servers list",
                id_server
            ));
        }

        Ok(())
    }
}
