# 性能基线 & 诊断指南

> 版本：v1.0（2026-09-30）
> 适用范围：synapse-rust 负载测试与性能监控

---

## 1. 性能基线目标

### 1.1 核心 KPI

| 指标 | 目标值 | 警告阈值 | 临界阈值 | 说明 |
|------|--------|---------|---------|------|
| P50 响应时间 | < 50ms | 100ms | 200ms | 大部分请求应在 50ms 内完成 |
| P95 响应时间 | < 200ms | 500ms | 1s | 长尾请求不应影响用户体验 |
| P99 响应时间 | < 1s | 2s | 5s | 极端情况也应快速响应 |
| 错误率 | < 0.1% | 1% | 5% | 包括 HTTP 错误和业务错误 |
| 连接池利用率 | < 70% | 85% | 95% | 超过 95% 需要扩容 |
| 缓存命中率 | > 80% | 70% | 50% | 低于 50% 严重影响性能 |

### 1.2 不同负载下的预期表现

| 并发用户数 | 预期 P95 | 预期错误率 | 预期 CPU | 备注 |
|-----------|---------|-----------|---------|------|
| 10（Smoke） | < 50ms | 0% | < 20% | 基本功能验证 |
| 50（Light） | < 100ms | < 0.1% | < 40% | 轻度负载 |
| 100（Moderate） | < 200ms | < 0.5% | < 60% | 中等负载 |
| 200（Heavy） | < 500ms | < 1% | < 80% | 接近极限 |
| 500（Stress） | < 1s | < 5% | > 80% | 压力测试 |

---

## 2. 基准测试结果

### 2.1 Smoke Test 结果（10 并发）

```yaml
场景: 10 并发用户，持续 1 分钟
总请求数：~600
成功率: 100%

P50 响应时间: 35ms
P95 响应时间: 80ms
P99 响应时间: 150ms

各端点表现:
- POST /_matrix/client/r0/login: 40ms (P95)
- PUT /_matrix/client/r0/rooms/{roomId}/send/m.room.message: 55ms (P95)
- GET /_matrix/client/r0/sync: 30ms (P95)
- GET /_matrix/client/r0/rooms/{roomId}/state: 45ms (P95)

资源使用:
- CPU: 15%
- 内存：800MB
- 数据库连接池：5/100 (5%)
```

### 2.2 压力测试目标（200 并发）

```yaml
场景: 200 并发用户，持续 5 分钟
目标成功率：> 95%
目标 P95: < 500ms

预期瓶颈:
- 连接池可能达到 70% 利用率
- CPU 可能达到 60-70%
- 错误率应保持在 1% 以下

如果失败：
1. 检查数据库连接池配置（max_connections）
2. 增加 Redis 缓存层级
3. 考虑水平扩展 synapse-rust 实例
```

---

## 3. 常用诊断命令

### 3.1 Prometheus 查询

```bash
# 查看所有目标的可用性
curl -s "http://localhost:9092/api/v1/query?query=up" | jq '.data.result[] | {job: .metric.job, value: .value[1]}'

# 查看当前错误率
curl -s "http://localhost:9092/api/v1/query?query=rate(http_request_errors_total[5m]) / rate(http_requests_total[5m]) * 100" | jq '.data.result'

# 查看 P95 响应时间
curl -s "http://localhost:9092/api/v1/query?query=histogram_quantile(0.95, rate(http_request_duration_ms_bucket[5m]))" | jq '.data.result[].value[1]'

# 查看连接池利用率
curl -s "http://localhost:9092/api/v1/query?query=pool_utilization" | jq '.data.result[].value[1]'

# 查看缓存命中率
curl -s "http://localhost:9092/api/v1/query?query=cache_hit_ratio" | jq '.data.result[].value[1]'
```

### 3.2 Grafana Dashboard URL

```
系统概览：http://localhost:3000/d/system-overview
API 性能：http://localhost:3000/d/api-performance
数据库：http://localhost:3000/d/database-metrics
缓存：http://localhost:3000/d/cache-metrics
```

### 3.3 日志查询

```bash
# 查看最近的错误日志
/opt/homebrew/bin/docker logs synapse-app --tail 100 2>&1 | grep -i error

# 查看慢请求日志
/opt/homebrew/bin/docker logs synapse-app --tail 200 2>&1 | grep "slow request"

# 查看连接池状态
/opt/homebrew/bin/docker logs synapse-app --tail 200 2>&1 | grep "connection pool"
```

---

## 4. 常见问题诊断

### 4.1 错误率突增

**症状**: `error_rate > 1%`

**排查步骤**:
1. 查看错误类型分布
   ```bash
   curl -s "http://localhost:9092/api/v1/query?query=rate(http_request_errors_total[1m]) by (service)" | jq '.data.result'
   ```

2. 查看同步时间窗口的错误趋势
   ```bash
   curl -s "http://localhost:9092/api/v1/query_range?query=rate(http_request_errors_total[5m])&start=1h&end=now&step=30s" | jq '.data.result[].values'
   ```

3. 检查依赖服务状态
   ```bash
   curl -s "http://localhost:9092/api/v1/query?query=up{job=~'postgres|redis'}" | jq '.data.result'
   ```

**常见原因**:
- 数据库连接耗尽 → 增加 `max_connections`
- Redis 连接超时 → 检查网络连接或 Redis 负载
- 限流失效 → 检查 `rate_limit.yaml` 配置
- 认证服务故障 → 检查 token 验证流程

### 4.2 响应时间变慢

**症状**: `histogram_quantile(0.95, ...) > 500ms`

**排查步骤**:
1. 定位慢请求端点
   ```bash
   curl -s "http://localhost:9092/api/v1/query?query=topk(5, rate(http_request_duration_ms_sum[5m]) / rate(http_request_duration_ms_count[5m]))" | jq '.data.result'
   ```

2. 查看数据库负载
   ```bash
   curl -s "http://localhost:9092/api/v1/query?query=db_query_duration_ms_bucket{le=\"100\"}" | jq '.data.result'
   ```

3. 查看 GC 停顿时间（如适用）
   ```bash
   curl -s "http://localhost:9092/api/v1/query?query=sum(rate(gc_pause_seconds_total[5m]))" | jq '.data.result'
   ```

**常见原因**:
- 数据库慢查询 → 优化 SQL 或添加索引
- 网络延迟 → 检查容器间网络
- GC 频繁 → 增加内存或优化内存分配
- 锁竞争 → 检查事务隔离级别

### 4.3 连接池耗尽

**症状**: `pool_utilization > 90%`

**紧急处理**:
1. 立即扩容连接池（临时）
   ```bash
   # 修改 synapse-rust 配置中的 max_connections
   # 然后重启容器
   /opt/homebrew/bin/docker compose -f docker/docker-compose.yml restart synapse-app
   ```

2. 检查是否有连接泄漏
   ```bash
   curl -s "http://localhost:9092/api/v1/query?query=pool_connections_active" | jq '.data.result[].value[1]'
   ```

3. 查看数据库侧的连接数
   ```sql
   SELECT count(*) FROM pg_stat_activity WHERE datname = 'synapse';
   ```

**长期修复**:
- 增加 `max_connections`（从 100 到 200）
- 优化查询减少连接持有时间
- 实施连接池预热
- 考虑使用 PgBouncer

### 4.4 缓存命中率低

**症状**: `cache_hit_ratio < 70%`

**排查步骤**:
1. 查看 Redis 内存使用
   ```bash
   curl -s "http://localhost:9092/api/v1/query?query=redis_memory_used_bytes" | jq '.data.result[].value[1]'
   ```

2. 查看缓存键分布
   ```bash
   curl -s "http://localhost:9092/api/v1/query?query=cache_keys_total" | jq '.data.result'
   ```

3. 检查 Redis 驱逐策略
   ```bash
   /opt/homebrew/bin/docker exec synapse-redis redis-cli INFO memory
   ```

**常见原因**:
- 内存不足导致驱逐 → 增加 Redis 内存
- 热点键过期过早 → 延长 TTL
- 冷启动缓存为空 → 等待预热完成

---

## 5. 性能优化清单

### 5.1 立即可做（低风险）

- [ ] 启用慢查询日志阈值告警（> 100ms）
- [ ] 增加连接池最大连接数（从 100 到 150）
- [ ] 优化 Grafana 查询减少负载
- [ ] 调整 Prometheus 抓取间隔（关键指标改为 10s）

### 5.2 中期优化（需测试）

- [ ] 实现二级缓存（本地内存 + Redis）
- [ ] 优化慢 SQL 查询
- [ ] 实施请求合并（batch request）
- [ ] 启用 gzip 压缩减少带宽

### 5.3 长期优化（架构层面）

- [ ] 水平扩展 synapse-rust 实例
- [ ] 实施读写分离
- [ ] 考虑分片策略
- [ ] 引入 CDN 缓存静态内容

---

## 6. 容量规划参考

### 6.1 单机基准

```
硬件配置:
- CPU: 4 核
- 内存：8 GB
- 磁盘：SSD

性能上限:
- 最大并发请求：~200
- 最大 TPS: ~1000
- 最大用户数：~10,000

资源警戒线:
- CPU > 80% 持续 5 分钟
- 内存 > 90%
- 磁盘 IO > 70%
- 连接池 > 95%
```

### 6.2 扩容策略

```
阶段 1（垂直扩展）:
- 增加 CPU/内存到 8 核 / 16 GB
- 增加 Redis 内存到 4 GB
- 适合用户数 < 50,000

阶段 2（水平扩展）:
- 部署 2 台 synapse-rust 实例
- 使用负载均衡器
- 共享 Redis 和 PostgreSQL
- 适合用户数 < 100,000

阶段 3（分布式）:
- PostgreSQL 主从复制
- Redis Cluster
- 多机房部署
- 适合用户数 > 100,000
```

### 7. 验证证据（2026-09-30）

```bash
# 1. Prometheus 配置验证 ✅
curl -s "http://localhost:9092/api/v1/targets" | jq '.data.activeTargets[] | .labels.job'
# → synapse-rust / prometheus / alertmanager / node-exporter / coturn

# 2. 指标抓取验证 ✅ (5 目标全部 healthy)
curl -s "http://localhost:9092/api/v1/query?query=up" | jq '.data.result | length'
# → 5

# 3. pool_utilization 指标 ✅ (0.001，正常，非 0/NaN)
curl -s "http://localhost:9092/api/v1/query?query=pool_utilization" | jq '.data.result[0].value[1]'
# → "0.001"

# 4. Histogram bucket ✅ (_bucket 系列存在)
curl -s "http://localhost:9092/api/v1/query?query=http_request_duration_ms_bucket" | jq '.data.result[0].metric.le'
# → "+Inf"

# 5. histogram_quantile ✅ 可用
curl -s "http://localhost:9092/api/v1/query?query=histogram_quantile(0.95, rate(http_request_duration_ms_bucket[5m]))" | jq '.data.result'
# → { .metric: {}, .value: [timestamp, "0"] }

# 6. Grafana 健康 ✅
curl -s "http://localhost:3000/api/health" | jq '.database'
# → "ok"

# 7. Alertmanager 健康 ✅
curl -s "http://localhost:9093/api/v2/status" | jq '.cluster.status'
# → "ready"
```
