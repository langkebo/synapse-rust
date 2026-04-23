-- Migration: Drop redundant tables (Phase B)
-- password_policy: PasswordPolicyService was never instantiated; policy is config-driven
-- key_rotation_history: Redundant with key_rotation_log; routes migrated to key_rotation_log
-- presence_routes: Module system over-engineering; presence routing is built-in
-- password_auth_providers: Module system over-engineering; auth is handled by AuthService/OIDC

DROP TABLE IF EXISTS password_policy CASCADE;
DROP TABLE IF EXISTS key_rotation_history CASCADE;
DROP TABLE IF EXISTS presence_routes CASCADE;
DROP TABLE IF EXISTS password_auth_providers CASCADE;
