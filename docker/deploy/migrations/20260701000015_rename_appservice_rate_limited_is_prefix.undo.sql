-- Undo: rename is_rate_limited back to rate_limited
ALTER TABLE application_services RENAME COLUMN is_rate_limited TO rate_limited;
