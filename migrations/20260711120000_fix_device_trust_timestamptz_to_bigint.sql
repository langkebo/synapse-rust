-- Fix device_trust_status.verified_at and cross_signing_trust.trusted_at:
-- v10 baseline missed these two columns in the TIMESTAMPTZ→BIGINT unification.
-- Per project rules, *_at suffix columns must be BIGINT NULLABLE (ms timestamps).
-- The Rust storage layer already binds/reads Option<i64>.

DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_name='device_trust_status' AND column_name='verified_at'
             AND data_type='timestamp with time zone') THEN
    ALTER TABLE device_trust_status ALTER COLUMN verified_at TYPE BIGINT
      USING EXTRACT(EPOCH FROM verified_at AT TIME ZONE 'UTC') * 1000;
  END IF;
END $$;

DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_name='cross_signing_trust' AND column_name='trusted_at'
             AND data_type='timestamp with time zone') THEN
    ALTER TABLE cross_signing_trust ALTER COLUMN trusted_at TYPE BIGINT
      USING EXTRACT(EPOCH FROM trusted_at AT TIME ZONE 'UTC') * 1000;
  END IF;
END $$;
