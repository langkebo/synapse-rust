-- Align typing table boolean column with v10 is_ prefix naming convention.
-- v7: typing BOOLEAN  →  v10 / Rust: is_typing BOOLEAN

ALTER TABLE typing
    RENAME COLUMN typing TO is_typing;
