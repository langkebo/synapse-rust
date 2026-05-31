-- Undo module result persistence fields.

DROP INDEX IF EXISTS idx_third_party_results_event_checked;
DROP INDEX IF EXISTS idx_spam_results_sender_checked;

ALTER TABLE third_party_rule_results
    DROP COLUMN IF EXISTS checked_ts,
    DROP COLUMN IF EXISTS modified_content,
    DROP COLUMN IF EXISTS reason,
    DROP COLUMN IF EXISTS rule_name,
    DROP COLUMN IF EXISTS event_type,
    DROP COLUMN IF EXISTS sender;

ALTER TABLE spam_check_results
    DROP COLUMN IF EXISTS action_taken,
    DROP COLUMN IF EXISTS checked_ts,
    DROP COLUMN IF EXISTS checker_module,
    DROP COLUMN IF EXISTS reason,
    DROP COLUMN IF EXISTS score,
    DROP COLUMN IF EXISTS result,
    DROP COLUMN IF EXISTS content,
    DROP COLUMN IF EXISTS event_type,
    DROP COLUMN IF EXISTS sender;
