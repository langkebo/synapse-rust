-- Relations 游标分页索引优化（P1 递归关系查询加速）
--
-- 背景：relations/mod.rs 的 get_relations 使用键集（keyset）分页，
-- ORDER BY origin_server_ts {ASC|DESC}, event_id {ASC|DESC}。现有索引
-- idx_event_relations_room_event (room_id, relates_to_event_id, relation_type)
-- 只覆盖等值过滤列，未覆盖排序列。热点事件（1000+ reactions）分页时
-- 触发 Sort / Top-N，且旧游标谓词 `event_id > $4` 与排序键不一致导致漏行/重行。
--
-- 方案：新增覆盖「等值前缀 + 排序键」的复合索引，使查询走索引顺序扫描、
-- 消除 Sort；配套把游标改为行值比较 (origin_server_ts, event_id) > ($ts, $eid)。
-- B-tree 支持反向扫描，单个 DESC 索引即可服务 f（ASC）与 b（DESC）两个方向。
--
-- relation_type / is_redacted 作为索引内的残差过滤（不能进前导列，否则破坏
-- 排序连续性）；对带 relation_type 的子集，索引顺序扫描 + filter 仍远优于 Sort。

CREATE INDEX IF NOT EXISTS idx_event_relations_room_rel_ts_evt
    ON event_relations (room_id, relates_to_event_id, origin_server_ts DESC, event_id DESC);
