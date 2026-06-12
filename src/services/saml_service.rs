#[cfg(feature = "saml-sso")]
pub use synapse_services::saml_service::*;

#[cfg(test)]
#[cfg(feature = "saml-sso")]
mod tests {
    use super::*;
    use crate::storage::saml::SamlStorage;

    fn create_test_config() -> SamlConfig {
        SamlConfig {
            enabled: true,
            metadata_url: Some("https://idp.example.com/metadata".to_string()),
            metadata_xml: None,
            sp_entity_id: "https://matrix.example.com".to_string(),
            sp_acs_url: None,
            sp_sls_url: None,
            sp_private_key: None,
            sp_private_key_path: None,
            sp_certificate: None,
            sp_certificate_path: None,
            attribute_mapping: crate::common::config::SamlAttributeMapping {
                uid: Some("uid".to_string()),
                displayname: Some("cn".to_string()),
                email: Some("mail".to_string()),
            },
            nameid_format: "urn:oasis:names:tc:SAML:2.0:nameid-format:persistent".to_string(),
            allow_existing_users: true,
            block_unknown_users: false,
            user_id_template: "{uid}".to_string(),
            use_name_id_for_user_id: false,
            sign_requests: false,
            want_response_signed: true,
            want_assertions_signed: true,
            want_assertions_encrypted: false,
            authn_context_class_ref: None,
            session_lifetime: 28800,
            metadata_refresh_interval: 3600,
            allowed_idp_entity_ids: Vec::new(),
            timeout: 10,
        }
    }

    fn create_test_service() -> SamlService {
        let pool = Arc::new(
            sqlx::PgPool::connect_lazy("postgresql://synapse:synapse@localhost:5432/synapse")
                .expect("valid lazy postgres url"),
        );
        let storage = Arc::new(SamlStorage::new(&pool));
        SamlService::new(Arc::new(create_test_config()), storage, "localhost".to_string())
    }

    #[test]
    fn test_saml_config_enabled() {
        let config = create_test_config();
        assert!(config.is_enabled());
    }

    #[test]
    fn test_saml_config_disabled() {
        let config = SamlConfig::default();
        assert!(!config.is_enabled());
    }

    #[test]
    fn test_generate_request_id() {
        let id = SamlService::generate_request_id();
        assert!(id.starts_with("id_"));
        assert_eq!(id.len(), 35);
    }

    #[test]
    fn test_generate_session_id() {
        let id = SamlService::generate_session_id();
        assert!(uuid::Uuid::parse_str(&id).is_ok());
    }

    #[test]
    fn test_parse_metadata_xml() {
        let xml = r#"
        <md:EntityDescriptor entityID="https://idp.example.com">
            <md:IDPSSODescriptor>
                <md:SingleSignOnService Location="https://idp.example.com/sso"/>
                <md:SingleLogoutService Location="https://idp.example.com/slo"/>
                <ds:KeyInfo>
                    <ds:X509Data>
                        <ds:X509Certificate>MIIC9jCCAd4CCQD...</ds:X509Certificate>
                    </ds:X509Data>
                </ds:KeyInfo>
            </md:IDPSSODescriptor>
        </md:EntityDescriptor>
        "#;

        let metadata = SamlService::parse_metadata_xml(xml).unwrap();
        assert_eq!(metadata.entity_id, "https://idp.example.com");
        assert_eq!(metadata.sso_url, "https://idp.example.com/sso");
        assert_eq!(metadata.slo_url, Some("https://idp.example.com/slo".to_string()));
    }

    #[test]
    fn test_parse_saml_assertion() {
        let xml = r#"
        <saml:Assertion>
            <saml:Issuer>https://idp.example.com</saml:Issuer>
            <saml:Subject>
                <saml:NameID>user123</saml:NameID>
            </saml:Subject>
            <saml:AttributeStatement>
                <saml:Attribute Name="uid">
                    <saml:AttributeValue>testuser</saml:AttributeValue>
                </saml:Attribute>
                <saml:Attribute Name="mail">
                    <saml:AttributeValue>test@example.com</saml:AttributeValue>
                </saml:Attribute>
            </saml:AttributeStatement>
            <saml:AuthnStatement SessionIndex="session123"/>
        </saml:Assertion>
        "#;

        let (name_id, issuer, attributes, session_index) = SamlService::parse_saml_assertion(xml).unwrap();
        assert_eq!(name_id, "user123");
        assert_eq!(issuer, "https://idp.example.com");
        assert_eq!(attributes.get("uid").unwrap().first().unwrap(), "testuser");
        assert_eq!(session_index, Some("session123".to_string()));
    }

    #[tokio::test]
    async fn test_validate_response_accepts_valid_constraints() {
        let mut config = create_test_config();
        // Disable signature verification for this test since we don't have IdP metadata
        config.want_response_signed = false;
        config.want_assertions_signed = false;

        let pool = Arc::new(
            sqlx::PgPool::connect_lazy("postgresql://synapse:synapse@localhost:5432/synapse")
                .expect("valid lazy postgres url"),
        );
        let storage = Arc::new(SamlStorage::new(&pool));
        let service = SamlService::new(Arc::new(config), storage, "localhost".to_string());

        let acs_url = service.config.get_sp_acs_url(&service.server_name);
        let xml = format!(
            r#"<samlp:Response InResponseTo="id_123">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://matrix.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                    <saml:Subject>
                        <saml:SubjectConfirmation>
                            <saml:SubjectConfirmationData Recipient="{}"/>
                        </saml:SubjectConfirmation>
                    </saml:Subject>
                </saml:Assertion>
            </samlp:Response>"#,
            (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
            (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
            acs_url
        );

        let result = service.validate_response("https://idp.example.com", &xml, Some("id_123"));
        if let Err(e) = &result {
            tracing::warn!(error = ?e, test_case = %"test_validate_response_allows_clock_skew", "Validation failed");
        }
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_response_rejects_wrong_audience() {
        let service = create_test_service();
        let acs_url = service.config.get_sp_acs_url(&service.server_name);
        let xml = format!(
            r#"<samlp:Response InResponseTo="id_123">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://another.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                    <saml:Subject>
                        <saml:SubjectConfirmation>
                            <saml:SubjectConfirmationData Recipient="{}"/>
                        </saml:SubjectConfirmation>
                    </saml:Subject>
                </saml:Assertion>
            </samlp:Response>"#,
            (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
            (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
            acs_url
        );

        let error = service.validate_response("https://idp.example.com", &xml, Some("id_123")).unwrap_err();
        assert!(error.to_string().contains("audience"));
    }

    #[tokio::test]
    async fn test_validate_response_rejects_mismatched_in_response_to() {
        let service = create_test_service();
        let acs_url = service.config.get_sp_acs_url(&service.server_name);
        let xml = format!(
            r#"<samlp:Response InResponseTo="id_actual">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://matrix.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                    <saml:Subject>
                        <saml:SubjectConfirmation>
                            <saml:SubjectConfirmationData Recipient="{}"/>
                        </saml:SubjectConfirmation>
                    </saml:Subject>
                </saml:Assertion>
            </samlp:Response>"#,
            (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
            (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
            acs_url
        );

        let error = service.validate_response("https://idp.example.com", &xml, Some("id_expected")).unwrap_err();
        assert!(error.to_string().contains("InResponseTo"));
    }

    #[tokio::test]
    async fn test_validate_response_rejects_non_success_status() {
        let service = create_test_service();
        let acs_url = service.config.get_sp_acs_url(&service.server_name);
        let xml = format!(
            r#"<samlp:Response InResponseTo="id_123">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Responder"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://matrix.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                    <saml:Subject>
                        <saml:SubjectConfirmation>
                            <saml:SubjectConfirmationData Recipient="{}"/>
                        </saml:SubjectConfirmation>
                    </saml:Subject>
                </saml:Assertion>
            </samlp:Response>"#,
            (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
            (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339(),
            acs_url
        );

        let error = service.validate_response("https://idp.example.com", &xml, Some("id_123")).unwrap_err();
        assert!(error.to_string().contains("status is not success"));
    }

    #[tokio::test]
    async fn test_validate_response_rejects_mismatched_destination() {
        let service = create_test_service();
        let xml = format!(
            r#"<samlp:Response InResponseTo="id_123" Destination="https://matrix.example.com/wrong">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://matrix.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                </saml:Assertion>
            </samlp:Response>"#,
            (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
            (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339()
        );

        let error = service.validate_response("https://idp.example.com", &xml, Some("id_123")).unwrap_err();
        assert!(error.to_string().contains("destination mismatch"));
    }

    #[tokio::test]
    async fn test_validate_response_rejects_mismatched_recipient() {
        let service = create_test_service();
        let xml = format!(
            r#"<samlp:Response InResponseTo="id_123">
                <samlp:Status>
                    <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
                </samlp:Status>
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Assertion>
                    <saml:Conditions NotBefore="{}" NotOnOrAfter="{}">
                        <saml:AudienceRestriction>
                            <saml:Audience>https://matrix.example.com</saml:Audience>
                        </saml:AudienceRestriction>
                    </saml:Conditions>
                    <saml:Subject>
                        <saml:SubjectConfirmation>
                            <saml:SubjectConfirmationData Recipient="https://matrix.example.com/invalid"/>
                        </saml:SubjectConfirmation>
                    </saml:Subject>
                </saml:Assertion>
            </samlp:Response>"#,
            (Utc::now() - chrono::Duration::minutes(1)).to_rfc3339(),
            (Utc::now() + chrono::Duration::minutes(5)).to_rfc3339()
        );

        let error = service.validate_response("https://idp.example.com", &xml, Some("id_123")).unwrap_err();
        assert!(error.to_string().contains("recipient mismatch"));
    }
}
