-- S14/SS-10: sliding_sync_tokens 增加 event_stream_pos 列。
-- 增量 sliding sync 的 timeline 此前恒取最新 N 条、不感知 pos，导致每次
-- 增量同步重复下发客户端已收事件。该列在每次同步结束时记录当时的
-- events.stream_ordering 快照，作为下一次增量同步 timeline 的起始水位线
-- （仅下发 stream_ordering > event_stream_pos 的事件）。

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'sliding_sync_tokens' AND column_name = 'event_stream_pos'
    ) THEN
        ALTER TABLE sliding_sync_tokens ADD COLUMN event_stream_pos BIGINT NOT NULL DEFAULT 0;
    END IF;
END $$;
