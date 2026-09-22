# Matrix 负载测试性能基线

## 预期性能指标（基准）

### 1. 延迟阈值

| 操作 | P50 | P95 | P99 | 说明 |
|------|-----|-----|-----|------|
| 登录 (login) | <100ms | <500ms | <1000ms | 密码验证 + 设备注册 |
| 同步 (/sync) | <200ms | <2000ms | <5000ms | **性能瓶颈**，时间线拉取 |
| 发送消息 | <50ms | <500ms | <1000ms | 事件持久化 |
| 加入房间 | <100ms | <500ms | <1000ms | 房间成员更新 |

### 2. 吞吐量阈值

| 指标 | 100 VU | 500 VU | 1000 VU | 5000 VU | 10000 VU |
|------|--------|--------|---------|---------|----------|
| 请求/秒 | 20-50 | 100-250 | 200-500 | 1000-2500 | 2000-5000 |
| 错误率 | <0.1% | <0.5% | <1% | <2% | <5% |
| CPU 使用率 | <30% | <50% | <70% | <85% | <95% |
| 内存增长 | <50MB/h | <250MB/h | <500MB/h | <2GB/h | <4GB/h |

### 3. 资源使用预期

| 组件 | 100 VU | 500 VU | 1000 VU | 5000 VU | 10000 VU |
|------|--------|--------|---------|---------|----------|
| synapse-app CPU | 0.5-1.0 | 1.5-2.5 | 2.5-4.0 | 6-8 | 10-12 |
| synapse-app 内存 | 500MB | 1-1.5GB | 2-2.5GB | 8-10GB | 15-20GB |
| PostgreSQL CPU | 0.3-0.5 | 0.8-1.2 | 1.5-2.0 | 4-6 | 8-10 |
| Redis CPU | 0.1-0.2 | 0.3-0.5 | 0.5-0.8 | 1.5-2.0 | 3-4 |

---

## 异常诊断指南

### 1. CPU 使用率低但延迟高

**可能原因**：
- I/O 瓶颈（磁盘读写慢）
- 锁竞争（Mutex 争用）
- 网络延迟
- 数据库查询慢

**诊断步骤**：
```bash
# 1. 检查磁盘 I/O
docker exec synapse-node-exporter node_disk_io_time_seconds_total

# 2. 检查数据库慢查询
docker exec synapse-postgres pg_stat_statements

# 3. 检查网络延迟
ping synapse-app
curl -w "@curl-format.txt" -o /dev/null -s http://localhost:8008/_matrix/client/r0/sync

# 4. 检查锁竞争（Rust 代码）
# 查看 metrics.rs 中的 Mutex 持有时间
```

### 2. 内存持续增长不回收

**可能原因**：
- Rust 代码中的循环引用
- 未正确释放的资源（文件描述符、网络连接）
- Histogram 的 `Vec<f64>` 累积（已知问题）
- 缓存未过期

**诊断步骤**：
```bash
# 1. 检查内存趋势
curl http://localhost:9092/api/v1/query?query=process_resident_memory_bytes{job="synapse-rust"}

# 2. 检查 Histogram 累积（P1-13 问题）
curl http://localhost:9092/api/v1/query?query=histogram_samples_count

# 3. 检查文件描述符
cat /proc/$(pidof synapse)/fd | wc -l

# 4. 检查网络连接数
netstat -an | grep ESTABLISHED | wc -l
```

### 3. P99 延迟剧烈抖动

**可能原因**：
- 大型堆分配（GC 暂停，虽然 Rust 无 GC）
- 网络队列积压
- 数据库死锁
- 外部依赖超时

**诊断步骤**：
```bash
# 1. 检查网络队列
ss -tnp | grep :8008 | wc -l

# 2. 检查数据库连接池
curl http://localhost:9092/api/v1/query?query=db_connections_active

# 3. 检查同步延迟分布
curl http://localhost:9092/api/v1/query_range?query=rate(matrix_sync_duration_ms_bucket[5m])

# 4. 检查日志中的慢请求
docker logs synapse-app 2>&1 | grep -i "slow" | tail -50
```

---

## 负载测试执行流程

### 阶段 1: 基线测试 (100 VU)

```bash
./scripts/load-test/run-load-test.sh light
```

**目标**：
- ✅ 所有阈值通过
- ✅ 建立性能基线
- ✅ 验证监控数据正常

**预期结果**：
- P95 同步延迟 < 500ms
- 错误率 < 0.1%
- CPU < 30%

### 阶段 2: 中等负载 (500 VU)

```bash
./scripts/load-test/run-load-test.sh medium
```

**目标**：
- ✅ 确认线性扩展
- ✅ 检查资源使用增长率

**预期结果**：
- P95 同步延迟 < 1000ms
- 错误率 < 0.5%
- CPU < 50%

### 阶段 3: 高负载 (1000 VU)

```bash
./scripts/load-test/run-load-test.sh heavy
```

**目标**：
- ✅ 识别性能拐点
- ✅ 检查数据库连接池

**预期结果**：
- P95 同步延迟 < 2000ms
- 错误率 < 1%
- CPU < 70%

### 阶段 4: 压力测试 (5000 VU)

```bash
./scripts/load-test/run-load-test.sh stress
```

**目标**：
- ✅ 找到系统瓶颈
- ✅ 验证限流机制

**预期结果**：
- P95 同步延迟 < 5000ms
- 错误率 < 2%
- CPU < 85%

### 阶段 5: 极限测试 (10000 VU)

```bash
./scripts/load-test/run-load-test.sh extreme
```

**目标**：
- ✅ 找到崩溃点
- ✅ 验证故障恢复

**预期结果**：
- P95 同步延迟 < 10000ms
- 错误率 < 5%
- CPU < 95%

---

## 数据收集与分析

### 1. Prometheus 查询示例

```bash
# 登录延迟 P95
curl -s 'http://localhost:9092/api/v1/query_range?query=histogram_quantile(0.95, rate(matrix_login_duration_ms_bucket[5m]))&start=1h&end=now'

# 同步延迟均值
curl -s 'http://localhost:9092/api/v1/query_range?query=rate(matrix_sync_duration_ms_sum[5m]) / rate(matrix_sync_duration_ms_count[5m])&start=1h&end=now'

# 错误率
curl -s 'http://localhost:9092/api/v1/query?query=rate(matrix_error_rate[5m])'

# 内存趋势
curl -s 'http://localhost:9092/api/v1/query_range?query=process_resident_memory_bytes{job="synapse-rust"}&start=1h&end=now'
```

### 2. Grafana 仪表板导出

```bash
# 导出当前仪表板
curl -s -H "Authorization: Bearer $GRAFANA_API_KEY" \
  http://localhost:3000/api/dashboards/uid/matrix-load-test | jq '.dashboard' > load-test-dashboard-export.json
```

### 3. k6 结果分析

```bash
# 生成 HTML 报告
k6 convert --format=html scripts/load-test/matrix-load-test.js > report.html

# 提取关键指标
jq '.metrics.matrix_sync_duration_ms.values.p95' results/stress_*.json
```

---

## 告警规则（负载测试期间）

```yaml
groups:
  - name: load-test-alerts
    rules:
      - alert: HighSyncLatency
        expr: rate(matrix_sync_duration_ms_sum[5m]) / rate(matrix_sync_duration_ms_count[5m]) > 2000
        for: 5m
        labels:
          severity: warning
        annotations:
          summary: "同步延迟超过 2 秒"

      - alert: HighErrorRate
        expr: rate(matrix_error_count[5m]) / rate(matrix_request_count[5m]) > 0.05
        for: 2m
        labels:
          severity: critical
        annotations:
          summary: "错误率超过 5%"

      - alert: MemoryLeakSuspected
        expr: increase(process_resident_memory_bytes[1h]) > 2000000000
        for: 1h
        labels:
          severity: warning
        annotations:
          summary: "1 小时内内存增长超过 2GB"
```

---

## 长期优化方向

1. **Histogram 桶模型重构**（P2-1）：消除 `Vec<f64>` 内存泄漏
2. **数据库连接池扩容**：根据负载动态调整 `POOL_SIZE`
3. **CDN 加速静态资源**：减轻后端压力
4. **水平扩展**：多实例 + 负载均衡
5. **缓存策略优化**：增加热点数据缓存 TTL

---

*负载测试完成后，将结果汇总到《负载测试报告》文档。*
