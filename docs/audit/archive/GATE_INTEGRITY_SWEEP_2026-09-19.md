# 门禁诚实性清查（Gate Integrity Sweep）— 2026-09-19

> 触发：本轮先后撞到**三个同类缺陷**——`scripts/cleanup_test_schemas.sh` 的"硬排除"在常见
> 路径下为空、`postgres` healthcheck 在 initdb 阶段假健康、`REQUIRED_INDEXES` 守卫只比对
> "声明过"而非"实际保留"。`AGENTS.md` 铁律 8 的原话是"看到长期 0 违规 / 长期全绿的门禁，
> 优先怀疑它没在工作"，因此做一次系统清查：**每个检查类门禁，是否真的能变红。**

## 0. 方法与口径

四个只读审计并行（不修改仓库、不跑 cargo，避免与并发会话争用构建）：
① ratchet/baseline 类脚本；② CI 接线诚实性；③ Rust 侧守卫测试；④ 质量/架构/契约门禁。
统一判据，每项都要回答：

1. **判据是什么**、如何决定 pass/fail；
2. **接线**：哪个 workflow/job/step 调用，是否阻塞；
3. **非空转**：扫描集合会不会为空、阈值是否恒满足、失败是否被 `|| true` / `2>/dev/null`
   吞掉、baseline 文件是否真的存在并被读取；
4. **能否变红**：故意违规能否让它失败（能实测就实测）；
5. 裁定：**真门禁 / 弱门禁 / 假门禁**。

**验证状态标记**：`已实测` = 本次由我或审计者用可复现命令当场验证（含红证明）；
`复算` = 审计者用谓词复算 / `/tmp` 副本实验 / 静态推理，未运行被审对象。
为避免夸大，两类严格区分。

---

## 1. 已修复（含提交与红证明）

### 1.1 `tests/unit/placeholder_scan_tests.rs` —— 整条守卫早已失效 ✅ `6929070b`

- **机制**：扫描根是 `<crate>/src/web/routes[/handlers]`，而 HTTP 面已在 `4ca7a635`
  搬到 `synapse-web` crate ⇒ 路径不存在；`collect_rs_files` 用
  `if let Ok(entries) = fs::read_dir(dir)` ⇒ 目录缺失被当作"没有文件" ⇒ 两个 assert
  都在**空集合**上通过。该文件自 2026-06-20 起再未同步，长期"全绿"。
- **连带**：`scripts/shell_routes_allowlist.txt` 37 条里 **26 条**匹配不到任何站点
  （路由换 crate + 行号漂移），且没有任何机制能发现。AGENTS.md 中"行号漂移会让
  placeholder_scan 失败"的说法，在守卫失效期间是错的。
- **修复**：路径改为 `synapse-web/src/routes[/handlers]`；`collect_rs_files` **fail-loud**
  （目录不可读即 panic 并点名路径）；新增 `assert_scan_is_non_trivial`（≥20 个 `.rs`）；
  新增**反向**校验——allowlist 里匹配不到站点的条目同样判失败；allowlist 收敛为真实的 11 条。
- **红证明（已实测，独立 rustc 复刻；harness 内运行待 workspace 可编译后补）**：
  ① 扫描根不存在 → 立即 panic 并点名路径（当年正是这种回归被吞掉）；
  ② 副本里多插一处 `Ok(empty_json())` → 报 `tags.rs:160` 未列入 allowlist；
  ③ allowlist 多加一条无命中条目 → 报其为陈旧；④ 真实仓库下两个检查 PASS。

### 1.2 `scripts/check_fmt_ratchet.sh` —— 测量工具坏了等于全绿 ✅ `4390c006`

- **机制**：`… | xargs -0 rustfmt --check … | grep -c '^Diff in' || true`。rustfmt
  **缺失或崩溃**时输出里没有 `Diff in`，与"0 处差异"完全同形 ⇒ 计数 0；baseline=0
  ⇒ 打印 `OK: fmt debt at baseline (0)`。这是当年"`cargo fmt` 不打印 Diff 块"假绿的下一层。
- **修复**：开头 `command -v rustfmt` 前置检查；`count_fmt_diffs` 显式识别工具错误并输出
  非数字 `rustfmt-failed-to-run`，由调用方 `^[0-9]+$` 兜住；`--update` 路径同样先校验数字
  （原来空值也会写进 baseline）；"offending locations" 里重复的第二份 `find` 列表改为复用
  `fmt_targets`（铁律 2）。
- **红证明（已实测）**：① 正常 current=51 baseline=0 → 非零报 increase；② PATH 无 rustfmt
  → `::error::rustfmt is not on PATH`，exit 1；③ 崩溃的假 rustfmt → `failed to count fmt diffs`，
  exit 1，**同一假 rustfmt 下旧管道计数为 0（即旧脚本会打印 OK）**——已实测对照；
  ④ `--update` + 崩溃 rustfmt → 拒绝写基线，exit 1。
- **残留（已记，未修）**：`count_fmt_diffs` 仍按 **diff 块数**而非文件数计数；同一文件多处
  差异会算作多条，故数字含义是"差异块数"。不影响棘轮语义，但报错文案里的 "debt" 是块数。

---

## 2. 假门禁 / 空转（按危害排序）

| # | 位置 | 机制 | 后果 | 状态 |
|---|---|---|---|---|
| A1 | `e2ee-interop.yml` vodozemac job（`:7,11,72,74,146,159`） | ①paths 只含 `src/e2ee/**`，**漏 `synapse-e2ee/**`**（真实代码在 `synapse-e2ee/src/vodozemac_interop_tests.rs`），且引用了不存在的 `src/e2ee/vodozemac_interop_tests.rs`；②`cargo test --lib 'e2ee::vodozemac_interop_tests'` 只测根包 ⇒ **0 命中退出 0**；③三处 `2>&1 \| tee` 掩盖退出码 | **三重死**：vodozemac 互操作门禁从未运行过 | 复算 |
| A2 | `db-migration-gate.yml:241,250,259,268,277` | `--test unit <模块名>`，而这些模块只在 `tests/integration`（且名为 `*_migrated`）⇒ libtest 跑 **0 个测试**、exit 0 | 5 个"DB 迁移门禁"步骤断言不了任何东西 | 复算 |
| A3 | `db-migration-gate.yml:319` | `--test unit database_integrity_tests::tests::test_verification_requests_pending_index_survives_full_migration_chain --exact`，该测试全仓不存在；`--exact` 使不匹配也 exit 0 | 静默通过 | 复算 |
| A4 | `db-migration-gate.yml:333` | `--test unit invite_blocklist_tests` 唯一命中 `tests/unit/msc_tests.rs:41` 的玩具断言（`starts_with('@')`） | 步骤名"DB smoke"与内容不符 | 复算 |
| A5 | `db-migration-gate.yml:30` | `check_baseline_consolidation.py \| tee`（bash 默认无 pipefail） | 该门禁**永不变红** | 复算 |
| A6 | `benchmark.yml:93,94,97,98`、`ci.yml:703`、`e2ee-interop.yml:74,146,159` | `cargo bench/test \| tee` 掩盖退出码 | 失败被吞 | 复算 |
| A7 | `ci.yml:522-526,833-837` | integration-test / coverage / build 的 `if` 要求 `event_name=='push'` 且 main/develop | **PR 上永不运行**（skipped 不阻断） | 复算 |
| A8 | `ci.yml` coverage job + `artifacts/coverage_baseline.json` | baseline 未入库（origin/main 也没有），bootstrap 只在 push 跑 | schedule 运行时 per-file 覆盖率棘轮必 `exit 2`（常驻假红） | 复算 |
| A9 | `mutation-testing.yml:30,98,141`、`ledger-export.yml:74,149` | job 级 `continue-on-error: true` + `\|\| true` + `exit 0`/`sys.exit(0)`，只断言 JSON 非空 | 纯报告，不是门禁 | 复算 |
| A10 | `db-replica-consistency.yml:23`、`db-tests-manual.yml:72` | secret 缺失 `exit 0` 假跳过；`ON_ERROR_STOP=0 … \|\| echo "(non-fatal)"` | 未验证却报成功 | 复算 |
| A11 | `drift-detection.yml:14-16,225-238,330` | PR 只挂 `branches:[main]`（PR→develop 不触发）；重复迁移检查对同目录 basename `uniq -d` ⇒ 结构恒空；硬编码 v12 文件名 | 漂移检测有洞 | 复算 |
| A12 | `scripts/ci/check_sdk_route_coverage.py` | 被 `check_route_contract.sh` 调用，但 CI 无 SDK、未设 `SDK_CONTRACT_STRICT=1` ⇒ 恒 SKIPPED | "SDK ⊆ ledger"方向从未在 CI 生效 | 复算 |
| A13 | 孤儿真门禁脚本 | `check_trait_ratchet.py`（文档称门禁；**基线已是 TOTAL=65，文档写 86，已漂移**）、`check_feature_matrix.py`、`run_ci_tests.sh`（TESTING.md 的"主门禁/CI 等价入口"，ci.yml 内联重实现）、`run_complement_tests.sh`、`check_sqlx_offline_cache.sh`、`run_cargo_audit.sh`、`ci_schema_health_check.sh`、`validate_config.sh`、`generate_sdk_ledger_fixtures.sh` | 无任何 workflow 调用；文档称门禁而实际从不运行 | 复算 |

---

## 3. 假/弱守卫（Rust 侧，按危害排序）

| # | 位置 | 机制 | 状态 |
|---|---|---|---|
| B1 | `services_sync_domain_refactor_tests.rs` L67/76/85/112/121 + `services_remaining_domains_refactor_tests.rs` L18 | **两侧写的是同一个路径**（如 `push::PushNotificationService == push::PushNotificationService`）。应比的 legacy 路径见 `lib.rs:134/140/48/100/175`。删掉那些平铺模块/别名，这 6 条**全绿** | 复算（含 lib.rs 行号核对） |
| B2 | `coverage_tests.rs` 34 条 + `worker_coverage_tests.rs` ~18 条 + `boundary_tests.rs` + `api_optimization_verification_tests.rs` | 零/极低生产耦合：断言对象是上一行刚构造的 `json!` 字面量 | 复算 |
| B3 | `worker_coverage_tests.rs:402` | `assert!(!x.is_empty() \|\| x.is_empty())` = `P \|\| !P` | 已实测（字面重言式） |
| B4 | `test_connection_budget_tests.rs:116-123` | 文档称"预算 ≤ PG max_connections(100)"，但 `worst_case_demand=480 > 100` 时只 `println!`；唯一真断言是 `pool_max < 100` | 复算：**不变量当前被违反 4.8× 而测试全绿** |
| B5 | `sliding_sync_perf_gate_tests.rs`（14 条，12 条） | 0 次 `Command`，在 Rust 里复刻脚本的 sed/awk 逻辑 ⇒ 改脚本阈值/键名/默认宽松度全不触发 | 复算 |
| B6 | `benchmark_pr_gate_tests.rs`（11 条，9 条） | 只有 2 条接触脚本，其余是"写临时文件断言它存在"/字面量算术。同目录另有高质量的 `pr_benchmark_gate_tests.rs`（真执行脚本+正反控）⇒ 这是重复实现留下的僵尸文件 | 复算 |
| B7 | `perf_gate_honesty_tests.rs::compute_perf_gate_fails_on_missing_measurements_by_default` | 把 `compute_perf_gate.sh` 的 `STRICT:-1` 改成 `:-0` 后三条断言仍过；第 2 条含 `\|\|` 恒真项（被 Summary echo 满足） | 复算（审计者实测 sed） |
| B8 | `test_fixture_error_handling_tests.rs` | doc 宣称同时匹配 `.ok();` 与 `let _ = …execute(…).await;`，实现只有 `ends_with(".ok();")` ⇒ 仓库 **187** 处该写法全漏；扫描根 7 个目录**不含 `tests/`**，而 `is_test_support()` 里写着 `s.contains("/tests/")` ⇒ `tests/integration` 下 11 处逃检 | 复算 |
| B9 | `schema_lifecycle_guard_tests.rs` | 扫描根不含 `synapse-test-utils/src`（6 处 CREATE SCHEMA）与 `tests/`（5 处）；靠 `total_sites>0` 保住非空性 | 复算 |
| B10 | `synapse-storage/src/migration_checks.rs:165,189` | `migrations/` 现无增量 ⇒ `discover_migration_files()` 返回空 ⇒ `for v in &versions` 迭代 0 次、`!any(v==0)` 空集恒真；且 `v==0` 永不可达。**配套生产侧** `discover_migration_files()` 在目录缺失时 `return Vec::new()`（只 warn）⇒ `check_migration_completeness` 的 `missing` 恒空 | 复算（含生产侧） |
| B11 | `migration_replayability_guard_tests`、`migration_search_path_tests::migration_foreign_keys_…` | 主体集合为空（0 个增量）⇒ 空集即成功 | 复算 |
| B12 | `mock_fidelity_tests:112` | `src.contains("block_room")` 被 `room/state/info.rs:121 pub async fn block_room` 恒定满足 | 复算 |
| B13 | `mod_guard_tests::test_no_duplicate_fixture_names` | `read_dir` 失败→空集→恒过；实现比的是"fixtures 目录名 ∩ mod 名"，与 docstring 所述不变式无关 | 复算 |
| B14 | `security_signature_check_tests::test_admin_registration_hmac_logic` | 仅断言 `hex.len()==64`，恒真 | 复算 |
| B15 | `synapse-common/src/test_schema_guard.rs:409` | CI 即 nextest（`NEXTEST=1`）⇒ 该测试断言体被跳过 | 复算 |
| B16 | `synapse-web/src/routes/derived_routes.rs` default/worker/all golden（3 条） | `--all-features` 打开 all-extensions 后 `cfg(not(any(...)))` 关掉；`--test unit` 不编译依赖 crate 的 `cfg(test)` ⇒ **任何 CI job 都不编译** | 复算 |
| B17 | CI 中永不编译的测试模块 | `mod voice_service_tests` / `voice_route_tests`（`#[cfg(feature="voice-extended")]`，而 CI unit 步骤不带 `--all-features`）；`storage_remaining_domains_refactor_tests` 的 7 条同因 | 复算 |
| B18 | `test_isolation_unification_tests` 弱边 | `production_half` 按"首个 `#[cfg(test)]`"截断（文件前部加一个小 `#[cfg(test)]` 项即可整段关掉负向守卫）；Guard1 负向断言是字面量 `"for stmt in"`；Guard3 是 `lib.contains("pub mod test_isolation;")`，注释掉也过 | 复算 |

**对照：确认为真守卫（节选）**：`mod_guard_tests`（存在即注册/注册即有文件）、`config_mount_tests`、
`cleanup_schema_script_tests`、`db_readiness_probe_tests`、`migration_consistency_tests`、
`test_db_url_convention_tests`（**自带正控，范例**）、`template_fingerprint_inputs_tests`（双向，范例）、
`sqlx_ratio_gate_tests`（真执行+棘轮红实验，范例）、`pagination_gate_tests`、`pr_benchmark_gate_tests`、
`ledger_export_tests`、`e2e_honesty_tests`、`self_silencing_config_tests`、96 条类型恒等中的 90 条
（编译期有效，但**运行时分支恒不可达**：`if let (Some,Some)=(None,None)`，pass 不含断言执行）。

---

## 4. 需要决策的项（不在本文件自行处置）

1. **PR 门禁面**（A7/A8）：integration/coverage/build 只在 push 跑 —— 是刻意的成本取舍还是漏洞？
   若要覆盖 PR，需要 CI 预算与 DB 服务。
2. **孤儿门禁脚本**（A13）：接线还是删除？（`check_trait_ratchet.py` 的基线已从 86 漂到 65，
   文档与事实不一致，无论哪种选择都要先修文档。）
3. **零耦合测试的去留**（B2/B6/B5）：删除、改写为真守卫、还是标注为非门禁？
   注意 B6 的同目录已有真版本，属重复实现（铁律 2 倾向删除）。
4. **B4 连接预算**：当前不变量被违反 4.8×，是"预算计算口径不对"还是"池上限该调"？这是**产品决策**。

---

## 5. 本轮已完成的收尾

- 已修并提交：`6929070b`（placeholder_scan 复活 + allowlist 双向校验）、
  `4390c006`（fmt 棘轮不再把工具故障当干净）。
- **受阻**：Rust 侧修复的 harness 级验证被并发会话的编译中断挡住
  （`synapse-e2ee/src/cross_signing/service.rs:208` E0282/E0308，属他人未提交工作）。
  故 1.1 的逻辑用**逐字复刻的独立 rustc 程序**验证；一旦 workspace 可编译，需补跑
  `cargo nextest --profile test --features test-utils --test unit -E 'test(placeholder_scan)'`。
- 一条**现存真守卫是红的**：`test_isolation_unification_tests::baseline_fingerprint_is_the_single_v12_source`
  （HEAD 的 v12 哈希 `45483ffa0a28b5d5` = 常量；工作区因并发会话改了 baseline 而为
  `b6a8b06fb13d22f9`）。**改 baseline 的一方必须同步该常量**，不要去替他们改。

---

## 6. ratchet / baseline 类门禁审计（第四路审计，只读）

**裁定汇总**：真门禁 —— `check_fmt_ratchet.sh`（瑕疵即 §1.2，已修）、`check_missing_docs_ratchet.*`、
`check_schema_contract_coverage.py`（CI 用 `--threshold 100`）、`check_schema_table_coverage.py`、
`check_schema_blind_guards.py`（error 级）、`check_sqlx_dynamic_ratio.sh`、`supply_chain_gate.sh`、
`check_route_storage_boundary.sh`、`check_connection_budget.py`、`check_workflow_steps.py`。
审计者对上述多数做了 `/tmp` 注入实验并确认 exit 1（见其报告）。
**假/弱门禁**如下（编号接 §2/§3）。

| # | 位置 | 机制 | 后果 | 状态 |
|---|---|---|---|---|
| C1 | `check_file_coverage.py:480-483` vs `:525-526` + `ci.yml:941,958` | `require_baseline()` 在 `save_baseline()` **之前**无条件要求 `--baseline` 已存在；而 CI 的 bootstrap 步骤恰好传入**不存在**的 `artifacts/coverage_baseline.json` ⇒ 按 CI 原样命令实测 `Coverage ratchet cannot run … not found`、**EXIT=2 且不建文件**。该 baseline 从未入库、`artifacts/` 不跨 run 保留 | **覆盖率棘轮从未执行过任何一次**：80/40/30/70 四条阈值与"touched 不得回退"全未评估；`ci.yml:931-935` 注释里的 "pass by construction" 不成立。push 时 bootstrap 先红、schedule 时棘轮直接 exit 2 | 已实测 |
| C2 | `db-migration-gate.yml:30` | `python3 check_baseline_consolidation.py \| tee artifacts/…`，该 step 无 `pipefail` ⇒ 管道退出码取 `tee`(0) | 脚本报 ❌ exit 1 时 step 仍判绿。脚本本身可红，缺陷纯在接线——与 §1.2 同型 | 已实测（脚本侧） |
| C3 | `check_baseline_consolidation.py` 主体 | `migrations/` 只剩 1 个 baseline ⇒ `^[0-9]{14}_.*\.sql$` 匹配 **0** | 今天对空集生效，"长期 0 违规"正因为扫描面为空 | 已实测 |
| C4 | `check_trait_ratchet.py` + `scripts/ci/trait_count_baseline` | 基线**存在但无人读**：唯一读取者是该脚本自己，而没有任何 workflow/脚本调用它 | AGENTS.md 列为"真实基线"的棘轮永不可能触发；且基线 TOTAL=65 与文档所述 86 已漂移 | 复算 |
| C5 | `check_sqlx_offline_cache.sh` | `.github/` 零引用 | 完全未接线（`.sqlx/` 现有 60 个 query-*.json，接线后会真的跑 `cargo check`） | 复算 |
| C6 | `check_web_layering.py:39-54,60-62` | **无"扫描面存在/非空"守卫**（对比 `check_route_storage_boundary.sh` 有两道）；且 `scripts/ci/web_layering_allowlist.txt` **磁盘上不存在** ⇒ `read_allowlist()` 返回空集 ⇒ `stale = allowed - current` 恒空 | `synapse-web/src` 不存在时 `os.walk` 空集 → 打印 OK **EXIT=0**；"stale 条目也红"半条逻辑永久死亡 | 已实测 |
| C7 | `check_missing_docs_ratchet.py:114,199,279` | 增量扫描默认 `--base HEAD~1`，而 ci.yml 全部 checkout 是裸 `actions/checkout@v4`（全仓仅 `drift-detection.yml:44` 有 `fetch-depth: 0`）⇒ `HEAD~1` 不可解析，`run()` 对非零码 `sys.exit(2)` | 推断：每轮 CI 该 step 红（非假绿），但增量 pub-doc 检查同样从未真正跑过 | 本地 shallow-clone 复现 `fatal: ambiguous argument`；CI 侧未证实 |
| C8 | `check_missing_docs_ratchet.py:211-213` | 自称 `-A missing_docs -D missing_docs` 能"抵消 crate 级 allow"。rustc 实测反证：attribute 胜出（`#![allow]` 压过 `-D`；`#![deny]` 压过 `-A`） | 今天测量仍有效（7 个 crate 用 `#![deny]`，两个两者皆无），但**机制描述是错的**，下一个人会照它推理 | 已实测（rustc） |
| C9 | `check_file_coverage.py:485,331-333,395` | `--tdd-files` 从未被传入 ⇒ `load_tdd_files(None)` 恒空 ⇒ `is_tdd` 永不成立 | 宣称的 "TDD ≥ 80%" 对 **0 个文件**生效；另 `:104-109` 对不存在路径静默返回空集（与其 baseline/core 的 fail-closed 不一致） | 已实测（65% 文件：不传 tdd exit 0、传则 exit 1） |
| C10 | `check_migration_consistency.py:11` | `REQUIRED_V8_BATCHES: list[str] = []`，且 0 个时间戳迁移、0 个 `V*` ⇒ undo 配对子检查扫描集为空 | 今天只校验 compose 挂载串这 1 个近乎恒真条件（该脚本对此文件名不符/错误挂载仍可红） | 已实测 |
| C11 | `check_schema_contract_coverage.py:536-541` | 默认 `--threshold 90`，而 CI 两处都传 100 | 手工调用时丢一张表 = 99.5% ≥ 90 ⇒ **打印 "missing table definition" 却 exit 0** | 复算 |
| C12 | `ci.yml:1045` | `ci-summary.needs` 不含 `repo-sanity`（承载 4 个门禁：migration consistency、schema table coverage、schema contract coverage、route-storage boundary 等） | 该 job 是否 blocking 取决于分支保护是否要求 "Repo Sanity"，与 `ci-summary` 无关 | 复算 |

**基线文件核查**：`.fmt-baseline`、`.missing-docs-baseline`、`sqlx_dynamic_ratio_baseline`、
`geiger_baseline.json` 均存在且被读取 ✓；`trait_count_baseline` 存在但**无人读** ✗；
`artifacts/coverage_baseline.json` **不存在且从未入库** ✗。

---

## 7. Rust 侧审计的三个补遗（第二批）

| # | 位置 | 机制 | 后果 |
|---|---|---|---|
| D1 | `synapse-test-utils/src/lib.rs:1124` | 模板指纹里 `let contents = fs::read(entry.path()).ok()?;` —— 某个 `.sql` **读失败会被静默跳过**，指纹不反映它 ⇒ 模板不重建；同函数上一行 `workspace_migrations_dir()` 用的是 `.expect`，只有这一处吞错。`template_fingerprint_inputs_tests` 只喂**可读**文件，覆盖不到该分支 | "被检查文件不可读 → 当作不存在 → 仍绿"，与 §2 同型；生产侧（非测试）路径 |
| D2 | `synapse-common/src/test_isolation.rs:54` + `docs/audit/P1D_seed_allowlist_design_2026-09-14.md` | 文档写"pinned by `seed_reference_tables_match_baseline`"，该测试名在 Rust 代码中**不存在**；同职责的实现是 `tests/unit/test_isolation_unification_tests.rs::the_seed_allowlist_matches_what_the_baseline_seeds`（真守卫）。设计文档声称的 `allowlist_clone_matches_full_clone_row_for_row` 也未按名落地 | 契约文档与实现漂移（不是守卫缺失），会误导后续查找 |
| D3 | `synapse-storage/src/migration_checks.rs:189`（补充 §3-B10） | 该测试对 `.undo.sql` **零断言**：删掉 `if name.ends_with(".undo.sql") { return None; }` 也**不会红**；`assert!(!versions.iter().any(\|v\| v == &0_i64))` 不可能因 baseline 复活而红，因为 `name[..14]` = `"00000000_unifie"` 非全数字、`parse::<i64>()` 直接 `None` | 该文件两条测试的**全部**断言在空集上恒真 |

---

## 8. 质量/架构/契约门禁审计（第一路补完）

**真门禁**：`check_doc_spelling.sh`（三态 0/1/2 实测）、`quality/format_check.sh` +
`format_audit.py`（`--fail-on-drift` 实测红）、`contract/check_route_contract.sh`
（52 检查 + 6 变异全过；`gen_derived_routes --check` 漂移即 exit 1）。

| # | 位置 | 机制 | 后果 | 状态 |
|---|---|---|---|---|
| E1 | `scripts/check_get_raw_usage.py:71` | 正则要求**空参数** `get_raw()`，真实 API 是 `get_raw(key)` ⇒ 永不命中；`is_allowed_path` 的 glob 全坏；`:109` 行内出现 `get_raw_shared()` 即整行豁免；红路径打印孤立代理对 emoji 抛 `UnicodeEncodeError` | **安全相关的不变量（缓存读写对称）门禁是死的**；"0 违规"是巧合 | **已修 ✅ `239c5780`**（regex + 路径谓词 + 花括号配平的 `#[cfg(test)]` 跳过 + 空扫描 fail-loud + 注释跳过，五条红绿证明） |
| E2 | `scripts/ci/run_cargo_geiger.py:56-63,125-144` | ①`--output-format json` 小写，cargo-geiger 0.13 的 `OutputFormat` 是大小写敏感的 strum 枚举 ⇒ 子进程非零 → `sys.exit(1)`；②即便修 flag，`classify_files` 迭代的是顶层 `SafetyReport` **对象**（键 `packages`…）⇒ `'str' has no attribute 'get'`；③`sum_unsafe` 读 `unsafe\|metrics.extern_blocks/traits/fns/…`，真实字段是`unsafety.used.{functions,exprs,item_impls,item_traits,methods}` ⇒ **恒 0** | 三种形态叠加：今天"一跑即崩或恒 0"，Gate1(`prod_total>0`) / Gate2(`>baseline`) 永不触发；docstring 宣称"修掉了恒 0 假绿" | 上游源码 + 同形载荷复现；**未修** |
| E3 | `scripts/ci/check_trait_ratchet.py` + `trait_count_baseline` | 无任何 workflow/Makefile/脚本调用（同 C4） | 棘轮永不触发；基线 TOTAL=65 与文档 86 漂移 | **未修**（需先决定接线或删除） |
| E4 | `check_pagination_benchmark.py` | 比值来自**同一次运行**内两个手写仿真函数（`benches/performance_api_benchmarks.rs:436-484`：O(175k) 扫描 vs 二分+100 行），30% 阈值由构造满足（≈1000× 余量）；不含 `synapse-storage` 的真实分页 SQL 与存储基线 | 弱门禁：改真实 SQL 不受影响 | 未修 |
| E5 | `check_sdk_route_coverage.py:292-305` | CI 无 SDK 且从不设 `SDK_CONTRACT_STRICT=1` ⇒ 打印 SKIPPED 后 **exit 0**；`sdk_uncovered_allowlist.txt` 0 条 ⇒ 卫生检查的 rotten 分支不可达 | "SDK ⊆ ledger"方向从未在 CI 生效（同 A12） | 未修 |
| E6 | `quality/check_route_layering.sh` | `find` 空集/目录缺失 = PASS（无扫描面守卫）；Pattern A 的 `use crate::storage` 在 `synapse-web` 下不可能出现（真实违规是 `synapse_storage`，只由 `check_web_layering.py` 覆盖，而后者本身也无守卫，见 C6）；头注释声明的 Pattern D/E 未实现 | 弱门禁 + 死路模式 | 未修 |
| E7 | `build_sqlx_migration_source.py` | 只写 `artifacts/` + `manifest.json`（无读者） | 生成器不是门禁；选择集无断言（当前只选 v12 baseline，`V*`/时间戳分支命中 0），下游靠 `sqlx migrate run` + `validate` 间接兜底 | 未修 |
| E8 | `api_test/gen_client_yaml.py`、`gen_route_table.py` 与已提交的 `docs/openapi/{client.yaml,route-table.json}` | 只生成上传，**无 diff 校验**；`run_api_tests.py`/`schemathesis_*` 零 CI 引用；`scripts/api_test/ledger.json` 是 8-12 的旧文件 | 契约产物可静默过期 | 未修 |
| E9 | `scripts/contract/extract_unresolved_allowlist.txt`（21 条） | 只有"新条目"会红；**陈旧条目仅打印提示**（`extract_registered.py:1743`） | 与 `shell_routes_allowlist.txt` 形成对比：后者已双向强制，前者仍可无声腐化 | 未修 |

**宽容豁免清单（本轮口径）**：`shell_routes_allowlist.txt` 11 条已双向强制 ✅；
`extract_unresolved_allowlist.txt` 21 条仅单向 ⚠️；`sdk_uncovered_allowlist.txt` 0 条（分支不可达）❌；
`check_get_raw_usage.py` 的 6 条模式全失效（已在 `239c5780` 改为路径谓词）✅；
`check_doc_spelling.sh:52` 的 `^[a-f]+$` 过滤会静默丢弃 `abcd`/`deadbeef` 这类纯十六进制词
（对 hex 摘要有用，但也会吞掉真实单词，属已知取舍，未修）。

### 8.1 geiger：上游 schema 已核实，问题比"字段名写错"更深（需裁定）

本机无 `cargo-geiger` 且本轮禁用 `cargo`，故我按任务要求去**核实上游 schema**，而不是
照抄审计结论。已从 docs.rs 取到 cargo-geiger-serde 0.3.0 的定义：

```rust
pub struct SafetyReport {
    pub packages: HashMap<PackageId, ReportEntry>,   // ← 按**包**索引
    pub packages_without_metrics: HashSet<PackageId>,
    pub used_but_not_scanned_files: HashSet<PathBuf>,
}
pub struct ReportEntry { pub package: PackageInfo, pub unsafety: UnsafeInfo }
```

（<https://docs.rs/cargo-geiger-serde/0.3.0/cargo_geiger_serde/struct.SafetyReport.html>、
<https://docs.rs/cargo-geiger-serde/0.3.0/cargo_geiger_serde/struct.ReportEntry.html>）

结论有三层，且第三层是**设计问题而非笔误**：

1. `GEIGER_CMD` 传 `--output-format json`（小写）——cargo-geiger 的 `OutputFormat` 是
   大小写敏感的 strum 枚举，应为 `Json`：子进程会以非零退出，脚本随即 `sys.exit(1)`
   （即"一跑即崩"）。**待修**。
2. `classify_files(metrics)` 把入参当"文件条目列表"逐条 `.get("file"/"path")`；而 JSON 的
   顶层是 `{packages: {...}}`——它拿到的是**对象**（键为 `packages` 等字符串），
   `entry.get(...)` 作用在 `str` 上 ⇒ `AttributeError`。**待修**。
3. 更关键：**JSON 报告里根本没有文件路径**，只有按包索引的 `ReportEntry`。因此脚本赖以
   区分"生产 vs 测试"的 `classify_files` 按路径切分的整套设计，**在 `--output-format Json`
   下不可实现**；而 `sum_unsafe` 读的 `unsafe|metrics.extern_blocks/traits/fns/impls/blocks`
   也不在 schema 里（真实字段在 `unsafety` 下，审计称 `used.{functions,exprs,item_impls,
   item_traits,methods}`，我未逐字复核该层）⇒ 即便修好 1、2 也是**恒 0**。

  可选的落地方式（都需要在 CI 里真跑一次才能确认，本机做不到）：
- **(A) 解析文本输出**：`cargo geiger` 默认输出按文件分组，能恢复"按路径分生产/测试"的
    设计；代价是要写一个**有 fixture 单测**的解析器（旧版正是脆弱的 `grep -oP` 才坏掉的）。
- **(B) 保留 JSON、改政策**：`--include-tests` 关闭时所有计数都属"要发布的代码"，
    于是"生产 unsafe 必须为 0"直接成立，而 `test_unsafe_total` 基线与 Gate 2 失去数据来源
    ⇒ 必须删掉该基线字段与 Gate 2（铁律 1），文档同步。
- **(C) 跑两次取差**：不带 `--include-tests` 得生产计数、带它再跑一次，按包相减得
    "仅测试"计数 ⇒ 两个 gate 都保住，代价是 2× 扫描时间。

  **我没有擅自选**：三种方式产出的门禁语义不同（B 会放弃测试侧棘轮），且都无法在本机
  端到端验证。另外 CI 是 `cargo install cargo-geiger --locked`（未钉版本），上游 schema
  一变就会再次漂移——无论选哪种，都应把版本钉住并把"schema 不符即 loud fail"写进脚本。

---

## 9. 交接：当前阻塞与执行顺序（2026-09-19）

> 详版见 `docs/superpowers/plans/2026-09-19-gate-integrity-and-coverage-followup.md`（该目录被 gitignore，
> 换 worktree 读不到，故把"必须随仓库走"的要点记在此处）。

### 9.1 当前唯一阻塞：`IsolatedTestPool` 放错 crate（铁律 2）

- 共享模块 `synapse-common::test_isolation` **无条件编译**且注释写明"给兄弟 crate 的 fixture 用"
  （`synapse-common/src/lib.rs:87-89`），但只提供**原语**（`test_isolation.rs:329/1254/1290/474`）；
  每个 DB 测试真正要的封装 `IsolatedTestPool` 只存在于 **`synapse-storage/src/test_isolation.rs:86`**，
  且除 storage 外**无人能用**。
- 后果：其它 crate 手搓 pool。`synapse-e2ee/src/verification/service.rs` 的两条测试因此
  **硬编码** `connect_lazy("postgres://…/synapse_test")` 直连 `public`，而同一 `--tests` 运行里的
  **unit 目标会清空 `public`**（T-1 陷阱）⇒ 42P01 `verification_requests does not exist`。
  **不是产品回归**：基线 `:750` 建了该表、storage 层一致地查它。
- 修法：把 `IsolatedTestPool` 搬进 `synapse-common/src/test_isolation.rs`，storage 保留 re-export；
  再改那两条测试（+ 同文件 `make_service()` 的同一颗雷）。**替换跨度陷阱**：rustfmt 把该语句排成
  `let pool =` 换行接 `connect_lazy(...)`，必须从 `let pool =` 那一行开始替换，否则报
  `expected expression, found let statement`。红证明：① `public` 被清空时两条测试必须通过；
  ② 未设 `TEST_DATABASE_URL` 时必须**明确报错**而非 42P01/静默。

### 9.2 覆盖率棘轮（链条进度）

```
synapse-rust 92 ✓ → synapse-common 895 ✓ → synapse-e2ee 431/2（§9.1）→ storage 腿 → 其余 crate → lcov
```
基线路径已迁移到 `scripts/ci/coverage_baseline.json`（见 §6 C1 的修复），**文件尚未生成**；
生成命令与"不要用失败那次的数据 bootstrap"的理由见 plan 文件 §2（棘轮 `save_baseline` 只升不降，
会永久固化偏低的基线）。播种：`DATABASE_URL=…/synapse_test bash docker/db_migrate.sh migrate`。

### 9.3 建议顺序

Phase 0 独立 worktree + 播种 → Phase 1 §9.1（解开阻塞）→ Phase 2 §9.2（跑到 lcov 并提交基线）
→ Phase 3 §3 的 B 系列（先 B1/B3/B10/B16，再 B5/B6 需决策，最后 B2）
→ Phase 4 §2/§3.1 的门禁接线（C6/C9/C11 最便宜；A9/A10/A12/A13 需决策）
→ Phase 5 §3.3 观察项。每个 Phase 都要**红证明**。

### 9.4 纪律（本会话 4 次踩坑的总结）

- **不要在共享 checkout 改**：另一 agent 的 `git reset`/rebase 已 4 次清掉改动甚至提交；用 worktree + 分支。
  已被清掉时 `git reflog` + `git cat-file -t <sha>` 通常能救回（本会话救回过一次）。
- 改 `migrations/` 后必须跑 `baseline_fingerprint` 守卫（哪怕只改注释）；改 `migrations/` 期间不要跑集成套件。
- `synapse_test.public` 会被 unit 目标清空（T-1），跑直连 `public` 的测试前先播种。
- 门禁改动一律要求"故意违规 → 必须失败 → 撤销"。
