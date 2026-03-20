# 数据库迁移脚本合并优化方案

> **项目**: synapse-rust 数据库迁移脚本优化
> **版本**: v1.0.0
> **创建日期**: 2026-03-20
> **状态**: 待审核

---

## 一、现状分析

### 1.1 脚本数量统计

| 类别 | 数量 | 说明 |
|------|------|------|
| 基础 Schema | 1 | `00000000_unified_schema_v6.sql` |
| 增量迁移脚本 | 29 | `202603*.sql` |
| Archive 脚本 | 5 | 已归档的历史脚本 |
| **总计** | **35** | |

### 1.2 脚本类型分类

| 类型 | 数量 | 示例 |
|------|------|------|
| 表创建脚本 | 12 | `20260311000006_add_e2ee_tables.sql` |
| 字段修复脚本 | 6 | `20260316000001_fix_field_consistency.sql` |
| 索引优化脚本 | 2 | `20260314000002_add_performance_indexes.sql` |
| 功能增强脚本 | 8 | `20260321000001_add_device_trust_tables.sql` |
| 综合迁移脚本 | 1 | `20260316000000_comprehensive_migration.sql` |

### 1.3 执行频率分析

| 频率 | 脚本数量 | 说明 |
|------|----------|------|
| 仅执行一次 | 25 | 大部分增量迁移 |
| 可能重复执行 | 8 | 包含 `IF NOT EXISTS` 检查的脚本 |
| 已在 archive | 5 | 历史归档脚本 |

### 1.4 脚本依赖关系图

```
unified_schema_v6.sql (基础)
    │
    ├── 20260309000001_password_security_enhancement.sql
    │
    ├── 20260310000004_create_federation_signing_keys.sql
    │
    ├── 20260311000001_add_space_members_table.sql
    │
    ├── 20260311000004_fix_ip_reputation_table.sql
    │
    ├── 20260311000006_add_e2ee_tables.sql
    │       └── 依赖: users 表
    │
    ├── 20260311000008_fix_key_backups_constraints.sql
    │
    ├── 20260313000000_create_room_tags_and_password_reset_tokens.sql
    │       └── 依赖: users 表
    │
    ├── 20260313000000_unified_migration.sql (已归档)
    │
    ├── 20260313000001_qr_login.sql
    │
    ├── 20260313000002_invite_blocklist.sql
    │
    ├── 20260313000003_sticky_event.sql
    │
    ├── 20260314000001_widget_support.sql
    │
    ├── 20260314000002_add_performance_indexes.sql
    │
    ├── 20260315000001_fix_field_names.sql (已归档)
    │
    ├── 20260315000002_create_admin_api_tables.sql
    │
    ├── 20260315000003_create_feature_tables.sql
    │
    ├── 20260315000004_fix_typing_columns.sql
    │
    ├── 20260315000005_fix_push_constraints.sql
    │
    ├── 20260315000005_fix_room_guest_access.sql
    │
    ├── 20260315000006_add_events_processed_ts.sql
    │
    ├── 20260315000006_fix_media_quota_config.sql
    │
    ├── 20260315000006_fix_room_summaries.sql
    │
    ├── 20260315000007_fix_media_quota_config_structure.sql
    │
    ├── 20260315000008_fix_user_media_quota_structure.sql
    │
    ├── 20260316000000_comprehensive_migration.sql (综合)
    │       └── 合并了字段修复逻辑
    │
    ├── 20260316000001_fix_field_consistency.sql
    │       └── 与 comprehensive_migration 重复
    │
    ├── 20260316000002_create_room_summary_state.sql
    │
    ├── 20260317000000_add_missing_tables.sql
    │       └── 创建: room_depth, event_auth, redactions
    │
    ├── 20260317000001_add_verification_tables.sql
    │
    ├── 20260318000001_add_event_relations.sql
    │
    ├── 20260318000002_fix_push_module.sql
    │
    ├── 20260319000001_add_application_services.sql
    │
    ├── 20260320000001_rename_must_change_password.sql
    │       └── 布尔字段重命名
    │
    ├── 20260320000002_rename_olm_boolean_fields.sql
    │
    ├── 20260321000001_add_device_trust_tables.sql
    │
    └── 20260321000003_add_secure_backup_tables.sql
```

---

## 二、问题识别

### 2.1 主要问题

| 问题 | 严重程度 | 描述 |
|------|----------|------|
| 脚本数量过多 | 高 | 29 个增量脚本难以维护 |
| 重复内容 | 高 | `comprehensive_migration.sql` 与 `fix_field_consistency.sql` 内容重复 |
| 命名不规范 | 中 | 部分脚本命名不符合 `YYYYMMDDHHMMSS` 格式 |
| 缺少幂等性 | 中 | 部分脚本在重复执行时可能出错 |
| 归档缺失 | 低 | archive 目录有 5 个脚本但未清理 |

### 2.2 重复内容分析

| 脚本 A | 脚本 B | 重复内容 |
|--------|--------|----------|
| `20260316000000_comprehensive_migration.sql` | `20260316000001_fix_field_consistency.sql` | 字段修复逻辑完全相同 |
| `20260315000005_fix_room_guest_access.sql` | `20260315000006_fix_room_summaries.sql` | 都有 `guest_access` 列添加 |

### 2.3 幂等性问题

| 脚本 | 幂等性 | 问题 |
|------|--------|------|
| `20260317000000_add_missing_tables.sql` | ✅ | 使用 `IF NOT EXISTS` |
| `20260311000006_add_e2ee_tables.sql` | ⚠️ | 部分 ALTER TABLE 无条件执行 |
| `20260319000001_add_application_services.sql` | ⚠️ | 某些列添加缺少检查 |

---

## 三、合并策略

### 3.1 合并类型分类

| 类型 | 描述 | 可合并性 |
|------|------|----------|
| 同功能合并 | 功能相似的脚本合并 | ✅ 推荐 |
| 时序合并 | 按时间顺序合并多个脚本 | ⚠️ 需分析依赖 |
| 分类合并 | 按功能模块合并 | ✅ 推荐 |
| 综合迁移 | 创建新的综合迁移脚本 | ✅ 推荐 |

### 3.2 推荐的合并方案

#### 方案 A：按功能模块合并（推荐）

| 新脚本名 | 合并内容 | 原脚本数量 |
|----------|----------|------------|
| `M001_e2ee_crypto.sql` | E2EE 相关表 | 4 |
| `M002_user_management.sql` | 用户管理相关 | 5 |
| `M003_room_features.sql` | 房间功能相关 | 6 |
| `M004_field_fixes.sql` | 字段修复 | 3 |
| `M005_security.sql` | 安全相关 | 3 |
| `M006_monitoring.sql` | 监控统计相关 | 2 |

#### 方案 B：创建统一综合迁移脚本

创建 `UNIFIED_MIGRATION_v1.sql`，包含：
- 所有表创建
- 所有字段修复
- 所有索引创建
- 幂等性保证

### 3.3 合并规则

1. **幂等性规则**: 所有语句必须可重复执行
2. **依赖规则**: 被依赖的表必须先创建
3. **顺序规则**: 表创建 → 列添加 → 索引创建 → 数据迁移
4. **回滚规则**: 每个变更必须有对应的回滚语句（注释形式）

---

## 四、实施步骤

### 4.1 阶段划分

```
┌─────────────────────────────────────────────────────────────┐
│ 阶段 1: 分析与规划 (1-2 天)                                  │
│   ├── 完整读取所有脚本                                      │
│   ├── 分析依赖关系                                          │
│   └── 制定合并计划                                          │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│ 阶段 2: 创建综合迁移脚本 (2-3 天)                            │
│   ├── 创建 unified_migration_v1.sql                         │
│   ├── 添加幂等性检查                                        │
│   └── 添加验证逻辑                                          │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│ 阶段 3: 测试验证 (2-3 天)                                   │
│   ├── 在测试环境执行                                        │
│   ├── 执行回滚测试                                          │
│   └── 性能基准测试                                          │
└─────────────────────────────────────────────────────────────┘
                           ↓
┌─────────────────────────────────────────────────────────────┐
│ 阶段 4: 部署与监控 (1-2 天)                                 │
│   ├── 备份数据库                                           │
│   ├── 执行综合迁移                                          │
│   └── 监控系统指标                                          │
└─────────────────────────────────────────────────────────────┘
```

### 4.2 备份机制

#### 备份策略

| 备份类型 | 时机 | 保留时间 |
|----------|------|----------|
| 完整数据库备份 | 迁移前 | 30 天 |
| Schema 备份 | 迁移前 | 30 天 |
| 数据备份 | 迁移前（关键表） | 7 天 |

#### 备份命令

```bash
# 完整数据库备份
pg_dump -U synapse -d synapse -F c -b -v -f backup/synapse_full_$(date +%Y%m%d).dump

# 仅 Schema 备份
pg_dump -U synapse -d synapse --schema-only -b -v -f backup/synapse_schema_$(date +%Y%m%d).dump

# 关键表数据备份
pg_dump -U synapse -d synapse -t users -t devices -t access_tokens -t refresh_tokens -F c -b -v -f backup/synapse_critical_$(date +%Y%m%d).dump
```

### 4.3 合并顺序

1. **第一批次** (基础表): `users`, `devices`, `rooms`, `events`
2. **第二批次** (认证相关): `access_tokens`, `refresh_tokens`, `token_blacklist`
3. **第三批次** (E2EE 相关): `device_keys`, `olm_accounts`, `megolm_sessions`
4. **第四批次** (业务功能): `room_memberships`, `room_summaries`, `presence`
5. **第五批次** (扩展功能): `application_services`, `pushers`, `media_*`

### 4.4 冲突处理

| 冲突类型 | 处理方式 |
|----------|----------|
| 重复表创建 | 使用 `CREATE TABLE IF NOT EXISTS` |
| 重复列添加 | 使用 `ADD COLUMN IF NOT EXISTS` |
| 重复索引创建 | 使用 `CREATE INDEX IF NOT EXISTS` |
| 数据冲突 | 使用 `ON CONFLICT DO NOTHING` |
| 类型不一致 | 以最后执行为准，添加类型转换 |

---

## 五、数据库完整性保障

### 5.1 事务处理

```sql
-- 标准迁移事务模板
BEGIN;

-- 1. 前置检查
DO $$
BEGIN
    -- 检查必要条件
    IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'users') THEN
        RAISE EXCEPTION 'Required table users does not exist';
    END IF;
END $$;

-- 2. 执行变更
-- ... 变更语句 ...

-- 3. 验证
DO $$
DECLARE
    error_count INTEGER := 0;
BEGIN
    -- 验证变更是否成功
    IF NOT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'new_table') THEN
        RAISE EXCEPTION 'Column was not created';
        error_count := error_count + 1;
    END IF;

    IF error_count > 0 THEN
        RAISE EXCEPTION 'Migration failed with % errors', error_count;
    END IF;
END $$;

COMMIT;
```

### 5.2 一致性校验

```sql
-- 迁移后一致性检查
DO $$
DECLARE
    issues INTEGER := 0;
BEGIN
    -- 检查所有必需表是否存在
    FOR table_name IN SELECT unnest(ARRAY['users', 'devices', 'rooms', 'events', ...]) LOOP
        IF NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = table_name) THEN
            RAISE WARNING 'Missing table: %', table_name;
            issues := issues + 1;
        END IF;
    END LOOP;

    -- 检查关键列是否存在
    FOR rec IN
        SELECT table_name, column_name FROM (
            VALUES ('users', 'user_id'), ('users', 'created_ts'), ...
        ) AS t(table_name, column_name)
    LOOP
        IF NOT EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_name = rec.table_name AND column_name = rec.column_name
        ) THEN
            RAISE WARNING 'Missing column: %.%', rec.table_name, rec.column_name;
            issues := issues + 1;
        END IF;
    END LOOP;

    -- 检查索引是否存在
    FOR index_name IN SELECT unnest(ARRAY['idx_users_email', 'idx_devices_user', ...]) LOOP
        IF NOT EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = index_name) THEN
            RAISE WARNING 'Missing index: %', index_name;
            issues := issues + 1;
        END IF;
    END LOOP;

    -- 汇总
    IF issues = 0 THEN
        RAISE NOTICE '✓ Database integrity check passed';
    ELSE
        RAISE WARNING '⚠ Database integrity check found % issues', issues;
    END IF;
END $$;
```

### 5.3 并发控制

```sql
-- 使用 advisory lock 防止并发执行
SELECT pg_advisory_lock(123456789);

BEGIN;

-- 迁移逻辑

COMMIT;

SELECT pg_advisory_unlock(123456789);
```

---

## 六、性能优化

### 6.1 执行效率提升

| 优化项 | 方法 | 预期提升 |
|--------|------|----------|
| 批量索引创建 | 使用 `CONCURRENTLY` | 避免锁表 |
| 并行数据导入 | 使用 `COPY` 而非 `INSERT` | 10x 提升 |
| 延迟约束检查 | 先添加列，后添加约束 | 减少锁时间 |
| 批量操作 | 合并多个 `ALTER TABLE` | 减少事务开销 |

### 6.2 资源占用控制

```sql
-- 限制迁移操作的内存使用
SET work_mem = '256MB';
SET maintenance_work_mem = '512MB';

-- 禁用自动 Vacuum 以加速迁移
SET vacuum_cost_delay = 0;

-- 批量提交减少 WAL 日志
SET synchronous_commit = off;
```

### 6.3 大表优化

对于超过 100 万行的表：

```sql
-- 使用部分索引减少索引大小
CREATE INDEX idx_large_table_active ON large_table(created_ts DESC)
WHERE is_active = TRUE;

-- 使用 covering index 减少回表
CREATE INDEX idx_large_table_covering ON large_table(user_id, created_ts DESC)
INCLUDE (id, name, status);

-- 使用分区表（如果适用）
CREATE TABLE large_table_partitioned (...)
PARTITION BY RANGE (created_ts);
```

---

## 七、回滚机制

### 7.1 回滚设计原则

1. **每个变更都有回滚语句**（以注释形式保留在脚本末尾）
2. **回滚前必须备份**
3. **回滚后必须验证**

### 7.2 回滚模板

```sql
-- =====================================================
-- 回滚方案 (如需回滚)
-- =====================================================
-- BEGIN;
-- -- 1. 删除新增的列
-- ALTER TABLE example_table DROP COLUMN IF EXISTS new_column;
--
-- -- 2. 删除新增的索引
-- DROP INDEX IF EXISTS idx_new_index;
--
-- -- 3. 删除新增的表
-- DROP TABLE IF EXISTS new_table CASCADE;
--
-- -- 4. 恢复被修改的数据
-- UPDATE example_table SET old_column = backup_data WHERE ...;
--
-- -- 5. 验证回滚
-- DO $$
-- BEGIN
--     IF EXISTS (SELECT 1 FROM information_schema.columns
--                WHERE table_name = 'example_table' AND column_name = 'new_column') THEN
--         RAISE EXCEPTION 'Rollback failed: column still exists';
--     END IF;
-- END $$;
-- COMMIT;
```

### 7.3 快速回滚脚本

创建 `rollback_migration.sh`：

```bash
#!/bin/bash
# 快速回滚脚本

MIGRATION_VERSION=$1
BACKUP_DIR="./backups"

if [ -z "$MIGRATION_VERSION" ]; then
    echo "Usage: $0 <migration_version>"
    exit 1
fi

# 1. 恢复数据库
pg_restore -U synapse -d synapse -c "$BACKUP_DIR/synapse_full_${MIGRATION_VERSION}.dump"

# 2. 验证恢复结果
psql -U synapse -d synapse -c "SELECT COUNT(*) FROM users;"

# 3. 输出结果
echo "Rollback to version $MIGRATION_VERSION completed"
```

---

## 八、测试验证方案

### 8.1 单元测试

```sql
-- 测试 1: 表创建测试
DO $$
BEGIN
    -- 创建测试表
    CREATE TABLE IF NOT EXISTS test_migration (
        id SERIAL PRIMARY KEY,
        name TEXT NOT NULL
    );

    -- 验证
    ASSERT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'test_migration'),
        'Test table should exist';

    -- 清理
    DROP TABLE test_migration;

    RAISE NOTICE 'Unit test passed: Table creation';
END $$;

-- 测试 2: 列添加测试
DO $$
BEGIN
    -- 添加测试列
    ALTER TABLE users ADD COLUMN IF NOT EXISTS test_column TEXT;

    -- 验证
    ASSERT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'users' AND column_name = 'test_column'
    ), 'Test column should exist';

    -- 清理
    ALTER TABLE users DROP COLUMN IF EXISTS test_column;

    RAISE NOTICE 'Unit test passed: Column addition';
END $$;
```

### 8.2 集成测试

```bash
#!/bin/bash
# 集成测试脚本

set -e

echo "=== 集成测试开始 ==="

# 1. 创建测试数据库
psql -U postgres -c "CREATE DATABASE synapse_test;"
psql -U synapse -d synapse_test -f migrations/UNIFIED_MIGRATION_v1.sql

# 2. 验证所有表
psql -U synapse -d synapse_test -c "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = 'public';"

# 3. 验证所有索引
psql -U synapse -d synapse_test -c "SELECT COUNT(*) FROM pg_indexes WHERE schemaname = 'public';"

# 4. 运行应用测试
cargo test --test integration

# 5. 清理测试数据库
psql -U postgres -c "DROP DATABASE synapse_test;"

echo "=== 集成测试通过 ==="
```

### 8.3 性能测试

```sql
-- 性能测试: 创建 100 万测试数据
DO $$
BEGIN
    -- 插入测试数据
    INSERT INTO users (user_id, username, created_ts)
    SELECT
        'test_user_' || i,
        'testuser' || i,
        EXTRACT(EPOCH FROM NOW())::BIGINT * 1000
    FROM generate_series(1, 1000000) i;

    RAISE NOTICE 'Performance test: Inserted 1000000 test users';

    -- 测试查询性能
    PERFORM * FROM users WHERE username = 'testuser500000';

    -- 清理
    DELETE FROM users WHERE user_id LIKE 'test_user_%';

    RAISE NOTICE 'Performance test completed';
END $$;
```

### 8.4 回归测试

```bash
#!/bin/bash
# 回归测试: 确保现有功能不受影响

echo "=== 回归测试开始 ==="

# 1. 用户认证流程
psql -U synapse -d synapse -c "SELECT * FROM users WHERE user_id = 'test_admin';"
curl -X POST http://localhost:8000/_matrix/client/v3/login \
    -H "Content-Type: application/json" \
    -d '{"identifier": {"type": "m.id.user", "user": "test_admin"}, "password": "Admin@123"}'

# 2. 房间创建流程
curl -X POST http://localhost:8000/_matrix/client/v3/createRoom \
    -H "Authorization: Bearer $TOKEN" \
    -d '{"name": "Test Room"}'

# 3. 设备管理流程
curl http://localhost:8000/_matrix/client/v3/devices \
    -H "Authorization: Bearer $TOKEN"

echo "=== 回归测试通过 ==="
```

---

## 九、监控计划

### 9.1 监控指标

| 指标 | 正常范围 | 告警阈值 |
|------|----------|----------|
| 迁移执行时间 | < 5 分钟 | > 30 分钟 |
| 锁等待时间 | < 1 秒 | > 10 秒 |
| 表膨胀率 | < 10% | > 50% |
| 索引大小 | < 表大小 30% | > 表大小 100% |
| 连接数 | < 100 | > 500 |

### 9.2 监控脚本

```bash
#!/bin/bash
# 迁移后监控脚本

# 1. 检查活动会话
psql -U synapse -d synapse -c "
SELECT COUNT(*) AS active_sessions
FROM pg_stat_activity
WHERE state = 'active';
"

# 2. 检查锁等待
psql -U synapse -d synapse -c "
SELECT COUNT(*) AS waiting_locks
FROM pg_locks
WHERE granted = FALSE;
"

# 3. 检查表膨胀
psql -U synapse -d synapse -c "
SELECT
    schemaname || '.' || tablename AS table_name,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS total_size,
    pg_size_pretty(pg_relation_size(schemaname||'.'||tablename)) AS table_size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC
LIMIT 10;
"

# 4. 检查索引使用率
psql -U synapse -d synapse -c "
SELECT
    indexrelname,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
ORDER BY idx_scan ASC
LIMIT 10;
"
```

### 9.3 告警机制

```bash
#!/bin/bash
# 告警脚本示例

METRICS_FILE="/tmp/migration_metrics.log"

# 检查指标
LOCK_WAIT=$(psql -U synapse -d synapse -t -A -c "SELECT COUNT(*) FROM pg_locks WHERE granted = FALSE;")

if [ "$LOCK_WAIT" -gt 10 ]; then
    echo "ALERT: High lock wait count: $LOCK_WAIT" | tee -a "$METRICS_FILE"
    # 发送告警 (可根据需要配置邮件、Slack 等)
    curl -X POST "https://hooks.example.com/alert" \
        -d "{\"text\": \"Database migration alert: High lock wait\"}"
fi
```

---

## 十、实施时间表

| 阶段 | 任务 | 预计时间 | 负责人 |
|------|------|----------|--------|
| 1 | 现状分析 | 1 天 | - |
| 2 | 方案设计 | 2 天 | - |
| 3 | 综合脚本创建 | 3 天 | - |
| 4 | 测试环境验证 | 3 天 | - |
| 5 | 生产部署 | 1 天 | - |
| 6 | 监控与调优 | 2 天 | - |
| **总计** | | **12 天** | |

---

## 十一、附录

### A. 脚本分类汇总

| 类别 | 脚本列表 |
|------|----------|
| E2EE | `20260311000006_add_e2ee_tables.sql`, `20260321000001_add_device_trust_tables.sql`, `20260321000003_add_secure_backup_tables.sql` |
| 用户管理 | `20260309000001_password_security_enhancement.sql`, `20260313000000_create_room_tags_and_password_reset_tokens.sql` |
| 房间功能 | `20260311000001_add_space_members_table.sql`, `20260315000006_fix_room_summaries.sql`, `20260316000002_create_room_summary_state.sql` |
| 字段修复 | `20260316000000_comprehensive_migration.sql`, `20260316000001_fix_field_consistency.sql` |
| 安全 | `20260317000001_add_verification_tables.sql`, `20260321000001_add_device_trust_tables.sql` |

### B. 关键表清单

| 表名 | 关键程度 | 行数预估 | 备注 |
|------|----------|----------|------|
| users | 极高 | 10K-100K | 核心用户表 |
| devices | 高 | 100K-1M | 设备表 |
| access_tokens | 高 | 100K-1M | 令牌表 |
| events | 极高 | 10M+ | 事件表 |
| room_memberships | 高 | 1M+ | 成员关系 |

---

## 文档更新记录

| 日期 | 版本 | 更新内容 |
|------|------|----------|
| 2026-03-20 | v1.0.0 | 初始版本 |