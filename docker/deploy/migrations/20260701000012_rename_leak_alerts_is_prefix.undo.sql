-- Undo: rename is_acknowledged back to acknowledged
ALTER TABLE leak_alerts RENAME COLUMN is_acknowledged TO acknowledged;
