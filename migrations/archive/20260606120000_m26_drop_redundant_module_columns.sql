-- ============================================================================
-- m-26: 移除 spam_check_results / third_party_rule_results 冗余列
-- 日期: 2026-06-06
--
-- spam_check_results 移除:
--   - user_id (与 sender 重复，INSERT 中 user_id = sender)
--   - spam_score (与 score 重复，INSERT 中 spam_score = score)
--   - is_spam (仅写入，从未读取)
--   - check_details (仅写入，从未读取)
--
-- third_party_rule_results 移除:
--   - rule_type (与 rule_name 重复，INSERT 中 rule_type = rule_name)
--   - user_id (与 sender 重复，INSERT 中 user_id = sender)
--   - rule_details (仅写入，从未读取)
--
-- 同时移除引用已删除列的索引:
--   - idx_third_party_rule_type (引用 rule_type)
-- ============================================================================

-- spam_check_results: 移除冗余列
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'spam_check_results' AND column_name = 'user_id'
    ) THEN
        ALTER TABLE spam_check_results DROP COLUMN user_id;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'spam_check_results' AND column_name = 'spam_score'
    ) THEN
        ALTER TABLE spam_check_results DROP COLUMN spam_score;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'spam_check_results' AND column_name = 'is_spam'
    ) THEN
        ALTER TABLE spam_check_results DROP COLUMN is_spam;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'spam_check_results' AND column_name = 'check_details'
    ) THEN
        ALTER TABLE spam_check_results DROP COLUMN check_details;
    END IF;
END $$;

-- third_party_rule_results: 移除冗余列
DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'third_party_rule_results' AND column_name = 'rule_type'
    ) THEN
        ALTER TABLE third_party_rule_results DROP COLUMN rule_type;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'third_party_rule_results' AND column_name = 'user_id'
    ) THEN
        ALTER TABLE third_party_rule_results DROP COLUMN user_id;
    END IF;

    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'third_party_rule_results' AND column_name = 'rule_details'
    ) THEN
        ALTER TABLE third_party_rule_results DROP COLUMN rule_details;
    END IF;
END $$;

-- 移除引用已删除列的索引
DROP INDEX IF EXISTS idx_third_party_rule_type;
