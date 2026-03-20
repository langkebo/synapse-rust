-- ============================================================================
-- 修复 typing 表字段名问题
-- 问题: SQL 代码使用 is_typing，但表定义使用 typing
-- 修复: 添加 is_typing 列并同步数据
-- ============================================================================

-- 添加 is_typing 列 (如果不存在)
ALTER TABLE typing ADD COLUMN IF NOT EXISTS is_typing BOOLEAN DEFAULT FALSE;

-- 同步数据: typing -> is_typing
UPDATE typing SET is_typing = typing WHERE is_typing IS NULL OR typing IS NOT NULL;

-- 确保 is_typing 不为空
ALTER TABLE typing ALTER COLUMN is_typing SET NOT NULL;

-- 删除旧的 typing 列 (可选，保留兼容性可以不做)
-- ALTER TABLE typing DROP COLUMN IF EXISTS typing;

-- 验证
SELECT user_id, room_id, typing, is_typing FROM typing LIMIT 10;

-- 添加索引 (如果不存在)
CREATE INDEX IF NOT EXISTS idx_typing_user_room ON typing(user_id, room_id);
CREATE INDEX IF NOT EXISTS idx_typing_is_typing ON typing(is_typing) WHERE is_typing = TRUE;

COMMENT ON COLUMN typing.is_typing IS '用户是否正在 typing';
