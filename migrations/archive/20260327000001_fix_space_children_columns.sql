-- 修复 space_children 表缺失字段
-- 添加 order, added_by, removed_ts 字段

ALTER TABLE space_children ADD COLUMN IF NOT EXISTS "order" TEXT DEFAULT '';
ALTER TABLE space_children ADD COLUMN IF NOT EXISTS suggested BOOLEAN DEFAULT FALSE;
ALTER TABLE space_children ADD COLUMN IF NOT EXISTS added_by TEXT DEFAULT '';
ALTER TABLE space_children ADD COLUMN IF NOT EXISTS removed_ts BIGINT;

CREATE INDEX IF NOT EXISTS idx_space_children_order ON space_children("order");
CREATE INDEX IF NOT EXISTS idx_space_children_suggested ON space_children(suggested) WHERE suggested = TRUE;
