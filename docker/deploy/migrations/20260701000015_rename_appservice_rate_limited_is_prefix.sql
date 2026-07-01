-- Rename application_services.rate_limited to is_rate_limited for v10 is_ prefix alignment
ALTER TABLE application_services RENAME COLUMN rate_limited TO is_rate_limited;
