# 认证、注册与账户

覆盖从"检查用户名"到"停用账户"的完整生命周期。所有端点以 `/_matrix/client/v3/` 为默认前缀，
`v1` 变体在表中单独标出。

## 目录

- [认证模型](#认证模型)
- [注册](#注册)
- [登录](#登录)
- [登出与 refresh](#登出与-refresh)
- [当前用户 whoami](#当前用户-whoami)
- [用户资料 Profile](#用户资料-profile)
- [修改密码](#修改密码)
- [停用账户](#停用账户)
- [第三方标识 3PID](#第三方标识-3pid)
- [Guest 访客](#guest-访客)
- [QR 登录令牌](#qr-登录令牌)
- [客户端易错点清单](#客户端易错点清单)

---

## 认证模型

1. **只有 `Authorization: Bearer <access_token>`** 会被读取；`?access_token=` 被忽略（401）。
2. `refresh_token` **只从请求体读**（见下），不接受放在 `Authorization` 头。
3. 登录成功后应保存 `access_token` + `refresh_token` + `device_id` + `user_id`；
   `expires_in` 单位是**毫秒**（与 `expires_in_ms` 同义，字段名沿用 `expires_in`）。
4. 需要用户二次验证的操作走 **UIA**：不带 `auth` 时返回 **401** + `flows`/`session`，
   客户端选一个 flow 带上 `auth` 重试。errcode 为 **`M_UIA_REQUIRED`**。

---

## 注册

### 检查用户名可用性

**端点**：`GET /_matrix/client/v3/register/available?username=<localpart>`

**认证**：不需要

**响应（实测）**：

```json
{ "available": true, "username": "probe_sdk" }
```

- 用户名非法 → 400 `M_INVALID_USERNAME`（不是 `available: false`）。
- 用户名已被占用 → 400 `M_USER_IN_USE`。
- 配置 `inhibit_user_in_use_error=true` 时，合法 localpart 一律报 `available: true`
  且**不查库**（防止该端点在注册关闭时充当成员探测口）；真正注册时仍会因占用被拒。

### 创建账户

**端点**：`POST /_matrix/client/v3/register`

**认证**：不需要（UIA 自行管理）

**请求体**：

```typescript
interface RegisterRequest {
  username?: string;                    // 省略则服务端自动生成 localpart（auto+12 位 hex）
  password: string;                     // 非 guest 必填
  auth?: { type: 'm.login.dummy' | 'm.login.password'; session?: string; password?: string };
  displayname?: string;                 // ⚠️ 是 displayname，不是 display_name
  initial_device_display_name?: string;
  kind?: 'guest';                       // 走访客注册分支
  refresh_token?: boolean;              // 是否发放 refresh_token
}
```

> **字段名陷阱**：本服务读的是 `displayname`（`auth_compat.rs` 中
> `body.get("displayname")`）。传 `display_name` 会被**静默忽略**，资料不会被写入。
> 这与 `PUT /profile/{user_id}/displayname` 的字段名一致，但与部分 SDK 文档不一致。

**UIA：不带 `auth` 必得 401**（实测行为，`P-038`）：

```http
HTTP/1.1 401 Unauthorized
```
```json
{
  "flows": [{ "stages": ["m.login.dummy"] }, { "stages": ["m.login.password"] }],
  "params": {},
  "session": "e0f1c0d4-...-..."
}
```

客户端照 Matrix 语义处理：把 `session` 原样放回下一次请求的 `auth.session`，并选择 dummy
或 password 完成注册。**不要**把"401"当作"用户名密码错误"。

**成功响应**（与登录同一形状，平铺，无信封）：

```json
{
  "access_token": "...",
  "refresh_token": "...",
  "expires_in": 3600000,
  "device_id": "ABCDEFGHIJ",
  "user_id": "@alice:matrix.test",
  "well_known": { "m.homeserver": { "base_url": "https://matrix.test" } }
}
```

**访客注册**（`kind: "guest"` 或 `?kind=guest`）额外带 `"is_guest": true`，且不返回 `refresh_token`。

**安全相关行为**

| 行为 | 说明 |
|---|---|
| `admin: true` 字段 | **被忽略**（仅记 warn 日志）。管理员必须走 `POST /_synapse/admin/v1/register`（共享密钥） |
| 注册关闭 | `server.enable_registration=false` 时 403 `M_FORBIDDEN` |
| 验证码 | 配置启用时先 `POST /register/captcha/send` → `verify`，状态见 `GET /register/captcha/status` |
| 邮件验证 | `POST /register/email/requestToken` / `submitToken` |

**常见错误**

| 场景 | 状态 | errcode |
|---|---|---|
| 用户名已占用 | 400 | `M_USER_IN_USE` |
| 用户名格式非法 | 400 | `M_INVALID_USERNAME` |
| 密码不满足策略 | 400 | `M_INVALID_PARAM`（`error` 说明具体规则） |
| 缺密码（非 guest） | 400 | `M_BAD_JSON`（`error: Password required`） |
| 未完成 UIA | 401 | `M_UIA_REQUIRED`（注册入口此处 errcode 体里给的是 flows 对象，不含 errcode 字段时按 401+flows 处理） |

### 应用服务注册虚拟用户（MSC4190）

应用服务（AS）用**自己的 `as_token`** 调 `POST /register`，为它命名空间内的用户建号：

```http
POST /_matrix/client/v3/register
Authorization: Bearer <as_token>
Content-Type: application/json

{ "username": "asbot_alice", "inhibit_login": true, "displayname": "AS Alice" }
```

```json
{ "user_id": "@asbot_alice:example.org", "well_known": { "m.homeserver": { "base_url": "..." } } }
```

判据（缺一条就偏离上游）：

| 条件 | 结果 |
|---|---|
| `inhibit_login` 为 `true` | 建用户；**响应里没有 `access_token` / `device_id`**（这条路径不登录） |
| 缺 `inhibit_login` 或为 `false` | **400 `M_APPSERVICE_LOGIN_UNSUPPORTED`** —— 服务器不会替 AS 登录 |
| `username` 缺失 | 400 `M_BAD_JSON`（AS 注册**不**自动生成 localpart） |
| 用户不在该 AS 的 `users` 命名空间 | 403 `M_FORBIDDEN` |

> **身份判定看 token，不看 body 里的 `type`**：token 属于某个 AS 才走这条分支（与上游一致）。
> 校验用的是**任一** `users` 命名空间条目，**不要求 `exclusive`** —— `exclusive` 只在
> `/login` 的 `m.login.application_service` 冒充时要求。
>
> 要拿 token 请走 `POST /login`：`{"type": "m.login.application_service", "identifier": {"type": "m.id.user", "user": "asbot_alice"}}`。

---

## 登录

### 登录方式发现

`GET /_matrix/client/v3/login` 返回服务端当前可用的 `flows`。实测见
[discovery.md](./discovery.md#登录流程发现-get-login)——**不要**内置固定列表。

### 密码登录

**端点**：`POST /_matrix/client/v3/login`

```typescript
interface LoginRequest {
  type: 'm.login.password';
  identifier?: { type: 'm.id.user'; user: string };  // 推荐写法
  user?: string;                                      // 兼容写法（本服务两者都认）
  password: string;
  device_id?: string;                                 // 省略则服务端生成
  initial_device_display_name?: string;
  refresh_token?: boolean;
}
```

**成功响应**：与注册相同（`access_token` / `refresh_token` / `expires_in` / `device_id` /
`user_id` / `well_known`）。

**其他 `type`**：`m.login.token`（QR/一次性 token）、`m.login.cas`、`m.login.sso`、
`m.login.application_service`（仅 AS，且不出现在 `/login` 的 `flows` 中）。

**错误**

| 场景 | 状态 | errcode |
|---|---|---|
| 用户名或密码错 | 403 | `M_FORBIDDEN` |
| 账户已停用 | 403 | `M_USER_DEACTIVATED` |
| 失败次数超限 | 429 | `M_LIMIT_EXCEEDED`（带 `retry_after_ms` 与限流头） |
| 缺 `password` | 400 | `M_BAD_JSON` |

> 登录失败计数按 `(client_ip, username)` 维度累计，登录成功后清零。429 时应退避到
> `retry_after_ms` 之后再试，不要立即重试。

---

## 登出与 refresh

### 登出当前设备

`POST /_matrix/client/v3/logout` — 认证：是 — 响应：`{}`

吊销**当前 token** 及其 refresh token。

### 登出全部设备

`POST /_matrix/client/v3/logout/all` — 认证：是 — 响应：`{}`

吊销该用户所有 token 与设备（等价于"退出所有会话"）。

### 刷新访问令牌

**端点**：`POST /_matrix/client/v3/refresh`

**认证**：**不需要** `Authorization` 头。**refresh token 放在请求体**：

```json
{ "refresh_token": "<refresh_token>" }
```

```typescript
async function refresh(base: string, refreshToken: string) {
  const res = await fetch(`${base}/_matrix/client/v3/refresh`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ refresh_token: refreshToken }),
  });
  if (!res.ok) throw await res.json();       // 401 M_UNAUTHORIZED “Invalid refresh token”
  return res.json() as Promise<{
    access_token: string; refresh_token: string; expires_in: number; device_id: string;
  }>;
}
```

**响应**：`{ access_token, refresh_token, expires_in, device_id }`

**实测两种错误**（这是最高频的接入错误）：

| 写法 | 结果 |
|---|---|
| `Authorization: Bearer <refresh_token>`，body 空 | 400 `M_BAD_JSON`，`error: Refresh token required` |
| body 里给错 token | 401 `M_UNAUTHORIZED`，`error: Invalid refresh token` |

> refresh 会**轮换** refresh token：响应里的新 `refresh_token` 必须覆盖旧的，
> 否则并发刷新会互相失效。

---

## 当前用户 whoami

`GET /_matrix/client/v3/account/whoami`（另有 `v1` 变体）

**响应（实测字段）**：

```json
{ "user_id": "@alice:matrix.test", "device_id": "ABCDEFGHIJ", "is_guest": false }
```

**错误**：缺 token → 401 `M_MISSING_TOKEN`；token 无效 → 401 `M_UNKNOWN_TOKEN`。

---

## 用户资料 Profile

**路径是 `/profile/{user_id}`，不是 `/account/profile/{user_id}`**——后者在本服务中不存在（404）。

| 方法 | 端点 | 认证 | 说明 |
|---|---|---|---|
| `GET` | `/profile/{user_id}` | 否 | 返回 `{displayname?, avatar_url?}` |
| `GET` | `/profile/{user_id}/displayname` | 否 | `{displayname}` |
| `GET` | `/profile/{user_id}/avatar_url` | 否 | `{avatar_url}` |
| `PUT` | `/profile/{user_id}/displayname` | 是（本人或管理员） | 体：`{"displayname": "..."}` |
| `PUT` | `/profile/{user_id}/avatar_url` | 是（本人或管理员） | 体：`{"avatar_url": "mxc://..."}` |
| `GET`/`PUT`/`DELETE` | `/profile/{user_id}/{key_name}` | 读：否；写：本人 | MSC4133 扩展字段 |
| `GET` | `/profile/{user_id}/...`（`v1` 变体） | 否 | 与 v3 同义 |
| `GET` | `/user_directory/profiles/{user_id}` | 是 | 目录视图（含共享房间判定） |

**没有** `PUT /profile/{user_id}`（整体替换）。只支持按字段写。

### 扩展 profile 字段（MSC4133）

`key_name` **不是**固定枚举（`about` / `birthday` 等由客户端约定），服务端只做长度与体积校验：

| 违反项 | 状态 | errcode |
|---|---|---|
| `key_name` > 255 字符 | 400 | `M_KEY_TOO_LARGE` |
| 写入后 profile 文档超上限 | 400 | `M_PROFILE_TOO_LARGE` |
| `key_name` 为空 | 400 | `M_MISSING_PARAM` |

能力发现：`capabilities.m.profile_fields.enabled`（当前 `true`）。

> `PUT /profile/{user_id}/avatar_url` 不会校验 `mxc://` 前缀之外的语义；传空字符串表示清除头像。

---

## 修改密码

**端点**：`POST /_matrix/client/v3/account/password`（另有 `v1`）

**请求体**：

```json
{
  "new_password": "s3cret-new",
  "auth": {
    "type": "m.login.password",
    "password": "<当前密码>",
    "session": "<可选>",
    "logout_devices": true
  }
}
```

> ⚠️ `logout_devices` 读的是 **`auth` 对象内部**，不是顶层字段
> （`account_compat.rs`：`auth.get("logout_devices")`）。放在顶层会被忽略。
> 默认 `true`（吊销所有其它会话）；置 `false` 仅改密码、保留其它设备登录态。

**UIA 语义（本仓实测行为）**

- 不带 `auth.type` → **401**，体为 `{errcode: "M_UIA_REQUIRED", error: "...", flows, params, session}`
  （`UiaService::get_password_change_flows()` 决定 `flows`）。
- `auth.type = "m.login.password"` 时校验 `auth.password`；`auth.identifier.user` /
  `auth.user` / `auth.user_id` 三种写法都接受，用于确定要改哪个账户。
- `auth.logout_devices`（默认 `true`）决定是否吊销其它设备。

| 场景 | 状态 | errcode |
|---|---|---|
| 缺 `new_password` | 400 | `M_BAD_JSON`（`New password required`） |
| 未完成 UIA | 401 | `M_UIA_REQUIRED` |
| 当前密码错 | 403 | `M_FORBIDDEN` |
| 新密码不达策略 | 400 | `M_INVALID_PARAM` |

邮箱辅助流程：`POST /account/password/email/requestToken` → `.../submitToken`。

---

## 停用账户

**端点**：`POST /_matrix/client/v3/account/deactivate`（另有 `v1`）

**认证**：是 + **UIA 必需**

```json
{ "auth": { "type": "m.login.password", "password": "<当前密码>" } }
```

不带 `auth` → 401 + `flows`/`session`（`require_deactivate_account_uia`）。

**停用会连带做四件事**（同一个后端流程，不是只改标记）：

1. 取消该用户全部 MSC4140 延迟事件；
2. 从所有房间 part；
3. 删除 pushers；
4. 吊销 token 与设备。

| 场景 | 状态 | errcode |
|---|---|---|
| 未完成 UIA | 401 | UIA 体（`flows` + `session`） |
| 已是停用状态 | 403 | `M_USER_DEACTIVATED` |

---

## 第三方标识 3PID

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/account/3pid` | 列出已绑定标识 → `{threepids: [{medium, address, validated_ts, added_at}]}` |
| `POST` | `/account/3pid` | 兼容入口（新增） |
| `POST` | `/account/3pid/add` | 用 `sid` 完成新增（体：`{"sid": "...", "client_secret": "..."}`） |
| `POST` | `/account/3pid/bind` | 绑定到当前账户 |
| `POST` | `/account/3pid/delete` | 删除（推荐用 `/unbind` 语义） |
| `POST` | `/account/3pid/unbind` | 解绑 |
| `POST` | `/account/3pid/email/requestToken` | 发验证邮件 |
| `POST` | `/account/3pid/email/submitToken` | 提交邮件里的 token |

`medium` 取 `email`。`GET` 的 `validated_ts` 未验证时为 `0`（不是 `null`）。

能力门控：`capabilities.m.3pid_changes.enabled`。

---

## Guest 访客

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/register/guest` | 等价于 `POST /register` 带 `kind=guest`；返回 `is_guest: true` |
| `GET` | `/account/guest` | 当前账户是否访客 |
| `POST` | `/account/guest/upgrade` | 访客升级为正式账户（需 UIA） |

访客注册同样受 `server.enable_registration` 控制（关闭则 403 `M_FORBIDDEN`）。

---

## QR 登录令牌

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/_matrix/client/v1/login/qr_token` | 已登录设备生成一次性 login token → `{login_token, expires_in_ms}` |
| — | `/_matrix/client/v1/rendezvous/{session_id}` 与 `.../messages` | MSC4108 rendezvous 通道（`POST` 建会话、`PUT` 发消息、`GET` 取状态） |

拿到 `login_token` 的新设备走 `POST /login` 且 `type: "m.login.token"`。

---

## 客户端易错点清单

| # | 易错点 | 正确做法 |
|---|---|---|
| 1 | 把注册成功响应当 `{status, data}` 解析 | 平铺取 `access_token`；`body.data` 是 `undefined` |
| 2 | 用 `display_name` 注册 | 用 `displayname` |
| 3 | refresh 用 `Authorization` 头 | 放 body：`{"refresh_token": ...}` |
| 4 | 把 `/account/profile/{id}` 当资料路径 | 用 `/profile/{id}` |
| 5 | 把注册 401 当"密码错" | 401 + `flows` = 需要 UIA，继续带 `auth.session` 重试 |
| 6 | 改密/停用不带 `auth` | 都会 401 + `flows`，先走 UIA |
| 7 | 忘掉 refresh 轮换 | 新 `refresh_token` 覆盖旧值 |
| 8 | 用 `?access_token=` 给 `<img>` 传 token | 用非鉴权 `/_matrix/media/v3/thumbnail/...`，或 fetch 成 blob |
