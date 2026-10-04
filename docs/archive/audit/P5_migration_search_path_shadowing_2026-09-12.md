# DB-06 迁移的 search_path 遮蔽：`public` 外键指向临时 test schema

> **日期**: 2026-09-12
> **基线提交**: `0cfa1227`
> **范围**: P5「工程质量 / 防复发」；同时结清
> `P5_test_fixture_error_swallowing_2026-09-11.md` §3 遗留的未定位失败
> **性质**: 测试基础设施缺陷（**不是**应用层 bug）

---

## 0. 结论

`room_summary::db_tests::test_add_member_creates_record` 的失败根因**不是**
夹具吞错、不是事务顺序、也不是并发——而是：

**迁移 `20260831070000_room_summary_members_fk_not_deferred.sql` 用未限定的
表名添加外键，`public` 与测试 schema 同名表互相遮蔽，导致 `public` 里的外键被
固化指向一个 2026-07 的临时测试 schema。**

```sql
-- 修复前（search_path 依赖）
ALTER TABLE room_summary_members
    ADD CONSTRAINT fk_room_summary_members_room
    FOREIGN KEY (room_id) REFERENCES rooms(room_id) ON DELETE CASCADE NOT VALID;
```

`room_summary_members` 与 `rooms` **两个**表名都由 `search_path` 解析。
当 `public` 里已存在同名表（历史遗留），而 `CREATE TABLE IF NOT EXISTS` 又
**静默跳过**了目标 schema 的建表，约束就会挂到 `public` 副本上，父表则解析到
当时 search_path 里的那个临时 schema。

实测污染状态：

| schema | 约束 | 父表 | validated |
|---|---|---|---|
| `public` | `fk_room_summary_members_room` | `test_51027_403_1788832624579288000.rooms` | `false` |
| `public` | `fk_room_summary_members_user` | `test_51027_403_1788832624579288000.users` | `false` |
| `test_template_v2_*` | 同名两约束 | 自身 schema | `true` |

后果：所有**通过 `public` 访问 `room_summary_members`** 的测试
（即 `synapse-storage` 里全部手写 `test_pool()` 的套件）都以
SQLSTATE `23503` 假性失败——报「房间不存在」，而房间**确实存在**
（`SELECT count(*) FROM rooms WHERE room_id=$1` 返回 `1`）。

### 为什么这个错误极具误导性

`public.rooms` 有 2089 行、`public.users` 有数据，`ensure_test_room` 的
`.expect("...")` 也**没有** panic。所有"数据在哪"的直觉都在说"没问题"。
唯一能一行否掉整个方向的是**直接读约束的父表 OID**：

```sql
SELECT conname, confrelid::regclass::text AS parent, convalidated
FROM pg_constraint
WHERE conrelid='public.room_summary_members'::regclass AND contype='f';
```

---

## 1. 为什么 `CREATE TABLE` 不带 schema 限定是**对的**（但 `ALTER TABLE` 不是）

这是本次修复最关键的判断依据，值得单独说明，否则很容易把"未限定表名"一刀切。

| 形态 | 父表缺失时的行为 | 能否静默错绑 |
|---|---|---|
| `CREATE TABLE t (... REFERENCES p(x))` | 报错，**迁移失败**（可见） | 否 |
| `ALTER TABLE t ADD CONSTRAINT ... REFERENCES p(x)` | 若 `t` 已存在则成功，约束挂到错误 schema | **是** |

基线 `00000000_unified_schema_v11.sql` 有 **145 处**未限定 `REFERENCES`，
**全部**是内联在 `CREATE TABLE` 里的——它必须在目标 schema 里成功建表，
否则那条 `CREATE TABLE` 直接失败。所以基线是**安全的**，不需要动，
本次也**没有**动它（守卫测试明确豁免基线文件）。

危险性只存在于 `ALTER TABLE ... ADD CONSTRAINT` 这一族。

---

## 2. 修复

### 2.1 根因修复：把 DDL 钉死在 `current_schema()`

`migrations/20260831070000_room_summary_members_fk_not_deferred.sql`
改写为 `DO $$ ... EXECUTE format(...) $$`，所有表名由
`format('%I.%I', current_schema(), 'room_summary_members')` 与
`REFERENCES %I.rooms` 生成：

```sql
DO $$
DECLARE
    target text := format('%I.%I', current_schema(), 'room_summary_members');
BEGIN
    IF to_regclass(target) IS NULL THEN
        RAISE NOTICE 'room_summary_members not present in schema %, skipping DB-06 FK rewrite', current_schema();
        RETURN;
    END IF;

    EXECUTE format('ALTER TABLE %s DROP CONSTRAINT IF EXISTS fk_room_summary_members_room', target);
    ...
    EXECUTE format(
        'ALTER TABLE %s ADD CONSTRAINT fk_room_summary_members_room '
        'FOREIGN KEY (room_id) REFERENCES %I.rooms(room_id) ON DELETE CASCADE NOT VALID',
        target, current_schema()
    );
    ...
END $$;
```

同时把 `VALIDATE CONSTRAINT` 包进 `EXCEPTION WHEN foreign_key_violation`：
`NOT VALID` 已经对**未来**写入强制生效，历史孤儿行不该让整个迁移失败
（否则一个健康部署会因为历史脏数据而"无法迁移"）。

### 2.2 同一缺陷族的其余 3 个文件

守卫测试（§3.1）在同一批次里又抓出 **3 个文件、8 处**同类 DDL，全部按同一
模式修复：

| 文件 | 处数 | 说明 |
|---|---|---|
| `20260831070000_room_summary_members_fk_not_deferred.sql` | 2 | 本次根因 |
| `20260831070000_..._fk_not_deferred.undo.sql` | 2 | 回滚脚本，同缺陷 |
| `20260831060000_events_no_cascade.sql` | 1 | `events.room_id` 的 CASCADE→NO ACTION |
| `20260831060000_events_no_cascade.undo.sql` | 1 | 回滚脚本，同缺陷 |
| `20260904010000_schema_p1_federation_and_integrity.sql` | 2 | `fk_event_edges_prev`、`fk_events_redacted_by` |
| `20260904030000_schema_p3_perf.sql` | 1 | `fk_backup_keys_room` |

### 2.3 对既有污染数据库的自愈

`src/test_utils.rs` 新增 `heal_public_cross_schema_foreign_keys`，在
`init_template_schema` 里于 `DROP/CREATE SCHEMA public` 之后调用：

* 扫描 `public` 中 `confrelid` 落在**其它 schema** 的外键；
* 同名父表若在 `public` 存在，则重建为 `public.<child> -> public.<parent>`
  （保留原 `ON DELETE` 动作与 `DEFERRABLE` 属性）；
* 同名父表**不存在**时**不**删除约束，只 `WARN` 等人工判断——
  静默 drop 会把"强制约束"变成"无约束"，是更糟的修复；
* 幂等：修完一次后再扫不到跨 schema 外键。

为什么需要它：`public` 只在**模板重建**时被清理，而模板是否重建取决于迁移
指纹（文件名+大小+mtime）。已污染的数据库若恰好不重建，就会一直坏下去。

---

## 3. 回归证据

### 3.1 静态守卫（RED → GREEN）

`tests/unit/migration_search_path_tests.rs`
:: `migration_foreign_keys_must_not_rely_on_search_path`

规则：`ADD CONSTRAINT ... REFERENCES <表>` 里的父表若是基线表且未限定 schema，
且附近没有 `current_schema()` 钉死，即判违规。**基线文件豁免**（理由见 §1）。

**RED**（修复 2.2 之前，仅根因文件已修；下列行号为**修复前**位置）：

```
$ cargo nextest run --profile test --features test-utils --test unit \
    -E 'test(/migration_foreign_keys/)' --no-capture
search_path-dependent foreign keys in migrations:
  migrations/20260831060000_events_no_cascade.sql:61 REFERENCES rooms ...
  migrations/20260831060000_events_no_cascade.undo.sql:8 REFERENCES rooms ...
  migrations/20260904010000_schema_p1_federation_and_integrity.sql:56 REFERENCES events ...
  migrations/20260904010000_schema_p1_federation_and_integrity.sql:75 REFERENCES events ...
  migrations/20260904030000_schema_p3_perf.sql:64 REFERENCES rooms ...
        FAIL
```

> 注：这几行现在都写成 `REFERENCES %I.rooms(...)`（`current_schema()` 钉死），
> 因此当前文件里 `%I.` 的位置是
> `20260831060000:74`、`20260831060000.undo:23`、
> `20260904010000:66`/`:92`、`20260904030000:73`。

**GREEN**（全部修复后）：

```
$ cargo nextest run --profile test --features test-utils --test unit \
    -E 'test(/migration_search_path_tests/)'
    Summary [5.558s] 2 tests run: 2 passed, 1846 skipped
```

### 3.2 动态守卫：自愈函数真的会改约束

`heal_repoints_cross_schema_foreign_key_at_same_named_parent`

用一次性 schema 造出"子表在 A、父表是 B.rooms、而 A.rooms 也存在"的损坏形态，
断言：

1. fixture 起始状态**确实是坏的**（父表 = `{stale}.rooms`）；
2. 调用自愈后父表变为 `{child}.rooms`（同名本地父表）；
3. 再调一次**幂等**，结果不变。

> fixture 刻意**不**使用共享 `public`：nextest 是"一进程一用例"并发跑在同一个
> 数据库上，拿 `public` 当夹具会污染其它用例。

### 3.3 迁移在真实 schema 里可执行且绑定正确

在一次性 schema 中实际执行 4 个改写后的迁移：

```
=== 结果（全部指向 a4f2_m.*，无一条指向 public.*）===
a4f2_m.events.fk_events_room_no_action             -> a4f2_m.rooms
a4f2_m.room_summary_members.fk_room_summary_members_room -> a4f2_m.rooms
a4f2_m.room_summary_members.fk_room_summary_members_user -> a4f2_m.users
```

**决定性对照**——目标 schema 有自建 `rooms`、`public` 也有 `rooms` 时：

| | 修复前 | 修复后 |
|---|---|---|
| 目标 schema 的约束父表 | `public.rooms`（错绑） | `a4f2_shadow.rooms` ✅ |
| `public` 的约束 | 被误改 | **未被触碰** ✅ |

### 3.4 原始失败用例

```
$ cargo nextest run --profile test --features test-utils -p synapse-storage --lib \
    -E 'test(/room_summary::db_tests/)'
    Summary [0.258s] 30 tests run: 30 passed, 1502 skipped
```

`test_add_member_creates_record` 由 FAIL（`0 passed; 1 failed`）转为 PASS，
且**同套件另外 29 个用例一并转绿**——之前它们只是被同一个 FK 挡住而已。

---

## 4. 门禁状态（全部本地实测）

| 门禁 | 结果 |
|---|---|
| `./scripts/check_fmt_ratchet.sh` | `current=0 baseline=0` OK |
| `bash scripts/ci/check_sqlx_dynamic_ratio.sh` | dynamic=1436 static=61 OK（基线 1432→1436，理由见 §5） |
| `python3 scripts/check_migration_consistency.py` | `issues: []` `warnings: []` |
| `python3 scripts/check_baseline_consolidation.py` | v11 已吸收全部 36 个增量迁移对象 |
| `python3 scripts/check_config_consistency.py` | OK |
| `cargo clippy --workspace --all-targets --all-features --locked` | EXIT=0，警告 15 条 = 基线，无新增 |
| `cargo nextest run --profile test --features test-utils --lib --test unit` | **2524 passed, 4 skipped**（较基线 +2：本次新增的 2 个守卫测试） |

> clippy 实测输出（`ensure_test_room` / `ensure_test_event` never-used 属**既有**
> 基线，位于本批次未触碰的 `synapse-storage/src/voice.rs`；`assert_eq!` 配字面
> bool 等 13 条同样为既有）：
>
> ```
> 5 warning: used `assert_eq!` with a literal bool
> 2 warning: the borrowed expression implements the required traits
> 2 warning: field assignment outside of initializer for an instance created with Default::default()
> 1 warning: unused import: `crate::test_mocks::InMemoryToDeviceStorage`
> 1 warning: function `ensure_test_room` is never used
> 1 warning: function `ensure_test_event` is never used
> ...
> clippy EXIT=0
> ```

---

## 5. sqlx 棘轮 +4 的理由（1432 → 1436）

新增 4 处动态查询，全部在 `src/test_utils.rs::heal_cross_schema_foreign_keys_in`
内：扫描 `pg_constraint`、`to_regclass` 探测同名父表、`DROP CONSTRAINT`、
`ADD CONSTRAINT`。表名与约束名必须用 `format!`/`quote_ident` 拼接，
**无法**静态化。

⚠️ 该函数位于 `#[cfg(any(test, feature = "test-utils"))]` 门控模块内，
**不是生产代码**，只在测试夹具初始化时运行。棘轮的单向收紧方向已写入
`scripts/ci/sqlx_dynamic_ratio_baseline`：等 22k 残留 test schema 清理完成、
这批手写 `test_pool()` 迁到共享 schema 隔离夹具后，本函数即无存在必要。

---

## 6. 未做 / 超出范围

| 项 | 状态 |
|---|---|
| 清理 **22,581 个残留 `test_*` schema** | **未做**，需用户确认（破坏性操作） |
| 把 `synapse-storage` 手写 `test_pool()` 迁到共享隔离夹具 | **未做**，约 30 个模块；这是本缺陷的**根本**土壤 |
| 24 个残留 `test_template_v2_*` schema | **未做**，随 22k 清理一并处理 |
| 已污染生产库的存量 FK 修复 | 依赖模板重建或 §2.3 自愈触发；**未验证**在真实部署上的路径 |

> 关于"未验证"最后一项：`20260831070000` 若在某个数据库上**已记录为执行过**，
> 改写迁移**不会**重跑。该库需要靠模板重建时 `DROP/CREATE SCHEMA public`，
> 或 §2.3 的自愈路径修正。本环境两条路径都已验证有效，但真实部署库未纳入验证范围。

---

## 7. 复现命令

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
export DATABASE_URL='postgresql://synapse:<pw>@localhost:5432/synapse'
export TEST_DATABASE_URL="$DATABASE_URL"
export SQLX_OFFLINE=true
export CARGO_TARGET_DIR=/tmp/prs1          # 必须隔离，避免并发构建污染 target/

# 1) 静态守卫：迁移不得依赖 search_path
cargo nextest run --profile test --features test-utils --test unit \
  -E 'test(/migration_foreign_keys_must_not_rely_on_search_path/)'

# 2) 动态守卫：自愈函数确实修复跨 schema 外键、且幂等
cargo nextest run --profile test --features test-utils --test unit \
  -E 'test(/heal_repoints_cross_schema_foreign_key/)'

# 3) 原始失败用例 + 同套件
cargo nextest run --profile test --features test-utils -p synapse-storage --lib \
  -E 'test(/room_summary::db_tests/)'

# 4) 直接检查 public 是否仍残留跨 schema 外键（应为 0 行）
psql "$DATABASE_URL" -c "
SELECT c.conname, c.conrelid::regclass, c.confrelid::regclass
FROM pg_constraint c
JOIN pg_class cl ON cl.oid = c.conrelid
JOIN pg_namespace n ON n.oid = cl.relnamespace
JOIN pg_class pl ON pl.oid = c.confrelid
JOIN pg_namespace pn ON pn.oid = pl.relnamespace
WHERE c.contype='f' AND n.nspname='public' AND pn.nspname <> 'public';"
```

---

## 8. 复盘：什么能防住这类 bug？

本缺陷能长期潜伏，是因为**三个独立条件同时成立**，缺一不可：

1. 迁移用 `ALTER TABLE ... ADD CONSTRAINT` + 未限定表名（本次修掉）；
2. 基线用 `CREATE TABLE IF NOT EXISTS`，使目标 schema 缺表时**静默跳过**而非报错
   （未改动——这是它在生产上的正确行为）；
3. 测试库的 `public` 里长期留有整份表副本（历史遗留，清理会被 §6 的破坏性操作
   触及）。

只修 (1) 就足以止血，**守卫测试**保证它不再回来。但真正让这次排查如此昂贵的，
是一个更普遍的信号问题：

> **当两个观测互相矛盾时（`rooms` 存在 vs FK 说不存在），
> 最该被怀疑的是"承载矛盾的那条声明"，而不是继续给两个观测各自找解释。**

本次绕的最大弯路，是先把「房间不存在」当成事实去查应用层的插入顺序与并发，
而不是先去读 `confrelid`——一行 SQL 就能推翻整个前提。
