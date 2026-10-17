# 管理 API（`/_synapse/admin/v1`）

服务端管理面，**266 条路由**。本页覆盖客户端/控制台常用部分；完整清单以
`docs/openapi/route-table.json` 为准（`/_synapse/admin/v1/*`）。

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
- [服务端管理（功能开关 / 保留策略 / 外部服务）](#服务端管理功能开关--保留策略--外部服务)
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
| `POST` | `/purge_history` | 清理历史（兼容入口，体含 `room_id` + `purge_up_to_ts`） |
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

## 服务端管理（功能开关 / 保留策略 / 外部服务）

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/spaces`、`/spaces/{space_id}`、`/spaces/{space_id}/rooms`、`/spaces/{space_id}/users`、`/spaces/{space_id}/stats` | 空间管理 |
| `GET` | `/jitsi/config` | Jitsi 配置 |
| `GET` | `/push/config` | 推送配置 |
| `GET` | `/policy/status` | 策略状态 |
| `POST` | `/restart` | 重启服务 |

---

## 管理端已知形状差异

这几条是接入管理面最容易踩的坑：

| 差异 | 说明 |
|---|---|
| **401 的 errcode** | 管理面缺 token 是 `M_UNAUTHORIZED`；client API 是 `M_MISSING_TOKEN` |
| **`{"status":"ok"}` 确实存在** | 仅 `/invite/blocklist`、`/invite/allowlist` 等少数写端点如此；其余平铺 |
| **偏移分页已废** | `/users` 等传 `offset>0` → 400 `M_BAD_JSON` |
| **`/status` 只有三个布尔** | 数量统计在 `/statistics` |
| **`server_version` 无 `python_version`** | 字段名是 `server_version` + `implementation` |
| **`shutdown_room` 名单恒空** | 成员确实被移除，但响应不给名单 |
| **`/server_stats`、`/media_stats`、`/logs`、`/security/*` 不存在** | 分别映射到 `/statistics`+`/room_stats`、`/media/*`、`/modules/logs/*`、`/users/{id}/shadow_ban` + `/users/{id}/rate_limit` + `/invite/blocklist` |
| **举报分页是 keyset** | `since_ts` / `since_id` / `since_score` |
