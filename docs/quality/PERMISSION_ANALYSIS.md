# 权限问题详细分析

生成时间: 2026-04-26 16:45
测试角色: admin
测试结果来源: /tmp/admin_test_result.log

---

## 测试结果概览

| 类型 | 数量 | 说明 |
|------|------|------|
| 失败用例 | 27 | admin 角色无法访问的端点 |
| 跳过用例 | 56 | 因各种原因跳过的测试 |
| 通过用例 | 468 | admin 角色正常工作的端点 |

---

## 一、安全漏洞（需立即修复）

### 🔴 1. Rust Synapse Version - 权限绕过漏洞
**端点**: `/_synapse/admin/info`  
**问题**: admin 角色可以访问，但应该只允许 super_admin  
**状态**: ✅ 已修复（待验证）  
**修复文件**: 
- `src/web/routes/admin/server.rs` - 添加 AdminUser 参数和角色检查
- `src/web/routes/admin/mod.rs` - 将路由移到 protected 组

---

## 二、失败用例分析（27个）

### 分类1: RBAC 权限配置过严（需要调整配置）

这些端点应该允许 admin 角色访问，但当前 RBAC 配置拒绝了：

#### 用户管理类（应该允许 admin）
1. **Admin User Sessions** - 查看用户会话
2. **Admin User Stats** - 查看用户统计
3. **Admin Account Details** - 查看账户详情
4. **List User Sessions** - 列出用户会话

#### 房间管理类（应该允许 admin）
5. **Admin Room Block** - 封禁房间
6. **Admin Room Unblock** - 解封房间
7. **Admin Room Stats** - 房间统计
8. **Admin Room Search** - 搜索房间
9. **Admin Block Room** - 封禁房间（重复）

#### 注册令牌类（应该允许 admin）
10. **List Registration Tokens** - 列出注册令牌
11. **Get Active Registration Tokens** - 获取活跃令牌

#### 系统统计类（应该允许 admin）
12. **Admin Stats Users** - 用户统计
13. **Admin Stats Rooms** - 房间统计
14. **Get Statistics** - 获取统计信息
15. **Get Version Info** - 版本信息

#### 后台任务类（应该允许 admin）
16. **List Background Updates** - 列出后台更新

#### 事件报告类（应该允许 admin）
17. **List Event Reports** - 列出事件报告

#### 空间管理类（应该允许 admin）
18. **Admin List Spaces** - 列出空间
19. **Admin Space Rooms** - 空间房间
20. **Admin Space Stats** - 空间统计
21. **Admin Space Users** - 空间用户

#### 功能标志类（应该允许 admin）
22. **Get Feature Flags** - 获取功能标志

#### 应用服务类（应该允许 admin）
23. **List App Services** - 列出应用服务

### 分类2: RBAC 正确拒绝（super_admin 专属）

这些端点正确地拒绝了 admin 角色：

24. **Admin Batch Users** - 批量用户操作（super_admin 专属）
25. **Admin Federation Resolve Remote** - 联邦解析（super_admin 专属）

### 分类3: 重复项
26. **Admin User Stats** - 与第2项重复

---

## 三、跳过用例分析（56个）

### 分类1: 合理跳过 - 破坏性测试（9个）
这些测试在开发环境中正确跳过：
- Delete Device
- Delete Devices (r0)
- Admin User Password
- Invalidate User Session
- Reset User Password
- Admin Deactivate
- Admin Room Delete
- Admin Delete User
- Admin Session Invalidate

### 分类2: 合理跳过 - 功能未配置（26个）

#### SSO/OIDC/SAML 未配置（15个）
- OIDC Authorize Endpoint
- OIDC Dynamic Client Registration
- SAML SP Metadata
- SAML IdP Metadata
- SAML Login Redirect
- SAML Callback GET
- SAML Callback POST
- SAML Admin Metadata Refresh
- Login Flows - m.login.oidc
- SSO Redirect v3
- SSO Redirect r0
- SSO Redirect (no redirectUrl)
- OIDC Callback (invalid state)
- OIDC Userinfo (with auth)
- SSO Userinfo (with auth)

#### Identity Server 未配置（6个）
- Identity v2 Lookup
- Identity v2 Hash Lookup
- Identity v1 Lookup
- Identity v1 Request Token
- Identity v2 Request Token
- Identity Lookup (algorithm validation)

#### 其他未配置功能（5个）
- Builtin OIDC Login
- CAS Admin Register Service
- Identity v2 Account Info
- Identity v2 Terms
- Identity v2 Hash Details

### 分类3: 合理跳过 - 测试前置条件不满足（11个）

#### 联邦相关（5个）
- Outbound Federation Version (matrix.org) - 需要公网域名
- Federation Members - 需要联邦签名请求
- Federation Hierarchy - 需要联邦签名请求
- Federation Room Auth - 需要已加入房间
- Admin Federation Destination Details - 需要联邦目标数据（重复）

#### 功能未实现（5个）
- Admin Version - not found
- Evict User - not found
- Get Registration Token - not found
- Get User Count - not found
- Get Room Count - not found

#### 数据依赖（1个）
- Admin Reset Federation Connection - 需要联邦目标数据

### 分类4: 🟡 不合理跳过 - 权限问题导致（9个）

这些测试因为 HTTP 403 被跳过，但它们应该是 admin 可以访问的：

1. **Admin Delete Room** - HTTP 403
2. **Admin Room Member Add** - HTTP 403
3. **Admin Room Ban User** - HTTP 403
4. **Admin Room Kick User** - HTTP 403
5. **Admin List Spaces** - HTTP 403（与失败用例重复）
6. **Admin Set Room Public** - HTTP 403
7. **Admin Delete User Device** - HTTP 403
8. **Admin Shadow Ban User** - HTTP 403
9. **Admin List Audit Events** - HTTP 403

---

## 四、修复优先级

### 🔴 P0 - 安全漏洞（立即修复）
1. ✅ Rust Synapse Version 权限绕过 - 已修复

### 🟡 P1 - 权限配置问题（高优先级）

需要调整 RBAC 配置，允许 admin 角色访问以下端点：

#### 用户管理（4个）
- Admin User Sessions
- Admin User Stats
- Admin Account Details
- List User Sessions

#### 房间管理（9个）
- Admin Room Block/Unblock
- Admin Room Stats
- Admin Room Search
- Admin Delete Room
- Admin Room Member Add
- Admin Room Ban User
- Admin Room Kick User
- Admin Set Room Public

#### 用户操作（2个）
- Admin Delete User Device
- Admin Shadow Ban User

#### 系统管理（10个）
- List Registration Tokens
- Get Active Registration Tokens
- Admin Stats Users
- Admin Stats Rooms
- Get Statistics
- Get Version Info
- List Background Updates
- List Event Reports
- Get Feature Flags
- List App Services

#### 空间管理（4个）
- Admin List Spaces
- Admin Space Rooms
- Admin Space Stats
- Admin Space Users

#### 审计（1个）
- Admin List Audit Events

**总计**: 30个端点需要调整权限配置

### 🟢 P2 - 功能实现（低优先级）

这些端点返回 "not found"，可能需要实现：
- Admin Version
- Evict User
- Get Registration Token
- Get User Count
- Get Room Count

---

## 五、修复计划

### 阶段1: 修复安全漏洞 ✅
- [x] 修复 `/_synapse/admin/info` 权限绕过

### 阶段2: 调整 RBAC 配置
需要修改的文件：
- `src/web/utils/admin_auth.rs` - 更新 admin 角色的权限规则
- 或者在各个端点的 handler 中调整权限检查逻辑

### 阶段3: 验证
- 运行 super_admin 测试 - 应该全部通过
- 运行 admin 测试 - 应该通过所有合理的端点
- 运行 user 测试 - 应该正确拒绝所有 admin 端点

---

## 六、RBAC 配置建议

### admin 角色应该能够：
✅ 查看用户信息和统计
✅ 管理房间（封禁、解封、搜索、统计）
✅ 管理房间成员（添加、踢出、封禁）
✅ 查看和管理注册令牌
✅ 查看系统统计信息
✅ 查看后台任务和事件报告
✅ 管理空间
✅ 查看审计日志
✅ 查看功能标志和应用服务

### admin 角色不应该能够：
❌ 批量用户操作（super_admin 专属）
❌ 联邦解析操作（super_admin 专属）
❌ 修改用户为 admin（super_admin 专属）
❌ 停用用户（super_admin 专属）
❌ 删除房间（super_admin 专属）
❌ 清除历史记录（super_admin 专属）
❌ 联邦黑名单操作（super_admin 专属）
❌ 服务器重启（super_admin 专属）
❌ 查看服务器版本信息（super_admin 专属）

---

**分析完成时间**: 2026-04-26 16:45  
**下一步**: 根据此分析更新 RBAC 配置
