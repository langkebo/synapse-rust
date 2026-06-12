#[cfg(feature = "saml-sso")]
pub use synapse_services::saml_service::*;

// Note: Tests referencing private methods (generate_request_id, generate_session_id,
// parse_metadata_xml, parse_saml_assertion, validate_response) and private fields
// (config, server_name) are in synapse-services/src/saml_service.rs where they have access.
