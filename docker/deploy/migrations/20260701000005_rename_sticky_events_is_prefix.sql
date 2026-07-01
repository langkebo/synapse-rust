-- Align room_sticky_events boolean column with v10 is_ prefix naming convention.
-- v7: sticky BOOLEAN  →  v10 / Rust: is_sticky BOOLEAN

ALTER TABLE room_sticky_events
    RENAME COLUMN sticky TO is_sticky;
