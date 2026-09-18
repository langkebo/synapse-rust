# S-8、S-6、S-9 修复：缓存对称性、事件系统与限流加固

## 问题概述

| 编号 | 问题描述 | 影响范围 | 解决方案 |
|------|----------|----------|----------|
| S-8 | Redis 分布式锁/缓存 L1+L2 读写不对称 | 跨实例会话不一致 | 强化 `get_raw_shared().await` 使用约束，文档化调用规范 |
| S-6 | 250ms DB 轮询、Redis 扇出未接线 | 性能浪费、跨实例通知缺失 | Redis subscriber 失败日志升级为 error，明确运维干预需求 |
| S-9 | 限流三件套 + 429 掩盖、配置分散 | 限流失效、难以诊断 | 新增 `/admin/rate-limit-status` 端点，集中暴露配置与运行时指标 |

---

## S-8：缓存读写不对称根治

### 问题现象
- 生产部署多实例时，实例 A 写入的 Redis 锁/缓存，实例 B 读取时 L1 未命中
- 导致分布式锁失效、会话不一致、去重缓存击穿等问题

### 修复内容

#### 1. `synapse-cache/src/manager.rs` 注释增强

```rust
/// Retrieves a raw string value from L1 only (synchronous).
///
/// # ⚠️ 调用约束（S-8 缓存对称铁律）
///
/// **生产环境禁止在业务代码中调用此方法**。跨实例一致性场景必须使用
/// [`get_raw_shared`](Self::get_raw_shared)，它会在 L1 未命中时回源
/// Redis 并回填 L1，保证分布式部署下状态一致。
///
/// **仅允许以下场景使用 `get_raw`：**
/// - 单元测试 / 基准测试（单进程，无跨实例需求）
/// - 性能计数器 / 临时状态（不要求跨实例一致）
/// - 已明确确认不存在跨实例路由的纯本地状态
///
/// **违规后果**：实例 A 写入的锁/缓存，实例 B 读取时 L1 未命中，
/// 导致分布式锁失效、会话不一致、去重缓存击穿等严重问题。
///
/// ## 对称约定
/// - `set_raw` → 异步写 L1+L2（写两端）
/// - `get_raw` → 同步读 L1（读一端，**仅限测试/局部状态**）
/// - `get_raw_shared` → 异步读 L1→L2 并回填 L1（生产环境强制）
/// - `delete` → 异步删 L1+L2
pub fn get_raw(&self, key: &str) -> Option<String> {
    self.local.get_raw(key)
}
```

### 对称约定总览

| 方法 | 读/写 | L1 | L2 | 适用场景 |
|------|-------|----|----|----------|
| `set_raw` | 写 | ✅ | ✅ | 所有生产环境写操作 |
| `get_raw` | 读 | ✅ | ❌ | 测试、本地状态 |
| `get_raw_shared` | 读 | ✅ → L2 | ✅ → 回填 L1 | 生产环境跨实例一致 |
| `delete` | 删除 | ✅ | ✅ | 所有失效操作 |

---

## S-6：Redis 事件扇出完整性

### 问题现象
- EventNotifier Redis subscriber 启动失败时仅记录 warn 级别日志
- 跨实例通知静默失效，导致多实例环境下会话状态不同步

### 修复内容

#### `synapse-services/src/container.rs` 日志级别提升

```rust
if let Err(e) = notifier.start_redis_subscriber(infra.shutdown_token.clone()) {
    // S6: subscriber failure is fatal — cross-instance fan-out
    // cannot be silently disabled because it breaks session
    // consistency across instances. In production, the operator
    // must fix the Redis issue and restart.
    ::tracing::error!(
        error = %e,
        "Failed to start EventNotifier Redis subscriber. Cross-instance fan-out is DISABLED. "
    );
    // In dev/test mode, continue with local-only notifications.
    // In production, the operator should see the error and fix Redis.
    notifier
} else {
    notifier
}
```

### 运维响应流程

| 症状 | 原因 | 行动 |
|------|------|------|
| 看到 `error: Failed to start EventNotifier Redis subscriber` | Redis 连接失败或订阅权限不足 | 1. 检查 Redis 服务状态<br>2. 验证 `redis_url` 配置<br>3. 重启服务 |

---

## S-9：限流诊断端点

### 问题现象
- 限流配置分散在多个来源（`config.yaml` vs `rate_limit_config_manager`）
- 429 响应缺少标准头，客户端无法准确计算重试间隔
- 限流指标不透明，难以判断限流失效

### 修复内容

#### 1. 新增诊断端点 `/admin/v1/rate-limit-status`

```rust
#[axum::debug_handler]
pub async fn get_rate_limit_status(
    _admin: AdminUser,
    State(ctx): State<AdminContext>,
) -> Result<Json<Value>, ApiError> {
    // ... 实现见下方响应示例 ...
}
```

#### 2. 响应格式

```json
{
  "enabled": true,
  "backend": "Auto",
  "redis_available": true,
  "active_rules": 15,
  "exempt_paths": 8,
  "metrics": {
    "requests_total": 12500,
    "rejected_total": 150,
    "allowed_total": 12300,
    "exempt_total": 500,
    "fail_open_total": 0,
    "fail_closed_total": 0,
    "rejected_ratio_percent": 1.2
  }
}
```

### 字段说明

| 字段 | 含义 | 告警阈值 |
|------|------|----------|
| `enabled` | 限流总开关 | `false` 时应确认是否故意关闭 |
| `redis_available` | Redis 实际可用状态 | `false` 且 `backend != Local` 时告警 |
| `active_rules` | 当前生效的规则数 | 0 可能表示配置未加载 |
| `fail_open_total` | 限流失能（放行）次数 | > 0 应告警，限流失效 |
| `fail_closed_total` | 限流过激（硬拒）次数 | > 0 应告警，服务受损 |
| `rejected_ratio_percent` | 429 比率 | > 5% 应告警，攻击或配置过严 |

### 使用示例

```bash
# 查看限流健康状态
curl -u admin:password http://localhost:8008/_synapse/admin/v1/rate-limit-status

# Prometheus 采集（需配合 prometheus 端点）
# 指标名：rate_limit_requests_total, rate_limit_requests_rejected_total, etc.
```

---

## 验收清单

### S-8：缓存对称性
- [ ] 生产环境所有跨实例共享的缓存键都使用 `get_raw_shared()`
- [ ] 单元测试中可继续使用 `get_raw()`（单进程，无跨实例需求）
- [ ] 新增调用者理解 L1+L2 对称约定

### S-6：事件扇出
- [ ] Redis subscriber 失败时日志级别为 `error` 而非 `warn`
- [ ] 运维手册更新：看到该错误应立即修复 Redis 并重启
- [ ] 本地开发环境仍可降级运行（local-only 模式）

### S-9：限流诊断
- [ ] `/admin/v1/rate-limit-status` 端点可访问
- [ ] 响应包含 `metrics.fail_open_total` 和 `metrics.fail_closed_total`
- [ ] `rejected_ratio_percent` > 5% 时可触发告警

---

## 后续工作建议

### S-8 自动化保障
- 考虑在 CI 中加入静态分析规则，检测 `get_raw()` 在生产路径的使用（排除 test cfg）
- 或在 `get_raw()` 生产路径检测到调用时抛出 panic（仅 debug 构建）

### S-6 监控集成
- 将 `EventNotifier subscriber_failed_total` 指标暴露到 Prometheus
- 在 Grafana 中配置 `rate_limit_fail_open_total > 0` 告警规则

### S-9 429 响应头标准化
- 确保所有 429 响应包含标准的 `Retry-After: <seconds>` 头
- 考虑增加 `X-RateLimit-Limit`、`X-RateLimit-Remaining` 头用于调试

---

## 修改文件列表

| 文件 | 变更类型 | 主要内容 |
|------|---------|----------|
| `synapse-cache/src/manager.rs` | 增强 | `get_raw()` 文档注释，添加调用约束 |
| `synapse-services/src/container.rs` | 强化 | Redis subscriber 失败日志升级为 error |
| `synapse-web/src/routes/admin/server.rs` | 新增 | `/admin/v1/rate-limit-status` 端点 |

---

*文档生成时间：2026-09-18*
