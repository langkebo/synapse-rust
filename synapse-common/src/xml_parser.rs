//! SAML XML metadata/response parser (security-wire format).

use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
/// Represents XmlParseError; see per-variant docs.
pub enum XmlParseError {
    #[error("XML parse error: {0}")]
    /// `Parse` variant.
    Parse(String),
    #[error("Missing required element: {0}")]
    /// `MissingElement` variant.
    MissingElement(String),
    #[error("Invalid XML structure: {0}")]
    /// `InvalidStructure` variant.
    InvalidStructure(String),
}

/// Parses saml response.
pub fn parse_saml_response(xml: &str) -> Result<SamlAssertionData, XmlParseError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut name_id = String::new();
    let mut issuer = String::new();
    let mut attributes: HashMap<String, Vec<String>> = HashMap::new();
    let mut session_index = None;

    let mut current_attr_name = String::new();
    let mut in_name_id = false;
    let mut in_issuer = false;
    let mut in_attr_value = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) => {
                let name = e.name();
                let local_name = name.local_name();
                let name_str = String::from_utf8_lossy(local_name.as_ref());

                match name_str.as_ref() {
                    "NameID" => {
                        in_name_id = true;
                    }
                    "Issuer" => {
                        in_issuer = true;
                    }
                    "Attribute" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"Name" {
                                current_attr_name = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    }
                    "AttributeValue" => {
                        in_attr_value = true;
                    }
                    "AuthnStatement" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"SessionIndex" {
                                session_index = Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                let text = e.xml10_content().unwrap_or_default().to_string();

                if in_name_id {
                    name_id = text;
                    in_name_id = false;
                } else if in_issuer {
                    issuer = text;
                    in_issuer = false;
                } else if in_attr_value && !current_attr_name.is_empty() {
                    attributes.entry(current_attr_name.clone()).or_default().push(text);
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local_name = name.local_name();
                let name_str = String::from_utf8_lossy(local_name.as_ref());

                match name_str.as_ref() {
                    "NameID" => in_name_id = false,
                    "Issuer" => in_issuer = false,
                    "AttributeValue" => {
                        in_attr_value = false;
                        current_attr_name.clear();
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(XmlParseError::Parse(format!("Error parsing XML: {e:?}")));
            }
            _ => {}
        }
        buf.clear();
    }

    if name_id.is_empty() {
        return Err(XmlParseError::MissingElement("NameID".to_string()));
    }

    Ok(SamlAssertionData { name_id, issuer, attributes, session_index })
}

/// Parses saml metadata.
pub fn parse_saml_metadata(xml: &str) -> Result<SamlMetadataParsed, XmlParseError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut entity_id = String::new();
    let mut sso_url = String::new();
    let mut slo_url = None;
    let mut certificate = String::new();

    let mut in_x509_cert = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local_name = name.local_name();
                let name_str = String::from_utf8_lossy(local_name.as_ref());

                match name_str.as_ref() {
                    "EntityDescriptor" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"entityID" {
                                entity_id = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    }
                    "SingleSignOnService" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"Location" {
                                sso_url = String::from_utf8_lossy(&attr.value).to_string();
                            }
                        }
                    }
                    "SingleLogoutService" => {
                        for attr in e.attributes().flatten() {
                            if attr.key.local_name().as_ref() == b"Location" {
                                slo_url = Some(String::from_utf8_lossy(&attr.value).to_string());
                            }
                        }
                    }
                    "X509Certificate" => {
                        in_x509_cert = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Text(ref e)) => {
                if in_x509_cert {
                    certificate = e.xml10_content().unwrap_or_default().to_string();
                    in_x509_cert = false;
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local_name = name.local_name();
                let name_str = String::from_utf8_lossy(local_name.as_ref());

                if name_str == "X509Certificate" {
                    in_x509_cert = false;
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(XmlParseError::Parse(format!("Error parsing XML: {e:?}")));
            }
            _ => {}
        }
        buf.clear();
    }

    if entity_id.is_empty() || sso_url.is_empty() {
        return Err(XmlParseError::MissingElement("entityID or SSO URL".to_string()));
    }

    Ok(SamlMetadataParsed { entity_id, sso_url, slo_url, certificate })
}

/// Reads a (namespace-agnostic) attribute value from a start/empty element.
fn attribute_value(element: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    element
        .attributes()
        .flatten()
        .find(|attr| attr.key.local_name().as_ref() == name)
        .map(|attr| String::from_utf8_lossy(&attr.value).to_string())
}

/// Parses the non-signature envelope fields of a SAML `<Response>` /
/// `<LogoutResponse>`.
///
/// This replaces ad-hoc string/regex scanning with a real XML reader, so
/// attribute lookup is name-based (order/whitespace independent), element
/// nesting is respected, and XML entity references are handled by the parser
/// rather than matched as raw text.
pub fn parse_saml_response_envelope(xml: &str) -> Result<SamlResponseEnvelope, XmlParseError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);

    let mut envelope = SamlResponseEnvelope::default();
    let mut response_seen = false;
    let mut in_audience = false;
    let mut in_issuer = false;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name = e.name();
                let local_name = name.local_name();
                let name_str = String::from_utf8_lossy(local_name.as_ref()).to_string();
                collect_element_attributes(e, &name_str, &mut envelope, &mut response_seen);
                match name_str.as_str() {
                    "Audience" => in_audience = true,
                    // Only the first issuer that follows the top-level <Response>
                    // start tag is the response issuer (matches SAML shape).
                    "Issuer" if response_seen && envelope.response_issuer.is_none() => in_issuer = true,
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                // Self-closing elements carry attributes but no text content.
                let name = e.name();
                let local_name = name.local_name();
                let name_str = String::from_utf8_lossy(local_name.as_ref()).to_string();
                collect_element_attributes(e, &name_str, &mut envelope, &mut response_seen);
            }
            Ok(Event::Text(ref e)) => {
                let text = e.xml10_content().unwrap_or_default().to_string();
                if in_audience {
                    envelope.audiences.push(text);
                } else if in_issuer && envelope.response_issuer.is_none() {
                    envelope.response_issuer = Some(text);
                }
            }
            Ok(Event::End(ref e)) => {
                let name = e.name();
                let local_name = name.local_name();
                match String::from_utf8_lossy(local_name.as_ref()).as_ref() {
                    "Audience" => in_audience = false,
                    "Issuer" => in_issuer = false,
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(XmlParseError::Parse(format!("Error parsing XML: {e:?}")));
            }
            _ => {}
        }
        buf.clear();
    }

    Ok(envelope)
}

fn collect_element_attributes(
    element: &quick_xml::events::BytesStart<'_>,
    local_name: &str,
    envelope: &mut SamlResponseEnvelope,
    response_seen: &mut bool,
) {
    // `InResponseTo` is carried by both <Response> and <LogoutResponse>.
    if local_name == "Response" || local_name == "LogoutResponse" {
        if local_name == "Response" {
            *response_seen = true;
        }
        if envelope.in_response_to.is_none() {
            envelope.in_response_to = attribute_value(element, b"InResponseTo");
        }
        if local_name == "Response" && envelope.destination.is_none() {
            envelope.destination = attribute_value(element, b"Destination");
        }
    }

    if local_name == "StatusCode" {
        if let Some(value) = attribute_value(element, b"Value") {
            envelope.status_codes.push(value);
        }
    }

    if local_name == "SubjectConfirmationData" {
        if let Some(recipient) = attribute_value(element, b"Recipient") {
            envelope.subject_confirmation_recipients.push(recipient);
        }
    }

    if let Some(not_before) = attribute_value(element, b"NotBefore") {
        envelope.not_before.push(not_before);
    }
    if let Some(not_on_or_after) = attribute_value(element, b"NotOnOrAfter") {
        envelope.not_on_or_after.push(not_on_or_after);
    }
}

#[derive(Debug, Clone)]
/// Represents SamlAssertionData.
pub struct SamlAssertionData {
    /// `name_id` field.
    pub name_id: String,
    /// `issuer` field.
    pub issuer: String,
    /// `attributes` field.
    pub attributes: HashMap<String, Vec<String>>,
    /// `session_index` field.
    pub session_index: Option<String>,
}

#[derive(Debug, Clone)]
/// Represents SamlMetadataParsed.
pub struct SamlMetadataParsed {
    /// `entity_id` field.
    pub entity_id: String,
    /// `sso_url` field.
    pub sso_url: String,
    /// `slo_url` field.
    pub slo_url: Option<String>,
    /// `certificate` field.
    pub certificate: String,
}

/// Non-signature fields of a SAML `<Response>` / `<LogoutResponse>` envelope.
///
/// Produced by [`parse_saml_response_envelope`]. The XMLDSig signature block is
/// intentionally out of scope here; it is validated separately by the
/// signature layer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SamlResponseEnvelope {
    /// `InResponseTo` of `<Response>` or `<LogoutResponse>`.
    pub in_response_to: Option<String>,
    /// `Destination` of `<Response>`.
    pub destination: Option<String>,
    /// All `NotBefore` attribute values (e.g. `<Conditions>` / `<SubjectConfirmationData>`).
    pub not_before: Vec<String>,
    /// All `NotOnOrAfter` attribute values.
    pub not_on_or_after: Vec<String>,
    /// All `StatusCode` `Value` attribute values.
    pub status_codes: Vec<String>,
    /// All `<Audience>` text contents.
    pub audiences: Vec<String>,
    /// All `<SubjectConfirmationData>` `Recipient` attribute values.
    pub subject_confirmation_recipients: Vec<String>,
    /// The first `<Issuer>` following the top-level `<Response>` start tag.
    pub response_issuer: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_saml_response() {
        let xml = r#"
        <samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol">
            <saml:Assertion xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Subject>
                    <saml:NameID>user@example.com</saml:NameID>
                </saml:Subject>
                <saml:AttributeStatement>
                    <saml:Attribute Name="email">
                        <saml:AttributeValue>user@example.com</saml:AttributeValue>
                    </saml:Attribute>
                </saml:AttributeStatement>
                <saml:AuthnStatement SessionIndex="session123"/>
            </saml:Assertion>
        </samlp:Response>
        "#;

        let result = parse_saml_response(xml).unwrap();
        assert_eq!(result.name_id, "user@example.com");
        assert_eq!(result.issuer, "https://idp.example.com");
        assert_eq!(result.session_index, Some("session123".to_string()));
    }

    #[test]
    fn test_parse_saml_metadata() {
        let xml = r#"
        <md:EntityDescriptor xmlns:md="urn:oasis:names:tc:SAML:2.0:metadata" entityID="https://idp.example.com">
            <md:IDPSSODescriptor>
                <md:SingleSignOnService Location="https://idp.example.com/sso"/>
                <md:SingleLogoutService Location="https://idp.example.com/slo"/>
                <md:KeyDescriptor>
                    <ds:X509Certificate>MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEA</ds:X509Certificate>
                </md:KeyDescriptor>
            </md:IDPSSODescriptor>
        </md:EntityDescriptor>
        "#;

        let result = parse_saml_metadata(xml).unwrap();
        assert_eq!(result.entity_id, "https://idp.example.com");
        assert_eq!(result.sso_url, "https://idp.example.com/sso");
        assert_eq!(result.slo_url, Some("https://idp.example.com/slo".to_string()));
    }

    #[test]
    fn test_parse_saml_response_missing_name_id() {
        let xml = r#"
        <samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol">
            <saml:Assertion xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">
                <saml:Issuer>https://idp.example.com</saml:Issuer>
            </saml:Assertion>
        </samlp:Response>
        "#;

        let result = parse_saml_response(xml);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), XmlParseError::MissingElement(_)));
    }

    #[test]
    fn test_parse_saml_response_multiple_attributes() {
        let xml = r#"
        <samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol">
            <saml:Assertion xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Subject>
                    <saml:NameID>user@example.com</saml:NameID>
                </saml:Subject>
                <saml:AttributeStatement>
                    <saml:Attribute Name="email">
                        <saml:AttributeValue>user@example.com</saml:AttributeValue>
                    </saml:Attribute>
                    <saml:Attribute Name="groups">
                        <saml:AttributeValue>admin</saml:AttributeValue>
                    </saml:Attribute>
                </saml:AttributeStatement>
            </saml:Assertion>
        </samlp:Response>
        "#;

        let result = parse_saml_response(xml).unwrap();
        assert_eq!(result.name_id, "user@example.com");
        assert_eq!(result.issuer, "https://idp.example.com");
        assert_eq!(result.attributes.get("email").unwrap(), &vec!["user@example.com".to_string()]);
        assert_eq!(result.attributes.get("groups").unwrap(), &vec!["admin".to_string()]);
    }

    #[test]
    fn test_parse_saml_response_no_session_index() {
        let xml = r#"
        <samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol">
            <saml:Assertion xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Subject>
                    <saml:NameID>user@example.com</saml:NameID>
                </saml:Subject>
                <saml:AuthnStatement/>
            </saml:Assertion>
        </samlp:Response>
        "#;

        let result = parse_saml_response(xml).unwrap();
        assert_eq!(result.name_id, "user@example.com");
        assert_eq!(result.session_index, None);
    }

    #[test]
    fn test_parse_saml_response_no_issuer() {
        let xml = r#"
        <samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol">
            <saml:Assertion xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">
                <saml:Subject>
                    <saml:NameID>user@example.com</saml:NameID>
                </saml:Subject>
            </saml:Assertion>
        </samlp:Response>
        "#;

        let result = parse_saml_response(xml).unwrap();
        assert_eq!(result.name_id, "user@example.com");
        assert_eq!(result.issuer, "");
    }

    #[test]
    fn test_parse_saml_metadata_missing_entity_id() {
        let xml = r#"
        <md:EntityDescriptor xmlns:md="urn:oasis:names:tc:SAML:2.0:metadata">
            <md:IDPSSODescriptor>
                <md:SingleSignOnService Location="https://idp.example.com/sso"/>
            </md:IDPSSODescriptor>
        </md:EntityDescriptor>
        "#;

        let result = parse_saml_metadata(xml);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), XmlParseError::MissingElement(_)));
    }

    #[test]
    fn test_parse_saml_metadata_missing_sso_url() {
        let xml = r#"
        <md:EntityDescriptor xmlns:md="urn:oasis:names:tc:SAML:2.0:metadata" entityID="https://idp.example.com">
            <md:IDPSSODescriptor>
            </md:IDPSSODescriptor>
        </md:EntityDescriptor>
        "#;

        let result = parse_saml_metadata(xml);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), XmlParseError::MissingElement(_)));
    }

    #[test]
    fn test_parse_saml_metadata_no_slo_url() {
        let xml = r#"
        <md:EntityDescriptor xmlns:md="urn:oasis:names:tc:SAML:2.0:metadata" entityID="https://idp.example.com">
            <md:IDPSSODescriptor>
                <md:SingleSignOnService Location="https://idp.example.com/sso"/>
            </md:IDPSSODescriptor>
        </md:EntityDescriptor>
        "#;

        let result = parse_saml_metadata(xml).unwrap();
        assert_eq!(result.entity_id, "https://idp.example.com");
        assert_eq!(result.sso_url, "https://idp.example.com/sso");
        assert_eq!(result.slo_url, None);
    }

    #[test]
    fn test_parse_saml_metadata_no_certificate() {
        let xml = r#"
        <md:EntityDescriptor xmlns:md="urn:oasis:names:tc:SAML:2.0:metadata" entityID="https://idp.example.com">
            <md:IDPSSODescriptor>
                <md:SingleSignOnService Location="https://idp.example.com/sso"/>
            </md:IDPSSODescriptor>
        </md:EntityDescriptor>
        "#;

        let result = parse_saml_metadata(xml).unwrap();
        assert_eq!(result.certificate, "");
    }

    #[test]
    fn test_parse_saml_response_empty_input() {
        let result = parse_saml_response("");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), XmlParseError::MissingElement(_)));
    }

    #[test]
    fn test_parse_saml_metadata_empty_input() {
        let result = parse_saml_metadata("");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), XmlParseError::MissingElement(_)));
    }

    #[test]
    fn test_parse_saml_response_envelope_full() {
        let xml = r#"
        <samlp:Response xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol"
                        InResponseTo="id_123" Destination="https://sp.example.com/acs">
            <saml:Issuer xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">https://idp.example.com</saml:Issuer>
            <samlp:Status>
                <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
            </samlp:Status>
            <saml:Assertion xmlns:saml="urn:oasis:names:tc:SAML:2.0:assertion">
                <saml:Issuer>https://idp.example.com</saml:Issuer>
                <saml:Conditions NotBefore="2026-01-01T00:00:00Z" NotOnOrAfter="2026-01-01T00:05:00Z">
                    <saml:AudienceRestriction>
                        <saml:Audience>https://sp.example.com</saml:Audience>
                    </saml:AudienceRestriction>
                </saml:Conditions>
                <saml:Subject>
                    <saml:SubjectConfirmation>
                        <saml:SubjectConfirmationData Recipient="https://sp.example.com/acs"
                                                      NotOnOrAfter="2026-01-01T00:05:00Z"/>
                    </saml:SubjectConfirmation>
                </saml:Subject>
            </saml:Assertion>
        </samlp:Response>
        "#;

        let env = parse_saml_response_envelope(xml).unwrap();
        assert_eq!(env.in_response_to, Some("id_123".to_string()));
        assert_eq!(env.destination, Some("https://sp.example.com/acs".to_string()));
        assert_eq!(env.status_codes, vec!["urn:oasis:names:tc:SAML:2.0:status:Success".to_string()]);
        assert_eq!(env.audiences, vec!["https://sp.example.com".to_string()]);
        assert_eq!(env.subject_confirmation_recipients, vec!["https://sp.example.com/acs".to_string()]);
        // First issuer after <Response> is the response issuer, not the assertion issuer.
        assert_eq!(env.response_issuer, Some("https://idp.example.com".to_string()));
        assert!(env.not_before.contains(&"2026-01-01T00:00:00Z".to_string()));
        assert!(env.not_on_or_after.contains(&"2026-01-01T00:05:00Z".to_string()));
    }

    #[test]
    fn test_parse_saml_response_envelope_logout_response() {
        let xml = r#"
        <samlp:LogoutResponse xmlns:samlp="urn:oasis:names:tc:SAML:2.0:protocol" InResponseTo="id_logout">
            <samlp:Status>
                <samlp:StatusCode Value="urn:oasis:names:tc:SAML:2.0:status:Success"/>
            </samlp:Status>
        </samlp:LogoutResponse>
        "#;

        let env = parse_saml_response_envelope(xml).unwrap();
        assert_eq!(env.in_response_to, Some("id_logout".to_string()));
        assert_eq!(env.destination, None);
        assert_eq!(env.status_codes, vec!["urn:oasis:names:tc:SAML:2.0:status:Success".to_string()]);
    }

    #[test]
    fn test_parse_saml_response_envelope_attribute_order_independent() {
        // Attributes in a different order / with different whitespace must parse identically.
        let xml = r#"<samlp:Response
             Destination="https://sp.example.com/acs"
             InResponseTo="id_456"></samlp:Response>"#;

        let env = parse_saml_response_envelope(xml).unwrap();
        assert_eq!(env.in_response_to, Some("id_456".to_string()));
        assert_eq!(env.destination, Some("https://sp.example.com/acs".to_string()));
    }

    #[test]
    fn test_parse_saml_response_envelope_empty_input() {
        let env = parse_saml_response_envelope("").unwrap();
        assert_eq!(env, SamlResponseEnvelope::default());
    }
}
