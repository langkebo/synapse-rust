# 数据库迁移说明

> 最后更新: 2026-06-04

## 目录结构

```
migrations/
├── 00000000_unified_schema_v8.sql          # v8 统一基线 (新环境唯一建库入口)
├── 00000001_extensions_v8.sql              # Feature-gated: 扩展表 (CAS/SAML/Friends/Voice)
├── extension_map.conf                      # 扩展迁移映射表
└── README.md                               # 本文件
```

**当前活跃链路**: `v8 baseline + 1 extension` (合计 2 个迁移文件)

## v8 变更摘要 (2026-06-04)

v8 基线将 v7 基线 + 8 个批次迁移 + 14 个增量迁移合并为单一真相源：

- 移除 19 个已 DROP 的冗余表
- 移除 v7 基线内部 ~30 个重复表定义（主干 + Folded Delta）
- 所有 ALTER TABLE 变更内联到表定义
- `voice_usage_stats` 使用 20260517 版本（与 Rust `VoiceUsageRecord` 匹配）
- `user_privacy_settings` 合并 visibility 列
- `spam_check_results`/`third_party_rule_results` 使用 20260529 新版本
- CAS 表使用 `_at` 后缀（`consumed_at`/`logout_sent_at`）
- 新增 burn_after_read, key_rotation, megolm_session_keys 等表
- 整合所有索引、视图、外键、触发器、默认数据
- 布尔字段统一 `is_` 前缀
- NOT NULL 时间戳使用 `_ts` 后缀，可空时间戳使用 `_at` 后缀

## 迁移执行顺序

1. `00000000_unified_schema_v8.sql` — 基线 (IF NOT EXISTS，幂等)
2. `00000001_extensions_v8.sql` — 按 ENABLED_EXTENSIONS 过滤

## v7 → v8 升级指引

1. 先执行 `bash docker/db_migrate.sh migrate`（自动检测 v7 基线并执行增量升级）
2. 再执行 `bash docker/db_migrate.sh validate`
3. CI 中统一通过 `scripts/build_sqlx_migration_source.py` 构建前向迁移链

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

### 第四轮合并 (2026-06-04) — v8 基线

将 v7 基线 + 8 个批次迁移 + 14 个增量迁移（共 25 个文件）合并为 2 个文件：

| 合并文件 | 源文件数 | 合并逻辑 |
|----------|---------|---------|
| `00000000_unified_schema_v8.sql` | 25 | 全量收敛：消除重复、内联变更、解决冲突 |
| `00000001_extensions_v8.sql` | 1 | 扩展表对齐 v8 命名规范 |

### 历史合并记录

- 第一轮 (2026-04-22): 26 个增量 → 4 个分组
- 第二轮 (2026-05-07): 5 个扩展 → 1 个，创建 v7 批次
- 第三轮 (2026-05-09): 14 个增量 → 3 个分组

## 相关文档

- `docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md` — 全面技术审查报告
