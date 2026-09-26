# SQLx 静态化：阶段总结与剩余工作（2026-09-23 启动 · 2026-09-25 阶段总结）

> **本文档现在只保留三样东西**：阶段总结（§0）、**仍存在的问题**（§7，唯一登记处）、
> **优化方案**（§8）。已经解决的问题不再在本文档显示。
>
> **已关闭内容的去向**：64 条登记缺陷里已关闭的 51 条（逐条明细）与 C1–C33 / W1–W5
> 各批次执行记录，**逐字保存在**
> [`docs/synapse-rust/archive/SQLX_STATICIZATION_PLAN_2026-09-23_HISTORY.md`](../synapse-rust/archive/SQLX_STATICIZATION_PLAN_2026-09-23_HISTORY.md)
> —— 该冻结快照保留原始编号，因此代码注释与提交信息里对 **§7.2 D-xx** / **§8.x** 的引用
> 仍然可查。规则本身沉淀在 `AGENTS.md` 的 **R1–R13**（索引见 §0.3）。

---

## 0. 阶段总结

### 0.1 战果（唯一数字口径，2026-09-25 C33 后实测）

| 指标 | 战役起点（2026-09-23） | 现在 | 变化 |
|---|---|---|---|
| `dynamic_production` | 1532（近似） | **421** | **−72.5%** |
| `static` | 61 | **1048** | +987 |
| `dynamic`（总） | 2151 | **1132** | −1019 |
| 静态占比 | 2.76% | **48.1%**（1048 / 2180） | +45.3pp |
| `.sqlx` 离线缓存 | 60 条 | **1016 条** | +956 |
| literal（逐文件棘轮，处 / 文件） | 876 / 98 | **350 / 63** | −526 |
| `query_builder`（白名单，单列统计） | — | 18 | — |
| `runtime` 残差 | — | 71 / 14 文件 | — |

**残量结构（这是"还剩多少活"的准确说法）**：

| 组成 | 处数 | 性质 |
|---|---|---|
| **可静态化残量** | **349** | 其中 **319 处是字面量**（纯机械转换）、**30 处是运行期拼装**（`format!` 拼列清单/排序方向等，需结构性替代） |
| 测试基建（有意保留） | 57 | `synapse-test-utils/src/lib.rs` 28、`synapse-common/src/test_isolation.rs` 25、`test_schema_guard.rs` 4 —— 无条件编译、按 D-13/D-14 保持动态 |
| 结构性保留（有意） | 15 | `synapse-storage/src/event/pagination.rs`（9 runtime 游标/`ORDER BY` 方向 + 6 literal） |
| **合计** | **421** | = 349 + 57 + 15 |

### 0.2 复现（唯一入口，勿手工数）

```bash
# 总量 / 分区 / 静态占比
python3 scripts/ci/sqlx_query_census.py

# 棘轮（唯一入口，内部调用上面的 census；生产动态不得增、静态不得减）
bash scripts/ci/check_sqlx_dynamic_ratio.sh

# 逐文件明细（生产区动态站点：`path:line:kind`，kind ∈ literal|runtime）
python3 scripts/ci/sqlx_query_census.py --list-production-dynamic .

# literal 棘轮输入（逐文件计数；与 scripts/ci/sqlx_literal_production_baseline 逐行比对）
python3 scripts/ci/sqlx_query_census.py --list-production-dynamic . \
  | grep ':literal$' | cut -d: -f1 | LC_ALL=C sort | uniq -c \
  | awk '{print $2"\t"$1}' | LC_ALL=C sort
```

### 0.3 战役留下了什么（比数字更耐用的资产）

1. **判据（`AGENTS.md` §SQLx 静态化规则 R1–R13）** —— 全部来自实测，不是推导：

   | 规则 | 一句话 | 关键实测判据 |
   |---|---|---|
   | R1 | 新增/改动的 SQL 一律宏化 | **宏的 SQL 实参必须是调用点字面量**；"藏进变量"会同时骗过两道门禁 |
   | R2 | 改了查询文本 ⇒ 同提交带 `.sqlx` 增量 | 离线缓存缺条目 = **整条 CI 编译失败**；`prepare` 必须 `--all-features` |
   | R3 | 宏内禁止 `RETURNING *` / `SELECT *` | 多列 E0560 / 少列 E0063 |
   | R4 | 可空性三选一（对齐 / 收紧 schema / 写清谁保证） | 无来源表达式与 UNION 结果被推可空；**反向不报错**（`Option` 字段不能当可空性证据） |
   | R5 | 绑定表达式 | `&Option<T>` 被拒 ⇒ `.as_deref()`/`.as_ref()`；`LIMIT $n` 按 `bigint` 定型 |
   | R6 | 别名 | 不认 `#[sqlx(rename)]`；`AS "col!"` 必须配 `r#"…"#`；**断言别名即真实列名** |
   | R7 | 例外白名单只减不增 | 动态标识符 + `Vec<Option<T>>` 两类，须有登记条目 |
   | R8 | 每批四道门禁 | 见 §8.5 |
   | R9 | 测试侧 | 真 baseline（`IsolatedTestPool`）；存在性断言锚定 `current_schema()` |
   | R10 | schema 变更连带清单 | 指纹自检 / 守卫 5 / 契约用例 / 缓存重建 |
   | R11 | 门禁自身必须可信 | 新门禁要用故意违规自证能变红 |
   | R12 | 先修再转，禁止夹带 | 既有缺陷独立提交，否则"编译期证伪"的证据链失效 |
   | R13 | 登记与计数唯一 | 新问题只进 §7，不要第二份清单 |

2. **四道门禁 + 两道棘轮**（都自证过能变红，自证记录见 HISTORY 的 §7.2 与各 §8.x）：
   ratio 棘轮（生产动态不得增 / 静态不得减）、literal 逐文件棘轮（新文件里写一个字面量也会红）、
   `check_sqlx_cache_fresh.sh --compile`（权威档）、两档 clippy（`-D warnings`）。
3. **登记制度**：所有缺陷汇集到 §7，状态计数只有一套口径（曾是双份计数漂移的受害者，D-16）。

### 0.4 已经解决的（只给类别与去向，不再逐条显示）

已关闭 **51 条**，明细见 HISTORY §7.2。类别分布（含十余条"真 schema 下必然失败"）：

- **真 schema 下必然失败**：列名写错、INSERT 漏 NOT NULL 列、两个已注册管理路由背靠**不存在的表**、
  `sent_at` 从不写入导致清理恒删 0 行、`WHERE $2 != '[]'` 对 `text[]` 在 prepare 阶段就报 22P02…
- **门禁自身失效 6 条**（D-50 / D-51 / D-52 / D-56 / D-57① / D-60）：红着的门禁与假绿的门禁。
- **死代码 / 第二份实现 / 吞错 / 缓存未失效**：`save_device_key` 的第二份写入实现、E-12 遗留死观测面、
  `unwrap_or_default` 吞错、`.ok().flatten()` 吞错、`let _ = <future>` 让缓存失效从未执行…
- **可空性与类型不符**（含靠收紧 schema 关掉的）：`key_backups.version`、`olm_sessions.message_index`、
  `rendezvous_session.content` 等。
- **文档级漂移 / 计数不一致**：双份计数、过时的"不变"表述、落后的状态行。

### 0.5 阶段结论

1. 动态 SQL 已从**系统性风险**降为**局部清单**：421 处里 72 处是有意保留（测试基建 + 结构性），
   真正待收的是 **349 处**，且其中 319 处是纯机械的字面量转换。
2. **收益性质变了**：早期批次每批都在挖"真 schema 下必败"的硬缺陷；现在批次更多是机械收敛，
   并且每批都顺手清理掉一类残留（最近三批：C31 清理 `FromRow` 死代码、C32 消掉手工
   `Row::get` 解码、C33 消掉 `PgRow` 泄漏与 10 处吞错）。
3. **长期资产是规则与门禁，不是数字**：数字会随并发改动漂移，R1–R13 与四道自证过的门禁
   才是"不再制造同类缺陷"的保证。

---

## 7. 仍存在的问题（唯一登记处）

**只登记未关闭项 + 结构性保留。** 已关闭项（51 条）的明细在 HISTORY §7.2，冻结不再更新。
新发现的问题追加到本表，状态计数以 §7.4 为准。

### 7.1 汇总表（未关闭）

| 编号 | 类别 | 位置 | 问题 | 状态 | 影响 | 下一步 |
|---|---|---|---|---|---|---|
| **D-62** | 响应形状与 schema 不符 | `synapse-services/src/client_push_service.rs::get_notifications`；`synapse-web/src/routes/handlers/room/events.rs:165` | 两条读路径都把 `notifications.notification_type` 渲染进 JSON 键 `profile_tag`，而表里另有一列**真 `profile_tag` 从未被 SELECT**；规范里 `profile_tag` 指"命中的推送规则的 profile tag"，不是通知类型。两处独立实现一致 ⇒ 更像产品选择而非笔误 | **未修（待产品裁定）** | 客户端读到的 `profile_tag` 语义与规范不一致；真 `profile_tag` 列的**读路径为零**（写入侧仍在写） | 二选一：① SELECT 真 `profile_tag` 填该键并**另加** `notification_type` 键（信息超集，最贴规范）；② 若确要让该键承载通知类型，则改名并同步 `events.rs`。`PushStorage::get_notifications` 已返回含两个字段的 `NotificationRow`，任一修法都只是改一行 JSON 组装 |
| **D-57** | 测试基建假绿 | `tests/integration/mod.rs::require_test_pool()`（search_path = `<clone>, public`）× `scripts/ci/prepare_test_db.sh:79`（`RESET_PUBLIC=0` 增量套 baseline）× `to_regclass($1)` 走 search_path 解析 | baseline 是 `CREATE TABLE IF NOT EXISTS` 合并脚本、**不含 DROP** ⇒ 从 baseline 删掉的表仍留在长期库的 `public` 里，断言"表存在/可用"的用例会**假绿**并**污染共享 `public` 而不自知** | **部分已修**（① 已做，② 未做） | 本地验证结论可能与 CI 不一致（CI 全新库无此问题） | ① 已完成：三处裸 `to_regclass($1)` 改为锚定 `current_schema()`，`tests/` 内已无裸 `to_regclass`。② **未做**：让 seed 对 `public` 也做**收敛**（`RESET_PUBLIC=1` 或对已删对象补 `DROP … IF EXISTS`）。注意 `RESET_PUBLIC=0` 的动机：`DROP SCHEMA public CASCADE` 会连带删掉**依赖 public 扩展**的其它 schema 对象 ⇒ ② 不能简单改成 1，需独立设计 |
| **D-37** | 冗余实现（铁律 2） | `synapse-storage/src/device/mod.rs:182` vs `synapse-e2ee/src/device_keys/storage.rs` | `DeviceStorage::record_device_list_change` 是**第二份** device-list-change 实现（两份 SQL 逐字相同，分属不同 crate 的不同类型） | **部分已修**（吞错与死包装已修） | 两份实现长期并存 ⇒ 改一处漏一处（正是铁律 2 要消除的形状） | 收敛成一份：需要共享位置（`synapse-common`，或让 storage 侧成为唯一实现并由 e2ee 侧委托），属**独立设计事项**，不是静态化批次 |

### 7.2 逐条明细（只写未关闭项）

#### D-62 通知响应把 `notification_type` 渲染成 `profile_tag` 键（待裁定）

- 事实：`notifications` 表**同时**有 `notification_type VARCHAR(50) DEFAULT 'message'` 与
  `profile_tag VARCHAR(255)`（`v12:1495-1496`）；`PushStorage::get_notifications` 的 SQL
  **只** SELECT `notification_type`；两条读路径都把它放进 JSON 键 `profile_tag`。
- 为什么没有当场改：两处**独立**实现写了同一个映射（`events.rs` 那条路径的
  `RoomNotification` 根本没有 `profile_tag` 字段），改它属于**响应形状变更**，
  按 R12（禁止夹带）不在静态化批次里做。C33 只做类型化、**行为保持**，并在代码处留注释指向本条。
- 需要谁裁定：产品/接口 owner。任一候选都只改 JSON 组装的一行。

#### D-57② seed 侧对 `public` 的收敛（① 已完成）

- ①（已完成）：`schema_contract_p0_tests_migrated::assert_table_exists`、
  `db_schema_smoke_tests_migrated::assert_table_exists` / `assert_view_exists` 三处改为
  `to_regclass(format('%I.%I', current_schema(), $1))::text`。**自证留了两步判据**：
  机制（陈旧 `public` 里造探针表 ⇒ 旧口径非空会假绿、新口径 NULL 如实报缺失）+
  用例能变红（把探针表加进 P0 清单 ⇒ FAIL 报 `… to exist in the current schema, got: None`，
  随后逐字节还原）。
- ②（未做）：`scripts/ci/prepare_test_db.sh` 对 `public` 用 `RESET_PUBLIC=0` 增量套 baseline
  ⇒ 长期库的 `public` 会一直保留已从 baseline 删除的对象。目标：让 seed 收敛 `public`
  （对"已从 baseline 删除的对象"补 `DROP … IF EXISTS`，或在能保证不误删依赖扩展对象的前提下
  用 `RESET_PUBLIC=1`）。
- **为什么不能简单改 `RESET_PUBLIC=1`**：`DROP SCHEMA public CASCADE` 会连带删掉
  **依赖 public 扩展**的其它 schema 对象（脚本注释已说明该动机）⇒ 需独立设计，
  例如"先枚举 baseline 当前对象集，再对 `public` 里多出来的对象逐个 DROP"。
- 验收判据（建议）：在长期库上跑一次 seed，`public` 中**只应存在** baseline 与扩展所需的对象；
  用故意残留（手工造一张表）证明收敛确实会删它。

#### D-37 两份 device-list-change 实现的收敛

- 已修部分：两个 `*_best_effort` 包装（零调用者）已删；3 处 `let _ = …` 吞错已按
  "重试能否自愈"定策（display-name 两处改 `?`、删除类四处改 `tracing::warn!`）。
- 未修部分：**两份实现**（storage 侧 `DeviceStorage::record_device_list_change` 与 e2ee 侧）
  仍未收敛。两份语句逐字相同（`device_lists_stream` + `device_lists_changes`）。
- 收敛要点：两处的类型/事务边界不同（一个在 `synapse-storage`，一个在 `synapse-e2ee`），
  需要选一个共享位置并把另一侧改为委托；同时确认 `device_lists_changes` 的写入语义
  （同一事务内可见性）在收敛后不变。属**独立设计事项**，可以单独一批做。

### 7.3 结构性保留（有意，不修，但必须遵守）

这些不是"待修缺陷"，而是**当前工具/接口的边界**，它们决定了批次里"哪些站点允许保持动态"。

| 编号 | 边界 | 出现位置 | 怎么办 |
|---|---|---|---|
| D-13 | `Vec<Option<T>>` 元素可空数组**无 sqlx 映射** ⇒ 整条语句必须保持动态（SQL 文本本身是字面量） | `room_summary/repository.rs:332`、`:579`；`presence/mod.rs:232` | 保持动态；回收方向：并行数组 → 单个 `jsonb_to_recordset($n)`（独立改造，本计划不排期） |
| D-14 | 运行期拼装 SQL（`format!` 拼列清单/`ORDER BY` 方向等）**有意保留**；另含**守卫的已知覆盖缺口**（见下） | `space/repository.rs:572/626`、`user/storage.rs:989/1233`、`src/server/database.rs:43-45`、`event/state.rs`、`membership/mod.rs`、`event/basic.rs`、`event/batch.rs`、`maintenance.rs`、`state_groups.rs`、`event/dag.rs` 等（合计 `runtime` 30 处属可转换面之外） | 保持动态；逐文件回收方向见 HISTORY §7.2 D-14；批次里**不要**为了压数字把动态标识符硬编码 |
| D-18 | 仅排序用列无对应结构体字段 ⇒ 用子查询包裹 | `thread/storage.rs:864` | 沿用子查询写法，不要新增"只为排序加字段"的结构体 |
| D-19 | `query_as!` **不认** `#[sqlx(rename)]` / `#[sqlx(skip)]` | `event_report/models.rs:29`、`module.rs:255` 等 | 在 SQL 里显式写别名 / 合成 `NULL` 列 |
| D-20 | LEFT JOIN 外侧列被 PG 透传为 NOT NULL ⇒ sqlx 误推非空 | C6/C8/C9/C13 多处 | 用 `AS "col?"` 覆盖（见 R4） |
| D-21 | 宏 `ty_match` 拒绝 `&Option<T>` 绑定 | `device_keys/storage.rs:121` 等 | `.as_deref()` / `.as_ref()`（见 R5） |
| D-22 | `query_as!` 不走 `FromRow`，`RETURNING *` 必须展开 | 多个批次 | 机械展开为显式列清单（见 R3） |

> **D-14 的守卫覆盖缺口（仍未收紧）**：literal 棘轮只看调用点的实参 token 形态，
> 因此**把字面量绑到别处再传进来**的写法会被归为 `runtime` 而绕过它。三种已知形状：
> ①同文件 `const`/`let` 字面量绑定；②**跨函数传参**
> （实例：`synapse-common/src/transaction.rs:66` 的 `statement: &'static str`，
> 调用方 `begin_serializable` 传的是字符串字面量）；③`QueryBuilder` 组装（计入 `query_builder=18`，
> 两侧棘轮都不计）。三种都**不会**绕过 ratio 棘轮（它们照样计入 `dynamic_production`），
> 只绕过 literal 棘轮。收紧方向：在守卫的 `iter_dynamic_sites` 里对同文件建立
> `name → 是否字面量绑定` 表（跨函数传参需要额外设计）。**注意**：收紧会让这些站点从
> `runtime` 变 `literal`，baseline 相应 +N —— 这是**加大**约束，不是放宽。

### 7.4 状态计数与口径

**状态计数（2026-09-25，C33 后）**：已修 **51** / 部分已修 **2**（D-37、D-57）/
未修 **1**（D-62）/ 结构性保留 **7**（D-13、D-14、D-18–D-22）/ 文档级已处置 **3**（D-16、D-23、D-26）
= 合计 **64**（D-01…D-64）。

口径说明（唯一计数，避免 D-16 型双份漂移）：

- 本文档 §7 **只显示未关闭项（3 条）与结构性保留（7 条）**；已关闭的 51 条明细在 HISTORY §7.2，
  该快照冻结、不参与当前计数。
- "已修 / 部分已修 / 未修"的分类只按**缺陷是否真的关掉**判定；
  "部分已修"指同一编号下仍有未做的明确子项（D-57②、D-37 的收敛）。
- 结构性保留是**有意不做**（工具有边界），不计入"待修"，但它们的约束力写在 §7.3 与 R1–R13 里。

### 7.5 处置约定（改 SQL / 查询前）

1. **先修再转，独立提交**（R12）：静态化是行为保持的机械重构；转换中撞到的既有缺陷单独提交，
   否则"编译器把改动证伪"这条证据链失效。
2. **登记唯一**（R13）：新问题只追加到 §7.1，不要在别处再开清单。
3. **白名单只减不增**（R7）：允许保持动态的只有"动态标识符"与 `Vec<Option<T>>` 两类，
   且必须有 D-13/D-14 这样的登记条目。
4. **门禁必须自证能变红**（R11）：新增/修改门禁，用故意违规证明它真的会失败，
   并把实验写进提交信息或本文档（历史批次记录见 HISTORY §8.x）。

---

## 8. 优化方案

### 8.1 剩余可静态化清单（按实测排序，2026-09-25）

排除 4 类（测试基建 3 文件 57 处、`event/pagination.rs` 15 处）后，**可转换残量 349 处**
（literal 319 / runtime 30）。按文件排（前 15）：

| 文件 | 处数 | 门控 | 备注 |
|---|---|---|---|
| `synapse-storage/src/burn_after_read.rs` | 15 | `burn-after-read` | 见 §8.3（需带 feature 的 CI 等价库） |
| `synapse-services/src/database_initializer/mod.rs` | 15 | — | 见 §8.3（需先判 D-14 归属） |
| `synapse-storage/src/event/basic.rs` | 11 | — | 与 `event/` 同域，注意并发会话活跃区 |
| `synapse-storage/src/event/redaction.rs` | 10 | — | 同上 |
| `synapse-storage/src/dehydrated_device.rs` | 10 | `privacy-ext`? | 单表模块，风险低 |
| `synapse-e2ee/src/ssss/storage.rs` | 10 | — | 与 C25–C27 同域 |
| `synapse-e2ee/src/secure_backup/service.rs` | 10 | — | 同上 |
| `synapse-storage/src/pruning.rs` | 9 | — | 单表模块；注意既有保留期清理测试 |
| `synapse-storage/src/invite_blocklist.rs` | 9 | — | 单表模块 |
| `synapse-storage/src/event/state.rs` | 9 | — | `event/` 同域 |
| `synapse-storage/src/event/batch.rs` | 9 | — | `event/` 同域 |
| `synapse-e2ee/src/key_request/storage.rs` | 9 | — | 与 C25–C27 同域 |
| `synapse-storage/src/federation_queue.rs` | 8 | — | |
| `synapse-storage/src/event/dag.rs` | 8 | — | `event/` 同域 |
| `synapse-storage/src/email_verification.rs` | 8 | — | |

> **门控列的判据**：整文件在 `#[cfg(feature = …)]` 下时，`cargo sqlx prepare` 必须
> `--all-features`（R2 的教训：枚举 feature 会漏条目），且 DB 往返要在带该 feature 的
> CI 等价库上跑；因此门控文件单列一批更省来回。

### 8.2 每批的标准流程（照抄即可）

1. **侦察**：`--list-production-dynamic` 取该文件站点；`grep` 确认零调用者（有则先按铁律 1 删）。
   **先查并发会话是否在途改这个文件**（`git status` / 最近提交）。
2. **先修**：撞到的既有缺陷（列名错、可空性不符、吞错、死代码、双实现）**独立提交**。
3. **转换**：`query!` / `query_as!` / `query_scalar!`；`RETURNING *`/`SELECT *` 展开（R3）；
   合成列与 UNION 输出列按需断言（R4）；`&Option<T>` → `.as_deref()`/`.as_ref()`（R5）；
   `LIMIT $n` 的 `i32` → `i64::from(…)`；断言别名不要与 `ORDER BY` 的列名冲突（R6）。
   tx/pool 双分支先收敛成 `&mut PgConnection` 再写一次宏（R1）。
4. **门禁**（§8.5 四道 + fmt），并在一次性 CI 等价库上复跑该模块的真 baseline 往返。
5. **棘轮**：`dynamic_production` 下调、`static` 上调、literal 逐文件行删除；同批提交。
6. **文档**：§0 数字表更新（若口径变化）、§7 若有新发现则登记、提交清单。

### 8.3 三项需要额外条件的

1. **`burn_after_read.rs`（15，门控 `burn-after-read`）** —— 整文件在 feature 门控下：
   `prepare` 必须 `--all-features`；DB 往返要在**带该 feature** 的一次性 CI 等价库上跑
   （`createdb` + `scripts/ci/prepare_test_db.sh`）。成本高于普通批次，但不难。
2. **`database_initializer/mod.rs`（15）** —— 先判 **D-14 归属**：该文件是 DDL/初始化类动态 SQL，
   可能整体属于"运行期拼装标识符"白名单（若是，应登记为结构性保留而不是硬转）。
   判定方法：逐站点看实参是不是 `format!` 拼出来的表名/列清单；只有字面量的才转。
3. **D-57②（seed 侧收敛 `public`）** —— 见 §7.2：不能简单用 `RESET_PUBLIC=1`
   （会连带删掉依赖 public 扩展的其它 schema 对象），需要"枚举 baseline 对象集 + 对多出来对象
   逐个 DROP"式的独立设计。

### 8.4 收尾条件（何时可以说"静态化战役结束"）

- `dynamic_production` 的**可转换部分归零**：即 421 → **72**（只剩测试基建 57 + 结构性 15），
  或每个残留都有 §7.3 那样的登记条目；
- literal 逐文件表只剩那 4 类（3 个测试基建文件 + `event/pagination.rs`）；
- **D-62 有裁定并落地、D-57② 落地、D-37 收敛**（§7 只剩结构性保留）；
- 四道门禁与两道棘轮在 CI 常驻，且都留有"能变红"的自证记录。

### 8.5 每批必须跑的门禁（R8）

```bash
# 1) 棘轮：生产动态不得增、静态不得减
bash scripts/ci/check_sqlx_dynamic_ratio.sh
# 2) 离线缓存完整性（权威 —— 不要只跑 --static）
bash scripts/ci/check_sqlx_cache_fresh.sh --compile
# 3) 两档 clippy（第二个入口才编译 integration 等 target，两档不可互相替代）
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
# 4) 该模块的「真 baseline」DB 往返（不是纯构造/序列化用例）
cargo nextest run -p <crate> --lib --features test-utils -E 'test(/<module>/)'
# 收尾
./scripts/check_fmt_ratchet.sh
```

> ⚠️ 两条易漏：**该模块的 db_tests 可能在 feature 门控后**（先跑一次 `--all-features` 版本对照，
> 否则会"跑少了却看起来通过"）；**integration 目标必须 `--all-features`**（否则快照/ledger 类用例假失败）。

---

## 附录 A　历史与编号映射

| 想找什么 | 去哪里 |
|---|---|
| 已关闭缺陷（D-01…D-61、D-63、D-64）的逐条明细 | HISTORY **§7.2**（冻结快照） |
| 各批次执行记录（C1–C33 / W1–W5、每条含侦察/手法/门禁/棘轮/提交清单） | HISTORY **§8.x** |
| 计划初稿（影响、Phase A–D 方案、陷阱与反例、工作量与顺序） | HISTORY **§2–§5** |
| 2026-09-23 的实测分布与残差清单快照 | HISTORY **§1 / §3** |
| 规则全文（R1–R13、反冗余铁律、已知坑） | `AGENTS.md` |
| 棘轮基线与逐段理由 | `scripts/ci/sqlx_dynamic_ratio_baseline`、`scripts/ci/sqlx_literal_production_baseline` |
