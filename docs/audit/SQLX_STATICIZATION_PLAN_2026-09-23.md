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
| `dynamic_production` | 1532（近似） | **393** | **−74.3%** |
| `static` | 61 | **1074** | +1013 |
| `dynamic`（总） | 2151 | **1104** | −1047 |
| 静态占比 | 2.76% | **49.3%**（1074 / 2178） | +46.6pp |
| `.sqlx` 离线缓存 | 60 条 | **1043 条** | +983 |
| literal（逐文件棘轮，处 / 文件） | 876 / 98 | **322 / 60** | −554 |
| `param` 传参（D-14 新棘轮，处 / 文件） | — | **1 / 1** | 新立棘轮（此前混在 `runtime`，两道棘轮都不管） |
| `runtime` 残差 / `query_builder`（白名单） | — | 70 / 13 文件 · 18 | — |

### 0.2 残量结构（"还剩多少活"的准确说法）

| 组成 | 处数 | 性质 |
|---|---|---|
| **可静态化残量** | **321** | **291 处字面量**（纯机械转换）+ **29 处运行期拼装**（`format!` 拼列清单/`ORDER BY` 方向等，需结构性替代）+ **1 处跨函数传参**（`param`，把字面量内联到调用点即可转），见 §7.3 D-14 |
| 测试基建（有意保留） | 57 | `synapse-test-utils/src/lib.rs` 28、`synapse-common/src/test_isolation.rs` 25、`test_schema_guard.rs` 4 |
| 结构性保留（有意） | 15 | `synapse-storage/src/event/pagination.rs`（9 runtime 游标/排序方向 + 6 literal） |
| **合计** | **393** | = 321 + 57 + 15 |

### 0.3 复现（唯一入口，勿手工数）

```bash
python3 scripts/ci/sqlx_query_census.py                     # 总量 / 分区 / 静态占比
bash scripts/ci/check_sqlx_dynamic_ratio.sh                 # 棘轮（内部调用上面的 census）
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
| ⑨ 阶段总结后新发现并已关闭 | 4 | D-62（通知响应的 `profile_tag` 键取自 `notification_type` ⇒ 已按修法① 改成真列 + 独立 `notification_type` 键）、D-65（并发改动只改一半 ⇒ 集成+clippy 双红）、D-66（worktree 共享 `CARGO_TARGET_DIR` ⇒ 跨树复用产物，假红/假绿）、D-67（新增测试里的死常量让 clippy 红） |
| ⑩ **本表新登记的未修项** | **1** | **D-68**（通知记录层没有生产写入者、也没有清理 ⇒ 三个已注册端点恒为空/恒失败，见 §7.1） |

**去向**：阶段总结前关闭的 57 条逐条明细在 HISTORY §7.2；总结后关闭的 5 条（D-37 / D-62 / D-65 /
D-66 / D-67）记在各自提交信息里（下次阶段总结时并入快照）。本表 ①–⑧ 是**发现时**的归类
（历史口径，不随修复变动），因此 D-57 仍计入 ⑥、D-37 仍计入 ④、D-62 已改判为"已修" ——
"还剩哪些没修"看结论行与 §7.1，不看桶号。
**结论：59 已关闭 / 2 未关闭（D-57 部分、D-68）/ 7 结构性例外。**

### 0.5 阶段结论

1. 动态 SQL 已从**系统性风险**降为**局部清单**：393 处里 72 处有意保留，待收 **321 处**，
   其中 291 处是纯机械转换。
2. **收益性质变了**：早期批次每批都在挖"真 schema 下必败"的硬缺陷（① 类 15 条）；
   现在批次以机械收敛为主，并顺手清理一类残留（C31 清 `FromRow` 死代码、C32 消手工 `Row::get`、
   C33 消 `PgRow` 泄漏与 10 处吞错、C34 消 `Row` 解码与死 derive）。
3. **长期资产是规则与门禁，不是数字**：数字会被并发改动推动，R1–R13 与四道自证过的门禁才是
   "不再制造同类缺陷"的保证；本阶段新增的两条规则（宏实参须为调用点字面量、worktree 各自 target 目录）
   都来自实测而非推导。

---

## 7. 仍存在的问题（唯一登记处）

只登记**未关闭项**与**结构性例外**。已关闭项不在此显示（去向见 §0.4）。

### 7.1 汇总表

| 编号 | 类别 | 位置 | 问题 | 状态 | 下一步 |
|---|---|---|---|---|---|
| **D-68** | **产品缺陷（端点空壳 + 无清理）** | `notifications` 表（`v12:1489`）× `synapse-storage/src/push/mod.rs::get_notifications`/`ack_notification` × `push_notification.rs::get_room_notifications` × 三个已注册端点（`GET /_matrix/client/v3/notifications`、`POST …/{id}/ack`、`GET …/rooms/{room_id}/notifications`） | 全仓**没有任何生产写入者**：唯一的 `INSERT INTO notifications` 在 `push/mod.rs` 的 `db_tests` 里，迁移里也没有触发器；`PushService::send_notification` 只写 `push_notification_queue`/`push_notification_log`。⇒ 上述端点**恒返回空列表**、`ack` 恒失败（实测：唯一的端到端断言就是 `notifications == []`）。另外该表**不在任何 pruning/retention 覆盖内** ⇒ 一旦接线会重演 D-33 的无界增长 | **未修（待裁定/排期）** | 二选一：① **接线**——在"推送规则命中、决定给该用户产生通知"的决策点补一条 `record_notification` 写入，**同批**在 `pruning.rs` 加保留期清理（阈值对齐 `push_notification_log`）；② **明确声明为 stub**——按铁律 1 删掉该表与两个读方法（`/notifications` 仍可按规范返回空列表）。**建议 ①**：路由是规范稳定面、客户端会调用，且决策点已存在（成本 = 一条 INSERT + 一条清理） |
| **D-57** | 测试基建假绿 | `tests/integration/mod.rs::require_test_pool()`（search_path = `<clone>, public`）× `scripts/ci/prepare_test_db.sh:79`（`RESET_PUBLIC=0`）× `to_regclass($1)` 走 search_path | baseline 是 `CREATE TABLE IF NOT EXISTS` 合并脚本、**不含 DROP** ⇒ 从 baseline 删掉的表仍留在长期库 `public` 里，"表存在/可用"类断言**假绿**且污染共享 `public` | **部分已修**（① 已做，② 未做） | ② 让 seed 对 `public` 也收敛（对已从 baseline 删除的对象补 `DROP … IF EXISTS`，或在不误删依赖扩展对象的前提下 `RESET_PUBLIC=1`）。**不能简单改成 1**：`DROP SCHEMA public CASCADE` 会连带删掉依赖 public 扩展的其它 schema 对象 |

### 7.2 逐条明细

**D-68**：证据链 ——（1）`grep -rn 'INSERT INTO notifications' --include=*.rs .` 全仓只有
`synapse-storage/src/push/mod.rs:604`（`mod db_tests` 内的夹具助手）；（2）迁移里没有写该表的
触发器/函数；（3）`PushService::send_notification` 的落点是 `push_notification_queue`
（+ `push_notification_log`），与 `notifications` 无交集；（4）唯一端到端断言
`api_enhanced_features_tests::test_push_routes_share_across_r0_and_v3` 断的正是
`notifications == []`；（5）`pruning.rs`/`retention.rs` 都不含该表。
⇒ 这不是"键名映射"问题，而是**记录层整体未接线**。建议按修法① 接线并同批加清理；
若产品决定不做通知收件箱，则按② 删掉假接口（铁律 1）。

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
> ③ **`QueryBuilder` 组装**（`query_builder=18`）⇒ 仍只统计不设棘轮，因为它的 SQL 文本
> 确由运行期决定，属 R7 白名单；**唯一残留缺口**，待后续决定是否加"只增不禁"的计数棘轮。
>
> 实测（收紧当天）：生产区 `literal` **322 处不变**（同文件字面量绑定在生产区为 0），
> `runtime` 71 → **70**，新类 `param` **1**（即 ②）；测试区 3 处 `const` 绑定从 runtime 转 literal
> （`presence/mod.rs` 的三个 `PRESENCE_SELECT_BY_USER` 用例，test 区不入棘轮）。
> ⇒ 所谓 "baseline +N" 在生产区**没有发生**；收紧的实质是**新增了 `param` 这道此前不存在的约束**
> （此前 ② 类站点可以被静默新增），而不是把 literal 数字做大。自证见守卫的
> `same_file_literal_binding_is_classified_as_literal` / `enclosing_fn_parameter_is_classified_as_param`
> / `macro_call_argument_is_not_mistaken_for_a_parameter` / `param_ratchet_fails_on_a_new_param_site_in_an_unknown_file`。
> `synapse-common/src/transaction.rs:66` 的回收方向：三个调用点传的都是字面量，
> 把 `SET TRANSACTION ISOLATION LEVEL …` 分别内联到调用点即可宏化（牵动公共 API 形状，属独立设计事项）。

### 7.4 计数与口径

- 合计 **68** 条（D-01…D-68）：**未关闭 2**（D-57 部分 / D-68 未修）、
  **结构性例外 7**（D-13 / D-14 / D-18–D-22，有意不修）、**已关闭 59**（含 D-37 收敛与 D-62 修法① 落地）。
- 本文档**只显示**未关闭项与结构性例外；已关闭项的明细在 HISTORY §7.2（冻结，不参与当前计数），
  阶段总结后关闭的 5 条（D-37 / D-62 / D-65 / D-66 / D-67）在各提交信息里。
- "部分已修"指同一编号下仍有明确未做子项；结构性例外**不计入**待修，其约束力写在 §7.3 与 R1–R13。

### 7.5 处置约定（改 SQL / 查询前）

1. **先修再转，独立提交**（R12）—— 否则"编译器把改动证伪"这条证据链失效。
2. **登记唯一**（R13）—— 新问题只追加到 §7.1，不再开第二份清单。
3. **白名单只减不增**（R7）—— 允许保持动态的只有"动态标识符"与 `Vec<Option<T>>` 两类，
   且必须有 D-13/D-14 这样的登记条目。
4. **门禁必须自证能变红**（R11）—— 用故意违规证明它真的会失败，并把实验写进提交信息或本文档。

---

## 8. 优化方案

### 8.1 剩余可静态化清单（按实测，2026-09-25 C34 后）

**可转换残量 321 处**（291 literal / 29 runtime / 1 param），头部按大小排（前 14）：

| 文件 | 处数 | 门控 | 备注 |
|---|---|---|---|
| `synapse-storage/src/burn_after_read.rs` | 15 | `burn-after-read` | 见 §8.3（需带 feature 的 CI 等价库） |
| `synapse-services/src/database_initializer/mod.rs` | 15 | — | D-14 归属**已判**：15 处实参全是字面量（无 `format!` 拼装）⇒ 全部可转换，见 §8.3 |
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

> **门控列的判据**：整文件在 `#[cfg(feature = …)]` 下时，`cargo sqlx prepare` 必须 `--all-features`
> （R2 的教训），且 DB 往返要在带该 feature 的 CI 等价库上跑 ⇒ 门控文件单列一批更省来回。

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
2. **`database_initializer/mod.rs`（15）** —— D-14 归属**已判**（2026-09-26）：
   15 处实参全是**字面量**（含多行 `r"…"`），无一由 `format!` 拼装 ⇒ 15 处全可转换，
   不需登记结构性例外。（该文件同时含 `#![cfg]` 之外无门控，`prepare` 用 `--all-features` 即可。）
3. **D-57②（seed 侧收敛 `public`）** —— 见 §7.2：需"枚举 baseline 对象集 + 对多出来的对象逐个
   DROP"式设计，不能简单 `RESET_PUBLIC=1`。

### 8.4 收尾条件（何时可称"静态化战役结束"）

- `dynamic_production` 的**可转换部分归零**：393 → **72**（只剩测试基建 57 + 结构性 15），
  或每个残留都有 §7.3 那样的登记条目；
- literal 逐文件表只剩 4 类（3 个测试基建文件 + `event/pagination.rs`）；
- **D-68 有裁定并落地、D-57② 落地**（D-62 已落地、D-37 已收敛 ⇒ §7 只剩 D-57② 与 D-68）；
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
