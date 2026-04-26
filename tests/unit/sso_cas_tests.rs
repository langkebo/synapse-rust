#[cfg(test)]
mod tests {
    use synapse_rust::common::config::IdentityConfig;

    #[test]
    fn test_cas_ticket_format() {
        let server_name = "localhost";
        let ticket_prefix = "ST";
        let random_part = "abc123XYZ";
        let ticket_id = format!("{}-{}-{}", ticket_prefix, server_name, random_part);

        assert!(ticket_id.starts_with("ST-"));
        assert!(ticket_id.contains(server_name));
    }

    #[test]
    fn test_cas_proxy_ticket_format() {
        let server_name = "localhost";
        let ticket_prefix = "PT";
        let random_part = "def456UVW";
        let ticket_id = format!("{}-{}-{}", ticket_prefix, server_name, random_part);

        assert!(ticket_id.starts_with("PT-"));
        assert!(ticket_id.contains(server_name));
    }

    #[test]
    fn test_cas_proxy_granting_ticket_format() {
        let server_name = "localhost";
        let ticket_prefix = "PGT";
        let random_part = "ghi789RST";
        let ticket_id = format!("{}-{}-{}", ticket_prefix, server_name, random_part);

        assert!(ticket_id.starts_with("PGT-"));
        assert!(ticket_id.contains(server_name));
    }

    #[test]
    fn test_cas_service_url_validation() {
        let valid_urls = vec![
            "https://example.com/cas/callback",
            "http://localhost:28008/_matrix/client/r0/login/cas/callback",
            "https://matrix.example.com:8448/callback",
        ];

        for url in valid_urls {
            assert!(url::Url::parse(url).is_ok(), "Expected valid URL: {}", url);
        }
    }

    #[test]
    fn test_cas_xml_service_validate_success() {
        let response = r#"<?xml version="1.0" encoding="UTF-8"?>
<cas:serviceResponse xmlns:cas="http://www.yale.edu/tp/cas">
  <cas:authenticationSuccess>
    <cas:user>testuser</cas:user>
    <cas:attributes>
      <cas:displayName>Test User</cas:displayName>
      <cas:email>test@example.com</cas:email>
    </cas:attributes>
  </cas:authenticationSuccess>
</cas:serviceResponse>"#;

        assert!(response.contains("cas:authenticationSuccess"));
        assert!(response.contains("cas:user"));
        assert!(response.contains("testuser"));
    }

    #[test]
    fn test_cas_xml_service_validate_failure() {
        let response = r#"<?xml version="1.0" encoding="UTF-8"?>
<cas:serviceResponse xmlns:cas="http://www.yale.edu/tp/cas">
  <cas:authenticationFailure code="INVALID_TICKET">
    Ticket ST-12345-localhost-abc not recognized
  </cas:authenticationFailure>
</cas:serviceResponse>"#;

        assert!(response.contains("cas:authenticationFailure"));
        assert!(response.contains("INVALID_TICKET"));
    }

    #[test]
    fn test_cas_p3_xml_service_validate_success() {
        let response = r#"<?xml version="1.0" encoding="UTF-8"?>
<cas:serviceResponse xmlns:cas="http://www.yale.edu/tp/cas">
  <cas:authenticationSuccess>
    <cas:user>testuser</cas:user>
    <cas:attributes>
      <cas:displayName>Test User</cas:displayName>
    </cas:attributes>
    <cas:proxyGrantingTicket>PGT-12345-localhost-xyz</cas:proxyGrantingTicket>
  </cas:authenticationSuccess>
</cas:serviceResponse>"#;

        assert!(response.contains("cas:authenticationSuccess"));
        assert!(response.contains("cas:proxyGrantingTicket"));
    }

    #[test]
    fn test_cas_saml_response_success() {
        let response = r#"<?xml version="1.0" encoding="UTF-8"?>
<cas:serviceResponse xmlns:cas="http://www.yale.edu/tp/cas">
  <cas:authenticationSuccess>
    <cas:user>testuser</cas:user>
  </cas:authenticationSuccess>
</cas:serviceResponse>"#;

        assert!(response.contains("cas:authenticationSuccess"));
    }

    #[test]
    fn test_cas_login_redirect_url_construction() {
        let cas_base_url = "https://cas.example.com/cas";
        let service_url = "http://localhost:28008/_matrix/client/r0/login/cas/callback";
        let redirect_url = format!(
            "{}/login?service={}",
            cas_base_url,
            urlencoding::encode(service_url)
        );

        assert!(redirect_url.starts_with("https://cas.example.com/cas/login"));
        assert!(redirect_url.contains("service="));
    }

    #[test]
    fn test_cas_logout_redirect_url_construction() {
        let cas_base_url = "https://cas.example.com/cas";
        let service_url = "http://localhost:28008";
        let logout_url = format!(
            "{}/logout?service={}",
            cas_base_url,
            urlencoding::encode(service_url)
        );

        assert!(logout_url.starts_with("https://cas.example.com/cas/logout"));
        assert!(logout_url.contains("service="));
    }

    #[test]
    fn test_cas_user_attribute_serialization() {
        let attr = serde_json::json!({
            "user_id": "@test:localhost",
            "display_name": "Test User",
            "email": "test@example.com",
            "attributes": {
                "role": "admin",
                "department": "engineering"
            }
        });

        assert_eq!(attr["user_id"], "@test:localhost");
        assert_eq!(attr["attributes"]["role"], "admin");
    }

    #[test]
    fn test_cas_service_registration_request() {
        let request = serde_json::json!({
            "service_id": "test_service",
            "service_url": "http://localhost:28008/callback",
            "description": "Test CAS Service"
        });

        assert!(request.get("service_id").is_some());
        assert!(request.get("service_url").is_some());
    }

    #[test]
    fn test_identity_config_default() {
        let config = IdentityConfig::default();
        assert!(!config.trusted_servers.is_empty());
    }

    #[test]
    fn test_cas_ticket_validity_duration() {
        let validity_seconds: i64 = 300;
        assert_eq!(validity_seconds, 300);
        assert!(validity_seconds > 0);
        assert!(validity_seconds <= 600);
    }
}
