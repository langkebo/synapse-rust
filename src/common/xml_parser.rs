use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashMap;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum XmlParseError {
    #[error("XML parse error: {0}")]
    Parse(String),
    #[error("Missing required element: {0}")]
    MissingElement(String),
    #[error("Invalid XML structure: {0}")]
    InvalidStructure(String),
}

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
                let text = e.unescape().unwrap_or_default().to_string();

                if in_name_id {
                    name_id = text;
                    in_name_id = false;
                } else if in_issuer {
                    issuer = text;
                    in_issuer = false;
                } else if in_attr_value && !current_attr_name.is_empty() {
                    attributes
                        .entry(current_attr_name.clone())
                        .or_default()
                        .push(text);
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
                return Err(XmlParseError::Parse(format!("Error parsing XML: {:?}", e)));
            }
            _ => {}
        }
        buf.clear();
    }

    if name_id.is_empty() {
        return Err(XmlParseError::MissingElement("NameID".to_string()));
    }

    Ok(SamlAssertionData {
        name_id,
        issuer,
        attributes,
        session_index,
    })
}

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
                    certificate = e.unescape().unwrap_or_default().to_string();
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
                return Err(XmlParseError::Parse(format!("Error parsing XML: {:?}", e)));
            }
            _ => {}
        }
        buf.clear();
    }

    if entity_id.is_empty() || sso_url.is_empty() {
        return Err(XmlParseError::MissingElement("entityID or SSO URL".to_string()));
    }

    Ok(SamlMetadataParsed {
        entity_id,
        sso_url,
        slo_url,
        certificate,
    })
}

#[derive(Debug, Clone)]
pub struct SamlAssertionData {
    pub name_id: String,
    pub issuer: String,
    pub attributes: HashMap<String, Vec<String>>,
    pub session_index: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SamlMetadataParsed {
    pub entity_id: String,
    pub sso_url: String,
    pub slo_url: Option<String>,
    pub certificate: String,
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
}
