# 房间管理 API

## 目录

- [创建房间](#创建房间)
- [加入/离开房间](#加入离开房间)
- [邀请用户](#邀请用户)
- [房间成员管理](#房间成员管理)
- [房间状态](#房间状态)
- [房间目录](#房间目录)

---

## 创建房间

### 创建房间

**端点:** `POST /_matrix/client/r0/createRoom`

**需要认证:** 是

**请求体:**
```typescript
interface CreateRoomRequest {
  visibility?: 'public' | 'private';  // 可见性，默认 private
  name?: string;                      // 房间名称
  topic?: string;                     // 房间主题
  invite?: string[];                  // 邀请的用户 ID 列表
  room_alias_name?: string;           // 房间别名
  preset?: 'private_chat' | 'trusted_private_chat' | 'public_chat';  // 预设
  creation_content?: {                // 初始内容
    type?: string;                     // 事件类型
    state_key?: string;                // 状态键
    content?: object;                  // 事件内容
  };
  is_direct?: boolean;                // 是否为私聊
}
```

**请求示例:**
```typescript
const createRoom = async (accessToken: string, options: CreateRoomRequest) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/createRoom`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify(options)
  });
  return handleApiResponse<{
    room_id: string;
  }>(response);
};

// 创建普通私聊房间
const createDMRoom = async (userId: string, accessToken: string) => {
  return createRoom(accessToken, {
    preset: 'trusted_private_chat',
    invite: [userId],
    is_direct: true
  });
};

// 创建公共房间
const createPublicRoom = async (name: string, topic: string, accessToken: string) => {
  return createRoom(accessToken, {
    visibility: 'public',
    name,
    topic,
    preset: 'public_chat'
  });
};
```

**响应:**
```json
{
  "status": "ok",
  "data": {
    "room_id": "!abc123:matrix.server.com"
  }
}
```

---

## 加入/离开房间

### 加入房间

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/join`

**需要认证:** 是

**请求示例:**
```typescript
const joinRoom = async (roomId: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/join`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({})  // 空对象或包含 third_party_signed 信息
  });
  return handleApiResponse(response);
};
```

---

### 离开房间

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/leave`

**需要认证:** 是

**请求示例:**
```typescript
const leaveRoom = async (roomId: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/leave`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({})
  });
  return handleApiResponse(response);
};
```

---

### 踢出用户

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/kick`

**请求体:**
```typescript
interface KickRequest {
  user_id: string;
  reason?: string;
}
```

**请求示例:**
```typescript
const kickUser = async (roomId: string, userId: string, reason: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/kick`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      user_id: userId,
      reason
    })
  });
  return handleApiResponse(response);
};
```

---

### 封禁用户

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/ban`

**请求体:**
```typescript
interface BanRequest {
  user_id: string;
  reason?: string;
}
```

**请求示例:**
```typescript
const banUser = async (roomId: string, userId: string, reason: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/ban`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      user_id: userId,
      reason
    })
  });
  return handleApiResponse(response);
};
```

---

### 解除封禁

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/unban`

**请求体:**
```typescript
interface UnbanRequest {
  user_id: string;
}
```

**请求示例:**
```typescript
const unbanUser = async (roomId: string, userId: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/unban`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      user_id: userId
    })
  });
  return handleApiResponse(response);
};
```

---

## 邀请用户

### 邀请用户加入房间

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/invite`

**请求体:**
```typescript
interface InviteRequest {
  user_id: string;
}
```

**请求示例:**
```typescript
const inviteUser = async (roomId: string, userId: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/invite`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      user_id: userId
    })
  });
  return handleApiResponse(response);
};
```

---

## 房间成员管理

### 获取房间成员列表

**端点:** `GET /_matrix/client/r0/rooms/{room_id}/members`

**需要认证:** 是

**参数:**
| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| limit | number | 否 | 限制返回数量 |
| since | string | 否 | 从哪个成员开始 |
| membership | string | 否 | 过滤成员状态 (join/invite/leave) |

**请求示例:**
```typescript
const getRoomMembers = async (roomId: string, accessToken: string) => {
  const url = new URL(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/members`);
  url.searchParams.set('limit', '100');

  const response = await fetch(url.toString(), {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    chunk: RoomMember[];
  }>(response);
};

interface RoomMember {
  room_id: string;
  user_id: string;
  membership: 'join' | 'invite' | 'leave' | 'ban';
  display_name?: string;
  avatar_url?: string;
}
```

---

### 获取成员事件

**端点:** `POST /_matrix/client/r0/rooms/{room_id}/get_membership_events`

**请求体:**
```typescript
interface MembershipEventsRequest {
  user_id?: string;
  limit?: number;
  since?: string;
}
```

**请求示例:**
```typescript
const getMembershipEvents = async (roomId: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/get_membership_events`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      limit: 100
    })
  });
  return handleApiResponse<{
    chunk: Event[];
    start: string;
    end: string;
  }>(response);
};
```

---

## 房间状态

### 获取房间状态

**端点:** `GET /_matrix/client/r0/rooms/{room_id}/state`

**需要认证:** 是

**请求示例:**
```typescript
const getRoomState = async (roomId: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/state`, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<Event[]>(response);
};
```

---

### 获取特定状态事件

**端点:** `GET /_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}`

**请求示例:**
```typescript
// 获取房间名称
const getRoomName = async (roomId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/state/m.room.name/`,
    { headers: { 'Authorization': `Bearer ${accessToken}` } }
  );
  return handleApiResponse<{ name: string }>(response);
};

// 获取成员列表
const getRoomMembersState = async (roomId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/state/m.room.member/`,
    { headers: { 'Authorization': `Bearer ${accessToken}` } }
  );
  return handleApiResponse(response);
};
```

---

### 设置房间状态

**端点:** `PUT /_matrix/client/r0/rooms/{room_id}/state/{event_type}/{state_key}`

**请求示例:**
```typescript
// 设置房间名称
const setRoomName = async (roomId: string, name: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/state/m.room.name/`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        name
      })
    }
  );
  return handleApiResponse(response);
};

// 设置房间主题
const setRoomTopic = async (roomId: string, topic: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/state/m.room.topic/`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        topic
      })
    }
  );
  return handleApiResponse(response);
};

// 设置房间头像
const setRoomAvatar = async (roomId: string, avatarUrl: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/state/m.room.avatar/`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        url: avatarUrl
      })
    }
  );
  return handleApiResponse(response);
};
```

---

### 设置加入规则

**端点:** `PUT /_matrix/client/r0/rooms/{room_id}/state/m.room.join_rules/`

**请求体:**
```typescript
interface JoinRules {
  join_rule: 'public' | 'invite' | 'knock' | 'restricted';
  allow?: string[];
}
```

**请求示例:**
```typescript
const setJoinRules = async (roomId: string, rule: 'public' | 'invite', accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/state/m.room.join_rules/`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        join_rule: rule
      })
    }
  );
  return handleApiResponse(response);
};
```

---

## 房间目录

### 获取公共房间列表

**端点:** `GET /_matrix/client/r0/publicRooms`

**参数:**
| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| limit | number | 否 | 每批数量 (默认 10) |
| since | string | 否 | 分页 Token |
| server | string | 否 | 指定服务器 |
| filter | string | 否 | 过滤条件 |

**请求示例:**
```typescript
const getPublicRooms = async (limit = 100, since?: string) => {
  const url = new URL(`${BASE_URL}/_matrix/client/r0/publicRooms`);
  url.searchParams.set('limit', limit.toString());
  if (since) url.searchParams.set('since', since);

  const response = await fetch(url.toString());
  return handleApiResponse<{
    chunk: PublicRoom[];
    next_batch: string;
    total_room_count_estimate?: number;
  }>(response);
};

interface PublicRoom {
  room_id: string;
  name?: string;
  topic?: string;
  num_joined_members: number;
  world_readable: boolean;
  guest_can_join: boolean;
}
```

---

### 设置房间目录

**端点:** `PUT /_matrix/client/r0/directory/room/{room_id}`

**请求体:**
```typescript
interface DirectoryRoomRequest {
  visibility: 'public' | 'private';
}
```

**请求示例:**
```typescript
const setRoomDirectory = async (roomId: string, visibility: 'public', accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/directory/room/${encodeURIComponent(roomId)}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ visibility })
    }
  );
  return handleApiResponse(response);
};
```

---

### 删除房间目录

**端点:** `DELETE /_matrix/client/r0/directory/room/{room_id}`

**请求示例:**
```typescript
const deleteRoomDirectory = async (roomId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/directory/room/${encodeURIComponent(roomId)}`,
    {
      method: 'DELETE',
      headers: { 'Authorization': `Bearer ${accessToken}` }
    }
  );
  return handleApiResponse(response);
};
```

---

## 完整房间服务示例

```typescript
class RoomService {
  constructor(private auth: AuthService) {}

  async createPrivateChat(userId: string) {
    return this.createRoom({
      preset: 'trusted_private_chat',
      invite: [userId],
      is_direct: true
    });
  }

  async createRoom(options: CreateRoomRequest) {
    const response = await fetch(`${BASE_URL}/_matrix/client/r0/createRoom`, {
      method: 'POST',
      headers: this.auth.getAuthHeaders(),
      body: JSON.stringify(options)
    });
    return this.auth.handleResponse(response);
  }

  async sendMessage(roomId: string, message: string) {
    const txnId = Date.now().toString();
    const response = await fetch(
      `${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/send/m.room.message/${txnId}`,
      {
        method: 'PUT',
        headers: this.auth.getAuthHeaders(),
        body: JSON.stringify({
          msgtype: 'm.text',
          body: message
        })
      }
    );
    return this.auth.handleResponse(response);
  }

  async getMessages(roomId: string, limit = 50, from?: string) {
    const url = new URL(`${BASE_URL}/_matrix/client/r0/rooms/${encodeURIComponent(roomId)}/messages`);
    url.searchParams.set('limit', limit.toString());
    if (from) url.searchParams.set('from', from);

    const response = await fetch(url.toString(), {
      headers: { 'Authorization': `Bearer ${this.auth.accessToken}` }
    });
    return this.auth.handleResponse(response);
  }
}
```
