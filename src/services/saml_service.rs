use crate::common::config::SamlConfig;
use crate::common::error::ApiError;
use crate::common::xml_parser::{parse_saml_metadata, parse_saml_response};
use crate::storage::saml::*;
use base64::{engine::general_purpose, Engine as _};
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tracing::info;

const SAML_REQUEST_TTL_SECONDS: u64 = 600;
const SAML_CLOCK_SKEW_SECONDS: i64 = 300;

#[derive(Debug, Clone)]
struct SamlPendingRequest {
    request_id: String,
    expires_at: u64,
}

static SAML_PENDING_REQUESTS: OnceLock<Mutex<HashMap<String, SamlPendingRequest>>> =
    OnceLock::new();

fn saml_pending_requests() -> &'static Mutex<HashMap<String, SamlPendingRequest>> {
    SAML_PENDING_REQUESTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn current_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn cleanup_expired_saml_requests(requests: &mut HashMap<String, SamlPendingRequest>, now: u64) {
    requests.retain(|_, request| request.expires_at >= now);
}

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
    pub expires_at: i64,
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
    pub fn new(config: Arc<SamlConfig>, storage: Arc<SamlStorage>, server_name: String) -> Self {
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

    pub async fn get_auth_redirect(
        &self,
        relay_state: Option<&str>,
    ) -> Result<SamlAuthRequest, ApiError> {
        let request_id = Self::generate_request_id();
        self.store_pending_request(&request_id, relay_state)?;

        let metadata = self.get_idp_metadata().await?;

        let sso_url = metadata.sso_url;

        let sp_entity_id = &self.config.sp_entity_id;
        let acs_url = self.config.get_sp_acs_url(&self.server_name);

        let authn_request = self.build_authn_request(&request_id, sp_entity_id, &acs_url);

        let redirect_url = self.build_redirect_url(&sso_url, &authn_request, relay_state)?;

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

        let expected_in_response_to = self.consume_pending_request(relay_state)?;
        self.validate_response(&issuer, &decoded, expected_in_response_to.as_deref())?;

        let user = self.map_user(&name_id, &issuer, &attributes)?;

        let existing_mapping = self
            .storage
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

        let session = self
            .storage
            .create_session(CreateSamlSessionRequest {
                session_id: session_id.clone(),
                user_id: user_id.clone(),
                name_id: Some(name_id.clone()),
                issuer: Some(issuer.clone()),
                session_index,
                attributes: attributes.clone(),
                expires_in_seconds: self.config.session_lifetime as i64,
            })
            .await?;

        self.storage
            .create_user_mapping(CreateSamlUserMappingRequest {
                name_id: name_id.clone(),
                user_id: user_id.clone(),
                issuer: issuer.clone(),
                attributes: attributes.clone(),
            })
            .await?;

        self.storage
            .create_auth_event(CreateSamlAuthEventRequest {
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
            })
            .await?;

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
        let session = self
            .storage
            .get_session(session_id)
            .await?
            .ok_or_else(|| ApiError::not_found("Session not found"))?;

        let metadata = self.get_idp_metadata().await?;

        let slo_url = metadata
            .slo_url
            .ok_or_else(|| ApiError::bad_request("IdP does not support Single Logout"))?;

        let request_id = Self::generate_request_id();

        self.storage
            .create_logout_request(CreateSamlLogoutRequestRequest {
                request_id: request_id.clone(),
                session_id: Some(session_id.to_string()),
                user_id: Some(session.user_id.clone()),
                name_id: session.name_id.clone(),
                issuer: session.issuer.clone(),
                reason: reason.map(|s| s.to_string()),
            })
            .await?;

        let logout_request = self.build_logout_request(&request_id, &session);

        let redirect_url = self.build_redirect_url(&slo_url, &logout_request, None)?;

        self.storage.invalidate_session(session_id).await?;

        info!("Initiated SAML logout for session: {}", session_id);

        Ok(redirect_url)
    }

    pub async fn process_logout_response(&self, saml_response: &str) -> Result<(), ApiError> {
        let decoded = Self::decode_saml_response(saml_response)?;

        let request_id = Self::extract_in_response_to(&decoded)?;

        let logout_request = self
            .storage
            .get_logout_request(&request_id)
            .await?
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
            if session.expires_at < Utc::now().timestamp_millis() {
                self.storage.invalidate_session(session_id).await?;
                return Ok(None);
            }
            self.storage.update_session_last_used(session_id).await?;
        }

        Ok(session)
    }

    pub async fn get_user_mapping(
        &self,
        user_id: &str,
    ) -> Result<Option<SamlUserMapping>, ApiError> {
        self.storage.get_user_mapping_by_user_id(user_id).await
    }

    pub async fn get_idp_metadata(&self) -> Result<SamlMetadata, ApiError> {
        if let Some(ref metadata) = self.cached_metadata {
            if let Some(last_refresh) = self.metadata_last_refresh {
                let refresh_interval =
                    chrono::Duration::seconds(self.config.metadata_refresh_interval as i64);
                if Utc::now() - last_refresh < refresh_interval {
                    return Ok(metadata.clone());
                }
            }
        }

        self.fetch_idp_metadata().await
    }

    async fn fetch_idp_metadata(&self) -> Result<SamlMetadata, ApiError> {
        let metadata_xml =
            if let Some(ref url) = self.config.metadata_url {
                let response = self.http_client.get(url).send().await.map_err(|e| {
                    ApiError::internal(format!("Failed to fetch IdP metadata: {}", e))
                })?;

                if !response.status().is_success() {
                    return Err(ApiError::internal(format!(
                        "IdP metadata request failed: {}",
                        response.status()
                    )));
                }

                response.text().await.map_err(|e| {
                    ApiError::internal(format!("Failed to read IdP metadata: {}", e))
                })?
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
            attributes
                .get(uid_attr)
                .and_then(|v| v.first())
                .map(|s| self.config.user_id_template.replace("{uid}", s))
                .unwrap_or_else(|| name_id.to_string())
        } else {
            name_id.to_string()
        };

        let displayname = mapping
            .displayname
            .as_ref()
            .and_then(|attr| attributes.get(attr))
            .and_then(|v| v.first())
            .map(|s| s.to_string());

        let email = mapping
            .email
            .as_ref()
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

    fn validate_response(
        &self,
        issuer: &str,
        response: &str,
        expected_in_response_to: Option<&str>,
    ) -> Result<(), ApiError> {
        if !self.config.allowed_idp_entity_ids.is_empty()
            && !self
                .config
                .allowed_idp_entity_ids
                .iter()
                .any(|id| id == issuer)
        {
            return Err(ApiError::unauthorized("IdP not allowed"));
        }

        Self::validate_response_time_window(response)?;
        Self::validate_response_audience(response, &self.config.sp_entity_id)?;
        Self::validate_response_status(response)?;
        Self::validate_response_destination(
            response,
            &self.config.get_sp_acs_url(&self.server_name),
        )?;
        Self::validate_response_recipient(
            response,
            &self.config.get_sp_acs_url(&self.server_name),
        )?;
        Self::validate_response_issuer(response, issuer)?;

        let in_response_to = Self::extract_in_response_to(response)?;
        if let Some(expected_in_response_to) = expected_in_response_to {
            if in_response_to != expected_in_response_to {
                return Err(ApiError::unauthorized("Unexpected InResponseTo"));
            }
        }

        if self.config.want_response_signed || self.config.want_assertions_signed {
            if let Err(e) = self.verify_saml_signature(response) {
                tracing::warn!("SAML signature verification failed: {}", e);
                if self.config.want_response_signed || self.config.want_assertions_signed {
                    return Err(ApiError::unauthorized(format!(
                        "SAML signature verification failed: {}",
                        e
                    )));
                }
            }
        }

        Ok(())
    }

    fn verify_saml_signature(&self, xml: &str) -> Result<(), String> {
        let metadata = match self.cached_metadata.clone() {
            Some(m) => m,
            None => return Err("No IdP metadata available for signature verification".to_string()),
        };

        if metadata.certificate.is_empty() {
            return Err("No IdP certificate available for signature verification".to_string());
        }

        let cert_der = match general_purpose::STANDARD.decode(&metadata.certificate) {
            Ok(bytes) => bytes,
            Err(e) => return Err(format!("Invalid IdP certificate encoding: {}", e)),
        };

        let has_response_sig = xml.contains("<ds:Signature") || xml.contains("<Signature");
        let has_assertion_sig = xml.contains("<ds:Signature") && xml.contains("<saml:Assertion");

        if self.config.want_response_signed && !has_response_sig && !has_assertion_sig {
            return Err("SAML response is not signed but signature is required".to_string());
        }

        if !has_response_sig && !has_assertion_sig {
            return Ok(());
        }

        let signature_value = Self::extract_signature_value(xml);
        let signed_info = Self::extract_signed_info(xml);
        let digest_value = Self::extract_digest_value(xml);

        let (Some(sig_value), Some(signed_info_xml), Some(digest)) =
            (signature_value, signed_info, digest_value)
        else {
            return Err("Could not extract signature components from SAML response".to_string());
        };

        let sig_bytes = match general_purpose::STANDARD.decode(&sig_value) {
            Ok(bytes) => bytes,
            Err(e) => return Err(format!("Invalid signature base64: {}", e)),
        };

        let digest_bytes = match general_purpose::STANDARD.decode(&digest) {
            Ok(bytes) => bytes,
            Err(e) => return Err(format!("Invalid digest base64: {}", e)),
        };

        let canonicalized_info = Self::canonicalize_xml(&signed_info_xml);

        let computed_digest = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(canonicalized_info.as_bytes());
            hasher.finalize().to_vec()
        };

        if !Self::constant_time_compare(&digest_bytes, &computed_digest) {
            return Err(
                "SAML digest verification failed - response may be tampered with".to_string(),
            );
        }

        Self::verify_rsa_signature(&cert_der, &sig_bytes, canonicalized_info.as_bytes())?;

        tracing::info!("SAML signature verified (digest + RSA-SHA256)");
        Ok(())
    }

    fn extract_signature_value(xml: &str) -> Option<String> {
        Regex::new(r#"<(?:\w+:)?SignatureValue>\s*([^<]+?)\s*</(?:\w+:)?SignatureValue>"#)
            .ok()
            .and_then(|regex| {
                regex
                    .captures(xml)
                    .and_then(|captures| captures.get(1).map(|m| m.as_str().to_string()))
            })
    }

    fn extract_signed_info(xml: &str) -> Option<String> {
        Regex::new(r#"<(?:\w+:)?SignedInfo>([\s\S]*?)</(?:\w+:)?SignedInfo>"#)
            .ok()
            .and_then(|regex| {
                regex
                    .captures(xml)
                    .and_then(|captures| captures.get(1).map(|m| m.as_str().to_string()))
            })
    }

    fn extract_digest_value(xml: &str) -> Option<String> {
        Regex::new(r#"<(?:\w+:)?DigestValue>\s*([^<]+?)\s*</(?:\w+:)?DigestValue>"#)
            .ok()
            .and_then(|regex| {
                regex
                    .captures(xml)
                    .and_then(|captures| captures.get(1).map(|m| m.as_str().to_string()))
            })
    }

    fn canonicalize_xml(xml: &str) -> String {
        xml.replace("\r\n", "\n")
            .replace("\r", "\n")
            .lines()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn constant_time_compare(a: &[u8], b: &[u8]) -> bool {
        if a.len() != b.len() {
            return false;
        }
        let mut result = 0u8;
        for (x, y) in a.iter().zip(b.iter()) {
            result |= x ^ y;
        }
        result == 0
    }

    fn verify_rsa_signature(
        cert_der: &[u8],
        signature: &[u8],
        signed_data: &[u8],
    ) -> Result<(), String> {
        use x509_cert::der::{Decode, Encode};

        let cert_der_bytes = if cert_der.starts_with(b"-----BEGIN") {
            let pem_str =
                std::str::from_utf8(cert_der).map_err(|e| format!("Invalid UTF-8: {}", e))?;
            let b64_content = pem_str
                .lines()
                .filter(|line| !line.starts_with("-----"))
                .collect::<Vec<_>>()
                .join("");
            base64::engine::general_purpose::STANDARD
                .decode(&b64_content)
                .map_err(|e| format!("Failed to decode PEM base64: {}", e))?
        } else {
            cert_der.to_vec()
        };

        let cert = match x509_cert::Certificate::from_der(&cert_der_bytes) {
            Ok(c) => c,
            Err(e) => return Err(format!("Failed to parse X.509 certificate: {}", e)),
        };

        let spki_der = match cert.tbs_certificate.subject_public_key_info.to_der() {
            Ok(der) => der,
            Err(e) => return Err(format!("Failed to encode SPKI: {}", e)),
        };

        let public_key = ring::signature::UnparsedPublicKey::new(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            &spki_der,
        );

        public_key
            .verify(signed_data, signature)
            .map_err(|e| format!("RSA-SHA256 signature verification failed: {:?}", e))
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

    fn build_redirect_url(
        &self,
        base_url: &str,
        saml_request: &str,
        relay_state: Option<&str>,
    ) -> Result<String, ApiError> {
        let mut url = url::Url::parse(base_url)
            .map_err(|e| ApiError::internal(format!("Invalid SAML base URL: {}", e)))?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("SAMLRequest", saml_request);
            if let Some(state) = relay_state {
                query.append_pair("RelayState", state);
            }
        }

        if self.config.sign_requests {
            if let Some(ref key_pem) = self.config.sp_private_key {
                match self.sign_redirect_url(&url, key_pem) {
                    Ok(signed_url) => return Ok(signed_url),
                    Err(e) => {
                        tracing::warn!("Failed to sign SAML redirect request: {}", e);
                    }
                }
            } else {
                tracing::warn!(
                    "SAML sign_requests is enabled but sp_private_key is not configured"
                );
            }
        }

        Ok(url.to_string())
    }

    fn sign_redirect_url(&self, url: &url::Url, private_key_pem: &str) -> Result<String, String> {
        let query = url.query().unwrap_or("");
        let sig_alg = "http://www.w3.org/2001/04/xmldsig-more#rsa-sha256";

        let private_key = pem_to_rsa_private_key(private_key_pem)?;

        let signing_key = ring::signature::RsaKeyPair::from_der(&private_key)
            .map_err(|e| format!("Invalid RSA private key: {}", e))?;

        let mut signature = vec![0u8; signing_key.public().modulus_len()];
        signing_key
            .sign(
                &ring::signature::RSA_PKCS1_SHA256,
                &ring::rand::SystemRandom::new(),
                query.as_bytes(),
                &mut signature,
            )
            .map_err(|e| format!("Failed to sign SAML request: {}", e))?;

        let sig_b64 = general_purpose::STANDARD.encode(&signature);

        let mut signed_url = url.clone();
        {
            let mut query = signed_url.query_pairs_mut();
            query.append_pair("SigAlg", sig_alg);
            query.append_pair("Signature", &sig_b64);
        }

        Ok(signed_url.to_string())
    }

    fn decode_saml_response(response: &str) -> Result<String, ApiError> {
        general_purpose::STANDARD
            .decode(response)
            .map(|bytes| String::from_utf8_lossy(&bytes).to_string())
            .map_err(|e| ApiError::bad_request(format!("Invalid SAML response encoding: {}", e)))
    }

    #[allow(clippy::type_complexity)]
    fn parse_saml_assertion(
        xml: &str,
    ) -> Result<(String, String, HashMap<String, Vec<String>>, Option<String>), ApiError> {
        let data = parse_saml_response(xml)
            .map_err(|e| ApiError::bad_request(format!("Failed to parse SAML assertion: {}", e)))?;

        Ok((
            data.name_id,
            data.issuer,
            data.attributes,
            data.session_index,
        ))
    }

    fn parse_metadata_xml(xml: &str) -> Result<SamlMetadata, ApiError> {
        let parsed = parse_saml_metadata(xml)
            .map_err(|e| ApiError::internal(format!("Failed to parse SAML metadata: {}", e)))?;

        Ok(SamlMetadata {
            entity_id: parsed.entity_id,
            sso_url: parsed.sso_url,
            slo_url: parsed.slo_url,
            certificate: parsed.certificate,
            valid_until: None,
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

    fn extract_attribute_values(xml: &str, attribute: &str) -> Vec<String> {
        let pattern = format!(r#"{attribute}="([^"]+)""#);
        Regex::new(&pattern)
            .ok()
            .map(|regex| {
                regex
                    .captures_iter(xml)
                    .filter_map(|captures| captures.get(1).map(|value| value.as_str().to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn extract_audiences(xml: &str) -> Vec<String> {
        Regex::new(r#"<(?:\w+:)?Audience>\s*([^<]+?)\s*</(?:\w+:)?Audience>"#)
            .ok()
            .map(|regex| {
                regex
                    .captures_iter(xml)
                    .filter_map(|captures| {
                        captures
                            .get(1)
                            .map(|value| value.as_str().trim().to_string())
                    })
                    .filter(|value| !value.is_empty())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn parse_saml_timestamp(value: &str) -> Result<DateTime<Utc>, ApiError> {
        DateTime::parse_from_rfc3339(value)
            .map(|value| value.with_timezone(&Utc))
            .map_err(|_| ApiError::unauthorized(format!("Invalid SAML timestamp: {}", value)))
    }

    fn validate_response_time_window(response: &str) -> Result<(), ApiError> {
        let now = Utc::now();
        let skew = chrono::Duration::seconds(SAML_CLOCK_SKEW_SECONDS);

        for not_before in Self::extract_attribute_values(response, "NotBefore") {
            let not_before = Self::parse_saml_timestamp(&not_before)?;
            if now + skew < not_before {
                return Err(ApiError::unauthorized("SAML response is not yet valid"));
            }
        }

        for not_on_or_after in Self::extract_attribute_values(response, "NotOnOrAfter") {
            let not_on_or_after = Self::parse_saml_timestamp(&not_on_or_after)?;
            if now - skew >= not_on_or_after {
                return Err(ApiError::unauthorized("SAML response has expired"));
            }
        }

        Ok(())
    }

    fn validate_response_audience(response: &str, expected_audience: &str) -> Result<(), ApiError> {
        let audiences = Self::extract_audiences(response);
        if audiences.is_empty() {
            return Err(ApiError::unauthorized("Missing SAML audience"));
        }
        if audiences
            .iter()
            .any(|audience| audience == expected_audience)
        {
            return Ok(());
        }
        Err(ApiError::unauthorized("SAML audience mismatch"))
    }

    fn extract_status_codes(xml: &str) -> Vec<String> {
        Regex::new(r#"<(?:\w+:)?StatusCode[^>]*\sValue="([^"]+)""#)
            .ok()
            .map(|regex| {
                regex
                    .captures_iter(xml)
                    .filter_map(|captures| captures.get(1).map(|value| value.as_str().to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn extract_response_destination(xml: &str) -> Option<String> {
        Regex::new(r#"<(?:\w+:)?Response[^>]*\sDestination="([^"]+)""#)
            .ok()
            .and_then(|regex| {
                regex
                    .captures(xml)
                    .and_then(|captures| captures.get(1).map(|value| value.as_str().to_string()))
            })
    }

    fn extract_subject_confirmation_recipients(xml: &str) -> Vec<String> {
        Regex::new(r#"<(?:\w+:)?SubjectConfirmationData[^>]*\sRecipient="([^"]+)""#)
            .ok()
            .map(|regex| {
                regex
                    .captures_iter(xml)
                    .filter_map(|captures| captures.get(1).map(|value| value.as_str().to_string()))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn extract_response_issuers(xml: &str) -> Vec<String> {
        Regex::new(r#"<(?:\w+:)?Response[^>]*>[\s\S]*?<(?:(?:\w+):)?Issuer>\s*([^<]+?)\s*</(?:(?:\w+):)?Issuer>"#)
            .ok()
            .and_then(|regex| {
                regex
                    .captures(xml)
                    .and_then(|captures| captures.get(1).map(|value| value.as_str().trim().to_string()))
            })
            .map(|issuer| vec![issuer])
            .unwrap_or_default()
    }

    fn validate_response_status(response: &str) -> Result<(), ApiError> {
        let status_codes = Self::extract_status_codes(response);
        if status_codes.is_empty() {
            return Err(ApiError::unauthorized("Missing SAML status code"));
        }
        if status_codes
            .iter()
            .any(|status| status.ends_with(":Success"))
        {
            return Ok(());
        }
        Err(ApiError::unauthorized("SAML status is not success"))
    }

    fn validate_response_destination(
        response: &str,
        expected_destination: &str,
    ) -> Result<(), ApiError> {
        if let Some(destination) = Self::extract_response_destination(response) {
            if destination != expected_destination {
                return Err(ApiError::unauthorized("SAML destination mismatch"));
            }
        }
        Ok(())
    }

    fn validate_response_recipient(
        response: &str,
        expected_recipient: &str,
    ) -> Result<(), ApiError> {
        let recipients = Self::extract_subject_confirmation_recipients(response);
        if recipients.is_empty() {
            return Ok(());
        }
        if recipients
            .iter()
            .any(|recipient| recipient == expected_recipient)
        {
            return Ok(());
        }
        Err(ApiError::unauthorized("SAML recipient mismatch"))
    }

    fn validate_response_issuer(response: &str, expected_issuer: &str) -> Result<(), ApiError> {
        let response_issuers = Self::extract_response_issuers(response);
        if response_issuers.is_empty() {
            return Ok(());
        }
        if response_issuers
            .iter()
            .any(|issuer| issuer == expected_issuer)
        {
            return Ok(());
        }
        Err(ApiError::unauthorized("SAML response issuer mismatch"))
    }

    fn store_pending_request(
        &self,
        request_id: &str,
        relay_state: Option<&str>,
    ) -> Result<(), ApiError> {
        let Some(relay_state) = relay_state else {
            return Ok(());
        };

        let now = current_unix_seconds();
        let mut requests = saml_pending_requests()
            .lock()
            .map_err(|_| ApiError::internal("Failed to acquire SAML request lock"))?;
        cleanup_expired_saml_requests(&mut requests, now);
        requests.insert(
            relay_state.to_string(),
            SamlPendingRequest {
                request_id: request_id.to_string(),
                expires_at: now + SAML_REQUEST_TTL_SECONDS,
            },
        );
        Ok(())
    }

    fn consume_pending_request(
        &self,
        relay_state: Option<&str>,
    ) -> Result<Option<String>, ApiError> {
        let Some(relay_state) = relay_state else {
            return Ok(None);
        };

        let now = current_unix_seconds();
        let mut requests = saml_pending_requests()
            .lock()
            .map_err(|_| ApiError::internal("Failed to acquire SAML request lock"))?;
        cleanup_expired_saml_requests(&mut requests, now);
        let request = requests
            .remove(relay_state)
            .ok_or_else(|| ApiError::unauthorized("Unknown or expired RelayState"))?;
        if request.expires_at < now {
            return Err(ApiError::unauthorized("Expired SAML request"));
        }
        Ok(Some(request.request_id))
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

fn pem_to_rsa_private_key(pem: &str) -> Result<Vec<u8>, String> {
    let der = pem.lines().filter(|line| !line.starts_with("-----")).fold(
        String::new(),
        |mut acc, line| {
            acc.push_str(line.trim());
            acc
        },
    );
    general_purpose::STANDARD
        .decode(&der)
        .map_err(|e| format!("Failed to decode PEM base64: {}", e))
}

#[cfg(test)]
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
        SamlService::new(
            Arc::new(create_test_config()),
            storage,
            "localhost".to_string(),
        )
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
        assert_eq!(
            metadata.slo_url,
            Some("https://idp.example.com/slo".to_string())
        );
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

        let (name_id, issuer, attributes, session_index) =
            SamlService::parse_saml_assertion(xml).unwrap();
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
            eprintln!("Validation failed: {:?}", e);
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

        let error = service
            .validate_response("https://idp.example.com", &xml, Some("id_123"))
            .unwrap_err();
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

        let error = service
            .validate_response("https://idp.example.com", &xml, Some("id_expected"))
            .unwrap_err();
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

        let error = service
            .validate_response("https://idp.example.com", &xml, Some("id_123"))
            .unwrap_err();
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

        let error = service
            .validate_response("https://idp.example.com", &xml, Some("id_123"))
            .unwrap_err();
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

        let error = service
            .validate_response("https://idp.example.com", &xml, Some("id_123"))
            .unwrap_err();
        assert!(error.to_string().contains("recipient mismatch"));
    }
}
