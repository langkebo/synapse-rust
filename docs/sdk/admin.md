# 管理员 API

## 概述

管理员 API 提供服务器管理和监控功能，需要管理员权限才能访问。

### 权限要求

所有管理员端点需要以下认证方式之一：
- **Bearer Token**: 使用管理员用户的访问令牌
- **Admin Header**: `Authorization: Bearer <admin_access_token>`

---

## 目录

- [服务器信息](#服务器信息)
- [用户管理](#用户管理)
- [房间管理](#房间管理)
- [安全管理](#安全管理)
- [管理员注册](#管理员注册)

---

## 服务器信息

### 获取服务器版本

**端点:** `GET /_synapse/admin/v1/server_version`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "version": "1.0.0",
  "python_version": "3.9.0"
}
```

**请求示例:**
```typescript
const getServerVersion = async (accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_synapse/admin/v1/server_version`, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    version: string;
    python_version: string;
  }>(response);
};
```

---

### 获取服务器状态

**端点:** `GET /_synapse/admin/v1/status`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "status": "running",
  "version": "1.0.0",
  "users": 1234,
  "rooms": 567,
  "uptime": 0
}
```

**请求示例:**
```typescript
const getServerStatus = async (accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_synapse/admin/v1/status`, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    status: string;
    version: string;
    users: number;
    rooms: number;
    uptime: number;
  }>(response);
};
```

---

### 获取服务器统计

**端点:** `GET /_synapse/admin/v1/server_stats`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "user_count": 1234,
  "room_count": 567,
  "total_message_count": 89012,
  "database_pool_size": 20,
  "cache_enabled": true
}
```

**请求示例:**
```typescript
const getServerStats = async (accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_synapse/admin/v1/server_stats`, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    user_count: number;
    room_count: number;
    total_message_count: number;
    database_pool_size: number;
    cache_enabled: boolean;
  }>(response);
};
```

---

### 获取服务器配置

**端点:** `GET /_synapse/admin/v1/config`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "server_name": "cjystx.top",
  "version": "1.0.0",
  "registration_enabled": true,
  "guest_registration_enabled": false,
  "password_policy": {
    "enabled": true,
    "minimum_length": 8,
    "require_digit": true,
    "require_lowercase": true,
    "require_uppercase": true,
    "require_symbol": true
  },
  "rate_limiting": {
    "enabled": true,
    "per_second": 10,
    "burst_size": 50
  }
}
```

---

## 用户管理

### 获取用户列表

**端点:** `GET /_synapse/admin/v1/users`

**需要认证:** 是 (管理员)

**查询参数:**
| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| limit | number | 否 | 每页数量 (默认 100) |
| offset | number | 否 | 偏移量 (默认 0) |

**响应:**
```json
{
  "users": [
    {
      "name": "alice",
      "is_guest": false,
      "admin": true,
      "deactivated": false,
      "displayname": "Alice",
      "avatar_url": "mxc://...",
      "creation_ts": 1234567890,
      "user_type": null
    }
  ],
  "total": 1234
}
```

**请求示例:**
```typescript
const getUsers = async (accessToken: string, options: {
  limit?: number;
  offset?: number;
} = {}) => {
  const url = new URL(`${BASE_URL}/_synapse/admin/v1/users`);
  if (options.limit) url.searchParams.set('limit', options.limit.toString());
  if (options.offset) url.searchParams.set('offset', options.offset.toString());

  const response = await fetch(url.toString(), {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    users: UserInfo[];
    total: number;
  }>(response);
};

interface UserInfo {
  name: string;
  is_guest: boolean;
  admin: boolean;
  deactivated: boolean;
  displayname?: string;
  avatar_url?: string;
  creation_ts: number;
  user_type?: string;
}
```

---

### 获取用户详情

**端点:** `GET /_synapse/admin/v1/users/{user_id}`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "name": "alice",
  "is_guest": false,
  "admin": true,
  "deactivated": false,
  "displayname": "Alice",
  "avatar_url": "mxc://...",
  "creation_ts": 1234567890,
  "user_type": null
}
```

**请求示例:**
```typescript
const getUser = async (userId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_synapse/admin/v1/users/${encodeURIComponent(userId)}`,
    {
      headers: { 'Authorization': `Bearer ${accessToken}` }
    }
  );
  return handleApiResponse<UserInfo>(response);
};
```

---

### 删除用户

**端点:** `DELETE /_synapse/admin/v1/users/{user_id}`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "user_id": "@alice:cjystx.top",
  "deleted": true
}
```

**请求示例:**
```typescript
const deleteUser = async (userId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_synapse/admin/v1/users/${encodeURIComponent(userId)}`,
    {
      method: 'DELETE',
      headers: { 'Authorization': `Bearer ${accessToken}` }
    }
  );
  return handleApiResponse<{
    user_id: string;
    deleted: boolean;
  }>(response);
};
```

---

### 设置管理员权限

**端点:** `PUT /_synapse/admin/v1/users/{user_id}/admin`

**需要认证:** 是 (管理员)

**请求体:**
```typescript
interface SetAdminRequest {
  admin: boolean;  // true = 设置为管理员, false = 取消管理员
}
```

**响应:**
```json
{
  "success": true
}
```

**请求示例:**
```typescript
const setAdmin = async (userId: string, admin: boolean, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_synapse/admin/v1/users/${encodeURIComponent(userId)}/admin`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ admin })
    }
  );
  return handleApiResponse<{ success: boolean }>(response);
};
```

---

### 停用用户

**端点:** `POST /_synapse/admin/v1/users/{user_id}/deactivate`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "id_server_unbind_result": "success"
}
```

**请求示例:**
```typescript
const deactivateUser = async (userId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_synapse/admin/v1/users/${encodeURIComponent(userId)}/deactivate`,
    {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({})
    }
  );
  return handleApiResponse(response);
};
```

---

### 重置用户密码

**端点:** `POST /_synapse/admin/v1/users/{user_id}/password`

**需要认证:** 是 (管理员)

**请求体:**
```typescript
interface ResetPasswordRequest {
  new_password: string;
}
```

**响应:**
```json
{}
```

**请求示例:**
```typescript
const resetPassword = async (userId: string, newPassword: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_synapse/admin/v1/users/${encodeURIComponent(userId)}/password`,
    {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ new_password: newPassword })
    }
  );
  return handleApiResponse(response);
};
```

---

### 获取用户的房间列表

**端点:** `GET /_synapse/admin/v1/users/{user_id}/rooms`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "rooms": ["!room1:cjystx.top", "!room2:cjystx.top"]
}
```

**请求示例:**
```typescript
const getUserRooms = async (userId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_synapse/admin/v1/users/${encodeURIComponent(userId)}/rooms`,
    {
      headers: { 'Authorization': `Bearer ${accessToken}` }
    }
  );
  return handleApiResponse<{ rooms: string[] }>(response);
};
```

---

### 获取用户统计

**端点:** `GET /_synapse/admin/v1/user_stats`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "total_users": 1234,
  "active_users": 1200,
  "admin_users": 5,
  "deactivated_users": 29,
  "guest_users": 5,
  "average_rooms_per_user": 2.5,
  "user_registration_enabled": true
}
```

---

## 房间管理

### 获取房间列表

**端点:** `GET /_synapse/admin/v1/rooms`

**需要认证:** 是 (管理员)

**查询参数:**
| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| limit | number | 否 | 每页数量 (默认 100) |
| offset | number | 否 | 偏移量 (默认 0) |

**响应:**
```json
{
  "rooms": [
    {
      "room_id": "!abc123:cjystx.top",
      "name": "General Chat",
      "topic": "General discussion",
      "creator": "@alice:cjystx.top",
      "joined_members": 15,
      "joined_local_members": 10,
      "is_public": true
    }
  ],
  "total": 567
}
```

**请求示例:**
```typescript
const getRooms = async (accessToken: string, options: {
  limit?: number;
  offset?: number;
} = {}) => {
  const url = new URL(`${BASE_URL}/_synapse/admin/v1/rooms`);
  if (options.limit) url.searchParams.set('limit', options.limit.toString());
  if (options.offset) url.searchParams.set('offset', options.offset.toString());

  const response = await fetch(url.toString(), {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    rooms: RoomInfo[];
    total: number;
  }>(response);
};

interface RoomInfo {
  room_id: string;
  name: string;
  topic: string;
  creator: string;
  joined_members: number;
  joined_local_members: number;
  is_public: boolean;
}
```

---

### 获取房间详情

**端点:** `GET /_synapse/admin/v1/rooms/{room_id}`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "room_id": "!abc123:cjystx.top",
  "name": "General Chat",
  "topic": "General discussion",
  "creator": "@alice:cjystx.top",
  "is_public": true,
  "join_rule": "public"
}
```

---

### 删除房间

**端点:** `DELETE /_synapse/admin/v1/rooms/{room_id}`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "room_id": "!abc123:cjystx.top",
  "deleted": true
}
```

**请求示例:**
```typescript
const deleteRoom = async (roomId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_synapse/admin/v1/rooms/${encodeURIComponent(roomId)}`,
    {
      method: 'DELETE',
      headers: { 'Authorization': `Bearer ${accessToken}` }
    }
  );
  return handleApiResponse<{
    room_id: string;
    deleted: boolean;
  }>(response);
};
```

---

### 关闭房间

**端点:** `POST /_synapse/admin/v1/shutdown_room`

**需要认证:** 是 (管理员)

**请求体:**
```typescript
interface ShutdownRoomRequest {
  room_id: string;
}
```

**响应:**
```json
{
  "kicked_users": [],
  "failed_to_kick_users": [],
  "closed_room": true
}
```

**请求示例:**
```typescript
const shutdownRoom = async (roomId: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_synapse/admin/v1/shutdown_room`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({ room_id: roomId })
  });
  return handleApiResponse(response);
};
```

---

### 清除历史消息

**端点:** `POST /_synapse/admin/v1/purge_history`

**需要认证:** 是 (管理员)

**请求体:**
```typescript
interface PurgeHistoryRequest {
  room_id: string;
  purge_up_to_ts?: number;  // Unix 时间戳 (毫秒)
}
```

**响应:**
```json
{
  "success": true,
  "deleted_events": 1234
}
```

**请求示例:**
```typescript
const purgeHistory = async (
  roomId: string,
  purgeUpToTs?: number,
  accessToken: string
) => {
  const response = await fetch(`${BASE_URL}/_synapse/admin/v1/purge_history`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      room_id: roomId,
      purge_up_to_ts: purgeUpToTs
    })
  });
  return handleApiResponse<{
    success: boolean;
    deleted_events: number;
  }>(response);
};
```

---

## 安全管理

### 获取安全事件

**端点:** `GET /_synapse/admin/v1/security/events`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "events": [
    {
      "id": 1,
      "event_type": "admin_action:block_ip",
      "user_id": "@admin:cjystx.top",
      "ip_address": "192.168.1.100",
      "user_agent": null,
      "details": null,
      "created_at": 1234567890
    }
  ],
  "total": 1
}
```

**请求示例:**
```typescript
const getSecurityEvents = async (accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_synapse/admin/v1/security/events`, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    events: SecurityEvent[];
    total: number;
  }>(response);
};

interface SecurityEvent {
  id: number;
  event_type: string;
  user_id?: string;
  ip_address?: string;
  user_agent?: string;
  details?: string;
  created_at: number;
}
```

---

### 获取 IP 封禁列表

**端点:** `GET /_synapse/admin/v1/security/ip/blocks`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "blocked_ips": [
    {
      "ip_address": "192.168.1.0/24",
      "reason": "Spam activity",
      "blocked_at": 1234567890,
      "expires_at": null
    }
  ],
  "total": 1
}
```

**请求示例:**
```typescript
const getIpBlocks = async (accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_synapse/admin/v1/security/ip/blocks`, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    blocked_ips: BlockedIp[];
    total: number;
  }>(response);
};

interface BlockedIp {
  ip_address: string;
  reason?: string;
  blocked_at: number;
  expires_at?: number;
}
```

---

### 封禁 IP 地址

**端点:** `POST /_synapse/admin/v1/security/ip/block`

**需要认证:** 是 (管理员)

**请求体:**
```typescript
interface BlockIpRequest {
  ip_address: string;
  reason?: string;
  expires_at?: string;  // ISO 8601 格式
}
```

**响应:**
```json
{
  "success": true,
  "ip_address": "192.168.1.100"
}
```

**请求示例:**
```typescript
const blockIp = async (
  ipAddress: string,
  reason?: string,
  expiresAt?: string,
  accessToken: string
) => {
  const response = await fetch(`${BASE_URL}/_synapse/admin/v1/security/ip/block`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      ip_address: ipAddress,
      reason,
      expires_at: expiresAt
    })
  });
  return handleApiResponse<{
    success: boolean;
    ip_address: string;
  }>(response);
};
```

---

### 解除 IP 封禁

**端点:** `POST /_synapse/admin/v1/security/ip/unblock`

**需要认证:** 是 (管理员)

**请求体:**
```typescript
interface UnblockIpRequest {
  ip_address: string;
}
```

**响应:**
```json
{
  "success": true,
  "ip_address": "192.168.1.100",
  "message": "IP unblocked"
}
```

**请求示例:**
```typescript
const unblockIp = async (ipAddress: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_synapse/admin/v1/security/ip/unblock`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({ ip_address: ipAddress })
  });
  return handleApiResponse<{
    success: boolean;
    ip_address: string;
    message: string;
  }>(response);
};
```

---

### 获取 IP 声誉

**端点:** `GET /_synapse/admin/v1/security/ip/reputation/{ip}`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "ip_address": "192.168.1.100",
  "score": 50,
  "last_seen_at": 1234567890,
  "updated_at": 1234567890,
  "details": null
}
```

**请求示例:**
```typescript
const getIpReputation = async (ip: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_synapse/admin/v1/security/ip/reputation/${encodeURIComponent(ip)}`,
    {
      headers: { 'Authorization': `Bearer ${accessToken}` }
    }
  );
  return handleApiResponse<{
    ip_address: string;
    score: number;
    last_seen_at: number;
    updated_at: number;
    details?: string;
  }>(response);
};
```

---

## 管理员注册

### 获取注册 Nonce

**端点:** `GET /_synapse/admin/v1/register/nonce`

**需要认证:** 否

**说明:** 此端点用于获取注册管理员账号所需的 nonce（一次性令牌），有速率限制。

**响应:**
```json
{
  "nonce": "random_nonce_string",
  "expires_at": 1234567890
}
```

**请求示例:**
```typescript
const getAdminRegisterNonce = async () => {
  const response = await fetch(`${BASE_URL}/_synapse/admin/v1/register/nonce`);
  return handleApiResponse<{
    nonce: string;
    expires_at: number;
  }>(response);
};
```

---

### 注册管理员账号

**端点:** `POST /_synapse/admin/v1/register`

**需要认证:** 否

**说明:** 使用 nonce 注册管理员账号，有速率限制。

**请求体:**
```typescript
interface AdminRegisterRequest {
  username: string;
  password: string;
  nonce: string;
  admin_token?: string;  // 可选的管理员令牌
}
```

**响应:**
```json
{
  "access_token": "syt_...",
  "user_id": "@admin:cjystx.top",
  "device_id": "DEVICE_ID"
}
```

**请求示例:**
```typescript
const adminRegister = async (
  username: string,
  password: string,
  nonce: string,
  adminToken?: string
) => {
  const response = await fetch(`${BASE_URL}/_synapse/admin/v1/register`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      username,
      password,
      nonce,
      admin_token: adminToken
    })
  });
  return handleApiResponse<{
    access_token: string;
    user_id: string;
    device_id: string;
  }>(response);
};
```

---

## 日志和监控

### 获取服务器日志

**端点:** `GET /_synapse/admin/v1/logs`

**需要认证:** 是 (管理员)

**查询参数:**
| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| level | string | 否 | 日志级别 (debug, info, warn, error) |
| limit | number | 否 | 限制数量 |

**响应:**
```json
{
  "logs": [
    {
      "timestamp": "2026-02-11T12:00:00Z",
      "level": "info",
      "message": "Server started successfully",
      "module": "synapse::server"
    }
  ],
  "total": 1,
  "level_filter": "info"
}
```

---

### 获取媒体统计

**端点:** `GET /_synapse/admin/v1/media_stats`

**需要认证:** 是 (管理员)

**响应:**
```json
{
  "total_storage_bytes": 1073741824,
  "total_storage_human": "1.00 GB",
  "file_count": 1234,
  "media_directory": "/app/data/media",
  "thumbnail_enabled": true,
  "max_upload_size_mb": 50
}
```

---

## 完整管理服务示例

```typescript
class AdminService {
  constructor(private auth: AuthService) {}

  // ========== 服务器信息 ==========
  async getServerInfo() {
    const [version, status, stats, config] = await Promise.all([
      this.request('/_synapse/admin/v1/server_version'),
      this.request('/_synapse/admin/v1/status'),
      this.request('/_synapse/admin/v1/server_stats'),
      this.request('/_synapse/admin/v1/config')
    ]);

    return { version, status, stats, config };
  }

  // ========== 用户管理 ==========
  async getUsers(options: { limit?: number; offset?: number } = {}) {
    const params = new URLSearchParams();
    if (options.limit) params.set('limit', options.limit.toString());
    if (options.offset) params.set('offset', options.offset.toString());

    return this.request(`/_synapse/admin/v1/users?${params}`);
  }

  async getUser(userId: string) {
    return this.request(`/_synapse/admin/v1/users/${encodeURIComponent(userId)}`);
  }

  async deleteUser(userId: string) {
    return this.request(
      `/_synapse/admin/v1/users/${encodeURIComponent(userId)}`,
      'DELETE'
    );
  }

  async setAdminStatus(userId: string, isAdmin: boolean) {
    return this.request(
      `/_synapse/admin/v1/users/${encodeURIComponent(userId)}/admin`,
      'PUT',
      { admin: isAdmin }
    );
  }

  async deactivateUser(userId: string) {
    return this.request(
      `/_synapse/admin/v1/users/${encodeURIComponent(userId)}/deactivate`,
      'POST',
      {}
    );
  }

  async resetUserPassword(userId: string, newPassword: string) {
    return this.request(
      `/_synapse/admin/v1/users/${encodeURIComponent(userId)}/password`,
      'POST',
      { new_password: newPassword }
    );
  }

  // ========== 房间管理 ==========
  async getRooms(options: { limit?: number; offset?: number } = {}) {
    const params = new URLSearchParams();
    if (options.limit) params.set('limit', options.limit.toString());
    if (options.offset) params.set('offset', options.offset.toString());

    return this.request(`/_synapse/admin/v1/rooms?${params}`);
  }

  async getRoom(roomId: string) {
    return this.request(`/_synapse/admin/v1/rooms/${encodeURIComponent(roomId)}`);
  }

  async deleteRoom(roomId: string) {
    return this.request(
      `/_synapse/admin/v1/rooms/${encodeURIComponent(roomId)}`,
      'DELETE'
    );
  }

  async shutdownRoom(roomId: string) {
    return this.request(
      '/_synapse/admin/v1/shutdown_room',
      'POST',
      { room_id: roomId }
    );
  }

  async purgeRoomHistory(roomId: string, beforeTs?: number) {
    return this.request(
      '/_synapse/admin/v1/purge_history',
      'POST',
      {
        room_id: roomId,
        purge_up_to_ts: beforeTs
      }
    );
  }

  // ========== 安全管理 ==========
  async getSecurityEvents() {
    return this.request('/_synapse/admin/v1/security/events');
  }

  async getIpBlocks() {
    return this.request('/_synapse/admin/v1/security/ip/blocks');
  }

  async blockIp(ipAddress: string, reason?: string, expiresAt?: string) {
    return this.request(
      '/_synapse/admin/v1/security/ip/block',
      'POST',
      {
        ip_address: ipAddress,
        reason,
        expires_at: expiresAt
      }
    );
  }

  async unblockIp(ipAddress: string) {
    return this.request(
      '/_synapse/admin/v1/security/ip/unblock',
      'POST',
      { ip_address: ipAddress }
    );
  }

  async getIpReputation(ip: string) {
    return this.request(`/_synapse/admin/v1/security/ip/reputation/${encodeURIComponent(ip)}`);
  }

  // ========== 日志和统计 ==========
  async getLogs(level = 'info', limit = 100) {
    const params = new URLSearchParams({ level, limit: limit.toString() });
    return this.request(`/_synapse/admin/v1/logs?${params}`);
  }

  async getMediaStats() {
    return this.request('/_synapse/admin/v1/media_stats');
  }

  async getUserStats() {
    return this.request('/_synapse/admin/v1/user_stats');
  }

  // 私有方法
  private async request(endpoint: string, method = 'GET', body?: any) {
    const url = `${BASE_URL}${endpoint}`;
    const options: RequestInit = {
      method,
      headers: {
        'Authorization': `Bearer ${this.auth.accessToken}`,
        'Content-Type': 'application/json'
      }
    };

    if (body && method !== 'GET') {
      options.body = JSON.stringify(body);
    }

    const response = await fetch(url, options);
    return this.auth.handleResponse(response);
  }
}
```

---

## React Hook 示例

```typescript
import { useState, useCallback } from 'react';

interface UseAdminResult {
  users: UserInfo[];
  rooms: RoomInfo[];
  ipBlocks: BlockedIp[];
  loading: boolean;
  error: string | null;
  getUsers: () => Promise<void>;
  deleteUser: (userId: string) => Promise<void>;
  blockIp: (ip: string, reason?: string) => Promise<void>;
  unblockIp: (ip: string) => Promise<void>;
}

export function useAdmin(accessToken: string): UseAdminResult {
  const [users, setUsers] = useState<UserInfo[]>([]);
  const [rooms, setRooms] = useState<RoomInfo[]>([]);
  const [ipBlocks, setIpBlocks] = useState<BlockedIp[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const getUsers = useCallback(async () => {
    setLoading(true);
    try {
      const response = await fetch(`${BASE_URL}/_synapse/admin/v1/users`, {
        headers: { 'Authorization': `Bearer ${accessToken}` }
      });
      const result = await response.json();
      setUsers(result.users || []);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }, [accessToken]);

  const deleteUser = useCallback(async (userId: string) => {
    const response = await fetch(
      `${BASE_URL}/_synapse/admin/v1/users/${encodeURIComponent(userId)}`,
      {
        method: 'DELETE',
        headers: { 'Authorization': `Bearer ${accessToken}` }
      }
    );
    if (!response.ok) {
      throw new Error('Failed to delete user');
    }
    await getUsers();
  }, [accessToken, getUsers]);

  const blockIp = useCallback(async (ip: string, reason?: string) => {
    const response = await fetch(`${BASE_URL}/_synapse/admin/v1/security/ip/block`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ ip_address: ip, reason })
    });
    if (!response.ok) {
      throw new Error('Failed to block IP');
    }
  }, [accessToken]);

  const unblockIp = useCallback(async (ip: string) => {
    const response = await fetch(`${BASE_URL}/_synapse/admin/v1/security/ip/unblock`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ ip_address: ip })
    });
    if (!response.ok) {
      throw new Error('Failed to unblock IP');
    }
  }, [accessToken]);

  return {
    users,
    rooms,
    ipBlocks,
    loading,
    error,
    getUsers,
    deleteUser,
    blockIp,
    unblockIp
  };
}
```

---

## 安全注意事项

1. **保护管理员 Token**: 管理员访问令牌具有完全权限，必须妥善保管
2. **记录管理操作**: 所有管理操作都会记录到安全事件日志中
3. **IP 封禁**: 支持单 IP 和 CIDR 范围封禁
4. **速率限制**: 管理员注册接口有严格的速率限制
5. **审计日志**: 定期检查安全事件日志以发现异常活动
