# 房间管理

覆盖建房、加入/离开、成员与权限、状态事件、别名与目录、层级与升级。
消息发送与历史见 [messages.md](./messages.md)；房间摘要/线程/置顶等扩展见 [extensions.md](./extensions.md)。

## 目录

- [创建房间](#创建房间)
- [加入 / 敲门 / 离开 / 遗忘](#加入--敲门--离开--遗忘)
- [成员管理](#成员管理)
- [状态事件](#状态事件)
- [房间信息](#房间信息)
- [别名与目录可见性](#别名与目录可见性)
- [公共房间列表](#公共房间列表)
- [层级与空间](#层级与空间)
- [房间升级](#房间升级)
- [举报](#举报)
- [响应形状速查](#响应形状速查)

---

## 创建房间

**端点**：`POST /_matrix/client/v3/createRoom`

**认证**：是

**请求体**（字段以 `management/create.rs` 为准）：

```typescript
interface CreateRoomRequest {
  visibility?: 'public' | 'private';         // 目录可见性，默认 private
  room_alias_name?: string;                  // localpart（不含域名）
  name?: string;
  topic?: string;
  invite?: Array<string | { user_id: string; reason?: string }>;  // 最多 100 个
  invite_3pid?: Array<object>;
  preset?: 'private_chat' | 'trusted_private_chat' | 'public_chat';
  is_direct?: boolean;
  room_version?: string;                     // 默认取 capabilities.m.room_versions.default（当前 "12"）
  creation_content?: Record<string, unknown>; // 任意 m.room.create 内容，如 { "m.federate": false }
  initial_state?: Array<{ type: string; state_key?: string; content: object }>;
  power_level_content_override?: object;
  room_type?: string;                        // 如 "m.space"；也可用 creation_content.type
  ignore_duplicate_name?: boolean;           // 允许重名（默认 false）
}
```

**响应**：`{ "room_id": "!abc:matrix.test" }`

**行为与错误**

| 场景 | 结果 |
|---|---|
| `preset` 非法 | 400 `M_INVALID_PARAM`（`Invalid preset value`） |
| `invite` 超过 100 人 | 400 `M_BAD_JSON`（`Too many invites (max 100)`） |
| `room_version` 不支持 | 400 `M_UNSUPPORTED_ROOM_VERSION` |
| `room_version` 与默认不兼容 | 400 `M_INCOMPATIBLE_ROOM_VERSION` |
| 别名已被占用 | 400 `M_ROOM_IN_USE` |
| 重名且 `ignore_duplicate_name=false` | 400（`M_ROOM_IN_USE` 语义） |

> `creation_content` 不是 `{type, state_key, content}` 结构——它**直接就是** `m.room.create` 的
> content。历史文档把它写成三层嵌套，客户端照此传会创建出错误的 create 事件。

---

## 加入 / 敲门 / 离开 / 遗忘

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/rooms/{room_id}/join` | 按房间 ID 加入 |
| `POST` | `/join/{room_id_or_alias}` | 按 ID **或别名**加入（推荐：别名场景唯一可用路径） |
| `POST` | `/knock/{room_id_or_alias}` | 敲门（`join_rule=knock` 时） |
| `POST` | `/rooms/{room_id}/leave` | 离开 |
| `POST` | `/rooms/{room_id}/forget` | 遗忘（离开后清理本地房间记录） |

**加入**请求体可空 `{}`；`third_party_signed` 用于第三方邀请。响应 `{ "room_id": "..." }`。

**离开时连带遗忘**（MSC4267，能力 `m.forget_forced_upon_leave`）：

```json
POST /_matrix/client/v3/rooms/{roomId}/leave
{ "forget": true }
```

`forget` 在**请求体**里（不是查询参数）。为 `true` 时服务端在**同一事务**内完成
leave + forget，事后单独调 `/forget` 不会与它竞争。

**错误**

| 场景 | 状态 | errcode |
|---|---|---|
| 房间不存在 | 404 | `M_NOT_FOUND` |
| 被 ban | 403 | `M_FORBIDDEN` |
| 需要邀请 / 敲门 | 403 | `M_FORBIDDEN` |
| 别名无法解析 | 404 | `M_NOT_FOUND` |

---

## 成员管理

| 方法 | 端点 | 请求体 |
|---|---|---|
| `POST` | `/rooms/{room_id}/invite` | `{"user_id": "@bob:x", "reason": "可选"}` |
| `POST` | `/rooms/{room_id}/kick` | `{"user_id": "...", "reason": "可选"}` |
| `POST` | `/rooms/{room_id}/ban` | `{"user_id": "...", "reason": "可选"}` |
| `POST` | `/rooms/{room_id}/unban` | `{"user_id": "...", "reason": "可选"}` |
| `GET` | `/rooms/{room_id}/members` | 见下 |
| `GET` | `/rooms/{room_id}/joined_members` | `{"joined": {"@u:x": {"display_name": "...", "avatar_url": "..."}}}` |
| `GET` | `/joined_rooms` | `{"joined_rooms": ["!a:x", "!b:x"]}` |
| `POST` | `/_matrix/vendor/v1/rooms/{room_id}/get_membership_events` | 成员变更历史（**vendor 路径**） |
| `GET` | `/_matrix/vendor/v1/rooms/{room_id}/members/recent` | 最近加入成员 |
| `GET` | `/_matrix/vendor/v1/rooms/{room_id}/membership/{user_id}` | 单人成员状态 |

**`GET /rooms/{room_id}/members` 查询参数**

| 参数 | 默认 | 说明 |
|---|---|---|
| `at` | 无 | MSC4502 分页游标（取上次响应的 `next_batch`） |
| `limit` | `100` | 夹在 `1..=1000`（负数不再透传到 SQL） |
| `membership` | 无 | `join` / `invite` / `leave` / `ban` / `knock` |
| `not_membership` | 无 | 排除某状态 |

响应为 `{ "chunk": [ <m.room.member 事件> ], "next_batch": "<游标>" }`。
非成员访问**私有房间**的成员列表 → 403 `M_FORBIDDEN`。

**权限失败**统一 403 `M_FORBIDDEN`；被操作者不存在 → 404 `M_NOT_FOUND`。
踢人/封禁需要足够的 `m.room.power_levels`，不足时 403（不会静默成功）。

---

## 状态事件

### 读取

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/rooms/{room_id}/state` | 全部状态事件，**裸数组** `[{…}, …]` |
| `GET` | `/rooms/{room_id}/state?type={event_type}` | MSC4497：按类型过滤（精确匹配），同样返回裸数组 |
| `GET` | `/rooms/{room_id}/state/{event_type}` | 该类型 `state_key=""` 事件的 **content**；该空键事件不存在 → 404 `M_NOT_FOUND` |
| `GET` | `/rooms/{room_id}/state/{event_type}/` | 同上（尾斜杠形式，等价） |
| `GET` | `/rooms/{room_id}/state/{event_type}/{state_key}` | 指定状态键的 **content**；不存在 → 404 `M_NOT_FOUND` |

> 三个读取端点均与 CS-API 一致：全量状态是裸数组；单事件读**只返回 content**，不含 `event_id` / `type`。
> `/state/{event_type}` 只认 `state_key=""` 的事件，同类型的其它状态键（如各用户自己的 `m.room.member`）不会被当作答案。

### 写入

| 方法 | 端点 | 说明 |
|---|---|---|
| `PUT` | `/rooms/{room_id}/state/{event_type}` | 写 `state_key=""` |
| `PUT` | `/rooms/{room_id}/state/{event_type}/` | 尾斜杠 = 空 `state_key` |
| `PUT` | `/rooms/{room_id}/state/{event_type}/{state_key}` | 写指定状态键 |
| `POST` | `/rooms/{room_id}/state/{event_type}` | 同上（POST 形态） |

响应 `{ "event_id": "$..." }`。

**MSC4354 Sticky 时长**：上述写路径与 `/send` 一样接受 sticky 参数
（`?org.matrix.msc4354.sticky_duration_ms=` 或短名 `sticky_duration_ms`），
取值必须为 `1..=3600000`（1 小时），超界/非整数 → 400 `M_INVALID_PARAM`。
详见 [extensions.md](./extensions.md#sticky-事件msc4354)。

**常用状态事件**

| 类型 | content 示例 |
|---|---|
| `m.room.name` | `{"name": "项目组"}` |
| `m.room.topic` | `{"topic": "..."}` |
| `m.room.avatar` | `{"url": "mxc://..."}` |
| `m.room.join_rules` | `{"join_rule": "public"\|"invite"\|"knock"\|"restricted"\|"knock_restricted", "allow": [{"type":"m.room_membership","room_id":"!x"}]}` |
| `m.room.history_visibility` | `{"history_visibility": "shared"\|"invited"\|"joined"\|"world_readable"}` |
| `m.room.guest_access` | `{"guest_access": "can_join"\|"forbidden"}` |
| `m.room.encryption` | `{"algorithm": "m.megolm.v1.aes-sha2", "rotation_period_ms": 604800000}` |
| `m.room.power_levels` | `{"users": {"@a:x": 100}, "events": {...}, "invite": 0, "kick": 50, ...}` |

> `m.room.join_rules` 用 `restricted` 时 `allow` 是**对象数组**（`{"type","room_id"}`），
> 不是字符串数组。旧文档写成 `allow?: string[]` 是错的。

---

## 房间信息

`GET /_matrix/client/v3/rooms/{room_id}`

**要求**：调用者必须是房间成员，否则 404 `M_NOT_FOUND`（`Room not found or not a member`）。

```json
{
  "room_id": "!abc:matrix.test",
  "name": "项目组",
  "avatar_url": "mxc://matrix.test/xyz",
  "topic": "排期与评审",
  "canonical_alias": "#project:matrix.test",
  "joined_members_count": 12,
  "invited_members_count": 1,
  "world_readable": false,
  "guest_can_join": false,
  "membership": "join"
}
```

其他房间级读取端点：

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/rooms/{room_id}/aliases` | 该房间的本地别名列表 |
| `GET` | `/rooms/{room_id}/event/{event_id}` | 单个事件（**含 `unsigned`**，sticky TTL 要在这里读） |
| `GET` | `/rooms/{room_id}/initialSync` | 旧版初始同步（`/sync` 已覆盖，仅为兼容保留） |
| `GET` | `/rooms/{room_id}/ephemeral` | 瞬时事件 |
| `GET` | `/rooms/{room_id}/context/{event_id}` | 事件上下文（前后各若干条） |
| `GET` | `/rooms/{room_id}/messages` | 历史（见 [messages.md](./messages.md)） |

---

## 别名与目录可见性

**这是两个不同的东西**，历史文档把它们混成了一个端点：

| 目的 | 方法 | 端点 | 成功响应 |
|---|---|---|---|
| 创建别名 | `PUT` | `/directory/room/{room_alias}` | `{"room_id", "alias", "created_ts"}` |
| 删除别名 | `DELETE` | `/directory/room/{room_alias}` | `{"removed": true, "alias"}` |
| 解析别名 | `GET` | `/directory/room/{room_alias}` | **`{"room_id"}`**（无 `servers`） |
| 房间挂别名 | `PUT` | `/directory/room/{room_id}/alias/{room_alias}` | `{"room_id", "alias", "created_ts"}` |
| 房间摘别名 | `DELETE` | `/directory/room/{room_id}/alias/{room_alias}` | `{}` |
| 列房间别名 | `GET` | `/directory/room/{room_id}/alias` | `{"aliases": [...]}` |
| **设置目录可见性** | `PUT` | `/directory/list/room/{room_id}` — 体 `{"visibility": "public"\|"private"}` | `{"room_id", "visibility", "updated_ts"}` |
| **读取目录可见性** | `GET` | `/directory/list/room/{room_id}` | `{"visibility": "..."}` |

- `PUT /directory/room/{room_alias}` 体**必须含 `room_id`**，且以 `!` 开头；
  `servers` 字段即使传入也被忽略。
- 别名格式必须为 `#localpart:server`；非法 → 400 `M_INVALID_PARAM`。
- 别名已存在 → 400 `M_ROOM_IN_USE`；调用者无权 → 403 `M_FORBIDDEN`。
- `PUT /directory/list/room/{room_id}` 需要成员 + 房间创建者（creator），否则 403；
  `visibility` 只接受 `public` / `private`，否则 400 `M_BAD_JSON`。

> **没有** `PUT /directory/room/{room_id}`。想改"是否公开"，用 `/directory/list/room/{room_id}`。
> **也没有** `DELETE /directory/list/room/{room_id}`（该路由不存在，会得到 405）——
> 取消公开发布用 `PUT` + `{"visibility": "private"}`。
> 这些端点**只注册在 `v3`**，没有 `r0` / `v1` 变体。

---

## 公共房间列表

| 方法 | 端点 | 说明 |
|---|---|---|
| `GET` | `/publicRooms` | 查询（`limit`、`since`、`server`；**不读 `filter`**） |
| `POST` | `/publicRooms` | 带 filter 体的查询（按关键词/房间类型过滤必须用这个） |

**响应**：

```json
{
  "chunk": [
    { "room_id": "!x:matrix.test", "name": "项目组", "topic": "...", "avatar_url": "mxc://...",
      "num_joined_members": 12, "world_readable": true, "guest_can_join": false,
      "canonical_alias": "#project:matrix.test", "join_rule": "public", "room_type": null }
  ],
  "next_batch": "t7",
  "total_room_count_estimate": 42
}
```

`POST` 体：

```json
{
  "limit": 20,
  "since": "t7",
  "filter": { "generic_search_term": "项目", "room_types": ["m.space"] },
  "include_all_networks": false,
  "third_party_instance_id": null
}
```

**分页与过滤的硬约束**：

- `limit`：GET/POST 均为默认 `20`，clamp 到 `1..=1000`。
- `next_batch` **仅当**本次返回条数等于 `limit` 时才有；否则视为没有下一页。
- **POST 且带 `generic_search_term` 时不返回 `next_batch`**——搜索结果不分页。
- `chunk[].room_type` 在本仓**恒为 `null`**，不能用来识别 space。
- `include_all_networks: true`、`third_party_instance_id` 非 `null`、`filter` 非对象、
  `filter` 里出现这两个键、`generic_search_term` 非字符串、`room_types` 非数组
  —— 全部 400 `M_INVALID_PARAM`。
- `room_types` 的条目可为字符串或 `null`；`null` 表示"包含普通房间"。

完整规则见 [directory.md](./directory.md#三房间目录-publicrooms)。

---

## 层级与空间

| 方法 | 端点 | 版本 |
|---|---|---|
| `GET` | `/rooms/{room_id}/hierarchy` | v3 |
| `GET` | `/client/v1/rooms/{room_id}/hierarchy` | v1 |
| `GET` | `/spaces/{space_id}/hierarchy` | v1 / v3 |
| `GET` | `/spaces/{space_id}/hierarchy/v1` | 显式 v1 |

参数：`suggested_only`、`limit`、`max_depth`、`from`。

空间（`room_type: "m.space"`）的完整管理（`/_matrix/vendor/v1/spaces/*`：
children、tree_path、summary、members、invite/join/leave）见 [extensions.md](./extensions.md#spaces)。

---

## 房间升级

`POST /_matrix/client/v3/rooms/{room_id}/upgrade`

```json
{ "new_version": "12" }
```

**响应**：`{"replacement_room": "!new:matrix.test"}`

- 需要 `m.room.tombstone` 的写权限；不足 → 403 `M_FORBIDDEN`。
- 房间版本未知 → 400 `M_UNSUPPORTED_ROOM_VERSION`。
- 升级会在旧房间写入 `m.room.tombstone` 并在新房间建 `m.room.create`；
  客户端应把旧房间标记为"已升级"，按 `replacement_room` 跳转。

---

## 举报

| 方法 | 端点 | 体 | 成功响应 |
|---|---|---|---|
| `POST` | `/rooms/{room_id}/report` | `{"reason"?, "description"?}`（举报房间） | `{"report_id", "room_id", "status": "submitted"}` |
| `POST` | `/rooms/{room_id}/report/{event_id}` | `{"reason"?, "score"?}`（举报事件，score 默认 `-100`） | `{"report_id"}` |
| `POST` | `/users/{user_id}/report` | `{"reason": "..."}`（举报用户；**字段必须存在**，可为空串） | `{}` |
| `POST` | `/client/v1/rooms/{room_id}/report/{event_id}` | v1 变体 | `{"report_id"}` |

- 事件/房间举报需要房间成员；事件不存在或不属于该房间 → 404。
- 用户举报（MSC4260）：`reason` 缺失 → 400 `M_BAD_JSON`；用户不存在 → 404 `M_NOT_FOUND`；
  允许举报自己。
- **三种响应形状不同**，SDK 不要统一按 `{}` 处理。
- `PUT /_matrix/vendor/v1/rooms/{room_id}/report/{event_id}/score` 恒 403；
  `GET .../scanner_info` 恒返回 `scanner_enabled: false`。
- 举报进入管理端的 `/_synapse/admin/v1/event_reports`（见 [admin.md](./admin.md)）。

完整契约见 [directory.md](./directory.md) 的「举报」一节。

---

## 响应形状速查

| 端点 | 成功响应 |
|---|---|
| `POST /createRoom` | `{"room_id"}` |
| `POST /rooms/{id}/join`、`/join/{idOrAlias}` | `{"room_id"}` |
| `POST /rooms/{id}/leave`、`/knock/*`、`/forget` | `{}` |
| `POST /rooms/{id}/invite`、`/kick`、`/ban`、`/unban` | `{}` |
| `PUT /rooms/{id}/state/*`、`POST /rooms/{id}/state/*` | `{"event_id"}` |
| `GET /rooms/{id}/state` | `{"events": [...]}`（**不是裸数组**） |
| `GET /rooms/{id}/state/{type}/{key}` | 该事件的 `content`（**只有 content**，不含 event_id） |
| `GET /rooms/{id}/members` | `{"chunk": [...], "next_batch"?: "..."}` |
| `GET /rooms/{id}/joined_members` | `{"joined": {"@u": {"display_name", "avatar_url"}}}` |
| `GET /joined_rooms` | `{"joined_rooms": [...]}` |
| `GET /rooms/{id}` | 房间信息对象（见上） |
| `GET /rooms/{id}/aliases` | `{"aliases": [...]}` |
| `GET /directory/room/{alias}` | `{"room_id"}`（**无 `servers`**） |
| `PUT /directory/room/{alias}` | `{"room_id", "alias", "created_ts"}` |
| `DELETE /directory/room/{alias}` | `{"removed": true, "alias"}` |
| `PUT/DELETE /directory/room/{id}/alias/{alias}` | `{"room_id", "alias", "created_ts"}` / `{}` |
| `GET /directory/list/room/{id}` | `{"visibility"}` |
| `PUT /directory/list/room/{id}` | `{"room_id", "visibility", "updated_ts"}` |
| `POST /rooms/{id}/upgrade` | `{"replacement_room"}` |
| `POST /rooms/{id}/report/{event_id}` | `{"report_id"}` |
| `POST /rooms/{id}/report` | `{"report_id", "room_id", "status": "submitted"}` |
| `POST /users/{id}/report` | `{}` |
