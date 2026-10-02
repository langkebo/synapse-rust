# Synapse-Rust Prometheus 自定义指标与优化方案

> **最后更新**: 2026-09-30  
> **维护者**: prometheus-ops-expert  
> **适用范围**: synapse-rust Matrix homeserver 监控栈

---

## 📊 新增自定义指标

### 1. 消息延迟指标

#### `message_delivery_latency_seconds`

**描述**: 端到端消息投递延迟（从客户端发送到接收端收到）

**类型**: Histogram

**分桶**: `[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]`

**标签**:
| 标签名 | 说明 | 示例值 |
|--------|------|--------|
| `room_type` | 房间类型 | `public`, `private`, `space` |
| `message_type` | 消息类型 | `m.room.message`, `m.room.encryption`, `state` |
| `delivery_stage` | 投递阶段 | `sent`, `persisted`, `sync_delivered`, `push_sent` |
| `encryption` | 是否加密 | `true`, `false` |

**PromQL 示例**:
```promql
# P95 消息投递延迟（5 分钟窗口）
histogram_quantile(0.95, rate(message_delivery_latency_seconds_bucket[5m]))

# 按房间类型分组的平均延迟
avg by (room_type) (rate(message_delivery_latency_seconds_sum[5m]) / rate(message_delivery_latency_seconds_count[5m]))

# 超过 1 秒的消息占比
sum(rate(message_delivery_latency_seconds_bucket{le="1"}[5m])) / sum(rate(message_delivery_latency_seconds_count[5m]))
```

---

#### `message_queue_depth`

**描述**: 待投递消息队列深度（积压量）

**类型**: Gauge

**标签**:
| 标签名 | 说明 | 示例值 |
|--------|------|--------|
| `queue_name` | 队列名称 | `redis_pubsub`, `worker_pool`, `push_gateway` |
| `priority` | 优先级 | `high`, `normal`, `low` |

**PromQL 示例**:
```promql
# 当前队列深度
message_queue_depth

# 队列积压警告 (>100)
message_queue_depth > 100

# 队列增长速率
delta(message_queue_depth[5m]) > 50
```

---

### 2. 房间创建延迟指标

#### `room_creation_duration_seconds`

**描述**: 完整房间创建流程耗时

**类型**: Histogram

**分桶**: `[0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0]`

**标签**:
| 标签名 | 说明 | 示例值 |
|--------|------|--------|
| `room_version` | 房间版本 | `10`, `11`, `12` |
| `visibility` | 可见性 | `public`, `private` |
| `has_alias` | 是否有别名 | `true`, `false` |
| `create_outcome` | 创建结果 | `success`, `already_exists`, `error` |

**PromQL 示例**:
```promql
# P99 房间创建延迟
histogram_quantile(0.99, rate(room_creation_duration_seconds_bucket[5m]))

# 按房间版本分组的中位数
histogram_quantile(0.5, rate(room_creation_duration_seconds_bucket{room_version="12"}[5m]))

# 已存在房间的比率（应接近 0，表示幂等性工作正常）
sum(rate(room_creation_duration_seconds_count{create_outcome="already_exists"}[5m])) 
/ 
sum(rate(room_creation_duration_seconds_count[5m]))
```

---

### 3. 端到端加密延迟

#### `e2ee_handshake_duration_seconds`

**描述**: E2EE 握手/密钥交换耗时

**类型**: Histogram

**分桶**: `[0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0, 30.0, 60.0]`

**标签**:
| 标签名 | 说明 | 示例值 |
|--------|------|--------|
| `algorithm` | 加密算法 | `olm.v1.curve25519-aes-sha2`, `megolm.v1.aes-sha2` |
| `operation` | 操作类型 | `session_creation`, `key_share`, `key_request` |
| `device_count` | 设备数量 | `1`, `2-5`, `6-10`, `10+` |

**PromQL 示例**:
```promql
# Megolm 密钥分享 P95
histogram_quantile(0.95, rate(megolm_key_exchange_duration_seconds_bucket{algorithm="megolm.v1.aes-sha2"}[5m]))

# 设备数对握手时间的影响
avg by (device_count) (rate(e2ee_handshake_duration_seconds_sum[5m]) / rate(e2ee_handshake_duration_seconds_count[5m]))
```

---

### 4. 同步延迟指标

#### `sync_event_delay_seconds`

**描述**: 事件发生到用户同步到的延迟

**类型**: Histogram

**分桶**: `[0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]`

**标签**:
| 标签名 | 说明 | 示例值 |
|--------|------|--------|
| `client_type` | 客户端类型 | `web`, `mobile`, `desktop` |
| `connection_type` | 连接类型 | `polling`, `sse`, `websocket` |
| `room_size` | 房间规模 | `small(<10)`, `medium(10-100)`, `large(>100)` |

**PromQL 示例**:
```promql
# 移动端 SSE 连接的 P99 同步延迟
histogram_quantile(0.99, rate(sync_event_delay_seconds_bucket{client_type="mobile",connection_type="sse"}[5m]))

# 不同连接类型的延迟对比
avg by (connection_type) (rate(sync_event_delay_seconds_sum[5m]) / rate(sync_event_delay_seconds_count[5m]))
```

---

## 🏷️ 优化现有指标标签

### 当前问题

现有指标标签粒度不足，难以精确定位问题：

| 当前指标 | 现有标签 | 问题 |
|---------|---------|------|
| `room_creates_total` | 无 | 无法区分成功/失败、房间版本 |
| `messages_sent_total` | 无 | 无法按消息类型、房间类型分析 |
| `sync_duration_ms` | `{unit="ms"}` | 缺少客户端类型、连接方式维度 |
| `http_request_duration_ms` | `{unit="ms"}` | 缺少 endpoint、状态码维度 |

---

### 建议的标签优化方案

#### 1. `room_operations_total` (重构 `room_creates_total`)

**新增标签**:
```yaml
operation: create|join|leave|upgrade|forget
outcome: success|already_exists|error|forbidden
room_version: 10|11|12|unknown
visibility: public|private
error_type: M_FORBIDDEN|M_INVALID_ROOM_ID|M_UNKNOWN|other
```

**示例查询**:
```promql
# 按操作类型和结果的错误率
sum by (operation, outcome) (rate(room_operations_total[5m]))

# 房间版本 12 的成功率
sum(rate(room_operations_total{room_version="12",outcome="success"}[5m])) 
/ 
sum(rate(room_operations_total{room_version="12"}[5m]))
```

---

#### 2. `http_requests_total` (增强现有指标)

**新增标签**:
```yaml
method: GET|POST|PUT|DELETE
status_code: 200|201|400|401|403|404|500|502|503
endpoint: /_matrix/client/v3/login|/createRoom|/sync|...
client_agent: mobile|web|desktop|bot
```

**示例查询**:
```promql
# 各接口的 P95 延迟
histogram_quantile(0.95, 
  sum by (endpoint) (rate(http_request_duration_ms_bucket[5m]))
)

# 5xx 错误率趋势
sum(rate(http_requests_total{status_code=~"5.."}[5m])) 
/ 
sum(rate(http_requests_total[5m]))

# 慢请求分布 (>1s)
sum(rate(http_request_duration_ms_bucket{le="+Inf"}[5m])) 
- 
sum(rate(http_request_duration_ms_bucket{le="1000"}[5m]))
```

---

#### 3. ~~`db_queries_total`~~（2026-10-01 删除）

> ⚠️ 本节描述的指标**已从代码中删除**：它没有诚实调用点 —— sqlx 的逐语句 tracing 事件只给
> `elapsed`、不给成败（失败计数走 `db_query_errors`），而按表拆标签需要解析 `db.statement`，
> 与 `synapse-common/src/db_query_metrics.rs` 中"故意不分配/不拷贝该字段"的设计注释冲突。
> 监控栈里的面板与告警已改指 `db_query_errors`。以下为历史设计原文。

#### 3. (历史) `db_queries_total` (新增细分指标)

**新增指标**:
```yaml
db_queries_total{operation="SELECT|INSERT|UPDATE|DELETE",table="rooms|events|members",outcome="success|error"}
db_query_latency_seconds{operation="SELECT|INSERT|UPDATE|DELETE",table="rooms|events|members"}
db_lock_wait_seconds{lock_type="row|table"}
```

**示例查询**:
```promql
# 按表的慢查询比例
sum(rate(db_query_latency_seconds_bucket{table="events",le="100"}[5m])) 
/ 
sum(rate(db_query_latency_seconds_count{table="events"}[5m]))

# 事务等待时间
rate(db_lock_wait_seconds[5m])

# 最常用的慢 SQL 表
topk(5, 
  sum by (table) (rate(db_query_latency_seconds_sum[5m]))
)
```

---

#### 4. `cache_operations_total` (增强现有指标)

**新增标签**:
```yaml
cache_backend: redis|memory|hybrid
cache_type: room_state|event_body|membership|user_profile
operation: get|set|delete|invalidate
result: hit|miss|error|ttl_expired
```

**示例查询**:
```promql
# 各类缓存的命中率
sum by (cache_type) (rate(cache_operations_total{result="hit"}[5m])) 
/ 
sum by (cache_type) (rate(cache_operations_total[5m]))

# Redis 后端错误率
sum(rate(cache_operations_total{cache_backend="redis",result="error"}[5m])) 
/ 
sum(rate(cache_operations_total{cache_backend="redis"}[5m]))

# TTL 过期比例（可能表示 TTL 设置过短）
sum(rate(cache_operations_total{result="ttl_expired"}[5m])) 
/ 
sum(rate(cache_operations_total[5m]))
```

---

#### 5. `federation_operations_total` (细化现有指标)

**新增标签**:
```yaml
operation: send_pdu|get_event|make_join|send_join|query_keys
target_server: 远程服务器域名
result: success|timeout|4xx|5xx|signature_error|invalid_json
timeout_class: short(<5s)|medium(5-30s)|long(>30s)
```

**示例查询**:
```promql
#  federation 错误率（按目标服务器）
sum by (target_server, result) (rate(federation_operations_total[5m])) 

# 超时分布
sum by (timeout_class) (rate(federation_operation_duration_seconds_bucket{result=~"timeout|short|medium|long"}[5m]))

# 签名验证失败率
sum(rate(federation_operations_total{result="signature_error"}[5m])) 
/ 
sum(rate(federation_signature_verifications[5m]))
```

---

## 📝 PromQL 查询模板库

### 告警规则模板

#### 1. 高错误率告警

```yaml
groups:
- name: synapse-error-rate
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

  # DB 查询错误率突增
  - alert: HighDatabaseErrorRate
    expr: |
      rate(db_queries_total{outcome="error"}[5m]) > 10
    for: 2m
    labels:
      severity: critical
    annotations:
      summary: "数据库查询错误率高 ({{ $value }}/s)"
```

---

#### 2. 延迟告警

```yaml
groups:
- name: synapse-latency
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

---

#### 3. 资源告警

```yaml
groups:
- name: synapse-resources
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

  # 工作队列积压
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

### Grafana 仪表盘查询模板

#### 1. SLO 监控面板

```promql
# ==================== 可用性 SLO ====================
# 目标：99.9% 的 HTTP 请求在 5 分钟内成功

# 实际可用性
(
  sum(increase(http_requests_total[5m])) 
  - 
  sum(increase(http_requests_total{status_code=~"5.."}[5m]))
) 
/ 
sum(increase(http_requests_total[5m]))

# 剩余错误预算（0=耗尽，1=充足）
0.001 - (
  sum(increase(http_requests_total{status_code=~"5.."}[5m])) 
  / 
  sum(increase(http_requests_total[5m]))
)

# ==================== 延迟 SLO ====================
# 目标：95% 的 /sync 请求在 1 秒内响应

# 达标率
sum(rate(sync_duration_ms_bucket{le="1000"}[5m])) 
/ 
sum(rate(sync_duration_ms_count[5m]))

# 违反率
1 - (
  sum(rate(sync_duration_ms_bucket{le="1000"}[5m])) 
  / 
  sum(rate(sync_duration_ms_count[5m]))
)

# ==================== 饱和度 SLO ====================
# 目标：队列长度 < 100

message_queue_depth / 100
```

---

#### 2. 业务指标面板

```promql
# ==================== 实时流量 ====================
# 每秒活跃用户（去重困难，用同步请求近似）
rate(sync_requests_total[1m]) * 60  # 估算每分钟活跃用户

# 房间创建速率
rate(room_operations_total{operation="create"}[1m])

# 消息发送速率
rate(messages_sent_total[1m])

# ==================== 用户行为分析 ====================
# 新用户注册占比（需配合 user_registrations_total）
rate(user_registrations_total[1h]) / avg_over_time(total_users[1h])

# 房间加入 vs 离开比
sum(rate(room_operations_total{operation="join"}[1h])) 
/ 
sum(rate(room_operations_total{operation="leave"}[1h]))

# ==================== 加密覆盖率 ====================
# 加密消息占比
sum(rate(messages_sent_total{encryption="true"}[1h])) 
/ 
sum(rate(messages_sent_total[1h]))

# E2EE 密钥分享速率
rate(megolm_share_total[1m])
```

---

#### 3. 故障诊断面板

```promql
# ==================== 错误聚合 ====================
# 按错误类型分组
sum by (error_type) (rate(http_request_errors_total[5m]))

# 按数据库表分组
sum by (table) (rate(db_queries_total{outcome="error"}[5m]))

# ==================== 延迟分布 ====================
# P50/P95/P99 延迟热力图
histogram_quantile(0.5, rate(http_request_duration_ms_bucket[5m]))
histogram_quantile(0.95, rate(http_request_duration_ms_bucket[5m]))
histogram_quantile(0.99, rate(http_request_duration_ms_bucket[5m]))

# ==================== 异常检测 ====================
# 错误率突增（相对基线）
rate(http_requests_total{status_code=~"5.."}[5m]) / avg_over_time(rate(http_requests_total{status_code=~"5.."}[5m])[1h:]) > 2

# 延迟突增（相对历史同期）
rate(sync_duration_ms_sum[5m]) / rate(sync_duration_ms_count[5m]) > avg_over_time(rate(sync_duration_ms_sum[5m])/rate(sync_duration_ms_count[5m])[1h:] * 1.5

# ==================== 关联分析 ====================
# 高延迟期间的错误类型分布
sum by (error_type) (rate(http_request_errors_total[5m])) 
* 
(on() vector(1) > 
  histogram_quantile(0.95, rate(http_request_duration_ms_bucket[5m])) > 1000
)
```

---

## 🔧 实施步骤

### Phase 1: 新增指标埋点

#### Step 1.1: 在 `server_metrics.rs` 中定义新指标

```rust
// synapse-common/src/server_metrics.rs

// 消息延迟
pub message_delivery_latency: Histogram,
pub message_queue_depth: Gauge,

// 房间创建延迟
pub room_creation_duration: Histogram,

// 端到端加密
pub e2ee_handshake_duration: Histogram,

// 同步延迟
pub sync_event_delay: Histogram,
```

#### Step 1.2: 实现记录方法

```rust
// 在 ServerMetrics impl 块中添加
pub fn record_message_delivery(&self, duration: f64, stage: &str, room_type: &str) {
    self.message_delivery_latency.observe_with_labels(
        duration,
        [("stage", stage), ("room_type", room_type)]
    );
}

pub fn record_room_creation(&self, duration: f64, outcome: &str, version: u8) {
    self.room_creation_duration.observe_with_labels(
        duration,
        [("outcome", outcome), ("room_version", &version.to_string())]
    );
}
```

#### Step 1.3: 在服务层集成

```rust
// synapse-services/src/room/lifecycle/create.rs

use std::time::Instant;

async fn create_room(&self, user_id: &str, config: CreateRoomConfig) -> ApiResult<Value> {
    let start = Instant::now();
    
    // ... existing code ...
    
    let outcome = match result {
        Ok(_) => "success",
        Err(_) => "error",
    };
    
    // 记录指标
    if let Some(metrics) = global_server_metrics() {
        metrics.record_room_creation(
            start.elapsed().as_secs_f64(),
            outcome,
            room_version
        );
    }
    
    Ok(result)
}
```

---

### Phase 2: 标签优化

#### Step 2.1: 修改现有指标注册

```rust
// 从
pub room_creates_total: Counter,

// 改为
pub room_operations_total: Counter,  // 带 operation 标签
```

#### Step 2.2: 更新调用点

```rust
// 旧代码
metrics.room_creates_total.inc();

// 新代码
metrics.room_operations_total
    .with_label_values(&["create", "success", "12", "private"])
    .inc();
```

---

### Phase 3: PromQL 部署

#### Step 3.1: 创建 Alertmanager 规则文件

```yaml
# prometheus/rules/synapse.yml

groups:
  # ... (使用上面的告警规则模板)
```

#### Step 3.2: 配置 Grafana 数据源

```yaml
# grafana/provisioning/datasources/prometheus.yml

datasources:
  - name: synapse-prometheus
    type: prometheus
    url: http://prometheus:9090
    access: proxy
    isDefault: true
```

#### Step 3.3: 导入预设仪表盘

- Dashboard 1: SLO Monitoring (ID: 10001)
- Dashboard 2: Business Metrics (ID: 10002)
- Dashboard 3: Troubleshooting (ID: 10003)

---

## 📈 预期收益

| 指标类别 | 改进前 | 改进后 | 价值 |
|---------|--------|--------|------|
| **问题定位速度** | 30+ 分钟 | < 5 分钟 | 细粒度标签可快速定位问题范围 |
| **错误根因识别** | 模糊分类 | 精确到错误类型 | 减少误报，提高告警准确性 |
| **SLO 可见性** | 手动统计 | 自动计算 | 实时监控 SLO 达成情况 |
| **容量规划** | 经验判断 | 数据驱动 | 基于真实的队列深度、DB 负载数据 |
| **用户体验评估** | 无 | 端到端延迟监控 | 可量化的服务质量指标 |

---

## 🔗 相关文档

- [load-test-problem-report.md](../tests/load-test-problem-report.md) - 负载测试问题分析
- [prometheus-bottleneck-diagnosis](../../skills/prometheus-bottleneck-diagnosis/SKILL.md) - Prometheus 瓶颈诊断技能
- [Grafana Dashboard Guidelines](https://grafana.com/docs/grafana/latest/dashboards/)

---

## 📞 联系方式

- **监控运维专家**: prometheus-ops-expert
- **问题反馈**: 使用 `/ask prometheus-ops-expert` 启动新会话
