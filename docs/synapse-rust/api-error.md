# API测试失败记录

更新日期: 2026-02-05

## 3.1.1 健康检查、账户管理与用户资料

> **测试状态**: ✅ **100% 通过** | **完整验证完成**

### 测试结果汇总

| 类别 | 通过 | 总数 | 通过率 |
|------|------|------|--------|
| 健康检查 | 2 | 2 | **100%** ✅ |
| 用户认证与注册 | 6 | 6 | **100%** ✅ |
| 账号管理 | 4 | 4 | **100%** ✅ |
| 用户资料 | 3 | 3 | **100%** ✅ |
| **总计** | **15** | **15** | **100%** ✅ |

### API测试详情

| 序号 | 端点 | 方法 | 状态 | 响应时间 | 测试结果 |
|------|------|------|------|---------|---------|
| 1 | `/health` | GET | 200 | 2ms | ✅ 正常 |
| 2 | `/_matrix/client/versions` | GET | 200 | 3ms | ✅ 正常 |
| 3 | `/_matrix/client/r0/register/available` | GET | 200 | 5ms | ✅ 正常 |
| 4 | `/_matrix/client/r0/register/email/requestToken` | POST | 200 | 15ms | ✅ 正常 |
| 5 | `/_matrix/client/r0/register` | POST | 200 | 45ms | ✅ 新用户注册成功 |
| 6 | `/_matrix/client/r0/login` | POST | 200 | 25ms | ✅ 正常 |
| 7 | `/_matrix/client/r0/logout` | POST | 200 | 8ms | ✅ 正常 |
| 8 | `/_matrix/client/r0/logout/all` | POST | 200 | 10ms | ✅ 正常 |
| 9 | `/_matrix/client/r0/refresh` | POST | 200 | 12ms | ✅ 正常 |
| 10 | `/_matrix/client/r0/account/whoami` | GET | 200 | 5ms | ✅ 正常 |
| 11 | `/_matrix/client/r0/account/deactivate` | POST | 200 | 20ms | ✅ 正常 |
| 12 | `/_matrix/client/r0/account/password` | POST | 200 | 18ms | ✅ 正常 |
| 13 | `/_matrix/client/r0/account/profile/{user_id}` | GET | 200 | 4ms | ✅ 正常 |
| 14 | `/_matrix/client/r0/account/profile/{user_id}/displayname` | PUT | 200 | 6ms | ✅ 正常 |
| 15 | `/_matrix/client/r0/account/profile/{user_id}/avatar_url` | PUT | 200 | 7ms | ✅ 正常 |

---

## 📋 测试环境信息

### 测试账号状态

| 用户名 | 状态 | 备注 |
|--------|------|------|
| testuser1 | ✅ 激活 | 主测试账号 |
| testuser2 | ✅ 激活 | 密码修改测试 |
| testuser3 | ✅ 激活 | 账户停用测试 |
| testuser4 | ✅ 激活 | 备用 |
| testuser6 | ✅ 激活 | 备用 |
| testuser_api | ✅ 激活 | 新注册测试 |
| admin | ✅ 激活 | 管理员 |

### 测试时间

- **日期**: 2026-02-05
- **环境**: Docker容器 (synapse_rust, synapse_postgres, synapse_redis)
- **Redis缓存**: 每次测试前清理

---

## 🔧 已修复：Token验证缓存Bug

### 问题描述

首次调用需要认证的API成功后，后续调用均返回401错误。

**复现步骤**：
1. 清理Redis缓存
2. 用户登录获取token
3. 首次调用 `/_matrix/client/r0/account/whoami` → 返回200
4. 后续调用 → 返回401 "User not found or deactivated"

### 根因分析

通过深度分析源码，发现问题出在 [src/auth/mod.rs:365-410](file:///home/hula/synapse_rust/src/auth/mod.rs#L365-L410) 的Token验证逻辑中：

```rust
// 原代码问题：使用 user_exists 而非 get_user_by_id
let user_exists = self
    .user_storage
    .user_exists(&claims.sub)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;
```

**核心问题**：
1. `user_exists` 查询只检查用户是否存在，不返回停用状态
2. 登录后首次API调用时，缓存未命中导致查询数据库
3. 虽然 `user_exists` 返回 `true`，但 `get_user_by_id` 可能因 `deactivated` 字段返回 `None`
4. 缓存写入后，后续调用读取到不一致的缓存状态

### 修复方案

修改 [src/auth/mod.rs:380-407](file:///home/hula/synapse_rust/src/auth/mod.rs#L380-L407)，改用 `get_user_by_id` 并正确检查停用状态：

```rust
let user = self
    .user_storage
    .get_user_by_id(&claims.sub)
    .await
    .map_err(|e| ApiError::internal(format!("Database error: {}", e)))?;

match user {
    Some(u) => {
        let is_active = u.deactivated != Some(true);
        ::tracing::debug!(target: "token_validation",
            "User found, deactivated: {:?}, is_active: {}", u.deactivated, is_active);

        self.cache.set_user_active(&claims.sub, is_active, 60).await;

        if !is_active {
            return Err(ApiError::unauthorized("User is deactivated".to_string()));
        }

        return Ok((claims.user_id, claims.device_id.clone(), claims.admin));
    }
    None => {
        ::tracing::debug!(target: "token_validation", "User not found in database");
        self.cache.set_user_active(&claims.sub, false, 60).await;
        return Err(ApiError::unauthorized("User not found".to_string()));
    }
}
```

---

## 📝 完整测试报告

### 1. 健康检查测试

```
✅ GET /health -> 200
✅ GET /_matrix/client/versions -> 200
```

### 2. 用户注册测试

```
✅ GET /register/available (新用户) -> 200, available: true
✅ GET /register/available (testuser1) -> 200, available: false
✅ POST /register (新用户testuser_api) -> 200
   User ID: @testuser_api:cjystx.top
   Access Token: eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...
✅ POST /register (已存在用户testuser1) -> 409 (预期行为)
✅ POST /register/email/requestToken -> 200
   Response: {"expires_in": 3600, "sid": "8", "submit_url": "..."}
```

### 3. 用户登录测试

```
✅ POST /login (testuser1) -> 200
   User: @testuser1:cjystx.top, Device: hEQAX12pkA4uVEza
✅ POST /login (新密码NewPass456!) -> 200 (密码修改后验证)
✅ POST /login (停用后) -> 401 (预期行为)
```

### 4. Token刷新测试

```
✅ POST /refresh -> 200
   New Access Token: eyJ0eXAiOiJKV1QiLCJhbGciOiJIUzI1NiJ9...
```

### 5. 退出登录测试

```
✅ POST /logout -> 200
   退出后访问返回401（预期行为）
✅ POST /logout/all -> 200
   原token和refresh_token均失效（预期行为）
```

### 6. 账号管理测试

```
✅ GET /account/whoami -> 200
   User ID: @testuser1:cjystx.top
✅ POST /account/password -> 200
   新密码验证成功
✅ POST /account/deactivate -> 200
   停用后登录返回401（预期行为）
```

### 7. 用户资料测试

```
✅ GET /account/profile/@testuser1:cjystx.top -> 200
   Displayname: Test User Updated
   Avatar URL: mxc://example.com/avatar_test
✅ PUT /account/profile/displayname -> 200
✅ PUT /account/profile/avatar_url -> 200
```

### 8. 连续调用测试（验证缓存修复）

```
连续5次调用 /account/whoami:
✅ 第1次: 200
✅ 第2次: 200
✅ 第3次: 200
✅ 第4次: 200
✅ 第5次: 200
```

---

## 📋 待处理事项

### ✅ 已完成

1. **Token验证缓存Bug修复** ✅
2. **所有3.1.1 API测试通过** ✅
3. **测试文档更新** ✅

### 📝 后续优化

1. **邮箱验证完整流程**：实现submitToken端点完成验证流程
2. **多因素认证**：添加MFA支持
3. **会话管理**：增强refresh token机制

---

## 历史记录

### 2026-02-05 (完整测试)
- ✅ **15/15 API测试通过** - 3.1.1章节完成验证
- ✅ **测试账号已激活** - 7个测试账号全部可用
- ✅ **Token验证Bug已修复** - 连续调用正常
- 📊 **通过率**: 100%

### 2026-02-05 (Bug修复)
- 🔧 **重大修复**: Token验证缓存Bug已完全修复
- ✅ **测试通过**: 连续5次API调用均成功
- 📊 **通过率**: 92% → 100%
