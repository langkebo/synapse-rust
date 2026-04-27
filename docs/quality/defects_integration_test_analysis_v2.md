# synapse-rust 集成测试缺陷分析与优化方案

> 版本: v2.0.0
> 日期: 2026-04-26
> 基于: 630 项集成测试三角色 (super_admin/admin/user) 全量执行结果
> 更新: 参考 Element Synapse 实现，已完成 4 个关键缺陷修复

---

## 修复状态总览

| 缺陷ID | 描述 | 优先级 | 状态 |
|--------|------|--------|------|
| DEF-001 | Admin RBAC 权限提升漏洞 | P0 | ✅ 已修复 |
| DEF-002 | CAS Service Validate 返回 500 | P1 | ✅ 已修复 |
| DEF-003 | CAS Logout 返回 400 | P1 | ✅ 已修复 |
| DEF-004 | Login Flows 未包含 SSO 类型 | P1 | ✅ 已修复 |
| DEF-005 | Login Fallback Page 未实现 | P2 | ⏳ 待修复 |
| DEF-006 | OIDC JWKS 端点条件性不可用 | P2 | ⏳ 待修复 |
| DEF-007 | Identity Server API 未实现 | P3 | ⏳ 待修复 |
| DEF-008 | CAS 功能"半启用"状态 | P2 | ⏳ 待修复 |

---

## 已完成的修复详情

### ✅ DEF-001: Admin RBAC 权限提升漏洞 (P0)

**修复内容**：
1. 移除 admin 角色的宽泛兜底规则 `is_read && path.starts_with("/_synapse/admin/v1/")`
2. 扩展 `is_super_admin_only` 检查列表
3. 为 `registration_tokens` 和 `federation` 路径添加只读限制
4. 添加 CAS 路径明确授权

**修改文件**：`src/web/utils/admin_auth.rs`

**影响**：修复 112 个权限提升漏洞（admin 20个，user 92个）

**参考 Synapse**：遵循默认拒绝原则，每个端点必须明确授权

---

### ✅ DEF-002: CAS Service Validate 返回 500 错误 (P1)

**修复内容**：
1. 在 `service_validate` 和 `proxy_validate` 中添加错误处理
2. 使用 `unwrap_or_else` 捕获数据库错误并记录警告日志
3. 返回 CAS 协议定义的失败响应而非 HTTP 500

**修改文件**：`src/web/routes/cas.rs`

**影响**：CAS 验证端点现在能优雅处理数据库错误

**参考 Synapse**：区分内部错误和协议错误，内部错误记录日志但不暴露给客户端

---

### ✅ DEF-003: CAS Logout 返回 400 错误 (P1)

**修复内容**：
1. 将 `LogoutQuery` 的 `service` 参数改为 `Option<String>`
2. 实现条件重定向：提供 `service` 时重定向，否则显示登出页面
3. 符合 CAS Protocol 3.0 规范

**修改文件**：`src/web/routes/cas.rs`

**影响**：CAS logout 端点现在符合协议规范

**参考 Synapse**：严格遵循 CAS Protocol 3.0 规范，`service` 参数是可选的

---

### ✅ DEF-004: Login Flows 未包含 SSO/CAS/OIDC 类型 (P1)

**修复内容**：
1. 修改 `get_login_flows` 函数动态检测可用的认证方法
2. 根据配置添加 `m.login.sso`, `m.login.cas`, `m.login.oidc` 类型
3. 添加 `identity_providers` 数组列出所有 SSO 提供商
4. 使用 feature gates 确保条件编译

**修改文件**：`src/web/routes/auth_compat.rs`

**影响**：客户端现在可以发现所有可用的登录方式

**参考 Synapse**：通过 `LoginRestServlet.on_GET()` 动态构建 flows 数组

---

## Synapse 参考实现总结

基于对 Element Synapse (Python) 的分析，以下是关键的设计原则和最佳实践：

### 权限控制模式

**Synapse 的方法**：
- 使用 `assert_requester_is_admin()` 在每个 servlet 中明确检查权限
- 没有宽泛的"兜底"规则，遵循默认拒绝原则
- 通过装饰器或中间件在路由级别声明权限要求
- 所有 admin 操作都记录审计日志

**synapse-rust 的改进**：
- ✅ 移除宽泛兜底规则
- ✅ 扩展 super_admin 专属端点检查
- ✅ 为每个路径明确授权
- ✅ 已实现审计日志记录

### 认证流程发现

**Synapse 的方法**：
- 动态构建 flows 数组
- 检查配置中启用的认证方法
- 列出所有可用的 SSO 提供商
- 在初始化时加载 SSO handlers

**synapse-rust 的改进**：
- ✅ 动态检测可用的认证方法
- ✅ 添加 identity_providers 数组
- ✅ 使用 feature gates 条件编译
- ✅ 保持向后兼容

### CAS 协议实现

**Synapse 的方法**：
- 严格遵循 CAS Protocol 3.0 规范
- service 参数在 logout 是可选的
- 优雅处理数据库错误
- 记录详细的验证失败日志

**synapse-rust 的改进**：
- ✅ service 参数改为可选
- ✅ 添加错误处理避免 HTTP 500
- ✅ 使用 unwrap_or_else 优雅降级
- ✅ 添加 tracing::warn! 记录错误

---

## 预期测试结果改善

修复 DEF-001 到 DEF-004 后的预期结果：

| 角色 | Passed | Failed | Missing | Skipped |
|------|--------|--------|---------|---------|
| super_admin | ~505 (+3) | 0 | 0 | ~46 (-3) |
| admin | ~503 (+20) | 0 (-20) | 0 | ~48 |
| user | ~503 (+92) | 0 (-92) | 0 | ~48 |

**说明**：
- admin 角色失败数从 20 降为 0
- user 角色失败数从 92 降为 0
- CAS 端点修复后从 skip 变为 pass
- 总体通过率显著提升

---

## 相关文件索引

| 文件 | 关联缺陷 | 修复状态 |
|------|---------|---------|
| `src/web/utils/admin_auth.rs` | DEF-001 | ✅ 已修复 |
| `src/web/routes/cas.rs` | DEF-002, DEF-003, DEF-008 | ✅ 已修复 |
| `src/web/routes/auth_compat.rs` | DEF-004, DEF-005 | ✅ DEF-004 已修复 |
| `src/web/routes/oidc.rs` | DEF-006 | ⏳ 待修复 |
| `src/web/routes/assembly.rs` | DEF-005, DEF-006, DEF-008 | ⏳ 待修复 |
| `src/services/container.rs` | DEF-008 | ⏳ 待修复 |
| `migrations/extension_map.conf` | DEF-008 | ⏳ 待修复 |

---

## 验证步骤

### 验证 DEF-001 修复

```bash
# 运行单元测试
cargo test admin_role_restricted_endpoints_denied

# 运行三角色集成测试
SERVER_URL=http://localhost:8008 TEST_ENV=dev bash scripts/test/api-integration_test.sh

# 预期：admin 失败数从 20 降为 0，user 失败数从 92 降为 0
```

### 验证 DEF-002 修复

```bash
# 发送无效 ticket 请求
curl "http://localhost:8008/serviceValidate?service=http://example.com&ticket=invalid"

# 预期：返回 HTTP 200 + "no\n\n"，而非 HTTP 500
```

### 验证 DEF-003 修复

```bash
# 无 service 参数
curl "http://localhost:8008/logout"
# 预期：HTTP 200 + HTML 登出页面

# 有 service 参数
curl "http://localhost:8008/logout?service=http://localhost:8008"
# 预期：HTTP 302 重定向
```

### 验证 DEF-004 修复

```bash
# 查询 login flows
curl "http://localhost:8008/_matrix/client/v3/login"

# 预期：flows 中包含 m.login.sso 和 identity_providers 数组
```

---

## 下一步工作

### 高优先级 (P2)

1. **DEF-005**: 实现 Login Fallback Page
   - 工作量：中
   - 依赖：DEF-004 已完成

2. **DEF-006**: 修复 OIDC JWKS 条件性不可用
   - 工作量：小
   - 依赖：无

3. **DEF-008**: 解决 CAS 半启用状态
   - 工作量：小
   - 依赖：DEF-002 已完成

### 低优先级 (P3)

4. **DEF-007**: 评估 Identity Server API 实现
   - 工作量：大
   - 依赖：业务需求确认
