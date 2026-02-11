-- 修复 voice_usage_stats 表结构以允许 room_id 为 NULL
-- 执行时间: 2026-02-06
-- 问题: voice_usage_stats 表的 room_id 列有 NOT NULL 约束，但语音消息可能没有 room_id

-- 1. 修改 room_id 列为可空
ALTER TABLE voice_usage_stats ALTER COLUMN room_id DROP NOT NULL;

-- 2. 修改唯一约束以适应 room_id 为 NULL 的情况
-- 先删除旧的唯一约束
ALTER TABLE voice_usage_stats DROP CONSTRAINT IF EXISTS voice_usage_stats_user_id_room_id_period_start_key;

-- 创建新的唯一约束，只对非 NULL 的 room_id 生效
CREATE UNIQUE INDEX voice_usage_stats_user_id_room_period_unique 
ON voice_usage_stats (user_id, period_start) 
WHERE room_id IS NOT NULL;

-- 3. 添加注释说明
COMMENT ON COLUMN voice_usage_stats.room_id IS '房间ID，可为NULL（用于按房间统计）';
COMMENT ON INDEX voice_usage_stats_user_id_room_period_unique IS '用户和日期的唯一约束（仅当room_id不为NULL时）';
