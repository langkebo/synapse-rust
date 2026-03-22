# synapse-rust API 完整文档 - 请求/响应示例与错误码

> 生成时间: 2026-03-22
> 端点数量: 284+
> 项目: synapse-rust

---

## 目录

1. [认证 API](#1-认证-api)
2. [用户管理 API](#2-用户管理-api)
3. [房间管理 API](#3-房间管理-api)
4. [消息 API](#4-消息-api)
5. [设备管理 API](#5-设备管理-api)
6. [E2EE 加密 API](#6-e2ee-加密-api)
7. [媒体 API](#7-媒体-api)
8. [好友系统 API](#8-好友系统-api)
9. [Space API](#9-space-api)
10. [Thread API](#10-thread-api)
11. [搜索 API](#11-搜索-api)
12. [推送 API](#12-推送-api)
13. [Widget API](#13-widget-api)
14. [Sliding Sync API](#14-sliding-sync-api)
15. [管理后台 API](#15-管理后台-api)
16. [联邦 API](#16-联邦-api)
17. [应用服务 API](#17-应用服务-api)
18. [CAS 认证 API](#18-cas-认证-api)
19. [SAML 认证 API](#19-saml-认证-api)
20. [OIDC 认证 API](#20-oidc-认证-api)
21. [QR 登录 API](#21-qr-登录-api)
22. [语音消息 API](#22-语音消息-api)
23. [VoIP API](#23-voip-api)
24. [密钥备份 API](#24-密钥备份-api)
25. [保留策略 API](#25-保留策略-api)
26. [媒体配额 API](#26-媒体配额-api)
27. [事件举报 API](#27-事件举报-api)
28. [账户数据 API](#28-账户数据-api)
29. [注册令牌 API](#29-注册令牌-api)
30. [Worker API](#30-worker-api)

---

## 错误码参考

### HTTP 状态码映射

| HTTP 状态码 | 错误码 (errcode) | 说明 |
|------------|------------------|------|
| 400 | M_BAD_JSON | JSON 格式错误 |
| 400 | M_INVALID_PARAM | 参数无效 |
| 400 | M_INVALID_USERNAME | 用户名无效 |
| 401 | M_MISSING_TOKEN | 缺少访问令牌 |
| 401 | M_UNKNOWN_TOKEN | 令牌未知 |
| 401 | M_UNAUTHORIZED | 未授权 |
| 403 | M_FORBIDDEN | 禁止访问 |
| 404 | M_NOT_FOUND | 资源未找到 |
| 409 | M_USER_IN_USE | 用户名已被使用 |
| 409 | M_ROOM_IN_USE | 房间别名已被使用 |
| 410 | M_GONE | 资源已删除 |
| 422 | M_INVALID_USERNAME | 用户名无效 |
| 429 | M_LIMIT_EXCEEDED | 超过速率限制 |
| 500 | M_UNKNOWN | 服务器内部错误 |

---

## 1. 认证 API

### 1.1 用户登录

**端点:** `POST /_matrix/client/r0/login`

**请求体:**
```json
{
  "type": "m.login.password",
  "identifier": {
    "type": "m.id.user",
    "user": "alice"
  },
  "password": "secretpassword",
  "initial_device_display_name": "My Device"
}
```

**成功响应 (200):**
```json
{
  "access_token": "syt_yoursever_abc123def456",
  "refresh_token": "syr_abc123def456ghi789",
  "expires_in": 3600,
  "device_id": "ABCDEFGH",
  "user_id": "@alice:example.com"
}
```

**错误响应 (401):**
```json
{
  "errcode": "M_UNKNOWN",
  "error": "Invalid password"
}
```

**错误响应 (403):**
```json
{
  "errcode": "M_USER_DEACTIVATED",
  "error": "This user account has been deactivated"
}
```

---

### 1.2 用户注册

**端点:** `POST /_matrix/client/r0/register`

**请求体:**
```json
{
  "auth": {
    "type": "m.login.dummy"
  },
  "username": "alice",
  "password": "secretpassword"
}
```

**成功响应 (200):**
```json
{
  "access_token": "syt_yoursever_abc123def456",
  "refresh_token": "syr_abc123def456ghi789",
  "expires_in": 3600,
  "device_id": "ABCDEFGH",
  "user_id": "@alice:example.com"
}
```

**错误响应 (400):**
```json
{
  "errcode": "M_INVALID_USERNAME",
  "error": "User ID must be lowercase"
}
```

**错误响应 (409):**
```json
{
  "errcode": "M_USER_IN_USE",
  "error": "User ID already taken"
}
```

---

### 1.3 检查用户名可用性

**端点:** `GET /_matrix/client/r0/register/available?username=alice`

**成功响应 (200):**
```json
{
  "available": true
}
```

---

### 1.4 刷新 Token

**端点:** `POST /_matrix/client/r0/refresh`

**请求体:**
```json
{
  "refresh_token": "syr_abc123def456ghi789"
}
```

**成功响应 (200):**
```json
{
  "access_token": "syt_yoursever_newtoken123",
  "expires_in": 3600
}
```

**错误响应 (401):**
```json
{
  "errcode": "M_UNKNOWN_TOKEN",
  "error": "Invalid refresh token"
}
```

---

### 1.5 登出

**端点:** `POST /_matrix/client/r0/logout`

**请求头:** `Authorization: Bearer $ACCESS_TOKEN`

**成功响应 (200):**
```json
{}
```

**错误响应 (401):**
```json
{
  "errcode": "M_UNKNOWN_TOKEN",
  "error": "Unknown token"
}
```

---

### 1.6 获取当前用户

**端点:** `GET /_matrix/client/r0/account/whoami`

**请求头:** `Authorization: Bearer $ACCESS_TOKEN`

**成功响应 (200):**
```json
{
  "user_id": "@alice:example.com"
}
```

---

## 2. 用户管理 API

### 2.1 修改密码

**端点:** `POST /_matrix/client/r0/account/password`

**请求体:**
```json
{
  "auth": {
    "type": "m.login.password",
    "identifier": {
      "type": "m.id.user",
      "user": "alice"
    },
    "password": "oldpassword"
  },
  "new_password": "newpassword"
}
```

**成功响应 (200):**
```json
{}
```

**错误响应 (401):**
```json
{
  "errcode": "M_UNKNOWN",
  "error": "Invalid password"
}
```

---

### 2.2 注销账户

**端点:** `POST /_matrix/client/r0/account/deactivate`

**请求体:**
```json
{
  "auth": {
    "type": "m.login.password",
    "identifier": {
      "type": "m.id.user",
      "user": "alice"
    },
    "password": "secretpassword"
  },
  "erase": false
}
```

**成功响应 (200):**
```json
{
  "account_id": "@alice:example.com",
  "erased": false
}
```

---

### 2.3 获取用户资料

**端点:** `GET /_matrix/client/r0/profile/{user_id}`

**成功响应 (200):**
```json
{
  "avatar_url": "mxc://example.com/avatar",
  "displayname": "Alice"
}
```

**错误响应 (404):**
```json
{
  "errcode": "M_NOT_FOUND",
  "error": "User not found"
}
```

---

### 2.4 设置用户资料

**端点:** `PUT /_matrix/client/r0/profile/{user_id}/displayname`

**请求体:**
```json
{
  "displayname": "Alice Smith"
}
```

**成功响应 (200):**
```json
{}
```

---

### 2.5 设置头像

**端点:** `PUT /_matrix/client/r0/profile/{user_id}/avatar_url`

**请求体:**
```json
{
  "avatar_url": "mxc://example.com/newavatar"
}
```

**成功响应 (200):**
```json
{}
```

---

## 3. 房间管理 API

### 3.1 创建房间

**端点:** `POST /_matrix/client/r0/createRoom`

**请求体:**
```json
{
  "name": "My Room",
  "topic": "A room for discussion",
  "room_alias_name": "my-room",
  "visibility": "public",
  "preset": "public_chat",
  "invite": ["@bob:example.com"],
  "is_direct": false
}
```

**成功响应 (200):**
```json
{
  "room_id": "!room123:example.com",
  "room_alias": "#my-room:example.com"
}
```

**错误响应 (400):**
```json
{
  "errcode": "M_INVALID_ROOM_STATE",
  "error": "Invalid room creation parameters"
}
```

---

### 3.2 加入房间

**端点:** `POST /_matrix/client/r0/join/{room_id}`

**请求体:**
```json
{
  "server_name": "example.com"
}
```

**成功响应 (200):**
```json
{
  "room_id": "!room123:example.com"
}
```

**错误响应 (403):**
```json
{
  "errcode": "M_FORBIDDEN",
  "error": "You are not invited to this room"
}
```

**错误响应 (404):**
```json
{
  "errcode": "M_NOT_FOUND",
  "error": "Room not found"
}
```

---

### 3.3 离开房间

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/leave`

**成功响应 (200):**
```json
{}
```

---

### 3.4 邀请用户

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/invite`

**请求体:**
```json
{
  "id_server": "example.com",
  "medium": "email",
  "address": "bob@example.com"
}
```

或使用用户 ID:
```json
{
  "user_id": "@bob:example.com"
}
```

**成功响应 (200):**
```json
{}
```

---

### 3.5 踢出用户

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/kick`

**请求体:**
```json
{
  "user_id": "@bob:example.com",
  "reason": "Behavior violation"
}
```

**成功响应 (200):**
```json
{}
```

---

### 3.6 封禁用户

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/ban`

**请求体:**
```json
{
  "user_id": "@bob:example.com",
  "reason": "Spam"
}
```

**成功响应 (200):**
```json
{}
```

---

### 3.7 解封用户

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/unban`

**请求体:**
```json
{
  "user_id": "@bob:example.com"
}
```

**成功响应 (200):**
```json
{}
```

---

### 3.8 获取房间成员

**端点:** `GET /_matrix/client/r0/rooms/{room_id}/members`

**查询参数:**
- `at` - 事件 ID (可选)
- `membership` - 成员类型 (join, leave, invite)

**成功响应 (200):**
```json
{
  "chunk": [
    {
      "sender": "@alice:example.com",
      "type": "m.room.member",
      "state_key": "@alice:example.com",
      "content": {
        "membership": "join",
        "displayname": "Alice"
      },
      "origin_server_ts": 1234567890,
      "event_id": "$event123"
    }
  ]
}
```

---

### 3.9 获取房间状态

**端点:** `GET /_matrix/client/r0/rooms/{room_id}/state`

**成功响应 (200):**
```json
{
  "name": "My Room",
  "topic": "Discussion room",
  "avatar_url": "mxc://example.com/room",
  "join_rule": "public",
  "guest_access": "can_join",
  "history_visibility": "shared"
}
```

---

### 3.10 设置房间名称

**端点:** `PUT /_matrix/client/r0/rooms/{room_id}/state/m.room.name`

**请求体:**
```json
{
  "name": "New Room Name"
}
```

**成功响应 (200):**
```json
{}
```

---

### 3.11 设置房间主题

**端点:** `PUT /_matrix/client/r0/rooms/{room_id}/state/m.room.topic`

**请求体:**
```json
{
  "topic": "New topic"
}
```

**成功响应 (200):**
```json
{}
```

---

### 3.12 获取房间别名

**端点:** `GET /_matrix/client/r0/directory/room/{room_alias}`

**成功响应 (200):**
```json
{
  "room_id": "!room123:example.com",
  "servers": ["example.com"]
}
```

---

### 3.13 创建房间别名

**端点:** `PUT /_matrix/client/r0/directory/room/{room_alias}`

**请求体:**
```json
{
  "room_id": "!room123:example.com"
}
```

**成功响应 (200):**
```json
{}
```

**错误响应 (409):**
```json
{
  "errcode": "M_ROOM_IN_USE",
  "error": "Room alias already exists"
}
```

---

### 3.14 删除房间别名

**端点:** `DELETE /_matrix/client/r0/directory/room/{room_alias}`

**成功响应 (200):**
```json
{}
```

---

### 3.15 获取公开房间列表

**端点:** `GET /_matrix/client/r0/publicRooms`

**查询参数:**
- `server` - 服务器名称
- `limit` - 返回数量限制
- `since` - 分页令牌

**成功响应 (200):**
```json
{
  "chunk": [
    {
      "room_id": "!room123:example.com",
      "name": "Test Room",
      "topic": "A test room",
      "avatar_url": "mxc://example.com/room",
      "num_joined_members": 10,
      "world_readable": true,
      "guest_can_join": false
    }
  ],
  "next_batch": "nextpage"
}
```

---

## 4. 消息 API

### 4.1 发送消息

**端点:** `PUT /_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}`

**请求体:**
```json
{
  "msgtype": "m.text",
  "body": "Hello, world!"
}
```

**成功响应 (200):**
```json
{
  "event_id": "$event123:example.com"
}
```

**错误响应 (400):**
```json
{
  "errcode": "M_INVALID_PARAM",
  "error": "Invalid message format"
}
```

---

### 4.2 获取消息

**端点:** `GET /_matrix/client/r0/rooms/{room_id}/messages`

**查询参数:**
- `from` - 起始令牌 (必填)
- `dir` - 方向 (b 或 f)
- `limit` - 消息数量限制
- `filter` - 过滤器 JSON

**成功响应 (200):**
```json
{
  "start": "s0",
  "end": "s1",
  "chunk": [
    {
      "event_id": "$event123:example.com",
      "type": "m.room.message",
      "sender": "@alice:example.com",
      "content": {
        "msgtype": "m.text",
        "body": "Hello!"
      },
      "origin_server_ts": 1234567890
    }
  ]
}
```

---

### 4.3 撤回消息

**端点:** `PUT /_matrix/client/r0/rooms/{room_id}/redact/{event_id}/{txn_id}`

**请求体:**
```json
{
  "reason": "Spam"
}
```

**成功响应 (200):**
```json
{
  "event_id": "$redacted123:example.com"
}
```

---

### 4.4 编辑消息

**端点:** `PUT /_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}`

**请求体:**
```json
{
  "msgtype": "m.text",
  "body": "Updated message",
  "m.new_content": {
    "msgtype": "m.text",
    "body": "Updated message"
  },
  "m.relates_to": {
    "rel_type": "m.replace",
    "event_id": "$original123"
  }
}
```

**成功响应 (200):**
```json
{
  "event_id": "$new123:example.com"
}
```

---

### 4.5 发送状态事件

**端点:** `PUT /_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}`

**请求体:** (根据事件类型不同而变化)

例如, m.room.member:
```json
{
  "membership": "join",
  "displayname": "Alice"
}
```

**成功响应 (200):**
```json
{
  "event_id": "$event123:example.com"
}
```

---

## 5. 设备管理 API

### 5.1 获取设备列表

**端点:** `GET /_matrix/client/r0/devices`

**成功响应 (200):**
```json
{
  "devices": [
    {
      "device_id": "ABCDEFGH",
      "display_name": "My Device",
      "last_seen_ts": 1234567890,
      "last_ip": "192.168.1.1"
    }
  ]
}
```

---

### 5.2 获取设备信息

**端点:** `GET /_matrix/client/r0/devices/{device_id}`

**成功响应 (200):**
```json
{
  "device_id": "ABCDEFGH",
  "display_name": "My Device",
  "last_seen_ts": 1234567890,
  "last_ip": "192.168.1.1"
}
```

---

### 5.3 更新设备

**端点:** `PUT /_matrix/client/r0/devices/{device_id}`

**请求体:**
```json
{
  "display_name": "New Device Name"
}
```

**成功响应 (200):**
```json
{}
```

---

### 5.4 删除设备

**端点:** `DELETE /_matrix/client/r0/devices/{device_id}`

**请求头:** `Authorization: Bearer $ACCESS_TOKEN`

**请求体:**
```json
{
  "auth": {
    "type": "m.login.password",
    "identifier": {
      "type": "m.id.user",
      "user": "alice"
    },
    "password": "secretpassword"
  }
}
```

**成功响应 (200):**
```json
{}
```

---

## 6. E2EE 加密 API

### 6.1 上传设备密钥

**端点:** `POST /_matrix/client/r0/keys/upload`

**请求体:**
```json
{
  "device_keys": {
    "user_id": "@alice:example.com",
    "device_id": "ABCDEFGH",
    "keys": {
      "curve25519:ABCDEFGH": "device_key_base64"
    },
    "signatures": {
      "@alice:example.com": {
        "ed25519:ABCDEFGH": "signature_base64"
      }
    }
  },
  "one_time_keys": {
    "curve25519:AAAAAQ": "key_base64"
  }
}
```

**成功响应 (200):**
```json
{
  "one_time_key_counts": {
    "curve25519": 49,
    "signed_curve25519": 49
  }
}
```

---

### 6.2 查询设备密钥

**端点:** `POST /_matrix/client/r0/keys/query`

**请求体:**
```json
{
  "device_ids": ["ABCDEFGH", "IJKLMNOP"]
}
```

或:
```json
{
  "users": {
    "@alice:example.com": ["ABCDEFGH"]
  }
}
```

**成功响应 (200):**
```json
{
  "device_keys": {
    "@alice:example.com": {
      "ABCDEFGH": {
        "user_id": "@alice:example.com",
        "device_id": "ABCDEFGH",
        "keys": {
          "curve25519:ABCDEFGH": "key_base64"
        }
      }
    }
  }
}
```

---

### 6.3 申领一次性密钥

**端点:** `POST /_matrix/client/r0/keys/claim`

**请求体:**
```json
{
  "users": {
    "@alice:example.com": ["ABCDEFGH"]
  }
}
```

**成功响应 (200):**
```json
{
  "one_time_keys": {
    "@alice:example.com": {
      "ABCDEFGH": {
        "curve25519:AAAAAQ": "key_base64"
      }
    }
  }
}
```

---

### 6.4 发送到设备消息

**端点:** `PUT /_matrix/client/r0/sendToDevice/{event_type}/{txn_id}`

**请求体:**
```json
{
  "messages": {
    "@bob:example.com": {
      "ABCDEFGH": {
        "type": "m.key.verification.request",
        "content": {}
      }
    }
  }
}
```

**成功响应 (200):**
```json
{}
```

---

## 7. 媒体 API

### 7.1 上传媒体

**端点:** `POST /_matrix/media/r0/upload`

**请求头:**
- `Content-Type`: image/png, audio/ogg 等
- `Content-Length`: 文件大小
- `Authorization`: Bearer token
- `X-Content-Type-Users`: 可选

**请求体:** (二进制文件)

**成功响应 (200):**
```json
{
  "content_uri": "mxc://example.com/randomstring"
}
```

**错误响应 (413):**
```json
{
  "errcode": "M_TOO_LARGE",
  "error": "File is too large"
}
```

---

### 7.2 下载媒体

**端点:** `GET /_matrix/media/r0/download/{server_name}/{media_id}`

**查询参数:**
- `filename` - 下载时使用的文件名

**成功响应 (200):** (二进制文件)

---

### 7.3 获取缩略图

**端点:** `GET /_matrix/media/r0/thumbnail/{server_name}/{media_id}`

**查询参数:**
- `width` - 目标宽度
- `height` - 目标高度
- `method` - 裁剪方式 (crop, scale)

**成功响应 (200):** (二进制图片)

---

### 7.4 URL 预览

**端点:** `GET /_matrix/media/r0/preview_url`

**查询参数:**
- `url` - 要预览的 URL (必填)
- `ts` - 时间戳 (可选)

**请求头:** `Authorization: Bearer $ACCESS_TOKEN`

**成功响应 (200):**
```json
{
  "og:title": "Example Page",
  "og:description": "Page description",
  "og:image": {
    "url": "mxc://example.com/image"
  }
}
```

---

## 8. 好友系统 API

### 8.1 获取好友列表

**端点:** `GET /_matrix/client/v1/friends`

**成功响应 (200):**
```json
{
  "friends": [
    {
      "user_id": "@bob:example.com",
      "displayname": "Bob",
      "avatar_url": "mxc://example.com/avatar"
    }
  ]
}
```

---

### 8.2 添加好友

**端点:** `POST /_matrix/client/v1/friends/{user_id}`

**成功响应 (200):**
```json
{}
```

---

### 8.3 删除好友

**端点:** `DELETE /_matrix/client/v1/friends/{user_id}`

**成功响应 (200):**
```json
{}
```

---

### 8.4 接受好友请求

**端点:** `POST /_matrix/client/v1/friends/{user_id}/accept`

**成功响应 (200):**
```json
{}
```

---

### 8.5 拉黑用户

**端点:** `POST /_matrix/client/v1/friends/{user_id}/block`

**成功响应 (200):**
```json
{}
```

---

### 8.6 解除拉黑

**端点:** `DELETE /_matrix/client/v1/friends/{user_id}/block`

**成功响应 (200):**
```json
{}
```

---

## 9. Space API

### 9.1 创建空间

**端点:** `POST /_matrix/client/v1/spaces`

**请求体:**
```json
{
  "name": "My Space",
  "topic": "Space description",
  "room_alias_name": "my-space",
  "visibility": "public"
}
```

**成功响应 (200):**
```json
{
  "room_id": "!space123:example.com"
}
```

---

### 9.2 获取公开空间

**端点:** `GET /_matrix/client/v1/spaces/public`

**查询参数:**
- `server` - 服务器名称
- `limit` - 返回数量

**成功响应 (200):**
```json
{
  "chunk": [
    {
      "room_id": "!space123:example.com",
      "name": "My Space",
      "topic": "Space description"
    }
  ]
}
```

---

### 9.3 获取空间层级

**端点:** `GET /_matrix/client/v1/spaces/{space_id}/hierarchy`

**查询参数:**
- `max_depth` - 最大深度

**成功响应 (200):**
```json
{
  "children": [
    {
      "room_id": "!room123:example.com",
      "name": "Child Room",
      "children": []
    }
  ],
  "room": {
    "name": "My Space"
  }
}
```

---

## 10. Thread API

### 10.1 获取线程

**端点:** `GET /_matrix/client/v1/threads/{thread_id}`

**成功响应 (200):**
```json
{
  "thread_id": "!thread123:example.com",
  "room_id": "!room456:example.com",
  "latest_event": {
    "event_id": "$event123",
    "content": {
      "body": "Thread message"
    }
  },
  "number_of_replies": 5
}
```

---

### 10.2 创建线程

**端点:** `POST /_matrix/client/v1/threads`

**请求体:**
```json
{
  "room_id": "!room123:example.com",
  "event_id": "$event456:example.com"
}
```

**成功响应 (200):**
```json
{
  "thread_id": "!thread123:example.com"
}
```

---

### 10.3 回复线程

**端点:** `POST /_matrix/client/v1/threads/{thread_id}/reply`

**请求体:**
```json
{
  "msgtype": "m.text",
  "body": "Reply to thread"
}
```

**成功响应 (200):**
```json
{
  "event_id": "$reply123:example.com"
}
```

---

### 10.4 置顶线程

**端点:** `POST /_matrix/client/v1/threads/{thread_id}/pin`

**成功响应 (200):**
```json
{}
```

---

## 11. 搜索 API

### 11.1 搜索消息

**端点:** `POST /_matrix/client/r0/search`

**请求体:**
```json
{
  "search_categories": {
    "room_events": {
      "keys_to_include": ["content.body"],
      "search_term": "hello",
      "order_by": "recent",
      "limit": 10
    }
  }
}
```

**成功响应 (200):**
```json
{
  "search_categories": {
    "room_events": {
      "results": [
        {
          "event_id": "$event123:example.com",
          "room_id": "!room123:example.com",
          "result_context": {}
        }
      ],
      "count": 1
    }
  }
}
```

---

### 11.2 搜索用户

**端点:** `POST /_matrix/client/r0/user_directory/search`

**请求体:**
```json
{
  "search_term": "alice",
  "limit": 10
}
```

**成功响应 (200):**
```json
{
  "results": [
    {
      "user_id": "@alice:example.com",
      "display_name": "Alice",
      "avatar_url": "mxc://example.com/avatar"
    }
  ]
}
```

---

## 12. 推送 API

### 12.1 设置推送器

**端点:** `POST /_matrix/client/r0/pushers/set`

**请求体:**
```json
{
  "pushkit": {
    "url": "https://pushkit.example.com/push"
  },
  "data": {
    "url": "https://push.example.com/notify"
  },
  "device_info": {
    "app_id": "com.example.app",
    "device_id": "ABCDEFGH",
    "pushkey": "pushkey123",
    "pushkey_ts": 1234567890
  },
  "kind": "http",
  "app_id": "com.example.app"
}
```

**成功响应 (200):**
```json
{}
```

---

### 12.2 获取推送规则

**端点:** `GET /_matrix/client/r0/pushrules`

**成功响应 (200):**
```json
{
  "global": {
    "override": [],
    "room": [],
    "sender": [],
    "underride": []
  }
}
```

---

### 12.3 获取通知

**端点:** `GET /_matrix/client/r0/notifications`

**查询参数:**
- `from` - 分页令牌
- `limit` - 返回数量
- `only` - 过滤类型 (highlight, nohighlight)

**成功响应 (200):**
```json
{
  "notifications": [
    {
      "room_id": "!room123:example.com",
      "event_id": "$event123",
      "sender": "@bob:example.com",
      "type": "m.room.message",
      "content": {
        "body": "Hello"
      },
      "highlight": true,
      "unread": true
    }
  ]
}
```

---

## 13. Widget API

### 13.1 获取 Widget 列表

**端点:** `GET /_matrix/client/v3/widgets`

**成功响应 (200):**
```json
{
  "widgets": [
    {
      "id": "widget123",
      "type": "m.custom",
      "url": "https://widget.example.com/",
      "name": "My Widget"
    }
  ]
}
```

---

### 13.2 创建 Widget

**端点:** `POST /_matrix/client/v3/widgets`

**请求体:**
```json
{
  "type": "m.custom",
  "url": "https://widget.example.com/",
  "name": "My Widget",
  "data": {}
}
```

**成功响应 (200):**
```json
{
  "widget_id": "widget123"
}
```

---

### 13.3 获取房间 Widget

**端点:** `GET /_matrix/client/v3/rooms/{room_id}/widgets`

**成功响应 (200):**
```json
{
  "widgets": [
    {
      "id": "widget123",
      "type": "m.custom",
      "url": "https://widget.example.com/"
    }
  ]
}
```

---

## 14. Sliding Sync API

### 14.1 Sliding Sync

**端点:** `GET /_matrix/client/unstable/org.matrix.msc3575/sync`

**查询参数:**
- `pos` - 位置令牌
- `timeout` - 超时时间 (毫秒)

**请求体:**
```json
{
  "lists": [
    {
      "name": "main",
      "ranged": true,
      "range": [0, 49],
      "required_state": [
        ["m.room.create", ""],
        ["m.room.name", ""],
        ["m.room.avatar", ""]
      ],
      "timeline_limit": 10
    }
  ],
  "rooms": []
}
```

**成功响应 (200):**
```json
{
  "pos": "newpos123",
  "lists": [
    {
      "name": "main",
      "ops": [
        {
          "op": "SYNC",
          "range": [0, 49],
          "room_id": "!room123:example.com"
        }
      ]
    }
  ],
  "rooms": {
    "!room123:example.com": {
      "timeline": {
        "events": []
      },
      "state": {
        "events": []
      }
    }
  }
}
```

---

## 15. 管理后台 API

### 15.1 获取服务器版本

**端点:** `GET /_synapse/admin/v1/server_version`

**成功响应 (200):**
```json
{
  "server_version": "1.0.0",
  "name": "synapse-rust"
}
```

---

### 15.2 获取用户列表

**端点:** `GET /_synapse/admin/v1/users`

**查询参数:**
- `from` - 分页起始
- `limit` - 返回数量
- `name` - 用户名过滤

**成功响应 (200):**
```json
{
  "users": [
    {
      "name": "@alice:example.com",
      "admin": false,
      "deactivated": false,
      "displayname": "Alice"
    }
  ],
  "total": 100
}
```

---

### 15.3 创建用户

**端点:** `POST /_synapse/admin/v1/users`

**请求体:**
```json
{
  "auth": {
    "type": "m.login.dummy"
  },
  "username": "newuser",
  "password": "password123",
  "admin": false
}
```

**成功响应 (200):**
```json
{
  "name": "@newuser:example.com"
}
```

---

### 15.4 获取房间列表

**端点:** `GET /_synapse/admin/v1/rooms`

**查询参数:**
- `from` - 分页起始
- `limit` - 返回数量

**成功响应 (200):**
```json
{
  "rooms": [
    {
      "room_id": "!room123:example.com",
      "name": "Test Room",
      "creator": "@alice:example.com",
      "joined_members": 10
    }
  ],
  "total": 50
}
```

---

### 15.5 获取房间详情

**端点:** `GET /_synapse/admin/v1/rooms/{room_id}`

**成功响应 (200):**
```json
{
  "room_id": "!room123:example.com",
  "name": "Test Room",
  "topic": "Room topic",
  "creator": "@alice:example.com",
  "joined_members": 10,
  "joined_local_members": 5,
  "version": "9"
}
```

---

### 15.6 删除房间

**端点:** `DELETE /_synapse/admin/v1/rooms/{room_id}`

**查询参数:**
- `purge` - 是否立即删除

**成功响应 (200):**
```json
{}
```

---

### 15.7 获取注册令牌列表

**端点:** `GET /_synapse/admin/v1/registration_tokens`

**成功响应 (200):**
```json
{
  "registration_tokens": [
    {
      "token": "abc123",
      "valid": true,
      "uses_allowed": 100,
      "uses": 50,
      "expiry_time": null
    }
  ]
}
```

