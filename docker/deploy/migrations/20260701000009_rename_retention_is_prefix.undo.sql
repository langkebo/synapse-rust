ALTER TABLE room_retention_policies
    RENAME COLUMN is_expire_on_clients TO expire_on_clients;

ALTER TABLE server_retention_policy
    RENAME COLUMN is_expire_on_clients TO expire_on_clients;
