# 全量套件隔离验证 — 2026-09-22 复核

> **目的**：把 2026-09-22 那次 `543 passed / 883 FAILED` 的"全量套件"结果定性，并给出可复现的干净复核。
> **结论**：失败与被测代码无关，根因是**测试进程被改写的源码树与共享测试库耦合**；隔离三个共享面后，
> 两次运行均 `exit 0`。**判据**：本文 §2 尸检、§3 隔离证据、§4 原始命令与计数。
> 本仓铁律 8 的推论同样适用：一次"全量绿"若没有证明它到底跑了哪些 target，就不能当作全量绿。

---

## 1. 结论摘要

| 项 | 值 |
|---|---|
| 被测提交 | `3ca9cb46715ef3eaea20384fe39ee0c9c698e0d4`（`main`） |
| 被测工作树 | 独立 `git worktree`，全程 `git status --short` 为 **0** 项 |
| 运行 A（AGENTS.md 原命令） | `cargo test --all-features --locked -- --test-threads=4` → **exit 0**，3308 passed / 0 failed / 13 ignored |
| 运行 B（member crate 补齐） | 同参数 + `-p <8 个 member>` → **exit 0**，6139 passed / 0 failed / 2 ignored |
| 合计 | **9447 passed / 0 failed / 15 ignored** |
| 施工产物 | 私有 `CARGO_TARGET_DIR`、专用测试库 `synapse_test_p3verify` |

日志留档：`/tmp/p3verify-suite.log`（3490 行）、`/tmp/p3verify-members.log`。

---

## 2. 昨夜那次结果的定性（尸检）

`543 + 883 = 1426`，而 **1426 精确等于 root `tests/integration` 这一个 target 的用例总数**。

**判据**：`cargo test` 默认 fail-fast —— 某个 target 失败后不再执行后续 target。因此那次运行的失败
**全部发生在 `tests/integration` 内部**，随后即中止，`unit`（1805）、`e2e`、`performance` 以及 8 个
member crate 的 lib target **一个都没跑**。

**结论**：该次结果既不能证明也不能否证代码质量，它是"运行期间源码树被替换、共享测试库被改写"的产物。
用它的失败数做任何判断都是错的。

---

## 3. 隔离方法：逐一拆开三个共享面

| 共享面 | 昨夜（作废） | 本次 |
|---|---|---|
| 源码树 | 主工作树，运行中被合并提交与另一会话的编辑改写 | 独立工作树钉死上述 SHA，只读使用 |
| 构建产物 | 与对方共享 `target/`，**复用了对方树编出的测试二进制** | 私有 `CARGO_TARGET_DIR`，本地 crate 全部重新编译到新路径 |
| 测试库 | 共享 `synapse_test`，`public` schema 被反复清空 | 专用库 + 专用 ready-marker 目录 |

### 3.1 跨树复用测试二进制已被证伪

共享 `CARGO_TARGET_DIR` 曾导致测试二进制内嵌**另一棵树**的绝对路径，使 `ts_order` 扫描到对方的
文件（实测 76 vs 103 处），从而产出与代码无关的红绿。本次在运行前做了两项防护并实测：

- 清空私有 target 内旧的模板 ready-marker；
- 将工作树内全部 1705 个文件 `touch` 一遍 —— 本地 crate 因 mtime 失效而全部重编，第三方依赖仍复用缓存。

**判据**（`strings` 检查已编译的测试二进制）：

```
unit 二进制：      指向旧树(已删除) 0 处，指向本树 104 处
integration 二进制：指向旧树 0 处，指向本树 207 处
```

### 3.2 测试库隔离

- 库名必须含 `test`，否则 `synapse-test-utils` 的 `DROP SCHEMA public` 守卫会 fail-closed 拒绝
  （判据：`synapse-test-utils/src/lib.rs` 的 `is_test_db` 判据），故取名 `synapse_test_p3verify`；
- 播种后 `public` 与 `test_template_ci` 各 **227** 张表；
- ready-marker 必须落在测试运行时真正读取的 `CARGO_TARGET_TMPDIR`（即私有 target 的 `tmp/`），
  否则播种与断言会看两个不同目录；
- 运行结束后共享库 `synapse_test` 仍为 227/227 表，未被本次触碰。

---

## 4. 复现命令

```bash
cd <独立工作树>
export SQLX_OFFLINE=true \
       TEST_DATABASE_URL="postgresql://synapse:synapse@localhost:5432/synapse_test_p3verify" \
       TEST_DB_TEMPLATE_SCHEMA=test_template_ci \
       CARGO_TARGET_DIR=<私有 target> \
       INSTA_UPDATE=no

# 建库 + 播种（可重复执行，幂等）
bash scripts/ci/prepare_test_db.sh

# 运行 A：root package
cargo test --all-features --locked -- --test-threads=4

# 运行 B：8 个 workspace member（补齐运行 A 覆盖不到的部分，见 §5）
cargo test --all-features --locked -p synapse-common -p synapse-cache -p synapse-storage \
  -p synapse-e2ee -p synapse-federation -p synapse-services -p synapse-test-utils -p synapse-web \
  -- --test-threads=4
```

各 target 实测：root integration 1426（1135s）、root unit 1805、`synapse-services` 2047、
`synapse-storage` 1763、`synapse-common` 920、`synapse-web` 646、`synapse-e2ee` 441、
`synapse-federation` 224、`synapse-cache` 92。所有 target 均 `0 failed`；integration 在
`--test-threads=4` 下未出现并发型假失败。

**说明**：日志里唯一的 `CHECK FAILED` 是**故意的门禁自证**（测试植入 stale 路由表以证明
`gen_route_table --check` 会变红），其上下文用例全部 `ok`，不是失败。

---

## 5. 顺带发现的缺陷：AGENTS.md 的"Full test suite"口径不足

AGENTS.md 把 `cargo test --all-features --locked -- --test-threads=4` 标注为 **Full test suite**，
但 root `Cargo.toml` 没有 `default-members`，所以该命令**只执行 root package 自己的 target**。

**判据**：运行 A 的日志中 `Running unittests src/lib.rs` 只出现一次（root crate），
**没有任何 workspace member 的 lib target**；而运行 B 的 8 个 member 带来了 **6139** 个用例 ——
比运行 A 实际跑的 3308 还多。

**影响**：按该命令自检会得到"全量绿"的假结论，且漏掉的正是承载大部分业务逻辑的 crate。

**CI 本身没有这个问题**（不需要修）：

- `ci.yml` 的 lib 批次是 `cargo nextest run --workspace --lib --all-features --locked --test-threads 4`；
- `scripts/ci_backend_validation.sh` 逐字执行同一批次；
- `TESTING.md` 已写明 lib 步骤必须 `--workspace` 或显式 `-p`，并有用例锁住每个 nextest 调用必须声明作用域。

**修法**：只改文档口径（把"Full test suite"改为 `--workspace`，并指向 `scripts/ci_backend_validation.sh`
作为 CI 同款入口）。**不要**用 `default-members` 修 —— 那会把 root package 从 `cargo build` /
`cargo clippy` 的默认集合里挤出去，属于用一个更大缺陷换小缺陷。

---

## 6. 残留与建议（本节四项均已处置，见下）

1. **共享库 schema 残留** —— 已量化，**未执行清理（需安静窗口）**。
   实测：共享 `synapse_test` 在 2026-09-22 第二次复核时 `cleanup_test_schemas.sh` 干跑报
   **待清理 1295 个 schema**（首次复核为 1281，期间被其它会话的运行自然回收了一部分）。
   脚本默认即干跑（`APPLY=0`，仅 `--apply` 才 DROP），且会保护各 live 模板。
   **为何不执行**：清理会 CASCADE 掉其它会话正在使用的 per-test schema，使其运行出现假红 ——
   这正是本报告通篇在防的跨界伤害类型。执行命令（请在确认无人跑测试时运行）：

   ```bash
   DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test \
     bash scripts/cleanup_test_schemas.sh           # 干跑，打印待清理数量与样本
   DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test \
     bash scripts/cleanup_test_schemas.sh --apply   # 确认后实际执行
   ```

   另注意：残留仍会增长，因为清理**没有**任何自动调用点。若要让"膨胀-清理"不再靠人工，
   应在某个已自动执行的入口（例如 `scripts/ci/prepare_test_db.sh` 播种前）加一次带白名单的
   清理 —— 这属于行为改动，需单独评估与裁定，本报告只登记。
2. **本地全量入口收敛** —— ✅ 已闭环。三处口径现已一致且都以 CI 为准：
   `AGENTS.md` 的 "Full test suite" 已改为 `--workspace` 并指向 `scripts/ci_backend_validation.sh`；
   `CLAUDE.md` 的 "Full suite" 同步改为 `nextest --workspace --lib`；
   `TESTING.md` 本就写明本地封装是 `scripts/ci_backend_validation.sh`、权威入口是 `ci.yml`
   （并记录了旧第二份实现 `scripts/run_ci_tests.sh` 已删除）。剩余的手工命令行只作为"这条命令到底跑什么"的解释而保留。
3. **文档门禁的版本敏感性** —— ✅ 已治本。`docs-quality-gate.yml` 不再安装 `markdownlint-cli` 的
   浮动 `latest`，改为钉死 **`markdownlint-cli@0.49.1`**（当前 latest），并在 workflow 注释里写明原因与实测：
   `0.37.0` 会把 `CLAUDE.md` 末尾空行判为 `MD012` 而 0.49.1 接受，于是"本地钉旧版"必然产生假红。
   钉死后本地与 CI 可以逐字对齐。
4. **测试代码里写死的 `synapse_test` URL** —— ✅ 已闭环，且发现范围比本节原先记录的更大：
   - 5 处 `connect_lazy("postgresql://…/synapse_test")`（`account_identity_service.rs` ×2、
     `saml_service.rs` ×3）改为 `&synapse_common::test_isolation::test_database_url()`，
     即本仓唯一的解析实现（优先级 `TEST_DATABASE_URL` → `DATABASE_URL` → 约定值）；
   - 复核时另发现 **第二份解析链**：`synapse-services/src/test_config.rs` 自己的
     `test_database_url()`（`unwrap_or_else` + `postgres://…/synapse_test`）。它的调用点
     `container.rs:645` 决定了它不能直接删除，故改为**委托**单一实现。这不只是重复实现
     （AGENTS.md 铁律 2），它还**绕过了 CI 下的 fallback 闸门** ——
     `every_resolver_copy_disables_the_fallback_under_ci` 只覆盖 5 个 canonical 副本，而它不在其中，
     因此在 `CI=1` 且 env 缺失时会静默连到 localhost。委托后该闸门自动覆盖此调用点；
   - 为防止再次长出第二份 chain，在既有约定门禁
     `tests/unit/test_db_url_convention_tests.rs` 中新增
     `DELEGATING_RESOLVERS` + 纯谓词 `delegates_to_the_shared_resolver()` +
     `delegating_resolvers_have_no_chain_of_their_own`，并配 `the_delegation_checker_rejects_a_second_chain`
     作红证明（把修复前的 chain 原文喂给谓词，必须被判为不合规）；
   - 该文件原有的两个测试（`test_database_url_default` / `_from_env`）随委托一并删除：
     它们断言的是已不存在的旧默认串，且直接 `set_var`/`remove_var` 而未持 env 锁（多线程二进制里的竞态源）。

---

## 7. 后续复核：`main` 推进到 `048a0fc6` 之后

验证完成后 `main` 又推进了一个提交：`048a0fc6 feat: Add unit tests to 7 Quick Win coverage files`
（**1313 行纯新增、0 删除**，且逐文件核对确认新增行**全部落在 `#[cfg(test)] mod tests` 内**，
生产代码零改动）。因此 §1 的结论对该尖端仍然成立：**生产行为与已验的 `3ca9cb46` 完全一致**。

但该提交自身带入了新的红：

### 7.1 fmt 棘轮变红（严格棘轮，baseline=0）

| 尖端 | 门禁输出 | 结果 |
|---|---|---|
| `3ca9cb46`（本次验证的 SHA） | `fmt debt: current=0 baseline=0` | OK |
| `048a0fc6`（当前 `main`） | `fmt debt: current=49 baseline=0` | **RED** |

违规分布（4 个文件，共 **15 处**不同违规点）：`rtc/metrics.rs` 7、`admin/security.rs` 4、
`olm/service.rs` 2、`server_notification/repository.rs` 2。

⚠️ **计数口径提醒**：门禁报的 `49` 不是 49 个不同位置 —— 独立 `rustfmt --check` 在按目录批量传入时
会沿 `mod` 递归把子模块重复计入，同一文件被重复报告 3～4 次（`6+6+21+16=49`）。
判据是"同一文件同一行号重复出现"。判断"是否变红"不受影响，但**引用数字时不要把它当成不同缺陷数**。

### 7.2 `--all-features` 下测试代码编译失败

| 位置 | 诊断 |
|---|---|
| `synapse-web/src/routes/cas.rs:389` | `E0063`：`CasRegisteredService` 初始化缺 `allowed_attributes` 等 7 个字段 |
| `synapse-storage/src/server_notification/repository.rs:1079/1100/1134/1162/1180` | `E0308`：类型不匹配 ×5 |
| `synapse-web/src/routes/admin/security.rs:211` | 未使用导入 `serde_json::json`（clippy 带 `-D warnings` 时同样红） |

**判据**：以 `-p` 选择受影响 crate 复跑，`cargo test --all-features` 直接以 exit 101 结束，
日志中**没有任何一行 `test result`**（即根本没进入运行阶段）。
CI 的 blocking lib 批次是 `cargo nextest run --workspace --lib --all-features`，
`--workspace` 是本次 `-p` 选择的超集、且 feature 只增不减（`E0063` 是"结构体字段变多"，
只会更多不会更少），故该批次同样失败。

**结论**：`main` 当前**不是绿的**。这批新测试需先 `cargo fmt --all` 并修掉上述编译错误，
才能重新认定 `main` 为绿；§1 对 `3ca9cb46` 的验证不受影响。另注意这批测试是在
**未跑 `--all-features`** 的情况下提交的 —— 与 AGENTS.md/`TESTING.md` 反复强调的
"`--all-features` 是 CI 口径、窄 feature 集会产生假绿"是同一类问题。

### 7.3 修复与闭环

修复提交 `bc1bad41`（7 个文件）。修编译之后又暴露出**断言/夹具**层面的缺陷 —— 这类缺陷
不会再让编译失败，而是让用例"因错误的理由红或绿"，正是本仓最在意的一类。

| 类别 | 具体修复 |
|---|---|
| fmt 棘轮 | `cargo fmt --all`（4 个文件、15 处） |
| 编译 `E0063` | `cas.rs` 的 `CasRegisteredService` 测试补齐 7 个字段（`From` 实现只读其余 5 个，故补齐不影响该用例的证明力） |
| 编译 `E0308` ×5 | `repository.rs` 的 `created_by` 是 `Option<String>`，5 处包 `Some(..)` |
| clippy | 删未使用导入 `serde_json::json`；`vec!` 仅用于 `sort_by` → 改数组 |
| 断言/夹具 | ① `test_e06_too_long_returns_error` 原用 65 位（奇数）输入，**先撞 hex 奇偶校验、根本没走到长度分支** → 改 66 位（33 字节）；② `test_e06_empty_string_returns_error` 断言 `"32 bytes"`，真实消息是 `is 0 bytes, must be exactly 32` → 改为 `"must be exactly 32"`；③ `test_olm_service_new_has_empty_state` 是普通 `#[test]` 却调用 `connect_lazy`（sqlx 0.8 会建内部后台任务，需要 Tokio 上下文）→ 改 `#[tokio::test]`，并把注释里"没 panic 即证明状态为空"换成对四个字段的直接断言；④ `push_notification` 的 `valid_config_accepted` 用了不存在的键 `apns.token`（允许列表的真相源只有 `apns.topic`）→ 改 `apns.topic` |
| 既有 flake（**非**本提交引入） | `synapse-common` 的 `released_pool_triggers_cleanup_without_any_sweep`：它用 `try_recv` 断言，而"本条目的 `on_release` 由本线程还是后台 janitor 执行"是竞态（`cleanup.take()` 只保证恰好一个执行者）—— janitor 先取走条目时，其回调可能尚未 `send`，`try_recv` 必然拿到 `Empty`。**实证**：同一二进制、同一旗标、同一位置（928/6217）在两次批次运行中一次通过一次失败 → 改为带上限的 `recv_timeout`（不变量是"`on_release` 会被执行"，而不是"它在本线程执行"），修复后连跑 10 次全稳 |

**红 → 绿对照（同一命令、同一工作树）**：

| 门禁 | 修复前 | 修复后 |
|---|---|---|
| `./scripts/check_fmt_ratchet.sh` | `current=49 baseline=0` → RED | `current=0 baseline=0` → OK |
| `cargo nextest run --workspace --lib --all-features --locked --test-threads 4` | exit 101（编译失败）；修编译后 6215/6217 | **exit 0 · 6217 passed / 0 failed**（`--no-fail-fast`，2122s） |
| clippy `--all-features … -D warnings` | 1 error | 0 error |
| clippy 默认档 `-D warnings` | 0 error | 0 error |

**方法说明**：最终批次加了 `--no-fail-fast`。若该模式下 6217 个用例全过，则**不加该旗标的 CI 命令必然也过** ——
fail-fast 只在遇到失败时提前停止，不可能把"通过"变成"失败"。故一条 `--no-fail-fast` 全绿足以裁定该批次为绿。
为免疑义，随后又**实跑了不带任何额外旗标的 CI 原样命令**：`exit 0`，`6217 passed / 0 failed`（1966s）。

### 7.4 尖端漂移：本分支只覆盖基线 `048a0fc6` 的那一批

修复完成后 `main` 继续推进（`048a0fc6` → `ff4a23d2`，3 个提交，均为"继续补测试"）。实测当前尖端：

| 项 | `048a0fc6`（本分支基线） | `ff4a23d2`（当前 `main`） |
|---|---|---|
| fmt 棘轮 | `current=49 baseline=0` | **`current=106 baseline=0`** |
| 违规文件 | 4 个 | **7 个**：原 4 个 + `invite.rs`、`handlers/dehydrated_device.rs`、`external_service.rs` |

**判据**：新提交**没有触碰**本分支修复的 6 个文件（`git diff --name-only 048a0fc6 ff4a23d2 -- <6 files>` 为空），
因此那些缺陷在当前 `main` 上**依然存在**；同时新加的 3 个测试文件又带来新的 fmt 债务。

**推论**：本分支合并进当前 `main` 后，fmt 门禁**仍会红**（`invite.rs` 等 3 个文件不在本分支范围内），
且这 3 个文件的新测试同样**未经 `--all-features` 验证**（是否带同类 `E0063`/`E0308`/断言缺陷，不编译看不出来）。
根因不是"漏了一批文件"，而是**提交时没有跑 `cargo fmt --all` 与 `--all-features` 自检** ——
本仓 AGENTS.md 已明文要求两者，属执行缺口而非规则缺口。故本次不止修文件，还补上了执行点（下段）。

**结构性处置（已实施）**：把 `./scripts/check_fmt_ratchet.sh` 接进 `.githooks/pre-commit`，并把
`core.hooksPath` 指向 `.githooks`。复核时发现一个更根本的事实：**该 hook 早已存在却从未执行** ——
`core.hooksPath` 指向默认的 `.git/hooks`，其中只有 `*.sample` 文件，而 hook 的 Stage 1 本来就是 fmt 检查。
所以"规则明文要求 `cargo fmt --all` 却连续两次未格式化入库"不是规则缺失，而是**执行点从未接上**。

自证（铁律 8：门禁必须自证能变红）：

| 场景 | 结果 |
|---|---|
| 净树直接运行 hook | exit 0，耗时 **7.0s** |
| 插入未格式化探针后直接运行 | exit 1，并列出 `Diff in …/zz_fmt_probe.rs:1` |
| 探针 `git add` 后执行**真实 `git commit`** | **exit 1 且 HEAD 未变**（提交确实被拦下） |
| 清理探针后再运行 | exit 0 |

顺带修正两处实现问题：

1. hook 的 fmt 阶段改为调用棘轮脚本（单一实现 = CI 门禁），从而也能拦住"债务下降却未收紧 baseline"这一方向；
2. 原 clippy 阶段作用域不足（`cargo clippy --all-features` 不带 `--workspace`/`--all-targets`，与 §5 那个
   `cargo test` 口径缺陷同型），已改成 CI 同款 `--workspace --all-targets --features test-utils --all-features`；
   但全工作区 clippy 需数分钟，故改为 `SYNAPSE_PRECOMMIT_CLIPPY=1` 按需启用 —— **格式阶段保持无条件阻断**。

### 7.5 无断言用例与扫描方法论

1. **已处置**：`synapse-web/src/routes/burn_after_read.rs` 的
   `test_create_burn_after_read_router_creates_routes` 函数体内只有
   `let _router_fn = create_burn_after_read_router;` —— 它取函数指针而**从不调用**，运行时零断言
   （注释自述"验证函数存在且能编译"，而编译通过由编译器保证）。**已删除**，理由有二：
   - 该函数已有**真实**的路由级契约测试：`tests/unit/burn_after_read_route_tests.rs` 覆盖
     v1/v3 路径清单、各端点 JSON 形状、错误码映射与逻辑镜像。in-crate 这条是重复的；
   - 它从未执行被测函数体，因此删除**不影响任何覆盖率**（`scripts/ci/coverage_baseline.json` 中
     也不存在 `burn_after_read.rs` 条目，棘轮无从下降）。保留一条"看起来在测、实际没测"的用例
     比缺少它更糟，符合 AGENTS.md 铁律 1 的删冗余取向。
   说明：同一模块里另有两个 mirror 式用例（`test_set_global_burn_config_*`）是在本地重算逻辑再断言，
   与 `tests/unit/burn_after_read_route_tests.rs` 头部声明的"pure-logic mirrors"是同一种既有约定，
   故未改动 —— 只处理了"完全没有断言"这一更明确的缺陷。
2. 严判据扫描本批 1313 行新测试（63 个测试函数，排除辅助函数）：仅上述 1 个用例完全没有断言，
   其余均有 `assert`/`panic!`/`unwrap_err` 等真实断言。**注意**：这类扫描必须从 `fn` 行本身起算
   大括号配平 —— 否则会在结构体字面量的收尾 `};` 处误判为函数结束，从而把"先构造结构体、后断言"
   的用例全部错标为"无断言"（本报告作者第一次扫描即踩此坑，得到 23 个假阳性后修正）。

---

### 7.6 第二次清扫（追上 `ff4a23d2`）：同型缺陷再次出现

`main` 在本分支基线之后又推进 3 个提交（`0651688a`、`ec495e4f`、`ff4a23d2`，均为"继续补测试"）。
本分支已 rebase 到 `ff4a23d2` 并完成清扫；`cargo fmt --all` 这次只改动新增的 3 个文件
（`federation/membership/invite.rs`、`handlers/dehydrated_device.rs`、`external_service.rs`）。

但以 CI 口径复跑后，**同一类缺陷再次出现**（判据：`nextest --workspace --lib --all-features`
直接 exit 101，日志中**没有任何一行 `test result`**，即根本没进入运行阶段）：

| 位置 | 诊断 | 处置 |
|---|---|---|
| `external_service.rs:530` | `E0425`：`serde_json::to_json` 不存在 | 改 `to_string`，并把 `contains("signature")` 换成对**解析后取值**的断言 |
| `external_service.rs:517` | `E0277`：`ApplicationService` 未实现 `Default`（它是 `FromRow` 模型） | 逐字段写出；`From` 只读 `as_id`/`is_enabled`/`created_ts`，其余为惰性夹具 |
| `external_service.rs:528` | `E0063`：`WebhookPayload` 缺 `event_type`/`timestamp` | 补全 |
| `handlers/dehydrated_device.rs:131` | 未使用导入 `super::*`（clippy 带 `-D warnings` 时红） | 删除 |

**结论比上一轮更强**：这不是一次性失误。两轮、相隔数小时、不同文件、不同作者时段，
都以同一模式出现 —— 新增测试**从未在 `--all-features` 下编译过**。这再次印证 §7.3/§7.4 的判断：
缺的是**提交前的执行点**，不是规则。当前 pre-commit hook 会在**格式**上拦住它；
**编译**（`--all-features`）仍属 CI 职责 —— 本报告不建议把数分钟的编译塞进 pre-commit，
但要求每次提交前至少自跑一次 `cargo nextest run --workspace --lib --all-features --locked`。

### 7.7 本轮全量验证（代码状态 = `ff4a23d2` + 本分支全部修复）

| 批次（CI 口径） | 结果 |
|---|---|
| `nextest --workspace --lib --all-features --test-threads 4` | **6229 passed / 0 failed**（2367s） |
| `nextest --test unit --all-features --test-threads 4` | **1807 passed / 0 failed / 2 skipped**（含本轮新增的 2 条委托守卫；`+2` 与新增条数一致） |
| `nextest --test integration --all-features --test-threads 4` | **1426 passed / 0 failed**（4707s） |
| clippy `--all-features … -D warnings` | 0 error |
| clippy 默认档 `-D warnings` | 0 error |
| `./scripts/check_fmt_ratchet.sh` | `current=0 baseline=0` → OK |
| **合计** | **9462 passed / 0 failed** |

**方法说明（值得记录的一次自伤）**：integration 第一次跑到 257/1426 时被本报告作者**主动中止**，
原因是**自己的专用测试库积累了 575 个残留 schema**，目录膨胀让每个用例明显变慢。
随后按 §6.1 的方法清理该库（569 个 schema；`test_template_ci` 229 表与 `public` 227 表完好）后重跑，
才得到上表结果。教训与 §6.1 同源：**per-test schema 残留首先是"慢到不可用"的前兆，其次才是空间问题** ——
`--apply` 的清理窗口不只是为了不被别人打断，也是为了让自己的验证跑得动。
