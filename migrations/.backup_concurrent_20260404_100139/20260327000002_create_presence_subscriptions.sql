-- Migration: Create presence_subscriptions table for MSC2776
-- Date: 2026-03-27
-- Description: Adds presence_subscriptions table for presence list subscriptions

-- Create presence_subscriptions table
CREATE TABLE IF NOT EXISTS presence_subscriptions (
    subscriber_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    PRIMARY KEY (subscriber_id, target_id)
);

-- Create index for looking up subscriptions by subscriber
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_presence_subscriptions_subscriber
    ON presence_subscriptions(subscriber_id);

-- Create index for looking up subscriptions by target
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_presence_subscriptions_target
    ON presence_subscriptions(target_id);

-- Add foreign key constraint
ALTER TABLE presence_subscriptions
    ADD CONSTRAINT fk_presence_subscriptions_subscriber
    FOREIGN KEY (subscriber_id)
    REFERENCES users(user_id)
    ON DELETE CASCADE;

COMMENT ON TABLE presence_subscriptions IS 'Stores presence subscriptions for MSC2776 (presence list)';