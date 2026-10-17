# Application Service（AS）契约

Application Service 不是"另一种客户端"。它同时存在**两个方向**的调用：

| 方向 | 调用方 → 接收方 | 认证凭据 | 命名空间 |
|---|---|---|---|
| AS → homeserver | AS → HS | `as_token`（`Authorization: Bearer`） | `/_matrix/app/v1/*`、`/_matrix/client/v1/appservice/fed_proxy` |
| homeserver → AS | HS → AS | `hs_token`（写入转发请求的 `Authorization`） | `{as.url}/_matrix/app/v1/*` |
| 管理员 → HS | 运维工具 → HS | 管理员 access token | `/_synapse/admin/v1/appservices/*` |

> 端点存在性以 `docs/openapi/route-table.json` 为准。`/_matrix/app/v1/` 在路由表中计 **12 条**
> （其中 `ANY /proxy/{as_id}/{*path}` 展开为 7 个方法），逻辑端点只有 6 个。

## 目录

- [两类 token 的边界](#两类-token-的边界)
- [端点总览](#端点总览)
- [客户端面：命名空间查询](#客户端面命名空间查询)
- [回调面：`/_matrix/app/v1/*`](#回调面_matrixappv1)
- [MSC4512 代理](#msc4512-代理)
- [管理面：`/_synapse/admin/v1/appservices/*`](#管理面_synapseadminv1appservices)
- [错误与状态码](#错误与状态码)
- [SDK 接入建议](#sdk-接入建议)

---

## 两类 token 的边界

| Token | 谁持有 | 谁校验 | 用途 |
|---|---|---|---|
| `as_token` | AS | homeserver | AS 调 `/_matrix/app/v1/*` 时出示；也是 AS 身份调用 `fed_proxy` 的凭据 |
| `hs_token` | homeserver | AS | HS 把请求代理到 AS（`proxy_to_as`）时，用调用方提供的 `hs_token` 做 `Bearer` 认证 |

两者都**不会**出现在任何管理响应里：`AppServiceResponse` 只回显 `id/as_id/url/sender/description/rate_limited/protocols/is_enabled/created_ts`。
注册后只能由注册请求的提交方自行保存。

`url: null` 是合法注册（upstream 20273）：

- 该 AS 仍可用 `as_token` 认证、仍可被 `fed_proxy` 识别；
- 但**没有推送端点**：HS 不排队事件、`proxy_to_as` 直接失败（503）、`ping` 恒 `alive: false`。

---

## 端点总览

| 方法 | 端点 | 认证 | 返回 |
|---|---|---|---|
| `GET` | `/_matrix/client/v3/appservice/user` | 管理员 | `{user_id, application_service, exists}` |
| `GET` | `/_matrix/client/v3/appservice/alias` | 管理员 | `{alias, application_service, exists}` |
| `GET` | `/_matrix/client/v1/user/{user_id}/appservice` | 用户（仅本人） | `{user_id, appservices: []}` |
| `POST` | `/_matrix/app/v1/ping` | `as_token` | `{as_id}` |
| `PUT` | `/_matrix/app/v1/transactions/{as_id}/{txn_id}` | `as_token` | `{}` |
| `GET` | `/_matrix/app/v1/users/{user_id}` | `as_token` | `{}` |
| `GET` | `/_matrix/app/v1/rooms/{alias}` | `as_token` | `{}` |
| `GET` | `/_matrix/app/v1/{as_id}` | `as_token` | AS 配置摘要 |
| `ANY` | `/_matrix/app/v1/proxy/{as_id}/{*path}` | `hs_token` | 原样透传 AS 响应 |
| `ANY` | `/_matrix/client/v1/proxy/{as_id}/{*path}` | `hs_token` | 同上（别名） |
| `POST` | `/_matrix/client/v1/appservice/fed_proxy` | AS（`as_token`） | `{status, content}` |
| `POST` | `/_matrix/client/unstable/io.element.msc4512/appservice/fed_proxy` | AS（`as_token`） | 同上（unstable 别名） |

管理面 17 条见[管理面](#管理面_synapseadminv1appservices)。

---

## 客户端面：命名空间查询

### `GET /_matrix/client/v3/appservice/user?user_id=@bot:example.com`

需要**管理员** token（handler 额外要求 `AdminUser`，即使路由挂在公开路由表里）。

```jsonc
{
  "user_id": "@bot:example.com",
  "application_service": "my-bridge",   // 无归属时为 null
  "exists": true
}
```

`exists` 等价于 `application_service != null`，即该 user id 命中某个 AS 的 `users` 命名空间。

### `GET /_matrix/client/v3/appservice/alias?alias=%23room:example.com`

同上，走 `aliases` 命名空间：

```jsonc
{ "alias": "#room:example.com", "application_service": "my-bridge", "exists": true }
```

`alias` 需 URL 编码（`#` → `%23`）。

### `GET /_matrix/client/v1/user/{user_id}/appservice`

普通用户端点，**只能查自己**，否则 `403 M_FORBIDDEN`（`Access denied`）。
当前实现恒返回空列表：

```jsonc
{ "user_id": "@alice:matrix.test", "appservices": [] }
```

> SDK 不要依赖这个端点做"我被哪些 AS 管理"的判断；请改用管理面的 `query/user` 或
> 服务端配置。该端点存在只是为了兼容 Element 类客户端的探测逻辑。

---

## 回调面：`/_matrix/app/v1/*`

这一组端点由 homeserver 实现，供 **AS 自检/握手**使用；`Authorization` 必须是
`Bearer <as_token>`。缺失时是 `401 M_UNAUTHORIZED`（`Missing or invalid authorization header`）。

### `POST /_matrix/app/v1/ping`

```jsonc
// 200
{ "as_id": "my-bridge" }
```

### `PUT /_matrix/app/v1/transactions/{as_id}/{txn_id}`

请求体从 `events` 取数组（缺失或非数组按空数组处理）：

```jsonc
{ "events": [ /* 任意事件对象 */ ] }
```

- `as_token` 对应的 `as_id` 必须等于路径里的 `as_id`，否则 `403 M_FORBIDDEN`
  （`Application service ID mismatch`）。
- `txn_id` 不参与校验，仅作日志/幂等语义占位；成功返回 `{}`。

### `GET /_matrix/app/v1/users/{user_id}`

校验目标 user id 是否落在当前 AS 的 `users` 命名空间内：

- 命中 → `200 {}`；
- 未命中 → `403 M_FORBIDDEN`（`User not in application service namespace`）。

### `GET /_matrix/app/v1/rooms/{alias}`

同上，但走 `aliases` 命名空间；未命中 → `403`（`Room alias not in application service namespace`）。
路径里的 `#` 必须编码为 `%23`。

### `GET /_matrix/app/v1/{as_id}`

返回该 AS 的配置摘要（注意键名是 `id`，不是 `as_id`；`as_token`/`hs_token` 不回显）：

```jsonc
{
  "id": "my-bridge",
  "url": "https://as.example.com",
  "sender": "bridgebot",
  "description": "IRC bridge",
  "is_enabled": true,
  "protocols": ["irc"]
}
```

`url: null` 的 AS 这里返回 `"url": null`。

### `ANY /_matrix/app/v1/proxy/{as_id}/{*path}`

`/_matrix/client/v1/proxy/{as_id}/{*path}` 是同一 handler 的别名，两者行为完全一致。

- 认证：`Authorization: Bearer <hs_token>`，必须与该 AS 注册的 `hs_token` 逐字节相等，
  否则 `401 M_UNAUTHORIZED`（`Invalid HS token`）。
- AS 被禁用 → `400 M_BAD_JSON`（`Application service is disabled`）。
- `url: null` → `503`（无端点可转发）。
- 转发目标：`{url}/_matrix/app/v1/{path}`；入站的 `.../proxy/{as_id}` 前缀**不进入**上游路径。
- 转发时剔除 hop-by-hop 头（`connection`、`keep-alive`、`te`、`trailer`、
  `transfer-encoding`、`upgrade`、`proxy-authenticate`、`proxy-authorization`、`host`、
  `content-length`），响应头同样剔除后回传；状态码与响应体原样透传。
- 上游连接失败 → `500`（`Failed to forward request to application service ...`）。

---

## MSC4512 代理

MSC4512 有两个独立半边，**开关不同**：

| 半边 | 路由 | 开关 |
|---|---|---|
| 入站前缀代理 | 全局 fallback，匹配 AS 声明的 `proxy_prefix` | `experimental.msc4512_enabled: true` |
| 出站联邦代理 `fed_proxy` | `POST /_matrix/client/v1/appservice/fed_proxy`（无条件注册） | 不需要该开关 |

### 入站前缀代理

AS 在注册配置里声明 `proxy_prefix` + `proxy_url`（也接受
`io.element.msc4512.proxy_prefix` / `io.element.msc4512.proxy_url`）：

- 只允许声明 `rtc/livekit` 及其子路径（`ALLOWED_PROXY_PREFIXES`），其他前缀一律拒绝；
- 请求进入 `/_matrix/{client,federation}/<version>/<proxy_prefix>/...`，由 fallback 匹配并转发到 `proxy_url`；
- 客户端面注入 `X-Matrix-User-Identifier`，联邦面注入 `X-Matrix-Origin`；
- 转发时把 `Authorization` **替换**为 AS 的 `hs_token`，并剥离 `access_token` 查询参数；
- 开关关闭、前缀未声明或路径不匹配 → `404 M_UNRECOGNIZED`。

### `fed_proxy`

请求体（全部字段名区分大小写）：

```jsonc
{
  "destination": "remote.example.com",          // 必填、非空
  "method": "POST",                             // GET | POST | PUT | DELETE
  "path": "/_matrix/federation/v1/query/profile", // 必填，必须在 /_matrix/federation/ 下
  "body": { "user_id": "@a:remote" },            // 可选，必须是 JSON 对象
  "query": { "user_id": "@a:remote" }            // 可选，值必须全是字符串
}
```

响应：

```jsonc
{ "status": 200, "content": { /* 远端响应体 */ } }
```

校验与错误：

| 条件 | 状态 | errcode |
|---|---|---|
| 调用方不是 AS（`auth.appservice_id` 为空） | 403 | `M_FORBIDDEN` |
| 缺少 `destination` / `method` / `path` | 400 | `M_MISSING_PARAM` |
| 字段非字符串或为空串 | 400 | `M_INVALID_PARAM` |
| `method` 不在白名单 | 400 | `M_INVALID_PARAM` |
| `GET`/`DELETE` 带了非 null `body` | 400 | `M_INVALID_PARAM` |
| `body` 不是 JSON 对象 | 400 | `M_INVALID_PARAM` |
| `query` 不是对象或值非字符串 | 400 | `M_INVALID_PARAM` |
| `destination` 就是本机 | 403 | `M_FEDPROXY_DESTINATION_DENIED` |
| `path` 含 `.` / `..`，或不在 `/_matrix/federation/` 下 | 403 | `M_FEDPROXY_PATH_NOT_ALLOWED` |
| AS 未声明 `proxy_prefix`，或 `path` 不在其下 | 403 | `M_FEDPROXY_PATH_NOT_ALLOWED` |
| AS 不存在 / 已禁用 | 403 | `M_FORBIDDEN` |
| 远端不可达 | 502 | `M_FEDPROXY_CONNECTION_FAILED` |

> `fed_proxy` 只能访问调用方 AS 自己 `proxy_prefix` 覆盖的联邦路径，不是通用出网代理。

---

## 管理面：`/_synapse/admin/v1/appservices/*`

全部要求**管理员** access token（`admin_auth_middleware`）。`as_id` 规则：
非空且 ≤ 255 字节，无其他字符限制。

### 注册、查询、更新、删除

| 方法 | 端点 | 成功状态 | 返回 |
|---|---|---|---|
| `GET` | `/appservices` | 200 | `AppServiceResponse[]`（仅 `is_enabled = true`） |
| `POST` | `/appservices` | **201** | `AppServiceResponse` |
| `GET` | `/appservices/{as_id}` | 200 | `AppServiceResponse` |
| `PUT` | `/appservices/{as_id}` | 200 | `AppServiceResponse` |
| `DELETE` | `/appservices/{as_id}` | **204** | 无响应体 |
| `POST` | `/appservices/{as_id}/ping` | 200 | `{as_id, alive}` |

注册请求（`deny_unknown_fields`，多传字段直接 400）：

```jsonc
{
  "id": "my-bridge",
  "url": "https://as.example.com",        // 可省略/null → 无推送端点
  "as_token": "...",
  "hs_token": "...",
  "sender": "bridgebot",                  // 或 sender_localpart；两者都缺 → 400
  "description": "IRC bridge",
  "rate_limited": true,                   // 注意：请求/响应键名都是 rate_limited
  "protocols": ["irc"],
  "namespaces": {
    "users":   [{ "regex": "^@irc_.*:example.com$", "exclusive": true }],
    "aliases": [{ "regex": "^#irc_.*:example.com$",  "exclusive": true }],
    "rooms":   []
  }
}
```

更新请求只接受 5 个字段：`url`、`description`、`rate_limited`、`protocols`、`is_enabled`；
`as_token`/`hs_token`/`namespaces` 不能通过该端点修改。

`AppServiceResponse`：

```jsonc
{
  "id": 12,                    // 数据库自增 BIGSERIAL，不是 as_id
  "as_id": "my-bridge",
  "url": "https://as.example.com",  // 无推送端点时为 null
  "sender": "bridgebot",
  "description": "IRC bridge",
  "rate_limited": true,
  "protocols": ["irc"],
  "is_enabled": true,
  "created_ts": 1750000000000
}
```

`ping` 对 `url: null` 的 AS 返回 `{"as_id": "...", "alive": false}`，不会构造目标地址。

### 状态存储

| 方法 | 端点 | 返回 |
|---|---|---|
| `POST` | `/appservices/{as_id}/state` | `{as_id, state_key, state_value, updated_ts}` |
| `GET` | `/appservices/{as_id}/state` | `[{as_id, state_key, state_value, updated_ts}]` |
| `GET` | `/appservices/{as_id}/state/{state_key}` | 单条；不存在 → 404 |

`POST` 请求体只有两个字段：`{ "state_key": "...", "state_value": "..." }`（值必须是字符串）。

### 虚拟用户

| 方法 | 端点 | 成功状态 | 返回 |
|---|---|---|---|
| `POST` | `/appservices/{as_id}/users` | **201** | `{as_id, user_id, displayname, avatar_url, created_ts}` |
| `GET` | `/appservices/{as_id}/users` | 200 | 上述对象数组 |

`POST` 请求体：`{ "user_id": "@irc_alice:example.com", "displayname": "Alice", "avatar_url": "mxc://..." }`，
后两者可选。

### 命名空间

`GET /appservices/{as_id}/namespaces`：

```jsonc
{
  "users":   [{ "id": 1, "as_id": "my-bridge", "namespace_pattern": "...", "is_exclusive": true, "regex": "^@irc_.*$", "created_ts": 1750000000000 }],
  "aliases": [],
  "rooms":   []
}
```

注意：顶层是 `users` / `aliases` / `rooms` 三个数组（不是规范注册文件里的 `namespaces.users` 嵌套）。

### 事件队列

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/appservices/{as_id}/events?limit=100` | 待推送事件；`limit` 默认 100，clamp 到 `1..=500` |
| `POST` | `/appservices/{as_id}/events` | 注入一条事件，**201** |

`GET` 返回 `ApplicationServiceEvent[]`：

```jsonc
{
  "event_id": "$abc:matrix.test",
  "as_id": "my-bridge",
  "room_id": "!room:matrix.test",
  "event_type": "m.room.message",
  "sender": "@alice:matrix.test",
  "content": { "msgtype": "m.text", "body": "hi" },
  "state_key": null,
  "origin_server_ts": 1750000000000,
  "processed_ts": null,
  "transaction_id": null
}
```

`POST` 请求体：

```jsonc
{
  "room_id": "!room:matrix.test",
  "event_type": "m.room.message",
  "sender": "@irc_alice:example.com",
  "content": { "msgtype": "m.text", "body": "hello" },
  "state_key": null            // 可选；给出即为状态事件
}
```

响应：`201 {event_id, as_id, room_id, event_type}`。

### 归属查询与统计

| 方法 | 端点 | 返回 |
|---|---|---|
| `GET` | `/appservices/query/user?user_id=@bot:example.com` | `{user_id, application_service, exists}` |
| `GET` | `/appservices/query/alias?alias=%23room:example.com` | `{alias, application_service, exists}` |
| `GET` | `/appservices/statistics` | 统计对象数组 |

`statistics` 每项字段：`id`、`as_id`、`name`、`is_enabled`、`is_rate_limited`、
`virtual_user_count`、`pending_event_count`、`pending_transaction_count`、`last_seen_ts`、
`created_ts`，外加服务层补的 `scheduler`：

```jsonc
"scheduler": {
  "available": true,
  "last_result": "backoff",
  "transaction_state": "retry_backoff",
  "total_backoff_count": 3,
  "total_success_count": null,
  "pending_event_count": null,
  "last_elapsed_ms": 42
}
```

`scheduler.available = false` 表示该 AS 从未写入调度器状态，其余字段为 `null`。

---

## 错误与状态码

| 场景 | 状态 | errcode |
|---|---|---|
| 回调面缺/坏 `Authorization` | 401 | `M_UNAUTHORIZED` |
| `as_token` 无效 | 401 | `M_UNAUTHORIZED` |
| `as_id` 与 token 不匹配 | 403 | `M_FORBIDDEN` |
| user/alias 不在 AS 命名空间 | 403 | `M_FORBIDDEN` |
| 代理时 `hs_token` 错误 | 401 | `M_UNAUTHORIZED` |
| 代理时 AS 已禁用 | 400 | `M_BAD_JSON` |
| 代理时 AS `url: null` | 503 | `M_UNKNOWN` |
| 注册体缺 `sender`/`sender_localpart` | 400 | `M_BAD_JSON` |
| 注册/更新体含未知字段 | 400 | `M_BAD_JSON` |
| `as_id` 为空或超 255 字节 | 400 | `M_INVALID_PARAM` |
| `fed_proxy` 参数错误 | 400 | `M_MISSING_PARAM` / `M_INVALID_PARAM` |
| `fed_proxy` 越界 | 403 | `M_FEDPROXY_*` |
| `fed_proxy` 远端不可达 | 502 | `M_FEDPROXY_CONNECTION_FAILED` |

---

## SDK 接入建议

1. **客户端 SDK 不要暴露 AS token 管理**。`/_matrix/app/v1/*` 是 AS 自检面，
   普通 App 只需要 `GET /_matrix/client/v1/user/{user_id}/appservice` 这类无害探测。
2. **AS SDK 分两个 client**：`hs_client`（用 `as_token` 打 `/_matrix/app/v1/*` 与
   `fed_proxy`）和 `as_server`（用 `hs_token` 校验 HS 转发进来的 `/proxy/*` 请求）。
   两者混淆会把 `hs_token` 泄露给 HS 之外的调用方。
3. **事务处理按 `txn_id` 幂等**。HS 可能重放同一 `txn_id`；AS 侧应以
   `(as_id, txn_id)` 为幂等键，重复请求直接返回 `{}`。
4. **`url: null` 的 AS 不要轮询事件**。它没有推送端点，事件队列不会被投递；
   用 `ping` 的 `alive` 字段区分"注册了但无端点"和"端点不可达"。
5. **命名空间是唯一的归属判据**。`query/user` 与 `query/alias` 返回的
   `application_service` 为 `null` 时，不要回退到"按 user id 前缀猜"。
6. **`fed_proxy` 是受限代理**。只用于 AS 自己 `proxy_prefix` 覆盖的联邦路径；
   需要访问其他联邦端点时应由 HS 自身实现，而不是扩大 `proxy_prefix`。
