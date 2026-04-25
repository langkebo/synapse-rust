# 监控告警指南

> 日期：2026-04-04  
> 版本：v1.0  
> 适用范围：生产环境、预发布环境

---

## 一、概述

本指南提供 synapse-rust 数据库监控和告警的完整方案，包括监控指标、告警规则、可视化仪表板和故障响应流程。

### 1.1 监控目标

1. **可用性监控**：确保数据库服务持续可用
2. **性能监控**：识别性能瓶颈和异常
3. **容量监控**：预测资源需求，避免容量问题
4. **安全监控**：检测异常访问和潜在威胁
5. **数据完整性监控**：确保数据一致性

### 1.2 监控工具栈

| 工具 | 用途 | 部署位置 |
|------|------|---------|
| Prometheus | 指标采集和存储 | 监控服务器 |
| postgres_exporter | PostgreSQL 指标导出 | 数据库服务器 |
| Grafana | 可视化仪表板 | 监控服务器 |
| Alertmanager | 告警路由和通知 | 监控服务器 |
| pgBadger | 日志分析 | 数据库服务器 |
| pg_stat_statements | 查询统计 | 数据库内 |

---

## 二、监控指标

### 2.1 数据库可用性指标

#### 连接状态
```sql
-- 当前连接数
SELECT count(*) AS current_connections
FROM pg_stat_activity
WHERE datname = 'synapse';

-- 按状态分组的连接数
SELECT 
    state,
    count(*) AS count
FROM pg_stat_activity
WHERE datname = 'synapse'
GROUP BY state;

-- 连接使用率
SELECT 
    count(*) AS current_connections,
    current_setting('max_connections')::int AS max_connections,
    round(count(*)::numeric / current_setting('max_connections')::int * 100, 2) AS usage_percent
FROM pg_stat_activity;
```

#### 数据库响应时间
```bash
# 使用 psql 测量响应时间
time psql -h prod-db -U synapse -d synapse -c "SELECT 1;"

# 使用 pg_isready
pg_isready -h prod-db -p 5432 -d synapse
```

### 2.2 性能指标

#### 查询性能
```sql
-- 最慢的查询（需要 pg_stat_statements）
SELECT 
    query,
    calls,
    total_exec_time,
    mean_exec_time,
    max_exec_time,
    stddev_exec_time,
    rows
FROM pg_stat_statements
WHERE query NOT LIKE '%pg_stat_statements%'
ORDER BY mean_exec_time DESC
LIMIT 20;

-- 查询吞吐量
SELECT 
    sum(calls) AS total_queries,
    sum(calls) / extract(epoch from (now() - stats_reset)) AS queries_per_second
FROM pg_stat_statements, pg_stat_database
WHERE datname = 'synapse';
```

#### 缓存命中率
```sql
-- 表缓存命中率
SELECT 
    sum(heap_blks_hit) / nullif(sum(heap_blks_hit) + sum(heap_blks_read), 0) * 100 AS cache_hit_ratio
FROM pg_statio_user_tables;

-- 索引缓存命中率
SELECT 
    sum(idx_blks_hit) / nullif(sum(idx_blks_hit) + sum(idx_blks_read), 0) * 100 AS index_cache_hit_ratio
FROM pg_statio_user_indexes;

-- 数据库级别缓存命中率
SELECT 
    blks_hit::float / nullif(blks_hit + blks_read, 0) * 100 AS cache_hit_ratio
FROM pg_stat_database
WHERE datname = 'synapse';
```

#### 事务性能
```sql
-- 事务统计
SELECT 
    xact_commit,
    xact_rollback,
    xact_commit::float / nullif(xact_commit + xact_rollback, 0) * 100 AS commit_ratio,
    tup_returned,
    tup_fetched,
    tup_inserted,
    tup_updated,
    tup_deleted
FROM pg_stat_database
WHERE datname = 'synapse';

-- 事务速率
SELECT 
    datname,
    xact_commit + xact_rollback AS total_transactions,
    (xact_commit + xact_rollback) / extract(epoch from (now() - stats_reset)) AS tps
FROM pg_stat_database
WHERE datname = 'synapse';
```

### 2.3 资源使用指标

#### 磁盘使用
```sql
-- 数据库大小
SELECT 
    pg_database.datname,
    pg_size_pretty(pg_database_size(pg_database.datname)) AS size
FROM pg_database
WHERE datname = 'synapse';

-- 表大小（Top 20）
SELECT 
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS total_size,
    pg_size_pretty(pg_relation_size(schemaname||'.'||tablename)) AS table_size,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename) - 
                   pg_relation_size(schemaname||'.'||tablename)) AS index_size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC
LIMIT 20;

-- 表空间使用
SELECT 
    spcname,
    pg_size_pretty(pg_tablespace_size(spcname)) AS size
FROM pg_tablespace;
```

#### 表膨胀
```sql
-- 死元组统计
SELECT 
    schemaname,
    tablename,
    n_live_tup,
    n_dead_tup,
    round(n_dead_tup::numeric / nullif(n_live_tup, 0) * 100, 2) AS dead_ratio,
    last_vacuum,
    last_autovacuum,
    last_analyze,
    last_autoanalyze
FROM pg_stat_user_tables
WHERE schemaname = 'public'
ORDER BY n_dead_tup DESC
LIMIT 20;

-- 表膨胀估算
SELECT 
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS total_size,
    round(100 * n_dead_tup / nullif(n_live_tup + n_dead_tup, 0), 2) AS bloat_percent
FROM pg_stat_user_tables
WHERE schemaname = 'public'
AND n_dead_tup > 1000
ORDER BY n_dead_tup DESC;
```

#### 锁和阻塞
```sql
-- 当前锁
SELECT 
    locktype,
    database,
    relation::regclass,
    mode,
    granted,
    count(*)
FROM pg_locks
WHERE database = (SELECT oid FROM pg_database WHERE datname = 'synapse')
GROUP BY locktype, database, relation, mode, granted
ORDER BY count(*) DESC;

-- 阻塞查询
SELECT 
    blocked_locks.pid AS blocked_pid,
    blocked_activity.usename AS blocked_user,
    blocking_locks.pid AS blocking_pid,
    blocking_activity.usename AS blocking_user,
    blocked_activity.query AS blocked_statement,
    blocking_activity.query AS blocking_statement
FROM pg_catalog.pg_locks blocked_locks
JOIN pg_catalog.pg_stat_activity blocked_activity ON blocked_activity.pid = blocked_locks.pid
JOIN pg_catalog.pg_locks blocking_locks 
    ON blocking_locks.locktype = blocked_locks.locktype
    AND blocking_locks.database IS NOT DISTINCT FROM blocked_locks.database
    AND blocking_locks.relation IS NOT DISTINCT FROM blocked_locks.relation
    AND blocking_locks.page IS NOT DISTINCT FROM blocked_locks.page
    AND blocking_locks.tuple IS NOT DISTINCT FROM blocked_locks.tuple
    AND blocking_locks.virtualxid IS NOT DISTINCT FROM blocked_locks.virtualxid
    AND blocking_locks.transactionid IS NOT DISTINCT FROM blocked_locks.transactionid
    AND blocking_locks.classid IS NOT DISTINCT FROM blocked_locks.classid
    AND blocking_locks.objid IS NOT DISTINCT FROM blocked_locks.objid
    AND blocking_locks.objsubid IS NOT DISTINCT FROM blocked_locks.objsubid
    AND blocking_locks.pid != blocked_locks.pid
JOIN pg_catalog.pg_stat_activity blocking_activity ON blocking_activity.pid = blocking_locks.pid
WHERE NOT blocked_locks.granted;
```

### 2.4 复制指标

#### 复制状态
```sql
-- 主库：复制槽状态
SELECT 
    slot_name,
    slot_type,
    database,
    active,
    restart_lsn,
    confirmed_flush_lsn,
    pg_size_pretty(pg_wal_lsn_diff(pg_current_wal_lsn(), restart_lsn)) AS lag_size
FROM pg_replication_slots;

-- 主库：复制连接
SELECT 
    client_addr,
    state,
    sync_state,
    sent_lsn,
    write_lsn,
    flush_lsn,
    replay_lsn,
    pg_wal_lsn_diff(sent_lsn, replay_lsn) AS lag_bytes
FROM pg_stat_replication;

-- 从库：复制延迟
SELECT 
    now() - pg_last_xact_replay_timestamp() AS replication_lag,
    pg_is_in_recovery() AS is_standby,
    pg_last_wal_receive_lsn() AS receive_lsn,
    pg_last_wal_replay_lsn() AS replay_lsn,
    pg_wal_lsn_diff(pg_last_wal_receive_lsn(), pg_last_wal_replay_lsn()) AS replay_lag_bytes
FROM pg_stat_wal_receiver;
```

### 2.5 WAL 指标

```sql
-- WAL 生成速率
SELECT 
    pg_wal_lsn_diff(pg_current_wal_lsn(), '0/0') / 
    extract(epoch from (now() - pg_postmaster_start_time())) AS wal_bytes_per_second;

-- WAL 文件数量
SELECT count(*) AS wal_files
FROM pg_ls_waldir();

-- WAL 归档状态
SELECT 
    archived_count,
    failed_count,
    last_archived_wal,
    last_archived_time,
    last_failed_wal,
    last_failed_time
FROM pg_stat_archiver;
```

---

## 三、监控部署

### 3.1 postgres_exporter 部署

#### 安装
```bash
# 下载 postgres_exporter
wget https://github.com/prometheus-community/postgres_exporter/releases/download/v0.15.0/postgres_exporter-0.15.0.linux-amd64.tar.gz
tar xzf postgres_exporter-0.15.0.linux-amd64.tar.gz
sudo mv postgres_exporter /usr/local/bin/

# 创建监控用户
psql -h prod-db -U postgres -c "
CREATE USER postgres_exporter WITH PASSWORD 'secure_password';
GRANT pg_monitor TO postgres_exporter;
"
```

#### 配置
```bash
# /etc/postgres_exporter/postgres_exporter.env
DATA_SOURCE_NAME="postgresql://postgres_exporter:secure_password@localhost:5432/synapse?sslmode=disable"
PG_EXPORTER_WEB_LISTEN_ADDRESS=":9187"
PG_EXPORTER_WEB_TELEMETRY_PATH="/metrics"
```

#### 自定义查询
```yaml
# /etc/postgres_exporter/queries.yaml
pg_database:
  query: |
    SELECT 
      datname,
      pg_database_size(datname) as size_bytes,
      numbackends as connections
    FROM pg_database
    WHERE datname = 'synapse'
  metrics:
    - datname:
        usage: "LABEL"
        description: "Database name"
    - size_bytes:
        usage: "GAUGE"
        description: "Database size in bytes"
    - connections:
        usage: "GAUGE"
        description: "Number of active connections"

pg_stat_statements_top:
  query: |
    SELECT 
      query,
      calls,
      total_exec_time,
      mean_exec_time,
      max_exec_time
    FROM pg_stat_statements
    WHERE query NOT LIKE '%pg_stat_statements%'
    ORDER BY mean_exec_time DESC
    LIMIT 10
  metrics:
    - query:
        usage: "LABEL"
        description: "Query text"
    - calls:
        usage: "COUNTER"
        description: "Number of times executed"
    - total_exec_time:
        usage: "COUNTER"
        description: "Total execution time in ms"
    - mean_exec_time:
        usage: "GAUGE"
        description: "Mean execution time in ms"
    - max_exec_time:
        usage: "GAUGE"
        description: "Maximum execution time in ms"
```

#### Systemd 服务
```ini
# /etc/systemd/system/postgres_exporter.service
[Unit]
Description=PostgreSQL Exporter
After=network.target

[Service]
Type=simple
User=postgres
EnvironmentFile=/etc/postgres_exporter/postgres_exporter.env
ExecStart=/usr/local/bin/postgres_exporter \
    --extend.query-path=/etc/postgres_exporter/queries.yaml
Restart=always

[Install]
WantedBy=multi-user.target
```

```bash
# 启动服务
sudo systemctl daemon-reload
sudo systemctl enable postgres_exporter
sudo systemctl start postgres_exporter

# 验证
curl http://localhost:9187/metrics | grep pg_
```

### 3.2 Prometheus 配置

```yaml
# /etc/prometheus/prometheus.yml
global:
  scrape_interval: 15s
  evaluation_interval: 15s

scrape_configs:
  - job_name: 'postgresql'
    static_configs:
      - targets: ['prod-db:9187']
        labels:
          instance: 'prod-db'
          environment: 'production'
      
      - targets: ['staging-db:9187']
        labels:
          instance: 'staging-db'
          environment: 'staging'

  - job_name: 'synapse-rust'
    static_configs:
      - targets: ['app-server:28008']
        labels:
          instance: 'app-server'
          environment: 'production'
```

### 3.3 Grafana 仪表板

#### 导入预制仪表板
```bash
# PostgreSQL 官方仪表板
# Dashboard ID: 9628
# https://grafana.com/grafana/dashboards/9628
```

#### 自定义面板示例

**连接数面板**
```sql
-- Prometheus Query
pg_stat_activity_count{datname="synapse"}
```

**缓存命中率面板**
```sql
-- Prometheus Query
rate(pg_stat_database_blks_hit{datname="synapse"}[5m]) / 
(rate(pg_stat_database_blks_hit{datname="synapse"}[5m]) + 
 rate(pg_stat_database_blks_read{datname="synapse"}[5m])) * 100
```

**查询延迟面板**
```sql
-- Prometheus Query
pg_stat_statements_mean_exec_time_seconds{datname="synapse"}
```

---

## 四、告警规则

### 4.1 Prometheus 告警规则

```yaml
# /etc/prometheus/rules/postgresql.yml
groups:
  - name: postgresql_alerts
    interval: 30s
    rules:
      # 数据库不可用
      - alert: PostgreSQLDown
        expr: pg_up == 0
        for: 1m
        labels:
          severity: critical
        annotations:
          summary: "PostgreSQL is down (instance {{ $labels.instance }})"
          description: "PostgreSQL instance {{ $labels.instance }} is down for more than 1 minute."

      # 连接数过高
      - alert: PostgreSQLTooManyConnections
        expr: |
          sum by (instance) (pg_stat_activity_count) / 
          pg_settings_max_connections * 100 > 80
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "PostgreSQL connection usage high (instance {{ $labels.instance }})"
          description: "Connection usage is {{ $value }}% on {{ $labels.instance }}."

      # 缓存命中率低
      - alert: PostgreSQLLowCacheHitRatio
        expr: |
          rate(pg_stat_database_blks_hit{datname="synapse"}[5m]) / 
          (rate(pg_stat_database_blks_hit{datname="synapse"}[5m]) + 
           rate(pg_stat_database_blks_read{datname="synapse"}[5m])) * 100 < 90
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "PostgreSQL low cache hit ratio (instance {{ $labels.instance }})"
          description: "Cache hit ratio is {{ $value }}% on {{ $labels.instance }}."

      # 死元组过多
      - alert: PostgreSQLHighDeadTuples
        expr: |
          (pg_stat_user_tables_n_dead_tup / 
           (pg_stat_user_tables_n_live_tup + pg_stat_user_tables_n_dead_tup)) * 100 > 10
        for: 15m
        labels:
          severity: warning
        annotations:
          summary: "PostgreSQL high dead tuples (instance {{ $labels.instance }})"
          description: "Dead tuple ratio is {{ $value }}% on table {{ $labels.relname }}."

      # 复制延迟
      - alert: PostgreSQLReplicationLag
        expr: |
          pg_replication_lag_seconds > 300
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "PostgreSQL replication lag (instance {{ $labels.instance }})"
          description: "Replication lag is {{ $value }} seconds on {{ $labels.instance }}."

      # 磁盘使用率高
      - alert: PostgreSQLDiskUsageHigh
        expr: |
          (pg_database_size_bytes / node_filesystem_size_bytes) * 100 > 80
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "PostgreSQL disk usage high (instance {{ $labels.instance }})"
          description: "Disk usage is {{ $value }}% on {{ $labels.instance }}."

      # 慢查询
      - alert: PostgreSQLSlowQueries
        expr: |
          pg_stat_statements_mean_exec_time_seconds > 1
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "PostgreSQL slow queries detected (instance {{ $labels.instance }})"
          description: "Query {{ $labels.query }} has mean execution time {{ $value }}s."

      # WAL 归档失败
      - alert: PostgreSQLArchiveFailing
        expr: |
          rate(pg_stat_archiver_failed_count[5m]) > 0
        for: 5m
        labels:
          severity: critical
        annotations:
          summary: "PostgreSQL WAL archiving failing (instance {{ $labels.instance }})"
          description: "WAL archiving is failing on {{ $labels.instance }}."

      # 事务回滚率高
      - alert: PostgreSQLHighRollbackRate
        expr: |
          rate(pg_stat_database_xact_rollback{datname="synapse"}[5m]) / 
          (rate(pg_stat_database_xact_commit{datname="synapse"}[5m]) + 
           rate(pg_stat_database_xact_rollback{datname="synapse"}[5m])) * 100 > 5
        for: 10m
        labels:
          severity: warning
        annotations:
          summary: "PostgreSQL high rollback rate (instance {{ $labels.instance }})"
          description: "Rollback rate is {{ $value }}% on {{ $labels.instance }}."
```

### 4.2 Alertmanager 配置

```yaml
# /etc/alertmanager/alertmanager.yml
global:
  resolve_timeout: 5m
  smtp_smarthost: 'smtp.example.com:587'
  smtp_from: 'alertmanager@example.com'
  smtp_auth_username: 'alertmanager@example.com'
  smtp_auth_password: 'password'

route:
  group_by: ['alertname', 'cluster', 'service']
  group_wait: 10s
  group_interval: 10s
  repeat_interval: 12h
  receiver: 'default'
  routes:
    - match:
        severity: critical
      receiver: 'critical'
      continue: true
    
    - match:
        severity: warning
      receiver: 'warning'

receivers:
  - name: 'default'
    email_configs:
      - to: 'team@example.com'
        headers:
          Subject: '[Monitoring] {{ .GroupLabels.alertname }}'

  - name: 'critical'
    email_configs:
      - to: 'oncall@example.com'
        headers:
          Subject: '[CRITICAL] {{ .GroupLabels.alertname }}'
    slack_configs:
      - api_url: 'https://hooks.slack.com/services/YOUR/SLACK/WEBHOOK'
        channel: '#alerts-critical'
        title: '{{ .GroupLabels.alertname }}'
        text: '{{ range .Alerts }}{{ .Annotations.description }}{{ end }}'

  - name: 'warning'
    email_configs:
      - to: 'team@example.com'
        headers:
          Subject: '[WARNING] {{ .GroupLabels.alertname }}'
    slack_configs:
      - api_url: 'https://hooks.slack.com/services/YOUR/SLACK/WEBHOOK'
        channel: '#alerts-warning'
        title: '{{ .GroupLabels.alertname }}'
        text: '{{ range .Alerts }}{{ .Annotations.description }}{{ end }}'

inhibit_rules:
  - source_match:
      severity: 'critical'
    target_match:
      severity: 'warning'
    equal: ['alertname', 'instance']
```

---

## 五、日志监控

### 5.1 PostgreSQL 日志配置

```ini
# postgresql.conf

# 日志目的地
logging_collector = on
log_directory = '/var/log/postgresql'
log_filename = 'postgresql-%Y-%m-%d_%H%M%S.log'
log_rotation_age = 1d
log_rotation_size = 100MB

# 日志内容
log_line_prefix = '%t [%p]: [%l-1] user=%u,db=%d,app=%a,client=%h '
log_checkpoints = on
log_connections = on
log_disconnections = on
log_duration = off
log_lock_waits = on
log_statement = 'ddl'
log_temp_files = 0

# 慢查询日志
log_min_duration_statement = 1000  # 记录超过 1 秒的查询
```

### 5.2 pgBadger 日志分析

```bash
# 安装 pgBadger
sudo apt-get install pgbadger

# 生成报告
pgbadger /var/log/postgresql/postgresql-*.log \
    --prefix '%t [%p]: [%l-1] user=%u,db=%d,app=%a,client=%h ' \
    --outfile /var/www/html/pgbadger/report.html \
    --title "PostgreSQL Performance Report"

# 定期生成报告（cron）
0 1 * * * pgbadger /var/log/postgresql/postgresql-$(date -d yesterday +\%Y-\%m-\%d)*.log -o /var/www/html/pgbadger/report-$(date -d yesterday +\%Y-\%m-\%d).html
```

---

## 六、故障响应流程

### 6.1 告警响应流程

```
告警触发
    ↓
确认告警（1分钟内）
    ↓
评估严重程度
    ↓
├─ Critical → 立即响应，通知 oncall
├─ Warning → 15分钟内响应
└─ Info → 记录，定期审查
    ↓
诊断问题
    ↓
执行修复
    ↓
验证修复
    ↓
记录事件
    ↓
事后分析
```

### 6.2 常见告警处理

#### PostgreSQLDown
```bash
# 1. 检查进程
ps aux | grep postgres

# 2. 检查日志
tail -100 /var/log/postgresql/postgresql-*.log

# 3. 尝试启动
systemctl start postgresql

# 4. 如果无法启动，检查配置
pg_ctl -D /var/lib/postgresql/data configtest
```

#### PostgreSQLTooManyConnections
```bash
# 1. 查看当前连接
psql -c "SELECT count(*), state FROM pg_stat_activity GROUP BY state;"

# 2. 终止空闲连接
psql -c "SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE state = 'idle' AND state_change < now() - interval '10 minutes';"

# 3. 检查应用连接池配置
```

#### PostgreSQLLowCacheHitRatio
```bash
# 1. 检查 shared_buffers 配置
psql -c "SHOW shared_buffers;"

# 2. 检查系统内存
free -h

# 3. 考虑增加 shared_buffers
# 编辑 postgresql.conf
# shared_buffers = 4GB
```

---

## 七、性能报告

### 7.1 日报生成

```bash
#!/bin/bash
# daily_report.sh

REPORT_DATE=$(date +%Y-%m-%d)
REPORT_FILE="/var/reports/postgresql_daily_$REPORT_DATE.md"

cat > "$REPORT_FILE" <<EOF
# PostgreSQL Daily Report - $REPORT_DATE

## Database Size
$(psql -h prod-db -U synapse -d synapse -c "SELECT pg_size_pretty(pg_database_size('synapse'));")

## Connection Statistics
$(psql -h prod-db -U synapse -d synapse -c "SELECT state, count(*) FROM pg_stat_activity WHERE datname = 'synapse' GROUP BY state;")

## Top 10 Slowest Queries
$(psql -h prod-db -U synapse -d synapse -c "SELECT query, calls, mean_exec_time FROM pg_stat_statements ORDER BY mean_exec_time DESC LIMIT 10;")

## Cache Hit Ratio
$(psql -h prod-db -U synapse -d synapse -c "SELECT round(sum(heap_blks_hit) / nullif(sum(heap_blks_hit) + sum(heap_blks_read), 0) * 100, 2) AS cache_hit_ratio FROM pg_statio_user_tables;")

## Table Sizes (Top 10)
$(psql -h prod-db -U synapse -d synapse -c "SELECT tablename, pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS size FROM pg_tables WHERE schemaname = 'public' ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC LIMIT 10;")

EOF

# 发送报告
mail -s "PostgreSQL Daily Report - $REPORT_DATE" team@example.com < "$REPORT_FILE"
```

### 7.2 周报生成

```bash
#!/bin/bash
# weekly_report.sh

WEEK_START=$(date -d "last monday" +%Y-%m-%d)
WEEK_END=$(date -d "next sunday" +%Y-%m-%d)
REPORT_FILE="/var/reports/postgresql_weekly_${WEEK_START}_${WEEK_END}.md"

# 生成 pgBadger 周报
pgbadger /var/log/postgresql/postgresql-*.log \
    --begin "$WEEK_START" \
    --end "$WEEK_END" \
    --outfile "$REPORT_FILE"

# 发送报告
mail -s "PostgreSQL Weekly Report - $WEEK_START to $WEEK_END" team@example.com < "$REPORT_FILE"
```

---

## 八、参考资料

- [性能优化指南](PERFORMANCE_OPTIMIZATION_GUIDE.md)
- [灾难恢复指南](DISASTER_RECOVERY_GUIDE.md)
- [Prometheus 文档](https://prometheus.io/docs/)
- [Grafana 文档](https://grafana.com/docs/)
- [postgres_exporter](https://github.com/prometheus-community/postgres_exporter)

---

**文档版本**：v1.0  
**创建日期**：2026-04-04  
**维护者**：数据库团队  
**审核者**：运维团队
