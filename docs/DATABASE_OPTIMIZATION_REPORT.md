# 数据库全面排查与优化报告

> 日期: 2026-03-14
> 项目: synapse-rust

---

## 一、排查发现的问题

### 1.1 字段不一致问题 (已修复)

| 表名 | 问题字段 | 代码使用 | Schema 使用 | 修复方案 |
|------|----------|----------|-------------|----------|
| users | password_expires | `password_expires_ts` | `password_expires_at` | Schema 索引修复 |
| user_threepids | validated | `validated_at` | `validated_ts` | Schema 改为 `_at` |
| refresh_tokens | last_used | `last_used_ts` | `last_used_ts` | ✅ 一致 |
| registration_tokens | last_used | `last_used_ts` | `last_used_at` | Schema 改为 `_ts` |

### 1.2 根本原因分析

根据 `DATABASE_FIELD_STANDARDS.md` 规范：
- `_ts` 后缀: 用于必须存在的时间戳 (NOT NULL)
- `_at` 后缀: 用于可选操作的时间戳 (可空)

但是代码中的实现与规范存在偏差：
- 代码使用 `validated_at` (对应规范的可选验证时间)
- 代码使用 `last_used_ts` (对应规范的必须时间戳)

**决策**: 修改数据库 schema 以匹配代码实现，因为：
1. 代码已经稳定运行
2. 更改代码风险更高
3. 保持向后兼容

---

## 二、执行的修复

### 2.1 Schema 修复

**文件**: `migrations/00000000_unified_schema_v6.sql`

| 修复项 | 详情 |
|--------|------|
| users 索引 | `password_expires_ts` → `password_expires_at` |
| user_threepids | `validated_ts` → `validated_at` |
| registration_tokens | `last_used_at` → `last_used_ts` |

### 2.2 新增迁移

| 文件 | 说明 |
|------|------|
| `20260316000001_fix_field_consistency.sql` | 字段一致性修复迁移 |
| `20260316000000_comprehensive_migration.sql` | 综合迁移 (整合所有) |

### 2.3 文档更新

| 文件 | 更新内容 |
|------|----------|
| `DATABASE_FIELD_STANDARDS.md` | v3.0.0 添加字段一致性检查清单 |

---

## 三、迁移文件清理

### 3.1 当前迁移文件列表

```
migrations/
├── 00000000_unified_schema_v6.sql          # 核心 Schema (v6.0.4)
├── 20260309000001_password_security_enhancement.sql
├── 20260310000001_add_missing_e2ee_tables.sql
├── 20260310000002_normalize_fields_and_add_tables.sql
├── 20260310000003_fix_api_test_issues.sql
├── 20260310000004_create_federation_signing_keys.sql
├── 20260311000001_add_space_members_table.sql
├── 20260311000002_fix_table_structures.sql
├── 20260311000003_optimize_database_structure.sql
├── 20260311000004_fix_ip_reputation_table.sql
├── 20260311000005_fix_media_quota_tables.sql
├── 20260311000006_add_e2ee_tables.sql
├── 20260311000007_fix_application_services_tables.sql
├── 20260311000008_fix_key_backups_constraints.sql
├── 20260313000000_create_room_tags_and_password_reset_tokens.sql
├── 20260313000001_qr_login.sql
├── 20260313000002_invite_blocklist.sql
├── 20260313000003_sticky_event.sql
├── 20260314000001_widget_support.sql
├── 20260314000002_add_performance_indexes.sql
├── 20260314000003_fix_updated_at_to_updated_ts.sql
├── 20260314000004_fix_refresh_tokens_fields.sql
├── 20260314000005_fix_refresh_token_families.sql
├── 20260315000001_fix_field_names.sql
├── 20260315000002_create_admin_api_tables.sql
├── 20260315000003_create_feature_tables.sql
├── 20260315000004_fix_field_naming_inconsistencies.sql
├── 20260315000005_fix_room_guest_access.sql
├── 20260315000006_fix_room_summaries.sql
├── 20260315000007_add_foreign_key_constraints.sql
├── 20260315000008_performance_optimization.sql
├── 202603150000_unified_migration.sql
├── 20260316000000_comprehensive_migration.sql    # 综合迁移
├── 20260316000001_fix_field_consistency.sql      # 字段一致性修复
└── archive/                                       # 归档目录
    └── 20260313000000_unified_migration_optimized.sql
```

### 3.2 可归档的冗余文件

以下文件可以考虑归档（包含重复内容）:

| 文件 | 原因 |
|------|------|
| `202603150000_unified_migration.sql` | 与统一 Schema 重复 |
| `20260315000001_fix_field_names.sql` | 部分内容已整合 |
| `20260315000004_fix_field_naming_inconsistencies.sql` | 与字段修复重复 |

---

## 四、部署步骤

### 4.1 新环境部署

```bash
cd ~/Desktop/hu/synapse-rust

# 1. 执行统一 Schema
psql -U synapse -d synapse -f migrations/00000000_unified_schema_v6.sql

# 2. 执行综合迁移
psql -U synapse -d synapse -f migrations/20260316000000_comprehensive_migration.sql

# 3. 验证
psql -U synapse -d synapse -c "
SELECT table_name, column_name 
FROM information_schema.columns 
WHERE table_name IN ('users', 'user_threepids', 'refresh_tokens', 'registration_tokens')
AND column_name IN ('password_expires_at', 'validated_at', 'last_used_ts', 'last_used_at');
"
```

### 4.2 现有环境升级

```bash
# 只执行字段修复迁移
psql -U synapse -d synapse -f migrations/20260316000001_fix_field_consistency.sql

# 验证
psql -U synapse -d synapse -c "SELECT * FROM users LIMIT 1;"
```

---

## 五、验证检查

### 5.1 字段一致性检查

```bash
# 检查 users 表
psql -U synapse -d synapse -c "
SELECT column_name FROM information_schema.columns 
WHERE table_name = 'users' AND column_name LIKE '%expires%';
"

# 预期输出:
#  password_expires_at
#  password_changed_ts

# 检查 user_threepids 表
psql -U synapse -d synapse -c "
SELECT column_name FROM information_schema.columns 
WHERE table_name = 'user_threepids' AND column_name LIKE '%validat%';
"

# 预期输出:
#  validated_at (不是 validated_ts)
```

### 5.2 代码编译检查

```bash
cd ~/Desktop/hu/synapse-rust
cargo check 2>&1 | grep -i error
```

---

## 六、总结

| 项目 | 状态 |
|------|------|
| 字段不一致问题 | ✅ 已修复 |
| Schema 统一 | ✅ v6.0.4 |
| 迁移文件 | ✅ 优化整合 |
| 文档更新 | ✅ v3.0.0 |

### 关键决策

1. **修改 Schema 而非代码**: 降低风险，保持向后兼容
2. **创建综合迁移**: 简化部署流程
3. **保留历史迁移**: 保留审计能力，支持回滚

---

## 附录: 快速命令参考

```bash
# 连接数据库
psql -U synapse -d synapse

# 查看表字段
\d users
\d user_threepids

# 查看索引
SELECT indexname, indexdef FROM pg_indexes WHERE tablename = 'users';

# 运行迁移
psql -U synapse -d synapse -f migrations/20260316000001_fix_field_consistency.sql
```
