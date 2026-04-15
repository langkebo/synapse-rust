ALTER TABLE application_services
ADD COLUMN IF NOT EXISTS api_key TEXT;

ALTER TABLE application_services
ADD COLUMN IF NOT EXISTS config JSONB NOT NULL DEFAULT '{}'::jsonb;
