# Tasks

## 第一阶段：测试脚本修复（已完成）

- [x] Task 1.1: CURRENT_TEST_PASS 修复 - 用户已添加 `CURRENT_TEST_PASS="$TEST_PASS"`
- [x] Task 1.2: Admin Get Room 缩进修复 - 已移入 admin_ready 块内
- [x] Task 1.3: Presence Bulk 订阅用户修复 - 改为订阅 `@testuser1:cjystx.top`

## 第二阶段：后端代码修复（已完成）

### 2.1 Room 与测试房间链路问题
- [x] 检查 room_storage.create_room() 是否被调用并成功
- [x] 检查 rooms 表是否有对应记录
- [x] 修复事务提交问题
- [x] 修复房间删除后仍复用旧 ROOM_ID 的测试脚本问题

**影响测试**: Admin Get Room, Get Room Version

### 2.2 Token revoke 无效问题
- [x] 检查 invalidate session 后 is_revoked 标志是否更新
- [x] 检查 re-login 时是否正确重置标志
- [x] 修复 access_tokens 表更新逻辑
- [x] 修复测试账号被 deactivated 后未恢复导致的后续 401

**影响测试**: Get Presence Bulk

### 2.3 其他后端问题
- [x] Get Device - "Missing field: device" - 检查 API 响应格式
- [x] Admin Room Make Admin - HTTP 405 - 检查 API 方法
- [x] Create Widget - HTTP 500 - 检查后端错误
- [x] 修复 Space 存储字段映射与子房间查询兼容问题
- [x] 修复 Event Reports resolved_at / resolved_ts 映射
- [x] 修复 Media Quota 聚合类型与隔离状态解码
- [x] 修复 Device Verification Respond 时间戳写入类型
- [x] 修复 API 集成脚本数据库容器与库名硬编码

## 第三阶段：测试结果验证

- [x] Task 3.1: 运行完整测试套件
- [x] Task 3.2: 确认通过率 > 99%
- [x] Task 3.3: 确认失败数 < 3
- [ ] Task 3.4: 确认跳过数 < 50

## 关键失败项收敛结果

| 测试 | 错误 | 类别 | 状态 |
|------|------|------|------|
| Get Device | Missing field: device | 后端 | 已验证通过 |
| Server Key Query | federation signing key | 预期行为 | 跳过 |
| Admin Room Make Admin | HTTP 405 | 后端 | 已验证通过 |
| Admin Get Room | HTTP 404 | 测试脚本 | 已验证通过 |
| Get Presence Bulk | HTTP 401 | 后端 | 已验证通过 |
| Get Room Version | HTTP 404 | 测试脚本 | 已验证通过 |
| Create Widget | HTTP 500 | 后端/测试脚本 | 已验证通过 |
| Create Space | HTTP 500 | 后端 | 已验证通过 |
| List Event Reports | HTTP 500 | 后端 | 已验证通过 |
| Get Media Quota | HTTP 500 | 后端 | 已验证通过 |
| Device Verification Respond | HTTP 500 | 后端 | 已验证通过 |
| Admin Room Member Add | HTTP 500 | 测试脚本/后端 | 已修复，待环境回归 |
| Admin Room Ban User | HTTP 404 | 测试脚本/兼容性 | 已修复，待环境回归 |
| Admin Room Kick User | HTTP 404 | 测试脚本/兼容性 | 已修复，待环境回归 |
| Get Room Hierarchy | HTTP 500 | 后端 | 已修复，待环境回归 |

## 跳过测试分类

| 类别 | 数量 | 处理 |
|------|------|------|
| Federation 需要签名 | ~33 | 预期行为，跳过 |
| destructive test | ~1 | 预期行为，跳过 |
| 端点未实现 / not found | ~120+ | 待后续分类收敛 |
| 代表性接口返回 404/405/500 | ~10+ | 已记录为下一轮跟踪项 |

# Task Dependencies

- Task 2.1, 2.2, 2.3 可并行处理
- Task 3 依赖 Task 2 完成后

## 当前阻塞

- 跳过数仍为 169，距离目标 `< 50` 仍有明显差距
- 当前完整回归已通过（413 passed, 0 failed），后续重点转为收敛未实现接口与预期跳过分类
- 本轮新增验证已完成 `cargo test --no-run` 与脚本语法检查，但集成测试环境仍存在 `synapse_test` 数据库缺失，需先修复测试数据库配置后再做定向回归
