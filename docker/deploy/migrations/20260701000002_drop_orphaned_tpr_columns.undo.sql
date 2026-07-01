-- Undo: restore the orphaned columns.

ALTER TABLE third_party_rule_results
    ADD COLUMN IF NOT EXISTS rule_type TEXT,
    ADD COLUMN IF NOT EXISTS user_id TEXT,
    ADD COLUMN IF NOT EXISTS rule_details JSONB DEFAULT '{}';
