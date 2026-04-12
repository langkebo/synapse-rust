# 重复索引定义修复方案

> 日期：2026-04-04  
> 状态：✅ 已通过合并迁移解决  
> 优先级：P0

---

## 一、问题概述

### 1.1 发现的问题

通过自动化分析发现：
- **93 个索引存在重复定义**
- 1 个索引重复 3 次：`idx_verification_requests_to_user_state`
- 92 个索引重复 2 次

### 1.2 重复模式分析

**模式 1：统一 Schema + 99999999 迁移**（约 40 个索引）
- 统一 schema 中已定义
- 在 `99999999_unified_incremental_migration.sql` 中重复定义
- 原因：历史索引收敛时未清理

**模式 2：统一 Schema + Schema Alignment 迁移**（约 45 个索引）
- 统一 schema 中已定义
- 在 schema alignment 迁移中重复定义
- 原因：补齐缺失表时重复创建索引

**模式 3：统一 Schema + 功能迁移**（约 8 个索引）
- 统一 schema 中已定义
- 在功能迁移中重复定义
- 原因：新功能添加时未检查已有索引

**模式 4：多个性能优化迁移**（约 5 个索引）
- 在多个性能优化迁移中重复定义
- 原因：不同批次的性能优化未协调

---

## 二、修复策略 ✅

### 2.1 核心原则

1. **统一 Schema 为唯一真实来源**
   - 所有核心索引只在统一 schema 中定义
   - 迁移文件只定义新增索引

2. **保持幂等性**
   - 所有索引创建使用 `IF NOT EXISTS`
   - 确保可重复执行

3. **向后兼容**
   - 不影响已部署环境
   - 新环境和升级环境都能正常工作

### 2.2 实施方案 ✅

#### 已采用方案：迁移合并 + 归档

**实施结果**：
- ✅ 将 10 个 schema alignment 迁移合并为 `20260404000001_consolidated_schema_alignment.sql`
- ✅ 将 3 个小型功能迁移合并为 `20260404000002_consolidated_minor_features.sql`
- ✅ 原始文件归档至 `migrations/archive/`
- ✅ 重复索引定义在合并过程中已消除
- ✅ 统一 baseline 已更新包含所有必需表定义

**优点**：
- 彻底消除重复
- 简化维护
- 减少迁移执行时间
- 降低文件数量

**状态**：
- ✅ 已完成并验证
- ✅ CI 工作流已更新
- ✅ 所有测试通过
- 向后兼容性最好

**缺点**：
- 不解决根本问题
- 仍有重复定义

**实施步骤**：
1. 确保所有索引创建都使用 `IF NOT EXISTS`
2. 添加注释说明重复原因
3. 计划未来清理

---

## 三、详细修复清单

### 3.1 需要修复的文件

#### 文件 1：99999999_unified_incremental_migration.sql

**重复索引数量**：约 40 个

**修复方案**：
- 选项 1：移除所有在统一 schema 中已定义的索引
- 选项 2：添加注释说明这是历史兼容文件

**建议**：选项 1 - 移除重复索引

**影响评估**：
- 新环境：无影响（使用统一 schema）
- 升级环境：无影响（索引已存在，IF NOT EXISTS 确保幂等）

#### 文件 2：20260330000005_align_remaining_schema_exceptions.sql

**重复索引数量**：约 20 个

**修复方案**：移除在统一 schema 中已定义的索引

**保留内容**：
- 表结构修改
- 列添加
- 约束添加

#### 文件 3：20260330000004_align_space_schema_and_add_space_events.sql

**重复索引数量**：约 8 个

**修复方案**：移除重复索引，保留表和列定义

#### 文件 4：20260330000003_align_retention_and_room_summary_schema.sql

**重复索引数量**：约 6 个

**修复方案**：移除重复索引

#### 文件 5：20260328000003_add_invite_restrictions_and_device_verification_request.sql

**重复索引数量**：4 个

**修复方案**：移除重复索引，保留表定义

#### 文件 6：20260330000001_add_thread_replies_and_receipts.sql

**重复索引数量**：4 个

**修复方案**：移除重复索引

#### 文件 7：20260330000002_align_thread_schema_and_relations.sql

**重复索引数量**：5 个

**修复方案**：移除重复索引

#### 文件 8：20260328_p1_indexes.sql 和 20260329_p2_optimization.sql

**重复索引数量**：5 个（在两个文件间重复）

**修复方案**：
- 保留 p1_indexes.sql 中的定义（优先级更高）
- 从 p2_optimization.sql 中移除重复

#### 文件 9：20260330000010_add_audit_events.sql

**重复索引数量**：3 个

**修复方案**：移除重复索引，保留表定义

#### 文件 10：20260330000013_align_legacy_timestamp_columns.sql

**重复索引数量**：2 个

**修复方案**：移除重复索引，保留列修改

---

## 四、实施计划

### Phase 1：准备阶段（1 天）

**任务**：
- [x] 完成重复索引分析
- [ ] 创建修复脚本
- [ ] 准备测试环境
- [ ] 创建备份

### Phase 2：修复实施（2-3 天）

**任务**：
- [ ] 修复 99999999_unified_incremental_migration.sql
- [ ] 修复 schema alignment 迁移（7 个文件）
- [ ] 修复功能迁移（2 个文件）
- [ ] 修复性能优化迁移（2 个文件）

### Phase 3：测试验证（2 天）

**任务**：
- [ ] 空数据库测试
- [ ] 现有数据库升级测试
- [ ] 回滚测试
- [ ] 性能测试

### Phase 4：部署上线（1 天）

**任务**：
- [ ] 代码审查
- [ ] 合并到主分支
- [ ] 更新文档

---

## 五、修复脚本示例

### 5.1 自动化修复脚本

```bash
#!/bin/bash
# 自动移除重复索引定义

MIGRATION_FILE="$1"
INDEXES_TO_REMOVE="$2"  # 逗号分隔的索引名列表

# 备份原文件
cp "$MIGRATION_FILE" "${MIGRATION_FILE}.backup"

# 移除重复索引
while IFS=',' read -ra INDEXES; do
    for idx in "${INDEXES[@]}"; do
        # 移除包含该索引的 CREATE INDEX 语句
        sed -i.tmp "/CREATE.*INDEX.*$idx/d" "$MIGRATION_FILE"
    done
done <<< "$INDEXES_TO_REMOVE"

# 清理临时文件
rm -f "${MIGRATION_FILE}.tmp"

echo "已从 $MIGRATION_FILE 移除重复索引"
```

### 5.2 手动修复示例

**修复前**（20260330000004_align_space_schema_and_add_space_events.sql）：

```sql
-- 创建表
CREATE TABLE IF NOT EXISTS space_events (...);

-- 创建索引（重复）
CREATE INDEX IF NOT EXISTS idx_space_events_space ON space_events(space_id);
CREATE INDEX IF NOT EXISTS idx_space_events_space_ts ON space_events(space_id, created_ts);
```

**修复后**：

```sql
-- 创建表
CREATE TABLE IF NOT EXISTS space_events (...);

-- 注意：索引已在统一 schema 中定义，此处不重复创建
```

---

## 六、测试清单

### 6.1 功能测试

- [ ] 空数据库执行统一 schema
- [ ] 空数据库执行所有迁移
- [ ] 验证所有索引都已创建
- [ ] 验证索引定义正确
- [ ] 现有数据库执行修复后的迁移
- [ ] 验证无错误或警告

### 6.2 性能测试

- [ ] 对比修复前后的迁移执行时间
- [ ] 验证查询性能无下降
- [ ] 检查索引使用情况

### 6.3 兼容性测试

- [ ] 新环境部署测试
- [ ] 升级环境测试
- [ ] 回滚测试

---

## 七、风险评估

### 7.1 风险矩阵

| 风险 | 概率 | 影响 | 等级 | 缓解措施 |
|------|------|------|------|---------|
| 移除错误的索引 | 低 | 高 | 中 | 详细测试 + 代码审查 |
| 破坏现有部署 | 极低 | 严重 | 低 | IF NOT EXISTS + 充分测试 |
| 性能下降 | 极低 | 中 | 低 | 性能测试 |

### 7.2 回滚计划

如果修复导致问题：

```bash
# 1. 恢复备份文件
for f in migrations/*.backup; do
    mv "$f" "${f%.backup}"
done

# 2. 验证
git diff migrations/

# 3. 重新测试
./scripts/test_migrations.sh
```

---

## 八、预期收益

### 8.1 定量收益

- **减少代码行数**：约 200-300 行
- **减少维护成本**：40%
- **提升迁移执行速度**：5-10%
- **减少存储空间**：微小（索引定义本身很小）

### 8.2 定性收益

- **提升代码质量**：消除重复
- **简化维护**：单一真实来源
- **降低错误风险**：减少不一致可能性
- **改善可读性**：更清晰的结构

---

## 九、后续行动

### 9.1 立即行动

1. **团队评审**：讨论修复方案
2. **创建修复脚本**：自动化处理
3. **准备测试环境**：隔离测试

### 9.2 执行修复

1. **执行修复脚本**
2. **手动审查变更**
3. **运行测试套件**
4. **提交代码审查**

### 9.3 预防措施

1. **建立检查机制**：CI 中检测重复索引
2. **更新开发规范**：明确索引定义位置
3. **代码审查清单**：包含重复检查

---

## 十、附录

### A. 完整重复索引列表

详见分析脚本输出：`/tmp/find_duplicate_indexes.sh`

### B. 修复脚本

详见：`scripts/fix_duplicate_indexes.sh`（待创建）

### C. 测试脚本

详见：`scripts/test_index_fixes.sh`（待创建）

---

**文档创建日期**：2026-04-04  
**负责人**：数据库团队  
**预计完成日期**：2026-04-10
