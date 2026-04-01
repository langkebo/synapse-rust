# Database Migration Changelog

> 本文件记录所有迁移脚本的删除历史，包括归档备份信息。
> 所有删除操作必须先标记 deprecated，保留一个发布周期后才能物理删除。

---

## 2026-03-29

### OPTIMIZATION_PLAN.md 实施更新

本次更新按照 `docs/OPTIMIZATION_PLAN.md` 实施各项优化任务：

#### P1 任务完成

| 任务 | 变更文件 | 说明 |
|------|----------|------|
| **历史迁移归档** | `migrations/archive/` | 15 个历史迁移脚本归档至 archive 目录 |
| **Git Tag 创建** | `archive/v6.0.4__P1_migrations_20260329` | 归档标记，用于追溯 |
| **API 测试脚本修正** | `scripts/api-integration_test.sh` | SERVER_URL 端口 28008 → 8008 |

#### 归档清单 (2026-03-29)

以下迁移已归档至 `migrations/archive/`：

| 文件名 | 说明 |
|--------|------|
| 20260321000001_fix_field_naming.sql | 字段命名修复 |
| 20260321000002_add_missing_columns.sql | 缺失列添加 |
| 20260321000003_fix_ephemeral.sql | Ephemeral 修复 |
| 20260322000001_performance_indexes.sql | 性能索引 v1 |
| 20260322000002_performance_indexes_v2.sql | 性能索引 v2 |
| 20260323225620_add_ai_connections.sql | AI 连接 |
| 20260326000001_add_event_relations.sql | 事件关系 |
| 20260326000002_fix_missing_tables.sql | 缺失表修复 |
| 20260326000003_fix_media_quota_and_other_tables.sql | 媒体配额修复 |
| 20260326000005_create_space_members_table.sql | Space 成员表 |
| 20260326000006_create_space_statistics_table.sql | Space 统计表 |
| 20260327_p0_fixes.sql | P0 修复 |
| 20260327_p2_fixes.sql | P2 修复 |
| 20260327000001_fix_space_children_columns.sql | Space 子列修复 |
| 20260327000002_create_presence_subscriptions.sql | Presence 订阅 |

**归档恢复方式**：
```bash
# 从 Git Tag 恢复
git checkout archive/v6.0.4__P1_migrations_20260329 -- migrations/archive/

# 或提取归档
git archive archive/v6.0.4__P1_migrations_20260329 | tar -xf -
```

#### 新增文档

| 文档 | 说明 |
|------|------|
| `docs/OPTIMIZATION_PLAN_IMPLEMENTATION.md` | OPTIMIZATION_PLAN 实施台账，记录各项任务状态和证据 |

#### Schema Exceptions 清理计划

以下表已列入清理计划，截止版本 v6.1.0：

- dehydrated_devices
- delayed_events
- e2ee_audit_log
- e2ee_secret_storage_keys
- e2ee_security_events
- e2ee_stored_secrets
- email_verification_tokens
- federation_access_stats
- federation_blacklist_config
- federation_blacklist_log
- federation_blacklist_rule
- key_rotation_log
- key_signatures
- leak_alerts
- room_sticky_events
- user_reputations

#### drift-detection.yml 修复详情

- 允许遗留迁移格式 `YYYYMMDDHHMMSS_*.sql` 存在（向后兼容）
- 新迁移必须使用格式 `V{version}__{Jira}_{description}.sql`
- special 文件 (`00000000_unified_schema_v6.sql`, `99999999_unified_incremental_migration.sql`, `MANIFEST-template.txt`) 豁免检查
- 重复迁移检查改为仅扫描根目录
- 回滚脚本检查支持 `.undo.sql`, `.down.sql`, `.rollback.sql` 三种后缀

#### Schema Exceptions 清理

- 创建 `V260330_001__MIG-XXX__add_missing_schema_tables.sql` 迁移脚本
- 为 11 个缺失 schema 表创建定义:
  - dehydrated_devices, delayed_events, e2ee_audit_log
  - e2ee_secret_storage_keys, e2ee_stored_secrets, email_verification_tokens
  - federation_access_stats, federation_blacklist_config, federation_blacklist_log
  - federation_blacklist_rule, leak_alerts
- 同步创建回滚脚本 `V260330_001__MIG-XXX__add_missing_schema_tables.undo.sql`
- 更新 `schema_table_coverage_exceptions.txt` 移除已补齐的表

#### 测试验证

- Rust 测试套件: **762 passed, 0 failed**
- Schema 检查: 全部通过 (209 引用表, 231 schema 表)

---

## 2026-03-12

### 初始版本

- 本文件建立，用于记录迁移脚本的生命周期变更

---

## 删除记录模板

```markdown
## YYYY-MM-DD - 删除: {filename}

- **Version**: V{version}
- **Jira**: {jira}
- **Description**: {description}
- **Reason**: {reason}
- **Approved by**: {reviewers}
- **Git Archive**: {archive_tag}
- **Archive File**: {archive_path}
```

---

## 归档恢复指南

### 从 Git Tag 恢复

```bash
# 查看归档 tag
git tag -l "archive/*"

# 检出归档文件
git checkout archive/{version}__{jira} -- migrations/{filename}

# 或提取归档文件
git archive archive/{version}__{jira} | tar -xf -
```

### 从备份文件恢复

```bash
# 解压归档
tar -xzf {archive_name}.tar.gz

# 恢复文件
cp {archive_name}/migrations/{filename} migrations/
```
