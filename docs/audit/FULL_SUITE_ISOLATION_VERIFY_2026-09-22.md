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

## 6. 残留与建议

1. **共享库 schema 残留**：验证时实测共享 `synapse_test` 已积累 **1281** 个 per-test schema，
   专用库仅 10 个。残留量本身是超时型假失败的来源，但清理动作会打断其他会话正在跑的测试，
   未在本次执行；建议在无人跑测试的窗口用 `scripts/cleanup_test_schemas.sh` 处理。
2. **本地全量入口建议收敛**：目前"全量"有多个手工命令行版本（AGENTS.md 的 root-only 版本、
   `TESTING.md` 的 `--workspace --lib` 版本、`scripts/ci_backend_validation.sh`）。
   建议以 `scripts/ci_backend_validation.sh` 为唯一本地入口，文档只引用它。
3. **本地复跑文档门禁必须用 CI 同版本**：`docs-quality-gate.yml` 安装的是 `markdownlint-cli` 的 **latest**。
   本机若钉旧版（实测 `0.37.0`）会把 `CLAUDE.md` 文件末尾的空行判为 `MD012`，而 CI 版本不报 —— 这是**假红**。
   假红与假绿同样损伤门禁可信度：它会让复核者去"修"一个并不存在的问题。本次已用 CI 同版本复核三个改动文件，均为绿。
4. **测试代码中写死的 `synapse_test` URL**：`synapse-services/src/{account_identity_service,saml_service}.rs`
   存在 `connect_lazy` 形式写死库名。本次未造成跨界（这些用例使用 in-memory store，
   lazy 池未取连接），但它们使 `TEST_DATABASE_URL` 的隔离在这些用例上不是结构性的，建议改为读同一配置源。

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

### 7.5 非阻塞观察（本次未改动）

1. `synapse-web/src/routes/burn_after_read.rs` 的 `test_create_burn_after_read_router_creates_routes`
   函数体内只有 `let _router_fn = create_burn_after_read_router;`，运行时**不断言任何东西**
   （注释自述"验证函数存在且能编译"—— 而编译通过由编译器保证）。
   它与本仓既有的"router 结构"测试（如 `test_reactions_routes_structure` 只对硬编码字符串数组断言）
   同属弱测试，不是本提交新引入的标准问题，故未单方面抬高他人测试批次的标准。
2. 严判据扫描本批 1313 行新测试（63 个测试函数，排除辅助函数）：仅上述 1 个用例完全没有断言，
   其余均有 `assert`/`panic!`/`unwrap_err` 等真实断言。**注意**：这类扫描必须从 `fn` 行本身起算
   大括号配平 —— 否则会在结构体字面量的收尾 `};` 处误判为函数结束，从而把"先构造结构体、后断言"
   的用例全部错标为"无断言"（本报告作者第一次扫描即踩此坑，得到 23 个假阳性后修正）。
