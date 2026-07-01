-- Drop orphaned columns from third_party_rule_results that were replaced by new
-- columns in 20260529000002_module_result_persistence.sql. The Rust storage
-- layer no longer writes to these columns.

ALTER TABLE third_party_rule_results
    DROP COLUMN IF EXISTS rule_type,
    DROP COLUMN IF EXISTS user_id,
    DROP COLUMN IF EXISTS rule_details;
