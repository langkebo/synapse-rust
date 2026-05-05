ALTER TABLE backup_keys
    DROP COLUMN IF EXISTS is_verified,
    DROP COLUMN IF EXISTS forwarded_count,
    DROP COLUMN IF EXISTS first_message_index;
