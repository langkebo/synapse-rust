# T10 MSC2666 完成报告

## 提交概览

**Commit**: `9ea4175f` - `feat(T10): MSC2666 Get rooms in common (mutual_rooms endpoint)`

**变更**: 9 files, +450/-5 lines

---

## 实现概览

### MSC2666 语义
- **端点**: `GET /_matrix/client/v1/user/mutual_rooms`
- **查询参数**: `user_id` (必需), `from`/`batch_token` (分页), `limit` (默认 100, 最大 1000)
- **响应**: `{"joined": ["!room:server"], "next_batch_token": "..."}`
- **特殊规则**: 查询自己 → M_FORBIDDEN (400)

---

## 架构层级

```
handler (query.rs)
    ↓
service (membership/service.rs:417-454)
    ↓
storage (membership/api.rs + membership/mod.rs + test_mocks/member.rs)
```

### 数据层 (storage)
- **Postgres 实现** (`membership/mod.rs:1032-1065`): 自连接 JOIN 双用户 membership 表，按 `room_id` keyset 分页 (`LIMIT $limit+1` 检测 has_more)
- **InMemory Mock** (`test_mocks/member.rs:565-600`): HashSet 交集 + sort + 过滤 + 分页

### 业务层 (service)
- `get_mutual_rooms_between(user_id, other_user_id, limit, after)`
  - 自查询 → `ApiError::forbidden()`
  - 调用 storage，返回 `serde_json::Value { joined: [...], next_batch_token? }`

### 接口层 (handler)
- `get_mutual_rooms` 从 query params 解析 `user_id`/`from`/`limit`
- 统一经过 `validate_user_id` 验证

### 路由层
- **stable**: `/_matrix/client/v1/user/mutual_rooms`
- **unstable**: `/_matrix/client/unstable/uk.half-shot.msc2666/user/mutual_rooms`
- 两条均登记 `room_route_manifest()`，同步写入 route-ledger snapshots

---

## 测试覆盖

### Storage 层 (5 条)
| 测试 | 场景 |
|------|------|
| `mutual_rooms_returns_only_common_joined_rooms` | 交叉房间返回，排序 |
| `mutual_rooms_excludes_non_join_membership` | 只返回 join 状态 |
| `mutual_rooms_empty_when_no_common` | 无共同房间 → 空数组 |
| `mutual_rooms_after_filter_is_strictly_greater` | after 游标 > 条件 |
| `mutual_rooms_pagination_truncates_and_emits_token` | LIMIT+1 检测 + token |

### Service 层 (5 条)
| 测试 | 场景 |
|------|------|
| `mutual_rooms_returns_common_joined_rooms` | 多房间互斥测试 |
| `mutual_rooms_self_query_returns_forbidden` | 自查询 M_FORBIDDEN |
| `mutual_rooms_empty_when_no_common` | JSON 空数组结构 |
| `mutual_rooms_excludes_non_join_membership` | 仅 join 有效 |
| `mutual_rooms_pagination_emits_next_batch_token` | 完整分页流程 |

---

## 代码质量
- ✅ `cargo clippy --all-features -D warnings` 通过
- ✅ `cargo check -p synapse-rust --features test-utils` 通过
- ✅ 单元测试 10/10 通过

---

## 后续
T08 MSC4354 已完成（见下），T09 MSC4284 Policy Server 与 T11 Cache 读写对称审查进行中。

---

# T08 MSC4354 完成报告

## 提交概览

**Commit**: `75dc1b10` - `feat(T08): MSC4354 Sticky Events in v2 /sync response`

**变更**: 8 files, +77/-2 lines

---

## 实现概览

### MSC4354 语义
- 用户可将房间内某类事件标记为 "sticky"（置顶/固定）
- v2 `/sync` 响应中，每个配置了 sticky 事件的房间新增 `sticky_events` 数组：
  `[{"event_type": "...", "event_id": "...", "is_sticky": true}, ...]`
- sliding sync 早已实现（`sliding_sync_service/filters.rs`），本次对齐到 v2 `/sync`

### 架构层级
```
SyncServiceDeps.sticky_event_storage (types.rs)
    ↓ from_deps
SyncService.sticky_event_storage (mod.rs)
    ↓ build_sync_response
response.rs: 预取房间集 + 逐房间注入 sticky_events
```

### 关键设计
- **N+1 避免**：先 `get_rooms_with_is_sticky_events(user_id)` 取 DISTINCT 房间集（一次廉价查询），仅对有 sticky 的房间调用 `get_all_is_sticky_events`
- **Fail-open**：sticky 存储查询失败返回空集合 + warn 日志，不破坏整个 `/sync`
- **Option 注入**：`sticky_event_storage: Option<Arc<dyn StickyEventStoreApi>>`，未配置时整个特性静默跳过（向后兼容，18 个 `new()` 测试调用点传 None 不受影响）

### 生产 wiring
`wiring/rooms.rs` 的 `SyncServiceDeps` 构造点注入 `Some(sticky_event_storage.clone())`（该参数在 `RoomSyncServices::new` 签名已存在，原仅用于 sliding sync）。

---

## 代码质量
- ✅ `cargo clippy --workspace --all-features -D warnings` 通过
- ✅ `cargo test -p synapse-services --lib --features test-utils sync_service` 通过（296 passed）
- ✅ route-ledger snapshot 无需更新（未新增路由，仅注入响应字段）

---

## 踩坑记录（已写入 memory）
1. **辅助方法误入 tests 模块**：`get_sticky_event_rooms` 曾被插入 `#[cfg(test)] mod tests` 块内（同文件 partial impl 之后），导致 E0599 "no method found"。`impl SyncService` 的私有方法必须在主 impl 块（line 596 `}` 之前）。
2. **trait 方法名**：`StickyEventStoreApi` 提供的是 `get_rooms_with_is_sticky_events(user_id)`，不是 `get_sticky_event_rooms`（后者是 service 层自定义 helper）。
3. **as_object_mut 需 mut**：`build_room_sync_value` 返回值若要 `as_object_mut()` 注入字段，绑定时必须 `let mut room_sync`。
4. **try_join 9-tuple 推断失败**：曾尝试把 sticky 预取加入 `tokio::try_join!`，9 元素触发 E0282；改为 try_join 后顺序 `.await` 解决。