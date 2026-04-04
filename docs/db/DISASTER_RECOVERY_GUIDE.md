# 灾难恢复指南

> 日期：2026-04-04  
> 版本：v1.0  
> 适用范围：生产环境、预发布环境

---

## 一、概述

本指南提供 synapse-rust 数据库灾难恢复的完整流程，包括备份策略、恢复程序、故障场景处理和演练计划。

### 1.1 灾难恢复目标

- **RTO (Recovery Time Objective)**：目标恢复时间 < 1 小时
- **RPO (Recovery Point Objective)**：目标恢复点 < 5 分钟
- **数据完整性**：确保恢复后数据一致性
- **业务连续性**：最小化服务中断时间

### 1.2 灾难场景分类

| 场景 | 严重程度 | RTO | RPO | 恢复策略 |
|------|---------|-----|-----|---------|
| 单表数据损坏 | 低 | 15分钟 | 5分钟 | 从备份恢复单表 |
| 数据库实例故障 | 中 | 30分钟 | 5分钟 | 主从切换 |
| 数据中心故障 | 高 | 1小时 | 5分钟 | 跨区域恢复 |
| 数据完全丢失 | 严重 | 2小时 | 最后备份点 | 完整恢复 |

---

## 二、备份策略

### 2.1 备份类型

#### 完整备份（Full Backup）
```bash
# 使用 pg_dump 进行完整备份
pg_dump -h prod-db -U synapse -d synapse \
    -Fc \
    -f /backup/synapse_full_$(date +%Y%m%d_%H%M%S).dump

# 使用 pg_basebackup 进行物理备份
pg_basebackup -h prod-db -U replication \
    -D /backup/basebackup_$(date +%Y%m%d_%H%M%S) \
    -Ft -z -P
```

#### 增量备份（Incremental Backup）
```bash
# 使用 WAL 归档实现增量备份
# postgresql.conf 配置
archive_mode = on
archive_command = 'cp %p /backup/wal_archive/%f'
wal_level = replica
```

#### 差异备份（Differential Backup）
```bash
# 使用 pgBackRest
pgbackrest --stanza=synapse --type=diff backup
```

### 2.2 备份计划

#### 生产环境备份计划
```bash
# 每日完整备份（凌晨 2:00）
0 2 * * * /usr/local/bin/backup_full.sh

# 每小时增量备份（WAL 归档）
# 自动归档，无需 cron

# 每周验证备份（周日 3:00）
0 3 * * 0 /usr/local/bin/verify_backup.sh
```

#### 备份脚本示例
```bash
#!/bin/bash
# backup_full.sh - 完整备份脚本

set -e

BACKUP_DIR="/backup/postgresql"
RETENTION_DAYS=30
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_FILE="$BACKUP_DIR/synapse_full_$TIMESTAMP.dump"

# 创建备份目录
mkdir -p "$BACKUP_DIR"

# 执行备份
echo "Starting full backup at $(date)"
pg_dump -h prod-db -U synapse -d synapse \
    -Fc \
    -f "$BACKUP_FILE"

# 验证备份
if [ -f "$BACKUP_FILE" ]; then
    SIZE=$(du -h "$BACKUP_FILE" | cut -f1)
    echo "Backup completed: $BACKUP_FILE ($SIZE)"
else
    echo "ERROR: Backup failed"
    exit 1
fi

# 清理旧备份
find "$BACKUP_DIR" -name "synapse_full_*.dump" -mtime +$RETENTION_DAYS -delete

# 上传到远程存储（S3/OSS）
aws s3 cp "$BACKUP_FILE" s3://synapse-backups/postgresql/

echo "Backup completed successfully at $(date)"
```

### 2.3 备份验证

#### 自动验证脚本
```bash
#!/bin/bash
# verify_backup.sh - 备份验证脚本

set -e

BACKUP_FILE="$1"
TEST_DB="synapse_backup_test"

echo "Verifying backup: $BACKUP_FILE"

# 创建测试数据库
psql -h localhost -U postgres -c "DROP DATABASE IF EXISTS $TEST_DB;"
psql -h localhost -U postgres -c "CREATE DATABASE $TEST_DB OWNER synapse;"

# 恢复备份到测试数据库
pg_restore -h localhost -U synapse -d $TEST_DB "$BACKUP_FILE"

# 验证关键表
TABLES=("users" "rooms" "events" "devices")
for table in "${TABLES[@]}"; do
    COUNT=$(psql -h localhost -U synapse -d $TEST_DB -t -c "SELECT COUNT(*) FROM $table;")
    echo "Table $table: $COUNT rows"
    
    if [ "$COUNT" -eq 0 ]; then
        echo "WARNING: Table $table is empty"
    fi
done

# 清理测试数据库
psql -h localhost -U postgres -c "DROP DATABASE $TEST_DB;"

echo "Backup verification completed successfully"
```

### 2.4 备份存储

#### 本地存储
```bash
# 备份目录结构
/backup/postgresql/
├── full/
│   ├── synapse_full_20260404_020000.dump
│   └── synapse_full_20260403_020000.dump
├── wal_archive/
│   ├── 000000010000000000000001
│   └── 000000010000000000000002
└── logs/
    └── backup_20260404.log
```

#### 远程存储（S3/OSS）
```bash
# 上传到 S3
aws s3 sync /backup/postgresql/ s3://synapse-backups/postgresql/ \
    --exclude "*.tmp" \
    --storage-class STANDARD_IA

# 设置生命周期策略
# - 30 天后转移到 Glacier
# - 90 天后删除
```

---

## 三、恢复程序

### 3.1 完整恢复

#### 场景：数据库完全丢失

```bash
# 1. 停止应用
systemctl stop synapse-rust

# 2. 创建新数据库
psql -h prod-db -U postgres -c "DROP DATABASE IF EXISTS synapse;"
psql -h prod-db -U postgres -c "CREATE DATABASE synapse OWNER synapse;"

# 3. 恢复最新完整备份
LATEST_BACKUP=$(ls -t /backup/postgresql/full/synapse_full_*.dump | head -1)
echo "Restoring from: $LATEST_BACKUP"

pg_restore -h prod-db -U synapse -d synapse \
    --verbose \
    --no-owner \
    --no-acl \
    "$LATEST_BACKUP"

# 4. 应用 WAL 归档（如果有）
# 恢复到最新状态
# (需要配置 recovery.conf)

# 5. 验证恢复
psql -h prod-db -U synapse -d synapse -c "SELECT COUNT(*) FROM users;"
psql -h prod-db -U synapse -d synapse -c "SELECT COUNT(*) FROM rooms;"
psql -h prod-db -U synapse -d synapse -c "SELECT COUNT(*) FROM events;"

# 6. 运行 Schema 验证
bash scripts/validate_schema_all.sh

# 7. 启动应用
systemctl start synapse-rust

# 8. 验证服务
curl http://localhost:8008/_matrix/client/versions
```

### 3.2 时间点恢复（PITR）

#### 场景：恢复到特定时间点

```bash
# 1. 停止应用
systemctl stop synapse-rust

# 2. 恢复基础备份
pg_restore -h prod-db -U synapse -d synapse "$LATEST_BACKUP"

# 3. 配置恢复目标时间
cat > /var/lib/postgresql/data/recovery.conf <<EOF
restore_command = 'cp /backup/wal_archive/%f %p'
recovery_target_time = '2026-04-04 14:30:00'
recovery_target_action = 'promote'
EOF

# 4. 启动 PostgreSQL
systemctl start postgresql

# 5. 等待恢复完成
tail -f /var/log/postgresql/postgresql.log

# 6. 验证恢复点
psql -h prod-db -U synapse -d synapse -c "SELECT now();"

# 7. 启动应用
systemctl start synapse-rust
```

### 3.3 单表恢复

#### 场景：单个表数据损坏

```bash
# 1. 创建临时数据库
psql -h prod-db -U postgres -c "CREATE DATABASE synapse_temp OWNER synapse;"

# 2. 恢复备份到临时数据库
pg_restore -h prod-db -U synapse -d synapse_temp "$LATEST_BACKUP"

# 3. 导出目标表数据
pg_dump -h prod-db -U synapse -d synapse_temp \
    -t user_preferences \
    --data-only \
    -f /tmp/user_preferences_data.sql

# 4. 在生产数据库中清空表
psql -h prod-db -U synapse -d synapse -c "TRUNCATE TABLE user_preferences CASCADE;"

# 5. 导入数据
psql -h prod-db -U synapse -d synapse -f /tmp/user_preferences_data.sql

# 6. 验证数据
psql -h prod-db -U synapse -d synapse -c "SELECT COUNT(*) FROM user_preferences;"

# 7. 清理临时数据库
psql -h prod-db -U postgres -c "DROP DATABASE synapse_temp;"
```

### 3.4 主从切换

#### 场景：主库故障，切换到从库

```bash
# 1. 确认主库不可用
pg_isready -h prod-db-master -p 5432 || echo "Master is down"

# 2. 提升从库为主库
pg_ctl promote -D /var/lib/postgresql/data

# 或使用 pg_ctlcluster
pg_ctlcluster 15 main promote

# 3. 更新应用配置
# 修改 DATABASE_URL 指向新主库
export DATABASE_URL="postgresql://synapse:password@prod-db-standby:5432/synapse"

# 4. 重启应用
systemctl restart synapse-rust

# 5. 验证服务
curl http://localhost:8008/_matrix/client/versions

# 6. 配置新的从库（可选）
# 将旧主库修复后配置为新从库
```

---

## 四、故障场景处理

### 4.1 数据损坏

#### 症状
```
ERROR: invalid page in block 12345 of relation base/16384/67890
```

#### 处理步骤
```bash
# 1. 识别损坏的表
psql -h prod-db -U synapse -d synapse -c "
SELECT 
    c.relname,
    c.relfilenode
FROM pg_class c
WHERE c.relfilenode = 67890;
"

# 2. 尝试使用 pg_amcheck 检查
pg_amcheck -h prod-db -U synapse -d synapse

# 3. 如果损坏严重，从备份恢复该表
# (参考 3.3 单表恢复)

# 4. 如果是索引损坏，重建索引
REINDEX TABLE CONCURRENTLY table_name;
```

### 4.2 磁盘空间不足

#### 症状
```
ERROR: could not extend file: No space left on device
```

#### 紧急处理
```bash
# 1. 检查磁盘使用
df -h

# 2. 清理 WAL 归档
find /backup/wal_archive -mtime +7 -delete

# 3. 清理临时文件
rm -rf /var/lib/postgresql/data/base/pgsql_tmp/*

# 4. VACUUM 回收空间
VACUUM FULL;

# 5. 扩展磁盘（如果可能）
# 或迁移到更大的磁盘
```

### 4.3 连接数耗尽

#### 症状
```
FATAL: sorry, too many clients already
```

#### 处理步骤
```bash
# 1. 查看当前连接
psql -h prod-db -U synapse -d synapse -c "
SELECT 
    count(*),
    state,
    application_name
FROM pg_stat_activity
GROUP BY state, application_name;
"

# 2. 终止空闲连接
psql -h prod-db -U synapse -d synapse -c "
SELECT pg_terminate_backend(pid)
FROM pg_stat_activity
WHERE state = 'idle'
AND state_change < now() - interval '10 minutes';
"

# 3. 增加最大连接数（临时）
ALTER SYSTEM SET max_connections = 300;
SELECT pg_reload_conf();

# 4. 配置连接池（长期解决方案）
# 使用 PgBouncer
```

### 4.4 复制延迟

#### 症状
```
从库延迟主库 > 1 分钟
```

#### 处理步骤
```bash
# 1. 检查复制延迟
psql -h prod-db-standby -U synapse -d synapse -c "
SELECT 
    now() - pg_last_xact_replay_timestamp() AS replication_lag;
"

# 2. 检查复制槽
psql -h prod-db-master -U synapse -d synapse -c "
SELECT * FROM pg_replication_slots;
"

# 3. 检查网络和 I/O
# 网络带宽、磁盘 I/O

# 4. 如果延迟过大，考虑重建从库
pg_basebackup -h prod-db-master -U replication \
    -D /var/lib/postgresql/data \
    -Fp -Xs -P -R
```

---

## 五、灾难恢复演练

### 5.1 演练计划

#### 季度演练计划
- **Q1**：完整恢复演练
- **Q2**：时间点恢复演练
- **Q3**：主从切换演练
- **Q4**：跨区域恢复演练

### 5.2 演练流程

#### 完整恢复演练
```bash
# 1. 准备演练环境
# 使用独立的测试环境

# 2. 模拟灾难
# 删除测试数据库

# 3. 执行恢复
# 按照恢复程序操作

# 4. 验证恢复
# 检查数据完整性

# 5. 记录演练结果
# 记录 RTO、RPO、遇到的问题

# 6. 改进恢复流程
# 根据演练结果优化流程
```

### 5.3 演练检查清单

- [ ] 备份文件可访问
- [ ] 恢复脚本可执行
- [ ] 恢复时间符合 RTO
- [ ] 数据完整性验证通过
- [ ] 应用启动成功
- [ ] 服务功能正常
- [ ] 记录演练时间和结果
- [ ] 识别改进点

---

## 六、监控和告警

### 6.1 备份监控

```bash
# 检查最近备份时间
LAST_BACKUP=$(ls -t /backup/postgresql/full/synapse_full_*.dump | head -1)
BACKUP_AGE=$(( ($(date +%s) - $(stat -c %Y "$LAST_BACKUP")) / 3600 ))

if [ $BACKUP_AGE -gt 24 ]; then
    echo "ALERT: Last backup is $BACKUP_AGE hours old"
fi

# 检查备份大小
BACKUP_SIZE=$(du -b "$LAST_BACKUP" | cut -f1)
MIN_SIZE=$((1024*1024*100))  # 100MB

if [ $BACKUP_SIZE -lt $MIN_SIZE ]; then
    echo "ALERT: Backup size is too small: $BACKUP_SIZE bytes"
fi
```

### 6.2 复制监控

```sql
-- 主库：检查复制槽
SELECT 
    slot_name,
    active,
    restart_lsn,
    pg_size_pretty(pg_wal_lsn_diff(pg_current_wal_lsn(), restart_lsn)) AS lag_size
FROM pg_replication_slots;

-- 从库：检查复制延迟
SELECT 
    now() - pg_last_xact_replay_timestamp() AS replication_lag,
    pg_is_in_recovery() AS is_standby;
```

### 6.3 告警规则

| 指标 | 阈值 | 严重程度 | 处理 |
|------|------|---------|------|
| 备份失败 | 1 次 | 严重 | 立即调查 |
| 备份延迟 | > 24 小时 | 高 | 检查备份任务 |
| 复制延迟 | > 5 分钟 | 中 | 检查网络和 I/O |
| 磁盘使用率 | > 80% | 高 | 清理或扩容 |
| WAL 归档失败 | > 10 次 | 严重 | 检查归档配置 |

---

## 七、最佳实践

### 7.1 备份最佳实践

1. **3-2-1 规则**
   - 3 份数据副本
   - 2 种不同存储介质
   - 1 份异地备份

2. **定期验证备份**
   - 每周验证备份可恢复性
   - 记录验证结果

3. **自动化备份**
   - 使用 cron 或调度系统
   - 监控备份任务状态

4. **加密备份**
   ```bash
   # 加密备份文件
   gpg --encrypt --recipient backup@example.com synapse_full.dump
   ```

5. **文档化流程**
   - 维护最新的恢复文档
   - 记录配置变更

### 7.2 恢复最佳实践

1. **先在测试环境验证**
   - 不要直接在生产环境操作
   - 验证恢复流程

2. **保留原始数据**
   - 恢复前备份当前状态
   - 以防恢复失败

3. **记录操作步骤**
   - 记录每个操作和结果
   - 便于事后分析

4. **通知相关人员**
   - 通知用户服务中断
   - 通知团队成员

5. **验证数据完整性**
   - 检查关键表行数
   - 验证业务功能

---

## 八、应急联系

### 8.1 联系人

| 角色 | 姓名 | 电话 | 邮箱 |
|------|------|------|------|
| DBA 主管 | - | - | dba-lead@example.com |
| 值班 DBA | - | - | dba-oncall@example.com |
| 运维主管 | - | - | ops-lead@example.com |
| 技术总监 | - | - | cto@example.com |

### 8.2 升级流程

1. **P0 - 严重**：数据库完全不可用
   - 立即通知 DBA 主管和技术总监
   - 启动应急响应

2. **P1 - 高**：部分功能不可用
   - 通知值班 DBA
   - 1 小时内响应

3. **P2 - 中**：性能下降
   - 通知值班 DBA
   - 4 小时内响应

---

## 九、参考资料

- [迁移操作指南](MIGRATION_OPERATIONS_GUIDE.md)
- [监控告警指南](MONITORING_GUIDE.md)
- [PostgreSQL 备份文档](https://www.postgresql.org/docs/current/backup.html)
- [PostgreSQL PITR 文档](https://www.postgresql.org/docs/current/continuous-archiving.html)

---

**文档版本**：v1.0  
**创建日期**：2026-04-04  
**维护者**：数据库团队  
**审核者**：运维团队
