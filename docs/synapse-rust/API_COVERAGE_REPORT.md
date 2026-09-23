# synapse-rust API 覆盖率分析 (v1.4)

> **对齐基准**：element-hq/synapse **v1.161.0**（发布于 2026-09-15，当前最新稳定版）；上游 `CHANGES.md` 已核对 1.157→1.161 全部条目。
> Matrix Specification 基线：**v1.19**（上游 v1.161 release notes 引用 `spec.matrix.org/v1.19`）。
> **本次复核日期**：2026-09-22；复核时仓库 HEAD 为 `008d610d`（`git log -1`）。
>
> **权威来源声明（三条，冲突时按此优先级）**：
> 1. **机器权威（路由）**：[`ROUTE_CONTRACT.md`](./ROUTE_CONTRACT.md) —— 由 `scripts/contract/extract_registered.py` 从真实 `.route()` 注册面抽取，生成于 2026-09-21，
>    并与 `derived_routes.rs` 派生表 + `tests/unit/fixtures/ledger_export/*.json` 双向对账（两份独立事实来源，差额必须为 0）。
> 2. **语义权威（MSC 编号）**：[`MSC_SEMANTICS.md`](./MSC_SEMANTICS.md) —— 本仓存在**借用 MSC 编号**承载非官方语义的情况（MSC4155 / MSC4204 / MSC3967），
>    按编号推断语义前必须先查表。
> 3. **人工权威（本文档）**：覆盖率分析。**与 1/2 冲突时以 1/2 为准。**
>
> ⚠️ **本文档为人工维护，不构成事实来源。** 凡本文档出现的数字，均标注其口径与可复现命令（见 §8）；
> 无法机器复核的（如上游文档章节端点计数）显式标注 `[人工口径·未机器复核]`。

---

## 一、路由实测口径（机器可复现）

### 1.1 三种口径，不可混用

`ROUTE_CONTRACT.md` 的抽取器已递归应用 `.nest()` 前缀，因此条目是客户端可直接拼接的**绝对路径**。
同一路径常因**多方法注册**（如 `/rooms/{room_id}/summary` 同时挂 GET/POST）与**多版本前缀**
（`v3` / `r0` / `v1` / `unstable/org.matrix.*`）而膨胀为多条。
故本文档统一给出三种口径，**任何表格都不得把不同口径的数字相加或相除**：

| 口径 | 含义 | 全部 | `/_matrix/client` | `/_synapse/admin` | 其他命名空间 |
|---|---|---|---|---|---|
| **注册条目** | 唯一 `(method, absolute_path)` 对 | **1151** | 658 | 277 | 216 |
| **唯一路径** | 去掉方法后的唯一 `absolute_path` | **931** | 519 | 218 | 194 |
| **逻辑端点** | 在上者基础上折叠版本前缀（`v3`/`r0`/`v1`/`unstable/*` → `vX`）后的唯一路径 | **811** | 401 | 216 | 194 |

- `ROUTE_CONTRACT.md` 总览的 **1151** 与上表「注册条目」一致（该清单本身无 `(method,path)` 重复），
  66 个含路由注册的模块文件 / 74 个 `registered_by` 标签同为其总览数字。
- 1151 → 931 的差额**不是漂移**，而是"同路径多方法"（如 `summary` 4 个方法）；
  931 → 811 的差额是"同路径多版本前缀"。
- 「其他命名空间」194（唯一路径）= `/_matrix/`（非 client，如 federation/key）142 + `/_synapse/`（非 admin）35 + `/.well-known/` 5 + 根级非命名空间 12。
  其中根级端点按 `ROUTE_CONTRACT.md` §前缀之外 为 **14 条有意注册**（3 条探活 + 11 条 CAS 根协议），该桶由
  `test_extract_registered.py::check_non_namespace_bucket` 守卫钉死（出现新成员即转红）。

> 🚨 **对历史版本的纠正**：v1.3 及更早版本声称"HEAD 真实注册路由条目约 **883**"、"Client ~237 / Admin ~174 / 总计 ~411"。
> 883 与 237/174 均**无机器来源**，且与 2026-09-21 的 `ROUTE_CONTRACT.md` 不符。本版一律改为上表实测值。
> （同批次缺陷亦记录在 [`docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md`](../audit/COMPARISON_REPORT_REVIEW_2026-09-22.md) §2/§3。）

### 1.2 分类归属规则

下表按**有序优先级规则**把每条路由唯一归属到一类（因此各类可相加 = 该命名空间总数）。规则见 §8 的 `classify_routes.py`。
关键归属约定：`/rooms/{id}/…` 的**全部**子资源（含 `send` / `messages` / `state` / `tags` / `account_data` / `receipt`）归**房间**，
仅 `/sendToDevice`、`msc4140`、`/rooms/{id}/event/…` 归**消息**；`/users/{id}/media` 归**用户管理**而非媒体。

---

## 二、Client API 分类统计（机器口径，synapse-rust 实测）

| 类别 | 逻辑端点 | 唯一路径 | 注册条目 | 说明 |
|------|---------:|--------:|--------:|------|
| **房间** | 206 | 250 | 302 | 含 join/knock/leave/invite/state/tags/relations/threads/summary/spaces |
| **设备与密钥** | 71 | 113 | 154 | `/devices`、`/keys/*`、`/room_keys/*`、`cross_signing`、`device_verification`、`dehydrated_device`(MSC3814) |
| **认证** | 44 | 60 | 71 | login/logout/register/refresh/oidc/saml/cas/rendezvous(MSC4108)/account(password·3pid·deactivate)/MSC2965 |
| **用户** | 24 | 28 | 47 | profile/presence/user_directory/thirdparty/capabilities/account_data |
| **同步** | 20 | 24 | 31 | `/sync`、`notifications`、MSC3575 + simplified MSC3575、pushrules/pushers/push、to_device |
| **消息** | 20 | 24 | 31 | `sendToDevice`、MSC4140 delayed_events、`/rooms/{id}/event/…` |
| **搜索** | 8 | 10 | 12 | `/search` |
| **媒体** | 8 | 10 | 10 | `/media/*`、`/upload`、`thumbnail`、`preview_url` |
| **合计** | **401** | **519** | **658** | — |

## 三、Admin API 分类统计（机器口径，synapse-rust 实测）

| 类别 | 逻辑端点 | 唯一路径 | 注册条目 | 说明 |
|------|---------:|--------:|--------:|------|
| **房间管理** | 50 | 50 | 59 | rooms/retention/purge_room/purge_history/shutdown_room/spaces/room_stats/statistics/server_notices/jitsi/cleanup |
| **用户管理** | 48 | 50 | 68 | users/user_sessions/registration_tokens/register/account_validity/whois/whoami/account/invite |
| **安全** | 46 | 46 | 58 | event_reports(16)/reports/policy/audit/feature-flags/experimental_features/background_updates(17) |
| **服务器** | 45 | 45 | 60 | 未被前四类命中的 server/version/rate-limit-status/modules/appservices/telemetry/saml/cas/external_services 等 |
| **联邦** | 20 | 20 | 23 | `/federation`、`destinations` |
| **媒体** | 7 | 7 | 9 | `/media*`、`quarantine_media`、`purge_media_cache`、`media_callbacks` |
| **合计** | **216** | **218** | **277** | — |

> 「逻辑端点」低于「唯一路径」是因为 `/v1/*` 与 `/v2/*` 折回同一 `vX` 路径时的合并（Admin 侧表现在 `users` 族与 `rooms` 族）。

---

## 四、与上游逻辑端点对照（上游列为人工口径，仅供参考）

> ⚠️ **口径警告**：下表"上游 Synapse"列**不是**机器抽取结果，而是按 Synapse 官方文档（Client-Server API / Admin API）
> 章节端点索引做的**人工估计**，标注为 `[人工口径·未机器复核]`。由于上游口径是"文档章节端点"而本仓列是"折叠版本前缀后的注册路径"，
> **两侧覆盖率百分比只反映趋势，不构成精确结论**。要得到可辩护的覆盖率，必须把上游端点也用机器抽取（见 §7 优化建议）。
> 本节保留该表仅因历史延续性；**权威口径以 §二/§三 与 `ROUTE_CONTRACT.md` 为准**。

### Client API

| 类别 | synapse-rust（逻辑端点） | 上游 Synapse `[人工口径]` | 参考覆盖率 |
|------|------------------------:|-------------------------:|-----------:|
| 认证 | 44 | 35 | >100%（本仓含 CAS/SAML/OIDC/MSC2965 等非 C-S 标准项） |
| 房间 | 206 | 50 | 口径不可比 |
| 消息 | 20 | 40 | 口径不可比 |
| 媒体 | 8 | 20 | 口径不可比 |
| 用户 | 24 | 25 | ≈96% |
| 设备 | 71 | 18 | 口径不可比 |
| 同步 | 20 | 15 | 口径不可比 |
| 搜索 | 8 | 10 | 80% |

### Admin API

| 类别 | synapse-rust（逻辑端点） | 上游 Synapse `[人工口径]` | 参考覆盖率 |
|------|------------------------:|-------------------------:|-----------:|
| 用户管理 | 48 | 27 | 口径不可比 |
| 房间管理 | 50 | 33 | 口径不可比 |
| 服务器 | 45 | 17 | 口径不可比 |
| 媒体 | 7 | 18 | 39% ← **真实差距**（本仓 admin 媒体端点偏少，见 §6.3） |
| 联邦 | 20 | 14 | 口径不可比 |
| 安全 | 46 | 10 | 口径不可比 |

**结论**：v1.3 表格中"认证 34/35（97%）""房间 46/50（92%）"等百分比，源于两个**互相不可比的口径**相除，
且本仓侧数字（34/46/38/18/23/17/13/8）已无机器来源。本版不再给出统一覆盖率百分比。

---

## 五、上游 v1.157 → v1.161 增量逐条判定（本轮新增）

> 依据：`element-hq/synapse` 的 `v1.161.0` 标签 `CHANGES.md`（`gh api repos/element-hq/synapse/contents/CHANGES.md?ref=v1.161.0`）。
> 判定口径：**TRUE** = 代码支撑；**PARTIAL** = 部分成立/语义不符；**MISSING** = 不存在；**N/A** = 本仓无对应结构。

### 5.1 协议与端点（影响 API 契约）

| 上游条目 | 版本 | 本仓实测 | 证据 |
|---|---|---|---|
| **默认房间版本改为 11**（MSC4239，Matrix v1.14） | 1.158 | **TRUE（一致）** | `synapse-common/src/room_versions.rs:89` `DEFAULT_ROOM_VERSION = "11"` |
| v12/v13 房间可创建 | 1.158 | **MISSING** | 同上 `:114-115` 标为 `stable_parse_only`（可 parse/join/federate，**不可创建**）；联邦中存在 v12 房间 |
| 缩略图动画支持（`animated` 查询参数） | 1.158 | **MISSING** | `animated` 在 `synapse-web/src/routes/` 与 `synapse-services/` 中均 **0 命中** |
| MSC4335 媒体上传超限返回 `M_USER_LIMIT_EXCEEDED` | 1.158 | **PARTIAL** | 错误码已定义（`synapse-common/src/error/code.rs:83,131,225,292`），但**未见**媒体上传限额路径使用它（`synapse-services/` 0 命中） |
| **MSC3814 脱水设备 `/events` 端点由 POST 改为 GET + query** | 1.157 | **PARTIAL（方法漂移）** | 本仓仅注册 `POST /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/events`，`next_batch` 从 **body** 读取 |
| **删除 `GET /_matrix/client/unstable/org.matrix.msc2965/auth_issuer`** | 1.161 | **反向：本仓仍注册（多余端点）** | `/_matrix/client/unstable/org.matrix.msc2965/auth_issuer` 仍在册（`/tmp/paths.txt` 可复现）；上游 1.161 #20163 已删除 |
| **MSC4140 新增"获取单个延迟事件"端点** | 1.161 | **PARTIAL** | 本仓有 `/_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`；缺联邦 EDU（`synapse-federation/src/edu.rs:15-35` 无 delayed-event 变体，`m.delayed_event` 全仓 0 命中） |
| **MSC4502 定向房间成员查询** | 1.160 | **PARTIAL** | `msc4502` 命中 8 个 `.rs` 文件（`room/membership/{mod,service}.rs`、`handlers/room/members.rs`、`sync_service/*`、`membership/api.rs`） |
| **MSC4262 / MSC4429 Profile 更新进 sync** | 1.159/1.160 | **PARTIAL** | `msc4262\|msc4429` 命中 8 个 `.rs` 文件（`user_service.rs`、`user/storage.rs`、`sliding_sync_service/extensions.rs`、`federation/edu.rs` 等） |
| **MSC4512 App Service 命名空间代理 / 联邦请求** | 1.161 | **MISSING** | `msc4512` 在 `*.rs` 中 **0 命中** |
| MSC3861 实验性 auth delegation 移除 | 1.157 | **N/A** | 本仓以 MAS 稳定集成为准（`synapse-services/src/auth/mas_validator.rs`） |

### 5.2 安全 / 限流 / 错误码

| 上游条目 | 版本 | 本仓实测 | 证据 |
|---|---|---|---|
| **`rc_reports` 限流应用于房间举报端点** | 1.161 | **MISSING** | 全仓 `grep -rn 'rc_reports'` = 0；`directory_reporting.rs` 举报 handler 只吃通用默认桶 |
| `M_APPSERVICE_LOGIN_UNSUPPORTED`（Matrix 1.17 稳定码） | 1.161 | **MISSING（根因：AS 登录整体缺失）** | 无 `m.login.application_service`、无 `MSC4190`；登录类型仅 password/token/sso/cas/oidc/dummy |
| MSC4178 3PID `requestToken` 非法邮箱/国家码返回 `M_INVALID_PARAM` | 1.161 | **未核对** | — |
| MSC3866：`GET /_synapse/admin/v2/users` 在未启用时省略 approval 标记 | 1.161 | **未核对** | — |
| Profile 自定义字段：PUT/DELETE 返回 403 + `M_FORBIDDEN` | 1.161 | **已实现（另一触发路径）** | `account_compat.rs:195-197,224-226`、`extended_profile.rs:121-123,153-155` |
| Profile 不存在用户写自定义字段返回 404 而非 500 | 1.161 | **PARTIAL（与上游相反）** | `user/storage.rs:663-673` 的 `user_exists` 过滤 `is_deactivated = FALSE` → 对**已停用但存在**用户返回 404，上游要求成功 |
| MSC4222 `/sync` 左房 `state_after` 成员泄漏修复 | 1.161 | **N/A** | 全仓 `state_after` / `MSC4222` = 0，本仓无该实现，故该 bug 不适用 |
| MSC3912 关系性撤回（room version > 10 时 `redacts` 置入 `content`） | 1.161 | **MISSING** | 撤回事件 `content` 只有 `{"reason":…}`，目标 id 仍写**顶层** `redacts`；而本仓**默认创建 v11 房间** → 与合规 v11 消费方互操作风险 |
| MSC4242 State DAG（联邦客户端 + 存储） | 1.161 | **PARTIAL（仅存储层）** | `dag.rs` 注释声称被 `/send_join`、`/get_missing_events` 使用，实际 **0 调用点**；上游本身亦为 experimental |
| **v1.157.2 安全版本**（6 High / 4 Moderate / 2 Low，ELEMENTSEC/GHSA） | 1.157.2 | **未判定** | 本报告不应对上游安全版本沉默；需逐条产出"受影响/不受影响 + 证据"对照表 |

### 5.3 外部依赖类（非路由，但影响能力声明）

| 上游条目 | 本仓实测 | 证据 |
|---|---|---|
| 1.157 `exclude_rooms_from_presence` / `presence` 分节新增 `last_active_granularity`、`sync_online_timeout`、`idle_timeout` | **未核对** | 需核对 `synapse-common/src/config` 的 presence 段 |
| 1.157 存在感禁用后把此前在线用户标记为离线（#19948） | **需关注** | 本仓 presence 语义见 §6 相关条目 |
| 1.161 弃用 `matrix_rtc.livekit_service_url`，改用 SFU WebSocket URL | **PARTIAL（死配置）** | `LivekitConfig.ws_url`（`config/voip.rs:92`）**全仓无读取点**；`rtc/transports` 只返回 ICE |
| 1.158 `register_federation_callbacks(...)` 模块 API | **未核对** | — |

---

## 六、本轮纠正的错误结论与仍缺失项

### 6.1 已实现、但历史版本文档判定为"缺失"（纠偏）

| 端点 | v1.3 文档 | 实测 |
|---|---|---|
| `GET/DELETE /_synapse/admin/v1/rooms/{room_id}/reports[/{report_id}]` | §三"缺失（待实现）" | **已实现**（`_synapse/admin/v1/rooms/{room_id}/reports`、`.../reports/{report_id}` 均在册；`synapse-web/src/routes/admin/report.rs`） |
| `GET /_synapse/admin/v1/quarantine_media/{media_id}/changes` | §三"缺失（待实现）" | **已实现**（在册） |
| `GET/DELETE /_synapse/admin/v1/reports[/{report_id}]` | 未提及 | **已实现**（在册，与 `event_reports` 并存） |
| `POST /_matrix/client/v3/keys/upload` 拒绝 `device_keys: null` | §三"缺失（待实现）" | 需下一轮按代码复核（本版未实测，标 `[未验证]`） |
| 事件举报 API（`event_reports` 全家族 16 条，含 `rate_limit/{user_id}/block`） | v1.3 完全未列 | **已实现且超出上游文档面** |

> ⚠️ **§三 的"缺失清单"在 v1.3 中停更于 2026-05-28**，其"待实现"标记已不可作为缺失证据。
> 本版起：该清单每条必须附 `路径:行号` 或"在册证据"，否则不写入。

### 6.2 结构性缺失（本仓无对应实现，需决策）

| 项 | 状态 | 影响 |
|---|---|---|
| **App Service 登录**（`m.login.application_service`） | 整体缺失 | 无法支持 AS 伪装的客户端登录；上游已稳定 `M_APPSERVICE_LOGIN_UNSUPPORTED` |
| **MSC4512** App Service 命名空间代理 / 联邦请求 | 缺失 | 上游为 experimental + opt-in |
| **MSC3912 / v11 撤回格式** | 缺失（且与默认房间版本 11 冲突） | 协议互操作缺陷：本仓发出的撤回在合规 v11 实现上可能不生效 |
| **v12/v13 房间创建** | 不可创建 | 联邦中存在 v12 房间；v12 认证规则未实现 |
| **`rc_reports` 限流桶** | 缺失 | 举报端点未按规范限流 |
| **MSC4140 联邦 EDU** | 缺失 | 延迟事件仅单机可用 |
| **MSC3814 `/events` 方法** | 漂移（POST vs 上游 GET） | 与上游/客户端契约不一致 |

### 6.3 Admin 媒体端点（对照表中唯一可辩护的真实差距）

本仓 admin 媒体类仅 **7** 条逻辑端点；上游文档索引列出的 admin 媒体面更宽（含按用户/按房间的媒体列举与删除、媒体缓存清除、
隔离媒体变更流等）。本仓已有 `quarantine_media/{media_id}/changes`、`purge_media_cache`、`media_callbacks` 等，
但**缺少**形如 `GET/DELETE /_synapse/admin/v1/users/{user_id}/media` 的按用户媒体管理族。是否补齐取决于产品是否需要该运维面板。

---

## 七、优化建议

> 原则：**不做人日估算**；每条给"现象 → 动作 → 验收判据"。与
> [`docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md`](../audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md)
> 和 [`PROJECT_REMAINING_ISSUES_2026-09-14.md`](../audit/PROJECT_REMAINING_ISSUES_2026-09-14.md) 交叉引用，**本文档不新开 backlog**。

### A. 文档可信度（本文档自身）

| 编号 | 动作 | 验收判据 |
|---|---|---|
| A1 | ✅ 本版已修正：路由总数改为 `ROUTE_CONTRACT.md` 实测（931 条目 / 811 逻辑端点），删除无源的 883 / 411 | §1 的每个数字都能由 §8 命令复现 |
| A2 | ✅ 本版已修正：删除"34/35（97%）"等不可比口径相除得到的覆盖率百分比，改为显式口径警告 | §四 表格不含未标注口径的百分比 |
| A3 | ✅ 本版已修正：章节编号重复（v1.3 出现两个"三、"）| 章节编号唯一 |
| A4 | 把"缺失清单"改为**带证据的清单**：每条必须有 `路径:行号` 或在册证据 | 评审清单项：无证据条目不得出现 |
| A5 | 引用其他人工文档时必须带时间戳（沿用 2026-09-22 复核口径） | 脚本化检查（建议 D 类） |

### B. 协议正确性（优先于实验性 MSC）

| 编号 | 动作 | 验收判据 | 关联 |
|---|---|---|---|
| B1 | **v11 撤回格式**：`room_version > 10` 时把目标 id 写入 `content.redacts` | 新增测试：v11 房间撤回的 `content.redacts` == 目标 id；v10 仍在顶层 | 本报告 §5.2 首次指出 |
| B2 | **`rc_reports` 限流桶** | 命中限流返回 429 + `retry-after`；配置有测试覆盖 | 上游 1.161 #20036 |
| B3 | **MSC3814 `/events` 改为 GET + query**，`next_batch` 末页返回 null | 契约测试断言方法为 GET、末页 `next_batch` 为 null | 上游 1.157 #19896 |
| B4 | **移除上游已删除的 `msc2965/auth_issuer`**（或明确记录为有意兼容） | 决策记录；若保留需在本文档标注为"上游已删除的本仓扩展" | 上游 1.161 #20163 |
| B5 | **v12/v13 支持边界决策**：实现 v12 认证规则并放开创建，或明确记录"仅 join/federate" | 决策记录 + `capabilities` 输出与之一致 | 上游 1.158 |
| B6 | **v1.157.2 安全公告同类性逐条判定** | 每条给出"受影响/不受影响 + 证据" | 上游 1.157.2 |
| B7 | **Profile 语义对齐**（停用但存在的用户写自定义字段应成功；account_data 非对象 ⇒ 400 而非 500） | 各状态码有测试断言 | 上游 1.161 #20172/#20149 |

### C. 功能补齐（保留原方向，重新定级）

| 编号 | 项 | 原定级 | 建议定级 | 理由 |
|---|---|---|---|---|
| C1 | App Service 登录（含稳定错误码） | 未列 | **高（决策项）** | 功能整体缺失，非"未稳定化"；若不实现应显式声明不支持 |
| C2 | MSC4502 / MSC4262 从 PARTIAL 收敛到完整或显式声明边界 | 未列 | **中** | 代码已有实现痕迹但未验证语义完整性 |
| C3 | MSC4140 联邦 EDU | "已对齐" | **中** | 客户端链路真实，缺联邦；需按草案确认是否必须 |
| C4 | Admin 媒体端点族补齐 | 未列 | **中（决策项）** | 唯一可辩护的覆盖率差距（§6.3） |
| C5 | Admin `stats` 接口接运维仪表盘 | 短期 | **中（保留）** | 保留 v1.3 方向 |
| C6 | MSC4242 / MSC4512 | P0 阻断性 | **低（观察项）** | 上游本身 experimental + opt-in；应修正 `dag.rs` 不实注释 |
| C7 | OIDC 完善 / Push 优化 / Worker 架构激活 | 中期 | **中（保留）** | 保留 v1.3 方向 |

### D. 把"可辩护的覆盖率"变成机器产物（建议新增）

| 编号 | 动作 | 验收判据 |
|---|---|---|
| D1 | 在 SDK/脚本侧增加**上游端点机器抽取**（从 `element-hq/synapse` 的 `synapse/rest/**` 或官方 OpenAPI 导出），产出与 `ROUTE_CONTRACT.md` 同构的清单 | 生成上游清单的脚本可复现；两侧口径一致后**才**允许输出覆盖率百分比 |
| D2 | 本文档的每个数字必须来自 §8 命令或 `ROUTE_CONTRACT.md` | 脚本对故意写错的数字能变红 |
| D3 | 文档中出现的 MSC 编号必须已登记在 `MSC_SEMANTICS.md` | 未登记编号报错 |

---

## 八、复核命令（可复制）

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# ① 路由权威口径（机器生成于 2026-09-21）
grep -n '注册路由条目\|含路由注册的模块文件\|registered_by' docs/synapse-rust/ROUTE_CONTRACT.md | head

# ② 从 ROUTE_CONTRACT.md 抽取路由，复现 §1.1 的三种口径
grep -oE '^- `(GET|PUT|POST|DELETE|OPTIONS|PATCH)` `[^`]+`' docs/synapse-rust/ROUTE_CONTRACT.md \
  | sed -E 's/^- `([A-Z]+)` `([^`]+)`$/\1 \2/' > /tmp/mp.txt
wc -l < /tmp/mp.txt                                          # 1151 注册条目 (method,path)
awk '{print $2}' /tmp/mp.txt | sort -u > /tmp/paths.txt
wc -l < /tmp/paths.txt                                       #  931 唯一路径
grep -c '^/_matrix/client' /tmp/paths.txt                    #  519 唯一路径 client
grep -c '^/_synapse/admin' /tmp/paths.txt                    #  218 唯一路径 admin

# ③ 逻辑端点口径（折叠版本前缀）
python3 - <<'PY'
import re
paths=[l.strip() for l in open('/tmp/paths.txt') if l.strip()]
def norm(p):
    p=re.sub(r'^/_matrix/client/(v3|v1|r0|v[0-9]+|versions)/','/_matrix/client/vX/',p)
    p=re.sub(r'^/_matrix/client/(unstable|org\.matrix\.[a-z0-9._]*)/','/_matrix/client/vX/',p)
    p=re.sub(r'^/_matrix/client/vX/org\.[a-z0-9._]+/','/_matrix/client/vX/',p)
    p=re.sub(r'^/_synapse/admin/v[0-9]+/','/_synapse/admin/vX/',p)
    return p
L=sorted({norm(p) for p in paths})
print('逻辑端点(全部) =', len(L))                                                   # 811
print('逻辑端点(client) =', sum(1 for p in L if p.startswith('/_matrix/client')))   # 401
print('逻辑端点(admin)  =', sum(1 for p in L if p.startswith('/_synapse/admin')))   # 216
open('/tmp/logical_routes.txt','w').write('\n'.join(L)+'\n')
PY

# ④ 分类统计（§二/§三）：先把 §8.1 的脚本存为 /tmp/classify_routes.py
python3 /tmp/classify_routes.py /tmp/paths.txt          # 唯一路径口径
python3 /tmp/classify_routes.py /tmp/mp_paths_all.txt   # 注册条目口径（awk '{print $2}' /tmp/mp.txt > /tmp/mp_paths_all.txt）
python3 /tmp/classify_routes.py /tmp/logical_routes.txt # 逻辑端点口径

# ⑤ 上游基准与增量（v1.161.0 = 2026-09-15）
gh release list --repo element-hq/synapse --limit 8
gh api "repos/element-hq/synapse/contents/CHANGES.md?ref=v1.161.0" -H "Accept: application/vnd.github.raw" > /tmp/synapse_changes.md
awk '/^# Synapse 1\.158\.0 \(/{f=1} f' /tmp/synapse_changes.md | awk '/^# Synapse 1\.157\.0 \(/{exit} {print}'

# ⑥ 关键事实核对
grep -n 'DEFAULT_ROOM_VERSION: &str\|stable_parse_only' synapse-common/src/room_versions.rs   # 默认 v11；v12/v13 只读
grep -rn 'rc_reports' --include='*.rs' . || echo 'rc_reports = 0（限流缺失）'
grep -rli 'msc4512' --include='*.rs' . || echo 'msc4512 = 0（未实现）'
grep -rli 'msc4502' --include='*.rs' . | wc -l                # 8 个文件（PARTIAL）
grep -rli 'msc4262\|msc4429' --include='*.rs' . | wc -l       # 8 个文件（PARTIAL）
grep -n 'msc2965' /tmp/paths.txt                              # auth_issuer 仍在本仓注册（上游 1.161 已删除）
```

### 8.1 分类归属脚本（`classify_routes.py`）

按**有序优先级**把每条路径唯一归属到一类，因此各类可相加 = 该命名空间总数。规则内置于脚本，
关键归属：`/users/{id}/media` → 用户管理（非媒体）；`/rooms/{id}/…` 的全部子资源 → 房间（仅 `sendToDevice`/`msc4140`/`/rooms/{id}/event/` → 消息）。

```python
#!/usr/bin/env python3
"""用法: python3 classify_routes.py <唯一路径文件>"""

import re, sys
from collections import Counter

CLIENT = [
    (
        "认证",
        r"/(login|logout|register|refresh|oidc|saml|cas|rendezvous)"
        r"|/account/(password|deactivate|3pid)|msc2965|msc4108|msc3882|/organizations",
    ),
    (
        "同步",
        r"/sync|/notifications|msc3575|/to_device|/pushrules|/pushers|/push$|/push/",
    ),
    (
        "设备",
        r"/devices|/keys|/room_keys|/device_verification|/device_trust"
        r"|/cross_signing|/dehydrated_device|msc3814",
    ),
    ("搜索", r"/search"),
    ("媒体", r"/media|/upload|/thumbnail|/preview_url"),
    (
        "用户",
        r"/profile|/presence|/user_directory|/thirdparty|/users|/capabilities|/account_data",
    ),
    (
        "消息",
        r"/rooms/[^/]+/(send|messages|receipt|typing|redact|report|read_markers)"
        r"|/sendToDevice|msc4140|/rooms/[^/]+/event/",
    ),
    ("房间", r"."),
]
ADMIN = [
    (
        "用户管理",
        r"/users|/user_sessions|/registration_tokens|/register|/account_validity"
        r"|/whois|/whoami|/account|/password_auth_providers|/user_stats|/invite",
    ),
    ("媒体", r"/media|/quarantine_media|/purge_media_cache|/media_callbacks"),
    ("联邦", r"/federation|/destinations|/server|/version|/rate-limit-status"),
    (
        "安全",
        r"/event_reports|/reports|/policy|/audit|/feature-flags|/experimental_features"
        r"|/background_updates",
    ),
    (
        "房间管理",
        r"/rooms|/retention|/purge_room|/purge_history|/shutdown_room|/spaces"
        r"|/room_stats|/stats|/statistics|/server_notices|/send_server_notice"
        r"|/jitsi|/cleanup|/config|/captcha",
    ),
    ("服务器", r"."),
]

lines = [l.strip() for l in open(sys.argv[1]) if l.strip()]
for key, rules in (("Client", CLIENT), ("Admin", ADMIN)):
    prefix = "/_matrix/client" if key == "Client" else "/_synapse/admin"
    c = Counter()
    for l in lines:
        if not l.startswith(prefix):
            continue
        for name, pat in rules:
            if re.search(pat, l):
                c[name] += 1
                break
    print(f"### {key} (总计 {sum(c.values())})")
    for name, n in c.most_common():
        print(f"  {name}: {n}")
```

---

## 九、关联文档

| 文档 | 用途 | 时效性 |
|---|---|---|
| [`ROUTE_CONTRACT.md`](./ROUTE_CONTRACT.md) | **路由机器权威**（逐模块 `(method, path)`） | 2026-09-21 生成 |
| [`MSC_SEMANTICS.md`](./MSC_SEMANTICS.md) | **MSC 编号语义唯一真相源**（含"借用编号"登记） | 2026-09-14 |
| [`ELEMENT_SYNAPSE_GAP_ANALYSIS_2026-07-28.md`](./ELEMENT_SYNAPSE_GAP_ANALYSIS_2026-07-28.md) | 对标 v1.156.0 的功能级差距分析 | 2026-07-28（基准已落后 5 个版本） |
| [`../audit/COMPARISON_REPORT_REVIEW_2026-09-22.md`](../audit/COMPARISON_REPORT_REVIEW_2026-09-22.md) | 对 `synapse-rust-vs-synapse-comparison.md` 的复核（含 v1.157–1.161 逐条实测） | 2026-09-22 |
| [`LEDGER_EXPORT_SCHEMA.md`](./LEDGER_EXPORT_SCHEMA.md) | ledger 导出格式与 SDK 契约同步口径 | — |

---

*创建日期: 2026-03-19*
*最后更新: 2026-09-22（v1.4：对齐上游 v1.161.0；路由口径改为 `ROUTE_CONTRACT.md` 机器实测并区分「注册条目 / 唯一路径 / 逻辑端点」三种口径；
剔除无源的 883 / 411 计数与不可比口径相除得到的覆盖率百分比；新增 v1.157–v1.161 增量逐条判定；修正章节编号重复与已实现项误判；内嵌可复现的分类脚本）*
