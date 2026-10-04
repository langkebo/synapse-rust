# CI 测试范围缺口 + workspace lib 测试隔离现状（根因与修复方案）

> **日期**: 2026-09-11
> **基线提交**: `4f043166`
> **范围**: P0「测试基线」/ P5「CI blocking 有效性」

---

## 0. 结论摘要

核查发现两个互相纠缠的问题，**都不修就无法扩大 CI 覆盖范围**：

| # | 问题 | 性质 |
|---|---|---|
| 1 | **CI 从不运行 workspace crate 的 lib 测试** | 范围缺口（高） |
| 2 | workspace lib 测试的 DB 隔离**两极分化**：要么不安全、要么极慢 | 阻塞 #1 的修复 |

实测数据：

```
CI 步骤 --lib --all-features            :   687 个测试   （仅根包）
CI 步骤 --test unit --features test-utils: 1839 个测试   （仅根包）
--lib --all-features --workspace        :  6118 个测试   （7 个 binary）
```

即 CI 覆盖量约为应有量的 **1/9**，缺口恰好落在本次审查改动最密集的三层：
`synapse-common`（配置/错误/限流叶子类型，单独 862 个 lib 测试）、
`synapse-storage`（持久化）、`synapse-services`（业务逻辑）。

**对本次审查的直接影响**：我新增的两个"防静默降级"守卫
（`degradation_tests` 6 个、`strictness_tests` 6 个）位于 `synapse-common`，
因此**不在 CI 执行范围内** —— 守卫自己没有生效。

---

## 1. 问题 1：CI 作用域缺口

所有 workflow 的 nextest 调用均未传 `-p <crate>` / `--workspace`：

```console
$ grep -n "nextest run" .github/workflows/*.yml | grep -E "\-p |--workspace" || echo "(none)"
(none)
```

Cargo 在未指定作用域时默认作用于**当前包**（根 crate `synapse-rust`），
于是 6 个 workspace 成员的 lib 测试从未在 CI 中编译或执行。

### 已落地的防复发守卫

`tests/unit/ci_test_scope_tests.rs`（3 项）：

* `nextest_invocations_declare_their_scope` —— 遍历所有 workflow，
  凡未指定 `--test <target>` 的 nextest 调用必须声明 `--workspace` 或 `-p`；
* `lib_test_step_covers_the_workspace` —— `--lib` 步骤必须带 `--workspace`；
* 守卫已 RED 验证：当前会失败并精确指出
  `ci.yml:285 的 --lib 步骤缺少 --workspace`。

**注意**：该守卫在 CI 修复前会**持续失败**。这是刻意的 —— 它是对
"作用域缺口"的编码化提醒；但必须先解决问题 2 才能安全地修 CI，
否则 CI 会直接变红。

---

## 2. 问题 2：DB 隔离两极分化

### 2.1 现状

`synapse-storage` 的 `db_tests` 模块存在两种截然不同的隔离方式：

| 方式 | 模块数 | 隔离性 | 单测耗时 |
|---|---|---|---|
| `IsolatedTestPool`（每测试新建 schema） | 8 | ✅ 完全隔离 | **40–134 秒** |
| 直连 `TEST_DATABASE_URL` + `unique_suffix` | **约 30** | ❌ 共享库，互相干扰 | 毫秒级 |

`IsolatedTestPool` 的 8 个模块：
`captcha` / `dehydrated_device` / `federation_blacklist` / `invite_blocklist` /
`media_quota` / `openid_token` / `user` / `worker`。

其余约 30 个（`account_data`、`audit`、`beacon`、`call_session`、
`cas`、`device`、`event`、`filter`、`membership`、`moderation` …）
使用 `account_data/mod.rs:104-114` 那种裸 `test_pool()`。

### 2.2 两个方向都不可接受

**裸 test_pool（多数模块）**：共享同一个数据库，仅靠 `unique_suffix()` 区分数据。
实测在 `--workspace` 全量并发下，`account_data::db_tests` 的 6 个用例失败：

```console
FAIL synapse-storage account_data::db_tests::test_list_multiple
FAIL synapse-storage account_data::db_tests::test_list_empty
FAIL synapse-storage account_data::db_tests::test_get_not_found
FAIL synapse-storage account_data::db_tests::test_delete_existing
FAIL synapse-storage account_data::db_tests::test_delete_not_found
FAIL synapse-storage account_data::db_tests::test_multiple_users_isolation
```

已定位为**并发干扰而非缺陷**：
本地 schema 与 upsert SQL 匹配、手工执行同一条 upsert 成功、
这 17 个用例在 `--test-threads 1` 与 `8` 下**均 17/17 通过**，
只在与其他 workspace lib 测试并发时失败。

**`IsolatedTestPool`（8 个模块）**：隔离正确，但**每个测试都要重新应用
整个 v11 基线迁移**（`test_isolation.rs:220-250`：逐条 `sqlx::query` 执行
`00000000_unified_schema_v11.sql` + `00000001_extensions_v10.sql`）。
实测代价：

| 测量 | 结果 |
|---|---|
| `synapse-storage --lib` 单独运行 | 604/1751，约 12.5 分钟 |
| 单测耗时样本 | 134.5s / 102.0s / 88.3s / 62.0s / 42.0s / 40.4s / 31.3s |
| 外推 1751 个 | **约 35 分钟**（仅 storage 一个 crate） |

### 2.3 根因

`IsolatedTestPool` 选择了"**每测试重建 schema**"而非"**克隆模板 schema**"。
根 crate 的集成测试用了后者的优化版（`src/test_utils.rs::clone_schema_from_template`，
COPY 语句批量插入 + 单条 `DO $$` 块），而 storage 这份实现是**逐语句往返**
—— 每个建表/索引语句一次网络往返。

因此：**修好隔离性的前提是先让隔离变便宜**，否则把 30 个模块都迁到
`IsolatedTestPool` 只会让 CI 从"跑错范围"变成"永远跑不完"。

---

## 3. 修复方案（本轮未落地，理由见 §4）

正确的顺序是三件事，**必须一起做**：

1. **让 `IsolatedTestPool` 克隆模板 schema**，而不是每测试重放 baseline。
   参照 `src/test_utils.rs::clone_schema_from_template`：先用模板构建一次
   （或复用已有的 `public`），之后 `CREATE TABLE ... LIKE` 批量克隆 +
   单条 `DO $$` 一次性完成。预期把单测从 ~100s 降到个位数秒。
2. **把约 30 个裸 `test_pool()` 模块迁移到 `IsolatedTestPool`**（机械改动，
   逐个可验证）。
3. **给 CI 的 `--lib` 步骤加 `--workspace`** —— 此时才安全，
   且 `ci_test_scope_tests` 守卫会随之转绿。

另需注意 `IsolatedTestPool` 默认 URL 指向 `localhost:15432/synapse_test`，
而本地实际实例在 `5432` —— 该默认值需一并核对（依赖 `TEST_DATABASE_URL`
注入，CI 有设，但默认值本身已过时）。

---

## 4. 为什么本轮只做第 3 步的守卫、不落地修复

* 第 1 步（改造 `IsolatedTestPool`）是**共享测试基础设施的性能改造**，
  改动会让全部 ~30 个迁移后的模块语义依赖新的克隆逻辑；
* 第 2 步是**约 30 个文件的机械迁移**，每步都需要 DB 验证，
  而当前单个模块验证就要跑数分钟；
* 本轮实测已表明：**storage 一个 crate 的 lib 套件就要约 35 分钟**，
  先把 30 个模块迁到更慢的路径上会让 CI 实际失去可用性；
* 三项合起来是一次独立的、需要完整验证窗口的工作，
  不应顺手塞进审查轮次。

**本轮交付**：根因定位 + 三个问题的量化数据 + 防复发守卫（第 3 步的一部分）。

---

## 5. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
export DATABASE_URL='postgresql://synapse:<pw>@localhost:5432/synapse'
export TEST_DATABASE_URL="$DATABASE_URL"

# 1) 作用域缺口
grep -n "nextest run" .github/workflows/*.yml | grep -E "\-p |--workspace" || echo "(none = 缺口)"
cargo nextest run --lib --all-features --locked --test-threads 8                       # 687（CI 实际）
cargo nextest run --lib --all-features --locked --workspace --test-threads 8           # 6118（应有）

# 2) 干扰 vs 隔离
cargo nextest run -p synapse-storage --lib -E 'test(/account_data::db_tests/)' --test-threads 1  # 17/17
cargo nextest run -p synapse-storage --lib -E 'test(/account_data::db_tests/)' --test-threads 8  # 17/17
cargo nextest run --lib --all-features --locked --workspace --test-threads 8                     # 6 个失败

# 3) 隔离实现代价
sed -n '200,255p' synapse-storage/src/test_isolation.rs   # 逐语句重放 baseline
cargo nextest run -p synapse-storage --lib --all-features --test-threads 8   # ~35 分钟

# 4) 守卫
cargo nextest run --profile test --features test-utils --test unit -E 'test(/ci_test_scope_tests/)'
```

---

## 6. 仍待办

| # | 项 | 优先级 |
|---|---|---|
| 1 | 改造 `IsolatedTestPool` 为模板克隆（性能前置） | **高** |
| 2 | 迁移约 30 个裸 `test_pool()` 模块 | **高** |
| 3 | CI `--lib` 加 `--workspace`（依赖 1+2；守卫已在等） | **高** |
| 4 | 核对 `IsolatedTestPool` 的 `15432` 默认 URL 是否过时 | 中 |
| 5 | 全量集成套件完整跑一次（~3.3h） | 中 |
| 6 | 真实 CI 首跑确认（仓库 private、`gh` token 失效 → 本环境不可达） | **高** |
