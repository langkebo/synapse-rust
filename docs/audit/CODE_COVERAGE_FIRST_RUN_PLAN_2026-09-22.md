# 让 Code Coverage 第一次真正跑起来 —— 详细方案

> 目标文件：`docs/audit/GATE_INTEGRITY_FOLLOWUP_2026-09-19.md` §1 P0-1（唯一从未执行过的门禁）。
> 本文只写**方案**，不含实施改动。数字均为 2026-09-22 实测。

## 0. 完成判据（DoD）

1. 某次真 CI run 里 `Code Coverage` job **结论为 success**，日志里能看到
   `Run coverage` → `Per-file coverage ratchet` → artifact 三段的真实输出；
2. `run_slow_tier=true` 的 dispatch 里，**慢速车道不再被 skipped**（哨兵已能判定这一点）；
3. `scripts/ci/coverage_baseline.json` 是**用 CI 实际执行的那条命令**产出的（口径单一可复现）；
4. 把该 run id、行覆盖率、逐文件判定摘要写回 P0-1 行，并把 P0-1 标 ✅。

## 1. 现状：为什么它 8 次都没跑到

真 CI 里 coverage 的依赖链是 `coverage: needs [integration-test, changes]`，而
`integration-test: needs [test, changes]`。最近 12 个 run 的实测：

| run | 事件 | 结论 | Integration | Coverage |
|---|---|---|---|---|
| 35683146324 | workflow_dispatch（run_slow_tier=true） | failure | **skipped** | **skipped** |
| 35682869420 | pull_request | failure | skipped | skipped |
| 35599998883 | push | failure | **failure** | skipped |
| 35588897665 | push | failure | **failure** | skipped |

两个不同的前置红，任一都会让 coverage 永不启动：

- **红因 A（当前）**：`Test & Lint` 的两条 **default-features** 车道红在
  `Check metric instrumentation reachability` → fast tier 不过 → 慢速车道整体 skipped。
- **红因 B（历史）**：`Integration Tests` 自己红 —— 最近一次（35599998883）除
  `Run performance smoke gate` 外每一步都绿，而那个 perf smoke 缺陷已在 `3d5fd87a` 修掉，
  **但此后没有任何一次 run 真正执行到 integration**（fast tier 一直红）。

### 1.1 红因 A 的根因（实测，不是"基线过期"）

CI 日志（job `Test & Lint (1.93.0, default-features)`，run 35696025065）：

```
跳过 25 个同名冲突方法（无法安全判定）：http_request_finished, …, update_pool_metrics
埋点方法：25 个（可判定 0 个）
FAIL 基线已过期（对应埋点已接通或已删除，基线必须收紧）
已接通：0    未接通：0    基线：15
```

根因：`scripts/ci/check_metric_instrumentation.py` 把 **PCRE 风格** `\b` / `\s` 用在
`git grep -E` 模式里（`:222` 定义扫描、`:304` 调用点扫描），而 `git grep -E` 走的是各平台的
**POSIX ERE 引擎**，两边行为不同：

| 环境 | `git grep -E '\bfn\s+record_auth_attempt\s*[(<]'` | 后果 |
|---|---|---|
| macOS（本机 git/BSD） | **无命中** | `find_ambiguous_names` 得到空集 ⇒ 门禁"能工作"（假绿：同名冲突检查其实没生效） |
| Linux（CI/glibc） | **命中 `synapse-common/src/server_metrics.rs:360` 等定义** | 25 个埋点方法全被判为"同名冲突"⇒ 可判定 0 个 ⇒ 棘轮的 stale 规则判定"基线全部过期"⇒ **FAIL（假红）** |

本机对照（同一仓、同一命令）：

```bash
$ git grep -n -E '\bfn\s+record_auth_attempt\s*[(<]' -- '*.rs'      # 无输出
$ git grep -n -E 'fn[[:space:]]+record_auth_attempt[[:space:]]*[(<]' -- '*.rs'
synapse-common/src/server_metrics.rs:360:    pub fn record_auth_attempt(&self, success: bool) {
```

深层缺陷：`find_ambiguous_names` **没有排除定义文件本身**（`SERVER_METRICS_SRC`）。
一旦正则真的生效，`impl ServerMetrics` 自己的 `pub fn X(` 就把每个 `X` 标成"同名冲突"。
"同名冲突"的正确语义应是"**另有其它类型**定义了同名方法"。

## 2. 分阶段方案

### Stage 0 —— 修掉红因 A（唯一已知的 fast tier 红，约 1h）

改 `scripts/ci/check_metric_instrumentation.py`：

1. `:222` 定义扫描改 POSIX 字符类，并排除定义文件：
   `(^|[^[:alnum:]_])fn[[:space:]]+X[[:space:]]*[(<]`；
   `find_ambiguous_names` 里跳过 `rel == SERVER_METRICS_SRC.relative_to(ROOT)`。
2. `:304` 调用点扫描同样 POSIX 化：`\.[[:space:]]*X[[:space:]]*\(`。
3. 新增 `--print-ambiguous`（只打印歧义集合，不判定），让语义可被断言。
4. **自证（铁律 8）**：
   - 行为断言：`record_auth_attempt` **不在** `--print-ambiguous` 输出里（它只在
     `ServerMetrics` 上定义）；构造一个临时 fixture（或在真实树里找 `record_failure`
     这类跨类型重名）断言**在**输出里。
   - 平台断言（防复发）：断言脚本里 `git grep` 用到的 pattern **不含** `\s` / `\b`
     （PCRE-ism）；红证明 = 把 `\s` 加回 `:222` → 测试 FAILED。
5. 修完本地应仍是 `通过（未接通集合与基线一致）`；CI 上两条 default-features 车道应从
   "25 个全跳过"变成 `已接通 10 / 未接通 15` ⇒ 绿。
6. 基线尾部的"同名冲突"注释块目前是**空的**（因为本地扫描从未命中）——修好后跑
   `--update` 把它写成真实集合（纯注释，不影响判定）。

**这一步同时解掉当前每个 PR 的两条红车道**，所以它排在覆盖率之前。

### Stage 1 —— 把"覆盖率命令"收敛成一份实现（约 1h）

现状是**两份实现**（铁律 2），且已经不同：

| | `ci.yml::Run coverage` | `scripts/run_local_coverage.sh` |
|---|---|---|
| storage 步骤 feature | `test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,cas-sso,saml-sso`（8） | 同 8 个 **+ `external-services,builtin-oidc`**（10） |
| 其余步骤 | `cargo llvm-cov --workspace --exclude synapse-storage --features "$REST_FEATURES" -- --skip ledger_export_tests` | 相同 |
| 线程/清理 | 默认线程、无清理 | `TEST_THREADS=4`、跑完清理 schema |

实测 `synapse-storage/src` **完全不使用** `external-services` / `builtin-oidc`
（`grep -rn 'external-services\|builtin-oidc' synapse-storage/src` 为空），所以这 8/10 的差异
**不影响覆盖率数字**；但两份实现本身必须收敛：

1. 新建 `scripts/ci/run_coverage.sh`：把 CI 现在那段命令**逐字**搬进去（两步 llvm-cov + merge），
   env/前置自检（`TEST_DB_TEMPLATE_SCHEMA` 表数 >100，否则响亮失败）。
2. `ci.yml` 的 `Run coverage` 只调用该脚本；本地 `run_local_coverage.sh` 变薄封装
   （设 `TEST_THREADS`、清理、再调同一脚本）或直接删除。
3. 守卫（照 `ci_backend_validation_runs_the_ci_batches_verbatim` 的写法）：断言
   `ci.yml` 里不再出现 `cargo llvm-cov` 字面量，且脚本内的命令与文档口径一致。
   红证明：把 feature 列表改一处 → FAILED。
4. 钉版本：`cargo install cargo-llvm-cov --locked --version 0.8.7`
   （本机实测 0.8.7；不钉 ⇒ 门禁结论随安装日期变化，与 hadolint/ruff/trivy 同一判据）。

### Stage 2 —— 本地按 CI 口径干跑，先量化基线风险（35–50 min，不花 CI 分钟）

基线是 2026-09-19 用**本地脚本**（macOS、10-feature storage 步）产出的；CI 是 Linux + 8-feature。
棘轮是**单调**的（`save_baseline` 取 `max(prev, cur)`），所以"先跑 CI 再说"一旦报红，
基线不会自动下调，会一直红到有人显式改文件。因此先本地量化：

```bash
bash scripts/ci/prepare_test_db.sh                 # public + test_template_ci
bash scripts/ci/run_coverage.sh                    # → coverage/lcov.info（CI 口径）
cp scripts/ci/coverage_baseline.json /tmp/cov-baseline.probe.json   # 副本，避免探针改写仓库文件
python3 scripts/check_file_coverage.py --report coverage/lcov.info \
  --baseline /tmp/cov-baseline.probe.json --global-floor 40 --new-file-floor 30 \
  --core-files scripts/ci/core_file_coverage_prefixes.txt --core-threshold 70 \
  --non-unit-coverable scripts/ci/non_unit_coverable_prefixes.txt
```

判定与处置：

- `[TOUCHED] 文件 < 基线` 的清单 → 分成三类：**真实回退**（修代码/补测试）、
  **平台差异**（`cfg(target_os)` 类，需在基线说明里登记）、**基线过时**（provenance 不符）。
- 只有第三类才允许重置基线，且必须 `--save-baseline` **单独提交**，逐项写明为什么允许下调。
- 预期风险**不高**：storage 的 feature 差异已证明无影响；其余步骤两边 feature 集相同。
  剩下的变量是平台与线程数（CI 默认线程 ≈ 4 vCPU）。

### Stage 3 —— 修掉两个"首次运行必然踩"的坑（约 30 min）

1. **Codecov**：仓库是 public，但 `gh secret list --json` 返回 `[]`（**没有任何 repo secret**），
   而该步骤是 `fail_ci_if_error: true` —— tokenless 上传一旦失败，job 会在棘轮**已经通过之后**红。
   二选一：(a) 配 `CODECOV_TOKEN`；(b) 改 `fail_ci_if_error: false` 并注明
   "门禁是 per-file 棘轮，Codecov 只做可视化"。建议 (b)（可选再配 token）。
2. **版本钉死**（同 Stage 1.4）。
3. **明确基线归属**：`save_baseline` 每次都会写回 `--baseline`，所以 main push 上
   `Commit coverage baseline (auto-ratchet)` 确实会自动收紧；branch dispatch 上不提交（无害）。
   要写清"首跑若需要收紧/重置，由谁在哪个分支提交"。

### Stage 4 —— 第一次真跑与逐段排障（1–2h，含 40–60 min 等待）

```bash
gh workflow run ci.yml --ref <branch> -f run_slow_tier=true
gh run watch <run-id>          # 关注 Integration Tests → Code Coverage
```

逐段失败特征 → 处置：

| 失败点 | 特征 | 处置 |
|---|---|---|
| `Set up test database` | `prepare_test_db.sh` 报错 | 它是 integration job 同款步骤，先在 integration 上复现 |
| `Run coverage` | `53200 out of shared memory` | 降并发（`RUST_TEST_THREADS=4`）或 `CLONE_TABLES_PER_STATEMENT` 24→12 |
| `Run coverage` | llvm-cov 编译/运行超时 | 拆步骤或提高 `timeout-minutes` |
| `Per-file coverage ratchet` | `[TOUCHED] …` 清单 | 回到 Stage 2 的三分类；真实回退必须修 |
| 同上 | `Stale non-unit-coverable prefixes` exit 2 | 清单里的前缀在磁盘上没有对应文件 → 修清单 |
| `Upload coverage to Codecov` | 上传失败 | Stage 3.1 的策略 |
| 全绿但 artifact 空 | `coverage-report` 缺失 | 检查 `coverage/lcov.info` 是否真的生成（`Run coverage` 末尾 merge） |

### Stage 5 —— 让它不再"连续 8 次没跑到"（约 30 min）

- `ci-summary` 的哨兵已经实现"事件要求慢速车道且 fast tier 绿 ⇒ 慢速车道不得 skipped"
  （守卫 `ci_summary_sentinel_requires_the_slow_tier_to_have_run` 已存在且通过）。
- 补一条 step 级守卫：`ci.yml` 的 coverage 步骤必须带
  `--non-unit-coverable scripts/ci/non_unit_coverable_prefixes.txt`、`--format lcov`、
  基线路径为 `scripts/ci/coverage_baseline.json`；红证明：删掉 `--non-unit-coverable` → FAILED。

## 3. 三个决策点 —— 已裁定（2026-09-22，用户）

1. **Codecov = 可视化**：`fail_ci_if_error: false`（门禁是 per-file 棘轮，Codecov 只做可视化）。
   已落地，守卫 `coverage_job_cannot_be_blocked_by_codecov_and_pins_llvm_cov` 钉住
   （红证明：改回 `true` → FAILED）。
2. **唯一实现**：`scripts/ci/run_coverage.sh`；本地 `scripts/run_local_coverage.sh` **删除**。
   已落地，守卫 `coverage_command_has_a_single_implementation` 钉住
   （红证明：把 `cargo llvm-cov` 写回 `ci.yml`、或重建那份本地脚本 → FAILED）。
3. **基线重置已授权**：Stage 2 若判定某些文件的 floor 属于"本地口径产物"，
   允许用 CI 口径重算并**下调**这些条目 —— 逐项写明理由，单独提交。

## 4. 工时与 CI 预算

| 阶段 | 工时 | 备注 |
|---|---|---|
| 0 修 metric 门禁 | 1h | 顺带解掉当前每个 PR 的两条红车道 |
| 1 覆盖率命令收敛 + 钉版本 | 1h | 含守卫与红证明 |
| 2 本地干跑 + 基线判定 | 1h | 其中 35–50 min 是本机执行 |
| 3 Codecov / 版本 / 基线归属 | 30 min | 需决策 1 |
| 4 首次真跑 + 排障 | 1–2h | CI 等待 40–60 min |
| 5 step 级守卫 | 30 min | |

每次慢速车道 dispatch 的 CI 预算约 **90–120 min**（integration ≈42 + coverage ≈40–
60 + Build Check 3×18 + Security Audit）。

## 5. 明确不要做的事

- **不要**给 coverage 加 `continue-on-error`：那等于把它退回"从未真正执行"。
- **不要**跳过 Stage 2 直接 dispatch：棘轮单调，误红之后基线不会自动降，只会红到有人改文件。
- **不要**在无逐项理由的情况下 `--save-baseline` 下调 floor —— 单调棘轮的意义正在于此。
- **不要**用 `--features test-utils` 之类的窄 feature 集去"绕开"失败：覆盖率口径一旦分叉，
  数字就不再可比（这正是 Stage 1 要收敛两份实现的原因）。
