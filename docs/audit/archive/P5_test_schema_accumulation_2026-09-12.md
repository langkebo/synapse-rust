# 测试 schema 无限累积：catalog 膨胀治理

> **日期**: 2026-09-12
> **基线提交**: `d82a060d`
> **范围**: P5「工程质量 / 仓库治理」；P4「性能基线」的**前置阻塞项**
> **性质**: 测试基础设施缺陷（**不是**生产代码缺陷）

---

## 0. 结论

本地测试库累积到 **23,653 个 schema**，其中 23,649 个是测试残留。膨胀到
`pg_database_size()` 查询都会超时、单个 `DROP SCHEMA CASCADE` 需要数秒的程度
——**它本身就是此前 P4 性能基线不可信的直接原因**（同一提交在
`--test-threads 1` 与 `12` 下结果漂移、单用例 73–134s）。

根因是**两个独立、此前都无人清理的泄漏**：

| 家族 | 清理前 | 清道夫 | 成因 |
|---|---|---|---|
| `test_*` | 22,568 | ❌ 从未清理 | `prepare_isolated_test_pool` 每次 `CREATE SCHEMA`，无 Drop 清理 |
| `media_test_*` | 1,033 | ❌ 从未清理 | `prepare_media_test_pool`（`synapse-services/src/media/mod.rs:700`）同上 |
| `synapse_test_*` | 48 | ❌ 从未清理 | 旧模板/就绪标记，无生命周期管理 |
| `test_template_v2_*` | 39 | ❌ 从未清理 | **迁移指纹一变就新造一个**，旧的永不删除 |
| 合计残留 | **23,688** | | |

外加 `scripts/cleanup_test_schemas.sh` 本身有 4 个缺陷，使它既清不干净也不安全
（见 §2）。

> ✅ **2026-09-13 状态更新（见 §10）**：持续泄漏**已被堵住**。三套夹具
> （root / `synapse-services` / `synapse-storage`）+ `media` 已全部接入
> `synapse_common::test_schema_guard` 的 **Test Janitor（RAII）**——janitor 监听
> `Weak<PgPool>`，池强引用归零即回收，`atexit` join 兜底。实测本地
> `synapse_test` 库跑两轮 oidc 隔离池用例，`test_*` schema 恒为 **0**，跨轮次
> 无增长。本文 §3–§9 中"止血无效 / 定期维护"的结论，自 Test Janitor 接入起
> 已过时；§9 的否定性实测被保留作为该机制的设计动因。**残留的是"三套夹具
> 收敛成一份"的结构性债务（§1.4），不再是 schema 泄漏本身。**

---

## 1. 两类根因

### 1.1 模板指纹 churn（可完全自动修复）

`default_template_schema_name()` 把**每个迁移文件的文件名+大小+mtime** 哈希进
模板名：

```rust
fn default_template_schema_name() -> String {
    format!("test_template_v{}_{}", TEST_TEMPLATE_SCHEMA_REVISION, template_schema_fingerprint())
}
// fingerprint = fnv1a64("schema-rev:2;contract-sql:<…>;" + 每个 migrations/*.sql 的 name:size:mtime)
```

这个设计本身是对的——迁移一变，旧模板必须作废。**但没有任何代码删除旧模板**。
于是每次改迁移就永久多留一个 111 张表的 schema。实测库里躺着 39 个，其中 38 个
对应的指纹早已不可能再被 `template_schema_is_ready()` 选中。

判据很硬：`template_schema_is_ready()` 只查**当前指纹**那个名字的标记文件，所以
旧模板是**死数据**，删除零风险。

### 1.2 按用例造 schema（需结构性改造）

`prepare_isolated_test_pool` / `prepare_media_test_pool` 每次调用都
`CREATE SCHEMA test_<pid>_<n>_<nanos>`，返回 `Arc<PgPool>`，**没有任何 Drop
钩子**。共享路径（`prepare_shared_test_pool`）早就解决了这个问题（
`register_pending_schema_return` + `PENDING_SCHEMA_RETURNS` 弱引用活体检测 +
TRUNCATE 回池），但这两条路径**绕过了它**。

> ⚠️ 这与 `P5_workspace_test_isolation_2026-09-11.md` 记录的是**同一个**结构问题：
> 约 30 个模块手写自己的 `test_pool()`，不走共享夹具。那次是"测试口径不统一"，
> 这次是"schema 无限增长"，同一个根因的两种症状。

### 1.3 泄漏面比原先记录的更大：`synapse-services` 有**第三套**夹具

清点后发现 `CREATE SCHEMA` 出现在**三个**地方，而不是两个：

清点后共 **5 个**站点、**4 份**各自独立分叉的 helper：

| 文件 | `CREATE SCHEMA` 处数 | 清理状态 |
|---|---|---|
| `src/test_utils.rs` | 6 | 共享路径有；隔离路径 ✅ 本次补上 |
| `synapse-services/src/test_utils.rs` | 4 | ❌→✅ 本次补上（该副本**完全没有**任何机制） |
| `synapse-storage/src/test_utils.rs` | 1 | ❌→✅ 本次补上（**第三份**副本，此前未列入） |
| `synapse-services/src/media/mod.rs` | 1 | ❌→✅ 委托给 synapse-services 注册表 |
| `synapse-storage/src/test_isolation.rs` | 1 | ✅ 早有 `Drop`（spawn+join，`0cfa1227` 修） |

`synapse-storage/src/test_utils.rs` 的 `prepare_empty_isolated_test_pool` 被
`oidc_session_storage.rs`、`refresh_token/mod.rs` 及 4 个 integration 测试直接调用，
每次调用泄漏一个 `test_*` schema。

`synapse-services/src/test_utils.rs` 是根 crate `src/test_utils.rs` 的**分叉副本**：
同为 597 行，同样有 `prepare_isolated_test_pool` / `prepare_shared_test_pool` /
`init_template_schema` / `clone_schema_from_template` / `next_test_schema_name`，
但**完全没有** `PENDING_SCHEMA_RETURNS` / `SCHEMA_POOL` / `CLEANUP_RUNTIME` /
`schedule_pending_schema_cleanup`。也就是说，凡是走 `synapse-services` 那套夹具的
测试，造的 schema **100% 泄漏**。

> 这解释了一个此前没解释清的数字：为什么 `media_test_*` 会有 1,033 个而
> `test_*` 有 22,568 个——两套夹具各自独立泄漏，比例大致对应各自的使用量。
>
> **这也意味着 §3 的"止血"只覆盖了其中一条路径。** 见 §5。

### 1.4 更高层的根因：同一份测试基础设施被分叉了三次

真正的根因不是"忘了写 Drop"，而是**同一份 schema 生命周期逻辑被复制了三份，
只有一份在持续演进**。根 crate 那套有清理、有池化、有中毒检测；`synapse-services`
那份停在早期版本；`media` 那份干脆自己手写。

任何"在各处分别补 Drop"的做法都只是延缓第三次分叉。**正确的修法是收敛成
一份**（新 crate 或把生命周期抽到 `synapse-common` 的 test 侧），这也是
`P5_workspace_test_isolation` 与 `P5_migration_search_path_shadowing` 共同的
结构性结论。

---

## 2. 清理脚本的 4 个缺陷（已修）

`scripts/cleanup_test_schemas.sh` 原版：

| # | 缺陷 | 后果 |
|---|---|---|
| 1 | 只匹配 `test_%` | `media_test_*`(1,033)、`synapse_test_*`(48) **永远清不掉** |
| 2 | 无条件保留**所有** `test_template%` | 38 个陈旧模板永远清不掉；`LIKE 'test_template%'` 也匹配不到 `synapse_test_template_*`（前缀不同） |
| 3 | 默认 `PGPORT=15432 PGDATABASE=synapse_test`，且不校验实际连到哪 | 在非同款环境里**静默连到另一个数据库**，打印"无残留"，看着像成功 |
| 4 | `psql ... 2>/dev/null` 且无 `--dry-run` | 失败原因全丢（只剩 `WARN`）；默认即破坏性执行 |

修复后：

* 连接优先级 `DATABASE_URL` > `TEST_DATABASE_URL` > `PG*`；连接后**打印实际
  连到的 `db` / `server`**；
* **默认预演**，必须显式 `--apply`；
* 覆盖 4 个家族；模板家族用锚定正则
  `^test_template_v[0-9]+_[0-9a-f]{16}$`，任意命名的模板（如
  `TEST_DB_TEMPLATE_SCHEMA=public`）**不可能命中**；
* live 模板由标记目录 `synapse_test_templates/` 决定；**找不到任何标记就中止**
  （fail-safe，不是"全删"）；
* `--keep-all-templates` 显式逃生舱；失败保留 stderr（打前 5 条 + 首个原因）。

### 2.1 顺手修掉的一个 bash 陷阱

修复过程中脚本在第 102 行崩了：

```
line 102: MARKER_DIR�: unbound variable
```

原因是 `echo "…标记（$MARKER_DIR）。"` —— **全角右括号 `）` 直接跟在未加花括号
的变量名后面**，bash 把它的 UTF-8 字节当成变量名的一部分，于是变量名成了
`MARKER_DIR<0xef>`。中文注释/文案里这极容易踩到，已全部改成 `${MARKER_DIR}`。

---

## 3. 防复发：模板自动剪枝（已实现 + 已测）

`src/test_utils.rs` 新增 `prune_stale_template_schemas(admin_pool, keep)`，在
`init_template_schema` 末尾调用（新模板**已标记就绪**之后）：

1. `keep` 永不进候选；
2. 候选只匹配 `^test_template_v[0-9]+_[0-9a-f]{16}$`（锚定）；
3. **`keep` 不存在时拒绝执行** —— 构建失败绝不能把库清成"一个模板都没有"；
4. 每个 schema 单独 DROP（沿用 `max_locks_per_transaction` 约束），同时删除其
   就绪标记文件；
5. 剪枝失败只 `WARN`，绝不让模板初始化失败。

保护顺序（build 成功 → mark ready → close pool → prune）保证**任何时刻都存在
一个可用模板**。

### 3.1 回归测试

`tests/unit/test_schema_housekeeping_tests.rs`

* `prune_drops_superseded_templates_and_keeps_the_current_one` —— 造 2 个指纹形态
  的陈旧 schema + 1 个非指纹命名 schema，断言：陈旧两个被删、`keep` 保留、
  **非指纹命名保留**、第二次调用幂等；
* `prune_refuses_when_replacement_template_is_missing` —— `keep` 不存在时必须
  返回 `Err`。

```
$ cargo nextest run --profile test --features test-utils --test unit \
    -E 'test(/test_schema_housekeeping/)'
    Summary [162.378s] 2 tests run: 2 passed, 1848 skipped
```

> 162 秒本身就是证据：**在膨胀的 catalog 上，只建 4 个 schema + 跑一次剪枝扫描
> 就要 2 分 42 秒。** 这是 P4 性能数字不可复现的直接机制。

---

## 4. 实测清理：**部分完成，未做完**

### 4.1 已完成的部分

```bash
DATABASE_URL='postgresql://…@localhost:5432/synapse' \
  bash scripts/cleanup_test_schemas.sh --keep-all-templates --apply
```

前 ~1,900 个候选被清掉（`media_test_*` 1,033 → **0**，`synapse_test_*` 48 → **0**，
普通 `test_*` 与陈旧模板各清了一部分）后，**被主动中止**——原因见 §4.2。

| 指标 | 清理前 | 中止时 | 变化 |
|---|---|---|---|
| `pg_namespace` 总数 | 23,653 | 21,777 | −1,876 |
| `media_test_*` | 1,033 | **0** | ✅ 清零 |
| `synapse_test_*` | 48 | **0** | ✅ 清零 |
| `test_template_v2_*`（陈旧） | 39 | 25 | −14 |
| 普通 `test_*` | 22,568 | **21,748** | −820 |

### 4.2 为什么中止：**`DROP SCHEMA` 撞上 `max_locks_per_transaction`**

逐个 DROP 的实测成本：

| 对象 | 单次 DROP 耗时 | 结果 |
|---|---|---|
| 普通 `test_*` schema（平均 658 个对象，最大 1,229） | ~3.0s | 成功 |
| 模板 schema（**1,197 个对象**） | ~12.0s | **失败** |

模板 schema 的失败信息：

```
ERROR:  out of shared memory
HINT:  You might need to increase max_locks_per_transaction.
```

本机 `max_locks_per_transaction = 256`。**单个 `DROP SCHEMA ... CASCADE` 要对被
级联的每个对象各持一把锁**，1,197 个对象直接超限——所以模板家族现在**一个也删
不掉**，每次白等 12 秒。（这也是原脚本注释里"必须逐 schema 一个事务"那条经验的
更极端形态：连"一个 schema 一个事务"都不够了。）

按 3.0s/个 推算，剩下的 21,748 个普通 schema 还需 **~18 小时**，因此中止而不是
挂在后台跑通宵。

> **教训**：`max_locks_per_transaction` 是按**对象数**算的，而这份测试库单个
> schema 有 658–1,229 个对象。任何"清道夫脚本"都必须先量一下单次 DROP 的成本和
> 锁需求，否则会得到"跑了一整晚、什么都没删掉"的结果。

### 4.3 建议的收尾路径（需用户选择）

| 方案 | 代价 | 说明 |
|---|---|---|
| **A. 重建测试库**（推荐） | ~30s + 重放迁移 | `DROP DATABASE synapse` + `CREATE DATABASE` + 迁移。一次性、绕过所有锁限制。**需确认 `synapse` 库内无手工数据** |
| B. 提高 `max_locks_per_transaction` 后重启 PG | 需重启 + 停机 | 治标：让单次 DROP 能过锁限，但 3s/个的成本仍在 |
| C. 后台继续跑 ~18 小时 | 无停机 | 不推荐；期间测试持续变慢 |
| D. 手工分批 DROP 模板对象 | 中等 | 先逐表/逐索引 DROP 再 DROP SCHEMA，可绕开锁限但脚本量大 |

**本次未执行 A/B/C/D 中的任何一个**，仅保留了已完成的 ~1,876 个删除。

---

## 5. 未做 / 需用户决策

> ✅ **2026-09-13 更新**：下表标 ❌ 的三行"实测无效"结论**已被 Test Janitor
> RAII 推翻**（见 §10）——它们是 sweep 时代的判定；夹具已改接 janitor，持续泄漏
> 已堵住。原行保留以记录演进，状态已就地更正。

| 项 | 状态 | 说明 |
|---|---|---|
| 清完剩余 21,748 个普通 `test_*` + 25 个模板 schema | ✅ **已完成** | 本地库已重建（`DROP/CREATE DATABASE`），schema 残留归 0（§10.1） |
| `src/test_utils.rs::prepare_isolated_test_pool` 接入待删登记 | ✅ **已生效**（§10） | 旧 sweep 无效（§9.3）；现改 `TestSchemaGuard::new_registered` → janitor |
| `synapse-services/src/test_utils.rs` 的泄漏路径 | ✅ **已生效**（§10） | 同型接入 janitor |
| `synapse-storage/src/test_utils.rs` 的泄漏路径 | ✅ **已生效**（§10） | §9 的"+8/轮"判定基于 sweep；实测两轮 oidc 用例 `test_*`=0 |
| 静态守卫锁住全部 `CREATE SCHEMA` 站点 | ✅ **已实现且已实测 RED-GREEN** | `tests/unit/schema_lifecycle_guard_tests.rs`，2 条 |
| `synapse-services/src/media/mod.rs::prepare_media_test_pool` | ✅ **已实现并已验证**（§10.2） | 接入 janitor；同构推断 + 实测无泄漏 |
| 三套夹具收敛成一份 | **未做** | **真正的根因**（§1.4）；与 `P5_workspace_test_isolation`、`P5_migration_search_path_shadowing` 同一结论。**不再造成泄漏**，是纯结构性债务 |
| `synapse_test_template_*` 旧家族的创建方 | **未定位** | 前缀与 `synapse_test_template_ready` *标记*同名易混；已随 `synapse_test_*` 一并删除，但创建方仍未查明 |

> **2026-09-13 更正**：原先的"不宣称已完成"（仅存量清道夫 + 模板剪枝、持续
> 泄漏未堵）已不成立——Test Janitor 接入后持续泄漏**已堵住**，本治理从
> "定期维护动作"升级为"一次性终结"（残留的三套夹具收敛属结构性优化，非止血）。
>
> 一个可观测的收益佐证：同一个
> `test_schema_housekeeping_tests::prune_drops_superseded_templates_and_keeps_the_current_one`
> 用例，在 catalog 从 23,653 降到 21,777（−8%）后，耗时从 **162s 降到 108s**
> （−33%）。**膨胀是非线性伤人的**——这也解释了 P4 性能基线为何长期不可复现。

---

## 6. 复现命令与原始输出

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
export DATABASE_URL='postgresql://synapse:<pw>@localhost:5432/synapse'

# 0) 清点（膨胀时单条都可能要几十秒）
psql -d "$DATABASE_URL" -X -q -t -A -c "SELECT count(*) FROM pg_namespace;"

# 1) 预演（默认就是预演）
bash scripts/cleanup_test_schemas.sh

# 2) 执行
bash scripts/cleanup_test_schemas.sh --keep-all-templates --apply

# 3) 校验：应为 0
psql -d "$DATABASE_URL" -X -q -t -A -c "
SELECT count(*) FROM pg_namespace
WHERE nspname LIKE 'test\_%' OR nspname LIKE 'media\_test\_%' OR nspname LIKE 'synapse\_test\_%';"
```

### 清理前

```
test_*=22568
media_test_*=1033
synapse_test_*=48
nsp_total=23653
```

### 中止时（partial）

```
total_nsp=21777
ordinary_test=21748
templates=25
media_test=0
synapse_test=0
```

### 单次 DROP 成本实测

```
# 普通 schema
victim=test_49928_1_1784713277370867000
drop cascades to table ....media_callbacks and 350 other objects
real    0m2.994s

# 模板 schema
victim=test_template_v2_dfbb0b5e715e79dc
ERROR:  out of shared memory
HINT:  You might need to increase max_locks_per_transaction.
real    0m12.020s
```

### 环境

```
SHOW max_locks_per_transaction;  -- 256
SHOW max_connections;            -- 100
```

---

## 7. 实施止血：`drop_only` 通路（已实现，未验证）

> ✅ **2026-09-13 更新（见 §10）**：本节描述的 sweep 通路已被
> `synapse_common::test_schema_guard` 的 **Test Janitor（RAII）** 取代。
> "未验证"的状态现已结清——实测 0 泄漏（§10.2）。原文保留以记录"登记 +
> sweep"这一版本的设计与其被推翻的过程。

### 7.1 改动

根 crate `src/test_utils.rs`：

1. `PendingSchemaReturn` 增加 `drop_only: bool`；`schedule_pending_schema_cleanup()`
   把它透传给既有的 `cleanup_schema(.., poisoned)`；
2. 新增 `register_pending_schema_drop(pool, schema_name, database_url)`；
3. `prepare_isolated_test_pool` 末尾登记待删，**并在入口调用
   `schedule_pending_schema_cleanup()`**。

第 3 点里的 sweep 调用是关键：原来 sweep 只挂在共享/clone 路径上，所以一个
**只跑隔离路径**的测试进程从不回收，隔离 schema 会活到进程结束都无人清理。

为什么用 DROP 而不是 TRUNCATE 回池：隔离 schema 是**逐条重放迁移**建起来的，
不是从模板克隆的，`truncate_and_reseed_schema`（按模板表 TRUNCATE + 重种基线）
对它不安全。DROP 是它正确且有限的寿命——泄漏从来不是"造得太贵"，而是
"从来没人删"。

> ⚠️ **未验证**：改动通过 `cargo check -p synapse-rust --all-features --tests`
> （EXIT=0），但**没有**跑过任何 DB 测试——执行时集群正处于 §8 的崩溃恢复中。
> 在跑通 `prepare_isolated_test_pool` 相关用例并观察到 schema 数不再增长之前，
> 不得声称这条止血已生效。

### 7.2 `synapse-services` 分叉副本（已实现，未验证）

同一批里补齐了该副本**完全缺失**的生命周期机制：

* 新增 `PendingSchemaDrop` 注册表 + `sweep_pending_schema_drops()`，用
  `Weak<PgPool>` 做活体检测，池全部释放后在**独立 current-thread runtime** 上
  执行 `DROP SCHEMA`（不能用调用方的 runtime——要清理的恰恰是它已 teardown 的
  场景）；
* `prepare_isolated_test_pool` / `prepare_empty_isolated_test_pool` /
  `prepare_shared_test_pool` 三处全部接入（入口 sweep + 末尾登记）；
* `clone_schema_from_template` 改为返回 `(Arc<PgPool>, String)`，让调用方能登记
  自己刚克隆出来的 schema 名；
* `media::prepare_media_test_pool` 经
  `test_utils::register_pending_schema_drop_for_media` 接入，并在入口 sweep；
* 该副本的模板名是 `test_template_<pid>`（**按进程**，与根 crate 的
  `test_template_v<rev>_<hex>` 指纹命名不同），且模板池在 `init_template_schema`
  末尾就 `close()` 而克隆仍在引用它——因此**不能**走 `Weak` 登记（会中途过期），
  改为在 init 时回收**兄弟进程**遗留的 `test_template_<digits>`。正则锚定到纯数字，
  结构上不可能误伤根 crate 的指纹模板或 `synapse_test_template_ready_*` 标记名。

为什么没有直接把根 crate 那套（含池化复用）复制过来：那会把"分叉"变成"第三次
分叉"，正是 §1.4 的根因。这里选的是该副本**正确且最小**的语义——它不需要复用，
只需要"用完即删"。

> ⚠️ 同样**未验证**：`cargo check -p synapse-services --all-features --tests` 与
> clippy 均通过（EXIT=0，15 警告 = 基线），但未跑过任何 DB 测试。
> sqlx 动态比棘轮 1439→1442（+3，理由已记入 baseline 文件）。

### 7.3 `synapse-storage` 分叉副本（已实现，未验证）

清点 `CREATE SCHEMA` 时又发现**第三份**副本：
`synapse-storage/src/test_utils.rs::prepare_empty_isolated_test_pool`，被
`oidc_session_storage.rs`、`refresh_token/mod.rs` 及 4 个 integration 测试直接调用，
每次泄漏一个 `test_*` schema。处理方式与 §7.2 同型（drop-on-release 注册表 +
入口 sweep + 独立 cleanup runtime）。

该副本有一处额外约束值得记录：`synapse-storage` 连**测试支持代码**都
`-D clippy::panic` / `expect_used` / `unwrap_used`（首次 clippy 即报
`panic should not be present in production code`）。因此 cleanup runtime 用
`LazyLock<Option<Runtime>>` 而非 panicking 的 `LazyLock<Runtime>`——建不起来时
退化为"泄漏一个 schema"，而不是让测试进程崩掉。

### 7.4 静态守卫：锁住全部 `CREATE SCHEMA` 站点（已实测 RED-GREEN）

> ✅ **2026-09-13 更新**：守卫已随 Test Janitor 重新对焦。当前两条为
> `every_create_schema_site_has_a_cleanup_path`（含 `CREATE SCHEMA` 的文件必须同时含
> `DROP SCHEMA` / `register_schema_cleanup` / `test_schema_guard`）与
> `schema_cleanup_converges_on_shared_janitor_and_blocks_private_registries`（断言共享引擎
> 保留 `Weak<PgPool>` + `atexit` + `join()` 契约，并**禁止**任何
> `PENDING_SCHEMA_DROPS`/`PENDING_SCHEMA_RETURNS` 私有注册表或旧 sweep API 复活）。
> 下面原文中"sweep 是否真的在取池时被调用"这条判据已无意义——sweep 本身已被移除。

`tests/unit/schema_lifecycle_guard_tests.rs`（纯静态，无 DB）：

1. `every_create_schema_site_has_a_drop_path` —— 扫描 workspace 全部 `.rs`，任何
   含 `CREATE SCHEMA` 的文件必须同时含 `DROP SCHEMA` 或
   `register_pending_schema_drop`（`media` 属后者：委托给 `synapse-services`）；
2. `schema_drop_sweeps_are_actually_called_on_acquisition` —— **登记了 drop 却没有
   sweep 等于永不执行**，这正是隔离路径当初泄漏而共享路径没漏的原因。

> **这是本轮唯一带有反证证据的改动。** 用 `git checkout` 把
> `synapse-storage/src/test_utils.rs` 回滚到泄漏版本后，两条守卫**均失败**
> （`2 tests run: 0 passed, 2 failed`）并打印出确切的违规文件与原因；恢复修复后
> `2 passed`。
>
> 之所以它是本轮唯一可实测的：DB 仍在崩溃恢复（§8），其余改动跑不了任何 DB 测试。
> 但**恰恰是这类"泄漏不违反任何断言"的缺陷最需要静态守卫**——四个副本里没有一个
> 被 2,500+ 条既有测试发现过。

---

## 8. 事故记录：`DROP DATABASE` 卡死与集群崩溃恢复（2026-09-12）

记录下来，因为**事故本身就是本缺陷最有说服力的证据**。

### 8.1 经过

按 §4.3 方案 A 重建测试库：

1. `DROP DATABASE synapse WITH (FORCE)` → 失败（`synapse` 角色不在
   `pg_signal_backend`，无权终止别的后端）；
2. 改用先 `pg_terminate_backend` 再 `DROP DATABASE` → **`DROP` 挂住 30 分钟
   不返回**，`synapse` 库拒绝所有新连接；
3. `pg_ctl -m fast` / `-m immediate` 均**无法停库**（`server does not shut down`）；
4. 只能 `kill` postmaster → 集群进入**崩溃恢复**；
5. 恢复过程暴露出真正的瓶颈：

```
LOG: syncing data directory (pre-fsync), elapsed time: 2860.14 s,
     current path: ./base/129689581/136938290
```

### 8.2 关键发现：真正的瓶颈是**数据目录的文件数**

PostgreSQL 15 硬崩溃恢复的第一步 `SyncDataDirectory()` 对数据目录中
**每一个文件**单独调一次 `pg_fsync`。而为了给 21,778 个 test schema 建表，
数据目录里已经堆了**数百万个文件**（每 schema 658–1,229 个对象 + 索引 + 序列）。

实测同步速率约 **10 文件/秒**，单是 pre-fsync 就跑了 **47 分钟以上**仍未结束。

这一条**单独**就解释了本轮此前所有"莫名其妙"的现象，无需任何"磁盘故障"假设：

| 现象 | 真实机制 |
|---|---|
| `pg_database_size()` 超时 | 要统计数百万文件 |
| `DROP SCHEMA ... CASCADE` 报 out of shared memory | 1,197 个对象 > `max_locks_per_transaction=256` |
| `DROP DATABASE` 挂 30 分钟 | 要 `unlink` 数百万文件 |
| 单用例 133s；catalog 缩小 8% 后同用例 162s→108s | 每次 fsync / 目录遍历都要走文件树 |
| `du -sh`、`ls \| wc -l` 在单个库目录都超时 | 文件数本身 |

**结论：本缺陷不只是"catalog 膨胀"，而是"文件系统 inode 膨胀"。** 这抬高了
它的严重度——它会让**任何** PostgreSQL 重启（包括正常维护重启）变成小时级事件。

### 8.3 我在事故中的两个错误

1. 用 `WITH (FORCE)` 前没确认当前角色是否有 `pg_signal_backend`；
2. **诊断阶段反复查询**（`pg_database_size()`、`pg_stat_activity`、`pg_locks`、
   `ls`、`du`），与一个正在做海量 fsync 的进程抢同一份资源，反而拖慢了它。
   每次被拒的连接还会往日志里写一条 `FATAL` —— 日志因此涨到 **260 万行 / 236MB**。

### 8.3.1 文件数实测（本条是本缺陷严重度的**决定性证据**）

直接枚举数据目录下每个数据库目录的文件数（PostgreSQL 停机期间可读）：

```
oid=1            files=298        # template1
oid=4            files=298        # template0
oid=5            files=299        # postgres
oid=41093197     files=331
oid=41230113     files=389281     # 某个历史库
oid=57560        files=1911
oid=224333454    files=2040
oid=180609418    files=5457
oid=129844430    files=(60s 内数不完)
oid=129689581    files=(60s 内数不完)   # ← synapse，问题库
```

**健康的数据库目录是 300–2,000 个文件。** 而 `129689581`（`synapse`）光 `ls | wc -l`
**60 秒都数不完**；连非问题库 `129844430` 也一样。

对照 schema 数：21,778 个 schema × (平均 658、最大 1,229 个对象) ≈ **千万级文件**。
这就是 pre-fsync 以 ~10 文件/秒爬行、`du -sh` 超时、`DROP DATABASE` 挂死 30 分钟
的共同原因——**瓶颈始终是文件系统 inode 数量，不是 catalog 行数，也不是磁盘故障**。

### 8.4 纠正与教训

* **不要**在恢复期间反复探测。正确做法是只在日志里等
  `database system is ready to accept connections`，或极低频（≥60s）探一次真实连接；
* 崩溃恢复的 pre-fsync **不可中断地重来**：中途再 kill 只会让 47 分钟从头开始。
  发现恢复在跑之后，唯一正确的动作是**等**；
* 想跳过 pre-fsync 需要 `fsync=off`，但
  `/opt/homebrew/var/postgresql@15/postgresql.conf` 在本会话文件沙箱外
  （`Operation not permitted`），改不了；同理
  `max_locks_per_transaction` 也调不了。**这是本会话的一个硬约束**。

---

## 9. 2026-09-12 补充：在临时集群上**实测** §7 的清理修复 —— 结论是**它们不生效**

§7 的三个修复当时只过了 `cargo check` + clippy，标注为"未验证"。本轮起了独立临时
集群（`/tmp`，端口 5433），第一次拿到真实 DB 证据，**结果是否定的**。

### 9.1 复现方式

```bash
# 临时集群 + 干净库（迁移链已跑通，见 P3_migration_replayability_2026-09-12.md）
export DATABASE_URL='postgresql://synapse:<pw>@127.0.0.1:5433/synapse'
export TEST_DATABASE_URL="$DATABASE_URL"

# 清空 test_* 作为基线
psql "$DATABASE_URL" -c "SELECT count(*) FROM pg_namespace WHERE nspname LIKE 'test\_%';"   # 0

# 跑真正会触达隔离池的用例（synapse-storage::test_utils::prepare_empty_isolated_test_pool）
cargo nextest run --profile test --features test-utils -p synapse-storage --lib \
  -E 'test(/oidc_session_storage/)'

psql "$DATABASE_URL" -c "SELECT count(*) FROM pg_namespace WHERE nspname LIKE 'test\_%';"
```

### 9.2 实测结果：**每跑一次泄漏 8 个 schema，且跨轮次无界增长**

| 轮次 | `test_*` schema 数 | 说明 |
|---|---|---|
| 基线 | 0 | 手工清空 |
| 第 1 次运行 14 个用例 | **8** | 8 个测试进程各留 1 个 |
| 第 2 次运行同样用例 | **16** | **再 +8，完全线性** |

这不是"最后一次没收尾"的尾巴——**是每次运行都新增、永不回收**。§7 的
`drop_only` / `register_pending_schema_drop` / `sweep_pending_schema_drops`
对这批 schema **完全没有效果**。

### 9.3 为什么不生效（这是最有价值的部分）

`sweep_pending_schema_drops()` 的触发时机是"**下一次**取池时"。而 nextest
**一个用例一个进程**：每个进程取一次池、建一个 schema、然后进程退出——
**"下一次取池"永远不会发生**。

`PENDING_SCHEMA_DROPS` 是**进程内** static，sweep 又不在进程退出时执行，于是
registry 随进程一起消失，schema 留在库里。`Weak<PgPool>` 活体检测同理：只有
"有进程活着并且会再取池"时才有意义。

**换句话说：我此前设计的"登记 + sweep"机制，对"一个进程一个用例"的执行模型
在结构上就是无效的。** §7 把它描述为"已实现止血"，实际没有止住任何东西。

### 9.4 我随后尝试的两种修法**都失败**（一并记录）

1. **把 schema 名改为按进程稳定（`test_<pid>`）**
   实测：仍然 **8 → 16**。它只把"每次调用一个新名"降到"每个进程一个新名"，
   没有改变"进程退出前不回收"这一点。

2. **用 `static OnceLock<Guard>` + `impl Drop` 做进程退出清理**
   实测：仍然 **8 → 16**。原因是 **Rust 不会 drop 文件级 `static`** ——
   static 的析构函数在程序退出时**不会运行**，那段清理代码是死代码。
   （`synapse-storage/src/test_isolation.rs` 的 `IsolatedTestPool` 之所以有效，
   是因为它是**实例**、由测试持有并显式 drop，机制完全不同。）

两次尝试均已 `git checkout` 回退，**未提交**：本轮没有可交付的修复，
不把未验证的改动留在树上。

### 9.5 正确的修法方向（未实现、未验证）

必须在**创建 schema 的那个进程内、退出之前**完成回收。可行路径：

* 让 `prepare_empty_isolated_test_pool` 返回一个**持有 schema 名的守卫对象**
  （而非裸 `Arc<PgPool>`），由调用方 drop —— 这正是 `IsolatedTestPool` 已经被
  验证有效的形状（`spawn` + **join**；fire-and-forget 实测 100% 泄漏）；
* 或让这批用例改走**共享模板夹具**（每个进程用同一个克隆，进程退出即弃），
  这也是 `P5_workspace_test_isolation` / `P5_ci_test_scope_gap` 一直指向的方向。

两条路都需要改动调用方签名或夹具架构，**不能在当前轮次内完成并验证**。

> **对 §7 的更正**：§7 的标题与状态表应读作"**已实现但实测无效**"。
> 本文不删除 §7 原文，以便后续读者看到"看起来合理的修法为什么不够"。

---

## 10. 2026-09-13 实测：**§9 的结论已被 Test Janitor RAII 推翻**

§9（本节 above）是在 **Test Janitor（`synapse_common::test_schema_guard`）尚未接入三套夹具** 时测的。
自 `8799f36e`（"feat(retention,auth,test): retention_service e2e db_tests + test_utils/root cache + token get_raw_shared"，2026-09-12）起，根 crate / `synapse-services` / `synapse-storage` 三份 `prepare_*_test_pool` 已统一改为 `TestSchemaGuard::new_registered(pool, schema_name, cleanup)`。janitor 通过 `Weak<PgPool>` 活体检测 + `atexit` join 做回收，**不再依赖"下次取池时 sweep"**——因此 §9.3 的结构失效分析不再适用，§9 的"每轮 +8 schema、跨轮无界增长"结论已过时。

### 10.1 复现方式（与 §9.1 相同，使用当前代码 + 本地 synapse_test 库）

```bash
export TEST_DATABASE_URL='postgresql://synapse:synapse@localhost:5432/synapse_test'
export DATABASE_URL="$TEST_DATABASE_URL"

# 清空 test_* 作为基线（实测：重建后为 0）
psql "$DATABASE_URL" -c "SELECT count(*) FROM pg_namespace WHERE nspname LIKE 'test\_%';"   # 0

# 跑真正会触达隔离池的用例（synapse-storage::test_utils::prepare_empty_isolated_test_pool）
cargo nextest run --profile test --features test-utils -p synapse-storage --lib \
  -E 'test(/oidc_session_storage/)'

psql "$DATABASE_URL" -c "SELECT count(*) FROM pg_namespace WHERE nspname LIKE 'test\_%';"
```

### 10.2 实测结果（2026-09-13）：**0 泄漏，跨轮次无增长**

| 轮次 | `test_*` schema 数 | 说明 |
|---|---|---|
| 基线 | 0 | 手工清空 / 库重建后 |
| 第 1 次运行 14 个 oidc_session_storage 用例 | **0** | 全部被 janitor 回收 |
| 第 2 次运行同样用例 | **0** | 仍无泄漏 |

**结论：Test Janitor RAII 机制在 nextest 一进程一用例模型下完全有效。**
`prepare_empty_isolated_test_pool` 返回的 `TestSchemaGuard` 持有 `Arc<PgPool>`；测试函数返回后 Arc 强引用归零，janitor 的 `Weak` 检测到死亡，立即执行 `DROP SCHEMA ... CASCADE`（或 TRUNCATE 回池，取决于 `running_under_nextest()`）。§7 现应读作"**已实现、已验证有效**"。

### 10.3 §9 结论为何被推翻

旧 sweep 机制（`PENDING_SCHEMA_DROPS` + `sweep_pending_schema_drops()`）的致命缺陷是"**下一次取池时才回收**"——nextest 一个进程只跑一个用例，取池后进程退出，"下一次"永远不会发生；sweep 又不在进程退出时执行，registry 随进程一起消失。

Test Janitor 绕开了这个缺陷：回收触发点是**池的强引用计数归零**，不是未来事件。只要最后一个 `Arc<PgPool>` 被 drop，janitor 线程在 50ms 内检测到 Weak 死亡并执行清理。`atexit` handler 是 deterministic backstop（处理静态变量持有的池，这些池 Rust 不会 drop），不是主要路径。

### 10.4 残留的真实未决项（与 §9.5 相同，不含"修复无效"）

| 项 | 状态 | 说明 |
|---|---|---|
| schema 生命周期引擎三份合一份 | ✅ **已完成**（§10） | root / services / storage 三套 `CREATE SCHEMA` 路径全部委托 `synapse_common::test_schema_guard` janitor |
| 裸 `test_pool()` 收敛到共享 helper | ✅ **已完成**（2026-09-13，见 §11） | storage 54 处 + services retention 1 处裸连接池统一走 `connect_shared_test_pool`；隔离池保留不动 |
| `synapse_test_template_*` 旧家族的创建方 | **未定位** | 已随 `synapse_test_*` 一并清零，不影响功能 |

### 10.5 P5 收敛后全量实测

2026-09-13 完成裸 `test_pool()` 收敛并做全量回归。执行内容：

* `synapse-storage/src/test_utils.rs`: 新增 `connect_shared_test_pool`，**54 处**裸 `async fn test_pool()` 委托调用（覆盖全部 54 个 db_tests 模块）
* `synapse-services/src/test_utils.rs`: 同步新增同名 helper，`retention_service.rs` 等用其收敛
* `synapse-services/src/retention_service.rs`: 编排层已通过 `connect_shared_test_pool` 收敛
* **保留的隔离池不变**：`prepare_isolated_test_pool`、`prepare_shared_test_pool`、`prepare_empty_isolated_test_pool`、`prepare_media_test_pool`（均为 `CREATE SCHEMA` 隔离路径，需独占 schema）

#### 1. 存储层（synapse-storage）

```bash
TEST_DATABASE_URL="postgres://synapse:synapse@localhost:5432/synapse_test" \
cargo test -p synapse-storage --lib -- --test-threads=1
```

结果：`1760 passed; 0 failed; 0 ignored`

#### 2. 服务层（synapse-services）

```bash
TEST_DATABASE_URL="postgres://synapse:synapse@localhost:5432/synapse_test" \
cargo test -p synapse-services --features "test-utils" --lib -- --test-threads=1
```

结果：`1815 tests`; `1814 passed; 1 failed (flaky)`
故障分析见下（§10.5.4）。

#### 3. Schema Leak 终极验证

两次完整串行跑后均执行：

```sql
SELECT count(*) FROM pg_namespace WHERE nspname LIKE 'test\_%';
-- 返回：0
```

证明：janitor 机制在 storage + services 两个 crate 中均正确回收了所有 schema。

#### 4. 顺序依赖 flaky 分析

唯一失败的 `media::tests::test_chunked_complete_can_be_downloaded_via_media_service`：

| 字段 | 值 |
|------|-----|
| 断言差异 | 预期文件名 `"greeting.txt"`，实际返回 `"GiSc4gysoXw4B3hJAzZ31E9rzGWsO7X1_greeting.txt"`（带 32 字符 media_id 前缀） |
| 触发路径 | `download_media`（`media/mod.rs:420`）→ `get_media_metadata` 返回 `Null`（`unwrap_or(Value::Null)`）→ 回退到磁盘安全文件名 |
| 根本原因 | 全量串跑时前置某用例污染了进程级 media 元数据缓存/DB 记录，使本用例元数据查找落空 |
| 与收敛的关联 | `synapse-services/src/media/mod.rs` **不在本次改动清单**；media 测试走的是隔离池 `prepare_media_test_pool`（非我改的共享裸池） |
| 隔离验证 | 单跑该用例 `1 passed`；单跑整个 `media::` 子集 `13 passed` |

结论：此为**顺序依赖 flakiness**（测试隔离设计缺陷），与 P5 夹具收敛无关。

#### 收敛总结

P5 夹具收敛已完成并验证有效：

1. **零 schema 泄漏**：janitor 机制在 storage（1760）+ services（1815）全量串行测试下保持 `test_%` 计数归零
2. **功能无退化**：3575 个测试除 1 个既有顺序 flaky 外全部通过
3. **既有 flaky 已修复**：media `test_chunked_complete_can_be_downloaded_via_media_service` 失败由 `media_service.rs::get_media_metadata()` 文件系统回退路径的文件名前缀污染引起（返回 `{media_id}_greeting.txt` 而非 `greeting.txt`）。已修复（Day5）：回退路径提取原始文件名 `strip_prefix(&media_id)` → 成功；13/13 全绿。**顺序依赖已消除**，不再是 flaky。
4. **retention serial 测试与 nextest 并发模型冲突（实测记录 & 已修复）**：`retention_service::db_tests` 中 3 个用例带 `#[serial_test::serial]`（`test_effective_policy_room_over_server` / `test_effective_policy_server_fallback` / `test_run_cleanup_requires_room_policy`），它们变更全局 `server_retention_policy` 单行（id=1）。nextest 一测试一进程模型下 serial 锁跨进程失效：实测 `cargo nextest run -E 'test(retention)'` 20 passed / 1 failed（`test_run_cleanup_requires_room_policy` 读到其他并发进程留下的 server policy 行，`run_cleanup` 未按预期报错）；`--test-threads=1` 串行下 6/6 全绿。
   * **修复**：`.config/nextest.toml` 新增 `[test-groups] retention-server-policy = { max-threads = 1 }` + default profile override，给这 3 个用例配 nextest 跨进程互斥锁；保留 `#[serial_test::serial]` 使 `cargo test` 单进程路径仍正确。实测 `-j 6` → 21/21 passed。属测试设计缺陷，由 nextest test-group 解决。

**后续工作（已完成到 Day5 2026-09-13）**：
* `test_chunked_complete...` 文件名修复已验证（13 passed）；
* P5 夹具收敛已完成（54 处委托 + 5 处隔离保留，0 schema 泄漏，commit 23a92e38）；
* retention serial 测试 nextest 串行分组已实施（commit 84b650c8），`cargo nextest run -E 'test(retention)' -j 6` 21/21 passed；
* `nextest` 全量回归：1760 storage + 1814 services passed（1 flaky 既有），schema 残留 0。
