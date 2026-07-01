-- Align retention policy boolean columns with v10 is_ prefix convention.
-- v7: expire_on_clients BOOLEAN  →  v10 / Rust: is_expire_on_clients BOOLEAN

ALTER TABLE room_retention_policies
    RENAME COLUMN expire_on_clients TO is_expire_on_clients;

ALTER TABLE server_retention_policy
    RENAME COLUMN expire_on_clients TO is_expire_on_clients;
