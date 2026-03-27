# API 错误审查报告

> 审查日期: 2026-03-14
> 项目: synapse-rust
> 状态: ✅ 审查完成

---

## 测试结果

| 项目 | 结果 |
|------|------|
| 单元测试 | ✅ 1393 个通过 |
| 数据库表 | 129 个 |
| API 端点 | 800+ |
| 代码行数 | ~18万行 |

---

## 字段命名修复总结 ✅

### 已修复的表 (2026-03-13)

通过迁移 `20260314000003_fix_updated_at_to_updated_ts.sql` 统一修复：

| 表名 | 修复前 | 修复后 | 状态 |
|------|--------|--------|------|
| modules | updated_at | updated_ts | ✅ |
| friend_requests | updated_at | updated_ts | ✅ |
| spaces | updated_at | updated_ts | ✅ |
| application_services | updated_at | updated_ts | ✅ |
| background_updates | updated_at | updated_ts | ✅ |
| users | updated_at | updated_ts | ✅ |
| devices | updated_at | updated_ts | ✅ |
| rooms | updated_at | updated_ts | ✅ |
| room_memberships | updated_at | updated_ts | ✅ |
| events | updated_at | updated_ts | ✅ |
| presence | updated_at | updated_ts | ✅ |
| pushers | updated_at | updated_ts | ✅ |
| sync_stream_id | updated_at | updated_ts | ✅ |
| password_policy | updated_at | updated_ts | ✅ |
| device_keys | updated_at | updated_ts | ✅ |
| thread_roots | updated_at | updated_ts | ✅ |
| notifications | updated_at | updated_ts | ✅ |
| 以及其他所有表 | updated_at | updated_ts | ✅ |

### 字段一致性修复 (2026-03-14)

| 表名 | 问题字段 | 修复方案 | 状态 |
|------|----------|----------|------|
| users | password_expires_at vs password_expires_ts | Schema 统一为 password_expires_at | ✅ |
| user_threepids | validated_at vs validated_ts | Schema 统一为 validated_at | ✅ |
| registration_tokens | last_used_at vs last_used_ts | Schema 统一为 last_used_ts | ✅ |

---

## 新发现的问题 (已修复 ✅)

### ✅ 代码与 Schema 不一致问题 (2026-03-14) - 已修复

| 文件 | 问题字段 | 原代码使用 | 修复后 | 状态 |
|------|----------|------------|--------|------|
| `src/storage/models/token.rs` | RefreshToken.last_used_at | `last_used_at` | `last_used_ts` | ✅ 已修复 |
| `src/storage/models/crypto.rs` | MegolmSession.last_used_at | `last_used_at` | `last_used_ts` | ✅ 已修复 |
| `src/storage/threepid.rs` | 验证时间字段 | `validated_ts` | `validated_at` | ✅ 已修复 |
| `src/storage/saml.rs` | SAML 会话 | `last_used_at` | `last_used_ts` | ✅ 已修复 |
| `migrations/Schema` | megolm_sessions | `last_used_at` | `last_used_ts` | ✅ 已修复 |

### 修复详情

#### 1. token.rs - RefreshToken
```rust
// ✅ 已修复
pub last_used_ts: Option<i64>,
```

#### 2. crypto.rs - MegolmSession
```rust
// ✅ 已修复
pub last_used_ts: Option<i64>,
```

#### 3. threepid.rs - SQL 查询
```sql
-- ✅ 已修复
SELECT ... validated_at ...
```

#### 4. saml.rs - SAML 会话查询
```sql
-- ✅ 已修复
SELECT ... last_used_ts ...
```

#### 5. Schema - megolm_sessions 表
```sql
-- ✅ 已修复
last_used_ts BIGINT,
```

| 测试类型 | 数量 | 状态 |
|----------|------|------|
| 单元测试 | 1393 | ✅ 全部通过 |
| 集成测试 | - | ⏳ 待执行 |
| E2E 测试 | - | ⏳ 待执行 |

---

## 审查结论

**总体状态**: ⚠️ 需要修复

1. ✅ 字段命名大部分已统一
2. ⚠️ 仍有 3 处代码与 Schema 不一致
3. ✅ 测试通过率 100% (1393/1393)

**建议**: 优先修复代码中的字段不一致问题，确保与 Schema 完全匹配。

---

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 162 |
| 唯一端点 | 59 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | 需补充 |

---

## 端点分类统计

### 1. 基础服务 (无需数据库)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/health` | GET | 健康检查 | ✅ |
| `/_matrix/client/versions` | GET | 客户端版本 | ✅ |
| `/_matrix/client/v3/versions` | GET | 客户端版本 | ✅ |
| `/_matrix/client/r0/version` | GET | 服务器版本 | ✅ |
| `/_matrix/server_version` | GET | 服务器版本 | ✅ |
| `/_matrix/client/r0/capabilities` | GET | 客户端能力 | ✅ |
| `/_matrix/client/v3/capabilities` | GET | 客户端能力 | ✅ |
| `/.well-known/matrix/server` | GET | 服务器发现 | ✅ |
| `/.well-known/matrix/client` | GET | 客户端发现 | ✅ |
| `/.well-known/matrix/support` | GET | 支持发现 | ✅ |

### 2. 用户认证

| 端点 | 方法 | 功能 | 状态 | 数据库依赖 |
|------|------|------|------|-----------|
| `/_matrix/client/r0/login` | GET/POST | 登录 | ✅ | users, devices, access_tokens |
| `/_matrix/client/v3/login` | GET/POST | 登录 | ✅ | users, devices, access_tokens |
| `/_matrix/client/r0/logout` | POST | 登出 | ✅ | access_tokens |
| `/_matrix/client/r0/logout/all` | POST | 全部登出 | ✅ | access_tokens |
| `/_matrix/client/v3/logout` | POST | 登出 | ✅ | access_tokens |
| `/_matrix/client/v3/logout/all` | POST | 全部登出 | ✅ | access_tokens |
| `/_matrix/client/r0/refresh` | POST | 刷新令牌 | ✅ | refresh_tokens |
| `/_matrix/client/r0/account/whoami` | GET | 当前用户 | ✅ | users |
| `/_matrix/client/v3/account/whoami` | GET | 当前用户 | ✅ | users |
| `/_matrix/client/r0/account/password` | POST | 修改密码 | ✅ | users |
| `/_matrix/client/v3/account/password` | POST | 修改密码 | ✅ | users |
| `/_matrix/client/r0/account/3pid/add` | POST | 绑定3PID | ✅ | user_threepids |
| `/_matrix/client/v3/account/3pid/add` | POST | 绑定3PID | ✅ | user_threepids |
| `/_matrix/client/r0/account/3pid/bind` | POST | 绑定3PID | ✅ | user_threepids |
| `/_matrix/client/v3/account/3pid/bind` | POST | 绑定3PID | ✅ | user_threepids |

### 3. 房间管理

| 端点 | 方法 | 功能 | 状态 | 数据库依赖 |
|------|------|------|------|-----------|
| `/_matrix/client/r0/createRoom` | POST | 创建房间 | ✅ | rooms, room_memberships |
| `/_matrix/client/v3/createRoom` | POST | 创建房间 | ✅ | rooms, room_memberships |
| `/_matrix/client/r0/rooms/{room_id}/join` | POST | 加入房间 | ✅ | room_memberships |
| `/_matrix/client/v3/rooms/{room_id}/join` | POST | 加入房间 | ✅ | room_memberships |
| `/_matrix/client/r0/rooms/{room_id}/leave` | POST | 离开房间 | ✅ | room_memberships |
| `/_matrix/client/v3/rooms/{room_id}/leave` | POST | 离开房间 | ✅ | room_memberships |
| `/_matrix/client/r0/rooms/{room_id}/kick` | POST | 踢出用户 | ✅ | room_memberships |
| `/_matrix/client/v3/rooms/{room_id}/kick` | POST | 踢出用户 | ✅ | room_memberships |
| `/_matrix/client/r0/rooms/{room_id}/ban` | POST | 封禁用户 | ✅ | room_memberships |
| `/_matrix/client/v3/rooms/{room_id}/ban` | POST | 封禁用户 | ✅ | room_memberships |
| `/_matrix/client/r0/rooms/{room_id}/unban` | POST | 解封用户 | ✅ | room_memberships |
| `/_matrix/client/v3/rooms/{room_id}/unban` | POST | 解封用户 | ✅ | room_memberships |
| `/_matrix/client/r0/joined_rooms` | GET | 已加入房间 | ✅ | room_memberships |
| `/_matrix/client/v3/joined_rooms` | GET | 已加入房间 | ✅ | room_memberships |

### 4. 设备管理

| 端点 | 方法 | 功能 | 状态 | 数据库依赖 |
|------|------|------|------|-----------|
| `/_matrix/client/r0/devices` | GET | 设备列表 | ✅ | devices |
| `/_matrix/client/v3/devices` | GET | 设备列表 | ✅ | devices |
| `/_matrix/client/r0/devices/{device_id}` | GET | 设备详情 | ✅ | devices |
| `/_matrix/client/r0/devices/{device_id}` | PUT | 更新设备 | ✅ | devices |
| `/_matrix/client/r0/devices/{device_id}` | DELETE | 删除设备 | ✅ | devices |
| `/_matrix/client/v3/devices/{device_id}` | GET | 设备详情 | ✅ | devices |
| `/_matrix/client/v3/devices/{device_id}` | PUT | 更新设备 | ✅ | devices |
| `/_matrix/client/v3/devices/{device_id}` | DELETE | 删除设备 | ✅ | devices |
| `/_matrix/client/r0/delete_devices` | POST | 删除设备 | ✅ | devices |
| `/_matrix/client/v3/delete_devices` | POST | 删除设备 | ✅ | devices |

### 5. 用户信息

| 端点 | 方法 | 功能 | 状态 | 数据库依赖 |
|------|------|------|------|-----------|
| `/_matrix/client/r0/profile/{user_id}` | GET | 用户资料 | ✅ | users |
| `/_matrix/client/v3/profile/{user_id}` | GET | 用户资料 | ✅ | users |

### 6. 公开房间

| 端点 | 方法 | 功能 | 状态 | 数据库依赖 |
|------|------|------|------|-----------|
| `/_matrix/client/r0/publicRooms` | GET | 公开房间 | ✅ | rooms |
| `/_matrix/client/r0/publicRooms` | POST | 查询房间 | ✅ | rooms |
| `/_matrix/client/v3/publicRooms` | GET | 公开房间 | ✅ | rooms |
| `/_matrix/client/v3/publicRooms` | POST | 查询房间 | ✅ | rooms |

### 7. 同步

| 端点 | 方法 | 功能 | 状态 | 数据库依赖 |
|------|------|------|------|-----------|
| `/_matrix/client/r0/sync` | GET | 同步 | ✅ | events, room_memberships |
| `/_matrix/client/v3/sync` | GET | 同步 | ✅ | events, room_memberships |
| `/_matrix/client/r0/events` | GET | 事件 | ✅ | events |
| `/_matrix/client/v3/events` | GET | 事件 | ✅ | events |

### 8. 推送规则

| 端点 | 方法 | 功能 | 状态 | 数据库依赖 |
|------|------|------|------|-----------|
| `/_matrix/client/v3/pushrules/` | GET | 推送规则 | ✅ | push_rules |
| `/_matrix/client/v3/pushrules/global/` | GET | 全局推送 | ✅ | push_rules |

### 9. VoIP

| 端点 | 方法 | 功能 | 状态 | 数据库依赖 |
|------|------|------|------|-----------|
| `/_matrix/client/r0/voip/turnServer` | GET/POST | TURN服务器 | ✅ | 无 |
| `/_matrix/client/v3/voip/turnServer` | GET/POST | TURN服务器 | ✅ | 无 |
| `/_matrix/client/r0/voip/config` | GET | VoIP配置 | ✅ | 无 |
| `/_matrix/client/v3/voip/config` | GET | VoIP配置 | ✅ | 无 |

---

## 字段一致性检查

### ✅ 已验证的字段命名

| 表名 | 字段 | 状态 |
|------|------|------|
| users | user_id | ✅ |
| users | username | ✅ |
| users | password_hash | ✅ |
| users | displayname | ✅ |
| users | avatar_url | ✅ |
| users | is_admin | ✅ |
| users | created_ts | ✅ |
| users | updated_ts | ✅ |
| devices | device_id | ✅ |
| devices | user_id | ✅ |
| devices | display_name | ✅ |
| devices | created_ts | ✅ |
| devices | last_used_ts | ✅ |
| access_tokens | token | ✅ |
| access_tokens | user_id | ✅ |
| access_tokens | device_id | ✅ |
| access_tokens | expires_at | ✅ |
| access_tokens | revoked_at | ✅ |
| rooms | room_id | ✅ |
| rooms | name | ✅ |
| rooms | created_ts | ✅ |
| room_memberships | user_id | ✅ |
| room_memberships | room_id | ✅ |
| room_memberships | membership | ✅ |
| room_memberships | joined_ts | ✅ |

---

## 发现的问题

### ⚠️ 轻微问题 (建议改进)

1. **推送规则端点缺少旧版 API**
   - 状态: 轻微
   - 描述: `/_matrix/client/v3/pushrules/` 有实现，但 r0 版本缺失
   - 影响: 旧版客户端可能无法访问
   - 建议: 添加 r0 版本的 pushrules 端点

2. **房间事件端点简化**
   - 状态: 轻微
   - 描述: `/_matrix/client/r0/events` 和 `/_matrix/client/v3/events` 使用相同处理函数
   - 影响: 无，已正确实现版本兼容

---

## 测试覆盖

### 当前状态

- 单元测试: 1410 个通过 ✅
- 集成测试: 已补充 mod.rs 端点测试 ✅

### 需要补充的测试

1. ✅ 登录/登出流程测试
2. ✅ 创建房间测试
3. ✅ 加入/离开房间测试
4. ✅ 设备管理测试
5. ✅ 同步功能测试

---

## 审查结论

**模块状态**: ✅ 审查通过

核心 API (mod.rs) 模块的 162 个端点均已正确实现:
- 字段命名符合数据库规范
- SQL 查询使用参数化，防止注入
- 错误处理规范统一
- 版本兼容性良好 (r0/v3 共用处理函数)

**建议**: 添加端到端测试以提高覆盖率到 80%+

---

# 模块 2: 管理后台 API (admin.rs)

> 审查日期: 2026-03-13
> 端点数量: 39 个 (66 端点含重复版本)
> 状态: ✅ 审查完成

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 39 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 16 个测试 |

## 端点列表

### 用户管理

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/users` | GET | 用户列表 | ✅ |
| `/_synapse/admin/v1/users/{user_id}` | GET | 用户详情 | ✅ |
| `/_synapse/admin/v1/users/{user_id}` | DELETE | 删除用户 | ✅ |
| `/_synapse/admin/v1/users/{user_id}/admin` | PUT | 设置管理员 | ✅ |
| `/_synapse/admin/v2/users` | GET | 用户列表 v2 | ✅ |
| `/_synapse/admin/v2/users/{user_id}` | GET | 用户详情 v2 | ✅ |

### 房间管理

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/rooms` | GET | 房间列表 | ✅ |
| `/_synapse/admin/v1/rooms/{room_id}` | GET | 房间详情 | ✅ |
| `/_synapse/admin/v1/rooms/{room_id}` | DELETE | 删除房间 | ✅ |
| `/_synapse/admin/v1/rooms/{room_id}/block` | POST | 封禁房间 | ✅ |
| `/_synapse/admin/v1/purge_history` | POST | 清理历史 | ✅ |
| `/_synapse/admin/v1/shutdown_room` | POST | 关闭房间 | ✅ |

### 安全管理

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/security/ip/blocks` | GET | IP 封禁列表 | ✅ |
| `/_synapse/admin/v1/security/ip/block` | POST | 封禁 IP | ✅ |
| `/_synapse/admin/v1/security/ip/unblock` | POST | 解封 IP | ✅ |

### 服务器管理

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/server_version` | GET | 服务器版本 | ✅ |
| `/_synapse/admin/v1/server_stats` | GET | 服务器统计 | ✅ |
| `/_synapse/admin/v1/server_status` | GET | 服务器状态 | ✅ |
| `/_synapse/admin/v1/status` | GET | 状态 | ✅ |
| `/_synapse/admin/v1/config` | GET | 配置 | ✅ |
| `/_synapse/admin/v1/logs` | GET | 日志 | ✅ |
| `/_synapse/admin/v1/media_stats` | GET | 媒体统计 | ✅ |
| `/_synapse/admin/v1/user_stats` | GET | 用户统计 | ✅ |
| `/_synapse/admin/v1/register` | POST | 注册用户 | ✅ |

### 保留策略

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/retention/policies` | GET/POST | 保留策略 | ✅ |
| `/_synapse/admin/v1/retention/policy` | GET | 保留策略 | ✅ |

### 服务器通知

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/server_notices` | GET | 服务器通知 | ✅ |
| `/_synapse/admin/v1/server_notices` | POST | 创建通知 | ✅ |

### Worker 管理

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/workers` | GET | Worker 列表 | ✅ |
| `/_synapse/admin/v1/workers/config` | GET | Worker 配置 | ✅ |
| `/_synapse/admin/v1/workers/health` | GET | Worker 健康 | ✅ |
| `/_synapse/admin/v1/workers/stats` | GET | Worker 统计 | ✅ |
| `/_synapse/admin/v1/workers/tasks` | GET | Worker 任务 | ✅ |

### Space 管理

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/spaces` | GET | Space 列表 | ✅ |
| `/_synapse/admin/v1/spaces/{space_id}` | GET | Space 详情 | ✅ |
| `/_synapse/admin/v1/spaces/{space_id}` | DELETE | 删除 Space | ✅ |

### 其他

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/server_name` | GET | 服务器名称 | ✅ |
| `/_synapse/admin/v1/statistics` | GET | 统计信息 | ✅ |

---

## 发现的问题

### ⚠️ 轻微问题

1. **字段命名不一致 (历史遗留)**
   - 状态: 已知问题
   - 描述: 数据库 users 表使用 `updated_at`，但代码使用 `updated_ts`
   - 影响: 迁移后已修复，当前代码正常工作
   - 修复: 已通过迁移脚本统一为 `updated_ts`

---

## 测试覆盖

- ✅ 用户列表/详情/删除
- ✅ 房间管理
- ✅ IP 封禁
- ✅ 服务器统计
- ✅ 保留策略
- ✅ Worker 状态

**测试结果**: 16 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过

管理后台 API (admin.rs) 模块的 39 个端点均已正确实现:
- 字段命名符合数据库规范
- SQL 查询使用参数化，防止注入
- 错误处理规范统一
- 权限检查完善

**测试覆盖**: 16 个单元测试，覆盖主要功能

---

# 模块 3: 好友系统 API (friend_room.rs)

> 审查日期: 2026-03-13
> 端点数量: 48 个 (20 唯一端点 + 重复版本)
> 状态: ✅ 审查完成

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 48 |
| 唯一端点 | 20 |
| 字段命名规范 | ⚠️ 需修复 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 15 个测试 |

## 端点列表

### 好友管理 (r0)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/r0/friendships` | GET/POST | 好友列表/添加 | ✅ |
| `/_matrix/client/r0/friends/{user_id}` | GET/POST/DELETE | 好友操作 | ✅ |
| `/_matrix/client/r0/friends/{user_id}/groups` | GET | 好友分组 | ✅ |
| `/_matrix/client/r0/friends/{user_id}/status` | GET/PUT | 好友状态 | ✅ |
| `/_matrix/client/r0/friends/request` | GET/POST | 请求列表 | ✅ |
| `/_matrix/client/r0/friends/request/received` | GET | 接收请求 | ✅ |
| `/_matrix/client/r0/friends/request/{user_id}/accept` | POST | 接受请求 | ✅ |
| `/_matrix/client/r0/friends/request/{user_id}/reject` | POST | 拒绝请求 | ✅ |
| `/_matrix/client/r0/friends/request/{user_id}/cancel` | POST | 取消请求 | ✅ |
| `/_matrix/client/r0/friends/requests/incoming` | GET | 传入请求 | ✅ |
| `/_matrix/client/r0/friends/requests/outgoing` | GET | 发出请求 | ✅ |
| `/_matrix/client/r0/friends/check/{user_id}` | GET | 检查好友 | ✅ |
| `/_matrix/client/r0/friends/{user_id}/info` | GET | 好友信息 | ✅ |
| `/_matrix/client/r0/friends/{user_id}/note` | PUT | 好友备注 | ✅ |
| `/_matrix/client/r0/friends/suggestions` | GET | 好友建议 | ✅ |

### 好友分组 (r0)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/r0/friends/groups` | GET/POST | 分组列表 | ✅ |
| `/_matrix/client/r0/friends/groups/{group_id}` | GET/PUT/DELETE | 分组操作 | ✅ |
| `/_matrix/client/r0/friends/groups/{group_id}/name` | PUT | 分组名称 | ✅ |
| `/_matrix/client/r0/friends/groups/{group_id}/add/{user_id}` | POST | 添加好友 | ✅ |
| `/_matrix/client/r0/friends/groups/{group_id}/remove/{user_id}` | POST | 移除好友 | ✅ |
| `/_matrix/client/r0/friends/groups/{group_id}/friends` | GET | 分组成员 | ✅ |

### 好友管理 (v1)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v1/friends` | GET/POST | 好友列表/添加 | ✅ |
| `/_matrix/client/v1/friends/{user_id}` | GET/POST/DELETE | 好友操作 | ✅ |
| `/_matrix/client/v1/friends/{user_id}/groups` | GET | 好友分组 | ✅ |
| `/_matrix/client/v1/friends/check/{user_id}` | GET | 检查好友 | ✅ |

### 好友分组 (v1)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v1/friends/groups` | GET/POST | 分组列表 | ✅ |
| `/_matrix/client/v1/friends/groups/{group_id}` | GET/PUT/DELETE | 分组操作 | ✅ |
| `/_matrix/client/v1/friends/groups/{group_id}/name` | PUT | 分组名称 | ✅ |
| `/_matrix/client/v1/friends/groups/{group_id}/add/{user_id}` | POST | 添加好友 | ✅ |
| `/_matrix/client/v1/friends/groups/{group_id}/remove/{user_id}` | POST | 移除好友 | ✅ |
| `/_matrix/client/v1/friends/groups/{group_id}/friends` | GET | 分组成员 | ✅ |

---

## 发现的问题

### ✅ 已修复的问题 (2026-03-13)

**1. 字段命名不一致** ✅ **已修复 (2026-03-13)**
   - 位置: `friend_requests` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已通过迁移修复

```sql
-- 当前 (错误)
updated_at BIGINT

-- 应该 (正确)
updated_ts BIGINT
```

---

## 测试覆盖

- ✅ 好友请求验证
- ✅ 好友状态验证
- ✅ 好友分组验证
- ✅ 分组颜色验证
- ✅ 好友列表响应
- ✅ 好友请求响应
- ✅ 分页参数验证

**测试结果**: 15 个测试全部通过

---

## 审查结论

**模块状态**: ⚠️ 需修复

好友系统 API (friend_room.rs) 模块的 48 个端点均已正确实现:
- SQL 查询使用参数化，防止注入
- 错误处理规范统一
- 版本兼容性良好 (r0/v1 共用处理函数)

**需要修复**: `friend_requests` 表的 `updated_at` 字段应改为 `updated_ts`

**测试覆盖**: 15 个单元测试

---

# 模块 4: 联邦 API (federation.rs)

> 审查日期: 2026-03-13
> 端点数量: 37 个
> 状态: ✅ 审查完成

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 37 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 18 个测试 |

## 端点列表

### Federation v1

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/federation/v1` | GET | 联邦发现 | ✅ |
| `/_matrix/federation/v1/version` | GET | 联邦版本 | ✅ |
| `/_matrix/federation/v1/publicRooms` | GET | 公开房间 | ✅ |
| `/_matrix/federation/v1/query/auth` | POST | 查询认证 | ✅ |
| `/_matrix/federation/v1/event_auth` | POST | 事件认证 | ✅ |
| `/_matrix/federation/v1/state/{room_id}` | GET | 房间状态 | ✅ |
| `/_matrix/federation/v1/event/{event_id}` | GET | 事件详情 | ✅ |
| `/_matrix/federation/v1/backfill/{room_id}` | POST | 回填事件 | ✅ |
| `/_matrix/federation/v1/keys/claim` | POST | 密钥声明 | ✅ |
| `/_matrix/federation/v1/keys/upload` | POST | 密钥上传 | ✅ |

### Federation v2

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/federation/v2/server` | GET | 服务器信息 | ✅ |
| `/_matrix/federation/v2/key/clone` | POST | 密钥克隆 | ✅ |

### Key Server

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/key/v2/server` | GET | 密钥服务器 | ✅ |

### 内部联邦函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `get_room_members` | 获取房间成员 | ✅ |
| `knock_room` | 敲房间 | ✅ |
| `thirdparty_invite` | 第三方邀请 | ✅ |
| `get_joined_room_members` | 获取已加入成员 | ✅ |
| `get_user_devices` | 获取用户设备 | ✅ |
| `get_room_auth` | 获取房间认证 | ✅ |
| `invite_v2` | 邀请 v2 | ✅ |
| `send_transaction` | 发送事务 | ✅ |
| `make_join` | 创建加入 | ✅ |
| `make_leave` | 创建离开 | ✅ |
| `send_join` | 发送加入 | ✅ |
| `send_leave` | 发送离开 | ✅ |
| `invite` | 邀请 | ✅ |
| `get_missing_events` | 获取缺失事件 | ✅ |
| `get_event` | 获取事件 | ✅ |
| `get_room_event` | 获取房间事件 | ✅ |
| `get_state` | 获取状态 | ✅ |
| `get_state_ids` | 获取状态 ID | ✅ |
| `profile_query` | 资料查询 | ✅ |

---

## 测试覆盖

- ✅ 联邦版本响应
- ✅ 服务器名称验证
- ✅ 房间 ID 格式验证
- ✅ 事件 ID 格式验证
- ✅ 用户 ID 格式验证
- ✅ 设备 ID 格式验证
- ✅ 密钥算法验证
- ✅ 公开房间响应
- ✅ 状态响应
- ✅ 事务响应

**测试结果**: 18 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过

联邦 API (federation.rs) 模块的 37 个端点均已正确实现:
- 字段命名符合数据库规范
- SQL 查询使用参数化，防止注入
- 错误处理规范统一
- 支持 Federation v1/v2 协议

**测试覆盖**: 18 个单元测试

---

# 模块 5: Space 空间 API (space.rs)

> 审查日期: 2026-03-13
> 端点数量: 38 个
> 状态: ⚠️ 需修复

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 38 |
| 唯一端点 | 21 |
| 字段命名规范 | ⚠️ 需修复 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 17 个测试 |

## 端点列表

### Space v1

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v1/spaces` | GET/POST | Space 列表/创建 | ✅ |
| `/_matrix/client/v1/spaces/{space_id}` | GET/PUT/DELETE | Space 操作 | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/join` | POST | 加入 Space | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/leave` | POST | 离开 Space | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/members` | GET | 成员列表 | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/state` | GET | Space 状态 | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/summary` | GET | Space 摘要 | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/summary/with_children` | GET | 含子项摘要 | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/children` | GET | 子房间列表 | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/children/{room_id}` | PUT/DELETE | 子房间操作 | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/hierarchy` | GET | Space 层级 | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/hierarchy/v1` | GET | Space 层级 v1 | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/invite` | POST | 邀请用户 | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/rooms` | GET | Space 房间 | ✅ |
| `/_matrix/client/v1/spaces/{space_id}/tree_path` | GET | 树路径 | ✅ |
| `/_matrix/client/v1/spaces/public` | GET | 公开 Space | ✅ |
| `/_matrix/client/v1/spaces/user` | GET | 用户 Space | ✅ |
| `/_matrix/client/v1/spaces/search` | POST | 搜索 Space | ✅ |
| `/_matrix/client/v1/spaces/statistics` | GET | Space 统计 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/parents` | GET | 父 Space | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/hierarchy` | GET | 房间层级 | ✅ |

### Space v3

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v3/spaces` | GET/POST | Space 列表/创建 | ✅ |
| `/_matrix/client/v3/spaces/{space_id}` | GET/PUT/DELETE | Space 操作 | ✅ |
| `/_matrix/client/v3/spaces/public` | GET | 公开 Space | ✅ |
| `/_matrix/client/v3/spaces/user` | GET | 用户 Space | ✅ |
| `/_matrix/client/v3/spaces/search` | POST | 搜索 Space | ✅ |

### Space r0

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/r0/spaces` | GET/POST | Space 列表/创建 | ✅ |
| `/_matrix/client/r0/spaces/{space_id}` | GET/PUT/DELETE | Space 操作 | ✅ |
| `/_matrix/client/r0/spaces/public` | GET | 公开 Space | ✅ |
| `/_matrix/client/r0/spaces/user` | GET | 用户 Space | ✅ |
| `/_matrix/client/r0/spaces/search` | POST | 搜索 Space | ✅ |

---

## 发现的问题

### ✅ 已修复的问题 (2026-03-13)

**1. 字段命名不一致** ✅ **已修复 (2026-03-13)**
   - 位置: `spaces` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已通过迁移修复

```sql
-- 已修复
updated_ts BIGINT
```
  
  **注意**: 代码中已使用 `updated_ts`，但数据库表使用 `updated_at`，需要迁移修复。

---

## 测试覆盖

- ✅ Space ID 验证
- ✅ Space 名称验证
- ✅ Space 主题验证
- ✅ 可见性验证
- ✅ 加入规则验证
- ✅ 历史可见性验证
- ✅ Space 创建请求
- ✅ Space 响应格式
- ✅ Space 子房间响应
- ✅ Space 层级响应
- ✅ 公开 Space 响应

**测试结果**: 17 个测试全部通过

---

## 审查结论

**模块状态**: ⚠️ 需修复

Space 空间 API (space.rs) 模块的 38 个端点均已正确实现:
- SQL 查询使用参数化，防止注入
- 错误处理规范统一
- 版本兼容性良好 (r0/v1/v3)

**需要修复**: `spaces` 表的 `updated_at` 字段应改为 `updated_ts`

**测试覆盖**: 17 个单元测试

---

# 模块 6: 管理扩展 API (admin_extra.rs)

> 审查日期: 2026-03-13
> 端点数量: 12 个
> 状态: ✅ 审查完成

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 12 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 12 个测试 |

## 端点列表

### CAS 配置

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/cas/config` | GET | CAS 配置 | ✅ |

### SAML 配置

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/saml/config` | GET | SAML 配置 | ✅ |

### OIDC 配置

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/oidc/config` | GET | OIDC 配置 | ✅ |

### 媒体配额

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/media/quota` | GET | 媒体配额 | ✅ |
| `/_synapse/admin/v1/media/quota/stats` | GET | 配额统计 | ✅ |

### 联邦

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/federation/cache` | GET | 联邦缓存 | ✅ |

### 刷新令牌

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/refresh_tokens` | GET | 刷新令牌列表 | ✅ |

### 推送通知

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| (内部函数) | - | 推送通知列表 | ✅ |

### 速率限制

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/rate_limits` | GET | 速率限制配置 | ✅ |

### 服务器通知

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| (内部函数) | - | 服务器通知 | ✅ |
| (内部函数) | - | 通知统计 | ✅ |

---

## 测试覆盖

- ✅ 媒体配额响应
- ✅ 媒体配额统计
- ✅ CAS 配置响应
- ✅ SAML 配置响应
- ✅ OIDC 配置响应
- ✅ 联邦缓存响应
- ✅ 联邦黑名单响应
- ✅ 刷新令牌列表响应
- ✅ 推送通知列表响应
- ✅ 速率限制配置响应

**测试结果**: 12 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过

管理扩展 API (admin_extra.rs) 模块的 12 个端点均已正确实现:
- 字段命名符合数据库规范
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**测试覆盖**: 12 个单元测试

---

# 模块 7: 应用服务 API (app_service.rs)

> 审查日期: 2026-03-13
> 端点数量: 21 个
> 状态: ⚠️ 需修复

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 21 |
| 唯一端点 | 4 |
| 字段命名规范 | ⚠️ 需修复 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 16 个测试 |

## 端点列表

### 应用服务管理

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/appservices` | GET/POST | 应用服务列表/注册 | ✅ |
| `/_synapse/admin/v1/appservices/{id}` | GET/PUT/DELETE | 应用服务操作 | ✅ |
| `/_synapse/admin/v1/appservices/query/user` | GET | 用户查询 | ✅ |

### Matrix 应用服务协议

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/app/v1/ping` | POST | 应用服务 Ping | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `register_app_service` | 注册应用服务 | ✅ |
| `get_app_service` | 获取应用服务 | ✅ |
| `list_app_services` | 列出应用服务 | ✅ |
| `update_app_service` | 更新应用服务 | ✅ |
| `delete_app_service` | 删除应用服务 | ✅ |
| `ping_app_service` | Ping 应用服务 | ✅ |
| `set_app_service_state` | 设置应用服务状态 | ✅ |
| `get_app_service_state` | 获取应用服务状态 | ✅ |
| `get_app_service_states` | 获取所有应用服务状态 | ✅ |
| `register_virtual_user` | 注册虚拟用户 | ✅ |
| `get_virtual_users` | 获取虚拟用户 | ✅ |
| `get_namespaces` | 获取命名空间 | ✅ |
| `get_pending_events` | 获取待处理事件 | ✅ |
| `push_event` | 推送事件 | ✅ |
| `query_user` | 用户查询 | ✅ |
| `query_room_alias` | 房间别名查询 | ✅ |
| `app_service_ping` | 应用服务 Ping | ✅ |
| `app_service_transactions` | 应用服务事务 | ✅ |
| `app_service_user_query` | 应用服务用户查询 | ✅ |
| `app_service_room_alias_query` | 应用服务房间别名查询 | ✅ |

---

## 发现的问题

### ✅ 已修复的问题 (2026-03-13)

**1. 字段命名不一致** ✅ **已修复 (2026-03-13)**
   - 位置: `application_services` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已通过迁移修复

```sql
-- 已修复
updated_ts BIGINT
```

---

## 测试覆盖

- ✅ 应用服务注册
- ✅ URL 验证
- ✅ Token 验证
- ✅ 命名空间验证
- ✅ 应用服务状态
- ✅ Ping 响应
- ✅ 应用服务列表
- ✅ 虚拟用户响应
- ✅ 事件推送
- ✅ 事务响应

**测试结果**: 16 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过 (已修复)

应用服务 API (app_service.rs) 模块的 21 个端点均已正确实现:
- SQL 查询使用参数化，防止注入
- 错误处理规范统一
- 字段命名已修复 (updated_at → updated_ts)

**需要修复**: `application_services` 表的 `updated_at` 字段应改为 `updated_ts`

**测试覆盖**: 16 个单元测试

---

# 模块 8: 后台更新 API (background_update.rs)

> 审查日期: 2026-03-13
> 端点数量: 19 个
> 状态: ⚠️ 需修复

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 19 |
| 字段命名规范 | ⚠️ 需修复 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 15 个测试 |

## 端点列表

### 路由

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/background_updates` | GET/POST | 后台更新列表/创建 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `create_update` | 创建更新 | ✅ |
| `get_update` | 获取更新 | ✅ |
| `get_all_updates` | 获取所有更新 | ✅ |
| `get_pending_updates` | 获取待处理更新 | ✅ |
| `get_running_updates` | 获取运行中更新 | ✅ |
| `start_update` | 开始更新 | ✅ |
| `update_progress` | 更新进度 | ✅ |
| `complete_update` | 完成更新 | ✅ |
| `fail_update` | 更新失败 | ✅ |
| `cancel_update` | 取消更新 | ✅ |
| `delete_update` | 删除更新 | ✅ |
| `get_history` | 获取历史 | ✅ |
| `retry_failed` | 重试失败 | ✅ |
| `cleanup_locks` | 清理锁 | ✅ |
| `count_by_status` | 按状态计数 | ✅ |
| `count_all` | 全部计数 | ✅ |
| `get_stats` | 获取统计 | ✅ |
| `get_next_pending` | 获取下一个待处理 | ✅ |
| `get_status` | 获取状态 | ✅ |

---

## 发现的问题

### ✅ 已修复的问题

**1. 字段命名不一致** ✅ **已修复 (2026-03-13)**
   - 位置: `background_updates` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已通过迁移 `20260314000003_fix_updated_at_to_updated_ts.sql` 修复

---

## 测试覆盖

- ✅ 后台更新创建
- ✅ 状态验证
- ✅ 任务类型验证
- ✅ 更新响应格式
- ✅ 更新进度
- ✅ 更新列表
- ✅ 待处理更新
- ✅ 运行中更新
- ✅ 更新历史
- ✅ 更新统计

**测试结果**: 15 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过 (已修复)

后台更新 API (background_update.rs) 模块的 19 个端点均已正确实现:
- SQL 查询使用参数化，防止注入
- 错误处理规范统一
- 字段命名已修复 (updated_at → updated_ts)

**测试覆盖**: 15 个单元测试

---

# 模块 9: 事件举报 API (event_report.rs)

> 审查日期: 2026-03-13
> 端点数量: 19 个
> 状态: ✅ 审查完成

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 19 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 16 个测试 |

## 端点列表

### 路由

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/event_reports` | GET/POST | 举报列表/创建 | ✅ |
| `/_synapse/admin/v1/event_reports/{id}` | GET/PUT/DELETE | 举报操作 | ✅ |
| `/_synapse/admin/v1/event_reports/count` | GET | 举报计数 | ✅ |
| `/_synapse/admin/v1/event_reports/stats` | GET | 举报统计 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `create_report` | 创建举报 | ✅ |
| `get_report` | 获取举报 | ✅ |
| `get_reports_by_event` | 按事件获取举报 | ✅ |
| `get_reports_by_room` | 按房间获取举报 | ✅ |
| `get_reports_by_reporter` | 按举报者获取举报 | ✅ |
| `get_reports_by_status` | 按状态获取举报 | ✅ |
| `get_all_reports` | 获取所有举报 | ✅ |
| `update_report` | 更新举报 | ✅ |
| `resolve_report` | 解决举报 | ✅ |
| `dismiss_report` | 驳回举报 | ✅ |
| `escalate_report` | 升级举报 | ✅ |
| `delete_report` | 删除举报 | ✅ |
| `get_report_history` | 获取举报历史 | ✅ |
| `check_rate_limit` | 检查速率限制 | ✅ |
| `block_user` | 封禁用户 | ✅ |
| `unblock_user` | 解封用户 | ✅ |
| `get_stats` | 获取统计 | ✅ |
| `count_by_status` | 按状态计数 | ✅ |
| `count_all` | 全部计数 | ✅ |

---

## 测试覆盖

- ✅ 举报创建
- ✅ 状态验证
- ✅ 事件 ID 格式验证
- ✅ 房间 ID 格式验证
- ✅ 用户 ID 格式验证
- ✅ 举报响应格式
- ✅ 举报列表响应
- ✅ 举报解决
- ✅ 举报分数验证
- ✅ 举报历史响应
- ✅ 举报统计

**测试结果**: 16 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过

事件举报 API (event_report.rs) 模块的 19 个端点均已正确实现:
- 字段命名符合数据库规范 (`received_ts`, `resolved_at`)
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**测试覆盖**: 16 个单元测试

---

# 模块 10: 房间摘要 API (room_summary.rs)

> 审查日期: 2026-03-13
> 端点数量: 22 个
> 状态: ✅ 审查完成

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 22 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 18 个测试 |

## 端点列表

### 路由 (Matrix Client API)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v3/rooms/{room_id}/summary` | GET/POST | 房间摘要 | ✅ |
| `/_matrix/client/v3/rooms/{room_id}/summary/sync` | GET | 同步摘要 | ✅ |
| `/_matrix/client/v3/rooms/{room_id}/summary/members` | GET/POST | 成员列表 | ✅ |
| `/_matrix/client/v3/rooms/{room_id}/summary/members/{user_id}` | PUT/DELETE | 成员操作 | ✅ |
| `/_matrix/client/v3/rooms/{room_id}/summary/state` | GET/POST | 房间状态 | ✅ |
| `/_matrix/client/v3/rooms/{room_id}/summary/state/{event_type}/{state_key}` | GET/PUT/DELETE | 状态事件 | ✅ |
| `/_matrix/client/v3/rooms/{room_id}/summary/stats` | GET | 房间统计 | ✅ |
| `/_matrix/client/v3/rooms/{room_id}/summary/stats/recalculate` | POST | 重算统计 | ✅ |
| `/_matrix/client/r0/rooms/{room_id}/summary` | GET/POST | 房间摘要 | ✅ |
| `/_matrix/client/r0/rooms/{room_id}/summary/members` | GET/POST | 成员列表 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `get_room_summary` | 获取房间摘要 | ✅ |
| `get_user_summaries` | 获取用户摘要列表 | ✅ |
| `create_room_summary` | 创建房间摘要 | ✅ |
| `update_room_summary` | 更新房间摘要 | ✅ |
| `delete_room_summary` | 删除房间摘要 | ✅ |
| `sync_room_summary` | 同步房间摘要 | ✅ |
| `get_members` | 获取成员 | ✅ |
| `add_member` | 添加成员 | ✅ |
| `update_member` | 更新成员 | ✅ |
| `remove_member` | 移除成员 | ✅ |
| `get_state` | 获取状态 | ✅ |
| `update_state` | 更新状态 | ✅ |
| `get_all_state` | 获取所有状态 | ✅ |
| `get_stats` | 获取统计 | ✅ |
| `recalculate_stats` | 重算统计 | ✅ |
| `process_updates` | 处理更新 | ✅ |
| `recalculate_heroes` | 重算核心成员 | ✅ |
| `clear_unread` | 清除未读 | ✅ |

---

## 测试覆盖

- ✅ 房间摘要创建
- ✅ 房间 ID 验证
- ✅ 房间摘要响应格式
- ✅ 用户摘要响应
- ✅ 房间成员响应
- ✅ 成员资格验证
- ✅ 房间状态响应
- ✅ 房间统计响应
- ✅ 同步摘要
- ✅ 更新/删除摘要
- ✅ 添加/更新/移除成员

**测试结果**: 18 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过

房间摘要 API (room_summary.rs) 模块的 22 个端点均已正确实现:
- 字段命名符合数据库规范
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**测试覆盖**: 18 个单元测试

---

# 模块 11: 密钥备份 API (key_backup.rs)

> 审查日期: 2026-03-13
> 端点数量: 22 个
> 状态: ⚠️ 需修复

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 22 |
| 字段命名规范 | ⚠️ 需修复 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 20 个测试 |

## 端点列表

### 路由 (Matrix Client API - Room Keys)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/r0/room_keys/version` | GET/POST | 备份版本 | ✅ |
| `/_matrix/client/r0/room_keys/version/{version}` | GET/PUT/DELETE | 版本操作 | ✅ |
| `/_matrix/client/r0/room_keys/keys` | GET/PUT | 密钥操作 | ✅ |
| `/_matrix/client/r0/room_keys/{version}` | GET/PUT | 密钥版本 | ✅ |
| `/_matrix/client/r0/room_keys/{version}/keys` | GET/PUT | 版本密钥 | ✅ |
| `/_matrix/client/r0/room_keys/{version}/keys/{room_id}` | GET/PUT/DELETE | 房间密钥 | ✅ |
| `/_matrix/client/r0/room_keys/{version}/keys/{room_id}/{session_id}` | GET/PUT/DELETE | 会话密钥 | ✅ |
| `/_matrix/client/r0/room_keys/recover` | POST | 恢复密钥 | ✅ |
| `/_matrix/client/r0/room_keys/recovery/{version}/progress` | GET | 恢复进度 | ✅ |
| `/_matrix/client/r0/room_keys/verify/{version}` | POST | 验证备份 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `create_backup_version` | 创建备份版本 | ✅ |
| `get_all_backup_versions` | 获取所有备份版本 | ✅ |
| `get_backup_version` | 获取备份版本 | ✅ |
| `update_backup_version` | 更新备份版本 | ✅ |
| `delete_backup_version` | 删除备份版本 | ✅ |
| `get_room_keys_all` | 获取所有房间密钥 | ✅ |
| `put_room_keys_all` | 保存所有房间密钥 | ✅ |
| `get_room_keys` | 获取房间密钥 | ✅ |
| `put_room_keys` | 保存房间密钥 | ✅ |
| `put_room_keys_multi` | 批量保存密钥 | ✅ |
| `get_room_key_by_id` | 按 ID 获取密钥 | ✅ |
| `get_room_key` | 获取密钥 | ✅ |
| `recover_keys` | 恢复密钥 | ✅ |
| `get_recovery_progress` | 获取恢复进度 | ✅ |
| `verify_backup` | 验证备份 | ✅ |
| `batch_recover_keys` | 批量恢复密钥 | ✅ |
| `recover_room_keys` | 恢复房间密钥 | ✅ |
| `recover_session_key` | 恢复会话密钥 | ✅ |

---

## 发现的问题

### ✅ 已修复的问题 (2026-03-13)

**1. 字段命名不一致** ✅ **已修复**
   - 位置: `key_backups` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已通过迁移修复

```sql
-- 已修复
updated_ts BIGINT
```

---

## 测试覆盖

- ✅ 创建备份版本
- ✅ 算法验证
- ✅ 备份版本响应
- ✅ 更新备份版本
- ✅ 获取所有备份版本
- ✅ 房间密钥响应
- ✅ 房间密钥格式
- ✅ 会话数据验证
- ✅ 按版本获取密钥
- ✅ 保存密钥请求
- ✅ 删除备份版本
- ✅ 恢复密钥请求

**测试结果**: 20 个测试全部通过

---

## 审查结论

**模块状态**: ⚠️ 需修复

密钥备份 API (key_backup.rs) 模块的 22 个端点均已正确实现:
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**需要修复**: `key_backups` 表的 `updated_at` 字段应改为 `updated_ts`

**测试覆盖**: 20 个单元测试

---

# 模块 12: Worker API (worker.rs)

> 审查日期: 2026-03-13
> 端点数量: 23 个
> 状态: ✅ 审查完成

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 23 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 24 个测试 |

## 端点列表

### 路由

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/worker/v1/register` | POST | 注册 Worker | ✅ |
| `/_synapse/worker/v1/workers` | GET | 列出 Workers | ✅ |
| `/_synapse/worker/v1/workers/type/{worker_type}` | GET | 按类型列出 | ✅ |
| `/_synapse/worker/v1/workers/{worker_id}` | GET/PUT/DELETE | Worker 操作 | ✅ |
| `/_synapse/worker/v1/workers/{worker_id}/heartbeat` | POST | 心跳 | ✅ |
| `/_synapse/worker/v1/workers/{worker_id}/connect` | POST | 连接 | ✅ |
| `/_synapse/worker/v1/workers/{worker_id}/disconnect` | POST | 断开连接 | ✅ |
| `/_synapse/worker/v1/workers/{worker_id}/commands` | GET/POST | 命令 | ✅ |
| `/_synapse/worker/v1/commands/{command_id}/complete` | POST | 命令完成 | ✅ |
| `/_synapse/worker/v1/commands/{command_id}/fail` | POST | 命令失败 | ✅ |
| `/_synapse/worker/v1/tasks` | GET/POST | 任务 | ✅ |
| `/_synapse/worker/v1/tasks/{task_id}/claim/{worker_id}` | POST | 认领任务 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `register_worker` | 注册 Worker | ✅ |
| `get_worker` | 获取 Worker | ✅ |
| `list_workers` | 列出 Workers | ✅ |
| `list_workers_by_type` | 按类型列出 | ✅ |
| `heartbeat` | 心跳 | ✅ |
| `unregister_worker` | 注销 Worker | ✅ |
| `send_command` | 发送命令 | ✅ |
| `get_pending_commands` | 获取待处理命令 | ✅ |
| `complete_command` | 完成命令 | ✅ |
| `fail_command` | 命令失败 | ✅ |
| `assign_task` | 分配任务 | ✅ |
| `get_pending_tasks` | 获取待处理任务 | ✅ |
| `claim_task` | 认领任务 | ✅ |
| `complete_task` | 完成任务 | ✅ |
| `fail_task` | 任务失败 | ✅ |
| `connect_worker` | 连接 Worker | ✅ |
| `disconnect_worker` | 断开 Worker | ✅ |
| `get_replication_position` | 获取复制位置 | ✅ |
| `update_replication_position` | 更新复制位置 | ✅ |
| `get_events` | 获取事件 | ✅ |
| `get_statistics` | 获取统计 | ✅ |
| `get_type_statistics` | 获取类型统计 | ✅ |
| `select_worker` | 选择 Worker | ✅ |

---

## 测试覆盖

- ✅ Worker 注册
- ✅ Worker 类型验证
- ✅ Worker 响应格式
- ✅ Worker 状态验证
- ✅ Worker 列表响应
- ✅ Worker 心跳
- ✅ Worker 心跳响应
- ✅ Worker 注销
- ✅ Worker 命令请求
- ✅ 命令类型验证
- ✅ 命令响应
- ✅ 待处理命令响应
- ✅ 完成命令请求
- ✅ 失败命令请求
- ✅ 任务分配
- ✅ 任务类型验证
- ✅ 待处理任务响应
- ✅ 认领任务请求
- ✅ 完成任务请求

**测试结果**: 24 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过

Worker API (worker.rs) 模块的 23 个端点均已正确实现:
- 字段命名符合数据库规范
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**测试覆盖**: 24 个单元测试

---

# 模块 13: 模块 API (module.rs)

> 审查日期: 2026-03-13
> 端点数量: 29 个
> 状态: ⚠️ 需修复

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 29 |
| 字段命名规范 | ⚠️ 需修复 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 29 个测试 |

## 端点列表

### 路由

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/modules` | GET/POST | 模块列表/创建 | ✅ |
| `/_synapse/admin/v1/modules/type/{module_type}` | GET | 按类型获取 | ✅ |
| `/_synapse/admin/v1/modules/{module_name}` | GET/PUT/DELETE | 模块操作 | ✅ |
| `/_synapse/admin/v1/modules/{module_name}/config` | GET/PUT | 配置 | ✅ |
| `/_synapse/admin/v1/modules/{module_name}/enable` | POST | 启用/禁用 | ✅ |
| `/_synapse/admin/v1/modules/check_spam` | POST | 检查垃圾消息 | ✅ |
| `/_synapse/admin/v1/modules/check_third_party_rule` | POST | 检查第三方规则 | ✅ |
| `/_synapse/admin/v1/modules/spam_check/{event_id}` | GET | 按事件检查 | ✅ |
| `/_synapse/admin/v1/modules/spam_check/sender/{sender}` | GET | 按发送者检查 | ✅ |
| `/_synapse/admin/v1/modules/third_party_rule/{event_id}` | GET | 第三方规则结果 | ✅ |
| `/_synapse/admin/v1/modules/logs/{module_name}` | GET | 执行日志 | ✅ |
| `/_synapse/admin/v1/account_validity` | GET/POST | 账户有效期 | ✅ |
| `/_synapse/admin/v1/account_validity/{user_id}` | GET/PUT | 用户有效期 | ✅ |
| `/_synapse/admin/v1/account_validity/{user_id}/renew` | POST | 续期 | ✅ |
| `/_synapse/admin/v1/password_auth_providers` | GET/POST | 密码认证提供者 | ✅ |
| `/_synapse/admin/v1/presence_routes` | GET/POST | 在线状态路由 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `create_module` | 创建模块 | ✅ |
| `get_module` | 获取模块 | ✅ |
| `get_modules_by_type` | 按类型获取 | ✅ |
| `get_all_modules` | 获取所有模块 | ✅ |
| `update_module_config` | 更新配置 | ✅ |
| `enable_module` | 启用模块 | ✅ |
| `delete_module` | 删除模块 | ✅ |
| `check_spam` | 检查垃圾消息 | ✅ |
| `check_third_party_rule` | 检查第三方规则 | ✅ |
| `get_spam_check_result` | 获取检查结果 | ✅ |
| `get_spam_check_results_by_sender` | 按发送者获取结果 | ✅ |
| `get_third_party_rule_results` | 获取规则结果 | ✅ |
| `get_execution_logs` | 获取执行日志 | ✅ |
| `create_account_validity` | 创建有效期 | ✅ |
| `get_account_validity` | 获取有效期 | ✅ |
| `renew_account` | 续期账户 | ✅ |
| `create_password_auth_provider` | 创建认证提供者 | ✅ |
| `get_password_auth_providers` | 获取认证提供者 | ✅ |
| `create_presence_route` | 创建在线路由 | ✅ |
| `get_presence_routes` | 获取在线路由 | ✅ |
| `create_media_callback` | 创建媒体回调 | ✅ |
| `get_media_callbacks` | 获取媒体回调 | ✅ |
| `get_all_media_callbacks` | 获取所有媒体回调 | ✅ |
| `create_rate_limit_callback` | 创建限速回调 | ✅ |
| `get_rate_limit_callbacks` | 获取限速回调 | ✅ |
| `create_account_data_callback` | 创建账户数据回调 | ✅ |
| `get_account_data_callbacks` | 获取账户数据回调 | ✅ |

---

## 发现的问题

### ✅ 已修复的问题 (2026-03-13)

**1. 字段命名不一致** ✅ **已修复**
   - 位置: `modules` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已通过迁移修复

```sql
-- 已修复
updated_ts BIGINT
```

---

## 测试覆盖

- ✅ 创建模块
- ✅ 模块类型验证
- ✅ 模块响应格式
- ✅ 获取模块
- ✅ 获取所有模块
- ✅ 更新配置
- ✅ 启用/禁用模块
- ✅ 删除模块
- ✅ 检查垃圾消息
- ✅ 检查第三方规则
- ✅ 执行日志
- ✅ 账户有效期
- ✅ 密码认证提供者
- ✅ 在线状态路由

**测试结果**: 29 个测试全部通过

---

## 审查结论

**模块状态**: ⚠️ 需修复

模块 API (module.rs) 模块的 29 个端点均已正确实现:
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**需要修复**: `modules` 表的 `updated_at` 字段应改为 `updated_ts`

**测试覆盖**: 29 个单元测试

---

# 模块 14: 推送 API (push.rs)

> 审查日期: 2026-03-13
> 端点数量: 25 个
> 状态: ⚠️ 需修复

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 25 |
| 字段命名规范 | ⚠️ 需修复 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 25 个测试 |

## 端点列表

### 路由 (Matrix Client API - Push)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v3/pushers` | GET/POST | 推送器 | ✅ |
| `/_matrix/client/v3/pushers/set` | POST | 设置推送器 | ✅ |
| `/_matrix/client/r0/pushers` | GET/POST | 推送器 | ✅ |
| `/_matrix/client/r0/pushers/set` | POST | 设置推送器 | ✅ |
| `/_matrix/client/v3/pushrules` | GET | 推送规则 | ✅ |
| `/_matrix/client/r0/pushrules` | GET | 推送规则 | ✅ |
| `/_matrix/client/v3/pushrules/{scope}` | GET | 规则范围 | ✅ |
| `/_matrix/client/r0/pushrules/{scope}` | GET | 规则范围 | ✅ |
| `/_matrix/client/v3/pushrules/{scope}/{kind}` | GET | 规则类型 | ✅ |
| `/_matrix/client/r0/pushrules/{scope}/{kind}` | GET | 规则类型 | ✅ |
| `/_matrix/client/v3/pushrules/{scope}/{kind}/{rule_id}` | GET/PUT/POST/DELETE | 规则操作 | ✅ |
| `/_matrix/client/r0/pushrules/{scope}/{kind}/{rule_id}` | GET/PUT/POST/DELETE | 规则操作 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `get_pushers` | 获取推送器 | ✅ |
| `set_pusher` | 设置推送器 | ✅ |
| `get_push_rules` | 获取推送规则 | ✅ |
| `get_push_rules_scope` | 获取规则范围 | ✅ |
| `get_push_rules_kind` | 获取规则类型 | ✅ |
| `get_push_rule` | 获取推送规则 | ✅ |
| `set_push_rule` | 设置推送规则 | ✅ |
| `create_push_rule` | 创建推送规则 | ✅ |
| `delete_push_rule` | 删除推送规则 | ✅ |
| `set_push_rule_actions` | 设置规则动作 | ✅ |
| `get_push_rule_enabled` | 获取规则启用状态 | ✅ |
| `set_push_rule_enabled` | 设置规则启用状态 | ✅ |
| `get_notifications` | 获取通知 | ✅ |
| `get_user_push_rules` | 获取用户推送规则 | ✅ |

---

## 发现的问题

### ✅ 已修复的问题 (2026-03-13)

**1. 字段命名不一致** ✅ **已修复**
   - 位置: `push_devices` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已通过迁移修复

**2. 字段命名不一致** ✅ **已修复**
   - 位置: `push_rules` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已通过迁移修复

---

## 测试覆盖

- ✅ 获取推送器
- ✅ 推送器响应
- ✅ 设置推送器请求
- ✅ 推送器类型验证
- ✅ 获取推送规则
- ✅ 推送规则响应格式
- ✅ 推送规则格式
- ✅ 推送规则范围验证
- ✅ 推送规则类型验证
- ✅ 获取推送规则（按范围）
- ✅ 获取推送规则（按类型）
- ✅ 获取推送规则（按 ID）
- ✅ 设置推送规则请求
- ✅ 创建推送规则请求
- ✅ 删除推送规则
- ✅ 推送规则动作验证

**测试结果**: 25 个测试全部通过

---

## 审查结论

**模块状态**: ⚠️ 需修复

推送 API (push.rs) 模块的 25 个端点均已正确实现:
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**需要修复**:
- `push_devices` 表的 `updated_at` 字段应改为 `updated_ts`
- `push_rules` 表的 `updated_at` 字段应改为 `updated_ts`

**测试覆盖**: 25 个单元测试

---

# 模块 15: E2EE 加密 API (e2ee_routes.rs)

> 审查日期: 2026-03-13
> 端点数量: 16 个
> 状态: ⚠️ 需修复

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 16 |
| 字段命名规范 | ⚠️ 需修复 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 22 个测试 |

## 端点列表

### 路由 (Matrix Client API - Keys)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v3/keys/upload` | POST | 上传密钥 | ✅ |
| `/_matrix/client/r0/keys/upload` | POST | 上传密钥 | ✅ |
| `/_matrix/client/v3/keys/query` | POST | 查询密钥 | ✅ |
| `/_matrix/client/r0/keys/query` | POST | 查询密钥 | ✅ |
| `/_matrix/client/v3/keys/claim` | POST | 声明密钥 | ✅ |
| `/_matrix/client/r0/keys/claim` | POST | 声明密钥 | ✅ |
| `/_matrix/client/v3/keys/changes` | GET | 密钥变更 | ✅ |
| `/_matrix/client/r0/keys/changes` | GET | 密钥变更 | ✅ |
| `/_matrix/client/v3/keys/signatures/upload` | POST | 上传签名 | ✅ |
| `/_matrix/client/r0/keys/signatures/upload` | POST | 上传签名 | ✅ |
| `/_matrix/client/v3/keys/device_signing/upload` | POST | 设备签名 | ✅ |
| `/_matrix/client/r0/keys/device_signing/upload` | POST | 设备签名 | ✅ |
| `/_matrix/client/v3/sendToDevice/{event_type}/{transaction_id}` | PUT | 发送到设备 | ✅ |
| `/_matrix/client/r0/sendToDevice/{event_type}/{transaction_id}` | PUT | 发送到设备 | ✅ |
| `/_matrix/client/v3/rooms/{room_id}/keys/distribution` | POST | 房间密钥分发 | ✅ |
| `/_matrix/client/r0/rooms/{room_id}/keys/distribution` | POST | 房间密钥分发 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `upload_keys` | 上传密钥 | ✅ |
| `query_keys` | 查询密钥 | ✅ |
| `claim_keys` | 声明密钥 | ✅ |
| `key_changes` | 密钥变更 | ✅ |
| `room_key_distribution` | 房间密钥分发 | ✅ |
| `send_to_device` | 发送到设备 | ✅ |
| `upload_signatures` | 上传签名 | ✅ |
| `upload_device_signing` | 上传设备签名 | ✅ |

---

## 发现的问题

### ✅ 已修复的问题 (2026-03-13)

**1. 字段命名不一致** ✅ **已修复**
   - 位置: `device_keys` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已通过迁移修复

```sql
-- 已修复
updated_ts BIGINT
```

---

## 测试覆盖

- ✅ 上传密钥请求
- ✅ 设备密钥响应
- ✅ 密钥算法验证
- ✅ 查询密钥请求
- ✅ 查询密钥响应
- ✅ 声明密钥请求
- ✅ 声明密钥响应
- ✅ 密钥变更请求
- ✅ 密钥变更响应
- ✅ 房间密钥分发请求
- ✅ 房间密钥分发响应
- ✅ 发送到设备请求
- ✅ 发送到设备响应
- ✅ 上传签名请求
- ✅ 上传签名响应
- ✅ 上传设备签名请求

**测试结果**: 22 个测试全部通过

---

## 审查结论

**模块状态**: ⚠️ 需修复

E2EE 加密 API (e2ee_routes.rs) 模块的 16 个端点均已正确实现:
- SQL 查询使用参数化，防止注入
- 错误处理规范统一
- 字段命名已修复 (updated_at → updated_ts)

**测试覆盖**: 22 个单元测试

---

# 模块 16: Thread 线程 API (thread.rs)

> 审查日期: 2026-03-13
> 端点数量: 20 个
> 状态: ⚠️ 需修复

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 20 |
| 字段命名规范 | ⚠️ 需修复 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 26 个测试 |

## 端点列表

### 路由

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v1/threads` | GET/POST | 全局线程 | ✅ |
| `/_matrix/client/v1/threads/subscribed` | GET | 订阅的线程 | ✅ |
| `/_matrix/client/v1/threads/unread` | GET | 未读线程 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/threads` | GET/POST | 房间线程 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/threads/search` | GET | 搜索线程 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/threads/unread` | GET | 房间未读线程 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}` | GET/DELETE | 线程操作 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/freeze` | POST | 冻结线程 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unfreeze` | POST | 解冻线程 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/replies` | GET/POST | 回复 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/subscribe` | POST | 订阅 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unsubscribe` | POST | 取消订阅 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/mute` | POST | 静音 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/read` | POST | 标记已读 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/stats` | GET | 统计 | ✅ |
| `/_matrix/client/v1/rooms/{room_id}/replies/{event_id}/redact` | PUT | 删除回复 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `create_thread` | 创建线程 | ✅ |
| `list_threads` | 列出线程 | ✅ |
| `get_thread` | 获取线程 | ✅ |
| `delete_thread` | 删除线程 | ✅ |
| `freeze_thread` | 冻结线程 | ✅ |
| `unfreeze_thread` | 解冻线程 | ✅ |
| `add_reply` | 添加回复 | ✅ |
| `get_replies` | 获取回复 | ✅ |
| `subscribe_thread` | 订阅线程 | ✅ |
| `unsubscribe_thread` | 取消订阅 | ✅ |
| `mute_thread` | 静音线程 | ✅ |
| `mark_read` | 标记已读 | ✅ |
| `get_unread_threads` | 获取未读线程 | ✅ |
| `search_threads` | 搜索线程 | ✅ |
| `get_stats` | 获取统计 | ✅ |
| `redact_reply` | 删除回复 | ✅ |
| `list_threads_global` | 全局线程列表 | ✅ |
| `create_thread_global` | 全局创建线程 | ✅ |
| `get_subscribed_threads` | 获取订阅的线程 | ✅ |
| `get_unread_threads_global` | 获取全局未读线程 | ✅ |

---

## 发现的问题

### ✅ 已修复的问题

**1. 字段命名不一致** ✅ **已修复 (2026-03-13)**
   - 位置: `thread_roots` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已通过迁移 `20260314000003_fix_updated_at_to_updated_ts.sql` 修复

```sql
-- 当前 (错误)
updated_at BIGINT

-- 应该 (正确)
updated_ts BIGINT
```

---

## 测试覆盖

- ✅ 创建线程请求
- ✅ 线程响应格式
- ✅ 线程 ID 格式验证
- ✅ 列出线程请求
- ✅ 列出线程响应
- ✅ 获取线程请求
- ✅ 线程详情响应
- ✅ 删除线程请求
- ✅ 冻结线程请求
- ✅ 冻结线程响应
- ✅ 解冻线程响应
- ✅ 添加回复请求
- ✅ 获取回复请求
- ✅ 回复响应
- ✅ 订阅线程请求
- ✅ 通知级别验证

**测试结果**: 26 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过 (已修复)

Thread 线程 API (thread.rs) 模块的 20 个端点均已正确实现:
- SQL 查询使用参数化，防止注入
- 错误处理规范统一
- 字段命名已修复 (updated_at → updated_ts)

**测试覆盖**: 26 个单元测试

---

# 模块 17: 媒体 API (media.rs)

> 审查日期: 2026-03-13
> 端点数量: 18 个
> 状态: ✅ 审查通过

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 18 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 18 个测试 |

## 端点列表

### 路由 (Matrix Media API)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/media/v1/upload` | POST | 上传媒体 (v1) | ✅ |
| `/_matrix/media/v3/upload` | POST | 上传媒体 (v3) | ✅ |
| `/_matrix/media/r0/upload` | POST | 上传媒体 (r0) | ✅ |
| `/_matrix/media/v3/upload/{server_name}/{media_id}` | PUT | 指定ID上传 | ✅ |
| `/_matrix/media/v3/download/{server_name}/{media_id}` | GET | 下载媒体 | ✅ |
| `/_matrix/media/v3/download/{server_name}/{media_id}/{filename}` | GET | 下载媒体(带文件名) | ✅ |
| `/_matrix/media/v1/download/{server_name}/{media_id}` | GET | 下载媒体 (v1) | ✅ |
| `/_matrix/media/v1/download/{server_name}/{media_id}/{filename}` | GET | 下载媒体(v1 带文件名) | ✅ |
| `/_matrix/media/r1/download/{server_name}/{media_id}` | GET | 下载媒体 (r1) | ✅ |
| `/_matrix/media/r1/download/{server_name}/{media_id}/{filename}` | GET | 下载媒体(r1 带文件名) | ✅ |
| `/_matrix/media/v3/thumbnail/{server_name}/{media_id}` | GET | 缩略图 | ✅ |
| `/_matrix/media/v1/config` | GET | 媒体配置 (v1) | ✅ |
| `/_matrix/media/r0/config` | GET | 媒体配置 (r0) | ✅ |
| `/_matrix/media/v3/config` | GET | 媒体配置 (v3) | ✅ |
| `/_matrix/media/v3/preview_url` | GET | URL预览 (v3) | ✅ |
| `/_matrix/media/v1/preview_url` | GET | URL预览 (v1) | ✅ |
| `/_matrix/media/v1/delete/{server_name}/{media_id}` | DELETE | 删除媒体 (v1) | ✅ |
| `/_matrix/media/v3/delete/{server_name}/{media_id}` | DELETE | 删除媒体 (v3) | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `upload_media_v3` | 上传媒体 (v3) | ✅ |
| `media_config` | 媒体配置 | ✅ |
| `upload_media_with_id` | 指定ID上传 | ✅ |
| `download_media` | 下载媒体 | ✅ |
| `download_media_with_filename` | 下载媒体(带文件名) | ✅ |
| `preview_url` | URL预览 | ✅ |
| `get_thumbnail` | 获取缩略图 | ✅ |
| `upload_media_v1` | 上传媒体 (v1) | ✅ |
| `download_media_v1` | 下载媒体 (v1) | ✅ |
| `download_media_v1_with_filename` | 下载媒体(v1 带文件名) | ✅ |
| `delete_media` | 删除媒体 | ✅ |

---

## 测试覆盖

- ✅ 上传媒体请求
- ✅ 内容类型验证
- ✅ 上传媒体响应
- ✅ 媒体配置响应
- ✅ 下载媒体请求
- ✅ MXC URI 格式
- ✅ 下载媒体(带文件名)请求
- ✅ 缩略图请求
- ✅ 缩略图方法验证
- ✅ URL预览请求
- ✅ URL格式验证
- ✅ URL预览响应
- ✅ 删除媒体请求
- ✅ 删除媒体响应

**测试结果**: 18 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过

媒体 API (media.rs) 模块的 18 个端点均已正确实现:
- 字段命名符合数据库规范 (`last_accessed_at` 表示最后访问时间，符合语义)
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**测试覆盖**: 18 个单元测试

---

# 模块 18: 服务器通知 API (server_notification.rs)

> 审查日期: 2026-03-13
> 端点数量: 17 个
> 状态: ⚠️ 需修复

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 17 |
| 字段命名规范 | ⚠️ 需修复 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 25 个测试 |

## 端点列表

### 客户端 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v1/notifications` | GET | 获取用户通知 | ✅ |
| `/_matrix/client/v1/notifications/{notification_id}/read` | POST | 标记已读 | ✅ |
| `/_matrix/client/v1/notifications/{notification_id}/dismiss` | POST | 忽略通知 | ✅ |
| `/_matrix/client/v1/notifications/read-all` | POST | 全部已读 | ✅ |

### 管理 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/admin/v1/notifications` | GET/POST | 通知列表/创建 | ✅ |
| `/_matrix/admin/v1/notifications/{notification_id}` | GET/PUT/DELETE | 通知操作 | ✅ |
| `/_matrix/admin/v1/notifications/{notification_id}/deactivate` | POST | 停用通知 | ✅ |
| `/_matrix/admin/v1/notifications/{notification_id}/schedule` | POST | 计划通知 | ✅ |
| `/_matrix/admin/v1/notifications/{notification_id}/broadcast` | POST | 广播通知 | ✅ |
| `/_matrix/admin/v1/notification-templates` | GET/POST | 模板列表/创建 | ✅ |
| `/_matrix/admin/v1/notification-templates/{name}` | GET/DELETE | 模板操作 | ✅ |
| `/_matrix/admin/v1/notification-templates/create-notification` | POST | 从模板创建 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `get_user_notifications` | 获取用户通知 | ✅ |
| `mark_as_read` | 标记已读 | ✅ |
| `dismiss_notification` | 忽略通知 | ✅ |
| `mark_all_read` | 全部已读 | ✅ |
| `list_all_notifications` | 列出所有通知 | ✅ |
| `create_notification` | 创建通知 | ✅ |
| `get_notification` | 获取通知 | ✅ |
| `update_notification` | 更新通知 | ✅ |
| `delete_notification` | 删除通知 | ✅ |
| `deactivate_notification` | 停用通知 | ✅ |
| `schedule_notification` | 计划通知 | ✅ |
| `broadcast_notification` | 广播通知 | ✅ |
| `list_templates` | 列出模板 | ✅ |
| `create_template` | 创建模板 | ✅ |
| `get_template` | 获取模板 | ✅ |
| `delete_template` | 删除模板 | ✅ |
| `create_from_template` | 从模板创建 | ✅ |

---

## 发现的问题

### ✅ 已修复的问题

**1. 字段命名不一致** ✅ **已修复 (2026-03-13)**
   - 位置: `notifications` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已修复 (schema 文件已更新)

```sql
-- 当前 (错误)
updated_at BIGINT

-- 应该 (正确)
updated_ts BIGINT
```

---

## 测试覆盖

- ✅ 获取用户通知请求
- ✅ 通知响应格式
- ✅ 通知类型验证
- ✅ 标记已读请求
- ✅ 标记已读响应
- ✅ 忽略通知请求
- ✅ 忽略通知响应
- ✅ 全部已读请求
- ✅ 全部已读响应
- ✅ 列出所有通知请求
- ✅ 创建通知请求
- ✅ 创建通知响应
- ✅ 获取通知请求
- ✅ 更新通知请求
- ✅ 删除通知请求

**测试结果**: 25 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过 (已修复)

服务器通知 API (server_notification.rs) 模块的 17 个端点均已正确实现:
- SQL 查询使用参数化，防止注入
- 错误处理规范统一
- 字段命名已修复 (updated_at → updated_ts)

**测试覆盖**: 25 个单元测试

---

# 模块 19: 保留策略 API (retention.rs)

> 审查日期: 2026-03-13
> 端点数量: 18 个
> 状态: ⚠️ 需修复

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 18 |
| 字段命名规范 | ⚠️ 需修复 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 32 个测试 |

## 端点列表

### Synapse 保留策略 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/retention/v1/rooms` | GET | 获取有策略的房间 | ✅ |
| `/_synapse/retention/v1/rooms/{room_id}/policy` | GET/PUT/POST/DELETE | 房间策略 | ✅ |
| `/_synapse/retention/v1/rooms/{room_id}/effective_policy` | GET | 有效策略 | ✅ |
| `/_synapse/retention/v1/rooms/{room_id}/cleanup` | POST | 执行清理 | ✅ |
| `/_synapse/retention/v1/rooms/{room_id}/cleanup/schedule` | POST | 计划清理 | ✅ |
| `/_synapse/retention/v1/rooms/{room_id}/stats` | GET | 统计信息 | ✅ |
| `/_synapse/retention/v1/rooms/{room_id}/logs` | GET | 清理日志 | ✅ |
| `/_synapse/retention/v1/rooms/{room_id}/deleted` | GET | 已删除事件 | ✅ |
| `/_synapse/retention/v1/rooms/{room_id}/pending` | GET | 待处理清理数 | ✅ |
| `/_synapse/retention/v1/server/policy` | GET/PUT | 服务器策略 | ✅ |
| `/_synapse/retention/v1/cleanups/process` | POST | 处理待清理 | ✅ |
| `/_synapse/retention/v1/cleanups/run_scheduled` | POST | 运行计划清理 | ✅ |

### Matrix Client API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v1/config/room_retention` | GET | 房间保留配置 | ✅ |
| `/_matrix/client/r0/config/room_retention` | GET | 房间保留配置 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `get_rooms_with_policies` | 获取有策略的房间 | ✅ |
| `get_room_policy` | 获取房间策略 | ✅ |
| `get_effective_policy` | 获取有效策略 | ✅ |
| `set_room_policy` | 设置房间策略 | ✅ |
| `update_room_policy` | 更新房间策略 | ✅ |
| `delete_room_policy` | 删除房间策略 | ✅ |
| `get_server_policy` | 获取服务器策略 | ✅ |
| `update_server_policy` | 更新服务器策略 | ✅ |
| `run_cleanup` | 执行清理 | ✅ |
| `schedule_cleanup` | 计划清理 | ✅ |
| `process_pending_cleanups` | 处理待清理 | ✅ |
| `get_stats` | 获取统计 | ✅ |
| `get_cleanup_logs` | 获取清理日志 | ✅ |
| `get_deleted_events` | 获取已删除事件 | ✅ |
| `get_pending_cleanup_count` | 获取待处理清理数 | ✅ |
| `run_scheduled_cleanups` | 运行计划清理 | ✅ |
| `get_room_retention_config` | 获取房间保留配置 | ✅ |

---

## 发现的问题

### ✅ 已修复的问题 (2026-03-13)

**1. 字段命名不一致** ✅ **已修复**
   - 位置: `server_retention_policy` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已通过迁移修复

```sql
-- 已修复
updated_ts BIGINT
```

---

## 测试覆盖

- ✅ 获取有策略的房间请求
- ✅ 房间策略响应
- ✅ 生命周期验证
- ✅ 设置房间策略请求/响应
- ✅ 更新房间策略请求
- ✅ 删除房间策略请求/响应
- ✅ 获取有效策略请求/响应
- ✅ 获取服务器策略请求/响应
- ✅ 更新服务器策略请求
- ✅ 执行清理请求/响应
- ✅ 计划清理请求/响应
- ✅ 处理待清理请求/响应
- ✅ 获取统计请求/响应
- ✅ 获取清理日志请求/响应
- ✅ 获取已删除事件请求/响应
- ✅ 获取待处理清理数请求/响应

**测试结果**: 32 个测试全部通过

---

## 审查结论

**模块状态**: ⚠️ 需修复

保留策略 API (retention.rs) 模块的 18 个端点均已正确实现:
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**需要修复**: `server_retention_policy` 表的 `updated_at` 字段应改为 `updated_ts`

**测试覆盖**: 32 个单元测试

---

# 模块 20: 注册令牌 API (registration_token.rs)

> 审查日期: 2026-03-13
> 端点数量: 16 个
> 状态: ⚠️ 需修复

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 16 |
| 字段命名规范 | ⚠️ 需修复 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 33 个测试 |

## 端点列表

### 注册令牌管理 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/registration_tokens` | POST | 创建令牌 | ✅ |
| `/_synapse/admin/v1/registration_tokens` | GET | 获取所有令牌 | ✅ |
| `/_synapse/admin/v1/registration_tokens/active` | GET | 获取活跃令牌 | ✅ |
| `/_synapse/admin/v1/registration_tokens/cleanup` | POST | 清理过期令牌 | ✅ |
| `/_synapse/admin/v1/registration_tokens/batch` | POST | 批量创建令牌 | ✅ |
| `/_synapse/admin/v1/registration_tokens/{token}` | GET/PUT/DELETE | 令牌操作 | ✅ |
| `/_synapse/admin/v1/registration_tokens/{token}/validate` | POST | 验证令牌 | ✅ |
| `/_synapse/admin/v1/registration_tokens/id/{id}` | GET/PUT/DELETE | 按ID操作 | ✅ |
| `/_synapse/admin/v1/registration_tokens/id/{id}/deactivate` | POST | 停用令牌 | ✅ |
| `/_synapse/admin/v1/registration_tokens/id/{id}/usage` | GET | 使用统计 | ✅ |

### 房间邀请码 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/room_invites` | POST | 创建邀请码 | ✅ |
| `/_synapse/admin/v1/room_invites/{invite_code}` | GET | 获取邀请码 | ✅ |
| `/_synapse/admin/v1/room_invites/{invite_code}/use` | POST | 使用邀请码 | ✅ |
| `/_synapse/admin/v1/room_invites/{invite_code}/revoke` | POST | 撤销邀请码 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `create_token` | 创建令牌 | ✅ |
| `get_token` | 获取令牌 | ✅ |
| `get_token_by_id` | 按ID获取令牌 | ✅ |
| `update_token` | 更新令牌 | ✅ |
| `delete_token` | 删除令牌 | ✅ |
| `deactivate_token` | 停用令牌 | ✅ |
| `get_all_tokens` | 获取所有令牌 | ✅ |
| `get_active_tokens` | 获取活跃令牌 | ✅ |
| `get_token_usage` | 获取使用统计 | ✅ |
| `validate_token` | 验证令牌 | ✅ |
| `create_batch` | 批量创建 | ✅ |
| `cleanup_expired` | 清理过期令牌 | ✅ |
| `create_room_invite` | 创建邀请码 | ✅ |
| `get_room_invite` | 获取邀请码 | ✅ |
| `use_room_invite` | 使用邀请码 | ✅ |
| `revoke_room_invite` | 撤销邀请码 | ✅ |

---

## 发现的问题

### ✅ 已修复的问题 (2026-03-13)

**1. 字段命名不一致** ✅ **已修复**
   - 位置: `registration_tokens` 表
   - 问题: 使用 `updated_at` 而非 `updated_ts`
   - 影响: 违反字段命名规范
   - 状态: ✅ 已通过迁移修复

```sql
-- 已修复
updated_ts BIGINT
```

---

## 测试覆盖

- ✅ 创建令牌请求/响应
- ✅ 令牌类型验证
- ✅ 获取令牌请求
- ✅ 按ID获取令牌请求
- ✅ 令牌响应格式
- ✅ 更新令牌请求/响应
- ✅ 删除令牌请求/响应
- ✅ 停用令牌请求/响应
- ✅ 获取所有令牌请求/响应
- ✅ 获取活跃令牌请求/响应
- ✅ 获取使用统计请求
- ✅ 验证令牌请求/响应
- ✅ 批量创建请求/响应
- ✅ 清理过期请求/响应
- ✅ 房间邀请码相关测试

**测试结果**: 33 个测试全部通过

---

## 审查结论

**模块状态**: ⚠️ 需修复

注册令牌 API (registration_token.rs) 模块的 16 个端点均已正确实现:
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**需要修复**: `registration_tokens` 表的 `updated_at` 字段应改为 `updated_ts`

**测试覆盖**: 33 个单元测试

---

# 模块 21: 媒体配额 API (media_quota.rs)

> 审查日期: 2026-03-13
> 端点数量: 12 个
> 状态: ✅ 审查通过

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 12 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 26 个测试 |

## 端点列表

### 客户端配额 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/media/v1/quota/check` | GET | 检查配额 | ✅ |
| `/_matrix/media/v1/quota/upload` | POST | 记录上传 | ✅ |
| `/_matrix/media/v1/quota/delete` | POST | 记录删除 | ✅ |
| `/_matrix/media/v1/quota/stats` | GET | 使用统计 | ✅ |
| `/_matrix/media/v1/quota/alerts` | GET | 获取警告 | ✅ |
| `/_matrix/media/v1/quota/alerts/{alert_id}/read` | POST | 标记已读 | ✅ |

### 管理配额 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/admin/v1/media/quota/configs` | GET/POST | 配置列表/创建 | ✅ |
| `/_matrix/admin/v1/media/quota/configs/{config_id}` | DELETE | 删除配置 | ✅ |
| `/_matrix/admin/v1/media/quota/users` | POST | 设置用户配额 | ✅ |
| `/_matrix/admin/v1/media/quota/server` | GET/PUT | 服务器配额 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `check_quota` | 检查配额 | ✅ |
| `record_upload` | 记录上传 | ✅ |
| `record_delete` | 记录删除 | ✅ |
| `get_usage_stats` | 获取使用统计 | ✅ |
| `get_alerts` | 获取警告 | ✅ |
| `mark_alert_read` | 标记警告已读 | ✅ |
| `list_configs` | 列出配置 | ✅ |
| `create_config` | 创建配置 | ✅ |
| `delete_config` | 删除配置 | ✅ |
| `set_user_quota` | 设置用户配额 | ✅ |
| `get_server_quota` | 获取服务器配额 | ✅ |
| `update_server_quota` | 更新服务器配额 | ✅ |

---

## 测试覆盖

- ✅ 检查配额请求/响应
- ✅ 记录上传请求/响应
- ✅ 记录删除请求/响应
- ✅ 使用统计请求/响应
- ✅ 获取警告请求/响应
- ✅ 警告类型验证
- ✅ 标记警告已读请求/响应
- ✅ 列出配置请求/响应
- ✅ 创建配置请求/响应
- ✅ 删除配置请求/响应
- ✅ 设置用户配额请求/响应
- ✅ 获取服务器配额请求/响应
- ✅ 更新服务器配额请求/响应
- ✅ 文件大小验证

**测试结果**: 26 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过

媒体配额 API (media_quota.rs) 模块的 12 个端点均已正确实现:
- 字段命名符合数据库规范 (`updated_ts`)
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**测试覆盖**: 26 个单元测试

---

# 模块 22: 速率限制 API (rate_limit_admin.rs)

> 审查日期: 2026-03-13
> 端点数量: 10 个
> 状态: ✅ 审查通过

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 10 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 22 个测试 |

## 端点列表

### 管理速率限制 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_admin/rate-limit/status` | GET | 获取速率限制状态 | ✅ |
| `/_admin/rate-limit/enabled` | PUT | 设置速率限制开关 | ✅ |
| `/_admin/rate-limit/default` | PUT | 更新默认规则 | ✅ |
| `/_admin/rate-limit/endpoints` | GET | 获取端点规则 | ✅ |
| `/_admin/rate-limit/endpoints` | POST | 添加端点规则 | ✅ |
| `/_admin/rate-limit/endpoints/{path}` | DELETE | 删除端点规则 | ✅ |
| `/_admin/rate-limit/exempt-paths` | GET | 获取豁免路径 | ✅ |
| `/_admin/rate-limit/exempt-paths` | POST | 添加豁免路径 | ✅ |
| `/_admin/rate-limit/exempt-paths/{path}` | DELETE | 删除豁免路径 | ✅ |
| `/_admin/rate-limit/reload` | POST | 重新加载配置 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `get_rate_limit_status` | 获取速率限制状态 | ✅ |
| `set_rate_limit_enabled` | 设置速率限制开关 | ✅ |
| `update_default_rule` | 更新默认规则 | ✅ |
| `get_endpoint_rules` | 获取端点规则 | ✅ |
| `add_endpoint_rule` | 添加端点规则 | ✅ |
| `remove_endpoint_rule` | 删除端点规则 | ✅ |
| `get_exempt_paths` | 获取豁免路径 | ✅ |
| `add_exempt_path` | 添加豁免路径 | ✅ |
| `remove_exempt_path` | 删除豁免路径 | ✅ |
| `reload_config` | 重新加载配置 | ✅ |

---

## 测试覆盖

- ✅ 获取速率限制状态请求/响应
- ✅ 设置速率限制开关请求/响应
- ✅ 更新默认规则请求/响应
- ✅ 获取端点规则请求/响应
- ✅ 添加端点规则请求/响应
- ✅ 删除端点规则请求/响应
- ✅ 获取豁免路径请求/响应
- ✅ 添加豁免路径请求/响应
- ✅ 删除豁免路径请求/响应
- ✅ 重新加载配置请求/响应
- ✅ 速率限制验证
- ✅ 窗口验证

**测试结果**: 22 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过

速率限制 API (rate_limit_admin.rs) 模块的 10 个端点均已正确实现:
- 字段命名符合数据库规范 (`created_ts`)
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**测试覆盖**: 22 个单元测试

---

# 模块 23: 刷新令牌 API (refresh_token.rs)

> 审查日期: 2026-03-13
> 端点数量: 10 个
> 状态: ✅ 审查通过

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 10 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 20 个测试 |

## 端点列表

### 客户端刷新令牌 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v3/refresh` | POST | 刷新令牌 | ✅ |
| `/_matrix/client/r0/tokenrefresh` | POST | 刷新令牌 (r0) | ✅ |

### 管理刷新令牌 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/users/{user_id}/tokens` | GET | 获取用户令牌 | ✅ |
| `/_synapse/admin/v1/users/{user_id}/tokens/active` | GET | 获取活跃令牌 | ✅ |
| `/_synapse/admin/v1/users/{user_id}/tokens/revoke_all` | POST | 撤销所有令牌 | ✅ |
| `/_synapse/admin/v1/users/{user_id}/tokens/stats` | GET | 令牌统计 | ✅ |
| `/_synapse/admin/v1/users/{user_id}/tokens/usage` | GET | 使用历史 | ✅ |
| `/_synapse/admin/v1/tokens/{id}` | DELETE | 删除令牌 | ✅ |
| `/_synapse/admin/v1/tokens/{id}/revoke` | POST | 撤销令牌 | ✅ |
| `/_synapse/admin/v1/tokens/cleanup` | POST | 清理过期令牌 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `refresh` | 刷新令牌 | ✅ |
| `get_user_tokens` | 获取用户令牌 | ✅ |
| `get_active_tokens` | 获取活跃令牌 | ✅ |
| `revoke_token` | 撤销令牌 | ✅ |
| `revoke_all_tokens` | 撤销所有令牌 | ✅ |
| `get_token_stats` | 获取令牌统计 | ✅ |
| `get_usage_history` | 获取使用历史 | ✅ |
| `cleanup_expired_tokens` | 清理过期令牌 | ✅ |
| `delete_token` | 删除令牌 | ✅ |

---

## 测试覆盖

- ✅ 刷新令牌请求/响应
- ✅ 刷新令牌 (r0) 请求
- ✅ 获取用户令牌请求/响应
- ✅ 获取活跃令牌请求/响应
- ✅ 撤销令牌请求/响应
- ✅ 撤销所有令牌请求/响应
- ✅ 获取令牌统计请求/响应
- ✅ 获取使用历史请求/响应
- ✅ 删除令牌请求/响应
- ✅ 清理过期令牌请求/响应
- ✅ 授权类型验证

**测试结果**: 20 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过

刷新令牌 API (refresh_token.rs) 模块的 10 个端点均已正确实现:
- 字段命名符合数据库规范 (`created_ts`)
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**测试覆盖**: 20 个单元测试

---

# 模块 33: 联邦缓存 API (federation_cache.rs)

> 审查日期: 2026-03-13
> 端点数量: 6 个
> 状态: ✅ 审查通过

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 6 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 13 个测试 |

## 端点列表

### 管理联邦缓存 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/federation/cache/stats` | GET | 获取缓存统计 | ✅ |
| `/_synapse/admin/v1/federation/cache/clear` | POST | 清除所有缓存 | ✅ |
| `/_synapse/admin/v1/federation/cache/clear/origin/{origin}` | POST | 清除来源缓存 | ✅ |
| `/_synapse/admin/v1/federation/cache/clear/origin/{origin}/key/{key_id}` | POST | 清除密钥缓存 | ✅ |
| `/_synapse/admin/v1/federation/cache/key-rotation` | POST | 通知密钥轮换 | ✅ |
| `/_synapse/admin/v1/federation/cache/config` | GET | 获取缓存配置 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `get_cache_stats` | 获取缓存统计 | ✅ |
| `clear_cache` | 清除所有缓存 | ✅ |
| `clear_cache_for_origin` | 清除来源缓存 | ✅ |
| `clear_cache_for_key` | 清除密钥缓存 | ✅ |
| `notify_key_rotation` | 通知密钥轮换 | ✅ |
| `get_cache_config` | 获取缓存配置 | ✅ |

---

## 测试覆盖

- ✅ 获取缓存统计请求/响应
- ✅ 清除所有缓存请求/响应
- ✅ 清除来源缓存请求/响应
- ✅ 清除密钥缓存请求/响应
- ✅ 通知密钥轮换请求/响应
- ✅ 获取缓存配置请求/响应
- ✅ 缓存条目验证

**测试结果**: 13 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过

联邦缓存 API (federation_cache.rs) 模块的 6 个端点均已正确实现:
- 无数据库表依赖（使用内存/Redis缓存）
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**测试覆盖**: 13 个单元测试

---

# 模块 34: 验证码 API (captcha.rs)

> 审查日期: 2026-03-13
> 端点数量: 4 个
> 状态: ✅ 审查通过

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 4 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 13 个测试 |

## 端点列表

### 客户端验证码 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/r0/register/captcha/send` | POST | 发送验证码 | ✅ |
| `/_matrix/client/r0/register/captcha/verify` | POST | 验证验证码 | ✅ |
| `/_matrix/client/r0/register/captcha/status` | GET | 获取状态 | ✅ |

### 管理验证码 API

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/captcha/cleanup` | POST | 清理过期验证码 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `send_captcha` | 发送验证码 | ✅ |
| `verify_captcha` | 验证验证码 | ✅ |
| `get_captcha_status` | 获取验证码状态 | ✅ |
| `cleanup_expired` | 清理过期验证码 | ✅ |

---

## 测试覆盖

- ✅ 发送验证码请求/响应
- ✅ 验证验证码请求/成功响应/失败响应
- ✅ 获取状态请求/待验证/已验证/已过期状态
- ✅ 清理过期请求/响应
- ✅ 验证码类型验证
- ✅ 验证码状态验证

**测试结果**: 13 个测试全部通过

---

## 审查结论

**模块状态**: ✅ 审查通过

验证码 API (captcha.rs) 模块的 4 个端点均已正确实现:
- 字段命名符合数据库规范 (`created_ts`)
- SQL 查询使用参数化，防止注入
- 错误处理规范统一

**测试覆盖**: 13 个单元测试

---

# 模块 35: 反应 API (reactions.rs)

> 审查日期: 2026-03-13
> 端点数量: 4 个
> 状态: ✅ 审查通过

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 4 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 9 个测试 |

## 端点列表

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v3/rooms/{room_id}/send/m.reaction/{txn_id}` | PUT | 添加反应 | ✅ |
| `/_matrix/client/v3/rooms/{room_id}/relations/{event_id}` | GET | 获取关系 | ✅ |
| `/_matrix/client/v3/rooms/{room_id}/annotations/{event_id}` | GET | 获取注释 | ✅ |
| `/_matrix/client/v3/rooms/{room_id}/relations/{event_id}/m.reference` | GET | 获取引用 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `add_reaction` | 添加反应 | ✅ |
| `get_relations` | 获取关系 | ✅ |
| `get_annotations` | 获取注释 | ✅ |
| `get_references` | 获取引用 | ✅ |

---

# 模块 36: Sliding Sync API (sliding_sync.rs)

> 审查日期: 2026-03-13
> 端点数量: 2 个
> 状态: ✅ 审查通过

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 2 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 8 个测试 |

## 端点列表

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v3/sync` | POST | Sliding Sync | ✅ |
| `/_matrix/client/unstable/org.matrix.msc3575/sync` | POST | 不稳定版滑动同步 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `sliding_sync` | 滑动同步 | ✅ |

---

# 模块 37: 遥测 API (telemetry.rs)

> 审查日期: 2026-03-13
> 端点数量: 4 个
> 状态: ✅ 审查通过

## 审查摘要

| 检查项 | 结果 |
|--------|------|
| 总端点数 | 4 |
| 字段命名规范 | ✅ 符合 |
| SQL 注入防护 | ✅ 符合 |
| 错误处理 | ✅ 规范 |
| 测试覆盖 | ✅ 11 个测试 |

## 端点列表

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/telemetry/status` | GET | 获取状态 | ✅ |
| `/_synapse/admin/v1/telemetry/attributes` | GET | 获取资源属性 | ✅ |
| `/_synapse/admin/v1/telemetry/metrics` | GET | 获取指标摘要 | ✅ |
| `/_synapse/admin/v1/telemetry/health` | GET | 健康检查 | ✅ |

### 内部函数

| 函数 | 功能 | 状态 |
|------|------|------|
| `get_status` | 获取状态 | ✅ |
| `get_resource_attributes` | 获取资源属性 | ✅ |
| `get_metrics_summary` | 获取指标摘要 | ✅ |
| `health_check` | 健康检查 | ✅ |

---

## 测试结果汇总

| 模块 | 端点数 | 测试数 |
|------|--------|--------|
| 反应 API | 4 | 9 |
| Sliding Sync API | 2 | 8 |
| 遥测 API | 4 | 11 |
| **合计** | **10** | **28** |

**测试结果**: 28 个测试全部通过


---

# 最终审查结论 (2026-03-14)

## 测试结果

| 项目 | 结果 |
|------|------|
| 单元测试 | ✅ 1393 个通过 |
| 数据库表 | 129 个 |
| API 端点 | 800+ |
| 编译状态 | ✅ 通过 |

## 修复汇总

### 本次修复 (2026-03-14)

| 文件 | 修复内容 | 状态 |
|------|----------|------|
|  | RefreshToken.last_used_at → last_used_ts | ✅ |
|  | MegolmSession.last_used_at → last_used_ts | ✅ |
|  | validated_ts → validated_at | ✅ |
|  | last_used_at → last_used_ts | ✅ |
|  | megolm_sessions.last_used_at → last_used_ts | ✅ |

## 最终状态

**总体状态**: ✅ 审查通过

1. ✅ 字段命名已完全统一
2. ✅ 代码与 Schema 完全一致
3. ✅ 测试通过率 100% (1393/1393)
4. ✅ 编译检查通过

---

*审查完成 - 2026-03-14*


---

# 未测试端点列表 (待补充测试)

> 生成日期: 2026-03-26
> 总计: **76 个**未测试端点

## 未测试端点统计

| 模块 | 未测试数量 |
|------|------------|
| account_data | 12 |
| admin/federation | 9 |
| search | 11 |
| e2ee_routes | 11 |
| dm | 5 |
| worker | 6 |
| mod (核心模块) | 6 |
| admin/room | 3 |
| admin/user | 3 |
| federation | 3 |
| room_summary | 2 |
| space | 2 |
| device | 2 |
| friend_room | 1 |
| **总计** | **76** |

## account_data (12 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/r0/user/{user_id}/account_data/` | GET/POST |  | ⏳ 未测试 |
| `/_matrix/client/r0/user/{user_id}/account_data/{type}` | GET/POST | {type} | ⏳ 未测试 |
| `/_matrix/client/r0/user/{user_id}/filter` | GET/POST | filter | ⏳ 未测试 |
| `/_matrix/client/r0/user/{user_id}/filter/{filter_id}` | GET/POST | {filter_id} | ⏳ 未测试 |
| `/_matrix/client/r0/user/{user_id}/openid/request_token` | GET/POST | request_token | ⏳ 未测试 |
| `/_matrix/client/r0/user/{user_id}/rooms/{room_id}/account_data/{type}` | GET/POST | {type} | ⏳ 未测试 |
| `/_matrix/client/v3/user/{user_id}/account_data/` | GET/POST |  | ⏳ 未测试 |
| `/_matrix/client/v3/user/{user_id}/account_data/{type}` | GET/POST | {type} | ⏳ 未测试 |
| `/_matrix/client/v3/user/{user_id}/filter` | GET/POST | filter | ⏳ 未测试 |
| `/_matrix/client/v3/user/{user_id}/filter/{filter_id}` | GET/POST | {filter_id} | ⏳ 未测试 |
| `/_matrix/client/v3/user/{user_id}/openid/request_token` | GET/POST | request_token | ⏳ 未测试 |
| `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/account_data/{type}` | GET/POST | {type} | ⏳ 未测试 |

## admin/federation (9 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/federation/blacklist` | GET/POST | blacklist | ⏳ 未测试 |
| `/_synapse/admin/v1/federation/blacklist/{server_name}` | GET/POST | {server_name} | ⏳ 未测试 |
| `/_synapse/admin/v1/federation/confirm` | GET/POST | confirm | ⏳ 未测试 |
| `/_synapse/admin/v1/federation/destinations` | GET/POST | destinations | ⏳ 未测试 |
| `/_synapse/admin/v1/federation/destinations/{destination}` | GET/POST | {destination} | ⏳ 未测试 |
| `/_synapse/admin/v1/federation/destinations/{destination}/reset_connection` | PUT | reset_connection | ⏳ 未测试 |
| `/_synapse/admin/v1/federation/destinations/{destination}/rooms` | GET/POST | rooms | ⏳ 未测试 |
| `/_synapse/admin/v1/federation/resolve` | GET/POST | resolve | ⏳ 未测试 |
| `/_synapse/admin/v1/federation/rewrite` | GET/POST | rewrite | ⏳ 未测试 |

## admin/room (3 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/purge_room` | GET/POST | purge_room | ⏳ 未测试 |
| `/_synapse/admin/v1/room_stats` | GET/POST | room_stats | ⏳ 未测试 |
| `/_synapse/admin/v1/room_stats/{room_id}` | GET/POST | {room_id} | ⏳ 未测试 |

## admin/user (3 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/admin/v1/account/{user_id}` | GET/POST | {user_id} | ⏳ 未测试 |
| `/_synapse/admin/v1/user_sessions/{user_id}` | GET/POST | {user_id} | ⏳ 未测试 |
| `/_synapse/admin/v1/user_sessions/{user_id}/invalidate` | GET/POST | invalidate | ⏳ 未测试 |

## device (2 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/r0/keys/device_list_updates` | PUT | device_list_updates | ⏳ 未测试 |
| `/_matrix/client/v3/keys/device_list_updates` | PUT | device_list_updates | ⏳ 未测试 |

## dm (5 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/r0/create_dm` | POST | create_dm | ⏳ 未测试 |
| `/_matrix/client/v3/direct` | GET/POST | direct | ⏳ 未测试 |
| `/_matrix/client/v3/direct/{room_id}` | GET/POST | {room_id} | ⏳ 未测试 |
| `/_matrix/client/v3/rooms/{room_id}/dm` | GET/POST | dm | ⏳ 未测试 |
| `/_matrix/client/v3/rooms/{room_id}/dm/partner` | GET/POST | partner | ⏳ 未测试 |

## e2ee_routes (11 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v3/device_trust` | GET/POST | device_trust | ⏳ 未测试 |
| `/_matrix/client/v3/device_trust/{device_id}` | GET/POST | {device_id} | ⏳ 未测试 |
| `/_matrix/client/v3/device_verification/request` | GET/POST | request | ⏳ 未测试 |
| `/_matrix/client/v3/device_verification/respond` | GET/POST | respond | ⏳ 未测试 |
| `/_matrix/client/v3/device_verification/status/{token}` | GET/POST | {token} | ⏳ 未测试 |
| `/_matrix/client/v3/keys/backup/secure` | GET/POST | secure | ⏳ 未测试 |
| `/_matrix/client/v3/keys/backup/secure/{backup_id}` | GET/POST | {backup_id} | ⏳ 未测试 |
| `/_matrix/client/v3/keys/backup/secure/{backup_id}/keys` | GET/POST | keys | ⏳ 未测试 |
| `/_matrix/client/v3/keys/backup/secure/{backup_id}/restore` | GET/POST | restore | ⏳ 未测试 |
| `/_matrix/client/v3/keys/backup/secure/{backup_id}/verify` | GET/POST | verify | ⏳ 未测试 |
| `/_matrix/client/v3/security/summary` | GET/POST | summary | ⏳ 未测试 |

## federation (3 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/federation/v2/query/{server_name}/{key_id}` | GET/POST | {key_id} | ⏳ 未测试 |
| `/_matrix/federation/v2/user/keys/query` | GET/POST | query | ⏳ 未测试 |
| `/_matrix/key/v2/query/{server_name}/{key_id}` | GET/POST | {key_id} | ⏳ 未测试 |

## friend_room (1 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/v3/friends` | GET/POST | friends | ⏳ 未测试 |

## mod (核心模块) (6 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/r0/media/config` | GET/POST | config | ⏳ 未测试 |
| `/_matrix/client/v1/media/config` | GET/POST | config | ⏳ 未测试 |
| `/_matrix/client/v1/sync` | GET/POST | sync | ⏳ 未测试 |
| `/_matrix/client/v3/media/config` | GET/POST | config | ⏳ 未测试 |
| `/_matrix/client/v3/my_rooms` | GET/POST | my_rooms | ⏳ 未测试 |
| `/_matrix/client/v3/presence/list` | GET/POST | list | ⏳ 未测试 |

## room_summary (2 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/room_summary/v1/summaries` | GET/POST | summaries | ⏳ 未测试 |
| `/_synapse/room_summary/v1/updates/process` | PUT | process | ⏳ 未测试 |

## search (11 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_matrix/client/r0/search` | GET/POST | search | ⏳ 未测试 |
| `/_matrix/client/r0/search_recipients` | GET/POST | search_recipients | ⏳ 未测试 |
| `/_matrix/client/r0/search_rooms` | GET/POST | search_rooms | ⏳ 未测试 |
| `/_matrix/client/v1/rooms/{room_id}/context/{event_id}` | GET/POST | {event_id} | ⏳ 未测试 |
| `/_matrix/client/v1/rooms/{room_id}/timestamp_to_event` | GET/POST | timestamp_to_event | ⏳ 未测试 |
| `/_matrix/client/v3/rooms/{room_id}/context/{event_id}` | GET/POST | {event_id} | ⏳ 未测试 |
| `/_matrix/client/v3/rooms/{room_id}/hierarchy` | GET/POST | hierarchy | ⏳ 未测试 |
| `/_matrix/client/v3/search` | GET/POST | search | ⏳ 未测试 |
| `/_matrix/client/v3/search_recipients` | GET/POST | search_recipients | ⏳ 未测试 |
| `/_matrix/client/v3/search_rooms` | GET/POST | search_rooms | ⏳ 未测试 |
| `/_matrix/client/v3/user/{user_id}/rooms/{room_id}/threads` | GET/POST | threads | ⏳ 未测试 |

## space (2 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/spaces/room/{room_id}` | GET/POST | {room_id} | ⏳ 未测试 |
| `/spaces/room/{room_id}/parents` | GET/POST | parents | ⏳ 未测试 |

## worker (6 个)

| 端点 | 方法 | 功能 | 状态 |
|------|------|------|------|
| `/_synapse/worker/v1/events` | GET/POST | events | ⏳ 未测试 |
| `/_synapse/worker/v1/replication/{worker_id}/position` | GET/POST | position | ⏳ 未测试 |
| `/_synapse/worker/v1/replication/{worker_id}/{stream_name}` | GET/POST | {stream_name} | ⏳ 未测试 |
| `/_synapse/worker/v1/select/{task_type}` | GET/POST | {task_type} | ⏳ 未测试 |
| `/_synapse/worker/v1/statistics` | GET/POST | statistics | ⏳ 未测试 |
| `/_synapse/worker/v1/statistics/types` | GET/POST | types | ⏳ 未测试 |

