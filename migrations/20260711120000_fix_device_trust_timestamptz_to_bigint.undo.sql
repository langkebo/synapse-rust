-- Rollback for 20260711120000_fix_device_trust_timestamptz_to_bigint.sql
-- Converts verified_at and trusted_at back to TIMESTAMPTZ.
-- Note: sub-millisecond precision is lost in the round-trip.

DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_name='device_trust_status' AND column_name='verified_at'
             AND data_type='bigint') THEN
    ALTER TABLE device_trust_status ALTER COLUMN verified_at TYPE TIMESTAMPTZ
      USING to_timestamp(verified_at / 1000.0);
  END IF;
END $$;

DO $$ BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.columns
             WHERE table_name='cross_signing_trust' AND column_name='trusted_at'
             AND data_type='bigint') THEN
    ALTER TABLE cross_signing_trust ALTER COLUMN trusted_at TYPE TIMESTAMPTZ
      USING to_timestamp(trusted_at / 1000.0);
  END IF;
END $$;
