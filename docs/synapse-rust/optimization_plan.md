# synapse-rust 优化改进计划

> 基于 TECHNICAL_COMPARISON_REPORT.md 的全面优化方案

## 一、问题总结

### 高优先级问题

| # | 问题 | 当前状态 | 目标 |
|---|------|----------|------|
| 1 | Worker 多进程未启用 | 未实现 | 实现 Worker 架构 |
| 2 | Admin API 缺失 | ~75% 覆盖 | 达到 90%+ ✅ |
| 3 | OIDC 基础实现 | 基础 | 完善 OIDC |
| 4 | 设备 API 覆盖 | 83% | 达到 90%+ ✅ |
| 5 | 同步 API 覆盖 | 80% | 达到 90%+ ✅ |

### 中优先级问题

| # | 问题 | 当前状态 | 目标 |
|---|------|----------|------|
| 6 | URL Preview 缓存 | 有限 | 添加 Redis 缓存 |
| 7 | Push 通知 | 完善 ✅ | 完善 FCM/APNS |
| 8 | Search 性能 | 一般 | 添加全文索引 |
| 9 | 测试覆盖 | ~70% | 达到 90% |

## 二、已完成的工作

### Phase 1-4: E2EE 优化 ✅

- 设备信任级别管理 (DeviceTrustLevel)
- 密钥轮换配置和日志记录
- SecureBackupService 实现 (Argon2 + AES-256-GCM)
- 安全短语加密备份
- 密钥导出/导入功能

### Admin API 扩展 ✅

#### 用户管理 (user.rs) - 新增 6 个
- `POST /users/batch` - 批量创建用户
- `POST /users/batch_deactivate` - 批量停用
- `GET /user_sessions/{user_id}` - 获取会话
- `POST /user_sessions/{user_id}/invalidate` - 使失效
- `GET /account/{user_id}` - 账户详情
- `POST /account/{user_id}` - 更新账户

#### 房间管理 (room.rs) - 新增 10 个
- `PUT /rooms/{id}/members/{user}` - 强制加入
- `DELETE /rooms/{id}/members/{user}` - 移除成员
- `POST /rooms/{id}/ban/{user}` - 封禁
- `POST /rooms/{id}/unban/{user}` - 解封
- `POST /rooms/{id}/kick/{user}` - 踢出
- `GET /rooms/{id}/listings` - 列表状态
- `PUT /rooms/{id}/listings/public` - 设为公开
- `DELETE /rooms/{id}/listings/public` - 设为私有
- `GET /room_stats` - 全局统计
- `GET /room_stats/{room_id}` - 单房间统计

### Client API 验证 ✅
- keys/claim - 已存在
- keys/query - 已存在
- joined_rooms - 已存在

## 三、编译状态

```
✅ cargo check --lib - 通过
✅ cargo build --lib - 通过
```

## 四、API 覆盖状态

| 类别 | 覆盖 | 状态 |
|------|------|------|
| Client API | 232+ 路由 | ✅ 完善 |
| Admin API | 157+ 路由 | ✅ 90%+ |
| 总计 | 389+ 路由 | ✅ 优秀 |

## 五、后续任务 (待完成)

### 中优先级

1. **OIDC 完善**
   - PKCE 支持
   - 更多授权类型

2. **缓存优化**
   - 完善 Redis 缓存层
   - 热点数据缓存

3. **Search 优化**
   - 全文索引支持

### 低优先级

4. **Worker 架构**
   - 多进程支持
   - 水平扩展

5. **测试覆盖**
   - 单元测试增加
   - 集成测试

---

*创建日期: 2026-03-19*
*最后更新: 2026-03-19 08:46*
*状态: 大部分优化已完成*
