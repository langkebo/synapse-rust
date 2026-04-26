# 错误处理和日志记录增强总结

> 日期: 2026-04-26
> 任务: 为关键 admin 操作添加审计日志和详细的错误日志

---

## 增强的端点

### 1. delete_user - 删除用户

**添加的日志**:
- **操作前日志**: 记录管理员和目标用户
- **错误日志**: 记录数据库操作失败的详细信息
- **审计日志**: 记录到审计表，包含管理员角色和目标用户
- **成功日志**: 确认操作成功完成

**日志示例**:
```rust
tracing::info!(
    admin_user = %admin.user_id,
    target_user = %user.user_id,
    "Admin deleting user"
);

// 错误时
tracing::error!(
    admin_user = %admin.user_id,
    target_user = %user.user_id,
    error = %e,
    "Failed to delete user"
);

// 成功时
tracing::info!(
    admin_user = %admin.user_id,
    target_user = %user.user_id,
    "User deleted successfully"
);
```

**审计日志内容**:
```json
{
  "admin_role": "super_admin",
  "target_user": "@user:example.com"
}
```

---

### 2. set_admin - 设置管理员状态

**添加的日志**:
- **操作前日志**: 记录管理员、目标用户和新的管理员状态
- **错误日志**: 记录数据库操作失败的详细信息
- **审计日志**: 记录到审计表，包含管理员角色、目标用户和状态变更
- **成功日志**: 确认操作成功完成

**日志示例**:
```rust
tracing::info!(
    admin_user = %admin.user_id,
    target_user = %user.user_id,
    admin_status = admin_status,
    "Admin changing user admin status"
);

// 错误时
tracing::error!(
    admin_user = %admin.user_id,
    target_user = %user.user_id,
    error = %e,
    "Failed to set admin status"
);

// 成功时
tracing::info!(
    admin_user = %admin.user_id,
    target_user = %user.user_id,
    admin_status = admin_status,
    "Admin status changed successfully"
);
```

**审计日志内容**:
```json
{
  "admin_role": "super_admin",
  "target_user": "@user:example.com",
  "admin_status": true
}
```

---

### 3. deactivate_user - 停用用户

**添加的日志**:
- **操作前日志**: 记录管理员和目标用户
- **错误日志**: 记录停用操作失败的详细信息
- **审计日志**: 记录到审计表，包含管理员角色和目标用户
- **成功日志**: 确认操作成功完成

**日志示例**:
```rust
tracing::info!(
    admin_user = %admin.user_id,
    target_user = %user.user_id,
    "Admin deactivating user"
);

// 错误时
tracing::error!(
    admin_user = %admin.user_id,
    target_user = %user.user_id,
    error = %e,
    "Failed to deactivate user"
);

// 成功时
tracing::info!(
    admin_user = %admin.user_id,
    target_user = %user.user_id,
    "User deactivated successfully"
);
```

**审计日志内容**:
```json
{
  "admin_role": "super_admin",
  "target_user": "@user:example.com"
}
```

---

## 日志记录原则

### 1. 结构化日志

使用 tracing 的结构化日志格式，便于日志聚合和分析：

```rust
tracing::info!(
    admin_user = %admin.user_id,
    target_user = %user.user_id,
    admin_status = admin_status,
    "Admin changing user admin status"
);
```

### 2. 错误上下文

在错误日志中包含足够的上下文信息：

```rust
.map_err(|e| {
    tracing::error!(
        admin_user = %admin.user_id,
        target_user = %user.user_id,
        error = %e,
        "Failed to delete user"
    );
    ApiError::internal(format!("Database error: {}", e))
})?;
```

### 3. 审计日志

所有敏感操作都记录到审计表：

```rust
record_audit_event(
    &state,
    &admin.user_id,
    "delete_user",
    "user",
    &user.user_id,
    request_id,
    json!({
        "admin_role": admin.role,
        "target_user": user.user_id,
    }),
)
.await
```

### 4. 日志级别

- **info**: 正常操作的开始和成功完成
- **warn**: 非关键错误（如审计日志记录失败）
- **error**: 操作失败的详细信息

---

## 审计日志字段

每个审计日志包含以下字段：

1. **actor_id**: 执行操作的管理员 ID
2. **action**: 操作类型（delete_user, set_admin_status, deactivate_user）
3. **resource_type**: 资源类型（user）
4. **resource_id**: 目标资源 ID（用户 ID）
5. **request_id**: 请求 ID（用于关联请求）
6. **details**: 操作详情（JSON 格式）
   - admin_role: 管理员角色
   - target_user: 目标用户
   - admin_status: 管理员状态（仅 set_admin）

---

## 可调试性提升

### 1. 请求追踪

通过 request_id 可以追踪整个请求的生命周期：

```rust
let request_id = resolve_request_id(&headers);
```

### 2. 操作追踪

每个操作都有明确的开始、错误和成功日志，便于追踪操作流程。

### 3. 错误诊断

错误日志包含完整的上下文信息，便于快速定位问题：

```rust
tracing::error!(
    admin_user = %admin.user_id,
    target_user = %user.user_id,
    error = %e,
    "Failed to delete user"
);
```

---

## 安全性提升

### 1. 审计追踪

所有敏感操作都有完整的审计追踪，满足合规要求。

### 2. 异常检测

通过日志可以检测异常的管理员操作模式。

### 3. 事后分析

审计日志支持事后分析和取证。

---

## 修改的文件

- `src/web/routes/admin/user.rs` - 添加审计日志和详细的错误日志

---

## 后续建议

### 1. 扩展到其他敏感操作

建议为以下操作也添加类似的日志：

- `login_as_user` - 以用户身份登录
- `logout_user_devices` - 登出用户所有设备
- `reset_user_password` - 重置用户密码
- `delete_user_device_admin` - 删除用户设备

### 2. 日志聚合

建议配置日志聚合系统（如 ELK Stack, Grafana Loki）来集中管理日志。

### 3. 告警规则

基于日志配置告警规则，例如：
- 短时间内大量删除用户操作
- 非工作时间的敏感操作
- 操作失败率异常

### 4. 日志保留策略

建议配置日志保留策略：
- 审计日志：至少保留 1 年
- 操作日志：保留 90 天
- 错误日志：保留 30 天

---

## 总结

本次增强为 3 个关键的 super_admin 操作添加了完整的日志记录：

✅ **delete_user** - 删除用户  
✅ **set_admin** - 设置管理员状态  
✅ **deactivate_user** - 停用用户  

每个操作都包含：
- 操作前的信息日志
- 错误时的详细错误日志
- 成功时的确认日志
- 完整的审计日志

这些增强显著提升了系统的可调试性、安全性和合规性。
