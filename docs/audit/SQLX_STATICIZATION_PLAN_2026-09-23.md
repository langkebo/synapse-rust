# SQLx 静态化：阶段总结与剩余工作（2026-09-23 启动 · 2026-09-25 C34 后）

> **本文档只保留三样东西**：阶段总结（§0）、**仍存在的问题**（§7）、**优化方案**（§8）。
> 已关闭缺陷的逐条明细与各批次执行记录（C1–C34 / W1–W5）在冻结快照
> [`docs/synapse-rust/archive/SQLX_STATICIZATION_PLAN_2026-09-23_HISTORY.md`](../synapse-rust/archive/SQLX_STATICIZATION_PLAN_2026-09-23_HISTORY.md)
> —— 它保留原 **§7.2 / §8.x** 编号，因此代码注释与提交信息里的旧引用仍可查。
> 规则与门禁的唯一来源是 `AGENTS.md` 的 **R1–R13**（本文档不复制其正文）。

---

## 0. 阶段总结

### 0.1 战果（唯一数字口径）

| 指标 | 战役起点（2026-09-23） | 现在 | 变化 |
|---|---|---|---|
| `dynamic_production` | 1532（近似） | **384** | **−74.9%** |
| `static` | 61 | **1084** | +1023 |
| `dynamic`（总） | 2151 | **1097** | −1054 |
| 静态占比 | 2.76% | **49.7%**（1084 / 2181） | +47.0pp |
| `.sqlx` 离线缓存 | 60 条 | **1053 条** | +993 |
| literal（逐文件棘轮，处 / 文件） | 876 / 98 | **313 / 60** | −563 |
| `param` 传参（D-14 新棘轮，处 / 文件） | — | **1 / 1** | 新立棘轮（此前混在 `runtime`，两道棘轮都不管） |
| `runtime` 残差 / `query_builder` | — | 70 / 13 文件 · **18**（已入计数棘轮） | — |

### 0.2 残量结构（"还剩多少活"的准确说法）

| 组成 | 处数 | 性质 |
|---|---|---|
| **可静态化残量** | **312** | **282 处字面量**（纯机械转换）+ **29 处运行期拼装**（`format!` 拼列清单/`ORDER BY` 方向等，需结构性替代）+ **1 处跨函数传参**（`param`，把字面量内联到调用点即可转），见 §7.3 D-14 |
| 测试基建（有意保留） | 57 | `synapse-test-utils/src/lib.rs` 28、`synapse-common/src/test_isolation.rs` 25、`test_schema_guard.rs` 4 |
| 结构性保留（有意） | 15 | `synapse-storage/src/event/pagination.rs`（9 runtime 游标/排序方向 + 6 literal） |
| **合计** | **384** | = 312 + 57 + 15 |

### 0.3 复现（唯一入口，勿手工数）

```bash
python3 scripts/ci/sqlx_query_census.py                     # 总量 / 分区 / 静态占比 / query_builder
bash scripts/ci/check_sqlx_dynamic_ratio.sh                 # 四道棘轮（生产动态 / 测试动态 / 静态 / QueryBuilder）
python3 scripts/ci/sqlx_query_census.py --list-production-dynamic .   # path:line:kind（literal|param|runtime）
# literal 棘轮输入（逐文件计数，应与 scripts/ci/sqlx_literal_production_baseline 逐行相同）：
python3 scripts/ci/sqlx_query_census.py --list-production-dynamic . \
  | grep ':literal$' | cut -d: -f1 | LC_ALL=C sort | uniq -c \
  | awk '{print $2"\t"$1}' | LC_ALL=C sort
# param 棘轮输入（D-14；只把 grep 的形态换成 :param$，对应
# scripts/ci/sqlx_param_production_baseline）：
python3 scripts/ci/sqlx_query_census.py --list-production-dynamic . \
  | grep ':param$' | cut -d: -f1 | LC_ALL=C sort | uniq -c \
  | awk '{print $2"\t"$1}' | LC_ALL=C sort
```

### 0.4 缺陷发现总览（**68 条**；只给统计与去向，不逐条显示）

| 类别 | 条数 | 说明 |
|---|---|---|
| ① 真 schema 下必然失败 | 15 | 列名写错、INSERT 漏 NOT NULL 列、路由背靠不存在的表、`WHERE $2 != '[]'` 在 prepare 阶段必败、绑定类型必败… |
| ② 门禁自身失效或长期红 | 9 | 假绿（`--static`、`0 tests`、陈旧 `public`、魔数下界）与长期红（`--all-features` clippy、悬空夹具、契约未同步） |
| ③ 吞错 / 非确定性 / 缓存不生效 | 8 | `unwrap_or_default`、`.ok().flatten()`、`let _ = <future>`、缺决胜键、值恒为 0 |
| ④ 死代码 / 第二份实现 / 空壳 / 遗留 schema | 12 | 零调用者语句与包装、第二份写入实现、`RowNotFound` 空壳端点、删表后遗留 schema |
| ⑤ 可空性 / 解码类型不符 | 4 | 可空列配非 `Option` 字段、jsonb 解成 `Vec<String>` |
| ⑥ 覆盖缺口 / 测试基建假绿 | 2 | 静态化后无 DB 往返、自建 schema 掩盖写入端约束 |
| ⑦ 文档级 | 6 | 计数漂移、过时结论、误导性"规则"注释 |
| ⑧ 结构性例外（有意保留） | 7 | D-13 / D-14 / D-18–D-22，见 §7.3 |
| ⑨ 阶段总结后新发现并已关闭 | 5 | D-62（通知响应的 `profile_tag` 键取自 `notification_type` ⇒ 已按修法① 改成真列 + 独立 `notification_type` 键）、**D-68**（通知记录层没有生产写入者、也没有保留期清理 ⇒ 已按修法① 接线 `record_notification` + `prune_old_notifications`，边界见 §0.5、明细见提交信息）、D-65（并发改动只改一半 ⇒ 集成+clippy 双红）、D-66（worktree 共享 `CARGO_TARGET_DIR` ⇒ 跨树复用产物，假红/假绿）、D-67（新增测试里的死常量让 clippy 红） |

**去向**：阶段总结前关闭的 57 条逐条明细在 HISTORY §7.2；总结后关闭的 6 条（D-37 / D-62 / D-65 /
D-66 / D-67 / D-68）记在各自提交信息里（下次阶段总结时并入快照）。本表 ①–⑧ 是**发现时**的归类
（历史口径，不随修复变动），因此 D-57 仍计入 ⑥、D-37 仍计入 ④、D-62 已改判为"已修" ——
"还剩哪些没修"看结论行与 §7.1，不看桶号。
**结论：60 已关闭 / 1 未关闭（D-57 部分）/ 7 结构性例外。**

### 0.5 阶段结论

1. 动态 SQL 已从**系统性风险**降为**局部清单**：384 处里 72 处有意保留，待收 **312 处**，
   其中 282 处是纯机械转换。
2. **收益性质变了**：早期批次每批都在挖"真 schema 下必败"的硬缺陷（① 类 15 条）；
   现在批次以机械收敛为主，并顺手清理一类残留（C31 清 `FromRow` 死代码、C32 消手工 `Row::get`、
   C33 消 `PgRow` 泄漏与 10 处吞错、C34 消 `Row` 解码与死 derive）。
3. **长期资产是规则与门禁，不是数字**：数字会被并发改动推动，R1–R13 与四道自证过的门禁才是
   "不再制造同类缺陷"的保证；本阶段新增的两条规则（宏实参须为调用点字面量、worktree 各自 target 目录）
   都来自实测而非推导。
4. **D-68 的接线边界（写清楚，免得下次误判）**：`notifications` 现在的生产写入者是
   `PushNotificationService::send_notification`（"服务端决定推送"这一处，排队成功后记一条，
   同批接入 30 天保留期清理）。本仓**没有**按事件求值的推送规则引擎，`sync` 的
   `notification_count` 仍由 `events` + `read_markers` 现算 —— 因此 `/notifications` 是
   "服务端实际推送过的通知"的记录，不是"按规则应当通知"的推导结果；两者口径不同属**有意**，
   若要统一（把计数改为读 `notifications`）那是另一个需要排期的产品改造。

---

## 7. 仍存在的问题（唯一登记处）

只登记**未关闭项**与**结构性例外**。已关闭项不在此显示（去向见 §0.4）。

### 7.1 汇总表

| 编号 | 类别 | 位置 | 问题 | 状态 | 下一步 |
|---|---|---|---|---|---|
| **D-57** | 测试基建假绿 | `tests/integration/mod.rs::require_test_pool()`（search_path = `<clone>, public`）× `scripts/ci/prepare_test_db.sh:79`（`RESET_PUBLIC=0`）× `to_regclass($1)` 走 search_path | baseline 是 `CREATE TABLE IF NOT EXISTS` 合并脚本、**不含 DROP** ⇒ 从 baseline 删掉的表仍留在长期库 `public` 里，"表存在/可用"类断言**假绿**且污染共享 `public` | **部分已修**（① 已做，② 未做） | ② 让 seed 对 `public` 也收敛（对已从 baseline 删除的对象补 `DROP … IF EXISTS`，或在不误删依赖扩展对象的前提下 `RESET_PUBLIC=1`）。**不能简单改成 1**：`DROP SCHEMA public CASCADE` 会连带删掉依赖 public 扩展的其它 schema 对象 |

### 7.2 逐条明细

**D-57②**：①（已做）三处裸 `to_regclass($1)` 改为 `to_regclass(format('%I.%I', current_schema(), $1))::text`，
`tests/` 内已无裸 `to_regclass`；自证留了两步判据（陈旧 `public` 里造探针表 ⇒ 旧口径非空假绿、
新口径 NULL 如实报缺失；把探针表加进 P0 清单 ⇒ 用例 FAIL，随后逐字节还原）。
②（未做）见上表。**验收判据**：跑一次 seed 后长期库 `public` 只应剩 baseline 与扩展所需对象；
用故意残留的表证明收敛确实会删它。

### 7.3 结构性例外（有意，不修，但必须遵守）

这些不是待修缺陷，而是**当前工具/接口的边界**，决定批次里"哪些站点允许保持动态"。

| 编号 | 边界 | 出现位置 | 怎么办 |
|---|---|---|---|
| D-13 | `Vec<Option<T>>` 元素可空数组**无 sqlx 映射**（SQL 文本本身是字面量） | `room_summary/repository.rs:332`/`:579`；`presence/mod.rs:232` | 保持动态；回收方向：并行数组 → 单个 `jsonb_to_recordset($n)`（独立改造，不排期） |
| D-14 | 运行期拼装 SQL（`format!` 拼列清单/排序方向）**有意保留**；其守卫覆盖缺口已于 2026-09-26 收紧（见下） | `space/repository.rs:572/626`、`user/storage.rs:989/1233`、`src/server/database.rs:43-45`、`event/state.rs`、`membership/mod.rs`、`event/basic.rs`、`event/batch.rs`、`maintenance.rs`、`state_groups.rs`、`event/dag.rs` 等（`format!` 类共 29 处，见 §0.2） | 保持动态，**不要**为压数字把动态标识符硬编码；逐文件回收方向见 HISTORY §7.2 D-14 |
| D-18 | 仅排序用列无对应结构体字段 ⇒ 用子查询包裹 | `thread/storage.rs:864` | 沿用子查询写法 |
| D-19 | `query_as!` 不认 `#[sqlx(rename)]` / `#[sqlx(skip)]` | `event_report/models.rs:29`、`module.rs:255` 等 | SQL 里显式写别名 / 合成 `NULL` 列 |
| D-20 | LEFT JOIN 外侧列被 PG 透传为 NOT NULL ⇒ sqlx 误推非空 | C6/C8/C9/C13 多处 | 用 `AS "col?"` 覆盖 |
| D-21 | 宏 `ty_match` 拒绝 `&Option<T>` 绑定 | `device_keys/storage.rs:121` 等 | `.as_deref()` / `.as_ref()` |
| D-22 | `query_as!` 不走 `FromRow`，`RETURNING *` 必须展开 | 多个批次 | 机械展开为显式列清单 |

> **D-14 的守卫覆盖缺口（2026-09-26 已收紧）**：literal 棘轮原先只看调用点实参的 token 形态，
> "把字面量绑到别处再传进来"的写法会被归为 `runtime` 而**两道棘轮都不管**。三种形状的处置：
> ① **同文件 `const`/`let` 字面量绑定** ⇒ census 现判 `literal`（进 literal 棘轮）；
> ② **跨函数传参**（实例：`synapse-common/src/transaction.rs:66` 的 `statement: &'static str`）
> ⇒ 新判 `param`，配独立棘轮 `scripts/ci/sqlx_param_production_baseline`；
> ③ **`QueryBuilder` 组装**（`query_builder=18`）⇒ SQL 文本确由运行期决定，属 R7 白名单
> （**不要求转换**），但已加**计数棘轮**：`BASELINE_QUERY_BUILDER=18`、
> 判据 `query_builder <= 基线`（只禁增；口径是**总数**，含测试区 1 处，比只看生产更严）。
> 自证：植入一处 `QueryBuilder::new(...)` ⇒ 门禁红（`18 → 19`，`FAIL: QueryBuilder 组装增加 1 处`），
> 还原后绿；把上限抬到 19（等价于实测值下降）同样放行 —— 即"只禁增不禁减"。
> ⇒ D-14 的三种形状（① 字面量绑定 / ② 跨函数传参 / ③ QueryBuilder）现已**全部有约束**。
>
> 实测（收紧当天）：生产区 `literal` **322 处不变**（同文件字面量绑定在生产区为 0；
> 该值随后由 C35a-0 的锁方法去重下调为 321），
> `runtime` 71 → **70**，新类 `param` **1**（即 ②）；测试区 3 处 `const` 绑定从 runtime 转 literal
> （`presence/mod.rs` 的三个 `PRESENCE_SELECT_BY_USER` 用例，test 区不入棘轮）。
> ⇒ 所谓 "baseline +N" 在生产区**没有发生**；收紧的实质是**新增了 `param` 这道此前不存在的约束**
> （此前 ② 类站点可以被静默新增），而不是把 literal 数字做大。自证见守卫的
> `same_file_literal_binding_is_classified_as_literal` / `enclosing_fn_parameter_is_classified_as_param`
> / `macro_call_argument_is_not_mistaken_for_a_parameter` / `param_ratchet_fails_on_a_new_param_site_in_an_unknown_file`。
> `synapse-common/src/transaction.rs:66` 的回收方向：三个调用点传的都是字面量，
> 把 `SET TRANSACTION ISOLATION LEVEL …` 分别内联到调用点即可宏化（牵动公共 API 形状，属独立设计事项）。

### 7.4 计数与口径

- 合计 **68** 条（D-01…D-68）：**未关闭 1**（D-57 部分）、
  **结构性例外 7**（D-13 / D-14 / D-18–D-22，有意不修）、**已关闭 60**（含 D-37 收敛、
  D-62 修法① 与 D-68 接线落地）。
- 本文档**只显示**未关闭项与结构性例外；已关闭项的明细在 HISTORY §7.2（冻结，不参与当前计数），
  阶段总结后关闭的 6 条（D-37 / D-62 / D-65 / D-66 / D-67 / D-68）在各提交信息里。
- "部分已修"指同一编号下仍有明确未做子项；结构性例外**不计入**待修，其约束力写在 §7.3 与 R1–R13。

### 7.5 处置约定（改 SQL / 查询前）

1. **先修再转，独立提交**（R12）—— 否则"编译器把改动证伪"这条证据链失效。
2. **登记唯一**（R13）—— 新问题只追加到 §7.1，不再开第二份清单。
3. **白名单只减不增**（R7）—— 允许保持动态的只有"动态标识符"与 `Vec<Option<T>>` 两类，
   且必须有 D-13/D-14 这样的登记条目。
4. **门禁必须自证能变红**（R11）—— 用故意违规证明它真的会失败，并把实验写进提交信息或本文档。

---

## 8. 优化方案

### 8.1 剩余可静态化清单（按实测，2026-09-26 C35a 后）

**可转换残量 312 处**（282 literal / 29 runtime / 1 param），头部按大小排（前 14）：

| 文件 | 处数 | 门控 | 备注 |
|---|---|---|---|
| `synapse-storage/src/burn_after_read.rs` | 15 | `burn-after-read` | 见 §8.3（需带 feature 的 CI 等价库） |
| `synapse-storage/src/event/basic.rs` | 11 | — | 其中 8 literal / 3 runtime；`event/` 同域；**动手前确认并发会话不在途**（v12 PDU 活跃区） |
| `synapse-storage/src/event/redaction.rs` | 10 | — | 同上 |
| `synapse-e2ee/src/secure_backup/service.rs` | 10 | — | 与 C25–C27 同域，可整批 |
| `synapse-e2ee/src/ssss/storage.rs` | 10 | — | 同上 |
| `synapse-storage/src/event/batch.rs` | 9 | — | `event/` 同域 |
| `synapse-storage/src/event/state.rs` | 9 | — | `event/` 同域；2026-09-25 刚被 PDU 投影改动过 |
| `synapse-e2ee/src/key_request/storage.rs` | 9 | — | 与 C25–C27 同域 |
| `synapse-storage/src/admin_media.rs` | 8 | — | 注意 U-3 的 hash 隔离查询 |
| `synapse-storage/src/email_verification.rs` | 8 | — | 单表模块 |
| `synapse-storage/src/event/dag.rs` | 8 | — | `event/` 同域 |
| `synapse-storage/src/federation_queue.rs` | 8 | — | 单表模块（与 `pruning` 同域） |
| `synapse-federation/src/event_broadcaster.rs` | 8 | — | `synapse-federation`，注意广播路径 |
| `synapse-federation/src/key_rotation.rs` | 8 | — | `synapse-federation`，注意密钥轮换路径 |

> **门控列的判据**：整文件在 `#[cfg(feature = …)]` 下时，`cargo sqlx prepare` 必须 `--all-features`
> （R2 的教训），且 DB 往返要在带该 feature 的 CI 等价库上跑 ⇒ 门控文件单列一批更省来回。

> **C35 剩余的 6 处**（`database_initializer/mod.rs`，已从"前 14"表退出）：B 类 2 处
> （锁 key / 取锁，可空性断言待**先修**）+ C 类 4 处（`CREATE INDEX` / `SET` / `ROLLBACK` ×2，
> utility 语句，需一次 `cargo sqlx prepare` 实测）—— 见 §8.3 第 2 条。

### 8.2 每批的标准流程

1. **侦察**：`--list-production-dynamic` 取站点；`grep` 确认零调用者（有则先按铁律 1 删）；
   **先查并发会话是否在途改这个文件**（`git status` + 最近提交）。
2. **先修**：撞到的既有缺陷（列名错、可空性不符、吞错、死代码、双实现）**独立提交**。
3. **转换**：`query!` / `query_as!` / `query_scalar!`；`RETURNING *` 展开（R3）；合成列与 UNION
   输出列按需断言（R4）；`&Option<T>` → `.as_deref()`/`.as_ref()`（R5）；`LIMIT $n` 的 `i32` →
   `i64::from(…)`；断言别名不要与 `ORDER BY` 列名冲突（R6）；tx/pool 双分支先收敛成
   `&mut PgConnection` 再写一次宏（R1）。
4. **门禁**（§8.5 四道 + fmt），并在**一次性 CI 等价库**上复跑该模块的真 baseline 往返。
5. **棘轮**：`dynamic_production` 下调、`static` 上调、literal 逐文件行删除；同批提交。
6. **文档**：§0.1/§0.2 数字更新；若有新发现则登记 §7。

### 8.3 三项需要额外条件的

1. **`burn_after_read.rs`（15，门控 `burn-after-read`）** —— `prepare` 必须 `--all-features`；
   DB 往返要在**带该 feature** 的一次性 CI 等价库上跑（`createdb` + `scripts/ci/prepare_test_db.sh`）。
2. **`database_initializer/mod.rs`（转换前 15 → 转换后剩 6）** —— D-14 归属**已判**（实参全是
   字面量，无 `format!` 拼装；文件无 feature 门控）。**按函数分类**（不再用会随 `fmt` 漂移的
   行号）：
   - **A 类 / 8 处（纯 DML/SELECT，可直接转）**：`check_cache_valid`、`update_init_timestamp`、
     `step_connection_test`（2 处）、`is_migration_executed`、`record_migration`、
     `release_migration_lock`、`run_runtime_migrations` 末尾的表数统计。
   - **B 类 / 2 处（可空性断言需理由）**：`migration_lock_key` 的
     `hashtext(current_database() || ':' || current_schema())` 与 `try_acquire_migration_lock`
     的 `pg_try_advisory_lock($1)` —— 函数结果无 NOT NULL 信息，宏推可空而调用方声明非 `Option`。
     `pg_try_advisory_lock` 恒非 NULL，可直接按 R4 断言；`hashtext` 有**真实边界**
     （`search_path` 为空时 `current_schema()` 为 NULL，现行 `let lock_key: i64` 会解码失败）
     ⇒ 先加 `COALESCE(…, current_database() || ':')` 再断言（属**先修**，独立提交）。
   - **C 类 / 4 处（utility 语句，需一次实测）**：`ensure_schema_migrations_table` 的
     `CREATE INDEX`、`run_runtime_migrations` 里的 `SET statement_timeout`、`ROLLBACK` ×2。
     全仓**没有**宏用在 utility 语句上的先例（grep 实测 0），而既有注释笼统写着
     "DDL 无法用 query! 静态化"——**该说法尚未逐条实测**。能 describe ⇒ 照转并修正那句注释；
     不能 ⇒ 按 R13 新登记一条结构性例外，本批只转 A/B。
   - ⚠️ 另有 **2 处吞错**（`step_connection_test` 的 `.ok().flatten()` 忽略 `version()` 失败、
     表数统计同样忽略失败）位于**日志/遥测**路径：按 R12「先修再转」需先判定
     "有意的 best-effort 还是缺陷"（判据：失败后是否有调用方据此做出错误决定）。

   ✅ **C35a-0 已完成（2026-09-26）** —— 该模块此前 13 条用例里 12 条是纯函数，唯一 DB 用例
   只覆盖 `ensure_schema_migrations_table` 且用的是**空 schema**（R9 禁止的形态）；
   `db_metadata` 缓存读写、advisory lock 取/放、`schema_migrations` 读写全无覆盖。
   本批补了 **5 条真 baseline（`IsolatedTestPool`）DB 往返用例**（缓存未命中→写入→命中、
   TTL 非正不命中、锁互斥+释放后可再取、`record_migration` 的 `ON CONFLICT` 覆盖路径、
   基线 schema 上 `ensure_schema_migrations_table` 幂等），并把锁协议抽成三个可测方法
   （`migration_lock_key` / `try_acquire_migration_lock` / `release_migration_lock`，行为不变）。
   **三条变异自证**：释放改 no-op ⇒ 锁用例红（"释放后必须能被另一会话取到"）；
   `ON CONFLICT … DO UPDATE` 改 `DO NOTHING` ⇒ upsert 用例红（checksum 未被覆盖）；
   `update_init_timestamp` 换 key ⇒ 缓存往返红。去重收益：`pg_advisory_unlock` 的两个同文本
   调用点合并为一处 ⇒ 生产动态 393 → **392**、本表该文件 15 → **14**。
   ✅ **C35a 已完成（2026-09-26，本批）** —— A 类 8 处全部宏化（`dynamic_production` 392 → **384**、
   `static` 1076 → **1084**、`.sqlx` 1045 → **1053**，+8 / 0 删除；`dynamic_test` 713 不变）。
   行为等价性由 C35a-0 的 5 条真 baseline 用例验证（`database_initializer` **31/31** 全绿）。
   转换时撞到 **3 个宏语义陷阱**，已按 R4 断言 + 写进 **AGENTS.md R6**：
   ① `version()` / `count(*)` 这类"无关系来源"结果按可空推断而调用方要非空 ⇒ `AS "version!"` /
   `AS "count!"`；② 断言后 `fetch_optional` 仍给 `Result<Option<T>>`，`.ok()` 再包一层 ⇒ 必须
   `.ok().flatten()`（三层 Option 的坑，用 `let _probe: () = …fetch_one(…)` 打印推断类型才定位到）；
   ③ **单列语句的 `query!` 生成 `Map`，没有 `.execute()`** ⇒ 改 `query_scalar!` + `fetch_one`。
   ⇒ 剩余 **C35b**：B 类 2 处（先修 `COALESCE` 后断言）+ C 类 4 处（utility 语句一次实测），
   以及 2 处吞错的定性（有意的 best-effort 还是缺陷）。

3. **D-57②（seed 侧收敛 `public`）** —— 见 §7.2：需"枚举 baseline 对象集 + 对多出来的对象逐个
   DROP"式设计，不能简单 `RESET_PUBLIC=1`。

### 8.4 收尾条件（何时可称"静态化战役结束"）

- `dynamic_production` 的**可转换部分归零**：384 → **72**（只剩测试基建 57 + 结构性 15），
  或每个残留都有 §7.3 那样的登记条目；
- literal 逐文件表只剩 4 类（3 个测试基建文件 + `event/pagination.rs`）；
- **D-57② 落地**（D-62 已落地、D-37 已收敛、**D-68 已接线并加保留期** ⇒ §7 只剩 D-57②）；
- 四道门禁与两道棘轮在 CI 常驻，且都留有"能变红"的自证记录。

### 8.5 每批必须跑的门禁

命令与解释见 `AGENTS.md` **R8**（四道，缺一不可：ratio 棘轮、`check_sqlx_cache_fresh.sh --compile`、
两档 clippy、该模块的真 baseline 往返；收尾 `./scripts/check_fmt_ratchet.sh`）。两条易漏项：

- **该模块的 db_tests 可能在 feature 门控后** —— 先跑一次 `--all-features` 版本对照，
  否则会"跑少了却看起来通过"（`voice.rs` 实测）。
- **`CARGO_TARGET_DIR` 不得跨 worktree 共享** —— 会跨树复用产物，既假红也假绿（D-66）。

---

## 附录 A　历史与编号映射

| 想找什么 | 去哪里 |
|---|---|
| 已关闭缺陷（D-01…D-64 中已关闭者）的逐条明细 | HISTORY **§7.2**（冻结快照） |
| 各批次执行记录（C1–C34 / W1–W5，含侦察/手法/门禁/棘轮/提交清单） | HISTORY **§8.x** |
| 计划初稿（影响、Phase A–D、陷阱与反例、工作量与顺序） | HISTORY **§2–§5** |
| 2026-09-23 的实测分布与残差清单快照 | HISTORY **§1 / §3** |
| 规则全文（R1–R13、反冗余铁律、已知坑） | `AGENTS.md` |
| 棘轮基线与逐段理由 | `scripts/ci/sqlx_dynamic_ratio_baseline`、`scripts/ci/sqlx_literal_production_baseline`、`scripts/ci/sqlx_param_production_baseline`（D-14 传参棘轮） |
