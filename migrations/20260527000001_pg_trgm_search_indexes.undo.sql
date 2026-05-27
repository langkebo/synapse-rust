-- Drop pg_trgm indexes
DROP INDEX IF EXISTS idx_rooms_canonical_alias_trgm;
DROP INDEX IF EXISTS idx_rooms_name_trgm;
DROP INDEX IF EXISTS idx_users_displayname_trgm;
DROP INDEX IF EXISTS idx_users_username_trgm;
DROP INDEX IF EXISTS idx_users_user_id_trgm;
DROP INDEX IF EXISTS idx_users_name_trgm;

-- Drop the extension (may fail if other objects depend on it)
DROP EXTENSION IF EXISTS pg_trgm;
