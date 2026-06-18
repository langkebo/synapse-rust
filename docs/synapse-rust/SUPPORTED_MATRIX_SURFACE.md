# Supported Matrix Surface

审查日期: 2026-06-14（P1-03.2 StaticStable 清理 + P1-06 surface 对齐）

基线:
- Matrix Specification latest: v1.18
- Synapse behavioral reference: element-hq/synapse v1.153.0 stable, v1.154.0rc1 pre-release
- 本文件记录 `synapse-rust` 对外声明的 Matrix 兼容面。声明必须保守: 只有实现、路由账本和测试证据能支撑的版本或 MSC 才能出现在 `/versions`、`/capabilities` 或联邦能力响应中。

## Client API Versions

当前 `/_matrix/client/versions` 由 `src/web/routes/handlers/versions.rs` 中的 `CLIENT_API_VERSION_SUPPORT` 生成。

已声明:
- Legacy r0: `r0.5.0`, `r0.6.0`, `r0.6.1`
- Stable v1: `v1.1` through `v1.13`

暂不声明:
- `v1.14` through `v1.18`

原因:
- Matrix 最新规范已到 v1.18，但本仓库尚未建立逐版本的端点、错误码、字段兼容性证据矩阵。
- Synapse 当前稳定版本也保持保守声明策略；本项目不应只因为上游规范发布就自动提升声明。

提升规则:
- 新增 stable version 前，必须列出该版本新增/变更的 Client-Server 行为。
- 对每个行为确认状态: implemented, intentionally unsupported, custom extension, or not applicable。
- 补齐 route ledger 或 contract/snapshot 测试后再修改 `CLIENT_API_VERSION_SUPPORT`。

## Client Capabilities

当前 `/_matrix/client/v3/capabilities` 由 `build_capabilities_response` 生成。

公共能力:
- `m.change_password`
- `m.room_versions`
- `m.set_displayname`
- `m.set_avatar_url`
- `m.3pid_changes`
- `m.room.summary`
- `m.room.suggested`
- `m.voice`
- `m.thread`
- `io.hula.sliding_sync`

认证后才暴露的私有/扩展能力:
- `m.sso`
- `io.hula.friends`
- `io.hula.widget`
- `io.hula.burn_after_read`
- `io.hula.voice_extended`
- `external_services`
- `ai_connection`
- `openclaw`

治理规则:
- 每个 capability 必须通过 `CapabilityFlag` 显式声明，并归入以下两类治理之一：
  - **RouteSurface**: 通过 `manifest_has_route` 检查路由清单中对应路由是否存在（如 `m.change_password`、`m.set_displayname`、`m.set_avatar_url`、`m.3pid_changes` 通过 `account_compat::account_compat_route_manifest()` 检查；`m.room.summary`、`m.voice`、`m.thread`、`io.hula.sliding_sync`、`external_services`、`io.hula.voice_extended`、`io.hula.widget`、`io.hula.burn_after_read`、`io.hula.friends` 等通过各自模块的路由清单检查）。
  - **ConfigControlled**: 通过配置文件控制（如 `m.sso` 通过 `sso_providers` 检查；`openclaw`、`ai_connection` 通过 `openclaw_routes_enabled` 检查）。
- 所有 capability 均无遗留 `StaticStable` 治理，由 `test_no_residual_static_stable_governance` 合约测试保障。
- 标准 `m.*` capability 必须对应 Matrix 规范能力或明确的兼容扩展。
- 私有能力必须使用 `io.hula.*` 或已有项目命名空间，避免冒充 Matrix stable / MSC。
- feature-gated capability 必须随编译特性或配置关闭而声明为 disabled 或不对未认证请求暴露。

## Room Versions

当前 room version 能力由 `src/common/room_versions.rs` 生成。

默认创建版本:
- `10`

当前能力矩阵:

| Version | Disposition | Create | Join/Accept | Parse | Federation |
| --- | --- | --- | --- | --- | --- |
| `1` through `13` | stable | yes | yes | yes | yes |

提升规则:
- 新增 room version 前，必须确认 create/join/upgrade/redaction/auth rules/state resolution 行为。
- `m.room_versions` client capability 与 federation `m.room_versions` 必须来自同一能力矩阵。
- 对仅能解析、不能创建或不能加入的 room version，应显式拆分能力模型，不能简单标记为 stable。
- `resolve_room_version` 只应返回可创建版本；联邦入口应使用 join/parse/federation 维度，避免“能读旧房间”被误解释为“能创建新房间”。
- Federation membership handlers now check the federation dimension before exposing or mutating room membership state, including join, leave, knock, invite, third-party invite, and member-query paths.

当前证据:
- `src/common/room_versions.rs` 中 `SUPPORTED_ROOM_VERSIONS` 已声明 `1..13`，默认版本为 `10`
- `src/web/routes/handlers/versions.rs` 通过 `client_room_versions_capability()` 直接复用同一能力矩阵
- `tests/integration/api_auth_routes_tests.rs` 已校验 `/versions` 与公开 `/capabilities` 返回值和 room version 常量一致
- `src/web/routes/handlers/versions.rs` 单元测试模块包含 5 个 contract/snapshot 测试：
  - `test_versions_response_snapshot_keys` — 校验 versions 响应键完整性
  - `test_capabilities_response_snapshot_public_surface` — 校验公开能力面
  - `test_capabilities_response_snapshot_authenticated_surface` — 校验认证能力面
  - `test_all_capabilities_have_governance_classification` — 校验所有 capability 均有治理分类
  - `test_no_residual_static_stable_governance` — 确保无遗留 StaticStable 治理

### Room Version 逐版本证据矩阵

| Version | Create | Join | Upgrade | Redaction | Auth Rules | State Resolution | 证据 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1-10 | ✅ | ✅ | ✅ | ✅ | v1-v10 auth rules | v1-v10 algorithm | `tests/integration/api_room_tests.rs` 覆盖 create/join/redaction；`src/services/room_service.rs` 实现 upgrade |
| 11 | ✅ | ✅ | ✅ | ✅ | MSC3667: restricted join rules | v11 (same as v10) | `src/common/room_versions.rs` 声明 stable；`src/services/room_service.rs` 支持 restricted join；`tests/integration/api_room_tests.rs` 验证 create+join |
| 12 | ✅ | ✅ | ✅ | ✅ | MSC3787: knock_restricted join rules | v12 (same as v11) | `src/common/room_versions.rs` 声明 stable；knock_restricted preset 在 `room_service.rs` 中处理 |
| 13 | ✅ | ✅ | ✅ | ✅ | MSC4151: simplified restricted join rules | v13 (same as v12) | `src/common/room_versions.rs` 声明 stable；restricted join 简化逻辑在 `room_service.rs` 中实现 |
- `src/web/routes/account_compat.rs` 中 `account_compat_route_manifest()` 声明了 `m.change_password`、`m.set_displayname`、`m.set_avatar_url`、`m.3pid_changes` 四个能力对应的路由清单

## Unstable And Custom Features

当前 `/versions.unstable_features` 声明:
- `m.lazy_load_members`
- `m.require_identity_server`
- `m.supports_login_via_phone_number`
- `org.matrix.msc3882`
- `org.matrix.msc3983`
- `org.matrix.msc3245`
- `org.matrix.msc3266`
- `org.matrix.msc3916`
- `uk.tcpip.msc4133`
- `org.matrix.msc3886.sliding_sync`
- `org.matrix.msc4261.widget`
- `io.hula.burn_after_read`
- `io.hula.friends`
- `org.matrix.msc3814` when enabled by config

治理规则:
- MSC identifiers must map to route/service tests or a documented partial-support note.
- Hula extensions must stay out of Matrix stable namespaces.
- Experimental performance-sensitive features, especially sync/sliding sync, need benchmark guardrails before default enablement changes.

## Verification

Focused gate for this surface:

```bash
# Unit-level contract/snapshot tests (P1-03.2)
cargo test --lib web::routes::handlers::versions::tests -- --nocapture
```

Broader gates before raising protocol declarations:

```bash
cargo test --test integration api_route_ledger_tests -- --nocapture
cargo test --test integration api_auth_routes_tests -- --nocapture
cargo test --test integration api_protocol_alignment_tests -- --nocapture
```

Contract/snapshot tests (5, all in `src/web/routes/handlers/versions.rs`):
- `test_versions_response_snapshot_keys` — versions 响应键完整性
- `test_capabilities_response_snapshot_public_surface` — 公开能力面
- `test_capabilities_response_snapshot_authenticated_surface` — 认证能力面
- `test_all_capabilities_have_governance_classification` — 治理分类完整性
- `test_no_residual_static_stable_governance` — 无残留 StaticStable

Governance classification summary:

| Capability | Governance | Evidence |
|---|---|---|
| `m.change_password` | RouteSurface | `account_compat::account_compat_route_manifest()` |
| `m.set_displayname` | RouteSurface | 同上 |
| `m.set_avatar_url` | RouteSurface | 同上 |
| `m.3pid_changes` | RouteSurface | 同上 |
| `m.room.summary` | RouteSurface | `room_summary::room_summary_route_manifest()` |
| `m.room.suggested` | RouteSurface | 派生自 `room_summary_capability()` |
| `m.voice` | RouteSurface | 派生自 `room_summary_capability()` |
| `m.thread` | RouteSurface | `thread::thread_route_manifest()` |
| `io.hula.sliding_sync` | RouteSurface | `sliding_sync::sliding_sync_route_manifest()` |
| `io.hula.friends` | RouteSurface | `friend_room::friend_route_manifest()` |
| `external_services` | RouteSurface | `external_service::external_service_route_manifest()` |
| `io.hula.voice_extended` | RouteSurface | `voice::voice_route_manifest()` |
| `io.hula.widget` | RouteSurface | `widget::widget_route_manifest()` |
| `io.hula.burn_after_read` | RouteSurface | `burn_after_read::burn_after_read_route_manifest()` |
| `m.sso` | ConfigControlled | `sso_providers(config)` |
| `openclaw` | ConfigControlled | `openclaw_routes_enabled(config)` |
| `ai_connection` | ConfigControlled | 同上 |
| `m.room_versions` | — | 特殊：由 `client_room_versions_capability()` 生成 |
