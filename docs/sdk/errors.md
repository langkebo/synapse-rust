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

所有 API 错误都遵循 Matrix 协议标准错误格式：

```typescript
interface MatrixError {
  errcode: string;          // Matrix 错误码 (如 "M_MISSING_TOKEN")
  error: string;            // 人类可读的错误描述
  retry_after_ms?: number;  // 仅 429 限流响应附带，建议的重试等待毫秒数
}
```

**示例响应:**
```json
{
  "errcode": "M_UNKNOWN_TOKEN",
  "error": "Unrecognized access token"
}
```

> HTTP 状态码由实现内部的错误类别独立决定（见下方「HTTP 状态码」），与 `errcode` 并非一一绑定。
> 429 响应会额外返回 `retry_after_ms`，并附带 `Retry-After` / `X-RateLimit-Retry-After-Ms` / `X-RateLimit-Limit` / `X-RateLimit-Remaining` 响应头。

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
| 400 Bad Request | 请求参数错误 | `M_BAD_JSON`, `M_INVALID_PARAM`, `M_MISSING_PARAM` |
| 401 Unauthorized | 未认证或认证失败 | `M_MISSING_TOKEN`, `M_UNKNOWN_TOKEN`, `M_UNAUTHORIZED` |
| 403 Forbidden | 无权限访问 | `M_FORBIDDEN`, `M_USER_DEACTIVATED`, `M_GUEST_ACCESS_FORBIDDEN` |
| 404 Not Found | 资源不存在 | `M_NOT_FOUND`, `M_UNKNOWN_DEVICE` |
| 405 Method Not Allowed | 功能不支持 | `M_UNSUPPORTED` |
| 409 Conflict | 资源冲突 | `M_ROOM_IN_USE`, `M_THREEPID_IN_USE`, `M_EXCLUSIVE` |
| 413 Payload Too Large | 请求体过大 | `M_TOO_LARGE` |
| 429 Too Many Requests | 超过速率限制 | `M_LIMIT_EXCEEDED`, `M_USER_LIMIT_EXCEEDED` |

---

### 服务器错误 (5xx)

| 状态码 | 说明 |
|--------|------|
| 500 Internal Server Error | 服务器内部错误 |
| 501 Not Implemented | 功能未实现 |
| 502 Bad Gateway | 上游/网关错误 |
| 503 Service Unavailable | 服务暂时不可用 |
| 504 Gateway Timeout | 网关超时 |

---

## 错误码

下表列出服务端实际定义的全部 `errcode`，按规范 HTTP 状态分组。唯一权威来源为
`synapse-common/src/error/code.rs` 的 `MatrixErrorCode`；本文档与之一一对应。

> 本服务**只**返回 `M_*` 命名空间的错误码。文档历史版本曾列出
> `M_INVALID_PASSWORD`、`M_USER_NOT_FOUND`、`M_ROOM_NOT_FOUND`、`M_NO_PERMISSION`、
> `M_INVALID_CONTENT_TYPE`、`M_INVALID_DISPLAYNAME` 及 `FRIEND_*` / `CANNOT_ADD_SELF` /
> `INVALID_USER_ID` 等，这些码**均不存在**，服务端不会返回；客户端不应据此实现分支。
> 好友等扩展接口复用标准 `M_*` 码（例如资源不存在返回 `M_NOT_FOUND`）。

> HTTP 状态列是该码的规范映射（`MatrixErrorCode::http_status`）。实际响应状态由错误
> 类别决定，可能与下表存在差异（例如 `M_UNRECOGNIZED` 见 501 一节的说明）。

### 400 Bad Request

| 错误码 | 说明 |
|--------|------|
| `M_BAD_JSON` | 请求体是合法 JSON 但结构不符合预期（也用于通用请求参数错误） |
| `M_NOT_JSON` | 请求体无法解析为 JSON |
| `M_UNRECOGNIZED` | 无法识别的请求（未知端点或事件类型） |
| `M_MISSING_PARAM` | 缺少必需参数 |
| `M_INVALID_PARAM` | 参数非法 |
| `M_INVALID_USERNAME` | 用户名格式无效 |
| `M_USER_IN_USE` | 用户 ID 已被占用 |
| `M_INVALID_ROOM_STATE` | 房间状态对该操作无效 |
| `M_BAD_STATE` | 房间状态与预期不符 |
| `M_UNSUPPORTED_ROOM_VERSION` | 请求的房间版本不受支持 |
| `M_INCOMPATIBLE_ROOM_VERSION` | 房间版本不兼容 |
| `M_THREEPID_NOT_FOUND` | 三方 ID 不存在 |
| `M_CAPTCHA_NEEDED` | 注册前需要完成验证码 |
| `M_CAPTCHA_INVALID` | 提供的验证码无效 |
| `M_UNKNOWN_POS` | Sliding Sync 的 `pos` 令牌无效或过期 (MSC4186) |
| `M_BAD_PAGINATION` | 分页查询参数非法 |
| `M_KEY_TOO_LARGE` | 配置字段名超出最大长度 (MSC4133) |
| `M_PROFILE_TOO_LARGE` | 存储的 profile 将超出大小上限 (MSC4133) |

---

### 401 Unauthorized

| 错误码 | 说明 |
|--------|------|
| `M_MISSING_TOKEN` | 缺少访问令牌 |
| `M_UNKNOWN_TOKEN` | 无效的访问令牌 |
| `M_UNAUTHORIZED` | 需要认证 |

---

### 403 Forbidden

| 错误码 | 说明 |
|--------|------|
| `M_FORBIDDEN` | 已认证但无权限执行此操作 |
| `M_USER_DEACTIVATED` | 用户账号已停用 |
| `M_THREEPID_AUTH_FAILED` | 三方 ID 认证失败 |
| `M_THREEPID_DENIED` | 该三方 ID 被拒绝使用 |
| `M_GUEST_ACCESS_FORBIDDEN` | 不允许访客访问 |
| `M_RESOURCE_LIMIT_EXCEEDED` | 超出服务器资源限制 |
| `M_CANNOT_LEAVE_SERVER_NOTICE_ROOM` | 不能离开服务器通知房间 |

---

### 404 Not Found

| 错误码 | 说明 |
|--------|------|
| `M_NOT_FOUND` | 请求的资源不存在 |
| `M_UNKNOWN_DEVICE` | 请求的设备不存在 (Matrix 1.17, MSC4326) |

---

### 405 Method Not Allowed

| 错误码 | 说明 |
|--------|------|
| `M_UNSUPPORTED` | 服务器不支持该功能（如在线状态被禁用） |

---

### 409 Conflict

| 错误码 | 说明 |
|--------|------|
| `M_ROOM_IN_USE` | 房间别名已被占用 |
| `M_THREEPID_IN_USE` | 三方 ID 已被使用 |
| `M_EXCLUSIVE` | 操作与独占资源冲突 |

---

### 413 Payload Too Large

| 错误码 | 说明 |
|--------|------|
| `M_TOO_LARGE` | 请求体或文件过大 |

---

### 429 Too Many Requests

| 错误码 | 说明 |
|--------|------|
| `M_LIMIT_EXCEEDED` | 超过速率限制 |
| `M_USER_LIMIT_EXCEEDED` | 服务器用户数已达上限 (MSC4335) |

**响应示例:**
```json
{
  "errcode": "M_LIMIT_EXCEEDED",
  "error": "Too many requests",
  "retry_after_ms": 2000
}
```

---

### 5xx 服务器错误

| 错误码 | HTTP 状态 | 说明 |
|--------|-----------|------|
| `M_UNKNOWN` | 500 | 未知错误 |
| `M_UNRECOGNIZED` | 501 | 操作未实现（与 400 的同名码复用，见下） |
| `M_CONTENT_SCAN_DISABLED` | 501 | 内容扫描器已禁用 (MSC3806) |
| `M_SERVER_NOT_TRUSTED` | 502 | 目标服务器不受信任 |
| `M_CONTENT_SCAN_FAILED` | 502 | 内容扫描失败，fail-closed (MSC3806) |
| `M_REQUEST_TIMEOUT` | 504 | 请求超时 |

> `M_UNRECOGNIZED` 在实现中由两个变体共用：`Unrecognized`（400，未知请求）与
> `Unimplemented`（501，未实现操作）。两者 `errcode` 字符串相同，客户端应结合
> HTTP 状态区分。

---

## 常见错误场景

### 1. 认证失败

**场景:** 访问受保护的资源时没有提供有效的访问令牌。

**请求:**
```typescript
const response = await fetch(`${BASE_URL}/_matrix/client/v3/sync`, {
  headers: {}  // 缺少 Authorization 头
});
```

**响应 (401):**
```json
{
  "errcode": "M_MISSING_TOKEN",
  "error": "Access token required"
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
  const response = await fetch(`${BASE_URL}/_matrix/client/v3/refresh`, {
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
const response = await fetch(`${BASE_URL}/_matrix/client/v3/register`, {
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
  "errcode": "M_INVALID_PARAM",
  "error": "Username must be at least 3 characters"
}
```

**处理方式:**
```typescript
const handleValidationError = (data: MatrixError) => {
  // 错误响应只提供一条人类可读的 error 描述，由调用方呈现给用户
  showToast('error', data.error);
};
```

---

### 5. 用户名已被占用

**场景:** 注册时使用了一个已存在的用户名。

**响应 (400):**
```json
{
  "errcode": "M_USER_IN_USE",
  "error": "Username already exists"
}
```

**处理方式:**
```typescript
const register = async (username: string, password: string) => {
  const response = await fetch(`${BASE_URL}/_matrix/client/v3/register`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ username, password })
  });

  const data = await response.json();

  if (response.status === 400 && data.errcode === 'M_USER_IN_USE') {
    // 提示用户换一个用户名
    showToast('error', '该用户名已被占用，请更换');
  }
};
```

---

### 6. 资源不存在

**场景:** 访问不存在的房间或用户。

**响应 (404):**
```json
{
  "errcode": "M_NOT_FOUND",
  "error": "Room not found"
}
```

---

## 错误处理最佳实践

### 1. 统一错误处理器

```typescript
class ApiError extends Error {
  constructor(
    public errcode: string,
    public status: number,
    message: string
  ) {
    super(message);
    this.name = 'ApiError';
  }
}

const handleApiResponse = async <T>(
  response: Response
): Promise<T> => {
  // 成功响应直接返回业务负载；错误响应形如 { errcode, error }
  const data = await response.json();

  if (!response.ok) {
    throw new ApiError(
      data.errcode || 'M_UNKNOWN',
      response.status,
      data.error || 'Request failed'
    );
  }

  return data as T;
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
    log.code = error.errcode;
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
    '/_matrix/client/v3/account/whoami',
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
