# 实时通信（RTC）：VoIP、TURN、Widget

本页覆盖三类易被 SDK 混在一起的能力：

| 能力 | 命名空间 | 默认可用 | 用途 |
|---|---|---|---|
| TURN/STUN 凭据 | `/_matrix/client/v3/voip/*` | ✅ | 1:1 或群组通话的 ICE 服务器 |
| MatrixRTC transports（MSC4143） | `/_matrix/client/unstable/org.matrix.msc4143/rtc/transports` | ✅ | 新版 MatrixRTC/Element Call 的 ICE 服务器 |
| Widget API（MSC4261） | `/_matrix/vendor/v1/*` | ✅ | 房间内嵌应用（Jitsi、白板等） |
| MSC3079 call 事件 | `/_matrix/client/v3/rooms/{room_id}/send/m.call.*` | ⚠️ **需 `voip-tracking` 构建特性** | 服务端托管通话状态机 |

> 这 5 条 call 路由见于 SDK lane 的 `tests/unit/fixtures/ledger_export_sdk/default.json`，
> 不在 golden lane 的 `docs/openapi/route-table.json` 中。
> 它们在 `synapse-web/src/routes/assembly.rs` 中被 `#[cfg(feature = "voip-tracking")]` 包裹，
> 只有启用该 Cargo feature 才会注册。参考部署（`deploy.sh` 使用 `--features all-extensions`）
> **已包含** `voip-tracking`，因此运行时可用；但默认特性集不含它，SDK 仍应以探测结果为准。

## 目录

- [能力发现](#能力发现)
- [TURN/STUN 凭据](#turnstun-凭据)
- [MatrixRTC transports（MSC4143）](#matrixrtc-transportsmsc4143)
- [Widget API（MSC4261）](#widget-apimsc4261)
- [MSC3079 call 事件（feature-gated）](#msc3079-call-事件feature-gated)
- [错误与状态码](#错误与状态码)
- [客户端接入建议](#客户端接入建议)

---

## 能力发现

不要用产品版本号判断 RTC 能力，读两个字段：

```jsonc
// GET /_matrix/client/v3/versions
{
  "unstable_features": {
    "org.matrix.msc4143": true,          // RTC transports 路由存在
    "org.matrix.msc3245.voice": true     // voice 扩展（/_matrix/vendor/v1/voice/config）路由存在
  }
}
```

```jsonc
// GET /_matrix/client/v3/capabilities（需认证）
{
  "capabilities": {
    "m.voice": { "enabled": true }
  }
}
```

两者的判定都来自**路由探针**（`synapse-services/src/capability_governance.rs`），
不是配置开关：路由没注册就是 `false`。`m.voice` / `org.matrix.msc3245.voice`
宣告的是**语音消息**（MSC3245）能力，探针为 `GET /_matrix/vendor/v1/voice/config`；
未编译 `voice-extended` 特性时两者均为 `false`。**TURN 凭据是另一件事**：
是否真的拿到服务器要调用 `/voip/config` 才知道（见下一节）。

---

## TURN/STUN 凭据

### 端点

| 方法 | 端点 | 认证 | 说明 |
|---|---|---|---|
| `GET` / `POST` | `/_matrix/client/v3/voip/turnServer` | 需 `Authorization` | 单组 TURN 凭据（Matrix 规范端点） |
| `GET` | `/_matrix/client/v3/voip/config` | 需 `Authorization` | TURN + STUN 组合配置（本仓扩展语义） |
| `GET` | `/_matrix/client/v3/voip/turnServer/guest` | **仍需 `Authorization`** | guest 凭据；名字里的 guest 只是“用 guest 身份签发” |

> `turnServer/guest` 并不免认证：handler 仍要求 `AuthenticatedUser`。未启用 VoIP 时它返回
> `404 M_NOT_FOUND`，而不是像 `/voip/turnServer` 那样返回空凭据。

### `GET|POST /voip/turnServer`

响应：

```jsonc
{
  "username": "1750000000:@alice:matrix.test",
  "password": "base64-hmac",
  "uris": ["turn:turn.matrix.test:3478?transport=udp"],
  "ttl": 86400
}
```

- `ttl` 单位是**秒**，表示这组凭据的有效期；到期前重新调用即可。
- 服务端**未启用 VoIP**（`voip.enabled = false`）时**不报错**，返回
  `{"username":"","password":"","uris":[],"ttl":0}`。SDK 必须以 `uris` 为空作为“无 TURN”判据，
  不要只看 HTTP 200。
- 已启用但当前用户是 guest 且禁用 guest 使用 TURN → `403 M_FORBIDDEN`
  （`Guest access to TURN server is disabled`）。

### `GET /voip/config`

响应：

```jsonc
{
  "turn_servers": [
    { "username": "...", "password": "...", "uris": ["turn:..."], "ttl": 86400 }
  ],
  "stun_servers": ["stun:stun.matrix.test:3478"]
}
```

行为矩阵（顺序即 handler 的判断顺序）：

| 场景 | `turn_servers` | `stun_servers` |
|---|---|---|
| VoIP 未启用 | `[]`（**不是 `null`**） | `null` |
| 已启用、未配置 TURN URI | `[]` | 有 STUN 则为数组，否则 `null` |
| 已启用、配置了静态 `turn_username`/`turn_password` | 1 项，`ttl = 86400` | 同上 |
| 已启用、HMAC 动态凭据 | 1 项，`ttl` 由服务端给出 | 同上 |
| 已启用、动态凭据生成失败（未配 shared secret） | `[]`（**静默回退，不报错**） | 同上 |

> `turn_servers` 永远是数组（可能是空数组），`stun_servers` 可能是 `null`。
> 客户端不要假设 `stun_servers` 一定存在。

### `GET /voip/turnServer/guest`

响应形状同 `/voip/turnServer`，用户名基于固定身份 `@guest:anonymous` 生成。

| 场景 | 结果 |
|---|---|
| VoIP 未启用 | `404 M_NOT_FOUND`（`VoIP/TURN service is not configured`） |
| 禁用 guest 使用 TURN | `403 M_FORBIDDEN` |
| 正常 | `200` + `TurnServerResponse` |

---

## MatrixRTC transports（MSC4143）

### 端点

```http
GET /_matrix/client/unstable/org.matrix.msc4143/rtc/transports
Authorization: Bearer <token>
```

这是 Element Call / MatrixRTC 使用的入口，返回 MSC4403 形状的 ICE server transport。
本仓**无条件注册**该路由（不依赖 `voip-tracking`），但只有 `/versions` 的
`org.matrix.msc4143` 为 `true` 时才应调用。

### 响应

```jsonc
{
  "transports": [
    {
      "type": "org.matrix.msc4403.ice-server-transport",
      "ice_servers": [
        { "urls": ["stun:stun.matrix.test:3478"] },
        { "urls": ["turn:turn.matrix.test:3478?transport=udp"],
          "username": "...", "credential": "..." }
      ]
    }
  ]
}
```

关键点：

- TURN 凭据字段是 **`credential`**（MSC4403/WebRTC 口径），**不是** `password`。
  `/voip/turnServer` 返回的 `password` 要在这里改名后再用。
- STUN 项只有 `urls`，没有 `username` / `credential`。
- `urls` 是**数组**（`Vec<String>`），即使只有一条。
- 未启用 VoIP、或 STUN/TURN 都没配出任何一项时，返回 `{"transports": []}`（200，不报错）。
- 只有一种 `type`：`org.matrix.msc4403.ice-server-transport`。

### 与 `/voip/config` 的差异

| 项 | `/voip/config` | `/rtc/transports` |
|---|---|---|
| 顶层 | `turn_servers` / `stun_servers` | `transports[]` |
| 凭据字段 | `password` | `credential` |
| STUN 与 TURN 是否合并 | 分开两个数组 | 合并进同一个 `ice_servers` |
| 未启用 | `turn_servers: []` | `transports: []` |

SDK 里建议**优先用 MSC4143**（新版客户端），旧版回退到 `/voip/config`。

---

## Widget API（MSC4261）

Widget 全部挂在 **`/_matrix/vendor/v1`**，**没有** `/_matrix/client/v1|v3` 别名
（历史上的 `/client/v3/widgets/create`、`/client/v1/widgets` 已被删除）。

`/versions.unstable_features` **不声明** widget 的 key。能力探测只能依据
`POST /_matrix/vendor/v1/widgets` 是否存在，或直接调用后按 404 处理。

### 端点清单

| 方法 | 端点 | 说明 |
|---|---|---|
| `POST` | `/widgets` | 创建 widget（房间级） |
| `GET` | `/widgets/{widget_id}` | 读取 |
| `PUT` | `/widgets/{widget_id}` | 更新 `url` / `name` / `data` |
| `DELETE` | `/widgets/{widget_id}` | 删除 → `{"deleted": true}` |
| `GET` | `/widgets/{widget_id}/config` | 给 widget iframe 用的配置 |
| `GET` | `/rooms/{room_id}/widgets` | 房间内全部 widget |
| `GET` | `/rooms/{room_id}/widgets/jitsi/config` | Jitsi 会议配置 |
| `GET` / `PUT` | `/rooms/{room_id}/widgets/{widget_id}/capabilities` | 能力位 |
| `POST` / `GET` | `/widgets/{widget_id}/permissions` | 设置 / 列出权限 |
| `DELETE` | `/widgets/{widget_id}/permissions/{user_id}` | 删除某用户权限 |
| `POST` / `GET` | `/widgets/{widget_id}/sessions` | 创建 / 列出会话 |
| `GET` / `DELETE` | `/widgets/sessions/{session_id}` | 读取 / 终止单个会话 |

> 单会话路径是 `/widgets/sessions/{session_id}`，**不是**
> `/widgets/{widget_id}/sessions/{session_id}`。

### 创建与读取

`POST /widgets` 请求体（四个字段都必填，缺失/空串 → `400 M_BAD_JSON`）：

```jsonc
{
  "room_id": "!abc:matrix.test",
  "widget_type": "m.custom",
  "url": "https://widget.example/app",
  "name": "My Widget",
  "data": { "anything": true }        // 可选，默认 {}
}
```

响应（`WidgetApiResponse`）：

```jsonc
{
  "widget_id": "widget_<uuid>",
  "room_id": "!abc:matrix.test",
  "user_id": "@alice:matrix.test",
  "type": "m.custom",                 // 注意：请求里叫 widget_type，响应里叫 type
  "url": "https://widget.example/app",
  "name": "My Widget",
  "data": {},
  "creator": "@alice:matrix.test",
  "active": true
}
```

- `url` 只允许 `http` / `https` 且必须有 host；`javascript:`、`data:` → `400`。
- 创建者必须是房间成员，且是**房间管理员或房间创建者**，否则 `403`。
- 房间不存在 → `404`。

`GET /rooms/{room_id}/widgets` 返回的是 `Widget` 实体，字段名与单条接口**不一致**：

```jsonc
{
  "total": 1,
  "widgets": [
    { "id": 1, "widget_id": "widget_...", "room_id": "!abc:matrix.test",
      "user_id": "@alice:matrix.test", "widget_type": "m.custom",
      "url": "https://...", "name": "My Widget", "data": {},
      "created_ts": 1750000000000, "updated_ts": null, "is_active": true }
  ]
}
```

差异对照：

| 单条接口（`WidgetApiResponse`） | 列表接口（`Widget`） |
|---|---|
| `type` | `widget_type` |
| `creator` | `user_id` |
| `active` | `is_active` |
| 无 | `id`、`created_ts`、`updated_ts` |

### 配置与能力

`GET /widgets/{widget_id}/config`（供 iframe 初始化）：

```jsonc
{ "widget_id": "...", "room_id": "!abc:matrix.test",
  "url": "https://...", "name": "My Widget", "data": {}, "type": "m.custom" }
```

`GET /rooms/{room_id}/widgets/{widget_id}/capabilities`：

```jsonc
{ "capabilities": ["m.send_event", "m.receive_event"],
  "widget_id": "...", "room_id": "!abc:matrix.test" }
```

- `capabilities` 取自 widget 的 `data.capabilities`，没有则为 `[]`。
- `PUT` 只接受 `{"capabilities": [...]}`（`deny_unknown_fields`）。
- widget 不属于该房间 → `400`。

### 权限

`POST /widgets/{widget_id}/permissions`：

```jsonc
// 请求
{ "user_id": "@bob:matrix.test", "permissions": ["read", "write"] }
// 响应
{ "success": true, "permission_id": 12 }
```

`GET /widgets/{widget_id}/permissions`：

```jsonc
{ "permissions": [
  { "id": 12, "widget_id": "...", "user_id": "@bob:matrix.test",
    "permissions": ["read", "write"], "created_ts": 1750000000000, "updated_ts": null }
] }
```

访问判定：创建者始终可读写；其他房间成员默认只读；写操作需要 `write` 或通配 `"*"` 权限。

### 会话

`POST /widgets/{widget_id}/sessions`：

```jsonc
// 请求（均可选）
{ "widget_id": "widget_...", "device_id": "DEVICE", "expires_in_ms": 60000 }
// 响应
{ "session": { "id": 3, "session_id": "sess_...", "widget_id": "...",
               "user_id": "@alice:matrix.test", "device_id": "DEVICE",
               "created_ts": 1750000000000, "last_active_ts": null,
               "expires_at": 1750000060000, "is_active": true } }
```

- body 里若带 `widget_id`，必须与路径一致，否则 `400`。
- `GET /widgets/{widget_id}/sessions` → `{ "sessions": [...], "total": n }`（需要 `write` 权限）。
- `GET /widgets/sessions/{session_id}` → `{ "session": {...} }`。
- `DELETE /widgets/sessions/{session_id}` → `{ "terminated": true }`。

### Jitsi 配置

`GET /rooms/{room_id}/widgets/jitsi/config` 返回**占位值**，不读真实 Jitsi 配置：

```jsonc
{ "conf_id": "abc_jitsi_conference", "name": "Jitsi Conference",
  "domain": "meet.jit.si", "app_id": null, "jwt": null }
```

> `conf_id` 由 `room_id` 去掉 `!`、把 `:` 换成 `_` 后拼 `_jitsi_conference` 得到。
> 不要把它当作服务端生成的会议 ID。

---

## MSC3079 call 事件（feature-gated）

这 5 条路由由 `voip-tracking` 特性注册。参考部署（`all-extensions`）已启用，
**默认特性集下为 404**。SDK 可用 `GET /rooms/{room_id}/call/{call_id}` 探测，
或首次调用收到 404 后回退为客户端直连。

| 方法 | 端点 | 成功响应 |
|---|---|---|
| `PUT` | `/rooms/{room_id}/send/m.call.invite/{txn_id}` | `{"event_id": "$..."}` |
| `PUT` | `/rooms/{room_id}/send/m.call.candidates/{txn_id}` | `{}` |
| `PUT` | `/rooms/{room_id}/send/m.call.answer/{txn_id}` | `{"event_id": "$..."}` |
| `PUT` | `/rooms/{room_id}/send/m.call.hangup/{txn_id}` | `{}` |
| `GET` | `/rooms/{room_id}/call/{call_id}` | 见下 |

### 请求体

```jsonc
// m.call.invite
{ "call_id": "call-1", "version": 0,
  "offer": { "type": "offer", "sdp": "v=0..." },
  "invitee": "@bob:matrix.test",
  "lifetime": 60000 }

// m.call.candidates
{ "call_id": "call-1", "version": 0,
  "candidates": [
    { "candidate": "candidate:...", "sdpMid": "0",
      "sdpMLineIndex": 0, "type": "host" }
  ] }

// m.call.answer
{ "call_id": "call-1", "version": 0,
  "answer": { "type": "answer", "sdp": "v=0..." } }

// m.call.hangup
{ "call_id": "call-1", "version": 0 }
```

- `invite` 只有 `call_id`、`version` 必填；`offer` / `invitee` / `lifetime` 可选，
  `lifetime` 单位是**毫秒**。
- `invitee` 必须是**字符串 user_id** 才会被写入 `callee_id`；传对象（旧版 Matrix 的
  `{"user_id": ...}` 形状）会导致 `callee_id` 为 `null`，随后 `answer` 必然 403。
- `candidates` 里的字段名是 `sdpMid` / `sdpMLineIndex`（驼峰），不是 snake_case。

### 状态机与授权

会话创建时 `state = "ringing"`；`answer` 置为 `"connected"`；`hangup` 置为 `"ended"`。
`"held"` 由存储层定义但当前没有路由会产生它。

| 操作 | 授权规则 | 失败 |
|---|---|---|
| invite | 房间成员 | 该 `call_id` 已存在且未结束 → **409** |
| candidates | caller 或 callee | 非参与者 → **403**；会话不存在 → **404** |
| answer | **仅 callee** | caller 自己应答 → **403** |
| hangup | caller 或 callee | 非参与者 → **403** |
| get call | 房间成员 | 会话不存在或不属于该房间 → **404** |

> 所有 5 条都要求调用者是房间成员（`ensure_call_room_member`），否则 **403 `M_FORBIDDEN`**。

### `GET /rooms/{room_id}/call/{call_id}`

```jsonc
{ "call_id": "call-1", "room_id": "!abc:matrix.test",
  "caller_id": "@alice:matrix.test", "callee_id": "@bob:matrix.test",
  "state": "ringing",
  "candidates": [
    { "candidate": "candidate:...", "sdpMid": "0", "sdpMLineIndex": 0, "type": "host" }
  ] }
```

- `state` 取值：`ringing` / `connected` / `held` / `ended`。
- `candidates` 是历次 `m.call.candidates` 写入的候选对象数组；无人上报时为 `[]`。
- `callee_id` 可能为 `null`（见上面的 `invitee` 类型说明）。

> `call_invite` / `call_answer` 会把事件真正写入房间时间线（`m.call.invite` /
> `m.call.answer`），`txn_id` 的幂等语义与普通发送一致；SDK 应复用同一个 `txn_id` 重试。
> `candidates` / `hangup` **不写时间线**，只更新会话状态，因此没有 `event_id`。
>
> **SDK 的默认路径仍应是客户端直连**（把 `m.call.*` 作为普通 `PUT /rooms/{id}/send/...`
> 事件发送），只在确认托管端点可用时才使用这组 API。

---

## 错误与状态码

| 场景 | HTTP | `errcode` |
|---|---|---|
| 缺/坏 `Authorization` | `401` | `M_UNAUTHORIZED` |
| token 无效/过期 | `401` | `M_UNKNOWN_TOKEN` |
| guest 被禁止使用 TURN | `403` | `M_FORBIDDEN` |
| 非房间成员访问 call / widget | `403` | `M_FORBIDDEN` |
| 非管理员/创建者创建房间 widget | `403` | `M_FORBIDDEN` |
| widget URL scheme 非法 | `400` | `M_BAD_JSON` |
| widget 缺必填字段 | `400` | `M_BAD_JSON` |
| widget 不属于该房间 | `400` | `M_BAD_JSON` |
| widget / session / call 不存在 | `404` | `M_NOT_FOUND` |
| 重复 invite 且会话未结束 | `409` | `M_UNKNOWN` |
| `turnServer/guest` 且 VoIP 未启用 | `404` | `M_NOT_FOUND` |

> VoIP 未启用时 `/voip/turnServer`、`/voip/config`、`/rtc/transports` 都返回 **200**，
> 靠空数组表达“无可用服务器”。只有 `/voip/turnServer/guest` 返回 404。

---

## 客户端接入建议

1. **先发现再调用**：读 `/versions` 的 `org.matrix.msc4143` 与 `org.matrix.msc3245.voice`，
   不要按产品版本硬编码。
2. **凭据缓存到 `ttl`**：TURN 凭据有 `ttl`（静态凭据固定 86400 秒），到期前 5 分钟刷新一次；
   不要在每次通话时都请求。
3. **空数组即降级**：`uris` / `turn_servers` / `ice_servers` 为空时走“无 TURN”分支
   （仅 STUN 或直连），不要弹错误。
4. **字段改名**：`/voip/turnServer` 的 `password` → WebRTC 的 `credential`；
   `widget_type`（请求）↔ `type`（响应）↔ `widget_type`（列表）。
5. **Widget 权限前置校验**：创建前确认自己是房间管理员或创建者，否则会 403；
   iframe 侧通过 `/widgets/{id}/config` 拿 `data`，通过 `capabilities` 决定能发什么事件。
6. **MSC3079 托管端点按需使用**：参考部署已编译（`voip-tracking`），默认特性集没有；
   先探测 `GET /rooms/{room_id}/call/{call_id}`，收到 404 就把 `m.call.*` 当普通房间事件发送。
   即使托管端点可用，纯客户端直连仍是兼容性最好的路径。
7. **session 生命周期**：`expires_in_ms` 是毫秒；`expires_at` 是毫秒时间戳；
   终止会话用 `DELETE /widgets/sessions/{session_id}`。
