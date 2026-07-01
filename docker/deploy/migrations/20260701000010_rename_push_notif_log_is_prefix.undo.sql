-- Undo: rename is_success back to success
ALTER TABLE push_notification_log RENAME COLUMN is_success TO success;
