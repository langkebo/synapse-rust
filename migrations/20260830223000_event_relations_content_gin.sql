-- DB-05 (修正版): event_relations.content GIN 索引
--
-- 原诊断 P1-1 称 relates_to_event_id 单列索引缺失，但 baseline v10 中
-- idx_thread_relations_relates_to ON event_relations(relates_to_event_id) 已存在。
-- 本迁移仅补充缺失的 GIN 索引：
--   - 使用 jsonb_path_ops：仅索引标量值，体积最小，@> 查询性能最优
--   - 若需要 ?| ?& 等 key 存在性查询，改用 jsonb_ops（体积约 2x）
--
-- 无破坏性变更，IF NOT EXISTS 保护，纯增量索引。

CREATE INDEX IF NOT EXISTS idx_event_relations_content_gin
    ON event_relations USING GIN (content jsonb_path_ops);
