# 数据库迁移说明

> 最后更新: 2026-06-12

## 目录结构

```
migrations/
├── 00000000_unified_schema_v10.sql         # v10 统一基线（新环境唯一建库入口）
├── 00000001_extensions_v10.sql             # Feature-gated: 扩展表
├── archive/                                # 历史归档（v8 系列，已废弃，仅溯源）
│   ├── 00000000_unified_schema_v8.sql
│   ├── 00000001_extensions_v8.sql
│   ├── 20260605120000_megolm_vodozemac_dual_write_v8.sql
│   └── 20260606120000_m26_drop_redundant_module_columns.sql
├── extension_map.conf                      # 扩展迁移映射表
└── README.md                               # 本文件
```

**当前活跃链路**: `v10 baseline + 1 extension`（合计 2 个迁移文件）

> v8 系列已归档至 `archive/`，不再作为活跃迁移链路。新环境应使用 v10 基线建库。

## v10 变更摘要 (2026-06-12)

v10 基线在 v8 基础上进一步收敛：

- v8 系列文件归档至 `archive/`
- v10 双文件作为唯一生效基线
- 所有 ALTER TABLE 变更内联到表定义
- 布尔字段统一 `is_` 前缀
- NOT NULL 时间戳使用 `_ts` 后缀，可空时间戳使用 `_at` 后缀

## 迁移执行顺序

1. `00000000_unified_schema_v10.sql` — 基线 (IF NOT EXISTS，幂等)
2. `00000001_extensions_v10.sql` — 按 ENABLED_EXTENSIONS 过滤

## 首次部署

```bash
bash docker/db_migrate.sh migrate
```

## 升级已有环境

```bash
bash docker/db_migrate.sh migrate
bash docker/db_migrate.sh validate
```

## 扩展迁移选择

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

- 必填毫秒时间戳: `*_ts` (BIGINT NOT NULL)
- 可选时间戳: `*_at` (BIGINT NULLABLE)
- 布尔字段: `is_*` 前缀

## 合并历史

### 第五轮合并 (2026-06-12) — v10 基线

将 v8 系列归档，升级至 v10 双文件基线。

### 第四轮合并 (2026-06-04) — v8 基线（已归档）

v8 基线将 v7 基线 + 8 个批次迁移 + 14 个增量迁移（共 25 个文件）合并为 2 个文件。详见 `archive/` 目录。

### 历史合并记录

- 第一轮 (2026-04-22): 26 个增量 → 4 个分组
- 第二轮 (2026-05-07): 5 个扩展 → 1 个，创建 v7 批次
- 第三轮 (2026-05-09): 14 个增量 → 3 个分组

## 相关文档

- `docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md` — 全面技术审查报告（v7.0）