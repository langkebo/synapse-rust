--no-transaction
-- ============================================================================
-- Consolidated Minor Features Migration
-- Created: 2026-04-04
-- Description: Merges 3 small feature migrations into a single file
-- Original migrations: 20260328000002, 20260330000010, 20260330000011
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- Part 1: Federation Cache (原 20260328000002)
-- ============================================================================

CREATE TABLE IF NOT EXISTS federation_cache (
    id BIGSERIAL PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    value TEXT,
    expiry_ts BIGINT,
    created_ts BIGINT NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_federation_cache_key ON federation_cache(key);
CREATE INDEX IF NOT EXISTS idx_federation_cache_expiry ON federation_cache(expiry_ts);

-- ============================================================================
-- Part 2: Audit Events (原 20260330000010)
-- ============================================================================

-- Note: audit_events table already defined in unified baseline schema
-- This section intentionally empty as duplicate table definition was removed

-- ============================================================================
-- Part 3: Feature Flags (原 20260330000011)
-- ============================================================================

CREATE TABLE IF NOT EXISTS feature_flags (
    flag_key TEXT PRIMARY KEY,
    target_scope TEXT NOT NULL,
    rollout_percent INTEGER NOT NULL DEFAULT 0,
    expires_at BIGINT,
    reason TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    created_by TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT NOT NULL
);

CREATE TABLE IF NOT EXISTS feature_flag_targets (
    id BIGSERIAL PRIMARY KEY,
    flag_key TEXT NOT NULL,
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT fk_feature_flag_targets_flag_key
        FOREIGN KEY (flag_key) REFERENCES feature_flags(flag_key) ON DELETE CASCADE,
    CONSTRAINT uq_feature_flag_targets UNIQUE (flag_key, subject_type, subject_id)
);

CREATE INDEX IF NOT EXISTS idx_feature_flags_scope_status
ON feature_flags(target_scope, status, updated_ts DESC);

CREATE INDEX IF NOT EXISTS idx_feature_flags_expires_at
ON feature_flags(expires_at)
WHERE expires_at IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_feature_flag_targets_lookup
ON feature_flag_targets(flag_key, subject_type, subject_id);

-- ============================================================================
-- Migration Record
-- ============================================================================

INSERT INTO schema_migrations (version, name, success, description, applied_ts)
VALUES ('20260404000002', 'consolidated_minor_features', TRUE, 'Consolidated minor features (replaces 20260328000002, 20260330000010, 20260330000011)', EXTRACT(EPOCH FROM NOW())::BIGINT * 1000)
ON CONFLICT (version) DO NOTHING;
