#[cfg(test)]
mod tests {
    use synapse_rust::common::config::SamlConfig;
    use synapse_rust::common::config::SamlAttributeMapping;

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
            attribute_mapping: SamlAttributeMapping {
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
    fn test_saml_config_with_metadata_xml() {
        let mut config = SamlConfig::default();
        config.enabled = true;
        config.metadata_xml = Some("<EntityDescriptor>...</EntityDescriptor>".to_string());
        assert!(config.is_enabled());
    }

    #[test]
    fn test_saml_config_get_sp_acs_url() {
        let config = create_test_config();
        let acs_url = config.get_sp_acs_url("matrix.example.com");
        assert_eq!(
            acs_url,
            "https://matrix.example.com/_matrix/client/r0/login/sso/redirect/saml"
        );
    }

    #[test]
    fn test_saml_config_get_sp_acs_url_custom() {
        let mut config = create_test_config();
        config.sp_acs_url = Some("https://custom.example.com/saml/acs".to_string());
        let acs_url = config.get_sp_acs_url("matrix.example.com");
        assert_eq!(acs_url, "https://custom.example.com/saml/acs");
    }

    #[test]
    fn test_saml_config_get_sp_sls_url() {
        let config = create_test_config();
        let sls_url = config.get_sp_sls_url("matrix.example.com");
        assert!(sls_url.is_some());
        assert_eq!(
            sls_url.unwrap(),
            "https://matrix.example.com/_matrix/client/r0/logout/saml"
        );
    }

    #[test]
    fn test_saml_attribute_mapping_default() {
        let mapping = SamlAttributeMapping::default();
        assert!(mapping.uid.is_none());
        assert!(mapping.displayname.is_none());
        assert!(mapping.email.is_none());
    }

    #[test]
    fn test_saml_config_session_lifetime() {
        let config = create_test_config();
        assert_eq!(config.session_lifetime, 28800);
    }

    #[test]
    fn test_saml_config_metadata_refresh_interval() {
        let config = create_test_config();
        assert_eq!(config.metadata_refresh_interval, 3600);
    }

    #[test]
    fn test_saml_config_allowed_idp_entity_ids() {
        let mut config = create_test_config();
        config.allowed_idp_entity_ids = vec![
            "https://idp1.example.com".to_string(),
            "https://idp2.example.com".to_string(),
        ];
        assert_eq!(config.allowed_idp_entity_ids.len(), 2);
    }

    #[test]
    fn test_saml_config_block_unknown_users() {
        let mut config = create_test_config();
        config.block_unknown_users = true;
        assert!(config.block_unknown_users);
    }

    #[test]
    fn test_saml_config_use_name_id_for_user_id() {
        let mut config = create_test_config();
        config.use_name_id_for_user_id = true;
        assert!(config.use_name_id_for_user_id);
    }

    #[test]
    fn test_saml_config_sign_requests() {
        let config = create_test_config();
        assert!(!config.sign_requests);
    }

    #[test]
    fn test_saml_config_want_response_signed() {
        let config = create_test_config();
        assert!(config.want_response_signed);
    }

    #[test]
    fn test_saml_config_want_assertions_signed() {
        let config = create_test_config();
        assert!(config.want_assertions_signed);
    }

    #[test]
    fn test_saml_config_user_id_template() {
        let config = create_test_config();
        assert_eq!(config.user_id_template, "{uid}");
        
        let result = config.user_id_template.replace("{uid}", "testuser");
        assert_eq!(result, "testuser");
    }

    #[test]
    fn test_saml_config_nameid_format() {
        let config = create_test_config();
        assert_eq!(
            config.nameid_format,
            "urn:oasis:names:tc:SAML:2.0:nameid-format:persistent"
        );
    }

    #[test]
    fn test_saml_config_timeout() {
        let config = create_test_config();
        assert_eq!(config.timeout, 10);
    }
}
