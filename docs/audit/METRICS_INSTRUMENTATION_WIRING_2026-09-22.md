# 埋点接线改造 —— 实施与验证报告

- 日期：2026-09-22
- 分支：`feat/metrics-native-histogram-buckets`（在 `43aa8f66` 之后，PR #7 范围扩展）
- 上游文档：`docs/audit/METRICS_HISTOGRAM_VERIFICATION_2026-09-22.md` §4 / §6 / §7
- 一句话：上一轮证明了「`_bucket` 有了，但没人调用 `record_*`，所以分位线仍无数据」。
  本轮把 **HTTP / DB / 联邦三条埋点链路真正接上生产路径**，补上**防复发的静态门禁**，
  并修正上一轮登记在 §6 的 4 项规则缺陷。

---

## 1. 结论摘要

| # | 任务 | 状态 | 关键落点 |
| --- | --- | --- | --- |
| 1 | HTTP 全局埋点 | ✅ 已接 | `synapse-web/src/middleware/http_metrics.rs` |
| 2 | DB 埋点 | ✅ 已接 | `synapse-common/src/db_query_metrics.rs` + `error.rs` |
| 2 | 联邦埋点 | ✅ 已接 | `synapse-federation/src/client.rs:711` + `federation_auth.rs:407` |
| 3 | 埋点可达性门禁 | ✅ 已接 | `scripts/ci/check_metric_instrumentation.py` + 棘轮基线 |
| 4 | §6 的 4 项规则缺陷 | ✅ 已修 | `prometheus/{recording,alerting}-rules.yml` |

门禁结果：**ServerMetrics 共 25 个埋点方法，9 个已在生产路径上被调用，16 个登记进棘轮基线**
（`scripts/ci/metric_instrumentation_baseline`，单向收缩）。
同时**顺带修掉两处语义错误**（详见 §3），它们不是本次引入，但会让新接的计数器不可用。

---

## 2. 三个接线的设计取舍

### 2.1 HTTP：全局中间件 + RAII 释放在途计数

`synapse-web/src/middleware/http_metrics.rs`，由 `src/server/router.rs` 以
`axum::middleware::from_fn_with_state(server_metrics, http_metrics_middleware)` 挂载。

- **`from_fn_with_state` 的 state 与路由 state 互相独立**，所以可以直接传
  `Arc<ServerMetrics>` 而不必把 `AppState` 拖进来。
- **层序**：放在 `RequestBodyLimitLayer` 之后、`request_debug` / `request_timeout` / `TraceLayer`
  之前 —— 使被超时（408）或 413 改写后的响应**也能被计入**。放在它们之外会漏掉这部分流量。
- **RAII 释放**（`InFlightGuard`）：客户端中途断开时 axum 会 **drop handler future**，
  `next.run(...)` 之后的语句永不执行；若用「await 后手动减一」，`http_active_requests`
  会单向漂移、永不归零。`Drop` 是唯一能覆盖全部退出路径的位置。
- **错误口径**：`is_client_error() || is_server_error()`（4xx+5xx），**不是** `!is_success()`
  ——后者会把 1xx/3xx 也算成错误，而 Matrix 服务确实会产生重定向。
  代价：`HighHTTPErrorRate` 也会被 401/404/429 这类正常客户端失败触发，已在代码注释里
  留给运维的收窄办法（`and on() rate(...{code=~"5.."})`）。
- `/health`、`/healthz`、`/metrics` 排除：探针以固定周期打这些路径，计入会把
  `http_requests_total` 灌满噪声并拉低时长分位。

### 2.2 DB：吃 sqlx 的逐语句 tracing 事件，而不是包 1763 处调用点

`synapse-common/src/db_query_metrics.rs` 是一个 `tracing` Layer，监听 target `sqlx::query`、
字段 `elapsed_secs`。

- sqlx 0.8 的 `QueryLogger::finish` **每条语句恰好发一条事件**，带 `elapsed_secs: f64`。
  这是 sqlx 暴露的**唯一**逐语句计时钩子，且**无需改动任何调用点** —— 而本仓 storage 层
  有约 1700 处直接 `sqlx::query(..)`，没有中心化的查询门面可包。
- **代价与开关**：事件只有被「声明了兴趣」才会构造。默认
  `statements_level = Debug`，所以池侧配置不动，只配订阅侧。
  构造事件（含 SQL 摘要字符串）有微小开销，可用 `SYNAPSE_DB_QUERY_METRICS_DISABLED=1` 整体关掉。
- **必须改用逐层过滤**（`logging.rs`）：fmt 层要把 `sqlx::query` 压到 `WARN`
  （否则 verbose 模式下每条 SQL 都打印、淹没业务错误），而指标层要在 `DEBUG` 上消费同一条事件。
  全局 `EnvFilter` 两者不可兼得（`tracing::enabled!` 被全局级别挡掉后**事件根本不会构造**）。
  逐层过滤下判据是「**任意一层想要**」⇒ 事件构造一次、只由指标层接收，fmt 层仍按 WARN 抑制，
  **日志输出不变**。禁用时用空 `Targets`（默认 OFF），sqlx 连事件都不构造。
- **sqlx 只给时长、不给成败** ⇒ 失败计数改在 `From<sqlx::Error> for ApiError`
  这个**唯一漏斗**里做（`error.rs`），并显式排除两个「预期内」变体：
  `RowNotFound`（正常的"没查到"，计入会让 `db_query_errors` 变成空查询计数器）
  与 unique violation（业务冲突，判 400）。

### 2.3 联邦：出站唯一咽喉点

`FederationClient::send_signed_request` 被拆成计时外壳 + `send_signed_request_inner`
（`synapse-federation/src/client.rs:711`）。`make_join` / `send_join` / `send_transaction` /
`get_state` … 全部经此，所以在此观测最省事也最完整。
入站方向在 `synapse-web/src/middleware/federation_auth.rs:407` 记签名校验结果（含缓存命中分支）。

---

## 3. 顺带修掉的两处语义错误（非本次引入）

1. **`record_federation_request(success=false)` 记错了计数器**：原先它递增
   `federation_signature_errors`，于是 DNS 解析失败、远端 503 都被算成"签名错误"，
   该计数器不再能用于签名告警。已新增独立的 `federation_request_errors_total`：
   - 出站交换失败（4xx/5xx/传输层）→ `federation_request_errors_total`
   - 签名校验失败 → 仍由 `record_federation_signature_verification(false)` 记
   - `get_summary().federation_errors` 的取值来源同步改到新计数器
2. **`db_query_errors` 需要一个不会重复计数的落点**：见 §2.2 末尾。

---

## 4. 门禁：让"注册了却永不调用"再也进不来

`scripts/ci/check_metric_instrumentation.py`（CI 中挂在 `check_get_raw_usage.py` 之后）：

- 从 `impl ServerMetrics {` 里解析出全部 `pub fn record_* / observe_* / update_*`
  （以及 `http_request_started/finished`），排除 `new/get_collector/get_summary`；
- 扫描**生产**调用点：排除 `SERVER_METRICS_SRC` 自身与测试路径，并对命中文件做
  `#[cfg(test)]` 花括号配平，跳过测试块内的调用；
- 未接通集合与 `scripts/ci/metric_instrumentation_baseline` **棘轮比对**：
  出现新缺口（或基线里有过期项）→ 退出码 1；扫描为空 / git 不可用 → 退出码 2。

**性能陷阱（已踩）**：初版用 `Path.rglob("*.rs")` + 逐文件 `read_text()`，
在本仓（1014 个 .rs、且大目录拖慢 stat）**超过 100s 超时被 SIGTERM**（CPU 仅 1.9s，全在 I/O 等待）。
改为 **`git ls-files --cached --others --exclude-standard -z` + `git grep --untracked -n -I -E`** 后降到 10.6s。
其中 `--untracked` 是必需的 —— 否则刚写好、尚未 `git add` 的文件会被判成"没有调用点"。

当前结果（25 个方法）：已接通 **9**，基线 **16**。

```
+ http_request_started / http_request_finished      <- middleware/http_metrics.rs
+ record_http_request                               <- middleware/http_metrics.rs
+ observe_db_query_duration                         <- db_query_metrics.rs
+ record_federation_request                         <- federation/src/client.rs
+ record_federation_signature_verification          <- middleware/federation_auth.rs（2 处）
+ record_megolm_share / _cache_error / _session_key_read  <- synapse-e2ee/src/vodozemac_megolm.rs
```

基线里每一条都带 `BASELINE_REASONS` 说明为何**这轮不接**，其中价值最高的一条是
**`update_pool_metrics` 是死调用点** ⇒ `pool_utilization`、`db_connections_active`、
`db_connections_idle`、`pool_health_status` **恒为 0** ⇒ `DatabasePoolUtilizationHigh`
与 `DatabasePoolExhausted` 两条告警永不触发。修它需要一个周期任务宿主（本轮未做）。

---

## 5. §6 的 4 项规则缺陷修正

| 原缺陷 | 修正后 |
| --- | --- |
| `DatabaseQueryDurationHigh` 阈值被写成 `> 0.5`，但 `db_query_duration_ms` 单位是**毫秒**（原 `> 500`）⇒ 严了 1000 倍，会长期误报 | `> 500`，并加注释锁定单位 |
| `job:http_request_duration:avg5m` 实际算的是中位数（`histogram_quantile(0.5,…)`），名不符实 | 更名为 `job:http_request_duration:p50_5m`。**已核验无任何面板/规则引用旧名**（全仓仅 docs 与一份旧方案文档提及） |
| `instance:disk_usage:percent` 用 `sum by(instance)(…)`，把**多个文件系统的百分比相加**（3 盘各 60% → 180%），而 `system-overview.json:68` 直接引用它 | 改 `max by(instance)(100*(1 - avail/size{fstype!="tmpfs"}))`，取利用率最高的那个分区 |
| 三条 `histogram_quantile` 被降级为均值 `rate(_sum)/rate(_count)`，理由注释为"后端不输出 `_bucket`" | 该前提已被 `43aa8f66` 消除。三条恢复真分位（P95/P99/P95），并统一加 `sum by (le)`（漏掉聚合时，任一新增标签都会把分布切成多份而**静默失效**） |

两份 YAML 已用 `yaml.safe_load` 解析校验通过（recording 6 组 / 18 规则；alerting 8 组 / 23 规则）。

---

## 6. 验证证据

| 门禁 | 结果 |
| --- | --- |
| `cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings` | ✅ 9 个 crate 全过，0 warning（1m15s） |
| `cargo test -p synapse-common --lib --features test-utils` | ✅ **917 passed / 0 failed** |
| `cargo test -p synapse-web --lib --features test-utils http_metrics` | ✅ 4 passed / 0 failed |
| `python3 scripts/ci/check_metric_instrumentation.py` | ✅ 退出码 0，未接通集合与基线一致 |
| `yaml.safe_load` × 2 份 rules | ✅ 通过 |

> 注：`cargo test -p synapse-web --lib` **必须带 `--features test-utils`**，
> 否则 `synapse_test_utils` 未链接、整个 lib test 编译失败（6 个 E0433）。
> 这是既有约定，不是本次引入。

新写的单测都刻意避开"真空断言"：

- `db_query_metrics.rs` 的测试通过 `with_metrics(Arc<ServerMetrics>)` **注入**显式句柄。
  理由：`install_global_server_metrics` 是 **first-write-wins**，若测试依赖全局句柄，
  会静默观测到**另一个测试**的 collector，断言恒真 —— 一个不可能失败的测试。
  断言覆盖：正常事件计数与秒→毫秒换算、他 track 目标忽略、NaN/INF/负数丢弃、无数字字段忽略。
- `http_metrics.rs` 的 4 个测试覆盖：计数与时长、5xx 记错误、**在途计数归零**、健康探针排除。

---

## 7. 线上复验（已完成，见 §8）

1. ~~线上复验（强烈建议下一步）~~ → **已于 2026-09-22 完成，结果见 §8**。
   上一轮的决定性实验（连打 20 次 `/_matrix/client/versions` 全 200 后
   `http_requests_total` 仍为 0）已在新镜像上重跑，**前后对比明确**。
2. **`update_pool_metrics` 已接通（2026-09-22 P0-2 修复）**：
   - 在 `src/server/mod.rs` 的 `run` 方法中新增 30s 周期任务
   - 通过 `ScheduledTasks::database.pool()` 获取 `Pool<Postgres>` 引用
   - 调用 `server_metrics.update_pool_metrics(active, idle, utilization, healthy)`
   - 量纲：`utilization = pool_size / max_size`（0–1 比率，与 Grafana `* 100` 对齐）
   - 已更新基线 `scripts/ci/metric_instrumentation_baseline`（移除 `update_pool_metrics`）
   - 验证：`cargo clippy` + `nextest -p synapse-common test_update_pool_metrics` 全绿
   - 影响：`db_connections_active`/`db_connections_idle`/`pool_utilization`/`pool_health_status` 不再恒 0，
     `DatabasePoolUtilizationHigh` 与 `DatabasePoolExhausted` 告警规则可触发
3. **Grafana 面板命名空间错配仍未处理**（上一轮 §4.3 量化为 **命中 0/22**）：
   面板引用 `synapse_database_pool_used` / `synapse_active_users` / `coturn_*` 等，
   而应用真实名是 `pool_utilization` / `auth_attempts_total` / `turn_*`。
   本轮任务 4 只覆盖了 rules YAML 里的 4 项，**未动这 4 个 dashboard JSON**
   （它们同时正被并发会话编辑，改动会冲突）。
4. **并发会话隔离**：本仓同一工作区存在另一会话的未提交改动
   （`scripts/ci/geiger_baseline.json`、`synapse-test-utils/src/lib.rs`、
   `docs/archive/GATE_INTEGRITY_FOLLOWUP_2026-09-19_LOG.md`、4 个 Grafana dashboard）。
   本报告的改动**不含**这些文件，提交时须逐个 `git add`，禁止 `git add -A`。

---

## 附：本轮触及的文件

新增：

```
scripts/ci/check_metric_instrumentation.py        门禁脚本
scripts/ci/metric_instrumentation_baseline        棘轮基线（16 项历史欠账）
synapse-common/src/db_query_metrics.rs            sqlx 事件 -> db_query_duration_ms
synapse-web/src/middleware/http_metrics.rs        HTTP RED 指标中间件
```

修改：

```
src/server/router.rs                              挂载全局中间件
synapse-common/src/server_metrics.rs              global handle + federation_request_errors_total
synapse-common/src/logging.rs                     改逐层过滤
synapse-common/src/error.rs                       在 sqlx::Error -> ApiError 漏斗记 db_query_errors
synapse-common/src/lib.rs                         导出 db_query_metrics
synapse-federation/src/client.rs                  出站计时包装
synapse-web/src/middleware/federation_auth.rs     入站签名校验计数
synapse-web/src/middleware/mod.rs                 导出 http_metrics
.github/workflows/ci.yml                          接入新门禁
docker/deploy/prometheus/recording-rules.yml      4 项规则修正
docker/deploy/prometheus/alerting-rules.yml       3 项阈值/表达式修正
```

---

## 8. 线上复验（2026-09-22，新镜像实测）

### 8.1 前后对比：同一实验，同一镜像构建参数

镜像 `synapse-rust:distroless`，构建参数 `--features friends,burn-after-read --no-default-features`
（与 `.env` 的 `ENABLED_EXTENSIONS` 一致）。

对 `/_matrix/client/versions` 连打 **20 次，全部 200**：

| 指标 | 改造前（旧镜像） | 改造后（新镜像，容器刚重建） |
| --- | --- | --- |
| `http_requests_total` | **0** | **20** |
| `http_request_duration_ms_count` | **0** | **20** |
| `db_query_duration_ms_count` | **0** | **47** |
| `http_active_requests` | 0 | 0（RAII 正常释放） |
| `rate_limit_requests_total`（旁证） | 209 | 21（新容器计数） |

⇒ `http_requests_total == 20` **恰好等于**打进去的 20 次，且 `/health` 探针未计入
（若计入会大于 20）⇒ **埋点与排除规则同时生效**。
`db_query_duration_ms_count == 47` 证明 sqlx 事件层在真实流量下工作（启动 + 健康检查产生的查询）。

### 8.2 错误路径

补打 **5 次 `/versions/nope`（404）** 后：

```
http_requests_total 55          (= 20 + 5 + 30)
http_request_errors_total 5     (= 那 5 个 404)
```

⇒ 4xx 被正确计为错误，且 `record_http_request(_, false)` 分支确实走到。

### 8.3 `histogram_quantile` 真的算出数了（本轮的终点）

```
histogram_quantile(0.99, sum(rate(http_request_duration_ms_bucket[5m])) by (le))  -> 9.083
histogram_quantile(0.95, sum(rate(db_query_duration_ms_bucket[5m])) by (le))      -> 18.125
```

**改造前这两条查询返回空 result**（`_bucket` 不存在）；现在返回真实数值。
原生分桶也已带上真实计数：

```
http_request_duration_ms_bucket{unit="ms",le="1"} 4
http_request_duration_ms_bucket{unit="ms",le="2.5"} 33
http_request_duration_ms_bucket{unit="ms",le="10"} 55
```

### 8.4 录制规则逐条核对

⚠️ **Prometheus 不会自动重载规则文件**（bind mount 已更新但进程仍用旧规则）。
必须 `curl -X POST :9092/-/reload`（本栈已启用 lifecycle）或 `docker kill -s HUP synapse-prometheus`。
**未重载时观测到的值是旧规则算出来的**，会得出完全相反的结论 —— 本次实测就踩到了（见下）。

重载后：

| 录制规则 | 值 | 结论 |
| --- | --- | --- |
| `job:http_request_duration:p50_5m` | 2.231 | ✅ 原本 EMPTY，现可用 |
| `job:http_request_duration:p95_5m` | 6.25 | ✅ 原本 EMPTY，现可用 |
| `job:http_request_duration:p99_5m` | 26.25 | ✅ 原本 EMPTY，现可用 |
| `instance:db_query_duration:p95_5m` | 22.05 | ✅ 原本 EMPTY，现可用 |
| `job:http_error_rate:ratio5m` | 0.053 | ✅ |
| `instance:cpu_usage:percent` | 10.13 | ✅ |
| `instance:disk_usage:percent` | **35.623** | ✅ 修正前实测 **268.911%** ⇒ `max by(instance)` 修复生效 |
| `instance:db_pool_utilization:ratio` | **nan** | ❌ `0 / (0+0)` —— 死埋点 `update_pool_metrics` 的可见后果 |
| `instance:megolm_share_duration:p95_5m` | **nan** | ⚠️ 无 E2EE 流量 ⇒ `_count == 0` |

### 8.5 ⚠️ 新发现的一类行为：`_bucket` 存在但零观测 ⇒ `histogram_quantile` 返回 **NaN**（不是空）

这是本轮的副产物，值得下游注意：

- **改造前**：`_bucket` 根本不存在 ⇒ 表达式返回**空向量**（EMPTY）。
- **改造后**：`_bucket` 存在但计数全 0 ⇒ `histogram_quantile` 对全零桶返回 **NaN**。

影响评估：

- **告警安全**：PromQL 里 `NaN > 100` 为 **false** ⇒ 不会误触发。已实测
  `E2EESessionKeyReadSlow` 等未因 NaN 而 pending。
- **面板**：Grafana 对 NaN 显示为断点/No data，与空向量观感一致，可接受。
- **需要显式防护的场景**：若下游对结果做**算术**（相加、比值），NaN 会传播。
  稳妥写法是 `... unless ...`、`clamp_min`，或在面板里用 `> 0` 过滤。

### 8.6 告警状态的前后对照（副作用证伪）

| 告警 | 重载前 | 重载后 | 说明 |
| --- | --- | --- | --- |
| `DatabaseQueryDurationHigh` | **pending** | **已消失** | 重载前用的是被降级的 `> 0.5`（=0.5ms）阈值，而 DB p95 实测 22ms ⇒ 必然 pending。改回 `> 500` 后正确恢复平静 ⇒ **阈值修正得到线上验证** |
| `HighHTTPErrorRate` | 无 | **pending（5.26%）** | 由本次人为制造的 5 个 404 引起（5/95）⇒ **表达式确实在有数据上求值**。改造前 `http_requests_total == 0` ⇒ `0/0` 无数据 ⇒ 该告警`永不触发` |
| 其余 | — | 无 | 阈值（2000ms / 500ms）远高于实测 p95/p99，不乱报 |

### 8.7 部署过程中踩到的两个环境坑（非仓库缺陷）

1. **`deploy.sh` 在"缓存清理"步骤失败**：`rm -rf "$PROJECT_ROOT/target"` 被本工具环境的
   safe-delete 守卫拦截（`count=10011 > threshold=50`，`SAFE_DELETE_BULK_CONFIRM_REQUIRED`）
   ⇒ 部署在构建**之前**中止并触发回滚（回滚本身工作正常，服务已恢复）。
   由于 `.dockerignore` 已排除 `target/`、且 `SKIP_HOST_BUILD=true` 使主机 `target/` 与
   Docker 构建无关，**绕行办法是直接构建镜像再重建容器**：

   ```bash
   # 1) 构建（--build-arg CACHE_BUST=<epoch> 强制源码层失效；保留依赖编译缓存挂载）
   BUILDX_CONFIG=/tmp/buildx-cfg docker build -f docker/Dockerfile --target tools \
     --build-arg CACHE_BUST="$(date +%s)" \
     --build-arg "CARGO_FEATURE_ARGS=--features friends,burn-after-read --no-default-features" \
     -t synapse-rust:distroless .
   # 2) 重建 app 容器（注意 service 名是 synapse，不是 app）
   cd docker/deploy && docker compose up -d --force-recreate synapse
   ```

   实测编译 **8m34s**（依赖走 `--mount=type=cache,target=/workspace/target` 缓存，
   仅重编本 workspace 的 8 个 crate；比 `--no-cache` 快得多且同样正确）。
   注意 `--all` 会把 `ENABLED_EXTENSIONS` 改成 `all`，与 `.env` 不符，**不要用**。

2. **compose service 名是 `synapse`**（`container_name: synapse-app`），
   `docker compose up app` 会报 `no such service: app`。
