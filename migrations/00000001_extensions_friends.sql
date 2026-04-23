-- =============================================================================
-- Extension: Friends System (feature: friends)
-- Extracted from 00000000_unified_schema_v6.sql
-- Tables: friends, friend_requests, friend_categories
-- Note: blocked_users is kept in core (used by moderation regardless of friends feature)
-- =============================================================================

CREATE TABLE IF NOT EXISTS friends (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    friend_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_friends PRIMARY KEY (id),
    CONSTRAINT uq_friends_user_friend UNIQUE (user_id, friend_id),
    CONSTRAINT fk_friends_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_friends_friend FOREIGN KEY (friend_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS friend_requests (
    id BIGSERIAL,
    sender_id TEXT NOT NULL,
    receiver_id TEXT NOT NULL,
    message TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT,
    CONSTRAINT pk_friend_requests PRIMARY KEY (id),
    CONSTRAINT uq_friend_requests_sender_receiver UNIQUE (sender_id, receiver_id),
    CONSTRAINT fk_friend_requests_sender FOREIGN KEY (sender_id) REFERENCES users(user_id) ON DELETE CASCADE,
    CONSTRAINT fk_friend_requests_receiver FOREIGN KEY (receiver_id) REFERENCES users(user_id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS friend_categories (
    id BIGSERIAL,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT '#000000',
    created_ts BIGINT NOT NULL,
    CONSTRAINT pk_friend_categories PRIMARY KEY (id),
    CONSTRAINT fk_friend_categories_user FOREIGN KEY (user_id) REFERENCES users(user_id) ON DELETE CASCADE
);
