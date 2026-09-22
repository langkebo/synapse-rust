# Synapse-Rust 观测面建设指南

> **权威来源**：现存问题与待办见 `docs/audit/GATE_INTEGRITY_FOLLOWUP_2026-09-19.md`；
> 本轮（2026-09-22）的逐条核实细节见 `docs/archive/GATE_INTEGRITY_FOLLOWUP_2026-09-19_LOG.md §14.19`。
>
> 本文档是**建设性参考**（provisioning 陷阱、PromQL 向量匹配、比率 vs 百分位、指标名对照），
> 不包含任何"待修复处方"——所有历史缺陷已在 2026-09-22 `43aa8f66` 及其后续提交中闭环。

---

## 1. Provisioning 陷阱

### 1.1 Grafana `type: file` 不能读 API 导出信封

Grafana 面板从未加载的**第一因**是 provisioning 整体失败，与指标名无关：

- 7 个面板文件都用 Grafana API 导出信封 `{"dashboard": {...}, "meta": {...}}`
- `type: file` 的 provisioning 读不了该信封 ⇒ 日志 `error="Dashboard title cannot be empty"`，7/7 一个都没加载
- **修复**：面板 JSON 必须是裸仪表板对象（`{"dashboard": {...}}`），去掉信封包装
- **门禁**：`scripts/ci/check_dashboard_metrics.py` 校验 7 个面板的指标名可达性

### 1.2 面板 JSON 排版不统一

7 个面板里 2 个是单行 JSON、5 个是格式化过的。已统一为 `indent=2`（纯格式，无逻辑变更）。

---

## 2. PromQL 向量匹配陷阱

### 2.1 标签集不同 ⇒ 直接相除返回空

`auth_success_total{type="success"}` 与 `auth_attempts_total{type="attempt"}` 标签集不同，
直接相除**静默返回空**（不是 0）。必须先 `sum()` 聚合到同一标签集：

```promql
sum(rate(auth_success_total{type="success"}[5m]))
/
sum(rate(auth_attempts_total{type="attempt"}[5m]))
```

### 2.2 按 `:` 分词的假阳性

PromQL 分词器把 `instance:x:y` 当作指标名处理；`[5m]` 会被分词成 `m`。
复核录制规则/告警产物时**必须用 instant query**，避免按 `:` 分词的假阳性。

### 2.3 陈旧序列

`/api/v1/label/__name__/values` 会返回陈旧序列（TSDB 索引含已不再产生的历史名，
如废弃规则产物 `job:http_request_duration:mean_p95_5m`）。
复核时必须用 instant query 而非 label values API。

---

## 3. 比率 vs 百分位

### 3.1 0–1 比率 vs 百分位

部分指标是 0–1 比率（如 `pool_utilization`、`instance:cache_hit_ratio:ratio5m`），
Grafana 面板 unit=percent 时需 **×100** 显示。
PromQL 中 `pool_utilization >= 0.9` 对应"池利用率 > 90%"。

### 3.2 `_bucket` 存在但零观测 ⇒ `histogram_quantile` 返回 `NaN`

当直方图没有数据点时，`histogram_quantile` 返回 `NaN`（不是空向量）。
告警侧安全（`NaN > 阈值` 为 false），但下游算术会传播 NaN。

### 3.3 后端渲染器

后端使用自研 `MetricsCollector::to_prometheus_format()` 渲染 Prometheus 文本。
`turn_*` 来自 coturn exporter、`prometheus_*`/`alertmanager_*` 来自其自身，
不经过本渲染器。

---

## 4. 真实指标名对照表

> ⚠️ 常见错误名已在下表标出。复核时以"实测真实名"为准。

| 常见错误名 | ✅ 实测真实名 | 备注 |
|---|---|---|
| `pool_utilization_percent` | `pool_utilization` | **0–1 比率**，当 percent 用要先 `×100` |
| `federation_signature_verifications_total` | `federation_signature_verifications` | **无 `_total` 后缀** |
| `turn_requests_total{method="allocate"}` | `turn_total_allocations` | 语序相反；不存在 `turn_active_connections` |
| `coturn_requests_total` | 不存在 | coturn 使用独立的 exporter 指标 |
| `auth_requests_total` | `auth_attempts_total`（`type="attempt"`） | 与 `auth_success_total` 标签集不同 |
| `database_*` | `db_query_duration_ms_*` | 指标前缀是 `db_query_duration_ms`，非 `database_` |
| `turn_total_allocations` | `turn_total_allocations` | 语序是 `turn_total_allocations` 非 `turn_requests_total` |
| `auth_requests_total` | `auth_attempts_total` | 与 `auth_success_total` 的 `type` 标签不同 |

---

## 5. 已闭环的历史缺陷（仅供审计追溯）

| 提交 | 修复内容 | 证据 |
|---|---|---|
| `43aa8f66` | 渲染器原生分桶（`*_seconds` 秒桶 11 个 / 其余毫秒桶 13 个），复活 `histogram_quantile` | `/metrics` 实测 222 行 `_bucket` |
| `3f3178ee` / `f7226a62` | Grafana 面板 JSON 去掉信封包装，`type: file` provisioning 可读 | 7/7 面板加载 |
| `8edf16c0` | `update_pool_metrics` 死埋点接通，`ScheduledTasks::database.pool()` 30s 周期采集 | 基线 `metric_instrumentation_baseline` 已更新 |

---

## 6. 观测面文件索引

| 文件 | 用途 |
|---|---|
| `docker/deploy/prometheus/` | Prometheus 配置 + recording/alerting rules |
| `docker/deploy/grafana/dashboards/` | 7 个裸仪表板 JSON |
| `scripts/ci/check_dashboard_metrics.py` | 面板指标名可达性门禁 |
| `scripts/ci/check_metric_instrumentation.py` | 埋点生产调用点可达性门禁 |
| `scripts/ci/metric_instrumentation_baseline` | 未接线 `ServerMetrics` 方法基线 |
| `docs/archive/GATE_INTEGRITY_FOLLOWUP_2026-09-19_LOG.md §14.19` | 面板/provisioning 修复逐条核实 |
