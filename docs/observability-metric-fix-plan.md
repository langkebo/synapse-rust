# 观测面指标名系统性错配 — 修复方案

> ## ⚠️ 本文档的核心前提已于 2026-09-22 被推翻，处方**不得执行**
>
> **失效点 1（致命）**：文中"缺少 Prometheus 标准 `_bucket{le=}`"这一根因**已不成立**。
> `43aa8f66`（2026-09-22）已让 `MetricsCollector::to_prometheus_format()` 输出原生分桶，
> 线上实测 `/metrics` 有 **222 行 `_bucket`**（如 `db_query_duration_ms_bucket{le="1"} 363`），
> `histogram_quantile` 正常返回数值（HTTP P99 9.083ms / DB P95 21.14ms）。
> 本文写作时点**早于**该提交，此后**没有回写**。
>
> **失效点 2（有害）**：§"方案选择：规则侧改均值（放弃分位数）"以及
> "所有 `histogram_quantile(...)` 替换为 `rate(*_sum)/rate(*_count)`"这条处方，
> 已导致 3 个面板把真分位降级成均值 —— 而 `legendFormat` 仍写着 `p95 延迟`，
> 即**用均值的数字冒充 P95**。这是比"No data"更危险的形态：
> 面板不会报错，只会显示一个自信的错数字。
>
> **失效点 3（诊断不完整）**：文中"Grafana 面板命中率 0/22 ⇒ 仪表板永久空白"的归因是错的。
> 第一因是 **provisioning 整体失败**：7 个面板文件都用 Grafana API 导出信封
> `{"dashboard": {...}, "meta": {...}}`，而 `type: file` 的 provisioning 读不了 ⇒
> 日志 `error="Dashboard title cannot be empty"`，**7/7 一个都没加载**。
> 只改指标名**不可能**让任何面板亮起来。
>
> **失效点 4（名字仍有错）**：下方"真实指标命名空间"表里的 `pool_utilization_percent`、
> `federation_signature_verifications_total`、`turn_requests_total{method="allocate"}`、
> `coturn_requests_total` **都不存在**（实测：真实名是 `pool_utilization`、
> `federation_signature_verifications`（**无 `_total`**）、`turn_total_allocations`）。
>
> **现在的权威口径**：现存问题与待办见 `docs/audit/GATE_INTEGRITY_FOLLOWUP_2026-09-19.md`；
> 本轮（2026-09-22）的逐条核实细节见 `docs/archive/GATE_INTEGRITY_FOLLOWUP_2026-09-19_LOG.md` §14.19
> （面板/provisioning 修复 + 计量对照表 + 新门禁
> `scripts/ci/check_dashboard_metrics.py`）。本文件**保留为历史记录**，请勿据此改配置。

## 问题诊断

### 根因：自定义 MetricsCollector 的 Histogram 导出不符合 Prometheus 规范

后端 `synapse-common/src/metrics.rs` 的 `MetricsCollector::to_prometheus_format()` 将 Histogram 导出为：
```
{name}_count  (counter)
{name}_sum    (counter)
{name}_avg    (gauge)
```

但**缺少 Prometheus 标准 `_bucket{le="X"}` 系列**，导致所有 `histogram_quantile(..., rate(*_bucket[5m]))` 永久无数据。

### 量化取证结果

| 问题 | 数量 | 影响 |
|------|------|------|
| App 侧 duration 指标形态 | 全部为 `_ms_count` / `_ms_sum` / `_avg`，零 `_bucket` | — |
| 集群内 `_bucket` 序列 | 13 个，全部属于 prometheus/alertmanager 自监控 | — |
| recording rule 无输出 | 6 条（含 HTTP duration、DB p95、megolm p95） | 仪表板数据缺失 |
| 告警永不触发 | 3 条（"纸面告警"） | 故障无感知 |
| Grafana 面板命中率 | 0/22（引用的 `synapse_*`/`coturn_*` 名无一存在） | 仪表板永久空白 |
| `instance:disk_usage:percent` 根因 | `node_filesystem_avail_bytes / size` 未做 by(...) 聚合 | 指标计算错误 |

### 真实指标命名空间

实际存在的指标名不是 `synapse_*` / `coturn_*`（方向对，但下表右列**多数也是错的**，
2026-09-22 逐条实测订正过一次）：

| 本文原写（部分错误） | ✅ 实测真实名 |
|---|---|
| `pool_utilization_percent` | `pool_utilization`（**0–1 比率**，当 percent 用要先 `×100`） |
| `auth_failures_total{type="failure"}` | ✅ `auth_failures_total`（注意与 `auth_attempts_total` 的 `type` 标签**不同**，相除前必须 `sum()`） |
| `rate_limit_requests_rejected_total` | ✅（此条正确） |
| `federation_signature_verifications_total` | `federation_signature_verifications`（**没有 `_total` 后缀**） |
| `turn_requests_total{method="allocate"}` | `turn_total_allocations`（**语序相反**；且不存在 `turn_active_connections`） |

---

## ~~方案选择：规则侧改均值（放弃分位数）~~ → ❌ **已撤销，且已造成损害**

**本方案已废弃。** 它正是把 3 个面板的真分位降级成"均值冒充 P95"的来源。
四条理由逐条复核，三条被证伪：

- ~~后端代码改动风险大（需 rebuild + restart，当前 binary 已是 stale 状态）~~
  → **已做**：`43aa8f66` 改了渲染器、本轮又重建了镜像（编译 8m34s），
  `_bucket` 已在线上输出。所谓"风险大"没有兑现。
- ~~`MetricsCollector` 存储了完整 `Vec<f64>`，理论上可以输出 bucket，但需要改动
  `to_prometheus_format()` + 重新编译 + 重启容器~~ → **已做**，同上。
- ~~均值已足够反映趋势，分位数在运维监控中非刚需~~
  → **被推翻**：告警阈值本来就按分位数设计（`DatabaseQueryDurationHigh` /
  `HTTPRequestDurationHigh` / `E2EESessionKeyReadSlow`），均值会把长尾抹平，
  使这些告警对"少数慢请求"完全失明。
- "快速见效，不影响现有监控体系" → **后半句是错的**：它把分位口径换成了均值口径，
  属于**降级**，且因 `legendFormat` 未同步而变成误导。

---

## 修复清单

### 1. Recording Rules 修正

所有 `histogram_quantile(..., rate(*_bucket[5m]))` 替换为 `rate(*_sum[5m]) / rate(*_count[5m])`。

#### 1.1 HTTP Duration 均值
```yaml
# 旧（无数据）
- record: job:http_request_duration:avg5m
  expr: histogram_quantile(0.5, sum(rate(http_request_duration_ms_bucket[5m])) by (le, job))

# 新（均值）
- record: job:http_request_duration:mean5m
  expr: rate(http_request_duration_ms_sum[5m]) / rate(http_request_duration_ms_count[5m])
```

#### 1.2 DB Query Duration 均值
```yaml
# 旧
- record: job:db_query_duration:p95_5m
  expr: histogram_quantile(0.95, sum(rate(db_query_duration_ms_bucket[5m])) by (le, job))

# 新
- record: job:db_query_duration:mean5m
  expr: rate(db_query_duration_ms_sum[5m]) / rate(db_query_duration_ms_count[5m])
```

#### 1.3 Megolm Duration 均值
```yaml
# 旧
- record: job:megolm_duration:p95_5m
  expr: histogram_quantile(0.95, sum(rate(megolm_duration_ms_bucket[5m])) by (le, job))

# 新
- record: job:megolm_duration:mean5m
  expr: rate(megolm_duration_ms_sum[5m]) / rate(megolm_duration_ms_count[5m])
```

### 2. Alerting Rules 修复

#### 2.1 永不触发的告警
将 `histogram_quantile` 条件改为 `rate(*_sum) / rate(*_count)` 的均值比较：

```yaml
# 旧（纸面告警）
- alert: HttpLatencyP95High
  expr: histogram_quantile(0.95, sum(rate(http_request_duration_ms_bucket[5m])) by (le, job)) > 5000

# 新（均值告警）
- alert: HttpLatencyMeanHigh
  expr: rate(http_request_duration_ms_sum[5m]) / rate(http_request_duration_ms_count[5m]) > 5000
```

### 3. Grafana Dashboard 指标名映射

Grafana 面板需要替换所有引用为真实存在的指标名。

#### 3.1 通用映射表

| 面板查询（错误） | 修正后（真实） |
|----------------|---------------|
| `synapse_http_request_duration_ms_bucket` | `rate(http_request_duration_ms_sum[5m]) / rate(http_request_duration_ms_count[5m])` |
| `synapse_db_query_duration_ms_p95` | `rate(db_query_duration_ms_sum[5m]) / rate(db_query_duration_ms_count[5m])` |
| `coturn_requests_total` | `turn_requests_total` |
| `synapse_auth_failures` | `auth_failures_total` |
| `synapse_rate_limited` | `rate_limit_requests_rejected_total` |
| `synapse_federation_signature` | `federation_signature_verifications_total` |

#### 3.2 `instance:disk_usage:percent` 修复
```yaml
# 旧（多对多匹配错误）
instance:disk_usage:percent = node_filesystem_avail_bytes / node_filesystem_size_bytes

# 新（正确的 by(node) 聚合）
instance:disk_usage:percent = 
  (1 - avg by (instance) (node_filesystem_avail_bytes) / avg by (instance) (node_filesystem_size_bytes)) * 100
```

---

## 实施优先级

| 优先级 | 修复项 | 影响 | 工作量 |
|--------|--------|------|--------|
| P0 | Recording rules 修正 | 6 条 rule 恢复输出 | 10 min |
| P0 | Alerting rules 修复 | 3 条告警恢复触发 | 10 min |
| P1 | Grafana dashboard 指标名映射 | 22 个面板恢复数据 | 1-2h |
| P1 | `instance:disk_usage:percent` 修复 | 磁盘指标正确计算 | 5 min |
| P2 | 后端 Histogram 改原生 bucket | 完整分位数支持 | 需 rebuild |

---

## 长期方案（P2）

如果后续需要完整的分位数支持，需将 `MetricsCollector::to_prometheus_format()` 改为输出 Prometheus 原生 histogram 格式：

```rust
// 在 to_prometheus_format() 中添加 bucket 输出
const HISTOGRAM_BUCKETS: &[f64] = &[0.1, 0.5, 1.0, 5.0, 10.0, 50.0, 100.0];

for bucket in HISTOGRAM_BUCKETS {
    let le = format!("{bucket}");
    output.push_str(&format!("{name}_bucket{{le=\"{le}\"}} {count}\n"));
}
output.push_str(&format!("{name}_bucket{{le=\"+Inf\"}} {total_count}\n"));
```

这需要：
1. 修改 `synapse-common/src/metrics.rs` 的 `to_prometheus_format()` 方法
2. 重新编译 `synapse-rust` binary（需解决 OOM 问题）
3. 重启 `synapse-app` 容器
4. 恢复 recording rules 为 `histogram_quantile` 格式

---

## 验证方法

```bash
# 1. 检查 Prometheus targets 是否全部 UP
curl -s http://127.0.0.1:9092/api/v1/targets | python3 -c "import json,sys; [print(t['scrapePool'], t['health']) for t in json.loads(sys.stdin.read())['data']['activeTargets']]"

# 2. 检查 recording rules 是否有输出
curl -s 'http://127.0.0.1:9092/api/v1/query?query=job:http_request_duration:mean5m'

# 3. 检查告警是否触发
curl -s 'http://127.0.0.1:9092/api/v1/alerts'
```
