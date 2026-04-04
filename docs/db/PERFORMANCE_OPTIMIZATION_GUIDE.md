# 性能优化指南

> 日期：2026-04-04  
> 版本：v1.0  
> 适用范围：数据库性能调优、查询优化、索引优化

---

## 一、概述

本指南提供 synapse-rust 数据库性能优化的系统方法，包括性能基准测试、瓶颈识别、优化策略和验证方法。

### 1.1 性能优化原则

1. **测量先行**：优化前建立性能基准
2. **数据驱动**：基于实际数据和负载优化
3. **渐进式优化**：一次优化一个瓶颈
4. **验证效果**：每次优化后验证性能提升
5. **避免过度优化**：平衡性能和可维护性

### 1.2 性能优化工具链

| 工具 | 用途 | 使用场景 |
|------|------|---------|
| `benches/run_benchmarks.sh` | 性能基准测试 | 建立基准、对比优化效果 |
| `scripts/generate_benchmark_data.sh` | 生成测试数据 | 模拟真实负载 |
| `EXPLAIN ANALYZE` | 查询计划分析 | 识别慢查询瓶颈 |
| `pg_stat_statements` | 查询统计 | 找出最耗时查询 |
| `pgBadger` | 日志分析 | 生成性能报告 |

---

## 二、性能基准测试

### 2.1 建立性能基准

```bash
# 1. 生成测试数据（中等规模）
bash scripts/generate_benchmark_data.sh preset medium

# 2. 运行完整基准测试
bash benches/run_benchmarks.sh full

# 3. 保存基准报告
cp benches/results/BASELINE_REPORT_*.md \
   benches/results/baseline_before_optimization.md
```

### 2.2 基准测试套件

#### API 基准测试
```bash
# 测试客户端 API 性能
cargo bench --bench performance_api_benchmarks
```

测试覆盖：
- 用户注册和登录
- 房间创建和加入
- 消息发送和接收
- 同步操作

#### Federation 基准测试
```bash
# 测试联邦协议性能
cargo bench --bench performance_federation_benchmarks
```

测试覆盖：
- 联邦查询
- 事件传播
- 服务器发现

#### Database 基准测试
```bash
# 测试数据库查询性能
cargo bench --bench performance_database_benchmarks
```

测试覆盖：
- 用户查询（按 ID、按用户名）
- 房间查询（按 ID、公开房间列表）
- 事件查询（按 ID、最近事件、时间范围）
- 设备查询（按用户、按设备 ID）
- 批量插入（不同批次大小）
- 索引效率

### 2.3 解读基准测试结果

Criterion 输出示例：
```
database/user_query/by_user_id
                        time:   [1.2345 ms 1.2567 ms 1.2789 ms]
                        thrpt:  [782.15 elem/s 795.73 elem/s 809.31 elem/s]
                        change: [-5.2% -3.1% -1.0%] (p = 0.01 < 0.05)
                        Performance has improved.
```

关键指标：
- **time**: 平均执行时间及 95% 置信区间
- **thrpt**: 吞吐量（每秒操作数）
- **change**: 与上次基准的变化百分比
- **p-value**: 统计显著性（< 0.05 表示显著变化）

---

## 三、性能瓶颈识别

### 3.1 启用查询统计

```sql
-- 启用 pg_stat_statements 扩展
CREATE EXTENSION IF NOT EXISTS pg_stat_statements;

-- 重置统计
SELECT pg_stat_statements_reset();
```

### 3.2 识别慢查询

```sql
-- 查找最耗时的查询（总时间）
SELECT 
    query,
    calls,
    total_exec_time,
    mean_exec_time,
    max_exec_time,
    stddev_exec_time
FROM pg_stat_statements
WHERE query NOT LIKE '%pg_stat_statements%'
ORDER BY total_exec_time DESC
LIMIT 20;

-- 查找平均最慢的查询
SELECT 
    query,
    calls,
    mean_exec_time,
    max_exec_time
FROM pg_stat_statements
WHERE query NOT LIKE '%pg_stat_statements%'
AND calls > 10
ORDER BY mean_exec_time DESC
LIMIT 20;
```

### 3.3 分析查询计划

```sql
-- 使用 EXPLAIN ANALYZE 分析慢查询
EXPLAIN (ANALYZE, BUFFERS, VERBOSE) 
SELECT * FROM events 
WHERE room_id = '!room:example.com' 
ORDER BY origin_server_ts DESC 
LIMIT 50;
```

关键指标：
- **Seq Scan**: 全表扫描（通常需要优化）
- **Index Scan**: 索引扫描（理想情况）
- **Execution Time**: 实际执行时间
- **Planning Time**: 查询规划时间
- **Buffers**: 缓冲区使用情况

### 3.4 识别缺失索引

```sql
-- 查找没有索引的外键
SELECT 
    c.conrelid::regclass AS table_name,
    a.attname AS column_name,
    c.confrelid::regclass AS referenced_table
FROM pg_constraint c
JOIN pg_attribute a ON a.attnum = ANY(c.conkey) AND a.attrelid = c.conrelid
WHERE c.contype = 'f'
AND NOT EXISTS (
    SELECT 1 FROM pg_index i
    WHERE i.indrelid = c.conrelid
    AND a.attnum = ANY(i.indkey)
)
ORDER BY table_name, column_name;

-- 查找从未使用的索引
SELECT 
    schemaname,
    tablename,
    indexname,
    idx_scan,
    pg_size_pretty(pg_relation_size(indexrelid)) AS index_size
FROM pg_stat_user_indexes
WHERE idx_scan = 0
AND schemaname = 'public'
ORDER BY pg_relation_size(indexrelid) DESC;
```

---

## 四、索引优化

### 4.1 索引设计原则

1. **选择性高的列优先**：区分度高的列更适合索引
2. **覆盖索引**：包含查询所需的所有列
3. **复合索引顺序**：等值条件在前，范围条件在后
4. **避免过度索引**：每个索引都有维护成本

### 4.2 常见索引模式

#### 单列索引
```sql
-- 用于简单查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_users_username 
    ON users(username);
```

#### 复合索引
```sql
-- 用于多条件查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_room_time 
    ON events(room_id, origin_server_ts DESC);
```

#### 覆盖索引
```sql
-- 包含查询所需的所有列，避免回表
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_room_time_covering
    ON events(room_id, origin_server_ts DESC) 
    INCLUDE (event_id, sender, event_type, content);
```

#### 部分索引
```sql
-- 只索引满足条件的行
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_rooms_public 
    ON rooms(created_ts DESC) 
    WHERE is_public = true;
```

#### 表达式索引
```sql
-- 索引计算结果
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_users_username_lower 
    ON users(LOWER(username));
```

#### JSONB 索引
```sql
-- GIN 索引用于 JSONB 查询
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_content_gin 
    ON events USING GIN (content);

-- 特定路径索引
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_content_type 
    ON events((content->>'type'));
```

### 4.3 索引优化案例

#### 案例 1：优化房间事件查询

**问题**：查询房间最近事件很慢
```sql
SELECT event_id, sender, event_type, content, origin_server_ts
FROM events
WHERE room_id = '!room:example.com'
ORDER BY origin_server_ts DESC
LIMIT 50;
```

**分析**：
```sql
EXPLAIN ANALYZE
SELECT event_id, sender, event_type, content, origin_server_ts
FROM events
WHERE room_id = '!room:example.com'
ORDER BY origin_server_ts DESC
LIMIT 50;

-- 结果显示：Seq Scan on events (cost=0.00..1234567.89)
```

**优化**：创建覆盖索引
```sql
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_events_room_time_covering
    ON events(room_id, origin_server_ts DESC) 
    INCLUDE (event_id, sender, event_type, content);
```

**验证**：
```sql
EXPLAIN ANALYZE
SELECT event_id, sender, event_type, content, origin_server_ts
FROM events
WHERE room_id = '!room:example.com'
ORDER BY origin_server_ts DESC
LIMIT 50;

-- 结果显示：Index Only Scan using idx_events_room_time_covering
-- 执行时间从 1234ms 降至 5ms
```

#### 案例 2：优化用户设备查询

**问题**：查询用户所有设备慢
```sql
SELECT device_id, display_name, last_seen_ts
FROM devices
WHERE user_id = '@user:example.com';
```

**优化**：
```sql
-- 创建索引
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_devices_user_id 
    ON devices(user_id);

-- 进一步优化：覆盖索引
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_devices_user_covering
    ON devices(user_id) 
    INCLUDE (device_id, display_name, last_seen_ts);
```

#### 案例 3：优化公开房间列表

**问题**：获取公开房间列表慢
```sql
SELECT room_id, creator, created_ts, name
FROM rooms
WHERE is_public = true
ORDER BY created_ts DESC
LIMIT 20;
```

**优化**：部分索引
```sql
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_rooms_public_created
    ON rooms(created_ts DESC) 
    WHERE is_public = true
    INCLUDE (room_id, creator, name);
```

### 4.4 索引维护

#### 重建索引
```sql
-- 并发重建索引（不锁表）
REINDEX INDEX CONCURRENTLY idx_events_room_time_covering;

-- 重建表的所有索引
REINDEX TABLE CONCURRENTLY events;
```

#### 删除未使用的索引
```sql
-- 删除从未使用的索引
DROP INDEX CONCURRENTLY IF EXISTS idx_unused_index;
```

#### 监控索引膨胀
```sql
SELECT 
    schemaname,
    tablename,
    indexname,
    pg_size_pretty(pg_relation_size(indexrelid)) AS index_size,
    idx_scan,
    idx_tup_read,
    idx_tup_fetch
FROM pg_stat_user_indexes
WHERE schemaname = 'public'
ORDER BY pg_relation_size(indexrelid) DESC;
```

---

## 五、查询优化

### 5.1 查询优化技巧

#### 1. 避免 SELECT *
```sql
-- ❌ 不推荐
SELECT * FROM events WHERE room_id = '!room:example.com';

-- ✅ 推荐
SELECT event_id, sender, event_type, content 
FROM events 
WHERE room_id = '!room:example.com';
```

#### 2. 使用 LIMIT
```sql
-- ❌ 不推荐
SELECT * FROM events WHERE room_id = '!room:example.com' ORDER BY origin_server_ts DESC;

-- ✅ 推荐
SELECT event_id, sender, event_type 
FROM events 
WHERE room_id = '!room:example.com' 
ORDER BY origin_server_ts DESC 
LIMIT 50;
```

#### 3. 避免函数包裹索引列
```sql
-- ❌ 不推荐（无法使用索引）
SELECT * FROM users WHERE LOWER(username) = 'alice';

-- ✅ 推荐（使用表达式索引）
CREATE INDEX idx_users_username_lower ON users(LOWER(username));
SELECT * FROM users WHERE LOWER(username) = 'alice';

-- ✅ 或者在应用层处理
SELECT * FROM users WHERE username = 'alice';
```

#### 4. 使用 EXISTS 代替 IN
```sql
-- ❌ 不推荐（子查询返回大量数据）
SELECT * FROM users 
WHERE user_id IN (SELECT user_id FROM room_memberships WHERE room_id = '!room:example.com');

-- ✅ 推荐
SELECT u.* FROM users u
WHERE EXISTS (
    SELECT 1 FROM room_memberships rm 
    WHERE rm.user_id = u.user_id 
    AND rm.room_id = '!room:example.com'
);
```

#### 5. 批量操作
```sql
-- ❌ 不推荐（多次单行插入）
INSERT INTO devices (device_id, user_id, display_name) VALUES ('D1', '@user:example.com', 'Device 1');
INSERT INTO devices (device_id, user_id, display_name) VALUES ('D2', '@user:example.com', 'Device 2');

-- ✅ 推荐（批量插入）
INSERT INTO devices (device_id, user_id, display_name) VALUES
    ('D1', '@user:example.com', 'Device 1'),
    ('D2', '@user:example.com', 'Device 2'),
    ('D3', '@user:example.com', 'Device 3');
```

### 5.2 JOIN 优化

#### 选择正确的 JOIN 类型
```sql
-- INNER JOIN：只返回匹配的行
SELECT e.*, r.name 
FROM events e
INNER JOIN rooms r ON e.room_id = r.room_id
WHERE e.event_type = 'm.room.message';

-- LEFT JOIN：返回左表所有行
SELECT u.user_id, COUNT(d.device_id) AS device_count
FROM users u
LEFT JOIN devices d ON u.user_id = d.user_id
GROUP BY u.user_id;
```

#### JOIN 顺序优化
```sql
-- PostgreSQL 会自动优化 JOIN 顺序，但可以使用 EXPLAIN 验证
EXPLAIN ANALYZE
SELECT e.event_id, r.name, u.username
FROM events e
JOIN rooms r ON e.room_id = r.room_id
JOIN users u ON e.sender = u.user_id
WHERE e.event_type = 'm.room.message'
LIMIT 100;
```

### 5.3 聚合查询优化

#### 使用物化视图
```sql
-- 创建物化视图缓存聚合结果
CREATE MATERIALIZED VIEW room_stats AS
SELECT 
    room_id,
    COUNT(*) AS event_count,
    COUNT(DISTINCT sender) AS user_count,
    MAX(origin_server_ts) AS last_activity
FROM events
GROUP BY room_id;

-- 创建索引
CREATE INDEX idx_room_stats_room ON room_stats(room_id);

-- 定期刷新
REFRESH MATERIALIZED VIEW CONCURRENTLY room_stats;
```

#### 使用增量聚合
```sql
-- 维护聚合表
CREATE TABLE room_summary_stats (
    room_id TEXT PRIMARY KEY,
    total_events BIGINT DEFAULT 0,
    total_messages BIGINT DEFAULT 0,
    last_updated_ts BIGINT NOT NULL
);

-- 在插入事件时更新统计
-- (在应用层或触发器中实现)
```

---

## 六、数据库配置优化

### 6.1 PostgreSQL 配置调优

#### 内存配置
```ini
# postgresql.conf

# 共享缓冲区（建议为系统内存的 25%）
shared_buffers = 4GB

# 工作内存（每个查询操作可用内存）
work_mem = 64MB

# 维护工作内存（用于 VACUUM、CREATE INDEX）
maintenance_work_mem = 1GB

# 有效缓存大小（告诉优化器可用的缓存）
effective_cache_size = 12GB
```

#### 查询规划器配置
```ini
# 随机页面成本（SSD 可以降低）
random_page_cost = 1.1

# 顺序页面成本
seq_page_cost = 1.0

# 并行查询
max_parallel_workers_per_gather = 4
max_parallel_workers = 8
```

#### 连接配置
```ini
# 最大连接数
max_connections = 200

# 连接池（推荐使用 PgBouncer）
```

#### WAL 配置
```ini
# WAL 缓冲区
wal_buffers = 16MB

# 检查点配置
checkpoint_completion_target = 0.9
max_wal_size = 4GB
min_wal_size = 1GB
```

### 6.2 连接池配置

使用 PgBouncer：
```ini
# pgbouncer.ini

[databases]
synapse = host=localhost port=5432 dbname=synapse

[pgbouncer]
listen_addr = 127.0.0.1
listen_port = 6432
auth_type = md5
auth_file = /etc/pgbouncer/userlist.txt

# 连接池模式
pool_mode = transaction

# 连接池大小
default_pool_size = 25
max_client_conn = 200
```

---

## 七、表维护和优化

### 7.1 VACUUM 和 ANALYZE

#### 手动 VACUUM
```sql
-- 标准 VACUUM（回收空间）
VACUUM events;

-- VACUUM FULL（完全重建表，锁表）
VACUUM FULL events;

-- VACUUM ANALYZE（同时更新统计信息）
VACUUM ANALYZE events;
```

#### 自动 VACUUM 配置
```ini
# postgresql.conf

# 启用自动 VACUUM
autovacuum = on

# 自动 VACUUM 触发阈值
autovacuum_vacuum_threshold = 50
autovacuum_vacuum_scale_factor = 0.1

# 自动 ANALYZE 触发阈值
autovacuum_analyze_threshold = 50
autovacuum_analyze_scale_factor = 0.05
```

#### 监控 VACUUM
```sql
-- 查看表的膨胀情况
SELECT 
    schemaname,
    tablename,
    n_live_tup,
    n_dead_tup,
    n_dead_tup::float / NULLIF(n_live_tup, 0) AS dead_ratio,
    last_vacuum,
    last_autovacuum
FROM pg_stat_user_tables
WHERE schemaname = 'public'
ORDER BY n_dead_tup DESC;
```

### 7.2 表分区

#### 按时间分区（事件表）
```sql
-- 创建分区表
CREATE TABLE events_partitioned (
    event_id TEXT NOT NULL,
    room_id TEXT NOT NULL,
    sender TEXT NOT NULL,
    event_type TEXT NOT NULL,
    content JSONB,
    origin_server_ts BIGINT NOT NULL,
    created_ts BIGINT NOT NULL
) PARTITION BY RANGE (origin_server_ts);

-- 创建分区
CREATE TABLE events_2026_01 PARTITION OF events_partitioned
    FOR VALUES FROM (1704067200000) TO (1706745600000);

CREATE TABLE events_2026_02 PARTITION OF events_partitioned
    FOR VALUES FROM (1706745600000) TO (1709251200000);

-- 创建索引
CREATE INDEX idx_events_2026_01_room_time 
    ON events_2026_01(room_id, origin_server_ts DESC);
```

#### 分区维护
```sql
-- 添加新分区
CREATE TABLE events_2026_03 PARTITION OF events_partitioned
    FOR VALUES FROM (1709251200000) TO (1711929600000);

-- 删除旧分区
DROP TABLE events_2025_01;

-- 分离分区（归档）
ALTER TABLE events_partitioned DETACH PARTITION events_2025_12;
```

---

## 八、监控和告警

### 8.1 关键性能指标

#### 数据库级别
```sql
-- 数据库大小
SELECT pg_size_pretty(pg_database_size('synapse'));

-- 连接数
SELECT count(*) FROM pg_stat_activity WHERE datname = 'synapse';

-- 缓存命中率
SELECT 
    sum(heap_blks_hit) / (sum(heap_blks_hit) + sum(heap_blks_read)) AS cache_hit_ratio
FROM pg_statio_user_tables;
```

#### 表级别
```sql
-- 表大小
SELECT 
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) AS total_size,
    pg_size_pretty(pg_relation_size(schemaname||'.'||tablename)) AS table_size,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename) - 
                   pg_relation_size(schemaname||'.'||tablename)) AS index_size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC
LIMIT 10;

-- 表活动统计
SELECT 
    schemaname,
    tablename,
    seq_scan,
    seq_tup_read,
    idx_scan,
    idx_tup_fetch,
    n_tup_ins,
    n_tup_upd,
    n_tup_del
FROM pg_stat_user_tables
WHERE schemaname = 'public'
ORDER BY seq_scan DESC;
```

### 8.2 性能告警阈值

建议告警阈值：
- **缓存命中率** < 95%
- **死元组比例** > 10%
- **平均查询时间** > 100ms
- **慢查询数量** > 10/分钟
- **连接数** > 80% 最大连接数
- **磁盘使用率** > 80%

---

## 九、性能优化检查清单

### 9.1 优化前检查

- [ ] 建立性能基准
- [ ] 识别性能瓶颈
- [ ] 分析慢查询
- [ ] 检查索引使用情况
- [ ] 检查表膨胀情况

### 9.2 优化实施

- [ ] 创建必要的索引
- [ ] 优化慢查询
- [ ] 调整数据库配置
- [ ] 实施表分区（如需要）
- [ ] 配置连接池

### 9.3 优化后验证

- [ ] 运行性能基准测试
- [ ] 对比优化前后性能
- [ ] 验证查询计划改进
- [ ] 监控生产环境性能
- [ ] 记录优化效果

---

## 十、参考资料

- [性能基准测试指南](../PERFORMANCE_BASELINE.md)
- [迁移操作指南](MIGRATION_OPERATIONS_GUIDE.md)
- [监控告警指南](MONITORING_GUIDE.md)
- [PostgreSQL 性能调优文档](https://www.postgresql.org/docs/current/performance-tips.html)

---

**文档版本**：v1.0  
**创建日期**：2026-04-04  
**维护者**：数据库团队  
**审核者**：性能团队
