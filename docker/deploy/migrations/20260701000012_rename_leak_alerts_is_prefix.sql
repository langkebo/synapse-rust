-- Rename leak_alerts.acknowledged to is_acknowledged for v10 is_ prefix alignment
ALTER TABLE leak_alerts RENAME COLUMN acknowledged TO is_acknowledged;
