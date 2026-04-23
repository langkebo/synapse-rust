ALTER TABLE application_services
DROP COLUMN IF EXISTS config;

ALTER TABLE application_services
DROP COLUMN IF EXISTS api_key;
