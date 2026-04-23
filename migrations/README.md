# 数据库迁移说明

> 最后更新: 2026-04-22

## 目录结构

```
migrations/
├── 00000000_unified_schema_v6.sql                              # 统一基线 (新环境唯一建库入口)
├── 00000001_extensions_cas.sql                                 # Feature-gated: CAS SSO
├── 00000001_extensions_friends.sql                             # Feature-gated: 好友系统
├── 00000001_extensions_privacy.sql                             # Feature-gated: 隐私设置
├── 00000001_extensions_saml.sql                                # Feature-gated: SAML SSO
├── 00000001_extensions_voice.sql                               # Feature-gated: 语音消息
├── 20260401000001_consolidated_schema_additions.sql (+undo)    # 增量表/索引/列 (7 文件合并)
├── 20260406000001_consolidated_schema_fixes.sql (+undo)        # 约束/FK/契约修复 (8 文件合并)
├── 20260410000001_consolidated_feature_additions.sql (+undo)   # 功能特性添加 (7 文件合并)
├── 20260421000001_consolidated_drop_redundant_tables.sql (+undo) # 冗余表删除 (4 文件合并)
├── extension_map.conf                                          # 扩展迁移映射表
├── README.md                                                   # 本文件
├── archive/                                                    # 已归档的旧迁移文件
│   └── pre-consolidation-2026-04-22/                          # 合并前原始文件 (51 个)
└── undo/                                                       # 空目录 (回滚统一用 .undo.sql 后缀)
```

**文件数量**: 7 个正向迁移 + 4 个 undo + 5 个扩展 = 16 个活跃 SQL 文件
**合并前**: 32 个正向迁移 + 25 个 undo = 57 个 SQL 文件 (**-72%**)

## 迁移执行顺序

1. `00000000_unified_schema_v6.sql` — 基线 (IF NOT EXISTS，幂等)
2. `00000001_extensions_*.sql` — 按 ENABLED_EXTENSIONS 过滤
3. `20260401000001_consolidated_schema_additions.sql` — 增量 schema
4. `20260406000001_consolidated_schema_fixes.sql` — 约束修复
5. `20260410000001_consolidated_feature_additions.sql` — 功能添加
6. `20260421000001_consolidated_drop_redundant_tables.sql` — 冗余表清理

所有增量迁移均使用 `IF NOT EXISTS` / `IF EXISTS` 幂等保护，可安全重复执行。

## 合并说明 (2026-04-22)

### 合并策略

将 26 个独立增量迁移合并为 4 个逻辑分组：

| 合并文件 | 源文件数 | 合并逻辑 |
|----------|---------|---------|
| `consolidated_schema_additions` | 7 | 2026-03-29 ~ 2026-04-04 的表/列/索引添加 |
| `consolidated_schema_fixes` | 8 | 2026-04-05 ~ 2026-04-06 的约束/FK/契约修复 |
| `consolidated_feature_additions` | 7 | 2026-04-07 ~ 2026-04-18 的功能特性添加 |
| `consolidated_drop_redundant_tables` | 4 | 2026-04-21 ~ 2026-04-22 的冗余表删除 |

### 向后兼容

- 原始文件保留在 `archive/pre-consolidation-2026-04-22/` (只归档不删除)
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

- `docs/db/MIGRATION_GOVERNANCE.md` — 迁移治理方案
- `docs/db/MIGRATION_INDEX.md` — 迁移索引
- `docs/synapse-rust/OPTIMIZATION_AND_DEDUPLICATION_PLAN_2026-04-21.md` — 优化总方案
- `docs/synapse-rust/REDUNDANT_TABLE_DELETION_PLAN.md` — 冗余表删除专项方案
