-- =============================================================================
-- OIDC user mapping: bind external OIDC (issuer, subject) -> local Matrix user
-- =============================================================================
-- Without this binding, a local user @alice:server registered via password can
-- be impersonated by anyone who can make an OIDC IdP issue a token whose
-- mapped localpart resolves to "alice". The token endpoint must refuse to
-- issue Matrix credentials for an existing local user that has no recorded
-- (issuer, subject) ownership.

CREATE TABLE IF NOT EXISTS oidc_user_mapping (
    id BIGSERIAL,
    issuer TEXT NOT NULL,
    subject TEXT NOT NULL,
    user_id TEXT NOT NULL,
    first_seen_ts BIGINT NOT NULL,
    last_authenticated_ts BIGINT NOT NULL,
    authentication_count INTEGER NOT NULL DEFAULT 1,
    CONSTRAINT pk_oidc_user_mapping PRIMARY KEY (id),
    CONSTRAINT uq_oidc_user_mapping_issuer_subject UNIQUE (issuer, subject)
);

CREATE INDEX IF NOT EXISTS idx_oidc_user_mapping_user ON oidc_user_mapping(user_id);
