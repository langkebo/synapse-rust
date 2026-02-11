# 用户认证与注册 API

## 目录

- [用户注册](#用户注册)
- [用户登录](#用户登录)
- [退出登录](#退出登录)
- [Token 刷新](#token-刷新)
- [账户管理](#账户管理)

---

## 用户注册

### 检查用户名可用性

**端点:** `GET /_matrix/client/r0/register/available`

**参数:**
| 参数 | 类型 | 必填 | 说明 |
|------|------|------|------|
| username | string | 是 | 用户名 |

**请求示例:**
```typescript
const checkUsername = async (username: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/register/available?username=${encodeURIComponent(username)}`
  );
  const data = await response.json();
  return data.available; // boolean
};
```

**响应:**
```json
{
  "available": true
}
```

---

### 用户注册

**端点:** `POST /_matrix/client/r0/register`

**请求体:**
```typescript
interface RegisterRequest {
  username: string;        // 必填，用户名 (1-255字符，小写字母、数字、点、等号、减号)
  password: string;        // 必填，密码 (至少8字符)
  auth?: {                 // 必填（在某些配置下）
    type: string;          // 固定值: "m.login.dummy"
  };
  display_name?: string;   // 可选，显示名称
}
```

**请求示例:**
```typescript
const register = async (username: string, password: string, displayName?: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/register`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      username,
      password,
      auth: { type: 'm.login.dummy' },
      display_name: displayName
    })
  });
  return handleApiResponse(response);
};
```

**成功响应:**
```json
{
  "status": "ok",
  "data": {
    "user_id": "@alice:cjystx.top",
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGc...",
    "refresh_token": "refresh_token_value",
    "device_id": "ABCDEFGHIJ"
  }
}
```

**错误响应:**
```json
{
  "status": "error",
  "error": "User ID already taken",
  "errcode": "M_USER_IN_USE"
}
```

---

## 用户登录

### 密码登录

**端点:** `POST /_matrix/client/r0/login`

**请求体:**
```typescript
interface LoginRequest {
  type: string;            // 固定值: "m.login.password"
  user: string;            // 用户名或完整的 Matrix ID
  password: string;        // 密码
  device_id?: string;      // 设备 ID (可选，服务器自动生成)
  initial_device_display_name?: string;  // 设备显示名称
}
```

**请求示例:**
```typescript
const login = async (username: string, password: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/login`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      type: 'm.login.password',
      user: username,
      password: password,
      initial_device_display_name: 'My Device'
    })
  });
  return handleApiResponse<LoginResponse>(response);
};

interface LoginResponse {
  user_id: string;
  access_token: string;
  refresh_token?: string;
  device_id: string;
}
```

**成功响应:**
```json
{
  "status": "ok",
  "data": {
    "user_id": "@alice:cjystx.top",
    "access_token": "eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...",
    "device_id": "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
  }
}
```

---

## 退出登录

### 退出当前设备

**端点:** `POST /_matrix/client/r0/logout`

**需要认证:** 是

**请求示例:**
```typescript
const logout = async (accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/logout`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    }
  });
  return handleApiResponse(response);
};
```

### 退出所有设备

**端点:** `POST /_matrix/client/r0/logout/all`

**需要认证:** 是

**请求示例:**
```typescript
const logoutAll = async (accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/logout/all`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    }
  });
  return handleApiResponse(response);
};
```

---

## Token 刷新

### 刷新访问令牌

**端点:** `POST /_matrix/client/r0/refresh`

**需要认证:** 是 (使用 refresh_token)

**请求示例:**
```typescript
const refreshToken = async (refreshToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/refresh`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${refreshToken}`,
      'Content-Type': 'application/json'
    }
  });
  return handleApiResponse<{
    access_token: string;
    expires_in: number;
  }>(response);
};
```

---

## 账户管理

### 获取当前用户信息

**端点:** `GET /_matrix/client/r0/account/whoami`

**需要认证:** 是

**请求示例:**
```typescript
const whoami = async (accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/account/whoami`, {
    headers: {
      'Authorization': `Bearer ${accessToken}`
    }
  });
  return handleApiResponse<{
    user_id: string;
    device_id?: string;
  }>(response);
};
```

**响应:**
```json
{
  "status": "ok",
  "data": {
    "user_id": "@alice:cjystx.top",
    "device_id": "ABCDEFGHIJ"
  }
}
```

---

### 获取用户资料

**端点:** `GET /_matrix/client/r0/account/profile/{user_id}`

**请求示例:**
```typescript
const getProfile = async (userId: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/account/profile/${encodeURIComponent(userId)}`,
    {
      headers: { 'Authorization': `Bearer ${accessToken}` }
    }
  );
  return handleApiResponse<{
    displayname?: string;
    avatar_url?: string;
  }>(response);
};
```

---

### 更新显示名称

**端点:** `PUT /_matrix/client/r0/account/profile/{user_id}/displayname`

**请求体:**
```json
{
  "displayname": "Alice Smith"
}
```

**请求示例:**
```typescript
const updateDisplayName = async (userId: string, displayName: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/account/profile/${encodeURIComponent(userId)}/displayname`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ displayname: displayName })
    }
  );
  return handleApiResponse(response);
};
```

---

### 更新头像

**端点:** `PUT /_matrix/client/r0/account/profile/{user_id}/avatar_url`

**请求体:**
```json
{
  "avatar_url": "mxc://cjystx.top/abcdef123456"
}
```

**请求示例:**
```typescript
const updateAvatar = async (userId: string, avatarUrl: string, accessToken: string) => {
  const response = await fetch(
    `${BASE_URL}/_matrix/client/r0/account/profile/${encodeURIComponent(userId)}/avatar_url`,
    {
      method: 'PUT',
      headers: {
        'Authorization': `Bearer ${accessToken}`,
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ avatar_url: avatarUrl })
    }
  );
  return handleApiResponse(response);
};
```

---

### 修改密码

**端点:** `POST /_matrix/client/r0/account/password`

**请求体:**
```typescript
interface ChangePasswordRequest {
  new_password: string;     // 新密码 (至少8字符)
}
```

**请求示例:**
```typescript
const changePassword = async (newPassword: string, accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/account/password`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      new_password: newPassword
    })
  });
  return handleApiResponse(response);
};
```

---

### 停用账户

**端点:** `POST /_matrix/client/r0/account/deactivate`

**请求体:**
```typescript
interface DeactivateRequest {
  auth?: {
    type: string;
    session: string;  // UI auth session
  };
  id_server?: string;
}
```

**请求示例:**
```typescript
const deactivateAccount = async (accessToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/account/deactivate`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({
      auth: { type: 'm.login.dummy' }
    })
  });
  return handleApiResponse(response);
};
```

---

## 完整认证服务示例

```typescript
class AuthService {
  private baseUrl: string;
  private accessToken: string = '';
  private refreshToken: string = '';

  constructor(baseUrl: string) {
    this.baseUrl = baseUrl;
  }

  // 登录
  async login(username: string, password: string) {
    const response = await fetch(`${this.baseUrl}/_matrix/client/r0/login`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        type: 'm.login.password',
        user: username,
        password: password
      })
    });

    const data = await this.handleResponse(response);
    this.accessToken = data.access_token;
    this.refreshToken = data.refresh_token;

    // 保存到本地存储
    localStorage.setItem('access_token', this.accessToken);
    localStorage.setItem('refresh_token', this.refreshToken);

    return data;
  }

  // 注册
  async register(username: string, password: string, displayName?: string) {
    const response = await fetch(`${this.baseUrl}/_matrix/client/r0/register`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        username,
        password,
        auth: { type: 'm.login.dummy' },
        display_name: displayName
      })
    });
    return this.handleResponse(response);
  }

  // 获取认证头
  getAuthHeaders() {
    return {
      'Authorization': `Bearer ${this.accessToken}`,
      'Content-Type': 'application/json'
    };
  }

  // 退出登录
  async logout() {
    await fetch(`${this.baseUrl}/_matrix/client/r0/logout`, {
      method: 'POST',
      headers: { 'Authorization': `Bearer ${this.accessToken}` }
    });
    this.accessToken = '';
    this.refreshToken = '';
    localStorage.removeItem('access_token');
    localStorage.removeItem('refresh_token');
  }

  private async handleResponse(response: Response) {
    if (!response.ok) {
      const error = await response.json();
      throw new Error(error.error || 'Request failed');
    }
    return response.json();
  }
}
```
