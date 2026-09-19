# 门禁诚实性清查 — 执行交接（2026-09-19 第二轮）

> 上游文档：
> - 执行计划：`docs/superpowers/plans/2026-09-19-gate-integrity-and-coverage-followup.md`（该目录被 gitignore）
> - 首轮逐门禁裁定：`docs/audit/GATE_INTEGRITY_SWEEP_2026-09-19.md`
>
> 本文记录**本轮实际做了什么、红证明在哪、还有什么没做**。所有"已修"都带提交号；
> 所有"未修"都带复现命令或明确判定。

---

## 0. 结论摘要

| Phase | 状态 |
|---|---|
| Phase 1 解开阻塞（`IsolatedTestPool` 迁 crate） | ✅ 完成 + 双红证明 |
| Phase 2 覆盖率链路跑到产出 lcov 并提交基线 | ✅ 完成（**过程中又发现并修了 3 个环 + 2 个环境缺陷**） |
| Phase 3 B 系列假守卫 | ✅ B1 / B3 / B10 / B17 已修；B4 已按新口径重算并可真红；B5 / B6 / **B2** 已删除或保留并说明；B18 弱边已修一半 |
| Phase 4 CI 门禁 | ✅ C6 / C9 / C11 / C12 已修；A13 完成接线 1 个、删除 2 个（其余登记处置） |
| Phase 5 观察项 | ✅ 三条已修（`prepare_test_db.sh` `39211779`、janitor 退出 `4456eb80`、`events_fts_idx` `2904cbf4`）；清理脚本力度待办 |
| 额外 | 🔴 **HEAD 的 fmt 门禁当时就是红的**（99 块 vs baseline 0）——已修 |
| 额外 | 🔴 **HEAD 的 clippy 两个矩阵档都是红的**（见 §1.6）——已修 |

本轮提交（新 → 旧）：

```
c4434613 test(fix): 连接预算断言改为「需求口径」并真的可红（B4）
753f29ad fix(ci): 删除从未生效的 TDD 覆盖车道（C9）
003a54f2 fix(ci): 接线 trait 棘轮；删除两个与已接线门禁重复的孤儿脚本
f9701d8c test(ci): 删两个僵尸门禁副本 + 让 B16/B17 的门控测试真正编译
29ed5655 fix(ci): 覆盖率棘轮改为「基线文件只管不回退」+ 跳过 test-only 源
d32419e9 fix(ci): ci-summary 纳入 repo-sanity（4 个门禁的失败此前不进摘要）
4565cecd chore(coverage): 提交 per-file 覆盖率棘轮基线（623 个源文件）
993a1dd6 fix: 覆盖率链条上的五处假守卫/夹具缺陷 + 两处 CI 门禁补强
49365708 fix(tests): IsolatedTestPool 移入 synapse-common，e2ee 不再直连 public
bef65f53 style(fmt): 归零 fmt debt（HEAD 实测 99 块 vs baseline 0，1.93.0 下棘轮是红的）
```

---

## 1. 已修复（含证据）

### 1.1 🔴 HEAD 的 fmt 棘轮是红的（计划/SWEEP 未记录）`bef65f53`

- **事实**：在干净检出上（`git worktree add --detach /tmp/synapse-head HEAD`）跑
  `./scripts/check_fmt_ratchet.sh` → `fmt debt: current=99 baseline=0`、**exit 1**。
  本机 `rustc 1.93.0`，正是 `rust-toolchain.toml` 钉住、CI 也会用的版本
  （`rustup show active-toolchain` = `1.93.0-... (overridden by rust-toolchain.toml)`）。
- **含义**：`AGENTS.md` 的 "Format debt is at zero" 已过时；main 的 fmt 门禁长期是红的。
- **修复**：`cargo fmt --all`（AGENTS.md 规定的提交前步骤），14 个文件；
  随后棘轮 `current=0=baseline`。
- **顺带记录的测量瑕疵（未修）**：`fmt_targets | xargs -0 rustfmt --check` 会按
  `mod` 树重复格式化同一文件（如 `key_at_rest.rs` 的 8 处差异被计成 24 处），
  所以 `current` 是"差异块数 × 出现次数"。棘轮只比数字，pass/fail 不受影响，
  但报出的 debt 数值被放大。修法：计数去重，或在 `rustfmt.toml` 里设 `skip_children = true`。

### 1.2 Phase 1：`IsolatedTestPool` 迁入 `synapse-common` `49365708`

- **根因**：该封装在 `synapse-storage`，而那个模块是 `#[cfg(test)]`；Rust **不跨 crate
  传播 `cfg(test)`**，兄弟 crate 的 fixture 拿不到它。`synapse-e2ee` 的两条 DB 测试
  因此硬编码 `connect_lazy(...synapse_test)` 直连 `public`，而 unit 目标会清空 `public`
  （T-1）⇒ 42P01。
- **实现**：`IsolatedTestPool`（struct + `new(baseline_sql)` + `Drop` 清理）与
  `pub fn test_database_url()` 移入 `synapse-common/src/test_isolation.rs`（该模块
  **无条件编译**，正是 lib.rs:87-89 声明的用途）。baseline SQL 保持为**参数**，
  以维持"`synapse-common` 不得把 workspace migrations 编进生产构建"的既有设计规则。
  `synapse-storage` 缩成 adapter（`pub use` + `isolated_baseline_sql()` +
  `isolated_test_pool()`），16 处调用点机械替换。
- **守卫随代码走**：Guard 1 改为断言 COMMON 拥有两个原语、STORAGE 经共享 pool 委派；
  Guard 5 纳入 e2ee（否则它会静默铸第二份模板）；`test_db_url_convention_tests`
  的 `RUST_RESOLVERS` 随解析器更新。
- **顺带修掉 B18 的一条弱边**：`production_half()` 原来按**首个字符串** `#[cfg(test)]`
  截断，我新写的模块文档里提到该字符串，于是 STORAGE 的生产半被截到第 7 行、
  负向守卫整段失效。改为只认"整行 `#[cfg(test)]`"的真属性。
- **红证明**：
  1. 独立库 `synapse_gate_redproof`（`public` 表数 0，`select … from public.verification_requests`
     → 42P01）：18/18 通过；模板 230 表 + 10 个 trgm 索引，`public` 全程 0 表。
  2. `TEST_DATABASE_URL=...:59999/...` → 以
     `isolated test pool: Protocol("failed to connect admin pool for template
     test_isolation_template_b6a8b06fb13d22f9: pool timed out …")` 明确失败，exit 101。
  3. 守卫红证明：e2ee baseline 指向 `README.md` → Guard 5 报
     `left 56e5b76142b81fbd / right b6a8b06fb13d22f9`；`"for stmt in"` 放进 STORAGE
     生产半 → Guard 1 报错。探针均已撤销。

### 1.3 Phase 2：覆盖率链路（3 个环 + 2 个环境缺陷）`993a1dd6` `4565cecd` `29ed5655`

链条最初在 `synapse-e2ee` 断（§1.2）。修完后依次暴露：

| 环 | 现象 | 根因 | 处置 |
|---|---|---|---|
| 1 | `transaction_tests::test_trusted_private_chat_transaction` panic | 集成夹具 `create_test_config()` 的 `macaroon_secret_key: None`；e2ee wiring 在无 `megolm_encryption_key_path` 时从它派生 at-rest 密钥 | 与 `d8fcedab` 同因同修（补固定非生产值） |
| 2 | `synapse-services events.rs:791` 断言失败 | `EventNotFound(id) → ApiError::not_found(id)`，Display 是 `M_NOT_FOUND: $nonexistent:ex.com`，`.contains("not found")` **永不成立** | 改为断言机器可读的 `err.is_not_found()` |
| 3 | `voice_route_tests::ft130_..._media_id_required` 失败 | 命名说 required，正文断言 `is_ok()` 且消息说 optional；而 `RegisterEncryptedVoiceRequest.media_id: String` 是必填 | 改为断言反序列化失败（B17） |

两个**环境缺陷**（都不在计划里，是让链条真正能跑完的前提）：

| # | 现象 | 根因 | 处置 |
|---|---|---|---|
| E1 | `synapse-test-utils` 共享池测试 `relation "users" does not exist` | `synapse-test-utils::init_template_schema` 把建模板委托给运行时 `DatabaseInitService`，而后者在未设 `SYNAPSE_ENABLE_RUNTIME_DB_INIT` 时**按设计是 no-op**（迁移主链是 `docker/db_migrate.sh`）⇒ "重建模板"只会留下一个仅含 `schema_migrations` 的 schema。CI 靠预建 `test_template_ci` + 设 `TEST_DB_TEMPLATE_SCHEMA` 走"只校验"路径绕开 | `run_local_coverage.sh` 默认导出 `test_template_ci` 并在开跑前断言该 schema ≥100 张表，否则报错并指向 `scripts/ci/prepare_test_db.sh` |
| E2 | 我在排查时用 `DROP SCHEMA public CASCADE` 做"public 被清空"实验，级联删掉了 `pgcrypto`/`pg_trgm` 扩展与依赖它们的模板 trgm 索引 | 实验设计失误（扩装被清空时 `CASCADE` 会波及其它 schema） | 用 `docker/db_migrate.sh migrate` 重建 public + `prepare_test_db.sh` 重建 `test_template_ci`；旧模板指纹 `b6a8b06fb13d22f9` 已重建并核对（230 表 + 10 trgm 索引）。**教训：不要在有扩展的库上 DROP public CASCADE，用独立 scratch 库。** |

最终：`COVERAGE_EXIT=0`，0 失败（integration 1424 / unit 1894 / services 2001 / e2e 20 …），
`coverage/lcov.info` 14 MB，基线 623 个文件。

**棘轮语义修复** `29ed5655`：基线落盘后用 CI 真实阈值复跑，棘轮 exit 1、**188 个文件红**，
且每条 delta 都是 `+0.0%` —— 不是回退，是 `max(prev, global_floor)` 对从未达标的
绝对阈值反复报红（bootstrap 之后的棘轮按原语义**必然红**）。改为：
基线已记录的文件只判回退；绝对 floor 只约束新文件；跳过 test-only 源
（`test_mocks/`、`synapse-test-utils/`、`*test_utils.rs`、`*test_isolation.rs`、
`*test_schema_guard.rs`、`scripts/bench_harness.rs` —— 两条覆盖率腿结构上都测不到它们）。
- 红证明：基线里把 `synapse-web/routes/voice.rs` 抬到 99.0（实际 37.12）→
  `[TOUCHED] 37.1% < 99% (delta=-61.9%)` exit 1；把 `src/bin/federation_sign_request.rs`
  从基线移除使其成为新文件（实际 0.0%）→ `[NEW] 0.0% < 30%` exit 1；正常 → exit 0。

### 1.4 Phase 3：假守卫 `993a1dd6` `f9701d8c` `c4434613`

- **B1**（6 条路径恒等测试自比）：`push_notification_service` / `registration_service` /
  `account_identity_service` / `room::service` / `feature_flag_service` / `user_service`
  改为与真实 legacy 路径比较。
- **B3**（`assert!(P || !P)`）：改为断言 `can_handle_http`/`can_handle_federation`
  与 `supported_protocols` 一致 + `max_concurrent_requests > 0`。
- **B10**（空集恒真）：提取纯函数 `classify_migration_filename` / `scan_migration_files_in`，
  测试断言扫描面非空、baseline 被归类为非正向、`.undo.sql`/非数字前缀/过短名/备份各自钉死；
  生产侧 `discover_migration_files()` 的静默 `Vec::new()` 改为 `warn!`。
- **B17**（CI 永不编译的模块）：`voice_*` 两条真红已修（见 §1.3），并加 CI 步骤。
- **B5/B6**：`sliding_sync_perf_gate_tests.rs`（14 条中 12 条在 Rust 里复刻脚本逻辑、
  0 次执行脚本）与 `benchmark_pr_gate_tests.rs`（11 条中 9 条零耦合，同目录已有真版本
  `pr_benchmark_gate_tests.rs`）**删除**；真门禁分别是 `benchmark.yml:226` 调用的
  `scripts/ci/sliding_sync_perf_gate.sh` 与 `pr_benchmark_gate_tests.rs`。同步修了
  `mod.rs`、`sqlx_ratio_gate_tests.rs` 的对照注释、gap-analysis 文档的能力表述。
- **B16**：`derived_routes.rs` 的 default/worker/all golden 被
  `cfg(not(any(all-extensions, voice-extended, …)))` 门控，CI 只有
  `--workspace --lib --all-features`（关掉它们）与 `--test unit`（不编译依赖 crate 的
  `cfg(test)`）⇒ 三份 `ledger_export/*.json` 从未被校验。新增
  `cargo nextest run -p synapse-web --lib --features test-utils`。实测
  `-E 'test(derived_manifest_tests)'` → 3 tests run, 3 passed（此前 0）。
- **B4 连接预算**（`c4434613`）：口径错在把**单池上限**当**并发需求**。
  改为 `test-threads × CONNECTIONS_HELD_PER_TEST(1) + HARNESS_POOL_RESERVE(20) = 32 ≤ 100`，
  并写成真断言。红证明：`CONNECTIONS_HELD_PER_TEST` 临时改 8 → FAIL（116 > 100），
  撤销后 PASS。

### 1.5 Phase 4：CI 门禁 `29ed5655` `003a54f2` `753f29ad` `d32419e9`


- **C6** `check_web_layering.py`：加"扫描面存在且非平凡"守卫（≥20 个 `.rs`，`--update`
  也先过这道），并补上此前**磁盘上不存在**的 `scripts/ci/web_layering_allowlist.txt`。
  红证明：`WEB` 指向不存在目录 → 明确 FAIL、exit 1。
- **C9**：删除从未生效的 TDD 车道（`--tdd-files` 从未被传入，默认列表指向 gitignored
  且从未生成的 `artifacts/tdd_file_list.txt`；仓库里也没有权威清单，凭空造等于发明策略）。
- **C11** `check_schema_contract_coverage.py`：默认阈值 90 → 100。红证明：注入"少一项"
  （99.5%）→ 默认 exit 1，显式 `--threshold 90` exit 0。
- **C12** `ci-summary.needs` 纳入 `repo-sanity`。
- **A13**：接线 `check_trait_ratchet.py`（红证明：注入 `pub trait` 探针 → `65 -> 66`
  exit 1）；删除 `scripts/run_cargo_audit.sh`（`security-audit` job 已跑
  `supply_chain_gate.sh`）与 `scripts/ci/check_sqlx_offline_cache.sh`（其唯一动作
  `cargo check` 已由 `.cargo/config.toml` 的 `SQLX_OFFLINE=true` 下的 clippy 覆盖）。
  其余孤儿脚本的处置见 §3。

### 1.6 🔴 HEAD 的 clippy 两个矩阵档都是红的（计划/SWEEP 未记录）

- **复现**（与 ci.yml:285 的同一条命令，`${{ matrix.features-args }}` 分别取 `""` / `--all-features`）：
  ```
  SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings
  → error: `panic` should not be present in production code
    --> synapse-services/src/wiring/e2ee.rs:76:39   (exit 101)

  SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
  → 12 × synapse-web/src/routes/voice.rs（unwrap_used / map_unwrap / map_unwrap_or）(exit 101)
  ```
- **来源**：第一个由 `c22b41d4`（上一会话的 megolm at-rest 提交）引入；
  第二个在 `#[cfg(feature = "voice-extended")]` 的 `voice.rs` 里，默认档不编译所以一直没暴露。
  两个都是**主门禁 `test` job 的 blocking clippy 步骤**。
- **修复**：
  - `wiring/e2ee.rs`：`ServiceContainer::new` 的签名是 `-> Self`（无 Result），
    `resolve_at_rest_key` 的详细运维信息只能靠这条 panic 传出，故对该语句加
    **作用域内** `#[allow(clippy::panic)]` 并写明这是刻意的启动 fail-fast；
    正确后续是把 `Result` 穿透 `ServiceContainer::new`/`build_domains`（已记入 §5）。
  - `routes/voice.rs`：`duration_ms` 改为 `let Some(..) = … .filter(..) else { … }` 绑定
    （消除两处 `unwrap`）、`content_type.unwrap_or_else(|| infer::get(..).map_or_else(..))`、
    `existing_user.is_some_and(..)`。
- **验证**：两条 clippy 命令均 exit 0；`voice_` 单测 22/22；unit target 1787/1787；
  fmt 棘轮 0。

### 1.7 B2 零耦合 JSON 烟雾测试：删除三个整文件 `9ec10145`

- 逐个核对后删除（整文件只有 `json!` 字面量，无可指向的生产对象）：
  `tests/unit/coverage_tests.rs`(34)、`tests/unit/api_optimization_verification_tests.rs`
  （0 处 `synapse_` 引用）、`tests/unit/boundary_tests.rs`（断言 `String::len()`）。
- `tests/unit/worker_coverage_tests.rs` **保留**：核对后它确有生产耦合
  （`ReplicationCommand::parse/to_string`、`WorkerType::from_str`、serde round-trip 等），
  sweep 称其"~18 条零耦合"不成立；唯一的字面重言式已在 B3 修掉。
- 依据：`.trae/documents/测试覆盖率提升至80%优化方案-v2.md` 自己就裁定
  "全部 JSON shape 烟雾测试 … 若覆盖率无法提升则删除以避免误导"。
- 验证：unit target 1675 passed / 2 skipped；覆盖率基线里 `tests/` 条目为 0。

---

## 2. 本会话新发现

### 2.1 ✅ 已修 `39211779`：`scripts/ci/prepare_test_db.sh` 在当前基线下必然失败

- **复现**（本机，`sqlx-cli 0.8.6`，`artifacts/sqlx-migrations` 已存在）：
  ```
  TEST_DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test \
  DATABASE_URL=$TEST_DATABASE_URL bash scripts/ci/prepare_test_db.sh
  ==> [1/3] migrating public baseline into …
  error: while executing migration 0: error returned from database:
         CREATE INDEX CONCURRENTLY cannot run inside a transaction block
  ```
- **根因**（最小探针复现）：**sqlx-cli 0.8.x 不认**迁移文件首行的 `-- no-transaction`
  指令 —— 一个只含该指令 + 一条 `CREATE INDEX CONCURRENTLY` 的临时迁移仍然报同样的错；
  `sqlx migrate run --help` 也没有 `--no-transaction` 开关。baseline 有 14 处
  CONCURRENTLY ⇒ 该 seed 步骤在 ci.yml 的 3 处调用（:342 / :672 / :940）全部失败，
  `test_template_ci` 从未被真正重建。
- **修复**：改为调用**唯一实现** `scripts/init_test_public_schema.sh`（psql，逐个迁移文件、
  无事务包裹）= `docker/db_migrate.sh` 同一条路径；模板显式 `DROP … CASCADE` 后重建
  （旧版靠 `_sqlx_migrations` 判定"已应用"，会让旧 baseline 造出的模板**永远陈旧**；
  而对陈旧 schema 重放 baseline 也不是修复，实测 `column "recipient_user_id" does not exist`）。
  `init_test_public_schema.sh` 同时：
  * 加 `TARGET_SCHEMA` / `RESET_PUBLIC`（`RESET_PUBLIC=0` 不 DROP public，避免级联删掉
    其它 schema 依赖 public 扩展的对象 —— 这正是把隔离模板 10 个 `gin_trgm_ops`
    清成 0 的机制）；
  * `ON_ERROR_STOP=0`+忽略返回值 → `ON_ERROR_STOP=1`；结尾由"echo 表数"改为
    **断言 ≥100 表否则 exit 1**（旧版迁移整段没落地也 exit 0）。
- **验证**：`prepare_test_db.sh` EXIT=0，public 228 BASE TABLE / 模板 227 BASE TABLE + 10 trgm；
  红证明：`TEST_DATABASE_URL=…:59999` → EXIT=2 且报 "Connection refused"；
  隔离模板重建后 230 表 + 10 trgm；`check_schema_blind_guards.py` PASSED。

### 2.2 ✅ 已修 `4456eb80`：`test-schema-janitor` 让进程退出慢到数十分钟（不是死锁）

- **现象**：覆盖率的 integration 二进制在打印 `1424 passed` 后**卡在退出** 19 分钟以上。
  `sample` 栈：
  ```
  main-thread: exit → __cxa_finalize_ranges
                 → synapse_common::test_schema_guard::janitor_exit_handler
                 → JoinHandle::join → _pthread_join (阻塞)
  test-schema-janitor: run_release_cleanups → register_pending_schema_return
                 → run_cleanup_blocking → block_on → DROP SCHEMA …
  ```
  期间 `pg_stat_activity` 显示它**正在逐条 `DROP SCHEMA`**（377 个 `test_<pid>_*` schema），
  实测速率约 2.5 个/秒。
- **根因**：`janitor_exit_handler` 先置 `EXITING` 再 `join()`；但 janitor 当时已进入
  `run_release_cleanups(ready)` 的**串行**循环，`ready` 是 `EXITING` 置位**之前**收集的，
  所以那批（数百个）走的是"逐个 DROP"而不是 `run_exit_drain` 的**有界并行**路径。
- **修复**：`run_release_cleanups` 拆出可注入判定的 `run_release_cleanups_with`，
  每次迭代前询问"是否退出"，一旦置位就把当前条目 + 剩余全部交给 `run_exit_drain`
  （4 线程、走便宜的 `on_exit` DROP）；`janitor_exit_handler` 的 `join()` 改为**有界等待**
  （轮询 `is_finished()`，上限 120s），超时 detach 并把残留交给
  `scripts/cleanup_test_schemas.sh`，不再无限阻塞 CI。
- **红证明**：新测试 `release_pass_hands_the_remainder_to_the_exit_drain_once_exiting`
  用注入谓词（首 false 后 true）断言"只有 in-flight 条目走 `on_release`"；
  探针把判定改成 `if false && is_exiting()`（旧行为）→ FAIL
  `left: [0,1,2] / right: [0]`；撤销后 PASS。clippy 0；fmt 0；`test_schema_guard` 4/4。

### 2.3 ✅ 已修 `2904cbf4`：`events_fts_idx` 改 CONCURRENTLY + 检测 INVALID 残留

- **问题**：`synapse-storage/src/event/search.rs` 的 `CREATE INDEX IF NOT EXISTS
  events_fts_idx ON events USING GIN (…)` 由 `synapse-services/src/wiring/core.rs:109`
  在 **wiring/启动**时调用；`events` 是热表，普通 `CREATE INDEX` 整个构建期持写锁，
  大库首次启动会阻塞所有写。该索引也不在 schema 契约内（`migrations/` 里 0 处引用）。
- **修复**（用户裁定）：改 `CREATE INDEX CONCURRENTLY IF NOT EXISTS`（sqlx 裸
  `execute` 是 autocommit，满足"不能进事务"的要求）；并新增
  `fail_if_fts_index_invalid()`，在创建**前后各查一次** `pg_index.indisvalid` ——
  CONCURRENTLY 失败会留下 INVALID 索引，而 `IF NOT EXISTS` 之后永远跳过它，
  启动会"成功"但检索无索引。命中即返回明确错误并给出补救命令。
- **踩坑记录**：检查必须用 `to_regclass('events_fts_idx')`（按 `search_path` 解析）
  而非 `pg_class.relname` —— `pg_class` 是全库的，本库里存在有效的
  `public.events_fts_idx`，第一版守卫因此看错了索引（测试先红在 precondition 上）。
- **红证明**：新增 `test_create_postgres_fts_index_reports_invalid_leftover`，
  用 PostgreSQL 真正产生 INVALID 的方式复现（对含重复行的表做并发 UNIQUE 构建 →
  报 `could not create unique index` 且 `indisvalid=f`，索引保留），断言此时不能再报成功；
  用**每测试隔离 schema**避免残留污染共享池的 idempotent 测试。
  探针把检查改成 `Ok(())` → FAIL `an INVALID leftover index must not be reported as
  success: ()`；撤销后 2/2 通过。clippy 0；fmt 0。

### 2.4 🟡 覆盖率基线里仍有 88 个非 test-only 文件 <30%

按新语义它们不再红（基线只管不回退），但"新文件 30% ramp-up"这条未来会拦人。
其中不少是 `src/bin/*`（覆盖率腿不跑 bin）—— 是否把 `src/bin` 也纳入 test-only 豁免，
或给 bin 加测试，属后续取舍。

### 2.5 🟡 清理脚本的力度（Phase 5，观察项）

`docs/audit/GATE_INTEGRITY_SWEEP_2026-09-19.md` §15.8.4：候选谓词仍是"名称黑名单 +
家族正则"，将来出现**新的** shell 创建、无标记的 live 模板需手工加进 `STATIC_KEEP`。
本轮未动。

---

## 3. 用户裁定的五项决策与落地

| # | 决策 | 落地 |
|---|---|---|
| 1 | 覆盖率阈值改成"基线文件只管不回退 + 排除 test-only" | ✅ `29ed5655` |
| 2 | 删重复僵尸文件 + 给其余加 CI 接线 | ✅ `f9701d8c`（B5/B6 删，B16/B17 接线） |
| 3 | 孤儿脚本：接线高价值者 + 其余删除并同步文档 | ✅ 部分：`003a54f2` 接线 trait 棘轮、删 `run_cargo_audit.sh` + `check_sqlx_offline_cache.sh`；`753f29ad` 删 TDD 车道。**其余处置见下表** |
| 4 | PR 门禁面保持 push-only（刻意取舍） | ✅ 不改；但需确认分支保护已要求 `integration-test`/`coverage`/`build`，否则它们在 PR 上"永不运行"会静默漏检 |
| 5 | 连接预算按实际并发重算并可真红 | ✅ `c4434613` |

### 3.1 其余孤儿脚本 / 非门禁脚本的处置（未动，已裁定为"手动工具"）

| 脚本 | 引用方 | 处置 |
|---|---|---|
| `scripts/run_ci_tests.sh` | AGENTS.md / CLAUDE.md / TESTING.md / CHECKLIST.md / `ci_backend_validation.sh:193` | **保留**为本地入口。但 TESTING.md 把它列为"主门禁"而 ci.yml 是内联重实现 ⇒ 文档应注明"权威是 ci.yml，本脚本是本地便利封装"，否则双源漂移（铁律 2 的文档面） |
| `scripts/ci/run_complement_tests.sh` | TESTING.md（手动章节） | 保留（手动工具；interop 由 `e2ee-interop.yml` 承担） |
| `scripts/ci_schema_health_check.sh` | `Makefile:216`、migrations/README.md | 保留（`make` 可达） |
| `scripts/validate_config.sh` | `scripts/dev_start.sh:29`、README.md | 保留（被 dev 脚本调用） |
| `scripts/generate_sdk_ledger_fixtures.sh` | `docs/synapse-rust/LEDGER_EXPORT_SCHEMA.md`、`ledger_export_tests.rs` 注释 | 保留（fixture 生成器，非门禁） |
| `scripts/ci/check_feature_matrix.py` | 仅历史 audit 文档 | **保留但明确为手动工具**：CI 的 `test` 矩阵已覆盖 default / all-features；逐特性 `cargo check` 成本高，不建议每 PR 跑 |

---

## 4. 复现命令（本轮所有"已实测"的口令）

```bash
# 环境
export DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test
export TEST_DATABASE_URL="$DATABASE_URL"

# 播种（public + 模板）——注意 prepare_test_db.sh 当前会失败，见 §2.1；
# 现网可用：docker/db_migrate.sh migrate（public）+ 已有 test_template_ci
bash docker/db_migrate.sh migrate

# Phase 1 两条 DB 测试（空 public 也必须过）
cargo test -p synapse-e2ee --lib --all-features verification::service::tests -- --test-threads=4

# 隔离/URL 守卫
cargo nextest run --profile test --features test-utils --test unit \
  -E 'test(test_db_url_convention) | test(test_isolation_unification)'

# 完整覆盖率（约 30-40 分钟；结束前可能卡在 janitor 退出，见 §2.2）
bash scripts/run_local_coverage.sh

# 基线 + 棘轮（CI 同款）
python3 scripts/check_file_coverage.py --report coverage/lcov.info --format lcov \
  --baseline scripts/ci/coverage_baseline.json --global-floor 40 --new-file-floor 30 \
  --core-files scripts/ci/core_file_coverage_prefixes.txt --core-threshold 70

# 门禁自检
./scripts/check_fmt_ratchet.sh
python3 scripts/ci/check_trait_ratchet.py
python3 scripts/ci/check_web_layering.py
python3 scripts/ci/check_workflow_steps.py
```

> 纪律：任何"检查类"改动都要有 **故意违规 → 必须失败 → 撤销** 的红证明（AGENTS.md 铁律 8）。
> 本轮每一条都附了探针与输出。

---

## 5. 下一步建议顺序

0. **§1.6 的正解**：把 `Result` 穿透 `ServiceContainer::new` → `build_domains` →
   `E2eeServices::new`，让"缺 at-rest 密钥"成为一条干净的启动错误，去掉那条
   作用域 `#[allow(clippy::panic)]`（当前是刻意 fail-fast 的临时形态）。
1. **§2.5**：清理脚本力度（`STATIC_KEEP` 手工维护）。
2. **§2.4**：为非 test-only 的 <30% 文件补测试，或明确把 `src/bin` 也列为豁免。
3. TESTING.md / AGENTS.md 的措辞修正：fmt debt 已归零（本轮修），但棘轮计数含义是
   "差异块数 × 重复次数"；`run_ci_tests.sh` 已改标为本地封装（本轮修）。
4. sweep 剩余项（A1–A6 / A9–A12 / B7–B9 / B11–B16）：按 sweep §3/§4 的裁定逐个处理，
   每项都要红证明；A7/A8 保持 push-only 的取舍需在分支保护侧确认。
