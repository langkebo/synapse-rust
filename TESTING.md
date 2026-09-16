# Synapse Rust 测试策略与质量保证

本文档描述 synapse-rust 项目的测试策略、质量标准和执行规范。当前正式能力口径请以 `docs/INDEX.md` 为准；测试与 CI 语义收口请同时参考 `docs/INDEX.md` 与 `docs/INDEX.md`。

## 一、测试分层架构

### 1.1 测试金字塔

```
                    ┌─────────────┐
                    │   端到端    │  ← 5% (用户流程验证)
                   ┌┴─────────────┴┐
                   │   集成测试    │  ← 25% (API完整流程)
                  ┌┴───────────────┴┐
                  │    单元测试    │  ← 70% (组件功能验证)
                 └────────────────┘
```

### 1.2 测试类型说明

| 类型 | 位置 | 目的 | 覆盖率要求 |
|-----|------|------|-----------|
| 单元测试 | `tests/unit/*.rs` | 验证独立组件逻辑 | 目标 ≥80%，当前自动门槛以 `tarpaulin.toml` 的 `70%` 为准 |
| 集成测试 | `tests/integration/*.rs` | 验证 API 完整流程与高风险契约 | 主链与高风险能力域必覆盖 |
| 端到端测试 | `tests/e2e/*.rs` | ⚠️ **默认不验证端到端行为**（见下方注） | 真实 E2E 需 `E2E_RUN=1` + 运行中的服务 |

> ✅ **CI 的 lib 步骤已覆盖 workspace（2026-09-12 修复）**
> （原缺口记录见 `docs/audit/P5_ci_test_scope_gap_2026-09-11.md`）：
>
> | 范围 | 测试数 |
> |---|---|
> | 修复前（裸 `--lib`，只测根包） | **687** |
> | 现在（`--workspace --lib`） | **6133** |
>
> `.github/workflows/ci.yml` 的 lib 步骤现在是一条
> `cargo nextest run --workspace --lib --all-features --test-threads 4`，
> 实测 **6133 passed / 0 failed**。此前 `synapse-common`（862 个）、
> `synapse-storage`、`synapse-services`、`synapse-e2ee` 等 crate 的 lib 测试在 CI 中
> **从未编译、从未运行**——而这几个 crate 正是业务逻辑与持久化所在层。
>
> **media::tests 已回归主门禁**（2026-09-14 §4 修复）：该套件 13 个用例此前存在
> **进程内串扰**（`--test-threads 1` 下仍随机失败，实测同一命令连跑 4 次得到
> 0 / 1 / 3 / 3 个失败且失败集漂移），被移出 blocking 门禁并设自我收回守卫。
> 根因是 `prepare_media_test_pool` 自建**部分 schema**（9 张表）且
> `search_path = <schema>, public`，所需表缺失时静默回退 `public` 导致外键违约。
> 已改为复用共享隔离池 `prepare_isolated_test_pool`（从 v11 baseline 克隆完整
> schema，包含 users / upload_progress / quarantined_media_changes 等全部依赖表）。
> 修复后连跑 3 次（`--test-threads 4`）全部通过，排除式与豁免守卫
> `scripts/ci/check_media_exemption_still_needed.sh` 已一并删除。
>
> 守卫 `tests/unit/ci_test_scope_tests.rs` 的两条断言现已**激活**（不再 `#[ignore]`），
> 锁住"每个 nextest 调用都必须声明作用域"与"lib 步骤必须 `--workspace` 或显式 `-p`"。

> ⚠️ **关于 `tests/e2e/` 的真实内容**（2026-09-11 核查）：
>
> 该目录**默认不验证任何端到端行为**，如实说明如下 ——
>
> - `e2e_scenarios.rs`：**零 I/O**。不引入 HTTP 客户端、数据库或存储；
>   每个函数只对硬编码的局部字面量断言重言式，例如
>   `let join_success = true; assert!(join_success);`。
>   即使房间创建/加入/登录/媒体上传/E2EE **完全损坏**，这 20 个用例仍会通过
>   （整个目标耗时 48 ms）。函数已改名为 `simulated_*` 并就地说明，
>   `tests/unit/e2e_honesty_tests.rs` 会阻止它悄悄获得真实 I/O 而不更新文档。
> - `user_flow_tests.rs`：**真正的 HTTP**（`reqwest`，`E2E_BASE_URL`），
>   但全部用例 `#[ignore]` 且需 `E2E_RUN=1` —— 属手动/夜间入口，不是 CI 门禁。
>
> 因此 **`e2e` 目标被接入 CI 的目的仅是防止编译腐坏**，不是行为验证。
> 会被 CI 判红的真实端到端覆盖在 `tests/integration/`（约 115 个模块，
> 走真实路由 + PostgreSQL）。
| 性能测试 | `tests/performance/*.rs` | ⚠️ **多为模拟，非真实基线**（见下方注） | 见 `compute_perf_gate.sh` |

> ⚠️ **关于 `tests/performance/`**（2026-09-11 核查）：
>
> 这些文件**不是**可据以判断性能的门禁，如实说明如下 ——
>
> - `query_performance_tests.rs`：**完全模拟**，不连数据库。其唯一断言是
>   `duration.as_millis() < 100`，而 `duration` 测量的是一个
>   `tokio::task::yield_now()` —— 它断言不了任何查询性能。
> - `api_load_tests.rs` / `manual_smoke_tests.rs`：不连数据库；
>   `manual_smoke_tests.rs` 的关键用例标了 `#[ignore]`。
> - `appservice_scheduler_perf_tests.rs`：确实连库并打印 p50/p95/p99，
>   但**全部用例 `#[ignore]`**，只做**报告**、不做断言。
>
> 结论：真实、可执行、会变红的性能门禁目前只有
> `scripts/ci/compute_perf_gate.sh`（纯计算）与
> `scripts/ci/sliding_sync_perf_gate.sh`（需 Postgres）。其余仅作人工参考。

---

## 二、测试门禁分层

### 2.1 门禁分类总表

| 分类 | 入口 | 作用 | 是否阻断发布 | 备注 |
|-----|------|------|-------------|------|
| 主门禁 | `cargo fmt --all -- --check` / `cargo clippy --all-features --locked -- -D warnings` / `cargo test --doc --locked` / `bash scripts/run_ci_tests.sh` / `cargo test --test unit --features test-utils placeholder_scan_tests` / `bash scripts/contract/check_route_contract.sh` | 保障格式、静态检查、文档测试、默认回归与仓库治理检查 | 是 | 当前发布判断应以 `.github/workflows/ci.yml` 的 blocking 路径为准 |
| 扩展验证 | `cargo test --test e2e -- --ignored --nocapture`、覆盖率、专项能力验证、Criterion 基准 | 补充用户路径、覆盖率与专项能力证据 | 否（默认） | 仅补充证据，不自动升级为“已实现并验证” |
| 手动分析 | `cargo test --features performance-tests --test performance_manual -- --nocapture` | 手动性能分析与人工观察 | 否 | 不计入常规发布门禁 |

### 2.2 分类规则

- 主门禁用于判断“当前提交是否具备基本发布条件”。
- 扩展验证用于补齐联邦、E2EE、AppService、Worker 等能力域的专项证据。
- 手动分析用于性能、压测、人工排查，不得直接替代主门禁。
- 任何测试结果若要支撑“已实现并验证”，必须明确对应入口、产物与适用范围。

### 2.3 当前入口归类

| 测试入口 | 分类 | 说明 |
|---------|------|------|
| `bash scripts/run_ci_tests.sh` | 主门禁 | 当前 CI 等价默认测试入口 |
| `cargo test --test unit --features test-utils placeholder_scan_tests` | 主门禁 | 阻断新增 shell route / 空成功响应回归 |
| `bash scripts/contract/check_route_contract.sh` | 主门禁 | 阻断新增未接线的导出路由 handler / router factory |
| `cargo test --test e2e -- --ignored --nocapture` | 扩展验证 | 真实流程需显式启用，默认不纳入自动主门禁 |
| `cargo test --test unit --features test-utils e2ee_api_tests` | 扩展验证 | 串联 `/_matrix/client/*/keys/changes`、经典 `/sync` 与 `sliding-sync` 的 E2EE 观察面组合门 |
| `cargo tarpaulin --output-dir coverage/ --html` | 扩展验证 | 提供覆盖率证据，不单独阻断发布 |
| `cargo bench --bench performance_api_benchmarks --no-run` | 扩展验证 | 性能专项基准 |
| `cargo bench --bench performance_federation_benchmarks --no-run` | 扩展验证 | 联邦性能专项基准 |
| `cargo test --features performance-tests --test performance_manual -- --nocapture` | 手动分析 | 手动性能套件 |

## 三、运行测试

### 3.1 所有测试

```bash
# CI 等价默认回归入口
bash scripts/run_ci_tests.sh
bash scripts/contract/check_route_contract.sh

# 仅单元测试
cargo test --test unit

# 仅集成测试
cargo test --test integration

# 仅集成测试（单线程，避免连接池竞争）
cargo test --test integration -- --test-threads=1

# 仅端到端测试（显式执行 ignored 用例）
cargo test --test e2e -- --ignored --nocapture

# E2EE 三观察面组合门
cargo test --test unit --features test-utils e2ee_api_tests
```

补充说明：

- `tests/e2e/mod.rs` 已接入独立测试入口 `e2e`
- `tests/unit/` 与 `tests/integration/` 的实际执行范围仍受各自 `mod.rs` 接线控制
- **已知问题**：部分集成测试在高并发时会因数据库连接池耗尽而失败，使用 `--test-threads=1` 或 `--test-threads=2` 可避免此问题。详见 `docs/INDEX.md` 第 3.1 节。
- `cargo test --test unit --features test-utils e2ee_api_tests` 会默认以 `TEST_ISOLATED_SCHEMAS=1` 顺序运行 3 条 `test_key_changes_*` 精确用例，以及 `test_sync_device_lists_`、`sliding_sync_extensions_e2ee_` 两组 composite regression，适合作为 nightly smoke 或本地回归入口
- `user_flow_tests.rs` 真实 HTTP 流程依赖运行中的服务与 `E2E_RUN=1`，当前应通过 `#[ignore]` + 显式执行方式运行，而不是默认早退后显示通过
- `tests/performance/mod.rs` 已拆分为手动性能测试入口 `performance_manual`，仅在显式启用 `--features performance-tests` 时执行
- Criterion 基准入口已拆分为 `performance_api_benchmarks` 与 `performance_federation_benchmarks`，对应 `benches/` 目录下的独立基准文件
- `performance_manual` 属于手动验证套件，不计入常规发布门禁

### 3.2 本地集成测试数据库建议

**快速启动（一键）:**

```bash
bash scripts/dev-test-setup.sh up
export TEST_DB_TEMPLATE_SCHEMA=public
SQLX_OFFLINE=true cargo test --features test-utils --test integration -- --test-threads=2
# 完成后: bash scripts/dev-test-setup.sh down
```

对于本地回归，优先使用已经迁移完成的 `public` schema 作为测试模板，而不是让每次测试都重新执行 strict schema 初始化。

推荐步骤：

```bash
# 先确认本地数据库已经迁移到当前代码期望的版本
bash docker/db_migrate.sh migrate
bash docker/db_migrate.sh validate

# 然后再执行定向集成测试，直接从 public schema 克隆测试 schema
TEST_DB_TEMPLATE_SCHEMA=public cargo test --locked --features test-utils --test integration -- --nocapture
```

补充说明：

- `TEST_DB_TEMPLATE_SCHEMA=public` 适合本地开发机与回归排查，可显著减少测试卡在 `DatabaseInitService::initialize()` strict 初始化阶段的概率。
- 使用该变量前，必须先执行 `bash docker/db_migrate.sh migrate`，确保 `public` schema 已经与当前代码所依赖的列/索引契约一致。
- 若只跑单条高价值回归测试，建议保持定向执行，例如：

```bash
TEST_DB_TEMPLATE_SCHEMA=public cargo test --locked --features test-utils --test integration test_create_dm_is_idempotent_for_same_pair -- --nocapture
TEST_DB_TEMPLATE_SCHEMA=public cargo test --locked --features test-utils --test integration test_create_friend_dm_reuses_existing_room -- --nocapture
```

- 若模板 schema 不可用，测试框架仍会退回到 isolated schema 初始化路径，但本地运行时间会明显增加。

### 2.2 代码覆盖率

```bash
# 安装 tarpaulin
cargo install cargo-tarpaulin

# 生成覆盖率报告
cargo tarpaulin --locked --out Html --out Json --output-dir coverage --lib

# 查看HTML报告
open coverage/tarpaulin-report.html
```

**覆盖率口径**：
- 团队质量目标：≥80%
- 当前自动门槛：`tarpaulin.toml` 中 `fail-under = 70`
- 2026-06-09 最新实测：`cargo tarpaulin --locked --out Json --output-dir coverage --lib` 为 `20.11%`（`10352/51472`）

### 2.3 性能基准测试

```bash
# 手动性能测试
cargo test --features performance-tests --test performance_manual -- --nocapture

# Criterion 基准
cargo bench --bench performance_api_benchmarks --no-run
cargo bench --bench performance_federation_benchmarks --no-run
```

发布门禁不应把 `performance_manual` 计入常规 `cargo test` 通过率；该入口属于手动性能套件。
GitHub Actions 中已将 Criterion 基准与 `performance_manual` 分离；后者通过 `Benchmark` 工作流的手动触发入口按需执行。

**性能质量门禁**（2026-09-11 重写 — 见下方说明）：

- **纯计算基准**：`bash scripts/ci/compute_perf_gate.sh`（CI 阻塞）
- **sliding sync 延迟**：`bash scripts/ci/sliding_sync_perf_gate.sh`（需 Postgres，CI 阻塞）
- 指标与阈值以这两个脚本内的实测基线为准，本文档不再单独声明数字。

> ⚠️ **为什么删掉了原来的 P95 数字**
>
> 本节此前声明三组目标（搜索 P95≤500ms、同步 P95≤1000ms、DB 查询 P95≤100ms）。
> 实测核查（`docs/audit/P4_performance_baseline_2026-09-11.md` §5.1）发现：
>
> 1. **没有任何执行者** —— 没有测试断言它们，没有 CI 步骤读取它们；
> 2. 代码里唯一存在的阈值是 `sliding_sync_perf_gate.sh` 的 **5000ms**，
>    与这里的 500/1000ms **口径完全不同**，且该脚本当时**从未被接线**；
> 3. 实测值比这些数字**好 15–30 倍**（whoami 1.95ms vs 阈值 20ms；
>    room 状态查询 3.15ms vs 阈值 50ms）—— 即便运行也拦不住任何现实规模的退化。
>
> 一份无人执行、且宽松到失去判别力的阈值表比没有更糟：它让人以为有保护。
> 现已替换为**真实可执行**的门禁，阈值定义在脚本内并附实测基线。

---

## 三、测试用例清单

### 3.1 单元测试 (12个文件)

| 文件 | 覆盖模块 | 测试数量 |
|-----|---------|---------|
| `auth_service_tests.rs` | 认证服务 | 8+ |
| `friend_service_tests.rs` | 好友服务 | 6+ |
| `search_service_tests.rs` | 搜索服务 | 5+ |
| `room_service_tests.rs` | 房间服务 | 7+ |
| `storage_tests.rs` | 存储层 | 10+ |
| 其他 | 各种服务 | 15+ |

### 3.2 集成测试（按能力域接线）

| 测试套件 | 测试项 | 当前定位 |
|---------|-------|---------|
| `api_admin_tests.rs` | 管理功能 | 已接线，作为管理域回归的一部分 |
| `api_device_presence_tests.rs` | 设备与在线状态 | 已接线，受数据库与并发资源影响 |
| `api_e2ee_tests.rs` | 端到端加密 | 已接线，需结合专项证据判断验证强度 |
| `api_enhanced_features_tests.rs` | **增强功能** | 已接线，属于补充能力验证 |
| `api_federation_tests.rs` | 联邦功能 | 已接线，需结合互操作专项证据理解 |
| `api_room_tests.rs` | **房间功能** | 已接线，仍需持续补齐契约断言 |
| `cache_tests.rs` | 缓存功能 | 已接线 |
| `concurrency_tests.rs` | 并发控制 | 已接线 |
| `metrics_tests.rs` | 指标收集 | 已接线 |
| `regex_cache_tests.rs` | 正则缓存 | 已接线 |

**关键功能测试覆盖**：
- 用户目录搜索：已有测试入口
- 事件举报：已有测试入口
- 房间状态管理：已有测试入口
- 成员事件查询：已有测试入口
- 邮箱验证：已有测试入口
- 好友系统：已有测试入口
- 是否可表述为“已实现并验证”，仍应以 `docs/INDEX.md` 与对应专项证据为准

### 3.3 端到端测试

| 测试文件 | 覆盖场景 | 接线状态 |
|---------|---------|---------|
| `e2e_scenarios.rs` | 模拟端到端场景编排 | 已接线 |
| `user_flow_tests.rs` | 完整用户注册→登录→使用流程 | 已接线，真实 HTTP 流程默认受 `E2E_RUN=1` 控制 |

### 3.4 互通测试（Complement）

| 测试文件 | 覆盖场景 | 接线状态 |
|---------|---------|---------|
| `tests/complement/main_test.go` | Docker 黑盒互通：注册→登录→同步→建房间→发事件→联邦密钥→服务发现→媒体上传/下载 | 扩展验证（需 Docker + Go） |

**运行入口**：

```bash
# 构建并运行全部互通用例
bash scripts/ci/run_complement_tests.sh

# 运行单个用例
bash scripts/ci/run_complement_tests.sh TestRegisterLogin
```

**前置条件**：本机 Docker 守护进程运行中、Go ≥ 1.22、首次运行会自动克隆 `matrix-org/complement` 到 `target/complement-src`。

**镜像契约**：`docker/complement/Dockerfile` 产物遵循 [Complement base image contract](https://github.com/matrix-org/complement/blob/main/docs/homeserver-design-overview.md)，监听 8008（client）与 8448（federation），`/start.sh` 负责生成签名密钥、写入 `homeserver.yaml`、应用迁移并启动 `synapse-rust`。

---

## 四、性能测试规范

### 4.1 基准测试位置

当前性能资产分为手动性能测试与 Criterion 基准两类：

- `mod.rs`、`api_load_tests.rs`、`query_performance_tests.rs` 组成手动性能测试入口 `performance_manual`
- `benches/performance_api_benchmarks.rs` 对应 Criterion 基准 `performance_api_benchmarks`
- `benches/performance_federation_benchmarks.rs` 对应 Criterion 基准 `performance_federation_benchmarks`

### 4.2 性能指标定义

| 指标 | 定义 | 门禁 |
|-----|------|---------|
| 纯计算基准均值 | Criterion `mean`（state resolution / auth chain / membership 转移） | `scripts/ci/compute_perf_gate.sh`（上限写在脚本内，约 10× 基线） |
| sliding sync P95 | 长轮询热路径的 p95 延迟 | `scripts/ci/sliding_sync_perf_gate.sh`（需 Postgres） |
| P95/P99 延迟 | 95%/99% 请求的响应时间 | ⬜ **未设门禁**：仅在采集到的基线上人工比对，见 `docs/audit/P4_performance_baseline_2026-09-11.md` §4 |
| 吞吐量 | 每秒处理的请求数 | ⬜ 未设门禁（同上） |
| 错误率 | 失败请求的比例 | ⬜ 未设门禁（同上） |

> 上表刻意区分"**有门禁**"与"**只有基线**"。只有基线的指标不会让 CI 变红 ——
> 不要把它们当成保护。给它们加门禁前，需要先有稳定的采集环境（见 §4.5 的
> 采样不确定性说明）。

### 4.3 性能测试场景

Criterion 基准清单及**可执行门禁**：

```text
benches/performance_federation_benchmarks.rs      ← compute_perf_gate.sh 覆盖
├── state_resolution_chain_10        (纯计算)
├── state_resolution_chain_100       (纯计算)
└── auth_chain_build_10              (纯计算)

benches/performance_membership_benchmarks.rs      ← compute_perf_gate.sh 覆盖
└── membership_transitions/*  (14 个状态机转移用例，纯计算)

benches/performance_api_benchmarks.rs             ← 需 homeserver + BENCH_ADMIN_TOKEN
├── server_versions / concurrent_load_versions/*
├── user_directory_search_* / room_* / sync_* / whoami
└── pagination_offset_deep_page, pagination_keyset_deep_page  (纯内存，CI 中真实运行)
    └── 由 .github/workflows/benchmark.yml 的 check_pagination_benchmark 步骤断言
        （keyset 相对 offset 收益 ≥30%）

benches/performance_sliding_sync_benchmarks.rs    ← sliding_sync_perf_gate.sh 覆盖（需 DB）
└── sliding_sync_p95_p99_latency
```

> 依赖 homeserver / `BENCH_ADMIN_TOKEN` 的基准在 CI 中会跳过；改用
> `BENCH_REQUIRE=<组名>` 可让"被请求的基准静默跳过"变成硬失败
> （详见 `benches/performance_api_benchmarks.rs` 头部注释）。

### 4.4 执行性能测试

```bash
# 安装依赖
cargo install cargo-criterion

# 运行完整性能测试
cargo criterion

# 生成性能报告
cargo criterion --output-file BENCHMARK RESULTS.md
```

---

## 五、持续集成测试

### 5.1 CI测试流程

当前以两个工作流为主：

- `.github/workflows/ci.yml`
  - `repo-sanity`：扫描私钥、危险制品、仓库异常文件与 shell route 回归
  - `test`：执行 `cargo fmt --all -- --check`、`cargo clippy --all-features --locked -- -D warnings`、doc test 与 `bash scripts/run_ci_tests.sh`
  - `security-audit`：执行 RustSec 审计
  - `build`：执行 release 构建
  - `coverage`：执行 tarpaulin 覆盖率（补充证据）
  - `quality-evidence`：收集测试与质量证据（non-blocking）
- `.github/workflows/benchmark.yml`
  - 运行 `performance_api_benchmarks` 与 `performance_federation_benchmarks`
  - 通过手动触发入口按需执行 `performance_manual`
- `.github/workflows/ci.yml`
  - `workflow_dispatch` 手动触发
  - 主要用于补充测试/覆盖率执行，不应视为默认主门禁

建议核验命令：

```bash
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml"); YAML.load_file(".github/workflows/benchmark.yml"); puts "workflow yaml: OK"'
cargo fmt --all -- --check
cargo clippy --all-features --locked -- -D warnings
cargo test --doc --locked
bash scripts/run_ci_tests.sh
cargo test --test unit --features test-utils placeholder_scan_tests
```

### 5.2 测试执行时间

| 测试类型 | 预计时间 | 说明 |
|---------|---------|------|
| 单元测试 | 视机器与缓存而定 | 以本地环境与依赖状态为准 |
| 集成测试 | 视数据库初始化与并发配置而定 | 受连接池、迁移和线程数影响明显 |
| Clippy 检查 | 视增量编译缓存而定 | 首次运行通常显著慢于增量运行 |
| **总计** | **不再给出固定承诺值** | 推荐以当前 CI 实测为准 |

---

## 六、回归测试策略

### 6.1 自动回归

每次代码提交默认应触发：
1. 格式检查
2. Clippy 静态分析
3. doc test
4. `bash scripts/run_ci_tests.sh` 覆盖的默认测试集
5. `cargo test --test unit --features test-utils placeholder_scan_tests` 的仓库治理检查

说明：
- “默认自动触发”应以 `.github/workflows/ci.yml` 为准；
- E2E、覆盖率、性能基准属于扩展验证或手动分析，不应在这里写成默认主门禁。

### 6.2 手动回归清单

| 功能模块 | 测试场景 | 预期结果 |
|---------|---------|---------|
| 用户认证 | 注册→登录→修改密码 | 全部成功 |
| 用户目录 | 搜索→列表→分页 | 功能性通过（延迟基线见 `docs/audit/P4_performance_baseline_2026-09-11.md` §4，**无门禁**） |
| 房间功能 | 创建→加入→发送消息 | 状态正确 |
| 好友系统 | 发送请求→接受→删除 | 状态同步 |
| 事件举报 | 提交举报→更新分数 | 数据正确 |

### 6.3 回归测试周期

| 周期 | 触发条件 | 执行者 |
|-----|---------|-------|
| 提交时 | 代码提交 | CI自动 |
| 每日 | 每日构建 | CI自动 |
| 发布前 | 版本发布 | 人工+CI |

---

## 七、质量门禁标准

### 7.1 第一阶段（安全加固）

| 标准 | 要求 | 当前状态 |
|-----|------|---------|
| 安全测试 | 无高危/中危漏洞 | ✅ 通过 |
| 代码审查 | 100%通过 | ✅ 通过 |
| 密码验证 | 完整策略检查 | ✅ 通过 |

### 7.2 第二阶段（能力收敛）

| 标准 | 要求 | 当前状态 |
|-----|------|---------|
| 自动化测试 | 主门禁可重复执行，结果与 CI 语义一致 | 持续收敛中，以当前 CI 为准 |
| 性能测试 | 有专项入口与基线，不混入默认主门禁 | ⚠️ 需按需执行 |
| 代码覆盖率 | 作为补充证据，不单独替代发布判断 | ⚠️ 非默认阻断项 |

### 7.3 第三阶段（发布前补证）

| 标准 | 要求 | 当前状态 |
|-----|------|---------|
| 兼容性测试 | 关键能力域具备专项证据或互操作证明 | 持续收敛中 |
| 文档完整性 | 对外入口回指权威基线，无状态漂移 | 持续收口中 |
| 端到端测试 | 显式启用后真实执行，不允许早退假绿 | 已改为默认忽略、显式启用 |

---

## 八、缺陷跟踪

### 8.1 缺陷严重程度

| 级别 | 定义 | 响应时间 |
|-----|------|---------|
| P0 - 阻塞 | 系统不可用 | 立即修复 |
| P1 - 严重 | 核心功能失败 | 24小时 |
| P2 - 中等 | 非核心功能失败 | 1周 |
| P3 - 轻微 | 文档/优化建议 | 排期修复 |

### 8.2 测试结果报告

每次测试执行后建议生成：

```
测试执行报告
================
日期: <实际执行日期>
提交: <实际提交>
分支: <实际分支>

主门禁:
- cargo fmt --all -- --check: <结果>
- cargo clippy --all-features --locked -- -D warnings: <结果>
- cargo test --doc --locked: <结果>
- bash scripts/run_ci_tests.sh: <结果>
- cargo test --test unit --features test-utils placeholder_scan_tests: <结果>

扩展验证:
- cargo test --test e2e -- --ignored --nocapture: <是否执行 / 结果 / 前置条件>
- coverage / benchmark / 专项能力验证: <是否执行 / 结果>

发现缺陷: <数量与摘要>
回归问题: <数量与摘要>
```

---

## 九、测试环境

### 9.1 本地测试

```bash
# 启动PostgreSQL
docker run -d --name synapse_postgres \
  -e POSTGRES_USER=synapse \
  -e POSTGRES_PASSWORD=synapse \
  -e POSTGRES_DB=synapse_test \
  -p 5432:5432 \
  postgres:16

# 设置环境变量
export DATABASE_URL="postgres://synapse:synapse@localhost:5432/synapse_test"

# 运行测试
cargo test --test integration
```

### 9.1.1 ⚠️ 本地测试库必须定期清理（否则 PostgreSQL 会变成小时级不可用）

测试夹具每个用例建一个隔离 schema。**2026-09-12 实测本地库累积到 23,662 个
残留 schema**，后果远比"catalog 变慢"严重：

| 现象 | 实测 |
|---|---|
| 单个 `DROP SCHEMA ... CASCADE` | 撞 `max_locks_per_transaction=256`，报 `out of shared memory`（模板 schema 有 1,197 个对象） |
| 单个普通 schema 的 `DROP` | 约 3.0 秒 |
| `pg_database_size()` | 超时 |
| **PostgreSQL 崩溃恢复的 pre-fsync** | **>60 分钟未完成**，日志刷 `syncing data directory (pre-fsync), elapsed time: NNNN s` |
| 单条测试用例 | 133 秒（catalog 缩小 8% 后同用例 162s→108s） |

根因不只是 catalog 膨胀，而是**数据目录的文件数膨胀**：21,778 个 schema ×
(658–1,229 个对象 + 索引 + 序列) ≈ **数百万个文件**。PostgreSQL 硬崩溃恢复的
`SyncDataDirectory()` 对**每一个文件**单独 `pg_fsync`，实测约 10 文件/秒。

**所以：任何 PostgreSQL 重启（含正常维护重启）都可能变成小时级事件。**

```bash
# 定期清理（默认是预演，不会改动任何东西）
bash scripts/cleanup_test_schemas.sh

# 确认目标无误后执行
bash scripts/cleanup_test_schemas.sh --apply
```

> 清理脚本覆盖 `test_*` / `media_test_*` / `synapse_test_*` 三个家族以及**被新
> 迁移指纹取代的陈旧模板**（迁移文件一改，模板名就变，旧模板永不删除——这部分
> 现已由 `init_template_schema` 自动剪枝）。
>
> **如果残留已经很多，不要逐个 `DROP SCHEMA`**：实测约 3 秒/个，2 万个要跑十几
> 小时。直接重建测试库（`DROP DATABASE` + `CREATE DATABASE` + 重放迁移）是秒级
> 的，且能一次性绕过 `max_locks_per_transaction` 限制。
>
> 完整报告与实测原始输出：`docs/audit/P5_test_schema_accumulation_2026-09-12.md`。

#### ⚠️ 目前**没有**自动回收机制，泄漏仍在持续

2026-09-12 在独立临时集群上实测（干净库，跑 14 个 `oidc_session_storage` 用例）：

| 轮次 | `test_*` schema 数 |
|---|---|
| 基线 | 0 |
| 第 1 次运行 | **8** |
| 第 2 次运行 | **16** |

**每轮 +8，跨轮线性增长，永不回收。**

代码里看起来存在三套"drop-on-release 登记 + sweep"机制
（`synapse-test-utils/src/lib.rs`、`synapse-services/src/test_utils.rs`、
`synapse-storage/src/test_utils.rs`），但它们**对本场景无效**：

* `sweep` 的触发时机是"**下一次**取池时"；
* 而 nextest **一个用例一个进程** —— 进程取一次池、建一个 schema、然后退出，
  **"下一次取池"永远不会发生**；
* 登记表是**进程内** static，sweep 又不在进程退出时执行，于是随进程一起消失。

**因此不要依赖代码自动回收，必须定期跑上面的清理脚本（或重建测试库）。**
正确修法（未实现）是让 `prepare_empty_isolated_test_pool` 返回一个持有 schema 名
的**守卫对象**由调用方 drop（即 `synapse-storage/src/test_isolation.rs` 里
`IsolatedTestPool` 已被验证有效的形状：`spawn` + **join**），或让这批用例改走共享
模板夹具。详见报告 §9（含"为什么两种看起来合理的修法都失败"的记录：
按进程稳定命名只把泄漏从"每次调用"降到"每进程"；而 `static` + `impl Drop` 是死代码
——**Rust 不会 drop 文件级 static**）。

### 9.2 CI测试环境

- **操作系统**: Ubuntu 22.04 LTS
- **Rust版本**: 1.75+
- **PostgreSQL**: 16
- **Redis**: 7.0+

---

## 十、相关文档

- [工程收口计划](docs/API-OPTION/engineering-optimization-plan.md)
- [API错误文档](docs/api-error.md)
- [安全审计文档](docs/security-audit.md)
- [部署运维手册](docs/synapse-rust/DEPLOYMENT_GUIDE.md)

---

## 当前约束

- 不再在本文件中给出“457/457”“100% 通过”“E2E 默认已通过”这类脱离当前证据的静态结论
- 当前测试状态、验证强度与发布判断，必须回指 `docs/INDEX.md` 和 `docs/INDEX.md`

## 修订历史

| 版本 | 日期 | 修改内容 | 作者 |
|-----|------|---------|------|
| 1.0 | 2024-01-15 | 初始版本 | Synapse Rust Team |
| 1.1 | 2024-01-20 | 添加性能测试规范 | Synapse Rust Team |
