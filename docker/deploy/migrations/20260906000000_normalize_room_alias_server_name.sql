-- B-7: Normalize existing room_aliases rows so that the `server_name`
-- portion is lowercased, matching Matrix spec v1.11 § 4.3 (server_name is
-- case-insensitive) and aligning historical data with the new
-- `RoomAlias::new` / `set_room_alias` / `get_room_by_alias` write path
-- (which lowercases server_name on every write).
--
-- We rebuild `room_alias` from the column boundary (first `:`) — anything
-- before the `:` is the localpart (case-sensitive, left untouched) and
-- anything from the first `:` onward is the server_name (lowercased).
--
-- This migration is idempotent: rows where the server_name is already
-- lowercased satisfy `server_name = LOWER(server_name)` and are skipped.
-- Rows with empty server_name are also skipped — they have no `:` boundary
-- to safely split on, and the write path now lowercases new empty-server
-- inputs to `localhost` so they won't be inserted with `''` in the future.
--
-- After this migration the `idx_room_aliases_room_alias` UNIQUE index
-- guarantees that subsequent case-only variations collide on a single row,
-- even if the application temporarily bypasses the storage layer.

UPDATE room_aliases
SET
    room_alias  = SUBSTRING(room_alias FROM 1 FOR POSITION(':' IN room_alias))
                 || LOWER(SUBSTRING(room_alias FROM POSITION(':' IN room_alias) + 1)),
    server_name = LOWER(server_name)
WHERE
    server_name <> LOWER(server_name)
    AND POSITION(':' IN room_alias) > 0;
