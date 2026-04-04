# 数据库审计与优化工作 - 快速参考指南

> 创建日期：2026-04-04  
> 用途：快速查找和使用数据库优化相关的脚本和文档

---

## 📋 快速导航

### 核心文档

| 文档 | 用途 | 位置 |
|------|------|------|
| 完整审计报告 | 了解数据库现状和问题 | `docs/synapse-rust/DATABASE_AUDIT_REPORT_2026-04-04.md` |
| 审计总结 | 快速了解关键发现 | `docs/synapse-rust/DATABASE_AUDIT_SUMMARY_2026-04-04.md` |
| 管理标准 | 迁移开发规范 | `docs/synapse-rust/DATABASE_VERSION_MANAGEMENT_STANDARDS.md` |
| Phase 1 报告 | 实施成果总结 | `docs/synapse-rust/DATABASE_PHASE1_EXECUTION_REPORT.md` |
| 合并计划 | Phase 2 实施指南 | `migrations/CONSOLIDATION_PLAN.md` |

### 实用脚本

| 脚本 | 功能 | 使用方法 |
|------|------|---------|
| 数据库备份 | 备份 PostgreSQL 和 Redis | `./scripts/backup_database.sh` |
| 重复索引修复 | 修复重复索引定义 | `./scripts/fix_duplicate_indexes.sh` |
| 并发索引转换 | 转换为 CONCURRENTLY 模式 | `./scripts/convert_indexes_to_concurrent.sh` |
| 迁移测试 | 在隔离环境测试迁移 | `./scripts/test_migrations.sh` |

---

## 🚀 常用操作

### 1. 运行迁移测试

```bash
# 完整测试（创建测试数据库，运行所有测试，生成报告）
./scripts/test_migrations.sh

# 查看测试报告
cat docs/synapse-rust/MIGRATION_TEST_REPORT_*.md
```

### 2. 创建数据库备份

```bash
# 备份数据库
./scripts/backup_database.sh

# 备份位置
ls -lh backups/
```

### 3. 检查迁移文件

```bash
# 查看所有活跃迁移
ls -1 migrations/202*.sql migrations/999*.sql

# 检查索引定义
grep -r "CREATE.*INDEX" migrations/*.sql | grep -v ".undo.sql"

# 检查并发索引
grep -r "CONCURRENTLY" migrations/*.sql | wc -l
```

### 4. 验证幂等性

```bash
# 检查 IF NOT EXISTS
grep -r "CREATE.*INDEX" migrations/*.sql | grep -v "IF NOT EXISTS" | grep -v ".undo.sql"

# 应该没有输出（除了统一 schema）
```

---

## 📊 关键指标

### 当前状态（Phase 1 完成后）

| 指标 | 数值 |
|------|------|
| 迁移文件总数 | 31 个 |
| 重复索引 | 0 个（已修复为幂等） |
| 并发索引比例 | ~100% |
| 备份脚本 | 4 个 |
| 测试脚本 | 1 个 |
| 文档完整性 | 优秀 |

### 数据库统计

| 类型 | 数量 |
|------|------|
| 表 | 180+ |
| 索引 | 250+ |
| 外键 | 84+ |
| 迁移文件 | 31 个活跃 + 16 个归档 |

---

## 🔧 故障排查

### 问题：迁移执行失败

```bash
# 1. 检查数据库连接
psql -d synapse -c "SELECT version();"

# 2. 查看错误日志
tail -f /tmp/test_migration_*.log

# 3. 回滚到备份
cp migrations/.backup_*/* migrations/
```

### 问题：索引创建锁表

```bash
# 1. 检查是否使用 CONCURRENTLY
grep "CREATE INDEX" migrations/xxx.sql | grep -v "CONCURRENTLY"

# 2. 如果没有，手动添加
sed -i 's/CREATE INDEX/CREATE INDEX CONCURRENTLY/g' migrations/xxx.sql

# 3. 重新测试
./scripts/test_migrations.sh
```

### 问题：重复索引错误

```bash
# 1. 检查重复定义
./scripts/fix_duplicate_indexes.sh

# 2. 查看修复报告
cat docs/synapse-rust/DUPLICATE_INDEX_FIX_REPORT_*.md
```

---

## 📝 开发新迁移

### 步骤 1：创建迁移文件

```bash
# 使用时间戳命名
TIMESTAMP=$(date +%Y%m%d%H%M%S)
touch migrations/${TIMESTAMP}_your_description.sql
```

### 步骤 2：编写迁移内容

```sql
-- 设置时区
SET TIME ZONE 'UTC';

-- 创建表（使用 IF NOT EXISTS）
CREATE TABLE IF NOT EXISTS your_table (
    id BIGSERIAL PRIMARY KEY,
    -- 其他字段
);

-- 创建索引（使用 CONCURRENTLY 和 IF NOT EXISTS）
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_your_table_field
ON your_table(field);
```

### 步骤 3：创建 undo 文件

```bash
touch migrations/${TIMESTAMP}_your_description.undo.sql
```

### 步骤 4：测试

```bash
# 在测试环境测试
./scripts/test_migrations.sh

# 检查结果
echo $?  # 应该返回 0
```

---

## 🎯 Phase 2 准备

### 待执行任务

1. **合并 schema alignment 迁移**
   - 文件：10 个 → 1 个
   - 预计时间：12-16 小时
   - 参考：`migrations/CONSOLIDATION_PLAN.md`

2. **合并小型功能迁移**
   - 文件：3 个 → 1 个
   - 预计时间：4-6 小时

3. **创建 schema 漂移检测工具**
   - 自动化检测
   - CI 集成
   - 预计时间：8-12 小时

### 开始 Phase 2

```bash
# 1. 阅读合并计划
cat migrations/CONSOLIDATION_PLAN.md

# 2. 创建新的合并迁移文件
touch migrations/20260404000001_consolidated_schema_alignment.sql

# 3. 按计划合并内容
# （参考 CONSOLIDATION_PLAN.md 中的详细步骤）

# 4. 测试合并后的迁移
./scripts/test_migrations.sh
```

---

## 📚 相关资源

### 内部文档

- 迁移使用说明：`migrations/README.md`
- 迁移索引：`migrations/MIGRATION_INDEX.md`
- 字段命名规范：`migrations/DATABASE_FIELD_STANDARDS.md`
- 一致性检查：`scripts/check_field_consistency.sql`

### 外部参考

- PostgreSQL 索引文档：https://www.postgresql.org/docs/current/indexes.html
- CREATE INDEX CONCURRENTLY：https://www.postgresql.org/docs/current/sql-createindex.html
- 迁移最佳实践：https://www.postgresql.org/docs/current/ddl-alter.html

---

## 🆘 获取帮助

### 常见问题

1. **Q: 如何回滚迁移？**
   - A: 使用对应的 `.undo.sql` 文件，或从备份恢复

2. **Q: 为什么要使用 CONCURRENTLY？**
   - A: 避免在生产环境锁表，不阻塞读写操作

3. **Q: 如何验证迁移的幂等性？**
   - A: 运行 `./scripts/test_migrations.sh`，会自动测试重复执行

4. **Q: 备份在哪里？**
   - A: `migrations/.backup_*/` 目录

### 联系方式

- 查看文档：`docs/synapse-rust/DATABASE_*.md`
- 查看计划：`migrations/CONSOLIDATION_PLAN.md`
- 运行测试：`./scripts/test_migrations.sh`

---

**最后更新**：2026-04-04  
**维护者**：数据库团队  
**版本**：v1.0
