-- OPT-010 (audit 06 §5): the to-device dedup fix relies on an atomic
-- INSERT ... ON CONFLICT (sender_user_id, sender_device_id, message_id).
-- The v10 baseline only has UNIQUE (transaction_id, sender_user_id, sender_device_id),
-- so add the message_id-based unique index here.
-- PostgreSQL treats NULLs as distinct in UNIQUE indexes, so multiple NULL
-- message_id rows for the same (sender, device) will not conflict — this is
-- the desired behaviour since NULL transactions are not deduplicated.

-- Guard: collapse any pre-existing duplicate (sender, device, message_id) rows,
-- keeping the lowest id, so the unique index can be created.
DELETE FROM to_device_transactions a
USING to_device_transactions b
WHERE a.message_id IS NOT NULL
  AND a.message_id = b.message_id
  AND a.sender_user_id = b.sender_user_id
  AND a.sender_device_id = b.sender_device_id
  AND a.id > b.id;

CREATE UNIQUE INDEX IF NOT EXISTS uq_to_device_txn_msgid
    ON to_device_transactions (sender_user_id, sender_device_id, message_id);
