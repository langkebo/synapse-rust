# Synapse-Rust PromQL 查询模板

> **用途**: 可直接复制到 Prometheus/Grafana 的查询框  
> **最后更新**: 2026-09-30

---

## 🚨 告警规则 (Alertmanager YAML)

### 基础健康检查

```yaml
groups:
- name: synapse-basic
  interval: 30s
  rules:
  # HTTP 5xx 错误率 > 5%
  - alert: HighHTTPErrorRate
    expr: |
      sum(rate(http_requests_total{status_code=~"5.."}[5m])) 
      / 
      sum(rate(http_requests_total[5m])) > 0.05
    for: 5m
    labels:
      severity: critical
    annotations:
      summary: "HTTP 5xx 错误率过高 ({{ $value | humanizePercentage }})"
      description: "最近 5 分钟 HTTP 5xx 错误率为 {{ $value | humanizePercentage }}, 超过阈值 5%"

  # 房间创建失败率 > 1%
  - alert: HighRoomCreateFailureRate
    expr: |
      sum(rate(room_operations_total{operation="create",outcome="error"}[5m])) 
      / 
      sum(rate(room_operations_total{operation="create"}[5m])) > 0.01
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "房间创建失败率过高 ({{ $value | humanizePercentage }})"

  # 数据库查询错误
  - alert: HighDatabaseErrorRate
    expr: |
      rate(db_queries_total{outcome="error"}[5m]) > 10
    for: 2m
    labels:
      severity: critical
    annotations:
      summary: "数据库查询错误率高 ({{ $value }}/s)"
```

### 延迟告警

```yaml
- name: synapse-latency
  interval: 30s
  rules:
  # 房间创建 P99 > 5s
  - alert: SlowRoomCreation
    expr: |
      histogram_quantile(0.99, 
        rate(room_creation_duration_seconds_bucket[5m])
      ) > 5
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "房间创建 P99 延迟过高 ({{ $value | humanizeDuration }})"

  # 消息投递 P95 > 1s
  - alert: SlowMessageDelivery
    expr: |
      histogram_quantile(0.95, 
        rate(message_delivery_latency_seconds_bucket[5m])
      ) > 1
    for: 10m
    labels:
      severity: warning
    annotations:
      summary: "消息投递 P95 延迟 > 1 秒"

  # Sync 延迟 > 2s
  - alert: SlowSyncLatency
    expr: |
      histogram_quantile(0.95, 
        rate(sync_duration_ms_bucket[5m])
      ) > 2000
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "/sync 接口 P95 延迟 > 2 秒"
```

### 资源告警

```yaml
- name: synapse-resources
  interval: 30s
  rules:
  # 数据库连接池利用率 > 90%
  - alert: HighDatabaseConnectionUtilization
    expr: |
      db_connections_active / (db_connections_active + db_connections_idle) > 0.9
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "数据库连接池利用率 {{ $value | humanizePercentage }}"

  # 消息队列积压
  - alert: MessageQueueBacklog
    expr: |
      message_queue_depth > 100
    for: 5m
    labels:
      severity: warning
    annotations:
      summary: "消息队列积压 (depth={{ $value }})"

  # Cache 命中率下降
  - alert: LowCacheHitRate
    expr: |
      sum(cache_hits_total) / (sum(cache_hits_total) + sum(cache_misses_total)) < 0.8
    for: 10m
    labels:
      severity: warning
    annotations:
      summary: "缓存命中率下降 ({{ $value | humanizePercentage }})"
```

---

## 📊 Grafana 仪表盘查询

### 可用性监控 (Uptime Panel)

```promql
# HTTP 请求成功率 (%)
sum(rate(http_requests_total[5m])) 
/ 
sum(rate(http_requests_total[5m]))
* 100
```

**可视化**: Gauge 仪表 (min: 0, max: 100, thresholds: 99, 99.9)

---

### SLO 仪表盘

#### 错误预算面板

```promql
# 剩余错误预算 (0=耗尽，1=充足)
0.001 - (
  sum(increase(http_requests_total{status_code=~"5.."}[5m])) 
  / 
  sum(increase(http_requests_total[5m]))
)
```

**可视化**: Time series (y-axis: 0-1, color: green if >0.5, yellow if >0, red if <0)

#### 延迟 SLO

```promql
# /sync 接口达标率 (% 的响应在 1s 内)
sum(rate(sync_duration_ms_bucket{le="1000"}[5m])) 
/ 
sum(rate(sync_duration_ms_count[5m]))
* 100
```

**可视化**: Time series (target line at 95%)

---

### 延迟分布热力图

```promql
# P50 延迟 (ms)
histogram_quantile(0.5, 
  sum by (le) (rate(room_creation_duration_seconds_bucket[5m]))
) * 1000

# P95 延迟 (ms)
histogram_quantile(0.95, 
  sum by (le) (rate(room_creation_duration_seconds_bucket[5m]))
) * 1000

# P99 延迟 (ms)
histogram_quantile(0.99, 
  sum by (le) (rate(room_creation_duration_seconds_bucket[5m]))
) * 1000
```

**可视化**: Time series (叠加三条线)

---

### 错误分析表格

```promql
# 按错误类型分组
sum by (error_type) (rate(http_request_errors_total[5m]))

# 按 HTTP 状态码分组
sum by (status_code) (rate(http_requests_total[5m]))
```

**可视化**: Table (sortable by count)

---

### 业务指标概览

```promql
# 实时活跃用户 (估计值)
rate(sync_requests_total[1m]) * 60

# 房间创建速率 (/min)
rate(room_operations_total{operation="create"}[1m]) * 60

# 消息发送速率 (/sec)
rate(messages_sent_total[1m])

# E2EE 覆盖率 (%)
sum(rate(messages_sent_total{encryption="true"}[1h])) 
/ 
sum(rate(messages_sent_total[1h]))
* 100
```

**可视化**: Stat panels with sparklines

---

### 数据库性能

```promql
# 慢查询比例 (>100ms)
1 - (
  sum(rate(db_query_latency_seconds_bucket{le="0.1"}[5m])) 
  / 
  sum(rate(db_query_latency_seconds_count[5m]))
)
* 100

# 按表的查询延迟
avg by (table) (
  rate(db_query_latency_seconds_sum[5m]) 
  / 
  rate(db_query_latency_seconds_count[5m])
) * 1000

# 最慢的 SQL 表
topk(5, 
  sum by (table) (rate(db_query_latency_seconds_sum[5m]))
)
```

**可视化**: Heatmap + Bar chart

---

### 存储流位点

```promql
# 各存储流当前位点 (per-stream, 按 stream 标签区分)
synapse_storage_stream_current_position

# 单流位点 (如 events 流)
synapse_storage_stream_current_position{stream="events"}

# 流写入推进速率 (位点/秒)
rate(synapse_storage_stream_current_position{stream="events"}[5m])
```

**可视化**: Time series (legend `{{stream}}`)；位点在 `/metrics` 抓取时即时计算，随写入推进

---

### 缓存性能

```promql
# 整体缓存命中率
sum(cache_hits_total) 
/ 
(sum(cache_hits_total) + sum(cache_misses_total))
* 100

# 按缓存类型命中率
sum by (cache_type) (rate(cache_operations_total{result="hit"}[5m])) 
/ 
sum by (cache_type) (rate(cache_operations_total[5m]))
* 100

# Redis 错误率
sum(rate(cache_operations_total{cache_backend="redis",result="error"}[5m])) 
/ 
sum(rate(cache_operations_total{cache_backend="redis"}[5m]))
* 100
```

**可视化**: Multiple Gauges

---

### Federation 健康度

```promql
# Federation 错误率
sum(rate(federation_operations_total{result!="success"}[5m])) 
/ 
sum(rate(federation_operations_total[5m]))
* 100

# 按目标服务器的错误分布
sum by (target_server) (rate(federation_operations_total{result!="success"}[5m]))

# 超时分布
sum by (timeout_class) (rate(federation_operation_duration_seconds_bucket{result=~"timeout"}[5m]))
```

**可视化**: Pie chart + Time series

---

## 🔍 故障诊断查询

### 快速排查 Checklist

#### 1. 检查整体健康状态

```promql
# HTTP 5xx 错误数 (过去 5 分钟)
sum(increase(http_requests_total{status_code=~"5.."}[5m]))

# 活跃连接数
http_requests_active

# 正在处理的请求
rate(http_requests_total[1m])
```

---

#### 2. 定位问题源头

```promql
# 哪个 endpoint 的错误最多？
topk(5, 
  sum by (endpoint) (rate(http_requests_total{status_code=~"5.."}[5m]))
)

# 哪些用户的请求失败了？
topk(10, 
  sum by (user_id) (rate(http_requests_total{status_code=~"4.."}[5m]))
)

# 哪个数据库表的查询错误最多？
topk(5, 
  sum by (table) (rate(db_queries_total{outcome="error"}[5m]))
)
```

---

#### 3. 时间维度分析

```promql
# 最近 1 小时的错误趋势 (每分钟)
sum(rate(http_requests_total{status_code=~"5.."}[1m]))
  offset 0m

# 对比昨天同一时间
sum(rate(http_requests_total{status_code=~"5.."}[1m]))
  offset 24h

# 环比变化
(
  sum(rate(http_requests_total{status_code=~"5.."}[1m])) 
  -
  sum(rate(http_requests_total{status_code=~"5.."}[1m] offset 1h))
)
/
sum(rate(http_requests_total{status_code=~"5.."}[1m] offset 1h))
```

---

#### 4. 容量瓶颈分析

```promql
# 数据库连接池是否耗尽？
db_connections_active / (db_connections_active + db_connections_idle)

# 消息队列是否在积压？
message_queue_depth
  >
avg_over_time(message_queue_depth[1h]) * 2

# CPU 密集型还是 IO 密集型？
rate(process_cpu_seconds_total[1m]) 
/
rate(db_query_latency_seconds_sum[1m])
```

---

### 异常模式检测

#### 突增检测 (Spikes)

```promql
# 错误率突增 (相对 1 小时平均值)
rate(http_requests_total{status_code=~"5.."}[5m]) 
/ 
avg_over_time(rate(http_requests_total{status_code=~"5.."}[5m])[1h:]) 
> 2
```

**含义**: 当前错误率是 1 小时前平均值的 2 倍以上

---

#### 持续性检测 (Sustained Issues)

```promql
# 持续高延迟 (P95 > 2s 持续 10 分钟)
histogram_quantile(0.95, rate(sync_duration_ms_bucket[5m])) > 2000
  and
  changes(histogram_quantile(0.95, rate(sync_duration_ms_bucket[5m]))[10m:1m]) = 10
```

**含义**: P95 延迟连续 10 分钟保持在 2 秒以上

---

#### 相关性分析

```promql
# 高延迟期间的错误类型分布
sum by (error_type) (
  rate(http_request_errors_total[5m]) 
  *
  (histogram_quantile(0.95, rate(http_request_duration_ms_bucket[5m])) > 1000)
)
```

**含义**: 仅在延迟高时统计错误，找出与延迟相关的错误类型

---

## 💡 常用技巧

### 1. 平滑波动

```promql
# 使用移动平均平滑噪点
avg_over_time(rate(http_requests_total[5m])[5m:])

# 指数加权移动平均 (EWMA)
exp(
  sum(rate(http_requests_total[5m])) 
  - 
  avg_over_time(sum(rate(http_requests_total[5m]))[5m:])
)
```

---

### 2. 多标签组合查询

```promql
# 同时满足多个条件的查询
sum(
  rate(http_requests_total{
    status_code=~"5..",
    endpoint=~"/sync.*",
    client_type="mobile"
  }[5m])
)
```

---

### 3. 比率计算技巧

```promql
# 避免除零错误的安全除法
(rate(a[5m]) / (rate(b[5m]) + 0.00001)) * 100
```

---

### 4. 时间偏移比较

```promql
# 同比上周今天
rate(http_requests_total[5m]) 
- 
rate(http_requests_total[5m] offset 7d)

# 环比昨天此刻
rate(http_requests_total[5m]) 
- 
rate(http_requests_total[5m] offset 24h)
```

---

## 📋 快速参考卡

| 场景 | 查询 | 期望值 |
|------|------|--------|
| **系统是否正常？** | `sum(rate(http_requests_total{status_code="200"}[5m]))` | > 100/s |
| **有多少错误？** | `sum(rate(http_requests_total{status_code=~"5.."}[5m]))` | ~0 |
| **用户受影响吗？** | `histogram_quantile(0.95, rate(sync_duration_ms_bucket[5m]))` | < 2000ms |
| **数据库健康？** | `rate(db_queries_total{outcome="error"}[5m])` | 0 |
| **缓存有效吗？** | `sum(cache_hits)/(sum(cache_hits)+sum(cache_misses))` | > 80% |
| **队列正常吗？** | `message_queue_depth` | < 100 |
| **Federation 正常？** | `sum(rate(federation_operations_total{result="success"}[5m]))` | 稳定 |
