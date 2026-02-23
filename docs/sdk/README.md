# Synapse Rust Matrix Server - SDK 前端开发文档

## 概述

本文档为前端开发人员提供 Synapse Rust Matrix 服务器的完整 API 接口说明和开发指南。

### 服务器信息

```
BASE_URL: http://localhost:8008
测试域名: cjystx.top
版本: 0.1.0
协议: Matrix Client-Server API
```

---

## 目录结构

```
docs/sdk/
├── README.md                    # 本文档 - 总览
├── authentication.md            # 用户认证与注册
├── rooms.md                     # 房间管理
├── friends.md                   # 好友系统
├── messages.md                  # 消息与事件
├── media.md                     # 媒体文件
├── e2ee.md                      # 端到端加密
├── admin.md                     # 管理员接口
└── errors.md                    # 错误处理
```

---

## 文档导航

### 核心功能

| 文档 | 描述 | 主要端点 |
|------|------|----------|
| [认证](./authentication.md) | 用户注册、登录、Token 管理 | `/_matrix/client/r0/login` |
| [房间](./rooms.md) | 创建房间、加入、成员管理 | `/_matrix/client/r0/createRoom` |
| [好友](./friends.md) | 好友请求、私信、好友列表 | `/_matrix/client/v1/friends` |
| [消息](./messages.md) | 发送消息、历史记录、回执 | `/_matrix/client/r0/send` |
| [媒体](./media.md) | 上传下载、缩略图 | `/_matrix/media/v3/upload` |

### 高级功能

| 文档 | 描述 | 主要端点 |
|------|------|----------|
| [E2EE](./e2ee.md) | 端到端加密、设备密钥 | `/_matrix/client/r0/keys/upload` |
| [管理](./admin.md) | 服务器管理、用户管理 | `/_synapse/admin/v1/` |
| [错误](./errors.md) | 错误码、处理最佳实践 | - |

---

## 快速开始

### 1. 用户登录

```typescript
const login = async (username: string, password: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      type: 'm.login.password',
      user: username,
      password: password
    })
  });

  const data = await response.json();
  // 保存 access_token
  localStorage.setItem('access_token', data.access_token);
  localStorage.setItem('user_id', data.user_id);
  return data;
};
```

### 2. 发送消息

```typescript
const sendMessage = async (roomId: string, text: string, token: string) => {
  const txnId = Date.now().toString();
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/rooms/${roomId}/send/m.room.message/${txnId}`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${token}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        msgtype: 'm.text',
        body: text
      })
    }
  );
  return response.json();
};
```

### 3. 获取好友列表

```typescript
const getFriends = async (token: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends`, {
    headers: { 'Authorization': `Bearer ${token}` }
  });
  const result = await response.json();
  return result.data || [];
};
```

### 4. 上传图片

```typescript
const uploadImage = async (file: File, token: string) => {
  const formData = new FormData();
  formData.append('file', file);
  formData.append('filename', file.name);

  const response = await fetch(`${BASE_URL}/_matrix/media/v3/upload`, {
    method: 'POST',
    headers: { 'Authorization': `Bearer ${token}` },
    body: formData
  });
  const result = await response.json();
  return result.content_uri;
};
```

---

## API 版本说明

| 版本前缀 | 用途 |
|---------|------|
| `/_matrix/client/r0/` | 标准 Matrix 客户端 API |
| `/_matrix/client/v1/` | 自定义增强功能 (好友系统) |
| `/_matrix/media/v3/` | 媒体 API |
| `/_synapse/admin/v1/` | 管理员 API |
| `/_matrix/federation/v1/` | 联邦 API (跨服务器通信) |

---

## HTTP 状态码

| 状态码 | 含义 | 使用场景 |
|--------|------|----------|
| 200 | OK | 请求成功 |
| 201 | Created | 资源创建成功 |
| 400 | Bad Request | 请求参数错误 |
| 401 | Unauthorized | 未认证或 Token 无效 |
| 403 | Forbidden | 权限不足 |
| 404 | Not Found | 资源不存在 |
| 409 | Conflict | 资源冲突 (如用户名已存在) |
| 410 | Gone | 资源已废弃 (旧 API) |
| 429 | Too Many Requests | 请求过于频繁 |
| 500 | Internal Server Error | 服务器内部错误 |

---

## 常见错误码

| 错误码 | 说明 |
|--------|------|
| `M_BAD_JSON` | JSON 格式错误 |
| `M_NOT_JSON` | 非 JSON 内容 |
| `M_NOT_FOUND` | 资源不存在 |
| `M_MISSING_TOKEN` | 缺少 Token |
| `M_UNKNOWN_TOKEN` | Token 无效 |
| `M_LIMIT_EXCEEDED` | 超出速率限制 |
| `M_USER_IN_USE` | 用户名已被占用 |
| `M_INVALID_USERNAME` | 用户名格式无效 |
| `M_WEAK_PASSWORD` | 密码强度不足 |
| `M_FORBIDDEN` | 禁止访问 |
| `FRIEND_ALREADY_EXISTS` | 好友关系已存在 |
| `FRIEND_REQUEST_PENDING` | 好友请求待处理 |

> 详细错误处理请参考 [errors.md](./errors.md)

---

## 分页支持

列表类接口支持分页:

```typescript
interface PaginationParams {
  limit?: number;    // 每页数量 (默认: 100, 范围: 10-1000)
  offset?: number;   // 偏移量
  since?: string;    // 起始 Token
}

interface PaginatedResponse<T> {
  data: T[];
  next_token?: string;  // 下一页 Token
  total?: number;      // 总数
}
```

---

## 最佳实践

### 1. Token 管理

```typescript
class AuthManager {
  private token: string = '';
  private refreshToken: string = '';

  async login(username: string, password: string) {
    const response = await fetch(`${BASE_URL}/_matrix/client/r0/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        type: 'm.login.password',
        user: username,
        password: password
      })
    });
    const data = await response.json();
    this.token = data.access_token;
    this.refreshToken = data.refresh_token;

    // 保存到本地存储
    localStorage.setItem('access_token', this.token);
    localStorage.setItem('refresh_token', this.refreshToken);
  }

  async refreshAccessToken() {
    const response = await fetch(`${BASE_URL}/_matrix/client/r0/refresh`, {
      method: 'POST',
      headers: {
        'Authorization': `Bearer ${this.refreshToken}`,
        'Content-Type': 'application/json'
      }
    });
    const data = await response.json();
    this.token = data.access_token;
    localStorage.setItem('access_token', this.token);
  }

  getHeaders() {
    return {
      'Authorization': `Bearer ${this.token}`,
      'Content-Type': 'application/json'
    };
  }
}
```

### 2. 错误处理

```typescript
class ApiError extends Error {
  constructor(
    public code: string,
    public message: string,
    public status: number,
    public details?: any
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

async function handleApiResponse<T>(response: Response): Promise<T> {
  const data = await response.json();

  if (!response.ok) {
    throw new ApiError(
      data.code || data.errcode || 'UNKNOWN',
      data.message || data.error || 'Request failed',
      response.status,
      data.details
    );
  }

  if (data.status === 'error') {
    throw new ApiError(
      data.code || 'UNKNOWN',
      data.message || 'Request failed',
      response.status
    );
  }

  return (data.data || data) as T;
}
```

### 3. 请求拦截器

```typescript
class ApiClient {
  private baseURL: string;
  private token: string = '';

  constructor(baseURL: string) {
    this.baseURL = baseURL;

    // 从本地存储恢复 Token
    this.token = localStorage.getItem('access_token') || '';
  }

  setToken(token: string) {
    this.token = token;
    localStorage.setItem('access_token', token);
  }

  async request<T>(
    endpoint: string,
    options: RequestInit = {}
  ): Promise<T> {
    const url = `${this.baseURL}${endpoint}`;

    const response = await fetch(url, {
      ...options,
      headers: {
        'Content-Type': 'application/json',
        ...(this.token && { 'Authorization': `Bearer ${this.token}` }),
        ...options.headers
      }
    });

    return handleApiResponse<T>(response);
  }

  // 便捷方法
  get<T>(endpoint: string) {
    return this.request<T>(endpoint, { method: 'GET' });
  }

  post<T>(endpoint: string, data: any) {
    return this.request<T>(endpoint, {
      method: 'POST',
      body: JSON.stringify(data)
    });
  }

  put<T>(endpoint: string, data: any) {
    return this.request<T>(endpoint, {
      method: 'PUT',
      body: JSON.stringify(data)
    });
  }

  delete<T>(endpoint: string) {
    return this.request<T>(endpoint, { method: 'DELETE' });
  }
}

// 使用
const api = new ApiClient('http://localhost:8008');

// 登录
const loginData = await api.post('/_matrix/client/r0/login', {
  type: 'm.login.password',
  user: 'alice',
  password: 'password123'
});
api.setToken(loginData.access_token);

// 发送消息
await api.put(
  `/_matrix/client/r0/rooms/${roomId}/send/m.room.message/${txnId}`,
  { msgtype: 'm.text', body: 'Hello!' }
);

// 获取好友
const friends = await api.get<FriendInfo[]>('/_matrix/client/v1/friends');
```

### 4. 速率限制处理

```typescript
let requestQueue = Promise.resolve();
const MIN_REQUEST_INTERVAL = 100; // 100ms

async function rateLimitedFetch(url: string, options?: RequestInit) {
  await requestQueue;
  requestQueue = new Promise(resolve =>
    setTimeout(resolve, MIN_REQUEST_INTERVAL)
  );
  return fetch(url, options);
}

// 或者使用更高级的令牌桶算法
class RateLimiter {
  private queue: Array<() => void> = [];
  private running = 0;
  private maxConcurrent = 5;

  async run<T>(fn: () => Promise<T>): Promise<T> {
    if (this.running >= this.maxConcurrent) {
      await new Promise(resolve => this.queue.push(resolve));
    }

    this.running++;
    try {
      return await fn();
    } finally {
      this.running--;
      const next = this.queue.shift();
      if (next) next();
    }
  }
}
```

---

## 完整服务类示例

```typescript
// services/index.ts
export class MatrixService {
  private api: ApiClient;

  constructor(baseURL: string) {
    this.api = new ApiClient(baseURL);
  }

  // 认证
  async login(username: string, password: string) {
    return this.api.post('/_matrix/client/r0/login', {
      type: 'm.login.password',
      user: username,
      password
    });
  }

  async register(username: string, password: string) {
    return this.api.post('/_matrix/client/r0/register', {
      username,
      password
    });
  }

  // 房间
  async createRoom(options: CreateRoomOptions) {
    return this.api.post('/_matrix/client/r0/createRoom', options);
  }

  async joinRoom(roomId: string) {
    return this.api.post(`/_matrix/client/r0/rooms/${roomId}/join`, {});
  }

  // 好友
  async getFriends() {
    return this.api.get('/_matrix/client/v1/friends');
  }

  async sendFriendRequest(userId: string, message?: string) {
    return this.api.post('/_matrix/client/v1/friends/request', {
      user_id: userId,
      message
    });
  }

  // 消息
  async sendMessage(roomId: string, text: string) {
    const txnId = Date.now().toString();
    return this.api.put(
      `/_matrix/client/r0/rooms/${roomId}/send/m.room.message/${txnId}`,
      { msgtype: 'm.text', body: text }
    );
  }

  // 媒体
  async uploadMedia(file: File) {
    const formData = new FormData();
    formData.append('file', file);
    formData.append('filename', file.name);

    const response = await fetch(`${this.api.baseURL}/_matrix/media/v3/upload`, {
      method: 'POST',
      headers: { 'Authorization': `Bearer ${this.api.token}` },
      body: formData
    });
    return handleApiResponse<{ content_uri: string }>(response);
  }
}
```

---

## TypeScript 类型定义

```typescript
// types/index.ts

// 用户
export interface UserInfo {
  user_id: string;
  username?: string;
  displayname?: string;
  avatar_url?: string;
  is_admin?: boolean;
  deactivated?: boolean;
}

// 房间
export interface RoomInfo {
  room_id: string;
  name?: string;
  topic?: string;
  avatar_url?: string;
  is_public: boolean;
  joined_members: number;
}

// 好友
export interface FriendInfo {
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

// 消息
export interface Message {
  event_id: string;
  room_id: string;
  sender: string;
  content: MessageContent;
  origin_server_ts: number;
}

export interface MessageContent {
  msgtype: 'm.text' | 'm.image' | 'm.file' | 'm.audio';
  body: string;
  url?: string;
  info?: {
    mimetype?: string;
    size?: number;
    w?: number;
    h?: number;
  };
}

// 响应
export interface ApiResponse<T> {
  status: 'ok' | 'error';
  data?: T;
  code?: string;
  message?: string;
}
```

---

## 资源链接

- [Matrix 官方文档](https://spec.matrix.org/)
- [认证 API](./authentication.md)
- [房间 API](./rooms.md)
- [好友系统 API](./friends.md)
- [消息 API](./messages.md)
- [媒体 API](./media.md)
- [E2EE API](./e2ee.md)
- [管理 API](./admin.md)
- [错误处理](./errors.md)
