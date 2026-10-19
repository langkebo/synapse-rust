# 管理 API（`/_synapse/admin/v1`）

服务端管理面，**294 条路由**（`/_synapse/admin/v1/*` 289 条 + `/_synapse/admin/v2/*` 4 条 +
`/_synapse/admin/info` 1 条）。本页覆盖客户端/控制台常用部分；完整清单以 SDK lane 的
`tests/unit/fixtures/ledger_export_sdk/default.json` 为准（`docs/openapi/route-table.json`
是 golden lane 视图，不含下列特性门控端点）。

> **特性门控**：通知 / 服务器通告族需 `server-notifications`，SAML 管理需 `saml-sso`，
> CAS 管理需 `cas-sso`。默认特性集只含 `core-private-chat`、`widgets`、`external-services`、
> `beacons`，因此上述端点在默认构建下返回 **404**（不是 403）。SDK 应以运行时探测为准，
> 参考部署使用 `--features all-extensions`，全部可用。

## 目录

- [认证与通用约定](#认证与通用约定)
- [服务器信息](#服务器信息)
- [用户管理](#用户管理)
- [安全控制（封禁 / 限流 / 邀请名单）](#安全控制封禁--限流--邀请名单)
- [房间管理](#房间管理)
- [媒体管理](#媒体管理)
- [举报管理](#举报管理)
- [注册控制](#注册控制)
- [后台任务与清理](#后台任务与清理)
- [联邦管理](#联邦管理)
- [审计与可观测](#审计与可观测)
- [模块 / 遥测 / 策略 / 验证码 / Jitsi](#模块--遥测--策略--验证码--jitsi)
- [Application Service 管理](#application-service-管理)
- [服务端管理（功能开关 / 保留策略 / 外部服务）](#服务端管理功能开关--保留策略--外部服务)
- [通知与服务器通告](#通知与服务器通告)
- [SAML 管理](#saml-管理)
- [CAS 管理](#cas-管理)
- [管理端已知形状差异](#管理端已知形状差异)

---

## 认证与通用约定

- **只认 `Authorization: Bearer <admin_token>`**；`?access_token=` 无效。
- 缺头 → **401 `M_UNAUTHORIZED`**（不是 `M_MISSING_TOKEN`，与 client API 不同）。
- 非管理员 token → **403 `M_FORBIDDEN`**。
- 成功响应平铺；但注意少数端点返回 `{"status":"ok", "action":"..."}`（见
  [管理端已知形状差异](#管理端已知形状差异)）。
- **分页**：多数列表已从 `offset` 收敛到 **keyset 游标**——传 `offset > 0` 直接 400。

---

## 服务器信息

| 方法 | 端点 | 响应（实测/按实现） |
|---|---|---|
| `GET` | `/server_version` | `{"server_version": "6.2.0", "implementation": "synapse-rust"}` |
| `GET` | `/status` | `{"db_ok": true, "server_ok": true, "up": true}` |
| `GET` | `/config` | `{"server_name", "public_baseurl", "registration_enabled", "max_upload_size"}` |
| `GET` | `/whoami` | 当前管理员 `{user_id, ...}` |
| `GET` | `/statistics` | 见下 |
| `GET` | `/server` | 服务端信息 |
| `GET` | `/health` | 健康检查（比 `/status` 细） |
| `GET` | `/rate-limit-status` | 全局限流状态 |
| `GET` | `/experimental_features` | 实验特性开关（由 feature flags 表驱动） |
| `POST` | `/restart` | 重启（需谨慎） |

> ⚠️ **`GET /_synapse/admin/v1/server_version` 返回的是 `server_version` + `implementation`**，
> **没有** `python_version`（那是上游 Synapse 的字段）。
> `python_version` 只出现在无鉴权的 `GET /_matrix/server_version` 上，且值为字符串 `"Rust"`。
>
> ⚠️ **`/status` 只有三个布尔**，没有 `users` / `rooms` / `uptime`。要数量请用 `/statistics`。

### `/statistics`

```json
{
  "total_users": 1234,
  "total_rooms": 567,
  "daily_active_users": 88,
  "monthly_active_users": 320,
  "r30_users": 410,
  "non_deactivated_user_count": 1200,
  "non_deactivated_user_count_by_app_service": { "": 1180, "my-as": 20 },
  "total_messages": 89012,
  "active_rooms": 120,
  "total_members": 3400,
  "encrypted_rooms": 210,
  "daily_messages": 430
}
```

### 其他统计端点

| 端点 | 说明 |
|---|---|
| `GET /room_stats` / `/room_stats/{room_id}` | 房间统计 |
| `GET /user_stats` | 用户统计总览 |
| `GET /users/{user_id}/stats` | 单用户统计 |
| `GET /telemetry/metrics` | 指标快照 |
| `GET /telemetry/health` / `/telemetry/status` / `/telemetry/alerts` | 遥测状态与告警 |
| `POST /telemetry/alerts/{alert_id}/ack` | 确认告警 |

> **不存在** `GET /_synapse/admin/v1/server_stats`，也**不存在** `/media_stats`。
> 历史文档里的这两个端点（以及 `server_stats` 的 `database_pool_size` / `cache_enabled` 字段）
> 从未实现。

---

## 用户管理

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/users` | 列表（`limit`、`since`） |
| `GET` | `/v2/users` | v2 列表 |
| `GET` | `/users/{user_id}` | 详情 |
| `PUT` | `/v2/users/{user_id}` | 更新（v2） |
| `DELETE` | `/users/{user_id}` | 删除（连带数据） |
| `PUT` | `/users/{user_id}/admin` | 设/取消管理员 |
| `POST` | `/users/{user_id}/deactivate` | 停用 |
| `POST` | `/users/{user_id}/password` | 重置密码 |
| `POST` | `/users/batch` | 批量查询 |
| `POST` | `/users/batch_deactivate` | 批量停用 |
| `POST` | `/users/{user_id}/login` | 以该用户身份取 token（危险） |
| `POST` | `/users/{user_id}/logout` | 踢下线 |
| `POST` | `/users/{user_id}/evict` | 驱逐 |
| `GET` | `/users/{user_id}/rooms` | 该用户加入的房间 |
| `GET` | `/users/{user_id}/devices` | 设备列表 |
| `POST` | `/users/{user_id}/devices/{device_id}/delete` | 删设备 |
| `GET` | `/users/{user_id}/tokens` / `/refresh_tokens` | token 与 refresh token |
| `DELETE` | `/users/{user_id}/refresh_tokens/{token_id}` | 吊销 refresh token |
| `GET` | `/users/{user_id}/media` | 该用户的媒体 |
| `GET`/`POST`/`DELETE` | `/users/{user_id}/shadow_ban` | 影子封禁（见下） |
| `GET`/`PUT`/`DELETE` | `/users/{user_id}/rate_limit` | 限流覆盖 |
| `GET`/`POST`/`DELETE` | `/users/{user_id}/override_ratelimit` | 同上（旧路径） |
| `GET` | `/whois/{user_id}` / `/whois/{user_id}/{device_id}` | 在线信息 |
| `GET` | `/user_sessions/{user_id}` | 会话 |
| `POST` | `/user_sessions/{user_id}/invalidate` | 失效会话 |
| `GET`/`POST` | `/account/{user_id}` | 账户操作 |
| `GET` | `/account_validity/{user_id}` / `POST .../renew` | 账户有效期 |

**`GET /users` 响应**：

```json
{
  "users": [
    { "name": "alice", "is_guest": false, "admin": true, "deactivated": false,
      "displayname": "Alice", "avatar_url": "mxc://...", "created_ts": 1759000000000,
      "user_type": null }
  ],
  "total": 1234,
  "next_batch": "eyJ0cyI6..."
}
```

| 参数 | 说明 |
|---|---|
| `limit` | 夹在服务端上下限内，默认 100 |
| `since` | keyset 游标（上次的 `next_batch`） |
| `offset` | ⚠️ `> 0` 直接 **400 `M_BAD_JSON`**（`Legacy offset pagination is no longer supported; use since cursor`） |

> 字段名是 **`name`**（不是 `user_id`），且 `created_ts` 是毫秒。

---

## 安全控制（封禁 / 限流 / 邀请名单）

本仓把"安全"拆成三组**真实存在**的端点，而不是历史文档里的 `/_synapse/admin/v1/security/*`
（后者不存在）。

### 影子封禁

| 方法 | 端点 | 响应 |
|---|---|---|
| `POST` | `/users/{user_id}/shadow_ban` | `{"is_shadow_banned": true}` |
| `DELETE` | `/users/{user_id}/shadow_ban` | `{}` |

影子封禁的用户仍能正常收发（自己无感），但其消息**不投递给其他人**。用于处置刷屏账号
而不暴露封禁动作。

### 用户级限流

| 方法 | 端点 | 体/响应 |
|---|---|---|
| `GET` | `/users/{user_id}/rate_limit` | `{"messages_per_second": 10, "burst_count": 20}` |
| `PUT` | `/users/{user_id}/rate_limit` | 体同上；成功回同样两字段 |
| `DELETE` | `/users/{user_id}/rate_limit` | `{}`（回落全局） |
| `GET`/`POST`/`DELETE` | `/users/{user_id}/override_ratelimit` | 旧路径，语义同上 |

### 邀请白/黑名单

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/invite/blocklist` | `{"blocklist": [...], "total_count": N, "limit": L, "offset": O}` |
| `POST` | `/invite/blocklist` | 整体设置（体含名单） |
| `POST`/`DELETE` | `/invite/blocklist/{user_id}` | 单个增删 |
| `GET`/`POST` | `/invite/allowlist` | 白名单，同构 |
| `POST`/`DELETE` | `/invite/allowlist/{user_id}` | 单个增删 |

> 这里的写入响应是 `{"status": "ok", "action": "set_global_invite_blocklist"}`
> ——**这是全仓少数带 `status` 的成功响应**，客户端别把整个管理面都当成这个形状。

### 举报相关限流

| 方法 | 端点 |
|---|---|
| `GET` | `/event_reports/rate_limit/{user_id}` |
| `POST` | `/event_reports/rate_limit/{user_id}/block` |
| `POST` | `/event_reports/rate_limit/{user_id}/unblock` |

---

## 房间管理

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/rooms` | 房间列表（`search_term`、`limit`、`from`、`order_by`） |
| `GET` | `/rooms/{room_id}` | 详情 |
| `DELETE` | `/rooms/{room_id}` | 删除房间 |
| `POST` | `/rooms/{room_id}/purge_history` | 按房间清理历史 |
| `POST` | `/purge_history` | 清理历史（体含 `room_id` + 截断点） |
| `POST` | `/shutdown_room` | 关闭房间并移除成员 |
| `POST` | `/purge_room` | 彻底清除房间 |
| `GET` | `/rooms/{room_id}/members` | 成员列表 |
| `GET` | `/rooms/{room_id}/state` | 状态 |
| `GET` | `/rooms/{room_id}/messages` | 消息 |
| `GET` | `/rooms/{room_id}/event_context/{event_id}` | 事件上下文 |
| `GET` | `/rooms/{room_id}/aliases` | 别名 |
| `GET` | `/rooms/{room_id}/listings` | 目录条目 |
| `PUT` | `/rooms/{room_id}/listings/public` | 设公开 |
| `GET` | `/rooms/{room_id}/media` | 房间媒体 |
| `POST` | `/rooms/{room_id}/media/quarantine` / `unquarantine` | 隔离/解隔离 |
| `POST` | `/rooms/{room_id}/block` / `unblock`（`DELETE /rooms/{room_id}/block`） | 阻断房间 |
| `POST` | `/rooms/{room_id}/kick` / `ban` / `unban/{user_id}` | 管理成员 |
| `PUT` | `/rooms/{room_id}/members/{user_id}` | 改成员状态 |
| `POST`/`PUT` | `/rooms/{room_id}/make_admin` | 指定房间管理员 |
| `POST` | `/rooms/{room_id}/cascade_redact` | 级联撤回 |
| `POST` | `/rooms/{room_id}/backfill` | 触发回填 |
| `GET` | `/rooms/{room_id}/forward_extremities` | 前向极值 |
| `GET` | `/rooms/{room_id}/token_sync` | token 同步状态 |
| `GET` | `/rooms/{room_id}/version` | 房间版本 |
| `GET`/`POST` | `/rooms/search` | 房间搜索 |
| `GET` | `/rooms/{room_id}/reports` | 房间举报 |
| `POST` | `/rooms/cleanup` | 清理房间数据 |

**`POST /purge_history`** 体：

```json
{ "room_id": "!x:matrix.test", "purge_up_to_ts": 1760000000000, "dry_run": false }
```

- 截断点 `purge_up_to_ts` 与 `purge_up_to_event_id` **二选一且必须给其一**：都缺省 → 400，
  同时提供 → 400。给 `purge_up_to_event_id` 时服务端解析该事件的 `origin_server_ts`
  （事件不存在或不属于该房间 → 404）。
- `delete_local_events`（上游字段）可传但**不生效**：始终只清远端/联邦事件，且永不删
  `m.room.create`。
- 同步实现：无 `purge_id`，也无 `purge_history_status` 轮询端点。上游路径
  `/_synapse/admin/v1/purge_history/{room_id}` 本项目未实现，请用 `/purge_history` 并在体内传
  `room_id`。

响应：`{"success": true, "deleted_events": 1234, "dry_run": false}`

**`POST /shutdown_room`** 体：`{"room_id": "..."}`

响应：`{"kicked_users": [], "failed_to_kick_users": [], "closed_room": true}`

> ⚠️ `kicked_users` / `failed_to_kick_users` **恒为空数组**（实现里成员移除在存储层完成，
> 没有回传名单）。不要用这两个列表判断"谁被踢了"——需要名单请调 `/rooms/{id}/members` 对比。

---

## 媒体管理

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/rooms/{room_id}/media` | 房间媒体列表 |
| `GET` | `/users/{user_id}/media` | 用户媒体列表 |
| `POST` | `/media/quarantine/{server_name}/{media_id}` | 隔离媒体 |
| `POST` | `/media/unquarantine/{server_name}/{media_id}` | 解隔离 |
| `POST` | `/media/protect/{media_id}` / `/media/protect/{server_name}/{media_id}` | 保护（不被清理） |
| `POST` | `/media/unprotect/{media_id}` | 取消保护 |
| `POST` | `/media/delete` | 删除媒体 |
| `GET` | `/media/quarantine_changes` | 隔离变更日志 |
| `GET` | `/quarantine_media/{media_id}/changes` | 单个媒体的变更 |
| `POST` | `/purge_media_cache` | 清缓存 |
| `GET`/`POST` | `/media_callbacks` | 媒体回调 |

**注意**：管理面**没有** `/media_stats`。要看媒体用量：
`GET /_matrix/media/v1/quota/stats`（用户视角）或 `GET /media/quarantine_changes`。

被隔离的媒体对普通下载返回 **404**（不是 403），避免泄露"存在但被隔离"。

---

## 举报管理

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/event_reports` | 列表（`limit`、`since_ts`、`since_id`、`since_score`） |
| `GET` | `/event_reports/{id}` | 详情 |
| `PUT` | `/event_reports/{id}` | 更新 |
| `DELETE` | `/event_reports/{id}` | 删除 |
| `POST` | `/event_reports` | 创建（内部/兼容） |
| `POST` | `/event_reports/{id}/resolve` / `dismiss` / `escalate` | 处置 |
| `GET` | `/event_reports/count` | 计数 |
| `GET` | `/event_reports/stats` | 统计 |
| `GET` | `/event_reports/room/{room_id}` | 按房间 |
| `GET` | `/event_reports/event/{event_id}` | 按事件 |
| `GET` | `/event_reports/reporter/{reporter_user_id}` | 按举报人 |
| `GET` | `/event_reports/status/{status}`、`/status/{status}/count` | 按状态 |
| `GET` | `/reports`、`/reports/{report_id}`、`DELETE /reports/{report_id}` | 旧版兼容入口 |

> **分页参数**是 `since_ts` / `since_id` / `since_score`（keyset），**不是 `offset`**。
> 深分页用 `offset` 会 400。

---

## 注册控制

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/register/nonce` | 取 nonce（共享密钥注册第一步） |
| `POST` | `/register` | 共享密钥注册管理员/用户 |
| `GET` | `/registration_tokens` | 令牌列表 |
| `GET` | `/registration_tokens/{token}` | 单个 |
| `POST` | `/registration_tokens` | 新建 |
| `POST` | `/registration_tokens/{token}` | 更新 |
| `GET` | `/password_auth_providers` / `POST` | 密码认证提供方 |

**令牌对象**：

```json
{ "token": "abcd1234", "uses_allowed": 10, "pending": 0, "completed": 3 }
```

`uses_allowed` 为 `null` 表示不限次数（内部 `max_uses = 0`）。`pending` 目前恒为 `0`。

**共享密钥注册**流程：`GET /register/nonce` → HMAC(secret, nonce + user + password) →
`POST /register` 带 `nonce` + `mac` + 用户信息。**与公开注册端点无关**，是创建管理员的唯一途径。

---

## 后台任务与清理

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/background_updates` | 列表 |
| `GET` | `/background_updates/status`、`/status/{status}/count` | 状态 |
| `GET` | `/background_updates/stats` | 统计 |
| `GET` | `/background_updates/count`、`/next`、`/pending`、`/running` | 队列视图 |
| `POST` | `/background_updates` | 注册任务 |
| `POST` | `/background_updates/retry_failed` | 重试失败项 |
| `POST` | `/background_updates/cleanup_locks` | 清理死锁 |
| `GET`/`POST`/`DELETE` | `/background_updates/{job_name}` | 单任务 |
| `POST` | `/background_updates/{job_name}/start` / `complete` / `fail` / `progress` / `cancel` | 生命周期 |
| `GET` | `/background_updates/{job_name}/history` | 历史 |
| `POST` | `/cleanup/all`、`/cleanup/rooms`、`/cleanup/tokens` | 清理 |
| `POST` | `/captcha/cleanup` | 清验证码 |
| `GET`/`POST`/`DELETE` | `/retention/policy`、`/retention/policy/{room_id}` | 保留策略 |
| `GET` | `/retention/status`、`POST /retention/run` | 状态/触发 |
| `GET`/`POST` | `/account_data_callbacks`、`/media_callbacks` | 回调注册 |

---

## 联邦管理

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/federation/destinations` | 目的端列表 |
| `GET`/`POST`/`DELETE` | `/federation/blacklist[/{server_name}]` | 黑名单 |
| `GET` | `/federation/cache` | 缓存视图 |
| `POST` | `/federation/cache/clear`、`DELETE /federation/cache/{key}` | 清缓存 |
| `POST` | `/federation/confirm` | 确认目的端 |
| `POST` | `/federation/resolve` | 解析 server |
| `POST` | `/federation/rewrite` | 改写目的端 |
| `POST` | `/federation/destinations/{destination}/reset_connection` | 重置连接 |
| `GET` | `/appservices/*` | AS 注册与状态（`/appservices/{as_id}/...`） |
| `GET`/`POST`/`PUT`/`DELETE` | `/external_services[/{as_id}]` | 外部服务 |

---

## 审计与可观测

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/audit/events` | 审计事件列表 |
| `GET` | `/audit/events/{event_id}` | 单条 |
| `POST` | `/audit/events` | 写入（内部） |
| `GET` | `/modules`、`/modules/{module_name}` | 模块列表 |
| `GET` | `/modules/logs/{module_name}` | **模块日志**（唯一的日志读取入口） |
| `POST` | `/modules/{module_name}/enable`、`PUT /modules/{module_name}/config` | 模块开关/配置 |
| `GET` | `/modules/spam_check/{event_id}`、`/modules/spam_check/sender/{sender}` | 反垃圾检查 |
| `GET` | `/modules/third_party_rule/{event_id}`、`/modules/type/{module_type}` | 第三方规则 |
| `POST` | `/modules/check_spam`、`/modules/check_third_party_rule` | 主动检查 |
| `GET` | `/feature-flags`、`/feature-flags/{flag_key}` | 功能开关 |
| `POST`/`PATCH` | `/feature-flags`、`/feature-flags/{flag_key}` | 写开关 |
| `GET` | `/policy/status`、`POST /policy/check` | 策略 |
| `GET`/`POST` | `/push/config`、`/push/cleanup`、`/push/process` | 推送管理 |

> ⚠️ **不存在** `GET /_synapse/admin/v1/logs`。要看日志只能按模块：
> `/modules/logs/{module_name}`。全局日志走外部日志系统（容器 stdout）。

---

## 模块 / 遥测 / 策略 / 验证码 / Jitsi

### 模块 `/modules`

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/modules` | 注册模块，**返回 201**。体：`module_name`、`module_type`、`version` 必填；`description`、`is_enabled`、`priority`、`config` 可选 |
| `GET` | `/modules` | `{"modules": [...], "next_batch"}`；`limit` 默认 `100`、clamp `1..=500` |
| `GET` | `/modules/{module_name}` | 单个模块；不存在 → 404 |
| `GET` | `/modules/type/{module_type}` | 该类型模块数组（裸数组） |
| `PUT` | `/modules/{module_name}/config` | 体：`{"config": {...}}`（`config` 必填） |
| `POST` | `/modules/{module_name}/enable` | 体：`{"is_enabled": true\|false}` |
| `DELETE` | `/modules/{module_name}` | **204 无 body**（不是 `{}`） |
| `POST` | `/modules/check_spam` | 体 5 字段全必填：`event_id`、`room_id`、`sender`、`event_type`、`content` |
| `POST` | `/modules/check_third_party_rule` | 体 6 字段全必填：上面 5 个 + `state_events: []` |
| `GET` | `/modules/spam_check/{event_id}` | 单条结果；不存在 → 404 |
| `GET` | `/modules/spam_check/sender/{sender}` | 结果数组；`limit` 默认 `100`（**不 clamp**） |
| `GET` | `/modules/third_party_rule/{event_id}` | 规则结果数组 |
| `GET` | `/modules/logs/{module_name}` | 执行日志数组；`limit` 默认 `100` |

结果对象字段：

```jsonc
// SpamCheckResultResponse
{ "id": 1, "event_id": "$e", "room_id": "!r", "sender": "@u", "event_type": "m.room.message",
  "content": {...}|null, "result": "...", "score": 0, "reason": "...|null", "checker_module": "..." }

// ThirdPartyRuleResultResponse
{ "id": 1, "event_id": "$e", "room_id": "!r", "sender": "@u", "event_type": "...",
  "rule_name": "...", "allowed": true, "reason": "...|null", "modified_content": {...}|null, "checked_ts": 0 }
```

同一路由文件还注册了四个**不在 `/modules` 下**的家族（TOC 里未单列，见下表）：

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/account_validity` | 体：`user_id`、`expiration_ts` 必填，`is_valid` 可选 |
| `GET` | `/account_validity/{user_id}` | 账户有效期；不存在 → 404 |
| `POST` | `/account_validity/{user_id}/renew` | 体：`renewal_token`、`new_expiration_ts` |
| `POST`/`GET` | `/password_auth_providers` | 注册 / 列出密码认证提供者 |
| `POST`/`GET` | `/media_callbacks` | 注册 / 列出媒体回调 |
| `GET` | `/media_callbacks/{callback_type}` | 按类型列出 |
| `POST`/`GET` | `/account_data_callbacks` | 注册 / 列出 Account Data 回调 |

### 遥测 `/telemetry/*`

| 方法 | 端点 | 响应 |
|---|---|---|
| `GET` | `/telemetry/status` | `{enabled, trace_enabled, metrics_enabled, service_name, service_version, sampling_ratio, export_config:{otlp_endpoint, prometheus_port, prometheus_path, batch_export}}` |
| `GET` | `/telemetry/attributes` | `{"attributes": {"k": "v"}}` |
| `GET` | `/telemetry/metrics` | 指标摘要，见下 |
| `GET` | `/telemetry/health` | `{status, service, trace_enabled, metrics_enabled, checks, database, alerts}` |
| `GET` | `/telemetry/alerts` | `{"alerts": [...]}`；query：`status`、`severity`、`refresh`（默认 `true`，会先同步健康状态） |
| `POST` | `/telemetry/alerts/{alert_id}/ack` | 返回被确认的 alert 对象 |

`/telemetry/metrics` **是给人看的 JSON 摘要，不是 Prometheus 抓取点**：

```json
{
  "total_metrics": 120, "total_counters": 80, "total_gauges": 30, "total_histograms": 10,
  "rendered_bytes": 4096, "snapshot_ts": 1729100000000,
  "appservice_scheduler": {
    "total_services": 2, "scheduler_available_services": 2,
    "services_in_backoff": 0, "services_capacity_limited": 0,
    "services_with_pending_transactions": 1,
    "total_pending_events": 3, "total_pending_transactions": 1,
    "total_success_count": 12, "total_failure_count": 0,
    "total_backoff_count": 0, "total_capacity_limited_count": 0, "total_in_flight_count": 0
  },
  "prometheus_scrape_target": { "port": 9000, "path": "/metrics", "note": "..." }
}
```

- `prometheus_scrape_target` 为 `null` 表示独立抓取端口未监听
  （`telemetry.prometheus.enabled = false`，默认关闭）。
- `rendered_bytes` 只是 Prometheus 文本的**字节数**，不含文本本身。

### 策略 `/policy/*`（MSC4284）

| 方法 | 端点 | 响应 |
|---|---|---|
| `GET` | `/policy/status` | `{"enabled": bool, "endpoint": string\|null, "fail_mode": "fail_open"\|"fail_closed"}` |
| `POST` | `/policy/check` | 体：`room_id`、`user_id`、`action` 全必填；响应 `{"result": "allow", "allowed": true}` 或 `{"result": "deny", "allowed": false, "reason": "..."}` |

- `action` 只接受 `create` / `join` / `invite` / `send`；其他值 → 400 `M_BAD_JSON`。
- `room_id` / `user_id` trim 后为空 → 400 `M_BAD_JSON`。
- 请求体 `deny_unknown_fields`：多传字段直接 400。

### 验证码管理

| 方法 | 端点 | 响应 |
|---|---|---|
| `POST` | `/captcha/cleanup` | `{"cleaned_count": n, "message": "Cleaned up n expired captchas"}` |

客户端面同一服务实现还有 4 条（本仓扩展，**不需要**管理员 token）：

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/_matrix/client/v3/register/captcha/send` | 体：`captcha_type`、`target`、`template_name?` → `{captcha_id, expires_in, captcha_type}` |
| `POST` | `/_matrix/client/v3/register/captcha/verify` | 体：`captcha_id`、`code` → `{"verified": bool}` |
| `GET` | `/_matrix/client/v3/register/captcha/status` | query `captcha_id` → `{captcha_id, captcha_type, target, status, attempt_count, max_attempts, expires_at, created_at}` |
| `DELETE` | `/_matrix/client/v3/register/captcha/clean` | 与 `/captcha/cleanup` 同实现，但**无管理员鉴权** |

> `status` 响应里的时间字段是 `expires_at` / `created_at`（不是项目通用的 `_ts` 后缀），
> 且 `created_at` 的值来自 `created_ts`。按字面读即可，不要假设命名一致。

### Jitsi

`GET /_synapse/admin/v1/jitsi/config`：

```json
{ "domain": null, "app_id": null, "jwt_enabled": false, "jwt_asap_enabled": false,
  "jwt_auth_type": "none", "server_name": "matrix.test" }
```

本仓**不硬编码任何第三方 Jitsi 实例**：`domain` 恒为 `null`，表示"未配置"。
客户端应把 `null` 当作"不启用通话 UI"，不要回退到 `meet.jit.si`。

---

## Application Service 管理

`/_synapse/admin/v1/appservices/*` 共 17 条（`app_service.rs`），全部需管理员 token。

| 方法 | 端点 | 响应 / 说明 |
|---|---|---|
| `GET` | `/appservices` | AS 对象**裸数组**（只含 active） |
| `POST` | `/appservices` | 注册，**201** + AS 对象 |
| `GET` | `/appservices/{as_id}` | 单个；不存在 → 404 |
| `PUT` | `/appservices/{as_id}` | 更新，返回 AS 对象 |
| `DELETE` | `/appservices/{as_id}` | **204 无 body** |
| `POST` | `/appservices/{as_id}/ping` | `{"as_id", "alive": bool}`；`url: null` 的 AS 返回 `alive: false` |
| `GET` | `/appservices/{as_id}/state` | 状态数组 |
| `POST` | `/appservices/{as_id}/state` | 体：`state_key`、`state_value` → `{as_id, state_key, state_value, updated_ts}` |
| `GET` | `/appservices/{as_id}/state/{state_key}` | 同上；不存在 → 404 |
| `GET` | `/appservices/{as_id}/users` | 虚拟用户裸数组 |
| `POST` | `/appservices/{as_id}/users` | 体：`user_id` 必填，`displayname`/`avatar_url` 可选；**201** + `{as_id, user_id, displayname, avatar_url, created_ts}` |
| `GET` | `/appservices/{as_id}/namespaces` | 命名空间裸数组 |
| `GET` | `/appservices/{as_id}/events?limit=` | 待推送事件裸数组；`limit` 默认 `100`、clamp `1..=500` |
| `POST` | `/appservices/{as_id}/events` | 体：`room_id`、`event_type`、`sender`、`content`、`state_key?`；**201** + `{event_id, as_id, room_id, event_type}` |
| `GET` | `/appservices/query/user?user_id=` | `{user_id, application_service, exists}` |
| `GET` | `/appservices/query/alias?alias=` | `{alias, application_service, exists}` |
| `GET` | `/appservices/statistics` | 统计**裸数组**，每项带 `as_id` 与嵌套 `scheduler` 对象 |

AS 对象（`AppServiceResponse`）关键字段：

```json
{ "id": 1, "as_id": "my-as", "url": "https://as.example/hs"|null, "sender": "@as:matrix.test",
  "description": null, "rate_limited": false, "protocols": [], "is_enabled": true }
```

> `url` 在未配置推送端点时是 **JSON `null`**，不是空字符串。
> `rate_limited` 是 `is_rate_limited` 的**重命名**字段——请求/响应都用 `rate_limited`。

---

## 服务端管理（功能开关 / 保留策略 / 外部服务）

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/spaces`、`/spaces/{space_id}`、`/spaces/{space_id}/rooms`、`/spaces/{space_id}/users`、`/spaces/{space_id}/stats` | 空间管理 |
| `GET` | `/jitsi/config` | Jitsi 配置 |
| `GET` | `/push/config` | 推送配置 |
| `GET` | `/policy/status` | 策略状态 |
| `POST` | `/restart` | 重启服务 |

---

## 通知与服务器通告

> 本族受编译特性 `server-notifications` 门控；默认特性集下这些路径返回 **404**。
> 参考部署（`--features all-extensions`）全部可用。共 15 条。

### 站内通知（7 条）

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/notifications` | 创建通知 |
| `GET` | `/notifications` | 列表（`audience`、`limit`、`from`） |
| `GET` | `/notifications/{notification_id}` | 单条；不存在 → 404 |
| `PUT` | `/notifications/{notification_id}` | 部分更新；不存在 → 404 |
| `DELETE` | `/notifications/{notification_id}` | `{}`；不存在 → 404 |
| `PUT` | `/notifications/{notification_id}/deactivate` | 下线，返回 `{"is_enabled": false}`；不存在 → 404 |
| `GET` | `/notifications/active` | 当前生效通知**裸数组** |

**创建体**（`CreateNotificationRequest`）：

```jsonc
{ "title": "...", "content": "...",
  "notification_type": "...", "priority": "...",
  "target_audience": "...", "target_user_ids": ["@u:matrix.test"],
  "starts_at": 1760000000000, "expires_at": 1760100000000,
  "is_dismissable": true, "action_url": "...", "action_text": "...",
  "created_by": "@admin:matrix.test" }
```

只有 `title` / `content` 是核心字段，其余为可选。

**列表 query**：`audience` / `limit` / `from`（`deny_unknown_fields`，多传字段 → 400）。
`limit` 默认 **50**、上限 **100**；`from` 非法 → **400 `Invalid from cursor`**。
响应 `{"notifications": [...], "next_batch": "..."}`，游标内部形如 `<created_ts>|<id>`。

**通知对象**：

```jsonc
{ "id": 1, "title": "...", "content": "...",
  "notification_type": "...", "priority": "...",
  "target_audience": "...", "target_user_ids": [...],
  "starts_at": 1760000000000, "expires_at": null,
  "is_enabled": true, "is_dismissable": true,
  "action_url": null, "action_text": null,
  "created_by": "@admin:matrix.test", "created_ts": 1760000000000, "updated_ts": 1760000000000 }
```

> `GET /notifications/active` 与其他列表不同，**直接返回数组**，没有 `next_batch`。

### 服务器通告（4 条）

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/send_server_notice` | 向指定用户发送服务器通告 |
| `GET` | `/server_notices` | 列表（`limit`、`from`） |
| `GET` | `/server_notices/{notice_id}` | 单条；不存在 → 404 |
| `DELETE` | `/server_notices/{notice_id}` | `{}` |

**发送体**（`deny_unknown_fields`）：

```json
{ "user_id": "@alice:matrix.test", "content": { "msgtype": "m.text", "body": "..." } }
```

目标用户不存在 → **404**。响应 `{"event_id": "$...", "room_id": "!...", "notice_id": "..."}`。

**列表 query**：`limit` 默认 **10**、上限 **50**；响应
`{"notices": [...], "total": N, "next_batch": "..."}`。

### 用户通知开关与 pushers（4 条）

| 方法 | 端点 | 体 / 响应 |
|---|---|---|
| `GET` | `/users/{user_id}/notification` | `{"enabled": bool}` |
| `PUT` | `/users/{user_id}/notification` | 体 `{"is_enabled": bool}`（`deny_unknown_fields`）→ `{"is_enabled": bool}` |
| `GET` | `/users/{user_id}/pushers` | `{"pushers": [...], "total": N}` |
| `DELETE` | `/users/{user_id}/pushers/{pushkey}` | `{}`；不存在 → 404 |

> 读接口回 `enabled`、写接口收发 `is_enabled` —— **两个不同的键名**，SDK 不要复用同一个模型。

---

## SAML 管理

> 本族受编译特性 `saml-sso` 门控；默认特性集下返回 **404**。共 8 条。
> 客户端登录/回调面见 [authentication.md](./authentication.md)。

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/saml/metadata/refresh` | 重新拉取 IdP 元数据 → `{entity_id, sso_url, slo_url, certificate}`；SAML 未启用 → 403 |
| `GET` | `/saml/config` | 当前生效配置 |
| `PUT` | `/saml/config` | 传入部分配置，应用运行时覆盖后返回**合并结果** |
| `GET` | `/saml/mappings` | 映射分页列表 |
| `GET` | `/saml/mapping/{name_id}` | 单个映射；不存在 → 404 |
| `PUT` | `/saml/mapping/{name_id}` | 更新映射；不存在 → 404 |
| `DELETE` | `/saml/mapping/{name_id}` | `{"removed": n}`；`n == 0` → 404 |
| `POST` | `/saml/logout` | 管理员发起登出 |

**分页**（`GET /saml/mappings`）：query 仅 `limit` / `from`（`deny_unknown_fields`）。
`limit` 默认 **50**、clamp **1..=500**；`from` 是 `name_id` 游标。
响应 `{"mappings": [...], "next_token": "..."}`，**仅当返回行数等于 `limit`** 时才给
`next_token`（值为最后一条 `name_id`）。

**映射对象**：

```jsonc
{ "name_id": "user@idp.example", "user_id": "@alice:matrix.test",
  "issuer": "https://idp.example/metadata",
  "first_seen_ts": 1760000000000, "last_authenticated_ts": 1760000000000,
  "authentication_count": 3, "attributes": { "email": ["..."] } }
```

**更新体**（`deny_unknown_fields`）：`{"user_id"?: "...", "attributes"?: {...}}`。
两者都缺 → **400**（`At least one of user_id / attributes must be provided`）。

**登出体**：`{"user_id": "@alice:matrix.test"}`。
空字符串 → **400**；SAML 未启用 → **403**。响应：

```jsonc
{ "user_id": "@alice:matrix.test", "redirect_url": "https://idp.example/slo...", "sessions_invalidated": 1 }
```

无会话时不报错，返回 `redirect_url: null`、`sessions_invalidated: 0`。

---

## CAS 管理

> 本族受编译特性 `cas-sso` 门控；默认特性集下返回 **404**。共 5 条。
> 客户端重定向与 `/_synapse/cas/*` 协议面见 [authentication.md](./authentication.md)。

| 方法 | 端点 | 响应 / 说明 |
|---|---|---|
| `POST` | `/cas/services` | 注册服务，**201** + 服务对象 |
| `GET` | `/cas/services` | 服务对象**裸数组** |
| `DELETE` | `/cas/services/{service_id}` | **204 无 body** |
| `POST` | `/cas/users/{user_id}/attributes` | 体 `{attribute_name, attribute_value}` → `{user_id, attribute_name, attribute_value}` |
| `GET` | `/cas/users/{user_id}/attributes` | **裸数组** `[{"name": "...", "value": "..."}]` |

**注册体**：

```jsonc
{ "service_id": "my-service", "name": "My Service", "description": null,
  "service_url_pattern": "https://app.example/*",
  "allowed_attributes": ["mail"], "allowed_proxy_callbacks": [],
  "require_secure": true, "single_logout": false }
```

**服务对象**：`{service_id, name, description, service_url_pattern, is_enabled}`。

> CAS 未配置时，这 5 条同样会被 `cas_config_check_middleware` 拦截，返回
> **500**（`CAS service is not available...`）——而不是 404/403。属性读接口的元素键是
> `name` / `value`，写接口的键是 `attribute_name` / `attribute_value`，两者不对称。

---

## 管理端已知形状差异

这几条是接入管理面最容易踩的坑：

| 差异 | 说明 |
|---|---|
| **401 的 errcode** | 管理面缺 token / token 无效都是 `M_UNAUTHORIZED`；client API 同样以 `M_UNAUTHORIZED` 为主，只有 `whoami` 显式返回 `M_MISSING_TOKEN` / `M_UNKNOWN_TOKEN` |
| **`{"status":"ok"}` 确实存在** | 仅 `/invite/blocklist`、`/invite/allowlist` 等少数写端点如此；其余平铺 |
| **偏移分页已废** | `/users` 等传 `offset>0` → 400 `M_BAD_JSON` |
| **`/status` 只有三个布尔** | 数量统计在 `/statistics` |
| **`server_version` 无 `python_version`** | 字段名是 `server_version` + `implementation` |
| **`shutdown_room` 名单恒空** | 成员确实被移除，但响应不给名单 |
| **`/server_stats`、`/media_stats`、`/logs`、`/security/*` 不存在** | 分别映射到 `/statistics`+`/room_stats`、`/media/*`、`/modules/logs/*`、`/users/{id}/shadow_ban` + `/users/{id}/rate_limit` + `/invite/blocklist` |
| **举报分页是 keyset** | `since_ts` / `since_id` / `since_score` |
