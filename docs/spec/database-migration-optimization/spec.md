# 数据库初始化脚本与迁移脚本优化规范

## 1. 项目概述

本规范针对 `synapse-rust` 项目中的数据库初始化脚本与迁移脚本进行全面系统性分析和优化。

### 1.1 分析范围

- `/home/hula/synapse_rust/synapse/src/services/database_initializer.rs` - 数据库初始化服务
- `/home/hula/synapse_rust/synapse/schema.sql` - 基础 Schema 定义
- `/home/hula/synapse_rust/synapse/migrations/*.sql` - 28 个迁移文件

### 1.2 当前状态

- 迁移文件数量: 28 个
- 主 Schema 文件: `20260206000000_master_unified_schema.sql` (约 1000 行)
- 性能索引文件: `20260209100000_add_performance_indexes.sql` (约 300 行)
- 初始化服务代码: 约 650 行 Rust 代码

---

## 2. 问题识别与分析

### 2.1 SQL 语句分割器问题 (严重)

**问题描述:**
`split_sql_statements` 函数在处理 SQL 语句时存在多个边界情况问题：

```rust
// 当前实现存在的问题:
// 1. 块注释处理不完整，特别是文件末尾的注释
// 2. 引号转义处理不正确
// 3. Dollar-quoted 字符串处理有边界问题
```

**影响:**
- 迁移文件 `20260211000003_cleanup_legacy_friends.sql` 执行失败
- 错误信息: `unterminated /* comment`

**根因分析:**
- 块注释处理逻辑在文件末尾时无法正确终止
- 注释跳过后 `continue` 语句导致索引递增逻辑混乱

### 2.2 Schema 定义冲突 (严重)

**问题描述:**
`schema.sql` 和 `20260206000000_master_unified_schema.sql` 存在大量冲突定义：

| 表名 | schema.sql | master_unified_schema.sql | 冲突类型 |
|------|------------|---------------------------|----------|
| users | `admin BOOLEAN` | `is_admin BOOLEAN` | 列名不同 |
| devices | `device_id TEXT PRIMARY KEY` | `PRIMARY KEY (device_id, user_id)` | 主键定义不同 |
| access_tokens | 无 `invalidated` 列 | 有 `invalidated` 列 | 列缺失 |
| refresh_tokens | 无 `invalidated` 列 | 有 `invalidated` 列 | 列缺失 |
| friends | 有 `id BIGSERIAL` | 无 `id` 列，使用复合主键 | 主键定义不同 |

**影响:**
- 索引创建失败: `column "invalidated" does not exist`
- 外键约束失败
- 数据插入失败

### 2.3 迁移文件依赖问题 (中等)

**问题描述:**
迁移文件之间存在隐式依赖，但缺乏显式声明：

```
20260209100000_add_performance_indexes.sql
├── 依赖 access_tokens.invalidated 列 (不存在)
├── 依赖 voice_messages.sender_id 列 (不存在)
└── 依赖 synapse_performance_stats 表 (不存在)

20260211000003_cleanup_legacy_friends.sql
├── 依赖 friends 表存在
├── 依赖 friend_requests 表存在
└── 与 master_unified_schema.sql 冲突
```

### 2.4 性能瓶颈 (中等)

**问题描述:**

1. **无事务管理:**
   - 每个 SQL 语句独立执行
   - 失败后无法回滚
   - 可能导致数据库状态不一致

2. **无版本追踪:**
   - 每次启动都执行所有迁移
   - 无法跳过已执行的迁移
   - 浪费启动时间

3. **无并发保护:**
   - 多实例启动可能导致竞态条件
   - 无锁机制保护迁移过程

### 2.5 代码冗余 (低)

**问题描述:**

1. **重复表定义:**
   - `step_create_e2ee_tables` 函数重复创建 `device_keys` 表
   - `step_ensure_additional_tables` 函数重复创建 `typing` 表

2. **重复索引创建:**
   - 多个迁移文件创建相同索引
   - 使用 `IF NOT EXISTS` 掩盖问题

---

## 3. 优化方案

### 3.1 SQL 语句分割器重构

**方案:**
重写 `split_sql_statements` 函数，使用状态机模式：

```rust
enum ParserState {
    Normal,
    InSingleQuote,
    InDoubleQuote,
    InDollarQuote(String),
    InLineComment,
    InBlockComment,
}
```

**改进点:**
1. 明确的状态转换
2. 正确处理嵌套结构
3. 完善的边界情况处理

### 3.2 Schema 统一化

**方案:**
创建单一权威 Schema 文件，移除冲突定义：

1. **合并策略:**
   - 以 `master_unified_schema.sql` 为基础
   - 补充 `schema.sql` 中的兼容性定义
   - 移除重复和冲突的表定义

2. **迁移策略:**
   - 创建数据迁移脚本
   - 添加列别名视图
   - 逐步废弃旧定义

### 3.3 迁移版本控制

**方案:**
实现完整的迁移版本控制系统：

```sql
CREATE TABLE IF NOT EXISTS schema_migrations (
    version VARCHAR(255) PRIMARY KEY,
    checksum VARCHAR(64) NOT NULL,
    executed_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW(),
    execution_time_ms BIGINT,
    success BOOLEAN NOT NULL DEFAULT TRUE
);
```

**功能:**
1. 记录已执行的迁移
2. 校验迁移文件完整性
3. 支持回滚操作

### 3.4 事务管理优化

**方案:**
为每个迁移文件使用独立事务：

```rust
async fn execute_migration(&self, sql: &str) -> Result<(), Error> {
    let mut tx = self.pool.begin().await?;
    
    for statement in self.split_sql_statements(sql) {
        match sqlx::raw_sql(&statement).execute(&mut *tx).await {
            Ok(_) => continue,
            Err(e) => {
                tx.rollback().await?;
                return Err(e);
            }
        }
    }
    
    tx.commit().await?;
    Ok(())
}
```

### 3.5 性能优化

**方案:**

1. **批量执行:**
   - 合并小型迁移文件
   - 减少数据库往返次数

2. **延迟索引创建:**
   - 先创建表结构
   - 数据加载后创建索引

3. **并行迁移:**
   - 识别无依赖的迁移
   - 并行执行提升效率

---

## 4. 实施计划

### 阶段一: 紧急修复 (1-2 天)

1. 修复 SQL 语句分割器的块注释处理
2. 修复 `20260211000003_cleanup_legacy_friends.sql` 文件
3. 移除 `20260209100000_add_performance_indexes.sql` 中的无效索引

### 阶段二: Schema 统一 (3-5 天)

1. 创建统一的 Schema 定义文件
2. 合并冲突的表定义
3. 创建数据迁移脚本
4. 更新所有依赖代码

### 阶段三: 迁移系统重构 (5-7 天)

1. 实现迁移版本控制
2. 添加事务管理
3. 实现回滚机制
4. 添加并发保护

### 阶段四: 性能优化 (2-3 天)

1. 优化迁移执行顺序
2. 实现并行迁移
3. 添加性能监控

---

## 5. 风险评估

| 风险 | 等级 | 缓解措施 |
|------|------|----------|
| 数据丢失 | 高 | 完整备份 + 事务回滚 |
| 迁移失败 | 中 | 版本控制 + 回滚机制 |
| 性能下降 | 低 | 性能测试 + 监控 |
| 兼容性问题 | 中 | 兼容层 + 渐进迁移 |

---

## 6. 验证策略

### 6.1 单元测试

- SQL 语句分割器测试
- Schema 验证测试
- 迁移版本控制测试

### 6.2 集成测试

- 完整迁移流程测试
- 回滚测试
- 并发测试

### 6.3 性能测试

- 迁移执行时间测试
- 数据库查询性能测试
- 启动时间测试

---

## 7. 回滚机制

### 7.1 数据库备份

```bash
pg_dump -U synapse -d synapse_test > backup_$(date +%Y%m%d).sql
```

### 7.2 迁移回滚

每个迁移文件需要配套的回滚脚本：

```
migrations/
├── 20260206000000_master_unified_schema.sql
├── 20260206000000_master_unified_schema_rollback.sql
├── 20260209100000_add_performance_indexes.sql
└── 20260209100000_add_performance_indexes_rollback.sql
```

### 7.3 紧急恢复

```sql
-- 恢复到指定版本
DELETE FROM schema_migrations WHERE version > '20260206000000';
-- 执行回滚脚本
\i migrations/rollback_to_20260206000000.sql
```

---

## 8. 预期成果

| 指标 | 当前 | 优化后 | 改进 |
|------|------|--------|------|
| 迁移执行时间 | ~2s | ~0.5s | 75% |
| 启动时间 | ~60s | ~30s | 50% |
| 迁移失败率 | 15% | <1% | 93% |
| 代码可维护性 | 低 | 高 | - |
| 数据一致性风险 | 高 | 低 | - |
