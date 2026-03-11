# 数据库迁移历史

> 版本: 6.0.0
> 更新日期: 2026-03-13
> 状态: 已归档

## 概述

本文档记录 synapse-rust 项目数据库迁移的完整历史，包括所有已归档的迁移文件和当前使用的统一 Schema。

---

## 迁移版本时间线

### 第一阶段: 基础架构 (v1-v4)

早期版本使用分散的迁移文件管理数据库架构，存在以下问题：
- 迁移文件数量多，维护困难
- 字段命名不一致
- 外键约束不完整
- 索引优化分散

### 第二阶段: 统一架构 v5 (2026-03-01)

**文件**: `00000000_unified_schema_v5.sql`

首次尝试统一数据库架构，整合了核心表定义，但仍有以下问题：
- 部分字段命名不规范
- 缺少新功能表定义
- 测试覆盖不完整

### 第三阶段: 增量迁移 (2026-03-01 ~ 2026-03-13)

在此期间，创建了 **37 个增量迁移文件** 来修复和扩展架构：

#### 2026-03-01
| 文件 | 主要变更 |
|------|----------|
| `20260301_add_notifications_ts_column.sql` | 添加通知时间戳列 |

#### 2026-03-02
| 文件 | 主要变更 |
|------|----------|
| `20260302000002_add_retention_and_space_tables.sql` | 消息保留策略表、Space 相关表 |
| `20260302000003_add_media_quota_and_notification_tables.sql` | 媒体配额表、服务器通知表 |

#### 2026-03-05
| 文件 | 主要变更 |
|------|----------|
| `20260305000001_fix_events_and_account_data_dependencies.sql` | 修复事件和账户数据依赖 |
| `20260305000002_align_openid_tokens_schema.sql` | 对齐 OpenID Tokens Schema |
| `20260305000004_fix_appservice_schema_compat.sql` | 修复应用服务 Schema 兼容性 |
| `20260305000005_legacy_appservice_runtime_compat.sql` | 遗留应用服务运行时兼容 |
| `20260305000006_standardize_appservice_fields_phase1.sql` | 应用服务字段标准化第一阶段 |
| `20260305000007_appservice_runtime_compat_guard.sql` | 应用服务运行时兼容守护 |

#### 2026-03-07
| 文件 | 主要变更 |
|------|----------|
| `20260307000001_add_matrixrtc_tables.sql` | MatrixRTC 表 |
| `20260307000001_fix_field_names_to_match_standards.sql` | 字段命名标准化 |
| `20260307000002_add_missing_feature_tables.sql` | 缺失功能表 |
| `20260307000003_add_beacon_tables.sql` | Beacon 表 |
| `20260307000003_fix_schema_sync.sql` | Schema 同步修复 |

#### 2026-03-08
| 文件 | 主要变更 |
|------|----------|
| `20260308000000_fix_member_count_type.sql` | 修复成员计数类型 |
| `20260308000001_fix_field_naming_inconsistencies.sql` | 字段命名不一致修复 |
| `20260308000002_add_missing_foreign_key_constraints.sql` | 添加缺失的外键约束 |
| `20260308000003_optimize_database_indexes.sql` | 数据库索引优化 |
| `20260308000004_data_isolation_triggers.sql` | 数据隔离触发器 |
| `20260308000005_fix_test_failures.sql` | 测试失败修复 |
| `20260308000006_add_reactions.sql` | Reactions 表 |
| `20260308000007_add_push_gateway.sql` | Push Gateway 表 |

#### 2026-03-09
| 文件 | 主要变更 |
|------|----------|
| `20260309000000_fix_test_failures.sql` | 测试失败修复 |
| `20260309000001_add_missing_columns_and_tables.sql` | 缺失列和表 |

#### 2026-03-10
| 文件 | 主要变更 |
|------|----------|
| `20260310000001_add_presence_list_and_3pid_tokens.sql` | Presence 列表和 3PID Tokens |

#### 2026-03-12
| 文件 | 主要变更 |
|------|----------|
| `20260312000000_field_standardization_cleanup.sql` | 字段标准化清理 |
| `20260312000001_consolidated_test_fixes.sql` | 综合测试修复 |
| `20260312000003_comprehensive_fix.sql` | 全面 Schema 修复 |
| `20260312000004_final_schema_fix.sql` | 最终 Schema 修复 |

#### 2026-03-13
| 文件 | 主要变更 |
|------|----------|
| `20260313000001_add_sliding_sync_rooms_table.sql` | Sliding Sync Rooms 表 |
| `20260313000002_fix_federation_signing_keys.sql` | 联邦签名密钥修复 |
| `20260313000003_fix_sync_stream_id_type.sql` | 同步流 ID 类型修复 |
| `20260313000004_add_thread_subscriptions.sql` | Thread 订阅表 |
| `20260313000005_add_space_children.sql` | Space Children 表 |
| `20260313000006_add_space_hierarchy.sql` | Space Hierarchy 表 |

### 第四阶段: 统一架构 v6 (2026-03-13)

**文件**: `00000000_unified_schema_v6.sql`

将所有 37 个增量迁移合并为单一统一 Schema 文件，解决以下问题：
- 简化数据库初始化流程
- 消除迁移顺序依赖
- 统一字段命名规范
- 完整的外键约束
- 优化的索引策略

---

## 为什么合并到统一 Schema

### 问题

1. **迁移文件过多**: 37 个增量迁移文件难以维护
2. **执行顺序复杂**: 需要严格按照时间戳顺序执行
3. **依赖关系混乱**: 迁移之间存在隐式依赖
4. **测试困难**: 每个迁移都需要单独测试
5. **新环境初始化慢**: 需要逐个执行所有迁移

### 解决方案

将所有迁移合并为单一 `00000000_unified_schema_v6.sql` 文件：

1. **一键初始化**: 新环境只需执行一个文件
2. **无顺序依赖**: 所有定义在同一个文件中
3. **易于维护**: 修改 Schema 只需更新一个文件
4. **测试简化**: 只需测试最终 Schema
5. **版本清晰**: 统一版本号管理

---

## 如何使用新的统一 Schema

### 新环境初始化

```bash
# 设置数据库连接
export DATABASE_URL="postgres://synapse:synapse@localhost:5432/synapse"

# 创建数据库
createdb -U synapse synapse

# 执行统一 Schema
psql -U synapse -d synapse -f migrations/00000000_unified_schema_v6.sql
```

### 从旧版本迁移

如果已有运行中的数据库，建议：

1. **备份数据**
   ```bash
   pg_dump -U synapse synapse > backup_$(date +%Y%m%d).sql
   ```

2. **检查兼容性**
   ```bash
   # 检查字段命名是否一致
   ./scripts/check_schema_compat.sh
   ```

3. **执行迁移**
   ```bash
   # 如果是全新环境，直接使用 v6
   psql -U synapse -d synapse -f migrations/00000000_unified_schema_v6.sql
   
   # 如果是升级环境，使用迁移脚本
   ./scripts/migrate_to_v6.sh
   ```

### 验证 Schema

```bash
# 检查表数量
psql -U synapse -d synapse -c "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public';"

# 检查字段规范
psql -U synapse -d synapse -c "SELECT table_name, column_name FROM information_schema.columns WHERE column_name LIKE '%_at' AND data_type != 'bigint';"
```

---

## 归档文件位置

所有旧迁移文件已归档至 `migrations/archive/` 目录：

```
migrations/
├── archive/                              # 归档目录
│   ├── 00000000_unified_schema_v5.sql    # v5 统一架构
│   ├── 20260301_*.sql                    # 2026-03-01 迁移
│   ├── 20260302*.sql                     # 2026-03-02 迁移
│   ├── 20260305*.sql                     # 2026-03-05 迁移
│   ├── 20260307*.sql                     # 2026-03-07 迁移
│   ├── 20260308*.sql                     # 2026-03-08 迁移
│   ├── 20260309*.sql                     # 2026-03-09 迁移
│   ├── 20260310*.sql                     # 2026-03-10 迁移
│   ├── 20260312*.sql                     # 2026-03-12 迁移
│   └── 20260313*.sql                     # 2026-03-13 迁移
├── 00000000_unified_schema_v6.sql        # 当前统一架构
├── DATABASE_FIELD_STANDARDS.md           # 字段标准文档
├── MIGRATION_HISTORY.md                  # 本文档
├── MIGRATION_INDEX.md                    # 迁移索引
└── README.md                             # 迁移说明
```

---

## 统计信息

| 指标 | 数值 |
|------|------|
| 归档迁移文件 | 37 个 |
| 当前活跃迁移 | 1 个 (v6) |
| 数据库表 | 156+ 个 |
| 迁移时间跨度 | 2026-03-01 ~ 2026-03-13 |

---

## 相关文档

- [DATABASE_FIELD_STANDARDS.md](./DATABASE_FIELD_STANDARDS.md) - 字段命名标准
- [MIGRATION_INDEX.md](./MIGRATION_INDEX.md) - 迁移文件索引
- [README.md](./README.md) - 迁移使用说明

---

## 变更日志

### 2026-03-13
- 创建 `00000000_unified_schema_v6.sql` 统一架构
- 归档 37 个旧迁移文件到 `archive/` 目录
- 创建本文档记录迁移历史

### 2026-03-01
- 创建 `00000000_unified_schema_v5.sql` 首次统一架构
- 开始增量迁移阶段
