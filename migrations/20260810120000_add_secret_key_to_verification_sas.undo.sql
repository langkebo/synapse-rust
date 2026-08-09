-- Rollback: remove secret_key column from verification_sas.
ALTER TABLE verification_sas DROP COLUMN IF EXISTS secret_key;
