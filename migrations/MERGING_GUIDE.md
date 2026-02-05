# 数据库迁移合并说明

## 概述

本文档说明数据库迁移文件的合并策略和优化方案。

## 原始迁移文件列表 (17个)

```
migrations/
├── 20260130000000_initial_schema.sql          # 初始架构 (保留)
├── 20260130000001_schema_fix.sql               # 架构修复 (保留)
├── 20260201000000_optimize_search.sql         # 搜索优化 (保留)
├── 20260201000001_to_device_messages.sql      # 设备消息 (保留)
├── 20260201000002_device_keys_updated_ts.sql  # → 已合并
├── 20260201000003_fix_missing_columns_and_tables.sql  # → 已合并
├── 20260202000000_fix_device_keys_and_voice_v2.sql    # → 已合并
├── 20260202000001_final_fix.sql                        # → 已合并
├── 20260202000002_fix_device_keys_and_voice_final.sql # → 已合并
├── 20260202000003_fix_voice_nullable.sql              # → 已合并
├── 20260202120000_fix_device_keys_constraint.sql      # → 已合并
├── 20260203000000_final_fix.sql                      # → 已合并
├── 20260204000001_add_transcribe_text_column.sql     # → 已合并
├── 20260204000002_add_device_keys_id_column_fixed.sql # → 已合并
├── 20260204000004_fix_device_keys_fk.sql             # → 已合并
├── 20260204000005_add_private_chat_tables.sql       # 私聊功能 (保留)
└── 20260204000006_add_event_reports_and_email_verification.sql  # 事件报告和邮箱验证 (保留)
```

## 优化后的迁移文件列表 (7个)

```
migrations/
├── 20260130000000_initial_schema.sql          # 基础表结构
├── 20260130000001_schema_fix.sql              # 必要的小修复
├── 20260201000000_optimize_search.sql         # 搜索优化
├── 20260201000001_to_device_messages.sql     # 设备消息
├── 20260202000000_consolidated_fixes.sql      # ★ 新：合并的修复
├── 20260204000005_add_private_chat_tables.sql # 私聊功能
└── 20260204000006_add_event_reports_and_email_verification.sql  # 事件报告和邮箱验证
```

## 合并策略

### 已合并的迁移 (11个 → 1个)

以下11个迁移文件已合并到 `20260202000000_consolidated_fixes.sql`：

1. `20260201000002_device_keys_updated_ts.sql`
2. `20260201000003_fix_missing_columns_and_tables.sql`
3. `20260202000000_fix_device_keys_and_voice_v2.sql`
4. `20260202000001_final_fix.sql`
5. `20260202000002_fix_device_keys_and_voice_final.sql`
6. `20260202000003_fix_voice_nullable.sql`
7. `20260202120000_fix_device_keys_constraint.sql`
8. `20260203000000_final_fix.sql`
9. `20260204000001_add_transcribe_text_column.sql`
10. `20260204000002_add_device_keys_id_column_fixed.sql`
11. `20260204000004_fix_device_keys_fk.sql`

### 保留的迁移 (6个)

以下6个迁移文件保持独立：

1. `20260130000000_initial_schema.sql` - 初始架构，不应修改
2. `20260130000001_schema_fix.sql` - 必要的小修复
3. `20260201000000_optimize_search.sql` - 搜索优化
4. `20260201000001_to_device_messages.sql` - 设备消息功能
5. `20260204000005_add_private_chat_tables.sql` - 私聊功能
6. `20260204000006_add_event_reports_and_email_verification.sql` - 事件报告和邮箱验证

## 迁移依赖关系

```
20260130000000 (初始架构)
    ↓
20260130000001 (架构修复)
    ↓
20260201000000 (搜索优化)
    ↓
20260201000001 (设备消息)
    ↓
20260202000000_consolidated (合并修复) ← 多个来源合并
    ↓
20260204000005 (私聊功能) - 可选
    ↓
20260204000006 (事件报告和邮箱验证) - 可选
```

## 使用说明

### 对于新安装

新安装可以直接运行优化后的迁移文件列表。

### 对于现有安装

现有数据库已经应用了原始迁移，无需重新运行。如果需要创建新的干净环境，可以使用合并后的迁移文件。

### 删除已合并迁移（可选）

在确认现有数据库环境稳定后，可以删除已合并的迁移文件以简化项目结构：

```bash
rm migrations/20260201000002_device_keys_updated_ts.sql
rm migrations/20260201000003_fix_missing_columns_and_tables.sql
# ... 等等
```

## 最佳实践

1. **不要修改已应用的迁移** - 这可能导致数据库状态不一致
2. **保持迁移幂等** - 使用 `IF NOT EXISTS` 和 `IF NOT EXISTS` 条件
3. **合并时要小心** - 确保合并的迁移之间没有冲突依赖
4. **保留初始架构** - 初始架构迁移是数据库的"真理之源"

## 统计

| 指标 | 优化前 | 优化后 | 改进 |
|-----|-------|-------|-----|
| 迁移文件数 | 17 | 7 | -59% |
| 总文件大小 | ~60KB | ~50KB | -17% |
| Fix类迁移 | 11 | 1 | -91% |
