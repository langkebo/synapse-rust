# 迁移索引

## 概述

本文档记录 synapse-rust 数据库迁移脚本的治理规范、目录结构和执行策略。

## 核心迁移文件

| 文件 | 描述 | 状态 |
|------|------|------|
| `00000000_unified_schema_v6.sql` | 基础数据库 Schema (基线) | 必需 |
| `99999999_unified_incremental_migration.sql` | 历史综合迁移兼容资产 | 保留 |

## 目录结构

| 目录 | 用途 | 治理要求 |
|------|------|----------|
| `migrations/` | sqlx 默认迁移入口，包含基线和散列迁移 | 历史迁移待归档 |
| `migrations/rollback/` | 回滚脚本 (`.rollback.sql`) | 与迁移同名，按日期逆序执行 |
| `migrations/incremental/` | 常规增量迁移治理入口 | 使用版本化命名，纳入 manifest 与 layout audit |
| `migrations/hotfix/` | 紧急修复迁移治理入口 | 使用版本化命名，需在后续常规迁移中收敛 |
| `migrations/archive/` | 历史脚本归档 | 已归档脚本，不参与执行 |

## 迁移命名规范

### 旧格式 (遗留)
```
YYYYMMDDHHMMSS_description.sql
```
- 历史迁移使用此格式
- 已被 unified schema 吸收或待归档

### 新格式 (治理目录)
```
V{version}__{Jira编号}_{简短描述}.sql
V{version}__{Jira编号}_{简短描述}.down.sql
V{version}__{Jira编号}_{简短描述}.undo.sql
```

- 用于 `migrations/incremental/` 与 `migrations/hotfix/`
- 作为最小实现阶段的唯一治理命名规范
- 根目录历史 sqlx 迁移保留旧格式，按批次归档，不在本轮强制重写

示例:
- `V260330_001__MIG-142__add_audit_events.sql`
- `V260330_001__MIG-142__add_audit_events.undo.sql`

### 特殊文件
| 文件 | 说明 |
|------|------|
| `00000000_unified_schema_v6.sql` | 基线 schema，新环境唯一建库入口 |
| `99999999_unified_incremental_migration.sql` | 历史综合增量兼容资产，不作为唯一升级入口 |

## 迁移策略

### 新环境部署
```bash
# 由 Docker 运维入口或 CI 统一执行
bash docker/db_migrate.sh migrate
bash docker/db_migrate.sh validate
```

### 现有环境升级
```bash
# 由 Docker 运维入口或 CI 统一执行
bash docker/db_migrate.sh migrate
bash docker/db_migrate.sh validate
```

### 治理门禁职责

- `db-migration-gate.yml` 是唯一迁移治理门禁
- `ci.yml` 保留通用测试与 `sqlx migrate run` 初始化，不承担治理口径定义
- `99999999_unified_incremental_migration.sql` 在最小实现阶段继续保留，但新增关键迁移以专项门禁和运维脚本为准
- Docker 入口在 `RUN_MIGRATIONS=true` 时自动触发同一 `migrate` 入口，不构成独立迁移口径

## 回滚策略

### 回滚目录
回滚脚本位于 `rollback/` 目录，或与版本化脚本同名的 `.down.sql` / `.undo.sql` 文件

### 回滚原则
1. **DROP TABLE 操作不可逆** - 回滚会删除表和数据
2. **列重命名可逆** - 使用条件判断安全回滚
3. **列添加通常不可逆** - PostgreSQL 不容易删除列

### 回滚执行顺序
```bash
# 按日期顺序回滚（逆序）
psql -U synapse -d synapse -f migrations/rollback/20260330000009_...rollback.sql
psql -U synapse -d synapse -f migrations/rollback/20260330000008_...rollback.sql
# ...
```

## 迁移生命周期标签

| 标签 | 含义 | 保留窗口 | 删除要求 |
|------|------|----------|----------|
| `deprecated` | 已有替代脚本，仍处于兼容保留期 | 至少 2 个发布周期 | 删除前需完成替代关系验证与 DBA 审核 |
| `unused` | 当前链路不再执行，但仍需保留审计证据 | 至少 1 个发布周期 | 删除前需确认不在任何部署流程、CI、runbook 被引用 |
| `test-only` | 仅用于测试环境、演练或基准生成 | 可按测试策略保留 1 个周期 | 删除前需确认测试工单或基准场景已迁移 |

## Manifest 与防篡改

每次发布生成 `migrations/MANIFEST-{release}.txt`，记录：
- 文件名
- 字节大小
- SHA-256
- Owner
- 适用阶段

### 防篡改校验
CI 与发布脚本必须对 manifest 中所有 SQL 文件重算 SHA-256；任一不一致直接阻断。

## 迁移资产台账

按 OPTIMIZATION_PLAN.md Section 6.10 要求维护台账：

| 字段 | 说明 |
|------|------|
| 脚本名称 | 文件名与业务域 |
| 脚本类型 | 基线、增量、回滚、热修复、归档 |
| 生效范围 | 新环境、升级环境、紧急修复 |
| 替代关系 | 被哪个脚本或基线吸收 |
| 回滚方式 | 独立回滚、批次回滚、不可逆声明 |
| 关联测试 | smoke、roundtrip、schema contract、checksum |
| 归档状态 | 活跃、待归档、已归档、待删除 |

## 大表性能基线

| 项目 | 基线要求 | 超阈值处理 |
|------|----------|------------|
| 大表迁移数据量 | 以 1000 万行为标准样本 | 样本不足时不得宣称生产级通过 |
| 执行窗口 | 单批次大表迁移 30 秒内完成 | 超过即判定为风险变更 |
| 锁等待 | 单次阻塞锁等待 < 3 秒 | 超过阈值立即终止迁移 |
| 资源占用 | CPU < 70%, IO 等待 < 20%, 连接池 < 80% | 任一超限触发 SRE 告警 |

## 迁移整合说明

`99999999_unified_incremental_migration.sql` 作为历史综合增量兼容资产，主要保留早期索引收敛用途，不代表当前所有离散迁移已被其覆盖。

历史意图示例:
- 20260320000001 - 密码字段重命名
- 20260320000002 - Olm 布尔字段重命名
- 20260320000004 - processed_ts 列
- 20260321000001 - 设备信任表
- 20260321000003 - 安全备份表
- 20260321000005 - 时间戳字段修复
- 20260321000006 - 字段一致性修复
- 20260321000007 - revoked_at 到 is_revoked

## Schema Exceptions 台账

以下表在代码中引用但暂无 schema 定义，已列入清理计划（**已全部补齐**）：

> ✅ **2026-03-30 更新**: 所有 16 个表已通过 `V260330_001__MIG-XXX__add_missing_schema_tables.sql` 补齐

| 表名 | 类别 | 清理截止版本 | 状态 |
|------|------|--------------|------|
| dehydrated_devices | rtc | v6.1.0 | ✅ 已补齐 |
| delayed_events | events | v6.1.0 | ✅ 已补齐 |
| e2ee_audit_log | e2ee | v6.1.0 | ✅ 已补齐 |
| e2ee_secret_storage_keys | e2ee | v6.1.0 | ✅ 已补齐 |
| e2ee_stored_secrets | e2ee | v6.1.0 | ✅ 已补齐 |
| email_verification_tokens | auth | v6.1.0 | ✅ 已补齐 |
| federation_access_stats | federation | v6.1.0 | ✅ 已补齐 |
| federation_blacklist_config | federation | v6.1.0 | ✅ 已补齐 |
| federation_blacklist_log | federation | v6.1.0 | ✅ 已补齐 |
| federation_blacklist_rule | federation | v6.1.0 | ✅ 已补齐 |
| key_rotation_log | e2ee | v6.1.0 | ✅ 已补齐 |
| key_signatures | e2ee | v6.1.0 | ✅ 已补齐 |
| leak_alerts | e2ee | v6.1.0 | ✅ 已补齐 |
| room_sticky_events | notifications | v6.1.0 | ✅ 已补齐 |
| user_reputations | users | v6.1.0 | ✅ 已补齐 |

详细迁移脚本: `V260330_001__MIG-XXX__add_missing_schema_tables.sql`

## 文档

- [DATABASE_FIELD_STANDARDS.md](./DATABASE_FIELD_STANDARDS.md) - 字段命名规范
- [SCHEMA_OPTIMIZATION_REPORT.md](./SCHEMA_OPTIMIZATION_REPORT.md) - Schema 优化报告
- [CHANGELOG-DB.md](../CHANGELOG-DB.md) - 数据库迁移变更日志
- [ROLLBACK_RUNBOOK.md](../docs/ROLLBACK_RUNBOOK.md) - 回滚操作手册
