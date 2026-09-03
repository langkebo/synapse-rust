# 03: MSC3967 /sync 增量 state token (P1, 3d)

**What to build:** 实现 MSC3967——增量 /sync 的 `state` 字段返回**自上次 since 以来**的 state delta（新增/变化的 state event），而不是空数组。配合 `state_token` 让客户端确认 state 视图一致性。预计减少 ~40% /sync payload（基于 Synapse 实测）。

**Blocked by:** None（可与 ticket #01/#02 并行）

**Status:** ready-for-agent

**Spec reference:** https://github.com/matrix-org/matrix-spec-proposals/blob/main/proposals/3967-incremental-state-tokens.md

**现状（已调研）:**
- 路由: `GET /_matrix/client/{r0,v3}/sync` (`src/web/routes/sync.rs:32`)
- handler: `src/web/routes/handlers/sync.rs:1-216` —— 接收 `since` query，调用 `SyncToken::parse` 校验
- service 核心: `synapse-services/src/sync_service/mod.rs:152-164` —— 增量时 `is_incremental = since_token.is_some() && !is_full_state`
- **关键缺位**: `synapse-services/src/sync_service/response.rs:395` —— `let state_list = if is_incremental { Vec::new() } else { state_list };` 增量时**整个 state 数组被清空**。
- `SyncToken` (`synapse-services/src/sync_service/types.rs:12-19`) 仅有 5 字段，无 `state_token` 编码
- storage: `event_reader.get_state_events_by_type` 按 type 拉全量，缺少"过滤 stream_ordering > since" 路径

**实现计划（acceptance criteria）:**
- [ ] **PoC 阶段（半天）**: 在 test environment 验证 `state_events.stream_ordering` 索引存在且查询可走；记录 baseline benchmark（增量 sync payload size）
- [ ] `synapse-storage`: 新增 `get_state_events_changed_since(room_id, since_stream_ordering) -> Vec<StateEvent>`
  - 走 `current_state_events.stream_ordering` 索引（如不存在则 migration 加）
  - 或在 `events` 表做 `stream_ordering > since AND state_key IS NOT NULL` 扫描
- [ ] `synapse-services/src/sync_service/types.rs`: `SyncToken` 加 `state_token: Option<String>` + 双向 `parse`/`encode`
  - 旧 token 解析失败走 `400 M_BAD_PAGINATION`（现有逻辑 `sync.rs:97-98`）
  - **向后兼容**: 不带 state_token 字段的旧 token 仍可解析（视作 `state_token=None` → 走全量 state fallback）
- [ ] `synapse-services/src/sync_service/response.rs`:
  - 删除 line 395 的 `Vec::new()` 短路
  - 改为 `if is_incremental { self.compute_state_delta(room_id, since_token).await? } else { state_list }`
  - `next_batch` 编码 (line 202) 补 state_token
- [ ] `src/web/routes/handlers/sync.rs`: 透传 `state_token` 字段（如需要）
- [ ] Feature flag `msc3967_incremental_state` 控制开关（默认 off → 灰度到 on）
- [ ] 集成测试（`tests/integration/sync_service_tests_migrated.rs` 新增）:
  - [ ] 增量 sync 返回 state delta（新加 state event 后 since= 上次 token）
  - [ ] state_token 字段双向 round-trip
  - [ ] 旧 since token（无 state_token）走全量 fallback
  - [ ] Payload size 减少 ≥ 30%（基准对比）
- [ ] `cargo build --locked` + `cargo clippy --all-features --locked -- -D warnings`
- [ ] 提交 commit `feat(sync): MSC3967 incremental state tokens for /sync?since=`

**风险点:**
- ⚠️ **Wire format 兼容性**: `SyncToken.encode` 改了字段后老 token 解析失败率需要监控（Mitigation: 老 token fallback 全量 state，零回归）。
- ⚠️ **Storage 查询性能**: 大房间 state 增量可能跨数小时没有变化，但 `state_events` 表 scan 仍要 O(N) 索引范围。Mitigation: PoC 阶段 benchmark，确认 <100ms p99。
- ⚠️ **测试覆盖**: 1396 个 integration test 全部要复跑，耗时长（基线 51 分钟），必须 feature flag 灰度。
- ⚠️ **客户端兼容**: Element Web 已支持（v1.11+），其他客户端（Nheko、Dendrite）可能误把 state_token 当 unknown 字段——server 端只能确保 spec 合规。

**估算:** 3 人天（PoC 0.5d + storage 1d + service 1d + test/灰度 0.5d）
