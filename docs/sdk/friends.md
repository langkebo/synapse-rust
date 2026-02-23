# 好友系统 API

## 概述

好友系统已完全重构为基于 Matrix 房间的实现，符合 Matrix 协议规范。

### 架构特点

- **房间驱动**: 所有好友关系存储在专用房间中
- **联邦支持**: 支持跨服务器好友关系
- **E2EE 兼容**: 支持端到端加密的好友聊天
- **状态事件**: 使用 `m.friends.list` 等自定义事件类型

---

## 目录

- [获取好友列表](#获取好友列表)
- [发送好友请求](#发送好友请求)
- [处理好友请求](#处理好友请求)
- [删除好友](#删除好友)
- [私信房间](#私信房间)
- [检查好友关系](#检查好友关系)

---

## 获取好友列表

### 获取好友列表房间 ID

**端点:** `GET /_matrix/client/v1/friends/room`

**需要认证:** 是

**请求示例:**
```typescript
const getFriendListRoomId = async (accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends/room`, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    room_id: string;
    room_type: string;
    event_type: string;
  }>(response);
};
```

---

### 获取好友列表

**端点:** `GET /_matrix/client/v1/friends`

**需要认证:** 是

**响应:**
```json
{
  "status": "ok",
  "data": [
    {
      "user_id": "@bob:cjystx.top",
      "display_name": "Bob",
      "avatar_url": "mxc://...",
      "since": 1234567890,
      "status": "online",
      "last_active": 1234567890,
      "note": "College friend",
      "dm_room_id": "!dm_abc123:cjystx.top",
      "is_private": false
    }
  ]
}
```

**请求示例:**
```typescript
interface FriendInfo {
  user_id: string;
  display_name?: string;
  avatar_url?: string;
  since: number;
  status?: string;
  last_active?: number;
  note?: string;
  dm_room_id?: string;
  is_private?: boolean;
}

const getFriends = async (accessToken: string): Promise<FriendInfo[]> => {
  const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends`, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  const result = await handleApiResponse<{ data: FriendInfo[] }>(response);
  return result.data || [];
};
```

---

## 发送好友请求

### 发送好友请求

**端点:** `POST /_matrix/client/v1/friends/request`

**需要认证:** 是

**请求体:**
```typescript
interface SendFriendRequest {
  user_id: string;      // 目标用户 ID (必填，1-255字符)
  message?: string;     // 附言消息 (可选)
}
```

**请求示例:**
```typescript
const sendFriendRequest = async (userId: string, message?: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends/request`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      user_id: userId,
      message
    })
  });
  return handleApiResponse<{
    request_id: number;
    status: 'pending';
    created_at: string;
  }>(response);
};
```

**响应:**
```json
{
  "status": "ok",
  "data": {
    "request_id": 12345,
    "status": "pending",
    "created_at": "2026-02-11T12:00:00Z"
  }
}
```

---

### 获取待处理好友请求

**端点:** `GET /_matrix/client/v1/friends/requests`

**需要认证:** 是

**请求示例:**
```typescript
const getPendingRequests = async (accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends/requests`, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    data: FriendRequest[];
  }>(response);
};

interface FriendRequest {
  request_id: number;
  requester: string;
  recipient: string;
  message?: string;
  status: 'pending' | 'accepted' | 'declined';
  created_ts: number;
  updated_ts?: number;
}
```

---

## 处理好友请求

### 接受好友请求

**端点:** `POST /_matrix/client/v1/friends/request/{request_id}/accept`

**需要认证:** 是

**请求示例:**
```typescript
const acceptFriendRequest = async (requestId: number, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/v1/friends/request/${requestId}/accept`,
    {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({})
    }
  );
  return handleApiResponse<{
    status: 'accepted';
    dm_room_id: string;
    friend: FriendInfo;
  }>(response);
};
```

**响应:**
```json
{
  "status": "ok",
  "data": {
    "status": "accepted",
    "dm_room_id": "!dm_abc123:cjystx.top",
    "friend": {
      "user_id": "@bob:cjystx.top",
      "display_name": "Bob"
    }
  }
}
```

---

### 拒绝好友请求

**端点:** `POST /_matrix/client/v1/friends/request/{request_id}/decline`

**需要认证:** 是

**请求示例:**
```typescript
const declineFriendRequest = async (requestId: number, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/v1/friends/request/${requestId}/decline`,
    {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({})
    }
  );
  return handleApiResponse<{
    status: 'declined';
  }>(response);
};
```

---

## 删除好友

### 删除好友

**端点:** `DELETE /_matrix/client/v1/friends`

**需要认证:** 是

**请求体:**
```typescript
interface RemoveFriendRequest {
  user_id: string;  // 要删除的好友 ID
}
```

**请求示例:**
```typescript
const removeFriend = async (userId: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends`, {
    method: 'DELETE',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      user_id: userId
    })
  });
  return handleApiResponse<{
    status: 'removed';
    message: string;
  }>(response);
};
```

---

## 私信房间

### 获取私信房间

**端点:** `GET /_matrix/client/v1/friends/dm/{user_id}`

**需要认证:** 是

**请求示例:**
```typescript
const getDmRoom = async (userId: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends/dm/${encodeURIComponent(userId)}`, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    room_id: string;
    exists: boolean;
  }>(response);
};
```

---

### 创建私信房间

**端点:** `POST /_matrix/client/v1/friends/dm/{user_id}`

**需要认证:** 是

**请求体:**
```typescript
interface CreateDmRoomRequest {
  is_private?: boolean;  // 是否为私密房间 (默认 false)
}
```

**请求示例:**
```typescript
const createDmRoom = async (userId: string, isPrivate = false, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends/dm/${encodeURIComponent(userId)}`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      is_private: isPrivate
    })
  });
  return handleApiResponse<{
    room_id: string;
    created: boolean;
  }>(response);
};
```

---

## 检查好友关系

### 检查好友关系

**端点:** `GET /_matrix/client/v1/friends/check/{user_id}`

**需要认证:** 是

**请求示例:**
```typescript
const checkFriendship = async (userId: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends/check/${encodeURIComponent(userId)}`, {
    headers: { 'Authorization': `Bearer ${accessToken}` }
  });
  return handleApiResponse<{
    are_friends: boolean;
    user_id: string;
  }>(response);
};
```

---

## 完整好友服务示例

```typescript
class FriendService {
  constructor(private auth: AuthService) {}

  // 获取所有好友
  async getFriends(): Promise<FriendInfo[]> {
    const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends`, {
      headers: this.auth.getAuthHeaders()
    });
    const result = await this.auth.handleResponse<{ data: FriendInfo[] }>(response);
    return result.data || [];
  }

  // 发送好友请求
  async sendRequest(userId: string, message?: string) {
    const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends/request`, {
      method: 'POST',
      headers: this.auth.getAuthHeaders(),
      body: JSON.stringify({
        user_id: userId,
        message
      })
    });
    return this.auth.handleResponse(response);
  }

  // 获取待处理请求
  async getPendingRequests() {
    const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends/requests`, {
      headers: this.auth.getAuthHeaders()
    });
    const result = await this.auth.handleResponse<{ data: FriendRequest[] }>(response);
    return result.data || [];
  }

  // 接受请求
  async acceptRequest(requestId: number) {
    const response = await fetch(
      `${BASE_URL}/_matrix/client/v1/friends/request/${requestId}/accept`,
      {
        method: 'POST',
        headers: this.auth.getAuthHeaders(),
        body: JSON.stringify({})
      }
    );
    return this.auth.handleResponse(response);
  }

  // 拒绝请求
  async declineRequest(requestId: number) {
    const response = await fetch(
      `${BASE_URL}/_matrix/client/v1/friends/request/${requestId}/decline`,
      {
        method: 'POST',
        headers: this.auth.getAuthHeaders(),
        body: JSON.stringify({})
      }
    );
    return this.auth.handleResponse(response);
  }

  // 删除好友
  async removeFriend(userId: string) {
    const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends`, {
      method: 'DELETE',
      headers: this.auth.getAuthHeaders(),
      body: JSON.stringify({ user_id: userId })
    });
    return this.auth.handleResponse(response);
  }

  // 获取或创建私信房间
  async getDmRoom(userId: string, create = false) {
    if (create) {
      const response = await fetch(
        `${BASE_URL}/_matrix/client/v1/friends/dm/${encodeURIComponent(userId)}`,
        {
          method: 'POST',
          headers: this.auth.getAuthHeaders(),
          body: JSON.stringify({ is_private: false })
        }
      );
      return this.auth.handleResponse(response);
    } else {
      const response = await fetch(
        `${BASE_URL}/_matrix/client/v1/friends/dm/${encodeURIComponent(userId)}`,
        { headers: this.auth.getAuthHeaders() }
      );
      return this.auth.handleResponse(response);
    }
  }

  // 检查是否为好友
  async isFriend(userId: string): Promise<boolean> {
    const response = await fetch(
      `${BASE_URL}/_matrix/client/v1/friends/check/${encodeURIComponent(userId)}`,
      { headers: this.auth.getAuthHeaders() }
    );
    const result = await this.auth.handleResponse<{ are_friends: boolean }>(response);
    return result.are_friends;
  }
}
```

---

## React Hook 示例

```typescript
import { useState, useCallback } from 'react';

interface UseFriendsResult {
  friends: FriendInfo[];
  pendingRequests: FriendRequest[];
  loading: boolean;
  error: string | null;
  sendRequest: (userId: string, message?: string) => Promise<void>;
  acceptRequest: (requestId: number) => Promise<void>;
  declineRequest: (requestId: number) => Promise<void>;
  removeFriend: (userId: string) => Promise<void>;
  isFriend: (userId: string) => Promise<boolean>;
}

export function useFriends(accessToken: string): UseFriendsResult {
  const [friends, setFriends] = useState<FriendInfo[]>([]);
  const [pendingRequests, setPendingRequests] = useState<FriendRequest[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fetchFriends = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends`, {
        headers: { 'Authorization': `Bearer ${accessToken}` }
      });
      const result = await response.json();
      setFriends(result.data || []);
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }, [accessToken]);

  const sendRequest = useCallback(async (userId: string, message?: string) => {
    const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends/request`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ user_id: userId, message })
    });
    if (!response.ok) {
      throw new Error('Failed to send friend request');
    }
    await fetchFriends();
  }, [accessToken, fetchFriends]);

  // ... 其他方法

  return {
    friends,
    pendingRequests,
    loading,
    error,
    sendRequest,
    // ... 其他方法
  };
}
```
