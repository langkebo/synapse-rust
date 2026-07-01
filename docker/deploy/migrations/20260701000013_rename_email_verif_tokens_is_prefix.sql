-- Rename email_verification_tokens.used to is_used for v10 is_ prefix alignment
ALTER TABLE email_verification_tokens RENAME COLUMN used TO is_used;
