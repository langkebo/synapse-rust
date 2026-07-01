-- Drop orphaned columns from spam_check_results that were replaced by new
-- columns in 20260529000002_module_result_persistence.sql. The Rust storage
-- layer no longer writes to these columns.

ALTER TABLE spam_check_results
    DROP COLUMN IF EXISTS user_id,
    DROP COLUMN IF EXISTS spam_score,
    DROP COLUMN IF EXISTS is_spam,
    DROP COLUMN IF EXISTS check_details;
