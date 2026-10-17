# 目录、搜索与 Thirdparty 契约

本仓的"找人 / 找房间"能力分散在 **四个互不相通的路由族**里，字段命名也不一致。
SDK 必须按家族分别建模，**不要假设它们共享同一个用户对象或房间对象**：

| 家族 | 前缀 | 典型用途 | 是否规范 |
|---|---|---|---|
| 用户目录 | `/_matrix/client/v3/user_directory/*` | 通讯录搜索、用户列表、目录资料卡 | `search` 规范；`list` 为本仓扩展 |
| 全局搜索 | `/_matrix/client/v3/search` | 消息搜索 + `users` 类别 | 规范 |
| 房间目录 | `/_matrix/client/v3/publicRooms` | 公开房间列表 / 搜索 | 规范 |
| Thirdparty | `/_matrix/client/v3/thirdparty/*` | 桥接协议与位置/用户映射 | 规范（多数面本仓恒 404） |

另有 `/_matrix/vendor/v1/*` 三个扩展端点（`search_recipients` / `search_rooms`），见
[extensions.md](./extensions.md)。

> **端点版本**：上表 `user_directory/*`、`publicRooms`、`thirdparty/*`、`directory/*` 全部
> **只注册在 `/_matrix/client/v3`**。源码 `create_directory_compat_router` 仅
> `.nest("/_matrix/client/v3", ...)`，没有 `r0` / `v1` 变体。写死 v1 会 404。

---

## 一、用户目录 `user_directory/*`

三条端点，全部需要 `Authorization: Bearer`（缺头/坏头 → 401 `M_UNAUTHORIZED`）。

### 1.1 搜索用户

`POST /_matrix/client/v3/user_directory/search`

```json
{ "search_term": "ali", "limit": 10 }
```

| 字段 | 类型 | 必填 | 默认 | 约束 |
|---|---|---|---|---|
| `search_term` | string | 否 | `""` | 为空/缺省时返回一批目录用户（不做过滤） |
| `limit` | number | 否 | `10` | clamp 到 `1..=100`（超界不报错，直接夹取） |

**响应**：

```json
{
  "limited": false,
  "results": [
    { "user_id": "@alice:matrix.test", "display_name": "Alice", "avatar_url": "mxc://..." }
  ]
}
```

- 字段名是 **`display_name`**（不是 `/profile` 里的 `displayname`，也不是 `displayname`）。
- `limited` 的判定是 `results.len() >= limit`——即"可能还有更多"，**没有**下一页游标；
  要继续拿就增大 `limit`（上限 100）。
- 结果已按资料可见性过滤（见 1.3）。

### 1.2 列出用户（本仓扩展，非规范）

`POST /_matrix/client/v3/user_directory/list`

```json
{ "limit": 50, "since": "1729100000000|@alice:matrix.test" }
```

| 字段 | 类型 | 必填 | 默认 | 约束 |
|---|---|---|---|---|
| `limit` | number | 否 | `50` | clamp 到 `1..=200` |
| `since` | string | 否 | 无 | keyset 游标，格式 `"{created_ts}\|{user_id}"` |

**响应**：

```json
{
  "total": 128,
  "next_batch": "1729100123000|@bob:matrix.test",
  "users": [
    { "user_id": "@alice:matrix.test", "display_name": "Alice", "avatar_url": "mxc://..." }
  ]
}
```

- 这是 **keyset 游标**，不是 offset：把 `next_batch` 原样回填 `since` 即可。
- `next_batch` 为 `null` 表示已到末尾。
- 游标里的 `|` 是分隔符，`user_id` 本身含 `:` 但不含 `|`，可安全 `split_once('|')`。

### 1.3 目录资料卡

`GET /_matrix/client/v3/user_directory/profiles/{user_id}`（需认证）

**响应**：

```json
{ "user_id": "@alice:matrix.test", "displayname": "Alice", "avatar_url": "mxc://..." }
```

注意这里又变回 **`displayname`**（与 `/profile/{user_id}` 一致），和 1.1/1.2 的
`display_name` 不同——SDK 应在内部归一，不要把这个差异泄漏给业务层。

**可见性**：

| 情况 | 结果 |
|---|---|
| 可见 | 200 + 上面对象 |
| 不可见 | 403 `M_FORBIDDEN`，`error` = `Profile is private or not visible to you` |
| 用户不存在 | 404 |
| 内部错误 | 403（fail-closed） |

可见性判定在 `can_view_profile_for_requester` → `enforce_profile_visibility`，
底层是 `account_identity_service::batch_can_view_profile`：

- **默认构建（未启用 `privacy-ext` feature）恒为可见**，即本节 403 实际不会触发。
- 启用 `privacy-ext` 后才走 `privacy_storage.batch_can_view_profile`（按共享房间/隐私设置判定）。

> 因此 SDK **不要**把 403 当作常态分支做特殊 UI；按 `M_FORBIDDEN` 走通用"无权限"即可。

---

## 二、全局搜索里的 `users` 类别

`POST /_matrix/client/v3/search`

```json
{
  "search_categories": {
    "users": { "search_term": "ali", "limit": 10 }
  }
}
```

| 字段 | 约束 |
|---|---|
| `search_categories.users.search_term` | 必填；最大 **256** 字符；**允许空串**（返回一批目录用户） |
| `search_categories.users.limit` | 默认 `10`；`> 100` → 400 |

与 `room_events` 的差异：`room_events.search_term` 是**必填且非空**，空串会被拒；
`users` 空串合法。两类可以同时出现在一个请求里。

**响应**：

```json
{
  "search_categories": {
    "users": {
      "limited": false,
      "results": [
        {
          "user_id": "@alice:matrix.test",
          "display_name": "Alice",
          "avatar_url": "mxc://...",
          "presence": "online",
          "last_active_ts": 1729100000000,
          "match_score": 0.87,
          "match_type": "prefix"
        }
      ]
    }
  }
}
```

- 同样先做可见性过滤，再做排序。
- 限流：`ratelimit:search-users:{user_id}`，**2 次 / 20 秒**；超限 429 + `retry_after_ms`。
- `match_type` / `match_score` 是排序辅助，不要用于业务判断。

### vendor 变体：`search_recipients`

`POST /_matrix/vendor/v1/search_recipients`（本仓扩展，用于"选人发消息"场景）

```json
{ "search_term": "ali", "limit": 10 }
```

| 字段 | 约束 |
|---|---|
| `search_term` | **必填且非空**（空 → 400） |
| `limit` | 默认 `10`，最大 **50**（与上文的 100 不同） |

**响应**：

```json
{
  "results": [
    {
      "user_id": "@alice:matrix.test",
      "username": "alice",
      "display_name": "Alice",
      "avatar_url": "mxc://...",
      "presence": "online",
      "online": true,
      "last_active_ts": 1729100000000,
      "match_score": 0.87,
      "match_type": "prefix"
    }
  ],
  "count": 1,
  "next_batch": null
}
```

- 比 `/search` 的 `users` 结果多 `username` 与 `online` 两个字段。
- `next_batch` **恒为 `null`**——这是定长首屏接口，不分页。
- 限流同为 2 次 / 20 秒。

---

## 三、房间目录 `publicRooms`

### 3.1 GET（轻量查询）

`GET /_matrix/client/v3/publicRooms?limit=20&since=t7&server=matrix.test`

| 参数 | 默认 | 约束 |
|---|---|---|
| `limit` | `20` | clamp 到 `1..=1000` |
| `since` | 无 | 上页 `next_batch` |
| `server` | 无 | 仅允许查询本服务或已配置的联邦目标 |

> **GET 不读 `filter`**。想按关键词/房间类型过滤，必须用 POST。给 GET 加 `filter=...`
> 会被静默忽略（但有 `deny_unknown_fields` 的家族要注意，本端点不校验未知 query）。

### 3.2 POST（带过滤查询）

`POST /_matrix/client/v3/publicRooms`

```json
{
  "limit": 20,
  "since": "t7",
  "filter": { "generic_search_term": "项目", "room_types": ["m.space", null] },
  "include_all_networks": false,
  "third_party_instance_id": null
}
```

`filter` **只认两个键**：`generic_search_term`（string）与 `room_types`（array）。
`room_types` 的条目可以是字符串或 `null`，`null` 表示"包含普通（非 space）房间"。

**硬规则（违反即 400 `M_INVALID_PARAM`）**：

| 条件 | 结果 |
|---|---|
| 顶层 `include_all_networks: true` | 400 |
| 顶层 `third_party_instance_id` 非 `null` | 400 |
| `filter` 不是 JSON 对象 | 400 |
| `filter` 内出现 `include_all_networks` 或 `third_party_instance_id` | 400 |
| `generic_search_term` 不是字符串 | 400 |
| `room_types` 不是数组 | 400 |

`generic_search_term` 会 `trim`：全空白等同于"无搜索词"（退化为普通列表）。

### 3.3 响应与分页

```json
{
  "chunk": [
    {
      "room_id": "!x:matrix.test",
      "name": "项目组",
      "topic": "...",
      "avatar_url": "mxc://...",
      "canonical_alias": "#project:matrix.test",
      "num_joined_members": 12,
      "world_readable": true,
      "guest_can_join": false,
      "join_rule": "public",
      "room_type": null
    }
  ],
  "next_batch": "t7",
  "total_room_count_estimate": 42
}
```

**分页语义（易错）**：

- `next_batch` **仅当** `rooms.len() == limit` 时才返回；否则缺省（视为没有下一页）。
- **POST 且带 `generic_search_term` 时不发 `next_batch`**——搜索结果不分页。
- `chunk[].room_type` 在本仓**恒为 `null`**。想识别 space 只能看 `join_rule`/名称，
  或改用 `/_matrix/vendor/v1/spaces/*`（见 [extensions.md](./extensions.md#spaces)）。
- `world_readable` / `guest_can_join` / `join_rule` 是房间"目录可见性"的快照。

---

## 四、目录可见性 `directory/list/room/{room_id}`

| 方法 | 端点 | 认证 | 响应 |
|---|---|---|---|
| `GET` | `/directory/list/room/{room_id}` | 否 | `{"visibility": "public"\|"private"}` |
| `PUT` | `/directory/list/room/{room_id}` | 是 | `{"room_id", "visibility", "updated_ts"}` |

- `GET`：房间不存在 → 404；无需 token，可匿名读。
- `PUT`：需要调用者是房间成员**且**是房间创建者（creator），否则 403 `M_FORBIDDEN`。
- `PUT` 体只接受 `{"visibility": "public"}` 或 `{"visibility": "private"}`：
  - 缺字段 → 400 `M_BAD_JSON`（`Missing visibility field`）
  - 其他值 → 400 `M_BAD_JSON`（`visibility must be 'public' or 'private'`）

> **没有 `DELETE /directory/list/room/{room_id}`**。取消公开发布用
> `PUT` + `{"visibility": "private"}`。历史文档里的 DELETE 会得到 405。

---

## 五、房间别名（速查，详见 rooms.md）

这几条**只存在于 v3**：

| 方法 | 端点 | 成功响应 |
|---|---|---|
| `GET` | `/directory/room/{room_alias}` | **`{"room_id"}`**（只有 room_id，无 `servers`） |
| `PUT` | `/directory/room/{room_alias}` | `{"room_id", "alias", "created_ts"}` |
| `DELETE` | `/directory/room/{room_alias}` | `{"removed": true, "alias"}` |
| `GET` | `/directory/room/{room_id}/alias` | `{"aliases": [...]}` |
| `PUT` | `/directory/room/{room_id}/alias/{room_alias}` | `{"room_id", "alias", "created_ts"}` |
| `DELETE` | `/directory/room/{room_id}/alias/{room_alias}` | `{}` |

- `GET /directory/room/{room_alias}` 的响应**没有 `servers` 字段**——SDK 若按规范去读
  `servers` 会拿到 `undefined`；联邦路由信息在本服务不暴露。
- `PUT /directory/room/{room_alias}` 的体**必须含 `room_id`**，且必须以 `!` 开头；
  `servers` 即使传了也被忽略。
- 别名格式 `#localpart:server`；非法 → 400 `M_INVALID_PARAM`；已被占用 → 400 `M_ROOM_IN_USE`。
- `GET /directory/room/{room_id}/alias` 需要成员身份；房间不存在 → 404。

---

## 六、Thirdparty

6 条端点，全部在 `/_matrix/client/v3/thirdparty/*`，**全部需要认证**。

| 方法 | 端点 | 实际行为 |
|---|---|---|
| `GET` | `/thirdparty/protocols` | 聚合所有已注册 Application Service 的协议对象 → `{}` 形如 `{protocol_name: {...}}` |
| `GET` | `/thirdparty/protocol/{protocol}` | 返回该协议详情；未注册 → 404 `M_NOT_FOUND` |
| `GET` | `/thirdparty/location/{protocol}` | **恒 404 `M_UNRECOGNIZED`** |
| `GET` | `/thirdparty/location` | **恒 404 `M_UNRECOGNIZED`** |
| `GET` | `/thirdparty/user/{protocol}` | **恒 404 `M_UNRECOGNIZED`** |
| `GET` | `/thirdparty/user` | **恒 404 `M_UNRECOGNIZED`** |

**这两类 404 是设计行为，不是路由缺失**：

- `location*` → `error` = `No third-party location bridges configured`
- `user*` → `error` = `No third-party user bridges configured`
- errcode 是 **`M_UNRECOGNIZED`**（`ApiError::unrecognized` → HTTP 404），
  与 `/thirdparty/protocol/{protocol}` 未注册时的 `M_NOT_FOUND` **不同**。
  两者都是 404，SDK 若只按 errcode 分支会误判，应按端点语义分支。

**Query 参数严格校验**（DTO 均 `deny_unknown_fields`）：

| 端点 | 允许的 query |
|---|---|
| `/thirdparty/location` | `alias`、`search`、`server`、`channel` |
| `/thirdparty/user` | `userid`、`search`、`nickname`、`server` |

多传任何键 → 400。SDK 不要把内部状态（如 `limit`、`since`）塞进这两个端点的 query。

**SDK 建议**：把 thirdparty 视为"可选增强能力"。启动时调一次
`GET /thirdparty/protocols`；返回空对象就把相关入口整个隐藏，**不要**去探
`/thirdparty/location`——它必然 404，只会污染错误日志。

---

## 七、vendor 房间搜索

`POST /_matrix/vendor/v1/search_rooms`（本仓扩展）

```json
{ "search_term": "项目", "limit": 10 }
```

| 字段 | 约束 |
|---|---|
| `search_term` | **必填且非空**（空 → 400） |
| `limit` | 默认 `10`，最大 **50** |

**响应**：

```json
{
  "results": [
    {
      "room_id": "!x:matrix.test",
      "name": "项目组",
      "topic": "...",
      "avatar_url": "mxc://...",
      "is_public": true,
      "world_readable": true,
      "num_joined_members": 0
    }
  ],
  "count": 1,
  "next_batch": null
}
```

- `world_readable` 在本实现中**直接等于 `is_public`**（不是独立判定）。
- `num_joined_members` 在本实现中**恒为 `0`**——不要用它展示人数，改用
  `publicRooms` 的 `num_joined_members`。
- `next_batch` 恒 `null`。

---

## 八、举报（速查，详见 rooms.md / admin.md）

| 方法 | 端点 | 体 | 成功响应 |
|---|---|---|---|
| `POST` | `/rooms/{room_id}/report/{event_id}` | `{"reason"?, "score"?}`（score 默认 `-100`） | `{"report_id"}` |
| `POST` | `/client/v1/rooms/{room_id}/report/{event_id}` | 同上 | `{"report_id"}` |
| `POST` | `/rooms/{room_id}/report` | `{"reason"?, "description"?}` | `{"report_id", "room_id", "status": "submitted"}` |
| `POST` | `/users/{user_id}/report` | `{"reason": "..."}`（**字段必须存在**，可为空串） | `{}` |

- 事件举报与房间举报需要房间成员；事件不属于该房间 → 404。
- 用户举报（MSC4260）：`reason` 缺失 → 400 `M_BAD_JSON`；用户不存在 → 404 `M_NOT_FOUND`；
  **允许举报自己**。
- 三种响应形状不同，SDK 不要统一按 `{}` 处理。
- `PUT /_matrix/vendor/v1/rooms/{room_id}/report/{event_id}/score` **恒 403**；
  `GET .../scanner_info` 恒返回 `scanner_enabled: false`。

---

## 九、SDK 封装要点

### 9.1 归一化用户对象

同一用户在不同端点有三种形状，建议在 SDK 内部只暴露一个 `DirectoryUser`：

| 端点 | 显示名字段 | 额外字段 |
|---|---|---|
| `user_directory/search`、`user_directory/list` | `display_name` | 无 |
| `user_directory/profiles/{id}` | `displayname` | 无 |
| `search` 的 `users` | `display_name` | `presence`、`last_active_ts`、`match_score`、`match_type` |
| `vendor/search_recipients` | `display_name` | `username`、`online`、`presence` |

```typescript
function normalizeUser(raw: any): DirectoryUser {
  return {
    userId: raw.user_id,
    displayName: raw.display_name ?? raw.displayname ?? null,
    avatarUrl: raw.avatar_url ?? null,
    presence: raw.presence ?? null,
    online: raw.online ?? (raw.presence === 'online'),
    lastActiveTs: raw.last_active_ts ?? null,
  };
}
```

### 9.2 分页策略分三套

| 端点 | 分页 |
|---|---|
| `user_directory/list` | keyset：回填 `since = next_batch` |
| `user_directory/search` | **无分页**：用 `limited` 判断是否可能被截断，必要时增大 `limit` |
| `publicRooms` GET/POST | `since = next_batch`；**搜索结果（带 `generic_search_term`）无下一页** |

### 9.3 能力探测

- `GET /_matrix/client/v3/user_directory/list` 与 `/_matrix/vendor/v1/search_rooms`、
  `search_recipients` 是本仓扩展：**不要假设其它 homeserver 有**。
  建议在 SDK 初始化时用一次 404 探测或配置开关隔离。
- `privacy-ext` 默认关闭，意味着 `user_directory/profiles` 的 403 分支在默认部署下是死代码。
- `thirdparty/*` 的 4 条 `M_UNRECOGNIZED` 是设计行为，不要计入错误率监控。

### 9.4 限流

`search` 的 `users` 类别与 `search_recipients` 共用 `ratelimit:search-users:{user_id}`，
**2 次 / 20 秒**。SDK 的搜索输入框务必做防抖（建议 ≥ 300ms）并在 429 时读
`retry_after_ms` 退避，否则连续输入会整段被拒。

---

## 相关文档

- 房间别名、房间升级、目录可见性的房间侧语义：[rooms.md](./rooms.md)
- `/search` 的 `room_events` 类别与消息搜索：[messages.md](./messages.md)
- vendor 扩展总览：[extensions.md](./extensions.md)
- 举报的管理端消费：[admin.md](./admin.md)
- 错误码全表与限流头：[errors.md](./errors.md)
