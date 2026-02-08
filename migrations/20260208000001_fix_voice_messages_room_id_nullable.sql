-- 修复 voice_messages 表 room_id 字段约束
-- 执行时间: 2026-02-08
-- 问题: room_id 字段不应有 NOT NULL 约束，因为语音消息可以不在房间中

-- 移除 room_id 的 NOT NULL 约束
ALTER TABLE voice_messages ALTER COLUMN room_id DROP NOT NULL;

-- 验证约束已移除
-- SELECT column_name, is_nullable 
-- FROM information_schema.columns 
-- WHERE table_name = 'voice_messages' AND column_name = 'room_id';
