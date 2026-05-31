-- Persist module spam-check and third-party-rule result details used by the storage layer.

CREATE TABLE IF NOT EXISTS spam_check_results (
    id BIGSERIAL PRIMARY KEY,
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    spam_score REAL DEFAULT 0,
    is_spam BOOLEAN DEFAULT FALSE,
    check_details JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    sender TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content JSONB,
    result TEXT NOT NULL,
    score INTEGER NOT NULL DEFAULT 0,
    reason TEXT,
    checker_module TEXT NOT NULL,
    checked_ts BIGINT NOT NULL,
    action_taken TEXT
);

CREATE TABLE IF NOT EXISTS third_party_rule_results (
    id BIGSERIAL PRIMARY KEY,
    rule_type TEXT NOT NULL,
    event_id TEXT,
    room_id TEXT,
    user_id TEXT,
    is_allowed BOOLEAN DEFAULT TRUE,
    rule_details JSONB DEFAULT '{}',
    created_ts BIGINT NOT NULL,
    sender TEXT NOT NULL,
    event_type TEXT NOT NULL,
    rule_name TEXT NOT NULL,
    reason TEXT,
    modified_content JSONB,
    checked_ts BIGINT NOT NULL
);

ALTER TABLE spam_check_results
    ADD COLUMN IF NOT EXISTS sender TEXT,
    ADD COLUMN IF NOT EXISTS event_type TEXT,
    ADD COLUMN IF NOT EXISTS content JSONB,
    ADD COLUMN IF NOT EXISTS result TEXT,
    ADD COLUMN IF NOT EXISTS score INTEGER NOT NULL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS reason TEXT,
    ADD COLUMN IF NOT EXISTS checker_module TEXT,
    ADD COLUMN IF NOT EXISTS checked_ts BIGINT,
    ADD COLUMN IF NOT EXISTS action_taken TEXT;

UPDATE spam_check_results
SET
    sender = COALESCE(sender, user_id),
    checked_ts = COALESCE(checked_ts, created_ts),
    result = COALESCE(result, CASE WHEN is_spam THEN 'spam' ELSE 'ok' END),
    score = COALESCE(score, spam_score::INTEGER),
    event_type = COALESCE(event_type, check_details->>'event_type', 'm.room.message'),
    content = COALESCE(content, check_details->'content'),
    reason = COALESCE(reason, check_details->>'reason'),
    checker_module = COALESCE(checker_module, check_details->>'checker_module', 'unknown'),
    action_taken = COALESCE(action_taken, check_details->>'action_taken');

ALTER TABLE spam_check_results
    ALTER COLUMN sender SET NOT NULL,
    ALTER COLUMN event_type SET NOT NULL,
    ALTER COLUMN result SET NOT NULL,
    ALTER COLUMN checker_module SET NOT NULL,
    ALTER COLUMN checked_ts SET NOT NULL;

ALTER TABLE third_party_rule_results
    ADD COLUMN IF NOT EXISTS sender TEXT,
    ADD COLUMN IF NOT EXISTS event_type TEXT,
    ADD COLUMN IF NOT EXISTS rule_name TEXT,
    ADD COLUMN IF NOT EXISTS reason TEXT,
    ADD COLUMN IF NOT EXISTS modified_content JSONB,
    ADD COLUMN IF NOT EXISTS checked_ts BIGINT;

UPDATE third_party_rule_results
SET
    sender = COALESCE(sender, user_id),
    event_type = COALESCE(event_type, rule_details->>'event_type', 'm.room.message'),
    rule_name = COALESCE(rule_name, rule_type),
    reason = COALESCE(reason, rule_details->>'reason'),
    modified_content = COALESCE(modified_content, rule_details->'modified_content'),
    checked_ts = COALESCE(checked_ts, created_ts);

ALTER TABLE third_party_rule_results
    ALTER COLUMN sender SET NOT NULL,
    ALTER COLUMN event_type SET NOT NULL,
    ALTER COLUMN rule_name SET NOT NULL,
    ALTER COLUMN checked_ts SET NOT NULL;

CREATE INDEX IF NOT EXISTS idx_spam_results_sender_checked
    ON spam_check_results(sender, checked_ts DESC);

CREATE INDEX IF NOT EXISTS idx_spam_results_event
    ON spam_check_results(event_id);

CREATE INDEX IF NOT EXISTS idx_spam_results_room
    ON spam_check_results(room_id);

CREATE INDEX IF NOT EXISTS idx_third_party_results_event_checked
    ON third_party_rule_results(event_id, checked_ts DESC);

CREATE INDEX IF NOT EXISTS idx_third_party_rule_type
    ON third_party_rule_results(rule_type);
