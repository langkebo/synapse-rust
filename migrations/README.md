# 数据库迁移说明

> 最后更新: 2026-05-09

## 目录结构

```
migrations/
├── 00000000_unified_schema_v7.sql                              # 当前统一基线 (新环境唯一建库入口)
├── 00000001_extensions.sql (+undo)                             # Feature-gated: 扩展表 (CAS/SAML/Friends/Voice/Privacy)
├── 20260515000001_consolidated_schema_contract_and_features_v7.sql (+undo)
│                                                               # Batch-01: v7 后结构/契约/功能收敛
├── 20260515000002_consolidated_stream_ordering_online_fix_v7.sql (+undo)
│                                                               # Batch-02: stream_ordering 在线回填与覆盖索引
├── 20260515000003_consolidated_drop_redundant_tables_v7.sql (+undo)
│                                                               # Batch-03: 冗余表清理
├── 20260515000004_consolidated_schema_fixes_v7.sql (+undo)
│                                                               # Batch-04: Schema 修复 (room_ephemeral unique, backup_keys fields, expires_at fix)
├── 20260515000005_consolidated_table_indexes_v7.sql (+undo)
│                                                               # Batch-05: 表索引优化 (events/users/room_memberships/access_tokens/federation_queue/background_updates/trigram/spaces/threads)
├── 20260515000006_consolidated_constraint_governance_v7.sql (+undo)
│                                                               # Batch-06: 约束治理 (复合主键 + 外键补齐)
├── 20260515000007_rooms_summaries_materialized_view_v7.sql (+undo)
│                                                               # Batch-07: 物化视图 (rooms_summaries + public_room_directory)
├── extension_map.conf                                          # 扩展迁移映射表
├── README.md                                                   # 本文件
└── archive/                                                    # 已归档的旧迁移文件
    ├── pre-consolidation-2026-04-22/                           # 合并前原始文件 (51 个)
    └── full-backup-20260509-064803/                            # 第三轮合并前备份 (23 个)
```

**当前活跃链路**: `v7 baseline + 1 extension + 7 v7 batch` (合计 9 对迁移 + 1 基线 = 19 文件)

## 迁移执行顺序

1. `00000000_unified_schema_v7.sql` — 基线 (IF NOT EXISTS，幂等)
2. `00000001_extensions.sql` — 按 ENABLED_EXTENSIONS 过滤
3. `20260515000001_consolidated_schema_contract_and_features_v7.sql` — Batch-01
4. `20260515000002_consolidated_stream_ordering_online_fix_v7.sql` — Batch-02
5. `20260515000003_consolidated_drop_redundant_tables_v7.sql` — Batch-03
6. `20260515000004_consolidated_schema_fixes_v7.sql` — Batch-04
7. `20260515000005_consolidated_table_indexes_v7.sql` — Batch-05
8. `20260515000006_consolidated_constraint_governance_v7.sql` — Batch-06
9. `20260515000007_rooms_summaries_materialized_view_v7.sql` — Batch-07（物化视图，可选）

所有增量迁移均使用 `IF NOT EXISTS` / `IF EXISTS` 幂等保护，可安全重复执行。

## v7 升级指引

1. 先执行 `bash docker/db_migrate.sh migrate`
2. 再执行 `bash docker/db_migrate.sh validate`
3. 如需回滚，按批次执行对应 `*.undo.sql`
4. CI 中统一通过 `scripts/build_sqlx_migration_source.py` 构建前向迁移链，并通过 `scripts/check_migration_consistency.py` 检查镜像目录与回滚脚本完整性

## 合并说明

### 第一轮合并 (2026-04-22)

将 26 个独立增量迁移合并为 4 个逻辑分组：

| 合并文件 | 源文件数 | 合并逻辑 |
|----------|---------|---------|
| `consolidated_schema_additions` | 7 | 2026-03-29 ~ 2026-04-04 的表/列/索引添加 |
| `consolidated_schema_fixes` | 8 | 2026-04-05 ~ 2026-04-06 的约束/FK/契约修复 |
| `consolidated_feature_additions` | 7 | 2026-04-07 ~ 2026-04-18 的功能特性添加 |
| `consolidated_drop_redundant_tables` | 4 | 2026-04-21 ~ 2026-04-22 的冗余表删除 |

### 第二轮合并 (2026-05-07)

- 5 个扩展文件合并为 `00000001_extensions.sql`
- Batch-01/Batch-02/Batch-03 三个 v7 收敛批次创建

### 第三轮合并 (2026-05-09)

将 14 个增量迁移合并为 3 个逻辑分组：

| 合并文件 | 源文件数 | 合并逻辑 |
|----------|---------|---------|
| `consolidated_schema_fixes_v7` (Batch-04) | 3 | Schema 修复：room_ephemeral unique 约束、backup_keys 字段、expires_at 修正 |
| `consolidated_table_indexes_v7` (Batch-05) | 9 | 表索引优化：events/users/room_memberships/access_tokens/federation_queue/background_updates + trigram + spaces + threads |
| `consolidated_constraint_governance_v7` (Batch-06) | 1 | 约束治理：复合主键 + 外键补齐（重命名保持命名一致性） |

### 向后兼容

- 原始文件保留在 `archive/` 中
- 已部署环境的 `schema_migrations` 表中保留旧版本记录
- 合并文件使用新的版本号，不会与旧记录冲突
- 所有 SQL 语句幂等，新环境和已部署环境均可安全执行

## 使用方法

### 首次部署

```bash
bash docker/db_migrate.sh migrate
```

### 升级已有环境

```bash
bash docker/db_migrate.sh migrate
bash docker/db_migrate.sh validate
```

### 扩展迁移选择

通过 `ENABLED_EXTENSIONS` 环境变量控制：

```bash
# 全部功能（默认）
ENABLED_EXTENSIONS=all ./deploy.sh

# 仅核心 Matrix
ENABLED_EXTENSIONS=none ./deploy.sh

# 选择性启用
ENABLED_EXTENSIONS=openclaw-routes,friends ./deploy.sh
```

可用功能名称（与 Cargo feature flags 一致）：
`openclaw-routes`, `friends`, `voice-extended`, `saml-sso`, `cas-sso`,
`beacons`, `voip-tracking`, `widgets`, `server-notifications`,
`burn-after-read`, `privacy-ext`, `external-services`

## 字段命名规范

- 必填毫秒时间戳: `*_ts` (BIGINT)
- 可选时间戳: `*_at` (BIGINT 或 TIMESTAMPTZ)
- 冲突时以 Rust 模型和实际迁移文件为单一真实来源

## 相关文档

- `docs/db/MIGRATION_CONSOLIDATION_PLAN_2026-05-07.md` — 数据库重构优化方案
- `docs/synapse-rust/OPTIMIZATION_AND_DEDUPLICATION_PLAN_2026-04-21.md` — 优化总方案
- `docs/synapse-rust/REDUNDANT_TABLE_DELETION_PLAN.md` — 冗余表删除专项方案
