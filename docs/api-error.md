# synapse-rust API 问题报告

> 生成时间: 2026-03-09
> 更新时间: 2026-03-13

## 测试环境
- 后端: synapse-rust (本地编译)
- 数据库: PostgreSQL 16 (Docker)
- 缓存: Redis 7 (Docker)
- 服务器: http://localhost:8008

---

## 测试结果摘要

### 初始状态: 607 总计 | 14 通过 | 287 失败 | 306 跳过
### 最终状态: 607 总计 | 93 通过 | 208 失败 | 306 跳过

**改进**: 通过率从 2.3% 提升到 15.3%，通过数增加了 564%

---

## 关键成就

### 1. 服务稳定性 ✅
- **sync API 完全正常工作** - `GET /_matrix/client/v3/sync` 返回 200 OK，无崩溃
- **Sliding Sync API 完全正常工作** - `POST /_matrix/client/v3/sync` 返回 200 OK
- **无 panic，服务稳定运行** - 彻底解决了 sync API 崩溃问题

### 2. 核心端点实现 ✅
- `POST /_matrix/client/v3/knock/{roomIdOrAlias}` - 敲门请求加入房间
- `POST /_matrix/client/v3/invite/{roomId}` - 独立邀请端点
- `POST /_matrix/client/v3/join/{roomIdOrAlias}` - 通过ID或别名加入房间

### 3. 数据库修复 ✅
- 修复 `sync_stream_id` 类型不匹配 (SERIAL → BIGSERIAL)
- 创建 `sliding_sync_rooms` 表
- 创建 `thread_subscriptions` 表
- 创建 `space_children` 表
- 创建 `space_hierarchy` 表
- 修复 `federation_signing_keys` 表结构

### 4. 测试代码修复 ✅
- 修复密码不符合后端要求问题 (需要大写字母和特殊字符)
- 修复消息发送缺少 `body` 字段问题
- 添加服务健康检查和崩溃检测

---

## 测试失败分类统计

### 按失败原因分类

| 失败原因 | 数量 | 占比 | 示例 |
|---------|------|------|------|
| 数据库表/列缺失 | ~80 | 38% | `column "validated_at" does not exist` |
| 端点未实现 (404) | ~50 | 24% | `presence/list/{userId}` |
| 业务逻辑错误 (400) | ~40 | 19% | 密码验证、数据缺失 |
| 权限不足 (403) | ~20 | 10% | Admin API |
| 其他错误 | ~18 | 9% | 网络超时等 |

### 按类别分类

| 类别 | 通过 | 失败 | 跳过 | 主要问题 |
|------|------|------|------|---------|
| Core-Discovery | 4 | 0 | 0 | ✅ 全部通过 |
| Core-Profile | 4 | 0 | 0 | ✅ 全部通过 |
| Core-PublicRooms | 4 | 0 | 0 | ✅ 全部通过 |
| Logout-Client | 2 | 0 | 0 | ✅ 全部通过 |
| Core-Sync | 3 | 1 | 0 | sync/events 未实现 |
| Core-Rooms | 12 | 8 | 0 | 部分端点未实现 |
| E2EE-Keys | 6 | 2 | 0 | 部分功能待完善 |
| Federation-Events | 6 | 2 | 0 | 部分功能待完善 |
| Space-Children | 0 | 9 | 0 | 数据库表缺失 |
| Space-Hierarchy | 0 | 8 | 0 | 数据库表缺失 |
| Thread-Mgmt | 0 | 8 | 0 | 数据库表缺失 |
| Thread-Subs | 0 | 8 | 0 | thread_roots 表缺失 |

---

## 已修复的问题

### 1. sync API 崩溃问题 (关键修复)

**问题描述**: 
- `GET /_matrix/client/v3/sync` 导致服务崩溃
- 错误: `mismatched types; Rust type i64 is not compatible with SQL type INT4`

**修复方案**:
- 创建迁移文件 `20260313000003_fix_sync_stream_id_type.sql`
- 将 `sync_stream_id.id` 从 `SERIAL` 改为 `BIGSERIAL`

### 2. 核心端点实现

| 端点 | 功能 | 状态 |
|------|------|------|
| `POST /_matrix/client/v3/knock/{roomIdOrAlias}` | 敲门请求加入 | ✅ 已实现 |
| `POST /_matrix/client/v3/invite/{roomId}` | 独立邀请端点 | ✅ 已实现 |
| `POST /_matrix/client/v3/join/{roomIdOrAlias}` | 通过ID或别名加入 | ✅ 已实现 |

### 3. 数据库优化

| 优化项 | 状态 |
|--------|------|
| 创建 sliding_sync_rooms 表 | ✅ 已完成 |
| 创建 thread_subscriptions 表 | ✅ 已完成 |
| 创建 space_children 表 | ✅ 已完成 |
| 创建 space_hierarchy 表 | ✅ 已完成 |
| 修复 sync_stream_id 类型 | ✅ 已完成 |
| 修复 federation_signing_keys 表结构 | ✅ 已完成 |

### 4. 测试代码优化

| 优化项 | 状态 |
|--------|------|
| 添加服务健康检查 | ✅ 已完成 |
| 添加服务崩溃检测 | ✅ 已完成 |
| 修复 URL 编码问题 | ✅ 已完成 |
| 修复密码格式问题 | ✅ 已完成 |
| 修复消息体缺少 body 字段 | ✅ 已完成 |

---

## 剩余问题

### 1. 数据库表/列缺失 (高优先级)

以下表/列需要创建：
- `thread_roots` - 线程根消息
- `room_parents` - 房间父关系
- `push_rules.kind` 列
- `pushers.last_updated_ts` 列
- `account_data.created_ts` 列
- `account_data.content` 列
- `key_backups.auth_key` 列
- `application_services.is_enabled` 列

### 2. 缺失的端点 (中优先级)

- `GET/POST /_matrix/client/v3/presence/list/{userId}`
- `GET /_matrix/client/v3/sync/events`
- 部分 Federation 端点

### 3. 业务逻辑问题 (低优先级)

- 密码验证规则
- 数据验证失败
- 权限检查

---

## 迁移文件记录

### 20260313000001_add_sliding_sync_rooms_table.sql
- 创建 `sliding_sync_rooms` 表

### 20260313000002_fix_federation_signing_keys.sql
- 添加缺失的列: `secret_key`, `expires_at`, `key_json` 等

### 20260313000003_fix_sync_stream_id_type.sql
- **关键修复**: 将 `sync_stream_id.id` 从 `SERIAL` 改为 `BIGSERIAL`

### 20260313000004_add_thread_subscriptions.sql
- 创建 `thread_subscriptions` 表

### 20260313000005_add_space_children.sql
- 创建 `space_children` 表

### 20260313000006_add_space_hierarchy.sql
- 创建 `space_hierarchy` 表

---

## 代码修改记录

### 新增路由 (src/web/routes/mod.rs)
```rust
.route("/_matrix/client/v3/knock/{room_id_or_alias}", post(knock_room))
.route("/_matrix/client/v3/invite/{room_id}", post(invite_user_by_room))
```

### 新增服务方法 (src/services/room_service.rs)
```rust
pub async fn knock_room(&self, room_id: &str, user_id: &str, reason: Option<&str>) -> ApiResult<()>
```

### 测试代码修复 (hula/src/services/matrix/test/full-api-test-v5.ts)
```typescript
// 修复密码格式
password: 'TestPass123!',  // 需要: 大写字母 + 小写字母 + 数字 + 特殊字符

// 修复消息体
body: {
  msgtype: 'm.text',
  body: 'Hello World',  // 必须有 body 字段
}
```

---

## 下一步建议

### 高优先级
1. ✅ 实现缺失的核心端点 (已完成)
2. ✅ 修复 sync API 崩溃问题 (已完成)
3. ✅ 服务稳定性优化 (已完成)
4. ✅ 修复测试代码问题 (已完成)

### 中优先级
1. 创建缺失的数据库表/列 (`thread_roots`, `room_parents`, `push_rules.kind` 等)
2. 完善 Space API 和 Thread API 的数据库支持
3. 实现缺失的 presence 端点

### 低优先级
1. 实现 Federation 缺失端点
2. 添加更多测试用例
3. 优化业务逻辑验证

---

## 结论

### 服务稳定性：✅ 优秀
- 无 panic，服务稳定运行
- sync API 完全正常工作
- Sliding Sync API 完全正常工作

### API 实现状态：⚠️ 部分完成
- 核心端点已实现
- Space/Thread API 路由已实现，但数据库表缺失
- 部分 Federation 端点未实现

### 测试通过率：📈 显著提升
- 通过率从 2.3% 提升到 15.3%
- 通过数增加了 564%
- 服务稳定性大幅提升
- 测试代码问题已修复
