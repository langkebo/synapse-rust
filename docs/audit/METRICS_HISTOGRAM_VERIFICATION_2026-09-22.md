# Metrics 原生分桶改造 —— 部署后验证报告

- 日期：2026-09-22
- 分支：`feat/metrics-native-histogram-buckets`
- 提交：`43aa8f66 fix(metrics): Histogram 改输出 Prometheus 原生分桶，复活 histogram_quantile`
- PR：#7（OPEN，未合并）
- 镜像：`synapse-rust:distroless`，ID `4f6c63d4…`（重启前 `6f31191e…`）
- 验证对象：`MetricsCollector::to_prometheus_format()`（`synapse-common/src/metrics.rs`）

---

## 1. 一句话结论

**渲染器缺陷已修复、`_bucket` 已按预期产出（字面验收通过）；但"告警复活"这一目标尚未达成** ——
HTTP / DB / 联邦三类 duration 直方图的埋点 `ServerMetrics::record_*` 在生产代码中**零调用点**，
`_count` 恒为 0，`histogram_quantile` 仍然取不到样本。

| 验收项 | 结果 |
| --- | --- |
| `/metrics` 可抓取 | ✅ `http=200`，29250 字节 |
| `_bucket` 存在 | ✅ **0 → 210 行** |
| `# TYPE … histogram` | ✅ 15 个族，每族 14 桶（13 有限 + `+Inf`） |
| 累积语义正确 | ✅ 在活数据上验证（见 §3） |
| `histogram_quantile` 能算分位数 | ❌ 目标族 `_count = 0`，仍为空向量 |

---

## 2. 改动内容（`synapse-common/src/metrics.rs`，+393 / −40）

1. 新增分桶表 `HISTOGRAM_BUCKETS_MS`（13 档：`1,2.5,5,…,10000`）与
   `HISTOGRAM_BUCKETS_SECONDS`（11 档：`0.005…10`），均**严格升序**；
   `histogram_buckets_for(name)` 按名字后缀 `_seconds` 选表，否则用毫秒表。
2. 新增 `Histogram::snapshot(&self, bounds)`：**单次加锁**取计数、求和并还原**累积**桶，
   `NaN` / `+Inf` 归入 `+Inf` 桶。
3. 新增 `normalize_negative_zero()`：修掉空直方图 `_sum` 被渲染成 `-0` 的问题
   （`f64` 的 `Sum` 以 `-0.0` 为加法单位元）。
4. 新增 `push_label_value()` 转义 `\` `"` 换行；`render_labels()` 对标签键排序、
   **剔除用户自带的 `le`**、并把分桶 `le` 固定放最后。
5. 重写 `to_prometheus_format()`：按指标族名排序输出，产出
   `# TYPE <base> histogram` + `<base>_bucket{…,le="…"}` + `le="+Inf"` + `_sum` + `_count`。
6. 新增 12 条单测；并更新既有 `test_to_prometheus_format_includes_histogram_count_and_sum`
   的期望（由 `# TYPE latency_count counter` 改为 `# TYPE latency histogram`）。

**测试**：`cargo test -p synapse-common --features test-utils` → **913 passed / 0 failed**
（须先导出 `DATABASE_URL` / `TEST_DATABASE_URL` / `TEST_DB_TEMPLATE_SCHEMA=test_template_ci`，
否则 `test_isolation` / `test_schema_guard` 因环境缺失而红）；clippy `-D warnings` 干净。

---

## 3. 线上取证

### 3.1 结构正确性

```
http_request_duration_ms_bucket{unit="ms",le="1"} 0
…
http_request_duration_ms_bucket{unit="ms",le="+Inf"} 0
http_request_duration_ms_sum{unit="ms"} 0
http_request_duration_ms_count{unit="ms"} 0
```

标签在前、`le` 在最后；15 族 × 14 桶 = 210 行，与 `curl | grep -c '_bucket'` 一致。

### 3.2 累积语义（唯一有真实数据的族）

```
retention_lifecycle_cycle_duration_ms_count 1
retention_lifecycle_cycle_duration_ms_sum   4.0
```

观测值 4 ms 落在 `le="5"` 起往上的累计桶内，`+Inf == _count == 1` —— **累积逻辑在活数据上正确**。

---

## 4. 🔴 新发现：链路断在埋点，不在渲染器

### 4.1 决定性实验

| 步骤 | 观测 |
| --- | --- |
| 对 `/_matrix/client/versions` 连打 20 次 | 全部 `200` |
| 再看 `http_requests_total` | **仍为 0** |
| 同期 `rate_limit_requests_total` | **7 → 30**（旁证：请求确实进了应用） |
| `http_request_duration_ms_count` | `0`（未增长） |

排除了"服务空闲所以没流量"这一替代解释。

### 4.2 源码侧确认

| 方法 | 定义 | 生产调用点 |
| --- | --- | --- |
| `record_http_request` | `server_metrics.rs:395` | **无**（仅 `#[cfg(test)]` :753/:882） |
| `record_db_query` | `server_metrics.rs:348` | **无**（仅 `#[cfg(test)]` :847） |
| `record_federation_request` | `server_metrics.rs:373` | **无**（仅 `#[cfg(test)]` :726） |

`ServerMetrics` 在生产代码里的真实调用点只有 **3 处**：

1. `synapse-e2ee/src/vodozemac_megolm.rs`
   —— `record_megolm_share` ×3、`record_megolm_share_cache_error` ×1、`record_megolm_session_key_read` ×3
2. `src/server/mod.rs:562-583`
   —— dehydrated_device_cleanup 系列，**直接** `.inc()/.observe()`，未走 `record_*`
3. `synapse-services/src/container.rs:384` —— `event_notifier_subscriber_failures_total`

其余 `record_*`（`record_message_send`、`record_sync_request`、`record_auth_attempt`、
`record_room_operation`、`record_csrf_validation`、`record_federation_signature_verification` …）
**全部为 test-only**。

### 4.3 影响面

- 本次改动真正"复活"的只有 `retention_lifecycle_cycle_duration_ms` 一族。
- rules 里针对 http / db / federation 的 `histogram_quantile(rate(*_bucket[5m]))`
  **依然永久无样本**（`_count = 0` → `rate = 0` → 无序列），告警仍不触发。
- ⇒ **`_bucket` 存在 ≠ `histogram_quantile` 有数据**。

---

## 5. 与用户前提不符的两点（澄清）

1. **"9090 端口仍 500" 复现不出来**：连打 3 次全 `200`（19295 字节）；应用**全量**日志中
   `500` 命中 0 次。`Extension(prometheus_auth_token)` 已在 `src/server/mod.rs:820` 注册且该文件已提交；
   `.env` 未设 `PROMETHEUS_AUTH_TOKEN` ⇒ 中间件放行。**不存在"已写但未部署的 Extension fix"**。
2. **"P0 recording rules 修复"** —— 规则文件里 `histogram_quantile` **本来就还在**（5 + 3 处），
   并非"已修复"状态。

---

## 6. 并发会话改动中待处理的缺陷（非本次改动引入）

同一工作区另有会话并发编辑（`git status` 中 7 个文件 + 3 个未跟踪文档），引入如下问题：

| 文件 | 问题 | 建议 |
| --- | --- | --- |
| `prometheus/alerting-rules.yml` | `DatabaseQueryDurationHigh` 阈值改成 `> 0.5`，但 `db_query_duration_ms` 单位是**毫秒**（原 `> 500`）⇒ 严了 1000 倍，会长期误报。同批另两条保留 `> 2000` / `> 100` 的 ms 量级，自相矛盾 | 改回 `> 500` |
| `prometheus/recording-rules.yml` | `mean5m` / `mean_p95_5m` / `mean_p99_5m` 三条 HTTP 录制规则**表达式完全相同**（同值异名） | 保留 `histogram_quantile` 取真分位数 |
| 同上 | `instance:disk_usage:percent` 改为 `sum by(instance)(…)` —— 把**多个文件系统的百分比相加**（3 盘各 60% → 180%），`system-overview.json:68` 引用它 | 改 `max by(instance)` |
| 同上 | 将 `histogram_quantile` **降级为均值** `rate(_sum)/rate(_count)`，理由注释为"后端不输出 `_bucket`" | 该前提已被本次改动消除；均值丢尾部，而尾部正是告警要抓的 |

---

## 7. 建议的后续动作（**未执行，待授权**）

优先级从高到低：

1. **补 HTTP 埋点**：加 axum 全局中间件
   （`middleware::from_fn_with_state` + `Extension<Arc<ServerMetrics>>`），
   在响应后调用 `record_http_request(elapsed_ms, status.is_success())`。
2. **补 DB / 联邦埋点**：DB 侧在 sqlx 查询处包一层计时调 `record_db_query`；
   联邦侧在 outbound federation client 处调 `record_federation_request`。
3. **补"埋点可达性"门禁**：静态扫描每个 `record_*` 至少有一个非 `#[cfg(test)]` 调用点
   （仿仓库既有 `check_get_raw_usage.py`），防止"指标注册了却永不调用"再次发生。
4. 修正 §6 中 4 项规则/面板缺陷。

---

## 附：复现命令

```bash
# 抓取并统计
curl -s http://localhost:9090/metrics -o /tmp/metrics_now.txt
grep -c '_bucket' /tmp/metrics_now.txt                            # 210
grep -cE '^# TYPE .* histogram$' /tmp/metrics_now.txt             # 15

# 埋点可达性实验
for i in $(seq 1 20); do curl -s -o /dev/null -w "%{http_code} " \
  http://localhost:8008/_matrix/client/versions; done; echo
curl -s http://localhost:9090/metrics | grep -E '^(http_requests_total|rate_limit_requests_total) '

# 源码侧可达性
rg -n '\.record_[a-z_]+\(|server_metrics\.' --type rust -g '!tests/**'
rg -n 'pub fn record_|#\[cfg\(test\)\]' synapse-common/src/server_metrics.rs
```
