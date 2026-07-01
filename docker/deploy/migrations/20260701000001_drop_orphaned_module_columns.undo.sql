-- Undo: restore the orphaned columns.
-- Data was already migrated to the new columns; restored columns will be NULL/default.

ALTER TABLE spam_check_results
    ADD COLUMN IF NOT EXISTS user_id TEXT,
    ADD COLUMN IF NOT EXISTS spam_score REAL DEFAULT 0,
    ADD COLUMN IF NOT EXISTS is_spam BOOLEAN DEFAULT FALSE,
    ADD COLUMN IF NOT EXISTS check_details JSONB DEFAULT '{}';
