# Tasks

## Phase 1: Space 模块排查与优化 (21 个端点)

- [x] Task 1.1: 检查 Space 模块所有端点实现
  - [x] SubTask 1.1.1: 列出所有 Space 端点（21 个）
  - [x] SubTask 1.1.2: 检查每个端点的路由实现
  - [x] SubTask 1.1.3: 验证端点对应的处理函数

- [x] Task 1.2: 修复数据库表结构不匹配问题
  - [x] SubTask 1.2.1: 修复 `join_rule` vs `join_rules` 不匹配
  - [x] SubTask 1.2.2: 处理 `visibility` 字段缺失问题
  - [x] SubTask 1.2.3: 验证所有 SQL 查询使用正确的列名

- [x] Task 1.3: 验证 Space 相关数据库表
  - [x] SubTask 1.3.1: 检查 `spaces` 表结构
  - [x] SubTask 1.3.2: 检查 `space_children` 表结构
  - [x] SubTask 1.3.3: 创建 `space_members` 表（缺失）
  - [x] SubTask 1.3.4: 检查 `space_events` 表结构

- [x] Task 1.4: 编写 Space 模块 API 测试
  - [x] SubTask 1.4.1: 创建 Space API 测试脚本
  - [ ] SubTask 1.4.2: 测试所有 21 个端点
  - [ ] SubTask 1.4.3: 记录测试结果

- [ ] Task 1.5: 修复发现的问题并重新测试
  - [ ] SubTask 1.5.1: 修复所有测试失败的问题
  - [ ] SubTask 1.5.2: 确保所有测试通过
  - [ ] SubTask 1.5.3: 更新 `api-complete.md` 中的 Space 模块

## Phase 2: Media 模块排查与优化 (21 个端点)

- [ ] Task 2.1: 检查 Media 模块所有端点实现
  - [ ] SubTask 2.1.1: 列出所有 Media 端点（21 个）
  - [ ] SubTask 2.1.2: 检查每个端点的路由实现
  - [ ] SubTask 2.1.3: 验证端点对应的处理函数

- [ ] Task 2.2: 验证 Media 相关数据库表
  - [ ] SubTask 2.2.1: 检查 `media` 表结构
  - [ ] SubTask 2.2.2: 检查 `media_quota_config` 表结构
  - [ ] SubTask 2.2.3: 检查 `user_media_quota` 表结构
  - [ ] SubTask 2.2.4: 检查 `media_usage_log` 表结构
  - [ ] SubTask 2.2.5: 检查 `media_quota_alerts` 表结构
  - [ ] SubTask 2.2.6: 检查 `server_media_quota` 表结构

- [ ] Task 2.3: 编写 Media 模块 API 测试
  - [ ] SubTask 2.3.1: 创建 Media API 测试脚本
  - [ ] SubTask 2.3.2: 测试所有 21 个端点
  - [ ] SubTask 2.3.3: 记录测试结果

- [ ] Task 2.4: 修复发现的问题并重新测试
  - [ ] SubTask 2.4.1: 修复所有测试失败的问题
  - [ ] SubTask 2.4.2: 确保所有测试通过
  - [ ] SubTask 2.4.3: 更新 `api-complete.md` 中的 Media 模块

## Phase 3: Device 模块排查与优化 (8 个端点)

- [ ] Task 3.1: 检查 Device 模块所有端点实现
  - [ ] SubTask 3.1.1: 列出所有 Device 端点（8 个）
  - [ ] SubTask 3.1.2: 检查每个端点的路由实现
  - [ ] SubTask 3.1.3: 验证端点对应的处理函数

- [ ] Task 3.2: 验证 Device 相关数据库表
  - [ ] SubTask 3.2.1: 检查 `devices` 表结构
  - [ ] SubTask 3.2.2: 检查 `device_keys` 表结构

- [ ] Task 3.3: 编写 Device 模块 API 测试
  - [ ] SubTask 3.3.1: 创建 Device API 测试脚本
  - [ ] SubTask 3.3.2: 测试所有 8 个端点
  - [ ] SubTask 3.3.3: 记录测试结果

- [ ] Task 3.4: 修复发现的问题并重新测试
  - [ ] SubTask 3.4.1: 修复所有测试失败的问题
  - [ ] SubTask 3.4.2: 确保所有测试通过
  - [ ] SubTask 3.4.3: 更新 `api-complete.md` 中的 Device 模块

## Phase 4: E2EE Routes 模块排查与优化 (27 个端点)

- [ ] Task 4.1: 检查 E2EE Routes 模块所有端点实现
  - [ ] SubTask 4.1.1: 列出所有 E2EE Routes 端点（27 个）
  - [ ] SubTask 4.1.2: 检查每个端点的路由实现
  - [ ] SubTask 4.1.3: 验证端点对应的处理函数

- [ ] Task 4.2: 验证 E2EE 相关数据库表
  - [ ] SubTask 4.2.1: 检查 `device_keys` 表结构
  - [ ] SubTask 4.2.2: 检查 `one_time_keys` 表结构
  - [ ] SubTask 4.2.3: 检查 `key_backups` 表结构
  - [ ] SubTask 4.2.4: 检查 `cross_signing_keys` 表结构

- [ ] Task 4.3: 编写 E2EE Routes 模块 API 测试
  - [ ] SubTask 4.3.1: 创建 E2EE API 测试脚本
  - [ ] SubTask 4.3.2: 测试所有 27 个端点
  - [ ] SubTask 4.3.3: 记录测试结果

- [ ] Task 4.4: 修复发现的问题并重新测试
  - [ ] SubTask 4.4.1: 修复所有测试失败的问题
  - [ ] SubTask 4.4.2: 确保所有测试通过
  - [ ] SubTask 4.4.3: 更新 `api-complete.md` 中的 E2EE Routes 模块

## Phase 5: Search 模块排查与优化 (12 个端点)

- [ ] Task 5.1: 检查 Search 模块所有端点实现
  - [ ] SubTask 5.1.1: 列出所有 Search 端点（12 个）
  - [ ] SubTask 5.1.2: 检查每个端点的路由实现
  - [ ] SubTask 5.1.3: 验证端点对应的处理函数

- [ ] Task 5.2: 验证 Search 相关数据库表
  - [ ] SubTask 5.2.1: 检查 `search_index` 表结构
  - [ ] SubTask 5.2.2: 检查 `search_results` 表结构

- [ ] Task 5.3: 编写 Search 模块 API 测试
  - [ ] SubTask 5.3.1: 创建 Search API 测试脚本
  - [ ] SubTask 5.3.2: 测试所有 12 个端点
  - [ ] SubTask 5.3.3: 记录测试结果

- [ ] Task 5.4: 修复发现的问题并重新测试
  - [ ] SubTask 5.4.1: 修复所有测试失败的问题
  - [ ] SubTask 5.4.2: 确保所有测试通过
  - [ ] SubTask 5.4.3: 更新 `api-complete.md` 中的 Search 模块

## Phase 6: Account Data 模块排查与优化 (12 个端点)

- [ ] Task 6.1: 检查 Account Data 模块所有端点实现
  - [ ] SubTask 6.1.1: 列出所有 Account Data 端点（12 个）
  - [ ] SubTask 6.1.2: 检查每个端点的路由实现
  - [ ] SubTask 6.1.3: 验证端点对应的处理函数

- [ ] Task 6.2: 验证 Account Data 相关数据库表
  - [ ] SubTask 6.2.1: 检查 `account_data` 表结构
  - [ ] SubTask 6.2.2: 检查 `room_account_data` 表结构

- [ ] Task 6.3: 编写 Account Data 模块 API 测试
  - [ ] SubTask 6.3.1: 创建 Account Data API 测试脚本
  - [ ] SubTask 6.3.2: 测试所有 12 个端点
  - [ ] SubTask 6.3.3: 记录测试结果

- [ ] Task 6.4: 修复发现的问题并重新测试
  - [ ] SubTask 6.4.1: 修复所有测试失败的问题
  - [ ] SubTask 6.4.2: 确保所有测试通过
  - [ ] SubTask 6.4.3: 更新 `api-complete.md` 中的 Account Data 模块

## Phase 7: Thread 模块排查与优化 (16 个端点)

- [ ] Task 7.1: 检查 Thread 模块所有端点实现
  - [ ] SubTask 7.1.1: 列出所有 Thread 端点（16 个）
  - [ ] SubTask 7.1.2: 检查每个端点的路由实现
  - [ ] SubTask 7.1.3: 验证端点对应的处理函数

- [ ] Task 7.2: 验证 Thread 相关数据库表
  - [ ] SubTask 7.2.1: 检查 `threads` 表结构
  - [ ] SubTask 7.2.2: 检查 `thread_events` 表结构

- [ ] Task 7.3: 编写 Thread 模块 API 测试
  - [ ] SubTask 7.3.1: 创建 Thread API 测试脚本
  - [ ] SubTask 7.3.2: 测试所有 16 个端点
  - [ ] SubTask 7.3.3: 记录测试结果

- [ ] Task 7.4: 修复发现的问题并重新测试
  - [ ] SubTask 7.4.1: 修复所有测试失败的问题
  - [ ] SubTask 7.4.2: 确保所有测试通过
  - [ ] SubTask 7.4.3: 更新 `api-complete.md` 中的 Thread 模块

## Phase 8: Room Summary 模块排查与优化 (16 个端点)

- [ ] Task 8.1: 检查 Room Summary 模块所有端点实现
  - [ ] SubTask 8.1.1: 列出所有 Room Summary 端点（16 个）
  - [ ] SubTask 8.1.2: 检查每个端点的路由实现
  - [ ] SubTask 8.1.3: 验证端点对应的处理函数

- [ ] Task 8.2: 验证 Room Summary 相关数据库表
  - [ ] SubTask 8.2.1: 检查 `room_summaries` 表结构
  - [ ] SubTask 8.2.2: 检查 `room_summary_updates` 表结构

- [ ] Task 8.3: 编写 Room Summary 模块 API 测试
  - [ ] SubTask 8.3.1: 创建 Room Summary API 测试脚本
  - [ ] SubTask 8.3.2: 测试所有 16 个端点
  - [ ] SubTask 8.3.3: 记录测试结果

- [ ] Task 8.4: 修复发现的问题并重新测试
  - [ ] SubTask 8.4.1: 修复所有测试失败的问题
  - [ ] SubTask 8.4.2: 确保所有测试通过
  - [ ] SubTask 8.4.3: 更新 `api-complete.md` 中的 Room Summary 模块

## Phase 9: Push 模块排查与优化 (18 个端点)

- [ ] Task 9.1: 检查 Push 模块所有端点实现
  - [ ] SubTask 9.1.1: 列出所有 Push 端点（18 个）
  - [ ] SubTask 9.1.2: 检查每个端点的路由实现
  - [ ] SubTask 9.1.3: 验证端点对应的处理函数

- [ ] Task 9.2: 验证 Push 相关数据库表
  - [ ] SubTask 9.2.1: 检查 `pushers` 表结构
  - [ ] SubTask 9.2.2: 检查 `push_rules` 表结构
  - [ ] SubTask 9.2.3: 检查 `notifications` 表结构

- [ ] Task 9.3: 编写 Push 模块 API 测试
  - [ ] SubTask 9.3.1: 创建 Push API 测试脚本
  - [ ] SubTask 9.3.2: 测试所有 18 个端点
  - [ ] SubTask 9.3.3: 记录测试结果

- [ ] Task 9.4: 修复发现的问题并重新测试
  - [ ] SubTask 9.4.1: 修复所有测试失败的问题
  - [ ] SubTask 9.4.2: 确保所有测试通过
  - [ ] SubTask 9.4.3: 更新 `api-complete.md` 中的 Push 模块

## Phase 10: Voice 模块排查与优化 (10 个端点)

- [ ] Task 10.1: 检查 Voice 模块所有端点实现
  - [ ] SubTask 10.1.1: 列出所有 Voice 端点（10 个）
  - [ ] SubTask 10.1.2: 检查每个端点的路由实现
  - [ ] SubTask 10.1.3: 验证端点对应的处理函数

- [ ] Task 10.2: 验证 Voice 相关数据库表
  - [ ] SubTask 10.2.1: 检查 `voice_messages` 表结构
  - [ ] SubTask 10.2.2: 检查 `voice_stats` 表结构

- [ ] Task 10.3: 编写 Voice 模块 API 测试
  - [ ] SubTask 10.3.1: 创建 Voice API 测试脚本
  - [ ] SubTask 10.3.2: 测试所有 10 个端点
  - [ ] SubTask 10.3.3: 记录测试结果

- [ ] Task 10.4: 修复发现的问题并重新测试
  - [ ] SubTask 10.4.1: 修复所有测试失败的问题
  - [ ] SubTask 10.4.2: 确保所有测试通过
  - [ ] SubTask 10.4.3: 更新 `api-complete.md` 中的 Voice 模块

# Task Dependencies
- [Task 1.2] depends on [Task 1.1]
- [Task 1.3] depends on [Task 1.2]
- [Task 1.4] depends on [Task 1.3]
- [Task 1.5] depends on [Task 1.4]
- [Task 2.2] depends on [Task 2.1]
- [Task 2.3] depends on [Task 2.2]
- [Task 2.4] depends on [Task 2.3]
- [Task 3.2] depends on [Task 3.1]
- [Task 3.3] depends on [Task 3.2]
- [Task 3.4] depends on [Task 3.3]
- [Task 4.2] depends on [Task 4.1]
- [Task 4.3] depends on [Task 4.2]
- [Task 4.4] depends on [Task 4.3]
- [Task 5.2] depends on [Task 5.1]
- [Task 5.3] depends on [Task 5.2]
- [Task 5.4] depends on [Task 5.3]
- [Task 6.2] depends on [Task 6.1]
- [Task 6.3] depends on [Task 6.2]
- [Task 6.4] depends on [Task 6.3]
- [Task 7.2] depends on [Task 7.1]
- [Task 7.3] depends on [Task 7.2]
- [Task 7.4] depends on [Task 7.3]
- [Task 8.2] depends on [Task 8.1]
- [Task 8.3] depends on [Task 8.2]
- [Task 8.4] depends on [Task 8.3]
- [Task 9.2] depends on [Task 9.1]
- [Task 9.3] depends on [Task 9.2]
- [Task 9.4] depends on [Task 9.3]
- [Task 10.2] depends on [Task 10.1]
- [Task 10.3] depends on [Task 10.2]
- [Task 10.4] depends on [Task 10.3]
