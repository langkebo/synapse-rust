-- Persistent runtime overrides for SamlConfig fields.
-- Admin-edited values from PUT /_synapse/admin/v1/saml/config are stored
-- here keyed by field name (e.g. "metadata_url", "session_lifetime") so
-- they survive process restarts. Only fields listed in
-- SamlService::MUTABLE_CONFIG_FIELDS may be written; enforcement is in
-- the service layer, not in the schema.
CREATE TABLE IF NOT EXISTS saml_config_overrides (
    config_key TEXT NOT NULL,
    config_value JSONB NOT NULL,
    updated_ts BIGINT NOT NULL,
    CONSTRAINT pk_saml_config_overrides PRIMARY KEY (config_key)
);

CREATE INDEX IF NOT EXISTS idx_saml_config_overrides_updated_ts
    ON saml_config_overrides (updated_ts DESC);
