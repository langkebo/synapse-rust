# 数据库设计和迁移审查报告

**项目**: synapse-rust  
**审查日期**: 2026-03-22  
**审查范围**: 数据库 schema 设计、索引优化、迁移脚本、ORM 使用、连接池配置

---

## 1. 数据库 Schema 设计 (评分: ⭐⭐⭐⭐☆)

### 1.1 整体设计优点

- **结构完整**: 包含 60+ 张表，覆盖用户、房间、事件、加密、媒体、推送、认证等核心功能
- **命名规范**: 时间字段统一使用 `_ts` 后缀 (如 `created_ts`, `updated_ts`)，符合项目规范
- **主键策略**: 合理使用 `BIGSERIAL` 自增主键和 `TEXT` 主键 (如 `user_id`, `room_id`)
- **外键约束**: 关键关系都有外键约束并使用 `ON DELETE CASCADE`
- **JSONB 字段**: 正确使用 JSONB 存储灵活结构数据 (如 `content`, `data`, `signatures`)

### 1.2 发现的问题

#### 问题 1.1: 字段冗余 (中等)
```sql
-- rooms 表中已有 last_activity_ts，但 room_summaries 表又有 member_count 冗余
-- 注意: 注释已说明 member_count 冗余字段已移除，这是正确的改进
```

#### 问题 1.2: 部分表缺少 updated_ts (低)
```sql
-- room_summaries 表缺少 updated_ts 字段
-- room_directory 表缺少 updated_ts 字段
```

#### 问题 1.3: 字段类型不一致 (低)
```sql
-- password_changed_ts 使用 BIGINT，但某些地方可能期望 TIMESTAMP
-- 建议: 统一使用 BIGINT (毫秒时间戳) 或 TIMESTAMP
```

### 1.3 建议

1. 为 `room_summaries` 添加 `updated_ts` 字段以支持增量同步
2. 考虑为 `account_data` 表添加索引 `(user_id, updated_ts DESC)` 支持时间序查询
3. 统一所有时间相关字段的数据类型规范

---

## 2. 索引优化 (评分: ⭐⭐⭐⭐⭐)

### 2.1 现有索引分析

- **总计**: 183 个索引 (基础 schema) + 性能优化索引
- **主键**: 所有表都有主键索引
- **外键索引**: 关键外键字段已建立索引
- **复合索引**: 针对高频查询场景创建了复合索引

### 2.2 性能优化索引 (20260322000001)

```sql
-- 优秀: 范围查询优化
idx_events_room_time ON events(room_id, origin_server_ts DESC)
idx_events_sender_time ON events(sender, origin_server_ts DESC)

-- 优秀: 条件索引 (Partial Index)
idx_memberships_user_direct ON room_memberships(user_id, is_direct) WHERE is_direct = TRUE
idx_notifications_unread ON notifications(user_id, is_read, stream_ordering DESC) WHERE is_read = FALSE
```

### 2.3 发现的问题

#### 问题 2.1: 潜在缺失索引 (中等)
```sql
-- events 表的 is_redacted 查询缺少索引
-- 目前只有: idx_events_not_redacted (WHERE is_redacted = FALSE)
-- 建议: 添加 WHERE is_redacted = TRUE 的部分索引用于清理任务
```

#### 问题 2.2: 索引命名不一致 (低)
```sql
-- 有些用下划线: idx_events_room_id
-- 有些用驼峰: idxEventsSender
-- 建议: 统一命名规范
```

#### 问题 2.3: 未使用的索引风险
```sql
-- 建议定期运行 pg_stat_user_indexes 检查未使用的索引
-- 清理未使用索引可减少写入开销
```

### 2.4 建议

1. 添加 `idx_events_redacted_old` (WHERE is_redacted = TRUE AND redacted_at < NOW() - INTERVAL '90 days')
2. 每月审查 `pg_stat_user_indexes` 清理未使用索引
3. 考虑添加事件归档表的分区策略

---

## 3. 迁移脚本 (评分: ⭐⭐⭐⭐☆)

### 3.1 迁移文件结构

```
migrations/
├── 00000000_unified_schema_v6.sql    # 基础 Schema (2714 行)
├── 99999999_unified_incremental_migration.sql # 整合迁移
├── 20260321000001_fix_field_naming.sql
├── 20260321000002_add_missing_columns.sql
├── 20260321000003_fix_ephemeral.sql
├── 20260322000001_performance_indexes.sql
├── 20260322000002_performance_indexes_v2.sql
└── archive/
    └── schema_legacy.sql
```

### 3.2 优点

- ✅ 使用 `IF NOT EXISTS` 和 `IF NOT EXISTS` 保证幂等性
- ✅ 使用 `CONCURRENTLY` 创建索引避免锁表
- ✅ 整合迁移策略减少维护复杂度
- ✅ 每个迁移都有清晰的注释说明

### 3.3 发现的问题

#### 问题 3.1: 迁移顺序依赖 (中等)
```sql
-- 99999999_unified_incremental_migration.sql 整合了当前增量迁移
-- 但没有明确的依赖关系文档
-- 风险: 直接执行可能失败
```

#### 问题 3.2: 缺少回滚脚本 (中等)
```sql
-- 当前没有回滚脚本
-- 建议: 为每个破坏性变更添加回滚脚本或使用 pg_repack
```

#### 问题 3.3: 索引 v2 缺少说明 (低)
```sql
-- 20260322000002_performance_indexes_v2.sql 存在但内容未知
-- 建议: 添加文件头注释说明变更内容
```

### 3.4 建议

1. 添加迁移依赖关系图
2. 考虑使用 SQLx 的迁移框架自动管理版本
3. 为大表变更添加 `LOCK TIMEOUT` 保护
4. 添加迁移前数据校验 CHECK 语句

---

## 4. ORM 使用 (评分: ⭐⭐⭐⭐☆)

### 4.1 使用的技术栈

- **框架**: sqlx 0.8 (运行时 SQL 编译检查)
- **宏**: `sqlx::FromRow` 自动映射
- **连接池**: deadpool-postgres 0.12
- **查询构建**: 原生 SQL + sqlx 查询构建器

### 4.2 代码示例分析

```rust
// ✅ 正确: 使用 query_as 配合类型化返回
sqlx::query_as::<_, User>(
    "SELECT ... FROM users WHERE user_id = $1"
)
.bind(user_id)
.fetch_one(&*self.pool)
.await

// ✅ 正确: 使用事务
let mut tx = pool.begin().await?;
storage.create_user_tx(&mut tx, ...).await?;
tx.commit().await?
```

### 4.3 发现的问题

#### 问题 4.1: 重复 SQL 定义 (中等)
```rust
// user.rs 中同一 SELECT 语句出现多次
// 建议: 提取到常量或使用 sqlx::query! 宏
const USER_SELECT_COLUMNS: &str = "SELECT user_id, username, ...";
```

#### 问题 4.2: 缺少批量操作优化 (中等)
```rust
// 当前: 循环单条插入
for user in users {
    sqlx::query("INSERT INTO ...").execute(&pool).await;
}
// 建议: 使用 bulk insert 或 COPY
```

#### 问题 4.3: 错误处理不够细化 (低)
```rust
// 当前: 大多数函数返回 generic sqlx::Error
// 建议: 使用 thiserror 定义具体错误类型
```

### 4.4 建议

1. 提取重复 SQL 到常量或宏
2. 实现批量插入优化 (使用 `INSERT INTO ... VALUES (...), (...), (...)`)
3. 添加查询超时保护 (已有 query_utils.rs，需推广使用)
4. 考虑使用 SeaORM 或 ActiveRecord 模式简化 CRUD

---

## 5. 连接池配置 (评分: ⭐⭐⭐⭐⭐)

### 5.1 配置文件 (homeserver.yaml)

```yaml
database:
  pool_size: 10      # 当前连接数
  max_size: 20       # 最大连接数
  min_idle: 2        # 最小空闲连接
  connection_timeout: 30
```

### 5.2 代码中的配置 (connection_pool.rs)

```rust
// ✅ 提供多种环境配置
ConnectionPoolConfig::for_development()    // 20 连接
ConnectionPoolConfig::for_production()    // 100 连接
ConnectionPoolConfig::for_high_load()     // 200 连接
```

### 5.3 监控功能 (pool_monitor.rs)

- ✅ 健康检查 (`health_check`)
- ✅ 连接统计 (`get_pool_stats`)
- ✅ 自动健康检查任务
- ✅ 查询超时保护
- ✅ 重试机制与临时错误检测

### 5.4 发现的问题

#### 问题 5.1: 配置不一致 (低)
```yaml
# homeserver.yaml: max_size: 20
# connection_pool.rs: for_production() 默认 100
# 建议: 通过配置中心统一管理
```

#### 问题 5.2: 缺少连接泄漏检测 (低)
```rust
// 当前没有检测长时间占用的连接
// 建议: 添加 max_connection_time 监控
```

### 5.5 建议

1. 统一配置源 (环境变量或配置中心)
2. 添加连接池指标导出 (Prometheus)
3. 实现连接泄漏自动检测和告警
4. 考虑使用 PgBouncer 作为连接池代理

---

## 6. 总体评估

| 维度 | 评分 | 说明 |
|------|------|------|
| Schema 设计 | ⭐⭐⭐⭐☆ | 结构清晰，部分字段可优化 |
| 索引优化 | ⭐⭐⭐⭐⭐ | 覆盖全面，部分场景可增强 |
| 迁移脚本 | ⭐⭐⭐⭐☆ | 规范完整，缺少回滚 |
| ORM 使用 | ⭐⭐⭐⭐☆ | 正确使用，可进一步抽象 |
| 连接池 | ⭐⭐⭐⭐⭐ | 功能完善，配置灵活 |

### 关键优势

1. **完整的功能覆盖**: 60+ 表覆盖 Matrix 协议全部数据模型
2. **良好的命名规范**: 时间字段、命名约定统一
3. **性能意识**: 索引优化、查询超时、重试机制
4. **监控完善**: 连接池监控、性能指标、健康检查

### 优先改进项

1. **高优先级**: 添加迁移回滚脚本
2. **高优先级**: 统一配置管理 (连接池参数)
3. **中优先级**: 提取重复 SQL 到共享模块
4. **中优先级**: 添加批量操作优化
5. **低优先级**: 完善错误类型定义

---

## 附录: 统计数据

- Schema 文件行数: 2,714
- 索引数量: 183+
- 表数量: 60+
- 迁移文件数: 7
