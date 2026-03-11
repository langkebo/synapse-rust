# 数据库统一重构检查清单

## 项目信息
- **项目名称**: synapse-rust
- **数据库类型**: PostgreSQL 16
- **ORM框架**: SQLx
- **最后更新**: 2026-03-10

## Phase 1: 数据库架构审计（已完成）

- [x] Task 1: 数据库表结构审计
  - [x] SubTask 1.1: 审计所有核心表结构
  - [x] SubTask 1.2: 审计所有索引定义
  - [x] SubTask 1.3: 审计所有外键约束
  - [x] SubTask 1.4: 审计所有触发器

- [x] Task 2: 字段命名规范审计
  - [x] SubTask 2.1: 时间戳字段命名审计（_ts vs _at）
  - [x] SubTask 2.2: 布尔字段命名审计（is_/has_ 前缀）
  - [x] SubTask 2.3: ID字段命名审计（*_id 格式）
  - [x] SubTask 2.4: 外键字段命名审计

## Phase 2: 数据一致性问题修复（已完成）

- [x] Task 3: 时间戳字段后缀统一
  - [x] SubTask 3.1: users 表 updated_at → updated_ts
  - [x] SubTask 3.2: rooms 表 last_activity_at → last_activity_ts
  - [x] SubTask 3.3: room_memberships 表 updated_at → updated_ts
  - [x] SubTask 3.4: 批量修复其他表时间戳字段

- [x] Task 4: 类型不匹配修复
  - [x] SubTask 4.1: rooms.member_count INTEGER → BIGINT
  - [x] SubTask 4.2: room_summaries.member_count INTEGER → BIGINT

- [x] Task 5: 缺失字段补充
  - [x] SubTask 5.1: user_threepids.validated_at 添加
  - [x] SubTask 5.2: device_keys.ts_updated_ms 添加
  - [x] SubTask 5.3: device_keys.ts_added_ms 添加
  - [x] SubTask 5.4: key_backups.backup_id 添加
  - [x] SubTask 5.5: key_backups.auth_key 添加
  - [x] SubTask 5.6: key_backups.mgmt_key 添加
  - [x] SubTask 5.7: key_backups.backup_data 添加
  - [x] SubTask 5.8: key_backups.etag 添加

## Phase 3: 缺失表创建（已完成）

- [x] Task 6: 创建缺失的数据库表
  - [x] SubTask 6.1: 创建 search_index 表
  - [x] SubTask 6.2: 创建 spaces 表
  - [x] SubTask 6.3: 创建 backup_keys 表
  - [x] SubTask 6.4: 创建 space_summaries 表
  - [x] SubTask 6.5: 创建 space_statistics 表

## Phase 4: 代码层修复（已完成）

- [x] Task 7: 修复 Rust 模型定义
  - [x] SubTask 7.1: 修复 Room.member_count 类型 (i32 → i64)
  - [x] SubTask 7.2: 修复 User 时间戳字段命名
  - [x] SubTask 7.3: 修复 RoomMembership 时间戳字段命名

- [x] Task 8: 修复 SQL 查询
  - [x] SubTask 8.1: 修复 room.rs 中的 last_activity_ts 引用
  - [x] SubTask 8.2: 修复 user.rs 中的时间戳字段引用
  - [x] SubTask 8.3: 修复 membership.rs 中的时间戳字段引用

## Phase 5: E2EE 模块修复（已完成）

- [x] Task 9: 修复 E2EE 模块字段映射
  - [x] SubTask 9.1: 修复 KeyBackup 模型字段
  - [x] SubTask 9.2: 修复 MegolmSession 时间戳类型转换
  - [x] SubTask 9.3: 修复 device_keys 存储层查询

## Phase 6: 索引优化（已完成）

- [x] Task 10: 创建缺失索引
  - [x] SubTask 10.1: search_index 表 GIN 索引
  - [x] SubTask 10.2: spaces 表索引
  - [x] SubTask 10.3: backup_keys 表索引
  - [x] SubTask 10.4: space_summaries 表索引

## Phase 7: 安全配置检查（已完成）

- [x] Task 11: 数据库安全审计
  - [x] SubTask 11.1: 检查用户权限配置
  - [x] SubTask 11.2: 检查连接池配置
  - [x] SubTask 11.3: 检查 SSL/TLS 配置
  - [x] SubTask 11.4: 检查敏感数据加密

## Phase 8: API 测试验证（进行中）

- [x] Task 12: 基础 API 测试
  - [x] SubTask 12.1: 健康检查 API
  - [x] SubTask 12.2: 版本信息 API
  - [x] SubTask 12.3: 客户端能力 API

- [x] Task 13: 用户认证 API 测试
  - [x] SubTask 13.1: 登录 API
  - [x] SubTask 13.2: 注册 API
  - [x] SubTask 13.3: whoami API

- [x] Task 14: 房间管理 API 测试
  - [x] SubTask 14.1: 创建房间 API
  - [x] SubTask 14.2: 公开房间列表 API
  - [x] SubTask 14.3: 房间成员 API
  - [x] SubTask 14.4: 加入/离开房间 API

- [x] Task 15: 消息 API 测试
  - [x] SubTask 15.1: 发送消息 API
  - [x] SubTask 15.2: 获取消息 API

- [x] Task 16: E2EE API 测试
  - [x] SubTask 16.1: 密钥上传 API
  - [x] SubTask 16.2: 密钥查询 API
  - [x] SubTask 16.3: 密钥备份 API

- [ ] Task 17: 搜索和空间 API 测试
  - [x] SubTask 17.1: 搜索 API（需要进一步修复）
  - [x] SubTask 17.2: Space API（需要进一步修复）

## Phase 9: 文档更新（进行中）

- [x] Task 18: 更新规范文档
  - [x] SubTask 18.1: 更新 checklist.md
  - [x] SubTask 18.2: 更新 spec.md
  - [x] SubTask 18.3: 更新 tasks.md

- [x] Task 19: 更新 API 错误文档
  - [x] SubTask 19.1: 更新 api-error.md 测试结果

## 问题追踪

### 已修复的问题

| 问题ID | 问题描述 | 修复方案 | 状态 |
|--------|----------|----------|------|
| DB-001 | member_count 类型不匹配 | INTEGER → BIGINT | ✅ 已修复 |
| DB-002 | validated_at 字段缺失 | 添加字段 | ✅ 已修复 |
| DB-003 | ts_updated_ms 字段缺失 | 添加字段 | ✅ 已修复 |
| DB-004 | backup_id 字段缺失 | 添加字段 | ✅ 已修复 |
| DB-005 | spaces 表不存在 | 创建表 | ✅ 已修复 |
| DB-006 | search_index 表不存在 | 创建表 | ✅ 已修复 |
| DB-007 | 时间戳字段后缀不一致 | 统一为 _ts | ✅ 已修复 |

### 待修复的问题

| 问题ID | 问题描述 | 优先级 | 状态 |
|--------|----------|--------|------|
| DB-008 | 搜索 API type 字段问题 | P1 | ⏳ 待修复 |
| DB-009 | Space API room_id 字段问题 | P1 | ⏳ 待修复 |
| DB-010 | 管理员账户密码哈希格式 | P1 | ⏳ 待修复 |

## 迁移文件清单

| 文件名 | 创建日期 | 状态 |
|--------|----------|------|
| 00000000_unified_schema_v6.sql | 2026-03-01 | ✅ 已应用 |
| 20260309000001_password_security_enhancement.sql | 2026-03-09 | ✅ 已应用 |
| 20260310000001_add_missing_e2ee_tables.sql | 2026-03-10 | ✅ 已应用 |
| 20260310000002_normalize_fields_and_add_tables.sql | 2026-03-10 | ✅ 已应用 |
| 20260310000003_fix_api_test_issues.sql | 2026-03-10 | ✅ 已应用 |

## 测试覆盖率

| API模块 | 端点数 | 测试通过 | 覆盖率 |
|---------|--------|----------|--------|
| 基础服务 | 8 | 8 | 100% |
| 用户认证 | 5 | 5 | 100% |
| 账户管理 | 5 | 5 | 100% |
| 房间管理 | 12 | 12 | 100% |
| 消息发送 | 4 | 4 | 100% |
| 设备管理 | 2 | 2 | 100% |
| 推送通知 | 3 | 3 | 100% |
| E2EE加密 | 2 | 2 | 100% |
| 媒体服务 | 1 | 1 | 100% |
| 好友系统 | 1 | 1 | 100% |
| 同步功能 | 1 | 1 | 100% |
| VoIP服务 | 2 | 2 | 100% |
| 搜索服务 | 2 | 0 | 0% |
| 管理后台 | 4 | 0* | N/A |
| 联邦API | 2 | 2 | 100% |
| **总计** | **54** | **48** | **88.9%** |

*注：管理后台API需要管理员权限

## 下一步行动

1. 修复搜索 API type 字段问题
2. 修复 Space API room_id 字段问题
3. 注册管理员账户
4. 完成剩余 API 测试验证
