# 完整 API 文档

> **版本**：1.0.0  
> **创建日期**：2026-01-28  
> **项目状态**：开发中  
> **参考文档**：[Synapse 官方文档](https://element-hq.github.io/synapse/latest/)、[Matrix 规范](https://spec.matrix.org/)

---

## 一、客户端 API (Client API)

### 1.1 获取支持的 API 版本

**接口名称**：获取支持的 API 版本  
**请求方法**：GET  
**URL 路径**：`/_matrix/client/versions`  
**认证**：否

#### 请求参数

无

#### 响应格式

```json
{
  "versions": ["r0", "v1", "v3"],
  "unstable_features": {
    "org.matrix.e2e_cross_signing": true
  }
}
```

#### 错误码

无

#### 使用示例

```bash
curl -X GET http://localhost:8008/_matrix/client/versions
```

---

### 1.2 用户注册

**接口名称**：用户注册  
**请求方法**：POST  
**URL 路径**：`/_matrix/client/r0/register`  
**认证**：否

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| username | string | 是 | 用户名 |
| password | string | 是 | 密码 |
| auth | object | 否 | 认证信息（如果需要） |
| device_id | string | 否 | 设备 ID |
| initial_device_display_name | string | 否 | 设备显示名称 |

#### 请求示例

```json
{
  "username": "alice",
  "password": "secure_password",
  "device_id": "DEVICE123",
  "initial_device_display_name": "My Phone"
}
```

#### 响应格式

```json
{
  "user_id": "@alice:server.com",
  "access_token": "access_token_here",
  "device_id": "DEVICE123",
  "home_server": "server.com"
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_USER_IN_USE | 400 | 用户名已被使用 |
| M_INVALID_USERNAME | 400 | 用户名无效 |
| M_BAD_JSON | 400 | JSON 格式错误 |

#### 使用示例

```bash
curl -X POST http://localhost:8008/_matrix/client/r0/register \
  -H "Content-Type: application/json" \
  -d '{
    "username": "alice",
    "password": "secure_password"
  }'
```

---

### 1.3 用户登录

**接口名称**：用户登录  
**请求方法**：POST  
**URL 路径**：`/_matrix/client/r0/login`  
**认证**：否

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| type | string | 是 | 登录类型（m.login.password） |
| user | string | 是 | 用户名 |
| password | string | 是 | 密码 |
| device_id | string | 否 | 设备 ID |
| initial_device_display_name | string | 否 | 设备显示名称 |

#### 请求示例

```json
{
  "type": "m.login.password",
  "user": "alice",
  "password": "secure_password",
  "device_id": "DEVICE123",
  "initial_device_display_name": "My Phone"
}
```

#### 响应格式

```json
{
  "user_id": "@alice:server.com",
  "access_token": "access_token_here",
  "device_id": "DEVICE123",
  "home_server": "server.com",
  "well_known": {
    "m.homeserver": {
      "base_url": "https://server.com"
    }
  }
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_FORBIDDEN | 403 | 用户名或密码错误 |
| M_BAD_JSON | 400 | JSON 格式错误 |
| M_UNKNOWN | 500 | 未知错误 |

#### 使用示例

```bash
curl -X POST http://localhost:8008/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{
    "type": "m.login.password",
    "user": "alice",
    "password": "secure_password"
  }'
```

---

### 1.4 用户登出

**接口名称**：用户登出  
**请求方法**：POST  
**URL 路径**：`/_matrix/client/r0/logout`  
**认证**：是

#### 请求参数

无

#### 响应格式

```json
{}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_MISSING_TOKEN | 401 | 缺少访问令牌 |

#### 使用示例

```bash
curl -X POST http://localhost:8008/_matrix/client/r0/logout \
  -H "Authorization: Bearer access_token_here"
```

---

### 1.5 登出所有设备

**接口名称**：登出所有设备  
**请求方法**：POST  
**URL 路径**：`/_matrix/client/r0/logout/all`  
**认证**：是

#### 请求参数

无

#### 响应格式

```json
{}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_MISSING_TOKEN | 401 | 缺少访问令牌 |

#### 使用示例

```bash
curl -X POST http://localhost:8008/_matrix/client/r0/logout/all \
  -H "Authorization: Bearer access_token_here"
```

---

### 1.6 同步事件

**接口名称**：同步事件  
**请求方法**：GET  
**URL 路径**：`/_matrix/client/r0/sync`  
**认证**：是

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| filter | string | 否 | 过滤器 ID 或 JSON |
| since | string | 否 | 从哪个事件开始同步 |
| set_presence | string | 否 | 设置在线状态 |
| timeout | integer | 否 | 超时时间（毫秒） |
| full_state | boolean | 否 | 是否返回完整状态 |

#### 响应格式

```json
{
  "next_batch": "s1234567890",
  "rooms": {
    "join": {
      "!room_id:server.com": {
        "timeline": {
          "events": [],
          "limited": false,
          "prev_batch": "s1234567890"
        },
        "state": {
          "events": []
        },
        "ephemeral": {
          "events": []
        },
        "account_data": {
          "events": []
        }
      }
    }
  },
  "presence": {
    "events": []
  },
  "account_data": {
    "events": []
  },
  "to_device": {
    "events": []
  },
  "device_lists": {
    "changed": [],
    "left": []
  },
  "device_one_time_keys_count": {}
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_MISSING_TOKEN | 401 | 缺少访问令牌 |

#### 使用示例

```bash
curl -X GET "http://localhost:8008/_matrix/client/r0/sync?timeout=30000" \
  -H "Authorization: Bearer access_token_here"
```

---

### 1.7 创建房间

**接口名称**：创建房间  
**请求方法**：POST  
**URL 路径**：`/_matrix/client/r0/createRoom`  
**认证**：是

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| preset | string | 否 | 房间预设（private_chat, public_chat, trusted_private_chat） |
| visibility | string | 否 | 可见性（public, private） |
| name | string | 否 | 房间名称 |
| topic | string | 否 | 房间主题 |
| invite | array | 否 | 邀请的用户 ID 列表 |
| room_alias_name | string | 否 | 房间别名 |
| creation_content | object | 否 | 创建内容 |

#### 请求示例

```json
{
  "preset": "private_chat",
  "visibility": "private",
  "name": "My Room",
  "topic": "Room topic",
  "invite": ["@bob:server.com"]
}
```

#### 响应格式

```json
{
  "room_id": "!room_id:server.com"
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_BAD_JSON | 400 | JSON 格式错误 |
| M_ROOM_ALIAS_IN_USE | 400 | 房间别名已被使用 |

#### 使用示例

```bash
curl -X POST http://localhost:8008/_matrix/client/r0/createRoom \
  -H "Authorization: Bearer access_token_here" \
  -H "Content-Type: application/json" \
  -d '{
    "preset": "private_chat",
    "visibility": "private",
    "name": "My Room"
  }'
```

---

### 1.8 加入房间

**接口名称**：加入房间  
**请求方法**：POST  
**URL 路径**：`/_matrix/client/r0/rooms/{room_id}/join`  
**认证**：是

#### 路径参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| reason | string | 否 | 加入原因 |
| third_party_signed | object | 否 | 第三方签名 |

#### 响应格式

```json
{
  "room_id": "!room_id:server.com"
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_FORBIDDEN | 403 | 禁止加入房间 |
| M_NOT_FOUND | 404 | 房间不存在 |

#### 使用示例

```bash
curl -X POST http://localhost:8008/_matrix/client/r0/rooms/!room_id:server.com/join \
  -H "Authorization: Bearer access_token_here"
```

---

### 1.9 离开房间

**接口名称**：离开房间  
**请求方法**：POST  
**URL 路径**：`/_matrix/client/r0/rooms/{room_id}/leave`  
**认证**：是

#### 路径参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| reason | string | 否 | 离开原因 |

#### 响应格式

```json
{}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_NOT_FOUND | 404 | 房间不存在 |

#### 使用示例

```bash
curl -X POST http://localhost:8008/_matrix/client/r0/rooms/!room_id:server.com/leave \
  -H "Authorization: Bearer access_token_here"
```

---

### 1.10 发送房间消息

**接口名称**：发送房间消息  
**请求方法**：PUT  
**URL 路径**：`/_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}`  
**认证**：是

#### 路径参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |
| event_type | string | 是 | 事件类型（m.room.message） |
| txn_id | string | 是 | 事务 ID |

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| msgtype | string | 是 | 消息类型（m.text, m.image, m.audio 等） |
| body | string | 是 | 消息内容 |
| formatted_body | string | 否 | 格式化消息内容 |
| format | string | 否 | 格式类型（org.matrix.custom.html） |

#### 请求示例

```json
{
  "msgtype": "m.text",
  "body": "Hello, world!"
}
```

#### 响应格式

```json
{
  "event_id": "$event_id:server.com"
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_FORBIDDEN | 403 | 禁止发送消息 |
| M_NOT_FOUND | 404 | 房间不存在 |

#### 使用示例

```bash
curl -X PUT http://localhost:8008/_matrix/client/r0/rooms/!room_id:server.com/send/m.room.message/txn123 \
  -H "Authorization: Bearer access_token_here" \
  -H "Content-Type: application/json" \
  -d '{
    "msgtype": "m.text",
    "body": "Hello, world!"
  }'
```

---

### 1.11 获取房间消息

**接口名称**：获取房间消息  
**请求方法**：GET  
**URL 路径**：`/_matrix/client/r0/rooms/{room_id}/messages`  
**认证**：是

#### 路径参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| from | string | 否 | 从哪个事件开始 |
| to | string | 否 | 到哪个事件结束 |
| dir | string | 否 | 方向（f, b） |
| limit | integer | 否 | 限制数量 |
| filter | string | 否 | 过滤器 |

#### 响应格式

```json
{
  "start": "s1234567890",
  "end": "s1234567891",
  "chunk": [
    {
      "event_id": "$event_id:server.com",
      "type": "m.room.message",
      "sender": "@alice:server.com",
      "content": {
        "msgtype": "m.text",
        "body": "Hello, world!"
      },
      "origin_server_ts": 1234567890
    }
  ],
  "state": []
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_NOT_FOUND | 404 | 房间不存在 |

#### 使用示例

```bash
curl -X GET "http://localhost:8008/_matrix/client/r0/rooms/!room_id:server.com/messages?dir=b&limit=50" \
  -H "Authorization: Bearer access_token_here"
```

---

## 二、联邦 API (Federation API)

### 2.1 获取服务器版本

**接口名称**：获取服务器版本  
**请求方法**：GET  
**URL 路径**：`/_matrix/federation/v1/version`  
**认证**：是（服务器签名）

#### 请求参数

无

#### 响应格式

```json
{
  "server": {
    "name": "Synapse Rust",
    "version": "0.1.0"
  }
}
```

#### 错误码

无

#### 使用示例

```bash
curl -X GET http://localhost:8008/_matrix/federation/v1/version
```

---

### 2.2 发送联邦事务

**接口名称**：发送联邦事务  
**请求方法**：PUT  
**URL 路径**：`/_matrix/federation/v1/send/{txn_id}`  
**认证**：是（服务器签名）

#### 路径参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| txn_id | string | 是 | 事务 ID |

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| origin | string | 是 | 源服务器 |
| origin_server_ts | integer | 是 | 源服务器时间戳 |
| pdus | array | 是 | PDU 列表 |
| edus | array | 否 | EDU 列表 |

#### 响应格式

```json
{
  "pdus": {
    "$event_id:server.com": {
      "event_id": "$event_id:server.com"
    }
  }
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_FORBIDDEN | 403 | 禁止访问 |
| M_BAD_JSON | 400 | JSON 格式错误 |

#### 使用示例

```bash
curl -X PUT http://localhost:8008/_matrix/federation/v1/send/txn123 \
  -H "Authorization: X-Matrix origin=server.com,key=...,sig=..." \
  -H "Content-Type: application/json" \
  -d '{
    "origin": "server.com",
    "origin_server_ts": 1234567890,
    "pdus": []
  }'
```

---

## 三、Enhanced API

### 3.1 好友管理 API

#### 3.1.1 获取好友列表

**接口名称**：获取好友列表  
**请求方法**：GET  
**URL 路径**：`/_synapse/enhanced/friends`  
**认证**：是

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| category | string | 否 | 分类名称 |
| limit | integer | 否 | 限制数量 |
| offset | integer | 否 | 偏移量 |

#### 响应格式

```json
{
  "friends": [
    {
      "user_id": "@bob:server.com",
      "display_name": "Bob",
      "avatar_url": "mxc://server.com/...",
      "category": "Family",
      "added_at": 1234567890
    }
  ],
  "total": 1
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |

#### 使用示例

```bash
curl -X GET "http://localhost:8008/_synapse/enhanced/friends?category=Family" \
  -H "Authorization: Bearer access_token_here"
```

---

#### 3.1.2 发送好友请求

**接口名称**：发送好友请求  
**请求方法**：POST  
**URL 路径**：`/_synapse/enhanced/friend/request`  
**认证**：是

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| user_id | string | 是 | 目标用户 ID |
| message | string | 否 | 请求消息 |

#### 请求示例

```json
{
  "user_id": "@bob:server.com",
  "message": "Hi, I'd like to be your friend!"
}
```

#### 响应格式

```json
{
  "request_id": "request_id_here"
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_NOT_FOUND | 404 | 用户不存在 |
| M_ALREADY_FRIENDS | 400 | 已经是好友 |

#### 使用示例

```bash
curl -X POST http://localhost:8008/_synapse/enhanced/friend/request \
  -H "Authorization: Bearer access_token_here" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@bob:server.com",
    "message": "Hi, I'\''d like to be your friend!"
  }'
```

---

#### 3.1.3 响应好友请求

**接口名称**：响应好友请求  
**请求方法**：POST  
**URL 路径**：`/_synapse/enhanced/friend/request/{request_id}/respond`  
**认证**：是

#### 路径参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| request_id | string | 是 | 请求 ID |

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| accept | boolean | 是 | 是否接受 |
| category | string | 否 | 分类名称 |

#### 请求示例

```json
{
  "accept": true,
  "category": "Family"
}
```

#### 响应格式

```json
{}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_NOT_FOUND | 404 | 请求不存在 |

#### 使用示例

```bash
curl -X POST http://localhost:8008/_synapse/enhanced/friend/request/request123/respond \
  -H "Authorization: Bearer access_token_here" \
  -H "Content-Type: application/json" \
  -d '{
    "accept": true,
    "category": "Family"
  }'
```

---

### 3.2 私聊管理 API

#### 3.2.1 创建私聊会话

**接口名称**：创建私聊会话  
**请求方法**：POST  
**URL 路径**：`/_synapse/enhanced/private/sessions`  
**认证**：是

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| user_id | string | 是 | 目标用户 ID |
| session_name | string | 否 | 会话名称 |
| ttl_seconds | integer | 否 | TTL（秒） |
| auto_delete | boolean | 否 | 自动删除 |

#### 请求示例

```json
{
  "user_id": "@bob:server.com",
  "session_name": "Private Chat",
  "ttl_seconds": 86400,
  "auto_delete": false
}
```

#### 响应格式

```json
{
  "session_id": "session_id_here"
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_NOT_FOUND | 404 | 用户不存在 |

#### 使用示例

```bash
curl -X POST http://localhost:8008/_synapse/enhanced/private/sessions \
  -H "Authorization: Bearer access_token_here" \
  -H "Content-Type: application/json" \
  -d '{
    "user_id": "@bob:server.com",
    "session_name": "Private Chat"
  }'
```

---

#### 3.2.2 发送私聊消息

**接口名称**：发送私聊消息  
**请求方法**：POST  
**URL 路径**：`/_synapse/enhanced/private/sessions/{session_id}/messages`  
**认证**：是

#### 路径参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| session_id | string | 是 | 会话 ID |

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| content | string | 是 | 消息内容 |
| encrypted | boolean | 否 | 是否加密 |
| ttl_seconds | integer | 否 | TTL（秒） |

#### 请求示例

```json
{
  "content": "Hello, this is a private message!",
  "encrypted": true,
  "ttl_seconds": 86400
}
```

#### 响应格式

```json
{
  "message_id": "message_id_here"
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_NOT_FOUND | 404 | 会话不存在 |

#### 使用示例

```bash
curl -X POST http://localhost:8008/_synapse/enhanced/private/sessions/session123/messages \
  -H "Authorization: Bearer access_token_here" \
  -H "Content-Type: application/json" \
  -d '{
    "content": "Hello, this is a private message!",
    "encrypted": true
  }'
```

---

### 3.3 语音消息 API

#### 3.3.1 上传语音消息

**接口名称**：上传语音消息  
**请求方法**：POST  
**URL 路径**：`/_synapse/enhanced/voice/upload`  
**认证**：是

#### 请求参数

| 参数名 | 类型 | 必需 | 描述 |
|--------|------|------|------|
| file | file | 是 | 音频文件 |
| room_id | string | 否 | 房间 ID |
| duration | integer | 否 | 时长（秒） |
| language | string | 否 | 语言代码 |
| transcription | string | 否 | 转录文本 |

#### 响应格式

```json
{
  "message_id": "message_id_here",
  "file_url": "mxc://server.com/...",
  "duration": 30
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_BAD_JSON | 400 | JSON 格式错误 |

#### 使用示例

```bash
curl -X POST http://localhost:8008/_synapse/enhanced/voice/upload \
  -H "Authorization: Bearer access_token_here" \
  -F "file=@audio.mp3" \
  -F "duration=30"
```

---

## 四、Admin API

### 4.1 获取系统状态

**接口名称**：获取系统状态  
**请求方法**：GET  
**URL 路径**：`/_synapse/admin/v1/status`  
**认证**：是（管理员）

#### 请求参数

无

#### 响应格式

```json
{
  "version": "0.1.0",
  "uptime": 86400,
  "total_users": 1000,
  "total_rooms": 500,
  "total_events": 100000
}
```

#### 错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_FORBIDDEN | 403 | 无管理员权限 |

#### 使用示例

```bash
curl -X GET http://localhost:8008/_synapse/admin/v1/status \
  -H "Authorization: Bearer admin_token_here"
```

---

## 五、通用错误码

| 错误码 | HTTP 状态码 | 描述 |
|--------|------------|------|
| M_UNKNOWN | 500 | 未知错误 |
| M_BAD_JSON | 400 | JSON 格式错误 |
| M_NOT_JSON | 400 | 非 JSON 请求 |
| M_NOT_FOUND | 404 | 资源未找到 |
| M_LIMIT_EXCEEDED | 429 | 请求频率超限 |
| M_USER_IN_USE | 400 | 用户名已被使用 |
| M_INVALID_USERNAME | 400 | 用户名无效 |
| M_MISSING_PARAM | 400 | 缺少必需参数 |
| M_INVALID_PARAM | 400 | 参数无效 |
| M_FORBIDDEN | 403 | 禁止访问 |
| M_UNAUTHORIZED | 401 | 未授权 |
| M_UNKNOWN_TOKEN | 401 | 无效的访问令牌 |
| M_MISSING_TOKEN | 401 | 缺少访问令牌 |

---

## 六、参考资料

- [Matrix 客户端-服务器 API 规范](https://spec.matrix.org/v1.11/client-server-api/)
- [Matrix 联邦 API 规范](https://spec.matrix.org/v1.11/server-server-api/)
- [Synapse 官方文档](https://element-hq.github.io/synapse/latest/)

---

## 七、变更日志

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-01-28 | 初始版本，定义完整 API 文档 |
