# 数据库维护与优化方案

## 一、概述

本文档描述了 synapse-rust 项目的数据库维护与优化策略，包括性能监控、数据完整性校验、连接池管理等方面的最佳实践和建议。

## 二、连接池配置优化

### 2.1 当前配置

```rust
let pool = sqlx::PgPool::connect_with(
    sqlx::postgres::PgConnectOptions::from_str(database_url)?
        .max_connections(50)      // 最大连接数
        .min_connections(5)       // 最小连接数
        .connect_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600))
        .max_lifetime(Duration::from_secs(1800))
).await?;
```

### 2.2 配置说明

| 参数 | 当前值 | 推荐范围 | 说明 |
|------|--------|----------|------|
| max_connections | 50 | 25-100 | 根据服务器配置和并发需求调整 |
| min_connections | 5 | 5-10 | 保持最小连接数以减少连接开销 |
| idle_timeout | 600s | 300-600s | 空闲连接保持时间 |
| max_lifetime | 1800s | 1800-3600s | 连接最大生命周期 |
| connect_timeout | 30s | 15-30s | 连接超时时间 |

### 2.3 优化建议

1. **根据负载调整**：在高负载场景下，可适当增加 max_connections 至 100
2. **监控连接使用率**：当连接利用率持续超过 80% 时，考虑增加连接数
3. **定期检查慢查询**：识别并优化执行时间超过 100ms 的查询

## 三、性能监控指标

### 3.1 关键指标

| 指标 | 告警阈值 | 说明 |
|------|----------|------|
| 连接利用率 | >80% | 连接池使用率 |
| 平均查询时间 | >50ms | SQL 查询平均执行时间 |
| 慢查询数量 | >10/分钟 | 超过阈值的查询数量 |
| 死元组比例 | >20% | 表中无效数据比例 |
| 事务吞吐量 | <100/秒 | 每秒处理的事务数 |

### 3.2 监控方法

使用 `DatabaseMonitor` 组件获取性能指标：

```rust
let monitor = Arc::new(RwLock::new(DatabaseMonitor::new(pool.clone())));
let health_status = monitor.read().await.get_full_health_status().await?;
```

### 3.3 性能指标采集

```rust
pub async fn get_performance_metrics(&self) -> Result<PerformanceMetrics, sqlx::Error> {
    // 采集查询时间统计
    // 统计慢查询数量
    // 计算事务吞吐量
    // 收集表统计信息
}
```

## 四、数据完整性校验

### 4.1 校验项目

1. **外键约束检查**：验证所有外键关系有效性
2. **孤立记录检测**：查找孤儿记录
3. **重复数据检测**：识别重复条目
4. **非空约束检查**：验证必填字段

### 4.2 完整性检查方法

```rust
pub async fn verify_data_integrity(&self) -> Result<DataIntegrityReport, sqlx::Error> {
    let mut report = DataIntegrityReport {
        check_timestamp: Utc::now(),
        foreign_key_violations: Vec::new(),
        orphaned_records: Vec::new(),
        duplicate_entries: Vec::new(),
        null_constraint_violations: Vec::new(),
        overall_integrity_score: 100.0,
    };

    // 执行各项检查...
    report.overall_integrity_score = if total_issues == 0 {
        100.0
    } else {
        (100.0 - (total_issues as f64 * 0.5)).max(0.0)
    };

    Ok(report)
}
```

### 4.3 完整性评分标准

| 分数 | 状态 | 操作建议 |
|------|------|----------|
| 100-90 | 优秀 | 无需操作 |
| 89-80 | 良好 | 关注趋势 |
| 79-60 | 警告 | 计划修复 |
| <60 | 严重 | 立即处理 |

## 五、定期维护任务

### 5.1 维护任务清单

| 任务 | 频率 | 执行方式 |
|------|------|----------|
| VACUUM ANALYZE | 每日 | 自动 |
| 重建索引 | 每周 | 手动/计划 |
| 完整性校验 | 每日 | 自动 |
| 性能统计收集 | 持续 | 自动 |
| 过时会话清理 | 每日 | 自动 |

### 5.2 维护服务使用

```rust
pub struct DatabaseMaintenance {
    pool: Pool<Postgres>,
}

impl DatabaseMaintenance {
    pub async fn perform_maintenance(&self) -> Result<MaintenanceReport, sqlx::Error> {
        // 执行 VACUUM ANALYZE
        // 重建必要索引
        // 分析表统计信息
        // 清理过期会话
    }
}
```

### 5.3 VACUUM ANALYZE 策略

```sql
-- 对主表进行定期 VACUUM ANALYZE
VACUUM ANALYZE users;
VACUUM ANALYZE devices;
VACUUM ANALYZE rooms;
VACUUM ANALYZE room_events;
VACUUM ANALYZE room_memberships;
VACUUM ANALYZE events;
VACUUM ANALYZE private_messages;
VACUUM ANALYZE private_sessions;
```

### 5.4 索引重建策略

当出现以下情况时，建议重建索引：
- 索引膨胀率超过 20%
- 查询性能明显下降
- 大批量数据操作后

```sql
-- 重建用户相关索引
REINDEX INDEX idx_users_user_id;
REINDEX INDEX idx_users_username;

-- 重建房间相关索引
REINDEX INDEX idx_room_memberships_room;
REINDEX INDEX idx_room_memberships_user;
REINDEX INDEX idx_events_room;
```

## 六、数据库健康检查

### 6.1 健康检查接口

```rust
pub async fn health_check(&self) -> Result<DatabaseHealthStatus, sqlx::Error> {
    let is_healthy = self.check_connection().await?;
    let pool_status = self.get_connection_pool_status().await?;
    let performance = self.get_performance_metrics().await?;

    Ok(DatabaseHealthStatus {
        is_healthy,
        connection_pool_status: pool_status,
        performance_metrics: performance,
        last_checked: Utc::now(),
    })
}
```

### 6.2 健康状态指标

| 状态 | 连接池利用率 | 平均查询时间 | 完整性评分 |
|------|-------------|--------------|-----------|
| 健康 | <70% | <30ms | >90 |
| 警告 | 70-85% | 30-100ms | 80-90 |
| 危险 | >85% | >100ms | <80 |

## 七、性能优化建议

### 7.1 查询优化

1. **避免 SELECT ***：只查询需要的字段
2. **使用索引**：确保查询条件字段有索引
3. **批量操作**：使用批量插入和更新
4. **分页优化**：使用游标分页替代 OFFSET

### 7.2 索引策略

| 表名 | 索引字段 | 索引类型 |
|------|----------|----------|
| users | user_id | PRIMARY KEY |
| users | username | UNIQUE |
| devices | user_id | INDEX |
| room_memberships | (room_id, user_id) | COMPOSITE |
| events | (room_id, origin_server_ts) | COMPOSITE |
| private_messages | (session_id, created_ts) | COMPOSITE |

### 7.3 连接管理

1. **使用连接池**：避免频繁创建和销毁连接
2. **合理设置超时**：平衡响应时间和资源占用
3. **监控连接状态**：及时发现连接泄漏
4. **优雅降级**：在连接池耗尽时提供降级方案

## 八、备份与恢复

### 8.1 备份策略

```bash
# 每日全量备份
pg_dump -U synapse -d synapse -F c -f /backup/synapse_$(date +%Y%m%d).dump

# 持续归档
wal-g backup-push /var/lib/postgresql/data
```

### 8.2 恢复流程

```bash
# 停止服务
systemctl stop synapse-rust

# 恢复数据库
pg_restore -U synapse -d synapse /backup/synapse_20260129.dump

# 启动服务
systemctl start synapse-rust
```

## 九、故障排查

### 9.1 常见问题

| 问题 | 可能原因 | 解决方案 |
|------|----------|----------|
| 连接超时 | 连接池耗尽 | 增加 max_connections |
| 慢查询 | 缺少索引 | 添加必要索引 |
| 死锁 | 并发冲突 | 调整事务顺序 |
| 性能下降 | 统计信息过期 | 执行 VACUUM ANALYZE |

### 9.2 诊断命令

```sql
-- 查看慢查询
SELECT * FROM pg_stat_statements ORDER BY mean_time DESC LIMIT 10;

-- 查看锁等待
SELECT * FROM pg_locks WHERE granted = false;

-- 查看表膨胀
SELECT relname, n_dead_tup, n_live_tup,
       ROUND(n_dead_tup::numeric / NULLIF(n_live_tup + n_dead_tup, 0) * 100, 2) as dead_ratio
FROM pg_stat_user_tables
ORDER BY dead_ratio DESC;
```

## 十、监控告警配置

### 10.1 告警规则

```yaml
alerts:
  - name: high_connection_utilization
    condition: connection_utilization > 80
    severity: warning
    message: "数据库连接池利用率过高"
    
  - name: slow_query_detected
    condition: avg_query_time_ms > 100
    severity: warning
    message: "检测到慢查询"
    
  - name: data_integrity_issue
    condition: integrity_score < 80
    severity: critical
    message: "数据完整性检查失败"
```

### 10.2 监控指标收集

```rust
// 记录性能指标到数据库
record_performance_metric(
    &pool,
    "connection",
    "utilization",
    utilization_percentage,
    None,
).await?;
```

## 十一、最佳实践总结

1. **定期维护**：执行每日 VACUUM ANALYZE，每周索引检查
2. **性能监控**：持续收集性能指标，设置告警阈值
3. **数据校验**：每日运行完整性检查，及时发现数据问题
4. **容量规划**：根据增长趋势提前规划存储和连接资源
5. **文档更新**：记录配置变更和优化措施

## 十二、变更日志

| 版本 | 日期 | 变更说明 |
|------|------|----------|
| 1.0.0 | 2026-01-29 | 初始版本，包含连接池优化、监控机制、维护任务 |
