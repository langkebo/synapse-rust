# Prometheus 监控优化实施方案

> **创建时间**: 2026-09-30  
> **文档版本**: v1.0  
> **负责人**: prometheus-ops-expert

---

## ✅ 落地状态（2026-10-01，合并进 `main` 后复核）

`scripts/ci/check_metric_instrumentation.py`（CI 的「Check metric instrumentation
reachability」步骤）当时报 **FAIL 新增未接通埋点**：Phase 1 定义的 7 个 `record_*`
方法一个调用点都没有，而 Phase 3 的面板/告警已经在读它们 —— 即"指标会注册但永不产生
数据 ⇒ 告警永不触发"。本轮处置如下（以门禁为准，不以本文档为准）：

| 指标 | 处置 | 真实调用点 |
|---|---|---|
| `message_delivery_latency_seconds` | ✅ 已接线 | `MessagingService::send_message`（所有发送路径的唯一漏斗，含 txn 去重路径） |
| `room_creation_duration_seconds` | ✅ 已接线 | `LifecycleService::create_room` 埋点包装（覆盖内部全部提前返回） |
| `room_operations_total` | ✅ 已接线 | 同上，`operation="create"` + outcome/error_type 由内层 `ApiResult` 决定 |
| `e2ee_handshake_duration_seconds` | ✅ 已接线 | `DeviceKeysService::claim_keys`（建立 Olm 会话的密钥认领往返） |
| `cache_operations_total` | ✅ 已接线 | `CacheManager` 的单键 `get`/`get_checked`/`set`/`set_checked`/`delete` |
| `message_queue_depth` | ✅ 已接线 | worker 心跳 `heartbeat_load_stats`（Redis 任务队列 pending 数） |
| `db_queries_total` | ❌ **已删除** | 无诚实调用点：sqlx 逐语句事件只给 elapsed、不给成败（成败走 `db_query_errors`）；按表拆标签需要解析 `db.statement`，而 `db_query_metrics.rs` 有一条显式设计注释说故意不读它。面板与告警已改指已接线的 `db_query_errors` |
| `sync_event_delay_seconds` | ❌ **已删除** | 无消费方（监控栈 0 引用）、"事件发生到 sync 交付"在本仓没有可测的时间基准 |

回归保护：`synapse-cache` 与 `synapse-services` 各有一个断言标签组合的用例
（`test_cache_operations_total_records_hit_miss_and_set`、
`create_room_records_failure_outcome_with_labels`）。

**仍未接线**（门禁基线 `scripts/ci/metric_instrumentation_baseline` 里的 12 条历史债）：
`record_auth_attempt`、`record_cache_operation`、`record_csrf_validation`、
`record_db_query`、`record_message_send`、`record_presence_update`、
`record_replay_attack_blocked`、`record_room_operation`、`record_security_validation`、
`record_state_group_resolve`、`record_sync_request`、`record_token_validation`。

---

## 📋 总体实施路线图

```
Phase 1: 新增指标定义与辅助方法 ✅ COMPLETE (2026-09-30)
├── Step 1: 在 server_metrics.rs 中添加新指标定义 ✅
├── Step 2: 实现辅助记录方法 ✅
└── Step 3: Service 层集成调用点清单 ✅

Phase 2: 现有指标标签优化 ⏳ PENDING
├── Step 1: 重构 room_operations_total 统一计数器
├── Step 2: 优化 http_requests_total 添加 endpoint/status_code 标签
├── Step 3: 添加 db_queries_total 细分指标
└── Step 4: 增强 cache_operations_total 标签

Phase 3: 告警规则与可视化 ✅ COMPLETE (2026-09-30)
├── Step 1: 创建 Alertmanager 告警规则配置 ✅
├── Step 2: 生成 Grafana 仪表盘 JSON 配置 ✅
└── Step 3: Docker 部署配置 (需后续集成)
```

---

## 📦 交付成果清单

### Phase 1 交付物 (COMPLETE):
1. **`synapse-common/src/server_metrics.rs`** - 新增指标定义与辅助方法
   - 8 个新指标定义
   - 8 个辅助记录方法
   - MetricsSummary 更新

### Phase 3 交付物 (COMPLETE):
1. **`monitoring/alerting-rules.yml`** - Alertmanager 告警规则配置
   - 6 个告警组
   - 15+ 条告警规则
   - 覆盖 SLO、资源、业务三个维度

2. **`monitoring/grafana/dashboards/grafana-dashboard-slo.json`** - SLO 监控仪表盘
   - 6 个核心面板
   - SLO 可用性追踪
   - 延迟分位数监控

3. **`monitoring/grafana/dashboards/grafana-dashboard-business.json`** - 业务指标仪表盘
   - 10 个业务面板
   - 用户增长追踪
   - E2EE 覆盖率监控
   - 数据库性能分析

---

## Phase 1: 新增指标定义与辅助方法 ✅

### 已完成工作

#### 1.1 指标定义添加 (COMPLETE)

**文件**: `synapse-common/src/server_metrics.rs`

**新增指标**:
| 指标名 | 类型 | 说明 |
|--------|------|------|
| `message_delivery_latency` | Histogram | 消息投递延迟（秒） |
| `message_queue_depth` | Gauge | 消息队列深度 |
| `room_creation_duration` | Histogram | 房间创建耗时（秒） |
| `e2ee_handshake_duration` | Histogram | E2EE 握手耗时（秒） |
| `sync_event_delay` | Histogram | 同步事件延迟（秒） |
| `room_operations_total` | Counter | 统一的房间操作计数器 |
| `db_queries_total` | Counter | 数据库查询细分计数器 |
| `cache_operations_total` | Counter | 缓存操作细分计数器 |

**注册代码位置**: Lines 335-363

---

#### 1.2 辅助方法实现 (COMPLETE)

**新增方法**:

1. **`record_message_delivery()`** - 消息投递延迟记录
   ```rust
   pub fn record_message_delivery(
       &self,
       duration_sec: f64,
       stage: &str,           // "sent", "persisted", "sync_delivered", "push_sent"
       room_type: &str,        // "public", "private", "space"
       message_type: &str,     // "m.room.message", "m.room.encryption", "state"
       encryption: &str,       // "true", "false"
   )
   ```

2. **`set_message_queue_depth()`** - 队列深度设置
   ```rust
   pub fn set_message_queue_depth(&self, depth: f64)
   ```

3. **`record_room_creation()`** - 房间创建记录
   ```rust
   pub fn record_room_creation(
       &self,
       duration_sec: f64,
       outcome: &str,          // "success", "already_exists", "error"
       room_version: &str,     // "10", "11", "12"
       visibility: &str,       // "public", "private"
       has_alias: &str,        // "true", "false"
   )
   ```

4. **`record_e2ee_handshake()`** - E2EE 握手记录
   ```rust
   pub fn record_e2ee_handshake(
       &self,
       duration_sec: f64,
       algorithm: &str,        // "olm.v1.curve25519-aes-sha2", "megolm.v1.aes-sha2"
       operation: &str,        // "session_creation", "key_share", "key_request"
       device_count: &str,     // "1", "2-5", "6-10", "10+"
   )
   ```

5. **`record_sync_event_delay()`** - 同步延迟记录
   ```rust
   pub fn record_sync_event_delay(
       &self,
       duration_sec: f64,
       client_type: &str,      // "web", "mobile", "desktop"
       connection_type: &str,  // "polling", "sse", "websocket"
       room_size: &str,        // "small(<10)", "medium(10-100)", "large(>100)"
   )
   ```

6. **`record_room_operation_labeled()`** - 统一房间操作记录
   ```rust
   pub fn record_room_operation_labeled(
       &self,
       operation: &str,        // "create", "join", "leave", "upgrade", "forget"
       outcome: &str,          // "success", "already_exists", "error", "forbidden"
       room_version: &str,     // "10", "11", "12", "unknown"
       visibility: &str,       // "public", "private"
       error_type: &str,       // "M_FORBIDDEN", "M_INVALID_ROOM_ID", "other"
   )
   ```

7. **`record_db_query_labeled()`** - 数据库查询细分记录
   ```rust
   pub fn record_db_query_labeled(
       &self,
       table: &str,            // "rooms", "events", "members"
       operation: &str,        // "SELECT", "INSERT", "UPDATE", "DELETE"
       outcome: &str,          // "success", "error"
       error_type: &str,       // "unique_violation", "foreign_key_violation", "other"
   )
   ```

8. **`record_cache_operation_labeled()`** - 缓存操作细分记录
   ```rust
   pub fn record_cache_operation_labeled(
       &self,
       cache_type: &str,       // "room_state", "event_body", "membership"
       backend: &str,          // "redis", "memory", "hybrid"
       operation: &str,        // "get", "set", "delete", "invalidate"
       result: &str,           // "hit", "miss", "error", "ttl_expired"
   )
   ```

**方法实现位置**: Lines 581-724

---

### 待完成工作

#### 1.3 Service 层集成 (NEXT STEP)

需要在以下位置添加指标记录调用：

**A. 消息投递场景** (预计 3-5 处)
- `synapse-services/src/message/send.rs` - 消息发送成功时
- `synapse-services/src/message/persist.rs` - 消息持久化时
- `synapse-services/src/sync/handler.rs` - Sync 交付时
- `synapse-services/src/push/gateway.rs` - Push 通知发送时

**B. 房间创建场景** (预计 2-3 处)
- `synapse-services/src/room/lifecycle/create.rs` - 房间创建入口

**C. E2EE 场景** (预计 3-4 处)
- `synapse-services/src/crypto/olm/session.rs` - 会话创建
- `synapse-services/src/crypto/megolm/share.rs` - 密钥分享
- `synapse-services/src/crypto/key/request.rs` - 密钥请求

**D. Sync 延迟场景** (预计 2 处)
- `synapse-services/src/sync/handler.rs` - Sync 处理完成时

---

## Phase 2: 现有指标标签优化

### 2.1 room_operations_total 重构

**目标**: 合并 `room_creates_total`、`room_joins_total`、`room_leaves_total` 为统一计数器

**当前代码**:
```rust
// Line 290-292
pub room_creates_total: Counter,
pub room_joins_total: Counter,
pub room_leaves_total: Counter,
```

**使用点搜索**:
```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust && \
rg "\.room_(creates|joins|leaves)_total\.inc\(\)" -g "*.rs" --type rust
```

**迁移策略**:
1. 保留旧指标作为弃用标记（deprecated），兼容现有 dashboard
2. 新建 `room_operations_total` 带 label，供新调用使用
3. 提供迁移指南，逐步替换所有调用点

---

### 2.2 http_requests_total 标签增强

**目标**: 增加 `endpoint`、`status_code`、`client_agent` 标签

**当前状态**:
```rust
// Line 230
pub http_requests_total: Counter,
```

**实现方案**:
- 在 HTTP middleware 中提取 endpoint 路径模式（如 `/sync`, `/room/:roomId`）
- 从 response headers 提取 client agent 信息
- 使用 `with_label_values()` 记录带标签计数

**参考代码**:
```rust
#[instrument(skip(request))]
async fn http_middleware(
    Request<Body> request,
    Next next,
) -> Response {
    let start = Instant::now();
    let response = next.run(request).await;
    let duration = start.elapsed().as_secs_f64();
    
    let status = response.status().as_u16().to_string();
    let endpoint = extract_endpoint_pattern(&request.uri().path());
    let client_agent = extract_client_agent(&request.headers());
    
    metrics.http_requests_total
        .with_label_values(&[&status, &endpoint, &client_agent])
        .inc();
    
    response
}
```

---

### 2.3 db_queries_total 细分

**目标**: 按表、SQL 操作、结果细分

**实现方案**:
```rust
pub fn record_db_query_labeled(
    &self,
    table: &str,
    operation: &str,
    outcome: &str,
    error_type: &str,
)
```

**集成点**:
- SQLX query layer: 自动捕获表名和 SQL 类型
- Error handler: 分类错误类型

---

### 2.4 cache_operations_total 增强

**目标**: 区分 cache_type、backend、operation、result

**实现方案**:
```rust
pub fn record_cache_operation_labeled(
    &self,
    cache_type: &str,
    backend: &str,
    operation: &str,
    result: &str,
)
```

---

## Phase 3: 告警规则与可视化

### 3.1 Alertmanager 配置

**文件**: `prometheus/alerting-rules.yml`

**规则分类**:
1. **基础健康检查** (critical)
   - HTTP 5xx 错误率 > 5%
   - 数据库查询错误率突增
   
2. **延迟告警** (warning)
   - 房间创建 P99 > 5s
   - 消息投递 P95 > 1s
   - Sync 延迟 P95 > 2s

3. **资源告警** (warning)
   - DB 连接池利用率 > 90%
   - 消息队列积压 > 100
   - Cache 命中率 < 80%

**详细规则**: 见 `promql-queries.md`

---

### 3.2 Grafana 仪表盘

**生成的 dashboard JSON 文件**:
1. `grafana/dashboards/synapse-slo-monitoring.json`
2. `grafana/dashboards/synapse-business-metrics.json`
3. `grafana/dashboards/synapse-troubleshooting.json`

**参考查询模板**: 见 `promql-queries.md`

---

## 🔧 实施检查清单

### Phase 1 Checklist ✅
- [x] 新指标在 ServerMetrics 中定义
- [x] 辅助记录方法实现
- [x] MetricsSummary 更新
- [x] Service 层集成调用点改造（2026-10-01 完成；`db_queries_total` / `sync_event_delay` 因无可测语义删除，见文首「落地状态」）

### Phase 2 Checklist 🔄
- [ ] room_operations_total 重构
- [ ] http_requests_total 标签增强
- [x] ~~db_queries_total 细分~~（删除：无可诚实接线的调用点，见文首「落地状态」）
- [ ] cache_operations_total 增强

### Phase 3 Checklist ⏳
- [ ] Alertmanager 告警规则文件
- [ ] Grafana 仪表盘 JSON 配置
- [ ] Docker Compose 监控栈更新
- [ ] 验证测试

---

## 📊 编译验证

**编译命令**:
```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust && \
PATH="/usr/bin:/bin:$PATH" cargo check --all-targets
```

**预期结果**: 
- ✅ 无编译错误
- ⚠️ 可能有 warning 关于未使用的指标（需调用点改造后才消除）

---

## 📝 下一步行动

**立即执行**:
1. 运行 `cargo check` 验证 Phase 1 代码正确性
2. 搜索 service 层调用点，制定改造清单
3. 开始 Phase 2 的指标标签优化

**文档参考**:
- 指标设计文档：`prometheus-custom-metrics.md`
- PromQL 查询模板：`promql-queries.md`

---

## 🎯 验收标准

Phase 1 完成标准:
- [ ] 所有新指标定义编译通过
- [ ] 至少 2 个核心场景已集成（如房间创建、消息投递）
- [ ] 验证数据可通过 `curl localhost:9090/api/v1/query?query=message_delivery_latency_seconds` 查询

Phase 2 完成标准:
- [ ] 旧指标保留但标记为 deprecated
- [ ] 新标签在所有主要调用点生效
- [ ] 验证可用 `sum by (endpoint) (rate(http_requests_total[5m]))` 等新查询

Phase 3 完成标准:
- [ ] 所有告警规则通过 `promtool test rules` 验证
- [ ] Grafana 仪表盘正常导入并显示数据
- [ ] 模拟故障触发告警并收到通知
