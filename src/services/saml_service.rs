use crate::common::config::SamlConfig;
use crate::common::error::ApiError;
use crate::storage::saml::*;
use base64::{Engine as _, engine::general_purpose};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlAuthRequest {
    pub request_id: String,
    pub redirect_url: String,
    pub relay_state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlAuthResponse {
    pub session_id: String,
    pub user_id: String,
    pub name_id: String,
    pub issuer: String,
    pub attributes: HashMap<String, Vec<String>>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlUser {
    pub name_id: String,
    pub localpart: String,
    pub displayname: Option<String>,
    pub email: Option<String>,
    pub issuer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamlMetadata {
    pub entity_id: String,
    pub sso_url: String,
    pub slo_url: Option<String>,
    pub certificate: String,
    pub valid_until: Option<DateTime<Utc>>,
}

pub struct SamlService {
    config: Arc<SamlConfig>,
    storage: Arc<SamlStorage>,
    http_client: reqwest::Client,
    server_name: String,
    cached_metadata: Option<SamlMetadata>,
    metadata_last_refresh: Option<DateTime<Utc>>,
}

impl SamlService {
    pub fn new(
        config: Arc<SamlConfig>,
        storage: Arc<SamlStorage>,
        server_name: String,
    ) -> Self {
        let http_client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        Self {
            config,
            storage,
            http_client,
            server_name,
            cached_metadata: None,
            metadata_last_refresh: None,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.config.is_enabled()
    }

    pub async fn get_auth_redirect(&self, relay_state: Option<&str>) -> Result<SamlAuthRequest, ApiError> {
        let request_id = Self::generate_request_id();
        
        let metadata = self.get_idp_metadata().await?;
        
        let sso_url = metadata.sso_url;
        
        let sp_entity_id = &self.config.sp_entity_id;
        let acs_url = self.config.get_sp_acs_url(&self.server_name);
        
        let authn_request = self.build_authn_request(&request_id, sp_entity_id, &acs_url);
        
        let redirect_url = self.build_redirect_url(&sso_url, &authn_request, relay_state);
        
        info!("Generated SAML auth redirect for request: {}", request_id);
        
        Ok(SamlAuthRequest {
            request_id,
            redirect_url,
            relay_state: relay_state.map(|s| s.to_string()),
        })
    }

    pub async fn process_auth_response(
        &self,
        saml_response: &str,
        relay_state: Option<&str>,
        ip_address: Option<&str>,
        user_agent: Option<&str>,
    ) -> Result<SamlAuthResponse, ApiError> {
        let decoded = Self::decode_saml_response(saml_response)?;
        
        let (name_id, issuer, attributes, session_index) = Self::parse_saml_assertion(&decoded)?;
        
        self.validate_response(&issuer, &decoded)?;
        
        let user = self.map_user(&name_id, &issuer, &attributes)?;
        
        let existing_mapping = self.storage
            .get_user_mapping_by_name_id(&name_id, &issuer)
            .await?;
        
        let user_id = if let Some(mapping) = existing_mapping {
            if !self.config.allow_existing_users {
                return Err(ApiError::unauthorized("Existing users not allowed"));
            }
            mapping.user_id
        } else {
            if self.config.block_unknown_users {
                return Err(ApiError::unauthorized("Unknown user blocked"));
            }
            format!("@{}:{}", user.localpart, self.server_name)
        };
        
        let session_id = Self::generate_session_id();
        
        let session = self.storage.create_session(CreateSamlSessionRequest {
            session_id: session_id.clone(),
            user_id: user_id.clone(),
            name_id: Some(name_id.clone()),
            issuer: Some(issuer.clone()),
            session_index,
            attributes: attributes.clone(),
            expires_in_seconds: self.config.session_lifetime as i64,
        }).await?;
        
        self.storage.create_user_mapping(CreateSamlUserMappingRequest {
            name_id: name_id.clone(),
            user_id: user_id.clone(),
            issuer: issuer.clone(),
            attributes: attributes.clone(),
        }).await?;
        
        self.storage.create_auth_event(CreateSamlAuthEventRequest {
            session_id: Some(session_id.clone()),
            user_id: Some(user_id.clone()),
            name_id: Some(name_id.clone()),
            issuer: Some(issuer.clone()),
            event_type: "login".to_string(),
            status: "success".to_string(),
            error_message: None,
            ip_address: ip_address.map(|s| s.to_string()),
            user_agent: user_agent.map(|s| s.to_string()),
            request_id: relay_state.map(|s| s.to_string()),
            attributes: attributes.clone(),
        }).await?;
        
        info!("SAML authentication successful for user: {}", user_id);
        
        Ok(SamlAuthResponse {
            session_id,
            user_id,
            name_id,
            issuer,
            attributes,
            expires_at: session.expires_at,
        })
    }

    pub async fn initiate_logout(
        &self,
        session_id: &str,
        reason: Option<&str>,
    ) -> Result<String, ApiError> {
        let session = self.storage.get_session(session_id).await?
            .ok_or_else(|| ApiError::not_found("Session not found"))?;
        
        let metadata = self.get_idp_metadata().await?;
        
        let slo_url = metadata.slo_url
            .ok_or_else(|| ApiError::bad_request("IdP does not support Single Logout"))?;
        
        let request_id = Self::generate_request_id();
        
        self.storage.create_logout_request(CreateSamlLogoutRequestRequest {
            request_id: request_id.clone(),
            session_id: Some(session_id.to_string()),
            user_id: Some(session.user_id.clone()),
            name_id: session.name_id.clone(),
            issuer: session.issuer.clone(),
            reason: reason.map(|s| s.to_string()),
        }).await?;
        
        let logout_request = self.build_logout_request(&request_id, &session);
        
        let redirect_url = self.build_redirect_url(&slo_url, &logout_request, None);
        
        self.storage.invalidate_session(session_id).await?;
        
        info!("Initiated SAML logout for session: {}", session_id);
        
        Ok(redirect_url)
    }

    pub async fn process_logout_response(
        &self,
        saml_response: &str,
    ) -> Result<(), ApiError> {
        let decoded = Self::decode_saml_response(saml_response)?;
        
        let request_id = Self::extract_in_response_to(&decoded)?;
        
        let logout_request = self.storage.get_logout_request(&request_id).await?
            .ok_or_else(|| ApiError::not_found("Logout request not found"))?;
        
        self.storage.process_logout_request(&request_id).await?;
        
        if let Some(session_id) = logout_request.session_id {
            let _ = self.storage.invalidate_session(&session_id).await;
        }
        
        info!("Processed SAML logout response for request: {}", request_id);
        
        Ok(())
    }

    pub async fn get_session(&self, session_id: &str) -> Result<Option<SamlSession>, ApiError> {
        let session = self.storage.get_session(session_id).await?;
        
        if let Some(ref session) = session {
            if session.expires_at < Utc::now() {
                self.storage.invalidate_session(session_id).await?;
                return Ok(None);
            }
            self.storage.update_session_last_used(session_id).await?;
        }
        
        Ok(session)
    }

    pub async fn get_user_mapping(&self, user_id: &str) -> Result<Option<SamlUserMapping>, ApiError> {
        self.storage.get_user_mapping_by_user_id(user_id).await
    }

    pub async fn get_idp_metadata(&self) -> Result<SamlMetadata, ApiError> {
        if let Some(ref metadata) = self.cached_metadata {
            if let Some(last_refresh) = self.metadata_last_refresh {
                let refresh_interval = chrono::Duration::seconds(self.config.metadata_refresh_interval as i64);
                if Utc::now() - last_refresh < refresh_interval {
                    return Ok(metadata.clone());
                }
            }
        }
        
        self.fetch_idp_metadata().await
    }

    async fn fetch_idp_metadata(&self) -> Result<SamlMetadata, ApiError> {
        let metadata_xml = if let Some(ref url) = self.config.metadata_url {
            let response = self.http_client.get(url).send().await
                .map_err(|e| ApiError::internal(format!("Failed to fetch IdP metadata: {}", e)))?;
            
            if !response.status().is_success() {
                return Err(ApiError::internal(format!("IdP metadata request failed: {}", response.status())));
            }
            
            response.text().await
                .map_err(|e| ApiError::internal(format!("Failed to read IdP metadata: {}", e)))?
        } else if let Some(ref xml) = self.config.metadata_xml {
            xml.clone()
        } else {
            return Err(ApiError::internal("No IdP metadata configured"));
        };
        
        let metadata = Self::parse_metadata_xml(&metadata_xml)?;
        
        info!("Fetched SAML IdP metadata for: {}", metadata.entity_id);
        
        Ok(metadata)
    }

    fn map_user(
        &self,
        name_id: &str,
        issuer: &str,
        attributes: &HashMap<String, Vec<String>>,
    ) -> Result<SamlUser, ApiError> {
        let mapping = &self.config.attribute_mapping;
        
        let localpart = if self.config.use_name_id_for_user_id {
            name_id.to_string()
        } else if let Some(uid_attr) = &mapping.uid {
            attributes.get(uid_attr)
                .and_then(|v| v.first())
                .map(|s| {
                    self.config.user_id_template.replace("{uid}", s)
                })
                .unwrap_or_else(|| name_id.to_string())
        } else {
            name_id.to_string()
        };
        
        let displayname = mapping.displayname.as_ref()
            .and_then(|attr| attributes.get(attr))
            .and_then(|v| v.first())
            .map(|s| s.to_string());
        
        let email = mapping.email.as_ref()
            .and_then(|attr| attributes.get(attr))
            .and_then(|v| v.first())
            .map(|s| s.to_string());
        
        Ok(SamlUser {
            name_id: name_id.to_string(),
            localpart,
            displayname,
            email,
            issuer: issuer.to_string(),
        })
    }

    fn validate_response(&self, issuer: &str, _response: &str) -> Result<(), ApiError> {
        if !self.config.allowed_idp_entity_ids.is_empty()
            && !self.config.allowed_idp_entity_ids.iter().any(|id| id == issuer) {
                return Err(ApiError::unauthorized("IdP not allowed"));
            }
        
        Ok(())
    }

    fn build_authn_request(&self, request_id: &str, sp_entity_id: &str, acs_url: &str) -> String {
        let nameid_format = &self.config.nameid_format;
        
        let request = format!(
            r#"<samlp:AuthnRequest xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
   ID="{}"
   Version="2.0"
   ProtocolBinding="urn:oasis:names:tc:SAML:2.0:bindings:HTTP-POST"
   AssertionConsumerServiceURL="{}"
   IssueInstant="{}">
   <saml:Issuer xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">{}</saml:Issuer>
   <samlp:NameIDPolicy Format="{}" AllowCreate="true"/>
</samlp:AuthnRequest>"#,
            request_id,
            acs_url,
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            sp_entity_id,
            nameid_format
        );
        
        general_purpose::STANDARD.encode(request.as_bytes())
    }

    fn build_logout_request(&self, request_id: &str, session: &SamlSession) -> String {
        let name_id = session.name_id.as_deref().unwrap_or("");
        let sp_entity_id = &self.config.sp_entity_id;
        
        let request = format!(
            r#"<samlp:LogoutRequest xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
   ID="{}"
   Version="2.0"
   IssueInstant="{}">
   <saml:Issuer xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">{}</saml:Issuer>
   <saml:NameID xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">{}</saml:NameID>
   <samlp:SessionIndex>{}</samlp:SessionIndex>
</samlp:LogoutRequest>"#,
            request_id,
            Utc::now().format("%Y-%m-%dT%H:%M:%SZ"),
            sp_entity_id,
            name_id,
            session.session_index.as_deref().unwrap_or("")
        );
        
        general_purpose::STANDARD.encode(request.as_bytes())
    }

    fn build_redirect_url(&self, base_url: &str, saml_request: &str, relay_state: Option<&str>) -> String {
        let mut url = url::Url::parse(base_url).unwrap();
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("SAMLRequest", saml_request);
            if let Some(state) = relay_state {
                query.append_pair("RelayState", state);
            }
        }
        url.to_string()
    }

    fn decode_saml_response(response: &str) -> Result<String, ApiError> {
        general_purpose::STANDARD.decode(response)
            .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
            .map_err(|e| ApiError::bad_request(format!("Invalid SAML response encoding: {}", e)))
    }

    fn parse_saml_assertion(xml: &str) -> Result<(String, String, HashMap<String, Vec<String>>, Option<String>), ApiError> {
        let mut name_id = String::new();
        let mut issuer = String::new();
        let mut attributes = HashMap::new();
        let mut session_index = None;
        
        if let Some(start) = xml.find("<saml:NameID>") {
            if let Some(end) = xml[start..].find("</saml:NameID>") {
                name_id = xml[start + 14..start + end].to_string();
            }
        }
        
        if let Some(start) = xml.find("<saml:Issuer>") {
            if let Some(end) = xml[start..].find("</saml:Issuer>") {
                issuer = xml[start + 14..start + end].to_string();
            }
        }
        
        if let Some(start) = xml.find("SessionIndex=\"") {
            let rest = &xml[start + 14..];
            if let Some(end) = rest.find('"') {
                session_index = Some(rest[..end].to_string());
            }
        }
        
        let attr_pattern = "<saml:Attribute Name=\"";
        let mut pos = 0;
        while let Some(start) = xml[pos..].find(attr_pattern) {
            let attr_start = pos + start + attr_pattern.len();
            if let Some(name_end) = xml[attr_start..].find('"') {
                let attr_name = xml[attr_start..attr_start + name_end].to_string();
                
                let value_start = attr_start + name_end;
                if let Some(values_start) = xml[value_start..].find("<saml:AttributeValue>") {
                    let vs = value_start + values_start + 20;
                    if let Some(values_end) = xml[vs..].find("</saml:AttributeValue>") {
                        let value = xml[vs..vs + values_end].to_string();
                        attributes.entry(attr_name).or_insert_with(Vec::new).push(value);
                    }
                }
            }
            pos = attr_start;
        }
        
        Ok((name_id, issuer, attributes, session_index))
    }

    fn parse_metadata_xml(xml: &str) -> Result<SamlMetadata, ApiError> {
        let mut entity_id = String::new();
        let mut sso_url = String::new();
        let mut slo_url = None;
        let mut certificate = String::new();
        let valid_until = None;
        
        if let Some(start) = xml.find("entityID=\"") {
            if let Some(end) = xml[start + 10..].find('"') {
                entity_id = xml[start + 10..start + 10 + end].to_string();
            }
        }
        
        if let Some(start) = xml.find("<md:SingleSignOnService") {
            let rest = &xml[start..];
            if let Some(loc_start) = rest.find("Location=\"") {
                if let Some(loc_end) = rest[loc_start + 10..].find('"') {
                    sso_url = rest[loc_start + 10..loc_start + 10 + loc_end].to_string();
                }
            }
        }
        
        if let Some(start) = xml.find("<md:SingleLogoutService") {
            let rest = &xml[start..];
            if let Some(loc_start) = rest.find("Location=\"") {
                if let Some(loc_end) = rest[loc_start + 10..].find('"') {
                    slo_url = Some(rest[loc_start + 10..loc_start + 10 + loc_end].to_string());
                }
            }
        }
        
        if let Some(start) = xml.find("<ds:X509Certificate>") {
            if let Some(end) = xml[start..].find("</ds:X509Certificate>") {
                certificate = xml[start + 20..start + end].to_string();
            }
        }
        
        if entity_id.is_empty() || sso_url.is_empty() {
            return Err(ApiError::internal("Invalid IdP metadata: missing required fields"));
        }
        
        Ok(SamlMetadata {
            entity_id,
            sso_url,
            slo_url,
            certificate,
            valid_until,
        })
    }

    fn extract_in_response_to(xml: &str) -> Result<String, ApiError> {
        if let Some(start) = xml.find("InResponseTo=\"") {
            if let Some(end) = xml[start + 14..].find('"') {
                return Ok(xml[start + 14..start + 14 + end].to_string());
            }
        }
        Err(ApiError::bad_request("No InResponseTo in SAML response"))
    }

    fn generate_request_id() -> String {
        format!("id_{}", uuid::Uuid::new_v4().to_string().replace("-", ""))
    }

    fn generate_session_id() -> String {
        uuid::Uuid::new_v4().to_string()
    }

    pub fn get_config(&self) -> &SamlConfig {
        &self.config
    }

    pub async fn cleanup_expired_sessions(&self) -> Result<u64, ApiError> {
        self.storage.cleanup_expired_sessions().await
    }

    pub async fn cleanup_old_auth_events(&self, days: i64) -> Result<u64, ApiError> {
        self.storage.cleanup_old_auth_events(days).await
    }
}

pub struct SamlIdpManager {
    storage: Arc<SamlStorage>,
}

impl SamlIdpManager {
    pub fn new(storage: Arc<SamlStorage>) -> Self {
        Self { storage }
    }

    pub async fn register_idp(
        &self,
        request: CreateSamlIdentityProviderRequest,
    ) -> Result<SamlIdentityProvider, ApiError> {
        self.storage.create_identity_provider(request).await
    }

    pub async fn get_idp(&self, entity_id: &str) -> Result<Option<SamlIdentityProvider>, ApiError> {
        self.storage.get_identity_provider(entity_id).await
    }

    pub async fn list_idps(&self) -> Result<Vec<SamlIdentityProvider>, ApiError> {
        self.storage.get_all_identity_providers().await
    }

    pub async fn list_enabled_idps(&self) -> Result<Vec<SamlIdentityProvider>, ApiError> {
        self.storage.get_enabled_identity_providers().await
    }

    pub async fn delete_idp(&self, entity_id: &str) -> Result<(), ApiError> {
        self.storage.delete_identity_provider(entity_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
