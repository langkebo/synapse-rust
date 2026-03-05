# 数据库架构完整性与一致性审查报告（第二轮）

## 审查范围
- 迁移脚本：`migrations/`
- 运行时 SQL：`src/storage/*.rs`、`src/web/routes/*.rs`
- 输入依据：`.trae/specs/database-architecture-quality-review/*`、`/home/tzd/api-test/api-error.md`

## 严重（Severe）

### 1) 应用服务表结构与运行时写入不一致（已修复）
- 证据：`create_application_service` 使用 `sender_localpart` 写入，运行时模型使用 `sender`
- 影响：管理员创建应用服务端点返回 500，应用服务注册链路不可用
- 修复：
  - 管理端路由改为写入 `as_id/sender` 并补齐默认字段
  - 迁移中补齐 `application_services` 及关联表、统计视图
- 验证：
  - 编译通过
  - 需在开发环境复测 `POST /_synapse/admin/v1/application_services`

### 2) 后台更新锁表缺失（已修复）
- 证据：运行时访问 `background_update_locks`，但历史环境可能无该表
- 影响：后台更新清锁接口与任务调度链路失败
- 修复：迁移新增 `background_update_locks`、`background_update_history` 并补索引
- 验证：需复测 `POST /_synapse/admin/v1/background_updates/cleanup_locks`

## 中等（Medium）

### 1) 房间摘要成员缺失 `last_active_ts`（本轮修复）
- 证据：`room_summary_members` 在旧表场景下可能无该列，但运行时 `INSERT/SELECT/ORDER` 依赖
- 影响：`GET /_matrix/client/v3/rooms/{room_id}/summary/members` 可能 500
- 修复：新增增量迁移 `ALTER TABLE room_summary_members ADD COLUMN IF NOT EXISTS last_active_ts BIGINT`
- 验证：复测摘要成员查询并确认排序可用

### 2) 房间摘要统计缺失 `total_media`（本轮修复）
- 证据：运行时统计查询依赖 `total_media`，旧结构可能只有基础计数字段
- 影响：`GET /_matrix/client/v3/rooms/{room_id}/summary/stats` 可能 500
- 修复：
  - 新增 `total_media`
  - 同步补齐 `total_state_events/storage_size/last_updated_ts`
- 验证：复测摘要统计查询并检查字段完整性

### 3) 注册令牌缺失 `token_type`（本轮修复）
- 证据：运行时代码写入 `registration_tokens.token_type`，旧环境可能缺列
- 影响：`POST /_synapse/admin/v1/registration_tokens` 可能 500
- 修复：
  - 新增 `token_type`、数据回填、`NOT NULL + DEFAULT`
  - 补充类型索引
- 验证：复测注册令牌创建与筛选查询

## 轻微（Minor）

### 1) 审计字段更新时间不一致（本轮修复）
- 证据：`room_summary_members` 更新路径未同步刷新 `updated_ts`
- 影响：审计可追溯性与时间线一致性下降
- 修复：更新成员接口写入 `updated_ts = now`
- 验证：更新成员后检查 `updated_ts` 变化

## 修复落地文件
- `migrations/20260304000001_align_database_architecture.sql`
- `migrations/20260304000002_fix_medium_priority_schema_gaps.sql`
- `src/web/routes/admin.rs`
- `src/storage/room_summary.rs`

## Checklist 对齐结论
- A. 审查准备：已完成
- B. 架构完整性：严重项已修复，待环境复测确认
- C. 字段规范性：中等项已补齐迁移
- D. 索引质量：新增关键索引，待慢查询复核
- E. 设计缺陷：已补充审计字段更新逻辑
- F. 报告输出：已按严重/中等/轻微分级
- G. 实施与验证：代码编译通过；接口回归待执行
