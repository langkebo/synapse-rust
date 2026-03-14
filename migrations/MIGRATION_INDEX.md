# synapse-rust 迁移索引

> 版本: v2.0.0
> 更新日期: 2026-03-13
> 说明: 记录所有迁移文件的执行顺序和内容

---

## 一、迁移文件列表

### 1.1 核心架构文件

| 文件名 | 描述 | 稡块 | 状态 |
|------|------|------|------|
| `00000000_unified_schema_v6.sql` | 统一架构 v6 | 核心 | ✅ 已应用 |
| `DATABASE_FIELD_STANDARDS.md` | 字段标准文档 | 规范 | ✅ 已应用 |
| `SCHEMA_OPTIMIZATION_REPORT.md` | 优化报告 | 文档 | ✅ 已应用 |

| `README.md` | 迁移说明 | 文档 | ✅ 已应用 |

### 1.2 增量迁移文件（按时间顺序）

| 文件名 | 描述 | 模块 | 状态 |
|------|------|------|------|
| `20260309000001_password_security_enhancement.sql` | 密码安全增强 | 安全 | ✅ 已应用 |
| `20260310000001_add_missing_e2ee_tables.sql` | E2EE 表补充 | E2EE | ✅ 已应用 |
| `20260310000002_normalize_fields_and_add_tables.sql` | 字段规范化和表补充 | 核心 | ✅ 已应用 |
| `20260310000003_fix_api_test_issues.sql` | API 测试问题修复 | 测试 | ✅ 已应用 |
| `20260310000004_create_federation_signing_keys.sql` | 联邦签名密钥 | 联邦 | ✅ 已应用 |
| `20260311000001_add_space_members_table.sql` | Space 成员表 | Space | ✅ 已应用 |
| `20260311000002_fix_table_structures.sql` | 表结构修复 | 核心 | ✅ 已应用 |
| `20260311000003_optimize_database_structure.sql` | 数据库结构优化 | 性能 | ✅ 已应用 |
| `20260311000004_fix_ip_reputation_table.sql` | IP 信誉表修复 | 安全 | ✅ 已应用 |
| `20260311000005_fix_media_quota_tables.sql` | 媒体配额表修复 | 媒体 | ✅ 已应用 |
| `20260311000006_add_e2ee_tables.sql` | E2EE 表补充 | E2EE | ✅ 已应用 |
| `20260311000007_fix_application_services_tables.sql` | 应用服务表修复 | 服务 | ✅ 已应用 |
| `20260311000008_fix_key_backups_constraints.sql` | 密钥备份约束修复 | E2EE | ✅ 已应用 |
| `20260313000000_unified_migration_optimized.sql` | **统一优化迁移** | 综合 | 🆕 新增 |

---

## 二、迁移执行顺序

### 2.1 新环境初始化

```bash
# 1. 执行核心架构
psql -U synapse -d synapse -f migrations/00000000_unified_schema_v6.sql

# 2. 执行增量迁移（按时间顺序）
for f in migrations/202603*.sql; do
    psql -U synapse -d synapse -f "$f"
done
```

### 2.2 已有环境升级

```bash
# 只执行新的迁移文件
psql -U synapse -d synapse -f migrations/20260313000000_unified_migration_optimized.sql
```

---

## 三、迁移内容说明

### 3.1 统一优化迁移 (20260313000000)

**包含内容**:
- P1 高优先级表: presence_subscriptions, call_sessions, qr_login_transactions 等
- P2 中优先级表: background_update_*, federation_blacklist_*, retention_*, notification_*, push_device 等
- P3 低优先级表: beacon_*, dehydrated_devices, email_verification 等
- 索引优化: presence_subscriptions 复合索引
- 字段规范化: created_at → created_ts, updated_at → updated_ts, expires_ts → expires_at, revoked_ts → revoked_at

- 合并的迁移文件:
  - 20260312000001_add_missing_p1_tables.sql
  - 20260312000001_presence_subscriptions.sql
  - 20260312000002_add_missing_p2_tables.sql
  - 20260312000002_call_sessions.sql
  - 20260312000003_add_missing_p3_tables.sql
  - 20260312000004_add_missing_indexes.sql
  - 20260312000004_fix_timestamp_field_names.sql
  - 20260312000005_qr_login.sql
  - 20260312000006_invite_blocklist.sql
  - 20260312000007_sticky_event.sql
| `20260313000008_field_name_fix.sql` | 字段名修复 | 核心 | ✅ 已应用 |
| `20260313000008_unified_schema_field_fix.sql` | 统一字段修复 | 核心 | ✅ 已应用 |
| `20260314000001_widget_support.sql` | Widget 支持 | Widget | ✅ 已应用 |
| `20260314000002_add_performance_indexes.sql` | 性能索引 | 性能 | ✅ 已应用 |
| `20260314000003_fix_updated_at_to_updated_ts.sql` | updated_at → updated_ts | 规范 | ✅ 新增 |

---

## 四、字段命名规范
### 4.1 时间戳字段
| 后缀 | 数据类型 | 可空性 | 说明 |
|------|----------|--------|------|
| `_ts` | BIGINT | NOT NULL | 创建/更新/活跃时间 |
| `_at` | BIGINT | NULLABLE | 过期/撤销/验证等可选操作时间 |

### 4.2 字段映射
| 旧字段名 | 新字段名 | 说明 |
|----------|----------|------|
| `created_at` | `created_ts` | 创建时间 |
| `updated_at` | `updated_ts` | 更新时间 |
| `expires_ts` | `expires_at` | 过期时间（可选） |
| `revoked_ts` | `revoked_at` | 撤销时间（可选） |
| `validated_ts` | `validated_at` | 验证时间（可选） |

---

## 五、验证清单
### 5.1 迁移前验证
- [ ] 备份数据库
- [ ] 检查磁盘空间
- [ ] 确认数据库连接

### 5.2 迁移后验证
- [ ] 验证表结构
- [ ] 验证索引创建
- [ ] 验证字段命名
- [ ] 运行测试套件
