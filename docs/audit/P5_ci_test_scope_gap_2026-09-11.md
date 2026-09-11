# CI 测试范围缺口：workspace crate 的 lib 测试从未执行

> **日期**: 2026-09-11
> **基线提交**: `ec516ed9`
> **范围**: `.github/workflows/*.yml` 的测试命令范围（P0「测试基线」/ P5「CI blocking 有效性」）

---

## 0. 结论

**CI 从不运行 workspace crate 的 lib 测试。**

所有 workflow 里的 nextest 调用（`cargo nextest run ...`）**没有任何一个**传递
`-p <crate>` 或 `--workspace`，因此它们只作用于**根包 `synapse-rust`**。

```console
$ grep -n "nextest run" .github/workflows/ci.yml
285:  cargo nextest run --lib --all-features --locked --test-threads 8
296:  cargo nextest run --test unit --features test-utils --locked --test-threads 8
512:  cargo nextest run --test integration 'admin_registration_service_tests_migrated' ...
522:  cargo nextest run --test integration --all-features --locked --test-threads 6
538:  cargo nextest run --test e2e --all-features --locked --test-threads 4

$ grep -n "nextest run" .github/workflows/ci.yml | grep -E "\-p |--workspace"
(NEVER — 无任何 nextest 调用带 -p/--workspace)
```

### 实测差距

| 命令 | 测试数 | 覆盖 |
|---|---|---|
| CI 步骤 1：`--lib --all-features` | **687** | 仅根包 lib |
| CI 步骤 2：`--test unit --features test-utils` | **1839** | 仅根包 unit target |
| `--lib --all-features --workspace` | **6118** | 根包 + 6 个 workspace crate 的 lib |

即 workspace 的 6 个 crate
（`synapse-common` / `synapse-cache` / `synapse-storage` / `synapse-e2ee` /
`synapse-federation` / `synapse-services`）的 lib 测试**全部不在 CI 覆盖内**。

据 `--test unit` 的输出可交叉验证：该命令 1839 个用例中**没有任何**
`synapse-common::` 前缀的用例（`grep -oE "synapse-common::[a-z_:]+"` 无命中）。

---

## 1. 影响

### 1.1 数字规模

* `synapse-common` 单独就有 **862** 个 lib 测试；
* `--workspace --lib` 合计 **6118** 个 —— 是 CI 实际执行量的约 **9 倍**。

### 1.2 本次审查自身受到影响（重要）

我在前几轮新增的守卫测试**全部位于 workspace crate**，因此**不在 CI 执行范围内**，
只能手动运行：

| 守卫 | 位置 | 是否被 CI 执行 |
|---|---|---|
| `degradation_tests`（限流降级可观测性，6） | `synapse-common` | ❌ |
| `strictness_tests`（配置拼写错误拒绝，6） | `synapse-common` | ❌ |
| `via_servers_tests`（MSC4156 via，13） | 根包 `src/` | ✅ |
| `pagination_gate_tests` / `sqlx_ratio_gate_tests` 等 | `tests/unit/` | ✅ |

即：**两个针对"静默降级"的守卫自己也是静默的** —— 除非 CI 范围修正，
它们不会在 CI 中生效。这一点必须明确记录，避免我把"已有守卫"说得比实际更强。

---

## 2. 为什么没有直接加上 `--workspace`

因为加上之后 `--workspace --lib` 出现 **6 个失败**，且我不能在未定位前
就把它推成阻塞步骤（那会把 CI 直接变红）。

### 2.1 失败内容

```console
$ cargo nextest run --lib --all-features --locked --workspace --test-threads 8
    Starting 6118 tests across 7 binaries
    Summary [153.871s] 4373/6118 tests run: 4367 passed, 6 failed, 0 skipped

FAIL synapse-storage account_data::db_tests::test_list_multiple
FAIL synapse-storage account_data::db_tests::test_list_empty
FAIL synapse-storage account_data::db_tests::test_get_not_found
FAIL synapse-storage account_data::db_tests::test_delete_existing
FAIL synapse-storage account_data::db_tests::test_delete_not_found
FAIL synapse-storage account_data::db_tests::test_multiple_users_isolation
```

报错为包装后的通用信息（`ApiError::internal_with_context` 不带 cause），
例如 `Internal error: Failed to upsert account data`。

### 2.2 定位过程与结论：**是并发干扰，不是缺陷**

| 实验 | 结果 |
|---|---|
| 本地 schema 核对：`account_data` 列与 upsert SQL 完全匹配 | ✅ 匹配 |
| 手工执行同一条 upsert SQL | ✅ `INSERT 0 1` |
| 17 个 `account_data::db_tests`，`--test-threads 1` | ✅ **17/17 通过** |
| 17 个同上，`--test-threads 8` | ✅ **17/17 通过** |
| 与其余 workspace lib 测试一起跑（6118 个，8 线程） | ❌ 6 个失败 |

**结论**：这 6 个用例**单独运行稳定通过**，只在与其他 workspace lib 测试并发时失败。
原因是这批测试直接连 `TEST_DATABASE_URL`（`test_pool()` 见
`synapse-storage/src/account_data/mod.rs:104-114`），依赖 `unique_suffix()` +
`clean_account_data()` 做隔离，**不使用根包集成测试那套"每用例克隆独立 schema"**
的机制，因此在高并发下与其它会写库的测试互相干扰。

这与 CLAUDE.md 记录的「集成测试并发敏感」是同一类问题，
只是发生在 **workspace crate 的 lib 测试**里 —— 因为没有 `-p`/`--workspace`，
此前从未暴露。

### 2.3 因此正确的修复是两件事，而非一行改动

1. 给 CI 的 lib 步骤加 `--workspace`（否则 6000+ 用例永远不跑）；
2. **同时**处理 `synapse-storage` 的库级 DB 测试并发隔离问题 ——
   要么让它们走隔离 schema，要么把这批测试串行化
   （参考 CI 对 `admin_registration_service_tests_migrated` 的既有
   `--test-threads 1` 单独步骤）。

只做 (1) 会让 CI 变红；只做 (2) 不解决问题（它们本来就没在跑）。
这是一项需要单独验证的工作，本轮**只定位、不落地**。

---

## 3. 为什么值得优先修

* **9 倍的测试量差异**，且缺口恰好落在被重构最多的层：
  `synapse-common`（配置/错误/限流叶子类型）、`synapse-storage`（持久化）、
  `synapse-services`（业务逻辑）—— 正是本次审查改动最密集的地方；
* 本次审查的两个"防静默降级"守卫因此形同未接线；
* 与已修复的四个"文书门禁不生效"同源：**范围错了，门禁就没生效**。

---

## 4. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
export DATABASE_URL='postgresql://synapse:<pw>@localhost:5432/synapse'
export TEST_DATABASE_URL="$DATABASE_URL"

# CI 实际执行的范围（687 + 1839，均仅根包）
cargo nextest run --lib --all-features --locked --test-threads 8
cargo nextest run --test unit --features test-utils --locked --test-threads 8

# 缺口：workspace crate 的 lib（6118 个，其中 6 个在高并发下失败）
cargo nextest run --lib --all-features --locked --workspace --test-threads 8

# 证明那 6 个是并发干扰而非缺陷
cargo nextest run -p synapse-storage --lib -E 'test(/account_data::db_tests/)' --test-threads 1   # 17/17
cargo nextest run -p synapse-storage --lib -E 'test(/account_data::db_tests/)' --test-threads 8   # 17/17

# 确认 CI 从未传 -p/--workspace
grep -n "nextest run" .github/workflows/*.yml | grep -E "\-p |--workspace" || echo "(none)"
```

---

## 5. 仍待办

| # | 项 | 优先级 |
|---|---|---|
| 1 | 给 CI lib 步骤加 `--workspace`，**并**先解决 `synapse-storage` 库级 DB 测试并发隔离 | **高** |
| 2 | 重新评估：加 `--workspace` 后是否还有其它未被发现的失败（本轮只跑到 4373/6118 就被 6 个失败中止） | **高** |
| 3 | 全量集成套件完整跑一次（~3.3h） | 中 |
| 4 | 真实 CI 首跑确认（仓库 private、`gh` token 失效 → 本环境不可达） | **高** |
| 5 | presence stream 游标 / 联邦 knock / `get_raw` 改名 | S 系列 P2/P3 |

---

## 6. 2026-09-12 补充：阻塞项的**确切范围**已定位

第 1 项（"解决 synapse-storage 库级 DB 测试并发隔离"）此前只定位到"6 个用例在
高并发下失败"。进一步清点后，阻塞项的边界是明确的：

**`synapse-storage` 里有 57 个模块手写自己的 `test_pool()`**（

```
$ grep -rln "async fn test_pool" --include='*.rs' synapse-storage/src/ | wc -l
57
```

），它们全部直接连 `TEST_DATABASE_URL` 并使用 `public` schema，靠
`unique_suffix()` + 手工 `clean_*()` 做数据隔离，**不使用**根包集成测试那套
"每用例一个隔离 schema"的机制（`prepare_isolated_test_pool` / 共享模板克隆）。

这解释了三件事，且指向同一个根因：

1. **为什么 `--workspace` 会红**：57 个套件在 `public` 上并发写，彼此干扰；
2. **为什么 `public` 会残留跨 schema 外键**（见
   `P5_migration_search_path_shadowing_2026-09-12.md`）：这些套件从不建 schema，
   也就从不清理 schema，`public` 里的表是它们唯一的落脚点，长期被多轮迁移反复
   `ALTER`；
3. **为什么 schema 会累积到 23,662 个**（见
   `P5_test_schema_accumulation_2026-09-12.md`）：建 schema 的那几条路径与不建
   schema 的这 57 个套件是两套并存的隔离哲学。

**因此第 1 项的正确形态不是"给那 6 个用例加 `--test-threads 1`"，而是让这 57 个
模块改走共享的隔离 schema 夹具。** 这也正是
`P5_workspace_test_isolation_2026-09-11.md`、`P5_migration_search_path_shadowing`
与 `P5_test_schema_accumulation` 三份报告共同指向的同一项结构性改造。

> ⚠️ 本项的落地**无法在当前环境验证**：本地 PostgreSQL 自 2026-09-12 22:01 起处于
> 崩溃恢复（成因即 schema 累积导致的数百万文件 fsync，见
> `P5_test_schema_accumulation_2026-09-12.md` §8），集群不可连接。在能跑
> `--workspace --lib` 之前，不应改动 CI 范围——只做 (1) 会让 CI 变红。
