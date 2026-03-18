-- Fix push_rules constraint to include kind column
-- This fixes the ON CONFLICT issue in push rule operations

-- Drop the old constraint
ALTER TABLE push_rules DROP CONSTRAINT IF EXISTS uq_push_rules_user_scope_rule;

-- Add the new constraint with kind column
ALTER TABLE push_rules ADD CONSTRAINT uq_push_rules_user_scope_kind_rule 
    UNIQUE (user_id, scope, kind, rule_id);

-- Fix pushers pushkey_ts NOT NULL constraint
-- Allow pushkey_ts to be NULL for new pushers
ALTER TABLE pushers ALTER COLUMN pushkey_ts DROP NOT NULL;

-- Set default value for existing NULL pushkey_ts
UPDATE pushers SET pushkey_ts = EXTRACT(EPOCH FROM NOW()) * 1000 WHERE pushkey_ts IS NULL;
