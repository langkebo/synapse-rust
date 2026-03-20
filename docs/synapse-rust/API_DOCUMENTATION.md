# synapse-rust 完整 API 文档

> Matrix 服务器 Rust 实现

## 目录

1. [认证 API](#认证-api)
2. [房间 API](#房间-api)
3. [消息 API](#消息-api)
4. [用户 API](#用户-api)
5. [设备 API](#设备-api)
6. [媒体 API](#媒体-api)
7. [联邦 API](#联邦-api)
8. [Admin API](#admin-api)
9. [E2EE API](#e2ee-api)

---

## 认证 API

### 登录

**端点**: `POST /_matrix/client/v3/login`

**请求体**:
```json
{
    "type": "m.login.password",
    "identifier": {
        "type": "m.id.user",
        "user": "username"
    },
    "password": "password"
}
```

**响应**:
```json
{
    "access_token": "token",
    "device_id": "DEVICE_ID",
    "user_id": "@user:server.com",
    "expires_in": 36000
}
```

### 注册

**端点**: `POST /_matrix/client/v3/register`

**请求体**:
```json
{
    "auth": {
        "type": "m.login.dummy"
    },
    "username": "newuser",
    "password": "password"
}
```

### 登出

**端点**: `POST /_matrix/client/v3/logout`

**端点**: `POST /_matrix/client/v3/logout/all`

### 刷新 Token

**端点**: `POST /_matrix/client/v3/refresh`

**请求体**:
```json
{
    "refresh_token": "token"
}
```

---

## 房间 API

### 创建房间

**端点**: `POST /_matrix/client/v3/createRoom`

**请求体**:
```json
{
    "name": "Room Name",
    "topic": "Room Topic",
    "visibility": "private",
    "preset": "private_chat"
}
```

### 加入房间

**端点**: `POST /_matrix/client/v3/rooms/{room_id}/join`

**请求体**:
```json
{
    "server_name": ["server.com"]
}
```

### 离开房间

**端点**: `POST /_matrix/client/v3/rooms/{room_id}/leave`

### 邀请用户

**端点**: `POST /_matrix/client/v3/rooms/{room_id}/invite`

**请求体**:
```json
{
    "user_id": "@user:server.com"
}
```

### 踢出用户

**端点**: `POST /_matrix/client/v3/rooms/{room_id}/kick`

**请求体**:
```json
{
    "user_id": "@user:server.com",
    "reason": "Violation of rules"
}
```

### 封禁用户

**端点**: `POST /_matrix/client/v3/rooms/{room_id}/ban`

**请求体**:
```json
{
    "user_id": "@user:server.com",
    "reason": "Violation of rules"
}
```

### 解封用户

**端点**: `POST /_matrix/client/v3/rooms/{room_id}/unban`

**请求体**:
```json
{
    "user_id": "@user:server.com"
}
```

### 获取房间成员

**端点**: `GET /_matrix/client/v3/rooms/{room_id}/members`

### 获取房间状态

**端点**: `GET /_matrix/client/v3/rooms/{room_id}/state`

### 获取房间消息

**端点**: `GET /_matrix/client/v3/rooms/{room_id}/messages`

**查询参数**:
- `from`: 分页 token
- `dir`: 方向 (`b` 或 `f`)
- `limit`: 消息数量

### 获取已加入房间列表

**端点**: `GET /_matrix/client/v3/joined_rooms`

---

## 消息 API

### 发送消息

**端点**: `PUT /_matrix/client/v3/rooms/{room_id}/send/m.room.message/{txn_id}`

**请求体**:
```json
{
    "type": "m.room.message",
    "content": {
        "msgtype": "m.text",
        "body": "Hello world"
    }
}
```

### 撤回消息

**端点**: `PUT /_matrix/client/v3/rooms/{room_id}/redact/{event_id}/{txn_id}`

**请求体**:
```json
{
    "reason": "Sensitive information"
}
```

### 反应

**端点**: `PUT /_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn_id}`

**请求体**:
```json
{
    "m.relates_to": {
        "rel_type": "m.annotation",
        "event_id": "$event_id",
        "key": "👍"
    }
}
```

### 已读回执

**端点**: `POST /_matrix/client/v3/rooms/{room_id}/receipt/m.read/{event_id}`

**请求体**:
```json
{}
```

### 标记已读

**端点**: `POST /_matrix/client/v3/rooms/{room_id}/read_markers`

**请求体**:
```json
{
    "m.fully_read": "$event_id",
    "m.read": "$event_id"
}
```

---

## 用户 API

### 获取用户信息

**端点**: `GET /_matrix/client/v3/profile/{user_id}`

### 设置显示名

**端点**: `PUT /_matrix/client/v3/profile/{user_id}/displayname`

**请求体**:
```json
{
    "displayname": "New Name"
}
```

### 设置头像

**端点**: `PUT /_matrix/client/v3/profile/{user_id}/avatar_url`

**请求体**:
```json
{
    "avatar_url": "mxc://avatar-url"
}
```

### 获取账户数据

**端点**: `GET /_matrix/client/v3/user/{user_id}/account_data/{type}`

### 设置账户数据

**端点**: `PUT /_matrix/client/v3/user/{user_id}/account_data/{type}`

### 修改密码

**端点**: `POST /_matrix/client/v3/account/password`

**请求体**:
```json
{
    "auth": {
        "type": "m.login.password",
        "user": "username",
        "password": "oldpassword"
    },
    "new_password": "newpassword"
}
```

---

## 设备 API

### 获取设备列表

**端点**: `GET /_matrix/client/v3/devices`

### 获取设备

**端点**: `GET /_matrix/client/v3/devices/{device_id}`

### 更新设备

**端点**: `PUT /_matrix/client/v3/devices/{device_id}`

**请求体**:
```json
{
    "display_name": "My Device"
}
```

### 删除设备

**端点**: `DELETE /_matrix/client/v3/devices/{device_id}`

### 批量删除设备

**端点**: `POST /_matrix/client/v3/delete_devices`

**请求体**:
```json
{
    "device_ids": ["device1", "device2"]
}
```

---

## 媒体 API

### 上传媒体

**端点**: `POST /_matrix/media/v3/upload`

**请求头**:
- `Content-Type`: MIME 类型
- `Content-Length`: 文件大小

**请求体**: 二进制数据

**响应**:
```json
{
    "content_uri": "mxc://server.com/media-id"
}
```

### 获取媒体

**端点**: `GET /_matrix/media/v3/download/{server_name}/{media_id}`

### 获取缩略图

**端点**: `GET /_matrix/media/v3/thumbnail/{server_name}/{media_id}`

**查询参数**:
- `width`: 宽度
- `height`: 高度
- `method`: 方法 (`crop` 或 `scale`)

---

## 联邦 API

### 获取版本

**端点**: `GET /_matrix/federation/v1/version`

### 获取公钥

**端点**: `GET /_matrix/federation/v1/key/{server_name}`

### 声明密钥

**端点**: `POST /_matrix/federation/v1/keys/claim`

**请求体**:
```json
{
    "one_time_keys": {
        "@user:server.com": {
            "device_id": "DEVICE"
        }
    }
}
```

### 查询密钥

**端点**: `POST /_matrix/federation/v1/keys/query`

**请求体**:
```json
{
    "device_keys": {
        "@user:server.com": ["device_id"]
    }
}
```

### 获取公开房间

**端点**: `GET /_matrix/federation/v1/publicRooms`

### 发送事件

**端点**: `PUT /_matrix/federation/v1/send/{transaction_id}`

### 获取状态

**端点**: `GET /_matrix/federation/v1/state/{room_id}`

---

## Admin API

### 用户管理

#### 获取用户列表

**端点**: `GET /_synapse/admin/v1/users`

#### 获取用户

**端点**: `GET /_synapse/admin/v1/users/{user_id}`

#### 创建用户

**端点**: `PUT /_synapse/admin/v2/users/{user_id}`

#### 删除用户

**端点**: `DELETE /_synapse/admin/v1/users/{user_id}`

#### 设置管理员

**端点**: `PUT /_synapse/admin/v1/users/{user_id}/admin`

#### 停用用户

**端点**: `POST /_synapse/admin/v1/users/{user_id}/deactivate`

#### 重置密码

**端点**: `POST /_synapse/admin/v1/users/{user_id}/password`

#### 批量创建用户

**端点**: `POST /_synapse/admin/v1/users/batch`

**请求体**:
```json
{
    "users": [
        {
            "username": "user1",
            "password": "password"
        }
    ]
}
```

#### 获取用户会话

**端点**: `GET /_synapse/admin/v1/user_sessions/{user_id}`

#### 使会话失效

**端点**: `POST /_synapse/admin/v1/user_sessions/{user_id}/invalidate`

#### 获取账户详情

**端点**: `GET /_synapse/admin/v1/account/{user_id}`

#### 更新账户

**端点**: `POST /_synapse/admin/v1/account/{user_id}`

### 房间管理

#### 获取房间列表

**端点**: `GET /_synapse/admin/v1/rooms`

#### 获取房间

**端点**: `GET /_synapse/admin/v1/rooms/{room_id}`

#### 删除房间

**端点**: `DELETE /_synapse/admin/v1/rooms/{room_id}`

#### 获取房间成员

**端点**: `GET /_synapse/admin/v1/rooms/{room_id}/members`

#### 获取房间状态

**端点**: `GET /_synapse/admin/v1/rooms/{room_id}/state`

#### 获取房间消息

**端点**: `GET /_synapse/admin/v1/rooms/{room_id}/messages`

#### 封禁房间

**端点**: `POST /_synapse/admin/v1/rooms/{room_id}/block`

#### 解封房间

**端点**: `POST /_synapse/admin/v1/rooms/{room_id}/unblock`

#### 强制加入成员

**端点**: `PUT /_synapse/admin/v1/rooms/{room_id}/members/{user_id}`

#### 移除成员

**端点**: `DELETE /_synapse/admin/v1/rooms/{room_id}/members/{user_id}`

#### 封禁用户

**端点**: `POST /_synapse/admin/v1/rooms/{room_id}/ban/{user_id}`

#### 解封用户

**端点**: `POST /_synapse/admin/v1/rooms/{room_id}/unban/{user_id}`

#### 踢出用户

**端点**: `POST /_synapse/admin/v1/rooms/{room_id}/kick/{user_id}`

#### 获取房间列表状态

**端点**: `GET /_synapse/admin/v1/rooms/{room_id}/listings`

#### 设为公开

**端点**: `PUT /_synapse/admin/v1/rooms/{room_id}/listings/public`

#### 设为私有

**端点**: `DELETE /_synapse/admin/v1/rooms/{room_id}/listings/public`

#### 关闭房间

**端点**: `POST /_synapse/admin/v1/shutdown_room`

#### 清理历史

**端点**: `POST /_synapse/admin/v1/purge_history`

### 统计

#### 获取房间统计

**端点**: `GET /_synapse/admin/v1/room_stats`

#### 获取单房间统计

**端点**: `GET /_synapse/admin/v1/room_stats/{room_id}`

#### 获取用户统计

**端点**: `GET /_synapse/admin/v1/user_stats`

### 服务器

#### 获取服务器版本

**端点**: `GET /_synapse/admin/v1/server_version`

#### 获取状态

**端点**: `GET /_synapse/admin/v1/status`

#### 获取统计

**端点**: `GET /_synapse/admin/v1/statistics`

#### 健康检查

**端点**: `GET /_synapse/admin/v1/health`

---

## E2EE API

### 上传设备密钥

**端点**: `POST /_matrix/client/v3/keys/upload`

**请求体**:
```json
{
    "device_keys": {
        "user_id": "@user:server.com",
        "device_id": "DEVICE",
        "keys": {}
    }
}
```

### 声明一次性密钥

**端点**: `POST /_matrix/client/v3/keys/claim`

### 查询设备密钥

**端点**: `POST /_matrix/client/v3/keys/query`

### 密钥变更

**端点**: `GET /_matrix/client/v3/keys/changes`

### 上传签名

**端点**: `POST /_matrix/client/v3/keys/device_signing/upload`

### 设备验证

**端点**: `POST /_matrix/client/v1/keys/device_signing/verify_start`

### SAS 验证

**端点**: `PUT /_matrix/client/v1/keys/device_signing/verify_key_agreement`

### QR 码验证

**端点**: `PUT /_matrix/client/v1/keys/device_signing/verify_accept`

### Key Backup

#### 获取备份

**端点**: `GET /_matrix/client/v3/room_keys/version`

#### 上传备份

**端点**: `POST /_matrix/client/v3/room_keys/version`

#### 获取房间密钥

**端点**: `GET /_matrix/client/v3/room_keys/{room_id}/{session_id}`

---

*文档版本: 1.0*
*最后更新: 2026-03-19*
