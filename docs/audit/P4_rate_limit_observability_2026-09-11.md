# 限流配置降级可观测性（P4 §5.6 / §5.7 收尾）

> **日期**: 2026-09-11
> **基线提交**: `14570209`
> **对应待办**: `docs/audit/P4_performance_baseline_2026-09-11.md` §8.2 第 1 项（**高**）

---

## 1. 缺陷回顾

`docs/audit/P4_performance_baseline_2026-09-11.md` §5.6/§5.7 记录了两条**静默降级**路径：

| 路径 | 触发条件 | 修复前的可观测性 |
|---|---|---|
| 启动回退 | `RATE_LIMIT_CONFIG_PATH` 指向的文件不存在或解析失败 | 单条 `tracing::warn!`；**无 metric、无 health 字段** |
| 热加载失败 | 文件被删除/失效（如单文件 bind mount 的 inode 被宿主机原子替换） | watcher 每 30s 重复一条 WARN，**永远不升级、不计数** |

两条路径的共同后果：**服务继续用旧配置（或内置默认值）运行，运维完全无法从机器可读信号察觉**。
`src/server/mod.rs:211-243` 的两个分支都返回 `Some(manager)`，因此
`ctx.rate_limit_config()` 永不为 `None`，运维写在 `homeserver.yaml` 的
`rate_limit:` 段被静默忽略（§5.7）。

---

## 2. 修复

### 2.1 管理器暴露降级状态

`synapse-common/src/rate_limit_config.rs` 新增：

```rust
pub enum ConfigSource { File, Defaults }        // + as_str() -> "file" / "defaults"

pub struct RateLimitDegradation {
    pub source: ConfigSource,
    pub consecutive_failures: u64,   // 自上次成功以来的失败次数
    pub total_failures: u64,         // 进程生命周期累计
    pub last_error: Option<String>,  // 最近一次错误
}
impl RateLimitDegradation { pub fn is_degraded(&self) -> bool }

impl RateLimitConfigManager {
    pub fn degradation(&self) -> RateLimitDegradation;
    pub fn config_path(&self) -> &std::path::Path;
}
```

关键设计点：

* 降级状态放在 `Arc<parking_lot::Mutex<_>>` 内，**跨 `Arc` clone 共享** —— watcher 与
  health 端点看到同一份状态（有测试覆盖）。
* **`reload()` 成功时清零 `consecutive_failures` 并把 `source` 置回 `File`**：
  启动时回退到默认值、之后文件修好，服务能够自动恢复并如实报告。
* `total_failures` 保留累计值，供事后分析。
* `new()`（内置默认值构造器）显式标记 `source: Defaults` —— 它就是"配置文件缺失"的降级路径。

### 2.2 启动路径升级日志等级

`src/server/mod.rs`：

* 配置文件**存在但解析失败**：`tracing::warn!` → **`tracing::error!`**，
  带 `target: "security_audit"`、`event = "rate_limit_config_degraded"`。
  （"文件不存在"保持 WARN：首次部署尚未放置配置属正常。）
* 启动后注册 gauge：

```rust
app_state.services.core.metrics
    .register_gauge("rate_limit_config_source_is_file".to_string())
    .set(if is_file { 1.0 } else { 0.0 });
```

`1` = 运维配置生效，`0` = 已降级到内置默认值。告警可直接对 0 取值。

### 2.3 watcher 失败升级

`start_config_watcher` 现在读取降级状态：连续失败 ≥
`RELOAD_FAILURE_ESCALATION_THRESHOLD`（= 3，默认 30s 间隔即约 90s）后由 WARN
升级为 **ERROR**，并带上 `consecutive_failures` / `total_failures` / `path` 字段。

阈值取 3 的理由：足以骑过一次原子重命名的瞬时竞态，又不至于让真实故障长时间停留在 WARN。

### 2.4 `/health` 与 `/_health` 暴露来源

`src/web/routes/handlers/health.rs` 新增 `rate_limit_config` 字段：

```json
{
  "status": "healthy",
  "timestamp": "...",
  "rate_limit_config": {
    "status": "healthy",              // healthy | degraded | absent
    "source": "file",                 // file | defaults
    "degraded": false,                // ← 告警只需看这一个布尔
    "consecutive_reload_failures": 0,
    "total_reload_failures": 0,
    "last_error": null,
    "path": "/app/config/rate_limit.yaml"
  }
}
```

`/_health`（detailed）同样插入该片段，并纳入 `overall_status`：降级会把整体
从 `healthy` 拉低到 **`degraded`**（而不是 `unhealthy`）。

> **设计取舍**：降级**不**让 `/health` 返回 503。服务确实还在提供限流（用旧配置或默认值），
> 让容器反复重启比给出清晰信号更糟。这一点写在函数文档注释里，避免后人误改。

---

## 3. 回归证据（14 个新测试）

### `synapse-common`（6 个，`rate_limit_config::degradation_tests`）

```console
$ cargo test -p synapse-common --lib degradation_tests
running 6 tests
test manager_loaded_from_file_reports_file_source ... ok
test manager_built_from_defaults_reports_default_source ... ok
test reload_failure_records_error_and_counters ... ok
test successful_reload_clears_degradation ... ok
test degradation_is_readable_from_the_shared_handle ... ok
test config_source_labels_are_stable ... ok
test result: ok. 6 passed; 0 failed
```

覆盖：来源标记、解析失败与文件缺失两种失败模式、连续/累计计数、错误信息内容、
失败后恢复清零、跨 `Arc` 可见性、指标标签稳定性。

### 根 crate（5 个，`health::rate_limit_health_tests`）

```console
$ cargo nextest run --profile test --features test-utils --lib \
    -E 'test(/rate_limit_health_tests|degradation_tests/)'
    Summary [0.021s] 8 tests run: 8 passed
```

（8 = 5 个新的 health 片段测试 + 3 个既有 `lockout_degradation_tests`）

---

## 4. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 单元测试
cargo test -p synapse-common --lib degradation_tests
cargo nextest run --profile test --features test-utils --lib -E 'test(/rate_limit_health_tests/)'

# 运行时：健康字段
curl -s http://localhost:8008/health | python3 -m json.tool | grep -A 9 rate_limit_config
curl -s http://localhost:8008/_health | python3 -m json.tool | grep -A 9 rate_limit_config

# 运行时：gauge（Prometheus 端点，若已启用）
curl -s http://localhost:8008/metrics | grep rate_limit_config_source_is_file

# 故障注入（在本地 dev 栈上验证退化路径；之后务必恢复）
# 注意：单文件 bind mount 的 inode 失效需要重启容器才能恢复（P4 §5.6）
cp docker/deploy/config/rate_limit.yaml /tmp/rl.bak
rm docker/deploy/config/rate_limit.yaml          # 触发 watcher 连续失败
sleep 95                                          # > 3 × 30s，应出现 ERROR 级日志
curl -s http://localhost:8008/health | python3 -m json.tool   # degraded + last_error
cp /tmp/rl.bak docker/deploy/config/rate_limit.yaml
cd docker/deploy && docker compose restart synapse            # 恢复挂载
```

---

## 5. 仍未处理（承接 P4 §8.2）

| # | 项 | 优先级 |
|---|---|---|
| 2 | 单文件 bind mount → 目录挂载（§5.6 的**根因**；本次只是让它可观测） | 中 |
| 3 | 重新标定并落实 `TESTING.md` 的 P95 阈值，或删除以免误导（§5.1） | 中 |
| 4 | 以 §4.3/§4.4 为基线做同机回归比对（注意 §4.5 的采样限制） | 中 |
| 5 | 采集 `performance_sliding_sync_benchmarks`（8 个，需服务/DB） | 低 |
| 6 | 在真实 CI 上确认 `sliding-sync-perf-gate` job 首跑结果 | 中 |

> 本次修的是**可观测性**，不是根因。单文件 bind mount 仍是 §5.6 记录的
> 部署脆弱性来源 —— 现在它至少会以 `degraded` + ERROR 日志的形式暴露出来。
