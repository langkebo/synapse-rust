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
