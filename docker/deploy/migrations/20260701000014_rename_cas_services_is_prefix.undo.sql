-- Undo: rename is_require_secure and is_single_logout back
ALTER TABLE cas_services RENAME COLUMN is_require_secure TO require_secure;
ALTER TABLE cas_services RENAME COLUMN is_single_logout TO single_logout;
