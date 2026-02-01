-- Synapse-Rust Test Data Setup Script
-- Purpose: Populate the database with diverse test data for API verification

-- 1. Clean up existing data (optional, but recommended for clean state)
DELETE FROM ip_blocks;
DELETE FROM friend_requests;
DELETE FROM friends;
DELETE FROM private_messages;
DELETE FROM private_sessions;
DELETE FROM room_memberships;
DELETE FROM events;
DELETE FROM rooms;
DELETE FROM access_tokens;
DELETE FROM devices;
DELETE FROM users;

-- 2. Create Users
-- Admin User
INSERT INTO users (user_id, username, password_hash, is_admin, creation_ts, generation)
VALUES ('@admin:server', 'admin', '$2b$12$LQvPHiU.v6/7bX.G5v7t/.6o7v.7o.7o.7o.7o.7o.7o.7o.7o.7o', TRUE, 1700000000000, 1);

-- Regular Users
INSERT INTO users (user_id, username, password_hash, is_admin, creation_ts, generation)
VALUES ('@alice:server', 'alice', '$2b$12$LQvPHiU.v6/7bX.G5v7t/.6o7v.7o.7o.7o.7o.7o.7o.7o.7o.7o', FALSE, 1700000010000, 1);
INSERT INTO users (user_id, username, password_hash, is_admin, creation_ts, generation)
VALUES ('@bob:server', 'bob', '$2b$12$LQvPHiU.v6/7bX.G5v7t/.6o7v.7o.7o.7o.7o.7o.7o.7o.7o.7o', FALSE, 1700000020000, 1);
INSERT INTO users (user_id, username, password_hash, is_admin, creation_ts, generation)
VALUES ('@charlie:server', 'charlie', '$2b$12$LQvPHiU.v6/7bX.G5v7t/.6o7v.7o.7o.7o.7o.7o.7o.7o.7o.7o', FALSE, 1700000030000, 1);
INSERT INTO users (user_id, username, password_hash, is_admin, creation_ts, deactivated, generation)
VALUES ('@banned:server', 'banned_user', '$2b$12$LQvPHiU.v6/7bX.G5v7t/.6o7v.7o.7o.7o.7o.7o.7o.7o.7o.7o', FALSE, 1700000040000, TRUE, 1);

-- 3. Create Devices & Access Tokens
-- Admin token
INSERT INTO devices (device_id, user_id, display_name, created_at, first_seen_ts)
VALUES ('ADMIN_DEV', '@admin:server', 'Admin Laptop', 1700000000000, 1700000000000);
INSERT INTO access_tokens (token, user_id, device_id, expires_ts, created_ts)
VALUES ('admin_token', '@admin:server', 'ADMIN_DEV', 1800000000000, 1700000000000);

-- Alice token
INSERT INTO devices (device_id, user_id, display_name, created_at, first_seen_ts)
VALUES ('ALICE_DEV', '@alice:server', 'Alice Phone', 1700000010000, 1700000010000);
INSERT INTO access_tokens (token, user_id, device_id, expires_ts, created_ts)
VALUES ('alice_token', '@alice:server', 'ALICE_DEV', 1800000000000, 1700000010000);

-- 4. Create Rooms
-- Public Room
INSERT INTO rooms (room_id, creator, is_public, name, topic, creation_ts, last_activity_ts, visibility)
VALUES ('!public_room:server', '@admin:server', TRUE, 'Public Square', 'A place for everyone', 1700000050000, 1700000050000, 'public');

-- Private Room
INSERT INTO rooms (room_id, creator, is_public, name, topic, creation_ts, last_activity_ts, visibility, join_rule)
VALUES ('!private_room:server', '@alice:server', FALSE, 'Secret Club', 'Members only', 1700000060000, 1700000060000, 'private', 'invite');

-- 5. Memberships
-- Admin in Public Room
INSERT INTO room_memberships (room_id, user_id, event_id, membership, joined_ts)
VALUES ('!public_room:server', '@admin:server', '$event_1', 'join', 1700000050000);

-- Alice in Public and Private Room
INSERT INTO room_memberships (room_id, user_id, event_id, membership, joined_ts)
VALUES ('!public_room:server', '@alice:server', '$event_2', 'join', 1700000055000);
INSERT INTO room_memberships (room_id, user_id, event_id, membership, joined_ts)
VALUES ('!private_room:server', '@alice:server', '$event_3', 'join', 1700000060000);

-- Bob invited to Private Room
INSERT INTO room_memberships (room_id, user_id, event_id, membership, joined_ts, sender)
VALUES ('!private_room:server', '@bob:server', '$event_4', 'invite', 1700000065000, '@alice:server');

-- 6. Events (Messages)
INSERT INTO events (event_id, room_id, event_type, content, sender, origin_server_ts)
VALUES ('$msg_1', '!public_room:server', 'm.room.message', '{"body": "Hello World", "msgtype": "m.text"}', '@admin:server', 1700000070000);
INSERT INTO events (event_id, room_id, event_type, content, sender, origin_server_ts)
VALUES ('$msg_2', '!public_room:server', 'm.room.message', '{"body": "Hi Admin!", "msgtype": "m.text"}', '@alice:server', 1700000075000);

-- 7. Friends
INSERT INTO friends (user_id, friend_id, created_ts, note)
VALUES ('@alice:server', '@bob:server', 1700000080000, 'My best friend');
INSERT INTO friends (user_id, friend_id, created_ts, note)
VALUES ('@bob:server', '@alice:server', 1700000080000, 'Alice is cool');

-- Pending Request
INSERT INTO friend_requests (from_user_id, to_user_id, created_ts, status, message)
VALUES ('@charlie:server', '@alice:server', 1700000090000, 'pending', 'Let be friends!');

-- 8. Private Sessions
INSERT INTO private_sessions (id, user_id_1, user_id_2, session_type, created_ts, last_activity_ts)
VALUES ('session_alice_bob', '@alice:server', '@bob:server', 'direct', 1700000100000, 1700000100000);

INSERT INTO private_messages (session_id, sender_id, content, created_ts, message_type)
VALUES ('session_alice_bob', '@alice:server', 'Hey Bob, this is private', 1700000110000, 'm.text');

-- 9. IP Blocks (Admin Security)
INSERT INTO ip_blocks (ip_range, ip_address, reason, blocked_at, blocked_ts)
VALUES ('1.2.3.4/32', '1.2.3.4', 'Spamming', 1700000120000, 1700000120000);
INSERT INTO ip_blocks (ip_range, reason, blocked_at, blocked_ts)
VALUES ('5.6.7.0/24', 'Botnet range', 1700000130000, 1700000130000);
