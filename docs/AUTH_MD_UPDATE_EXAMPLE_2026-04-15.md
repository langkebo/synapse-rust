# auth.md 更新示例 - 注册端点

> 这是一个示范性的更新示例，展示如何根据后端代码更新 API 契约文档

## 更新前（原文档）

```markdown
| POST | `/_matrix/client/r0/register` | r0 | `username` `password` `auth` `device_id` | `access_token` `user_id` `device_id` `refresh_token?` | `200` `400` `401` `409` `429` |
```

## 更新后（详细版本）

### POST /_matrix/client/r0/register

**版本**: r0
**认证**: 公开
**处理器**: `auth_compat.rs::register()` (line 11)

#### 功能说明
注册新用户账户。如果未提供用户名和密码，返回可用的认证流程。

#### 请求参数

**请求体**:
```json
{
  "username": "alice",
  "password": "secret123",
  "auth": {
    "type": "m.login.dummy",
    "session": "xxxxx"
  },
  "device_id": "DEVICE123",
  "displayname": "Alice"
}
```

**字段说明**:

- `username` (string, 必需): 用户名
  - **长度限制**: 1-255 字符
  - **格式要求**: 由 `validator.validate_username()` 验证
  - **示例**: "alice", "bob123"

- `password` (string, 必需): 密码
  - **长度限制**: 1-128 字符
  - **验证**: 由 `validator.validate_password()` 验证
  - **示例**: "secret123"

- `auth` (object, 可选): 认证信息
  - `type` (string): 认证类型
    - 支持的类型: "m.login.dummy", "m.login.password"
  - `session` (string, 可选): 会话 ID

- `device_id` (string, 可选): 设备 ID
  - 如果不提供，服务器会自动生成
  - **示例**: "DEVICE123"

- `displayname` (string, 可选): 显示名称
  - 用户的初始显示名称
  - **示例**: "Alice"

#### 响应

**成功响应 (200)**:
```json
{
  "user_id": "@alice:example.com",
  "access_token": "MDAxOGxvY2F0aW9u...",
  "device_id": "GHTYAJCE",
  "refresh_token": "MDAxOGxvY2F0aW9u..."
}
```

**字段说明**:
- `user_id` (string): 完整的用户 ID，格式为 `@localpart:domain`
- `access_token` (string): 访问令牌，用于后续 API 调用
- `device_id` (string): 设备 ID
- `refresh_token` (string, 可选): 刷新令牌，用于获取新的访问令牌

**认证流程响应 (200)** - 当未提供用户名/密码时:
```json
{
  "flows": [
    { "stages": ["m.login.dummy"] },
    { "stages": ["m.login.password"] }
  ],
  "params": {},
  "session": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

**错误响应**:

- **400 Bad Request** - 请求参数错误
  ```json
  {
    "errcode": "M_BAD_REQUEST",
    "error": "Username required"
  }
  ```
  可能的原因：
  - 用户名为空或未提供
  - 密码为空或未提供
  - 用户名长度超过 255 字符
  - 密码长度超过 128 字符
  - 用户名格式不正确

- **409 Conflict** - 用户名已存在
  ```json
  {
    "errcode": "M_USER_IN_USE",
    "error": "User ID already taken"
  }
  ```

- **429 Too Many Requests** - 请求过于频繁
  ```json
  {
    "errcode": "M_LIMIT_EXCEEDED",
    "error": "Too many requests",
    "retry_after_ms": 2000
  }
  ```

#### 示例

**请求 - 直接注册**:
```bash
curl -X POST "https://matrix.example.com/_matrix/client/r0/register" \
  -H "Content-Type: application/json" \
  -d '{
    "username": "alice",
    "password": "secret123",
    "auth": {
      "type": "m.login.dummy"
    }
  }'
```

**响应**:
```json
{
  "user_id": "@alice:example.com",
  "access_token": "MDAxOGxvY2F0aW9uIGxvY2FsaG9zdDo4MDgwCjAwMTNpZGVudGlmaWVyIGtleQowMDEwY2lkIGdlbiA9IDEKMDAyZGNpZCB1c2VyX2lkID0gQGFsaWNlOmxvY2FsaG9zdAowMDE2Y2lkIHR5cGUgPSBhY2Nlc3MKMDAyMWNpZCBub25jZSA9IDEyMzQ1Njc4OTAK",
  "device_id": "GHTYAJCE"
}
```

**请求 - 获取认证流程**:
```bash
curl -X POST "https://matrix.example.com/_matrix/client/r0/register" \
  -H "Content-Type: application/json" \
  -d '{}'
```

**响应**:
```json
{
  "flows": [
    { "stages": ["m.login.dummy"] },
    { "stages": ["m.login.password"] }
  ],
  "params": {},
  "session": "xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx"
}
```

#### 实现细节

**代码位置**:
- 路由定义: `assembly.rs:237` - `.route("/register", get(get_register_flows).post(register))`
- 处理器: `auth_compat.rs:11` - `pub(crate) async fn register()`
- 验证器: `auth_service.rs` - `validator.validate_username()`, `validator.validate_password()`
- 注册服务: `registration_service.rs` - `register_user()`

**处理流程**:
1. 解析请求体，提取 `username`, `password`, `auth` 等字段
2. 如果未提供用户名或密码，返回可用的认证流程
3. 验证用户名格式（通过 `validate_username()`）
4. 验证密码格式（通过 `validate_password()`）
5. 调用 `registration_service.register_user()` 创建用户
6. 返回用户 ID、访问令牌和设备 ID

**验证规则**:
- 用户名: 1-255 字符，格式由验证器定义
- 密码: 1-128 字符，格式由验证器定义

#### 变更记录

- **[2026-04-15]** 更新了详细的请求参数说明
- **[2026-04-15]** 添加了字段长度限制
- **[2026-04-15]** 添加了完整的错误响应示例
- **[2026-04-15]** 添加了实现细节和代码位置
- **[2026-04-15]** 添加了认证流程响应示例

---

## 同样的方法应用于 v3 版本

### POST /_matrix/client/v3/register

**版本**: v3
**认证**: 公开
**处理器**: `auth_compat.rs::register()` (line 11)

> **注意**: v3 版本与 r0 版本使用相同的处理器，行为完全一致。
> 路由定义: `assembly.rs:253` - `.nest("/_matrix/client/v3", create_auth_compat_router())`

（其他内容与 r0 版本相同）

---

## 更新要点总结

### 1. 基本信息
- ✅ 完整的路径
- ✅ HTTP 方法
- ✅ API 版本
- ✅ 认证要求
- ✅ 处理器位置

### 2. 请求参数
- ✅ 每个字段的详细说明
- ✅ 类型、必需/可选
- ✅ 长度限制
- ✅ 格式要求
- ✅ 验证规则
- ✅ 示例值

### 3. 响应结构
- ✅ 成功响应示例
- ✅ 字段说明
- ✅ 所有错误响应
- ✅ 错误码和原因

### 4. 示例
- ✅ 完整的 curl 命令
- ✅ 真实的请求/响应
- ✅ 多种场景

### 5. 实现细节
- ✅ 代码位置
- ✅ 处理流程
- ✅ 验证规则

### 6. 变更记录
- ✅ 更新日期
- ✅ 变更内容

---

## 下一步

使用相同的方法更新 auth.md 中的其他端点：

1. ✅ POST /register (已完成示例)
2. ⏭️ GET /register (获取注册流程)
3. ⏭️ GET /register/available (检查用户名可用性)
4. ⏭️ POST /register/email/requestToken
5. ⏭️ POST /register/email/submitToken
6. ⏭️ GET /login (获取登录流程)
7. ⏭️ POST /login (登录)
8. ⏭️ POST /logout (登出)
9. ⏭️ POST /logout/all (登出所有设备)
10. ⏭️ POST /refresh (刷新令牌)
11. ⏭️ QR 登录端点 (5个)
12. ⏭️ 账户端点 (whoami, password, deactivate, 3pid, profile)
13. ⏭️ 目录端点 (user_directory, directory/list, publicRooms)

**预计时间**: 每个端点 2-3 分钟，总计约 30-40 分钟完成整个 auth.md
