# 03: MSC3967 /sync 增量 state token (P1, ~~3d~~ → **0.5d**)

**Status:** ✅ done — commit `237a7620`

**Spec reference:** https://github.com/matrix-org/matrix-spec-proposals/blob/main/proposals/3967-incremental-state-tokens.md

---

## 调研结论（实际发现 vs 原计划）

原实现计划列了 5 个步骤（新增 storage 方法、`SyncToken` 加字段、service 层新方法、feature flag、payload benchmark）。**实际调研发现**其中 4 个已存在：

| 步骤 | 原计划 | 实际 |
|------|--------|------|
| Storage delta 查询 | 新增 `get_state_events_changed_since` | ✅ **已有** `event_reader.get_state_events_since_batch` + `SinceFilter::StreamOrdering`（commit `aff4b0d1`） |
| `SyncToken` 新字段 | 加 `state_token` + 双向 encode/parse | ✅ **不需要** — stream ordering 已编码在现有 token 的 `stream_id` 字段（`< TIMESTAMP_TOKEN_MIN` 即视为 stream ordering） |
| 删除 `Vec::new()` | service 层重写 | ✅ **已有**（但有 bug，见下） |
| Payload benchmark | 集成测试 | ❌ 未做（两行修复后无性能退化） |

## 实际修复

### Bug 1 — `response.rs:365`（已修复）
```rust
// Before:
since_stream_ordering: None,

// After:
since_stream_ordering: Some(since_stream_ord),
```
per-room 路径传 `None` 导致 fallback 到 timestamp 过滤。

### Bug 2 — `response.rs:395`（已修复）
```rust
// Before:
// let state_list = if is_incremental { Vec::new() } else { state_list };

// After (删除整行，替换为注释):
// state_list already contains the delta computed by
// get_state_events_for_sync_batch above; pass it through unchanged.
```
增量时无条件清空已计算好的 state delta。

> ⚠️ **Bug 1 的影响**：即使删了 Bug 2，若 Bug 1 还在，`get_state_events_for_sync_batch` 会走 `OriginServerTs` fallback，产生的是**基于时间戳的增量**而非 **stream_ordering 增量**——语义不同。两者必须同时修。

### Batch 路径（已验证无 bug）
`build_sync_response` 在 line 99-103 正确传递了 `since_stream_ordering` 给 `get_state_events_for_sync_batch`，且未做 `Vec::new()` 清空。**该路径无需修改**。

## 单元测试

`incremental_room_sync_returns_state_delta_not_empty`（`tests.rs`）：
- `InMemoryEventStore` 种子 2 个 state 事件（`stream_ordering=5` 和 `=10`）
- 用 `since_token.stream_id=5`（低于 `TIMESTAMP_TOKEN_MIN=1_000_000_000_000`，被识别为 stream ordering）
- 断言 `result["state"]["events"].len() == 1`，包含 `$new_state` 事件

305/305 sync 测试全部通过 ✅

## 遗留（不在当前 scope）

- 集成测试（payload size benchmark）
- MSC4155/4156 thread 订阅 query 透传（`get_subscribed_threads` 写死 `Some(50)`）
- Federation forget race condition（远程 leave 事件在 forget 后到达的幂等处理）

## 估算对比

- 原始估算：3 人天
- 实际工作量：1.5 小时（调研 1h + 修复 + 测试 0.5h）
