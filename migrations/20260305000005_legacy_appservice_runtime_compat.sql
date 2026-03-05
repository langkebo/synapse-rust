-- Migration: Legacy appservice runtime compatibility for existing deployed binaries
-- Version: 20260305000005
-- Date: 2026-03-05

ALTER TABLE application_services
ALTER COLUMN protocols SET DEFAULT '[]'::jsonb;

UPDATE application_services
SET protocols = COALESCE(protocols, '[]'::jsonb)
WHERE protocols IS NULL;

CREATE OR REPLACE FUNCTION text_array_to_jsonb(value TEXT[])
RETURNS JSONB
LANGUAGE SQL
IMMUTABLE
AS $$
    SELECT to_jsonb(value)
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_cast
        WHERE castsource = 'text[]'::regtype
          AND casttarget = 'jsonb'::regtype
    ) THEN
        CREATE CAST (TEXT[] AS JSONB) WITH FUNCTION text_array_to_jsonb(TEXT[]) AS IMPLICIT;
    END IF;
END $$;

CREATE OR REPLACE FUNCTION jsonb_to_text_array(value JSONB)
RETURNS TEXT[]
LANGUAGE SQL
IMMUTABLE
AS $$
    SELECT COALESCE(ARRAY(SELECT jsonb_array_elements_text(value)), '{}'::TEXT[])
$$;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_cast
        WHERE castsource = 'jsonb'::regtype
          AND casttarget = 'text[]'::regtype
    ) THEN
        CREATE CAST (JSONB AS TEXT[]) WITH FUNCTION jsonb_to_text_array(JSONB) AS IMPLICIT;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_state'
          AND column_name = 'appservice_id'
    ) THEN
        ALTER TABLE application_service_state
        ALTER COLUMN appservice_id DROP NOT NULL;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_state'
          AND column_name = 'state'
    ) THEN
        ALTER TABLE application_service_state
        ALTER COLUMN state SET DEFAULT '';
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_users'
          AND column_name = 'appservice_id'
    ) THEN
        ALTER TABLE application_service_users
        ALTER COLUMN appservice_id DROP NOT NULL;
    END IF;
END $$;

DO $$
BEGIN
    IF EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'application_service_events'
          AND column_name = 'appservice_id'
    ) THEN
        ALTER TABLE application_service_events
        ALTER COLUMN appservice_id DROP NOT NULL;
    END IF;
END $$;
