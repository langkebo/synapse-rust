# Synapse Rust 测试策略与质量保证

本文档描述 synapse-rust 项目的测试策略、质量标准和执行规范。当前正式能力口径请以 `docs/synapse-rust/CAPABILITY_STATUS_BASELINE_2026-04-02.md` 为准；测试与 CI 语义收口请同时参考 `docs/synapse-rust/TEST_AND_CI_SEMANTICS_ALIGNMENT_2026-04-05.md` 与 `docs/synapse-rust/FALSE_GREEN_AND_PLACEHOLDER_GOVERNANCE_2026-04-05.md`。

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
| 单元测试 | `tests/unit/*.rs` | 验证独立组件逻辑 | ≥80% |
| 集成测试 | `tests/integration/*.rs` | 验证 API 完整流程与高风险契约 | 主链与高风险能力域必覆盖 |
| 端到端测试 | `tests/e2e/*.rs` | 模拟真实用户操作 | 关键路径 |
| 性能测试 | `tests/performance/*.rs` | 验证性能指标 | P95≤500ms |

---

## 二、测试门禁分层

### 2.1 门禁分类总表

| 分类 | 入口 | 作用 | 是否阻断发布 | 备注 |
|-----|------|------|-------------|------|
| 主门禁 | `cargo fmt --all -- --check` / `cargo clippy --all-features --locked -- -D warnings` / `cargo test --doc --locked` / `bash scripts/run_ci_tests.sh` / `bash scripts/detect_shell_routes.sh` / `bash scripts/detect_unwired_route_candidates.sh` | 保障格式、静态检查、文档测试、默认回归与仓库治理检查 | 是 | 当前发布判断应以 `.github/workflows/ci.yml` 的 blocking 路径为准 |
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
| `bash scripts/detect_shell_routes.sh` | 主门禁 | 阻断新增 shell route / 空成功响应回归 |
| `bash scripts/detect_unwired_route_candidates.sh` | 主门禁 | 阻断新增未接线的导出路由 handler / router factory |
| `cargo test --test e2e -- --ignored --nocapture` | 扩展验证 | 真实流程需显式启用，默认不纳入自动主门禁 |
| `cargo tarpaulin --output-dir coverage/ --html` | 扩展验证 | 提供覆盖率证据，不单独阻断发布 |
| `cargo bench --bench performance_api_benchmarks --no-run` | 扩展验证 | 性能专项基准 |
| `cargo bench --bench performance_federation_benchmarks --no-run` | 扩展验证 | 联邦性能专项基准 |
| `cargo test --features performance-tests --test performance_manual -- --nocapture` | 手动分析 | 手动性能套件 |

## 三、运行测试

### 3.1 所有测试

```bash
# CI 等价默认回归入口
bash scripts/run_ci_tests.sh
bash scripts/detect_unwired_route_candidates.sh

# 仅单元测试
cargo test --test unit

# 仅集成测试
cargo test --test integration

# 仅集成测试（单线程，避免连接池竞争）
cargo test --test integration -- --test-threads=1

# 仅端到端测试（显式执行 ignored 用例）
cargo test --test e2e -- --ignored --nocapture
```

补充说明：

- `tests/e2e/mod.rs` 已接入独立测试入口 `e2e`
- `tests/unit/` 与 `tests/integration/` 的实际执行范围仍受各自 `mod.rs` 接线控制
- **已知问题**：部分集成测试在高并发时会因数据库连接池耗尽而失败，使用 `--test-threads=1` 或 `--test-threads=2` 可避免此问题。详见 `docs/synapse-rust/FALSE_GREEN_AND_PLACEHOLDER_GOVERNANCE_2026-04-05.md` 第 3.1 节。
- `user_flow_tests.rs` 真实 HTTP 流程依赖运行中的服务与 `E2E_RUN=1`，当前应通过 `#[ignore]` + 显式执行方式运行，而不是默认早退后显示通过
- `tests/performance/mod.rs` 已拆分为手动性能测试入口 `performance_manual`，仅在显式启用 `--features performance-tests` 时执行
- Criterion 基准入口已拆分为 `performance_api_benchmarks` 与 `performance_federation_benchmarks`，对应 `benches/` 目录下的独立基准文件
- `performance_manual` 属于手动验证套件，不计入常规发布门禁

### 2.2 代码覆盖率

```bash
# 安装 tarpaulin
cargo install cargo-tarpaulin

# 生成覆盖率报告
cargo tarpaulin --output-dir coverage/ --html

# 查看HTML报告
open coverage/tarpaulin-report.html
```

**覆盖率质量门禁**：≥80%

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

**性能质量门禁**：
- 搜索API P95延迟：≤500ms
- 同步请求 P95延迟：≤1000ms
- 数据库查询 P95延迟：≤100ms

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
- 是否可表述为“已实现并验证”，仍应以 `CAPABILITY_STATUS_BASELINE_2026-04-02.md` 与对应专项证据为准

### 3.3 端到端测试

| 测试文件 | 覆盖场景 | 接线状态 |
|---------|---------|---------|
| `e2e_scenarios.rs` | 模拟端到端场景编排 | 已接线 |
| `user_flow_tests.rs` | 完整用户注册→登录→使用流程 | 已接线，真实 HTTP 流程默认受 `E2E_RUN=1` 控制 |

---

## 四、性能测试规范

### 4.1 基准测试位置

当前性能资产分为手动性能测试与 Criterion 基准两类：

- `mod.rs`、`api_load_tests.rs`、`query_performance_tests.rs` 组成手动性能测试入口 `performance_manual`
- `benches/performance_api_benchmarks.rs` 对应 Criterion 基准 `performance_api_benchmarks`
- `benches/performance_federation_benchmarks.rs` 对应 Criterion 基准 `performance_federation_benchmarks`

### 4.2 性能指标定义

| 指标 | 定义 | 质量门禁 |
|-----|------|---------|
| P95延迟 | 95%请求的响应时间 | ≤500ms |
| P99延迟 | 99%请求的响应时间 | ≤1000ms |
| 吞吐量 | 每秒处理的请求数 | ≥100 RPS |
| 错误率 | 失败请求的比例 | ≤1% |

### 4.3 性能测试场景

```rust
// 用户目录搜索性能
benchmark_user_directory_search
├── 单用户搜索 → P95 ≤100ms
└── 批量搜索(10并发) → P95 ≤500ms

// 房间操作性能
benchmark_room_operations
├── 状态查询 → P95 ≤50ms
└── 成员列表 → P95 ≤100ms

// 同步操作性能
benchmark_sync_operations
├── 带超时同步 → P95 ≤500ms
└── 快速同步 → P95 ≤200ms

// 认证操作性能
benchmark_auth_operations
└── Whoami查询 → P95 ≤20ms
```

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
- `.github/workflows/test.yml`
  - `workflow_dispatch` 手动触发
  - 主要用于补充测试/覆盖率执行，不应视为默认主门禁

建议核验命令：

```bash
ruby -e 'require "yaml"; YAML.load_file(".github/workflows/ci.yml"); YAML.load_file(".github/workflows/benchmark.yml"); puts "workflow yaml: OK"'
cargo fmt --all -- --check
cargo clippy --all-features --locked -- -D warnings
cargo test --doc --locked
bash scripts/run_ci_tests.sh
bash scripts/detect_shell_routes.sh
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
5. `bash scripts/detect_shell_routes.sh` 的仓库治理检查

说明：
- “默认自动触发”应以 `.github/workflows/ci.yml` 为准；
- E2E、覆盖率、性能基准属于扩展验证或手动分析，不应在这里写成默认主门禁。

### 6.2 手动回归清单

| 功能模块 | 测试场景 | 预期结果 |
|---------|---------|---------|
| 用户认证 | 注册→登录→修改密码 | 全部成功 |
| 用户目录 | 搜索→列表→分页 | 响应≤500ms |
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
- bash scripts/detect_shell_routes.sh: <结果>

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
- 当前测试状态、验证强度与发布判断，必须回指 `docs/synapse-rust/CAPABILITY_STATUS_BASELINE_2026-04-02.md` 和 `docs/synapse-rust/TEST_AND_CI_SEMANTICS_ALIGNMENT_2026-04-05.md`

## 修订历史

| 版本 | 日期 | 修改内容 | 作者 |
|-----|------|---------|------|
| 1.0 | 2024-01-15 | 初始版本 | Synapse Rust Team |
| 1.1 | 2024-01-20 | 添加性能测试规范 | Synapse Rust Team |
