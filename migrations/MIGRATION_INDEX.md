# 迁移索引

## 核心迁移文件

| 文件 | 描述 | 状态 |
|------|------|------|
| `00000000_unified_schema_v6.sql` | 基础数据库 Schema | 必需 |
| `UNIFIED_MIGRATION_v1.sql` | 综合迁移 (整合所有增量迁移) | 推荐 |

## 迁移策略

### 新环境部署
```bash
# 1. 执行基础 Schema
psql -U synapse -d synapse -f migrations/00000000_unified_schema_v6.sql

# 2. 执行综合迁移
psql -U synapse -d synapse -f migrations/UNIFIED_MIGRATION_v1.sql
```

### 现有环境升级
```bash
# 直接执行综合迁移 (幂等)
psql -U synapse -d synapse -f migrations/UNIFIED_MIGRATION_v1.sql
```

## 迁移整合说明

UNIFIED_MIGRATION_v1.sql 已整合以下迁移:
- 20260320000001 - 密码字段重命名
- 20260320000002 - Olm 布尔字段重命名
- 20260320000004 - processed_ts 列
- 20260321000001 - 设备信任表
- 20260321000003 - 安全备份表
- 20260321000005 - 时间戳字段修复
- 20260321000006 - 字段一致性修复
- 20260321000007 - revoked_at 到 is_revoked

## 文档

- [DATABASE_FIELD_STANDARDS.md](./DATABASE_FIELD_STANDARDS.md) - 字段命名规范
- [SCHEMA_OPTIMIZATION_REPORT.md](./SCHEMA_OPTIMIZATION_REPORT.md) - Schema 优化报告
