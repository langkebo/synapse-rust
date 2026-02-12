# Synapse Rust Matrix Server API Reference

> **服务器地址**: `http://localhost:8008`  
> **版本**: 0.1.0  
> **文档版本**: 4.0  
> **最后更新**: 2026-02-12

---

## 目录

1. [概述](#1-概述)
2. [认证方式](#2-认证方式)
3. [核心客户端 API](#3-核心客户端-api)
4. [管理员 API](#4-管理员-api)
5. [联邦通信 API](#5-联邦通信-api)
6. [好友系统 API](#6-好友系统-api)
7. [端到端加密 API](#7-端到端加密-api)
8. [媒体文件 API](#8-媒体文件-api)
9. [语音消息 API](#9-语音消息-api)
10. [VoIP API](#10-voip-api)
11. [密钥备份 API](#11-密钥备份-api)
12. [错误码参考](#12-错误码参考)
13. [API 统计](#13-api-统计)

---

## 1. 概述

### 1.1 API 分类

| 分类 | 端点数量 | 说明 |
|------|---------|------|
| 核心客户端 API | 62 | 用户认证、房间管理、消息操作 |
| 管理员 API | 27 | 服务器管理、用户管理、房间管理 |
| 联邦通信 API | 39 | 服务器间通信 |
| 好友系统 API | 11 | 基于 Matrix 房间的好友管理 |
| 端到端加密 API | 6 | E2EE 相关功能 |
| 媒体文件 API | 8 | 媒体上传下载 |
| 语音消息 API | 10 | 语音消息处理 |
| VoIP API | 3 | VoIP 配置 |
| 密钥备份 API | 11 | 密钥备份管理 |
| **总计** | **177** | |

### 1.2 基础 URL

```
http://localhost:8008
```

### 1.3 请求头

| 请求头 | 说明 | 示例 |
|--------|------|------|
| `Authorization` | Bearer Token 认证 | `Bearer syt_abc123...` |
| `Content-Type` | 请求体格式 | `application/json` |
| `Accept` | 响应格式 | `application/json` |

---

## 2. 认证方式

### 2.1 Bearer Token 认证

大多数 API 需要在请求头中携带 Access Token：

```http
Authorization: Bearer <access_token>
```

### 2.2 获取 Token

通过登录接口获取：

```http
POST /_matrix/client/r0/login
Content-Type: application/json

{
  "type": "m.login.password",
  "user": "alice",
  "password": "password123"
}
```

**响应**:
```json
{
  "access_token": "syt_abc123...",
  "device_id": "DEVICEID",
  "user_id": "@alice:example.com",
  "expires_in": 86400000,
  "refresh_token": "abc123..."
}
```

---

## 3. 核心客户端 API

### 3.1 健康检查与版本

#### 3.1.1 服务器欢迎信息

| 属性 | 值 |
|------|-----|
| **端点** | `/` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "name": "Synapse Rust",
  "version": "0.1.0"
}
```

#### 3.1.2 健康检查

| 属性 | 值 |
|------|-----|
| **端点** | `/health` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "status": "ok",
  "database": "connected",
  "cache": "connected"
}
```

#### 3.1.3 获取客户端 API 版本

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/versions` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "versions": ["r0.5.0", "r0.6.0", "v1.0", "v1.1", "v1.2"],
  "unstable_features": {
    "org.matrix.label_based_auth": true
  }
}
```

#### 3.1.4 获取服务端版本

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/version` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "server": {
    "name": "Synapse Rust",
    "version": "0.1.0"
  }
}
```

---

### 3.2 用户注册与认证

#### 3.2.1 检查用户名可用性

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/register/available` |
| **方法** | `GET` |
| **认证** | 不需要 |

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `username` | string | 是 | 要检查的用户名 |

**响应示例**:
```json
{
  "available": true
}
```

**状态码**:

| 状态码 | 说明 |
|--------|------|
| 200 | 检查成功 |
| 400 | 用户名格式无效 |
| 429 | 请求过于频繁 |

#### 3.2.2 请求邮箱验证

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/register/email/requestToken` |
| **方法** | `POST` |
| **认证** | 不需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `email` | string | 是 | 邮箱地址 |
| `client_secret` | string | 是 | 客户端密钥 |

**请求示例**:
```json
{
  "email": "user@example.com",
  "client_secret": "abc123"
}
```

**响应示例**:
```json
{
  "sid": "session_id_123"
}
```

#### 3.2.3 用户注册

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/register` |
| **方法** | `POST` |
| **认证** | 不需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `username` | string | 是 | 用户名 |
| `password` | string | 是 | 密码 |
| `device_id` | string | 否 | 设备ID |
| `initial_device_display_name` | string | 否 | 设备显示名称 |
| `inhibit_login` | boolean | 否 | 是否禁止自动登录 |

**请求示例**:
```json
{
  "username": "alice",
  "password": "password123",
  "device_id": "DEVICEID",
  "initial_device_display_name": "My Device"
}
```

**响应示例**:
```json
{
  "user_id": "@alice:example.com",
  "access_token": "syt_abc123...",
  "device_id": "DEVICEID",
  "expires_in": 86400000,
  "refresh_token": "abc123..."
}
```

**状态码**:

| 状态码 | 说明 |
|--------|------|
| 200 | 注册成功 |
| 400 | 请求参数无效 |
| 403 | 注册被禁止 |
| 409 | 用户名已存在 |
| 429 | 请求过于频繁 |

#### 3.2.4 用户登录

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/login` |
| **方法** | `POST` |
| **认证** | 不需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `type` | string | 是 | 登录类型，如 `m.login.password` |
| `user` | string | 是 | 用户名或完整用户ID |
| `password` | string | 是 | 密码 |
| `device_id` | string | 否 | 设备ID |
| `initial_device_display_name` | string | 否 | 设备显示名称 |

**请求示例**:
```json
{
  "type": "m.login.password",
  "user": "alice",
  "password": "password123",
  "device_id": "DEVICEID"
}
```

**响应示例**:
```json
{
  "user_id": "@alice:example.com",
  "access_token": "syt_abc123...",
  "device_id": "DEVICEID",
  "expires_in": 86400000,
  "refresh_token": "abc123..."
}
```

#### 3.2.5 退出登录

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/logout` |
| **方法** | `POST` |
| **认证** | 需要 |

**响应示例**:
```json
{}
```

#### 3.2.6 退出所有设备

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/logout/all` |
| **方法** | `POST` |
| **认证** | 需要 |

**响应示例**:
```json
{}
```

#### 3.2.7 刷新令牌

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/refresh` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `refresh_token` | string | 是 | 刷新令牌 |

**响应示例**:
```json
{
  "access_token": "syt_new_token...",
  "expires_in": 86400000,
  "refresh_token": "new_refresh_token..."
}
```

---

### 3.3 账户管理

#### 3.3.1 获取当前用户信息

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/account/whoami` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "user_id": "@alice:example.com",
  "device_id": "DEVICEID"
}
```

#### 3.3.2 停用账户

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/account/deactivate` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `erase` | boolean | 否 | 是否删除所有数据 |

**响应示例**:
```json
{
  "id_server_unbind_result": "success"
}
```

#### 3.3.3 修改密码

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/account/password` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `new_password` | string | 是 | 新密码 |
| `logout_devices` | boolean | 否 | 是否登出其他设备 |

**响应示例**:
```json
{}
```

#### 3.3.4 获取用户资料

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/profile/{user_id}` |
| **方法** | `GET` |
| **认证** | 不需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `user_id` | string | 是 | 用户ID |

**响应示例**:
```json
{
  "displayname": "Alice",
  "avatar_url": "mxc://example.com/avatar"
}
```

#### 3.3.5 更新显示名称

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/profile/{user_id}/displayname` |
| **方法** | `PUT` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `displayname` | string | 是 | 新的显示名称 |

**请求示例**:
```json
{
  "displayname": "Alice Smith"
}
```

**响应示例**:
```json
{}
```

#### 3.3.6 更新头像

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/profile/{user_id}/avatar_url` |
| **方法** | `PUT` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `avatar_url` | string | 是 | MXC URL 格式的头像地址 |

**请求示例**:
```json
{
  "avatar_url": "mxc://example.com/new_avatar"
}
```

**响应示例**:
```json
{}
```

---

### 3.4 用户目录

#### 3.4.1 搜索用户

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/user_directory/search` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `search_term` | string | 是 | 搜索关键词 |
| `limit` | integer | 否 | 返回结果数量限制 |

**请求示例**:
```json
{
  "search_term": "alice",
  "limit": 10
}
```

**响应示例**:
```json
{
  "results": [
    {
      "user_id": "@alice:example.com",
      "display_name": "Alice",
      "avatar_url": "mxc://example.com/avatar"
    }
  ],
  "limited": false
}
```

#### 3.4.2 获取用户列表

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/user_directory/list` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `limit` | integer | 否 | 返回结果数量限制 |
| `from` | string | 否 | 分页令牌 |

**响应示例**:
```json
{
  "results": [
    {
      "user_id": "@alice:example.com",
      "display_name": "Alice"
    }
  ],
  "next_batch": "token_123"
}
```

---

### 3.5 设备管理

#### 3.5.1 获取设备列表

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/devices` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "devices": [
    {
      "device_id": "DEVICEID",
      "display_name": "My Device",
      "last_seen_ip": "127.0.0.1",
      "last_seen_ts": 1234567890000,
      "user_id": "@alice:example.com"
    }
  ]
}
```

#### 3.5.2 获取设备信息

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/devices/{device_id}` |
| **方法** | `GET` |
| **认证** | 需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `device_id` | string | 是 | 设备ID |

**响应示例**:
```json
{
  "device_id": "DEVICEID",
  "display_name": "My Device",
  "last_seen_ip": "127.0.0.1",
  "last_seen_ts": 1234567890000,
  "user_id": "@alice:example.com"
}
```

#### 3.5.3 更新设备

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/devices/{device_id}` |
| **方法** | `PUT` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `display_name` | string | 是 | 设备显示名称 |

**请求示例**:
```json
{
  "display_name": "My Laptop"
}
```

**响应示例**:
```json
{}
```

#### 3.5.4 删除设备

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/devices/{device_id}` |
| **方法** | `DELETE` |
| **认证** | 需要 |

**响应示例**:
```json
{}
```

#### 3.5.5 批量删除设备

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/delete_devices` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `devices` | array | 是 | 要删除的设备ID列表 |

**请求示例**:
```json
{
  "devices": ["DEVICE1", "DEVICE2"]
}
```

**响应示例**:
```json
{}
```

---

### 3.6 在线状态

#### 3.6.1 获取在线状态

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/presence/{user_id}/status` |
| **方法** | `GET` |
| **认证** | 需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `user_id` | string | 是 | 用户ID |

**响应示例**:
```json
{
  "presence": "online",
  "last_active_ago": 12345,
  "status_msg": "Working from home",
  "currently_active": true
}
```

#### 3.6.2 设置在线状态

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/presence/{user_id}/status` |
| **方法** | `PUT` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `presence` | string | 是 | 状态：`online`、`offline`、`unavailable` |
| `status_msg` | string | 否 | 状态消息 |

**请求示例**:
```json
{
  "presence": "online",
  "status_msg": "Working from home"
}
```

**响应示例**:
```json
{}
```

---

### 3.7 同步与状态

#### 3.7.1 同步数据

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/sync` |
| **方法** | `GET` |
| **认证** | 需要 |

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `since` | string | 否 | 上次同步的令牌 |
| `timeout` | integer | 否 | 长轮询超时时间（毫秒） |
| `filter` | string | 否 | 过滤器ID或过滤器对象 |
| `full_state` | boolean | 否 | 是否返回完整状态 |
| `set_presence` | string | 否 | 设置在线状态 |

**响应示例**:
```json
{
  "next_batch": "s72594_4483_1934",
  "rooms": {
    "join": {
      "!room:example.com": {
        "timeline": {
          "events": [],
          "limited": false,
          "prev_batch": "t392-516_47314_0_7_1_1_1_11444_1"
        },
        "state": {
          "events": []
        },
        "ephemeral": {
          "events": []
        },
        "account_data": {
          "events": []
        },
        "unread_notifications": {
          "highlight_count": 0,
          "notification_count": 0
        }
      }
    },
    "invite": {},
    "leave": {}
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

#### 3.7.2 设置打字状态

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/typing/{user_id}` |
| **方法** | `PUT` |
| **认证** | 需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `room_id` | string | 是 | 房间ID |
| `user_id` | string | 是 | 用户ID |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `typing` | boolean | 是 | 是否正在输入 |
| `timeout` | integer | 否 | 超时时间（毫秒） |

**请求示例**:
```json
{
  "typing": true,
  "timeout": 30000
}
```

**响应示例**:
```json
{}
```

#### 3.7.3 发送已读回执

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/receipt/{receipt_type}/{event_id}` |
| **方法** | `POST` |
| **认证** | 需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `room_id` | string | 是 | 房间ID |
| `receipt_type` | string | 是 | 回执类型：`m.read`、`m.read.private` |
| `event_id` | string | 是 | 事件ID |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `thread_id` | string | 否 | 线程ID |

**请求示例**:
```json
{
  "thread_id": null
}
```

**响应示例**:
```json
{}
```

#### 3.7.4 设置已读标记

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/read_markers` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `m.fully_read` | string | 是 | 完全读取的事件ID |
| `m.read` | string | 否 | 读取位置事件ID |

**请求示例**:
```json
{
  "m.fully_read": "$event_id:example.com",
  "m.read": "$event_id:example.com"
}
```

**响应示例**:
```json
{}
```

---

### 3.8 房间管理

#### 3.8.1 创建房间

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/createRoom` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `visibility` | string | 否 | 可见性：`public`、`private` |
| `room_alias_name` | string | 否 | 房间别名 |
| `name` | string | 否 | 房间名称 |
| `topic` | string | 否 | 房间主题 |
| `invite` | array | 否 | 邀请的用户ID列表 |
| `invite_3pid` | array | 否 | 邀请的第三方用户列表 |
| `room_version` | string | 否 | 房间版本 |
| `creation_content` | object | 否 | 创建内容 |
| `initial_state` | array | 否 | 初始状态事件 |
| `preset` | string | 否 | 预设：`private_chat`、`public_chat`、`trusted_private_chat` |
| `is_direct` | boolean | 否 | 是否为私信房间 |
| `power_level_content_override` | object | 否 | 权限级别覆盖 |

**请求示例**:
```json
{
  "name": "My Room",
  "topic": "A test room",
  "preset": "private_chat",
  "invite": ["@bob:example.com"],
  "is_direct": false
}
```

**响应示例**:
```json
{
  "room_id": "!room:example.com"
}
```

#### 3.8.2 加入房间

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/join` |
| **方法** | `POST` |
| **认证** | 需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `room_id` | string | 是 | 房间ID或别名 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `third_party_signed` | object | 否 | 第三方签名 |

**响应示例**:
```json
{
  "room_id": "!room:example.com"
}
```

#### 3.8.3 离开房间

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/leave` |
| **方法** | `POST` |
| **认证** | 需要 |

**响应示例**:
```json
{}
```

#### 3.8.4 踢出用户

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/kick` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `user_id` | string | 是 | 要踢出的用户ID |
| `reason` | string | 否 | 原因 |

**请求示例**:
```json
{
  "user_id": "@bob:example.com",
  "reason": "Spamming"
}
```

**响应示例**:
```json
{}
```

#### 3.8.5 封禁用户

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/ban` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `user_id` | string | 是 | 要封禁的用户ID |
| `reason` | string | 否 | 原因 |

**请求示例**:
```json
{
  "user_id": "@bob:example.com",
  "reason": "Harassment"
}
```

**响应示例**:
```json
{}
```

#### 3.8.6 解除封禁

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/unban` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `user_id` | string | 是 | 要解除封禁的用户ID |

**请求示例**:
```json
{
  "user_id": "@bob:example.com"
}
```

**响应示例**:
```json
{}
```

#### 3.8.7 邀请用户

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/invite` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `user_id` | string | 是 | 要邀请的用户ID |
| `reason` | string | 否 | 原因 |

**请求示例**:
```json
{
  "user_id": "@bob:example.com",
  "reason": "Join our chat"
}
```

**响应示例**:
```json
{}
```

---

### 3.9 房间状态与消息

#### 3.9.1 获取房间状态

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/state` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "events": [
    {
      "type": "m.room.name",
      "state_key": "",
      "content": {
        "name": "My Room"
      },
      "sender": "@alice:example.com",
      "event_id": "$event_id:example.com",
      "origin_server_ts": 1234567890000
    }
  ]
}
```

#### 3.9.2 获取特定状态事件

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/state/{event_type}` |
| **方法** | `GET` |
| **认证** | 需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `room_id` | string | 是 | 房间ID |
| `event_type` | string | 是 | 事件类型 |

**响应示例**:
```json
{
  "name": "My Room"
}
```

#### 3.9.3 获取状态事件（带状态键）

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}` |
| **方法** | `GET` |
| **认证** | 需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `room_id` | string | 是 | 房间ID |
| `event_type` | string | 是 | 事件类型 |
| `state_key` | string | 是 | 状态键 |

**响应示例**:
```json
{
  "displayname": "Alice",
  "membership": "join"
}
```

#### 3.9.4 设置房间状态

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}` |
| **方法** | `PUT` |
| **认证** | 需要 |

**请求体**: 根据事件类型而定

**请求示例**:
```json
{
  "name": "New Room Name"
}
```

**响应示例**:
```json
{
  "event_id": "$event_id:example.com"
}
```

#### 3.9.5 发送事件/消息

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}` |
| **方法** | `PUT` |
| **认证** | 需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `room_id` | string | 是 | 房间ID |
| `event_type` | string | 是 | 事件类型 |
| `txn_id` | string | 是 | 事务ID |

**请求示例**:
```json
{
  "msgtype": "m.text",
  "body": "Hello, World!"
}
```

**响应示例**:
```json
{
  "event_id": "$event_id:example.com"
}
```

#### 3.9.6 获取房间消息

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/messages` |
| **方法** | `GET` |
| **认证** | 需要 |

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `from` | string | 否 | 起始令牌 |
| `to` | string | 否 | 结束令牌 |
| `dir` | string | 是 | 方向：`f`（向前）、`b`（向后） |
| `limit` | integer | 否 | 数量限制 |
| `filter` | string | 否 | 过滤器 |

**响应示例**:
```json
{
  "chunk": [
    {
      "type": "m.room.message",
      "content": {
        "msgtype": "m.text",
        "body": "Hello!"
      },
      "sender": "@alice:example.com",
      "event_id": "$event_id:example.com",
      "origin_server_ts": 1234567890000
    }
  ],
  "start": "t392-516_47314_0_7_1_1_1_11444_1",
  "end": "t392-516_47314_0_7_1_1_1_11444_2"
}
```

#### 3.9.7 获取房间成员

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/members` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "chunk": [
    {
      "type": "m.room.member",
      "state_key": "@alice:example.com",
      "content": {
        "membership": "join",
        "displayname": "Alice"
      },
      "sender": "@alice:example.com",
      "event_id": "$event_id:example.com"
    }
  ]
}
```

#### 3.9.8 删除事件

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/redact/{event_id}` |
| **方法** | `PUT` |
| **认证** | 需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `room_id` | string | 是 | 房间ID |
| `event_id` | string | 是 | 事件ID |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `reason` | string | 否 | 删除原因 |

**请求示例**:
```json
{
  "reason": "Inappropriate content"
}
```

**响应示例**:
```json
{
  "event_id": "$redaction_id:example.com"
}
```

---

### 3.10 房间目录

#### 3.10.1 获取房间信息

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/directory/room/{room_id}` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "room_id": "!room:example.com",
  "servers": ["example.com", "other.com"]
}
```

#### 3.10.2 获取公共房间列表

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/publicRooms` |
| **方法** | `GET` |
| **认证** | 不需要 |

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `limit` | integer | 否 | 数量限制 |
| `since` | string | 否 | 分页令牌 |
| `server` | string | 否 | 服务器名称 |

**响应示例**:
```json
{
  "chunk": [
    {
      "room_id": "!room:example.com",
      "name": "Public Room",
      "topic": "A public room",
      "num_joined_members": 42,
      "world_readable": true,
      "guest_can_join": true,
      "avatar_url": "mxc://example.com/avatar"
    }
  ],
  "total_room_count_estimate": 100,
  "next_batch": "token_123"
}
```

#### 3.10.3 通过别名获取房间

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/directory/room/alias/{room_alias}` |
| **方法** | `GET` |
| **认证** | 不需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `room_alias` | string | 是 | 房间别名 |

**响应示例**:
```json
{
  "room_id": "!room:example.com",
  "servers": ["example.com"]
}
```

---

### 3.11 用户房间

#### 3.11.1 获取用户房间列表

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/user/{user_id}/rooms` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "joined": ["!room1:example.com", "!room2:example.com"],
  "invited": [],
  "left": []
}
```

---

### 3.12 事件举报

#### 3.12.1 举报事件

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/report/{event_id}` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `reason` | string | 否 | 举报原因 |
| `score` | integer | 否 | 严重程度分数（-100 到 0） |

**请求示例**:
```json
{
  "reason": "Spam",
  "score": -50
}
```

**响应示例**:
```json
{}
```

---

## 4. 管理员 API

> 所有管理员 API 需要管理员认证。

### 4.1 服务器信息

#### 4.1.1 获取服务器版本

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/server_version` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "server_version": "0.1.0",
  "python_version": "rust"
}
```

#### 4.1.2 获取服务器状态

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/status` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "status": "ok",
  "database": "connected",
  "cache": "connected",
  "uptime_seconds": 86400
}
```

#### 4.1.3 获取服务器统计

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/server_stats` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "total_users": 100,
  "total_rooms": 50,
  "total_messages": 10000,
  "daily_active_users": 25,
  "monthly_active_users": 80
}
```

#### 4.1.4 获取服务器配置

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/config` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "server_name": "example.com",
  "public_baseurl": "https://example.com",
  "max_upload_size": 52428800
}
```

#### 4.1.5 获取用户统计

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/user_stats` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "total_users": 100,
  "active_users": 25,
  "new_users_today": 5
}
```

#### 4.1.6 获取媒体统计

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/media_stats` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "total_media": 500,
  "total_size_bytes": 1073741824
}
```

#### 4.1.7 获取服务器日志

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/logs` |
| **方法** | `GET` |
| **认证** | 管理员 |

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `limit` | integer | 否 | 日志条数限制 |
| `level` | string | 否 | 日志级别 |

**响应示例**:
```json
{
  "logs": [
    {
      "timestamp": "2026-02-12T00:00:00Z",
      "level": "INFO",
      "message": "Server started"
    }
  ]
}
```

---

### 4.2 用户管理

#### 4.2.1 获取用户列表

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/users` |
| **方法** | `GET` |
| **认证** | 管理员 |

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `limit` | integer | 否 | 数量限制 |
| `from` | string | 否 | 分页令牌 |
| `name` | string | 否 | 用户名过滤 |
| `guests` | boolean | 否 | 是否包含访客 |

**响应示例**:
```json
{
  "users": [
    {
      "user_id": "@alice:example.com",
      "displayname": "Alice",
      "avatar_url": "mxc://example.com/avatar",
      "admin": false,
      "deactivated": false
    }
  ],
  "total": 100,
  "next_token": "token_123"
}
```

#### 4.2.2 获取用户信息

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/users/{user_id}` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "user_id": "@alice:example.com",
  "displayname": "Alice",
  "avatar_url": "mxc://example.com/avatar",
  "admin": false,
  "deactivated": false,
  "creation_ts": 1234567890,
  "last_seen_ts": 1234567890
}
```

#### 4.2.3 删除用户

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/users/{user_id}` |
| **方法** | `DELETE` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "deleted": true
}
```

#### 4.2.4 设置管理员

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/users/{user_id}/admin` |
| **方法** | `PUT` |
| **认证** | 管理员 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `admin` | boolean | 是 | 是否为管理员 |

**请求示例**:
```json
{
  "admin": true
}
```

**响应示例**:
```json
{}
```

#### 4.2.5 停用用户

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/users/{user_id}/deactivate` |
| **方法** | `POST` |
| **认证** | 管理员 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `erase` | boolean | 否 | 是否删除所有数据 |

**请求示例**:
```json
{
  "erase": true
}
```

**响应示例**:
```json
{
  "id_server_unbind_result": "success"
}
```

#### 4.2.6 获取用户房间

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/users/{user_id}/rooms` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "rooms": [
    {
      "room_id": "!room:example.com",
      "name": "My Room",
      "joined_members": 5,
      "joined_local_members": 3
    }
  ],
  "total": 10
}
```

#### 4.2.7 重置用户密码

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/users/{user_id}/password` |
| **方法** | `POST` |
| **认证** | 管理员 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `new_password` | string | 是 | 新密码 |
| `logout_devices` | boolean | 否 | 是否登出所有设备 |

**请求示例**:
```json
{
  "new_password": "newpassword123",
  "logout_devices": true
}
```

**响应示例**:
```json
{}
```

#### 4.2.8 获取注册 nonce

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/register/nonce` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "nonce": "abc123"
}
```

#### 4.2.9 管理员注册

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/register` |
| **方法** | `POST` |
| **认证** | 管理员 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `username` | string | 是 | 用户名 |
| `password` | string | 是 | 密码 |
| `nonce` | string | 是 | 注册 nonce |
| `mac` | string | 是 | HMAC 签名 |
| `admin` | boolean | 否 | 是否为管理员 |

**请求示例**:
```json
{
  "username": "newuser",
  "password": "password123",
  "nonce": "abc123",
  "mac": "signature",
  "admin": false
}
```

**响应示例**:
```json
{
  "user_id": "@newuser:example.com",
  "access_token": "syt_token..."
}
```

---

### 4.3 房间管理

#### 4.3.1 获取房间列表

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/rooms` |
| **方法** | `GET` |
| **认证** | 管理员 |

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `limit` | integer | 否 | 数量限制 |
| `from` | string | 否 | 分页令牌 |
| `search_term` | string | 否 | 搜索关键词 |

**响应示例**:
```json
{
  "rooms": [
    {
      "room_id": "!room:example.com",
      "name": "My Room",
      "creator": "@alice:example.com",
      "joined_members": 5
    }
  ],
  "total_rooms": 50,
  "next_batch": "token_123"
}
```

#### 4.3.2 获取房间信息

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/rooms/{room_id}` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "room_id": "!room:example.com",
  "name": "My Room",
  "topic": "A test room",
  "creator": "@alice:example.com",
  "joined_members": 5,
  "state_events": 20,
  "version": "6"
}
```

#### 4.3.3 删除房间

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/rooms/{room_id}` |
| **方法** | `DELETE` |
| **认证** | 管理员 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `purge` | boolean | 否 | 是否清除数据 |
| `force_purge` | boolean | 否 | 强制清除 |

**请求示例**:
```json
{
  "purge": true
}
```

**响应示例**:
```json
{
  "deleted": true
}
```

#### 4.3.4 清理历史

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/purge_history` |
| **方法** | `POST` |
| **认证** | 管理员 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `room_id` | string | 是 | 房间ID |
| `delete_local_events` | boolean | 否 | 是否删除本地事件 |

**请求示例**:
```json
{
  "room_id": "!room:example.com",
  "delete_local_events": false
}
```

**响应示例**:
```json
{
  "purge_id": "purge_123"
}
```

#### 4.3.5 关闭房间

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/shutdown_room` |
| **方法** | `POST` |
| **认证** | 管理员 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `room_id` | string | 是 | 房间ID |
| `new_room_user_id` | string | 否 | 新房间用户ID |
| `new_room_name` | string | 否 | 新房间名称 |
| `message` | string | 否 | 关闭消息 |

**请求示例**:
```json
{
  "room_id": "!room:example.com",
  "message": "This room has been shut down"
}
```

**响应示例**:
```json
{
  "kicked_users": ["@alice:example.com", "@bob:example.com"],
  "failed_to_kick_users": [],
  "local_aliases": [],
  "new_room_id": null
}
```

---

### 4.4 安全相关

#### 4.4.1 获取安全事件

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/security/events` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "events": [
    {
      "id": 1,
      "type": "login_failed",
      "user_id": "@alice:example.com",
      "timestamp": 1234567890000,
      "ip": "127.0.0.1"
    }
  ]
}
```

#### 4.4.2 获取 IP 阻止列表

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/security/ip/blocks` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "blocks": [
    {
      "ip": "192.168.1.1",
      "reason": "Spam",
      "blocked_at": 1234567890000,
      "blocked_by": "@admin:example.com"
    }
  ]
}
```

#### 4.4.3 阻止 IP

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/security/ip/block` |
| **方法** | `POST` |
| **认证** | 管理员 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `ip` | string | 是 | IP 地址 |
| `reason` | string | 否 | 原因 |

**请求示例**:
```json
{
  "ip": "192.168.1.1",
  "reason": "Spam"
}
```

**响应示例**:
```json
{
  "blocked": true
}
```

#### 4.4.4 解除 IP 阻止

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/security/ip/unblock` |
| **方法** | `POST` |
| **认证** | 管理员 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `ip` | string | 是 | IP 地址 |

**请求示例**:
```json
{
  "ip": "192.168.1.1"
}
```

**响应示例**:
```json
{
  "unblocked": true
}
```

#### 4.4.5 获取 IP 信誉

| 属性 | 值 |
|------|-----|
| **端点** | `/_synapse/admin/v1/security/ip/reputation/{ip}` |
| **方法** | `GET` |
| **认证** | 管理员 |

**响应示例**:
```json
{
  "ip": "192.168.1.1",
  "score": 50,
  "last_seen": 1234567890000,
  "login_attempts": 5,
  "failed_logins": 2
}
```

---

## 5. 联邦通信 API

### 5.1 密钥与发现（无需签名）

#### 5.1.1 获取服务器密钥

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v2/server` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "server_name": "example.com",
  "valid_until_ts": 1234567890000,
  "verify_keys": {
    "ed25519:a_ABCD": {
      "key": "base64_encoded_key"
    }
  },
  "old_verify_keys": {},
  "signatures": {
    "example.com": {
      "ed25519:a_ABCD": "signature"
    }
  }
}
```

#### 5.1.2 获取联邦版本

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v1/version` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "server": {
    "name": "Synapse Rust",
    "version": "0.1.0"
  }
}
```

#### 5.1.3 获取公共房间

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v1/publicRooms` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "chunk": [
    {
      "room_id": "!room:example.com",
      "name": "Public Room"
    }
  ]
}
```

---

### 5.2 房间操作（需要签名）

#### 5.2.1 发送事务

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v1/send/{txn_id}` |
| **方法** | `PUT` |
| **认证** | 联邦签名 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `origin` | string | 是 | 发送方服务器 |
| `pdus` | array | 是 | PDU 列表 |
| `edus` | array | 否 | EDU 列表 |

**响应示例**:
```json
{
  "pdus": {
    "$event_id:example.com": {}
  }
}
```

#### 5.2.2 生成加入模板

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v1/make_join/{room_id}/{user_id}` |
| **方法** | `GET` |
| **认证** | 联邦签名 |

**响应示例**:
```json
{
  "room_version": "6",
  "event": {
    "type": "m.room.member",
    "room_id": "!room:example.com",
    "sender": "@alice:example.com",
    "state_key": "@alice:example.com",
    "content": {
      "membership": "join"
    }
  }
}
```

#### 5.2.3 发送加入

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v1/send_join/{room_id}/{event_id}` |
| **方法** | `PUT` |
| **认证** | 联邦签名 |

**响应示例**:
```json
{
  "room_version": "6",
  "origin": "example.com",
  "state": [],
  "auth_chain": []
}
```

#### 5.2.4 邀请

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v2/invite/{room_id}/{event_id}` |
| **方法** | `PUT` |
| **认证** | 联邦签名 |

**响应示例**:
```json
{
  "event": {}
}
```

#### 5.2.5 获取事件

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v1/event/{event_id}` |
| **方法** | `GET` |
| **认证** | 联邦签名 |

**响应示例**:
```json
{
  "origin": "example.com",
  "origin_server_ts": 1234567890000,
  "pdus": []
}
```

#### 5.2.6 获取房间状态

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v1/state/{room_id}` |
| **方法** | `GET` |
| **认证** | 联邦签名 |

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `event_id` | string | 是 | 事件ID |

**响应示例**:
```json
{
  "room_version": "6",
  "pdus": [],
  "auth_chain": []
}
```

#### 5.2.7 获取房间成员

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v1/members/{room_id}` |
| **方法** | `GET` |
| **认证** | 联邦签名 |

**响应示例**:
```json
{
  "joined": ["@alice:example.com", "@bob:example.com"]
}
```

---

### 5.3 好友系统联邦（需要签名）

#### 5.3.1 查询用户好友列表

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v1/friends/query/{user_id}` |
| **方法** | `GET` |
| **认证** | 联邦签名 |

**响应示例**:
```json
{
  "user_id": "@alice:example.com",
  "friends": ["@bob:other.com", "@charlie:third.com"]
}
```

#### 5.3.2 验证好友关系

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v1/friends/relationship/{user_id}/{friend_id}` |
| **方法** | `GET` |
| **认证** | 联邦签名 |

**响应示例**:
```json
{
  "are_friends": true,
  "since": 1234567890
}
```

#### 5.3.3 发送跨服务器好友请求

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/federation/v1/friends/request` |
| **方法** | `POST` |
| **认证** | 联邦签名 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `requester` | string | 是 | 请求者用户ID |
| `target` | string | 是 | 目标用户ID |
| `message` | string | 否 | 请求消息 |

**响应示例**:
```json
{
  "request_id": "req_123",
  "status": "pending"
}
```

---

## 6. 好友系统 API

> 好友系统已完全重构为基于 Matrix 房间的实现。

### 6.1 好友管理

#### 6.1.1 获取好友列表

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v1/friends` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "friends": [
    {
      "user_id": "@bob:example.com",
      "display_name": "Bob",
      "avatar_url": "mxc://example.com/avatar",
      "since": 1234567890,
      "status": "online",
      "note": "Best friend"
    }
  ],
  "total": 1
}
```

#### 6.1.2 发送好友请求

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v1/friends/request` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `user_id` | string | 是 | 目标用户ID |
| `message` | string | 否 | 请求消息 |

**请求示例**:
```json
{
  "user_id": "@bob:example.com",
  "message": "Hi, let's be friends!"
}
```

**响应示例**:
```json
{
  "room_id": "!dm:example.com",
  "status": "pending"
}
```

**状态码**:

| 状态码 | 说明 |
|--------|------|
| 200 | 请求发送成功 |
| 400 | 参数无效 |
| 409 | 已经是好友或已有待处理请求 |

#### 6.1.3 接受好友请求

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v1/friends/request/{user_id}/accept` |
| **方法** | `POST` |
| **认证** | 需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `user_id` | string | 是 | 请求者用户ID |

**响应示例**:
```json
{
  "room_id": "!dm:example.com",
  "status": "accepted"
}
```

#### 6.1.4 拒绝好友请求

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v1/friends/request/{user_id}/reject` |
| **方法** | `POST` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "status": "rejected"
}
```

#### 6.1.5 取消好友请求

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v1/friends/request/{user_id}/cancel` |
| **方法** | `POST` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "status": "cancelled"
}
```

#### 6.1.6 获取收到的请求

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v1/friends/requests/incoming` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "requests": [
    {
      "user_id": "@bob:example.com",
      "display_name": "Bob",
      "message": "Hi!",
      "timestamp": 1234567890000,
      "status": "pending"
    }
  ]
}
```

#### 6.1.7 获取发出的请求

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v1/friends/requests/outgoing` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "requests": [
    {
      "user_id": "@charlie:example.com",
      "timestamp": 1234567890000,
      "status": "pending"
    }
  ]
}
```

#### 6.1.8 删除好友

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v1/friends/{user_id}` |
| **方法** | `DELETE` |
| **认证** | 需要 |

**响应示例**:
```json
{}
```

**状态码**:

| 状态码 | 说明 |
|--------|------|
| 200 | 删除成功 |
| 404 | 好友不存在 |

#### 6.1.9 更新好友备注

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v1/friends/{user_id}/note` |
| **方法** | `PUT` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `note` | string | 是 | 备注内容（最大1000字符） |

**请求示例**:
```json
{
  "note": "Met at conference"
}
```

**响应示例**:
```json
{}
```

#### 6.1.10 更新好友状态

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v1/friends/{user_id}/status` |
| **方法** | `PUT` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `status` | string | 是 | 状态：`favorite`、`normal`、`blocked`、`hidden` |

**请求示例**:
```json
{
  "status": "favorite"
}
```

**响应示例**:
```json
{}
```

#### 6.1.11 获取好友信息

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v1/friends/{user_id}/info` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "user_id": "@bob:example.com",
  "display_name": "Bob",
  "avatar_url": "mxc://example.com/avatar",
  "since": 1234567890,
  "status": "normal",
  "note": "Best friend",
  "dm_room_id": "!dm:example.com"
}
```

---

## 7. 端到端加密 API

### 7.1 上传设备密钥

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/keys/upload` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `device_keys` | object | 否 | 设备密钥 |
| `one_time_keys` | object | 否 | 一次性密钥 |

**请求示例**:
```json
{
  "device_keys": {
    "user_id": "@alice:example.com",
    "device_id": "DEVICEID",
    "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
    "keys": {
      "curve25519:DEVICEID": "base64_key",
      "ed25519:DEVICEID": "base64_key"
    },
    "signatures": {
      "@alice:example.com": {
        "ed25519:DEVICEID": "signature"
      }
    }
  },
  "one_time_keys": {
    "curve25519:ABCDEF": {
      "key": "base64_key"
    }
  }
}
```

**响应示例**:
```json
{
  "one_time_key_counts": {
    "curve25519": 50,
    "signed_curve25519": 50
  }
}
```

### 7.2 查询设备密钥

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/keys/query` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `device_keys` | object | 是 | 要查询的用户和设备 |
| `timeout` | integer | 否 | 超时时间 |
| `token` | string | 否 | 同步令牌 |

**请求示例**:
```json
{
  "device_keys": {
    "@bob:example.com": []
  }
}
```

**响应示例**:
```json
{
  "device_keys": {
    "@bob:example.com": {
      "DEVICEID": {
        "user_id": "@bob:example.com",
        "device_id": "DEVICEID",
        "algorithms": ["m.olm.v1.curve25519-aes-sha2"],
        "keys": {},
        "signatures": {}
      }
    }
  },
  "failures": {}
}
```

### 7.3 声明一次性密钥

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/keys/claim` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `one_time_keys` | object | 是 | 要声明的一次性密钥 |
| `timeout` | integer | 否 | 超时时间 |

**请求示例**:
```json
{
  "one_time_keys": {
    "@bob:example.com": {
      "DEVICEID": "signed_curve25519"
    }
  }
}
```

**响应示例**:
```json
{
  "one_time_keys": {
    "@bob:example.com": {
      "DEVICEID": {
        "signed_curve25519:ABCDEF": {
          "key": "base64_key",
          "signatures": {}
        }
      }
    }
  },
  "failures": {}
}
```

### 7.4 获取密钥变更通知

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/keys/changes` |
| **方法** | `GET` |
| **认证** | 需要 |

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `from` | string | 是 | 起始令牌 |
| `to` | string | 是 | 结束令牌 |

**响应示例**:
```json
{
  "changed": ["@bob:example.com"],
  "left": []
}
```

### 7.5 获取房间密钥分发

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/rooms/{room_id}/keys/distribution` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "room_id": "!room:example.com",
  "algorithm": "m.megolm.v1.aes-sha2",
  "session_id": "session_id",
  "session_key": "base64_session_key"
}
```

### 7.6 发送设备到设备消息

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/sendToDevice/{event_type}/{transaction_id}` |
| **方法** | `PUT` |
| **认证** | 需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `event_type` | string | 是 | 事件类型 |
| `transaction_id` | string | 是 | 事务ID |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `messages` | object | 是 | 消息内容 |

**请求示例**:
```json
{
  "messages": {
    "@bob:example.com": {
      "DEVICEID": {
        "algorithm": "m.megolm.v1.aes-sha2",
        "sender_key": "sender_curve25519_key",
        "session_id": "session_id",
        "session_key": "session_key"
      }
    }
  }
}
```

**响应示例**:
```json
{}
```

---

## 8. 媒体文件 API

### 8.1 上传媒体

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/media/v3/upload` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `content` | array/string | 是 | 文件内容（字节数组或 Base64） |
| `content_type` | string | 否 | MIME 类型 |
| `filename` | string | 否 | 文件名 |

**请求示例**:
```json
{
  "content": "base64_encoded_content",
  "content_type": "image/png",
  "filename": "avatar.png"
}
```

**响应示例**:
```json
{
  "content_uri": "mxc://example.com/media_id"
}
```

### 8.2 下载媒体

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/media/v3/download/{server_name}/{media_id}` |
| **方法** | `GET` |
| **认证** | 不需要 |

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `server_name` | string | 是 | 服务器名称 |
| `media_id` | string | 是 | 媒体ID |

**响应**: 二进制文件内容

**响应头**:

| 响应头 | 说明 |
|--------|------|
| `Content-Type` | MIME 类型 |
| `Content-Length` | 文件大小 |

### 8.3 获取缩略图

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/media/v3/thumbnail/{server_name}/{media_id}` |
| **方法** | `GET` |
| **认证** | 不需要 |

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `width` | integer | 否 | 宽度（默认 800） |
| `height` | integer | 否 | 高度（默认 600） |
| `method` | string | 否 | 缩放方式：`scale`、`crop` |

**响应**: 缩略图二进制内容

### 8.4 获取媒体配置

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/media/v1/config` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "m.upload.size": 52428800
}
```

---

## 9. 语音消息 API

### 9.1 获取语音配置

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/voice/config` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "supported_formats": ["audio/ogg", "audio/mpeg", "audio/wav"],
  "max_size_bytes": 104857600,
  "max_duration_ms": 600000,
  "default_sample_rate": 48000,
  "default_channels": 2
}
```

### 9.2 上传语音消息

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/voice/upload` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `content` | string | 是 | Base64 编码的音频内容 |
| `content_type` | string | 否 | MIME 类型（默认 audio/ogg） |
| `duration_ms` | integer | 是 | 时长（毫秒） |
| `room_id` | string | 否 | 房间ID |
| `session_id` | string | 否 | 会话ID |

**请求示例**:
```json
{
  "content": "base64_encoded_audio",
  "content_type": "audio/ogg",
  "duration_ms": 5000,
  "room_id": "!room:example.com"
}
```

**响应示例**:
```json
{
  "message_id": "msg_123",
  "content_uri": "mxc://example.com/voice_123",
  "duration_ms": 5000
}
```

### 9.3 获取语音消息

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/voice/{message_id}` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "message_id": "msg_123",
  "content": "base64_encoded_audio",
  "content_type": "audio/ogg",
  "size": 102400
}
```

### 9.4 删除语音消息

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/voice/{message_id}` |
| **方法** | `DELETE` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "deleted": true,
  "message_id": "msg_123"
}
```

### 9.5 获取用户语音消息

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/voice/user/{user_id}` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "messages": [
    {
      "message_id": "msg_123",
      "duration_ms": 5000,
      "created_at": 1234567890000
    }
  ]
}
```

### 9.6 获取房间语音消息

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/voice/room/{room_id}` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "messages": [
    {
      "message_id": "msg_123",
      "user_id": "@alice:example.com",
      "duration_ms": 5000
    }
  ]
}
```

### 9.7 获取用户语音统计

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/voice/user/{user_id}/stats` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "user_id": "@alice:example.com",
  "total_messages": 10,
  "total_duration_ms": 50000,
  "total_size_bytes": 1024000
}
```

### 9.8 获取当前用户语音统计

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/voice/stats` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "user_id": "@alice:example.com",
  "total_messages": 10,
  "total_duration_ms": 50000,
  "total_size_bytes": 1024000
}
```

### 9.9 语音格式转换

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/voice/convert` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `message_id` | string | 是 | 消息ID |
| `target_format` | string | 是 | 目标格式（如 audio/mpeg） |
| `quality` | integer | 否 | 质量（32-320 kbps） |
| `bitrate` | integer | 否 | 比特率（64000-320000 bps） |

**请求示例**:
```json
{
  "message_id": "msg_123",
  "target_format": "audio/mpeg",
  "quality": 128,
  "bitrate": 128000
}
```

**响应示例**:
```json
{
  "status": "success",
  "message_id": "msg_123",
  "target_format": "audio/mpeg",
  "quality": 128,
  "bitrate": 128000
}
```

### 9.10 语音优化

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/voice/optimize` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `message_id` | string | 是 | 消息ID |
| `target_size_kb` | integer | 否 | 目标大小（10-10000 KB） |
| `preserve_quality` | boolean | 否 | 是否保持质量 |
| `remove_silence` | boolean | 否 | 是否移除静音 |
| `normalize_volume` | boolean | 否 | 是否标准化音量 |

**请求示例**:
```json
{
  "message_id": "msg_123",
  "target_size_kb": 500,
  "preserve_quality": true,
  "remove_silence": false,
  "normalize_volume": true
}
```

**响应示例**:
```json
{
  "status": "success",
  "message_id": "msg_123",
  "target_size_kb": 500
}
```

---

## 10. VoIP API

### 10.1 获取 TURN 服务器

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v0/voip/turnServer` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "username": "user:1234567890",
  "password": "credential",
  "uris": [
    "turn:turn.example.com:3478?transport=udp",
    "turn:turn.example.com:3478?transport=tcp"
  ],
  "ttl": 86400
}
```

### 10.2 获取 VoIP 配置

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v0/voip/config` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "turn_servers": [
    {
      "username": "user",
      "password": "pass",
      "uris": ["turn:turn.example.com:3478"],
      "ttl": 86400
    }
  ],
  "stun_servers": ["stun:stun.example.com:3478"]
}
```

### 10.3 获取访客 TURN 凭证

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/v0/voip/turnServer/guest` |
| **方法** | `GET` |
| **认证** | 不需要 |

**响应示例**:
```json
{
  "username": "guest:1234567890",
  "password": "guest_credential",
  "uris": ["turn:turn.example.com:3478"],
  "ttl": 86400
}
```

---

## 11. 密钥备份 API

### 11.1 获取所有备份版本

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/room_keys/version` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "versions": [
    {
      "algorithm": "m.megolm.v1.aes-sha2",
      "auth_data": {},
      "version": "1"
    }
  ]
}
```

### 11.2 创建备份版本

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/room_keys/version` |
| **方法** | `POST` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `algorithm` | string | 否 | 算法（默认 m.megolm.v1.aes-sha2） |
| `auth_data` | object | 否 | 认证数据 |

**请求示例**:
```json
{
  "algorithm": "m.megolm.v1.aes-sha2",
  "auth_data": {
    "public_key": "base64_public_key"
  }
}
```

**响应示例**:
```json
{
  "version": "1"
}
```

### 11.3 获取特定备份版本

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/room_keys/version/{version}` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "algorithm": "m.megolm.v1.aes-sha2",
  "auth_data": {},
  "version": "1"
}
```

### 11.4 更新备份版本

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/room_keys/version/{version}` |
| **方法** | `PUT` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `auth_data` | object | 否 | 认证数据 |

**请求示例**:
```json
{
  "auth_data": {
    "public_key": "new_base64_public_key"
  }
}
```

**响应示例**:
```json
{
  "version": "1"
}
```

### 11.5 删除备份版本

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/room_keys/version/{version}` |
| **方法** | `DELETE` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "deleted": true,
  "version": "1"
}
```

### 11.6 获取房间密钥

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/room_keys/{version}` |
| **方法** | `GET` |
| **认证** | 需要 |

**响应示例**:
```json
{
  "rooms": {
    "!room:example.com": {
      "sessions": {
        "session_id": {
          "first_message_index": 0,
          "forwarded_count": 0,
          "is_verified": true,
          "session_data": {}
        }
      }
    }
  },
  "etag": "1_1234567890"
}
```

### 11.7 上传房间密钥

| 属性 | 值 |
|------|-----|
| **端点** | `/_matrix/client/r0/room_keys/{version}` |
| **方法** | `PUT` |
| **认证** | 需要 |

**请求体**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `room_id` | string | 否 | 房间ID |
| `sessions` | array | 否 | 会话列表 |

**请求示例**:
```json
{
  "room_id": "!room:example.com",
  "sessions": [
    {
      "session_id": "session_id",
      "session_data": {}
    }
  ]
}
```

**响应示例**:
```json
{
  "count": 1,
  "etag": "1_1234567890"
}
```

---

## 12. 错误码参考

### 12.1 标准 Matrix 错误码

| 错误码 | HTTP 状态码 | 说明 |
|--------|-------------|------|
| `M_FORBIDDEN` | 403 | 禁止访问 |
| `M_UNKNOWN_TOKEN` | 401 | 无效或过期的令牌 |
| `M_MISSING_TOKEN` | 401 | 缺少令牌 |
| `M_BAD_JSON` | 400 | JSON 格式错误 |
| `M_NOT_JSON` | 400 | 不是 JSON 格式 |
| `M_NOT_FOUND` | 404 | 资源不存在 |
| `M_LIMIT_EXCEEDED` | 429 | 请求过于频繁 |
| `M_UNKNOWN` | 500 | 未知错误 |
| `M_UNRECOGNIZED` | 400 | 无法识别的请求 |
| `M_UNAUTHORIZED` | 401 | 未授权 |
| `M_USER_DEACTIVATED` | 403 | 用户已停用 |
| `M_USER_IN_USE` | 400 | 用户名已存在 |
| `M_INVALID_USERNAME` | 400 | 无效的用户名 |
| `M_ROOM_IN_USE` | 400 | 房间已存在 |
| `M_INVALID_ROOM_STATE` | 400 | 无效的房间状态 |
| `M_THREEPID_IN_USE` | 400 | 第三方ID已存在 |
| `M_THREEPID_NOT_FOUND` | 400 | 第三方ID不存在 |
| `M_THREEPID_AUTH_FAILED` | 401 | 第三方ID认证失败 |
| `M_THREEPID_DENIED` | 403 | 第三方ID被拒绝 |
| `M_SERVER_NOT_TRUSTED` | 401 | 服务器不受信任 |
| `M_UNSUPPORTED_ROOM_VERSION` | 400 | 不支持的房间版本 |
| `M_INCOMPATIBLE_ROOM_VERSION` | 400 | 不兼容的房间版本 |
| `M_BAD_STATE` | 400 | 错误的状态 |
| `M_GUEST_ACCESS_FORBIDDEN` | 403 | 访客禁止访问 |
| `M_CAPTCHA_INVALID` | 400 | 验证码无效 |
| `M_MISSING_PARAM` | 400 | 缺少参数 |
| `M_INVALID_PARAM` | 400 | 无效参数 |
| `M_TOO_LARGE` | 413 | 请求体过大 |
| `M_EXCLUSIVE` | 400 | 排他性错误 |
| `M_RESOURCE_LIMIT_EXCEEDED` | 429 | 资源限制超出 |

### 12.2 错误响应格式

```json
{
  "errcode": "M_NOT_FOUND",
  "error": "Resource not found",
  "status": "error"
}
```

---

## 13. API 统计

| 分类 | 端点数量 |
|------|---------|
| 核心客户端 API | 62 |
| 管理员 API | 27 |
| 联邦通信 API | 39 |
| 好友系统 API | 11 |
| 端到端加密 API | 6 |
| 媒体文件 API | 8 |
| 语音消息 API | 10 |
| VoIP API | 3 |
| 密钥备份 API | 11 |
| **总计** | **177** |

---

## 更新日志

### 2026-02-12 (v4.0)
- ✅ 全面更新 API 文档，包含详细请求/响应格式
- ✅ 添加所有请求参数、请求体、响应体说明
- ✅ 添加状态码和错误码参考
- ✅ 添加认证方式和权限要求
- ✅ 更新好友系统 API 端点

### 2026-02-11 (v3.0)
- ✅ 重写 API 参考文档，基于实际代码实现
- ✅ 更新好友系统 API

### 之前版本
- 详见项目提交历史
