# Task 14 - 搜索性能基线

## 1. 基线目标

在不切外部搜索后端的前提下，以 Postgres FTS 为最小闭环建立统一性能门槛。

## 2. 目标指标

| 场景 | 目标 |
| --- | --- |
| 房间事件搜索 P50 | <= 200 ms |
| 房间事件搜索 P95 | <= 800 ms |
| 用户/房间补全 P95 | <= 150 ms |
| 深分页稳定性 | 同一查询多次翻页结果顺序稳定 |
| 索引命中 | 关键事件搜索查询应命中 FTS 或复合索引，不允许长期全表 `LIKE` 扫描 |

## 3. 索引策略

- 事件正文最小策略：对可搜索文本建立 `to_tsvector` 索引。
- 权限裁剪依赖表：`room_memberships(room_id, user_id, membership)` 维持高效过滤。
- 排序字段：`events(origin_server_ts DESC)` 或复合索引支持 `recent` 排序。
- `LIKE` 兜底路径必须标记为过渡方案，并限制在小数据集或非主链。

## 4. 验证方法

- 为主查询保留 `EXPLAIN ANALYZE` 样例并归档。
- 在集成测试中固定小样本，验证分页 token 与排序稳定性。
- provider 切换时，运行同一 DSL 样例集比较结果结构与耗时。

### 4.1 数据集假设（用于可重复对标）

- 房间数：>= 10k
- 每房间事件量：>= 10k（用于暴露深分页与排序稳定性问题）
- 活跃用户：>= 100k（用于 sender 过滤与权限裁剪）

### 4.2 最小 EXPLAIN 模板（Postgres）

Room events + recent（示意）：

```sql
EXPLAIN (ANALYZE, BUFFERS)
SELECT e.event_id, e.room_id, e.sender, e.event_type, e.origin_server_ts
FROM events e
JOIN room_memberships m
  ON m.room_id = e.room_id AND m.user_id = $1 AND m.membership = 'join'
WHERE e.room_id = ANY($2)
  AND e.origin_server_ts <= $3
ORDER BY e.origin_server_ts DESC, e.event_id DESC
LIMIT $4;
```

命中期望（最小）：
- `events(room_id, origin_server_ts DESC)` 或等价覆盖索引（recent 排序）
- `room_memberships(room_id, user_id, membership)` 或等价复合索引（权限裁剪）

## 5. 失败回滚条件

- P95 明显高于当前主链且无索引修正方案。
- 分页结果不稳定或重复/漏项。
- 高亮语义对客户端造成行为回退。
- provider 切换后权限范围变弱。
