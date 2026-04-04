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

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_feature_flags_scope_status
ON feature_flags(target_scope, status, updated_ts DESC);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_feature_flags_expires_at
ON feature_flags(expires_at)
WHERE expires_at IS NOT NULL;

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_feature_flag_targets_lookup
ON feature_flag_targets(flag_key, subject_type, subject_id);
