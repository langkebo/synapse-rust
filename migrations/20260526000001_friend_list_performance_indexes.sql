-- Friend list performance indexes
-- Partial indexes for friend room lookup and friend list content queries

-- Index for get_friend_list_room_id: finds the friend room by sender + event_type + content type
CREATE INDEX IF NOT EXISTS idx_events_friend_room
ON events(sender, room_id, origin_server_ts DESC)
WHERE event_type = 'm.room.create' AND content->>'type' = 'm.friends';

-- Index for get_friend_list_content: finds the latest friend list state in a room
CREATE INDEX IF NOT EXISTS idx_events_friend_list
ON events(room_id, origin_server_ts DESC)
WHERE event_type = 'm.friends.list' AND state_key = '';

-- Composite indexes for friend_requests queries
CREATE INDEX IF NOT EXISTS idx_friend_requests_receiver_status
ON friend_requests(receiver_id, status, created_ts DESC);

CREATE INDEX IF NOT EXISTS idx_friend_requests_sender_status
ON friend_requests(sender_id, status, created_ts DESC);
