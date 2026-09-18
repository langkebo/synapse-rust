# 缓存/事件/限流 问题诊断与修复方案

**创建时间**: 2026-09-18  
**优先级**: P1（性能 & 一致性）  
**影响范围**: 跨实例会话一致性、sync 性能、限流失效

---

## 问题总览

| 编号 | 问题名称 | 症状 | 根因 | 优先级 |
|------|---------|------|------|--------|
| S-8 | 缓存不对称 | 跨实例会话不一致 | `get_raw` 只读 L1，调用方误用 | P1 |
| S-6~S-9 | 事件驱动未接线 | 250ms DB 轮询浪费 | `EventNotifier` 未完全接入 sync | P2 |
| S-9 | 限流三件套 + 429 掩盖 | 限流失效 | 配置分散 + backend 选择不当 | P1 |

---

## S-8: 缓存不对称（Redis 分布式锁/缓存 L1+L2 读写不对称）

### 症状
- 跨实例会话不一致：实例 A 设置缓存，实例 B 读不到
- 分布式锁在多实例部署中失效

### 现状分析

#### 缓存层次设计（已正确实现）
```rust
// synapse-cache/src/manager.rs:427-458

/// Synchronous read — L1 only
pub fn get_raw(&self, key: &str) -> Option<String> {
    self.local.get_raw(key)
}

/// Async read — L1 → L2 fallback + backfill
pub async fn get_raw_shared(&self, key: &str) -> Option<String> {
    if let Some(val) = self.local.get_raw(key) {
        return Some(val);
    }
    
    // L2: Redis
    if let Some(redis) = &self.redis {
        if let Some(val) = redis.get(key).await {
            self.local.set_raw(key, &val); // Backfill L1
            return Some(val);
        }
    }
    None
}
```

#### 对称性约定
- ✅ `set_raw` → 异步写 L1+L2
- ❌ 同步 `get_raw` → 只读 L1（**正确**）
- ✅ 异步 `get_raw_shared` → 读 L1 回源 Redis 回填 L1（**正确**）

### 问题调用点

搜索结果显示业务代码中存在以下模式：

```rust
// tests/integration/federation_error_tests.rs:232
let result = cache.get_raw("nonexistent");  // ❌ 同步调用，无法回源 Redis
```

**关键发现**: 目前业务代码（`synapse-services/`）**未发现**错误的 `get_raw` 调用；错误仅出现在测试代码中。

### 风险评估

#### 高敏感调用点（已正确使用 `get_raw_shared`）
1. **Token Revocation** (`synapse-services/src/auth/token.rs:57-100`):
   ```rust
   // T11: use get_raw_shared for cross-instance consistency.
   if self.cache.get_raw_shared(&revocation_ok_key).await.is_none() { ... }
   if let Some(marker_val) = self.cache.get_raw_shared(&logout_marker).await { ... }
   ```

2. **Sliding Sync De-duplication** (`synapse-services/src/sliding_sync_service/extensions.rs:234-246`):
   ```rust
   // S7: 去重状态走 `get_raw_shared`（L1 未命中回源 Redis）。
   || self.cache.get_raw_shared(&cache_key).await.map(|prev| prev != canonical).unwrap_or(true);
   ```

3. **Auth Service** (`synapse-services/src/auth/token.rs:378-429`):
   - 所有断言测试均使用 `get_raw_shared`

### 修复建议

#### 行动项
- [ ] **S-8-A1**: 在 `synapse-cache` 模块上增加 lint 门禁，禁止业务代码（非测试）使用 `get_raw`
  ```rust
  // 在 Cargo.toml 或 clippy.toml 中添加
  #[deny(clippy::expect_used)]  // 已经存在
  // 新增：业务代码禁止使用 get_raw
  ```

- [ ] **S-8-A2**: 测试代码清理（可选，不影响生产）
  ```bash
  # 将测试中的 get_raw 改为 get_raw_shared（如果需要跨实例测试）
  ```

- [ ] **S-8-A3**: 文档强化
  在 `synapse-cache/src/manager.rs` 顶部添加调用指南：
  ```rust
  /// ## 调用约定
  /// - 生产环境：**必须**使用 `get_raw_shared().await`（异步 + Redis 回源）
  /// - 测试环境：可使用 `get_raw()` 快速模拟（单进程）
  ```

---

## S-6~S-9: 事件驱动未接线（250ms DB 轮询、Redis 扇出未接线）

### 症状
- `/sync` 接口即使有 `EventNotifier` 仍会进入 250ms DB 轮询
- Redis Pub/Sub未接线导致跨实例通知丢失

### 现状分析

#### EventNotifier 架构（已正确实现）
```rust
// synapse-services/src/event_notifier.rs:47-75
/// When Redis is configured (via [`EventNotifier::with_redis`]), notifications
/// are also published to a Redis Pub/Sub channel so that other server
/// instances in the same deployment can wake their local waiters.
pub struct EventNotifier {
    room_notifiers: Arc<DashMap<String, Arc<Notify>>>,
    user_notifiers: Arc<DashMap<String, Arc<Notify>>>,
    redis_pool: Option<Pool>,
    redis_url: Option<String>,
    // ...
}
```

#### 接入流程（已正确接线）
```rust
// synapse-services/src/container.rs:353-392
let event_notifier = if config.redis.enabled {
    if let Ok(pool) = Pool::create(&redis_config, &connection_url) {
        let notifier = crate::event_notifier::EventNotifier::new()
            .with_redis(pool, redis_url)  // ✅ Redis fan-out wired
            .with_idle_timeout_secs(config.server.event_notifier_idle_timeout_secs);
        
        if let Err(e) = notifier.start_subscriber_task(...) {
            tracing::warn!("Failed to start EventNotifier Redis subscriber: {e}");
        }
    }
}

// Inject into UserService
storage.user_service.set_event_notifier(event_notifier.clone());
```

#### Sync Polling Fallback（降级策略）
```rust
// synapse-services/src/sync_service/event_fetch.rs:172-177
if long_poll_waiters.is_empty() {
    // No notifier wired (tests, benchmarks): degrade to
    // periodic polling. Sleep for the lesser of the poll
    // interval or the remaining timeout.
    let poll_interval = self.sync_poll_interval();  // 250ms default
    tokio::time::sleep(poll_interval.min(remaining)).await;
}
```

### 问题根因

**"250ms 轮询"不是 Bug**，而是设计良好的降级策略：
1. ✅ 生产环境：`EventNotifier` 已接线 → **不轮询**
2. ⚠️ 测试/基准环境：无 `EventNotifier` → **250ms 轮询**（合理降级）

#### 真正的问题（未实现的优化）
1. **Presence 回声自激**: 
   ```rust
   // synapse-services/src/sync_service/data_fetch.rs:237
   // 变化或新增的目标；无变化时返回空，避免 250ms 轮询下 presence 回声自激
   ```
   - **已有去重机制**（S7），但仍存在"未接线时的轮询开销"

2. **Redis 扇出未完全生效**:
   - `start_subscriber_task` 失败时 `warn` 但不阻止启动 → 静默降级

### 风险评估

#### 已验证的接线点
| 组件 | 接线状态 | 证据 |
|------|---------|------|
| EventNotifier 创建 | ✅ | `container.rs:353-378` |
| Redis Pub/Sub | ✅ | `container.rs:359-365` |
| Sync 长轮询 | ✅ | `event_fetch.rs:132-188` |
| UserService 注入 | ✅ | `container.rs:392` |

#### 潜在风险
- **A-1**: `start_subscriber_task` 失败后，跨实例 fan-out 静默失效
- **A-2**: Presence 去重仅在 `sliding_sync` 实现，`/sync v2` 可能回声

### 修复建议

#### 行动项
- [ ] **S-6-A1**: 强化 Redis subscriber 失败处理
  ```rust
  // 修改 container.rs:363
  if let Err(e) = notifier.start_subscriber_task(...) {
      tracing::error!("Failed to start EventNotifier Redis subscriber: {e}. CRITICAL: cross-instance fan-out DISABLED.");
      // 可选：退出启动或发送告警
  }
  ```

- [ ] **S-6-A2**: 统一 Presence 去重到 `/sync v2`
  参考 `sliding_sync_service` 的 S7 实现，在 `data_fetch.rs::get_presence_events` 中添加相同缓存逻辑。

- [ ] **S-6-A3**: 添加健康检查指标
  ```yaml
  metrics:
    event_notifier_redis_fanout_enabled: boolean
    event_notifier_subscriber_active: boolean
  ```

---

## S-9: 限流三件套 + 429 掩盖（配置收敛到单一实现）

### 症状
- 限流配置分散在 `homeserver.yaml` 和专题文件两处
- `backend` 选择混乱（Auto/Redis/Local）
- 429 响应头可能被误判为"限流失效"

### 现状分析

#### 配置层级（已收敛）
```rust
// synapse-common/src/config/rate_limit.rs:1-13
// B-1：限流叶子类型全仓只有一份定义
pub use crate::rate_limit_config::{RateLimitEndpointRule, ...};

// synapse-common/src/rate_limit_config.rs:26-200
// RateLimitConfigFile - 权威实现（支持热更新）
pub struct RateLimitConfigFile {
    pub backend: RateLimitBackend,  // Auto | Redis | Local
    // ...
}
```

#### 限流中间件（已正确处理）
```rust
// synapse-web/src/middleware/rate_limit.rs:13-81
pub async fn rate_limit_middleware(State(ctx): State<CoreContext>, ...) -> Response {
    let config = &ctx.config.rate_limit;
    let file_config = ctx.rate_limit_config();  // 专题文件优先
    
    let enabled = file_config.as_ref().map_or(config.enabled, |c| c.enabled);
    // ...
    
    let backend = file_config.as_ref().map_or(RateLimitBackend::Auto, |c| c.backend);
    if matches!(backend, RateLimitBackend::Redis) && !redis_available {
        // 明确拒绝，不回退到 Local
    }
}
```

### 问题根因

**"限流失效"是误判**。实际上：
1. ✅ **单一实现**: `RateLimitConfigFile` 是权威源（已收敛）
2. ✅ **后端选择**: `Auto` 模式正确（Redis 可用则用，否则 Local）
3. ✅ **错误处理**: `fail_open_on_error` 可配置

#### 429 掩盖问题
- 429 响应包含 `Retry-After` 头
- 客户端误认为"限流失效"可能是：
  1. 客户端未正确解析 429
  2. 限流阈值配置过高（看起来像没有限制）

### 风险评估

#### 已验证的安全措施
| 检查项 | 状态 | 位置 |
|--------|------|------|
| 配置去重 | ✅ | `rate_limit_config.rs` |
| Backend 强制 | ✅ | `rate_limit.rs:80-84` |
| Fail-Closed | ✅ | `rate_limit.rs:70-74` |
| 429 Headers | ✅ | `rate_limit.rs:71 (include_headers)` |

#### 潜在问题
- **B-1**: `homeserver.yaml` 不支持 `backend` 字段（已记录为预期行为）
- **B-2**: `/sync` 专有限流可能在 `sync: enabled=false` 时无效

### 修复建议

#### 行动项
- [ ] **S-9-A1**: 添加限流诊断端点
  ```rust
  // GET /health/rate-limit-status
  {
    "enabled": true,
    "backend": "auto",
    "redis_available": true,
    "active_rules": 15,
    "requests_last_minute": 1234,
    "limited_last_minute": 56
  }
  ```

- [ ] **S-9-A2**: 完善 429 响应头
  ```rust
  // 确保所有 429 响应包含
  - Retry-After: <seconds>
  - X-RateLimit-Limit: <burst>
  - X-RateLimit-Remaining: 0
  - X-RateLimit-Reset: <unix_timestamp>
  ```

- [ ] **S-9-A3**: 监控告警
  ```yaml
  alerts:
    - name: "RateLimitThresholdHigh"
      condition: "limited_requests_ratio > 0.1 over 5m"
      severity: warning
  ```

---

## 总体行动计划

### Phase 1: 立即修复（本周）
| 编号 | 行动 | 优先级 | 预计工时 |
|------|------|--------|---------|
| S-8-A1 | 添加 clippy 门禁禁止业务代码使用 `get_raw` | P1 | 1h |
| S-6-A1 | 强化 Redis subscriber 失败处理 | P1 | 2h |
| S-9-A1 | 添加限流诊断端点 | P1 | 3h |

### Phase 2: 优化完善（下周）
| 编号 | 行动 | 优先级 | 预计工时 |
|------|------|--------|---------|
| S-6-A2 | `/sync v2` Presence 去重统一 | P2 | 4h |
| S-9-A2 | 429 响应头完善 | P2 | 1h |
| S-8-A3 | 文档强化 | P3 | 1h |

### Phase 3: 监控建设（两周内）
| 编号 | 行动 | 优先级 | 预计工时 |
|------|------|--------|---------|
| S-6-A3 | EventNotifier 健康指标 | P2 | 2h |
| S-9-A3 | 限流告警规则 | P2 | 2h |

---

## 验证清单

修复完成后需验证：

- [ ] `cargo clippy --workspace` 无新警告
- [ ] 跨实例测试：实例 A 设置锁，实例 B 能感知
- [ ] `/sync` 压力测试：无 250ms 轮询（生产环境）
- [ ] 限流开启：请求超过阈值返回 429 + 正确 Headers
- [ ] 监控面板：EventNotifier Redis fanout 状态可见

---

## 附录：相关文件索引

| 模块 | 文件 | 职责 |
|------|------|------|
| Cache | `synapse-cache/src/manager.rs` | L1+L2 缓存管理 |
| Events | `synapse-services/src/event_notifier.rs` | 事件通知 |
| Sync | `synapse-services/src/sync_service/event_fetch.rs` | Sync 轮询逻辑 |
| RateLimit | `synapse-web/src/middleware/rate_limit.rs` | 限流中间件 |
| Config | `synapse-common/src/rate_limit_config.rs` | 限流配置 |
