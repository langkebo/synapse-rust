# Supported Matrix Surface

审查日期: 2026-06-22（P1-01/T4 收紧超前声明 + 修正 m.voice/m.room.suggested 派生 + 补 MSC3814/MSC4143 声明 + 修复 sso_providers OIDC 遗漏）

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

> 注: `io.hula.sliding_sync` 已从公开能力面移除。客户端通过标准 `org.matrix.msc3886.sliding_sync` unstable feature 发现 sliding sync。

认证后才暴露的私有/扩展能力:
- `m.sso`（providers 包含 `saml`/`oidc`/`cas`，取决于配置）
- `io.hula.friends`
- `io.hula.burn_after_read`
- `io.hula.voice_extended`
- `external_services`
- `ai_connection`
- `openclaw`

治理规则:
- 每个 capability 必须通过 `CapabilityFlag` 显式声明，并归入以下两类治理之一：
  - **RouteSurface**: 通过 `manifest_has_route` 检查路由清单中对应路由是否存在。
  - **ConfigControlled**: 通过配置文件控制（如 `m.sso` 通过 `sso_providers` 检查；`openclaw`、`ai_connection` 通过 `openclaw_routes_enabled` 检查）。
- 所有 capability 均无遗留 `StaticStable` 治理，由 `test_no_residual_static_stable_governance` 合约测试保障。
- 标准 `m.*` capability 必须对应 Matrix 规范能力或明确的兼容扩展。
- 私有能力必须使用 `io.hula.*` 或已有项目命名空间，避免冒充 Matrix stable / MSC。
- feature-gated capability 必须随编译特性或配置关闭而声明为 disabled 或不对未认证请求暴露。
- 私有 `io.hula.*` 扩展不应在 `/versions.unstable_features`（未认证面）暴露，仅在 `/capabilities` 认证面声明。

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
- `resolve_room_version` 只应返回可创建版本；联邦入口应使用 join/parse/federation 维度，避免"能读旧房间"被误解释为"能创建新房间"。
- Federation membership handlers now check the federation dimension before exposing or mutating room membership state, including join, leave, knock, invite, third-party invite, and member-query paths.

当前证据:
- `src/common/room_versions.rs` 中 `SUPPORTED_ROOM_VERSIONS` 已声明 `1..13`，默认版本为 `10`
- `src/web/routes/handlers/versions.rs` 通过 `client_room_versions_capability()` 直接复用同一能力矩阵
- `tests/integration/api_auth_routes_tests.rs` 已校验 `/versions` 与公开 `/capabilities` 返回值和 room version 常量一致

### Room Version 逐版本证据矩阵

| Version | Create | Join | Upgrade | Redaction | Auth Rules | State Resolution | 证据 |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1-10 | ✅ | ✅ | ✅ | ✅ | v1-v10 auth rules | v1-v10 algorithm | `tests/integration/api_room_tests.rs` 覆盖 create/join/redaction；`src/services/room_service.rs` 实现 upgrade |
| 11 | ✅ | ✅ | ✅ | ✅ | MSC3667: restricted join rules | v11 (same as v10) | `src/common/room_versions.rs` 声明 stable（can_create=true）；v11+ redaction 格式（MSC2174/MSC3820）已实现 |
| 12 | ✅ | ✅ | ✅ | ✅ | MSC3787: knock_restricted join rules | v12 (same as v11) | `src/common/room_versions.rs` 声明 stable；redaction 链路已完成 |
| 13 | ✅ | ✅ | ✅ | ✅ | MSC4151: simplified restricted join rules | v13 (same as v12) | `src/common/room_versions.rs` 声明 stable；redaction 链路已完成 |

## Unstable And Custom Features

当前 `/versions.unstable_features` 声明（全部 route-surface driven 或无条件标准特性）:

| 特性 | 声明方式 | 说明 |
| --- | --- | --- |
| `m.lazy_load_members` | Unconditional (true) | 标准特性，始终支持 |
| `m.require_identity_server` | Unconditional (false) | 标准特性，不需要 identity server |
| `m.supports_login_via_phone_number` | Unconditional (true) | 标准特性 |
| `org.matrix.msc3882` | Unconditional (true) | QR code login，路由始终注册 |
| `uk.tcpip.msc4133` | Unconditional (true) | Extended profile，路由始终注册 |
| `org.matrix.msc3886.sliding_sync` | RouteSurface | 检查 `POST /_matrix/client/v1/sync` |
| `org.matrix.msc3266` | RouteSurface | 检查 `POST /_synapse/room_summary/v1/summaries/batch` |
| `org.matrix.msc3245` | RouteSurface | 检查 `GET /_matrix/client/v3/rooms/{room_id}/summary` |
| `org.matrix.msc3983` | RouteSurface | 检查 `GET /_matrix/client/v1/threads` |
| `org.matrix.msc3814` | RouteSurface | 检查 `GET /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device` |
| `org.matrix.msc4143` | RouteSurface | 检查 `GET /_matrix/client/unstable/org.matrix.msc4143/rtc/transports` |

已从 `/versions` 移除的声明:
- `io.hula.burn_after_read` — 私有扩展，仅在 `/capabilities` 认证面暴露
- `io.hula.friends` — 私有扩展，仅在 `/capabilities` 认证面暴露
- `org.matrix.msc3916` — 实现证据不明，已移除

治理规则:
- MSC identifiers must map to route/service tests or a documented partial-support note.
- Hula extensions must stay out of Matrix stable namespaces and out of the unauthenticated `/versions` surface.
- Experimental performance-sensitive features, especially sync/sliding sync, need benchmark guardrails before default enablement changes.
- `/versions.unstable_features` 与 `/capabilities.unstable_features` 应保持一致（route-surface driven）。

## Admin And Federation Endpoint Support Matrix

### Admin 端点（`/_synapse/admin/v1/*`）

| 端点 | 状态 | 说明 |
| --- | --- | --- |
| `GET /backups` | 501 M_UNRECOGNIZED | 备份由外部工具（docker volume、pg_dump）管理，非进程内职责；返回 501 区分"端点已知但未实现"与"端点未知" |
| `POST /restart` | 501 M_UNRECOGNIZED | 进程重启由进程管理器（systemd/docker）负责；返回 501 区分"端点已知但未实现"与"端点未知" |
| `GET /experimental_features` | 200 OK | 已实现：桥接 DB 型 `FeatureFlagService` 到 Synapse `experimental_features` 表面，返回 `{features: {flag_key: enabled_bool}, total}` |

### Federation 非标准端点（`/_synapse/federation/v1/*`）

| 端点 | 状态 | 说明 |
| --- | --- | --- |
| `POST /user/keys/upload` | M_UNRECOGNIZED | 非标准端点，密钥上传走客户端 `/_matrix/client/v3/keys/upload` |
| `POST /keys/claim` | M_UNRECOGNIZED | legacy 别名，标准端点 `/_matrix/federation/v1/user/keys/claim` 已实现 |
| `POST /keys/query` | M_UNRECOGNIZED | legacy 别名，标准端点 `/_matrix/federation/v1/user/keys/query` 已实现 |
| `POST /keys/upload` | M_UNRECOGNIZED | 同 `/user/keys/upload` |
| `GET /query/auth` | M_UNRECOGNIZED | 非标准端点，标准端点 `/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}` 已实现 |
| `GET /event_auth` | M_NOT_FOUND | 非标准端点，标准端点 `/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}` 已实现 |

### Federation 事件图与回填（`/_matrix/federation/v1/*`）

| 端点 | 状态 | 说明 |
| --- | --- | --- |
| `GET /event/{event_id}` | ✅ 已实现 | 按 event_id 获取单个 PDU |
| `GET /state/{room_id}` | ✅ 已实现 | 返回房间当前状态事件 |
| `GET /state_ids/{room_id}` | ✅ 已实现 | 返回房间当前状态事件 ID |
| `GET /backfill/{room_id}` | ✅ 已实现（入站） | 向其他服务器提供历史事件回填服务 |
| `POST /get_missing_events/{room_id}` | ✅ 已实现 | 入站：通过 `event_edges` 表反向 BFS 遍历事件 DAG，返回 `earliest_events` 与 `latest_events` 之间的事件；出站：`FederationClient::get_missing_events` 在 `/send` transaction handler 的 `fill_in_prev_events` 流程中被调用 |
| 出站 backfill 触发 | ✅ 已实现 | `RoomService::backfill_room_history` 实现完整出站 backfill 链路：通过 `MembershipStorage::get_joined_servers_in_room` 收集候选服务器 → 通过 `EventStorage::get_latest_event_ids_in_room` 获取种子事件 → 调用 `FederationClient::backfill` 向候选服务器请求历史事件 → 通过 `create_event_with_graph` 持久化补回的事件。两个触发入口：`POST /_synapse/admin/v1/rooms/{room_id}/backfill`（手动/测试）和 `/messages` 向后分页 best-effort 异步触发 |

> **影响（已修复）**：OPT-10 核心缺口已全部修复——`EventStorage::create_event_with_graph` 在持久化 PDU 时同时写入 `events.prev_events` / `events.auth_events` / `events.depth` 并填充 `event_edges` 表；`/send` transaction handler 在持久化前检查 `prev_events` 是否缺失，缺失则通过 `get_missing_events` 向源服务器请求补齐；`RoomService::backfill_room_history` 实现完整出站 backfill，通过 `/messages` 向后分页和管理端点两个入口触发，从联邦对端服务器拉取历史事件。

### Federation 出站能力（Outbound Federation）

> 修复"半双工联邦"问题：服务器此前只能接收联邦请求（入站），无法主动发起（出站）。`FederationClient` 有 20+ 出站方法但只有 3 个被实际调用。下表记录当前出站能力状态。

| 出站能力 | 状态 | 说明 |
| --- | --- | --- |
| `backfill` | ✅ 已实现 | `RoomService::backfill_room_history` 通过 `FederationClient::backfill` 向候选服务器请求历史事件；`/messages` 向后分页 + 管理端点两个触发入口；60 秒 per-room cooldown 防止过度请求 |
| `get_missing_events` | ✅ 已实现 | `/send` transaction handler 在 `fill_in_prev_events` 流程中调用 `FederationClient::get_missing_events` 补齐缺失的前置事件 |
| `send_transaction` (EDU) | ✅ 已实现 | `EventBroadcaster` 通过 `FederationClient::send_transaction` 广播 EDU（typing、receipts、device list updates） |
| `query_keys` | ✅ 已实现 | 客户端 `POST /_matrix/client/v3/keys/query` 对远程用户调用 `FederationClient::query_keys`，并行查询多个远程服务器，合并 `device_keys` / `master_keys` / `self_signing_keys` / `user_signing_keys` / `failures`。参考 Synapse `E2eKeysHandler.query_devices` |
| `claim_keys` | ✅ 已实现 | 客户端 `POST /_matrix/client/v3/keys/claim` 对本地未命中的远程设备调用 `FederationClient::claim_keys`，按 server 分组并行请求，合并 one-time keys 结果。参考 Synapse `E2eKeysHandler.claim_keys` |
| `media_download` | ✅ 已实现 | 客户端 `GET /_matrix/media/{v1,v3}/download/{server_name}/{media_id}` 当 `server_name` 非本地时通过 `FederationClient::media_download` 代理远程媒体，复用本地 CSP / 安全头。参考 Synapse `MediaRepositoryServer._download_remote` |
| `media_thumbnail` | ✅ 已实现 | 客户端 `GET /_matrix/media/{v1,v3}/thumbnail/{server_name}/{media_id}` 当 `server_name` 非本地时通过 `FederationClient::media_thumbnail` 代理远程缩略图 |
| `query_profile` | ✅ 已实现 | 客户端 `GET /_matrix/client/v3/profile/{user_id}`（及 `/displayname` / `/avatar_url`）当 `user_id` 属于远程服务器时通过 `FederationClient::query_profile` 代理远程用户资料查询。参考 Synapse `ProfileHandler.get_profile` |
| `send_transaction` (PDU) | ✅ 已实现 | 本地事件（消息、状态变更、成员变更）创建后通过 `RoomService::sign_and_broadcast_event` 签名并广播到远程联邦对端。`sign_and_hash_event` 设置 `origin` / `hashes.sha256` / `signatures`；`update_event_signatures_and_hashes` 持久化签名到 `events` 表；`EventBroadcaster::broadcast_event` 发送 PDU 到所有有 joined member 的远程服务器。集成在 `create_event` wrapper 和 membership actions（join/leave/invite/ban/unban/kick）中 |
| `make_join` / `send_join` | ✅ 已实现 | `RoomService::join_room_via_federation` 实现完整出站联邦加入流程：`make_join` 获取模板 PDU → 本地签名 → `send_join` 发送签名事件 → 创建本地房间记录 → 持久化返回的 state events + auth chain → 添加成员。`join_room_with_via_servers` 自动检测本地/远程房间并委托，`via_servers` 参数从路由处理器传递到联邦 join |
| `make_leave` / `send_leave` | ✅ 已实现 | `RoomService::leave_room_via_federation` 实现出站联邦离开：`make_leave` 获取模板 PDU → 本地签名 → `send_leave` 发送签名事件 → 更新本地成员状态。`leave_room` 自动检测远程房间并委托 |
| `invite` | ✅ 已实现 | `RoomService::invite_user_via_federation` 实现出站联邦邀请：构建 `m.room.member` invite 事件 → 本地签名 → `FederationClient::invite` 发送到被邀请者所在服务器 → 持久化返回的签名事件。`invite_user` 自动检测远程被邀请者并委托 |
| `query_directory` | ✅ 已实现 | 客户端 `GET /_matrix/client/v3/directory/room/{room_alias}` 和 `join_room_by_id_or_alias` 当本地别名查找失败且别名属于远程服务器时，通过 `FederationClient::query_directory` 查询远程房间别名解析 |
| `exchange_third_party_invite` | ✅ 已实现 | 入站：`exchange_third_party_invite` handler 验证事件后用本地服务器密钥签名并返回（房间主服务器签名）。出站：`RoomService::exchange_third_party_invite_via_federation` 调用远程服务器交换第三方邀请，持久化返回的签名事件 |

> **影响**：出站联邦能力已全部修复。"半双工联邦"问题已解决——服务器现在可以主动发起联邦请求。`query_keys` / `claim_keys` 支持跨服务器 E2EE；`media_download` / `media_thumbnail` 支持远程媒体访问；`query_profile` 支持远程用户资料查询；`send_transaction` (PDU) 支持本地事件广播；`make_join` / `send_join` 支持联邦加入；`make_leave` / `send_leave` 支持联邦离开；`invite` 支持联邦邀请；`query_directory` 支持远程别名解析；`exchange_third_party_invite` 支持第三方邀请交换。

### Voice 端点（自定义 Hula 扩展）

| 端点 | 状态 | 说明 |
| --- | --- | --- |
| `POST /voice/{media_id}/convert` | M_UNRECOGNIZED | MSC3245 设计为客户端处理 |
| `POST /voice/{media_id}/optimize` | M_UNRECOGNIZED | 同上 |
| `POST /voice/{media_id}/transcription` | M_UNRECOGNIZED | 同上 |

## OIDC Support Matrix

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| External OIDC (Relying Party) | ✅ 已实现 | 对接外部 IdP（Keycloak/Auth0 等） |
| Builtin OIDC Provider | ✅ 已实现（dev only） | 内置 Provider，仅用于开发测试 |
| MSC2965 `auth_metadata` | ✅ 已实现 | OIDC 未启用时返回 404 + M_UNRECOGNIZED |
| MSC2964 `m.login.oidc` | ✅ 已实现 | auth_compat.rs 中声明 |
| `m.sso.providers` 包含 `oidc` | ✅ 已实现 | sso_providers() 检测 OIDC 配置 |
| Dynamic Client Registration (RFC 7591) | ❌ 不支持 | discovery 中 `registration_endpoint` 为 None |
| Element native OIDC flow | ❌ 不完整 | 缺少动态注册，Element 探测时会降级 |

## Verification

Focused gate for this surface:

```bash
# Unit-level contract/snapshot tests (P1-03.2 + P1-01/T4)
cargo test --lib web::routes::handlers::versions::tests -- --nocapture
```

Broader gates before raising protocol declarations:

```bash
cargo test --test integration api_route_ledger_tests -- --nocapture
cargo test --test integration api_auth_routes_tests -- --nocapture
cargo test --test integration api_protocol_alignment_tests -- --nocapture
```

Contract/snapshot tests (in `src/web/routes/handlers/versions.rs`):
- `test_versions_response_snapshot_keys` — versions 响应键完整性
- `test_capabilities_response_snapshot_public_surface` — 公开能力面
- `test_capabilities_response_snapshot_authenticated_surface` — 认证能力面
- `test_all_capabilities_have_governance_classification` — 治理分类完整性
- `test_no_residual_static_stable_governance` — 无残留 StaticStable
- `test_sso_providers_includes_oidc_when_enabled` — OIDC SSO provider 检测回归

Governance classification summary:

| Capability | Governance | Evidence |
|---|---|---|
| `m.change_password` | RouteSurface | `account_compat::account_compat_route_manifest()` |
| `m.set_displayname` | RouteSurface | 同上 |
| `m.set_avatar_url` | RouteSurface | 同上 |
| `m.3pid_changes` | RouteSurface | 同上 |
| `m.room.summary` | RouteSurface | `room_summary::room_summary_route_manifest()` |
| `m.room.suggested` | RouteSurface | `search::search_route_manifest()` (hierarchy 路由) |
| `m.voice` | RouteSurface | `voip::voip_route_manifest()` (turnServer 路由) |
| `m.thread` | RouteSurface | `thread::thread_route_manifest()` |
| `io.hula.friends` | RouteSurface | `friend_room::friend_route_manifest()` |
| `external_services` | RouteSurface | `external_service::external_service_route_manifest()` |
| `io.hula.voice_extended` | RouteSurface | `voice::voice_route_manifest()` |
| `io.hula.burn_after_read` | RouteSurface | `burn_after_read::burn_after_read_route_manifest()` |
| `m.sso` | ConfigControlled | `sso_providers(config)` (检测 saml/oidc/cas) |
| `openclaw` | ConfigControlled | `openclaw_routes_enabled(config)` |
| `ai_connection` | ConfigControlled | 同上 |
| `m.room_versions` | — | 特殊：由 `client_room_versions_capability()` 生成 |
