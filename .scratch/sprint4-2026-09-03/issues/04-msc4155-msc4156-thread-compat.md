# 04: MSC4155/4156 Thread subscription 验证 + 兼容路径补齐 (P2, 1.5d)

**What to build:** 验证已实现的 thread subscription 端点 (MSC4155/4156) 的 spec 合规性，并补齐 Element Web 旧版本期望的 `/_matrix/client/unstable/org.matrix.msc4155` 兼容路径。**改动量小、风险低**——核心逻辑已经实现，主要是补缺 + E2E 验证。

**Blocked by:** None

**Status:** ready-for-agent

**Spec references:**
- MSC4155: https://github.com/matrix-org/matrix-spec-proposals/blob/main/proposals/4155-get-thread-relationships.md
- MSC4156: https://github.com/matrix-org/matrix-spec-proposals/blob/main/proposals/4156-relationship-listing.md

**现状（已调研）:**
- **已实现端点**:
  - `GET /_matrix/client/v1/rooms/{roomId}/threads` (`src/web/routes/handlers/thread.rs:156-158` → service `thread_service.rs:300`)
  - `POST /_matrix/client/v1/rooms/{roomId}/threads/{threadId}/subscribe` (`thread.rs:191-194`)
  - `POST /_matrix/client/v1/rooms/{roomId}/threads/{threadId}/unsubscribe` (`thread.rs:195-198`)
  - `GET /_matrix/client/v1/rooms/{roomId}/threads/{threadId}/stats` (`thread.rs:207-210`)
  - `GET /_matrix/client/v1/threads/subscribed` (`thread.rs:138-141` → `thread_service.rs:483-506`)
  - `GET /_matrix/client/v1/threads/unread` (`thread.rs:142-145`)
  - `GET /_matrix/client/v1/threads` (`thread.rs:136-137`)
  - `GET /_matrix/client/v3/user/{userId}/rooms/{roomId}/threads` (`thread.rs:146-149` legacy shape)
- **数据层**: `migrations/00000000_unified_schema_v11.sql` 已有 `thread_subscriptions` 表 + 索引
- **Storage**: `synapse-storage/src/thread.rs:40-51` + mock
- **缺口 1**: `get_subscribed_threads` 写死 `Some(50)` (`thread.rs:650`)，未透传 `ListQuery` 的 `limit` / `from` / `include_all`
- **缺口 2**: `SubscribedThreadsResponse` 不带 `next_batch` / `from` 分页字段（`thread_service.rs:85` struct 需扩）
- **缺口 3**: 缺少 `/_matrix/client/unstable/org.matrix.msc4155` + `org.matrix.msc4156` 兼容 path stub（参考 `sliding_sync.rs:38` 模式）

**实现计划（acceptance criteria）:**
- [ ] `src/web/routes/handlers/thread.rs:646-650`: `get_subscribed_threads` 接受 `ListQuery { limit, from, include_all }` query 参数
  - 复用 line 58-63 已有的 `ListQuery` struct
  - 透传给 `ctx.thread_service.get_subscribed_threads(user_id, limit, from, include_all)`
- [ ] `synapse-services/src/thread_service.rs:483-506`: `get_subscribed_threads` 签名扩展 + 实现 `from` cursor 分页
- [ ] `synapse-services/src/thread_service.rs:85` 附近: `SubscribedThreadsResponse` struct 增 `from: Option<String>` / `to: Option<String>` 字段
- [ ] `src/web/routes/handlers/thread.rs:218-248` 路由清单（`thread_route_manifest`）加 2 条 unstable 路由:
  - `/_matrix/client/unstable/org.matrix.msc4155/rooms/{roomId}/threads`
  - `/_matrix/client/unstable/org.matrix.msc4156/threads/subscribed`
- [ ] 这 2 条新路由的 handler 直接调用现有 v1 handler（无业务逻辑，仅 path 兼容）
- [ ] E2E 测试（`tests/e2e/e2e_scenarios.rs:56` 已有 `test_thread_subscription`）补 2 个用例:
  - [ ] MSC4155 unstable 路径订阅 + 列表
  - [ ] MSC4156 unstable 路径分页
- [ ] 集成测试覆盖（如果有 `thread_service_tests_migrated.rs`）:
  - [ ] `get_subscribed_threads` 透传 `limit` / `from`
  - [ ] 分页边界（limit=0 / 越界 / from 不存在）
- [ ] `cargo build --locked` + `cargo clippy --all-features --locked -- -D warnings`
- [ ] 提交 commit `feat(thread): MSC4155/MSC4156 query 透传 + unstable 兼容路径`

**风险点:**
- ⚠️ 路由清单变更需要重新生成 `route_ledger_*.snapshot`（`UPDATE_ROUTE_LEDGER_SNAPSHOTS=1` + 同 feature 集，**沿用 Sprint 3 教训**）
- ⚠️ 旧 Element 客户端用 `unstable` 路径访问——v1 路由与 unstable 路由实现必须 100% 一致，否则行为漂移
- ⚠️ 已有 `test_thread_subscription` e2e 通过——本 ticket 风险极低

**估算:** 1.5 人天（query 透传 0.5d + 兼容路径 0.5d + E2E/集成测试 0.5d）
