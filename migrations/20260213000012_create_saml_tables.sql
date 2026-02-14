-- SAML Authentication Support Migration
-- Implements SAML 2.0 SSO authentication following Synapse's implementation
-- Reference: https://element-hq.github.io/synapse/latest/openid.html#saml

-- SAML Sessions Table
-- Stores active SAML authentication sessions
CREATE TABLE IF NOT EXISTS saml_sessions (
    id SERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    name_id VARCHAR(1024),
    issuer VARCHAR(512),
    session_index VARCHAR(512),
    attributes JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    expires_at TIMESTAMP WITH TIME ZONE NOT NULL,
    last_used_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    status VARCHAR(50) DEFAULT 'active',
    CONSTRAINT saml_sessions_session_id_unique UNIQUE(session_id)
);

-- SAML User Mapping Table
-- Maps SAML NameID to Matrix user IDs
CREATE TABLE IF NOT EXISTS saml_user_mapping (
    id SERIAL PRIMARY KEY,
    name_id VARCHAR(1024) NOT NULL,
    user_id VARCHAR(255) NOT NULL,
    issuer VARCHAR(512) NOT NULL,
    first_seen_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    last_authenticated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    authentication_count INTEGER DEFAULT 1,
    attributes JSONB DEFAULT '{}',
    CONSTRAINT saml_user_mapping_name_id_issuer_unique UNIQUE(name_id, issuer)
);

-- SAML Identity Providers Table
-- Stores registered SAML IdP configurations
CREATE TABLE IF NOT EXISTS saml_identity_providers (
    id SERIAL PRIMARY KEY,
    entity_id VARCHAR(512) NOT NULL UNIQUE,
    display_name VARCHAR(255),
    description TEXT,
    metadata_url VARCHAR(1024),
    metadata_xml TEXT,
    enabled BOOLEAN DEFAULT true,
    priority INTEGER DEFAULT 100,
    attribute_mapping JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    updated_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    last_metadata_refresh_at TIMESTAMP WITH TIME ZONE,
    metadata_valid_until TIMESTAMP WITH TIME ZONE,
    CONSTRAINT saml_idp_entity_id_unique UNIQUE(entity_id)
);

-- SAML Authentication Events Table
-- Logs SAML authentication events for audit and debugging
CREATE TABLE IF NOT EXISTS saml_auth_events (
    id SERIAL PRIMARY KEY,
    session_id VARCHAR(255),
    user_id VARCHAR(255),
    name_id VARCHAR(1024),
    issuer VARCHAR(512),
    event_type VARCHAR(50) NOT NULL,
    status VARCHAR(50) NOT NULL,
    error_message TEXT,
    ip_address VARCHAR(45),
    user_agent TEXT,
    request_id VARCHAR(255),
    attributes JSONB DEFAULT '{}',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- SAML Logout Requests Table
-- Tracks logout requests for SLO (Single Logout)
CREATE TABLE IF NOT EXISTS saml_logout_requests (
    id SERIAL PRIMARY KEY,
    request_id VARCHAR(255) NOT NULL UNIQUE,
    session_id VARCHAR(255),
    user_id VARCHAR(255),
    name_id VARCHAR(1024),
    issuer VARCHAR(512),
    reason VARCHAR(255),
    status VARCHAR(50) DEFAULT 'pending',
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    processed_at TIMESTAMP WITH TIME ZONE,
    CONSTRAINT saml_logout_requests_request_id_unique UNIQUE(request_id)
);

-- SAML NameID Format Table
-- Stores supported NameID formats for each IdP
CREATE TABLE IF NOT EXISTS saml_nameid_formats (
    id SERIAL PRIMARY KEY,
    idp_entity_id VARCHAR(512) NOT NULL,
    nameid_format VARCHAR(255) NOT NULL,
    is_default BOOLEAN DEFAULT false,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    CONSTRAINT saml_nameid_formats_idp_format_unique UNIQUE(idp_entity_id, nameid_format)
);

-- Create indexes for SAML tables
CREATE INDEX IF NOT EXISTS idx_saml_sessions_user_id ON saml_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_saml_sessions_expires_at ON saml_sessions(expires_at);
CREATE INDEX IF NOT EXISTS idx_saml_sessions_status ON saml_sessions(status);
CREATE INDEX IF NOT EXISTS idx_saml_sessions_created_at ON saml_sessions(created_at);

CREATE INDEX IF NOT EXISTS idx_saml_user_mapping_user_id ON saml_user_mapping(user_id);
CREATE INDEX IF NOT EXISTS idx_saml_user_mapping_name_id ON saml_user_mapping(name_id);
CREATE INDEX IF NOT EXISTS idx_saml_user_mapping_issuer ON saml_user_mapping(issuer);
CREATE INDEX IF NOT EXISTS idx_saml_user_mapping_last_authenticated ON saml_user_mapping(last_authenticated_at);

CREATE INDEX IF NOT EXISTS idx_saml_identity_providers_enabled ON saml_identity_providers(enabled);
CREATE INDEX IF NOT EXISTS idx_saml_identity_providers_priority ON saml_identity_providers(priority);

CREATE INDEX IF NOT EXISTS idx_saml_auth_events_user_id ON saml_auth_events(user_id);
CREATE INDEX IF NOT EXISTS idx_saml_auth_events_session_id ON saml_auth_events(session_id);
CREATE INDEX IF NOT EXISTS idx_saml_auth_events_event_type ON saml_auth_events(event_type);
CREATE INDEX IF NOT EXISTS idx_saml_auth_events_created_at ON saml_auth_events(created_at);
CREATE INDEX IF NOT EXISTS idx_saml_auth_events_status ON saml_auth_events(status);

CREATE INDEX IF NOT EXISTS idx_saml_logout_requests_session_id ON saml_logout_requests(session_id);
CREATE INDEX IF NOT EXISTS idx_saml_logout_requests_user_id ON saml_logout_requests(user_id);
CREATE INDEX IF NOT EXISTS idx_saml_logout_requests_status ON saml_logout_requests(status);
CREATE INDEX IF NOT EXISTS idx_saml_logout_requests_created_at ON saml_logout_requests(created_at);

-- Add comments to tables
COMMENT ON TABLE saml_sessions IS 'Active SAML authentication sessions';
COMMENT ON TABLE saml_user_mapping IS 'Mapping between SAML NameID and Matrix user IDs';
COMMENT ON TABLE saml_identity_providers IS 'Registered SAML Identity Provider configurations';
COMMENT ON TABLE saml_auth_events IS 'Audit log of SAML authentication events';
COMMENT ON TABLE saml_logout_requests IS 'Single Logout (SLO) request tracking';
COMMENT ON TABLE saml_nameid_formats IS 'Supported NameID formats for each IdP';

-- Insert default NameID formats
INSERT INTO saml_nameid_formats (idp_entity_id, nameid_format, is_default)
VALUES 
    ('*', 'urn:oasis:names:tc:SAML:2.0:nameid-format:persistent', true),
    ('*', 'urn:oasis:names:tc:SAML:2.0:nameid-format:transient', false),
    ('*', 'urn:oasis:names:tc:SAML:2.0:nameid-format:emailAddress', false),
    ('*', 'urn:oasis:names:tc:SAML:1.1:nameid-format:emailAddress', false),
    ('*', 'urn:oasis:names:tc:SAML:1.1:nameid-format:unspecified', false)
ON CONFLICT (idp_entity_id, nameid_format) DO NOTHING;
