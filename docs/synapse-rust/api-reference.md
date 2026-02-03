# Synapse Rust API 参考文档

> **版本**：1.0.0  
> **生成日期**：2026-02-03  
> **项目**：Synapse Rust Matrix Server  
> **文档目的**：系统性梳理所有API端点，为后续分模块测试提供基础

---

## 测试环境配置

本文档中的API测试使用以下预配置的测试账号和测试房间。

### 测试账号

| 账号类型 | 用户ID | 密码 | 设备ID | Access Token | 说明 |
|---------|--------|------|--------|-------------|------|
| 管理员 | `@admin:matrix.cjystx.top` | `Wzc9890951!` | `q5Gm1PW0Kr_j0rliSTRWSw` | `eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOnRydWUsImV4cCI6MTc3MDEzMjMyMSwiaWF0IjoxNzcwMTI4NzIxLCJkZXZpY2VfaWQiOiJxNUdtMVBXMEtyX2owcmxpU1RSV1N3In0.gAHe9KBK5nPA6LQ7V9zt2UdpTQHp-9CuJC47uWj6FGI` | 系统管理员 |
| 普通用户1 | `@testuser1:matrix.cjystx.top` | `TestUser123456!` | `2XLNKR5uUGJpDrYL23AmZA` | `eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MDEzMTE5NywiaWF0IjoxNzcwMTI3NTk3LCJkZXZpY2VfaWQiOiIyWExOS1I1dVVHSnBEcllMMjNBbVpBIn0.QAXxISfR527_g4Leo_Ipi7iBUcec88bgwcKfN0UEq2o` | 测试用户1 |
| 普通用户2 | `@testuser2:matrix.cjystx.top` | `TestUser123456!` | `FlXY3dpzhS55wghIeJClvA` | `eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIyOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjI6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MDEzMTIxNSwiaWF0IjoxNzcwMTI3NjE1LCJkZXZpY2VfaWQiOiJGbFhZM2RwemhTNTV3Z2hJZUpDbHZBIn0.ftvbXHZojSujQZ6gtyNuAjD8e1waJebLLyQLiz6OA4c` | 测试用户2 |

### 测试房间

| 房间名称 | 房间ID | 类型 | 主题 | 创建者 | 说明 |
|---------|--------|------|------|--------|------|
| Test Room 1 | `!BfUBQVVQfR0EQUmS9kwF_EQ2:matrix.cjystx.top` | 公开 | Test room for API testing | `@testuser1:matrix.cjystx.top` | 公开测试房间 |
| Test Room 2 | `!pdsb0b_OqRVJazC6JYW1CZRQ:matrix.cjystx.top` | 私有 | Private test room | `@testuser1:matrix.cjystx.top` | 私有测试房间 |

### 使用说明

1. **认证方式**：在请求头中添加 `Authorization: Bearer {access_token}`
2. **Token有效期**：Access Token 有效期为 1 小时（3600秒）
3. **Token刷新**：使用 `refresh_token` 调用 `/_matrix/client/r0/refresh` 端点刷新
4. **服务器地址**：`http://localhost:8008` 或 `http://matrix.cjystx.top:8008`
5. **测试环境**：Docker Compose 部署，包含 PostgreSQL、Redis、Nginx 和 Synapse Rust

### 快速测试示例

```bash
# 使用 testuser1 登录
curl -X POST http://localhost:8008/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{"username":"testuser1","password":"TestUser123456!"}'

# 获取房间列表
curl -X GET http://localhost:8008/_matrix/client/r0/user/@testuser1:matrix.cjystx.top/rooms \
  -H "Authorization: Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MDEzMTE5NywiaWF0IjoxNzcwMTI3NTk3LCJkZXZpY2VfaWQiOiIyWExOS1I1dVVHSnBEcllMMjNBbVpBIn0.QAXxISfR527_g4Leo_Ipi7iBUcec88bgwcKfN0UEq2o"

# 发送消息到房间
curl -X PUT "http://localhost:8008/_matrix/client/r0/rooms/!BfUBQVVQfR0EQUmS9kwF_EQ2:matrix.cjystx.top/send/m.room.message/txn123" \
  -H "Authorization: Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAdGVzdHVzZXIxOm1hdHJpeC5janlzdHgudG9wIiwidXNlcl9pZCI6IkB0ZXN0dXNlcjE6bWF0cml4LmNqeXN0eC50b3AiLCJhZG1pbiI6ZmFsc2UsImV4cCI6MTc3MDEzMTE5NywiaWF0IjoxNzcwMTI3NTk3LCJkZXZpY2VfaWQiOiIyWExOS1I1dVVHSnBEcllMMjNBbVpBIn0.QAXxISfR527_g4Leo_Ipi7iBUcec88bgwcKfN0UEq2o" \
  -H "Content-Type: application/json" \
  -d '{"msgtype":"m.text","body":"Hello, this is a test message!"}'

# 使用管理员账号获取服务器状态
curl -X GET http://localhost:8008/_synapse/admin/v1/status \
  -H "Authorization: Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOnRydWUsImV4cCI6MTc3MDEzMjMyMSwiaWF0IjoxNzcwMTI4NzIxLCJkZXZpY2VfaWQiOiJxNUdtMVBXMEtyX2owcmxpU1RSV1N3In0.gAHe9KBK5nPA6LQ7V9zt2UdpTQHp-9CuJC47uWj6FGI"

# 使用管理员账号获取用户列表
curl -X GET "http://localhost:8008/_synapse/admin/v1/users?limit=10&offset=0" \
  -H "Authorization: Bearer eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9.eyJzdWIiOiJAYWRtaW46bWF0cml4LmNqeXN0eC50b3AiLCJ1c2VyX2lkIjoiQGFkbWluOm1hdHJpeC5janlzdHgudG9wIiwiYWRtaW4iOnRydWUsImV4cCI6MTc3MDEzMjMyMSwiaWF0IjoxNzcwMTI4NzIxLCJkZXZpY2VfaWQiOiJxNUdtMVBXMEtyX2owcmxpU1RSV1N3In0.gAHe9KBK5nPA6LQ7V9zt2UdpTQHp-9CuJC47uWj6FGI"
```

---

## 文档说明

本文档按功能模块分类，详细列出Synapse Rust项目中的所有API端点。每个API条目包含：
- **请求路径**：完整的API端点路径
- **请求方法**：HTTP方法（GET/POST/PUT/DELETE）
- **请求参数**：参数名称、数据类型、是否必填、默认值
- **响应格式**：响应数据结构
- **状态码说明**：成功/失败状态码及其含义
- **功能描述**：API的功能说明

---

## 目录

1. [核心客户端API](#一核心客户端api)
2. [管理员API](#二管理员api)
3. [联邦通信API](#三联邦通信api)
4. [端到端加密API](#四端到端加密api)
5. [语音消息API](#五语音消息api)
6. [好友系统API](#六好友系统api)
7. [媒体文件API](#七媒体文件api)
8. [私聊API](#八私聊api)
9. [密钥备份API](#九密钥备份api)
10. [认证与错误处理](#十认证与错误处理)

---

## 一、核心客户端API

### 1.1 基础信息与认证

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/` | GET | - | `{"msg": "Synapse Rust Matrix Server", "version": "0.1.0"}` | 200 | 服务器根路径信息 |
| `/health` | GET | - | 健康检查状态JSON | 200 | 系统健康检查 |
| `/_matrix/client/versions` | GET | - | `{"versions": ["r0.0.1", "r0.1.0", "r0.2.0", "r0.3.0", "r0.4.0", "r0.5.0", "r0.6.0"], "unstable_features": {...}}` | 200 | 获取支持的客户端版本 |
| `/_matrix/client/r0/register` | POST | `username` (str, 必填), `password` (str, 必填), `displayname` (str, 可选), `auth.type` (str, 可选) | `{"user_id": "@username:server", "access_token": "..."}` | 200 | 用户注册 |
| `/_matrix/client/r0/register/available` | GET | `username` (str, 必填, Query) | `{"available": true/false, "username": "..."}` | 200 | 检查用户名可用性 |
| `/_matrix/client/r0/login` | POST | `user`/`username` (str, 必填), `password` (str, 必填), `device_id` (str, 可选), `initial_display_name` (str, 可选) | `{"access_token": "...", "refresh_token": "...", "device_id": "...", "user_id": "..."}` | 200 | 用户登录 |
| `/_matrix/client/r0/logout` | POST | 需要认证 | `{}` | 200 | 登出当前设备 |
| `/_matrix/client/r0/logout/all` | POST | 需要认证 | `{}` | 200 | 登出所有设备 |
| `/_matrix/client/r0/refresh` | POST | `refresh_token` (str, 必填) | `{"access_token": "...", "refresh_token": "...", "device_id": "..."}` | 200 | 刷新访问令牌 |
| `/_matrix/client/r0/account/whoami` | GET | 需要认证 | `{"user_id": "@username:server", "displayname": "...", "avatar_url": "...", "admin": bool}` | 200 | 获取当前用户信息 |

### 1.2 用户资料管理

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_matrix/client/r0/account/profile/{user_id}` | GET | `user_id` (str, Path) | 用户资料JSON | 200 | 获取用户资料 |
| `/_matrix/client/r0/account/profile/{user_id}/displayname` | PUT | `user_id` (str, Path), `displayname` (str, 必填) | `{}` | 200 | 更新显示名称 |
| `/_matrix/client/r0/account/profile/{user_id}/avatar_url` | PUT | `user_id` (str, Path), `avatar_url` (str, 必填) | `{}` | 200 | 更新头像URL |
| `/_matrix/client/r0/account/password` | POST | `new_password` (str, 必填, 8-128字符) | `{}` | 200 | 修改密码 |
| `/_matrix/client/r0/account/deactivate` | POST | 需要认证 | `{}` | 200 | 停用账户 |

### 1.3 房间管理

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_matrix/client/r0/sync` | GET | `timeout` (u64, 可选, 默认30000), `full_state` (bool, 可选), `set_presence` (str, 可选) | 同步响应JSON | 200 | 客户端同步 |
| `/_matrix/client/r0/rooms/{room_id}/messages` | GET | `room_id` (str, Path), `from` (i64, 可选), `limit` (u64, 可选, 默认10), `dir` (str, 可选) | 消息列表JSON | 200 | 获取房间消息 |
| `/_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}` | PUT | `room_id`, `event_type`, `txn_id` (str, Path), `msgtype` (str, 可选), `body` (str, 必填) | `{"event_id": "..."}` | 200 | 发送消息 |
| `/_matrix/client/r0/rooms/{room_id}/join` | POST | `room_id` (str, Path), 需要认证 | `{}` | 200 | 加入房间 |
| `/_matrix/client/r0/rooms/{room_id}/leave` | POST | `room_id` (str, Path), 需要认证 | `{}` | 200 | 离开房间 |
| `/_matrix/client/r0/rooms/{room_id}/members` | GET | `room_id` (str, Path), 需要认证 | 成员列表JSON | 200 | 获取房间成员 |
| `/_matrix/client/r0/createRoom` | POST | `visibility` (str, 可选), `room_alias_name` (str, 可选), `name` (str, 可选), `topic` (str, 可选), `invite` (array, 可选), `preset` (str, 可选), 需要认证 | `{"room_id": "..."}` | 200 | 创建房间 |
| `/_matrix/client/r0/directory/room/{room_id}` | GET | `room_id` (str, Path) | 房间信息JSON | 200 | 获取房间目录信息 |
| `/_matrix/client/r0/directory/room/{room_id}` | DELETE | `room_id` (str, Path), 需要管理员权限 | `{}` | 200 | 从目录删除房间 |
| `/_matrix/client/r0/publicRooms` | GET | `limit` (u64, 可选, 默认10), `since` (str, 可选) | 公共房间列表 | 200 | 获取公共房间列表 |
| `/_matrix/client/r0/publicRooms` | POST | 同createRoom参数 | `{"room_id": "..."}` | 200 | 创建公共房间 |
| `/_matrix/client/r0/user/{user_id}/rooms` | GET | `user_id` (str, Path), 需要认证 | `{"joined_rooms": [...]}` | 200 | 获取用户加入的房间 |

### 1.4 房间状态与权限

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_matrix/client/r0/rooms/{room_id}/state` | GET | `room_id` (str, Path) | `{"state": [...]}` | 200 | 获取房间所有状态 |
| `/_matrix/client/r0/rooms/{room_id}/state/{event_type}` | GET | `room_id`, `event_type` (str, Path) | `{"events": [...]}` | 200 | 按类型获取状态事件 |
| `/_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}` | GET | `room_id`, `event_type`, `state_key` (str, Path) | 状态事件JSON | 200 | 获取特定状态事件 |
| `/_matrix/client/r0/rooms/{room_id}/redact/{event_id}` | PUT | `room_id`, `event_id` (str, Path), `reason` (str, 可选), 需要认证 | `{"event_id": "..."}` | 200 | 撤回事件 |
| `/_matrix/client/r0/rooms/{room_id}/kick` | POST | `room_id` (str, Path), `user_id` (str, 必填), `reason` (str, 可选), 需要认证 | `{}` | 200 | 踢出用户 |
| `/_matrix/client/r0/rooms/{room_id}/ban` | POST | `room_id` (str, Path), `user_id` (str, 必填), `reason` (str, 可选), 需要认证 | `{}` | 200 | 封禁用户 |
| `/_matrix/client/r0/rooms/{room_id}/unban` | POST | `room_id` (str, Path), `user_id` (str, 必填), 需要认证 | `{}` | 200 | 解封用户 |

### 1.5 设备管理

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_matrix/client/r0/devices` | GET | 需要认证 | `{"devices": [...]}` | 200 | 获取设备列表 |
| `/_matrix/client/r0/devices/{device_id}` | GET | `device_id` (str, Path), 需要认证 | 设备信息JSON | 200 | 获取设备详情 |
| `/_matrix/client/r0/devices/{device_id}` | PUT | `device_id` (str, Path), `display_name` (str, 必填), 需要认证 | `{}` | 200 | 更新设备显示名称 |
| `/_matrix/client/r0/devices/{device_id}` | DELETE | `device_id` (str, Path), 需要认证 | `{}` | 200 | 删除设备 |
| `/_matrix/client/r0/delete_devices` | POST | `devices` (array, 必填), 需要认证 | `{}` | 200 | 批量删除设备 |

### 1.6 在线状态

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_matrix/client/r0/presence/{user_id}/status` | GET | `user_id` (str, Path), 需要认证 | `{"presence": "...", "status_msg": "..."}` | 200 | 获取用户在线状态 |
| `/_matrix/client/r0/presence/{user_id}/status` | PUT | `user_id` (str, Path), `presence` (str, 必填), `status_msg` (str, 可选), 需要认证 | `{}` | 200 | 设置用户在线状态 |

---

## 二、管理员API

### 2.1 服务器管理

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_synapse/admin/v1/server_version` | GET | 需要管理员权限 | `{"version": "1.0.0", "python_version": "3.9.0"}` | 200 | 获取服务器版本 |
| `/_synapse/admin/v1/status` | GET | 需要管理员权限 | `{"status": "running", "version": "...", "users": N, "rooms": N, "uptime": 0}` | 200 | 获取服务器状态 |

### 2.2 用户管理

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_synapse/admin/v1/users` | GET | `limit` (i64, 可选, 默认100, 最大1000), `offset` (i64, 可选, 默认0), 需要管理员权限 | `{"users": [...], "total": N}` | 200 | 获取用户列表（分页） |
| `/_synapse/admin/v1/users/{user_id}` | GET | `user_id` (str, Path), 需要管理员权限 | 用户详情JSON | 200 | 获取用户详情 |
| `/_synapse/admin/v1/users/{user_id}/admin` | PUT | `user_id` (str, Path), `admin` (bool, 必填), 需要管理员权限 | `{"success": true}` | 200 | 设置管理员权限 |
| `/_synapse/admin/v1/users/{user_id}/deactivate` | POST | `user_id` (str, Path), 需要管理员权限 | `{"id_server_unbind_result": "success"}` | 200 | 停用用户 |
| `/_synapse/admin/v1/users/{user_id}/rooms` | GET | `user_id` (str, Path), 需要管理员权限 | `{"rooms": [...]}` | 200 | 获取用户的房间列表 |

### 2.3 房间管理

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_synapse/admin/v1/rooms` | GET | `limit` (i64, 可选, 默认100, 最大1000), `offset` (i64, 可选, 默认0), 需要管理员权限 | `{"rooms": [...], "total": N}` | 200 | 获取房间列表（分页） |
| `/_synapse/admin/v1/rooms/{room_id}` | GET | `room_id` (str, Path), 需要管理员权限 | 房间详情JSON | 200 | 获取房间详情 |
| `/_synapse/admin/v1/rooms/{room_id}/delete` | POST | `room_id` (str, Path), 需要管理员权限 | `{"delete_id": "..."}` | 200 | 删除房间 |
| `/_synapse/admin/v1/purge_history` | POST | `room_id` (str, 必填), `purge_up_to_ts` (i64, 可选, 默认30天前), 需要管理员权限 | `{"success": true, "deleted_events": N}` | 200 | 清理房间历史 |
| `/_synapse/admin/v1/shutdown_room` | POST | `room_id` (str, 必填), 需要管理员权限 | `{"kicked_users": [], "failed_to_kick_users": [], "closed_room": true}` | 200 | 关闭房间 |

### 2.4 安全管理

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_synapse/admin/v1/security/events` | GET | 需要管理员权限 | `{"events": [...], "total": N}` | 200 | 获取安全事件日志 |
| `/_synapse/admin/v1/security/ip/blocks` | GET | 需要管理员权限 | `{"blocked_ips": [...], "total": N}` | 200 | 获取IP封禁列表 |
| `/_synapse/admin/v1/security/ip/block` | POST | `ip_address` (str, 必填), `reason` (str, 可选), `expires_at` (str, 可选, RFC3339格式), 需要管理员权限 | `{"success": true, "ip_address": "..."}` | 200 | 封禁IP地址 |
| `/_synapse/admin/v1/security/ip/unblock` | POST | `ip_address` (str, 必填), 需要管理员权限 | `{"success": bool, "ip_address": "...", "message": "..."}` | 200 | 解封IP地址 |
| `/_synapse/admin/v1/security/ip/reputation/{ip}` | GET | `ip` (str, Path), 需要管理员权限 | IP信誉信息JSON | 200 | 获取IP信誉评分 |

### 2.5 管理员注册

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_synapse/admin/v1/register/nonce` | GET | 从Header获取X-Forwarded-For | `{"nonce": "..."}` | 200 | 获取管理员注册nonce（限流：3次/IP） |
| `/_synapse/admin/v1/register` | POST | `username` (str, 必填), `password` (str, 必填), `nonce` (str, 必填), `hmac` (str, 必填), `displayname` (str, 可选) | 注册响应JSON | 200 | 管理员注册（限流：2次/IP） |

---

## 三、联邦通信API

### 3.1 公开端点（无需认证）

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_matrix/federation/v2/server` | GET | - | 服务器密钥JSON | 200 | 获取服务器签名密钥 |
| `/_matrix/key/v2/server` | GET | - | 服务器密钥JSON | 200 | 获取服务器密钥（v1兼容） |
| `/_matrix/federation/v2/query/{server_name}/{key_id}` | GET | `server_name`, `key_id` (str, Path) | 密钥查询结果 | 200 | 查询服务器密钥 |
| `/_matrix/key/v2/query/{server_name}/{key_id}` | GET | `server_name`, `key_id` (str, Path) | 密钥查询结果 | 200 | 查询服务器密钥（v1兼容） |
| `/_matrix/federation/v1/version` | GET | - | `{"version": "...", "server": {...}}` | 200 | 获取联邦版本信息 |
| `/_matrix/federation/v1` | GET | - | 联邦发现信息 | 200 | 联邦服务发现 |
| `/_matrix/federation/v1/publicRooms` | GET | `limit` (i64, 可选, 默认10), `since` (str, 可选) | `{"chunk": [...], "next_batch": null}` | 200 | 获取公共房间列表 |

### 3.2 保护端点（需要联邦认证）

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_matrix/federation/v1/send/{txn_id}` | PUT | `txn_id` (str, Path), `origin` (str, 必填), `pdus` (array, 必填) | `{"results": [...]}` | 200 | 发送事务（批量PDU） |
| `/_matrix/federation/v1/make_join/{room_id}/{user_id}` | GET | `room_id`, `user_id` (str, Path) | `{"room_version": "...", "auth_events": [...], "event": {...}}` | 200 | 生成加入事件模板 |
| `/_matrix/federation/v1/make_leave/{room_id}/{user_id}` | GET | `room_id`, `user_id` (str, Path) | `{"room_version": "...", "auth_events": [...], "event": {...}}` | 200 | 生成离开事件模板 |
| `/_matrix/federation/v1/send_join/{room_id}/{event_id}` | PUT | `room_id`, `event_id` (str, Path), `origin` (str, 必填), `event` (object, 必填) | `{"event_id": "..."}` | 200 | 发送加入事件 |
| `/_matrix/federation/v1/send_leave/{room_id}/{event_id}` | PUT | `room_id`, `event_id` (str, Path), `origin` (str, 必填), `event` (object, 必填) | `{"event_id": "..."}` | 200 | 发送离开事件 |
| `/_matrix/federation/v1/invite/{room_id}/{event_id}` | PUT | `room_id`, `event_id` (str, Path), `origin` (str, 必填) | `{"event_id": "..."}` | 200 | 发送邀请事件 |
| `/_matrix/federation/v1/get_missing_events/{room_id}` | POST | `room_id` (str, Path), `earliest_events` (array, 必填), `latest_events` (array, 必填), `limit` (i64, 可选, 默认10) | `{"events": [...]}` | 200 | 获取缺失事件 |
| `/_matrix/federation/v1/get_event_auth/{room_id}/{event_id}` | GET | `room_id`, `event_id` (str, Path) | `{"auth_chain": [...]}` | 200 | 获取事件授权链 |
| `/_matrix/federation/v1/state/{room_id}` | GET | `room_id` (str, Path) | `{"state": [...]}` | 200 | 获取房间状态 |
| `/_matrix/federation/v1/event/{event_id}` | GET | `event_id` (str, Path) | 事件JSON | 200 | 获取单个事件 |
| `/_matrix/federation/v1/state_ids/{room_id}` | GET | `room_id` (str, Path) | `{"state_ids": [...]}` | 200 | 获取房间状态ID列表 |
| `/_matrix/federation/v1/query/directory/room/{room_id}` | GET | `room_id` (str, Path) | 房间目录信息JSON | 200 | 查询房间目录 |
| `/_matrix/federation/v1/query/profile/{user_id}` | GET | `user_id` (str, Path) | 用户资料JSON | 200 | 查询用户资料 |
| `/_matrix/federation/v1/backfill/{room_id}` | GET | `room_id` (str, Path), `limit` (i64, 可选, 默认10), `v` (array, 必填) | `{"origin": "...", "pdus": [...], "limit": N}` | 200 | 回填历史消息 |
| `/_matrix/federation/v1/keys/claim` | POST | KeyClaimRequest格式 | `{"one_time_keys": {...}, "failures": {...}}` | 200 | 声明一次性密钥 |
| `/_matrix/federation/v1/keys/upload` | POST | KeyUploadRequest格式 | `{"one_time_key_counts": {...}}` | 200 | 上传设备密钥 |
| `/_matrix/federation/v2/key/clone` | POST | - | `{"success": true}` | 200 | 克隆密钥 |
| `/_matrix/federation/v2/user/keys/query` | POST | 用户密钥查询请求 | `{"device_keys": {...}}` | 200 | 查询用户密钥 |

---

## 四、端到端加密API

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_matrix/client/r0/keys/upload` | POST | `device_keys` (object, 可选), `one_time_keys` (object, 可选), 需要认证 | `{"one_time_key_counts": {...}}` | 200 | 上传设备密钥 |
| `/_matrix/client/r0/keys/query` | POST | KeyQueryRequest格式, 需要认证 | `{"device_keys": {...}, "failures": {...}}` | 200 | 查询设备密钥 |
| `/_matrix/client/r0/keys/claim` | POST | KeyClaimRequest格式, 需要认证 | `{"one_time_keys": {...}, "failures": {...}}` | 200 | 声明一次性密钥 |
| `/_matrix/client/r0/keys/changes` | GET | `from` (str, 可选, 默认"0"), `to` (str, 可选, 默认""), 需要认证 | `{"changed": [...], "left": [...]}` | 200 | 获取密钥变更列表 |
| `/_matrix/client/r0/rooms/{room_id}/keys/distribution` | GET | `room_id` (str, Path), 需要认证 | `{"room_id": "...", "algorithm": "...", "session_id": "...", "session_key": "..."}` | 200 | 获取房间密钥分发信息 |
| `/_matrix/client/r0/sendToDevice/{event_type}/{transaction_id}` | PUT | `event_type`, `transaction_id` (str, Path), `messages` (object, 必填), 需要认证 | `{"txn_id": "..."}` | 200 | 发送设备到设备消息 |

---

## 五、语音消息API

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_matrix/client/r0/voice/upload` | POST | `content` (str, Base64编码, 必填), `content_type` (str, 可选, 默认"audio/ogg"), `duration_ms` (i32, 可选), `room_id` (str, 可选), `session_id` (str, 可选), 需要认证 | 语音消息上传结果 | 200 | 上传语音消息 |
| `/_matrix/client/r0/voice/stats` | GET | 需要认证 | 用户语音统计JSON | 200 | 获取当前用户语音统计 |
| `/_matrix/client/r0/voice/{message_id}` | GET | `message_id` (str, Path) | `{"message_id": "...", "content": "...", "content_type": "...", "size": N}` | 200 | 获取语音消息 |
| `/_matrix/client/r0/voice/{message_id}` | DELETE | `message_id` (str, Path), 需要认证 | `{"deleted": bool, "message_id": "..."}` | 200 | 删除语音消息 |
| `/_matrix/client/r0/voice/user/{user_id}` | GET | `user_id` (str, Path) | 用户语音消息列表 | 200 | 获取用户语音消息（限制50条） |
| `/_matrix/client/r0/voice/room/{room_id}` | GET | `room_id` (str, Path) | 房间语音消息列表 | 200 | 获取房间语音消息（限制50条） |
| `/_matrix/client/r0/voice/user/{user_id}/stats` | GET | `user_id` (str, Path) | 用户语音统计JSON | 200 | 获取指定用户语音统计 |

---

## 六、好友系统API

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_synapse/enhanced/friends/search` | GET | `query`/`search_term` (str, 必填), `limit` (i64, 可选, 默认20, 最大100), 需要认证 | `{"results": [...], "count": N}` | 200 | 搜索用户 |
| `/_synapse/enhanced/friends` | GET | 需要认证 | `{"friends": [...], "count": N}` | 200 | 获取好友列表 |
| `/_synapse/enhanced/friend/request` | POST | `user_id` (str, 必填), `message` (str, 可选), 需要认证 | `{"request_id": N, "status": "pending"}` | 200 | 发送好友请求 |
| `/_synapse/enhanced/friend/requests` | GET | 需要认证 | `{"requests": [...], "count": N}` | 200 | 获取好友请求列表 |
| `/_synapse/enhanced/friend/request/{request_id}/accept` | POST | `request_id` (i64, Path), 需要认证 | `{"status": "accepted"}` | 200 | 接受好友请求 |
| `/_synapse/enhanced/friend/request/{request_id}/decline` | POST | `request_id` (i64, Path), 需要认证 | `{"status": "declined"}` | 200 | 拒绝好友请求 |
| `/_synapse/enhanced/friend/blocks/{user_id}` | GET | `user_id` (str, Path), 需要认证 | `{"blocked_users": [...]}` | 200 | 获取封禁用户列表 |
| `/_synapse/enhanced/friend/blocks/{user_id}` | POST | `user_id` (str, Path), `user_id` (str, 必填, Body), `reason` (str, 可选), 需要认证 | `{"status": "blocked"}` | 200 | 封禁用户 |
| `/_synapse/enhanced/friend/blocks/{user_id}/{blocked_user_id}` | DELETE | `user_id`, `blocked_user_id` (str, Path), 需要认证 | `{"status": "unblocked"}` | 200 | 解封用户 |
| `/_synapse/enhanced/friend/categories/{user_id}` | GET | `user_id` (str, Path), 需要认证 | `{"categories": [...]}` | 200 | 获取好友分类 |
| `/_synapse/enhanced/friend/categories/{user_id}` | POST | `user_id` (str, Path), `name` (str, 必填), `color` (str, 可选, 默认"#000000"), 需要认证 | `{"category_id": N}` | 200 | 创建好友分类 |
| `/_synapse/enhanced/friend/categories/{user_id}/{category_name}` | PUT | `user_id`, `category_name` (str, Path), `name` (str, 可选), `color` (str, 可选), 需要认证 | `{"status": "updated", "category_name": "..."}` | 200 | 更新好友分类 |
| `/_synapse/enhanced/friend/categories/{user_id}/{category_name}` | DELETE | `user_id`, `category_name` (str, Path), 需要认证 | `{"status": "deleted", "category_name": "..."}` | 200 | 删除好友分类 |
| `/_synapse/enhanced/friend/recommendations/{user_id}` | GET | `user_id` (str, Path), 需要认证 | `{"recommendations": [...], "count": N}` | 200 | 获取好友推荐（基于共同房间，限制10条） |

---

## 七、媒体文件API

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_matrix/media/v1/config` | GET | - | `{"m.upload.size": 52428800}` | 200 | 获取媒体配置（最大50MB） |
| `/_matrix/media/v1/upload` | POST | `content` (array, 必填), `content_type` (str, 可选, 默认"application/octet-stream"), `filename` (str, 可选), 需要认证 | 上传响应JSON | 200 | 上传媒体文件（v1） |
| `/_matrix/media/v3/upload` | POST | 同v1 | 上传响应JSON | 200 | 上传媒体文件（v3） |
| `/_matrix/media/v3/upload/{server_name}/{media_id}` | POST | `server_name`, `media_id` (str, Path), `content` (array, 必填), `content_type` (str, 可选), `filename` (str, 可选), 需要认证 | 上传响应JSON | 200 | 上传媒体文件（带ID） |
| `/_matrix/media/v1/download/{server_name}/{media_id}` | GET | `server_name`, `media_id` (str, Path) | 文件二进制内容 | 200 | 下载媒体文件（v1） |
| `/_matrix/media/r1/download/{server_name}/{media_id}` | GET | `server_name`, `media_id` (str, Path) | 文件二进制内容 | 200 | 下载媒体文件（r1） |
| `/_matrix/media/v3/download/{server_name}/{media_id}` | GET | `server_name`, `media_id` (str, Path) | 文件二进制内容 | 200 | 下载媒体文件（v3） |
| `/_matrix/media/v3/thumbnail/{server_name}/{media_id}` | GET | `server_name`, `media_id` (str, Path), `width` (u32, 可选, 默认800), `height` (u32, 可选, 默认600), `method` (str, 可选, 默认"scale") | 缩略图二进制内容 | 200 | 获取媒体缩略图 |

**支持的文件类型**：
- 图片：jpg/jpeg, png, gif, webp, svg
- 视频：mp4, webm
- 音频：mp3, wav, ogg
- 文档：pdf
- 其他：application/octet-stream

---

## 八、私聊API

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_matrix/client/r0/dm` | GET | 需要认证 | DM房间列表 | 200 | 获取所有私聊房间 |
| `/_matrix/client/r0/createDM` | POST | `user_id`/`other_user_id` (str, 必填), 需要认证 | `{"room_id": "..."}` | 200 | 创建私聊房间 |
| `/_matrix/client/r0/rooms/{room_id}/dm` | GET | `room_id` (str, Path), 需要认证 | DM房间详情JSON | 200 | 获取DM房间详情 |
| `/_matrix/client/r0/rooms/{room_id}/unread` | GET | `room_id` (str, Path), 需要认证 | `{"room_id": "...", "notification_count": N, "highlight_count": N}` | 200 | 获取未读通知数 |
| `/_synapse/enhanced/private/sessions` | GET | 需要认证 | 私聊会话列表 | 200 | 获取私聊会话列表 |
| `/_synapse/enhanced/private/sessions` | POST | `other_user_id` (str, 必填), 需要认证 | 会话创建结果 | 200 | 创建私聊会话 |
| `/_synapse/enhanced/private/sessions/{session_id}` | GET | `session_id` (str, Path), 需要认证 | 会话详情JSON | 200 | 获取会话详情 |
| `/_synapse/enhanced/private/sessions/{session_id}` | DELETE | `session_id` (str, Path), 需要认证 | `{}` | 200 | 删除会话 |
| `/_synapse/enhanced/private/sessions/{session_id}/messages` | GET | `session_id` (str, Path), 需要认证 | 会话消息列表（限制50条） | 200 | 获取会话消息 |
| `/_synapse/enhanced/private/sessions/{session_id}/messages` | POST | `session_id` (str, Path), `message_type` (str, 可选, 默认"m.text"), `content` (object, 可选), `encrypted_content` (str, 可选), 需要认证 | 消息发送结果 | 200 | 发送会话消息 |
| `/_synapse/enhanced/private/messages/{message_id}` | DELETE | `message_id` (str, Path), 需要认证 | `{}` | 200 | 删除消息 |
| `/_synapse/enhanced/private/messages/{message_id}/read` | POST | `message_id` (str, Path), 需要认证 | `{}` | 200 | 标记消息已读 |
| `/_synapse/enhanced/private/unread-count` | GET | 需要认证 | `{"unread_count": N}` | 200 | 获取未读消息总数 |
| `/_synapse/enhanced/private/search` | POST | `query` (str, 可选, 默认""), `limit` (i64, 可选, 默认50), 需要认证 | 搜索结果JSON | 200 | 搜索私聊消息 |

---

## 九、密钥备份API

| 路由路径 | HTTP方法 | 请求参数 | 响应格式 | 状态码 | 功能描述 |
|---------|---------|---------|---------|--------|---------|
| `/_matrix/client/r0/room_keys/version` | POST | `algorithm` (str, 可选, 默认"m.megolm.v1.aes-sha2"), `auth_data` (object, 可选), 需要认证 | `{"version": "..."}` | 200 | 创建备份版本 |
| `/_matrix/client/r0/room_keys/version/{version}` | GET | `version` (str, Path), 需要认证 | `{"algorithm": "...", "auth_data": {...}, "version": "..."}` | 200 | 获取备份版本信息 |
| `/_matrix/client/r0/room_keys/version/{version}` | PUT | `version` (str, Path), `auth_data` (object, 可选), 需要认证 | `{"version": "..."}` | 200 | 更新备份版本 |
| `/_matrix/client/r0/room_keys/version/{version}` | DELETE | `version` (str, Path), 需要认证 | `{"deleted": true, "version": "..."}` | 200 | 删除备份版本 |
| `/_matrix/client/r0/room_keys/{version}` | GET | `version` (str, Path), 需要认证 | `{"rooms": {...}, "etag": "..."}` | 200 | 获取所有房间密钥 |
| `/_matrix/client/r0/room_keys/{version}` | PUT | `version` (str, Path), `room_id` (str, 必填), `sessions` (array, 必填), 需要认证 | `{"count": N, "etag": "..."}` | 200 | 上传房间密钥 |
| `/_matrix/client/r0/room_keys/{version}/keys` | POST | `version` (str, Path), 多房间密钥对象, 需要认证 | `{"count": N, "etag": "..."}` | 200 | 批量上传房间密钥 |
| `/_matrix/client/r0/room_keys/{version}/keys/{room_id}` | GET | `version`, `room_id` (str, Path), 需要认证 | `{"rooms": {"room_id": {"sessions": {...}}}` | 200 | 获取指定房间的密钥 |
| `/_matrix/client/r0/room_keys/{version}/keys/{room_id}/{session_id}` | GET | `version`, `room_id`, `session_id` (str, Path), 需要认证 | `{"room_id": "...", "session_id": "...", "first_message_index": N, "forwarded_count": N, "is_verified": bool, "session_data": {...}}` | 200 | 获取指定会话的密钥 |

---

## 十、认证与错误处理

### 10.1 认证机制

**AuthenticatedUser** - 标准用户认证提取器
- 从请求头提取Bearer Token
- 验证Token有效性
- 提取用户ID、设备ID、管理员状态

**AdminUser** - 管理员认证提取器
- 需要AuthenticatedUser认证
- 额外检查is_admin=true权限

**federation_auth_middleware** - 联邦认证中间件
- 验证联邦服务器签名
- 检查服务器白名单

### 10.2 错误响应格式

所有API错误响应遵循统一格式：

```json
{
  "errcode": "M_UNAUTHORIZED | M_NOT_FOUND | M_BAD_JSON | M_FORBIDDEN | M_UNKNOWN",
  "error": "错误描述信息"
}
```

### 10.3 状态码说明

| 状态码 | 说明 | 使用场景 |
|--------|------|---------|
| 200 | 请求成功 | 所有成功响应 |
| 400 | 请求格式错误 | 参数缺失、类型错误、格式不正确 |
| 401 | 未授权 | 缺少认证Token、Token无效 |
| 403 | 禁止访问 | 权限不足、非管理员操作 |
| 404 | 资源未找到 | 用户不存在、房间不存在、媒体文件不存在 |
| 429 | 请求过于频繁 | 触发速率限制 |
| 500 | 服务器内部错误 | 数据库错误、未知异常 |

### 10.4 错误码（errcode）说明

| 错误码 | 说明 |
|--------|------|
| M_UNAUTHORIZED | 认证失败，Token无效或过期 |
| M_NOT_FOUND | 请求的资源不存在 |
| M_BAD_JSON | 请求的JSON格式错误 |
| M_FORBIDDEN | 没有权限执行此操作 |
| M_LIMIT_EXCEEDED | 超过速率限制 |
| M_UNKNOWN | 未知错误 |
| M_INVALID_USERNAME | 用户名无效 |
| M_USER_IN_USE | 用户名已被使用 |
| M_INVALID_PASSWORD | 密码不符合要求 |
| M_ROOM_NOT_FOUND | 房间不存在 |
| M_INVALID_ROOM_ID | 房间ID格式错误 |
| M_MISSING_PARAM | 缺少必需参数 |

---

## 附录

### A. 数据类型说明

| 类型 | 说明 | 示例 |
|------|------|------|
| str | 字符串 | `"hello"` |
| i64 | 64位整数 | `1234567890123` |
| i32 | 32位整数 | `123456` |
| u64 | 无符号64位整数 | `1234567890123` |
| u32 | 无符号32位整数 | `123456` |
| bool | 布尔值 | `true` / `false` |
| array | 数组 | `["item1", "item2"]` |
| object | 对象 | `{"key": "value"}` |

### B. 常见参数说明

| 参数 | 说明 | 示例 |
|------|------|------|
| user_id | Matrix用户ID格式 | `@username:server.com` |
| room_id | Matrix房间ID格式 | `!roomid:server.com` |
| device_id | 设备标识符 | `ABCDEFGHIJ` |
| event_id | 事件ID | `$event_id` |
| txn_id | 事务ID | `txn123` |
| limit | 分页限制 | `10` |
| offset | 分页偏移 | `0` |
| since | 同步起始点 | `s123456` |

### C. API统计汇总

| 模块 | 端点数量 |
|------|---------|
| 核心客户端API | 约40个端点 |
| 管理员API | 14个端点 |
| 联邦通信API | 20个端点 |
| 端到端加密API | 6个端点 |
| 语音消息API | 7个端点 |
| 好友系统API | 14个端点 |
| 媒体文件API | 8个端点 |
| 私聊API | 13个端点 |
| 密钥备份API | 9个端点 |
| **总计** | **约131个API端点** |

---

## 更新日志

| 版本 | 日期 | 更新内容 |
|------|------|---------|
| 1.0.0 | 2026-02-03 | 初始版本，完整梳理所有API端点 |

---

## 注意事项

1. **认证要求**：所有需要用户认证的API必须在请求头中携带有效的Bearer Token
2. **速率限制**：部分API有速率限制，超出限制将返回429状态码
3. **分页参数**：列表类API支持limit和offset参数进行分页
4. **错误处理**：所有错误响应遵循统一的JSON格式
5. **版本兼容性**：同时支持Matrix规范v1和v2的端点
6. **管理员权限**：管理员API需要额外的is_admin权限检查
7. **联邦认证**：联邦通信API需要服务器间认证
8. **文件大小限制**：媒体上传最大50MB，超出将返回错误
9. **参数验证**：所有输入参数都应进行类型和格式验证
10. **安全考虑**：敏感操作（如删除、封禁）需要额外权限验证
