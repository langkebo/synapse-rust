-- Initialize Friend List Rooms for Existing Users
-- Execution time: 2026-02-11
-- Description: Create friend list rooms for all existing users and migrate friend relationships from the friends table

-- Step 1: Create a helper function to generate friend list room IDs
-- This will be called by the migration script

-- Step 2: Create friend list rooms for all existing users
-- We create one room per user with the format: !friends:@user:server.com

-- Note: This migration uses PL/pgSQL to iterate over existing users
-- For large user bases, consider running this in batches

DO $$
DECLARE
    user_record RECORD;
    room_id TEXT;
    now_ts BIGINT;
    event_id TEXT;
    room_name TEXT;
BEGIN
    -- Get current timestamp in milliseconds
    now_ts := EXTRACT(EPOCH FROM NOW()) * 1000;

    -- Create friend list rooms for all existing users
    FOR user_record IN
        SELECT user_id FROM users WHERE user_id LIKE '@_%'
    LOOP
        -- Generate room ID: !friends:@user:server.com
        room_id := '!friends:' || substring(user_record.user_id from 2);

        -- Insert the room if it doesn't exist
        INSERT INTO rooms (room_id, creator, join_rule, version, is_public, member_count,
                          history_visibility, creation_ts, last_activity_ts)
        VALUES (room_id, user_record.user_id, 'invite', '1', false, 1, 'joined', now_ts, now_ts)
        ON CONFLICT (room_id) DO NOTHING;

        -- Set room name
        room_name := split_part(user_record.user_id, ':', 1);
        room_name := substring(room_name from 2) || '''s Friends';

        -- Insert m.room.name event
        event_id := '$' || now_ts || ':' || split_part(room_id, ':', 2);

        INSERT INTO events (event_id, room_id, user_id, sender, event_type, content,
                          state_key, origin_server_ts, processed_ts)
        VALUES (event_id, room_id, user_record.user_id, user_record.user_id, 'm.room.name',
                jsonb_build_object('name', room_name), '', now_ts, now_ts)
        ON CONFLICT (event_id) DO NOTHING;

        -- Add the user as a member
        event_id := '$' || (now_ts + 1) || ':' || split_part(room_id, ':', 2);

        INSERT INTO room_memberships (room_id, user_id, membership, event_id,
                                     sender, origin_server_ts, state_key)
        VALUES (room_id, user_record.user_id, 'join', event_id, user_record.user_id,
                now_ts, user_record.user_id)
        ON CONFLICT (room_id, user_id) DO NOTHING;

        -- Create initial empty friend list state event
        event_id := '$' || (now_ts + 2) || ':' || split_part(room_id, ':', 2);

        INSERT INTO events (event_id, room_id, user_id, sender, event_type, content,
                          state_key, origin_server_ts, processed_ts)
        VALUES (event_id, room_id, user_record.user_id, user_record.user_id, 'm.friends.list',
                jsonb_build_object('friends', '[]'::jsonb, 'version', 1), '', now_ts, now_ts)
        ON CONFLICT (event_id) DO NOTHING;

        RAISE NOTICE 'Created friend list room for user: %', user_record.user_id;
    END LOOP;
END $$;

-- Step 3: Migrate existing friend relationships from friends table to friend list rooms
DO $$
DECLARE
    friendship RECORD;
    room_id TEXT;
    now_ts BIGINT;
    event_id TEXT;
    friend_list jsonb;
    new_version INT;
BEGIN
    now_ts := EXTRACT(EPOCH FROM NOW()) * 1000;

    -- Process each friendship in the friends table
    FOR friendship IN
        SELECT user_id, friend_id, created_ts
        FROM friends
        ORDER BY user_id, created_ts
    LOOP
        -- Get the friend list room for the user
        room_id := '!friends:' || substring(friendship.user_id from 2);

        -- Read current friend list
        SELECT content INTO friend_list
        FROM events
        WHERE room_id = room_id
          AND event_type = 'm.friends.list'
          AND state_key = ''
        ORDER BY origin_server_ts DESC
        LIMIT 1;

        -- If no friend list exists, skip
        IF friend_list IS NULL THEN
            CONTINUE;
        END IF;

        -- Get current version
        new_version := COALESCE((friend_list->>'version')::int, 1) + 1;

        -- Add friend to the list
        -- We construct the new friend list by appending to the existing array
        friend_list := jsonb_set(
            friend_list,
            '{friends}',
            friend_list->'friends' || jsonb_build_array(
                jsonb_build_object(
                    'user_id', friendship.friend_id,
                    'since', friendship.created_ts,
                    'display_name', NULL,
                    'avatar_url', NULL,
                    'status', NULL,
                    'last_active', NULL,
                    'note', NULL,
                    'is_private', NULL
                )
            )
        );
        friend_list := jsonb_set(friend_list, '{version}', to_jsonb(new_version));

        -- Create new friend list state event
        event_id := '$' || now_ts || '_migration_' || friendship.user_id || '_' || friendship.friend_id;

        INSERT INTO events (event_id, room_id, user_id, sender, event_type, content,
                          state_key, origin_server_ts, processed_ts)
        VALUES (event_id, room_id, friendship.user_id, friendship.user_id, 'm.friends.list',
                friend_list, '', now_ts, now_ts)
        ON CONFLICT (event_id) DO NOTHING;

        RAISE NOTICE 'Migrated friendship: % -> %', friendship.user_id, friendship.friend_id;
    END LOOP;
END $$;

-- Step 4: Add comment to document the migration
COMMENT ON TABLE friends IS 'Legacy friends table - migrated to friend list rooms. Use m.friends.list events in friend list rooms instead.';

-- Step 5: Create index for friend list room lookups
CREATE INDEX IF NOT EXISTS idx_events_friend_list_rooms
ON events(event_type)
WHERE event_type = 'm.friends.list';

-- Step 6: Create index for friend request lookups
CREATE INDEX IF NOT EXISTS idx_friend_requests_status
ON friend_requests(status, created_ts)
WHERE status = 'pending';

-- Migration complete
--
-- Summary:
-- - Created friend list rooms for all existing users
-- - Migrated friend relationships from friends table to m.friends.list events
-- - The friends table is kept for reference but should not be used for new code
-- - Use FriendRoomStorage for all friend operations going forward
