# Synapse Rust JavaScript SDK API 文档

## 目录

- [概述](#概述)
- [认证 API](#认证-api)
- [用户 API](#用户-api)
- [房间 API](#房间-api)
- [消息 API](#消息-api)
- [设备 API](#设备-api)
- [端到端加密 API](#端到端加密-api)
- [密钥备份 API](#密钥备份-api)
- [好友 API](#好友-api)
- [私聊 API](#私聊-api)
- [语音通话 API](#语音通话-api)
- [媒体 API](#媒体-api)
- [联邦 API](#联邦-api)
- [管理 API](#管理-api)
- [错误码](#错误码)
- [类型定义](#类型定义)

## 概述

本文档详细描述了 Synapse Rust JavaScript SDK 的所有公开 API 接口。所有 API 调用都基于 Matrix 客户端-服务器协议。

### 基础 URL

```
https://your-server.com/_matrix/client/r0
```

### 认证方式

大多数 API 需要通过以下方式之一进行认证：

1. **访问令牌（Access Token）**：在请求头中包含
   ```
   Authorization: Bearer <access_token>
   ```

2. **查询参数**：在 URL 中包含
   ```
   ?access_token=<access_token>
   ```

### 响应格式

所有 API 响应都遵循以下格式：

```typescript
interface ApiResponse<T> {
  data?: T;
  error?: string;
  errcode?: string;
}
```

## 认证 API

### 1. 注册用户

注册新用户账户。

**接口**: `POST /_matrix/client/r0/register`

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| username | string | 是 | 用户名 |
| password | string | 是 | 密码（至少 8 个字符） |
| auth | object | 否 | 认证信息（如果需要） |
| device_id | string | 否 | 设备 ID |
| initial_device_display_name | string | 否 | 设备显示名称 |
| inhibit_login | boolean | 否 | 是否禁止自动登录 |
| admin | boolean | 否 | 是否创建管理员账户 |
| displayname | string | 否 | 显示名称 |

**请求示例**:

```javascript
const response = await client.register({
  username: 'alice',
  password: 'securePassword123',
  device_id: 'DEVICE123',
  initial_device_display_name: 'My Laptop'
});
```

**响应**:

```typescript
interface RegisterResponse {
  user_id: string;           // 用户 ID，格式：@username:server.com
  access_token: string;       // 访问令牌
  device_id: string;         // 设备 ID
  home_server: string;       // 服务器名称
}
```

**错误码**:

- `M_USER_IN_USE`: 用户名已被使用
- `M_INVALID_USERNAME`: 用户名格式无效
- `M_WEAK_PASSWORD`: 密码强度不足

---

### 2. 登录

使用用户名和密码登录。

**接口**: `POST /_matrix/client/r0/login`

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| type | string | 是 | 登录类型，通常为 `m.login.password` |
| user | string | 是 | 用户名或用户 ID |
| password | string | 是 | 密码 |
| device_id | string | 否 | 设备 ID |
| initial_device_display_name | string | 否 | 设备显示名称 |

**请求示例**:

```javascript
const response = await client.login({
  type: 'm.login.password',
  user: 'alice',
  password: 'securePassword123',
  device_id: 'DEVICE123'
});
```

**响应**:

```typescript
interface LoginResponse {
  user_id: string;           // 用户 ID
  access_token: string;       // 访问令牌
  device_id: string;         // 设备 ID
  home_server: string;       // 服务器名称
  well_known?: {
    "m.homeserver": {
      base_url: string;
    };
    "m.identity_server"?: {
      base_url: string;
    };
  };
}
```

**错误码**:

- `M_FORBIDDEN`: 用户名或密码错误
- `M_USER_DEACTIVATED`: 账户已被停用
- `M_LIMIT_EXCEEDED`: 请求过于频繁

---

### 3. 登出

登出当前会话。

**接口**: `POST /_matrix/client/r0/logout`

**请求参数**: 无

**请求示例**:

```javascript
await client.logout();
```

**响应**:

```typescript
interface LogoutResponse {
  // 空对象
}
```

---

### 4. 登出所有设备

登出所有设备的会话。

**接口**: `POST /_matrix/client/r0/logout/all`

**请求参数**: 无

**请求示例**:

```javascript
await client.logoutAll();
```

**响应**: 空对象

---

### 5. 刷新令牌

刷新访问令牌。

**接口**: `POST /_matrix/client/r0/refresh`

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| refresh_token | string | 是 | 刷新令牌 |

**请求示例**:

```javascript
const response = await client.refreshToken({
  refresh_token: 'refresh_token_value'
});
```

**响应**:

```typescript
interface RefreshResponse {
  access_token: string;       // 新的访问令牌
  refresh_token?: string;     // 新的刷新令牌（如果服务器支持）
  expires_in?: number;       // 过期时间（秒）
}
```

---

## 用户 API

### 1. 获取当前用户信息

获取当前登录用户的信息。

**接口**: `GET /_matrix/client/r0/account/whoami`

**请求参数**: 无

**请求示例**:

```javascript
const response = await client.whoami();
console.log(response.user_id);
```

**响应**:

```typescript
interface WhoamiResponse {
  user_id: string;       // 用户 ID
  device_id?: string;     // 设备 ID
}
```

---

### 2. 获取用户资料

获取指定用户的资料信息。

**接口**: `GET /_matrix/client/r0/account/profile/{user_id}`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| user_id | string | 是 | 用户 ID |

**请求示例**:

```javascript
const profile = await client.getProfile('@alice:example.com');
console.log(profile.displayname);
console.log(profile.avatar_url);
```

**响应**:

```typescript
interface UserProfile {
  displayname?: string;    // 显示名称
  avatar_url?: string;     // 头像 URL
}
```

---

### 3. 更新显示名称

更新当前用户的显示名称。

**接口**: `PUT /_matrix/client/r0/account/profile/{user_id}/displayname`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| user_id | 是 | string | 用户 ID |

**请求体**:

```typescript
{
  displayname: string;
}
```

**请求示例**:

```javascript
await client.updateDisplayname('@alice:example.com', 'Alice Smith');
```

**响应**: 空对象

---

### 4. 更新头像

更新当前用户的头像。

**接口**: `PUT /_matrix/client/r0/account/profile/{user_id}/avatar_url`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| user_id | string | 是 | 用户 ID |

**请求体**:

```typescript
{
  avatar_url: string;  // mxc:// URI
}
```

**请求示例**:

```javascript
await client.updateAvatar('@alice:example.com', 'mxc://example.com/abc123');
```

**响应**: 空对象

---

### 5. 修改密码

修改当前用户的密码。

**接口**: `POST /_matrix/client/r0/account/password`

**请求体**:

```typescript
{
  new_password: string;
  auth?: object;  // 认证信息
}
```

**请求示例**:

```javascript
await client.changePassword({
  new_password: 'newSecurePassword456'
});
```

**响应**: 空对象

**错误码**:

- `M_WEAK_PASSWORD`: 新密码强度不足

---

### 6. 停用账户

停用当前用户的账户。

**接口**: `POST /_matrix/client/r0/account/deactivate`

**请求体**:

```typescript
{
  auth?: object;  // 认证信息
  id_server?: string;  // 身份服务器（可选）
}
```

**请求示例**:

```javascript
await client.deactivateAccount();
```

**响应**:

```typescript
interface DeactivateAccountResponse {
  id_server_unbind_result: 'success' | 'no-support';
}
```

---

## 房间 API

### 1. 创建房间

创建新房间。

**接口**: `POST /_matrix/client/r0/createRoom`

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| preset | string | 否 | 房间预设（private_chat, public_chat, trusted_private_chat） |
| room_alias_name | string | 否 | 房间别名 |
| name | string | 否 | 房间名称 |
| topic | string | 否 | 房间主题 |
| invite | string[] | 否 | 初始邀请的用户 ID 列表 |
| invite_3pid | object[] | 否 | 通过第三方邀请 |
| creation_content | object | 否 | 创建内容 |
| initial_state | object[] | 否 | 初始状态事件 |
| room_version | string | 否 | 房间版本 |

**请求示例**:

```javascript
const room = await client.createRoom({
  name: 'My Room',
  topic: 'Discussion about SDK',
  preset: 'private_chat',
  invite: ['@bob:example.com']
});
```

**响应**:

```typescript
interface CreateRoomResponse {
  room_id: string;  // 房间 ID
}
```

---

### 2. 加入房间

加入指定房间。

**接口**: `POST /_matrix/client/r0/rooms/{room_id}/join`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID 或别名 |

**请求体**:

```typescript
{
  reason?: string;  // 加入原因
  third_party_signed?: object;  // 第三方签名
}
```

**请求示例**:

```javascript
const response = await client.joinRoom('!room:example.com');
console.log(response.room_id);
```

**响应**:

```typescript
interface JoinRoomResponse {
  room_id: string;  // 房间 ID
}
```

---

### 3. 离开房间

离开指定房间。

**接口**: `POST /_matrix/client/r0/rooms/{room_id}/leave`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |

**请求体**:

```typescript
{
  reason?: string;  // 离开原因
}
```

**请求示例**:

```javascript
await client.leaveRoom('!room:example.com');
```

**响应**: 空对象

---

### 4. 邀请用户

邀请用户加入房间。

**接口**: `POST /_matrix/client/r0/rooms/{room_id}/invite`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |

**请求体**:

```typescript
{
  user_id: string;  // 被邀请用户的 ID
  reason?: string;  // 邀请原因
}
```

**请求示例**:

```javascript
await client.inviteUser('!room:example.com', '@bob:example.com');
```

**响应**: 空对象

---

### 5. 踢出用户

将用户踢出房间。

**接口**: `POST /_matrix/client/r0/rooms/{room_id}/kick`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |

**请求体**:

```typescript
{
  user_id: string;  // 被踢出用户的 ID
  reason?: string;  // 踢出原因
}
```

**请求示例**:

```javascript
await client.kickUser('!room:example.com', '@bob:example.com', 'Violation of rules');
```

**响应**: 空对象

---

### 6. 禁止用户

禁止用户进入房间。

**接口**: `POST /_matrix/client/r0/rooms/{room_id}/ban`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |

**请求体**:

```typescript
{
  user_id: string;  // 被禁止用户的 ID
  reason?: string;  // 禁止原因
}
```

**请求示例**:

```javascript
await client.banUser('!room:example.com', '@bob:example.com', 'Spamming');
```

**响应**: 空对象

---

### 7. 解禁用户

解除对用户的禁止。

**接口**: `POST /_matrix/client/r0/rooms/{room_id}/unban`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |

**请求体**:

```typescript
{
  user_id: string;  // 被解禁用户的 ID
}
```

**请求示例**:

```javascript
await client.unbanUser('!room:example.com', '@bob:example.com');
```

**响应**: 空对象

---

### 8. 获取房间信息

获取房间的详细信息。

**接口**: `GET /_matrix/client/r0/directory/room/{room_id}`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |

**请求示例**:

```javascript
const roomInfo = await client.getRoomInfo('!room:example.com');
```

**响应**:

```typescript
interface RoomInfo {
  room_id: string;
  name?: string;
  topic?: string;
  num_joined_members: number;
  world_readable: boolean;
  guest_can_join: boolean;
  avatar_url?: string;
  join_rule: string;
}
```

---

### 9. 删除房间

删除房间（仅限房间创建者）。

**接口**: `DELETE /_matrix/client/r0/directory/room/{room_id}`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |

**请求示例**:

```javascript
await client.deleteRoom('!room:example.com');
```

**响应**: 空对象

---

### 10. 获取公开房间列表

获取服务器上的公开房间列表。

**接口**: `GET /_matrix/client/r0/publicRooms`

**查询参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| limit | number | 否 | 返回的最大房间数 |
| since | string | 否 | 分页令牌 |
| server | string | 否 | 指定服务器 |
| search_term | string | 否 | 搜索关键词 |

**请求示例**:

```javascript
const rooms = await client.getPublicRooms({
  limit: 20,
  search_term: 'SDK'
});
```

**响应**:

```typescript
interface PublicRoomsResponse {
  chunk: RoomInfo[];
  next_batch?: string;
  total_room_count_estimate?: number;
}
```

---

### 11. 获取用户房间列表

获取当前用户加入的所有房间。

**接口**: `GET /_matrix/client/r0/user/{user_id}/rooms`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| user_id | string | 是 | 用户 ID |

**请求示例**:

```javascript
const rooms = await client.getUserRooms('@alice:example.com');
```

**响应**:

```typescript
interface UserRoomsResponse {
  joined: RoomInfo[];
  invited: RoomInfo[];
}
```

---

## 消息 API

### 1. 发送消息

向房间发送消息。

**接口**: `POST /_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |
| event_type | string | 是 | 事件类型（通常为 m.room.message） |
| txn_id | string | 否 | 事务 ID（用于去重） |

**请求体**:

```typescript
{
  msgtype: string;  // 消息类型：m.text, m.image, m.video 等
  body: string;    // 消息内容
  url?: string;     // 媒体 URL（用于图片/视频）
  info?: object;   // 媒体信息
}
```

**请求示例**:

```javascript
// 发送文本消息
const response = await client.sendMessage('!room:example.com', 'm.room.message', {
  msgtype: 'm.text',
  body: 'Hello, World!'
});

// 发送图片
await client.sendMessage('!room:example.com', 'm.room.message', {
  msgtype: 'm.image',
  body: 'Image description',
  url: 'mxc://example.com/abc123',
  info: {
    mimetype: 'image/jpeg',
    w: 800,
    h: 600,
    size: 123456
  }
});
```

**响应**:

```typescript
interface SendEventResponse {
  event_id: string;  // 事件 ID
}
```

---

### 2. 获取消息历史

获取房间的消息历史。

**接口**: `GET /_matrix/client/r0/rooms/{room_id}/messages`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |

**查询参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| from | string | 是 | 分页令牌 |
| dir | string | 是 | 方向：f（向前）或 b（向后） |
| limit | number | 否 | 返回的最大事件数 |
| to | string | 否 | 停止令牌 |
| filter | string | 否 | 过滤器 |

**请求示例**:

```javascript
const messages = await client.getMessages('!room:example.com', {
  from: 'start_token',
  dir: 'f',
  limit: 50
});
```

**响应**:

```typescript
interface MessagesResponse {
  start: string;
  end: string;
  chunk: MatrixEvent[];
  state?: MatrixEvent[];
}
```

---

### 3. 编辑消息

编辑已发送的消息。

**接口**: `PUT /_matrix/client/r0/rooms/{room_id}/send/{event_type}/{txn_id}`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |
| event_type | string | 是 | 事件类型（m.room.message） |
| txn_id | string | 否 | 事务 ID |

**请求体**:

```typescript
{
  "m.new_content": {
    msgtype: string;
    body: string;
  };
  "m.relates_to": {
    rel_type: "m.replace";
    event_id: string;  // 原消息的事件 ID
  };
  body: string;  // 回退文本
  msgtype: string;
}
```

**请求示例**:

```javascript
await client.editMessage('!room:example.com', '$event_id', {
  body: 'Updated message',
  msgtype: 'm.text',
  'm.new_content': {
    body: 'Updated message',
    msgtype: 'm.text'
  },
  'm.relates_to': {
    rel_type: 'm.replace',
    event_id: '$event_id'
  }
});
```

---

### 4. 回复消息

回复某条消息。

**接口**: `POST /_matrix/client/r0/rooms/{room_id}/send/m.room.message`

**请求体**:

```typescript
{
  msgtype: string;
  body: string;
  'm.relates_to': {
    rel_type: 'm.reply';
    event_id: string;  // 被回复消息的事件 ID
  };
}
```

**请求示例**:

```javascript
await client.replyMessage('!room:example.com', '$event_id', 'Reply text');
```

---

### 5. 撤回消息

撤回已发送的消息。

**接口**: `PUT /_matrix/client/r0/rooms/{room_id}/redact/{event_id}/{txn_id}`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 是 | 房间 ID |
| event_id | string | 是 | 要撤回的事件 ID |
| txn_id | string | 否 | 事务 ID |

**请求体**:

```typescript
{
  reason?: string;  // 撤回原因
}
```

**请求示例**:

```javascript
await client.redactEvent('!room:example.com', '$event_id', 'Mistake');
```

**响应**:

```typescript
interface RedactEventResponse {
  event_id: string;
}
```

---

## 同步 API

### 1. 同步事件

同步服务器上的事件。

**接口**: `GET /_matrix/client/r0/sync`

**查询参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| filter | string | 否 | 过滤器 ID 或 JSON |
| since | string | 否 | 上次同步的令牌 |
| set_presence | string | 否 | 在线状态：online, offline, unavailable |
| timeout | number | 否 | 超时时间（毫秒） |

**请求示例**:

```javascript
// 初次同步
const response = await client.sync();

// 后续同步
const response = await client.sync({
  since: 'next_batch_token',
  timeout: 30000
});

// 长轮询
const response = await client.sync({
  since: 'next_batch_token',
  timeout: 60000,
  set_presence: 'online'
});
```

**响应**:

```typescript
interface SyncResponse {
  next_batch: string;  // 下次同步的令牌
  rooms: {
    join: {
      [room_id: string]: {
        timeline: {
          events: MatrixEvent[];
          limited: boolean;
          prev_batch: string;
        };
        state: {
          events: MatrixEvent[];
        };
        ephemeral: {
          events: MatrixEvent[];
        };
        account_data: {
          events: MatrixEvent[];
        };
        unread_notifications: {
          highlight_count: number;
          notification_count: number;
        };
      };
    };
    invite: {
      [room_id: string]: {
        invite_state: {
          events: MatrixEvent[];
        };
      };
    };
    leave: {
      [room_id: string]: {
        timeline: {
          events: MatrixEvent[];
          prev_batch: string;
        };
        state: {
          events: MatrixEvent[];
        };
      };
    };
  };
  presence: {
    [user_id: string]: {
      presence: string;
      last_active_ago: number;
      status_msg?: string;
      currently_active: boolean;
    };
  };
  to_device: {
    [event_type: string]: {
      [device_id: string]: object;
    };
  };
  device_lists: {
    changed: string[];
    left: string[];
  };
  device_one_time_keys_count: {
    [algorithm: string]: number;
  };
}
```

---

## 设备 API

### 1. 获取设备列表

获取当前用户的所有设备。

**接口**: `GET /_matrix/client/r0/devices`

**请求示例**:

```javascript
const devices = await client.getDevices();
```

**响应**:

```typescript
interface DevicesResponse {
  devices: Device[];
}

interface Device {
  device_id: string;
  display_name?: string;
  last_seen_ip?: string;
  last_seen_ts?: number;
}
```

---

### 2. 获取特定设备信息

获取指定设备的详细信息。

**接口**: `GET /_matrix/client/r0/devices/{device_id}`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| device_id | string | 是 | 设备 ID |

**请求示例**:

```javascript
const device = await client.getDevice('DEVICE123');
```

**响应**: `Device` 对象

---

### 3. 更新设备信息

更新设备的显示名称。

**接口**: `PUT /_matrix/client/r0/devices/{device_id}`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| device_id | string | 是 | 设备 ID |

**请求体**:

```typescript
{
  display_name: string;
}
```

**请求示例**:

```javascript
await client.updateDevice('DEVICE123', {
  display_name: 'My Phone'
});
```

**响应**: 空对象

---

### 4. 删除设备

删除指定设备。

**接口**: `DELETE /_matrix/client/r0/devices/{device_id}`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| device_id | string | 是 | 设备 ID |

**请求体**:

```typescript
{
  auth?: object;  // 认证信息
}
```

**请求示例**:

```javascript
await client.deleteDevice('DEVICE123');
```

**响应**: 空对象

---

## 在线状态 API

### 1. 获取用户在线状态

获取指定用户的在线状态。

**接口**: `GET /_matrix/client/r0/presence/{user_id}/status`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| user_id | string | 是 | 用户 ID |

**请求示例**:

```javascript
const presence = await client.getPresence('@alice:example.com');
console.log(presence.presence);  // online, offline, unavailable
```

**响应**:

```typescript
interface PresenceResponse {
  presence: string;  // online, offline, unavailable
  last_active_ago?: number;
  status_msg?: string;
  currently_active?: boolean;
}
```

---

### 2. 设置在线状态

设置当前用户的在线状态。

**接口**: `PUT /_matrix/client/r0/presence/{user_id}/status`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| user_id | string | 是 | 用户 ID |

**请求体**:

```typescript
{
  presence: string;  // online, offline, unavailable
  status_msg?: string;
}
```

**请求示例**:

```javascript
await client.setPresence('@alice:example.com', {
  presence: 'online',
  status_msg: 'Working on SDK'
});
```

**响应**: 空对象

---

## 端到端加密 API

### 1. 启用端到端加密

为客户端启用端到端加密。

**接口**: 客户端内部方法

**请求示例**:

```javascript
await client.enableE2EE();
```

**响应**:

```typescript
interface E2EEEnableResponse {
  success: boolean;
  device_keys: object;
  one_time_keys: object;
}
```

---

### 2. 加密消息

加密消息内容。

**接口**: 客户端内部方法

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| roomId | string | 是 | 房间 ID |
| content | object | 是 | 要加密的消息内容 |

**请求示例**:

```javascript
const encrypted = await client.encryptMessage('!room:example.com', {
  msgtype: 'm.text',
  body: 'Secret message'
});
```

**响应**:

```typescript
interface EncryptedContent {
  algorithm: string;
  sender_key: string;
  device_id: string;
  ciphertext: {
    [device_id: string]: {
      body: string;
    };
  };
}
```

---

### 3. 解密消息

解密接收到的加密消息。

**接口**: 客户端内部方法

**请求参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| event | MatrixEvent | 是 | 加密的事件对象 |

**请求示例**:

```javascript
const decrypted = await client.decryptMessage(encryptedEvent);
console.log(decrypted.content);
```

**响应**: 解密后的消息内容

---

### 4. 上传密钥

上传设备密钥到服务器。

**接口**: `POST /_matrix/client/r0/keys/upload`

**请求体**:

```typescript
{
  device_keys: object;
  one_time_keys?: object;
  fallback_keys?: object;
}
```

**请求示例**:

```javascript
await client.uploadKeys({
  device_keys: deviceKeys,
  one_time_keys: oneTimeKeys
});
```

**响应**:

```typescript
interface UploadKeysResponse {
  one_time_key_counts: {
    [algorithm: string]: number;
  };
}
```

---

### 5. 下载密钥

下载其他用户的设备密钥。

**接口**: `POST /_matrix/client/r0/keys/query`

**请求体**:

```typescript
{
  device_keys: {
    [user_id: string]: {
      [device_id: string]: string;  // 空字符串表示下载所有设备
    };
  };
  timeout?: number;
  token?: string;
}
```

**请求示例**:

```javascript
const keys = await client.downloadKeys({
  device_keys: {
    '@alice:example.com': {},
    '@bob:example.com': {}
  }
});
```

**响应**:

```typescript
interface QueryKeysResponse {
  device_keys: {
    [user_id: string]: {
      [device_id: string]: DeviceKeys;
    };
  };
  failures?: {
    [user_id: string]: object;
  };
}
```

---

## 密钥备份 API

### 1. 创建密钥备份版本

创建新的密钥备份版本。

**接口**: `POST /_matrix/client/r0/room_keys/version`

**请求体**:

```typescript
{
  algorithm: string;
  auth_data: {
    public_key: string;
    signatures: object;
  };
}
```

**请求示例**:

```javascript
const response = await client.createKeyBackupVersion({
  algorithm: 'm.megolm_backup.v1.curve25519-aes-sha2',
  auth_data: {
    public_key: 'public_key_value',
    signatures: {}
  }
});
```

**响应**:

```typescript
interface CreateKeyBackupVersionResponse {
  version: string;
}
```

---

### 2. 获取密钥备份版本

获取当前的密钥备份版本信息。

**接口**: `GET /_matrix/client/r0/room_keys/version`

**请求示例**:

```javascript
const version = await client.getKeyBackupVersion();
```

**响应**:

```typescript
interface KeyBackupVersion {
  algorithm: string;
  auth_data: object;
  version: string;
  count: number;
  etag: string;
}
```

---

### 3. 上传密钥备份

上传房间密钥到备份。

**接口**: `PUT /_matrix/client/r0/room_keys/keys`

**请求体**:

```typescript
{
  rooms: {
    [room_id: string]: {
      sessions: {
        [session_id: string]: {
          first_message_index: number;
          forwarded_count: number;
          is_verified: boolean;
          session_data: string;
        };
      };
    };
  };
}
```

**请求示例**:

```javascript
await client.uploadKeyBackup({
  rooms: {
    '!room:example.com': {
      sessions: {
        'session_id': {
          first_message_index: 0,
          forwarded_count: 0,
          is_verified: true,
          session_data: 'encrypted_session_data'
        }
      }
    }
  }
});
```

---

### 4. 下载密钥备份

下载密钥备份。

**接口**: `GET /_matrix/client/r0/room_keys/keys`

**查询参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| room_id | string | 否 | 指定房间 ID |
| session_id | string | 否 | 指定会话 ID |

**请求示例**:

```javascript
const backup = await client.downloadKeyBackup();
```

---

## 媒体 API

### 1. 上传媒体

上传媒体文件到服务器。

**接口**: `POST /_matrix/media/r0/upload`

**查询参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| filename | string | 否 | 文件名 |

**请求体**: 文件内容（multipart/form-data）

**请求示例**:

```javascript
const response = await client.uploadMedia(file, {
  filename: 'image.jpg',
  contentType: 'image/jpeg'
});
```

**响应**:

```typescript
interface UploadMediaResponse {
  content_uri: string;  // mxc:// URI
}
```

---

### 2. 下载媒体

下载媒体文件。

**接口**: `GET /_matrix/media/r0/download/{serverName}/{mediaId}`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| serverName | string | 是 | 服务器名称 |
| mediaId | string | 是 | 媒体 ID |

**查询参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| allow_remote | boolean | 否 | 是否允许从远程服务器下载 |

**请求示例**:

```javascript
const blob = await client.downloadMedia('example.com', 'abc123');
```

**响应**: Blob 对象

---

### 3. 获取媒体缩略图

获取媒体文件的缩略图。

**接口**: `GET /_matrix/media/r0/thumbnail/{serverName}/{mediaId}`

**路径参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| serverName | string | 是 | 服务器名称 |
| mediaId | string | 是 | 媒体 ID |

**查询参数**:

| 参数名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| width | number | 是 | 宽度 |
| height | number | 是 | 高度 |
| method | string | 否 | 缩放方法：crop, scale |
| allow_remote | boolean | 否 | 是否允许远程 |

**请求示例**:

```javascript
const thumbnail = await client.getThumbnail('example.com', 'abc123', {
  width: 200,
  height: 200,
  method: 'crop'
});
```

---

## 错误码

### Matrix 标准错误码

| 错误码 | HTTP 状态码 | 说明 |
|--------|------------|------|
| M_FORBIDDEN | 403 | 禁止访问 |
| M_UNKNOWN_TOKEN | 401 | 未知的访问令牌 |
| M_MISSING_TOKEN | 401 | 缺少访问令牌 |
| M_BAD_JSON | 400 | JSON 格式错误 |
| M_NOT_JSON | 400 | 不是有效的 JSON |
| M_NOT_FOUND | 404 | 资源未找到 |
| M_LIMIT_EXCEEDED | 429 | 请求过于频繁 |
| M_USER_IN_USE | 400 | 用户名已被使用 |
| M_INVALID_USERNAME | 400 | 用户名格式无效 |
| M_ROOM_IN_USE | 400 | 房间别名已被使用 |
| M_INVALID_ROOM_STATE | 400 | 房间状态无效 |
| M_UNSUPPORTED_ROOM_VERSION | 400 | 不支持的房间版本 |
| M_INCOMPATIBLE_ROOM_VERSION | 400 | 不兼容的房间版本 |
| M_BAD_STATE | 400 | 状态错误 |
| M_GUEST_ACCESS_FORBIDDEN | 403 | 禁止访客访问 |
| M_CAPTCHA_NEEDED | 400 | 需要验证码 |
| M_CAPTCHA_INVALID | 400 | 验证码无效 |
| M_BAD_PAGINATION | 400 | 分页参数错误 |
| M_CANNOT_LEAVE_SERVER_NOTICE_ROOM | 403 | 无法离开服务器通知房间 |
| M_CANNOT_UPDATE_SERVER_NOTICE_ROOM | 403 | 无法更新服务器通知房间 |
| M_MISSING_PARAM | 400 | 缺少必需参数 |
| M_INVALID_PARAM | 400 | 参数无效 |
| M_TOO_LARGE | 413 | 请求过大 |
| M_EXCLUSIVE | 400 | 资源已被锁定 |
| M_RESOURCE_LIMIT_EXCEEDED | 403 | 超出资源限制 |
| M_USER_DEACTIVATED | 403 | 用户账户已停用 |
| M_WEAK_PASSWORD | 400 | 密码强度不足 |

### SDK 特定错误

| 错误码 | 说明 |
|--------|------|
| NETWORK_ERROR | 网络连接错误 |
| TIMEOUT_ERROR | 请求超时 |
| ENCRYPTION_ERROR | 加密/解密失败 |
| STORAGE_ERROR | 存储操作失败 |
| INVALID_STATE | 客户端状态无效 |

---

## 类型定义

### 基础类型

```typescript
// Matrix 事件
interface MatrixEvent {
  event_id: string;
  room_id?: string;
  sender: string;
  type: string;
  content: any;
  origin_server_ts: number;
  state_key?: string;
  prev_content?: any;
  age?: number;
  unsigned?: {
    age?: number;
    transaction_id?: string;
  };
}

// 房间事件类型
type RoomEventType =
  | 'm.room.message'
  | 'm.room.member'
  | 'm.room.name'
  | 'm.room.topic'
  | 'm.room.avatar'
  | 'm.room.power_levels'
  | 'm.room.join_rules'
  | 'm.room.canonical_alias'
  | 'm.room.encrypted';

// 消息类型
type MessageType =
  | 'm.text'
  | 'm.emote'
  | 'm.notice'
  | 'm.image'
  | 'm.video'
  | 'm.audio'
  | 'm.file'
  | 'm.location';

// 在线状态
type PresenceStatus = 'online' | 'offline' | 'unavailable';

// 房间成员资格
type RoomMembership = 'join' | 'leave' | 'invite' | 'ban' | 'knock';

// 房间加入规则
type JoinRule =
  | 'public'
  | 'invite'
  | 'knock'
  | 'private'
  | 'restricted';
```

### 消息内容类型

```typescript
// 文本消息
interface TextMessage {
  msgtype: 'm.text';
  body: string;
  formatted_body?: string;
  format?: 'org.matrix.custom.html';
}

// 图片消息
interface ImageMessage {
  msgtype: 'm.image';
  body: string;
  url: string;
  info: {
    mimetype: string;
    w: number;
    h: number;
    size: number;
    thumbnail_url?: string;
    thumbnail_info?: object;
  };
}

// 视频消息
interface VideoMessage {
  msgtype: 'm.video';
  body: string;
  url: string;
  info: {
    mimetype: string;
    w: number;
    h: number;
    duration: number;
    size: number;
    thumbnail_url?: string;
    thumbnail_info?: object;
  };
}

// 音频消息
interface AudioMessage {
  msgtype: 'm.audio';
  body: string;
  url: string;
  info: {
    mimetype: string;
    duration: number;
    size: number;
  };
}

// 文件消息
interface FileMessage {
  msgtype: 'm.file';
  body: string;
  url: string;
  info: {
    mimetype: string;
    size: number;
  };
}

// 位置消息
interface LocationMessage {
  msgtype: 'm.location';
  body: string;
  geo_uri: string;
}
```

### 客户端配置

```typescript
interface ClientConfig {
  baseUrl: string;              // 服务器基础 URL
  accessToken?: string;          // 访问令牌
  userId?: string;             // 用户 ID
  deviceId?: string;            // 设备 ID
  enableE2EE?: boolean;       // 是否启用端到端加密
  store?: object;              // 存储配置
  timeout?: number;            // 请求超时时间（毫秒）
  maxRetries?: number;         // 最大重试次数
  presence?: PresenceStatus;    // 默认在线状态
}
```

### 设备密钥

```typescript
interface DeviceKeys {
  user_id: string;
  device_id: string;
  algorithms: string[];
  keys: {
    [key_id: string]: string;
  };
  signatures: {
    [user_id: string]: {
      [key_id: string]: string;
    };
  };
}
```

---

## 使用示例

### 完整示例：创建客户端并发送消息

```javascript
import { MatrixClient } from 'synapse-rust-sdk';

// 创建客户端
const client = new MatrixClient({
  baseUrl: 'https://matrix.example.com',
  enableE2EE: true
});

// 登录
const loginResponse = await client.login({
  type: 'm.login.password',
  user: 'alice',
  password: 'securePassword123'
});

console.log('Logged in as:', loginResponse.user_id);

// 创建房间
const room = await client.createRoom({
  name: 'SDK Test Room',
  topic: 'Testing the SDK',
  preset: 'private_chat'
});

console.log('Created room:', room.room_id);

// 发送消息
const messageResponse = await client.sendMessage(room.room_id, {
  msgtype: 'm.text',
  body: 'Hello from the SDK!'
});

console.log('Message sent:', messageResponse.event_id);

// 同步事件
const syncResponse = await client.sync({
  timeout: 30000
});

console.log('Synced, next batch:', syncResponse.next_batch);

// 登出
await client.logout();
```

### 示例：端到端加密

```javascript
import { MatrixClient } from 'synapse-rust-sdk';

const client = new MatrixClient({
  baseUrl: 'https://matrix.example.com',
  enableE2EE: true
});

await client.login({
  type: 'm.login.password',
  user: 'alice',
  password: 'securePassword123'
});

// 启用端到端加密
await client.enableE2EE();

// 加密并发送消息
const encrypted = await client.encryptMessage('!room:example.com', {
  msgtype: 'm.text',
  body: 'Secret message'
});

await client.sendMessage('!room:example.com', 'm.room.encrypted', encrypted);

// 接收并解密消息
const syncResponse = await client.sync();
for (const roomId in syncResponse.rooms.join) {
  const events = syncResponse.rooms.join[roomId].timeline.events;
  for (const event of events) {
    if (event.type === 'm.room.encrypted') {
      const decrypted = await client.decryptMessage(event);
      console.log('Decrypted:', decrypted);
    }
  }
}
```

### 示例：媒体上传

```javascript
import { MatrixClient } from 'synapse-rust-sdk';

const client = new MatrixClient({
  baseUrl: 'https://matrix.example.com',
  accessToken: 'your_access_token'
});

// 上传图片
const fileInput = document.getElementById('file-input');
const file = fileInput.files[0];

const uploadResponse = await client.uploadMedia(file, {
  filename: file.name,
  contentType: file.type
});

console.log('Uploaded to:', uploadResponse.content_uri);

// 发送图片消息
await client.sendMessage('!room:example.com', {
  msgtype: 'm.image',
  body: file.name,
  url: uploadResponse.content_uri,
  info: {
    mimetype: file.type,
    size: file.size,
    w: 800,
    h: 600
  }
});
```

---

## 最佳实践

### 1. 错误处理

```javascript
try {
  const response = await client.sendMessage(roomId, message);
  console.log('Message sent:', response.event_id);
} catch (error) {
  if (error.errcode === 'M_FORBIDDEN') {
    console.error('Permission denied');
  } else if (error.errcode === 'M_LIMIT_EXCEEDED') {
    console.error('Rate limited, retry later');
    // 实现退避重试
    setTimeout(() => retrySend(), error.retry_after_ms || 5000);
  } else {
    console.error('Unknown error:', error);
  }
}
```

### 2. 同步循环

```javascript
let nextBatch = null;

async function syncLoop() {
  while (true) {
    try {
      const response = await client.sync({
        since: nextBatch,
        timeout: 30000
      });

      nextBatch = response.next_batch;

      // 处理新事件
      processEvents(response);

    } catch (error) {
      console.error('Sync error:', error);
      // 等待后重试
      await new Promise(resolve => setTimeout(resolve, 5000));
    }
  }
}

syncLoop();
```

### 3. 密钥管理

```javascript
// 定期上传密钥
setInterval(async () => {
  try {
    await client.uploadKeys({
      device_keys: client.getDeviceKeys(),
      one_time_keys: client.generateOneTimeKeys()
    });
    console.log('Keys uploaded successfully');
  } catch (error) {
    console.error('Failed to upload keys:', error);
  }
}, 24 * 60 * 60 * 1000); // 每 24 小时

// 定期备份密钥
setInterval(async () => {
  try {
    await client.uploadKeyBackup({
      rooms: client.exportRoomKeys()
    });
    console.log('Keys backed up successfully');
  } catch (error) {
    console.error('Failed to backup keys:', error);
  }
}, 7 * 24 * 60 * 60 * 1000); // 每周
```

---

## 参考资源

- [Matrix 客户端-服务器 API 规范](https://matrix.org/docs/client_server_api.html)
- [Matrix 端到端加密规范](https://matrix.org/docs/specifications/appendices/#e2e-encryption)
- [SDK 开发指南](./SDK-Development-Guide.md)
- [示例代码](../examples/)

---

## 许可证

MIT License - 详见 [LICENSE](../../LICENSE) 文件
