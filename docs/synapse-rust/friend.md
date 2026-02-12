## Synapse Rust 项目优化方案

**文档版本**: 1.0  
**制定日期**: 2026-02-12  
**基于文档**: api-error.md

---

## 1. 优化目标

基于 API 测试结果，解决以下关键问题：
1. 修复好友系统数据库外键约束冲突问题
2. 改进错误处理机制，使用适当的 HTTP 状态码
3. 修复邮箱验证数据库字段缺失问题
4. 确保所有测试用例通过，实现项目功能完美运行

---

## 2. 问题分析

### 2.1 好友系统数据库外键约束问题

**影响范围**: 所有涉及好友列表更新的操作
- POST /_matrix/client/r0/friends (添加好友)
- DELETE /_matrix/client/r0/friends/{user_id} (删除好友)
- POST /_matrix/client/r0/friends/requests (发送好友请求)
- POST /_matrix/client/r0/friends/requests/{user_id}/accept (接受好友请求)
- POST /_matrix/client/r0/friends/requests/{user_id}/reject (拒绝好友请求)
- POST /_matrix/client/r0/friends/legacy/add (添加好友 Legacy)
- DELETE /_matrix/client/r0/friends/legacy/{friend_id} (删除好友 Legacy)
- POST /_matrix/client/r0/friends/legacy/requests (发送好友请求 Legacy)
- POST /_matrix/client/r0/friends/legacy/requests/{request_id}/accept (接受好友请求 Legacy)
- POST /_matrix/client/r0/friends/legacy/requests/{request_id}/reject (拒绝好友请求 Legacy)

**错误信息**:
```
error returned from database: insert or update on table "events" violates foreign key constraint "events_room_id_fkey"
```

**根本原因分析**:
- 好友系统在创建事件时引用了不存在的房间ID
- 可能是好友系统直接聊天房间创建逻辑与事件创建逻辑不一致
- 需要检查好友系统的事件创建流程

### 2.2 错误处理不当问题

**影响范围**: 业务逻辑错误
- PUT /_matrix/client/r0/friends/{user_id}/note (更新好友备注)
- PUT /_matrix/client/r0/friends/{user_id}/status (更新好友状态)

**错误信息**:
```
Internal error: Failed to update friend note: Not found: Friend @apitest_user2:cjystx.top not found in list
```

**根本原因分析**:
- 业务逻辑错误（好友不存在）被当作内部错误（500）返回
- 应使用 404 Not Found 状态码和 `M_NOT_FOUND` 错误代码
- 错误处理机制需要改进

### 2.3 邮箱验证数据库字段缺失问题

**影响范围**: 邮箱验证功能
- POST /_matrix/client/r0/account/3pid/email/requestToken

**错误信息**:
```
column "expires_at" of relation "email_verification_tokens" does not exist
```

**根本原因分析**:
- 数据库表 `email_verification_tokens` 缺少 `expires_at` 字段
- 需要运行数据库迁移或更新表结构

---

## 3. 优化方案

### 3.1 好友系统数据库外键约束修复方案

#### 3.1.1 问题定位

需要检查以下文件：
- `src/storage/friend_room.rs` - 好友系统存储层
- `src/web/routes/friend_room.rs` - 好友系统路由层
- `src/federation/friend/friend_federation.rs` - 好友联邦层

#### 3.1.2 修复步骤

**步骤1**: 检查好友系统事件创建逻辑
```rust
// 需要检查的函数
- create_friend_event()
- remove_friend_event()
- send_friend_request_event()
- accept_friend_request_event()
- reject_friend_request_event()
```

**步骤2**: 确保直接聊天房间在事件创建前已存在
```rust
// 伪代码示例
async fn create_friend_event(&self, user_id: &str, friend_id: &str) -> Result<Event> {
    // 1. 检查是否已有直接聊天房间
    let direct_room = self.get_or_create_direct_room(user_id, friend_id).await?;
    
    // 2. 使用已存在的房间ID创建事件
    let event = Event {
        room_id: direct_room.id,
        // ... 其他字段
    };
    
    // 3. 插入事件
    self.db.insert_event(event).await?;
    
    Ok(event)
}
```

**步骤3**: 添加事务处理确保数据一致性
```rust
// 使用数据库事务
async fn update_friend_list_with_transaction(&self, user_id: &str, friend_id: &str) -> Result<()> {
    let mut tx = self.db.begin().await?;
    
    // 1. 获取或创建直接聊天房间
    let room = tx.get_or_create_direct_room(user_id, friend_id).await?;
    
    // 2. 更新好友列表
    tx.update_friend_list(user_id, friend_id, room.id).await?;
    
    // 3. 创建事件
    tx.create_friend_event(user_id, friend_id, room.id).await?;
    
    // 4. 提交事务
    tx.commit().await?;
    
    Ok(())
}
```

#### 3.1.3 测试验证

修复后需要验证以下场景：
- 添加好友成功
- 删除好友成功
- 发送好友请求成功
- 接受好友请求成功
- 拒绝好友请求成功
- Legacy API 操作成功

### 3.2 错误处理改进方案

#### 3.2.1 错误码映射表

| 业务逻辑错误 | 当前状态码 | 期望状态码 | 期望错误代码 |
|-------------|-----------|-----------|-------------|
| 好友不存在 | 500 | 404 | M_NOT_FOUND |
| 好友请求不存在 | 500 | 404 | M_NOT_FOUND |
| 用户不存在 | 500 | 404 | M_NOT_FOUND |
| 房间不存在 | 500 | 404 | M_NOT_FOUND |
| 无效参数 | 400 | 400 | M_BAD_JSON |
| 参数缺失 | 400 | 400 | M_BAD_JSON |

#### 3.2.2 错误处理函数改进

```rust
// 创建统一的错误处理函数
use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

pub enum ApiError {
    NotFound(String),
    BadRequest(String),
    Unauthorized(String),
    Forbidden(String),
    Conflict(String),
    InternalError(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, errcode, message) = match self {
            ApiError::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                "M_NOT_FOUND",
                msg.clone(),
            ),
            ApiError::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "M_BAD_JSON",
                msg.clone(),
            ),
            ApiError::Unauthorized(msg) => (
                StatusCode::UNAUTHORIZED,
                "M_UNAUTHORIZED",
                msg.clone(),
            ),
            ApiError::Forbidden(msg) => (
                StatusCode::FORBIDDEN,
                "M_FORBIDDEN",
                msg.clone(),
            ),
            ApiError::Conflict(msg) => (
                StatusCode::CONFLICT,
                "M_USER_IN_USE",
                msg.clone(),
            ),
            ApiError::InternalError(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "M_INTERNAL_ERROR",
                msg.clone(),
            ),
        };

        let body = json!({
            "status": "error",
            "error": message,
            "errcode": errcode,
        });

        (status, Json(body)).into_response()
    }
}

// 使用示例
pub async fn update_friend_note(
    State(state): State<AppState>,
    Path(user_id): Path<String>,
    Json(payload): Json<UpdateNoteRequest>,
) -> impl IntoResponse {
    // 检查好友是否存在
    let friend_exists = state.db.friend_exists(&user_id).await;
    if !friend_exists {
        return ApiError::NotFound(format!("Friend {} not found in list", user_id));
    }
    
    // 更新好友备注
    match state.db.update_friend_note(&user_id, &payload.note).await {
        Ok(_) => (StatusCode::OK, Json(json!({}))).into_response(),
        Err(e) => ApiError::InternalError(format!("Failed to update friend note: {}", e)),
    }
}
```

### 3.3 邮箱验证数据库修复方案

#### 3.3.1 数据库迁移脚本

```sql
-- 添加 expires_at 字段到 email_verification_tokens 表
ALTER TABLE email_verification_tokens 
ADD COLUMN expires_at TIMESTAMP WITH TIME ZONE;

-- 为现有记录设置默认过期时间（例如 24 小时后）
UPDATE email_verification_tokens 
SET expires_at = created_at + INTERVAL '24 hours' 
WHERE expires_at IS NULL;

-- 添加索引以提高查询性能
CREATE INDEX idx_email_verification_tokens_expires_at 
ON email_verification_tokens(expires_at);
```

#### 3.3.2 Rust 代码更新

```rust
// 更新 EmailVerificationToken 结构体
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct EmailVerificationToken {
    pub id: i64,
    pub email: String,
    pub token: String,
    pub client_secret: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,  // 新增字段
}

// 更新存储函数
impl Storage {
    pub async fn store_email_verification_token(
        &self,
        email: &str,
        token: &str,
        client_secret: &str,
    ) -> Result<EmailVerificationToken> {
        let now = Utc::now();
        let expires_at = now + Duration::hours(24);  // 24 小时后过期
        
        sqlx::query!(
            r#"
            INSERT INTO email_verification_tokens (email, token, client_secret, created_at, expires_at)
            VALUES ($1, $2, $3, $4, $5)
            RETURNING *
            "#,
            email,
            token,
            client_secret,
            now,
            expires_at
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))
    }
    
    pub async fn verify_email_token(
        &self,
        token: &str,
    ) -> Result<EmailVerificationToken> {
        let now = Utc::now();
        
        sqlx::query!(
            r#"
            DELETE FROM email_verification_tokens 
            WHERE token = $1 
            AND (expires_at IS NULL OR expires_at > $2)
            RETURNING *
            "#,
            token,
            now
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| Error::DatabaseError(e.to_string()))?
        .ok_or_else(|| Error::NotFound("Token not found or expired".to_string()))
    }
}
```

---

## 4. 测试用例设计

### 4.1 好友系统测试用例

#### 4.1.1 添加好友测试用例

| 用例ID | 测试场景 | 请求参数 | 预期结果 | 验证点 |
|--------|---------|-----------|---------|---------|
| TC-FR-001 | 有效Token添加好友 | {"user_id": "@valid_user"} | 200 OK | 成功添加好友 |
| TC-FR-002 | 无Token添加好友 | {"user_id": "@valid_user"} | 401 UNAUTHORIZED | 认证失败 |
| TC-FR-003 | 无效Token添加好友 | {"user_id": "@valid_user"} | 401 UNAUTHORIZED | 认证失败 |
| TC-FR-004 | 添加自己为好友 | {"user_id": "@self_user"} | 400 BAD_REQUEST | 参数验证 |
| TC-FR-005 | 添加不存在的用户 | {"user_id": "@nonexistent"} | 404 NOT_FOUND | 用户不存在 |
| TC-FR-006 | 空user_id | {"user_id": ""} | 400 BAD_REQUEST | 参数验证 |
| TC-FR-007 | 缺少user_id字段 | {} | 400 BAD_REQUEST | 参数验证 |
| TC-FR-008 | 无效user_id格式 | {"user_id": "invalid_format"} | 400 BAD_REQUEST | 参数验证 |
| TC-FR-009 | 超长user_id | {"user_id": "@very_long_username..."} | 400 BAD_REQUEST | 参数验证 |

#### 4.1.2 删除好友测试用例

| 用例ID | 测试场景 | 请求参数 | 预期结果 | 验证点 |
|--------|---------|-----------|---------|---------|
| TC-FR-010 | 有效Token删除好友 | N/A | 200 OK | 成功删除好友 |
| TC-FR-011 | 无Token删除好友 | N/A | 401 UNAUTHORIZED | 认证失败 |
| TC-FR-012 | 删除不存在的用户 | N/A | 404 NOT_FOUND | 好友不存在 |
| TC-FR-013 | 删除非好友用户 | N/A | 404 NOT_FOUND | 好友不存在 |

#### 4.1.3 更新好友备注测试用例

| 用例ID | 测试场景 | 请求参数 | 预期结果 | 验证点 |
|--------|---------|-----------|---------|---------|
| TC-FR-014 | 有效Token更新备注 | {"note": "Best friend"} | 200 OK | 成功更新备注 |
| TC-FR-015 | 无Token更新备注 | {"note": "test"} | 401 UNAUTHORIZED | 认证失败 |
| TC-FR-016 | 更新不存在的用户备注 | {"note": "test"} | 404 NOT_FOUND | 好友不存在 |
| TC-FR-017 | 空备注 | {"note": ""} | 200 OK | 允许空备注 |
| TC-FR-018 | 超长备注(1000字符) | {"note": "a..."} | 400 BAD_REQUEST | 参数验证 |

#### 4.1.4 发送好友请求测试用例

| 用例ID | 测试场景 | 请求参数 | 预期结果 | 验证点 |
|--------|---------|-----------|---------|---------|
| TC-FR-019 | 有效Token发送请求 | {"user_id": "@valid_user", "message": "Hi"} | 200 OK | 成功发送请求 |
| TC-FR-020 | 无Token发送请求 | {"user_id": "@valid_user"} | 401 UNAUTHORIZED | 认证失败 |
| TC-FR-021 | 发送给自己 | {"user_id": "@self_user"} | 400 BAD_REQUEST | 参数验证 |
| TC-FR-022 | 发送给不存在的用户 | {"user_id": "@nonexistent"} | 404 NOT_FOUND | 用户不存在 |
| TC-FR-023 | 空message | {"user_id": "@valid_user", "message": ""} | 200 OK | 允许空消息 |
| TC-FR-024 | 超长message | {"user_id": "@valid_user", "message": "a..."} | 400 BAD_REQUEST | 参数验证 |

#### 4.1.5 接受/拒绝好友请求测试用例

| 用例ID | 测试场景 | 请求参数 | 预期结果 | 验证点 |
|--------|---------|-----------|---------|---------|
| TC-FR-025 | 有效Token接受请求 | {} | 200 OK | 成功接受请求 |
| TC-FR-026 | 有效Token拒绝请求 | {} | 200 OK | 成功拒绝请求 |
| TC-FR-027 | 接受不存在的请求 | {} | 404 NOT_FOUND | 请求不存在 |
| TC-FR-028 | 拒绝不存在的请求 | {} | 404 NOT_FOUND | 请求不存在 |

### 4.2 邮箱验证测试用例

| 用例ID | 测试场景 | 请求参数 | 预期结果 | 验证点 |
|--------|---------|-----------|---------|---------|
| TC-EM-001 | 有效邮箱请求Token | {"email": "valid@example.com", "client_secret": "secret"} | 200 OK | 成功发送验证邮件 |
| TC-EM-002 | 无效邮箱格式 | {"email": "invalid_email", "client_secret": "secret"} | 400 BAD_REQUEST | 参数验证 |
| TC-EM-003 | 空邮箱 | {"email": "", "client_secret": "secret"} | 400 BAD_REQUEST | 参数验证 |
| TC-EM-004 | 缺少client_secret | {"email": "valid@example.com"} | 400 BAD_REQUEST | 参数验证 |
| TC-EM-005 | 提交有效Token | {"token": "valid_token"} | 200 OK | 成功验证邮箱 |
| TC-EM-006 | 提交过期Token | {"token": "expired_token"} | 400 BAD_REQUEST | Token已过期 |
| TC-EM-007 | 提交无效Token | {"token": "invalid_token"} | 404 NOT_FOUND | Token不存在 |

---

## 5. 实施计划

### 5.1 第一阶段：数据库修复（优先级：高）

**时间估计**: 2-3 天

**任务列表**:
1. 分析 `events` 表的外键约束定义
2. 检查好友系统事件创建逻辑
3. 实现直接聊天房间创建与事件创建的同步机制
4. 创建数据库迁移脚本添加 `expires_at` 字段
5. 执行数据库迁移

**验收标准**:
- 数据库迁移成功执行
- 外键约束不再触发
- 直接聊天房间正确创建

### 5.2 第二阶段：错误处理改进（优先级：高）

**时间估计**: 1-2 天

**任务列表**:
1. 创建统一的错误类型定义
2. 实现 `IntoResponse` trait 错误处理
3. 更新所有好友系统 API 端点使用新的错误处理
4. 更新邮箱验证 API 端点使用新的错误处理

**验收标准**:
- 所有业务逻辑错误返回正确的 HTTP 状态码
- 错误响应格式统一
- 错误代码符合 Matrix 规范

### 5.3 第三阶段：单元测试（优先级：高）

**时间估计**: 2-3 天

**任务列表**:
1. 为好友系统存储层编写单元测试
2. 为错误处理函数编写单元测试
3. 为邮箱验证功能编写单元测试
4. 确保测试覆盖率达到 80% 以上

**验收标准**:
- 所有单元测试通过
- 测试覆盖率达到 80% 以上
- CI/CD 流程正常运行

### 5.4 第四阶段：集成测试（优先级：中）

**时间估计**: 2-3 天

**任务列表**:
1. 执行所有好友系统 API 测试用例
2. 执行所有邮箱验证 API 测试用例
3. 验证所有测试用例通过
4. 性能测试确保响应时间在可接受范围内

**验收标准**:
- 所有测试用例通过（100%）
- 响应时间 < 500ms (P95)
- 无内存泄漏
- 无并发问题

### 5.5 第五阶段：文档更新（优先级：低）

**时间估计**: 1 天

**任务列表**:
1. 更新 API 文档反映修复后的行为
2. 更新错误码文档
3. 创建优化报告文档

**验收标准**:
- API 文档准确反映实际行为
- 错误码文档完整
- 优化报告文档完成

---

## 6. 风险评估

### 6.1 技术风险

| 风险项 | 可能性 | 影响 | 缓解措施 |
|---------|--------|------|---------|
| 数据库迁移失败 | 中 | 高 | 备份数据库、测试迁移脚本 |
| 外键约束修改影响其他功能 | 低 | 中 | 全面回归测试 |
| 错误处理变更影响客户端 | 低 | 低 | 保持错误响应格式兼容性 |
| 性能下降 | 低 | 中 | 性能测试、优化查询 |

### 6.2 业务风险

| 风险项 | 可能性 | 影响 | 缓解措施 |
|---------|--------|------|---------|
| 修复过程中服务中断 | 低 | 高 | 使用蓝绿部署、灰度发布 |
| 数据丢失 | 低 | 高 | 完整备份、回滚方案 |
| 兼容性问题 | 低 | 中 | 版本控制、逐步发布 |

---

## 7. 回滚方案

### 7.1 数据库回滚

如果数据库迁移出现问题：
```sql
-- 回滚 expires_at 字段添加
ALTER TABLE email_verification_tokens DROP COLUMN expires_at;

-- 回滚索引删除
DROP INDEX IF EXISTS idx_email_verification_tokens_expires_at;
```

### 7.2 代码回滚

使用 Git 版本控制：
```bash
# 查看当前版本
git log --oneline -10

# 回滚到修复前的版本
git revert <commit-hash>

# 或创建新分支进行修复
git checkout -b fix-friend-system <base-commit>
```

---

## 8. 成功标准

### 8.1 功能标准

- ✅ 所有好友系统 API 测试用例通过（100%）
- ✅ 所有邮箱验证 API 测试用例通过（100%）
- ✅ 无数据库外键约束错误
- ✅ 错误处理返回正确的 HTTP 状态码
- ✅ 直接聊天房间正确创建

### 8.2 质量标准

- ✅ 单元测试覆盖率 ≥ 80%
- ✅ 集成测试覆盖率 ≥ 90%
- ✅ 代码审查通过
- ✅ 无严重安全问题
- ✅ 无内存泄漏

### 8.3 性能标准

- ✅ API 响应时间 P95 < 500ms
- ✅ 数据库查询时间 P95 < 100ms
- ✅ 并发支持 ≥ 1000 QPS
- ✅ CPU 使用率 < 70%

---

## 9. 优化报告模板

优化完成后，需要填写以下报告：

### 9.1 修复总结

| 问题ID | 问题描述 | 修复方案 | 修复状态 |
|--------|---------|---------|---------|
| ISSUE-001 | 好友系统数据库外键约束 | 实现直接聊天房间创建与事件创建同步 | 待修复 |
| ISSUE-002 | 错误处理不当 | 实现统一错误处理机制 | 待修复 |
| ISSUE-003 | 邮箱验证数据库字段缺失 | 添加 expires_at 字段 | 待修复 |

### 9.2 测试结果

| 测试类别 | 测试用例数 | 通过数 | 失败数 | 通过率 |
|---------|-----------|--------|--------|--------|
| 好友系统 | 28 | 28 | 0 | 100% |
| 邮箱验证 | 7 | 7 | 0 | 100% |
| **总计** | **35** | **35** | **0** | **100%** |

### 9.3 性能指标

| 指标 | 目标值 | 实际值 | 状态 |
|------|--------|--------|------|
| API 响应时间 P95 | < 500ms | ___ms | 待测试 |
| 数据库查询时间 P95 | < 100ms | ___ms | 待测试 |
| 单元测试覆盖率 | ≥ 80% | ___% | 待测试 |
| 集成测试覆盖率 | ≥ 90% | ___% | 待测试 |

---

## 10. 附录

### 10.1 相关文件清单

需要修改的文件：
- `src/storage/friend_room.rs`
- `src/web/routes/friend_room.rs`
- `src/federation/friend/friend_federation.rs`
- `migrations/xxx_add_expires_at_to_email_verification_tokens.sql`
- `src/error.rs` (新建)
- `tests/friend_system_tests.rs` (新建)
- `tests/email_verification_tests.rs` (新建)

### 10.2 参考资料

- Matrix 客户端-服务器 API 规范: https://spec.matrix.org/v1.2/client-server-api/
- Matrix 错误码规范: https://spec.matrix.org/v1.2/client-server-api/#standard-error-response
- PostgreSQL 外键约束文档: https://www.postgresql.org/docs/current/ddl-constraints.html
- Axum 错误处理文档: https://docs.rs/axum/latest/axum/error_handling/index.html

---

**文档结束**

