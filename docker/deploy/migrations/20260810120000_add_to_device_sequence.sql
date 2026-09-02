-- E2EE-10: Ensure to-device messages are ordered by stream_id for delivery.
--
-- The to_device_messages table already has a stream_id column (assigned via
-- nextval('to_device_stream_id_seq')) that provides monotonic, globally-unique
-- sequence numbers. All active query paths (get_messages, get_messages_since)
-- already ORDER BY stream_id ASC.
--
-- This migration adds a composite index on (recipient_user_id, recipient_device_id,
-- stream_id ASC) to optimise the ordered retrieval pattern used by:
--   - get_messages_since: WHERE recipient_user_id = $1 AND recipient_device_id = $2
--                          AND stream_id > $3 ORDER BY stream_id ASC LIMIT $4
--   - get_and_delete_messages: WHERE recipient_user_id = $1 AND recipient_device_id = $2
--                               ORDER BY stream_id ASC  (fixed in this task)
--
-- The existing idx_to_device_recipient (recipient_user_id, recipient_device_id)
-- does not include stream_id, so the database must sort after fetching. The
-- existing idx_to_device_stream (recipient_user_id, stream_id) does not include
-- recipient_device_id, so it may scan rows for other devices of the same user.
-- This new index covers both filters and the ordering in a single index scan.

CREATE INDEX IF NOT EXISTS idx_to_device_ordered
    ON to_device_messages (recipient_user_id, recipient_device_id, stream_id ASC);
