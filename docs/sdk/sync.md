# 同步 `/sync`

客户端保持状态的唯一入口：首次拿全量，之后拿增量。本页给出本仓的实际参数语义与响应形状；
Sliding Sync（MSC3575 / MSC4186）单独一节。

## 目录

- [端点矩阵](#端点矩阵)
- [长轮询参数](#长轮询参数)
- [响应结构](#响应结构)
- [房间分区（join / invite / knock / leave）](#房间分区join--invite--knock--leave)
- [Filter](#filter)
- [Account Data 与 Tags](#account-data-与-tags)
- [错误与限流](#错误与限流)
- [Sliding Sync](#sliding-sync)
- [客户端实现要点](#客户端实现要点)

---

## 端点矩阵

| 方法 | 端点 | 形态 |
|---|---|---|
| `GET` | `/_matrix/client/v3/sync` | **经典同步**（本页主体） |
| `POST` | `/_matrix/client/v1/sync` | Simplified Sliding Sync（MSC4186） |
| `POST` | `/_matrix/client/v4/sync` | 同上，稳定版本号 |
| `POST` | `/_matrix/client/unstable/org.matrix.simplified_msc3575/sync` | 同上，不稳定命名空间 |
| `POST` | `/_matrix/client/unstable/org.matrix.msc3575/sync` | 原始 MSC3575 命名空间 |

> ⚠️ `/_matrix/client/v3/sync` **只有 GET**。对 v3 发 POST 实测返回
> **405** `{"errcode":"M_UNRECOGNIZED","error":"Method not allowed for this endpoint"}`
> （方法不允许的 errcode 是 `M_UNRECOGNIZED`，不是 `M_UNSUPPORTED`）。
> Sliding Sync 必须走 `v1` / `v4` / `unstable/*`。

---

## 长轮询参数

`GET /_matrix/client/v3/sync`

| 参数 | 类型 | 默认 | 语义 |
|---|---|---|---|
| `since` | string | 无 | 增量游标，取上次响应的 `next_batch`。**缺失或空串 = 初始同步** |
| `timeout` | int (ms) | `30000` | 服务端最多等待多久再返回（长轮询）。`0` = 立即返回 |
| `full_state` | bool | `false` | 为 `true` 时必须返回全量 `state`（忽略增量裁剪） |
| `set_presence` | string | `online` | `online` / `offline` / `unavailable`，本次请求顺带上报在线状态 |
| `filter` | string | 无 | filter **ID** 或**内联 filter JSON**（见 [Filter](#filter)） |
| `use_state_after` | bool | `false` | MSC4222：把 `state` 段改名为 `state_after`（事件是该点之后的状态） |
| `org.matrix.msc4222.use_state_after` | bool | `false` | 同上，不稳定拼写；两者都给时以不稳定拼写优先 |

布尔参数接受 `true/false`、`1/0`、`yes/no`（大小写不敏感）。

**服务端超时**：内部等待上限 = 客户端 `timeout` + 15s 缓冲（`constants::sync_server_timeout`），
用于覆盖序列化开销。客户端侧仍应按 `timeout + 少量余量` 设置自己的 HTTP 超时。

---

## 响应结构

初始同步（`since` 缺失）：

```json
{
  "next_batch": "s1234_5_7",
  "rooms": {
    "join":   { "!room:a": { /* 见下 */ } },
    "invite": { "!room:b": { "invite_state": { "events": [ /* stripped state */ ] } } },
    "knock":  { "!room:c": { "knock_state":  { "events": [ /* stripped state */ ] } } },
    "leave":  { "!room:d": { /* 与 join 同构，但通常无 state */ } }
  },
  "presence":     { "events": [] },
  "account_data": { "events": [ { "type": "m.direct", "content": {} } ] },
  "to_device":    { "events": [] },
  "device_lists": { "changed": ["@bob:a"], "left": [] },

  "device_one_time_keys_count": { "signed_curve25519": 42 },
  "device_unused_fallback_key_types": ["signed_curve25519"],

  "key_rotation_needed": { "rooms": [] },
  "device_list_changes": {
    "users": { "@bob:a": { "change_type": "changed", "device_count": 3 } },
    "changed_count": 1,
    "left_count": 0
  }
}
```

**逐字段说明**

| 字段 | 说明 |
|---|---|
| `next_batch` | 下次的 `since`。**必须原样保存**，不要解析/拼接 |
| `rooms` | 四个分区始终存在（可能是空对象） |
| `presence.events` | 订阅用户的 presence（见 [devices.md](./devices.md)） |
| `account_data.events` | 全局账户数据（`m.direct`、`m.push_rules`、`im.vector.setting.*` 等） |
| `to_device.events` | 设备间消息（E2EE 的 `m.room.encrypted`、`m.room_key` 走这里） |
| `device_lists` | 设备清单变化（规范字段） |
| `device_one_time_keys_count` | 当前设备各类 OTK 余量；E2EE 依此决定是否补传 |
| `device_unused_fallback_key_types` | 尚未使用的 fallback key 类型 |
| `key_rotation_needed.rooms` | **本仓扩展**：需要轮换 megolm 会话的房间（服务端判定） |
| `device_list_changes` | **本仓扩展**：`device_lists` 的带计数投影，`{users, changed_count, left_count}`；`users[user_id]` = `{change_type: "changed"\|"left", device_count?}`（`device_count` 仅 changed 且计数查询成功时存在） |

> `key_rotation_needed` 与 `device_list_changes` 是**本仓在规范之外追加**的键。
> 标准客户端可忽略；自有客户端（Tjg）可据此触发密钥轮换与设备重查询。
> 注意 `device_list_changes` 不是 `device_lists` 的同义副本：它按用户聚合并附带设备数，
> 且 changed 用户若设备数查询失败会从 `users` 中缺席（`changed_count` 仍计入）。

---

## 房间分区（join / invite / knock / leave）

### `rooms.join.{room_id}`

```json
{
  "state":   { "events": [ /* 状态事件 */ ] },
  "timeline": { "events": [ /* 时间线事件 */ ], "limited": true, "prev_batch": "t42-1" },
  "ephemeral": { "events": [ /* m.typing / m.receipt */ ] },
  "account_data": { "events": [ /* 房间级账户数据 */ ] },
  "unread_notifications": { "highlight_count": 0, "notification_count": 3 },
  "sticky_events": [
    { "event_type": "m.room.message", "event_id": "$abc:a", "is_sticky": true }
  ]
}
```

| 字段 | 要点 |
|---|---|
| `timeline.limited` | `true` 表示这段事件与上次之间**有缺口**，需要用 `/messages?from=timeline.prev_batch&dir=b` 回补 |
| `timeline.prev_batch` | 回补游标（仅 `limited=true` 时有意义） |
| `ephemeral.events` | typing、回执（`m.receipt`）都在这里，**不在 timeline** |
| `unread_notifications` | 只给计数（`highlight_count` / `notification_count`），不含具体事件 |
| `state` vs `state_after` | `use_state_after=true` 时该键改名为 `state_after`（不稳定拼写为 `org.matrix.msc4222.state_after`），且**不再有 `state`** |
| `sticky_events` | **本仓 MSC4354 扩展，可选**：仅当房间存在 sticky 事件时出现；元素为 `{event_type, event_id, is_sticky}`，按 per-user 读取（含过期过滤），与 `GET /_matrix/client/unstable/org.matrix.msc4354/rooms/{room_id}/sticky_events` 返回同一集合 |

### `rooms.invite.{room_id}` / `rooms.knock.{room_id}`

只有 stripped state，**没有** timeline：

```json
{ "invite_state": { "events": [ { "type": "m.room.name", "state_key": "", "sender": "@a:x", "content": { "name": "组" } } ] } }
```

- 必需事件类型（按服务端 `STRIPPED_STATE_TYPES` 顺序）：`m.room.create`、`m.room.join_rules`、
  `m.room.name`、`m.room.avatar`、`m.room.topic`、`m.room.encryption`、`m.room.canonical_alias`、
  `m.room.member` 等。
- **fail-closed**：若既没有邀请方带来的 stripped state，且本地房间状态里**没有 `m.room.create`**，
  该房间会被**整体略过**（不会出现在 `invite` 里），并记 warn 日志。客户端不要假设
  "收到邀请就一定能看到房间名"。
- knock 房间**永远不会**以 `join` 形态出现（防止把敲门当成已加入）。

### `rooms.leave.{room_id}`

结构与 join 相同，但通常只有 `timeline` 尾部与 `account_data`。`leave` 分区同时覆盖
"主动离开"与"被踢/被封禁"。

---

## Filter

### 上传/读取 filter

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/user/{user_id}/filter` | 上传 filter 定义 → `{"filter_id": "1"}` |
| `GET` | `/user/{user_id}/filter/{filter_id}` | 取回 filter 定义（原样 JSON）；不存在 → 404 `M_NOT_FOUND` |
| `PUT` | `/user/{user_id}/filter` | 覆盖/写入 filter |
| `DELETE` | `/user/{user_id}/filter/{filter_id}` | 删除 |
| `GET` | `/user/{user_id}/filter/{filter_id}` | 仅本人可读：他人 → 403 `M_FORBIDDEN` |

`POST /_matrix/client/v3/sync?filter=<filter_id>` 直接用 ID；也可把 filter JSON 直接塞进 `filter`
（会被 URL 编码）。

### filter 结构（本仓实际支持的字段）

```typescript
interface Filter {
  room?: {
    timeline?: { limit?: number; types?: string[]; not_types?: string[] };
    state?: { types?: string[]; not_types?: string[]; lazy_load_members?: boolean; include_redundant_members?: boolean };
    ephemeral?: { types?: string[]; not_types?: string[] };
    account_data?: { types?: string[]; not_types?: string[] };
    rooms?: string[]; not_rooms?: string[];
    include_leave?: boolean;
  };
  presence?: { types?: string[]; not_types?: string[] };
  account_data?: { types?: string[]; not_types?: string[] };
  event_fields?: string[];
  event_format?: 'client' | 'federation';
}
```

> `room.state.lazy_load_members=true` 依赖服务端 `m.lazy_load_members` 特性（当前 `true`）。
> 开启后首次同步的 `state` 只含与 timeline 相关的成员，其余成员需 `/members` 补齐。
> `/messages` 的 `filter` 参数则是**另一回事**：不被支持，传非空值直接 400（见
> [messages.md](./messages.md)）。

---

## Account Data 与 Tags

### 全局账户数据

| 方法 | 端点 |
|---|---|
| `GET`/`PUT`/`POST` | `/user/{user_id}/account_data/{type}` |
| `DELETE` | `/user/{user_id}/account_data/{type}` |
| `GET` | `/user/{user_id}/account_data/`（列出全部） |

写入体就是该类型的 `content` 原文；响应 `{}`。只有本人可读写（他人 403 `M_FORBIDDEN`）。

### 房间级账户数据

| 方法 | 端点 |
|---|---|
| `GET`/`PUT`/`DELETE`/`POST` | `/user/{user_id}/rooms/{room_id}/account_data/{type}` |
| `GET`/`PUT` | `/rooms/{room_id}/account_data/{type}`（v3 别名家族，**无 DELETE**） |

### Tags

| 方法 | 端点 | 体 |
|---|---|---|
| `GET` | `/user/{user_id}/rooms/{room_id}/tags` | — |
| `PUT` | `/user/{user_id}/rooms/{room_id}/tags/{tag}` | `{"order": 0.5}`（`m.favourite` / `m.lowpriority` / `u.xxx`） |
| `DELETE` | `/user/{user_id}/rooms/{room_id}/tags/{tag}` | — |

`GET /user/{user_id}/tags` 是**全量视图**（房间 → tags 映射）。

> 房间排序（sidebar 顺序）靠 tag 的 `order`；`m.favourite` 的 `order` 通常取 `0.5` 附近。
> 客户端改序后服务端会以 `m.tag` 账户数据经 `/sync` 推回。

---

## 错误与限流

| 场景 | 状态 | errcode | 说明 |
|---|---|---|---|
| `since` 无法解析 | **400** | `M_BAD_PAGINATION` | `Invalid since token`。**不是 401**——早期实现返回 401 `M_UNKNOWN_TOKEN`，会让 SDK 误清 token 强制重登 |
| `timeout` 为负数 | 400 | `M_BAD_JSON` | `timeout must be a non-negative integer` |
| `filter` 引用的 ID 不存在 | 4xx | 视 filter 来源 | 内联 JSON 无此问题；用 ID 时先 `POST /user/{id}/filter` 拿到 ID |
| 超出同步限流 | 429 | `M_LIMIT_EXCEEDED` | 带 `retry_after_ms` 与 `Retry-After` / `X-RateLimit-*` 头 |

**限流分桶**：初始同步与增量同步**分开计数**（`ratelimit:sync:{user}:{device}:initial|incremental`）。
限流后端故障时默认 **fail-open**（放行），除非配置关闭。

**客户端处理**：`M_BAD_PAGINATION` → 丢弃本地 `since` 走初始同步；`429` → 按
`retry_after_ms` 退避，**不要**立即重连（会造成同步风暴）。

---

## Sliding Sync

`POST /_matrix/client/v1/sync`（及 `v4` / `unstable/*`）

**请求体**（`synapse-storage/src/sliding_sync/models.rs`）：

```typescript
interface SlidingSyncRequest {
  conn_id?: string;
  lists?: Record<string, {              // 也接受"旧的数组形态"，服务端两种都解析
    ranges: [number, number][];
    sort?: string[];
    filters?: object;
    timeline_limit?: number;            // 别名 timelineLimit
    required_state?: [string, string][];
    slow_by?: number;
    bump_event_types?: string[];
  }>;
  room_subscriptions?: Record<string, unknown>;
  unsubscribe_rooms?: string[];
  extensions?: object;
  pos?: string;                          // 上次响应的 pos；缺失 = 初始
  timeout?: number;                      // 毫秒
  clientTimeout?: number;                // 注意 camelCase
  txn_id?: string;                       // MSC4186：幂等键
}
```

**响应**：

```json
{ "pos": "s42", "conn_id": "conn-1", "lists": { "main": { } }, "rooms": { }, "extensions": { } }
```

**本仓语义要点**

- `txn_id` 是**幂等键**：同 `(user, device, txn_id)` 重放返回缓存响应（响应丢失后重试安全）。
- `pos` 过期/非法 → 400 `M_UNKNOWN_POS`（能力 `org.matrix.msc4186`）。
- 限流计数器独立（`sliding_sync_rate_limited_total`），429 语义与经典 sync 相同。
- `lists` 的**两种形态**都接受：对象（`{"main": {...}}`）与数组（`[{list_key, ranges, ...}]`）。

### 扩展 `extensions`

开启判定：`extensions[name] === true`，或对象形态且 `enabled !== false`（缺省即开启）。
响应里的 `extensions` 以请求对象为底（请求过的键会回声），再按需覆盖实际载荷。

| 扩展 | 响应载荷 |
|---|---|
| `e2ee` | `{device_lists:{changed,left}, device_one_time_keys_count:{algo:count}, device_unused_fallback_key_types:[algo]}` |
| `to_device` | `{events:[...], next_batch?}`（`since` 由请求体 `extensions.to_device.since` 指定） |
| `account_data` | `{global:[{type,content}], rooms:{room_id:[{type,content}]}}`（**事件数组**，不是 `type→content` 映射） |
| `presence` | `{events:[m.presence]}`；仅当内容变化时下发（服务端按连接去重） |
| `receipts` | `{rooms:{room_id:{...}}}` |
| `typing` | `{rooms:{room_id:{user_ids:[...]}}}`（**不是** `m.typing` 事件形状，没有 `type`/`content`） |
| `profile_updates` | `{users:{user_id:{displayname,avatar_url,updated_ts} \| null}}` |

`profile_updates` 实现 MSC4262 的 `LEFT_ROOM` 语义（上游 #20203）：

- 只覆盖**本地用户**（`@` 开头）且排除自己；
- 用户不再与我共享任何已加入房间时，该用户的值是 **`null`** —— 客户端应删除其本地资料缓存；
- 该判定独立于"资料是否更新"：一次纯离开也可能只带 `{"users":{"@bob:x":null}}`；
- 同一窗口内若既有新资料又是离开，**新资料优先**（不会写成 `null`）；
- 载荷未变化时整个扩展不下发（与 `presence` 相同的去重策略）。

**什么时候用它**：`/versions` 的 `org.matrix.simplified_msc3575` 或
`org.matrix.msc3886.sliding_sync` 为 `true`，且客户端实现了 `lists` 语义。
否则用经典 `/sync` —— 两者的 `rooms` 结构**不同**，不要混用解析器。

---

## 客户端实现要点

1. **先存 `next_batch` 再渲染**：渲染期间崩溃不会丢游标（丢游标 = 下次全量重拉）。
2. **`limited=true` 必须回补**：否则消息列表出现空洞。
3. **`ephemeral` 不要塞进 timeline**：typing/回执是瞬时态，写入消息表会导致重复与错序。
4. **一次只有一个 `since` 在飞**：并发同步会拿到重叠事件并打乱 `next_batch`。
   需要"强制刷新"时用 `timeout=0` 重新发起，而不是并行发两个请求。
5. **`timeout` 与 HTTP 超时要拉开**：服务端可能等 `timeout+15s`；客户端 HTTP 超时应 ≥ `timeout+20s`。
6. **`device_one_time_keys_count` 低于阈值就补传**：E2EE 依赖它判断何时 `/keys/upload`。
7. **401 才清 token**；400 `M_BAD_PAGINATION` 只清 `since`。
