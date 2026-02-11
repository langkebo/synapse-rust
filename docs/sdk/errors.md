# 错误处理

## 概述

API 使用标准的 HTTP 状态码和统一的错误响应格式。客户端应该正确处理这些错误以提供良好的用户体验。

---

## 目录

- [错误响应格式](#错误响应格式)
- [HTTP 状态码](#http-状态码)
- [错误码](#错误码)
- [常见错误场景](#常见错误场景)
- [错误处理最佳实践](#错误处理最佳实践)

---

## 错误响应格式

### 标准错误响应

所有 API 错误都遵循统一的响应格式：

```typescript
interface ApiError {
  status: string;        // 总是 "error"
  code: string;          // 错误码 (如 "M_MISSING_TOKEN")
  message: string;       // 人类可读的错误描述
  details?: {            // 可选的额外详情
    field?: string;      // 验证错误的字段名
    [key: string]: any;
  };
}
```

**示例响应:**
```json
{
  "status": "error",
  "code": "M_MISSING_TOKEN",
  "message": "Access token required",
  "details": null
}
```

---

### Matrix 错误格式

某些端点使用 Matrix 协议标准错误格式：

```typescript
interface MatrixError {
  errcode: string;       // Matrix 错误码
  error: string;         // 人类可读的错误描述
}
```

**示例响应:**
```json
{
  "errcode": "M_UNKNOWN_TOKEN",
  "error": "Unrecognized access token"
}
```

---

## HTTP 状态码

### 成功响应 (2xx)

| 状态码 | 说明 |
|--------|------|
| 200 OK | 请求成功 |
| 201 Created | 资源创建成功 |
| 202 Accepted | 请求已接受，正在处理 |

---

### 客户端错误 (4xx)

| 状态码 | 说明 | 常见错误码 |
|--------|------|------------|
| 400 Bad Request | 请求参数错误 | `M_BAD_JSON`, `M_INVALID_PARAM` |
| 401 Unauthorized | 未认证或认证失败 | `M_MISSING_TOKEN`, `M_UNKNOWN_TOKEN` |
| 403 Forbidden | 无权限访问 | `M_FORBIDDEN` |
| 404 Not Found | 资源不存在 | `M_NOT_FOUND` |
| 409 Conflict | 资源冲突 | `M_USER_IN_USE`, `M_ROOM_IN_USE` |
| 410 Gone | 资源已废弃 | `M_RESOURCE_LIMIT_EXCEEDED` |
| 429 Too Many Requests | 超过速率限制 | `M_LIMIT_EXCEEDED` |
| 422 Unprocessable Entity | 请求格式正确但语义错误 | `M_INVALID_USERNAME` |

---

### 服务器错误 (5xx)

| 状态码 | 说明 |
|--------|------|
| 500 Internal Server Error | 服务器内部错误 |
| 502 Bad Gateway | 网关错误 |
| 503 Service Unavailable | 服务暂时不可用 |
| 504 Gateway Timeout | 网关超时 |

---

## 错误码

### 认证错误

| 错误码 | HTTP 状态 | 说明 |
|--------|----------|------|
| `M_MISSING_TOKEN` | 401 | 缺少访问令牌 |
| `M_UNKNOWN_TOKEN` | 401 | 无效的访问令牌 |
| `M_INVALID_USERNAME` | 400 | 用户名格式无效 |
| `M_INVALID_PASSWORD` | 400 | 密码不符合要求 |
| `M_USER_DEACTIVATED` | 403 | 用户已停用 |

---

### 用户错误

| 错误码 | HTTP 状态 | 说明 |
|--------|----------|------|
| `M_USER_IN_USE` | 400 | 用户名已被使用 |
| `M_USER_NOT_FOUND` | 404 | 用户不存在 |
| `M_INVALID_DISPLAYNAME` | 400 | 显示名称无效 |

---

### 房间错误

| 错误码 | HTTP 状态 | 说明 |
|--------|----------|------|
| `M_ROOM_NOT_FOUND` | 404 | 房间不存在 |
| `M_ROOM_IN_USE` | 409 | 房间别名已被使用 |
| `M_INVALID_ROOM_STATE` | 400 | 房间状态无效 |
| `M_NO_PERMISSION` | 403 | 没有权限执行此操作 |

---

### 好友系统错误

| 错误码 | HTTP 状态 | 说明 |
|--------|----------|------|
| `FRIEND_ALREADY_EXISTS` | 409 | 已经是好友关系 |
| `FRIEND_NOT_FOUND` | 404 | 好友关系不存在 |
| `FRIEND_REQUEST_NOT_FOUND` | 404 | 好友请求不存在 |
| `FRIEND_REQUEST_EXPIRED` | 410 | 好友请求已过期 |
| `FRIEND_REQUEST_PENDING` | 409 | 已有待处理的好友请求 |
| `CANNOT_ADD_SELF` | 400 | 不能添加自己为好友 |
| `INVALID_USER_ID` | 400 | 无效的用户 ID |

---

### 限流错误

| 错误码 | HTTP 状态 | 说明 |
|--------|----------|------|
| `M_LIMIT_EXCEEDED` | 429 | 超过速率限制 |

**响应示例:**
```json
{
  "errcode": "M_LIMIT_EXCEEDED",
  "error": "Too many requests",
  "retry_after_ms": 2000
}
```

---

### 媒体错误

| 错误码 | HTTP 状态 | 说明 |
|--------|----------|------|
| `M_TOO_LARGE` | 413 | 文件过大 |
| `M_INVALID_CONTENT_TYPE` | 400 | 不支持的媒体类型 |

---

### 通用错误

| 错误码 | HTTP 状态 | 说明 |
|--------|----------|------|
| `M_BAD_JSON` | 400 | JSON 格式错误 |
| `M_NOT_JSON` | 400 | 请求体不是有效的 JSON |
| `M_NOT_FOUND` | 404 | 资源不存在 |
| `M_FORBIDDEN` | 403 | 权限不足 |
| `M_UNRECOGNIZED` | 400 | 无法识别的请求 |
| `M_UNKNOWN` | 500 | 未知错误 |

---

## 常见错误场景

### 1. 认证失败

**场景:** 访问受保护的资源时没有提供有效的访问令牌。

**请求:**
```typescript
const response = await fetch(`${BASE_URL}/_matrix/client/r0/sync`, {
  headers: {}  // 缺少 Authorization 头
});
```

**响应 (401):**
```json
{
  "status": "error",
  "code": "M_MISSING_TOKEN",
  "message": "Access token required"
}
```

**处理方式:**
```typescript
if (response.status === 401) {
  // 清除本地存储的令牌
  localStorage.removeItem('access_token');
  // 重定向到登录页面
  window.location.href = '/login';
}
```

---

### 2. Token 过期

**场景:** 访问令牌已过期。

**响应 (401):**
```json
{
  "errcode": "M_UNKNOWN_TOKEN",
  "error": "Access token has expired"
}
```

**处理方式:**
```typescript
// 使用刷新令牌获取新的访问令牌
const refreshAccessToken = async (refreshToken: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/r0/refresh`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ refresh_token: refreshToken })
  });

  if (response.ok) {
    const data = await response.json();
    // 保存新令牌
    localStorage.setItem('access_token', data.access_token);
    return data.access_token;
  } else {
    // 刷新令牌也无效，需要重新登录
    logout();
  }
};
```

---

### 3. 速率限制

**场景:** 请求过于频繁。

**响应 (429):**
```json
{
  "errcode": "M_LIMIT_EXCEEDED",
  "error": "Too many requests",
  "retry_after_ms": 2000
}
```

**处理方式:**
```typescript
let retryCount = 0;
const maxRetries = 3;

const fetchWithRetry = async (url: string, options: RequestInit) => {
  while (retryCount < maxRetries) {
    const response = await fetch(url, options);

    if (response.status === 429) {
      const data = await response.json();
      const retryAfter = data.retry_after_ms || 1000;

      // 等待指定时间后重试
      await new Promise(resolve => setTimeout(resolve, retryAfter));
      retryCount++;
      continue;
    }

    return response;
  }

  throw new Error('Max retries exceeded');
};
```

---

### 4. 验证错误

**场景:** 请求参数验证失败。

**请求:**
```typescript
const response = await fetch(`${BASE_URL}/_matrix/client/r0/register`, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({
    username: 'ab',  // 用户名太短
    password: '123'   // 密码太弱
  })
});
```

**响应 (400):**
```json
{
  "status": "error",
  "code": "M_INVALID_PARAM",
  "message": "Validation failed",
  "details": {
    "errors": [
      { "field": "username", "message": "Username must be at least 3 characters" },
      { "field": "password", "message": "Password must be at least 8 characters" }
    ]
  }
}
```

**处理方式:**
```typescript
const handleValidationErrors = (data: ApiError) => {
  if (data.details?.errors) {
    // 显示每个字段的错误
    data.details.errors.forEach(error => {
      showFieldError(error.field, error.message);
    });
  }
};
```

---

### 5. 好友请求已存在

**场景:** 向已经是好友的用户或已有待处理请求的用户发送好友请求。

**响应 (409):**
```json
{
  "status": "error",
  "code": "FRIEND_REQUEST_PENDING",
  "message": "A friend request already exists for this user"
}
```

**处理方式:**
```typescript
const sendFriendRequest = async (userId: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/v1/friends/request`, {
    method: 'POST',
    headers: {
      'Authorization': `Bearer ${accessToken}`,
      'Content-Type': 'application/json'
    },
    body: JSON.stringify({ user_id: userId })
  });

  const data = await response.json();

  if (response.status === 409 && data.code === 'FRIEND_REQUEST_PENDING') {
    // 提示用户已有待处理的请求
    showToast('info', '好友请求已发送，请等待对方确认');
  }
};
```

---

### 6. 资源不存在

**场景:** 访问不存在的房间或用户。

**响应 (404):**
```json
{
  "status": "error",
  "code": "M_NOT_FOUND",
  "message": "Room not found"
}
```

---

## 错误处理最佳实践

### 1. 统一错误处理器

```typescript
interface ApiResponse<T> {
  status: 'ok' | 'error';
  data?: T;
  code?: string;
  message?: string;
}

class ApiError extends Error {
  constructor(
    public code: string,
    public status: number,
    message: string,
    public details?: any
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

const handleApiResponse = async <T>(
  response: Response
): Promise<T> => {
  const data = await response.json();

  if (!response.ok) {
    throw new ApiError(
      data.code || data.errcode || 'UNKNOWN_ERROR',
      response.status,
      data.message || data.error || 'Request failed',
      data.details
    );
  }

  if (data.status === 'error') {
    throw new ApiError(
      data.code || 'UNKNOWN_ERROR',
      response.status,
      data.message || 'Request failed',
      data.details
    );
  }

  return (data.data || data) as T;
};
```

---

### 2. 使用 React Error Boundary

```typescript
import React, { Component, ErrorInfo, ReactNode } from 'react';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
}

interface State {
  hasError: boolean;
  error?: Error;
}

class ApiErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error('API Error:', error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      return this.props.fallback || (
        <div className="error-boundary">
          <h2>出错了</h2>
          <p>{this.state.error?.message}</p>
          <button onClick={() => window.location.reload()}>
            刷新页面
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}

// 使用
<ApiErrorBoundary fallback={<ErrorPage />}>
  <App />
</ApiErrorBoundary>
```

---

### 3. 错误提示组件

```typescript
import React, { createContext, useContext, useState } from 'react';

type ToastType = 'success' | 'error' | 'warning' | 'info';

interface Toast {
  id: string;
  type: ToastType;
  message: string;
  duration?: number;
}

interface ToastContextType {
  showToast: (type: ToastType, message: string, duration?: number) => void;
  removeToast: (id: string) => void;
  toasts: Toast[];
}

const ToastContext = createContext<ToastContextType | undefined>(undefined);

export const ToastProvider: React.FC<{ children: React.ReactNode }> = ({ children }) => {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const showToast = (type: ToastType, message: string, duration = 3000) => {
    const id = Date.now().toString();
    setToasts(prev => [...prev, { id, type, message, duration }]);

    if (duration > 0) {
      setTimeout(() => removeToast(id), duration);
    }
  };

  const removeToast = (id: string) => {
    setToasts(prev => prev.filter(t => t.id !== id));
  };

  return (
    <ToastContext.Provider value={{ showToast, removeToast, toasts }}>
      {children}
      <ToastContainer />
    </ToastContext.Provider>
  );
};

const ToastContainer: React.FC = () => {
  const context = useContext(ToastContext);
  if (!context) return null;

  return (
    <div className="toast-container">
      {context.toasts.map(toast => (
        <div key={toast.id} className={`toast ${toast.type}`}>
          <span>{toast.message}</span>
          <button onClick={() => context.removeToast(toast.id)}>×</button>
        </div>
      ))}
    </div>
  );
};

export const useToast = () => {
  const context = useContext(ToastContext);
  if (!context) {
    throw new Error('useToast must be used within ToastProvider');
  }
  return context;
};
```

---

### 4. API 请求 Hook

```typescript
import { useState, useCallback } from 'react';
import { useToast } from './useToast';

interface UseApiResult<T> {
  data: T | null;
  loading: boolean;
  error: Error | null;
  execute: () => Promise<void>;
  reset: () => void;
}

export function useApi<T>(
  apiFunction: () => Promise<T>,
  options: {
    onSuccess?: (data: T) => void;
    onError?: (error: Error) => void;
    showErrorToast?: boolean;
  } = {}
): UseApiResult<T> {
  const [data, setData] = useState<T | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<Error | null>(null);
  const { showToast } = useToast();

  const execute = useCallback(async () => {
    setLoading(true);
    setError(null);

    try {
      const result = await apiFunction();
      setData(result);
      options.onSuccess?.(result);
    } catch (err) {
      const error = err as Error;
      setError(error);
      options.onError?.(error);

      if (options.showErrorToast !== false) {
        // 根据错误类型显示不同的提示
        if (error instanceof ApiError) {
          switch (error.status) {
            case 401:
              showToast('error', '请先登录');
              break;
            case 403:
              showToast('error', '没有权限执行此操作');
              break;
            case 404:
              showToast('error', '请求的资源不存在');
              break;
            case 429:
              showToast('warning', '请求过于频繁，请稍后再试');
              break;
            default:
              showToast('error', error.message || '请求失败');
          }
        } else {
          showToast('error', '网络错误，请检查连接');
        }
      }
    } finally {
      setLoading(false);
    }
  }, [apiFunction, options, showToast]);

  const reset = useCallback(() => {
    setData(null);
    setError(null);
    setLoading(false);
  }, []);

  return { data, loading, error, execute, reset };
}

// 使用示例
function UserProfile() {
  const { data: user, loading, error, execute } = useApi(
    () => fetchUserProfile(accessToken),
    {
      onSuccess: (data) => console.log('User loaded:', data),
      showErrorToast: true
    }
  );

  useEffect(() => {
    execute();
  }, []);

  if (loading) return <LoadingSpinner />;
  if (error) return <ErrorMessage error={error} />;

  return <div>{user?.displayname}</div>;
}
```

---

### 5. 网络重试策略

```typescript
interface RetryOptions {
  maxRetries?: number;
  retryDelay?: number;
  retryableStatuses?: number[];
}

const fetchWithRetry = async (
  url: string,
  options: RequestInit = {},
  retryOptions: RetryOptions = {}
): Promise<Response> => {
  const {
    maxRetries = 3,
    retryDelay = 1000,
    retryableStatuses = [408, 429, 500, 502, 503, 504]
  } = retryOptions;

  let lastError: Error | null = null;

  for (let attempt = 0; attempt <= maxRetries; attempt++) {
    try {
      const response = await fetch(url, options);

      if (!retryableStatuses.includes(response.status) || attempt === maxRetries) {
        return response;
      }

      lastError = new Error(`HTTP ${response.status}`);
    } catch (err) {
      lastError = err as Error;
      if (attempt === maxRetries) {
        throw lastError;
      }
    }

    // 指数退避
    await new Promise(resolve =>
      setTimeout(resolve, retryDelay * Math.pow(2, attempt))
    );
  }

  throw lastError;
};
```

---

### 6. 错误日志上报

```typescript
interface ErrorLog {
  timestamp: number;
  message: string;
  stack?: string;
  code?: string;
  status?: number;
  url?: string;
  userAgent?: string;
  userId?: string;
}

const logError = (error: Error | ApiError, context?: Record<string, any>) => {
  const log: ErrorLog = {
    timestamp: Date.now(),
    message: error.message,
    stack: error.stack,
    url: window.location.href,
    userAgent: navigator.userAgent,
    userId: getCurrentUserId()
  };

  if (error instanceof ApiError) {
    log.code = error.code;
    log.status = error.status;
  }

  // 发送到错误收集服务
  fetch(`${BASE_URL}/_matrix/client/v1/logs/error`, {
    method: 'POST',
    headers: {
      'Content-Type': 'application/json',
      'Authorization': `Bearer ${getAccessToken()}`
    },
    body: JSON.stringify({ ...log, context })
  }).catch(() => {
    // 忽略上报失败
  });

  // 开发环境打印到控制台
  if (import.meta.env.DEV) {
    console.error('[API Error]', log);
  }
};
```

---

## 完整错误处理示例

```typescript
// api.ts
import { handleApiResponse, ApiError } from './error-handler';

export const apiClient = {
  async get<T>(url: string, token?: string): Promise<T> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json'
    };

    if (token) {
      headers['Authorization'] = `Bearer ${token}`;
    }

    const response = await fetch(`${BASE_URL}${url}`, { headers });
    return handleApiResponse<T>(response);
  },

  async post<T>(url: string, data: any, token?: string): Promise<T> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json'
    };

    if (token) {
      headers['Authorization'] = `Bearer ${token}`;
    }

    const response = await fetch(`${BASE_URL}${url}`, {
      method: 'POST',
      headers,
      body: JSON.stringify(data)
    });

    return handleApiResponse<T>(response);
  }
};

// 使用
try {
  const user = await apiClient.get<UserInfo>(
    '/_matrix/client/r0/account/whoami',
    accessToken
  );
  console.log('Current user:', user);
} catch (error) {
  if (error instanceof ApiError) {
    switch (error.status) {
      case 401:
        console.log('需要重新登录');
        break;
      case 403:
        console.log('权限不足');
        break;
      default:
        console.log('请求失败:', error.message);
    }
  }
}
```
