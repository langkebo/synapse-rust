-- scripts/create-default-admin.sql
-- ============================================================================
-- Default admin account bootstrap (post-deploy)
--
-- PURPOSE:
--   Provides an opt-in seed account for fresh database initialization.
--   This file was previously embedded in the baseline schema, which is
--   insecure — any server that imports the schema gets a known admin
--   with a guessable (test) password hash.
--
-- USAGE:
--   Run this ONLY on a fresh database that you are setting up for the first time.
--   Run it AFTER `cargo sqlx migrate run` completes.
--
--   psql "$DATABASE_URL" -f scripts/create-default-admin.sql
--
-- PRODUCTION WARNING:
--   Delete this account (or change its password immediately) before
--   exposing the server to untrusted users. The password_hash below is
--   a test placeholder — it matches the plaintext "TestSaltForAdmin".
--   DO NOT use this account with default credentials in production.
--
-- CONTEXT:
--   DB-04 Part A: Removed hardcoded admin INSERT from 00000000_unified_schema_v10.sql.
--   See: artifacts/数据库架构诊断报告-2026-08-30.md §P0-6
-- ============================================================================

INSERT INTO users (user_id, username, password_hash, is_admin, is_guest, created_ts, displayname)
SELECT
    '@admin:localhost',
    'admin',
    '$argon2id$v=19$m=65536,t=3,p=1$VGVzdFNhbHRGb3JBZG1pbg$K7G8H5J3M2N9P4Q6R8S0T2U4V6W8X0Y2Z4A6B8C0D2E4F6G8H0J2K4L6M8N0P2Q4',
    TRUE,
    FALSE,
    (EXTRACT(EPOCH FROM NOW()) * 1000)::BIGINT,
    'Administrator'
WHERE NOT EXISTS (
    SELECT 1 FROM users WHERE user_id = '@admin:localhost'
);
