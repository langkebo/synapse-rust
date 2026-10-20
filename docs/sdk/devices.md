# 设备、会话与在线状态

客户端的"设备面板""消息推送""在线状态"三块共用同一组账户级接口。E2EE 视角的设备密钥
（`/keys/*`、cross-signing、key backup）见 [e2ee.md](./e2ee.md)。

## 目录

- [设备列表](#设备列表)
- [单设备读写](#单设备读写)
- [删除设备](#删除设备)
- [Presence 在线状态](#presence-在线状态)
- [Pushers 推送注册](#pushers-推送注册)
- [Notifications 通知中心](#notifications-通知中心)
- [Push Rules 推送规则](#push-rules-推送规则)
- [设备清单与 E2EE 的关系](#设备清单与-e2ee-的关系)

---

## 设备列表

**端点**：`GET /_matrix/client/v3/devices`

**认证**：是

**响应（字段按 `routes/device.rs` 提取）**：

```json
{
  "devices": [
    {
      "device_id": "ABCDEFGHIJ",
      "display_name": "MacBook Pro",
      "last_seen_ts": 1760700000000,
      "last_seen_ip": "203.0.113.7"
    }
  ]
}
```

- `last_seen_ts` / `last_seen_ip` 可能为 `null`（从未上报）。
- 列表**不含** `device_keys`；设备密钥要从 `/keys/query` 拿。

---

## 单设备读写

| 方法 | 端点 | 请求体 | 响应 |
|---|---|---|---|
| `GET` | `/devices/{device_id}` | — | **同时**给嵌套对象与平铺字段（见下） |
| `PUT` | `/devices/{device_id}` | `{"display_name": "新名字"}` | `{}` |

`GET /devices/{device_id}` 的响应把同一份数据给两遍（`device` 子对象 + 顶层平铺），
且**不含 `last_seen_ip`**（列表接口才有）：

```json
{
  "device": { "device_id": "ABCDEFGHIJ", "display_name": "MacBook Pro", "last_seen_ts": 1760700000000 },
  "device_id": "ABCDEFGHIJ",
  "display_name": "MacBook Pro",
  "last_seen_ts": 1760700000000
}
```

客户端取 `device` 或取平铺字段都可以，但**不要假设两种形态同时存在于所有部署版本**——
稳妥写法是 `resp.device ?? resp`。

**错误**：设备不存在，或不属于调用者 → **404 `M_NOT_FOUND`**。

> 服务端**刻意不用** `M_UNKNOWN_DEVICE` 表示 404（该码属 MSC4326 的范围，本服务未启用）。
> 客户端不要按 `M_UNKNOWN_DEVICE` 分支。

---

## 删除设备

两个端点，**都需要 UIA**：

### 删除单个

`DELETE /_matrix/client/v3/devices/{device_id}`

```json
{ "auth": { "type": "m.login.password", "password": "<当前密码>", "session": "<可选>" } }
```

### 批量删除

`POST /_matrix/client/v3/delete_devices`

```json
{
  "devices": ["ABCDEFGHIJ", "KLMNOPQRST"],
  "auth": { "type": "m.login.password", "password": "<当前密码>" }
}
```

- 不带 `auth` → 401 + `flows`/`session`（UIA，flows 来自 `get_delete_device_flows()`）。
- **例外（MSC4190）**：调用者是 Application Service（`auth_user.appservice_id` 非空）时
  **跳过 UIA**，直接删设备。
- 响应 `{}`。
- 删除设备会**同时吊销该设备的 token**；被删除设备的客户端下一次请求收到 401 `M_UNKNOWN_TOKEN`。

> 设备被其它设备删除后，E2EE 客户端要重新同步 device list（`/keys/query`）；
> 服务端会通过 `/sync` 的 `device_lists.changed` 与 `device_list_changes` 通知（见 [sync.md](./sync.md)）。

---

## Presence 在线状态

**前置条件**：`server.presence_enabled=true`。关闭时所有 presence 端点返回
**403 `M_FORBIDDEN`**（`error: Presence is not supported on this server`）——这是"行政关闭"，
客户端不应重试。

### 读取状态

`GET /_matrix/client/v3/presence/{user_id}/status`（另有 `v1`）

**可见性规则**（本仓实现，与规范一致的保守版）：

1. 目标用户**不存在** → 404 `M_NOT_FOUND`（先判存在，避免通过 403 探测用户是否存在）；
2. 读**自己** → 允许；
3. 读**别人** → 必须与其**共享至少一个房间**，否则 403 `M_FORBIDDEN`。

**响应**：

```json
{
  "presence": "online",
  "status_msg": "在开会",
  "last_active_ago": 12000,
  "currently_active": true
}
```

| 字段 | 语义 |
|---|---|
| `presence` | `online` / `unavailable` / `offline` |
| `status_msg` | 自定义状态文案，可为 `null` |
| `last_active_ago` | 距今毫秒数；`presence=offline` 或其无记录时为 `null` |
| `currently_active` | 近 5 分钟内有活动且 `presence=online` 时为 `true`，否则 `null` |

无记录时服务端返回 `{"presence":"offline","status_msg":null,"last_active_ago":null,"currently_active":null}`。

### 设置状态

`PUT /_matrix/client/v3/presence/{user_id}/status`（另有 `POST`、`v1` 变体）

```json
{ "presence": "online", "status_msg": "在开会" }
```

**只能设置自己**——`user_id` 不是本人时 403 `M_FORBIDDEN`（`Access denied`）。
`status_msg` 长度上限同消息体上限（超长 400 `M_INVALID_PARAM`）。

### 订阅列表（presence list）

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/presence/list` | 返回**状态快照**，不是 ID 数组（见下） |
| `POST` | `/presence/list` | 体 `{"subscribe": ["@a:x"], "unsubscribe": ["@b:x"]}`，返回同样的快照 |
| `GET` | `/presence/list/{user_id}` | 订阅范围内某人的状态（同单用户读取形状） |

**`GET`/`POST /presence/list` 的响应**：

```json
{
  "presences": [
    { "user_id": "@bob:matrix.test", "presence": "online", "status_msg": null,
      "last_active_ago": 3200, "currently_active": true },
    { "user_id": "@carol:matrix.test", "presence": "offline", "status_msg": null,
      "last_active_ago": null, "currently_active": null }
  ]
}
```

- 请求体的键是 **`subscribe` / `unsubscribe`**（不是 `invite` / `drop`）。
- 订阅与返回都按"共享房间"过滤：不可见的用户不会被订阅，也不会出现在结果里。
- 已订阅但查不到状态的用户，也会以 `presence: "offline"` 补齐一条（保证数量与订阅一致）。

---

## Pushers 推送注册

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/pushers`（兼容 `/pushers/`） | 列出当前设备的 pusher |
| `POST` | `/pushers/set`（兼容 `/pushers`、`/pushers/`） | 新增/更新/删除 pusher |

**`POST /pushers/set` 请求体**

```typescript
interface PusherSet {
  pushkey: string;              // 必填（设备推送 token）
  app_id: string;               // 必填
  kind: string | null;          // 必填键；null = 删除该 pusher
  app_display_name?: string;    // kind 非 null 时必填
  device_display_name?: string;
  lang?: string;                // kind 非 null 时必填
  profile_tag?: string;
  data?: { url?: string; format?: string; [k: string]: unknown };
  append?: boolean;             // true 时追加而非替换
}
```

- `kind` 必须存在（可以是 `null`）；`app_id` / `pushkey` 缺失 → 400 `M_BAD_JSON`。
- 响应 `{}`。
- `GET /pushers` 返回 `{"pushers": [...]}`，按 **当前设备** 过滤。

> 本仓另有 `/_matrix/vendor/v1/push/devices*`（设备推送扩展）与 `POST /_matrix/vendor/v1/push/send`，
> 见 [extensions.md](./extensions.md)。

---

## Notifications 通知中心

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/notifications?limit=N&from=C&only=...` | 列通知；`limit` 默认 **50**、上限 500；`from` 为上一页的 `next_token`；`only` 可重复 |
| `POST` | `/notifications/{notification_id}/ack` | 标记已读 |

`only` 支持三类过滤器（可重复，同类取并集）：

- `room_id:!room:server`
- `type:m.room.message`
- `sender:@user:server`

缺 `:`、值为空或使用其它 key（`highlight` / `is_encrypted` / `is_one_to_one` / `contains_user_name:`）
→ 400 `M_INVALID_PARAM`（后端不会静默忽略）。`limit` 非整数/负数 → 400；`>500` 夹紧到 500。

**响应**：

```json
{
  "notifications": [
    {
      "notification_id": 42,
      "event_id": "$abc:matrix.test",
      "room_id": "!room:matrix.test",
      "ts": 1760700000000,
      "profile_tag": null,
      "notification_type": "highlight",
      "read": false
    }
  ],
  "next_token": "1760700000000_42"
}
```

> `next_token` 现在是**真实可用**的分页游标：仅当后端确认还有下一页时才非 `null`。
> 回填到下一次请求的 `from` 即可；不要解析或自行构造（内部是 `(ts, id)` keyset）。
> 翻页终止条件是 `next_token === null`（不是 `notifications.length === 0`）。

**ack 响应**：`{"notification_id": 42, "is_read": true, "updated_ts": 1760700000000}`；
不属于本人的通知 → 404 `M_NOT_FOUND`。

`notification_type` 与 `profile_tag` 是**并列的两个键**（`profile_tag` 是命中的推送规则 tag，
`notification_type` 是通知类型），不要互相替代。

---

## Push Rules 推送规则

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/pushrules` | 全部规则 |
| `GET` | `/pushrules/{scope}` | `scope` ∈ `global` / `device` |
| `GET` | `/pushrules/{scope}/{kind}` | `kind` ∈ `override` / `content` / `room` / `sender` / `underride` |
| `GET` | `/pushrules/{scope}/{kind}/{rule_id}` | 单条 |
| `PUT` | `/pushrules/{scope}/{kind}/{rule_id}` | 新增/覆盖 |
| `DELETE` | `/pushrules/{scope}/{kind}/{rule_id}` | 删除 |
| `PUT` | `/pushrules/{scope}/{kind}/{rule_id}/enabled` | 体 `{"enabled": true}` |
| `GET` | `/pushrules/{scope}/{kind}/{rule_id}/enabled` | 读启用状态 |

`enabled` 的写入响应含 `rule_id` / `enabled` / `updated_ts`（不是空对象）。
规则不存在 → 404 `M_NOT_FOUND`。

---

## 设备清单与 E2EE 的关系

| 关注点 | 端点 |
|---|---|
| 我在哪些设备上登录 | `GET /devices` |
| 各设备的身份密钥 | `POST /keys/query`（[e2ee.md](./e2ee.md)） |
| 设备清单变化通知 | `/sync` 的 `device_lists.changed` / `left`，以及 `device_list_changes` |
| 一次性密钥剩余量 | `/sync` 的 `device_one_time_keys_count` |
| 未使用的 fallback key 类型 | `/sync` 的 `device_unused_fallback_key_types` |

**正确姿势**：设备管理与密码学信任是两件事。`GET /devices` 只给"这个账户有哪几台设备"，
**不给**任何可用于加密的公钥；加密前必须 `/keys/query` 并做签名校验。
