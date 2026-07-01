-- Undo: rename is_used back to used
ALTER TABLE email_verification_tokens RENAME COLUMN is_used TO used;
