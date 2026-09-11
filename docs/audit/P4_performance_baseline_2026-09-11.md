# P4 性能基线 — 实测数据、测量有效性缺陷与门禁基础设施审查

> **日期**: 2026-09-11
> **基线提交**: `fd0d173f`
> **运行环境**: Docker Compose 本地栈（`docker/deploy/docker-compose.yml`，服务名 `synapse`，容器 `synapse-app`，镜像 `synapse-rust:local`，构建于 2026-09-11T09:12），PostgreSQL 16 + Redis 7，宿主机 macOS（OrbStack）
> **基准目标**: `benches/performance_api_benchmarks.rs`、`benches/performance_federation_benchmarks.rs`
> **隔离构建**: `CARGO_TARGET_DIR=/tmp/pbench`（规避与并发构建争用 `target/`，见 P5 §3.1）

---

## 0. 结论摘要

| 项 | 状态 |
|---|---|
| Federation 基准（纯计算） | ✅ 已采集 |
| API 基准（需服务） | ✅ 已采集，**11/11 全部完成**（干净基线，零 429） |
| 基准自身能否在默认环境下产出有效数据 | 🔴 **否** —— 默认环境只跑 1/11，且受 429 支配（见 §2、§3） |
| `TESTING.md` §4.3 逐端点 P95 指标的执行者 | 🔴 **不存在**（见 §5.1） |
| sliding sync 性能回滚门禁 | 🔴 **存在但从未接线**（见 §5.2，**已修复**） |
| 动态/静态 SQL 比例门禁 | 🔴 **未接线 + 扫描范围错误 + 当前 FAIL**（见 §5.3，**已修复**） |
| 分页性能门禁（`benchmark.yml` 阻塞步骤） | 🔴 **门禁在、基准没了 → 必然失败 98 天**（见 §5.4，**已修复**） |
| 基准"静默跳过"防护 | 🔴 缺失（见 §2，**已修复**） |
| 单文件 bind mount 与配置降级路径 | 🟠 见 §5.5、§5.6（未修复，已移交） |
| `TESTING.md` 阈值与代码阈值一致性 | 🔴 不一致（500/1000ms vs 5000ms） |

> **核心结论**：本项目有**四套性能/SQL 质量门禁的文书**，修复前的实际状态是：
> 两套从未接线、一套接线了但断言的基准早已被删除（必然失败 98 天）、
> 一套的扫描范围与阈值都不可用。与 P5 记录的 doc-test 空门禁同型：
> **门禁存在 ≠ 门禁生效**。
>
> 本文件记录缺陷；对应修复见 §9。

---

## 0.1 本轮已落地的修复

| # | 缺陷 | 修复 | 回归证据 |
|---|---|---|---|
| 1 | 11 个基准静默跳过、退出码仍 0 | `BENCH_REQUIRE` 必需组守护（`benches/performance_api_benchmarks.rs`） | `tests/unit/pagination_gate_tests.rs`（11 项） |
| 2 | 分页门禁断言的基准已被删除 | 恢复 `benchmark_pagination_strategies` 并注册 | `pagination_gate_tests::pagination_gate_benchmarks_exist_in_api_bench_source` 等 |
| 3 | SQLx 门禁只扫 `src/`、阈值不可达、死引用、未接线 | 改为 workspace 棘轮基线并接入 `ci.yml` | `tests/unit/sqlx_ratio_gate_tests.rs`（9 项） |
| 4 | sliding sync 门禁从未接线 | 新增 `sliding-sync-perf-gate` job（带 Postgres） | `scripts/ci/sliding_sync_perf_gate.sh` 预检改为不依赖 `pg_isready` |
| 5 | sliding sync 基准的"零执行"守卫永不触发 | 改为 `SLIDING_SYNC_REQUIRE` 组级守护 | 同上 |

> ⚠️ 修复 1 与 5 的共同教训：**"有没有任何基准跑过"是无效判据**。
> `pagination`（API bench）与 `benchmark_request_construction`（sliding sync bench）
> 都是纯内存计算、总会执行，因此 `executed > 0` 恒为真。
> 最初按此实现的 `BENCH_STRICT` 经实测确认**在服务不可达时仍退出 0**，
> 已改为按组点名（`BENCH_REQUIRE` / `SLIDING_SYNC_REQUIRE`）。

---

## 1. 采集方法

### 1.1 命令

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
export CARGO_TARGET_DIR=/tmp/pbench

# Federation（纯计算，无需服务）
cargo bench --bench performance_federation_benchmarks \
  -- --warm-up-time 1 --measurement-time 3 --sample-size 30

# API（需服务 + admin token）
export BENCH_BASE_URL=http://localhost:8008
export BENCH_ADMIN_TOKEN='<见 §1.3>'
cargo bench --bench performance_api_benchmarks \
  -- --warm-up-time 2 --measurement-time 5 --sample-size 50
```

> ⚠️ **可比性说明**：命令行采样参数覆盖了 `criterion_group!` 内的配置。
> criterion 对部分基准仍报 `Unable to complete 50 samples in 5.0s`
> （`sync_*`、`concurrent_load_versions/128`）—— 说明 5s 对这些基准偏短。
> 若要用于回归判定，应统一采样参数、固定负载并在同机比较。

### 1.2 服务状态

栈由 `docker/deploy/docker-compose.yml` 编排（**非** `docker/docker-compose.yml`）：

```bash
cd docker/deploy && docker compose config --services
# redis postgres synapse migrator nginx
```

### 1.3 取得 bench token

`docker/deploy/config/homeserver.yaml` 开启 `enable_registration: true`，
可直接注册一个临时用户取 token（本次为 `@benchprobe:matrix.test`，
**采集结束后已按 §6 删除**）：

```bash
BENCH_ADMIN_TOKEN=$(curl -s -X POST http://localhost:8008/_matrix/client/v3/register \
  -H 'Content-Type: application/json' \
  -d '{"username":"benchprobe","password":"<临时强口令>","device_id":"BENCH",
       "auth":{"type":"m.login.dummy"}}' | python3 -c 'import json,sys;print(json.load(sys.stdin)["access_token"])')
export BENCH_ADMIN_TOKEN
```

> ⚠️ 该 token 是**真实凭据**，不要写进文档或提交历史。
> 采集完成后请删除临时用户（见 §6）。

---

## 2. 🔴 缺陷一：默认环境下 11 个基准只有 1 个真正执行

`performance_api_benchmarks.rs` 注册 11 个基准，分 6 个 benchmark 函数：

| 函数 | 基准数 | 守卫 |
|---|---|---|
| `benchmark_versions_endpoint` | 1 | `server_required` |
| `benchmark_user_directory_search` | 2 | **`BENCH_ADMIN_TOKEN`** |
| `benchmark_room_operations` | 2 | **`BENCH_ADMIN_TOKEN`** |
| `benchmark_sync_operations` | 2 | **`BENCH_ADMIN_TOKEN`** |
| `benchmark_auth_operations` | 1 | **`BENCH_ADMIN_TOKEN`** |
| `benchmark_concurrent_throughput` | 4 | `server_required` |

其中 7 个被 `bench_admin_token()` 守卫拦截；守卫失败时**只打印一行 `eprintln!` 后 `return`**，
**不改变进程退出码、不产出任何报告**：

```rust
let Some(admin_token) = bench_admin_token() else {
    eprintln!("[perf] BENCH_ADMIN_TOKEN not set; skipping room benches");
    return;
};
```

实测（未设 token，`/tmp/bench_api.log`）：

```
[perf] BENCH_ADMIN_TOKEN not set; skipping authenticated benches
[perf] BENCH_ADMIN_TOKEN not set; skipping room benches
[perf] BENCH_ADMIN_TOKEN not set; skipping sync benches
[perf] BENCH_ADMIN_TOKEN not set; skipping whoami bench
EXIT=0
```

⇒ **11 个基准里只跑了 1 个（`server_versions`），而 `cargo bench` 退出码为 0。**
任何一个"跑了 benchmark 就算过"的 CI 步骤都会被这个假绿骗过 ——
与 P5 §3.1 记录的 `cargo test --doc` 空门禁是**同一类失效模式**。

**修复方向**：bench 归零时以非零码退出，或提供 `--require-server` 选项让显式要求服务的运行失败。

---

## 3. 🔴 缺陷二：默认限流配置下，实测延迟主要是 429 拒绝路径

### 3.1 现象

未设 token 的那次运行中，13 行计时输出里 `server_versions` 均值 **1.0557 ms**，
而 `--measurement-time 3` × 1395 iterations ≈ **465 req/s**。
而 `docker/deploy/config/rate_limit.yaml` 对 `/_matrix/client/versions` 的规则是：

```yaml
  - path: "/_matrix/client/versions"
    rule:
      per_second: 10
      burst_size: 30
```

`match_type` 缺省为 `Exact`（`synapse-common/src/rate_limit_config.rs:55-57`），
且该路径**不在 `exempt_paths`**（那里只有 `/`）—— 所以 `/_matrix/client/versions`
在 465 req/s 下**绝大多数请求被 429 掉**。

### 3.2 为什么这不是猜测

容器日志的限流拒绝记录（去掉 ANSI 后按端点聚合，覆盖两次运行共 25 分钟）：

| 端点 | 429 次数 |
|---|---|
| `/_matrix/client/versions` | **22,836** |
| `/_matrix/client/r0/user_directory/search` | 12,292 |
| `/_matrix/client/r0/rooms/!test:localhost/state` | 2,915 |
| `/_matrix/client/r0/rooms/!test:localhost/members` | 1,164 |
| `/_matrix/client/r0/account/whoami` | 893 |
| **合计** | **40,100** |

拒绝日志自带生效规则，与配置逐条吻合，例如：

```
rate limit rejected request ... request_path=/_matrix/client/versions
  endpoint=/_matrix/client/versions per_second=10 burst_size=30 retry_after_seconds=1
rate limit rejected request ... request_path=/_matrix/client/r0/user_directory/search
  ... is_authenticated=true per_second=50 burst_size=100 retry_after_seconds=1
```

⇒ **每一个被测端点都在限流**，因此 §3.3 那批数字**不能作为端点真实性能的锚点**。

### 3.3 受污染基线（带 token，但限流仍开）——仅作限流行为证据

| 基准 | 均值 | 下界 | 上界 | 区间/均值 |
|---|---|---|---|---|
| `server_versions` | 2.9086 ms | 2.6793 ms | 3.1007 ms | 1.14× |
| `user_directory_search_single` | 1.2510 ms | 1.0663 ms | 1.5226 ms | 1.36× |
| `user_directory_search_batch_10` | 37.609 ms | 4.7386 ms | 111.97 ms | **23.6×** |
| `room_state_query` | 2.5552 ms | 2.0517 ms | 2.9733 ms | 1.45× |
| `room_members_list` | 2.1858 ms | 1.5209 ms | 2.7480 ms | 1.81× |
| `sync_with_timeout` | 7.7433 ms | 7.5463 ms | 7.9445 ms | 1.05× |
| `sync_short_timeout` | 7.1204 ms | 6.6302 ms | 7.7427 ms | 1.17× |
| `whoami` | 3.0797 ms | 2.7397 ms | 3.3786 ms | 1.21× |
| `concurrent_load_versions/1` | 3.6179 ms | 3.4396 ms | 3.8017 ms | 1.11× |
| `concurrent_load_versions/8` | 32.288 ms | 4.5649 ms | 64.497 ms | **14.1×** |
| `concurrent_load_versions/32` | 10.292 ms | 9.0455 ms | 12.014 ms | 1.33× |
| `concurrent_load_versions/128` | 329.74 ms | 188.16 ms | 486.61 ms | 2.59× |

> 同一次运行内的**超宽置信区间**（23.6×、14.1×）本身就是限流注入的直接指纹：
> 迭代过程中 burst/令牌桶反复耗尽与恢复。

### 3.4 对照：同一基准在另一次运行中的漂移

`server_versions` 在未设 token 运行中为 **1.0557 ms**，在带 token 运行中为 **2.9086 ms**（2.8×）。
两次数值差异来自**限流状态与环境负载**，与端点实现无关 —— 再次说明 §3.3 不可用作锚点。

---

## 4. ✅ 干净基线（临时关闭限流）

### 4.1 取得方法

```bash
# 1) 备份
cp docker/deploy/config/rate_limit.yaml /tmp/deploy_rate_limit.yaml.orig
shasum -a 256 docker/deploy/config/rate_limit.yaml   # cb876d870d16d8ed…

# 2) 关闭（顶层 enabled: true → false）
sed -i '' 's/^enabled: true$/enabled: false/' docker/deploy/config/rate_limit.yaml

# 3) 必须重启容器 —— 理由见 §5.5（单文件 bind mount inode 失效）
cd docker/deploy && docker compose restart synapse

# 4) 验证限流确实关闭：60 次 /versions 应全部 200
for i in $(seq 1 60); do curl -s -o /dev/null -w "%{http_code}\n" \
  http://localhost:8008/_matrix/client/versions; done | sort | uniq -c
#   60 200          ← 通过

# 5) 采集
export BENCH_BASE_URL=http://localhost:8008 BENCH_ADMIN_TOKEN='<§1.3>'
export CARGO_TARGET_DIR=/tmp/pbench
cargo bench --bench performance_api_benchmarks \
  -- --warm-up-time 2 --measurement-time 5 --sample-size 50

# 6) 恢复（必须！）
git checkout -- docker/deploy/config/rate_limit.yaml
shasum -a 256 docker/deploy/config/rate_limit.yaml   # 必须仍为 cb876d870d16d8ed…
cd docker/deploy && docker compose restart synapse
```

### 4.2 有效性验证

| 检查 | 结果 |
|---|---|
| 采集窗口内 429 数（`docker logs ... \| grep -c rate_limit_rejected`） | **0** |
| 完成的基准数 | **11 / 11** |
| criterion `Found N outliers` | 11 个基准均有（2%–22%） |
| `Unable to complete 50 samples in 5.0s` 警告 | 3 处（`sync_with_timeout`、`sync_short_timeout`、`concurrent_load_versions/128`） |

### 4.3 API 干净基线

| 基准 | 下界 | **均值** | 上界 | 区间/均值 | 异常值 |
|---|---|---|---|---|---|
| `server_versions` | 1.9016 ms | **2.2042 ms** | 2.5012 ms | 1.14× | 3 (6%) |
| `user_directory_search_single` | 3.0932 ms | **3.6620 ms** | 4.2947 ms | 1.39× | 6 (12%) |
| `user_directory_search_batch_10` | 6.1900 ms | **6.3778 ms** | 6.5817 ms | 1.06× | 2 (4%) |
| `room_state_query` | 2.7780 ms | **3.1510 ms** | 3.5503 ms | 1.28× | – |
| `room_members_list` | 2.3377 ms | **2.6294 ms** | 2.9424 ms | 1.26× | 3 (6%) |
| `sync_with_timeout` | 3.7194 ms | **4.0905 ms** | 4.5802 ms | 1.23× | 3 (6%) |
| `sync_short_timeout` | 5.4025 ms | **5.8190 ms** | 6.3301 ms | 1.16× | 5 (10%) |
| `whoami` | 1.8852 ms | **1.9506 ms** | 2.0100 ms | 1.06× | 6 (12%) |
| `concurrent_load_versions/1` | 1.5882 ms | **1.6939 ms** | 1.7906 ms | 1.13× | 5 (10%) |
| `concurrent_load_versions/8` | 3.6005 ms | **55.946 ms** | 160.47 ms | **43.9×** | 1 (2%) |
| `concurrent_load_versions/32` | 9.4776 ms | **9.7503 ms** | 10.088 ms | 1.06× | 2 (4%) |
| `concurrent_load_versions/128` | 103.88 ms | **341.32 ms** | 713.18 ms | **6.9×** | 11 (22%) |

### 4.4 Federation 干净基线（纯计算，无 DB / 无服务）

| 基准 | 均值 |
|---|---|
| `state_resolution_chain_10` | **271 ns** [269.89, 272.78] |
| `state_resolution_chain_100` | **283 ns** [278.61, 286.80] |
| `auth_chain_build_10` | **5.21 µs** [5.1983, 5.2305] |

### 4.5 ⚠️ 基线自身的不确定性必须随数字一起使用

即使关闭限流，两个并发基准仍**极不稳定**：

- `concurrent_load_versions/8` 区间跨度 **43.9×**（3.60 – 160.47 ms）
- `concurrent_load_versions/128` 区间跨度 **6.9×**，异常值占 **22%**

且 `concurrent_load_versions/*` 的语义 **不是"每请求延迟"而是"N 个并发请求的墙钟总时长"**
（`benches/performance_api_benchmarks.rs:261-276`，代码注释亦承认"criterion 0.5 does not
expose `Bencher::throughput` on the parameterised path"）。因此：

> **这批并发数字只能用于同机、同参数、同负载下的相对回归比较，
> 不可解读为绝对 P95，也不应与 §4.3 的单请求基准直接并列。**

建议后续以 `--sample-size 10` + 更长 `measurement-time`（criterion 自身提示 131s）
或改用支持 `Throughput` 的 criterion API 重做并发基准。

### 4.6 基准资产清单

| 目标 | 行数 | 基准数 | 需服务/DB |
|---|---|---|---|
| `performance_api_benchmarks` | 296 | 11 | 服务（+ token 7 项） |
| `performance_federation_benchmarks` | 119 | 5 | 否 |
| `performance_membership_benchmarks` | 65 | 3 | 否 |
| `performance_sliding_sync_benchmarks` | 399 | 8 | **是** |

> `performance_sliding_sync_benchmarks`（8 个）本次**未采集** —— 明确标注为缺口。

---

## 5. 🔴 门禁基础设施：四套门禁的实际状态

### 5.1 `TESTING.md` §4.3 的逐端点 P95 指标 —— 没有任何执行者

`TESTING.md:261-277` 给出了**与基准函数名逐条对应**的目标：

| 基准函数 | 文档目标 | §4.3 实测均值 | 是否被断言 |
|---|---|---|---|
| `benchmark_user_directory_search` 单用户 | ≤100 ms | 3.66 ms | 🔴 否 |
| `benchmark_user_directory_search` 10 并发 | ≤500 ms | 6.38 ms | 🔴 否 |
| `benchmark_room_operations` 状态查询 | ≤50 ms | 3.15 ms | 🔴 否 |
| `benchmark_room_operations` 成员列表 | ≤100 ms | 2.63 ms | 🔴 否 |
| `benchmark_sync_operations` 带超时同步 | ≤500 ms | 4.09 ms | 🔴 否 |
| `benchmark_sync_operations` 快速同步 | ≤200 ms | 5.82 ms | 🔴 否 |
| `benchmark_auth_operations` whoami | ≤20 ms | 1.95 ms | 🔴 否 |

另有 §1.2 表（性能测试 P95≤500ms）与 §测试质量门禁（搜索 500ms / 同步 1000ms / DB 100ms）。

**结论**：这些阈值**全部只存在于文档**。没有测试、没有 CI 步骤读取或断言它们。
唯一涉及 p95 的测试是 §5.2 的日志解析器单测，**从不测量任何真实延迟**。

> 附注：相对当前实测，这些阈值宽松到失去回归检测能力（whoami 阈值 20ms vs 实测 1.95ms，10×；
> room 状态查询 50ms vs 3.15ms，16×）。即便接线，也需要重新标定。

### 5.2 sliding sync 性能门禁 —— 存在、逻辑完整、**从未运行**

`scripts/ci/sliding_sync_perf_gate.sh`（可执行）自述：

> Runs the sliding sync criterion benchmark and enforces a p95 latency threshold.
> **Inspired by Synapse v1.153.0rc3, which reverted a [sliding-sync optimization]
> because no threshold gate existed.**

即该脚本的设计目的**正是** AGENTS.md 记录的"上游 Synapse 教训"。

| 检查 | 结果 |
|---|---|
| 脚本存在且可执行、阈值逻辑完整 | ✅ 默认 5000ms，可经 `SLIDING_SYNC_P95_THRESHOLD_MS` 覆盖 |
| **被任意 CI workflow 调用** | 🔴 **否** (`grep -rn sliding_sync_perf_gate .github/workflows/` → 无命中) |

**测试覆盖的假象**：`tests/unit/sliding_sync_perf_gate_tests.rs` 存在且通过，
但只断言脚本存在、可执行，以及能**解析** `[perf] sliding_sync manual_p95_ms=…` 日志行
——**从不执行脚本**。⇒ 即使 sliding sync 延迟退化 10 倍，这套"门禁"也不会变红。

**修复（本次）**：

* `benchmark.yml` 新增 `sliding-sync-perf-gate` job：起 Postgres service、
  应用迁移、以 `SLIDING_SYNC_PERF_GATE_STRICT=1` 运行脚本。
* 脚本原先用 `pg_isready` 做预检，而它属于 `postgresql-client`、GitHub runner
  上**未必存在**，会让门禁永久"数据库不可达"。已改为三级回退：
  `pg_isready` → Python TCP 探测 → bash `/dev/tcp`。
* 基准侧的"零执行"守卫原为 `BENCH_STRICT` 式的计数判定，同样因
  `benchmark_request_construction`（纯内存、总会执行）而**永不触发**；
  已改为 `SLIDING_SYNC_REQUIRE=sliding_sync_p95_p99_latency` 按组点名。

### 5.3 动态/静态 SQL 比例门禁 —— 未接线 + 扫描范围错误 + 当前 FAIL

`scripts/ci/check_sqlx_dynamic_ratio.sh` 自述为 CI gate，超过 0.30 即失败。

```console
$ bash scripts/ci/check_sqlx_dynamic_ratio.sh
check_sqlx_dynamic_ratio: dynamic=25 static=0 total=25 ratio=1.0000 (max=0.30)
check_sqlx_dynamic_ratio: FAIL (dynamic ratio 1.0000 超过阈值 0.30)
$ echo $?
1
```

| 检查 | 结果 |
|---|---|
| 被任意 CI workflow 调用 | 🔴 **否** |
| 直接运行 | 🔴 **EXIT=1**（ratio 1.0 > 0.30） |
| 引用的 `docs/synapse-rust/M3_SQLX_MIGRATION_PLAN.md` | 🔴 **不存在**（全仓无此文件，死引用） |

**扫描范围错误**：脚本只 `grep ... src/`，而 SQL 调用绝大多数在 workspace crate：

| 范围 | `sqlx::query` 出现数 |
|---|---|
| 脚本实际扫描的 `src/` | **25** |
| 仅 `synapse-storage/src/` | **1,751** |

⇒ 报告的 `ratio=1.0000` 基于 **~0.6%** 的样本，数字本身不可信。

**修复后的实测口径（本次）**：扫描 `src/` + 6 个 workspace crate 的 `src/`，
排除 `target/`、`.git/`、`.claude/`（后者含旧仓库副本，会随本地 worktree 漂移）：

| 范围 | dynamic | static |
|---|---|---|
| `src/` | 25 | 0 |
| `synapse-common/src` | 5 | 0 |
| `synapse-cache/src` | 0 | 0 |
| `synapse-storage/src` | 1,123 | 52 |
| `synapse-e2ee/src` | 118 | 0 |
| `synapse-federation/src` | 15 | 9 |
| `synapse-services/src` | 141 | 0 |
| **合计** | **1,427** | **61** |

ratio = 0.9590。**静态调用仅集中在 6 个安全敏感模块**：
`refresh_token/mod.rs`(27)、`token.rs`(16)、`key_rotation.rs`(9)、
`federation_blacklist.rs`(5)、`sliding_sync/repository.rs`(3)、`user/storage.rs`(1)。

> 这同时说明原阈值的性质：`max=0.30` 要求把约 1,400 处调用改完，
> 是个"先重构再接线"的前置条件，而不是可执行的质量门禁。
> 已改为棘轮（动态不得增加、静态不得减少），基线文件
> `scripts/ci/sqlx_dynamic_ratio_baseline`，并接入 `ci.yml` 的 `repo-sanity` job。

### 5.4 分页性能门禁 —— 门禁在、基准没了，**必然失败 98 天**

`.github/workflows/benchmark.yml` 有一个**阻塞**步骤：

```console
python3 scripts/check_pagination_benchmark.py benchmark.txt --minimum-improvement 0.30
```

它断言 `benchmark.txt` 含 `pagination_offset_deep_page` 与
`pagination_keyset_deep_page` 两行 Criterion 输出（`scripts/check_pagination_benchmark.py:38-41`）：

```python
offset = results.get("pagination_offset_deep_page")
keyset = results.get("pagination_keyset_deep_page")
if offset is None or keyset is None:
    raise SystemExit("pagination benchmark rows were not found in benchmark output")
```

**但这两个基准在 `8c7b4860`（2026-06-05）被删除了。** 该提交从
`benches/performance_api_benchmarks.rs` 移除了 `benchmark_pagination_strategies`
（连同 `synthetic_reports` / `offset_page_checksum` / `keyset_page_checksum`），
而 `benchmark.yml` 的这一步没有被同步移除：

```console
$ git log --format='%h %ad %s' --date=short -1 8c7b4860
8c7b4860 2026-06-05 M-3 Batch 1 phase A+B+C: critical path hardening + v8 migration baseline

$ git log --format='%h %ad %s' --date=short -1 -- .github/workflows/benchmark.yml
2fb48ced 2026-09-10 chore: 提交全部改动（W1-W4 遗留 + db_tests 环境修复 + 联邦落库模板）
```

实测该门禁对空输入的行为：

```console
$ : > benchmark.txt
$ python3 scripts/check_pagination_benchmark.py benchmark.txt --minimum-improvement 0.30
pagination benchmark rows were not found in benchmark output
$ echo $?
1
```

**双重损害**：

1. 该阻塞步骤在 push/PR 上**必然失败**（除非有人在 workspace 手放一个含这两行的
   `benchmark.txt`——`benchmark.txt` 未被 git 跟踪，见 `8149d12b`），
   持续 **98 天**（2026-06-05 → 2026-09-11）。
2. 该门禁本应守护的**分页性能完全没有被测量**——这才是设计意图的落空。

> 该步骤自 `a465d0fd`（2026-05-09，引入门禁）起存在，
> 与基准的删除从未对账。

**修复**：恢复基准本体（它是**纯内存** offset-vs-keyset 对比，250k 合成行，
不需要服务或数据库，因此在 CI 中真实可跑），并加回归保护。

### 5.5 `TESTING.md` 与代码的阈值口径不一致

| 来源 | 指标 | 值 |
|---|---|---|
| `TESTING.md` §4.3 等 | 搜索 / 同步 / DB P95 | 500 / 1000 / 100 ms |
| `scripts/ci/sliding_sync_perf_gate.sh` | sliding sync P95 | **5000 ms** |

两套数字**没有任何换算或对应关系**，且都无执行者。

### 5.6 为什么"临时关限流"必须重启容器 —— 单文件 bind mount 脆弱性（新发现）

本次取证过程中命中一个**真实且与限流无关的部署隐患**。
`docker/deploy/docker-compose.yml` 用**单文件** bind mount 挂载配置：

```yaml
volumes:
  - ./config/homeserver.yaml:/app/config/homeserver.yaml:ro
  - ./config/rate_limit.yaml:/app/config/rate_limit.yaml:ro
```

当宿主机对该文件做**原子替换**（写临时文件 + `rename`，编辑器与 `sed -i` 的常见实现）后，
Docker 的绑定仍指向**已失效的 inode**，容器内路径随即"消失"，**而容器继续运行、继续服务**：

```console
$ docker exec synapse-app head -2 /app/config/rate_limit.yaml
head: cannot open '/app/config/rate_limit.yaml' for reading: No such file or directory

$ docker logs synapse-app | grep "Failed to reload"
WARN rate_limit_config: Failed to reload rate limit config:
     Failed to read config file: No such file or directory (os error 2)
```

**影响面**：

1. 配置热加载**静默失效**，进程继续用旧配置服务（本次实测：限流仍按 10/30 生效，
   而我已把宿主机文件改为 `enabled: false`）；
2. `homeserver.yaml` 使用**同一挂载模式**，同样暴露；
3. 必须 `docker compose restart synapse` 才能让挂载重新解析。

**缓解**：改为挂载**目录**（`./config:/app/config:ro`）而非单个文件，
或约定"改配置后必须重启"并在 `deploy.sh` 中强制。

### 5.7 限流配置缺失时的静默降级路径（新发现）

`src/server/mod.rs:211-237`：

```rust
let (rate_limit_config_manager, config_watcher_handle) = if rate_limit_config_path.exists() {
    match RateLimitConfigManager::from_file(&rate_limit_config_path).await {
        Ok(manager) => { /* ...启动 watcher... */ }
        Err(e) => {
            tracing::warn!("限流配置加载失败 ({:?}): {}, 使用默认配置", rate_limit_config_path, e);
            let manager = create_rate_limit_manager(&rate_limit_config_path);   // ← 内置默认值
            (Some(manager), None)
        }
    }
} else {
    tracing::info!("限流配置文件不存在 ({:?}), 使用默认配置", rate_limit_config_path.display());
    let manager = create_rate_limit_manager(&rate_limit_config_path);           // ← 内置默认值
    (Some(manager), None)
};
```

而回退实现是：

```rust
fn create_rate_limit_manager(config_path: &std::path::Path) -> Arc<RateLimitConfigManager> {
    let default_config = RateLimitConfigFile::default();
    Arc::new(RateLimitConfigManager::new(default_config, config_path.to_path_buf()))
}
```

⇒ 配置缺失/加载失败时，生效的是 **`RateLimitConfigFile::default()`（代码内硬编码默认值）**，
**既不读 `homeserver.yaml` 的 `rate_limit` 段，也不读 `rate_limit.yaml`**：
运维人员写在配置文件里的限制被**无声丢弃**。

叠加 §5.5：单文件挂载失效 + 重启 → 走到 `exists() == false` 分支 → 静默使用硬编码默认值。

**可观测性缺口**：全仓无任何 metric / health 信号反映"限流配置已降级"：

```console
$ grep -rn "reload_fail\|config_degraded\|rate_limit.*health" --include='*.rs' src/ synapse-common/src/
(无命中)
```

且 §5.5 场景下 `RateLimitConfigManager` 会**每个 `reload_interval_seconds`（默认 30s）
重复打一条 WARN**，但该 WARN 从不升级、不计数、不影响 readiness。

> **旁证**：`docker/config/homeserver.yaml`（dev compose 用）保留了 4 条认证端点
> 限流覆盖（`per_second: 5, burst_size: 3`），而 `docker/deploy/config/homeserver.yaml`
> 将其删成 `endpoints: []`，并加注：
> *"the effective endpoint rules live in `rate_limit.yaml` … `endpoints` here is therefore inert"*。
> 该注释对**正常路径**是正确的（`AppState::rate_limit_config()` 返回文件配置优先），
> 但在 §5.6 的回退路径下 `homeserver.yaml` 的段**同样不被读取** —— 两处都失效。

### 5.8 已排除的候选项（避免夸大）

| 脚本 | 判定 |
|---|---|
| `check_sqlx_offline_cache.sh` | 自述为 **"Advisory gate"** ⇒ 未接线合乎其定位 |
| `run_complement_tests.sh` | 手动运行工具；另有 `.github/workflows/e2ee-interop.yml` 承担相关验证 |
| `check_route_storage_boundary.sh` · `supply_chain_gate.sh` · `benchmark_pr_gate.sh` | ✅ **均已接线** |

---

## 6. ⚠️ 本次取证的副作用（已恢复 / 待处理）

| 项 | 状态 |
|---|---|
| `docker/deploy/config/rate_limit.yaml` 临时 `enabled: false` | ✅ 已 `git checkout --` 恢复，SHA256 与备份逐字节一致（`cb876d870d16d8ed…`） |
| `docker/config/rate_limit.yaml` 误改（非实际挂载文件） | ✅ 已从备份恢复，SHA256 一致；`git status` 干净 |
| 容器重启（2 次） | ✅ 已恢复，`healthy`，限流重新生效（实测 35×200 / 25×429），reload 错误 0 |
| 临时用户 `@benchprobe:matrix.test` | ✅ **已删除**（连同其 `access_tokens` / `refresh_tokens` / `devices` 各 1 行）；`SELECT count(*) FROM users` = 0 |
| 临时 token | ✅ 未写入仓库（文档中只有 `<临时强口令>` / `<token>` 占位符）；已随用户删除失效 |

删除命令（本次实际执行的，仅作记录 —— 生产环境应走管理 API/服务层而非裸 SQL）：

```sql
BEGIN;
DELETE FROM access_tokens    WHERE user_id='@benchprobe:matrix.test';
DELETE FROM refresh_tokens   WHERE user_id='@benchprobe:matrix.test';
DELETE FROM devices          WHERE user_id='@benchprobe:matrix.test';
DELETE FROM users            WHERE user_id='@benchprobe:matrix.test';
COMMIT;
```

---

## 7. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
export CARGO_TARGET_DIR=/tmp/pbench

# --- 基线数字（需先按 §4.1 关限流 + 重启）---
cargo bench --bench performance_federation_benchmarks -- --warm-up-time 1 --measurement-time 3 --sample-size 30
BENCH_BASE_URL=http://localhost:8008 BENCH_ADMIN_TOKEN='<token>' \
  cargo bench --bench performance_api_benchmarks -- --warm-up-time 2 --measurement-time 5 --sample-size 50

# --- 缺陷二：默认环境下只跑 1/11 且退出码仍为 0 ---
cargo bench --bench performance_api_benchmarks -- --warm-up-time 1 --measurement-time 3 --sample-size 30
echo "EXIT=$?"        # 0

# --- 缺陷二：429 统计 ---
docker logs synapse-app --since 25m 2>&1 | sed -e 's/\x1b\[[0-9;]*m//g' \
  | grep -oE 'request_path=[^ ]+ endpoint=[^ ]+' | sort | uniq -c | sort -rn

# --- §5.2 门禁未接线（应为空）---
grep -rn "sliding_sync_perf_gate" .github/workflows/
# --- §5.3 门禁未接线 + 直接运行即 FAIL ---
grep -rn "check_sqlx_dynamic_ratio" .github/workflows/
bash scripts/ci/check_sqlx_dynamic_ratio.sh; echo "EXIT=$?"     # 1
ls docs/synapse-rust/M3_SQLX_MIGRATION_PLAN.md                  # No such file

# --- §5.5 单文件挂载脆弱性（改文件后容器内路径消失）---
sed -i '' 's/^enabled: true$/enabled: false/' docker/deploy/config/rate_limit.yaml
docker exec synapse-app head -1 /app/config/rate_limit.yaml     # No such file or directory
git checkout -- docker/deploy/config/rate_limit.yaml             # 恢复

# --- §5.6 降级路径 ---
sed -n '211,237p' src/server/mod.rs
sed -n '108,111p' src/server/mod.rs
```

---

## 8. 移交后续

### 8.1 本次已完成

| # | 项 | 证据 |
|---|---|---|
| 1 | 接入 `sliding_sync_perf_gate.sh`（新增 `sliding-sync-perf-gate` job，带 Postgres + 迁移） | `.github/workflows/benchmark.yml` |
| 2 | bench "被请求的基准静默跳过"改为非零退出（`BENCH_REQUIRE` / `SLIDING_SYNC_REQUIRE`） | 端到端实测：`pagination`→EXIT 0；`user_directory` 无 token→EXIT 1 |
| 3 | 恢复被删除的分页基准，使阻塞门禁可真实运行 | `benchmark_pagination_strategies` + `pagination_gate_tests` |
| 4 | SQLx 门禁改为 workspace 棘轮基线并接入 CI | `ci.yml` `repo-sanity` → `check_sqlx_dynamic_ratio.sh`（1427/61） |
| 5 | 删除 `check_sqlx_dynamic_ratio.sh` 的死引用（改为自述基线文件） | `sqlx_ratio_gate_does_not_reference_missing_docs` |

### 8.2 仍需处理

| # | 项 | 优先级 | 状态 |
|---|---|---|---|
| 1 | **加 metric/health 信号 + 告警**反映限流配置降级（§5.7） | **高** | ✅ **已完成**（提交 `4111d9eb`，见 `docs/audit/P4_rate_limit_observability_2026-09-11.md`） |
| 2 | 单文件 bind mount → 目录挂载（§5.6 的**根因**；§8.2 #1 只是让它可观测） | 中 | ✅ **已完成**（提交 `e1aa439c`，见 `docs/audit/P4_bind_mount_fix_2026-09-11.md`） |
| 3 | 重新标定并落实 `TESTING.md` 的 P95 阈值，或删除以免误导（§5.1） | 中 | ✅ **已完成**（提交 `99a63d47`，见 `docs/audit/P4_perf_gate_honesty_2026-09-11.md`） |
| 4 | 以 §4.3 / §4.4 为锚点，同机同参数比对回归（注意 §4.5 限制） | 中 | ⬜ 待办 |
| 5 | 采集 `performance_sliding_sync_benchmarks`（8 个，需服务/DB） | 低 | ⬜ 待办 |
| 6 | 在真实 CI 上确认 `sliding-sync-perf-gate` job 首跑结果（本地无法完整复现 runner 环境） | 中 | ⬜ 待办 |

> **#1 完成说明**：新增 `ConfigSource` / `RateLimitDegradation`、启动 ERROR 升级、
> `rate_limit_config_source_is_file` gauge、`/health` 与 `/_health` 的
> `rate_limit_config` 片段、watcher 连续失败 ≥3 升级 ERROR；11 个新测试。
> **它不修复 #2 的根因** —— 单文件 bind mount 仍会因宿主机原子替换而失效，
> 只是现在会以 `degraded` + ERROR 的形式暴露，而非静默。

---

## 9. 修复清单（代码对照）

| 文件 | 变更 |
|---|---|
| `benches/performance_api_benchmarks.rs` | 恢复 `SyntheticReportRow` / `synthetic_reports` / `offset_page_checksum` / `keyset_page_checksum` / `benchmark_pagination_strategies` 并注册；`criterion_main!` → 显式 `main` + `enforce_required_groups()`；新增 `require_bench_group("…")` 7 处 |
| `benches/performance_sliding_sync_benchmarks.rs` | 同上，组名：`request_construction` / `sync_response` / `subscription_changes` / `p95_p99` |
| `scripts/ci/check_sqlx_dynamic_ratio.sh` | 扫描范围 → `src/` + 6 个 workspace crate；排除 `.claude/`；`max=0.30` → 棘轮基线；删除死引用 |
| `scripts/ci/sqlx_dynamic_ratio_baseline`（新） | `BASELINE_DYNAMIC=1427` / `BASELINE_STATIC=61`（含测量记录与口径说明） |
| `scripts/ci/sliding_sync_perf_gate.sh` | 预检三级回退（不依赖 `pg_isready`）；`SLIDING_SYNC_REQUIRE` 取代旧 strict 计数 |
| `.github/workflows/ci.yml` | `repo-sanity` 新增 SQLx 棘轮步骤（阻塞） |
| `.github/workflows/benchmark.yml` | API 基准只请求 `pagination` 组 + `BENCH_REQUIRE`；分页门禁加事故注释；新增 `sliding-sync-perf-gate` job |
| `tests/unit/pagination_gate_tests.rs`（新） | 11 项：基准存在性/注册/无服务依赖、门禁真实子进程行为、`BENCH_REQUIRE` 契约、CI 接线审计 |
| `tests/unit/sqlx_ratio_gate_tests.rs`（新） | 9 项：扫描范围、排除 worktree、棘轮双向语义、基线文件存在性、死引用 |
| `tests/unit/mod.rs` | 注册上述两个模块 |

> ℹ️ 两个门禁测试模块刻意**不在测试内 spawn `cargo bench`**：
> 那会嵌套等待 cargo 构建锁，单个测试实测超过 12 分钟，会拖垮测试套件。
> 端到端契约改为 `#[ignore]`（显式运行命令见测试文档注释），
> 默认门禁只跑毫秒级的脚本子进程与源码契约断言（23 项约 0.5s）。
