# 数据库迁移索引 - Migration Index

> 版本: v3.1.0
> 更新日期: 2026-03-14
> 项目: synapse-rust

---

## 一、迁移文件清单

### 1.1 核心架构文件

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `00000000_unified_schema_v6.sql` | 99KB | ✅ 必需 | 统一 Schema 基线，包含 129 个表定义 |

### 1.2 增量迁移文件

#### 2026-03-09

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `20260309000001_password_security_enhancement.sql` | 4KB | ✅ 必需 | 密码安全增强 |

#### 2026-03-10

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `20260310000001_add_missing_e2ee_tables.sql` | - | ❌ 已删除 | 功能已合并到 unified_schema_v6.sql |
| `20260310000002_normalize_fields_and_add_tables.sql` | - | ❌ 不存在 | 已移除 |
| `20260310000003_fix_api_test_issues.sql` | - | ❌ 已删除 | 功能已合并到 unified_schema_v6.sql |
| `20260310000004_create_federation_signing_keys.sql` | 1KB | ✅ 必需 | 联邦签名密钥 |

#### 2026-03-11

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `20260311000001_add_space_members_table.sql` | 2KB | ✅ 必需 | Space 成员表 |
| `20260311000002_fix_table_structures.sql` | - | ❌ 已删除 | 功能已合并到 unified_schema_v6.sql |
| `20260311000003_optimize_database_structure.sql` | - | ❌ 已删除 | 功能已合并到 unified_schema_v6.sql |
| `20260311000004_fix_ip_reputation_table.sql` | 2KB | ✅ 必需 | IP 信誉表修复 |
| `20260311000005_fix_media_quota_tables.sql` | - | ❌ 已删除 | 功能已合并到 unified_schema_v6.sql |
| `20260311000006_add_e2ee_tables.sql` | 6KB | ✅ 必需 | E2EE 表补充 |
| `20260311000007_fix_application_services_tables.sql` | - | ❌ 已删除 | 功能已合并到 unified_schema_v6.sql |
| `20260311000008_fix_key_backups_constraints.sql` | 2KB | ✅ 必需 | 密钥备份约束 |

#### 2026-03-13

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `20260313000000_create_room_tags_and_password_reset_tokens.sql` | 2KB | ✅ 必需 | 房间标签/密码重置 |
| `20260313000001_qr_login.sql` | 2KB | ✅ 必需 | QR 登录 |
| `20260313000002_invite_blocklist.sql` | 1KB | ✅ 必需 | 邀请黑名单 |
| `20260313000003_sticky_event.sql` | 1KB | ✅ 必需 | 粘性事件 |

#### 2026-03-14

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `20260314000001_widget_support.sql` | 2KB | ✅ 必需 | Widget 支持 |
| `20260314000002_add_performance_indexes.sql` | 3KB | ✅ 必需 | 性能索引 |
| `20260314000003_fix_updated_at_to_updated_ts.sql` | - | ❌ 已删除 | 功能已合并到 unified_schema_v6.sql |
| `20260314000004_fix_refresh_tokens_fields.sql` | - | ❌ 已删除 | 功能已合并到 unified_schema_v6.sql |
| `20260314000005_fix_refresh_token_families.sql` | - | ❌ 已删除 | 功能已合并到 unified_schema_v6.sql |

#### 2026-03-15

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `20260315000001_fix_field_names.sql` | 1KB | 📁 已归档 | 字段名修复 |
| `20260315000002_create_admin_api_tables.sql` | 3KB | ✅ 必需 | Admin API 表 |
| `20260315000003_create_feature_tables.sql` | 5KB | ✅ 必需 | 功能模块表 |
| `20260315000004_fix_field_naming_inconsistencies.sql` | 3KB | 📁 已归档 | 字段命名一致性 |
| `20260315000005_fix_room_guest_access.sql` | 1KB | ✅ 必需 | 房间访客访问 |
| `20260315000006_fix_room_summaries.sql` | 3KB | ✅ 必需 | 房间摘要修复 |
| `20260315000007_add_foreign_key_constraints.sql` | - | ❌ 已删除 | 功能已合并到 unified_schema_v6.sql |
| `20260315000008_performance_optimization.sql` | - | ❌ 已删除 | 功能已合并到 unified_schema_v6.sql |

#### 2026-03-16

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `20260316000000_comprehensive_migration.sql` | 4KB | ✅ 必需 | 综合字段修复 |
| `20260316000001_fix_field_consistency.sql` | 4KB | ✅ 必需 | 字段一致性修复 |

### 1.3 统一迁移文件

| 文件名 | 大小 | 状态 | 说明 |
|--------|------|------|------|
| `202603150000_unified_migration.sql` | 15KB | 🆕 新建 | 合并统一迁移 |

### 1.4 归档文件

| 文件名 | 状态 | 说明 |
|--------|------|------|
| `archive/20260313000000_unified_migration_optimized.sql` | 📁 已归档 | 重复文件 |

---

## 二、迁移执行顺序

### 2.1 新环境部署

```bash
# 1. 执行基础架构 (包含所有核心表定义)
psql -U synapse -d synapse -f migrations/00000000_unified_schema_v6.sql

# 2. 按时间顺序执行增量迁移 (仅执行存在的文件)
# 2026-03-09
psql -U synapse -d synapse -f migrations/20260309000001_password_security_enhancement.sql

# 2026-03-10
psql -U synapse -d synapse -f migrations/20260310000004_create_federation_signing_keys.sql

# 2026-03-11
psql -U synapse -d synapse -f migrations/20260311000001_add_space_members_table.sql
psql -U synapse -d synapse -f migrations/20260311000004_fix_ip_reputation_table.sql
psql -U synapse -d synapse -f migrations/20260311000006_add_e2ee_tables.sql
psql -U synapse -d synapse -f migrations/20260311000008_fix_key_backups_constraints.sql

# 2026-03-13
psql -U synapse -d synapse -f migrations/20260313000000_create_room_tags_and_password_reset_tokens.sql
psql -U synapse -d synapse -f migrations/20260313000001_qr_login.sql
psql -U synapse -d synapse -f migrations/20260313000002_invite_blocklist.sql
psql -U synapse -d synapse -f migrations/20260313000003_sticky_event.sql

# 2026-03-14
psql -U synapse -d synapse -f migrations/20260314000001_widget_support.sql
psql -U synapse -d synapse -f migrations/20260314000002_add_performance_indexes.sql

# 2026-03-15
psql -U synapse -d synapse -f migrations/20260315000002_create_admin_api_tables.sql
psql -U synapse -d synapse -f migrations/20260315000003_create_feature_tables.sql
psql -U synapse -d synapse -f migrations/20260315000005_fix_room_guest_access.sql
psql -U synapse -d synapse -f migrations/20260315000006_fix_room_summaries.sql

# 2026-03-16 (字段一致性修复)
psql -U synapse -d synapse -f migrations/20260316000000_comprehensive_migration.sql
psql -U synapse -d synapse -f migrations/20260316000001_fix_field_consistency.sql
```

### 2.2 现有环境升级

```bash
# 只执行新增的迁移
psql -U synapse -d synapse -f migrations/20260316000001_fix_field_consistency.sql
```

---

## 三、版本历史

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

### 4.1 单个迁移回滚

每个迁移文件都有对应的回滚逻辑：

```sql
-- 示例：回滚外键约束
ALTER TABLE devices DROP CONSTRAINT IF EXISTS fk_devices_user;

-- 示例：回滚字段修改
ALTER TABLE some_table RENAME COLUMN new_field TO old_field;

-- 示例：回滚索引
DROP INDEX IF EXISTS idx_new_index;
```

### 4.2 完整回滚

```bash
# 恢复到备份
pg_restore -d synapse backup_20260314.dump
```

---

## 五、验证检查

### 5.1 执行后检查

```sql
-- 检查表数量
SELECT COUNT(*) FROM information_schema.tables 
WHERE table_schema = 'public' AND table_type = 'BASE TABLE';
-- 预期: 129

-- 检查外键数量
SELECT COUNT(*) FROM information_schema.table_constraints 
WHERE constraint_type = 'FOREIGN KEY';
-- 预期: 95+

-- 检查索引数量
SELECT COUNT(*) FROM pg_indexes WHERE schemaname = 'public';
-- 预期: 200+
```

### 5.2 字段检查

```bash
# 运行健康检查
bash scripts/db_health_check.sh
```
