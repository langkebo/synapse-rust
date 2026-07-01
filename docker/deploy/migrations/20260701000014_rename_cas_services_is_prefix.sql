-- Rename cas_services boolean columns for v10 is_ prefix alignment
ALTER TABLE cas_services RENAME COLUMN require_secure TO is_require_secure;
ALTER TABLE cas_services RENAME COLUMN single_logout TO is_single_logout;
