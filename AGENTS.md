# AGENTS.md

This file provides guidance to Codex (Codex.ai/code) when working with code in this repository.

## Common commands

### Local Rust development
- Build: `cargo build --locked`
- Run server: `SYNAPSE_CONFIG_PATH=homeserver.yaml cargo run --release`
- Run worker binary: `cargo run --bin synapse_worker`
- Format check: `cargo fmt --all` before every commit — the **real CI gate** is `./scripts/check_fmt_ratchet.sh` (ratchet, baseline 0, both `current > baseline` and `current < baseline` fail). It counts **standalone `rustfmt --check`'s `Diff in` blocks**: one file can contribute several (measured: two disjoint hunks in one file = 2), and `cargo fmt -- --check` prints no such marker at all — which is exactly why the old counter reported `current=0` for 56 unformatted files across 21 CI runs. After `cargo fmt --all` you are at `current=0=baseline` and pass.
- Clippy (CI runs both matrix entries — `features-args` empty and `--all-features`): `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils [--all-features] --locked -- -D warnings`. `--all-targets` covers test code workspace-wide; the old gap (CI checked only `-p synapse-services --tests`) was closed in `ci.yml:301`.
- Doc tests: `cargo test --doc --locked` — ⚠️ **this is currently an empty gate** (root crate has 0 doc tests; workspace-wide there are only 4 and all are `#[ignore]`d). A real rustdoc-only compile error (E0106) once shipped green through this gate. Prefer `cargo test --doc --locked --workspace` when touching doc examples, and note rustdoc catches lifetime elision errors that `cargo check`/`clippy` miss entirely.
- Full test suite: `cargo test --workspace --all-features --locked -- --test-threads=4` — ⚠️ `--workspace` 不可省：root `Cargo.toml` 没有 `default-members`，从 root 跑不带 `--workspace` 的 `cargo test` **只执行 root package 自己的 target**，8 个 workspace member 的 lib 单测一个都不跑（2026-09-22 实测：漏掉 **6139** 个用例，比它实际跑的 3308 还多，等于给出"全量绿"的假结论）。CI 同款入口是下一行的 `scripts/ci_backend_validation.sh`（其 lib 批次本就是 `--workspace --lib`）。判据见 `docs/audit/FULL_SUITE_ISOLATION_VERIFY_2026-09-22.md` §5。
- Local CI run (⚠️ **not** the CI entrypoint): `bash scripts/ci_backend_validation.sh` — it runs `ci.yml`'s three nextest batches **verbatim** (lib / unit / integration at `--test-threads 4`), so it cannot drift from CI. The former second implementation `scripts/run_ci_tests.sh` was deleted (sweep A13; it still ran the 4 CI-unstable manual perf smokes via `--ignored`). For what CI actually runs, read `ci.yml` / `TESTING.md`.
- Git hooks (version-controlled under `.githooks/`): enable **once per clone** with `git config core.hooksPath .githooks` — without that setting the files in `.githooks/` **never execute** (the active default is `.git/hooks`, which held only `*.sample`). `pre-commit` blocks on `./scripts/check_fmt_ratchet.sh` (the CI gate itself, so a local pass means a CI pass; whole-tree, not staged-only), skips the multi-minute clippy stage unless `SYNAPSE_PRECOMMIT_CLIPPY=1`, and keeps cargo-audit advisory. `pre-push` blocks on the CI-shaped clippy (`SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings`) and then keeps the original cargo-deny advisories check. Emergency bypass: `git commit --no-verify` / `git push --no-verify`. **判据**：2026-09-22 的事故正是"hook 早已写好但从未启用"—— 两次提交在没跑格式化的前提下入库，fmt 债务 0 → 49 → 106、违规文件 4 → 7 个，CI 变红；随后两轮又各自入库了**从未在 `--all-features` 下编译过**的测试（CI 的 blocking lib 批次两次直接 exit 101）。格式由 `pre-commit` 拦，编译由 `pre-push` 拦 —— 两者都是 CI 的同一实现，因此本地过 = CI 过（见 `docs/audit/FULL_SUITE_ISOLATION_VERIFY_2026-09-22.md` §7）。
- ⚠️ **`CARGO_TARGET_DIR` 不得在两个 `git worktree` 之间共享。** 实测（2026-09-26）：两个 worktree
  指向同一个 `target/` 时，cargo 会**跨树复用产物** —— 主树的 `synapse-web`（新签名）与
  本树的 `tests/unit/*`（旧调用点）被链在一起，报出 **9 处 E0061**，而报错注释里的
  `--> /Users/.../synapse-rust/synapse-web/...` 指向的**根本不是本树的文件**。换成私有
  `CARGO_TARGET_DIR` 后同一命令 EXIT=0。**这类混用既会假红，也能假绿**（复用到旧产物 ⇒
  本树的改动没被编译却"通过"）。规则：**一个 worktree 一个 target 目录**；
  看到"编译错误指向另一个 worktree 的路径"就是它，换目录重跑即可。
- Local test tooling needs `cargo-nextest` (CI runs every batch with it): `cargo install cargo-nextest --locked`.
- `cargo nt` is a repo alias (`.cargo/config.toml`) for `cargo nextest run --profile test --features test-utils`. It works for `--lib`/`--test unit` but **not for `--test integration`** (nextest 0.9.140 silently ignores `features` in profiles + the integration target has `required-features`); use the explicit `--all-features` command below for integration.

### Running specific tests
- Unit test target: `cargo test --test unit`
- Integration tests: **`cargo nextest run --profile ci --all-features --test integration --test-threads 1`** ⚠️ `--all-features` is REQUIRED (CI port), otherwise `/login` snapshots, route ledger tests, and other feature-gated tests will fake-fail.
- E2E target: `cargo test --test e2e`
- Performance manual target: `cargo test --features performance-tests --test performance_manual -- --nocapture`
- Run one named integration test: `cargo nextest run --profile ci --all-features --test integration <test_name> -- --nocapture`
- Compile one integration test target without running DB setup: `cargo test --features test-utils --all-features --test integration <test_name> --no-run`
- Run one unit test from the unit target: `cargo test --test unit <test_name> -- --exact --nocapture`
- Run one library unit test by substring: `cargo test --lib <test_name> -- --nocapture`
- Single test with TDD profile (mock support): `cargo nextest run -p <crate> <test_name> -P tdd --features test-utils`

### Benchmarks and coverage
- API benchmark compile/run path: `cargo bench --bench performance_api_benchmarks --no-run`
- Federation benchmark compile/run path: `cargo bench --bench performance_federation_benchmarks --no-run`
- Coverage: CI uses `cargo llvm-cov --workspace` (tarpaulin was replaced). Local end-to-end run: `bash scripts/ci/run_coverage.sh`（CI 调用的**同一条命令**；本机 ~35–50 min）。
- Note: Coverage scripts may timeout when DB has accumulated many test schemas (1363 x 255 tables observed). Run `scripts/cleanup_test_schemas.sh` before coverage if needed.

### Database and migrations
- Migration source of truth: `docker/db_migrate.sh` (applies `migrations/`).
- **`migrations/` is the SINGLE source of truth for migration SQL.** The former duplicate directory `docker/deploy/migrations/` was deleted (commit `2b16dc3c`) after it drifted (+13 missing files) from the authoritative set. Do NOT recreate any copy — the deploy path mounts `./migrations` directly. 详见 `migrations/README.md`（副本独有 131 = 非 `archive/` 82［其中正向 42］+ `archive/` 49）。
- Naming convention: rollback files, **if any are ever (re)introduced**, must use the `.undo.sql` suffix — a `_undo.sql` (underscore) name is a violation, because the migrator then treats it as a **forward** migration and applies it. There are currently **0** `.undo.sql` files (`git ls-files 'migrations/*undo*'` is empty): the baseline is consolidated, so the suffix exists only as a naming rule and as a check in `scripts/check_migration_consistency.py`. (This line previously claimed "32 existing" — a stale count from before the consolidation.)
- Apply migrations locally: `bash docker/db_migrate.sh migrate`
- Validate migrations/schema locally: `bash docker/db_migrate.sh validate`
- Migration verification helpers live under `scripts/` and `migrations/`; prefer existing scripts over ad hoc SQL.
- **Postgres error codes**: `schema "x" does not exist` is SQLSTATE `3F000` (invalid_schema_name), **not** `42P01` (undefined_table). EXCEPTION blocks in DO $$ must catch both.
- **`CREATE OR REPLACE FUNCTION` cannot change parameter names** — parameter names are part of the function signature; an idempotent migration must `DROP FUNCTION IF EXISTS ...(old_signature)` first.
- **Never split migration SQL with `split(';')`** — `$$...$$` dollar-quoted bodies, string literals, and comments contain internal `;` and will produce truncated functions. Use a character-level splitter.

### Docker workflow
Two stacks, different jobs, deliberately side by side:
`docker/docker-compose.yml` (dev/CI: builds in place, services `synapse-rust`/`db`/`redis`,
entrypoint migrations, no nginx — the `backend-validation` and `e2ee-interop` CI workflows
start the stack **by these service names**) and `docker/deploy/docker-compose.yml`
(production full stack: postgres/redis/migrator/synapse/nginx, `deploy.sh`).
- Start dev stack: `cd docker && docker compose up -d --build`
- Validate containerized migrations: `cd docker && docker compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust migrate`
- Validate schema in container: `cd docker && docker compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust validate`
- CI-like local validation: `bash scripts/ci_backend_validation.sh`
- **Config has a single source of truth: `docker/config/`** — both stacks mount it
  (deploy: `../config`, dev: `./config`) and the Dockerfile bakes the same files.
  There is no `docker/deploy/config/` and there must not be one; see
  `tests/unit/config_mount_tests.rs`.

## 项目状态与反冗余铁律（未发布项目，先读这一节）

**项目状态：未发布、无外部用户、无生产数据。** 因此**不存在向后兼容义务**。
这一条决定了下面所有规则：宁可一次性改干净，不要叠加兼容层。

1. **禁止兼容残留。** 不得为"向后兼容"保留 `#[deprecated]` 项、旧路径别名、
   双实现、feature 开关式的死代码、或"先留着以后可能有人用"的中间态。
   改动直接替换原实现。**判据**：如果某符号/分支/配置的唯一存在理由是
   "兼容旧行为"，就删掉它。
2. **同一职责只允许一份实现。** 模板/schema 隔离、schema 清理、配置解析、
   URL 解析这类基础设施出现第二份实现即视为缺陷。新增前先搜索是否已有
   可复用实现（`synapse-common` 是共享基础设施的首选位置）。
   **反例（已发生）**：测试隔离曾经有三份实现，导致同一类 bug 要修三次。
3. **测试/bench 专用依赖必须放 `[dev-dependencies]`。** 不得进 `[dependencies]`，
   否则会污染生产依赖图。新增依赖时必须说明它对生产依赖图的影响。
   **自查**：`cargo machete`。注意它只扫 `[dependencies]`，所以它报"未使用"
   往往意味着依赖放错了段，而不是真的没用——先核实再决定删还是移。
4. **构建产物与派生缓存不入库。** `artifacts/`、`target/`、SQLx 派生缓存等
   一律 gitignore，且不得用 `git add -f` 绕过。**迁移只有一个真相源：
   `migrations/`**；不得在 `artifacts/` 或任何其他目录再放迁移副本。
5. **`cargo metadata` 必须始终可用。** 任何 vendored 或独立 crate 必须在根
   `Cargo.toml` 的 `[workspace] members` 或 `exclude` 中显式声明。否则
   machete / deny / audit / IDE 等**整套**基于 `cargo metadata` 的工具链会
   直接失效。**反例（已发生）**：`vendor/pastey` 未列入 `exclude`，
   `cargo machete` 整体退出——注意该 vendor 本身是必要的（见 `[patch.crates-io]`
   注释），要修的是 workspace 声明，不是删掉它。
6. **薄壳禁止。** 根 crate 的 `src/{services,storage,common}/mod.rs` 只允许
   re-export，且必须有实际使用者。新增只做转发的模块一律合并进对应 workspace crate。
7. **消除冗余优先于绕过问题。** 遇到并发/共享状态类缺陷，先问"能否从结构上
   消除共享"（如 per-test schema 隔离），而不是加锁/串行化/重试去绕过。
   **反例（已修正）**：retention 测试曾用 `max-threads=1` 串行分组绕过全局
   单例行的跨进程 race；正确修法是让每个测试用独立 schema，race 随之消失。
8. **门禁必须自证能变红。** 任何"检查类"门禁（fmt/clippy/覆盖率/自定义守卫）
   在新增或修改后，必须用**故意制造的违规**证明它真的会失败。报"通过"的门禁
   未必在工作。
   **反例（已修正）**：`scripts/check_fmt_ratchet.sh` 曾用
   `cargo fmt --all -- --check | grep -c '^Diff in'` 计数，但 `cargo fmt`
   检测到差异时**只返回非零退出码、不打印 `Diff in` 块**（那是独立 `rustfmt`
   的输出格式），因此计数恒为 0 —— 对 56 个未格式化文件连续 21 个 CI 运行都报
   "fmt debt: current=0 / OK"。改为 `rustfmt --check` 逐文件计数后，插入一个
   未格式化探针文件即可让它变红。
   **推论**：看到"长期 0 违规 / 长期全绿"的门禁，优先怀疑它没在工作，而不是
   相信代码很干净。

9. **同一工作树同一时刻只允许一个写者（含"另一个 AI 会话"）。** 本仓实际发生过三次
   危害（2026-09-22）：① 一端 `git add -A` 把另一端尚未完成的改动卷进自己的提交
   （提交信息与内容不符）；② 一端 `git commit` 把另一端 **已 staged** 的删除一起提交
   （`git commit` 提交的是整个索引，不只是你 `git add` 的路径）；③ HEAD 一度自相矛盾
   （守卫还在读一个刚被删掉的脚本）。
   **规则**：`git add` **逐路径**（禁止 `git add -A` / `git add .`）；提交前
   `git diff --cached --stat` 复核；提交后 `git status --short` 确认没有把别人的在途
   改动带走；需要并行时用 `git worktree` 开独立目录。

## SQLx 静态化规则（改任何 SQL / 查询前必读）

**为什么有这一节**：2026-09-23–25 的静态化战役把 `dynamic_production` 从 1532 降到 **5xx**、
`static` 从 61 升到 **9xx**（`.sqlx` 60 → **9xx** 条）—— **具体数字不写在这里**，它是会漂的，
一律以 `docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` **§0 的实测表**为准（D-16 型
"双份计数漂移"已发生过一次）。真正值得记住的不是数字：动态 `.bind()` + `FromRow` 会把列名、
列类型、可空性一路吞到运行期，而 `query!` / `query_as!` 连的是**真库 catalog** —— 一旦改成宏，
这些错误在**编译期**就被证伪。该战役因此挖出 **64 条**既有缺陷（**已关闭 51 条**），其中十余条是
"真 schema 下必然失败"（列名写错、INSERT 漏 NOT NULL 列、两个已注册管理路由背靠一张
**不存在的表**、`sent_at` 从不写入导致清理**恒删 0 行**、`WHERE $2 != '[]'` 对 `text[]`
在 **prepare 阶段**就报 22P02 导致整条 DAG 写入必败）。

> **唯一登记处**：`docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` **§7**（**只登记未关闭项
> 与结构性保留**；已关闭项的明细在 `docs/synapse-rust/archive/SQLX_STATICIZATION_PLAN_2026-09-23_HISTORY.md`
> 的 §7.2，冻结不再更新）。新发现的问题追加到 §7；状态计数也以 §7 表为准 —— 不要在别处再开第二份清单。

本节规则的目标：**别再制造新的同类缺陷，也别让已经修好的面退化。**

### R1　新增/修改 SQL 一律静态化（正向强制）

任何新增或改动的查询必须用 `query!` / `query_as!` / `query_scalar!` / `query_file!`。
**禁止新增** `sqlx::query(...)` / `query_as::<_, T>(...)` / `query_scalar(...)` 的
**字面量**形态。

- **判据/门禁**：`bash scripts/ci/check_sqlx_dynamic_ratio.sh`（生产动态不得增、静态不得减）
  ＋ `cargo nextest run --test unit --features test-utils -E 'test(/sqlx_dynamic_literal_guard/)'`
  （逐文件 literal 棘轮；新文件里写一个字面量也会红）。

> ⚠️ **宏的 SQL 实参必须是调用点字面量，不得经中间变量传递。**
> 把静态 SQL 赋给局部变量再 `sqlx::query_as(query)`，会**同时骗过两道门禁**：
> literal 棘轮只看调用点实参形态（变量 ⇒ 看不见），ratio 棘轮只看总数（照样计入）。
> 实测（2026-09-25，D-59）：并发会话正是用 `let query = r"…"` + `query_as(query)`
> 新增了 2 处，使 `opt/consolidated` 的 ratio + literal 双门禁同时红而"看起来只是 runtime 残差"。
> 需要按 tx/pool 分支执行时，**先把连接收敛成一个 `&mut PgConnection`，再写一次宏调用**
> —— 宏的绑定实参属于调用点，这正是不能"先建字符串、后分支绑定"的原因。

### R2　改了查询文本 ⇒ 同一提交必须带 `.sqlx` 增量

任何查询**文本**变化（列清单、别名、`AS "col!"`、谓词）都要重跑
`cargo sqlx prepare --workspace -- --all-features`，并把 `.sqlx/` 增量**一起提交**。

- **为什么**：`.cargo/config.toml` 的 `[env] SQLX_OFFLINE = "true"` 让**所有**构建都走离线缓存
  ⇒ 缺一条不是"某个用例失败"，而是**整个 CI 编译失败**。历史上正是"只提交 `.rs`、不提交
  `.sqlx`"造成的（D-51）。
- **判据/门禁**：`bash scripts/ci/check_sqlx_cache_fresh.sh --compile`（**权威**）。
  `--static`（CI 现在跑的那档）只查"存在/非空/被 git 跟踪"，**抓不到缺条目**。
- **feature 集必须用 `--all-features`**：用枚举 feature 会漏掉门控模块。实测漏掉
  `privacy-ext` 时，静态化的 5 条不进缓存，`--all-features` 构建直接 6 个 error（C26）。

### R3　宏内禁止 `RETURNING *` / `SELECT *`

`query_as!` 不走 `FromRow`，星号必须展开为**显式列清单**；多列 E0560、少列 E0063。（D-22）

### R4　可空性：对齐 / 收紧 schema / 写清理由，三选一

列可空 ⇔ 字段 `Option<T>`。若确实需要 `AS "col!"` **断言非空**，必须在同一提交里说明
**"谁保证非空"**；凡是能从结构上保证的（唯一写者恒写该列、语义本就非空），应当**直接收紧
schema**（`NOT NULL DEFAULT …`），而不是长期留一个断言别名。

- 正例：`key_backups.version`、`olm_sessions.message_index`、`rendezvous_session.content`
  都是靠收紧 schema 才真正关掉的（D-46 / D-48 / D-49）。
- **两个方向都会错**：sqlx 推**可空**而结构体非 `Option` ⇒ 用 `!` 或收紧 schema；
  sqlx 推**非空**而语义可空 ⇒ 用 `AS "col?"`（LEFT JOIN 外侧列会被 PG 透传成 NOT NULL，D-20）。
- **两类"看起来非空却推成可空"的来源**（实测，遇到时先怀疑它们，别急着改结构体）：
  ① **无关系来源的表达式** —— `COALESCE(…)`、`COUNT(*)`、`0::BIGINT`、`'pending'`、`NULL::text`
     （C29/C30 的 `AS "depth!"` / `AS "count!"` / `AS "exists!"`）；
  ② **UNION / 集合运算的输出列** —— PG 的 `Describe` **不给 UNION 结果透传 NOT NULL**，
     即使两侧都是 `NOT NULL` 列（C31 的 `device_exists_batch` ⇒ `AS "user_id!"`）。
  > **别把这条推广成"所有派生列都可空"**：C31 实测 `DELETE … RETURNING` 的 CTE 外层投影
  > **确实**继承了非空（`get_and_delete_messages` 未加任何断言即可编译）。
  > 判据是**编译器的报错**，不是直觉 —— 先写最直接的列清单，报 `Option<…>` 不匹配再断言。
- ⚠️ **反向不报错，所以 `Option` 字段不能当作可空性的证据。** 宏只在
  「列可空 ⇒ 字段非 `Option`」这个方向报错；**NOT NULL 列配 `Option<T>` 字段照过**
  （C32 实测：`upload_progress.expires_at` 是 `BIGINT NOT NULL`，而 `UploadProgress.expires_at`
  是 `Option<i64>`，编译无碍）。想知道真实可空性就看 schema（或 `\d <table>`），
  **不要从字段类型反推** —— 反推会得出"该列可空"的错误结论。

### R5　绑定表达式

- `&Option<T>` 会被宏的 `ty_match` 拒绝（旧 `.bind()` 接受）⇒ 用 `.as_deref()` / `.as_ref()`（D-21）；
- 数组参数元素类型写 `Vec<String>`（不要 `Vec<&str>`）；`&[i64]` / `&Vec<String>` 可直接传
  （`= ANY($1)` / `unnest($1::text[], …)` 都行）；
- `Option<i64>` 按值直接传；
- ⚠️ **`LIMIT $n` / `OFFSET $n` 的参数在宏下按 `bigint` 定型**。形参是 `i32` 时必须显式
  `i64::from(limit)` —— 动态路径由 PG 隐式放宽，宏把这一步变成编译期错（C31 实测 2 处：
  `expected i64, found i32`）。别改公共签名去迁就，转换点加一次无损转换即可。

### R6　别名：`query_as!` 不认 `#[sqlx(rename)]` / `#[sqlx(skip)]`

需要改名或跳过字段时，在 SQL 里显式写 `AS "字段名"`（D-19）。
⚠️ **用了双引号别名的 raw string 必须是 `r#"…"#`**：写成 `r"…"` 开头 + `"#` 收尾会让宏报
`no rules expected #`（实测一次踩出 9 处）。

⚠️ **断言别名是"真的列名"，不是给编译器看的注解。** 写 `COUNT(*) AS "count!"` 之后，
输出列就叫 `count!` —— 同一条 SQL 里的 `ORDER BY count` 会**在 prepare 阶段报
`column "count" does not exist`**（C31 实测）。要么 `ORDER BY COUNT(*) DESC`，
要么给排序单独留一个不带 `!` 的别名。

### R7　例外白名单（只减不增）

只允许两类保留动态 SQL：**动态标识符**（`format!` 拼列清单 / 表名 / 排序方向）与
**`Vec<Option<T>>` 数组参数**（sqlx 无该映射）。
例外必须：① 在 §7 **有登记条目**；② **不新增 literal 动态站点**（literal 棘轮）；
③ 优先按 §7.2 的方向回收（`Vec<Option<T>>` → `jsonb_to_recordset`，D-13 / D-14）。

### R8　每批必须跑的门禁（四道，缺一不可）

```bash
# 1) 棘轮：生产动态不得增、静态不得减
bash scripts/ci/check_sqlx_dynamic_ratio.sh
# 2) 离线缓存完整性（权威 —— 不要只跑 --static）
bash scripts/ci/check_sqlx_cache_fresh.sh --compile
# 3) 两档 clippy（第二个入口才编译 integration 等 target，两档不可互相替代）
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
# 4) 该模块的「真 baseline」DB 往返（不是纯构造/序列化用例）
cargo nextest run -p <crate> --lib --features test-utils -E 'test(/<module>/)'
# 收尾
./scripts/check_fmt_ratchet.sh
```

**"schema 到底变没变"不要只在长期库上验证**：本机库可能带陈旧 `public`（见 R11），
需要时应建**一次性库**并跑 `scripts/ci/prepare_test_db.sh` 复现 CI 口径。

### R9　测试侧

- 新增 DB 覆盖一律 `IsolatedTestPool::new(<真 baseline>)`，**禁止自建 schema**
  —— 自建 schema 会与真 baseline 漂移，曾**结构性地**掩盖 D-46/D-47；
- 断言"表/列存在"**必须锚定当前 schema**（`current_schema()`），不要用会回退到 `public`
  的 `to_regclass($1)`（D-57）；
- `tests/` 与 `#[cfg(test)]` 内的夹具按 D-13/D-14 保持动态（宏不进 `cargo sqlx prepare`，
  `--all-targets` 会 E0432）。

### R10　schema 变更的连带清单

改 `migrations/` 后，同一批必须：

① 复算并同步 `EXPECTED_BASELINE_FINGERPRINT`（**先用旧值自检哈希实现**，再取新值）；
② 跑守卫 5：`cargo nextest run --test unit --features test-utils -E 'test(/test_isolation_unification/)'`；
③ 同步**所有断言该 schema 的契约用例** —— 并发会话删表后漏改 3 条，直接让 CI 集成批次必红（D-56）；
④ 跑 R8 的第 2、3 条（表/列变化会影响宏的类型推断与整份缓存）。

### R11　门禁自身必须可信

- **红着的门禁等于没有门禁**。新增/修改任何守卫、棘轮、脚本，都要**用故意制造的违规证明它
  会失败**，并把该实验写进提交信息或 `docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md`
  （历史批次记录在 `docs/synapse-rust/archive/SQLX_STATICIZATION_PLAN_2026-09-23_HISTORY.md` 的 §8.x）
  —— 本战役一共撞到 **5 次门禁失效**
  （3 次红：D-50 / D-52 / D-56；2 次假绿：D-51 / D-57），其中两次是**长期存在**的。
- 遇到与本批无关的**既有红门禁**：按"先修再转"**独立提交**修掉，不得绕过、不得加
  `#[allow]` 放宽 lint。
- 看到长期全绿 / 长期 0 违规的门禁，先怀疑它没在工作，而不是相信代码很干净（铁律 8）。

### R12　先修再转，禁止夹带

静态化是**行为保持**的机械重构。转换过程中撞到的既有缺陷（列名错、可空性不符、死代码、
吞错）必须**独立提交**，禁止夹进转换批次 —— 否则"编译期把改动证伪"这条证据链就失效
（§7.x 处置约定 1）。

**转换前先查有没有死代码**：零调用者的语句先按铁律 1 删掉，再转换剩下的 —— C25/C27 都这么做，
直接省掉一处转换与一次 `.sqlx` 往返。

### R13　登记与计数唯一

新发现的问题**追加到** `docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` **§7**，
不要在 baseline 脚本或别的文档里再开第二份清单；状态计数一律以 §7 表为准
（已修 / 部分已修 / 未修 / 结构性保留 / 文档级），不要在别处另算一套 ——
双份计数已导致过 D-16 型漂移。
缺陷关闭后，其逐条明细移入
`docs/synapse-rust/archive/SQLX_STATICIZATION_PLAN_2026-09-23_HISTORY.md` 的 §7.2，
**§7 只保留未关闭项与结构性保留**（关闭项不再占正文篇幅，但仍可在 HISTORY 里按编号查到）。

## High-level architecture

### Runtime shape
- `src/main.rs` bootstraps config, telemetry/logging, builds `SynapseServer`, and runs the main homeserver process.
- `src/server/mod.rs` is the main composition root. It creates the Postgres pool, runs schema health checks, wires Redis/in-memory cache, builds `ServiceContainer`, configures rate-limit state, and assembles the Axum router.
- The server exposes both client and federation listeners from the same application state.

### Core layering
The root crate's `src/` is reduced to bootstrap + composition (`main.rs`, `server/`, `bin/`, `tasks/`, plus the `common`/`e2ee`/`cache`/`storage` facades); the HTTP surface and business logic live in workspace crates:

- `synapse-web/` (workspace crate): HTTP boundary. Axum routes, extractors, middleware, validators, and Matrix-compatible endpoint assembly, plus the federation glue that depends on the HTTP context (`synapse-web/src/federation/edu.rs`).
- `synapse-services/`: business logic layer (workspace crate). Feature behavior lives here; composition root is `synapse-services/src/container.rs`.
- `synapse-storage/`: persistence layer over PostgreSQL (sqlx), plus schema/health/performance helpers (workspace crate).
- `synapse-e2ee/`, `synapse-federation/`: E2EE crypto and federation transport/auth logic (workspace crates).
- `synapse-cache/`, `synapse-common/`: Redis-backed cache (in-memory fallback) and shared config/logging/security/rate-limit/task-queue utilities (workspace crates).

The root crate's `src/services/` and `src/storage/` are thin shells (`mod.rs` re-exports only) — new logic belongs in the corresponding workspace crate.

The codebase generally follows `route (synapse-web/src/) -> service (synapse-services/) -> storage (synapse-storage/)`, with `AppState`/`ServiceContainer` carrying shared dependencies.

### Router organization
- `synapse-web/src/routes/assembly.rs` is the top-level router assembly point.
- It merges many feature routers under Matrix-compatible prefixes such as `/_matrix/client/*`, `/_matrix/federation/*`, and admin/auxiliary endpoints.
- Middleware layering is centralized here: CORS, security headers, compression, CSRF, and rate limiting.
- Route implementation is split by domain under `synapse-web/src/routes/` and `synapse-web/src/routes/handlers/`.
- Every route must be declared in `synapse-web/src/routes/route_ledger.rs` and its module's `*_route_manifest()`; the ledger guards against silent `Router::merge` path collisions and is exported via `ledger_export.rs` as the SDK contract source.
- Non-standard/private endpoints must use the `/_matrix/vendor/v1` prefix (ISSUE-13 migration), namespaced apart from Matrix stable and MSC identifiers.

### Dependency wiring
- `synapse-services/src/container.rs` is the main dependency graph for application features.
- It constructs storages and services for auth, rooms, sync, sliding sync, E2EE, federation helpers, media, push, retention, feature flags, worker integration, and more.
  (The client "report" routes in `synapse-web/src/routes/moderation.rs` and member
  management in `synapse-services/src/room/membership/moderation.rs` are unrelated to
  the removed moderation rule-engine domain, which had its own storage/service here.)
- If you need to understand how a feature is actually enabled end-to-end, start at `ServiceContainer::new(...)`, then trace the relevant router and storage.

### Storage and schema model
- Postgres is the primary source of truth.
- `synapse-storage/src/lib.rs` re-exports domain-specific storages; most features have a corresponding storage module.
- `synapse-storage/src/schema_health_check.rs` is part of startup validation. Missing critical tables/columns fail startup. **Exception**: `SYNAPSE_SKIP_SCHEMA_CHECK=I_UNDERSTAND_SCHEMA_CHECKS_ARE_SKIPPED` bypasses all schema health checks at startup (logged at **error** level) — this escape hatch exists for emergency recovery scenarios and should never be used in production. It deliberately does **not** accept `true`: a plain boolean could be switched on by habit, by a copy-pasted snippet or by a stale `.env` line, and a bypass that reads like an ordinary feature flag never looks like a decision. Any other value (including the old `true`) means "do not skip" — the check runs as if the variable were unset, with a warning naming the required sentinel (`schema_check_skip_requested` in `src/server/database.rs`).
- Runtime DB initialization is intentionally not the default path. The expected migration flow is externalized through `docker/db_migrate.sh`; server startup only performs schema health checks unless `SYNAPSE_ENABLE_RUNTIME_DB_INIT` is explicitly enabled and `SYNAPSE_SKIP_DB_INIT` is not set.

### Configuration model
- Config types live in `synapse-common/src/config/` (the root crate's `src/common/config/` is a thin re-export).
- Main config is file-based (`SYNAPSE_CONFIG_PATH`, default `homeserver.yaml`) with `SYNAPSE_` environment variable overrides using `__` for nesting.
- Docker uses `docker/config/homeserver.yaml` and mounts `docker/config/rate_limit.yaml` — `docker/config/` is the single config source for both compose stacks and for the image; there is no second copy under `docker/deploy/`.
- Search must exist structurally in config; when Elasticsearch is not used it should still be explicitly disabled.
- Prefer `server.server_name`/`ServerConfig::get_server_name()` for Matrix identity. `server.name` exists for compatibility but can differ from the public Matrix server name in delegated or reverse-proxy deployments.
- Federation config has its own `federation.server_name`; when validating local identity, account for all locally accepted names rather than hard-coding one config field.

### Caching and async/background work
- Redis is optional but first-class. When enabled, the server uses Redis-backed cache and task queue infrastructure; otherwise cache falls back to local memory.
- `ScheduledTasks` and task metrics are initialized in `src/server/mod.rs`.
- Worker-related code lives under `src/worker/`, with an additional binary in `src/bin/synapse_worker.rs` for queue/replication/metrics processing.
- The worker subsystem includes Redis bus support, replication protocol, health checking, and load balancing abstractions.

### Major feature domains
- `synapse-e2ee/` (+ the root crate's `src/e2ee/` facade): device keys, cross-signing, megolm/olm, verification, secure backup, to-device flows.
- `synapse-federation/` (+ `synapse-web/src/federation/` glue, which depends on the HTTP context): federation transport/auth logic.
- `synapse-services/src/search_service.rs`: supports optional Elasticsearch as well as Postgres-backed search/FTS paths.
- `synapse-services/src/room/`, `src/sync/`, `src/sync_service/`, `src/sliding_sync_service/`: core Matrix room and sync flows.
- The repo also contains non-standard/private-chat extensions described in `README.md`: trusted private chat (`preset=trusted_private_chat`), anti-screenshot signaling (`com.hula.privacy`), and burn-after-read (feature `core-private-chat = friends + burn-after-read`).

## Matrix/Synapse protocol guidance

### Current external baselines
- Treat Matrix Specification latest as the normative protocol source. As of 2026-05-29, the latest published spec is v1.18.
- Treat `element-hq/synapse` as the main behavioral reference for production homeserver tradeoffs. As of 2026-05-29, the latest stable tag observed was `v1.153.0`, with `v1.154.0rc1` as the latest pre-release.
- When changing compatibility-sensitive behavior, record the spec/Synapse version you used in the relevant doc or test name so the baseline is auditable later.

### Protocol declaration discipline
- Be conservative with `/_matrix/client/versions` and `/_matrix/client/v3/capabilities`: only declare stable versions or MSCs that are backed by implementation and tests.
- `synapse-web/src/routes/handlers/versions.rs` currently owns versions, `.well-known`, and capabilities. If changing this surface, prefer typed builders and snapshot/contract tests over ad hoc JSON edits.
- Room-version capability must match actual event/auth behavior. Do not add a room version to `m.room_versions` until create/join/upgrade/redaction/state-resolution behavior is reviewed.
- Keep custom Hula/private-chat extensions namespaced and clearly separated from Matrix stable and MSC identifiers.

### Federation safety rules
- Federation request signing depends on canonical JSON over `method`, `uri`, `origin`, `destination`, and optional `content`. Any change here needs focused tests.
- `Authorization: X-Matrix ...` parsing should be tolerant of normal header formatting, but strict about required fields and signature verification.
- Server key responses and notary query responses have different shapes. Validate `server_name`, `verify_keys`, `old_verify_keys`, `valid_until_ts`, and signatures before caching remote keys.
- Do not weaken origin/user-domain checks in federation membership, device key query/claim, media, or directory endpoints. Prefer returning `M_NOT_FOUND` where the spec expects avoiding room/user existence leaks.

### Upstream Synapse lessons to preserve
- Performance-sensitive experimental behavior should have rollback criteria. Synapse reverted a sliding-sync optimization in v1.153.0rc3 after performance issues.
- Long-running deployments need pruning/background-update paths for device list changes, presence-like state, media quarantine history, and other append-only streams.
- Worker deployments need explicit ownership validation for routes, stream writers, background jobs, and admin operations.
- Canonical JSON, event signatures, and `unsigned` handling are hot and security-sensitive paths; keep tests close to spec vectors and Synapse behavior.

## Repo-specific guidance
- Prefer existing migration/check scripts in `scripts/` and `docker/` over inventing new one-off commands.
- For test expectations and gate definitions, use `TESTING.md` as the current source for what counts as main gate vs extended/manual verification.
- For current capability/status documents, start from the docs index in `README.md` under `docs/synapse-rust/`.
- This repository is broad and heavily modularized; when changing behavior, confirm all three layers affected by the feature: route, service, and storage.
- For the current Matrix/Synapse gap analysis and phased optimization backlog, start from `docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md`.
- Keep route declarations in sync with `synapse-web/src/routes/route_ledger.rs` and the route manifests. New routes should have manifest entries and duplicate-route coverage.
- If a test requires Postgres setup and hangs in local integration setup, first run the same target with `--no-run` to distinguish compile failures from environment blockers.
- **Format debt is at zero and the CI ratchet is strict (`baseline=0`, both increase AND decrease fail).** Run `cargo fmt --all` before every commit rather than avoiding it — with debt at 0 there is no drift to "hide", and a stale format will fail CI. After large multi-file changes, always verify with `./scripts/check_fmt_ratchet.sh`.
- When a route/format refactor changes line numbers, update `scripts/shell_routes_allowlist.txt` (it is line-number based and drifts with `cargo fmt`) — otherwise placeholder_scan tests fail.
- Snapshot tests (`api_route_ledger` / `api_route_snapshots` / `login_flows_v3`): NEVER run `cargo insta accept` after seeing failures under a narrow feature set — re-run with `--all-features` first; the failures may be feature-set artifacts, and accepting them would bake a false baseline.

## Known pitfalls (cross-referenced from CLAUDE.md §踩过的坑经验总结)
- **Never `unwrap_or_default()` on DB queries in security-relevant paths** (token replay detection, federation prev_events, lazy-load membership) — it silently converts DB errors to "empty/false" defaults. Propagate with `?` or fail-closed.
- **Cache read/write symmetry**: `set_raw` writes L1+L2 async; sync `get_raw` reads L1 only. Cross-instance correctness REQUIRES `get_raw_shared().await`.
- **EDU drop paths** must return `{ dropped: 1, ..Default::default() }`, not `EduProcessResult::default()` (which carries `dropped: 0` and breaks counter aggregation).
- **MSC number discipline**: verify MSC titles against matrix-spec-proposals before wiring routes. MSC4155=Invite filtering, MSC4156=Migrate server_name to via (NOT thread subscriptions — those are private per-user state over account_data and need no EDU federation).
- **`[patch.crates-io]` name iron law**: the patch source crate name must equal the target crate name; there is no rename mechanism. proc-macro crates also cannot re-export another crate's `#[proc_macro]` items — forking is the only path.
- **Batch regex rewrites on Rust attributes**: always dry-run the diff; single-line regexes like `#\[derive\(([^)]+)\)\]` can eat adjacent tokens in multi-line attribute syntax.
- **`tokio::spawn` panics are swallowed** when JoinHandles are dropped; fire-and-forget tasks need `AssertUnwindSafe(..).catch_unwind()` supervision or abort policies. Background loops must wire `CancellationToken` into `tokio::select!` (including reconnect sleeps).

## TDD Workflow

This project follows Red-Green-Refactor TDD. Before implementing any new behavior or fixing a bug, consult:

- **Workflow skill**: `.claude/skills/tdd-rust/SKILL.md` — mandatory Red-Green-Refactor self-check, Cargo command binding, Mock adapter decision tree, insta snapshot rules.
- **Execution checklist**: `.trae/documents/TDD落地执行清单.md` — phased rollout plan (Phase 1–4) with concrete task IDs (P1-x, P2-x, STO-x, FED-x, SYNC-x, P4-x).

### When TDD applies (mandatory)
- Any new feature or route handler behavior
- Bug fixes that touch service/storage logic
- Refactors that change a public response shape

### When TDD does not apply (exceptions)
- Pure formatting / doc-only changes
- Mechanical dependency bumps with no behavior change
- Test infrastructure / fixture-only changes

### Snapshot tests (insta)
- Lock API output shapes for high-frequency routes (`login`, `register`, `sync`, `join`, `profile`).
- Snapshots live under `tests/integration/snapshots/`.
- Dynamic fields (access_token, refresh_token, expires_in, origin_server_ts, user_id suffixes) MUST be redacted via `.redact()` — see SKILL.md §5.
- New snapshots: run `cargo insta test --review` to accept; never commit snapshots you did not review.
- CI asserts snapshots with `INSTA_UPDATE=no` plus a committed/leftover `.snap.new` check, and does **not** use cargo-insta at all (its CLI semantics changed twice) — see the `Snapshot gate` step in `ci.yml`.

### Pre-positioned Mocks
- `synapse-storage::test_mocks::FakeUserStore` / `SharedFakeUserStore` / `seed_locked_users()`
- `synapse-federation::test_mocks::MockFederationClient` (in-memory, pending trait extraction FED-1..4)
- `synapse-services::test_mocks::MockSyncServiceDepsBuilder` (scaffolding pending SYNC-1..6)

### TDD cycle commands (vertical slicing)
```bash
# RED: write one failing test, run it
cargo nextest run -p <crate> <test_name> -P tdd  # or: cargo test -p <crate> <test_name> -- --nocapture
# GREEN: minimal code to pass
# REFACTOR: keep tests green; cargo clippy --all-features --locked -- -D warnings
```
