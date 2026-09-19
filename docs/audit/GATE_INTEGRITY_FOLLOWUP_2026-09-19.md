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
| Phase 5 观察项 | ✅ 三条全修（`prepare_test_db.sh` `39211779`、janitor 退出 `4456eb80`、`events_fts_idx` `2904cbf4`）；清理脚本力度部分修（`8cfb683f`，剩隔离模板标记约定待抽公共实现） |
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

### 1.8 CI 等效验证跑了一次（真实 CI 无法触发）

`gh auth status` 显示 token 失效（`The token in keyring is invalid`）且没有收到推送指令，
**无法触发/观察真实 GitHub Actions**，故在本地逐条复刻 `.github/workflows/ci.yml` 的步骤：

| CI 步骤 | 本地结果 |
|---|---|
| fmt 棘轮（`check_fmt_ratchet.sh`） | ✅ current=0=baseline |
| clippy 默认档（`--workspace --all-targets --features test-utils -D warnings`） | ✅ 0 error |
| clippy `--all-features` 档 | ✅ 0 error |
| doc tests（`cargo test --doc --workspace`） | ✅ 0 failed（**0 个测试** —— 这就是 AGENTS.md 说的空门禁） |
| seed（`ci/prepare_test_db.sh`） | ✅ EXIT=0，public 228 + 模板 227 BASE TABLE + 10 trgm；不可达库时 EXIT=2 |
| workspace lib `--all-features --test-threads 4`（**带 `NEXTEST_RETRIES=2`**，与 CI 的 env 一致） | ✅ **6081/6081 passed（1 flaky）**，EXIT=0 |
| 同一命令**不带 retries**（更严格） | ❌ 三次跑出三个不同失败：`beacon::db_tests::test_create_and_get_beacon_info`（已修，见 `fff782dd`）、`synapse-common test_isolation::tests::validate_clone_rejects_an_incomplete_clone`、`synapse-services media::tests::test_chunked_complete_can_be_downloaded_via_media_service`；三条都能单独通过 |
| repo-sanity 各门禁 | ✅（下表） |

repo-sanity：schema table coverage / migration consistency / route-storage boundary /
sqlx ratio / web layering / trait ratchet / workflow steps / schema blind guards /
schema contract coverage / connection budget / route layering **全部 exit 0**。

**两个结论**：
1. **CI 的 `test` job 在「4 线程 + 2 次重试」下是绿的**，但 `--test-threads 4` 下存在
   跨 crate 资源争用导致的**偶发失败**（同一次提交三次跑出三个不同测试），
   由 `NEXTEST_RETRIES=2` 掩盖。这与仓库既有记录一致
   （`TEST_THREADS >= 6` 需先 `tune_test_db.sh`；本机 `max_locks_per_transaction=256`、
   `max_connections=100`）。**这是残留的测试稳定性问题，不是产品缺陷**，
   但值得单独一轮把这三条查明（它们的共同点是重 DB + 共享/克隆 schema）。
   → **已在 §1.9 查明并结构修复**：`media::tests`（fixture 提前释放 schema 租约 → 查询静默落回
   `public`）与 `validate_clone_rejects_an_incomplete_clone`（无锁 prune 删掉了正在构建的模板）；
   本轮复跑时又当场抓到同类的第三条 `audit::db_tests`（共享 `public` + 全表 sweep），见 §1.9.3。
2. 门禁确实在拦人：`check_schema_table_coverage.py` 抓到了我新增测试里的
   `INSERT INTO fts_invalid_probe`（一次性表名）并 exit 1 —— 已改用 CTAS 消除
   （`34dd5982`），**没有**加例外。

### 1.9 ✅ 测试稳定性：两条偶发失败的根因（各自"复现 → 结构修复 → 红/绿证明"）

§1.8 结论 1 记的三条偶发失败里，beacon 已在 `fff782dd` 修掉；本轮把剩下两条查成了
**两个互不相同的结构缺陷**，都在测试基础设施里，都**没有**用重试/串行化绕过（AGENTS.md 铁律 7）。

#### 1.9.1 `media::tests::*`（chunked download + quota）：fixture 提前释放了 schema 租约

- **机制**：`setup_test_media_domain*` 用 `prepare_isolated_test_pool()` 建 per-test schema，
  把 `Arc<PgPool>` 交给服务后**在 setup 返回时丢掉了最后一个 `Arc<PgPool>`**；而 media 侧三个
  storage（`ChunkedUploadStorage` / `MediaQuotaStorage` / `AdminMediaStorage`）当时都写
  `(**pool).clone()`，只留下**内层** `PgPool`。`test_schema_guard` 的 janitor 用
  `Weak<Arc<PgPool>>` 判断"池已释放"，于是它在测试体仍在查询时 `DROP SCHEMA`；连接的
  `search_path` 仍指向已消失的 schema，**未限定表名的查询静默回落到共享 `public`**。
- **复现（探针，改前）**：
  `PROBE schema=test_63977_1_1789820587877316000 dropped_while_in_use=true current_schema=public`
  （临时 `#[ignore]` 探针：建池 → 取内层 `PgPool` → drop 外层 `Arc` → 轮询 `to_regnamespace`）
- **同期 CI 日志佐证**（同一机制在三种数据状态下的三种表现）：
  - `/tmp/svc_lib_run3.log`：`test_delete_media_rolls_back_quota_usage` 失败于
    `23503 … schema: Some("public"), table: Some("user_media_quota")` —— 写入落到了 public，FK 指向 public.users；
  - `/tmp/svc_serial2.log`：`relation "upload_progress" does not exist` —— public 也没有该表时，静默回落变成响亮的 42P01；
  - `/tmp/lib_all_full.log`：`left: []` —— chunk 写进隔离 schema、读取落到 `public.upload_chunks`（0 行）→ 空文件。
- **修复**：三个 storage 改为持有 `Arc<PgPool>`（查询处 `&*self.pool`），
  于是 fixture 返回的服务本身把租约留到测试结束。
- **红/绿证明**：新增常驻守护 `media_fixture_keeps_its_isolated_schema_for_the_whole_test`
  （setup 返回 → 等 4 个 janitor 轮询周期 → 写一条 `upload_progress` → 断言 `public` 里没有它）：
  降级 storage 时它复现了与 CI **完全相同**的报错
  （`23503 / schema: Some("public") / table: Some("user_media_quota")`），修复后通过。

#### 1.9.2 `validate_clone_rejects_an_incomplete_clone`：无锁 prune 删掉了"半成品模板"

- **机制**：`prune_isolation_templates` 的规则是"**没有 readiness marker 表 = 构建被打断 = 可删**"。
  该规则只在"没有 builder 正处在 `CREATE SCHEMA` → `CREATE TABLE <marker>` 之间"时成立，
  即 builder 全程持有 `TEMPLATE_ADVISORY_LOCK_KEY` 的那个窗口。但公开入口
  `prune_stale_isolation_templates` **自己不取锁**（只有注释写"调用方需持锁"）。
  同二进制的 `prune_backfills_legacy_zero_row_marker_and_spares_it` 调它，
  并发时就把 `validate_clone…` 正在构建的短模板删了，builder 随后失败（0.2s 级快失败）。
- **复现（hammer 探针，改前）**：builder 与 prune 循环并发 30 轮：
  `PROBE mid_build_failures=30 of 30; first=Some("failed to seed the readiness marker of template … relation \"…_synapse_test_template_ready\" does not exist")`
- **修复**：公开入口自己 `acquire_template_lock(…)` / `pg_advisory_unlock(…)`，
  把"需持锁"从注释升级为不变量。
- **红/绿证明**：探针转为常驻守护 `concurrent_pruning_does_not_drop_a_mid_build_template`
  （8 轮）：改前 30/30 失败；修后 30/30 通过；去掉公开入口的锁该测试即变红。

#### 1.9.3 ✅ `audit::db_tests`：共享 public schema + 全表 sweep（本轮证据跑当场抓到）

- **机制**：`audit::db_tests::test_pool()` 用的是 `connect_shared_test_pool()`（共享 `public`），
  而 `delete_events_before(now)` 会删掉**所有**早于该时间戳的审计事件 ——
  一个测试的清理会删掉另一个测试刚插入的 fixture 行。
- **证据（本轮 3×全量跑的第 1 次，`--test-threads 4`，无 retries）**：
  `FAIL synapse-storage audit::db_tests::test_delete_events_before_bypasses_append_only_guard`
  → `panicked at synapse-storage/src/audit.rs:390: should have deleted at least the test event`
  （即 `deleted == 0`）。同模块的 `test_audit_events_reject_unflagged_delete` 在 §1.8 的
  CI 等效跑里也是 `TRY 1 FAIL → TRY 2 PASS` 的重试掩盖对象。
  注意后者源码里原本就写着一段"db_tests 共享 public、并发下兄弟测试的 `delete_events_before`
  可能已经删掉本行，所以不断言行数"的注释 —— 这正是把共享状态当成既定事实来绕过的写法。
- **修复**：`audit::db_tests::test_pool()` 改用
  `crate::test_isolation::isolated_test_pool()`（per-test schema），两个调用点绑定 `_isolated` 租约。
  这是 `beacon::db_tests` 在 `fff782dd` 用过的同一修法（铁律 7：消除共享，而不是串行/重试）。

#### 1.9.4 ✅ `url_preview_storage::db_tests`：同一个"共享 public + 全局 sweep"（第三次复跑抓到）

- **机制**：`cleanup_expired_previews(now)` 删掉当前 schema 里**所有** `expires_at <= now` 的行，
  而这些 fixture 的时间戳钉在 `BASE_TS = 1700000000000`（2023-11），相对墙钟早已过期。
  共享 `public` 下，兄弟测试的一次 sweep 就能删掉本测试刚 save 的行。
- **证据（本轮 3×全量跑的第 3 次）**：
  `FAIL … url_preview_storage::db_tests::test_round_trip_all_option_fields_none`
  → `panicked at synapse-storage/src/url_preview_storage.rs:438: preview should be found`。
- **修复**：同一修法 —— `test_pool()` 改用 `isolated_test_pool()`，6 个调用点绑定 `_isolated`。

#### 1.9.5 🟡 同类残留风险（已记录，本轮未展开）

1. **`synapse-storage` 的共享 `public` db_tests 是成片的**：51 个文件仍用
   `connect_shared_test_pool()`，其中约 20 个模块同时存在"全局 sweep"型操作
   （`cleanup_expired*` / `delete_*_before` / `purge_*` / `DELETE FROM <table>` 无前缀过滤等），
   也就是**任何一个都可能复现 §1.9.3 / §1.9.4**。判定口径：文件既 `connect_shared_test_pool`
   又含全局 sweep 即高风险。整批迁移到 `isolated_test_pool()` 是机械但面广的一轮工作。
2. **janitor 的租约口径仍是"最后一个 `Arc<PgPool>` 被释放"**，而仓库里仍有十几处 storage
   只持有内层 `PgPool`（`grep -rn '(**pool).clone()'`）。任何"fixture 造好服务后丢掉 `Arc`"
   的组合都会复现 §1.9.1（media 只是被 CI 撞到的那一个）。结构性收敛方向是把租约从
   "Arc 计数"换成**连接级租约**（隔离池 `after_connect` 里按 schema 名取一把 session advisory
   lock；janitor 释放前 `pg_try_advisory_lock` 失败即让位重排），一处生效、与具体 storage 是否
   降级无关；改动面覆盖全部隔离池构造点，需单独一轮。

#### 1.9.6 本轮门禁与证据汇总（全部本地等效，真实 CI 仍无法触发）

| 项 | 结果 |
|---|---|
| fmt 棘轮 | ✅ `current=0 baseline=0` |
| clippy 默认档（`--workspace --all-targets --features test-utils`） | ✅ `CLIPPY1_EXIT=0` |
| clippy `--all-features` 档 | ✅ `CLIPPY2_EXIT=0` |
| sqlx 动态/静态棘轮 | ✅ `dynamic=1484 <= 1484`，`static=61 >= 61`（新增的守护测试改用生产读路径，未抬基线） |
| unit 目标（`--test unit --features test-utils --test-threads 4`） | ✅ **1679 passed / 2 skipped / 0 failed**（含新增 `workflow_pipefail_tests` 2 条） |
| 全量 `--workspace --lib --all-features --test-threads 4`（**无 retries**） | 见下方"确认跑" |

**确认跑（无 retries，`--no-fail-fast`）**：

- 跑 1：❌ 1 失败 = `audit::db_tests::test_delete_events_before_bypasses_append_only_guard`（§1.9.3 的现场证据）
- 跑 2：✅ 6083/6083
- 跑 3：❌ 1 失败 = `url_preview_storage::db_tests::test_round_trip_all_option_fields_none`（§1.9.4 的现场证据）
- 跑 4/5：✅ **6083/6083 ×2**（冻结树、修复全部落地后、同一命令连跑两次，`--test-threads 4` 无 retries）

结论：用户点名的两条（`validate_clone_rejects_an_incomplete_clone`、
`media::tests::test_chunked_complete_can_be_downloaded_via_media_service`）在本轮所有复跑中
**一次都没有再出现**；跑 1/跑 3 暴露的是同一类的其他两条，已一并按同一原则修掉。

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

### 2.5 🟡→✅ 部分已修 `8cfb683f`：清理脚本的 live 模板保护

- **原观察**（sweep §15.8.4）：候选谓词是"名称黑名单 + 家族正则"，将来出现**新的**
  shell 创建、无标记的 live 模板需手工加进 `STATIC_KEEP`；漏一次就是一次 `--apply`
  CASCADE 误删。
- **已修**：`scripts/ci/prepare_test_db.sh` 现在按
  `synapse-test-utils::template_marker_dir()` 的同一约定写标记文件，于是清理脚本用
  通用机制（keep reason #1）就能认出 `test_template_ci`；`STATIC_KEEP` 保留为无条件兜底。
  实测：预演输出 `由标记文件认定的 live 模板: 1 个 test_template_ci`，且候选里不含它。
  脚本内已写明"新增 shell 建的 live 模板要写标记，而不是加名单"。
- **仍存的缝（已关闭）**：`test_isolation_template_<fingerprint>` 原本只用 **schema 内标记表**
  标记，清理脚本看不见它，于是会把"当前那个"当候选删掉（下次测试重建 ~2s，无正确性影响）。
  现已把标记路径抽成 `synapse_common::test_isolation::{template_marker_dir,
  template_ready_marker_path}` 的**唯一实现**，供 `synapse-test-utils`、隔离模板与清理脚本
  共用；`build_template` 在写 schema 内标记表的同一处也写文件标记，`prune_stale_isolation_templates`
  删除被清模板的标记文件。实测：隔离模板重建后出现
  `synapse_test_template_ready_test_isolation_template_b6a8b06fb13d22f9`，清理预演把它列为
  **由标记文件认定的 live 模板**（2 个）而不再是候选；删掉该标记文件则它立刻重新成为候选
  （6 个）、放回后又是 5 个 —— 证明是标记在保护它。

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

## 5. 下一步建议顺序（⚠️ 旧版本，已由 §7 取代；仅保留历史）

已完成（见上文各节）：§1.6（`Result` 穿透）、§2.1、§2.2、§2.3、§2.5、A10、A12、
B8（**仅扫描面**；匹配面见 §6.5）、B13、B14、B15、C7、D1，CI 等效验证（§1.8），以及
CI 等效跑发现的 beacon 竞争与表覆盖门禁命中的一次性表名。
**注**：本行此前把 A9 记为"已完成"，第二轮核实（§6.5）表明 A9 只是**部分修复**
（`mutation-testing.yml:37` 的 job 级 `continue-on-error` 仍在）；且"`ledger-export.yml` job 级 COE"
的说法是错的（该文件只有可选通知 step 的 step 级 COE）。以 §6/§7 为准。

## 6. 逐项核实（第二轮，2026-09-19 晚）：用户列出的 5 组问题

> 方法：三路只读审计（A1–A6 / A7–A12 / B7–B16）直接读文件、跑 `grep`/`git log -S`，不采信本文件旧结论；
> §1.6/§2.4/§2.5/文档措辞由我本人核实。真实 CI 仍无法触发（`gh` token 失效，见 §1.8）。

### 6.1 §1.6 `Result` 穿透 —— ✅ 已解决（本轮之前 `a30ea637`）

- `synapse-services/src/wiring/e2ee.rs:62-68` `E2eeServices::new(...) -> Result<Self, String>`；
  `:159-187` `resolve_at_rest_key(...) -> Result<[u8;32], String>`，缺键时返回**命名了两个配置键**的
  `Err`（`megolm_encryption_key_path` / `macaroon_secret_key`），不再 panic。
- `synapse-services/src/container.rs:146` `ServiceContainer::new(...) -> Result<Self, String>`、
  `:306` `build_domains(...) -> Result<DomainPhase, String>`；`src/server/services.rs:66-68` 用
  `.map_err(...)?` 把它变成启动错误。
- 全仓已无生产路径的作用域 `#[allow(clippy::panic)]`：仅剩
  `src/bin/synapse_worker.rs:1` / `synapse_ledger_export.rs:2` 的 `#![cfg_attr(test, allow(clippy::panic))]`（测试专用）。
- 守护：`tests/unit/container_service_tests.rs:244` `container_construction_reports_a_missing_megolm_at_rest_key`
  断言 `Err` 且错误文本含两个键名。

### 6.2 §2.5 模板 ready 标记路径统一 —— ✅ 已解决（`8cfb683f` + 后续），留 1 个守卫缺口

- 单一实现：`synapse-common/src/test_isolation.rs:47` `TEMPLATE_READY_MARKER_PREFIX`、
  `:63` `template_marker_dir()`、`:82` `template_ready_marker_path()`；模块**无条件编译**
  （`synapse-common/src/lib.rs:89 pub mod test_isolation;`，不在 `cfg(test)` 下）。
- 隔离模板也写标记文件：`build_template` 在写完 in-schema 标记后于 `:732-742` 写
  `synapse_test_template_ready_<schema>`（best-effort + warning）；prune 时 `:631-634` 同步删文件标记。
- 消费者：`synapse-test-utils/src/lib.rs:928` 直接 `use synapse_common::test_isolation::template_ready_marker_path`；
  `scripts/cleanup_test_schemas.sh:123-134` 用同一目录/前缀构造 keep 集合，无标记时降级 `--keep-all-templates`；
  `scripts/ci/prepare_test_db.sh:82-86` 也写同一约定的标记。
- 🟡 **缺口**：三个写入方（Rust 常量、cleanup 脚本、prepare_test_db 脚本）之间**没有守卫**钉住前缀/目录一致
  （`grep -rn "TEMPLATE_READY_MARKER_PREFIX" tests/` 为空）。改常量或改脚本任一侧都会静默漂移
  （后果：清理脚本认不出 live 模板 → 误删 → 重建约 2 秒，或更糟：删掉正在被并发 clone 的模板）。
  **下一步**：加一条只读守卫测试（读常量 + 断言两个脚本含 `<PREFIX>_` 与 `synapse_test_templates`），
  红证明 = 改常量后该测试失败。

### 6.3 §2.4 覆盖率 <30% 文件 —— 🟡 决策未落地，但**已被基线豁免**（不是红门禁）

- 复算：`scripts/ci/coverage_baseline.json` 623 条中非 test-only 且 <30% 的仍是 **88** 个；
  其中 **46** 个命中 `core_file_coverage_prefixes.txt` 的 29 个前缀；**37** 个行覆盖为 **0.0%**
  （`src/main.rs`、`src/server/{router,services,telemetry}.rs`、`src/bin/*`、`src/common/error.rs`、
  `synapse-common/{config/manager,logging}.rs` …）。
- 棘轮语义（`scripts/check_file_coverage.py:370-390`，优先级 **baseline-known > core > new**）：
  基线已知文件 `floor = prev`（只管不回退）⇒ 这 88 个今天**不红**，`--core-threshold 70` 对它们不生效。
- 🟡 **真正的两个坑**：(1) **新文件**走 30%（core 前缀走 70%）；(2) **重命名/移动**一个低覆盖的 core 前缀文件
  会让它变成 "new" → 直接按 70% 判 → 必红（例如 `synapse-storage/event/*.rs` 现为 0–20%）。
- **下一步（二选一，建议 A）**：
  A. 把"覆盖率腿不执行"的路径显式豁免并加注释清单：`src/bin/**`、`src/main.rs`（可选 `src/server/{router,services,telemetry}.rs`），
     同时在 `check_file_coverage.py` 里为该豁免加**只读守卫测试**（清单里的路径必须真的不可从 `--lib`/`--test unit` 触达），
     红证明 = 往清单塞一个普通模块 → 测试变红。
  B. 保持现状，只在文档里写明"重命名 core 低覆盖文件会触发 70% 地板"，避免下一个人踩坑。

### 6.4 TESTING.md / AGENTS.md / CLAUDE.md 措辞 —— ✅ 本轮修正

- `TESTING.md`（更早一轮已修）：`:111`、`:438` 已把 `run_ci_tests.sh` 标为"本地便利封装；不在 CI 中调用；改 CI 需同步（sweep A13）"。
- `AGENTS.md` **本轮修正**（此前是错的）：
  - `:11` 原写"counts diff blocks via `grep -c '^Diff in'`"（那是坏掉的旧实现）→ 改为"独立 `rustfmt --check` 的
    `Diff in` 块计数；同一文件可有多个块（实测 2）；`cargo fmt -- --check` 不打印该标记，所以旧计数恒 0"。
  - `:13` 原写"CI clippy 不覆盖 workspace 测试代码（只有 `-p synapse-services --tests`）"→ 改为 CI 两档
    `--workspace --all-targets --features test-utils [--all-features]`（`ci.yml:301`），旧缺口已关。
  - `:16` 原写"CI-equivalent Rust test entrypoint: `run_ci_tests.sh`" → 改为"本地 CI 复刻（**非** CI 入口，无 workflow 调用）"。
- `CLAUDE.md` **本轮修正**：`:12` clippy 命令、`:28` "Full CI suite"→"Local CI replica (not the CI entrypoint)"、
  `:376` 速查表同步。

### 6.5 sweep 剩余项逐项核实

**A 组（workflow 接线）**

| 项 | 核实结论 | 证据 / 残留 |
|---|---|---|
| A1 | ✅ 已修 `e60832c6` | `e2ee-interop.yml:10` 含 `synapse-e2ee/**`；`scripts/ci/require_tests_ran.sh` 让"0 命中"变 exit 1；3 处 tee 均有 pipefail。残留：19 条 interop 测试在 `E2EE_INTEROP!=1` 时**早退仍计为 passed** |
| A2 | ✅ 已修 `e60832c6` | 5 步改为 `--test integration <mod>_migrated` + 包装器；模块真实存在。残留：feature 列表是显式 6 个而非 `--all-features` |
| A3 | ✅ 已修 `e60832c6` | 不存在的测试名只剩注释；改为 4 个真实名字循环 + 包装器。残留：`database_integrity_tests::connect_integrity_pool()` 返回 `None` 时测试**早退即 passed** |
| A4 | ✅ 已修 `e60832c6` | 改用 `--test integration invite_blocklist_tests_migrated`（5 条真测试）；`tests/unit/msc_tests.rs:41` 的玩具模块仍在但无人引用 |
| A5 | ✅ 已修 `dee46f6e` | `db-migration-gate.yml:31` `set -o pipefail` 在 `:33` 的 `\| tee` 之前 |
| A6 | ✅ 已修 `dee46f6e` + `7eb97e47` | 6 个**步骤**、9 处 tee 全部有前置 pipefail；常驻扫描门禁 `tests/unit/workflow_pipefail_tests.rs`（我 Python 复刻扫描 7 块 / 0 违规）。§5 旧文案的"4 处/e2ee 2 处"是错的（分别应为 4 处 tee=1 个块、e2ee **3** 处） |
| A7 | ❌ **未修（刻意取舍）** | `ci.yml:568/850/888` 的 `integration-test`/`build`/`coverage` 仍是 `schedule \|\| push(main/develop)`；PR 上 skip。取舍只在 audit 文档里，`AGENTS.md`/`TESTING.md` 零记录 |
| A8 | ✅ 已修 `4565cecd`/`68b0ec83` | `scripts/ci/coverage_baseline.json` 已入库（623 文件）；CI 引用的路径已更新；`check_file_coverage.py` 增加 bootstrapping 豁免，C1 死锁消除。残留：ratchet 仍只在 push/schedule 的 coverage job 里跑（受 A7 影响） |
| A9 | 🟡 **部分** | `mutation-testing.yml:37` **job 级 `continue-on-error` 仍在**（新增的 `require_mutation_report()` 只能让 step 红，job 永不阻断）；`ledger-export.yml` **没有** job 级 COE——只有 `:74` 可选通知 step 的 step 级 COE，**本文件 §5 旧文案这一点是错的** |
| A10 | ✅ 已修 `a8ce30f4` | `db-replica-consistency.yml:17-41` preflight + `configured` 输出；`db-tests-manual.yml:77` `ON_ERROR_STOP=1`，`\|\| echo` 已删 |
| A11 | ❌ **未修（3 个子问题全在）** | `drift-detection.yml:14-16` PR 只挂 `branches:[main]`；`:225-238` 单目录 `uniq -d` 结构性恒空；`:330` 硬编码 v12 文件名 |
| A12 | ✅ 已修 `a8ce30f4`（有 PR 覆盖缺口） | `e2ee-interop.yml:125-129` strict + `SDK_CONTRACT_STRICT=1`；缺 SDK + `GITHUB_ACTIONS=true` 时 `check_route_contract.sh:72-74` 发 warning。残留：strict 步骤所在 job 只在 nightly/dispatch/feature 分支触发，普通 PR 仍不跑 |

**B 组（Rust 弱守卫）**

| 项 | 核实结论 | 证据 / 残留 |
|---|---|---|
| B7 | ❌ 未修 | `perf_gate_honesty_tests.rs:74-82` 仍是三条文本断言；`\|\| script.contains("missing")` **仍在**（脚本散文即可满足）；`compute_perf_gate.sh:54` 的 `:-1` 无任何断言；无用例执行脚本/断言退出码 |
| B8 | 🟡 部分 | 扫描面已修（`SCAN_ROOTS` 8 根含 `tests`，非空性断言 ≥100 文件/≥10 在 tests 下），docstring 已诚实；但匹配仍只有 `.ok();`；**187** 处 `let _ = …execute(…).await;`（test-support 子集 **99**）仍未强制，docstring 声称的"债务记录"在 docs 里**不存在** |
| B9 | ❌ 未修 | `schema_lifecycle_guard_tests.rs:48-56` 扫描根仍缺 `synapse-test-utils/src` 与 `tests`；非空性仍只有 `total_sites > 0`。根外真实站点：test-utils 6 处 + tests 2 处 |
| B11 | ❌ 未修 | `migrations/` 只有 1 个 baseline ⇒ 两个守卫的 `entries` 为空、断言 `violations.is_empty()` 空集恒真；无非空性守卫、无"合并基线"豁免注释 |
| B12 | 🟡 部分 | 第一条断言已钉住唯一测试名；第二条仍是文本代理 `src.contains("block_room") && src.contains("MOCK DEVIATION")`，可被无关生产函数/断言消息满足 |
| B13 | ✅ 已修 `fff782dd` | `read_dir`/`read_to_string` 失败 panic；非空性 ≥50 模块 + fixtures 非空；`iter().min().unwrap()` 区间断言已删；实现与 docstring 一致 |
| B14 | ✅ 已修 `fff782dd` | 调用生产 helper、钉住 digest `655dc0be…`、独立重算、对 admin/user_type/password/nonce/secret 逐个 `assert_ne!` |
| B15 | ✅ 已修 `fff782dd` | `nextest_marker_present(value)` 注入环境值，测试在 `cargo test` 与 nextest 下都真跑 |
| B16 | ✅ 已修 `f9701d8c` | `ci.yml:419-424` 新增 `-p synapse-web --lib --features test-utils`；`derived_manifest_tests` 3 条在该 feature 集下确实编译（commit body 记录 3 run/3 passed，此前 0） |

### 6.6 A7/A8：分支保护侧需要人工确认（无法用 `gh` 查询）

> **状态（2026-09-19，据维护者提供的 GitHub 设置页截图核对）**：`main` 的 required status checks **为空**
> （“尚未添加任何检查”）⇒ 三个 push-only job 确认不在 required（A7/A8 的可执行检查项全部满足）；
> `Require branches to be up to date` 已勾选但**当前不生效**（无 required 检查）；`Require a pull request`、
> `请勿允许绕过以上设置` 均未勾选。另有 `develop` 规则，但页面显示**适用于 0 个分行**，且远端实测
> 没有 `develop` 分支（`git ls-remote --heads origin` = main + 2 个 feature 分支）⇒ 该规则不生效，
> CI 中所有 `develop` 触发条件均为空转配置 —— **已按维护者指示清理**：9 个 workflow 的 push/PR
> `branches`、三个 push-only job 的 `if:` 与相关注释不再引用 `develop`（`grep -rn develop .github/workflows/`
> 为空，YAML 可解析，push-only 钉住测试与 `check_workflow_steps.py` 仍绿）。
> 结论：A7/A8 要核对的可执行项**全部满足**（三个 push-only job 在任何规则里都不在 required）；
> 反过来两条规则的 required 列表都是空的，所以目前"PR 全绿"没有任何强制力。
> 策略层面的未决问题：是否把常开门禁设为 required、是否给 `develop` 加规则。
>
> **裁定（2026-09-19，维护者确认）：保持 push-only。** 三个 job 不加入 `pull_request`；PR 侧只要求
> 常开门禁。决定已用 `tests/unit/ci_test_scope_tests.rs::push_only_ci_jobs_keep_their_deliberate_trigger_scope`
> 钉住（非空性：恰好 3 个 job；红证明：把 `integration-test` 的 `schedule` 换成 `pull_request` →
> 该测试 FAILED 并提示"这是该门禁唯一真正运行的地方"）。**剩下只需人工确认**：这三者不在 required
> checks 里（否则 PR 会卡在 Expected），且 `Mutation Testing (nightly, REPORT ONLY …)`/`Secrets preflight`/
> `Logical Checksum Compare` 也不在 required 里。

`ci.yml` 中承载受影响门禁的 check 名（矩阵展开后）：
- `Integration Tests`（`integration-test`，`:568`）
- `Code Coverage`（`coverage`，`:888`）—— 覆盖率 + per-file 棘轮
- `Build Check (core-matrix-min)` / `Build Check (core-private-chat)` / `Build Check (all-extensions)`（`build`，`:850`，矩阵 `:860-866`）
- 常开 PR 门禁（需一并确认是否 required）：`Repo Sanity`、`Test & Lint (stable, default-features)`、
  `Test & Lint (stable, all-features)`、`Test & Lint (1.93.0, default-features)`、`Test & Lint (1.93.0, all-features)`、
  `Security Audit`、`PR Benchmark Gate`

需要人确认的 3 件事：(a) 上述三个 push-only job 是否在 required checks 里；(b) 如果它们在 required 里，
PR 侧"从未上报/skip"的 check GitHub 如何判定（Expected 会不会卡住合并）；(c) 确认
`Mutation Testing (nightly, REPORT ONLY — not a merge gate)`、`Secrets preflight`、`Logical Checksum Compare` **不在** required 里。

---

## 7. 修正后的下一步清单（替代 §5 的旧版本，按投入产出排序）

> 纪律不变：每条"检查类"改动都要 **故意违规 → 必须失败 → 撤销** 的红证明（铁律 8）；
> 每条的完成判据写进本文件，不要只写"已修"。

1. **A9 收口（最小、最诚实）**：`mutation-testing.yml:37` 的 job 级 `continue-on-error: true`
   要么删掉（让 `require_mutation_report()` 真能阻断），要么在 job 名/注释里把"REPORT ONLY"写死
   并确认它不在分支保护里。**红证明**：删掉 COE 后注入 `mutants.out` 缺失 → job 红。
   同时修正本文件 §5 关于 `ledger-export.yml` job 级 COE 的错误描述（§6.5 已给正确事实）。
2. **B9 / B11 补"扫描面非空"守卫（各 1 条断言 + 红证明）**：B9 把 `synapse-test-utils/src`、`tests`
   加入扫描根；B11 为两个守卫加"entries 为空时必须显式声明合并基线豁免"的断言。
   **红证明**：把扫描根改回旧集合 / 清空 entries → 测试红。
3. **B7 改成真执行 + 正反控**：用 `Command` 跑 `compute_perf_gate.sh`，断言
   (a) 默认 strict（缺少测量必须非 0 退出）、(b) 显式 `COMPUTE_PERF_GATE_STRICT=0` 时为 0，
   删掉 `|| script.contains("missing")`。**红证明**：把脚本 `:-1` 改成 `:-0` → 测试红。
4. **DB 门禁"假跳过"统一收口（A2/A3 残留）**：`database_integrity_tests::connect_integrity_pool()`
   与 `tests/common/mod.rs::db_tests_required()` 都自建了一套"DB 不可达就跳过"的判断
   （铁律 2：同一职责两份实现）。统一到 `tests/integration/mod.rs::skip_or_fail_without_db()`
   （已存在、已按 `CI` 决定 fail-closed）。**红证明**：CI=1 且 DB 不可达 → 测试必须红，而不是 pass。
5. **A1 残留**：`require_tests_ran.sh` 只能证明"跑了 ≥1 条"，无法证明"断言真的执行了"。
   让 interop 测试在 `E2EE_INTEROP!=1` 时 **panic（CI 下）** 而不是早退，或让包装器把
   "全部测试都走早退分支"识别为失败（成本更高）。**红证明**：去掉 env 变量 → 步骤红。
6. **§2.5 标记约定守卫（§6.2）**：一条只读测试钉住 `TEMPLATE_READY_MARKER_PREFIX` 与两个 shell 脚本的
   前缀/目录一致。**红证明**：改常量 → 测试红。
7. **§2.4 覆盖率豁免决策（§6.3）**：建议方案 A（显式豁免 `src/bin/**` + `src/main.rs`，附只读守卫）；
   否则方案 B（只在文档写明"重命名 core 低覆盖文件会触发 70% 地板"）。
8. **A11 三处修复**（各自可独立提交）：PR 触发补 `develop`；重复迁移检查改为跨目录（或对
   `migrations/` 与 `artifacts/sqlx-migrations` 同时扫描）；去掉硬编码 v12 文件名（用
   `ls migrations/00000000_*.sql` 或复用 `migrations/README.md` 的约定）。**红证明**：构造一个重复
   basename 的临时迁移 → 检查必须红。
9. **B8 / B12 补强**：B8 把 `let _ = …await;` 形态纳入扫描（或把 99 处按模块 allowlist 白名单化并把
   债务写进 `docs/`——目前 docstring 指向的债务记录不存在，属文档撒谎，至少要删掉那句话）；
   B12 把第二条断言改成"真的调用了 `block_room`"的语法级/行为级检查。
10. **A7/A8 取舍落地（§6.6）**：人工确认分支保护后，把结论写进 `TESTING.md`（当前
    `AGENTS.md`/`TESTING.md` 对"这三个 job 只在 push 跑"零记录）。若确认它们不在 required 里，
    至少把 CI 预算取舍写成明文决定，避免下一个人再当漏洞重报。
11. ✅ **C3 / C8 / C10、D2、E4 / E6 / E7 / E8 / E9**：第四轮已逐项核实并修复（见 §9）。其中 E4 只做到了解析器 fail-closed（审计所述自比不成立），真实修法需改 benches（见 §9 残留）；另新增 3 条待办：route-table.json 需有意重新生成（缺 2 条未门控路由，且 CI 用 ledger.json 生成 1292 条与提交的 1047 不一致）、E4 的真实 DB 分页基准、`extract_registered.py` Python 侧棘轮仍单向 + `EXTRACT_STRICT=1` 在 HEAD 上的 3 类既有失败。

已完成且本轮**已复核**的：§1.6、§2.5（代码侧）、A1–A6、A8、A10、A12、B13、B14、B15、B16、§1.9 的四条测试稳定性修复。

---

## 8. 第三轮落地：§7 清单的执行结果（全部带红证明）

> 规则：每项都是"先复现弱点 / 制造违规 → 变红 → 撤销 → 修 → 绿"。改动未提交前先在本地跑聚焦用例；
> 合并后统一跑 fmt/clippy/unit/全量 lib（见本节末尾"门禁"）。

| §7 项 | 状态 | 改了什么 | 红证明（关键输出） | 残留 |
|---|---|---|---|---|
| 1 A9 | ✅ | `.github/workflows/mutation-testing.yml` **删掉 job 级 `continue-on-error`**（保留 `cargo mutants … \|\| true` 与 "REPORT ONLY" 命名，并改写注释说明"只在报告无法产出时失败，突变存活不算失败"） | 抽出 `require_mutation_report()` 逐字逻辑：无 `mutants.out` → `exit 1`；存在 → `exit 0`；YAML `safe_load` 通过；`grep continue-on-error` 只剩注释 | job 仍 `if: always()` 上传 artifact（不变） |
| 2 B9 | ✅ | `tests/unit/schema_lifecycle_guard_tests.rs`：两份重复根列表合一为 `SCAN_ROOTS`（9 根，新增 `synapse-test-utils/src`、`tests`）+ `EXPECTED_SCAN_ROOT_COUNT` + 6 个已知站点必须被扫到 | 在 `tests/` 注入 `CREATE SCHEMA probe_without_cleanup` → 新守卫 FAILED（同输入跑 HEAD 版本守卫 `ok`）；删 `"tests",` → `left: 8 right: 9`；删站点文件断言 → 点名未找到 | 站点计数 = 6；注释识别仍是行首式（既有行为） |
| 3 B11 | ✅ | 两个守卫在 `entries` 为空时改为：独立重扫并断言"前向 `.sql` 恰好 1 个且等于 BASELINE"，并打印 `consolidated-baseline-only` 标记；一旦出现增量迁移，原不变量循环照常生效 | 造 `migrations/99999999999999_probe.sql`（含未守卫 `ADD COLUMN events.soft_failed` + 未限定 `REFERENCES rooms`）→ 两个守卫均 FAILED 并点名 `probe.sql:2` / `:3`；删 probe → 均 GREEN；模拟"subject 发现逻辑腐化" → FAILED（`left: 2 right: 1`） | 要求前向 `.sql` 恰好 1 个：将来出现非 `*.undo.sql` 回滚文件会（正确地）变红 |
| 4 B7 | ✅ | `tests/unit/perf_gate_honesty_tests.rs`：删掉文本断言，改为用 `std::process::Command` **真跑** `compute_perf_gate.sh`（脚本按字节复制到 temp，stub `cargo` 产出 Criterion 形状输出），4 条测试：默认严格缺测量→非 0、`=1` 同、`=0`→0 且破顶仍非 0、健康输入正控制→0 | 旧文本断言下 `sed 's/:-1/:-0/'` → `1 passed`（完全盲）；新测试同 flip → `compute_perf_gate_fails_on_missing_measurements_by_default FAILED`（`exit=Some(0)`、`Summary: measured=0 breaches=0 missing=0`、`PASSED`）；恢复 → 6 passed | 只 stub 了 benchmark runner，不校验真实 Criterion 目标名仍存在 |
| 5 DB 假跳过（A2/A3 残留） | ✅ | `tests/integration/database_integrity_tests.rs` 的 Err 分支改调 `skip_or_fail_without_db()`；删掉 `tests/common/mod.rs::db_tests_required()`，唯一实现 `integration_tests_required()` 放 common（`#[path]` 双目标编译），`tests/integration/mod.rs` 改为 `pub(crate) use common::integration_tests_required;` | `CI=1 TEST_DB_CONNECT_TIMEOUT_SECS=0 TEST_DATABASE_URL=…/does_not_exist` → FAILED「refusing to silently skip」（改前同环境 `ok` / exit 0）；真库 → 5/5 通过；无 CI + 不可达 → 仍跳过 | **新发现**：`candidate_database_urls()` 无条件追加 `localhost:5432/synapse_test` 兜底，CI 下写错的 `TEST_DATABASE_URL` 会被兜底掩盖（见 §7 新增项） |
| 6 A1 残留 | ✅ | `synapse-e2ee/src/vodozemac_interop_tests.rs::skip_message()` 在 **CI 下 panic**（本地仍打印跳过）；每条 skip 路径都走它 | `CI=1` 且无 `E2EE_INTEROP` → **19 failed**（改前 19 passed、0 断言）；无 CI → 19 passed | CI 下 `E2EE_INTEROP=1` 但 interop 环境缺失时仍按原逻辑失败（正确） |
| 7 §2.5 | ✅ | 新增 `tests/unit/test_isolation_marker_convention_tests.rs`（3 条：常量前缀 vs 两个脚本、目录叶子 `synapse_test_templates`、逐脚本非空性） | 把 `TEMPLATE_READY_MARKER_PREFIX` 改成 `…_v2` → FAILED 并说明"cleanup 会认不出 live 模板、可能删掉它"；恢复 → 3 passed | 只钉字面量，不校验脚本周边逻辑 |
| 8 §2.4 | ✅ | 新增 `scripts/ci/non_unit_coverable_prefixes.txt`（**仅** `src/bin/`、`src/main.rs`）+ `check_file_coverage.py --non-unit-coverable`（fail-closed：缺失/空/过期前缀 → exit 2；豁免只影响 new-file 地板，基线回退仍拦）+ ci.yml 棘轮调用接线；新增 `tests/unit/coverage_ratchet_exemption_tests.rs`（5 条，含合成 lcov 端到端） | 往清单塞 `synapse-services/src/` → 守卫 `FAILED`，e2e 因 stale 前缀 exit 2；合成 lcov：豁免文件 0% + flag → exit 0、无 flag → exit 1、非豁免文件 → exit 1、基线回退不被掩盖 | 将来新增 `src/bin/*` 自动获得豁免（有意），扩大豁免需同时改清单与守卫 |
| 9 B8 | ✅ | `tests/unit/test_fixture_error_handling_tests.rs` 增加语句级 `let _ = …execute(…).await;` 扫描（可跨行），棘轮常量 `LET_UNDERSCORE_AWAIT_WRITE_BASELINE = 231`（实测，含多行链与内联 `#[cfg(test)]`）；docstring 不再指向不存在的记录 | 注入一行 → `232 > 231` FAILED 并点名文件/行；把常量改 230 → `231 > 230` FAILED | 语句边界是文本启发式（`;` 收尾）；棘轮只封上限，收紧需手动改常量 |
| 10 B12 | ✅ | `tests/unit/mock_fidelity_tests.rs` 增加 `function_body()`（按名字取大括号内函数体），断言体内真的调用 `block_room(` 且含 `MOCK DEVIATION`，替换整文件 `contains` 代理 | 删掉 `info.rs` 里的 `svc.block_room(...)` → FAILED；恢复 → 4 passed | 大括号计数式提取，函数体内出现字符串/注释花括号会误判（当前无） |
| 11 A11 | ✅ | `drift-detection.yml`：PR 触发补 `develop`；重复迁移检查改**跨目录 basename 冲突**检测（`migrations/` + `artifacts/sqlx-migrations*`）；性能基线段改为动态解析 `migrations/00000000_*.sql`（0 个或 2 个 → `::error::` 退出 1） | 构造两目录同名迁移：旧命令 `exit 0` + 空输出（结构性失明），新检查 `exit 1` 并点名两个路径；临时把 v12 改名/移出前缀/加第二个 baseline → 分别 exit 0 / exit 1 / exit 1；YAML + `bash -n` 全通过 | 跨目录检查会在本地跑过 `build_sqlx_migration_source.py` 后对 `artifacts/` 副本报警（按铁律 4 这算正确） |
| 12 A7/A8 | 🟡 人工项 | 已在 `TESTING.md` §2.4 记录 push-only 取舍、影响与需确认的 3 个问题（check 名清单见 §6.6），并把主门禁表格里的 fmt/clippy 命令改成事实 | 无代码改动，故无红证明（分支保护无法本地验证） | 仍需人工在 GitHub 侧确认 required checks |

**门禁（本轮合并后，本地等效；真实 CI 无法触发）**：
fmt 棘轮 `current=0=baseline`；clippy 两档（`--workspace --all-targets --features test-utils` 与 `--all-features`）均 exit 0；
unit 目标 **1691 passed / 2 skipped / 0 failed**；全量 `--workspace --lib --all-features --test-threads 4`
（**无 retries**）**6083/6083 passed**。

**✅ 已修（第四轮）——`candidate_database_urls()` 的 CI 兜底改为 fail-closed**：
在 `synapse-common::test_isolation` 新增**唯一判定** `test_db_fallback_allowed()`（`CI` 存在即 false），
5 份 resolver 副本（synapse-common / synapse-test-utils / synapse-services / synapse-storage / tests/common）
在追加 hard-coded localhost 兜底之前统一 `if !… { return urls; }`。
**红证明**：修复前 `CI=1 TEST_DATABASE_URL=…/does_not_exist`（不设 `TEST_DB_CONNECT_TIMEOUT_SECS`）→
`test_audit_critical_indexes_exist` **PASS**（静默回退到 localhost 库，即"配错了也绿"）；修复后同一命令
**FAIL ×3**；`CI=1` + 正确 URL → PASS；无 `CI`（本地默认）→ 兜底仍生效（PASS）。
**守卫**：`tests/unit/test_db_url_convention_tests.rs::every_resolver_copy_disables_the_fallback_under_ci`
断言 5 份副本都含该判定（非空性：`resolvers.len() >= 5`）；**红证明**：删掉 `tests/common/mod.rs` 的 gate 块
→ 该测试 FAILED 并点名该文件，恢复 → 7 passed。

---

## 9. 第四轮落地：§7 第 11 条尾巴（C3/C8/C10/D2/E4/E6/E7/E8/E9）

> 每个"检查类"改动都做了"故意违规 → 变红 → 撤销 → 绿"，且违规探针一律在 temp 副本里构造，
> `migrations/` 与生成产物从未被就地改动。

| 项 | 状态 | 改了什么 | 红证明（关键输出） | 残留 |
|---|---|---|---|---|
| **C3** `check_baseline_consolidation.py` | ✅ | 空扫描面不再报绿：新增 `consolidated-baseline-only` 标记 + `scan_surface_problems()` 自检（正向迁移必须恰为 baseline） | 改前真仓库 `已吸收全部 0 个增量迁移` EXIT=0；把 `TS_RE` 打坏 `\d{14}`→`\d{20}`（temp）→ 改前 EXIT=0 空转、改后 EXIT=1「扫描面自检失败」点名漏掉的迁移；构造"未吸收表/重复索引" → EXIT=1 | 将来新增合法 `00000001_extensions*` 会触发该断言，需有意更新（已写进 docstring） |
| **C8** `check_missing_docs_ratchet.py` | ✅ | 机制描述改为 rustc 事实；删掉**字节级重复**的 `list_changed_rs_files`（铁律 2）；移除已失效的 `-A missing_docs`（只留 `-D`） | rustc 1.93 实验：`#![allow]`+`-D` 绿、`#![deny]`+`-A` 红 ⇒ **属性压过 CLI**；无属性时 `-A -D` 红 / `-D -A` 绿 ⇒ CLI 内**后者生效**（旧"`-A` 抵消 crate allow"的机制从不存在，`-A` 早已被后面的 `-D` 覆盖）；棘轮红/绿：temp 包 baseline 2 → 注入无 `//!` 的 `src/bin/three.rs` → `3>2` EXIT=1 → 撤销 EXIT=0 | 单次全量计数受并发编辑影响（快照）；`-D` 对 6 个 bin crate root 是必要的 |
| **C10** `check_migration_consistency.py` | ✅ | 删除空的 `REQUIRED_V8_BATCHES` 与其空转循环（铁律 1/2 残留）；`scan_surface()` 单一实现供两条分支共用；空集显式自证 + JSON 增 `marker`/`scan_surface` | 改前：undo 配对迭代 0 主体仍 ok；temp 构造"增量无 `.undo.sql`" → EXIT=1 `missing_primary_undo`；命名漂移/删基线 → EXIT=1 `empty_incremental_scan_surface`；真仓库 EXIT=0 且 JSON 含 `marker: consolidated-baseline-only` | 旧 mirror 分支保留（非本次范围）；marker 走 stderr（stdout 仍是纯 JSON） |
| **D2** P1D 设计文档 | ✅ | 3 处把不存在的 `seed_reference_tables_match_baseline` 改成真实测试名，§9.2 落地说明改为真实实现（含"哪些断言落地/哪些没落地、由谁覆盖"）；新增 Guard 8 守卫（解析文档里反引号 snake_case 名并断言 Rust 源码里有 `fn`，非空性 ≥2 名/实际 6 名） | 往文档塞 `nonexistent_probe_test_name_here` → FAILED 点名；把文档还原成提交版（含两个漂移名）→ FAILED；恢复 → 9 passed | **已顺手修掉**：`synapse-common/src/test_isolation.rs:97` 的同一陈旧名字（该 agent 无权改） |
| **E4** `check_pagination_benchmark.py` | 🟡 部分（审计所述"自比"不成立） | 审计说的"自比/永不变化"**不成立**（offset/keyset 是两行独立测量，且能变红）；真正可修的是解析器：重复行 last-wins 会静默换样本 → 新增重复行/缺文件/空行守卫，全部 exit 2 | 改前：重复行（900ms 后 71ms）→ EXIT=0 且用 71ms 算出 39.47%；缺文件 → 裸 traceback。改后：同输入 EXIT=2「appears more than once」；真 Criterion 输出 → `improvement=99.94%` EXIT=0 | **需要单独一轮**：bench 本身是内存仿真（`benches/performance_api_benchmarks.rs:436-484`），30% 阈值余量约 1550×，真实分页 SQL 改动不可能触发；需改为 DB 支撑的真实基准（超出本轮允许文件集） |
| **E6** `scripts/quality/check_route_layering.sh` | ✅ | 补"扫描面非空"守卫（`find -print -quit`，空目录 EXIT=2，与 `check_route_storage_boundary.sh` 同型）；header 不再宣传未实现的 Pattern D/E | 改前：`SYNAPSE_WEB_CRATE_DIR=<tmp>` 且 `<tmp>/src/routes` 为空 → `PASS` EXIT=0；改后 → EXIT=2「would inspect nothing and pass」；真仓库 `bash scripts/quality/check_route_layering.sh` EXIT=0 | Pattern A 对 synapse-web 仍不可达（真实违规在 synapse_storage，由 `check_web_layering.py` 覆盖） |
| **E7** `build_sqlx_migration_source.py` | ✅ | 选择集完整性改为 fail-closed：正向 `.sql` 必须全部被选中，否则 EXIT=2 并列出漏掉的迁移；新增空/缺目录守卫与拷贝后字节一致性校验（含 `SYNAPSE_MIGRATIONS_DIR` 测试缝） | 改前：temp 树 = v12 + `20990101000000_probe.sql` → EXIT=0 且 manifest count=1（探针被静默丢弃 → 迁移源不完整）；改后同输入 EXIT=2「would silently drop 1 migration(s)」；真 CI 调用 EXIT=0 | 将来新增时间戳迁移必须先折进 baseline 或显式选中（有意的 fail-closed） |
| **E8** OpenAPI/route-table | ✅（按裁定缩小范围） | 新增共享 `scripts/api_test/artifact_common.py`（`describe_drift()` 单一实现）；两个生成器都加 `--check/--expected`（temp 生成 + diff，漂移 EXIT=1，永不写被检文件）；把**字节可复现**的 `client.yaml` 校验接进 `ci.yml`（放在生成步骤之前，避免"生成后再校验"变成空转）；route-table 走"确定性输出形状钉住"+ `--check` 红/绿证明 | 改前：两个生成器永远 EXIT=0，无 diff 步骤；`gen_route_table.py --check`（新）对提交产物 → EXIT=1（提交 1047 vs ledger.json 1292，且默认 feature 源导出 1049 → 恰好缺 2 条）；`gen_client_yaml.py --skip-export --check` 对真文件 → EXIT=0（已接线） | **需有意重新生成 `docs/openapi/route-table.json`**：缺 2 条真实未门控路由（`GET /_matrix/client/v3/auth/{auth_type}/fallback/web`、`GET /_synapse/admin/v1/rate-limit-status`），另有 80 条是 feature 门控差异；CI 现在用 `ledger.json`（1292 条）生成上传，与提交的 1047 不一致 —— 重新生成时应改用"默认 feature 的新导出 + 固定 timestamp" |
| **E9** `extract_unresolved_allowlist.txt` | ✅ | 陈旧条目从"只打印提示"改为进入 `strict_failures`（双向棘轮）；清理 5 条已不再命中的条目（21→16）并重写 header；新增回归测试 | 改前：隔离 `EXTRACT_STRICT=1` 对 5/21 条陈旧条目仍 EXIT=0（只在 stdout 提示）；改后同输入 EXIT=1 并列出 5 条；"新条目"方向仍红；沙箱 + 清理后的真清单 EXIT=0 | `scripts/contract/test_extract_registered.py::check_ratchet`（Python 侧）仍是单向（超出允许文件集）；`EXTRACT_STRICT=1` 在 HEAD 上另有 **3 类既有失败**（S-14 三条真路由缺席两条车道、3 条 ledger_export_sdk 车道/profile 集不匹配、1 条 emitted-cfg 计数 1151 vs 1148），均为本轮之前既有 |

**本轮门禁（本地等效；真实 CI 无法触发）**：fmt 棘轮 `current=0=baseline`；clippy 两档（`--workspace --all-targets --features test-utils` 与 `--all-features`）均 exit 0；unit 目标 **1706 passed / 2 skipped / 0 failed**；全量 `--workspace --lib --all-features --test-threads 4`（**无 retries**）**6083/6083 passed**。

### 9.1 E8 收尾补记（route-table 已重新生成，门禁已真实阻塞）

- **产物已更新**：`docs/openapi/route-table.json` 由默认 feature 的新导出 + 固定 timestamp
  (`2026-09-16T00:00:00Z`) 重新生成，1047 → **1049**，diff = `19 insertions(+), 1 deletion(-)`，
  **恰好新增 2 条未门控路由、删除 0**：
  `GET /_matrix/client/v3/auth/{auth_type}/fallback/web`（assembly::auth_compat）、
  `GET /_synapse/admin/v1/rate-limit-status`（admin::server）；`total_routes` 之外的 `generated_at`/`source`/
  `profile`/`schema_version`/`_meta` 与既有 1047 条路由对象**逐字节且同序**不变（非 churn）。
  ⇒ §9 E8 行里"需有意重新生成"这条待办**已关闭**。
- **CI 现在真的会拦**（`ci.yml` `openapi-artifact`，步骤顺序）：build → **新增**默认 feature 导出到
  `$RUNNER_TEMP`（固定 timestamp）→ `client.yaml --check` → **新增阻塞** `gen_route_table.py --check`
  → 生成 client.yaml → 用**同一份**新导出生成 route-table（不再用会产出 1292 条的旧 `ledger.json`）→ 上传。
  两个 check 都在任何"就地覆盖"之前，避免"先覆盖再校验"的空转。
- **红证明**（全部在 temp 副本，未改动提交产物）：(a) 篡改一行 `method: GET` → `GET_MUTATED` → EXIT=1 且
  diff 点名该行，恢复 → EXIT=0；(b) 用 all-extensions 导出（1129 条 / profile=all 1148 条）→ EXIT=1，
  diff 显示 `total_routes 1049→1129` 与 voice 门控路由 ⇒ 门禁是 profile-aware、非空转；
  (c) 删掉一条真实路由 → EXIT=1 并把该行加回。端到端模拟：导出 1049 → 两个 check EXIT=0 →
  生成字节与提交产物 `cmp` 完全一致。
- **过程记录（诚实说明）**：这批 E8 改动（`docs/openapi/route-table.json` 与 `ci.yml`）是在我提交
  `9fb0e46a`（本意只改两份文档）时被 `git add -A` **一并卷入**的，因此该 commit 的 message 未提及 E8；
  内容已逐条复核无误，后续以 §9.1 为准（不再重写历史）。
- **§9 末尾"新增待办"相应收敛为 2 条**：E4 的真实 DB 分页基准；`extract_registered.py` Python 侧棘轮仍单向
  + `EXTRACT_STRICT=1` 在 HEAD 上的 3 类既有失败。

---

## 10. 第五轮：路由契约漂移（EXTRACT_STRICT 的 3 类失败）已修 + 过程教训

> 本节**取代** §9 中关于 "`EXTRACT_STRICT=1` 在 HEAD 上仍有 3 类既有失败" 与 "新增待办收敛为 2 条" 的表述：
> 3 类失败已修复并复核（下面），待办收敛为 **1 条**（E4 的真实 DB 分页基准，仍在收尾）。

### 10.1 根因：一次功能提交没有重新生成派生/契约产物

`10a18b3f`（feat(voice): POST /voice/register）在 `synapse-web/src/routes/voice.rs:64,69,92` 加了 3 条路由，
但**没有重新生成任何派生产物**（`15540350` 上一次生成是 9-18；`e23e2dd0` 只重新生成了 `route-table.json`）。
**代码是权威侧**（`create_voice_router` 确实在 `voice-extended` 下经 `route_module.rs:256` 合并），
派发表 / SDK 车道 fixture / 生成型契约文档才是过期的。**没有任何检查被放宽**；重新生成由
`gen_derived_routes.py --check`（派发表必须能复现两条 fixture 车道）与 SDK 车道 golden 测试共同校验。

三"类"失败其实是同一批 3 条路由的三种表现：

| 类 | 表现 | 实体 |
|---|---|---|
| S-14 | 3 条已服务路由在**两条 ledger 车道**都缺席 | `POST /_matrix/client/v1/voice/register`、`POST /_matrix/client/v3/voice/register`、`POST /_matrix/vendor/v1/voice/register` |
| ledger_export_sdk 车道/profile 集 | 派发表比 SDK fixture **多 3 条** | 同上 3 条（default 1129 / worker 1140 / all 1148 → 1132 / 1143 / 1151） |
| emitted-cfg | sdk 1151 vs fixture 1148，差正好 3 | 同上 3 条 |

顺带澄清：本轮简报里点名的两条（`GET /_matrix/client/v3/auth/{auth_type}/fallback/web`、
`GET /_synapse/admin/v1/rate-limit-status`）**在六个 ledger fixture 里本来就都在**，属已收口的 E8/route-table 故事。
`scripts/api_test/ledger.json`（1292 条、2026-08-12）是另一条遗留输入，不受影响。

### 10.2 修复与精确 delta

| 文件 | delta |
|---|---|
| `synapse-web/src/routes/derived_route_table_always.inc.rs` | +16/−1：容量 1129→1132，**只多 3 行** `#[cfg(feature = "voice-extended")] POST …/voice/register "voice"`；`worker`/`oidc`/`derived_routes.rs` 逐字节未变 |
| `tests/unit/fixtures/ledger_export_sdk/{default,worker,all}.json` | 各 **+3 条目**，`entry_count` 1129→1132 / 1140→1143 / 1148→1151；golden 车道 `tests/unit/fixtures/ledger_export/*` 未动；重跑**幂等**（字节一致） |
| `docs/synapse-rust/ROUTE_CONTRACT.md`（生成型契约文档，非审计交接文档） | 1148→1151、voice 27→30、+3 条路由项（另仅时间戳行变化） |
| `scripts/contract/test_extract_registered.py` | `check_ratchet` 增加**陈旧条目**分支（+14 行），与 `extract_registered.py` 的 `strict_failures` 对齐 ⇒ Python 侧棘轮**双向** |

### 10.3 红/绿证明（agent 侧 + 我独立复核）

- **改前**：`EXTRACT_STRICT=1 bash scripts/contract/check_route_contract.sh` → `❌ 6 check(s) failed`，**EXIT=1**
  （S-14 ×2、sdk all/default/worker、emitted-cfg 1151 vs 1148）。
- **改后（已提交 HEAD 上，我本人复核）**：同一命令 → **EXIT=0**，
  `✅ 所有 SDK 调用的端点都在 ledger 中（且 method 一致）`、`✅ ROUTE_CONTRACT.md is up to date`；
  `python3 scripts/contract/test_extract_registered.py --mutation-check` → **`✅ all 53 guard checks passed`**（EXIT=0）。
- **Python 棘轮双向红证明**：往 `extract_unresolved_allowlist.txt` 追加 1 条陈旧条目 → `FAIL no stale
  unresolved-allowlist entry (bidirectional ratchet)` EXIT=1；删掉 1 条真实条目 → `FAIL no new unresolved
  parser construct` EXIT=1；两者撤销后绿（清单与 HEAD 字节一致）。
- 聚焦验证：`py_compile` / `bash -n` 全通过；`-p synapse-web --lib --all-features derived_manifest_tests` 3/3、
  golden 车道 3/3；`placeholder` 5 项、`ledger` 7 项通过。

### 10.4 过程教训（必须记住）

**`git commit` 提交的是整个 index，不只是你刚 `git add` 的文件。** 本次（以及 §9.1 那次）出现了同一类事故：
并发 subagent 自己 `git add` 过它的修复文件，我用**显式路径** `git add` 自己的文件后提交，
结果把它 6 个文件一并卷进了消息完全不相关的 commit（`79ce60e3` develop 清理；`9fb0e46a` 分支保护回填）。
**规矩**：共享工作区提交前必须先 `git diff --cached --stat` 核对 index；跨 agent 并行时优先"一个 agent 一个提交"，
或串行化提交动作。本文档 §9.1 与本节各留一条记录，避免重复踩。

### 10.5 残留

- **E4（真实 DB 分页基准）**：仍在收尾（工作树未提交），见 §9 的 E4 行；其新增
  `benches/performance_pagination_benchmarks.rs` 目前是 `check_fmt_ratchet.sh` 唯一的违规来源（`current=1`），
  待其收尾后统一跑 fmt/clippy/unit/全量 lib 并提交。
- Python harness 的 `--mutation-check` 尚未**永久**自证陈旧方向（本次为手工证明）；如需铁律 8 的常驻自证，
  可在其 mutation 列表里加一条（小跟进项）。
