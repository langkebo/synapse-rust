use super::*;
use std::collections::HashMap;

#[test]
fn test_saml_session_creation() {
    let mut attributes = HashMap::new();
    attributes.insert("email".to_string(), vec!["alice@example.com".to_string()]);

    let session = SamlSession {
        id: 1,
        session_id: "session123".to_string(),
        user_id: "@alice:example.com".to_string(),
        name_id: Some("alice".to_string()),
        issuer: Some("https://idp.example.com".to_string()),
        session_index: Some("index123".to_string()),
        attributes: serde_json::json!(attributes),
        created_ts: 1234567800000,
        expires_at: 1234567890000,
        last_used_ts: 1234567890000,
        status: "active".to_string(),
    };
    assert_eq!(session.session_id, "session123");
    assert_eq!(session.user_id, "@alice:example.com");
}

#[test]
fn test_saml_user_mapping_creation() {
    let mut attributes = HashMap::new();
    attributes.insert("email".to_string(), vec!["alice@example.com".to_string()]);

    let mapping = SamlUserMapping {
        id: 1,
        name_id: "alice".to_string(),
        user_id: "@alice:example.com".to_string(),
        issuer: "https://idp.example.com".to_string(),
        first_seen_ts: 1234567800000,
        last_authenticated_ts: 1234567890000,
        authentication_count: 1,
        attributes: serde_json::json!(attributes),
    };
    assert_eq!(mapping.user_id, "@alice:example.com");
}

#[test]
fn test_saml_identity_provider_creation() {
    let idp = SamlIdentityProvider {
        id: 1,
        entity_id: "https://idp.example.com".to_string(),
        display_name: Some("Example IdP".to_string()),
        description: Some("Test IDP".to_string()),
        metadata_url: None,
        metadata_xml: Some("<xml>metadata</xml>".to_string()),
        is_enabled: true,
        priority: 0,
        attribute_mapping: serde_json::json!({}),
        created_ts: 1234567800000,
        updated_ts: Some(1234567890000),
        last_metadata_refresh_ts: None,
        metadata_valid_until: None,
    };
    assert!(idp.is_enabled);
    assert_eq!(idp.entity_id, "https://idp.example.com");
}

#[test]
fn test_saml_auth_event_creation() {
    let event = SamlAuthEvent {
        id: 1,
        session_id: Some("session123".to_string()),
        user_id: Some("@alice:example.com".to_string()),
        name_id: Some("alice".to_string()),
        issuer: Some("https://idp.example.com".to_string()),
        event_type: "authentication".to_string(),
        status: "success".to_string(),
        error_message: None,
        ip_address: Some("192.168.1.1".to_string()),
        user_agent: None,
        request_id: None,
        attributes: serde_json::json!({}),
        created_ts: 1234567890000,
    };
    assert_eq!(event.status, "success");
}

#[test]
fn test_create_saml_session_request() {
    let attributes = HashMap::new();
    let request = CreateSamlSessionRequest {
        session_id: "new_session".to_string(),
        user_id: "@alice:example.com".to_string(),
        name_id: Some("alice".to_string()),
        issuer: Some("https://idp.example.com".to_string()),
        session_index: Some("index123".to_string()),
        attributes,
        expires_in_seconds: 3600,
    };
    assert_eq!(request.user_id, "@alice:example.com");
}

#[test]
fn test_create_saml_identity_provider_request() {
    let request = CreateSamlIdentityProviderRequest {
        entity_id: "https://new-idp.example.com".to_string(),
        display_name: Some("New IdP".to_string()),
        description: Some("New Identity Provider".to_string()),
        metadata_url: None,
        metadata_xml: Some("<xml>new metadata</xml>".to_string()),
        enabled: Some(true),
        priority: Some(0),
        attribute_mapping: None,
    };
    assert_eq!(request.entity_id, "https://new-idp.example.com");
}
