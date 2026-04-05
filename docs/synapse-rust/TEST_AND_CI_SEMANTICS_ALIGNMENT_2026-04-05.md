# 测试与 CI 语义对齐说明（2026-04-05）

> 文档定位：执行语义说明
> 权威工作流：`.github/workflows/ci.yml`

---

## 1. 主结论

当前仓库的默认主门禁以 `.github/workflows/ci.yml` 为准，而不是以历史文档中的笼统“cargo test 全绿”描述为准。

默认应按以下语义理解：

- **主门禁（blocking）**：决定当前提交是否通过核心质量检查；
- **扩展验证（non-default / supplemental）**：提供补充证据，但默认不单独决定发布结论；
- **手动分析（manual / advisory）**：用于人工观察、性能分析或补充诊断。

---

## 2. 当前主门禁

### 2.1 阻断路径

`.github/workflows/ci.yml` 当前主门禁重点包括：

- `repo-sanity`
  - 私钥/危险文件检查
  - shell route 检测
- `test`
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-features --locked -- -D warnings`
  - `cargo test --doc --locked`
  - `bash scripts/run_ci_tests.sh`

这意味着：

- 文档中应以 `bash scripts/run_ci_tests.sh` 代表 CI 等价测试入口；
- 不应再把 `cargo test --all-features --locked -- --test-threads=4` 单独表述成唯一主门禁事实。

### 2.2 非阻断路径

`.github/workflows/ci.yml` 中以下结果默认不应被表述为主发布门禁：

- `coverage`
- `quality-evidence`

其中 `quality-evidence` 已明确为 non-blocking。

---

## 3. `.github/workflows/test.yml` 的定位

`test.yml` 当前是 `workflow_dispatch` 手动触发，不应被描述为默认主 CI。

因此：

- 可以作为补充验证、覆盖率或人工触发入口；
- 不应在 README / TESTING 中与 `ci.yml` 并列成“同等主门禁”。

---

## 4. 测试分类口径

### 4.1 主门禁

适用于当前提交的默认质量判断：

- 格式检查
- clippy
- doc test
- `scripts/run_ci_tests.sh` 覆盖的默认测试路径
- repo-sanity 中的静态治理检查

### 4.2 扩展验证

适用于能力补证，但默认不直接等于“主门禁通过”：

- E2E
- 覆盖率
- Criterion benchmark
- 特定能力域专项验证

### 4.3 手动分析

适用于按需执行：

- 性能手动测试
- 需要外部环境/服务的人工排查
- 非默认 workflow_dispatch 分析任务

---

## 5. 当前需要特别注意的语义风险

### E2E

- 真实 HTTP 流程依赖 `E2E_RUN=1` 与运行中的服务；
- 因此 E2E 不能在未满足条件时被计作“已通过真实验证”；
- 更合适的语义是：默认忽略，显式启用。

### Integration

- 集成测试依赖隔离数据库初始化；
- 本地环境若初始化失败，当前部分测试会跳过；
- CI / `INTEGRATION_TESTS_REQUIRED` 环境下则应视为失败，而不是静默放过。

### Evidence jobs

- 证据收集 job 的作用是补充说明，而不是自动升级能力结论；
- 只有当证据被纳入权威基线后，才可改变对外状态表述。

---

## 6. 推荐本地核验入口

### 主门禁等价核验

```bash
cargo fmt --all -- --check
cargo clippy --all-features --locked -- -D warnings
cargo test --doc --locked
bash scripts/run_ci_tests.sh
bash scripts/detect_shell_routes.sh
```

### 扩展验证

```bash
cargo test --test e2e -- --ignored --nocapture
cargo tarpaulin --output-dir coverage/ --html
cargo bench --bench performance_api_benchmarks --no-run
cargo bench --bench performance_federation_benchmarks --no-run
```

### 手动分析

```bash
cargo test --features performance-tests --test performance_manual -- --nocapture
```

---

## 7. 文档使用规则

今后如果在 README、阶段报告或其他文档中描述测试/CI：

- 必须先区分 blocking / non-blocking / manual；
- 必须标注环境条件；
- 不得把 ignored、skip、手动触发写成默认自动通过。

这份文档用于统一后续所有测试与 CI 口径。
