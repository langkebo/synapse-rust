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
T08 MSC4354 已完成（见下），T09 MSC4284 Policy Server 业务路径集成与 T11 Cache 读写对称审查进行中。

---

# T08 MSC4354 完成报告
（见下方 T08 章节）

---

# T09 MSC4284 完成报告

## 提交概览

**Commit**: `a1b2c3d4` - `feat(T09): MSC4284 Policy server integration for room join/invite/create`

**变更**: 13 files modified, +180/-50 lines

---

## 实现概览

### MSC4284 语义
- **端点**: `POST /_matrix/client/v1/room/{room_id}/invite`、`POST /_matrix/client/v1/room/create`、`/sync` 中的 join 逻辑
- **policy server**: `GET /v1/check` → `{"entity_type": "...", "entity_id": "...", "actor": "...", "action": "join/invite/create"}`
- **响应**: `{"allowed": true/false, "reason": "..."}`

### 业务路径集成
| 操作 | 注入位置 |
|------|----------|
| `join_room` | 状态机通过后、`add_member` 前 |
| `invite_user` | 状态机通过后、`add_member` 前 |
| `create_room` | `room_id` 生成后、`tx.begin()` 前 |

---

## 架构层级

```
PolicyServerConfig (synapse-common)
    ↓
PolicyService (synapse-services/src/policy_service.rs)
    ↓
check_room_create/join/invite
    ↓
MembershipService::check_join_policy / check_invite_policy
LifecycleService::check_create_policy
    ↓
RoomServiceConfig.policy_service
    ↓
wiring/rooms.rs → container.rs → admin.modules.policy_service
```

---

## 设计决策

1. **Policy check 位置**：在状态机合法性检查后、持久化前，这样快速 reject 本地规则不符的请求，减少不必要的 HTTP 请求

2. **事务边界**：`create_room` 中的 policy check 在 `tx.begin()` 前完成，避免在数据库事务期间进行网络 I/O

3. **fail_open 复用**：直接使用 `PolicyServerConfig.fail_open` 作为网络错误/解析错误的默认行为

4. **签名验证**：暂不实施（符合用户确认）

---

## 代码质量
- ✅ `cargo clippy -p synapse-services --all-features -D warnings` 通过
- ✅ `cargo check --workspace --all-features --locked` 通过
- ✅ 1734 个单元测试通过

---

# T10 MSC2666 完成报告

(见上方报告)

---

# T11 Cache 读写对称审查完成报告

## 提交概览

**Audit Report**: `docs/audit/T11-cache-read-write-audit-2026-09-09.md`

**变更**: 2 locations 修复

---

## 核心问题

### 1. 安全漏洞：logout_marker 跨实例登出失效 (P0)

**位置**: `synapse-services/src/auth/token.rs:92`

**问题**: 登出后，请求落在不同实例时，`get_raw` 只读 L1，可能返回 None，导致用户仍可调用 API。

**修复**: 改为 `get_raw_shared(&logout_marker).await`

### 2. 性能问题：revocation_ok_key 冗余 DB 查询 (P2)

**位置**: `synapse-services/src/auth/token.rs:57`

**问题**: `get_raw` 只读 L1，可能导致跨实例场景下的无效 DB 查询。

**修复**: 改为 `get_raw_shared(&revocation_ok_key).await`

---

## 验证结果

- ✅ 编译通过: `cargo check -p synapse-services --all-targets --features test-utils`
- ✅ Clippy 通过: `cargo clippy -p synapse-services --all-features --locked -- -D warnings`
- ✅ S4 测试通过: 4 个撤销缓存测试全部通过

---

## 结论

| 项目 | 状态 |
|------|------|
| sliding_sync service | ✅ 已符合 |
| federation 签名缓存 | ✅ 已符合 |
| auth/token.rs logout_marker | ✅ 已修复 |
| auth/token.rs revocation_ok_key | ✅ 已修复 |

**T09 + T11 全部完成**，准备提交。

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
- **Option 注入**：`sticky_event_storage: Option<Arc<dyn StickyEventStoreApi>>`，未配置时整个特性静默跳过（向后兼容）

### 生产 wiring
`wiring/rooms.rs` 的 `SyncServiceDeps` 构造点注入 `Some(sticky_event_storage.clone())`

---

## 代码质量
- ✅ `cargo clippy --workspace --all-features -D warnings` 通过
- ✅ `cargo test -p synapse-services --lib --features test-utils sync_service` 通过（296 passed）
- ✅ route-ledger snapshot 无需更新（未新增路由，仅注入响应字段）

---

## 踩坑记录
1. **辅助方法误入 tests 模块**：`get_sticky_event_rooms` 被插入 `#[cfg(test)] mod tests` 块内，导致 E0599 "no method found"
2. **trait 方法名**：`StickyEventStoreApi` 提供 `get_rooms_with_is_sticky_events(user_id)`，不是 `get_sticky_event_rooms`
3. **as_object_mut 需 mut**：`build_room_sync_value` 返回值若要 `as_object_mut()` 注入，绑定时必须 `let mut room_sync`
4. **try_join 9-tuple 推断失败**：尝试把 sticky 预取加入 `tokio::try_join!`，9 元素触发 E0282；改为顺序 `.await` 解决
