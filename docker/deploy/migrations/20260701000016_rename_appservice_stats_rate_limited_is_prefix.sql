-- Rename application_service_statistics.rate_limited to is_rate_limited for v10 is_ prefix alignment
ALTER TABLE application_service_statistics RENAME COLUMN rate_limited TO is_rate_limited;
