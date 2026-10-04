# 全新部署验证报告 — 2026-09-21

> 目标：确认 `synapse-rust` 在「清缓存 → 停容器 → 删残留 → 重编译 → 重建镜像 → `./deploy.sh --all` 一键部署」全链路后可正常部署并稳定运行。
>
> 结论：**通过**。核心栈 4/4 healthy、监控栈 5/5 healthy、健康端点 200、启动日志洁净、DB 版本一致性 drift=0。
>
> 同时发现 **1 项未修复的遗留缺陷**（观测面指标名系统性错配，见 §6），不阻塞部署，但导致部分告警/面板永久无数据。

---

## 1. 交付物（main 分支，领先 origin/main 5 个提交）

| 提交 | 内容 |
| --- | --- |
| `9bbd2263` | fix(ci): rate_limit 敏感端点限流 + ci_test_scope grep 模式 |
| `298f74b8` | fix(deploy,services,gates): 修复 9 项部署/可观测性缺陷 + 监控栈 compose 化 + B' 守卫连带修复 |
| `4c0e6301` | **fix(deploy): 修复全新部署无法启动（megolm.key 未供给）+ 4 处 bash 多字节变量名崩溃** |
| `7cf54661` | fix(deploy): `ensure_app_data_keys` 收紧既有 megolm.key 权限至 0600 |
| `db1538b7` | fix(monitoring): 修复监控栈两个启动失败 + 保护 `/app/data` 落点 |

改动面：`57 files changed, 2893 insertions(+), 89 deletions(-)`。工作区 **clean**。

---

## 2. 本次修复的缺陷（现象 / 影响 / 验收）

### 2.1 P0 — 全新部署必崩：`megolm.key` 未供给

- **现象**：`synapse-app` 启动即 exit 133，崩溃循环，日志 `Failed to read key file /app/data/megolm.key`。
- **根因**：`docker-compose.yml` 将 `/app/data` 挂为**初始为空的命名卷** `synapse_data`；`.env` 配置了密钥路径，而 `KeyAtRest::load_plaintext` 是 **fail-closed**（读取失败 → 返回启动错误，不 panic、不自动生成）；distroless 镜像无 shell，无法自举供给。
- **影响范围**：所有全新机器 / 首次部署 100% 失败；已有部署（卷里已有 key）不受影响 —— 属于「只在新环境爆炸」的隐性缺陷。
- **修复**：
  1. compose 改为宿主机 bind mount：`- ./synapse-data:/app/data`，移除 `synapse_data` 命名卷（容器以 uid 1000:1000 运行，宿主侧目录需可读写）。
  2. `deploy.sh` 新增 `ensure_app_data_keys()`：目录创建 → 存在则 base64 解码校验必须为 32 字节 → 权限收紧至 0600 → 不存在或空文件则 `umask 077 && openssl rand -base64 32` 原子生成（`tmp + mv`）。
  3. 遗留命名卷守卫：若检测到 `synapse_synapse_data` 卷仍存在则 **fail-closed 中止**，并打印 `cp` 抢救 + `docker volume rm` 命令，避免静默生成新 key 导致历史加密数据不可解。
- **验收**：
  - 全新部署 23/23 步全绿，`synapse-app` 达 **Healthy**（修复前同一路径为 crash-loop）。
  - 5 个分支隔离测试全过：生成（45B→解码 32B / 0600）、幂等（重复执行 hash 不变）、非法 base64 fail-closed、4 字节 fail-closed、遗留卷守卫 fail-closed。
  - 生产态：`megolm.key` 45B、权限 `-rw-------`、解码 **32** 字节。

### 2.2 P1 — 监控栈两处启动失败

| 现象 | 根因 | 修复 |
| --- | --- | --- |
| `network synapse-monitoring_network declared as external, but could not be found` | `deploy.sh` 以 `-p synapse-monitoring` 启动监控栈，覆盖了 `COMPOSE_PROJECT_NAME`，而 compose 里网络名用 `${COMPOSE_PROJECT_NAME:-synapse}_network` 拼接 | 三处（两个 compose + `deploy.sh`）统一改用独立变量 `${SYNAPSE_NETWORK_NAME:-synapse_network}` |
| `Bind for 127.0.0.1:9090 failed: port is already allocated` | 监控栈复用了 `PROMETHEUS_PORT=9090`，而该端口正是 **应用自身** 的 metrics 端口（synapse-app 已 publish 9090） | 监控栈改用 `${PROMETHEUS_UI_PORT:-9092}`，并在 `.env.example` 中显式区分两者语义 |

- **验收**：监控栈 5/5 healthy；Prometheus `/api/v1/targets` 全部 `up`（synapse-rust → `synapse-app:9090`、coturn → `coturn:9641`、prometheus/alertmanager/node-exporter）。

### 2.3 P1 — bash 多字节变量名吞并（7 处）

- **现象**：脚本在**报错/告警分支**莫名失败，报 `KEY?: unbound variable (location: 某行)` —— 而报错点正是「本该打印诊断信息」的那一行，真实错误被完全吞掉。
- **根因**：UTF-8 locale 下 `$VAR）`/`$VAR，` 被 bash 解析为变量名 `VAR）`（变量名字符集被扩展吞入全角字符），`set -u` 下直接崩。
- **修复**：`$x` → `${x}` 显式定界。全仓扫描 `rg '\$[A-Za-z_][A-Za-z0-9_]*[^\x00-\x7F]' -g '*.sh'` 共命中 7 处（`deploy.sh` 4 + `scripts/` 3），全部修复。
- **验收**：同类扫描 0 命中；相关门禁测试 7/7 通过。

### 2.4 其他

- `ensure_hosts_entry` 的 BSD-grep 假阴性正则 → 改为 awk 按字段比较。
- `scripts/init_test_public_schema.sh` / `reset_database_v12.sh` / `tune_test_db.sh` 的表计数硬编码漂移修正。
- 敏感端点限流 + `ci_test_scope` grep 模式 bug。
- 新增 `docker/deploy/synapse-data/README.md` 说明 `/app/data` 用途与恢复流程；`.gitignore` 忽略该目录内容但保留 README。

---

## 3. 部署执行证据

- 命令：`BUILDX_CONFIG=/tmp/buildx-cfg ./deploy.sh --all`
  - `--all` 部署全部扩展（跳过交互菜单）；`BUILDX_CONFIG` 是沙箱内跑 buildx 的必要重定向 —— 默认 `~/.docker/buildx/activity/` 写入被沙箱拦截（`operation not permitted`），且该失败**对后台任务 `dangerouslyDisableSandbox` 无效**。
- 结果：**23/23 步全绿**，耗时 **16m02s**。

---

## 4. 运行态验证

```
NAMES                   STATUS                    PORTS
synapse-app             Up 17 minutes (healthy)   127.0.0.1:8008->8008, 127.0.0.1:9090->9090
synapse-nginx           Up 17 minutes (healthy)   0.0.0.0:80, 443, 8448
synapse-postgres        Up 17 minutes (healthy)   5432/tcp
synapse-redis           Up 17 minutes (healthy)   6379/tcp
synapse-prometheus      Up 14 minutes (healthy)   127.0.0.1:9092->9090
synapse-alertmanager    Up 15 minutes (healthy)   127.0.0.1:9093->9093
synapse-grafana         Up 15 minutes (healthy)   127.0.0.1:3000->3000
synapse-node-exporter   Up 15 minutes (healthy)   127.0.0.1:9100->9100
synapse-alert-handler   Up 15 minutes (healthy)   127.0.0.1:8080->8080
coturn                  Up                       3478/5349 tcp+udp, 9641
```

| 项目 | 结果 |
| --- | --- |
| `GET /health` | **200** |
| `GET /_matrix/client/versions`（http） | 301 → `https://` （nginx 对 Matrix 端点强制 HTTPS，**预期行为**） |
| `GET /_matrix/client/versions`（https / 跟随跳转） | **200**，返回 `unstable_features` 完整 |
| 启动日志校验 | 已知错误 0 / 未知错误 0 |
| DB 版本一致性 | schema 227 vs 227，**drift = 0** PASS（注：`information_schema.tables` 计 231，因含 4 个 view；门禁按 `CREATE TABLE`/`pg_tables` 计数） |
| 监控 target | 5/5 `up` |

**安全收口**：此前误留在部署目录的 Ed25519 PEM 私钥（`signing.key`，118B）已被应用覆写为自有 `ed25519 <key_id> <secret>` 格式（72B、0600）。已确认部署目录内不存在任何 `BEGIN PRIVATE KEY`，且 3 个备份 tarball 均不含 `signing.key`。

---

## 5. 复现命令

```bash
# 部署（沙箱内必须重定向 buildx 状态）
cd docker/deploy && BUILDX_CONFIG=/tmp/buildx-cfg ./deploy.sh --all

# 健康
curl -s -o /dev/null -w '%{http_code}\n' http://localhost/health
curl -sk -o /dev/null -w '%{http_code}\n' https://localhost/_matrix/client/versions

# 监控 target
curl -s http://127.0.0.1:9092/api/v1/targets | python3 -m json.tool | grep -E '"(job|health)"'

# 单测门禁（PATH 前置避坑，见 §7）
PATH="/usr/bin:/bin:$PATH" cargo nextest run --test unit --features test-utils
```

---

## 6. ⚠️ 未修复的遗留缺陷：观测面指标名系统性错配

**已发现、已取证、本次未修复**（不在原指令范围内，需单独立项）。

### 6.1 现象 A — 所有 `histogram_quantile` 表达式永久无数据

应用对自有的 `*_ms` 指标**只导出 `_count` / `_sum`，不导出 `_bucket` / `le=`**。Prometheus 中 13 个 `_bucket` 序列**全部来自 prometheus/alertmanager 自监控**，与业务无关。

实测：

```
应用侧 duration 类指标实际形态：
  db_query_duration_ms_count / db_query_duration_ms_sum
  http_request_duration_ms_count / http_request_duration_ms_sum
  megolm_session_key_read_duration_ms_count / _sum
  ...（全部无 _bucket）

_bucket 序列（13 个）：
  alertmanager_*_bucket, prometheus_*_bucket   ← 无一属于业务
```

但规则文件里有 **8 处**在用 `histogram_quantile(..., rate(*_bucket[5m]))`：

| 文件 | 行 | 表达式引用的指标 |
| --- | --- | --- |
| `prometheus/recording-rules.yml` | 31 / 35 / 39 | `http_request_duration_ms_bucket` |
| `prometheus/recording-rules.yml` | 63 | `db_query_duration_ms_bucket` |
| `prometheus/recording-rules.yml` | 94 | `megolm_share_db_duration_ms_bucket` |
| `prometheus/alerting-rules.yml` | 148 | `db_query_duration_ms_bucket` |
| `prometheus/alerting-rules.yml` | 179 | `http_request_duration_ms_bucket` |
| `prometheus/alerting-rules.yml` | 249 | `megolm_session_key_read_duration_ms_bucket` |

**逐条产出实测**（`/api/v1/label/__name__/values` 比对）：

```
有数据    job:http_requests:rate5m
有数据    job:http_errors:rate5m
无数据!!  job:http_request_duration:avg5m     ← 依赖死 _bucket
无数据!!  job:http_request_duration:p95_5m    ← 依赖死 _bucket
无数据!!  job:http_request_duration:p99_5m    ← 依赖死 _bucket
无数据!!  instance:db_query_duration:p95_5m   ← 依赖死 _bucket
无数据!!  instance:megolm_share_duration:p95_5m ← 依赖死 _bucket
无数据!!  instance:disk_usage:percent         ← 见 6.2，另一根因
```

**影响**：5 条 recording rule 永不产数据；3 条告警规则（DB 慢查询 p95>500ms、HTTP p99>2000ms、megolm 会话密钥读 p95>100ms）**永不触发** —— 属于「纸面告警」。

**建议修法**：二选一 ——
(a) 应用侧改用 Prometheus histogram（导出 `_bucket` + `le=`）；或
(b) 应用侧维持 `_count`/`_sum`，规则侧改为 `rate(*_ms_sum[5m]) / rate(*_ms_count[5m])` 算均值（放弃分位数，或改用 summary 形态）。

### 6.2 现象 B — `instance:disk_usage:percent` 无数据

规则 `(1 - (node_filesystem_avail_bytes / node_filesystem_size_bytes)) * 100` 未做聚合/过滤：`node_filesystem_*` 带多个 `mountpoint`，触发**多对多向量匹配错误**，规则无输出，仅靠静态 `labels: mountpoint: "/"` 无法修正。需补 `by (instance)` 或 `ignoring(mountpoint,fstype,device)` + `fstype!~"tmpfs|overlay"`。

### 6.3 现象 C — Grafana 面板 100% 引用不存在的指标

7 个仪表盘中引用 `synapse_*` / `coturn_*` 命名空间的指标共 **22 个，无一存在**（命中率 0/22）：

```
coturn_active_connections          synapse_active_users
coturn_allocations_total           synapse_auth_failure_total          ← 实为 auth_failures_total
coturn_relay_addresses_in_use      synapse_clientlogin_*_total        ← 无此命名空间
coturn_relay_addresses_total       synapse_database_pool_used/max     ← 实为 pool_utilization
coturn_stun_requests_total         synapse_database_query_duration_seconds_bucket
coturn_turn_allocation_duration_seconds_bucket
                                   synapse_e2ee_session_count
                                   synapse_federated_rooms
                                   synapse_federation_txmsg_success/failure_total ← 实为 federation_signature_*
                                   synapse_persist_events_duration_seconds_count/sum
                                   synapse_rate_limit_rejections_total ← 实为 rate_limit_requests_rejected_total
                                   synapse_signing_duration_seconds_bucket
```

真实命名空间为：`pool_utilization`、`auth_failures_total`、`rate_limit_*`、`federation_*`、`turn_*`（coturn 实为 `turn_total_allocations` / `turn_traffic_*` / `turn_unauthenticated_401_requests` 等）。

**影响**：相关面板**永久空白**（不是偶发，是命名空间整体错位）。

**取证脚本**（可重复执行）：

```bash
curl -s 127.0.0.1:9092/api/v1/label/__name__/values > /tmp/names.json
# 注意假阳性陷阱：instance:x:y 形态的 recording rule 产物、'- record:' 列表项本身
```

---

## 7. 环境备注（会咬人的坑）

1. **沙箱内跑 buildx 必须 `BUILDX_CONFIG=/tmp/buildx-cfg`** —— 默认写 `~/.docker/buildx/activity/` 被拦截；且该拦截对后台任务的 `dangerouslyDisableSandbox: true` **无效**。
2. **WorkBuddy CLI 的 `grep` 是 toybox 0.8.13 垫片**，缺 GNU BRE 的 `\+` / `\|`。后果：
   - `tikv-jemalloc-sys` 的 `configure` 用 `grep '^[0-9]\+\.[0-9]\+\.[0-9]\+-...'` 判版本 → 误判失败，构建中断；
   - 任何依赖 `\+`/`\|` 的正则**静默漏匹配**（不报错，只少结果）。
   规避：`PATH="/usr/bin:/bin:$PATH"` 前置真实 GNU grep。
3. **同一文件并行 Edit 会丢更新**（read-modify-write 竞态）—— 同一文件的多次编辑必须**串行**，并改后用 `awk 'NR==n'` 逐行确认。
