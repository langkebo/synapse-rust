# 数据库迁移优化方案

## 执行时间: 2026-02-11

## 当前状态分析

### 存在的问题

1. **时间戳冲突**: `20260206000001` 被两个文件使用
2. **危险操作**: 多个文件使用 `DROP TABLE CASCADE` 会删除数据
3. **Schema 分散**: 3 个文件定义相同的表结构
4. **重复修复**: voice_messages 表被修复两次

### 当前文件 (11个)
```
20260206000000_master_unified_schema.sql    # 主Schema
20260206000001_fix_device_keys_table.sql     # ❌ 冲突 + DROP/CREATE
20260206000001_fix_e2ee_tables.sql           # ❌ 冲突 + DROP/CREATE
20260206000002_fix_e2ee_foreign_keys.sql     # ✅ 保留（使用ALTER）
20260206000003_fix_voice_messages_table.sql  # ⚠️ 改为ALTER
20260206000004_fix_voice_usage_stats_room_id.sql  # ✅ 保留
20260208000001_fix_voice_messages_room_id_nullable.sql  # ❌ 与00003重复
20260208000002_drop_private_chat_tables.sql   # ✅ 保留
20260208100000_init_schema.sql                # ❌ 与master重复
20260209100000_add_performance_indexes.sql     # ✅ 保留
20260209110000_fix_schema_consistency.sql     # ⚠️ 检查冲突
```

## 优化方案

### 优化后的文件 (7个)

```
migrations/
├── 20260206000000_master_unified_schema.sql         # 主Schema（完整表定义）
├── 20260206000002_add_e2ee_constraints.sql           # E2EE外键修复（仅ALTER）
├── 20260206000003_alter_voice_messages.sql           # 语音消息修复（仅ALTER）
├── 20260206000004_alter_voice_usage_stats.sql         # 语音统计修复
├── 20260208000002_drop_private_chat_tables.sql       # 删除私聊旧表
├── 20260209100000_add_performance_indexes.sql         # 性能索引
├── 20260211000001_migrate_friends_to_rooms.sql       # 迁移好友到房间（新增）
```

### 需要删除的文件 (5个)

```bash
rm migrations/20260206000001_fix_device_keys_table.sql
rm migrations/20260206000001_fix_e2ee_tables.sql
rm migrations/20260208000001_fix_voice_messages_room_id_nullable.sql
rm migrations/20260208100000_init_schema.sql
rm migrations/20260209110000_fix_schema_consistency.sql  # 合并到master或创建新ALTER迁移
```

### 需要修改的文件 (2个)

1. **20260206000003_fix_voice_messages_table.sql**
   - 改为使用 `ALTER TABLE` 而不是 `DROP/CREATE`
   - 重命名为 `20260206000003_alter_voice_messages.sql`

2. **20260206000002_fix_e2ee_foreign_keys.sql**
   - 确认只使用 `ALTER TABLE`，改为 `20260206000002_add_e2ee_constraints.sql`

## 迁移依赖关系

```
20260206000000 (主Schema)
    ↓
20260206000002 (E2EE外键)
    ↓
20260206000003 (语音消息修复)
    ↓
20260206000004 (语音统计修复)
    ↓
20260208000002 (删除私聊表)
    ↓
20260209100000 (性能索引)
    ↓
20260211000001 (好友系统迁移到房间)
```

## 执行步骤

1. 创建新的 ALTER 类型迁移文件
2. 删除冗余和冲突的迁移文件
3. 验证新的迁移顺序
4. 更新 .sqlx 文件（如果使用 sqlx-cli）
5. 测试数据库迁移

## 风险评估

| 风险 | 级别 | 缓解措施 |
|------|------|----------|
| 数据丢失 | 高 | 不使用 DROP TABLE，只使用 ALTER TABLE |
| 迁移失败 | 中 | 在测试环境先验证 |
| 外键约束错误 | 中 | 先删除约束再添加 |
| 性能回归 | 低 | 保留所有索引 |

## 预期效果

| 指标 | 优化前 | 优化后 | 改进 |
|------|-------|-------|------|
| 迁移文件数 | 11 | 6 | -45% |
| DROP TABLE 操作 | 5 | 0 | -100% |
| Schema 定义位置 | 3 | 1 | -67% |
| 潜在数据丢失点 | 5 | 0 | -100% |
