-- Drop tables that have no code references and are not part of the Matrix spec.
-- These were over-engineered features that were never wired into the application.
-- Safe: verified zero DML references in src/ for each table.

DROP TABLE IF EXISTS private_messages CASCADE;
DROP TABLE IF EXISTS private_sessions CASCADE;
DROP TABLE IF EXISTS room_children CASCADE;
DROP TABLE IF EXISTS ip_reputation CASCADE;
