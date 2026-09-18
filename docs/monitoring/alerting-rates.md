# Grafana 告警规则集（事件驱动 & 限流）

> S-6 / S-9 配套监控规则

---

## S-6: EventNotifier 监控规则

| 规则名 | PromQL | 条件 | 严重性 | 含义 |
|--------|--------|------|--------|------|
| `EventNotifierRedisSubscriberDown` | `increase(event_notifier_subscriber_failures_total[5m]) > 0` | 5 分钟内任何失败 | Critical | Redis Pub/Sub 断开，跨实例 fan-out 失效 |
| `EventNotifierSubscriberFailuresSpike` | `rate(event_notifier_subscriber_failures_total[1m]) > 0.1` | 每秒 >0.1 次失败 | Warning | 持续重连中，Redis 可能不稳定 |

### Grafana 面板建议

```json
{
  "title": "EventNotifier Redis Subscriber Health",
  "panels": [
    {
      "title": "Subscriber Failures (5m rate)",
      "type": "stat",
      "targets": [{
        "expr": "rate(event_notifier_subscriber_failures_total[5m])",
        "legendFormat": "failures/sec"
      }],
      "thresholds": {
        "mode": "absolute",
        "steps": [
          {"color": "green", "value": null, "op": "gt", "limit": 0},
          {"color": "red", "value": 0.1, "op": "gt", "limit": 0.1}
        ]
      }
    },
    {
      "title": "Cumulative Failures",
      "type": "timeseries",
      "targets": [{
        "expr": "increase(event_notifier_subscriber_failures_total[1h])",
        "legendFormat": "failures/1h"
      }]
    }
  ]
}
```

---

## S-9: 限流监控规则

| 规则名 | PromQL | 条件 | 严重性 | 含义 |
|--------|--------|------|--------|------|
| `RateLimitFailOpen` | `rate(rate_limit_fail_open_total[5m]) > 0` | 任何 fail-open | Critical | 限流失效（后端不可用仍放行） |
| `RateLimitFailClosed` | `rate(rate_limit_fail_closed_total[5m]) > 0` | 任何 fail-closed | Critical | 后端不可用，全站 429 |
| `RateLimitRejectedHigh` | `rate(rate_limit_requests_rejected_total[5m]) / rate(rate_limit_requests_total[5m]) > 0.05` | 5% 拒绝率 | Warning | 可能遭受攻击或配置过严 |
| `RateLimitDebugHighRatio` | `rate(rate_limit_requests_rejected_total[5m]) / rate(rate_limit_requests_total[5m]) > 0.01` | 1% 拒绝率 | Info | 高关注（调试阈值） |

### `/admin/v1/rate-limit-status` 端点集成

在 Grafana 中使用该端点作为数据源（需要 Prometheus blackbox exporter 或直接调用）：

```yaml
# Grafana annotation: 429 事件标记
# 在 dashboard 中标注限流触发时刻
annotations:
  - name: "Rate Limit Triggered"
    datasource: synapse-rust-admin
    query: 'rate-limit-status.metrics.rejected_ratio_percent > 0'
    icon-color: "red"
```

---

## 429 响应头标准（已实施）

所有 429 响应现在包含：

```http
HTTP/1.1 429 Too Many Requests
retry-after: 5
x-ratelimit-retry-after-ms: 4800
x-ratelimit-after: 4800
x-ratelimit-remaining: 0
```

### 各头说明

| 响应头 | 格式 | 示例 | 用途 |
|--------|------|------|------|
| `retry-after` | 秒（RFC 7231） | `5` | 标准重试间隔，所有 HTTP 客户端通用 |
| `x-ratelimit-retry-after-ms` | 毫秒 | `4800` | Matrix SDK 精确重试计算 |
| `x-ratelimit-after` | 毫秒 | `4800` | 备用精确重试（同 x-ratelimit-retry-after-ms） |
| `x-ratelimit-remaining` | 数字 | `0` | 剩余请求次数（429 时恒为 0） |

> **注意**: `X-RateLimit-Limit` 和 `X-RateLimit-Remaining` 头在限流中间件的请求通过时也会设置（非 429 响应），提供当前窗口配额信息。

---

## 配置建议

### 环境特定阈值

```yaml
# docker-compose.yml 环境变量示例
# 生产环境严格阈值
GRAFANA_ALERT_RATE_LIMIT_REJECTED_RATIO_CRITICAL: "0.1"
GRAFANA_ALERT_RATE_LIMIT_REJECTED_RATIO_WARNING: "0.05"
GRAFANA_ALERT_EVENT_NOTIFIER_FAILURE_WINDOW: "5m"

# 开发环境宽松阈值（避免告警疲劳）
GRAFANA_ALERT_RATE_LIMIT_REJECTED_RATIO_CRITICAL_DEV: "0.5"
GRAFANA_ALERT_EVENT_NOTIFIER_FAILURE_WINDOW_DEV: "15m"
```

### 告警路由

```yaml
# alertmanager.yml 示例
route:
  receiver: 'synapse-oncall'
  group_wait: 30s
  group_interval: 5m
  repeat_interval: 4h

  routes:
    - match:
        alertname: RateLimitFailOpen
      receiver: 'synapse-oncall-critical'
      group_wait: 10s
    
    - match:
        alertname: RateLimitFailClosed
      receiver: 'synapse-oncall-critical'
      group_wait: 10s
    
    - match:
        alertname: EventNotifierRedisSubscriberDown
      receiver: 'synapse-oncall-critical'
    
    - match:
        alertname: RateLimitRejectedHigh
      receiver: 'synapse-oncall-warning'
```

---

## 验证清单

部署前需确认：

- [ ] Prometheus 已配置 `event_notifier_subscriber_failures_total` 抓取
- [ ] 限流指标（`rate_limit_*`）已在 Prometheus 中可见
- [ ] `/admin/v1/rate-limit-status` 端点可访问（admin 认证）
- [ ] Alertmanager 路由配置正确
- [ ] 429 响应包含标准 `retry-after` 头

---

*文档创建时间：2026-09-18*
