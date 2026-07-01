-- Undo: rename is_success back to success
ALTER TABLE registration_token_usage RENAME COLUMN is_success TO success;
