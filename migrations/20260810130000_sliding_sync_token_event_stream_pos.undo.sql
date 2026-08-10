-- Undo S14: drop the event_stream_pos watermark column.

ALTER TABLE sliding_sync_tokens DROP COLUMN IF EXISTS event_stream_pos;
