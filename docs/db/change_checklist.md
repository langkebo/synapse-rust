# 数据库变更检查清单

> **项目**: synapse-rust 数据库全面排查与优化
> **版本**: v1.0.0
> **生成日期**: 2026-03-20
> **适用场景**: SQL 迁移脚本 / Rust 代码数据库变更

---

## 1. 概述

本文档定义了数据库表结构变更的标准流程和检查清单，确保所有变更都经过充分审查，符合 `DATABASE_FIELD_STANDARDS.md` 规范。

---

## 2. 变更类型分类

### 2.1 变更等级

| 等级 | 类型 | 说明 | 需要审批 |
|------|------|------|----------|
| **P0** | 紧急修复 | 数据丢失风险、安全漏洞修复 | 口头确认即可 |
| **P1** | 重大变更 | 表结构变化、索引变更 | 必须代码审查 |
| **P2** | 中等变更 | 字段新增、约束变更 | 需要审查 |
| **P3** | 轻微变更 | 注释更新、文档变更 | 自行检查 |

---

## 3. SQL 迁移脚本检查

### 3.1 必检项目

#### 命名规范检查

- [ ] 所有字段名使用 `snake_case`
- [ ] 时间戳字段正确使用 `_ts` 或 `_at` 后缀
  - `_ts`: 创建时间、更新时间、活跃时间
  - `_at`: 过期时间、撤销时间、验证时间
- [ ] 布尔字段使用 `is_` 或 `has_` 前缀
- [ ] 外键字段使用 `{table}_id` 格式

#### 安全检查

- [ ] 不使用被禁止的字段名
- [ ] `IF NOT EXISTS` 用于表创建
- [ ] `ADD COLUMN IF NOT EXISTS` 用于列添加
- [ ] 不删除现有列（使用软删除）
- [ ] 不删除现有索引

#### 性能检查

- [ ] 新表有适当的索引
- [ ] 唯一约束有对应索引
- [ ] 外键有对应索引
- [ ] 大表变更避开高峰期

### 3.2 迁移脚本模板

```sql
-- ============================================
-- 迁移脚本: YYYYMMDDHHMMSS_description.sql
-- 作者: [姓名]
-- 日期: YYYY-MM-DD
-- 变更类型: [ADD_TABLE/ADD_COLUMN/ADD_INDEX/etc]
-- 严重程度: [P0/P1/P2/P3]
-- ============================================

-- 变更描述:
-- [详细说明变更原因和内容]

BEGIN;

-- 1. 检查是否存在
-- 例如: 检查表是否存在
SELECT 1 FROM pg_tables WHERE tablename = 'your_table';

-- 2. 执行变更
-- 例如: 添加新表
CREATE TABLE IF NOT EXISTS your_table (
    id BIGSERIAL PRIMARY KEY,
    field1 TEXT NOT NULL,
    created_ts BIGINT NOT NULL,
    updated_ts BIGINT
);

-- 例如: 添加新列
ALTER TABLE your_table ADD COLUMN IF NOT EXISTS new_column TEXT;

-- 3. 创建索引
CREATE INDEX IF NOT EXISTS idx_your_table_field1 ON your_table(field1);

-- 4. 记录迁移
INSERT INTO schema_migrations (version, description, success)
VALUES ('YYYYMMDDHHMMSS', 'description', TRUE)
ON CONFLICT (version) DO UPDATE SET success = TRUE;

COMMIT;

-- 回滚方案 (如需回滚):
-- ALTER TABLE your_table DROP COLUMN IF EXISTS new_column;
```

### 3.3 检查清单示例

```markdown
## 迁移检查清单

### 基本信息
- [ ] 脚本名称格式正确: `YYYYMMDDHHMMSS_description.sql`
- [ ] 包含变更描述
- [ ] 包含作者信息
- [ ] 包含回滚方案

### 命名规范
- [ ] 字段名使用 snake_case
- [ ] 时间戳使用正确后缀 (_ts/_at)
- [ ] 布尔字段使用 is_/has_ 前缀
- [ ] 无禁止字段名

### 安全规范
- [ ] 使用 IF NOT EXISTS
- [ ] 使用 ADD COLUMN IF NOT EXISTS
- [ ] 不删除现有列
- [ ] 不删除现有索引

### 性能规范
- [ ] 有适当的索引
- [ ] 避开高峰期执行
- [ ] 考虑大表影响
```

---

## 4. Rust 代码检查

### 4.1 必检项目

#### 模型定义检查

- [ ] 结构体字段与 SQL 表结构一致
- [ ] 使用正确的 Rust 类型映射
  - `BIGINT NOT NULL` → `i64`
  - `BIGINT` → `Option<i64>`
  - `TEXT NOT NULL` → `String`
  - `TEXT` → `Option<String>`
  - `BOOLEAN` → `bool` 或 `Option<bool>`
- [ ] `sqlx::FromRow` 派生正确

#### SQL 查询检查

- [ ] 字段名与模型一致
- [ ] 使用参数化查询 (`.bind()`)
- [ ] 无 SQL 注入风险

### 4.2 模型定义模板

```rust
// ✅ 正确示例
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct YourModel {
    pub id: i64,
    pub field1: String,
    pub created_ts: i64,
    pub updated_ts: Option<i64>,
    pub is_active: bool,
}

// ❌ 错误示例
#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize)]
pub struct YourModel {
    pub id: i32,                    // 错误: BIGSERIAL 应为 i64
    pub field1: Option<String>,     // 错误: NOT NULL 列不应为 Option
    pub created_at: String,         // 错误: 应为 created_ts
    pub is_active: Option<bool>,    // 错误: 非空布尔不应为 Option
}
```

### 4.3 检查清单示例

```markdown
## Rust 代码检查清单

### 模型定义
- [ ] 结构体字段与 SQL 一致
- [ ] 类型映射正确
- [ ] sqlx::FromRow 派生正确
- [ ] 无多余的 UNUSED 字段

### SQL 查询
- [ ] 字段名与模型一致
- [ ] 使用参数化查询
- [ ] 错误处理完善
- [ ] 事务使用正确

### 索引对应
- [ ] 新增字段有对应索引
- [ ] 唯一约束有对应索引
- [ ] 外键有对应索引
```

---

## 5. 一致性检查

### 5.1 三方一致检查

| 检查项 | SQL Schema | Rust 表定义 | Rust 模型 |
|--------|------------|-------------|-----------|
| 表名 | ✅ | ✅ | ✅ |
| 列名 | ✅ | ✅ | ✅ |
| 数据类型 | ✅ | ✅ | ✅ |
| 约束 | ✅ | ✅ | N/A |
| 索引 | ✅ | ✅ | N/A |

### 5.2 检查命令

```bash
#!/bin/bash
# 数据库一致性检查脚本

echo "=== 检查 SQL Schema vs Rust 模型一致性 ==="

# 检查 users 表
echo "检查 users 表..."
psql -U synapse -d synapse -c "SELECT column_name FROM information_schema.columns WHERE table_name = 'users' ORDER BY column_name;"

# 检查 refresh_tokens 表
echo "检查 refresh_tokens 表..."
psql -U synapse -d synapse -c "SELECT column_name FROM information_schema.columns WHERE table_name = 'refresh_tokens' ORDER BY column_name;"

# 检查索引
echo "检查索引..."
psql -U synapse -d synapse -c "SELECT indexname, indexdef FROM pg_indexes WHERE schemaname = 'public' ORDER BY tablename, indexname;"
```

---

## 6. 审查流程

### 6.1 提交流程

```
开发者 → 本地测试 → 代码审查 → 合并到主分支 → 部署
   ↓
填写变更清单
```

### 6.2 审查要点

1. **命名规范**: 符合 `DATABASE_FIELD_STANDARDS.md`
2. **安全性**: 无 SQL 注入风险
3. **性能**: 索引设计合理
4. **兼容性**: 向后兼容
5. **可回滚**: 有回滚方案

---

## 7. 相关文档

| 文档 | 位置 |
|------|------|
| 字段标准 | `migrations/DATABASE_FIELD_STANDARDS.md` |
| 迁移索引 | `migrations/MIGRATION_INDEX.md` |
| SQL 表清单 | `docs/db/sql_table_inventory.md` |
| Rust 表清单 | `docs/db/rust_table_inventory.md` |
| Rust 模型清单 | `docs/db/rust_model_inventory.md` |

---

## 8. 文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|----------|
| 2026-03-20 | v1.0.0 | 初始版本 |