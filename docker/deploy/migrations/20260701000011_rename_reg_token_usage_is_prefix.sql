-- Rename registration_token_usage.success to is_success for v10 is_ prefix alignment
ALTER TABLE registration_token_usage RENAME COLUMN success TO is_success;
