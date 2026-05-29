# Supported Matrix Surface

审查日期: 2026-05-29

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
- 标准 `m.*` capability 必须对应 Matrix 规范能力或明确的兼容扩展。
- 私有能力必须使用 `io.hula.*` 或已有项目命名空间，避免冒充 Matrix stable / MSC。
- feature-gated capability 必须随编译特性或配置关闭而声明为 disabled 或不对未认证请求暴露。
- 下一阶段应把 profile/password/3PID 等稳定能力从静态 `true` 收敛到配置和 route ledger 证据。

## Room Versions

当前 room version 能力由 `src/common/room_versions.rs` 生成。

默认创建版本:
- `10`

当前能力矩阵:

| Version | Disposition | Create | Join/Accept | Parse | Federation |
| --- | --- | --- | --- | --- | --- |
| `1` through `11` | stable | yes | yes | yes | yes |

暂不声明:
- `12`

提升规则:
- 新增 room version 前，必须确认 create/join/upgrade/redaction/auth rules/state resolution 行为。
- `m.room_versions` client capability 与 federation `m.room_versions` 必须来自同一能力矩阵。
- 对仅能解析、不能创建或不能加入的 room version，应显式拆分能力模型，不能简单标记为 stable。
- `resolve_room_version` 只应返回可创建版本；联邦入口应使用 join/parse/federation 维度，避免“能读旧房间”被误解释为“能创建新房间”。
- Federation membership handlers now check the federation dimension before exposing or mutating room membership state, including join, leave, knock, invite, third-party invite, and member-query paths.

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
cargo test --lib web::routes::handlers::versions::tests -- --nocapture
```

Broader gates before raising protocol declarations:

```bash
cargo test --test integration api_route_ledger_tests -- --nocapture
cargo test --test integration api_auth_routes_tests -- --nocapture
cargo test --test integration api_protocol_alignment_tests -- --nocapture
```
