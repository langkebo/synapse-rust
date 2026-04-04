# 索引并发创建转换报告

> 执行日期：2026-04-04
> 执行脚本：scripts/convert_indexes_to_concurrent.sh

## 一、转换概述

本次转换将所有迁移文件中的索引创建改为 CONCURRENTLY 模式，避免在生产环境中锁表。

### 转换策略

- **统一 schema**：不转换（新环境从空表开始，无需 CONCURRENTLY）
- **迁移文件**：全部转换为 CONCURRENTLY
- **归档文件**：跳过（不再使用）

### 转换规则

```sql
-- 转换前
CREATE INDEX idx_name ON table(column);

-- 转换后
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_name ON table(column);
```

## 二、转换统计

### 2.1 文件统计

- 迁移文件总数：      92
- 包含索引的文件：      79
- 并发索引总数：0

### 2.2 转换详情

- 20260330000003_align_retention_and_room_summary_schema.sql: 7 个并发索引
- 20260329000100_add_missing_schema_tables.sql: 23 个并发索引
- 20260329_p2_optimization.sql: 8 个并发索引
- 20260330000010_add_audit_events.sql: 3 个并发索引
- 20260330000004_align_space_schema_and_add_space_events.sql: 9 个并发索引
- 20260330000008_align_background_update_exceptions.sql: 3 个并发索引
- 20260328_p1_indexes.sql: 9 个并发索引
- 99999999_unified_incremental_migration.sql: 35 个并发索引
- 20260329000000_create_migration_audit_table.sql: 3 个并发索引
- 20260330000013_align_legacy_timestamp_columns.sql: 3 个并发索引
- 20260403000001_add_openclaw_integration.sql: 17 个并发索引
- 20260330000003_align_retention_and_room_summary_schema.sql: 7 个并发索引
- 20260322000002_performance_indexes_v2.sql: 3 个并发索引
- 20260329000100_add_missing_schema_tables.sql: 23 个并发索引
- 20260322000001_performance_indexes.sql: 35 个并发索引
- 20260326000006_create_space_statistics_table.sql: 1 个并发索引
- 20260329_p2_optimization.sql: 8 个并发索引
- 20260330000010_add_audit_events.sql: 3 个并发索引
- 20260326000005_create_space_members_table.sql: 3 个并发索引
- 20260330000004_align_space_schema_and_add_space_events.sql: 9 个并发索引
- 20260330000008_align_background_update_exceptions.sql: 3 个并发索引
- schema_legacy.sql: 20 个并发索引
- 20260327_p2_fixes.sql: 12 个并发索引
- 20260328_p1_indexes.sql: 9 个并发索引
- 20260326000001_add_event_relations.sql: 3 个并发索引
- 99999999_unified_incremental_migration.sql: 35 个并发索引
- 20260329000000_create_migration_audit_table.sql: 3 个并发索引
- 20260327000001_fix_space_children_columns.sql: 2 个并发索引
- 20260330000013_align_legacy_timestamp_columns.sql: 3 个并发索引
- 20260326000003_fix_media_quota_and_other_tables.sql: 4 个并发索引
- 20260403000001_add_openclaw_integration.sql: 17 个并发索引
- 20260326000002_fix_missing_tables.sql: 6 个并发索引
- 20260330000012_add_federation_signing_keys.sql: 2 个并发索引
- 20260330000005_align_remaining_schema_exceptions.sql: 33 个并发索引
- 20260323225620_add_ai_connections.sql: 2 个并发索引
- 20260328000002_add_federation_cache.sql: 2 个并发索引
- 20260330000002_align_thread_schema_and_relations.sql: 6 个并发索引
- 20260330000009_align_beacon_and_call_exceptions.sql: 12 个并发索引
- 20260327_p0_fixes.sql: 4 个并发索引
- 20260330000011_add_feature_flags.sql: 3 个并发索引
- 20260328000003_add_invite_restrictions_and_device_verification_request.sql: 6 个并发索引
- 20260331000100_add_event_relations_table.sql: 4 个并发索引
- 20260330000006_align_notifications_push_and_misc_exceptions.sql: 9 个并发索引
- 20260330000001_add_thread_replies_and_receipts.sql: 4 个并发索引
- 20260327000002_create_presence_subscriptions.sql: 2 个并发索引
- 20260330000007_align_uploads_and_user_settings_exceptions.sql: 3 个并发索引
- 20260330000012_add_federation_signing_keys.sql: 2 个并发索引
- 20260330000005_align_remaining_schema_exceptions.sql: 33 个并发索引
- 20260328000002_add_federation_cache.sql: 2 个并发索引
- 20260330000002_align_thread_schema_and_relations.sql: 6 个并发索引
- 20260330000009_align_beacon_and_call_exceptions.sql: 12 个并发索引
- 20260330000003_align_retention_and_room_summary_schema.sql: 7 个并发索引
- 20260329_p2_optimization.sql: 8 个并发索引
- 20260330000010_add_audit_events.sql: 3 个并发索引
- 20260330000004_align_space_schema_and_add_space_events.sql: 9 个并发索引
- 20260328_p1_indexes.sql: 9 个并发索引
- 99999999_unified_incremental_migration.sql: 35 个并发索引
- 20260330000013_align_legacy_timestamp_columns.sql: 3 个并发索引
- 20260330000005_align_remaining_schema_exceptions.sql: 33 个并发索引
- 20260330000002_align_thread_schema_and_relations.sql: 6 个并发索引
- 20260328000003_add_invite_restrictions_and_device_verification_request.sql: 6 个并发索引
- 20260330000001_add_thread_replies_and_receipts.sql: 4 个并发索引
- 20260330000003_align_retention_and_room_summary_schema.sql: 7 个并发索引
- 20260329_p2_optimization.sql: 8 个并发索引
- 20260330000010_add_audit_events.sql: 3 个并发索引
- 20260330000004_align_space_schema_and_add_space_events.sql: 9 个并发索引
- 20260328_p1_indexes.sql: 9 个并发索引
- 99999999_unified_incremental_migration.sql: 35 个并发索引
- 20260330000013_align_legacy_timestamp_columns.sql: 3 个并发索引
- 20260330000005_align_remaining_schema_exceptions.sql: 33 个并发索引
- 20260330000002_align_thread_schema_and_relations.sql: 6 个并发索引
- 20260328000003_add_invite_restrictions_and_device_verification_request.sql: 6 个并发索引
- 20260330000001_add_thread_replies_and_receipts.sql: 4 个并发索引
- 20260330000011_add_feature_flags.sql: 3 个并发索引
- 20260328000003_add_invite_restrictions_and_device_verification_request.sql: 6 个并发索引
- 20260331000100_add_event_relations_table.sql: 4 个并发索引
- 20260330000006_align_notifications_push_and_misc_exceptions.sql: 9 个并发索引
- 20260330000001_add_thread_replies_and_receipts.sql: 4 个并发索引
- 20260330000007_align_uploads_and_user_settings_exceptions.sql: 3 个并发索引

## 三、性能影响

### 3.1 优势

1. **避免锁表**：CONCURRENTLY 模式不会阻塞表的读写操作
2. **生产友好**：可以在生产环境安全执行
3. **降低风险**：减少部署时的停机时间

### 3.2 注意事项

1. **执行时间更长**：CONCURRENTLY 模式比普通模式慢 2-3 倍
2. **需要更多资源**：占用更多 CPU 和内存
3. **不能在事务中**：CONCURRENTLY 不能在事务块中执行

### 3.3 建议

- 在维护窗口执行大表的索引创建
- 监控索引创建进度
- 预留足够的执行时间

## 四、验证建议

### 4.1 语法验证

```bash
# 检查 SQL 语法
for f in migrations/*.sql; do
    psql -d postgres -c "\i $f" --single-transaction --set ON_ERROR_STOP=on
done
```

### 4.2 功能验证

```bash
# 在测试数据库验证
createdb synapse_test
psql -d synapse_test -f migrations/00000000_unified_schema_v6.sql

# 执行迁移
for f in migrations/202*.sql; do
    echo "执行: $f"
    psql -d synapse_test -f "$f"
done

# 验证索引
psql -d synapse_test -c "SELECT schemaname, tablename, indexname FROM pg_indexes WHERE schemaname = 'public' ORDER BY tablename, indexname;"

# 清理
dropdb synapse_test
```

## 五、回滚方案

如需回滚：

```bash
# 恢复备份
cp migrations/.backup_concurrent_*/* migrations/

# 验证
git diff migrations/
```

## 六、后续行动

- [ ] 在测试环境验证
- [ ] 监控索引创建性能
- [ ] 更新部署文档
- [ ] 培训运维团队

---

**报告生成时间**：2026-04-04 10:01:41
**备份位置**：/Users/ljf/Desktop/hu/synapse-rust/migrations/.backup_concurrent_20260404_100139
