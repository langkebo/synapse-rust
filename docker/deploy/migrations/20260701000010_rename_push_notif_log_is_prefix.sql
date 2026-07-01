-- Rename push_notification_log.success to is_success for v10 is_ prefix alignment
ALTER TABLE push_notification_log RENAME COLUMN success TO is_success;
