# 数据库迁移索引 - Migration Index

> 版本: v4.0.0
> 更新日期: 2026-03-20
> 项目: synapse-rust

---

## 一、迁移文件清单

### 1.1 核心架构文件

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `00000000_unified_schema_v6.sql` | 99KB | ✅ 必需 | 统一 Schema 基线，包含 129 个表定义 |

### 1.2 统一迁移文件 (v4.0.0 新增)

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `UNIFIED_MIGRATION_v1.sql` | ~60KB | ✅ 推荐 | 统一迁移脚本，合并28个增量迁移 |

### 1.3 增量迁移文件

#### 布尔字段重命名 (2026-03-20)

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `20260320000001_rename_must_change_password.sql` | 1KB | ✅ 必需 | 用户表布尔字段重命名 |
| `20260320000002_rename_olm_boolean_fields.sql` | 1KB | ✅ 必需 | Olm模型布尔字段重命名 |

#### 设备信任与安全备份 (2026-03-21)

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `20260321000001_add_device_trust_tables.sql` | 2KB | ✅ 必需 | 设备信任表 |
| `20260321000003_add_secure_backup_tables.sql` | 3KB | ✅ 必需 | 安全备份表 |

### 1.4 已归档迁移文件 (v4.0.0 清理)

以下文件已被 `UNIFIED_MIGRATION_v1.sql` 合并，已删除：

| 原文件名 | 状态 | 合并说明 |
|----------|------|----------|
| `20260309000001_password_security_enhancement.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260310000004_create_federation_signing_keys.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260311000001_add_space_members_table.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260311000004_fix_ip_reputation_table.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260311000006_add_e2ee_tables.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260311000008_fix_key_backups_constraints.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260313000000_create_room_tags_and_password_reset_tokens.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260313000001_qr_login.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260313000002_invite_blocklist.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260313000003_sticky_event.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260314000001_widget_support.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260314000002_add_performance_indexes.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260315000002_create_admin_api_tables.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260315000003_create_feature_tables.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260315000004_fix_typing_columns.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260315000005_fix_push_constraints.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260315000005_fix_room_guest_access.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260315000006_add_events_processed_ts.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260315000006_fix_media_quota_config.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260315000006_fix_room_summaries.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260315000007_fix_media_quota_config_structure.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260315000008_fix_user_media_quota_structure.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260316000000_comprehensive_migration.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260316000001_fix_field_consistency.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260316000002_create_room_summary_state.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260317000000_add_missing_tables.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260317000001_add_verification_tables.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260318000001_add_event_relations.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260318000002_fix_push_module.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |
| `20260319000001_add_application_services.sql` | ❌ 已删除 | 已合并到 UNIFIED_MIGRATION_v1.sql |

### 1.5 归档目录文件

| 文件名 | 状态 | 说明 |
|--------|------|------|
| `archive/20260313000000_unified_migration_optimized.sql` | 📁 已归档 | 历史版本 |
| `archive/20260315000001_fix_field_names.sql` | 📁 已归档 | 历史版本 |
| `archive/20260315000004_fix_field_naming_inconsistencies.sql` | 📁 已归档 | 历史版本 |
| `archive/202603150000_unified_migration.sql` | 📁 已归档 | 历史版本 |

---

## 二、迁移执行顺序

### 2.1 新环境部署 (推荐方式)

```bash
# 1. 执行基础架构
psql -U synapse -d synapse -f migrations/00000000_unified_schema_v6.sql

# 2. 执行统一迁移 (合并了28个增量迁移)
psql -U synapse -d synapse -f migrations/UNIFIED_MIGRATION_v1.sql

# 3. 执行字段重命名迁移
psql -U synapse -d synapse -f migrations/20260320000001_rename_must_change_password.sql
psql -U synapse -d synapse -f migrations/20260320000002_rename_olm_boolean_fields.sql

# 4. 执行新增功能迁移
psql -U synapse -d synapse -f migrations/20260321000001_add_device_trust_tables.sql
psql -U synapse -d synapse -f migrations/20260321000003_add_secure_backup_tables.sql
```

### 2.2 现有环境升级

```bash
# 只执行新增的迁移
psql -U synapse -d synapse -f migrations/20260321000001_add_device_trust_tables.sql
psql -U synapse -d synapse -f migrations/20260321000003_add_secure_backup_tables.sql
```

---

## 三、版本历史

### v4.0.0 (2026-03-20)
- 删除28个已被统一迁移合并的冗余脚本
- 新增 UNIFIED_MIGRATION_v1.sql 统一迁移脚本
- 更新迁移执行顺序为推荐方式
- 清理迁移索引，标记已删除文件

### v3.1.0 (2026-03-14)
- 删除已合并到 unified_schema_v6.sql 的冗余迁移文件
- 清理迁移索引，标记已删除的文件
- 更新执行顺序

### v3.0.0 (2026-03-14)
- 创建完整迁移索引
- 添加统一迁移文件
- 归档重复文件
- 优化执行顺序

### v2.0.0 (2026-03-13)
- 统一 Schema v6
- 字段规范化完成

### v1.0.0 (2026-03-09)
- 初始迁移

---

## 四、回滚方案

### 4.1 统一迁移回滚

由于统一迁移是幂等的，可以选择性回滚特定部分：

```sql
-- 回滚外键约束
ALTER TABLE devices DROP CONSTRAINT IF EXISTS fk_devices_user;

-- 回滚字段修改
ALTER TABLE some_table RENAME COLUMN new_field TO old_field;

-- 回滚索引
DROP INDEX IF EXISTS idx_new_index;
```

### 4.2 完整回滚

```bash
# 恢复到备份
pg_restore -d synapse backup_20260320.dump
```

---

## 五、验证检查

### 5.1 执行后检查

```sql
-- 检查表数量
SELECT COUNT(*) FROM information_schema.tables
WHERE table_schema = 'public' AND table_type = 'BASE TABLE';
-- 预期: 129+

-- 检查外键数量
SELECT COUNT(*) FROM information_schema.table_constraints
WHERE constraint_type = 'FOREIGN KEY';
-- 预期: 95+

-- 检查索引数量
SELECT COUNT(*) FROM pg_indexes WHERE schemaname = 'public';
-- 预期: 200+

-- 检查迁移记录
SELECT * FROM schema_migrations ORDER BY executed_at DESC LIMIT 10;
```

### 5.2 字段检查

```bash
# 运行健康检查
bash scripts/db_health_check.sh
```

---

## 六、相关文档

- [MIGRATION_CONSOLIDATION_PLAN.md](./MIGRATION_CONSOLIDATION_PLAN.md) - 迁移合并计划
- [MIGRATION_IMPLEMENTATION_GUIDE.md](./MIGRATION_IMPLEMENTATION_GUIDE.md) - 实施指南
- [DATABASE_FIELD_STANDARDS.md](./DATABASE_FIELD_STANDARDS.md) - 字段规范
