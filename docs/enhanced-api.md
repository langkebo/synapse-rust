# Enhanced API 文档

本文档列出了 Enhanced 项目中的所有 API 端点。

## 目录

- [好友管理 API](#好友管理-api)
- [私聊 API](#私聊-api)
- [管理员 API v1](#管理员-api-v1)
- [管理员 API v2](#管理员-api-v2)
- [语音消息 API v2](#语音消息-api-v2)

---

## 好友管理 API

### 好友请求 API

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_matrix/client/v1/friends/requests` | 发送好友请求 |
| POST | `/_matrix/client/v1/friends/requests/{room_id}/accept` | 接受好友请求 |
| POST | `/_matrix/client/v1/friends/requests/{room_id}/reject` | 拒绝好友请求 |

### 好友分类 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/v1/friends/categories` | 获取好友分类列表 |
| POST | `/_matrix/client/v1/friends/categories` | 设置好友分类 |
| GET | `/_matrix/client/v1/friends/categories/{category_name}` | 获取指定分类详情 |
| PUT | `/_matrix/client/v1/friends/categories/{category_name}` | 更新指定分类 |
| DELETE | `/_matrix/client/v1/friends/categories/{category_name}` | 删除指定分类 |
| POST | `/_matrix/client/v1/friends/categories/{category_name}/users` | 添加用户到分类 |
| DELETE | `/_matrix/client/v1/friends/categories/{category_name}/users` | 从分类移除用户 |

### 黑名单 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/v1/friends/blocked` | 获取黑名单列表 |
| POST | `/_matrix/client/v1/friends/blocked` | 添加用户到黑名单 |
| GET | `/_matrix/client/v1/friends/blocked/{user_id}` | 检查用户是否被拉黑 |
| DELETE | `/_matrix/client/v1/friends/blocked/{user_id}` | 将用户从黑名单移除 |
| POST | `/_matrix/client/v1/friends/blocked/batch` | 批量拉黑用户 |

### 好友在线状态 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/v1/friends/online` | 获取好友在线状态 |
| POST | `/_matrix/client/v1/friends/online` | 批量获取好友在线状态 |
| GET | `/_matrix/client/v1/presence/{user_id}` | 获取用户在线状态 |
| PUT | `/_matrix/client/v1/presence/{user_id}` | 更新自己的在线状态 |

### 好友搜索 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/v1/friends/search` | 搜索好友 |
| GET | `/_matrix/client/v1/friends/list` | 获取好友列表 |
| GET | `/_matrix/client/v1/friends/stats` | 获取好友统计 |

---

## 私聊 API

### 私聊管理 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/v1/chatrooms` | 获取聊天室列表 |
| POST | `/_matrix/client/v1/chatrooms` | 创建私聊/聊天室 |
| GET | `/_matrix/client/v1/chatrooms/{room_id}` | 获取聊天室详情 |
| POST | `/_matrix/client/v1/chatrooms/{room_id}/leave` | 离开聊天室 |
| POST | `/_matrix/client/v1/chatrooms/{room_id}/mute` | 设置聊天室静音 |
| GET | `/_matrix/client/v1/chatrooms/{room_id}/messages` | 获取消息列表 |
| POST | `/_matrix/client/v1/chatrooms/{room_id}/messages` | 发送消息 |
| DELETE | `/_matrix/client/v1/chatrooms/{room_id}/messages/{message_id}` | 删除消息 |
| POST | `/_matrix/client/v1/chatrooms/{room_id}/read` | 标记消息已读 |
| GET | `/_matrix/client/v1/chatrooms/{room_id}/files` | 获取文件列表 |
| GET | `/_matrix/client/v1/chatrooms/{room_id}/voice` | 获取语音消息列表 |
| GET | `/_matrix/client/v1/chatrooms/{room_id}/statistics` | 获取会话统计 |

### 私聊 REST API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/v1/unread-count` | 获取未读消息数 |
| GET | `/_matrix/client/v1/messages/search` | 搜索消息 |

### 直接聊天 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_matrix/client/v1/direct_chats` | 获取直接聊天列表 |
| POST | `/_matrix/client/v1/direct_chats` | 创建直接聊天 |

---

## 管理员 API v1

### 管理员客户端 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/client/enhanced/admin/profile` | 获取管理员信息 |
| GET | `/_synapse/client/enhanced/admin/dashboard` | 获取仪表盘统计数据 |
| GET | `/_synapse/client/enhanced/admin/statistics` | 获取系统统计数据 |
| GET | `/_synapse/client/enhanced/admin/health` | 获取系统健康状态 |
| GET | `/_synapse/client/enhanced/admin/users` | 获取用户列表 |
| GET | `/_synapse/client/enhanced/admin/users/{user_id}` | 获取用户详情 |
| GET | `/_synapse/client/enhanced/admin/admins` | 获取管理员列表 |
| GET | `/_synapse/client/enhanced/admin/admins/{user_id}` | 获取管理员详情 |
| GET | `/_synapse/client/enhanced/admin/rooms` | 获取房间列表 |
| GET | `/_synapse/client/enhanced/admin/messages` | 获取消息列表 |
| GET | `/_synapse/client/enhanced/admin/config` | 获取系统配置 |
| PUT | `/_synapse/client/enhanced/admin/config` | 更新系统配置 |
| GET | `/_synapse/client/enhanced/admin/audit-logs` | 获取审计日志 |

---

## 管理员 API v2

### 基础管理 API

| 方法 | 路径 | 描述 |
|------|------|------|
| GET | `/_synapse/admin/v2/dashboard` | 获取仪表盘统计数据 |
| GET | `/_synapse/admin/v2/statistics` | 获取系统统计数据 |
| GET | `/_synapse/admin/v2/admins` | 获取管理员列表 |
| POST | `/_synapse/admin/v2/admins/{user_id}/{action}` | 执行管理员操作 (promote/demote/revoke) |
| GET | `/_synapse/admin/v2/audit` | 获取审计日志 |
| GET | `/_synapse/admin/v2/export` | 导出数据 |
| GET | `/_synapse/admin/v2/config` | 获取系统配置 |
| PUT | `/_synapse/admin/v2/config` | 更新系统配置 |

### 私聊管理 API v2

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_synapse/admin/v2/private_chat/create` | 创建私聊房间 |
| GET | `/_synapse/admin/v2/private_chat/list` | 获取私聊房间列表 |
| GET | `/_synapse/admin/v2/private_chat/{room_id}` | 获取私聊房间详情 |
| POST | `/_synapse/admin/v2/private_chat/{room_id}/delete` | 删除私聊房间 |
| PUT | `/_synapse/admin/v2/private_chat/{room_id}/ttl` | 设置房间 TTL |
| GET | `/_synapse/admin/v2/private_chat/statistics` | 获取私聊统计数据 |

---

## 语音消息 API v2

| 方法 | 路径 | 描述 |
|------|------|------|
| POST | `/_synapse/admin/v2/voice/upload` | 上传语音消息 |
| POST | `/_synapse/admin/v2/voice/convert` | 转换语音消息格式 |
| POST | `/_synapse/admin/v2/voice/optimize` | 优化语音消息质量 |
| GET | `/_synapse/admin/v2/voice/info/{media_id}` | 获取语音消息信息 |
| GET | `/_synapse/admin/v2/voice/statistics` | 获取语音消息统计数据 |

---

## API 认证

所有 API 端点都需要有效的访问令牌（access_token）。令牌应通过以下方式之一提供：

1. **Authorization Header**: `Authorization: Bearer <access_token>`
2. **Cookie**: `access_token=<access_token>`

管理员 API v1 和 v2 还需要请求者具有管理员权限。

---

## 响应格式

### 成功响应

```json
{
  "status": "ok",
  "data": { ... },
  "meta": { ... }
}
```

### 错误响应

```json
{
  "errcode": "M_UNKNOWN",
  "error": "错误描述"
}
```

---

## 分页

支持分页的 API 使用以下参数：

- `page`: 页码（从 1 开始）
- `limit` 或 `page_size`: 每页数量
- `cursor`: 分页游标（用于游标分页）

---

## 缓存

部分 API 使用缓存以提高性能，缓存时间（TTL）通常在 5-300 秒之间。
