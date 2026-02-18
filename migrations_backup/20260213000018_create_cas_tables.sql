-- CAS Authentication Tables
-- Implements CAS (Central Authentication Service) single sign-on protocol

-- CAS service tickets table
CREATE TABLE IF NOT EXISTS cas_tickets (
    id SERIAL PRIMARY KEY,
    ticket_id VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    service_url TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    consumed_by TEXT,
    is_valid BOOLEAN NOT NULL DEFAULT TRUE,
    
    CONSTRAINT fk_cas_ticket_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX idx_cas_tickets_ticket_id ON cas_tickets(ticket_id);
CREATE INDEX idx_cas_tickets_user_id ON cas_tickets(user_id);
CREATE INDEX idx_cas_tickets_expires_at ON cas_tickets(expires_at);
CREATE INDEX idx_cas_tickets_service_url ON cas_tickets(service_url);

-- CAS proxy tickets table
CREATE TABLE IF NOT EXISTS cas_proxy_tickets (
    id SERIAL PRIMARY KEY,
    proxy_ticket_id VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    service_url TEXT NOT NULL,
    pgt_url TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    consumed_at TIMESTAMPTZ,
    is_valid BOOLEAN NOT NULL DEFAULT TRUE,
    
    CONSTRAINT fk_cas_proxy_ticket_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX idx_cas_proxy_tickets_ticket_id ON cas_proxy_tickets(proxy_ticket_id);
CREATE INDEX idx_cas_proxy_tickets_user_id ON cas_proxy_tickets(user_id);

-- CAS proxy granting tickets table
CREATE TABLE IF NOT EXISTS cas_proxy_granting_tickets (
    id SERIAL PRIMARY KEY,
    pgt_id VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    service_url TEXT NOT NULL,
    iou VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    is_valid BOOLEAN NOT NULL DEFAULT TRUE,
    
    CONSTRAINT fk_cas_pgt_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX idx_cas_pgt_pgt_id ON cas_proxy_granting_tickets(pgt_id);
CREATE INDEX idx_cas_pgt_user_id ON cas_proxy_granting_tickets(user_id);
CREATE INDEX idx_cas_pgt_iou ON cas_proxy_granting_tickets(iou);

-- CAS service registry table
CREATE TABLE IF NOT EXISTS cas_services (
    id SERIAL PRIMARY KEY,
    service_id VARCHAR(255) NOT NULL UNIQUE,
    name VARCHAR(255) NOT NULL,
    description TEXT,
    service_url_pattern TEXT NOT NULL,
    allowed_attributes JSONB DEFAULT '[]',
    allowed_proxy_callbacks JSONB DEFAULT '[]',
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    require_secure BOOLEAN NOT NULL DEFAULT TRUE,
    single_logout BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_cas_services_service_id ON cas_services(service_id);
CREATE INDEX idx_cas_services_is_enabled ON cas_services(is_enabled);

-- CAS single logout sessions table
CREATE TABLE IF NOT EXISTS cas_slo_sessions (
    id SERIAL PRIMARY KEY,
    session_id VARCHAR(255) NOT NULL UNIQUE,
    user_id VARCHAR(255) NOT NULL,
    service_url TEXT NOT NULL,
    ticket_id VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    logout_sent_at TIMESTAMPTZ,
    
    CONSTRAINT fk_cas_slo_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE INDEX idx_cas_slo_session_id ON cas_slo_sessions(session_id);
CREATE INDEX idx_cas_slo_user_id ON cas_slo_sessions(user_id);

-- CAS user attribute mappings table
CREATE TABLE IF NOT EXISTS cas_user_attributes (
    id SERIAL PRIMARY KEY,
    user_id VARCHAR(255) NOT NULL,
    attribute_name VARCHAR(255) NOT NULL,
    attribute_value TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    
    CONSTRAINT fk_cas_attr_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT uq_cas_user_attribute UNIQUE (user_id, attribute_name)
);

CREATE INDEX idx_cas_attr_user_id ON cas_user_attributes(user_id);
CREATE INDEX idx_cas_attr_name ON cas_user_attributes(attribute_name);

-- Insert comment
COMMENT ON TABLE cas_tickets IS 'CAS service tickets for authentication';
COMMENT ON TABLE cas_proxy_tickets IS 'CAS proxy tickets for proxy authentication';
COMMENT ON TABLE cas_proxy_granting_tickets IS 'CAS proxy granting tickets for proxy callback';
COMMENT ON TABLE cas_services IS 'Registered CAS services';
COMMENT ON TABLE cas_slo_sessions IS 'CAS single logout sessions';
COMMENT ON TABLE cas_user_attributes IS 'CAS user attribute mappings';
