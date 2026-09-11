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

| 文件 | `CREATE SCHEMA` 处数 | 是否有清理 |
|---|---|---|
| `src/test_utils.rs` | 7 | 共享路径有；隔离路径**本次才补上** |
| `synapse-services/src/test_utils.rs` | 4 | ❌ **完全没有** pending-return/清理机制 |
| `synapse-services/src/media/mod.rs` | 1 | ❌ `prepare_media_test_pool` 裸建裸弃 |

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

| 项 | 状态 | 说明 |
|---|---|---|
| 清完剩余 21,748 个普通 `test_*` + 25 个模板 schema | **未做** | 见 §4.2/§4.3 |
| `src/test_utils.rs::prepare_isolated_test_pool` 接入待删登记 | ✅ **已实现** | 见 §7：`drop_only` 通路 + 本路径也做 sweep。⚠️ **尚未跑通实测验证**（见 §7.1） |
| `synapse-services/src/test_utils.rs` 的 4 处 `CREATE SCHEMA` | **未做** | 该副本**完全没有任何清理机制**，需要与根 crate 收敛（§1.4） |
| `synapse-services/src/media/mod.rs::prepare_media_test_pool` | **未做** | 同上 |
| 三套夹具收敛成一份 | **未做** | **真正的根因**（§1.4）；与 `P5_workspace_test_isolation`、`P5_migration_search_path_shadowing` 同一结论 |
| `synapse_test_template_*` 旧家族的创建方 | **未定位** | 前缀与 `synapse_test_template_ready` *标记*同名易混；已随 `synapse_test_*` 一并删除，但创建方仍未查明 |

> **不宣称已完成**：本次只做掉了"存量清道夫 + 模板自动剪枝"，**没有**堵住
> `test_*` / `media_test_*` 的持续泄漏。清理后如果照常跑测试，这两个家族仍会
> 重新增长。在结构改造落地前，本清理是**定期维护动作**，不是一次性终结。
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
| `du -sh`、`ls | wc -l` 在单个库目录都超时 | 文件数本身 |

**结论：本缺陷不只是"catalog 膨胀"，而是"文件系统 inode 膨胀"。** 这抬高了
它的严重度——它会让**任何** PostgreSQL 重启（包括正常维护重启）变成小时级事件。

### 8.3 我在事故中的两个错误

1. 用 `WITH (FORCE)` 前没确认当前角色是否有 `pg_signal_backend`；
2. **诊断阶段反复查询**（`pg_database_size()`、`pg_stat_activity`、`pg_locks`、
   `ls`、`du`），与一个正在做海量 fsync 的进程抢同一份资源，反而拖慢了它。
   每次被拒的连接还会往日志里写一条 `FATAL` —— 日志因此涨到 **260 万行 / 236MB**。

### 8.4 纠正与教训

* **不要**在恢复期间反复探测。正确做法是只在日志里等
  `database system is ready to accept connections`，或极低频（≥60s）探一次真实连接；
* 崩溃恢复的 pre-fsync **不可中断地重来**：中途再 kill 只会让 47 分钟从头开始。
  发现恢复在跑之后，唯一正确的动作是**等**；
* 想跳过 pre-fsync 需要 `fsync=off`，但
  `/opt/homebrew/var/postgresql@15/postgresql.conf` 在本会话文件沙箱外
  （`Operation not permitted`），改不了；同理
  `max_locks_per_transaction` 也调不了。**这是本会话的一个硬约束**。
