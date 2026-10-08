# MSC 编号 — 语义对照表（本项目迭代语义）

> **这份表为什么存在**：`synapse-rust` 与 `matrix-js-sdk` fork 曾对同一批 MSC 编号采用
> **不同语义** —— 后端 Sprint 4 用同一批编号实现了与官方提案不同的功能，导致 fork 侧出现
> 「旧语义孤儿」封装，并在多轮回归审计中反复踩坑。
>
> **依据**：`docs/audit/AUDIT_SUMMARY_2026-09-12.md` §3-3、
> `docs/audit/sdk-encapsulation-audit.md` §8。
>
> **维护规则**：新增或变更任何 MSC 编号用法，**必须同时更新本对照表（§1）或引用登记表（§1.1）**；
> 只写编号不写语义的注释一律视为漂移。门禁 `tests/unit/msc_semantics_guard_tests.rs` 会强制
> `API_COVERAGE_REPORT.md` 中出现的每个 `MSC####` 都能在本文件里找到登记行，否则判红。

## 1. 对照表

「官方标题」列的证据来源标注在括号内：*proposals* = 已从 matrix-spec-proposals 检索确认；
*仓库既有结论* = 仅由本仓 `AGENTS.md` / 代码注释断言，未在本次独立复核官方仓库。

| MSC 编号 | 官方提案标题 | 本项目实际实现 | 后端落点 | SDK fork 对应封装 | 对齐状态 |
|---|---|---|---|---|---|
| **MSC4155** | Invite filtering（*proposals*） | 官方「邀请过滤」语义**已实现**：`m.invite_permission_config` account data（写入校验 + 读取）+ membership 邀请门禁强制；**同时**借用 `org.matrix.msc4155` 号段承载**线程订阅读接口**（unstable 仅作旧客户端兼容，主路径为 `v1/threads/subscribed`）（订正 2026-10-06，原判「未实现」已作废） | 官方语义：`synapse-services/src/invite_blocklist_service.rs`（`INVITE_PERMISSION_CONFIG_TYPE`、`InvitePolicyGate::check_invite_allowed`）、`synapse-services/src/account_data_service.rs:258`（写入校验）、`synapse-services/src/room/membership/service.rs:268`（门禁）；编号借用：`synapse-web/src/routes/handlers/thread.rs:159-162` | `ThreadingManager.getSubscribedThreads()`（走 v1，正常）；`InviteBlocklistManager.get/setInvitePermissionConfig()` 走 `m.invite_permission_config` account data，后端**已消费** | 🟡 编号借用（不影响功能） |
| **MSC4156** | Migrate `server_name` to `via`（*仓库既有结论*，见 `AGENTS.md` MSC number discipline） | join / knock 的 `via` 参数 | `synapse-web/src/routes/handlers/room/members.rs:60,229,722-731` | `RoomManager.joinRoom` / `knockRoom` 发 `via` | ✅ 一致 |
| **MSC4204** | 本次定向检索**未在 matrix-spec-proposals 命中该编号**；该能力的官方提案为 **MSC2457**「Invalidating devices during password modification」（*proposals*） | 改密默认吊销全部设备（`logout_devices` 默认 true） | 后端 Sprint 4 T01 | 既有 `setPassword(auth, pw, logoutDevices?)` | 🟡 编号借用 |
| **MSC4267** | Automatically forgetting rooms on leave（*proposals*） | 原子 leave + forget（单事务） | 后端 Sprint 4 T02 | `RoomManager.leave(roomId, { forget? })` | ✅ 一致 |
| **MSC3967** | Do not require UIA when first uploading cross signing keys（*proposals*） | `/sync` 增量 state token（后端内部优化） | 后端 Sprint 4 T03 | 无需专属封装（正常消费 `/sync`） | 🟡 编号借用 |
| **MSC3083** | Restricted rooms（*proposals*） | `m.room.join_rules` 的 `allow` 数组按 `m.room_membership` 解析 | `synapse-services/src/room/join_rules.rs`（2026-09-13 起为**单一解析器**） | — | ✅ 已收敛 |

### 1.1 引用登记表（`API_COVERAGE_REPORT.md` 全量）

§1 是**语义分歧/编号借用**的详表；本表是**登记**表 —— 只要 `API_COVERAGE_REPORT.md` 里出现
`MSC####`，就必须在本文件（§1 或 §1.1）有一行，否则 `tests/unit/msc_semantics_guard_tests.rs` 判红。

「官方标题」列沿用 §1 的证据约定：`*proposals*` = 已独立检索 matrix-spec-proposals 确认；
`*仓库既有结论*` = 仅由本仓报告/代码断言，本次**未**独立复核官方仓库（**不得**据本列反推官方语义）；
报告自身标「未核对」的项照记为 ⚠️。

| MSC 编号 | 官方口径（证据） | 本项目实际实现 | 落点/证据 | 对齐状态 |
|---|---|---|---|---|
| **MSC2946** | Spaces summary / hierarchy（*仓库既有结论*） | 已实现：`GET /_matrix/client/{v1,v3}/rooms/{room_id}/hierarchy`（`space.rs` 的 `spec` 桶，连同 `hierarchy/v1` 共 4 条）与私有的 `children`/`membership`/`summary` 面分离；后者在 ISSUE-13 第二批迁到 `/_matrix/vendor/v1` 时**保持 hierarchy 原地不动** | `synapse-web/src/routes/space/children_hierarchy.rs`（`create_space_hierarchy_spec_routes`）、`synapse-web/src/routes/space.rs`（`create_space_spec_router`）；报告 §二「空间」行 | ✅ 一致（2026-10-08 ISSUE-13 第二批复核） |
| **MSC3266** | Room summary（*仓库既有结论*） | 已实现**唯一规范形态**：`GET /_matrix/client/v1/rooms/{room_id}/summary`（`create_room_summary_v1_router()`）；同 handler 的 v3 挂载点属重复版本前缀，已随 ISSUE-13 第二批把整个私有面（含 v3 的 `GET summary`）迁到 `/_matrix/vendor/v1` | `synapse-web/src/routes/room_summary.rs`；报告 §二「房间摘要」行 | ✅ 已收敛（2026-10-08 ISSUE-13 第二批） |
| **MSC2965** | 认证元数据端点 `auth_metadata` / `auth_issuer`（*仓库既有结论*） | 仅保留 `auth_metadata`；上游 1.161 已删除的 `auth_issuer` 同批摘除 | `.../org.matrix.msc2965/auth_metadata` 在册；`auth_issuer` 于 `76e5f9136` 删除（报告 §5.1 / B4） | ✅ 已收敛 |
| **MSC3575** | Sliding sync（*仓库既有结论*） | 在 `/sync` 之外另注册 simplified sliding sync 端点族 | 报告 §二「同步」行 | ✅ 一致 |
| **MSC3720** | Account status endpoint（*proposals*：`proposals/3720-account-status.md`） | 已实现：客户端 + 联邦 `POST /_matrix/{client,federation}/unstable/org.matrix.msc3720/account_status`，`{user_ids} → {account_statuses, failures}`；capability `org.matrix.msc3720.account_status` 为「路由 ∧ `experimental.msc3720_enabled`」，开关关闭时 fail-closed 403 | `synapse-web/src/routes/handlers/account_status.rs`、`synapse-services/src/account_status_service.rs`（报告 §1.1 末段） | ✅ 已对齐 |
| **MSC3814** | 脱水设备（dehydrated devices）（*仓库既有结论*） | `dehydrated_device` 端点族；`/events` 由 POST 改 GET + query | `GET .../org.matrix.msc3814.v1/dehydrated_device/{device_id}/events`（报告 §5.1 / B3） | ✅ 已对齐（2026-09-25） |
| **MSC3861** | 实验性 auth delegation（*仓库既有结论*） | 不实现；以 MAS 稳定集成为准 | `synapse-services/src/auth/mas_validator.rs`（报告 §5.1） | ⚪ N/A |
| **MSC3866** | Admin `GET /_synapse/admin/v2/users` 未启用时省略 approval 标记（*仓库既有结论*） | 未核对 | —（报告 §5.1 标「未核对」） | ⚠️ 未核对 |
| **MSC3882** | QR code login（*仓库既有结论*） | 已实现（报告 §八 认证配方里作 `msc3882` 出现） | 报告 §八 认证配方（`/account/password`、`/account/deactivate`、`/account/3pid` 等，含 `msc2965` / `msc4108` / `msc3882`） | ✅ 一致 |
| **MSC3912** | 关系性（级联）撤回（*仓库既有结论*） | 格式已修（v11+ 写 `content.redacts`）+ 管理端级联；**客户端撤回路径在 `with_rel_types` 非空时同样级联**（单层级联，不递归、不触碰父事件；订正 2026-10-04） | `synapse-storage/src/event/cascade.rs`、`synapse-services/src/event_redaction_service.rs:58`、`synapse-web/src/routes/handlers/room/events.rs:1134-1169`（客户端撤回）、`POST /_synapse/admin/v1/rooms/{room_id}/cascade_redact`（管理端；报告 §5.2 / B8） | ✅ 已对齐（2026-10-04 订正） |
| **MSC4108** | rendezvous 登录（*仓库既有结论*） | 已实现 | 报告 §二「认证」行 | ✅ 一致 |
| **MSC4133** | Extended profile（*仓库既有结论*） | 非对象 body 由 500 改为 400 | 报告 §7-B7（2026-09） | ✅ 已完成 |
| **MSC4140** | Delayed events（*仓库既有结论*） | 单事件端点 + 联邦 EDU 已实现；**schedule 的 `state_key` 已补齐（2026-10-04）**：`/send` 保持 `None`，`/state` 三个写端点按路径 / computed `state_key` 透传，后台 dispatcher 既有的 `state_key` 分支据此走状态事件路径（`create_event`），不再全部落到消息路径 | `synapse-web/src/routes/handlers/room/mod.rs`（`schedule_delayed_event_if_requested`）、`.../handlers/room/state.rs`、`src/server/mod.rs:837-871`；测试 `tests/integration/api_delayed_state_event_tests.rs` | ✅ 已实现（2026-10-04） |
| **MSC4178** | 3PID `requestToken` 非法邮箱/国家码返回 `M_INVALID_PARAM`（*仓库既有结论*） | 未核对 | —（报告 §5.1 标「未核对」） | ⚠️ 未核对 |
| **MSC4222** | Adding `state_after` to `/sync`（*proposals*：`proposals/4222-sync-v2-state-after.md`） | ✅ **已按规范实现（2026-10-01）**：接受 `?use_state_after=true` 与不稳定拼写 `org.matrix.msc4222.use_state_after`；opt-in 后房间段**省略 `state`**、改为 `state_after`（不稳定拼写镜像为 `org.matrix.msc4222.state_after`），**字段即使为空也返回**；内容 = 本次 timeline **末尾**之前的状态变化（本仓状态窗口本就是 `stream_ordering > since` 上界无界，故无需第二次查询）；未 opt-in 的默认路径逐字不变。**原实现的 `?state_after=<event_id>` + 左房时间戳过滤是编号借用，已删除** | `synapse-web/src/routes/handlers/sync.rs`（`resolve_use_state_after`）、`synapse-services/src/sync_service/response.rs`（`build_room_sync_value`） | ✅ 一致（证据：4 条单测 + 2 条解析单测 + 默认路径快照无 diff） |
| **MSC4239** | Matrix v1.14 / 房间版本 11 的发布 MSC（*仓库既有结论*） | 默认房间版本已越过 v11、推进到 v12 | `synapse-common/src/room_versions.rs:94` `DEFAULT_ROOM_VERSION = "12"`（报告 §5.1） | ✅ 已越过 |
| **MSC4242** | **State DAGs**（*proposals*：Open / In Review，`requires-room-version`，**尚无房间版本指派**；作者 kegsay） | 仅存储层；`dag.rs` 不实注释**已修正（2026-10-02）**（如实陈述：该函数无生产调用点、仅 `db_tests` 覆盖）。上游 #20133 的 serving 是把 MSC4242 接进**既有**联邦端点（`/make_join`、`/send_join`、`/get_missing_events` 的状态 DAG 回溯、`/send` 目的地按 `prev_state_events` 计算），**不新增路由**；受阻于 MSC4242 房间版本 + `experimental_features` opt-in | 报告 §5.1 / C6、§18.4 L-2 | 🟡 PARTIAL（观察项） |
| **MSC4262** | **Sliding Sync Extension: Profile Updates**（*proposals*：扩展名 **`profiles`**；响应 `extensions.profiles.users.{uid}` = `{updated, removed}`，用户离开全部共享房间时值为 `null`；`fields` 数组 opt-in，**省略＝所有 profile 字段在范围内**） | **编号借用 + 形状漂移**：本仓扩展名写作 `profile_updates`、形状为 `{users:{uid:{displayname,avatar_url,updated_ts}}}`，**无** `fields` / `removed` / `null` 语义；官方 `profiles` 扩展**未实现** | `synapse-services/src/sliding_sync_service/extensions.rs`、`synapse-services/src/user_service.rs`（`m.profile_update` EDU 广播）（报告 §5.1 / C2） | 🟡 编号借用 + 形状漂移 |
| **MSC4291** | Room IDs as hashes of the create event（*proposals*：`proposals/4291-room-ids-as-hashes.md`） | v12+ 的 room_id 由 create 事件的 reference hash 派生（`room_id_from_create_event_id`）；v12 是唯一可创建版本（v1–v11 不可创建、v13 已移除） | `synapse-services/src/room/lifecycle/create.rs`、`synapse-common/src/room_versions.rs`（报告 §5.4 #19768） | ✅ 已落地（仅 v12） |
| **MSC4297** | State resolution v2.1（*仓库既有结论*） | **已实现并接线到生产路径（2026-10-02 订正，原判「未实现」已作废）**：分叉时 `resolve_forked_state` → `StateWalker::resolve` → `resolve_state_for_version_with_rules`；v12+ 以空 state map 起步（＝v2.1 Modification 1），v1–v11 沿用 unconflicted map（＝v2）；写入接缝在 `create_event_with_graph` 与联邦加入 state 批次，分叉时重算并回写 state group；v12 **不再**「声明领先实现」 | `synapse-services/src/room/state_record.rs:192,212,471`、`synapse-federation/src/event_auth/state_resolution.rs`（报告 §5.1） | ✅ 已实现 |
| **MSC4304** | Matrix v1.15 / 房间版本 12 的发布 MSC（*仓库既有结论*） | 仅 v12 可创建；v1–v11 不可创建、v13 已移除 | `room_versions.rs:151` `stable("12")`（报告 §5.1 / B5） | ✅ 已落地 |
| **MSC4311** | Use full PDU's in stripped state (like `invite_room_state`) over federation and always include `m.room.create`（*proposals*；标题引自上游 PR element-hq/synapse#19723） | 上游 v1.162 把严格校验推迟到 **2027-06-01**；本仓以配置开关 `msc4311_strict_validation`（默认 `false` ＝宽限期内宽松）承载，联邦侧发全量 PDU | `synapse-common/src/config/federation.rs:228-245`、`synapse-services/src/room/membership/federation.rs:653`（报告 §5.4 #19723） | ✅ 以配置开关承载（宽限至 2027-06-01） |
| **MSC4326** | Device masquerading for appservices（*proposals*；该提案引入 unstable `ORG.MATRIX.MSC4326.M_UNKNOWN_DEVICE`） | 稳定码 `M_UNKNOWN_DEVICE` **已定义**（`synapse-common/src/error/code.rs:99,149,255`），不再使用 MSC4326 unstable 前缀。⚠️ **但没有任何端点使用它**：本仓不实现 appservice device masquerading，而设备 CRUD 的 404 按规范是 `M_NOT_FOUND`（`device_management.yaml` 全文件无 `M_UNKNOWN_DEVICE`）⇒ `synapse-web/src/routes/device.rs::device_not_found_error()` 返回 `M_NOT_FOUND`（2026-10-02 更正，详见报告 §18.7.1 P-11） | `synapse-common/src/error/code.rs:99,149,255`、`synapse-web/src/routes/device.rs`（报告 §5.4 #20181） | ⚠️ 码已稳定化但**无适用端点**（本仓无 masquerading） |
| **MSC4335** | 媒体上传超限返回 `M_USER_LIMIT_EXCEEDED`（*仓库既有结论*） | 错误码已定义，但**未见**媒体上传限额路径使用 | `synapse-common/src/error/code.rs:83,131,225,292`（报告 §5.1） | 🟡 PARTIAL |
| **MSC4354** | Sticky Events（*proposals*：matrix-org/matrix-spec-proposals#4354） | 本仓**无 sticky soft-fail 维度**：sticky 仅以 `room_sticky_events.is_sticky` 落库 + 联邦 EDU，不做状态相关 auth 评估、无 `un_soft_fail` 重算 ⇒ 无可「取消」的对象（⚠️ `events.soft_failed` 是 B-8 事务去重专用，**同名不同义**）。**2026-10-08 M2：路由前缀已归位** —— 从借用的稳定 `v3` 移到 `/_matrix/client/unstable/org.matrix.msc4354/rooms/{room_id}/sticky_events*`（号段依据三处一致：联邦 EDU 名 `org.matrix.msc4354.sticky_event`、SDK 字段 `msc4354_sticky_key`、`sticky_event.rs` 头注） | `synapse-storage/src/sticky_event.rs`、`synapse-services/src/room/service.rs:661-684,739-799`、`synapse-web/src/routes/room.rs`（`create_room_msc4354_router`）（报告 §5.4 #20204 / §5.6 M5） | 🟡 PARTIAL（无 soft-fail 维度；路由已归位 unstable） |
| **MSC3981** | `/relations` 递归（*proposals*：matrix-org/matrix-spec-proposals#3981；**spec v1.10 起稳定**） | `recurse` 与不稳定名 `org.matrix.msc3981.recurse` 均已接线，`/versions` 广告 `org.matrix.msc3981`(`.stable`)；递归在存储层用**静态递归 CTE**（`events` 在递归内 join），排序/分页键是 `events.stream_ordering`（拓扑序，与同 `dir` 的 `/messages` 一致），深度口径与上游一致（0 基 `depth <= 3`，上报 `recursion_depth = 3`）。⚠️ **过滤语义跟随参考实现而非 MSC 正文**：正文说 `rel_type`/`event_type` 过滤同时剪枝中间节点，但 MSC 自己的第 5 个示例与之自相矛盾（G 需同时满足 `m.thread` 与 `m.annotation`），Synapse 的 CTE 是先递归、后过滤返回集，本仓取后者（理由见对照报告 §18.3 #14）。`event_type` 过滤的 **HTTP 入口已补齐（2026-10-02，L-6）**：spec 的 `GET …/{relType}/{eventType}` 4 段路由已新增（与同路径既有 `PUT` 合并进同一 `MethodRouter`）并接通过滤，见对照报告 §18.4 L-6 | `synapse-storage/src/relations/mod.rs`（`MSC3981_RECURSION_DEPTH`）、`synapse-services/src/relations_service.rs`、`synapse-web/src/routes/relations.rs`、`synapse-services/src/capability_governance.rs` | ✅ 已实现（含 `event_type` 路由） |
| **MSC4429** | **Profile Updates for Legacy Sync**（*proposals*：legacy `/sync` 新增**顶层 `users` 对象**承载 per-user `profile_updates`（`updated`/`removed`）；经**新 filter 字段 `profile_fields.ids`** opt-in，**省略＝无字段在范围内**） | **legacy 半边未实现**：本仓仅在 sliding sync 侧实现 `profile_updates`，legacy `/sync` **不输出**顶层 `users` 对象、**无** `profile_fields` filter；`msc4429` 无独立落点（仅与 MSC4262 同行命中） | 同 MSC4262（报告 §5.1 / C2） | 🟡 PARTIAL（legacy 半边未实现） |
| **MSC4502** | **Targeted and unrestricted room member queries**（*proposals*：新 CS 端点 **`GET /_matrix/client/v3/rooms/{roomId}/is_joined`**，query 恰取 `mxid` **或** `server_name` 之一；响应 `{"joined": bool}`；由 **OAuth scope** `urn:matrix:client:rooms:is_joined` 保护，appservice 注册文件新增顶层 `scopes`） | **编号借用**：官方 `is_joined` 端点 / appservice `scopes` / OAuth scope **均未实现**（全仓无 `is_joined`、无 `io.element.msc4502`）；本仓把该编号借给 `/members` 的 `at`/`dir`/`limit`/`membership`/`not_membership` 分页 | `synapse-web/src/routes/handlers/room/members.rs`、`synapse-storage/src/membership/mod.rs`（报告 §5.1 / C2） | 🟡 编号借用 |
| **MSC4512** | App Service 命名空间代理 / 联邦请求（*仓库既有结论*） | 代理已实现；联邦侧代理请求未做 | `synapse-web/src/routes/app_service.rs:722-723` + handler `proxy_to_as`（报告 §5.1 / §六） | ✅ 代理已实现 |
| **MSC3856** | Threads API（*仓库既有结论*） | 房间级线程**读面**按 MSC3856 形状保留在 `/_matrix/client/v1`：`GET /rooms/{room_id}/threads`、`GET .../threads/{thread_id}`、`.../threads/{thread_id}/replies`（GET/POST）、`.../subscribe`、`.../unsubscribe`、`.../unfreeze`。本仓**额外**的私有线程面（创建/删除/搜索/未读/冻结/静音/已读/统计、全局列表与创建、用户级列表、非规范 redact 形状）**不是** MSC3856 形状 —— 已随 ISSUE-13 **第三批（2026-10-08）**迁到 `/_matrix/vendor/v1`，**不留 client 别名** | `synapse-web/src/routes/handlers/thread.rs`（`create_thread_routes` 拆出的 `vendor_routes` / `client_routes` 两个子 router）；报告 §1.1 v1.19 增量、`docs/后端冗余清除与功能完善优化方案-2026-10-08.md` §3 Batch 3 | ✅ 已收敛（2026-10-08 ISSUE-13 第三批） |

> 另有三项（**MSC4155** / **MSC4204** / **MSC3967**）已在 §1 登记，此处不重复。

## 2. 已知「旧语义孤儿」清单（后端零消费，保留为草案）

这些 API 在 fork 侧按**官方** MSC 语义实现，但本后端不实现对应能力，调用不会产生服务端效果。
保留是为了不破坏已发布的公开面，**不代表后端支持**。

| SDK 符号 | 位置 | 后端证据 | 处理 |
|---|---|---|---|
| `PolicyRecommendation.Takedown = "m.takedown"` | `matrix-js-sdk/src/models/invites-ignorer-types.ts:39` | 全仓 grep `m.takedown` / `takedown` = **0** | 标注为草案（JSDoc） |

> **已移出本清单（2026-10-06）**：`InviteBlocklistManager.getInvitePermissionConfig()` /
> `setInvitePermissionConfig()` 曾因后端零消费列为孤儿。现 `m.invite_permission_config`
> 已被后端消费 —— 写入校验见 `synapse-services/src/account_data_service.rs:258`，
> 邀请门禁见 `synapse-services/src/room/membership/service.rs:268` —— 故不再属于孤儿，
> 详见 §1 MSC4155 行。

## 2.1 请求体兼容分支裁定（by-design，2026-10-08）

方案 `docs/后端冗余清除与功能完善优化方案-2026-10-08.md` §2.5 把两处"请求体兼容分支"
列为**待裁定**（不直接删）。本轮取证后的裁定如下。

| # | 位置 | 裁定 | 依据 |
|---|---|---|---|
| 1 | `synapse-e2ee/src/device_keys/service.rs` 的 `body.get("signatures").cloned().unwrap_or(body)` | ✅ **已删除** | 规范请求体**直接就是**签名映射（无外层包装）。消费者取证为零：SDK 的 body 由 rust-crypto 的 `SignatureUploadRequest` 直接给出裸映射；后端单测（`device_keys/service.rs::upload_signatures_*`）与集成测试（`tests/integration/api_e2ee_advanced_tests.rs:403`）**都发裸映射**。对裸映射输入行为**等价**（裸映射不含顶层 `signatures` 键 ⇒ 原 `unwrap_or` 本就取整个 body） |
| 2 | `synapse-web/src/routes/reactions.rs` 的 `body.get("body")`（annotation key 回退） | ⏸ **保留（by-design）** | 规范形态是 `m.relates_to.key`，SDK 生产侧已发该形态（`matrix-js-sdk/src/room-events/index.ts:144-149`）。但**回退读的顶层 `body` 与输出侧保留的 `content.body` 是配套设计** —— `tests/integration/api_msc3912_redaction_cascade_tests.rs:373` 明确断言写入的 `m.reaction` 事件含 `"body": "👍"`；且后端集成测试 `tests/integration/api_relations_authorization_tests.rs:143` 仍按该形态发请求。删输入侧会让它比输出侧"更严格"，属**行为变更而非清理**，需连同输出侧与测试一起裁定 |

> **判据重申**（铁律 1）：兼容分支的唯一存在理由是"有调用方"。第 1 项无任何调用方
> （测试、SDK、Tjg 三方取证均为裸映射）故删；第 2 项仍有测试与输出侧消费者故留。

## 2.2 MSC3946 归属澄清（2026-10-08，M2 取证副产品）

`docs/audit/MIXED_MODULE_PRIVATE_ROUTES_PLAN_2026-10-08.md` 初稿把本仓的
`GET/POST /rooms/{room_id}/pinned_events` 标为 **MSC3946**，并据此准备迁到
`/_matrix/client/unstable/org.matrix.msc3946/...`。**执行前取证发现该断言不成立**：

- SDK 的事件类型常量写作 `EventType.RoomPredecessor = "org.matrix.msc3946.room_predecessor"`
  （`matrix-js-sdk/src/@types/event.ts:100`）⇒ 该号段属**房间前驱**语义，与 pinning 无关；
- 本仓与 SDK 的 pinned events 用的是**稳定的** `m.room.pinned_events` 状态事件类型
  （`@types/event.ts:94`），而 `GET/POST /rooms/{room_id}/pinned_events` 这组**端点**
  在本仓没有可用的 MSC 归属证据。

⇒ 处置：**不迁 unstable**，按"私有扩展"归位 `/_matrix/vendor/v1`（见方案 §2 类别 B2）。
⚠️ 未联网核对提案原文；若日后确认 MSC3946 确含 pinning 语义，再改判。

## 3. MSC3083 `allow` 解析收敛记录（2026-09-13）

同一份 `m.room.join_rules.allow` 数组此前由两处以**不同语义**解析：

| 调用点 | 旧语义 |
|---|---|
| `room::membership::service`（鉴权门） | `type == m.room_membership` 过滤 + room_id 语法校验 + 去重排序 |
| `room::summary::service`（`/summary` 的 `allowed_room_ids`） | 任意含字符串 `room_id` 的条目 + 保留声明顺序 |

现统一到 `room::join_rules::extract_allowed_join_rooms`，`/summary` 仅额外加 join_rule 门。
**行为变化**：`/summary` 的 `allowed_room_ids` 现在会过滤非 membership 条目、丢弃非法 room_id、
去重并按字典序排序 —— 即与鉴权门**永远给出同一个答案**，且输出确定性。

## 4. 复核命令

```bash
# 后端：两个调用点必须只依赖同一解析器
grep -rn "extract_allowed_join_rooms\|extract_allowed_room_ids" synapse-services/src/room/

# 后端：孤儿语义确认
# m.takedown 应为 0（本仓不实现 takedown 语义）
grep -rn "m\.takedown\|takedown" --include=*.rs src synapse-services synapse-common
# invite_permission_config 应 > 0（MSC4155 官方语义已实现：写入校验 + 邀请门禁，2026-10-06 订正）
grep -rn "invite_permission_config" --include=*.rs .

# 收敛性单测（鉴权解析器与 /summary 投影必须一致）
cargo nextest run -p synapse-services -P tdd --features test-utils summary_projection_agrees
```
