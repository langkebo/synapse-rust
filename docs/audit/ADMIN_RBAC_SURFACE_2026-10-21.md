# 普通 admin 的写操作可及面（RBAC 提权面审计）

> 数据来源：`docs/synapse-rust/ROUTE_HANDLERS.json` 中 299 条 `/_synapse/admin/**` 路由，
> 套用 `synapse-web/src/utils/admin_auth.rs` 的**当前**判定（`is_role_allowed("admin", …)`
> + `is_super_admin_only_endpoint`）逐条求值；登记表见
> `test-support/admin_rbac_write_allowlist.txt`，断言见
> `synapse-web/src/utils/admin_auth/admin_rbac_tests.rs`。
> 生成日期：2026-10-21。

## 1. 结论

普通 admin 角色可执行的写操作共 **115 条**（读操作为 120 条，未在此列）。按影响分为三级：

| 级别 | 条数 | 处置（2026-10-21 已执行） |
|---|---|---|
| **S — 收紧到 super_admin** | **25** | 其中 **21 条整条收紧**，4 条改为**字段级守卫**（`password` / `login` / `account` 的 `admin` 位），见 §2 |
| **A — 建议收紧（可讨论）** | **30** | **未动**，留作第二批 |
| **B — 建议保留** | **60** | 保留 |

另有 **21 条写操作曾被完全拒绝**（过严），本次**放宽 9 条**（`invite`、`captcha`、`server_notices`、`reports`），见 §6。

执行后普通 admin 可执行的写操作：**115 − 21 + 9 = 103 条**，由
`test-support/admin_rbac_write_allowlist.txt` 登记、`admin_rbac_tests` 双向断言。

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

## 4. A 级：建议收紧，可讨论（30 条）

| 方法 | 路径 |
|---|---|
| `DELETE` | `/_synapse/admin/v1/cas/services/{service_id}` |
| `DELETE` | `/_synapse/admin/v1/external_services/{as_id}` |
| `DELETE` | `/_synapse/admin/v1/federation/destinations/{destination}` |
| `DELETE` | `/_synapse/admin/v1/rooms/{room_id}/reports/{report_id}` |
| `DELETE` | `/_synapse/admin/v1/saml/mapping/{name_id}` |
| `DELETE` | `/_synapse/admin/v1/spaces/{space_id}` |
| `DELETE` | `/_synapse/admin/v1/users/{user_id}` |
| `DELETE` | `/_synapse/admin/v1/users/{user_id}/refresh_tokens/{token_id}` |
| `DELETE` | `/_synapse/admin/v1/users/{user_id}/shadow_ban` |
| `DELETE` | `/_synapse/admin/v2/users/{user_id}` |
| `POST` | `/_synapse/admin/v1/cas/services` |
| `POST` | `/_synapse/admin/v1/cas/users/{user_id}/attributes` |
| `POST` | `/_synapse/admin/v1/cleanup/rooms` |
| `POST` | `/_synapse/admin/v1/cleanup/tokens` |
| `POST` | `/_synapse/admin/v1/external_services` |
| `POST` | `/_synapse/admin/v1/federation/confirm` |
| `POST` | `/_synapse/admin/v1/federation/destinations/{destination}/reset_connection` |
| `POST` | `/_synapse/admin/v1/federation/rewrite` |
| `POST` | `/_synapse/admin/v1/media_callbacks` |
| `POST` | `/_synapse/admin/v1/rooms/{room_id}/backfill` |
| `POST` | `/_synapse/admin/v1/rooms/{room_id}/block` |
| `POST` | `/_synapse/admin/v1/rooms/{room_id}/unblock` |
| `POST` | `/_synapse/admin/v1/user_sessions/{user_id}/invalidate` |
| `POST` | `/_synapse/admin/v1/users/batch_deactivate` |
| `POST` | `/_synapse/admin/v1/users/{user_id}/deactivate` |
| `POST` | `/_synapse/admin/v1/users/{user_id}/logout` |
| `POST` | `/_synapse/admin/v1/users/{user_id}/shadow_ban` |
| `PUT` | `/_synapse/admin/v1/external_services/{as_id}` |
| `PUT` | `/_synapse/admin/v1/saml/mapping/{name_id}` |
| `PUT` | `/_synapse/admin/v2/users/{user_id}` |

理由：可恢复但影响大（强制下线、删设备令牌、停用账号、屏蔽房间、删联邦目标）、
改全局集成配置（CAS / external services / media callbacks）、或**破坏审计**
（`DELETE rooms/{room_id}/reports/{report_id}`）。

## 5. B 级：建议保留（60 条）

按域汇总：`rooms` 14、`background_updates` 9、`users` 9、`event_reports` 8、`media` 7、`notifications` 4、`modules` 2、`saml` 2、`federation` 1、`account_validity` 1、`appservices` 1、`external_services` 1、`telemetry` 1。

这些都是日常内容治理与运维：举报处置、踢人/封禁/解封、媒体隔离、后台任务控制、
系统通知、配额覆盖、联邦缓存清理、空间删除等。完整的 60 条见
`test-support/admin_rbac_write_allowlist.txt`（未在 S/A 中的条目即为此级）。

其中三条值得单独确认（当前判为 B，但有人可能想收紧）：

+ `POST /_synapse/admin/v1/users/batch_deactivate` —— 批量停用，源注释明确写"admin 可以做"，
  属**有意**保留。
+ `POST /_synapse/admin/v1/media/quarantine/{server_name}/{media_id}` —— 可借隔离机制
  让任意媒体不可见（内容层拒绝服务），但可逆。
+ `POST /_synapse/admin/v1/notifications` —— 可向任意用户发通知，存在钓鱼滥用面，但可逆。

## 6. 反向问题：21 条写操作被完全拒绝（过严）— **已放宽 9 条**

`is_role_allowed("admin", …)` 对这些返回 false，普通 admin 根本调不动，但它们是正常运维操作：

`DELETE /_synapse/admin/v1/invite/allowlist/{user_id}`、
`POST /_synapse/admin/v1/invite/allowlist`、`POST /_synapse/admin/v1/invite/blocklist`、
`POST /_synapse/admin/v1/captcha/cleanup`、`DELETE /_synapse/admin/v1/server_notices/{notice_id}`、
`DELETE /_synapse/admin/v1/reports/{report_id}`（以及同族的 allowlist/blocklist 增删）等。

根因：allowlist 是"前缀 OR 链"，这些路径（`/invite/`、`/captcha/`、`/server_notices/`、`/reports/`）
从未被任何前缀覆盖。**建议**：按运维实际需要，给 `admin` 角色补上 `invite`、`captcha`、`server_notices`、
`reports` 四个前缀（只加读 + 明确的写）。

## 7. 建议执行顺序

1. **先修 §2 的提权链**（两处守卫，最小改动、可独立验证）——这是唯一一处"当前就能提权"的问题。
2. **收紧 §3 的 25 条**（`is_super_admin_only_endpoint` 增加规则 + 重新生成登记表）。
   改完必须重跑 `plain_admin_write_surface_matches_the_register`：它会要求你同步登记表，
   这就是这套断言存在的意义。
3. **§6 的过严问题**按运维需要补前缀（会新增登记条目，属"放宽"，需单独确认）。
4. §4 的 30 条作为第二批讨论，逐条判断是否影响现有运维流程。

## 8. 本文件的边界

+ 只覆盖**写操作**。读操作（120 条）未分级——普通 admin 可读的内容包括房间消息、
  用户资料、审计日志等，属另一个议题（数据可见性），不在本文范围。
+ 判定为"当前实现的行为"，不含 `admin_rbac_enabled = false` 的情况（那时所有已认证 admin 直通，
  RBAC 完全不生效）。
