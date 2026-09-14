# P1-D — 可选 seed 白名单：设计（仅设计，不含实现）

- Date: 2026-09-14
- Branch: `main`
- HEAD when this record was written: `2f545f22`
- 状态：**已实现**（见 §12 实现记录）。本文档是设计说明 + 实现后的实测证据。
- 关联：`docs/audit/P1B_test_isolation_template_2026-09-13.md`、
  `docs/audit/P1C_unified_test_isolation_2026-09-13.md`、
  `docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md`

本文所有"事实"要么是本次在本机 `synapse_test` 库上实际查询的输出，要么标注为
`[推理]` 或 `[未验证]`。没有任何数字是估算或推测的。

---

## 1. 问题陈述

测试隔离的第二/第三份实现（`src/test_utils.rs`）在克隆模板时**只复制 3 张白名单表**：

```rust
// src/test_utils.rs:1058
const SEED_REFERENCE_TABLES: &[&str] = &["server_media_quota", "server_retention_policy", "sync_stream_id"];
```

而共享实现（`synapse-common/src/test_isolation.rs`，2f545f22 之前已在 `6a051fcb` 补齐索引名还原）
在 Phase 1b **复制模板里每一张表的行**：

```sql
-- synapse-common/src/test_isolation.rs:495-504（实测代码，逐字拷贝）
FOR r IN
    SELECT tablename FROM pg_tables
    WHERE schemaname = '{template}' AND tablename <> '{TEMPLATE_READY_TABLE}'
    ORDER BY tablename
LOOP
    EXECUTE format('INSERT INTO %I.%I SELECT * FROM %I.%I', '{schema}', r.tablename, '{template}', r.tablename);
END LOOP;
```

"复制全部"与"复制白名单"看起来是两种语义。合并两份实现时，必须先判定它们是
否等价、不等价的部分会不会改变测试行为。**这个判定就是 (a) 要解决的问题**：
给共享克隆加一个可选的 seed 白名单参数，让调用方能声明"我依赖哪些种子表"。

---

## 2. 现状：同一职责有 3 份硬编码

| # | 位置 | 形式 | 内容 |
|---|------|------|------|
| 1 | `src/test_utils.rs:1058` | `const SEED_REFERENCE_TABLES` | 3 张表 |
| 2 | `src/test_utils.rs:1498` | **内联数组字面量**（TRUNCATE 后重新播种路径） | 同样 3 张表 |
| 3 | `synapse-common/src/test_isolation.rs:481-505` | "复制全部"（隐含：无名单） | 全部表 |

第 2 行是纯冗余——它连常量都没用，直接把同一组表名又写了一遍。实测原文：

```
src/test_utils.rs:1498:    for table in &["server_media_quota", "server_retention_policy", "sync_stream_id"] {
```

这三处任何一处漂移，都会出现"克隆有这行 seed、池化复用的 schema 没有"这类
只在特定路径下复现的失败。

> 这正对应 `AGENTS.md` 铁律 2（同一职责只允许一份实现）。本设计在解决问题的同时
> 顺手把这三份收敛成一份。

---

## 3. 根因：白名单在 v11 基线上是"空操作"

seed 数据的**唯一**来源是 `migrations/00000000_unified_schema_v11.sql`。本次实测该文件
里全部 `INSERT INTO` 语句（`grep -n -i '^\s*INSERT INTO'`）：

```
4554:INSERT INTO sync_stream_id (stream_type, last_id, updated_ts)
4563:INSERT INTO server_retention_policy (id, max_lifetime, min_lifetime, is_expire_on_clients, created_ts, updated_ts)
4568:INSERT INTO server_media_quota (
```

共 3 条，正是白名单里的 3 张表。`migrations/00000001_extensions_v10.sql` 里
`INSERT` / `COPY` 条数 **0**。`build_template` 只 `strip_copy_blocks` 掉
`COPY ... FROM stdin` 块（实测函数体：仅识别 `COPY` + `FROM stdin`），上述
`INSERT` 会被逐条真实执行。

因此模板 schema 里**有且仅有这三张表非空**。本次在真实库上逐个精确计数
（`test_isolation_template_bec240fb79ed438b`，254 张 `relkind='r'` 表，
`query_to_xml` 精确 `count(*)`，滤掉 0）：

| 表 | 行数 |
|----|------|
| `server_media_quota` | 1 |
| `server_retention_policy` | 1 |
| `sync_stream_id` | 4 |
| （其余 250 张表） | 0 |

**结论（实测，非推理）：在当前基线上，"复制全部"与"复制这 3 张白名单"产生
逐字节等价的克隆。** 两份实现之间的语义差是 0 行。

还有一条被注释误导的历史包袱：

```sql
-- migrations/00000000_unified_schema_v11.sql:4551-4553
-- Default admin user removed (DB-04):
-- Hardcoded admin INSERT moved to scripts/create-default-admin.sql for opt-in bootstrap.
```

`users` 表的 admin seed **已被删除**，文件末尾只剩一句
`UPDATE users SET must_change_password = TRUE WHERE username = 'admin';`（作用于 0 行）。
所以 `src/test_utils.rs:1056` 那句"`@admin:localhost` seed 故意不复制"**描述的是一段
不存在的代码**——它是 v11 之前的历史记忆，属于 `AGENTS.md` 铁律 1（禁止兼容残留）
所说的"唯一存在理由是兼容旧行为"的残留注释。

---

## 4. 设计目标与非目标

**目标**

1. 共享克隆 API 增加**可选** seed 白名单参数；不传时的行为与今天**完全一致**。
2. 提供**单一真相源**常量，供三处硬编码引用，消除铁律 2 违规。
3. 让"复制全部 vs 复制白名单"的等价性成为一个**可自证变红**的不变量测试（铁律 8），
   而不是靠注释和记忆维持。
4. 不引入第二种实现、不引入兼容壳（铁律 1、6）。

**非目标**

- 不改变任何调用点的运行时行为（本次收敛后行为零变化）。
- 不做 `src/test_utils.rs` 的收敛改造——该文件当前正被另一 agent 编辑
  （写本文时 `git status` 显示 ` M src/test_utils.rs`）。本文只提供接口契约。
- 不引入"表名正则"、"按前缀匹配"、"复制前 N 行"等更花哨的过滤能力。
  当前没有任何调用方需要它们，属于过度设计（铁律 1）。

---

## 5. API 设计

### 5.1 新增公开类型（位于 `synapse-common/src/test_isolation.rs`）

```rust
/// The template rows a clone should begin with.
///
/// Both variants read from the same template schema; they differ only in
/// *which tables* phase 1b copies. The two are deliberately not equal by
/// construction: `Everything` copies whatever the template happens to hold,
/// while `Only` is a written-down contract that fails to build the clone if a
/// named table has disappeared from the template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeedSource<'a> {
    /// Phase 1b copies every baseline table, exactly as it does today.
    /// This is the default for every production call site.
    Everything,
    /// Phase 1b copies only these tables. Any name absent from the template
    /// aborts the clone with a `42P01` from the `INSERT` (fail loudly — a
    /// silently-missing seed is what made media upload tests fail with
    /// "no server_media_quota row" in the pre-unification fixture).
    Only(&'a [&'a str]),
}

/// The template rows the **baseline migrations** seed, and therefore the only
/// tables phase 1b has anything to copy for. Measured against
/// `migrations/00000000_unified_schema_v11.sql` (3 `INSERT INTO` statements)
/// and `00000001_extensions_v10.sql` (0). Guarded by
/// `seed_reference_tables_match_baseline` — if a future migration seeds a
/// fourth table, that test goes red and this constant must be extended.
pub const SEED_REFERENCE_TABLES: &[&str] =
    &["server_media_quota", "server_retention_policy", "sync_stream_id"];
```

### 5.2 签名变更

```rust
// before (2f545f22)
fn clone_statement(schema: &str, template: &str) -> String;

pub async fn clone_schema_from_template(
    pool: &sqlx::PgPool, schema: &str, template: &str,
) -> Result<(), String>;

// after
fn clone_statement(
    schema: &str, template: &str, seeds: SeedSource<'_>,
) -> Result<String, String>;   // Result 来自名单校验（见 §5.3）

pub async fn clone_schema_from_template(
    pool: &sqlx::PgPool, schema: &str, template: &str, seeds: SeedSource<'_>,
) -> Result<(), String>;
```

**决定：不加默认参数包装函数。** 生产调用点全部显式传参（见 §7）。
理由：包装函数属于 `AGENTS.md` 铁律 6（薄壳禁止）；而且调用方
"我依赖全部 / 我依赖这 3 张"是一个应当写在调用点上的事实，隐藏它才是成本。

### 5.3 Phase 1b 的生成逻辑

`clone_statement` 里 Phase 1b 的 `WHERE` 子句由 `SeedSource` 决定，其余
Phase 1 / 1b / 1c / 2 与今天**逐字符相同**：

| `seeds` | Phase 1b 的 `WHERE` | 结果 |
|---------|---------------------|------|
| `Everything` | `... AND tablename <> '{TEMPLATE_READY_TABLE}'` | 与今天完全一致 |
| `Only(&[])` | `... AND 1 = 0` | 不复制任何行（合法，见 §6.3） |
| `Only(&[a, b])` | `... AND tablename IN ('{a}', '{b}') AND tablename <> '{TEMPLATE_READY_TABLE}'` | 只复制 a、b |

`Only` 的名单来自 `const &[&str]`，**不是**用户输入，但仍然必须走标识符白名单校验，
因为它是拼进 `DO $$ ... $$` 的字符串字面量。这是既有的同类风险：`schema` 与
`template` 目前也是 `format!` 直接插入的（实测 `clone_statement` 全文如此）。
**注意：本模块目前没有任何标识符校验函数**（实测 `grep 'fn validate_schema_identifier'`
无输出），所以这是**新增**的一层防护，不是复用既有机制：

```rust
fn seed_table_list_sql(names: &[&str], schema: &str, template: &str) -> Result<String, String> {
    for n in names {
        if !n.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_') || n.is_empty() {
            return Err(format!("refusing to build a seed list from non-identifier table name {n:?}"));
        }
    }
    Ok(if names.is_empty() {
        "1 = 0".to_string()
    } else {
        let list = names.iter().map(|n| format!("'{n}'")).collect::<Vec<_>>().join(", ");
        format!("tablename IN ({list}) AND tablename <> '{TEMPLATE_READY_TABLE}'")
    })
}
```

> 注意：`Only` 模式下**不保留** `ORDER BY tablename` 的原有语义顾虑——FK 尚未重放
> （Phase 2 才建 FK），所以复制顺序仍然无关。这一点 Phase 1b 的既有注释已说明，
> 本次不改变。

### 5.4 为什么不把"缺失表"做成软跳过

现有根 fixture 用 `IF EXISTS (SELECT 1 FROM pg_tables ...) ... ELSE RAISE NOTICE` 软跳过
缺失表。**新接口不做软跳过**，理由（实测支撑）：

- 根 fixture 之所以需要软跳过，是因为它的模板是从 `migrations/` 目录**动态拼接**的
  （`src/test_utils.rs:983-1001`，逐文件读 `migrations/`）。模板内容随目录内容变化。
- 共享模块的模板来自**编译期内联的** `include_str!("../../migrations/…")`，
  模板内容与二进制绑定。表缺失只可能是"基线迁移被改坏了"，那是必须立刻暴露的缺陷，
  不是"环境差异"。静默跳过会把迁移回归伪装成"配置行不存在"的下游测试失败。
- 更根本地：`AGENTS.md` 铁律 8 的推论——**看到长期全绿的门禁，先怀疑它没在工作**。
  软跳过就是让克隆永久全绿的那种设计。

---

## 6. 语义细节与边界

### 6.1 Phase 1c（序列推进）与白名单正交

`SeedSource::Only` 让某些表在克隆里为 0 行。Phase 1c 用
`max_id = (SELECT max(id) FROM <clone>.<t>)` 推进克隆自有序列；0 行时 `max_id IS NULL`，
序列停在 `last_value = 1, is_called = false`，正是"刚迁移完的空表"应有的状态。
**不需要为白名单改 Phase 1c。**

**实测证据（2026-09-14，本机 `synapse_test`，复刻 Phase 1 + Phase 1c 的真实 SQL）**：

| 场景 | `max_id` | `last_value` / `is_called` | 省略 id 的 INSERT |
|------|----------|---------------------------|------------------|
| 空表（被 allowlist 排除） | 0 | NULL / false | 返回 `id = 1` ✅ |
| 有 1 行（copy-all） | 1 | 1 / true | 返回 `id = 2` ✅ |

两条都验证过，因为"空表序列正常"和"有数据的表序列被正确推进"是同一段代码的两种输入。
自动化版本见 §12 的 `allowlist_clone_can_insert_into_a_table_it_emptied`。

### 6.2 与索引名还原（`6a051fcb`）的交互

Phase 1d 的索引配对是 `PARTITION BY tbl` 的，与行数据无关。白名单不影响它。

### 6.3 空名单的合法性

`Only(&[])` 保留为合法输入（生成的 `WHERE 1 = 0`）。它有一个真实用途：测试
"克隆 = 纯结构、零数据"这个性质本身。不建议在生产调用点使用。

### 6.4 迁移新增第 4 张 seed 表时会怎样

1. `baseline_fingerprint` 变化 → `template_schema_name` 变化 → 新模板被重建；
   旧模板不被复用。**这一步是自动的**（`synapse-common/src/test_isolation.rs:44/54`）。
2. `SeedSource::Everything` 的调用点：**自动**拿到新增 seed 行。
3. `SeedSource::Only(&SEED_REFERENCE_TABLES)` 的调用点：**不会**拿到该行。
4. §9 的 `seed_reference_tables_match_baseline` 测试**变红**，强制维护者更新常量。

即"沉默的语义分叉"被转成"一个必须处理的红色测试"。这是本设计的**主要价值**——
不是今天省几行，而是把"两个 fixture 的种子内容一致"变成被机器守护的不变量。

---

## 7. 调用点影响

共享 `clone_schema_from_template` 的全部生产调用点（实测 `grep -rn`，排除 `target/`
与 `.claude/worktrees/`）：

| 文件:行 | 建议传参 | 理由 |
|---------|----------|------|
| `synapse-storage/src/test_isolation.rs:108` | `SeedSource::Everything` | 该 fixture 从无白名单，行为必须保持 |
| `synapse-services/src/test_utils.rs:526` | `SeedSource::Everything` | `ea1a3ddc` 已确认收敛后与旧 fixture 行为一致 |
| `src/test_utils.rs`（收敛后） | `SeedSource::Only(SEED_REFERENCE_TABLES)` | 它今天就是这个语义，显式写下来 |

`src/test_utils.rs` 内部还有一条**不经过共享克隆**的再播种路径
（`src/test_utils.rs:1498`，TRUNCATE 后 `INSERT ... ON CONFLICT DO NOTHING`）。
它应改为引用共享常量，消掉第 2 份硬编码：

```rust
// before
for table in &["server_media_quota", "server_retention_policy", "sync_stream_id"] {
// after
for table in synapse_common::test_isolation::SEED_REFERENCE_TABLES {
```

模块内测试调用点（共 10 处，实测行号 `1177/1247/1290/1298/1331/1409/1480/1530/1589/1665`；
其中 `1290/1298` 断言"模板缺失必须报错"、`1665` 断言"完整克隆必须通过校验"，
两者的断言语义与 seed 模式无关）统一传 `SeedSource::Everything`，保持既有断言不动。

> 收敛 `src/test_utils.rs` 到共享克隆本身**不在本文范围**——该文件当前正被另一
> agent 编辑。本文只规定：收敛发生的那一天，这个调用点传
> `SeedSource::Only(SEED_REFERENCE_TABLES)`，并删除它本地的 `SEED_REFERENCE_TABLES`
> 定义与那段"admin seed 故意不复制"的历史注释（§3 已证明该注释描述的行为不存在）。

---

## 8. 变更规模

| 项 | 规模 |
|----|------|
| `synapse-common/src/test_isolation.rs` | 新增 1 个 enum、1 个常量、1 个私有 helper（约 35 行）；`clone_statement` 与 `clone_schema_from_template` 各加 1 个参数；Phase 1b 的 `WHERE` 改为插值 |
| `synapse-storage/src/test_isolation.rs` | 1 行 |
| `synapse-services/src/test_utils.rs` | 1 行 |
| 模块内测试 | 10 行 |
| 新增测试 | 3 个（见 §9） |

---

## 9. 验证方案（实现后必须提供的证据）

### 9.1 `seed_reference_tables_match_baseline`（无 DB，纯静态，**必做**）

从 `include_str!` 的 v11 基线出发，用既有的 `split_sql_statements` +
`strip_copy_blocks` 切出语句，取出所有 `INSERT INTO <tbl>` 的 `<tbl>` 集合，
断言其等于 `SEED_REFERENCE_TABLES` 的集合（顺序无关）。

- **为什么必须做**：这是"复制全部 == 复制白名单"这一等价性赖以成立的**唯一前提**。
  没有它，今天测得的三表事实会在下一次迁移改动时静默失效。
- **变异验证（铁律 8）**：在 v11 基线里插入一条
  `INSERT INTO <某第四张表> ...`，该测试必须变红；恢复后必须变绿。
  本设计不接受"我读了迁移文件所以它是对的"作为证据。

### 9.2 `allowlist_clone_matches_full_clone_row_for_row`（DB，**必做**）

对同一模板做两次克隆——一次 `Everything`，一次 `Only(SEED_REFERENCE_TABLES)`——
然后逐表比对。本次已在本机库上**原样执行过比对用的 CTE**（输出见 §3），
实现时把该 CTE 落成断言：

```sql
WITH tab AS (
  SELECT c.relname
  FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
  WHERE n.nspname = $1 AND c.relkind = 'r'
),
cnt AS (
  SELECT relname,
         (xpath('/row/c/text()',
                query_to_xml(format('SELECT count(*) AS c FROM %I.%I', $1, relname),
                             false, true, '')))[1]::text::bigint AS n
  FROM tab
)
SELECT relname, n FROM cnt WHERE n <> 0 ORDER BY relname;
```

断言：`<allowlist_clone>.relname, n` 的集合（滤掉 `TEMPLATE_READY_TABLE` 与 0 行）
与全量克隆的**完全相同**。

- **变异 1（证明 allowlist 真在过滤）**：给 `Only(&[])` 也跑一遍，断言结果集为
  **空**（滤掉 marker 后）。若仍非空，说明参数没被接进 SQL。
- **变异 2（证明全量模式真在复制）**：断言全量克隆里
  `sync_stream_id` 恰为 4 行、`server_media_quota` 恰为 1 行。名字级别的断言
  （而不是只比两个克隆互相相等）才能防"两个都空也算相等"。
- `[未验证]` 逐列比对：如果要把强度提到"逐行逐列相同"，需要处理
  `server_media_quota.updated_ts`（`EXTRACT(EPOCH FROM NOW())::BIGINT * 1000`，
  毫秒，两次克隆必然不同）与 `sync_stream_id.updated_ts`（同源）。
  **本设计刻意只做逐表行数比对**，因为两份克隆本来就来自同一模板、同一 `INSERT`
  语句，逐列比对不会增加任何信息量，只会引入一个假的失败源。若将来要加，
  必须显式排除这两列，并用 `xpath(... query_to_xml('SELECT row_to_json(t)::text ...'))`
  逐行集合比对。

### 9.3 既有隔离测试保持全绿

`cargo test -p synapse-common --lib --all-features test_isolation`：
**实测 `20 passed; 0 failed`**（实现前基线 17/0，新增 3 个）。见 §12。

### 9.4 确定性 / 无回归

`synapse-common` 单 crate 无并发负载，直接
`SQLX_OFFLINE=true CARGO_TARGET_DIR=/tmp/audit_target cargo test -p synapse-common --lib`。
调用点只需编译验证 + 各自 lib 回归（行为零变化）。见 §12。

---

## 10. 回滚

改动是**纯 API 扩展 + 常量提取**，没有任何调用点行为变化：

- 回滚 = `git revert <commit>`，无需数据迁移、无需重建模板
  （`baseline_fingerprint` 不参与，`clone_statement` 的 `Everything` 输出字节相同）。
- 风险点：`Only` 模式若在实现时把 `WHERE` 拼错（例如忘了 `AND tablename <> marker`），
  会把 marker 表也纳入复制并 `42P01`。§9.2 的变异 1 就是针对这个的守卫；
  §12 记录的实际变异证明它确实会红。

---

## 11. 结果与遗留

### 11.1 设计问题（已裁定）

1. **`SeedSource` 进 `synapse-common` 公开 API —— 已采用。** `test_isolation` 模块
   本身已按 `test-utils`/`test` 门控，对生产依赖图为零，`SeedSource` 随之同样门控，
   无需 `#[doc(hidden)]`。
2. **`src/test_utils.rs` 收敛时机 —— 仍待办。** 该文件在本轮开始时正被另一 agent
   编辑，本轮未触碰（写本文时它已从 `git status` 消失，可以开始收敛了）。收敛时：
   调用点传 `SeedSource::Only(SEED_REFERENCE_TABLES)`；删掉本地 `SEED_REFERENCE_TABLES`
   常量；把 `src/test_utils.rs:1498` 那处裸数组字面量改为引用共享常量；
   删掉那段已被证伪的"admin seed 故意不复制"注释。
3. **§6.1 空表序列 —— 已实测（§6.1 表格），Phase 1c 无需改动。**

### 11.2 本轮新发现

- **另一个调用点被第一版 grep 漏掉**：`synapse-services/src/test_utils.rs:262`
  （参数名是 `&admin_pool`/`&template` 而不是 `&pool`/`template_name`）。
  说明"调用点清单"这类事实必须靠编译器复核，不能只靠模式匹配——
  编译器的 `E0061 argument #4 ... is missing` 才是权威列表。
- **`synapse-common` 不能内联迁移文件**（模块头部明确的设计约束），所以
  §9.1 的基线解析守卫无法放在 `synapse-common` 里，只能放在
  `tests/unit/test_isolation_unification_tests.rs`（该文件已经 `include_str!` 了这两个
  迁移并哈希它们）。DB 等价测试则放在 `synapse-common`，用一个**合成基线**
  （一张命中白名单的种子表、一张空表、一张不在白名单里的种子表）覆盖三种情形，
  因为它需要构造"不在白名单里的种子"这种生产基线里不存在的情形。
  两者分工：静态守卫钉住**生产常量**，DB 测试钉住**机制**。

---

## 12. 实现记录（2026-09-14）

### 12.1 改动文件

| 文件 | 改动 |
|------|------|
| `synapse-common/src/test_isolation.rs` | 新增 `SeedSource`、`SEED_REFERENCE_TABLES`、`seed_where_clause`；`clone_statement` 返回 `Result<String, String>` 并接受 `seeds`；`clone_schema_from_template` 加第 4 个参数；Phase 1b 的 `WHERE` 改为插值；新增 3 个测试 |
| `synapse-storage/src/test_isolation.rs` | 调用点传 `SeedSource::Everything` |
| `synapse-services/src/test_utils.rs` | 两个调用点（`:262`、`:526`）各传 `SeedSource::Everything` |
| `tests/unit/test_isolation_unification_tests.rs` | 新增 Guard 6 + `insert_statements` 解析辅助 |

### 12.2 绿灯证据

```
cargo test -p synapse-common --lib --all-features test_isolation
  running 20 tests
  test result: ok. 20 passed; 0 failed   （实现前 17/0）

cargo test --test unit --all-features the_seed_allowlist_matches_what_the_baseline_seeds
  test result: ok. 1 passed; 0 failed
```

### 12.3 变红证据（铁律 8；全部在真实代码上执行过，事后已还原并 `diff` 确认无残留）

| # | 故意制造的违规 | 预期 | 实测结果 |
|---|----------------|------|----------|
| 1 | 向 v11 追加 `INSERT INTO unify_mutation_probe (id) VALUES (1);` | Guard 6 红 | **红**：`left: [... "unify_mutation_probe"] / right: [3 张表]` |
| 2 | 把 `"users"` 加进 `SEED_REFERENCE_TABLES` | Guard 6 红 | **红**：`right: [... "users"]` |
| 3 | 注释掉 v11 里 `server_retention_policy` 的 `INSERT`（反方向） | Guard 6 红 | **红**：`left: ["server_media_quota", "sync_stream_id"]` |
| 4 | 追加一条**注释形式**的 `INSERT INTO users ...` | Guard 6 绿（证明解析器正确跳过注释，不会误报） | **绿** ✅ |
| 5 | 让 `seed_where_clause` 把 `Only(_)` 与 `Everything` 同等对待 | 两个 DB 测试红 | **红**：两个都失败，`unify_seed_outside_allowlist` 从 0 变 1，且空表仍剩 1 行 |

变异 5 是这里最关键的一条：它是"参数声明了但没接进 SQL"这一整类 bug 的直接检验，
并且证明了 §9.2 要求的两个断言方向都真的有效（一个管"过滤是否生效"，
一个管"空表是否真的空了"）。

### 12.4 还原确认

- `migrations/00000000_unified_schema_v11.sql` 与操作前备份 `diff` 为空，且
  `git diff --stat` 对该文件无输出。
- `synapse-common/src/test_isolation.rs` 与变异前备份 `diff` 为空。
- 全仓 `grep -c 'MUTATION PROBE'` = 0。

---

## 13. 第三份实现的收敛（2026-09-14 完成）

`src/test_utils.rs` 的私有克隆已删除，改为委托共享实现。它在上一轮正被另一 agent
编辑（`00c0aad2`），等其 `git status` 变干净后执行。

### 13.1 被删除的实现到底偏离了什么（逐条实测，不是印象）

对 `HEAD:src/test_utils.rs` 的 177 行克隆体做静态核查：

| 探测 | 命中数 | 结论 |
|------|--------|------|
| `ADD CONSTRAINT` | **0** | 完全**不重放 FOREIGN KEY** |
| `pg_get_triggerdef` | **0** | 完全**不重放 trigger** |
| `pg_get_functiondef` / `pg_proc` | **0** / **0** | 完全**不重放函数** |
| `matview` / `relkind IN ('v','m')` | **0** | 只查 `pg_views`，**漏物化视图** |
| `EXCEPTION WHEN OTHERS` | **2** | 每个索引与每个视图的失败都被 `NULL` 吞掉 |

共享实现逐项重放这些对象，并在最后用 `validate_clone` 比对对象清单。所以这不只是
"少复制了一些东西"——旧实现的失败模式是**静默的**：克隆缺 FK/trigger 时它自己不知道，
测试要等到很后面才以顺序相关的怪失败暴露出来。

### 13.2 收敛的做法

根夹具的 `clone_schema_from_template` 现在：

1. 用 `raw_sql` 跑一个 DDL 块完成 `DROP SCHEMA IF EXISTS` + `CREATE SCHEMA`，
   再用 `raw_sql` 执行 `SET search_path`；
2. 调 `synapse_common::test_isolation::clone_schema_from_template(..., SeedSource::Only(SEED_REFERENCE_TABLES))`。

第 1 步特意用 `raw_sql` 而不是三次 formatted query：标识符无法绑定，而
`check_sqlx_dynamic_ratio.sh` 把每个非宏查询调用计为动态 SQL。DDL 块 + `raw_sql`
`SET search_path` 的组合让**收敛这一步本身**对棘轮的贡献为 0
（若用三次 formatted query 会 +3）。
（该门禁使用**非锚定** grep，所以连注释里出现查询宏的字面量都会被计数——
写注释时也要避开。）

`SEED_REFERENCE_TABLES` 的第三份硬编码（原来 `:1498` 的裸数组字面量）也改为引用
共享常量，并补上了 `advance_schema_sequences`（见 §14）。

### 13.3 新增守卫 7：只允许一处构建克隆 SQL

```rust
// tests/unit/test_isolation_unification_tests.rs
fn exactly_one_place_builds_the_schema_clone()
```

扫描 `src/` 与全部 workspace crate 的 `src/`，断言 Phase 1b 的插值标记
`AND {seed_where}` 只出现在 `synapse-common/src/test_isolation.rs`。

- **为什么断言 `== 1` 而不是 `>= 1`**：`>= 1` 会让"完全没有这个子句"的树通过，
  而 `== 1` 在共享子句被改名/改签名时也会响亮失败——这正是它要防的漂移。
- **变异验证（铁律 8）**：向 `src/test_utils.rs` 注入一个含该标记的常量模拟
  "第二份实现重新长出来"，守卫**变红**并打印
  `Found it in ["src/test_utils.rs", "synapse-common/src/test_isolation.rs"]`；
  移除后恢复绿。另有一个 `visited > 100` 断言防止目录遍历失效导致的空扫描假绿。

---

## 14. 收敛过程中发现并修复的两个独立缺陷

两者都与 seed 白名单无关，但都是收敛的**前置条件**——不修它们，共享克隆在
"schema 被复用"这条路径上的语义是错的。

### 14.1 Phase 1c 建的克隆序列没有 `OWNED BY`（共享模块的缺陷）

Phase 1c 用 `CREATE SEQUENCE` + `ALTER COLUMN SET DEFAULT nextval(...)` 把克隆序列绑到
克隆列上，但**从未建立 OWNED BY 边**。实测最小复现：

| 表 | `pg_get_serial_sequence` | `TRUNCATE ... RESTART IDENTITY` 之后 |
|----|--------------------------|--------------------------------------|
| `bigserial`（模板原生） | `probe_own3.serial_t_id_seq` | `1 / false` ✅ 被重置 |
| 普通 sequence + `SET DEFAULT`（共享克隆的形态） | *(空)* | `42 / true` ❌ **未重置** |

后果：`TRUNCATE ... RESTART IDENTITY` 对克隆序列**静默失效**，于是"清空并复用 schema"
得不到确定状态。修法是 Phase 1c 增加
`ALTER SEQUENCE <clone>.<seq> OWNED BY <clone>.<tbl>.<col>`；`OWNED BY` 同时让序列
随列一起被 DROP，避免序列在 DROP SCHEMA 之外的路径上泄漏。

> 这个缺陷也解释了为什么根夹具的池化路径会残留陈旧的序列位置。

> 修复过程中我自己踩了一次 `format` 占位符不匹配：`OWNED BY %I.%I` 写了 5 个 `%I`
> 却只传 4 个参数，PG 报的是 `relation "<schema>" does not exist`（它把 schema 名当成了
> 列名）。**5 个对象名就必须有 5 个 `%I`**，报错信息完全没提占位符数量。

### 14.2 池化路径重新播种后不推进序列（根夹具的缺陷）

池化路径是 `TRUNCATE ... RESTART IDENTITY`（所有序列回到 1）**之后**再把模板的参考行
复制回来（显式 id 从 1 开始）。直接实测（对真实模板克隆）：

```
ERROR:  duplicate key value violates unique constraint "server_media_quota_pkey"
DETAIL:  Key (id)=(1) already exists.
```

修法：共享模块新增 `advance_schema_sequences(pool, schema)`，把"按现有行推进 schema 内
所有序列"实现一次（复用 Phase 1c 的同一条 catalog 路径
`pg_attrdef` → `pg_depend` → `relkind='S'`，**不**假定只有 serial 列）。池化路径在
重新播种后调用它。

放进共享模块而不是在根夹具里再写一遍，正是铁律 2 的适用场景——"按现有行推进序列"
已经存在（Phase 1c），写成第二份就是把同一个错误再犯一次。

### 14.3 对应的回归测试与红证

| 测试 | 覆盖 | 红证 |
|------|------|------|
| `reusing_a_schema_reseeds_its_sequences`（共享模块） | `OWNED BY` 成立 + `RESTART IDENTITY` 真的重置 + 重新播种后必撞 + 推进后可用 + 幂等 + 拒绝非标识符 schema 名 | 断言 `(last_value, is_called) == (1, false)`；去掉 `OWNED BY` 即失败 |
| `pooled_schema_can_insert_into_a_reseeded_table`（根 crate lib） | 真实池化重置路径 + 默认 id 插入 | **实测红**：去掉 `advance_schema_sequences` 后报 `duplicate key ... "server_media_quota_pkey"`、`Key (id)=(1) already exists`，并打印出具体 schema 名；恢复后绿 |

> 该测试第一版是**假绿**：它只调 `acquire_pooled_schema` 一次，而首次获取走的是**克隆**
> 路径（克隆自带 phase 1c 推进），所以去掉修复也照样通过。实测发现后改为**直接驱动
> `truncate_and_reseed_schema`**，即真正被测的那段代码。这条教训已写进测试注释，
> 避免以后有人把它"简化"回假绿形态。

---

## 15. 收敛后的验证

| 命令 | 结果 |
|------|------|
| `cargo test -p synapse-common --lib --all-features test_isolation` | **21 passed / 0 failed** |
| `cargo test --lib --all-features`（根 crate，含池化回归测试） | 见 §15.1 |
| `cargo test --test unit --all-features` | 1955 passed / **2 failed**（既有 sqlx 棘轮测试，见 §15.2） |
| `cargo test --test unit --all-features exactly_one_place_builds_the_schema_clone` | 1 passed（变异后红） |
| `cargo check --workspace --all-targets --all-features` | 干净，仅 3 个既有 warning |
| `./scripts/check_fmt_ratchet.sh` | `current=0 baseline=0` |
| `check_sqlx_dynamic_ratio.sh` | 1564 → **1578**（本次新增 14 处，全部在新增的测试与 `advance_schema_sequences` 夹具内；见 §15.2） |

### 15.1 根 crate `--lib`

**688 passed / 0 failed**（含新增的池化回归测试
`test_utils::pooled_schema_reseed_tests::pooled_schema_can_insert_into_a_reseeded_table`）。

### 15.2 棘轮：本次 +14，而门禁的红色来自**既有** +121 漂移

两次实测（`git stash` 隔离）：

| 状态 | dynamic |
|------|---------|
| HEAD（stash 掉本次三个文件） | **1564** |
| 本次改动后 | **1578** |
| 基线文件 `BASELINE_DYNAMIC` | **1443** |

所以：

- 工作树相对基线早已漂移 **+121**（1564 − 1443），这发生在本轮之前。
- 本次改动的增量是 **+14**，来源：新增的 `advance_schema_sequences` 辅助（3 处）、
  两个新回归测试（各若干处 catalog 查询）、守卫 7 的扫描（0 处）。
  这些都是**测试代码**，而该门禁的口径明确包含 `#[cfg(test)]` 内联测试模块。
- 因此 `sqlx_ratio_gate_tests` 的 2 个失败在本轮之前就已存在；本轮只是把数字从
  1564 推到 1578。

处理建议（**本轮不做**，避免洗掉真实漂移）：该门禁的意图是"防止新增不可静态化的
**生产**查询"，而测试夹具天生需要动态 DDL 与 catalog 查询。可行方向是把
`#[cfg(test)]`/`test-utils` 门控模块排除出扫描范围，再据实重设基线；
单向下调基线会把 +121 的真实漂移一起洗掉，因此不应只改数字。

---

## 16. 与 `src/test_utils.rs`（第三份实现）的关系 —— 已闭合

- 三份模板克隆 → **一份**（守卫 7 锁住）。
- 三份种子表名清单 → **一份**（`SEED_REFERENCE_TABLES`，守卫 6 锁住）。
- 被证伪的 "`@admin:localhost` seed 故意不复制"注释 → 已随旧克隆体一并删除。
- 旧克隆体漏掉的 FK / trigger / 函数 / 物化视图 → 由共享实现重放，并由
  `validate_clone` 的对象清单比对兜底。

`AGENTS.md` 铁律 2（同一职责只允许一份实现）在测试隔离这一项上至此闭合。
