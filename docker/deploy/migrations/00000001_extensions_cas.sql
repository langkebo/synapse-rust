-- =============================================================================
-- Extension: CAS SSO (feature: cas-sso)
-- Tables: cas_tickets, cas_proxy_tickets, cas_proxy_granting_tickets,
--         cas_services, cas_user_attributes, cas_slo_sessions
-- =============================================================================

CREATE TABLE IF NOT EXISTS cas_tickets (
    id BIGSERIAL,
    ticket_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    service_url TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    consumed_ts BIGINT,
    consumed_by TEXT,
    is_valid BOOLEAN NOT NULL DEFAULT TRUE,
    CONSTRAINT pk_cas_tickets PRIMARY KEY (id),
    CONSTRAINT uq_cas_tickets_ticket UNIQUE (ticket_id)
);

CREATE TABLE IF NOT EXISTS cas_proxy_tickets (
    id BIGSERIAL,
    proxy_ticket_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    service_url TEXT NOT NULL,
    pgt_url TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    consumed_ts BIGINT,
    is_valid BOOLEAN NOT NULL DEFAULT TRUE,
    CONSTRAINT pk_cas_proxy_tickets PRIMARY KEY (id),
    CONSTRAINT uq_cas_proxy_tickets_ticket UNIQUE (proxy_ticket_id)
);

CREATE TABLE IF NOT EXISTS cas_proxy_granting_tickets (
    id BIGSERIAL,
    pgt_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    service_url TEXT NOT NULL,
    iou TEXT,
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    is_valid BOOLEAN NOT NULL DEFAULT TRUE,
    CONSTRAINT pk_cas_proxy_granting_tickets PRIMARY KEY (id),
    CONSTRAINT uq_cas_proxy_granting_tickets_ticket UNIQUE (pgt_id)
);

CREATE TABLE IF NOT EXISTS cas_services (
    id BIGSERIAL,
    service_id TEXT NOT NULL,
    name TEXT NOT NULL,
    description TEXT,
    service_url_pattern TEXT NOT NULL,
    allowed_attributes JSONB NOT NULL DEFAULT '[]',
    allowed_proxy_callbacks JSONB NOT NULL DEFAULT '[]',
    is_enabled BOOLEAN NOT NULL DEFAULT TRUE,
    require_secure BOOLEAN NOT NULL DEFAULT FALSE,
    single_logout BOOLEAN NOT NULL DEFAULT FALSE,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_cas_services PRIMARY KEY (id),
    CONSTRAINT uq_cas_services_id UNIQUE (service_id)
);

CREATE TABLE IF NOT EXISTS cas_user_attributes (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    attribute_name TEXT NOT NULL,
    attribute_value TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_cas_user_attributes PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS cas_slo_sessions (
    id BIGSERIAL,
    session_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    service_url TEXT NOT NULL,
    ticket_id TEXT,
    created_ts BIGINT NOT NULL,
    logout_sent_ts BIGINT,
    CONSTRAINT pk_cas_slo_sessions PRIMARY KEY (id),
    CONSTRAINT uq_cas_slo_sessions_session UNIQUE (session_id)
);
