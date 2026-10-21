# 普通 admin 的写操作可及面（RBAC 提权面审计）

> 数据来源：`docs/synapse-rust/ROUTE_HANDLERS.json` 中 299 条 `/_synapse/admin/**` 路由，
> 套用 `synapse-web/src/utils/admin_auth.rs` 的**当前**判定（`is_role_allowed("admin", …)`
> + `is_super_admin_only_endpoint`）逐条求值；登记表见
> `test-support/admin_rbac_write_allowlist.txt`，断言见
> `synapse-web/src/utils/admin_auth/admin_rbac_tests.rs`。
> 生成日期：2026-10-21。

## 1. 结论

普通 admin 角色可执行的写操作共 **115 条**（读操作另计：143 条读路由中普通 admin 可读 131 条，
见 §9）。按影响分为三级：

| 级别 | 条数 | 处置（2026-10-21） |
|---|---|---|
| **S — 收紧到 super_admin** | **25** | **21 条整条收紧** + 4 条字段级守卫（`password` / `login` / `account` 的 `admin` 位），见 §2/§3 |
| **A — 第二批裁决（已执行）** | **30** | **9 条整条收紧** + **9 条改为目标级守卫**（目标账户是管理员时才收紧，见 §4.2）+ **12 条裁决保留**（见 §4.3） |
| **B — 建议保留** | **60** | 保留 |

另有 **21 条写操作曾被完全拒绝**（过严），本次**放宽 11 条**：第一批 9 条
（`invite`、`captcha`、`server_notices`、`reports`），第二批 2 条路径别名（§7 第 3 条）。

执行后普通 admin 可执行的写操作：**115 − 21 − 9 + 9 + 2 = 96 条**，由
`test-support/admin_rbac_write_allowlist.txt` 登记、`admin_rbac_tests` 双向断言。
（§4.2 的 9 条**不改这个数字**——端点仍对普通 admin 开放，只在目标本身是管理员时收紧。）

## 2. P0：改密码没有任何角色守卫（可提权到 super_admin）— **已修（2026-10-21）**

这是本轮最严重的发现，与"清单漏网"不是一回事——它是**单个 handler 缺守卫**。

`synapse-web/src/routes/admin/user.rs`：

+ `reset_user_password`（`POST /_synapse/admin/v1/users/{user_id}/password`）**全文没有
  `ensure_super_admin_for_privilege_change`**，直接校验密码强度后
  `change_password(..., logout_devices = true)`。
+ `create_or_update_user_v2`（`PUT /_synapse/admin/v2/users/{user_id}`）对
  `admin` / `user_type` 有守卫，但 **`password` 字段没有**，同样直接落库。

对照：同一文件的 `login_as_user` **有** P0-1 守卫——
"禁止普通 admin 互登录到其它管理员账户"。同一个风险，一处修了、一处没修。

完整提权链（无需任何特殊条件）：

1. 普通 admin 调 `POST /_synapse/admin/v1/users/{super_admin_id}/password`，设一个新密码；
2. `logout_devices = true` 把原 super_admin 全端踢下线（若原主人在线会被发现，否则静默）；
3. 攻击者用新密码走 `POST /_matrix/client/v3/login`，拿到 `is_admin = true` 的 access token；
4. `should_require_admin_mfa` 只对 `/_synapse/admin/**` 生效，`/login` 不在其内 ⇒ TOTP 不拦；
5. 攻击者以 super_admin 身份调用任意 admin API。

**建议修复（最小改动）**：两处都加同一守卫——目标是 admin 时要求操作者是 super_admin：

```rust
// reset_user_password
if user.is_admin { crate::routes::admin::ensure_super_admin_for_privilege_change(&admin)?; }

// create_or_update_user_v2：body.password.is_some() 时，若目标是 admin，同样要求 super_admin
if body.password.is_some() { /* 需先解析目标用户拿 is_admin */ }
```

不必直接收紧整个端点：普通 admin 给**普通用户**重置密码是合理的运维能力，问题只在目标是管理员时。

> 后续（§4.2）发现这不是孤例：同一形状还有硬删账号、停用、批量停用、踢下线、吊销
> 会话/refresh token、shadow ban 共 8 条。它们已在第二批用同一把守卫补齐——
> 也就是说"目标账户是管理员 ⇒ 凭据 / 身份位 / 可用性操作一律要 super_admin"这条
> **现在是完整的**，不再是单点补丁。

## 3. S 级：收紧到 super_admin（25 条，其中 21 条已整条收紧）

| 方法 | 路径 |
|---|---|
| `DELETE` | `/_synapse/admin/v1/appservices/{as_id}` |
| `DELETE` | `/_synapse/admin/v1/feature-flags/{flag_key}` |
| `DELETE` | `/_synapse/admin/v1/modules/{module_name}` |
| `DELETE` | `/_synapse/admin/v1/rooms/{room_id}` |
| `DELETE` | `/_synapse/admin/v1/users/{user_id}/tokens/{token_id}` |
| `PATCH` | `/_synapse/admin/v1/feature-flags/{flag_key}` |
| `POST` | `/_synapse/admin/v1/account/{user_id}` |
| `POST` | `/_synapse/admin/v1/account_validity` |
| `POST` | `/_synapse/admin/v1/appservices` |
| `POST` | `/_synapse/admin/v1/appservices/{as_id}/events` |
| `POST` | `/_synapse/admin/v1/appservices/{as_id}/state` |
| `POST` | `/_synapse/admin/v1/appservices/{as_id}/users` |
| `POST` | `/_synapse/admin/v1/cleanup/all` |
| `POST` | `/_synapse/admin/v1/feature-flags` |
| `POST` | `/_synapse/admin/v1/media/delete` |
| `POST` | `/_synapse/admin/v1/modules` |
| `POST` | `/_synapse/admin/v1/modules/{module_name}/enable` |
| `POST` | `/_synapse/admin/v1/rooms/cleanup` |
| `POST` | `/_synapse/admin/v1/rooms/{room_id}/cascade_redact` |
| `POST` | `/_synapse/admin/v1/shutdown_room` |
| `POST` | `/_synapse/admin/v1/users/{user_id}/login` |
| `POST` | `/_synapse/admin/v1/users/{user_id}/password` |
| `PUT` | `/_synapse/admin/v1/appservices/{as_id}` |
| `PUT` | `/_synapse/admin/v1/modules/{module_name}/config` |
| `PUT` | `/_synapse/admin/v1/saml/config` |

分组理由：

+ **身份与凭据**：`users/{user_id}/password`、`v2/users/{user_id}`、`users/{user_id}/login`、
  `account/{user_id}` —— 改密码、改账号、签发他人 token，等同接管账号。
+ **应用服务（等同后门）**：`appservices` 的增删改与 `appservices/{as_id}/events|state|users` ——
  appservice 的 `as_token` 可以以任意用户身份发事件，拿到即等于全服务器。
+ **可加载代码/改拦截策略**：`modules` 增删、`modules/{module_name}/enable`、
  `modules/{module_name}/config` —— 模块可执行代码并介入消息流。
+ **可关掉安全特性**：`feature-flags` 增删改 —— 能关闭安全相关开关。
+ **SSO 配置**：`saml/config` —— 改错可致全站登不上，改对可放行未授权身份。
+ **全站级不可逆**：`account_validity`（到期停用全部账号）、`cleanup/all`、`media/delete`（批量）、
  `rooms/cleanup`、`rooms/{room_id}/cascade_redact`（级联撤回全部消息）、`rooms/{room_id}`（删房间）、
  `shutdown_room`（关闭房间并踢出全部成员）。
+ **令牌吊销**：`users/{user_id}/tokens/{token_id}`。

## 4. A 级 30 条：第二批裁决（2026-10-21 已执行）

A 级原本写着"建议收紧（可讨论）"。逐条看下来，它其实混了两件不同的事：

+ 一部分**确实该收紧**，但判据不是"这条危险"，而是"**它与已收紧的那条是同一类**"——
  CAS 服务配置之于 `saml/config`、外部服务之于 `appservices`、`cleanup/rooms` 之于
  `cleanup/all`。这一类留在外面，等于上一批收紧只做了一半。
+ 另一部分（停用、踢下线、删令牌、shadow ban）**不能整条收紧**：普通 admin 对普通用户
  做这些是日常运维。真正的问题不在"能不能调这个端点"，而在"**目标是不是管理员**"。

因此第二批不做"30 条一刀切"，而是分成三种处置。

### 4.1 整条收紧到 super_admin（9 条）

| 方法 | 路径 | 为什么归到已收紧的那一类 |
|---|---|---|
| `DELETE` | `/_synapse/admin/v1/cas/services/{service_id}` | CAS 是 SSO 集成配置：改错全站登不上、改对可放行未授权身份——与 §3 的 `saml/config` 同类 |
| `POST` | `/_synapse/admin/v1/cas/services` | 同上 |
| `DELETE` | `/_synapse/admin/v1/external_services/{as_id}` | 带 `webhook_url` + `api_key`：可把事件外发到任意地址、也可伪造回调——与 `appservices` 同类后门 |
| `POST` | `/_synapse/admin/v1/external_services` | 同上 |
| `PUT` | `/_synapse/admin/v1/external_services/{as_id}` | 同上（改 URL 即可悄悄改外发目的地） |
| `PUT` | `/_synapse/admin/v1/saml/mapping/{name_id}` | 身份映射决定 IdP 的 `name_id` 落到哪个本地账号：改对即可放行他人身份，与 `saml/config` 同类 |
| `DELETE` | `/_synapse/admin/v1/saml/mapping/{name_id}` | 同上（删掉映射会让合法用户失去登录能力） |
| `POST` | `/_synapse/admin/v1/cleanup/rooms` | **这批里最值得注意的一条**：`cleanup/all` 已于第一批收紧，而 `/cleanup/rooms` 正是它的破坏性主体（`cleanup_abnormal_data`），留着就等于没收紧——普通 admin 改调组成端点即可拿到同样的删除效果 |
| `POST` | `/_synapse/admin/v1/media_callbacks` | 把媒体事件外发到注册的 URL，与外部服务同类 |

保留给普通 admin 的同族端点（都是只读探测或可逆）：

+ `POST /_synapse/admin/v1/external_services/{as_id}/health/check`（探测连通性）；
+ `POST /_synapse/admin/v1/cas/users/{user_id}/attributes`（改单个用户的 CAS 属性，可逆）；
+ `POST /_synapse/admin/v1/saml/metadata/refresh`、`POST /_synapse/admin/v1/saml/logout`；
+ `POST /_synapse/admin/v1/cleanup/tokens`（只清过期令牌，不删业务数据）。

### 4.2 目标级守卫：目标账户是管理员时才收紧（9 条）

这是第二批真正的发现：**P0 只修了"改密码"一条，而"让管理员消失"还有另外八条路。**
它们全都是"普通 admin 对普通用户做是对的、对管理员做就是提权"的形状，所以正确答案不是
整条收紧，而是**按目标判定**——与 §2 的 `ensure_super_admin_when_target_is_admin` 同一把守卫。

| 方法 | 路径 | 不加守卫的后果 |
|---|---|---|
| `DELETE` | `/_synapse/admin/v1/users/{user_id}` | **硬删 super_admin**——比改密码更彻底：账号从库里消失，部分部署还会把 localpart 放出来供重新注册 |
| `DELETE` | `/_synapse/admin/v2/users/{user_id}` | 同上（同一 handler） |
| `POST` | `/_synapse/admin/v1/users/{user_id}/deactivate` | 停用唯一的 super_admin ⇒ 无人能恢复 |
| `POST` | `/_synapse/admin/v1/users/batch_deactivate` | 批量通道，可绕过单条的守卫（本批已按"目标里含管理员即要求 super_admin"处理） |
| `POST` | `/_synapse/admin/v1/users/{user_id}/logout` | 全端踢下线 + `purge_all_device_keys`（顺带丢 OTK/密钥材料） |
| `POST` | `/_synapse/admin/v1/user_sessions/{user_id}/invalidate` | 同上（会话侧） |
| `DELETE` | `/_synapse/admin/v1/users/{user_id}/refresh_tokens/{token_id}` | 吊销 refresh token ⇒ 会话无法续期 |
| `POST` | `/_synapse/admin/v1/users/{user_id}/shadow_ban` | 让管理员**静默失声**（消息对他人不可见） |
| `DELETE` | `/_synapse/admin/v1/users/{user_id}/shadow_ban` | 同上（反向：普通 admin 也不该替管理员解除） |

实现：`routes/admin/mod.rs` 新增

```rust
pub(crate) async fn ensure_super_admin_when_target_user_is_admin(
    ctx: &AdminContext, target_user_id: &str, admin: &AdminUser,
) -> Result<(), ApiError>
```

（只拿路径参数的 handler 用它；已解析出用户记录的 `delete_user` / `deactivate_user` /
`logout_user_devices` / `invalidate_user_sessions` 直接用 `user.is_admin` + 同步版守卫，
不多查一次库。）`batch_deactivate` 为省查询只在操作者不是 super_admin 时逐条判。

**边界**：只覆盖"凭据 / 身份位 / 可用性"三件事。`PUT v2/users/{user_id}` 的其它字段
（`displayname`、`avatar_url`、`threepid`、`deactivated`…）依旧对普通 admin 开放——
它们不改凭据也不夺权；`password` / `user_type` / `admin` 三个字段的守卫在第一批已就位。

### 4.3 裁决保留（12 条）

| 方法 | 路径 | 保留理由 |
|---|---|---|
| `DELETE` | `/_synapse/admin/v1/federation/destinations/{destination}` | 删联邦目标可恢复（重新建即可），属日常联邦治理 |
| `POST` | `/_synapse/admin/v1/federation/confirm` | 确认联邦目的地，运维常规动作 |
| `POST` | `/_synapse/admin/v1/federation/destinations/{destination}/reset_connection` | 重置连接是可恢复的排障手段，收紧会让普通 admin 无法处理联邦故障 |
| `POST` | `/_synapse/admin/v1/federation/rewrite` | 实测该 handler 只统计受影响房间并记日志（`rewrite_federation` 无数仓写入），破坏力为零；真要改成真重写时须重新评估（已在 §5 备注） |
| `DELETE` | `/_synapse/admin/v1/rooms/{room_id}/reports/{report_id}` | 与第一批放宽的 `DELETE /reports/{report_id}` 是同一操作的两个路径。上游 Synapse 也把它当普通 admin 能力；且**删除动作本身会落 admin 审计事件**（`authorize_admin_from_services` 记录 method/path），审计链没断 |
| `DELETE` | `/_synapse/admin/v1/spaces/{space_id}` | 空间删除属内容治理（§5 已按 B 级收录）；本文件在 §4 与其重复登记，以 B 级为准 |
| `POST` | `/_synapse/admin/v1/rooms/{room_id}/backfill` | 资源消耗型但可中断、可恢复 |
| `POST` | `/_synapse/admin/v1/rooms/{room_id}/block` | 屏蔽房间可逆（有 `/unblock` 与 §5 的批量解封） |
| `POST` | `/_synapse/admin/v1/rooms/{room_id}/unblock` | 同上 |
| `POST` | `/_synapse/admin/v1/cleanup/tokens` | 只清过期令牌 |
| `POST` | `/_synapse/admin/v1/cas/users/{user_id}/attributes` | 改单个用户属性，可逆 |
| `PUT` | `/_synapse/admin/v2/users/{user_id}` | 见 §4.2 边界：`password` / `user_type` / `admin` 已守卫，其余字段无提权效果 |

## 5. B 级：建议保留（60 条）

按域汇总：`rooms` 14、`background_updates` 9、`users` 9、`event_reports` 8、`media` 7、`notifications` 4、`modules` 2、`saml` 2、`federation` 1、`account_validity` 1、`appservices` 1、`external_services` 1、`telemetry` 1。

这些都是日常内容治理与运维：举报处置、踢人/封禁/解封、媒体隔离、后台任务控制、
系统通知、配额覆盖、联邦缓存清理、空间删除等。完整的 60 条见
`test-support/admin_rbac_write_allowlist.txt`（未在 S/A 中的条目即为此级）。

其中三条值得单独确认（当前判为 B，但有人可能想收紧）：

+ `POST /_synapse/admin/v1/users/batch_deactivate` —— 批量停用，源注释明确写"admin 可以做"，
  属**有意**保留；目标里含管理员时仍要求 super_admin（§4.2）。
+ `POST /_synapse/admin/v1/media/quarantine/{server_name}/{media_id}` —— 可借隔离机制
  让任意媒体不可见（内容层拒绝服务），但可逆。
+ `POST /_synapse/admin/v1/notifications` —— 可向任意用户发通知，存在钓鱼滥用面，但可逆。

另有一条"名义危险、实际无破坏力"：`POST /_synapse/admin/v1/federation/rewrite`。
`admin_federation_service::rewrite_federation` 目前只统计受影响房间数并打日志，**没有任何
数仓写入**——它是个假实现。若将来补上真实重写（全站成员关系迁移），必须立刻收紧到
super_admin，并在此文件留一行记录。

## 6. 反向问题：21 条写操作被完全拒绝（过严）— **已放宽 11 条**

`is_role_allowed("admin", …)` 对这些返回 false，普通 admin 根本调不动，但它们是正常运维操作：

`DELETE /_synapse/admin/v1/invite/allowlist/{user_id}`、
`POST /_synapse/admin/v1/invite/allowlist`、`POST /_synapse/admin/v1/invite/blocklist`、
`POST /_synapse/admin/v1/captcha/cleanup`、`DELETE /_synapse/admin/v1/server_notices/{notice_id}`、
`DELETE /_synapse/admin/v1/reports/{report_id}`（以及同族的 allowlist/blocklist 增删）等。

根因：allowlist 是"前缀 OR 链"，这些路径（`/invite/`、`/captcha/`、`/server_notices/`、`/reports/`）
从未被任何前缀覆盖。**建议**：按运维实际需要，给 `admin` 角色补上 `invite`、`captcha`、`server_notices`、
`reports` 四个前缀（只加读 + 明确的写）。

### 6.1 路径别名："同一操作、两种判定"

判定完全按**路径字面量**做前缀匹配，于是同一 handler 的两条路径可以拿到相反结论。
第二批收敛了两条（都是"收紧前整条拒绝"，属过严，不是漏洞）：

| 旧路径（收紧前整条拒绝） | 规范路径（同 handler / 同语义） | 处置 |
|---|---|---|
| `POST /_synapse/admin/v1/deactivate/{user_id}` | `POST /_synapse/admin/v1/users/{user_id}/deactivate` | 补前缀，且两者共用带目标级守卫的 handler；上游 Element Admin 仍在调旧路径 |
| `POST /_synapse/admin/v1/user/{user_id}/media/quarantine` | `POST /_synapse/admin/v1/media/quarantine/{server_name}/{media_id}` | 补前缀（同为可逆媒体隔离） |

同类问题在**读**操作面同样存在（如 `/_synapse/admin/v1/user/{user_id}/media` 与
`/_synapse/admin/v1/users/{user_id}/media`），但读面不在本文件范围内（§8）。

## 7. 执行顺序与状态

1. **修 §2 的提权链**（两处守卫）—— ✅ 2026-10-21 完成（`90a0ebecf`）。
2. **收紧 §3 的 25 条** —— ✅ 2026-10-21 完成（21 条整条 + 4 条字段级）；
   同批把 `is_super_admin_only_endpoint` 拆到 `utils/admin_auth/privileged.rs`。
   改完必须重跑 `plain_admin_write_surface_matches_the_register`：它会要求你同步登记表，
   这就是这套断言存在的意义。
3. **§6 的过严问题** —— ✅ 放宽 11 条：第一批 `invite` / `captcha` / `server_notices` /
   `reports` 四个前缀，第二批两条路径别名（§6.1）。
   剩余**结构性过严的写操作共 10 条**（没有任何前缀覆盖，因此被整条拒绝）。逐条裁定
   「保持拒绝」而非放宽，因为其中 6 条保持拒绝恰好就是想要的权限：

   | 路径 | 裁定 | 说明 |
   |---|---|---|
   | `POST /_synapse/admin/v1/account_data_callbacks` | 保持拒绝 | 全局回调配置（数据外发），与 §4.1 的 `media_callbacks` 同类 |
   | `PUT /_synapse/admin/v1/push/config` | 保持拒绝 | 全局推送配置 |
   | `POST /_synapse/admin/v1/audit/events` | 保持拒绝 | 手写审计事件＝污染审计链 |
   | `POST /_synapse/admin/v1/register` | 保持拒绝 | 代注册（共享密钥注册面） |
   | `POST /_synapse/admin/v1/restart` | 保持拒绝 | 部署级动作 |
   | `POST /_synapse/admin/v1/password_auth_providers` | 保持拒绝 | 认证方式配置 |
   | `POST /_synapse/admin/v1/policy/check` | 可放宽（值低） | 只读策略检查，POST 只是携带查询体 |
   | `POST /_synapse/admin/v1/push/cleanup` | 可放宽（运维需要） | 清过期推送数据 |
   | `POST /_synapse/admin/v1/push/process` | 可放宽（运维需要） | 触发待发推送 |
   | `POST /_synapse/admin/v1/join/{room_id_or_alias}` | 可放宽（运维需要） | 管理员强制入房 |

   后四条不动的原因很实际：它们只在"运维确实需要"时才有价值，而放宽会永久扩大
   登记面。等出现真实调用方再补前缀，比先开再收便宜。
4. **§4 的 30 条（第二批）** —— ✅ 2026-10-21 完成，按 §4.1/§4.2/§4.3 三分解处置。

## 8. 本文件的边界

+ 写面见 §2–§7，读面见 §9。读面**不做角色收紧**（读就是审核工作的主体），只做分级、
  结构修正与登记。
+ 判定为"当前实现的行为"，不含 `admin_rbac_enabled = false` 的情况（那时所有已认证 admin 直通，
  RBAC 完全不生效）。

## 9. 读面：143 条读路由的分级与结构修正（2026-10-21）

写面按"能不能造成破坏"分级；读面不能照搬——**读本身不是提权**，但读会泄露数据。
所以读面的问题分两种：数据面该不该给（分级，§9.1），以及判定本身有没有不自洽（§9.2）。

### 9.1 现状与分级

`/_synapse/admin/**` 下共 **143 条**读路由（GET/HEAD），其中普通 admin 可读 **131 条**、
被拒绝 **12 条**。按"读到什么"分三级：

| 级别 | 读到什么 | 结论 |
|---|---|---|
| **R-1 用户数据** | 房间消息 / 状态 / 事件上下文 / 媒体 / 用户资料与设备（含最后 IP）/ 推送端点 / 举报内容 / 审计日志 | **保留**。这是审核能力的本体，收走它就等于取消普通 admin 这个角色 |
| **R-2 治理与运维** | 后台任务、统计、联邦目的地、空间、媒体隔离记录、通知、系统公告、邀请名单 | 保留 |
| **R-3 配置面** | appservices / external_services / CAS / SAML / modules / media_callbacks / registration_tokens / feature-flags | 保留，但需逐个确认**不返回凭据**（见下） |

（三档按资源域粗分，边界有交叉——房间的 `messages` / `state` 归 R-1，`/rooms` 列表与
`/rooms/search` 归 R-2。所以这里只给总数 131，不给分档条数。）

**关键核对**：写面收紧的那批配置端点，如果读接口把同一个密钥吐回来，收紧就是装饰。
逐个查过序列化结构，结论是**只有一处会回明文凭据**：

| 读接口 | 返回字段 | 结论 |
|---|---|---|
| `GET /appservices`、`/{as_id}` | `AppServiceResponse`：无 `as_token` / `hs_token` | 已裁剪 |
| `GET /external_services`、`/{as_id}/health` | `ExternalServiceResponse`：无 `api_key` / `webhook_url` | 已裁剪 |
| `GET /saml/config` | `sanitized_base_config()`：只给 `has_sp_private_key` / `has_sp_certificate` 布尔 | 已裁剪 |
| `GET /cas/services` | `ServiceResponse`：无 shared secret 字段 | 已裁剪 |
| `GET /registration_tokens`、`/{token}` | **返回明文 `token`** | 与上游 Synapse 同款；拿到可自注册账号，但调用者已是 admin，增量有限。**记录在案，不改** |

### 9.2 判定不自洽：三处已修（2026-10-21）

读面这次没有"收紧"，只有"修判定"——三处都属于**失败方向是拒绝**（fail-closed）的过严，
不改会出现"能做 A 不能做 B"的怪状态：

| 现象 | 具体 | 处置 |
|---|---|---|
| **结尾斜杠把集合端点挡在外面** | 第一批放宽写了 `path.starts_with("/_synapse/admin/v1/reports/")` ⇒ 能取 `GET /reports/{id}`，不能 `GET /reports`（列全部）；`room_stats` 同理（`/room_stats/{room_id}` 可读，`/room_stats` 不可读） | `reports` 改成精确集合 + 前缀两种形态；`room_stats` 去掉结尾斜杠 |
| **路径别名判定不同** | `GET /quarantine_media/{media_id}/changes`（旧路径）整条拒绝，`GET /media/quarantine_changes`（新路径）允许 | 补 `quarantine_media/` 前缀 |
| **无前缀覆盖的只读端点** | `GET /whoami`（只回调用者自己的身份）、`GET /rate-limit-status`（自己的限流状态）、`GET /policy/status`（`enabled` / `endpoint` / `fail_mode` 三个字段） | 三条各自补精确路径 |

`GET /reports` 的修复顺带说明一件事：**"放宽一个前缀"时要同时检查它的集合形态**，
否则得到的是"知道 id 才能读"的半开放状态——比全拒更像 bug。

### 9.3 仍被拒绝的 12 条（都是配置/密钥面）

`GET /_synapse/admin/info`、`/v1/server`、`/v1/config`、`/v1/jitsi/config`、`/v1/push/config`、
`/v1/account_data_callbacks`、`/v1/password_auth_providers`、`/v1/federation/blacklist`、
`/v1/retention/policy`、`/v1/retention/policy/{room_id}`、`/v1/retention/status`、
`/v1/register/nonce`。

前十个是全局配置（部分直接含连接串/密钥），后两个是"已被拒绝的写操作的前置"——
`register/nonce` 只用于计算 `POST /v1/register` 的 HMAC，写端既然只给 super_admin，
nonce 也不该给普通 admin。**这 12 条保持拒绝。**

### 9.4 MFA 覆盖：写全量，读只有三类

`is_sensitive_admin_request` 的现行规则：**所有写方法**都算敏感；读只有
`/v1/security`、`/v1/server`、`/v1/media/quarantine` 三类。
也就是说开了 `admin_mfa_required` 之后，普通 admin 读**房间消息、用户 IP、举报内容、审计日志**
仍不需要 TOTP。

**建议（未实施）**：把这些内容型读取纳入敏感集合。**没有直接改**的原因很实际——
MFA 是在**每个请求**上校验的，而现有 admin 前端只在写请求上带 `x-admin-mfa-code`；
贸然把读纳入会让开了 MFA 的部署**整个管理界面打不开**。真要推进，顺序是：
前端先支持读请求带码 → 再改判定 → 再在文档里标注 `admin_mfa_required` 的兼容性变化。

### 9.5 读面也进登记表

新增 `test-support/admin_rbac_read_allowlist.txt`（131 条），与写面共用同一套双向断言
（`admin_rbac_tests::plain_admin_read_surface_matches_the_register`）：

+ 新读端点落进宽前缀却没登记 ⇒ 红；
+ 登记了但已不可达 ⇒ 红。

写面那张表防的是"悄悄获得破坏能力"，这张表防的是"**悄悄开始输出数据**"。
