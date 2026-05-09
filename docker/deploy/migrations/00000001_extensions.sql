-- ============================================================================
-- Unified Extensions Schema
-- Version: 00000001
-- Updated: 2026-05-07 (consolidated from 5 extension files)
--
-- Merged source files:
--   - 00000001_extensions_cas.sql
--   - 00000001_extensions_saml.sql
--   - 00000001_extensions_friends.sql
--   - 00000001_extensions_voice.sql
--   - 00000001_extensions_privacy.sql
--
-- All feature-gated tables use IF NOT EXISTS guards for idempotent execution.
-- Feature filtering is managed via extension_map.conf and container-migrate.sh.
-- ============================================================================

--no-transaction

-- ============================================================================
-- Extension: CAS SSO (feature: cas-sso)
-- ============================================================================

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

-- ============================================================================
-- Extension: SAML SSO (feature: saml-sso)
-- ============================================================================

CREATE TABLE IF NOT EXISTS saml_sessions (
    id BIGSERIAL,
    session_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    name_id TEXT,
    issuer TEXT,
    session_index TEXT,
    attributes JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    last_used_ts BIGINT NOT NULL,
    status TEXT DEFAULT 'active',
    CONSTRAINT pk_saml_sessions PRIMARY KEY (id),
    CONSTRAINT uq_saml_sessions_session UNIQUE (session_id)
);
CREATE INDEX IF NOT EXISTS idx_saml_sessions_user ON saml_sessions(user_id);

CREATE TABLE IF NOT EXISTS saml_user_mapping (
    id BIGSERIAL,
    name_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    issuer TEXT NOT NULL,
    first_seen_ts BIGINT NOT NULL,
    last_authenticated_ts BIGINT NOT NULL,
    authentication_count INTEGER DEFAULT 1,
    attributes JSONB DEFAULT '{}',
    CONSTRAINT pk_saml_user_mapping PRIMARY KEY (id),
    CONSTRAINT uq_saml_user_mapping_name_issuer UNIQUE (name_id, issuer)
);

CREATE TABLE IF NOT EXISTS saml_identity_providers (
    id BIGSERIAL,
    entity_id TEXT NOT NULL,
    display_name TEXT,
    description TEXT,
    metadata_url TEXT,
    metadata_xml TEXT,
    is_enabled BOOLEAN DEFAULT TRUE,
    priority INTEGER DEFAULT 100,
    attribute_mapping JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    last_metadata_refresh_at BIGINT,
    metadata_valid_until_at BIGINT,
    CONSTRAINT pk_saml_identity_providers PRIMARY KEY (id),
    CONSTRAINT uq_saml_identity_providers_entity UNIQUE (entity_id)
);

CREATE TABLE IF NOT EXISTS saml_auth_events (
    id BIGSERIAL,
    session_id TEXT,
    user_id TEXT,
    name_id TEXT,
    issuer TEXT,
    event_type TEXT NOT NULL,
    status TEXT NOT NULL,
    error_message TEXT,
    ip_address TEXT,
    user_agent TEXT,
    request_id TEXT,
    attributes JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_saml_auth_events PRIMARY KEY (id)
);

CREATE TABLE IF NOT EXISTS saml_logout_requests (
    id BIGSERIAL,
    request_id TEXT NOT NULL,
    session_id TEXT,
    user_id TEXT,
    name_id TEXT,
    issuer TEXT,
    reason TEXT,
    status TEXT DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    processed_at BIGINT,
    CONSTRAINT pk_saml_logout_requests PRIMARY KEY (id),
    CONSTRAINT uq_saml_logout_requests_request UNIQUE (request_id)
);

-- ============================================================================
-- Extension: Friends System (feature: friends)
-- ============================================================================

CREATE TABLE IF NOT EXISTS friends (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    friend_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_friends PRIMARY KEY (id),
    CONSTRAINT uq_friends_user_friend UNIQUE (user_id, friend_id),
    CONSTRAINT fk_friends_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_friends_friend FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS friend_requests (
    id BIGSERIAL,
    sender_id TEXT NOT NULL,
    receiver_id TEXT NOT NULL,
    message TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_friend_requests PRIMARY KEY (id),
    CONSTRAINT uq_friend_requests_sender_receiver UNIQUE (sender_id, receiver_id),
    CONSTRAINT fk_friend_requests_sender FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_friend_requests_receiver FOREIGN KEY (receiver_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS friend_categories (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#000000',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_friend_categories PRIMARY KEY (id),
    CONSTRAINT fk_friend_categories_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);

-- ============================================================================
-- Extension: Voice Messages (feature: voice-extended)
-- ============================================================================

CREATE TABLE IF NOT EXISTS voice_messages (
    id BIGSERIAL,
    event_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    room_id TEXT,
    media_id TEXT,
    duration_ms INT NOT NULL,
    file_size BIGINT,
    file_path TEXT,
    mime_type TEXT DEFAULT 'audio/ogg',
    waveform JSONB,
    transcription TEXT,
    transcription_language TEXT,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_voice_messages PRIMARY KEY (id),
    CONSTRAINT uq_voice_messages_event UNIQUE (event_id)
);
CREATE INDEX IF NOT EXISTS idx_voice_messages_user ON voice_messages(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_messages_room ON voice_messages(room_id);

CREATE TABLE IF NOT EXISTS voice_usage_stats (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    date DATE NOT NULL,
    messages_sent INTEGER DEFAULT 0,
    total_duration_ms BIGINT DEFAULT 0,
    total_size_bytes BIGINT DEFAULT 0,
    CONSTRAINT pk_voice_usage_stats PRIMARY KEY (id),
    CONSTRAINT uq_voice_usage_stats_user_date UNIQUE (user_id, date)
);
CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_user ON voice_usage_stats(user_id);
CREATE INDEX IF NOT EXISTS idx_voice_usage_stats_date ON voice_usage_stats(date);

-- ============================================================================
-- Extension: Privacy Settings (feature: privacy-ext)
-- ============================================================================

CREATE TABLE IF NOT EXISTS user_privacy_settings (
    id BIGSERIAL PRIMARY KEY,
    user_id TEXT NOT NULL UNIQUE,
    profile_visibility TEXT NOT NULL DEFAULT 'public',
    avatar_visibility TEXT NOT NULL DEFAULT 'public',
    displayname_visibility TEXT NOT NULL DEFAULT 'public',
    presence_visibility TEXT NOT NULL DEFAULT 'contacts',
    room_membership_visibility TEXT NOT NULL DEFAULT 'contacts',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT user_privacy_settings_user_id_fkey
        FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_user_privacy_settings_user ON user_privacy_settings(user_id);
