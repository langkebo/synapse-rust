ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_rule;

ALTER TABLE push_rules
    DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_kind_rule;

ALTER TABLE push_rules
    ADD CONSTRAINT uq_push_rules_user_scope_kind_rule UNIQUE (user_id, scope, kind, rule_id);

