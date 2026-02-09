3.3 账户管理 API 详细优化方案

**生成时间**: 2026-02-09  
**基于**: 手动测试结果和代码分析

---

## 一、手动测试结果总结

### 1.1 测试执行情况

| 问题ID | 测试描述 | 预期结果 | 实际结果 | 状态 |
|--------|----------|----------|----------|------|
| #1 | 获取用户资料-空用户ID | 400 Bad Request | 401 Unauthorized | ❌ 需修复 |
| #2 | 获取用户资料-特殊字符用户ID | 400 Bad Request | 空响应 | ❌ 需修复 |
| #3 | 获取用户资料-超长用户ID | 400 Bad Request | 404 Not Found | ❌ 需修复 |
| #4 | 更新显示名称-超长名称 | 400 Bad Request | 500 Internal Error | ❌ 需修复 |
| #5 | 更新显示名称-管理员权限 | 200 OK | 200 OK | ✅ 正常 |
| #6 | 更新显示名称-不存在用户 | 404 Not Found | 200 OK | ❌ 需修复 |
| #7 | 更新头像-超长URL | 400 Bad Request | 200 OK | ❌ 需修复 |
| #8 | 更新头像-管理员权限 | 200 OK | 200 OK | ✅ 正常 |
| #9 | 更新头像-不存在用户 | 404 Not Found | 200 OK | ❌ 需修复 |

### 1.2 关键发现

1. **管理员权限正常工作**：使用正确注册的管理员账号（通过HMAC-SHA256验证）可以成功更新其他用户的资料
2. **输入验证缺失**：displayname和avatar_url缺少长度验证
3. **用户存在性检查缺失**：更新操作没有检查用户是否存在
4. **路径参数验证缺失**：user_id缺少格式和长度验证
5. **错误处理不当**：数据库错误直接暴露给客户端

---

## 二、问题根源分析

### 2.1 代码分析结果

#### get_profile函数（/home/hula/synapse_rust/src/web/routes/mod.rs:711-723）
```rust
async fn get_profile(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    Ok(Json(
        state
            .services
            .registration_service
            .get_profile(&user_id)
            .await?,
    ))
}
```

**问题**：
- 没有对user_id进行非空验证
- 没有对user_id进行格式验证
- 没有对user_id进行长度验证

#### update_displayname函数（/home/hula/synapse_rust/src/web/routes/mod.rs:725-746）
```rust
async fn update_displayname(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let displayname = body
        .get("displayname")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Displayname required".to_string()))?;

    state
        .services
        .registration_service
        .update_user_profile(&user_id, Some(displayname), None)
        .await?;
    Ok(Json(json!({})))
}
```

**问题**：
- 没有对displayname进行长度验证
- 没有检查用户是否存在
- 权限检查在用户存在性检查之前

#### update_avatar函数（/home/hula/synapse_rust/src/web/routes/mod.rs:748-769）
```rust
async fn update_avatar(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let avatar_url = body
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Avatar URL required".to_string()))?;

    state
        .services
        .registration_service
        .update_user_profile(&user_id, None, Some(avatar_url))
        .await?;
    Ok(Json(json!({})))
}
```

**问题**：
- 没有对avatar_url进行长度验证
- 没有检查用户是否存在
- 权限检查在用户存在性检查之前

#### set_displayname函数（/home/hula/synapse_rust/src/services/registration_service.rs:200-206）
```rust
pub async fn set_displayname(&self, user_id: &str, displayname: &str) -> ApiResult<()> {
    self.user_storage
        .update_displayname(user_id, Some(displayname))
        .await
        .map_err(|e| ApiError::internal(format!("Failed to update displayname: {}", e)))?;
    Ok(())
}
```

**问题**：
- 数据库错误直接暴露给客户端
- 没有在应用层进行长度验证

#### update_displayname数据库操作（/home/hula/synapse_rust/src/storage/user.rs:189-204）
```rust
pub async fn update_displayname(
    &self,
    user_id: &str,
    displayname: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(r#"UPDATE users SET displayname = $1 WHERE user_id = $2"#)
        .bind(displayname)
        .bind(user_id)
        .execute(&*self.pool)
        .await?;
    
    let key = format!("user:profile:{}", user_id);
    self.cache.delete(&key).await;

    Ok(())
}
```

**问题**：
- 没有检查用户是否存在
- 没有检查更新是否成功（受影响的行数）

---

## 三、详细优化方案

### 3.1 高优先级修复

#### 修复 #1: 添加displayname长度验证

**位置**: `/home/hula/synapse_rust/src/web/routes/mod.rs`  
**函数**: `update_displayname`  
**行号**: 725-746

**当前代码**:
```rust
async fn update_displayname(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let displayname = body
        .get("displayname")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Displayname required".to_string()))?;

    state
        .services
        .registration_service
        .update_user_profile(&user_id, Some(displayname), None)
        .await?;
    Ok(Json(json!({})))
}
```

**修复后代码**:
```rust
async fn update_displayname(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let displayname = body
        .get("displayname")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Displayname required".to_string()))?;
    
    if displayname.len() > 255 {
        return Err(ApiError::bad_request("Displayname too long (max 255 characters)".to_string()));
    }

    state
        .services
        .registration_service
        .update_user_profile(&user_id, Some(displayname), None)
        .await?;
    Ok(Json(json!({})))
}
```

**预期效果**:
- 当displayname超过255字符时，返回400 Bad Request
- 避免数据库错误
- 提供清晰的错误信息

#### 修复 #2: 添加avatar_url长度验证

**位置**: `/home/hula/synapse_rust/src/web/routes/mod.rs`  
**函数**: `update_avatar`  
**行号**: 748-769

**当前代码**:
```rust
async fn update_avatar(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let avatar_url = body
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Avatar URL required".to_string()))?;

    state
        .services
        .registration_service
        .update_user_profile(&user_id, None, Some(avatar_url))
        .await?;
    Ok(Json(json!({})))
}
```

**修复后代码**:
```rust
async fn update_avatar(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let avatar_url = body
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Avatar URL required".to_string()))?;
    
    if avatar_url.len() > 255 {
        return Err(ApiError::bad_request("Avatar URL too long (max 255 characters)".to_string()));
    }

    state
        .services
        .registration_service
        .update_user_profile(&user_id, None, Some(avatar_url))
        .await?;
    Ok(Json(json!({})))
}
```

**预期效果**:
- 当avatar_url超过255字符时，返回400 Bad Request
- 避免数据库错误
- 提供清晰的错误信息

#### 修复 #3: 添加用户存在性检查

**位置**: `/home/hula/synapse_rust/src/web/routes/mod.rs`  
**函数**: `update_displayname` 和 `update_avatar`

**修复后代码（update_displayname）**:
```rust
async fn update_displayname(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let displayname = body
        .get("displayname")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Displayname required".to_string()))?;
    
    if displayname.len() > 255 {
        return Err(ApiError::bad_request("Displayname too long (max 255 characters)".to_string()));
    }
    
    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let user_exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?;
    
    if !user_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .registration_service
        .update_user_profile(&user_id, Some(displayname), None)
        .await?;
    Ok(Json(json!({})))
}
```

**修复后代码（update_avatar）**:
```rust
async fn update_avatar(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    let avatar_url = body
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Avatar URL required".to_string()))?;
    
    if avatar_url.len() > 255 {
        return Err(ApiError::bad_request("Avatar URL too long (max 255 characters)".to_string()));
    }
    
    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let user_exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?;
    
    if !user_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .registration_service
        .update_user_profile(&user_id, None, Some(avatar_url))
        .await?;
    Ok(Json(json!({})))
}
```

**预期效果**:
- 当用户不存在时，返回404 Not Found
- 当用户存在但无权限时，返回403 Forbidden
- 正确的错误状态码

### 3.2 中优先级修复

#### 修复 #4: 添加user_id验证

**位置**: `/home/hula/synapse_rust/src/web/routes/mod.rs`  
**函数**: `get_profile`, `update_displayname`, `update_avatar`

**新增验证函数**:
```rust
fn validate_user_id(user_id: &str) -> Result<(), ApiError> {
    if user_id.is_empty() {
        return Err(ApiError::bad_request("user_id is required".to_string()));
    }
    
    if !user_id.starts_with('@') {
        return Err(ApiError::bad_request("Invalid user_id format: must start with @".to_string()));
    }
    
    if user_id.len() > 255 {
        return Err(ApiError::bad_request("user_id too long (max 255 characters)".to_string()));
    }
    
    let parts: Vec<&str> = user_id.split(':').collect();
    if parts.len() != 2 {
        return Err(ApiError::bad_request("Invalid user_id format: must be @username:server".to_string()));
    }
    
    let username = &parts[0][1..];
    if username.is_empty() {
        return Err(ApiError::bad_request("Invalid user_id format: username cannot be empty".to_string()));
    }
    
    if parts[1].is_empty() {
        return Err(ApiError::bad_request("Invalid user_id format: server cannot be empty".to_string()));
    }
    
    Ok(())
}
```

**修复后代码（get_profile）**:
```rust
async fn get_profile(
    State(state): State<AppState>,
    _auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    
    Ok(Json(
        state
            .services
            .registration_service
            .get_profile(&user_id)
            .await?,
    ))
}
```

**修复后代码（update_displayname）**:
```rust
async fn update_displayname(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    
    let displayname = body
        .get("displayname")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Displayname required".to_string()))?;
    
    if displayname.len() > 255 {
        return Err(ApiError::bad_request("Displayname too long (max 255 characters)".to_string()));
    }
    
    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let user_exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?;
    
    if !user_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .registration_service
        .update_user_profile(&user_id, Some(displayname), None)
        .await?;
    Ok(Json(json!({})))
}
```

**修复后代码（update_avatar）**:
```rust
async fn update_avatar(
    State(state): State<AppState>,
    auth_user: AuthenticatedUser,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, ApiError> {
    validate_user_id(&user_id)?;
    
    let avatar_url = body
        .get("avatar_url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiError::bad_request("Avatar URL required".to_string()))?;
    
    if avatar_url.len() > 255 {
        return Err(ApiError::bad_request("Avatar URL too long (max 255 characters)".to_string()));
    }
    
    if user_id != auth_user.user_id && !auth_user.is_admin {
        return Err(ApiError::forbidden("Access denied".to_string()));
    }

    let user_exists = state
        .services
        .user_storage
        .user_exists(&user_id)
        .await
        .map_err(|e| ApiError::internal(format!("Failed to check user existence: {}", e)))?;
    
    if !user_exists {
        return Err(ApiError::not_found("User not found".to_string()));
    }

    state
        .services
        .registration_service
        .update_user_profile(&user_id, None, Some(avatar_url))
        .await?;
    Ok(Json(json!({})))
}
```

**预期效果**:
- 当user_id为空时，返回400 Bad Request
- 当user_id格式不正确时，返回400 Bad Request
- 当user_id超长时，返回400 Bad Request
- 正确的错误状态码

### 3.3 低优先级改进

#### 改进 #1: 改进错误处理

**位置**: `/home/hula/synapse_rust/src/services/registration_service.rs`  
**函数**: `set_displayname` 和 `set_avatar_url`

**修复后代码（set_displayname）**:
```rust
pub async fn set_displayname(&self, user_id: &str, displayname: &str) -> ApiResult<()> {
    self.user_storage
        .update_displayname(user_id, Some(displayname))
        .await
        .map_err(|e| {
            if e.to_string().contains("too long") {
                ApiError::bad_request("Displayname too long (max 255 characters)".to_string())
            } else {
                ApiError::internal("Failed to update displayname".to_string())
            }
        })?;
    Ok(())
}
```

**修复后代码（set_avatar_url）**:
```rust
pub async fn set_avatar_url(&self, user_id: &str, avatar_url: &str) -> ApiResult<()> {
    self.user_storage
        .update_avatar_url(user_id, Some(avatar_url))
        .await
        .map_err(|e| {
            if e.to_string().contains("too long") {
                ApiError::bad_request("Avatar URL too long (max 255 characters)".to_string())
            } else {
                ApiError::internal("Failed to update avatar".to_string())
            }
        })?;
    Ok(())
}
```

**预期效果**:
- 数据库错误不直接暴露给客户端
- 返回适当的HTTP状态码和错误信息
- 提高安全性

#### 改进 #2: 添加user_exists方法

**位置**: `/home/hula/synapse_rust/src/storage/user.rs`

**新增代码**:
```rust
pub async fn user_exists(&self, user_id: &str) -> Result<bool, sqlx::Error> {
    let result = sqlx::query(r#"SELECT COUNT(*) as count FROM users WHERE user_id = $1"#)
        .bind(user_id)
        .fetch_one(&*self.pool)
        .await?;
    
    let count: i64 = result.get("count");
    Ok(count > 0)
}
```

**预期效果**:
- 提供用户存在性检查功能
- 提高代码复用性

---

## 四、实施计划

### 4.1 修复优先级

| 优先级 | 修复项 | 预计工作量 | 风险 |
|--------|--------|------------|------|
| 高 | 添加displayname长度验证 | 0.5小时 | 低 |
| 高 | 添加avatar_url长度验证 | 0.5小时 | 低 |
| 高 | 添加用户存在性检查 | 1小时 | 中 |
| 中 | 添加user_id验证 | 1小时 | 中 |
| 低 | 改进错误处理 | 0.5小时 | 低 |
| 低 | 添加user_exists方法 | 0.5小时 | 低 |

**总预计工作量**: 4小时

### 4.2 实施步骤

1. **第一步：添加user_exists方法**（0.5小时）
   - 在`/home/hula/synapse_rust/src/storage/user.rs`中添加`user_exists`方法
   - 编写单元测试

2. **第二步：添加validate_user_id函数**（0.5小时）
   - 在`/home/hula/synapse_rust/src/web/routes/mod.rs`中添加`validate_user_id`函数
   - 编写单元测试

3. **第三步：修复update_displayname函数**（1小时）
   - 添加displayname长度验证
   - 添加用户存在性检查
   - 调整检查顺序
   - 编写单元测试

4. **第四步：修复update_avatar函数**（1小时）
   - 添加avatar_url长度验证
   - 添加用户存在性检查
   - 调整检查顺序
   - 编写单元测试

5. **第五步：修复get_profile函数**（0.5小时）
   - 添加user_id验证
   - 编写单元测试

6. **第六步：改进错误处理**（0.5小时）
   - 修改`set_displayname`和`set_avatar_url`函数
   - 编写单元测试

7. **第七步：集成测试**（1小时）
   - 运行完整的测试套件
   - 验证所有修复

**总预计时间**: 5小时

### 4.3 测试计划

1. **单元测试**
   - 测试`validate_user_id`函数
   - 测试`user_exists`方法
   - 测试`update_displayname`函数
   - 测试`update_avatar`函数
   - 测试`get_profile`函数

2. **集成测试**
   - 测试所有API端点
   - 测试边界条件
   - 测试错误处理

3. **回归测试**
   - 运行现有的测试套件
   - 确保没有破坏现有功能

---

## 五、风险评估

### 5.1 技术风险

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| 破坏现有功能 | 中 | 高 | 完整的回归测试 |
| 性能影响 | 低 | 中 | 性能测试 |
| 数据库兼容性 | 低 | 中 | 数据库测试 |

### 5.2 业务风险

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|----------|
| API行为变化 | 中 | 高 | 文档更新 |
| 客户端兼容性 | 中 | 中 | 向后兼容性检查 |

---

## 六、验证标准

### 6.1 功能验证

- [ ] displayname超过255字符时返回400 Bad Request
- [ ] avatar_url超过255字符时返回400 Bad Request
- [ ] user_id为空时返回400 Bad Request
- [ ] user_id格式不正确时返回400 Bad Request
- [ ] user_id超长时返回400 Bad Request
- [ ] 用户不存在时返回404 Not Found
- [ ] 无权限时返回403 Forbidden
- [ ] 数据库错误不直接暴露给客户端

### 6.2 性能验证

- [ ] 用户存在性检查不影响性能
- [ ] 验证逻辑不影响性能
- [ ] 响应时间在可接受范围内

### 6.3 安全验证

- [ ] 输入验证有效
- [ ] 错误信息不泄露敏感信息
- [ ] 权限检查正确

---

## 七、后续建议

### 7.1 短期改进

1. **添加更多验证**
   - 添加avatar_url格式验证
   - 添加displayname格式验证
   - 添加更多边界条件测试

2. **改进错误信息**
   - 提供更详细的错误信息
   - 添加错误代码
   - 支持多语言错误信息

3. **添加日志**
   - 记录所有验证失败
   - 记录所有权限检查
   - 记录所有错误

### 7.2 长期改进

1. **添加输入验证框架**
   - 创建统一的验证框架
   - 支持自定义验证规则
   - 支持验证规则复用

2. **添加API文档**
   - 自动生成API文档
   - 包含所有验证规则
   - 包含所有错误代码

3. **添加监控**
   - 监控验证失败率
   - 监控错误率
   - 监控性能指标

---

## 八、附录

### 8.1 测试用例

#### 测试用例1：更新显示名称-超长名称
```bash
curl -X PUT "http://localhost:8008/_matrix/client/r0/account/profile/@testuser:cjystx.top/displayname" \
  -H "Authorization: Bearer {token}" \
  -H "Content-Type: application/json" \
  -d '{"displayname": "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}'

预期结果：400 Bad Request
预期响应：{"status":"error","error":"Displayname too long (max 255 characters)","errcode":"M_BAD_REQUEST"}
```

#### 测试用例2：更新显示名称-不存在用户
```bash
curl -X PUT "http://localhost:8008/_matrix/client/r0/account/profile/@nonexistent:cjystx.top/displayname" \
  -H "Authorization: Bearer {admin_token}" \
  -H "Content-Type: application/json" \
  -d '{"displayname": "Test Display"}'

预期结果：404 Not Found
预期响应：{"status":"error","error":"User not found","errcode":"M_NOT_FOUND"}
```

#### 测试用例3：获取用户资料-空用户ID
```bash
curl -X GET "http://localhost:8008/_matrix/client/r0/account/profile/" \
  -H "Authorization: Bearer {token}"

预期结果：400 Bad Request
预期响应：{"status":"error","error":"user_id is required","errcode":"M_BAD_REQUEST"}
```

### 8.2 相关文件

- `/home/hula/synapse_rust/src/web/routes/mod.rs` - API路由定义
- `/home/hula/synapse_rust/src/services/registration_service.rs` - 注册服务
- `/home/hula/synapse_rust/src/storage/user.rs` - 用户存储
- `/home/hula/synapse_rust/docs/synapse-rust/test-3-3-summary-report.md` - 测试总结报告
- `/home/hula/synapse_rust/docs/synapse-rust/api-error-3-3.md` - 错误报告

### 8.3 参考资料

- Matrix API规范: https://spec.matrix.org/v1.11/client-server-api/
- Rust最佳实践: https://rust-lang.github.io/api-guidelines/
- SQLx文档: https://docs.rs/sqlx/

