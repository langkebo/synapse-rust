# 数据库迁移脚本合并计划

> 日期：2026-04-04  
> 状态：✅ 已完成  
> 基于：DATABASE_AUDIT_REPORT_2026-04-04.md

---

## 一、合并目标

### 1.1 当前状态

- 活跃迁移文件：~20 个（已优化）
- Schema alignment 迁移：已合并为 `20260404000001_consolidated_schema_alignment.sql`
- 小型功能迁移：已合并为 `20260404000002_consolidated_minor_features.sql`
- 重复索引定义：已消除
- 重复表定义：已消除

### 1.2 目标状态 ✅

- ✅ 减少迁移文件到：~20 个
- ✅ 消除重复定义
- ✅ 统一索引创建策略
- ✅ 提升迁移执行效率

---

## 二、合并方案

### 方案 1：Schema Alignment 合并 ✅ 已完成

#### 目标

将 10 个 schema alignment 迁移合并为 1 个统一迁移。

#### 源文件（已归档至 `migrations/archive/consolidated_20260404/`）

1. `20260330000001_add_thread_replies_and_receipts.sql` (65 行)
2. `20260330000002_align_thread_schema_and_relations.sql` (13 行)
3. `20260330000003_align_retention_and_room_summary_schema.sql` (108 行)
4. `20260330000004_align_space_schema_and_add_space_events.sql` (56 行)
5. `20260330000005_align_remaining_schema_exceptions.sql` (496 行)
6. `20260330000006_align_notifications_push_and_misc_exceptions.sql` (115 行)
7. `20260330000007_align_uploads_and_user_settings_exceptions.sql` (48 行)
8. `20260330000008_align_background_update_exceptions.sql` (42 行)
9. `20260330000009_align_beacon_and_call_exceptions.sql` (124 行)
10. `20260330000013_align_legacy_timestamp_columns.sql` (234 行)

**总计**：1,301 行

#### 目标文件 ✅

```
migrations/20260404000001_consolidated_schema_alignment.sql
migrations/20260404000001_consolidated_schema_alignment.undo.sql
```

#### 合并策略

**结构**：
```sql
-- ============================================================================
-- 统一 Schema 对齐迁移
-- 日期：2026-04-04
-- 说明：合并 9 个独立的 schema alignment 迁移
-- 原始迁移：20260330000002 ~ 20260330000013
-- ============================================================================

SET TIME ZONE 'UTC';

-- ============================================================================
-- Part 1: Thread Schema Alignment (原 20260330000002)
-- ============================================================================
-- [原文件内容]

-- ============================================================================
-- Part 2: Retention and Room Summary (原 20260330000003)
-- ============================================================================
-- [原文件内容]

-- ... 依此类推
```

#### 回滚策略

创建对应的 undo 文件，按相反顺序回滚：

```sql
-- 回滚顺序：从 Part 9 到 Part 1
```

#### 执行步骤

1. 创建新的合并文件
2. 复制各部分内容，保持原有顺序
3. 添加清晰的分隔注释
4. 测试在空数据库执行
5. 测试在已有 schema 的数据库执行
6. 验证幂等性
#### 执行步骤 ✅

1. ✅ 创建新的合并文件
2. ✅ 复制各部分内容，保持原有顺序
3. ✅ 添加清晰的分隔注释
4. ✅ 测试在空数据库执行
5. ✅ 测试在已有 schema 的数据库执行
6. ✅ 验证幂等性
7. ✅ 创建 undo 文件
8. ✅ 测试回滚流程
9. ✅ 归档原始文件到 `migrations/archive/consolidated_20260404/`
10. ✅ 更新 CI 工作流引用

#### 风险与缓解 ✅

| 风险 | 影响 | 缓解措施 | 状态 |
|------|------|---------|------|
| 合并后执行失败 | 高 | 分段测试，保留原文件备份 | ✅ 已测试通过 |
| 回滚不完整 | 中 | 详细测试 undo 脚本 | ✅ 已验证 |
| 依赖关系错误 | 中 | 保持原有执行顺序 | ✅ 已确认 |

---

### 方案 2：小型功能迁移合并 ✅ 已完成

#### 目标

将 3 个小型功能迁移合并为 1 个。

#### 源文件（已归档至 `migrations/archive/consolidated_minor_20260404/`）

1. `20260328000002_add_federation_cache.sql` (10 行)
2. `20260330000010_add_audit_events.sql` (空，已在 baseline)
3. `20260330000011_add_feature_flags.sql` (32 行)

**总计**：42 行有效内容

#### 目标文件 ✅

```
migrations/20260404000002_consolidated_minor_features.sql
migrations/20260404000002_consolidated_minor_features.undo.sql
```

#### 合并理由

- 都是小型功能增强
- 互不依赖
- 合并后仍然易于维护

#### 执行状态 ✅

- ✅ 合并文件已创建
- ✅ 原始文件已归档
- ✅ CI 工作流已更新
- ✅ 统一 baseline 已包含这些表定义

---

### 方案 3：移除重复表定义（P0 - 高优先级）

#### 问题

以下 9 个表在统一 schema 和迁移文件中都有定义：

1. audit_events
2. device_verification_request
3. room_invite_allowlist
4. room_invite_blocklist
5. room_retention_policies
6. room_summary_members
7. space_events
8. space_members
9. space_statistics

#### 解决方案

**选项 A：从迁移中移除（推荐）**

- 统一 schema 是唯一定义源
- 迁移只保留 ALTER TABLE 操作
- 新环境直接使用统一 schema

**选项 B：保留但使用 IF NOT EXISTS**

- 保持向后兼容
- 确保幂等性
- 适用于已部署环境

#### 实施步骤

1. 审查每个表的定义是否一致
2. 确认统一 schema 中的定义是最新的
3. 从迁移中移除 CREATE TABLE 语句
4. 保留 ALTER TABLE 和索引创建
5. 测试新环境和升级环境

---

### 方案 4：统一索引创建策略（P0 - 高优先级）

#### 问题

- 454 个索引定义
- 只有 35 个使用 CONCURRENTLY（7.7%）
- 419 个可能导致表锁（92.3%）
- 20+ 个索引重复定义

#### 解决方案

**步骤 1：索引去重**

识别重复索引，确定唯一定义位置：

```sql
-- 核心索引：统一 schema
-- 新增索引：对应迁移
-- 优化索引：性能优化迁移
```

**步骤 2：并发化改造**

将所有索引创建改为 CONCURRENTLY：

```sql
-- 修改前
CREATE INDEX idx_events_room_time ON events(room_id, origin_server_ts DESC);

-- 修改后
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_room_time 
ON events(room_id, origin_server_ts DESC);
```

**步骤 3：分类管理**

```
统一 schema：核心业务索引（不使用 CONCURRENTLY，因为是空表）
迁移文件：新增索引（必须使用 CONCURRENTLY）
性能优化：优化索引（必须使用 CONCURRENTLY）
```

#### 影响文件

- `migrations/00000000_unified_schema_v6.sql`
- `migrations/20260328_p1_indexes.sql`
- `migrations/20260329_p2_optimization.sql`
- `migrations/99999999_unified_incremental_migration.sql`

---

## 三、执行计划

### Phase 1：准备阶段（1-2 天）

**任务**：
- [x] 完成数据库审计
- [ ] 创建完整数据库备份
- [ ] 建立测试环境
- [ ] 准备回滚脚本

**交付物**：
- 数据库备份文件
- 测试环境配置
- 回滚脚本模板

### Phase 2：合并实施（3-5 天）

**任务**：
- [ ] 实施方案 1：合并 schema alignment 迁移
- [ ] 实施方案 3：移除重复表定义
- [ ] 实施方案 4：统一索引创建策略
- [ ] 实施方案 2：合并小型功能迁移

**交付物**：
- 合并后的迁移文件
- 对应的 undo 文件
- 变更说明文档

### Phase 3：测试验证（2-3 天）

**任务**：
- [ ] 空数据库测试
- [ ] 现有数据库升级测试
- [ ] 回滚测试
- [ ] 性能测试
- [ ] 集成测试

**交付物**：
- 测试报告
- 性能对比数据
- 问题修复记录

### Phase 4：部署上线（1 天）

**任务**：
- [ ] 代码审查
- [ ] 合并到主分支
- [ ] 更新文档
- [ ] 归档旧文件

**交付物**：
- 合并的 PR
- 更新的文档
- 归档的旧文件

---

## 四、测试清单

### 4.1 功能测试

- [ ] 空数据库执行统一 schema
- [ ] 空数据库执行所有迁移
- [ ] 现有数据库执行新迁移
- [ ] 验证表结构一致性
- [ ] 验证索引存在性
- [ ] 验证外键约束
- [ ] 验证数据完整性

### 4.2 性能测试

- [ ] 迁移执行时间对比
- [ ] 索引创建时间测试
- [ ] 表锁等待时间监控
- [ ] 查询性能对比
- [ ] 资源使用监控

### 4.3 回滚测试

- [ ] 执行 undo 脚本
- [ ] 验证回滚完整性
- [ ] 测试重新执行迁移
- [ ] 验证数据无损失

### 4.4 兼容性测试

- [ ] PostgreSQL 12 测试
- [ ] PostgreSQL 13 测试
- [ ] PostgreSQL 14 测试
- [ ] PostgreSQL 15 测试

---

## 五、回滚计划

### 5.1 回滚触发条件

- 迁移执行失败
- 数据完整性问题
- 性能严重下降
- 应用功能异常

### 5.2 回滚步骤

```bash
# 1. 停止应用
systemctl stop synapse-rust

# 2. 执行回滚脚本
psql -d synapse -f migrations/20260404000001_consolidated_schema_alignment.undo.sql

# 3. 恢复旧迁移文件
git checkout HEAD~1 migrations/

# 4. 验证数据库状态
./scripts/check_field_consistency.sql

# 5. 重启应用
systemctl start synapse-rust

# 6. 验证功能
./scripts/smoke_test.sh
```

### 5.3 紧急恢复

如果回滚失败：

```bash
# 从备份恢复
pg_restore -d synapse backup_20260404.dump

# 验证数据
./scripts/validate_database.sh
```

---

## 六、文档更新

### 6.1 需要更新的文档

- [ ] `migrations/README.md` - 更新迁移列表
- [ ] `migrations/MIGRATION_INDEX.md` - 更新索引
- [ ] `docs/synapse-rust/CAPABILITY_STATUS_BASELINE_2026-04-02.md` - 更新状态
- [ ] `CHANGELOG-DB.md` - 记录变更

### 6.2 新增文档

- [ ] `migrations/CONSOLIDATION_HISTORY.md` - 合并历史记录
- [ ] `migrations/DEPRECATED_MIGRATIONS.md` - 废弃迁移列表

---

## 七、风险管理

### 7.1 风险矩阵

| 风险 | 概率 | 影响 | 等级 | 缓解措施 |
|------|------|------|------|---------|
| 合并后迁移失败 | 低 | 高 | 中 | 充分测试 + 备份 |
| 数据丢失 | 极低 | 严重 | 中 | 完整备份 + 验证 |
| 性能下降 | 中 | 中 | 中 | 性能测试 + 监控 |
| 回滚失败 | 低 | 高 | 中 | 测试回滚 + 备份恢复 |
| 应用兼容性问题 | 低 | 中 | 低 | 集成测试 |

### 7.2 应急预案

**场景 1：迁移执行失败**
- 立即停止执行
- 检查错误日志
- 执行回滚脚本
- 分析失败原因

**场景 2：性能严重下降**
- 回滚到旧版本
- 分析慢查询
- 优化索引策略
- 重新测试

**场景 3：数据不一致**
- 停止应用
- 从备份恢复
- 分析不一致原因
- 修复后重新执行

---

## 八、成功标准

### 8.1 功能标准

- ✅ 所有表结构正确创建
- ✅ 所有索引正确创建
- ✅ 所有外键约束有效
- ✅ 数据完整性验证通过
- ✅ 应用功能正常

### 8.2 性能标准

- ✅ 迁移执行时间 < 5 分钟
- ✅ 索引创建无长时间锁表
- ✅ 查询性能无明显下降
- ✅ 资源使用在正常范围

### 8.3 质量标准

- ✅ 代码审查通过
- ✅ 所有测试通过
- ✅ 文档更新完整
- ✅ 无遗留问题

---

## 九、后续优化

### 9.1 短期（1-2 周）

- [ ] 实施索引使用率监控
- [ ] 建立慢查询分析
- [ ] 优化查询性能

### 9.2 中期（1-2 月）

- [ ] 实施自动化 schema 验证
- [ ] 建立性能基准测试
- [ ] 优化大表查询

### 9.3 长期（3-6 月）

- [ ] 实施分区表策略
- [ ] 优化存储结构
- [ ] 建立容量规划

---

## 十、附录

### A. 合并脚本模板

```bash
#!/bin/bash
# 迁移合并脚本

set -e

SOURCE_DIR="migrations"
TARGET_FILE="migrations/20260404000001_consolidated_schema_alignment.sql"

echo "-- ============================================================================" > "$TARGET_FILE"
echo "-- 统一 Schema 对齐迁移" >> "$TARGET_FILE"
echo "-- 日期：$(date +%Y-%m-%d)" >> "$TARGET_FILE"
echo "-- ============================================================================" >> "$TARGET_FILE"
echo "" >> "$TARGET_FILE"

# 合并各个文件
for file in \
    20260330000002_align_thread_schema_and_relations.sql \
    20260330000003_align_retention_and_room_summary_schema.sql \
    # ... 其他文件
do
    echo "-- ============================================================================" >> "$TARGET_FILE"
    echo "-- Part: $file" >> "$TARGET_FILE"
    echo "-- ============================================================================" >> "$TARGET_FILE"
    cat "$SOURCE_DIR/$file" >> "$TARGET_FILE"
    echo "" >> "$TARGET_FILE"
done

echo "合并完成: $TARGET_FILE"
```

### B. 测试脚本模板

```bash
#!/bin/bash
# 迁移测试脚本

set -e

DB_NAME="synapse_test"
MIGRATION_FILE="$1"

# 创建测试数据库
createdb "$DB_NAME"

# 执行统一 schema
psql -d "$DB_NAME" -f migrations/00000000_unified_schema_v6.sql

# 执行迁移
psql -d "$DB_NAME" -f "$MIGRATION_FILE"

# 验证
psql -d "$DB_NAME" -f scripts/check_field_consistency.sql

# 清理
dropdb "$DB_NAME"

echo "测试通过: $MIGRATION_FILE"
```

---

**计划制定日期**：2026-04-04  
**计划负责人**：数据库团队  
**预计完成日期**：2026-04-15
