# SQLx 静态化：阶段总结与剩余工作（2026-09-23 启动 · 2026-09-26 C40 后）

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
| `dynamic_production` | 1532（近似） | **275** | **−82.0%** |
| `static` | 61 | **1194** | +1133 |
| `dynamic`（总） | 2151 | **991** | −1160 |
| 静态占比 | 2.76% | **54.6%**（1194 / 2185） | +51.9pp |
| `.sqlx` 离线缓存 | 60 条 | **1162 条** | +1102 |
| literal（逐文件棘轮，处 / 文件） | 876 / 98 | **204 / 47** | −672 |
| `param` 传参（D-14 新棘轮，处 / 文件） | — | **1 / 1** | 新立棘轮（此前混在 `runtime`，两道棘轮都不管） |
| `runtime` 残差 / `query_builder` | — | 70 / 13 文件 · **18**（已入计数棘轮） | — |

> **并发增益不固化入棘轮（口径说明）**：上表的 `static` 是**实测值**（1194）。其中
> **1 处来自并发批次的独立提交**（`0bd14ebce` 链条：新增一条静态查询 + 1 条 `.sqlx`），
> 按 §8.5「同批同向下调/上调」纪律**不由后续批次代替它固化**（`BASELINE_STATIC` 现为 1193
> = 上一基线 1174 + 本次 C40 的 19，因此棘轮仍留 **1 点余量** —— 与 D-12 记录的先例一致：
> 跨批次替他批改棘轮会让"哪批完成了多少"不可追溯）。该批次应自行把它再上调 1 点。

### 0.2 残量结构（"还剩多少活"的准确说法）

| 组成 | 处数 | 性质 |
|---|---|---|
| **可静态化残量** | **203** | **173 处字面量**（纯机械转换）+ **29 处运行期拼装**（`format!` 拼列清单 / `ORDER BY` 方向等，**属 §7.3 D-14 结构性例外：需先设计替代方案，不能靠硬编码压数字**）+ **1 处跨函数传参**（`param`，把字面量内联到调用点即可转） |
| 测试基建（有意保留） | 57 | `synapse-test-utils/src/lib.rs` 28、`synapse-common/src/test_isolation.rs` 25、`test_schema_guard.rs` 4 |
| 结构性保留（有意） | 15 | `synapse-storage/src/event/pagination.rs`（9 runtime 游标/排序方向 + 6 literal） |
| **合计** | **275** | = 203 + 57 + 15 |

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

### 0.4 缺陷发现总览（**74 条**；只给统计与去向，不逐条显示）

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
| ⑨ 阶段总结后新发现并已关闭 | 11 | **D-74**（`update_access_stats` 的 `COALESCE($7, 0)` 让 PG 把 `$7` 定型成 **int4**，宏因此要求 `Option<i32>` 而 Rust 侧是 `response_time_ms: Option<f64>`；动态路径靠 sqlx 显式发送 FLOAT8 才没暴露 ⇒ 改 `0::float8` 并补浮点往返用例，见 §8.3）、**D-72**（`e2ee_audit.rs` 两个方向同时错：`e2ee_audit_log.details` 是 `NOT NULL DEFAULT '{}'`，但 `log_key_operation` 会把 `KeyEvent.details = None` 直接绑成 `NULL` ⇒ 运行期 23502；读回结构体又把该列声明成 `Option` ⇒ 可空性反推失真。已按 R12 先用 RED 用例复现 23502，再 `COALESCE($7, '{}'::jsonb)` + 读侧收紧为非 `Option`，见 §8.3）、**D-71**（D-25 家族收口：23 个 `#[cfg(feature)] pub mod` 声明里有 **10 个带测试却不在** `scripts/ci/gated_module_test_matrix` ⇒ "过滤器必须命中"这道守卫对它们从未生效；补 10 行后全表 21 行实跑通过）、**D-70**（`e4bc400cb` 删掉 3 个埋点却漏收紧 `metric_instrumentation_baseline` ⇒ 埋点棘轮在 `opt/consolidated` 上**常驻红**；按 R11 独立收紧 15 → 12 并复跑门禁）、D-62（通知响应的 `profile_tag` 键取自 `notification_type` ⇒ 已按修法① 改成真列 + 独立 `notification_type` 键）、**D-68**（通知记录层没有生产写入者、也没有保留期清理 ⇒ 已按修法① 接线 `record_notification` + `prune_old_notifications`，边界见 §0.5、明细见提交信息）、**D-69**（运行时迁移的 advisory lock key 在 `search_path` 为空时因 `current_schema()` 为 NULL 而**必败** ⇒ 已先 `COALESCE` 并补边界用例，见 §8.3）、**D-57②**（seed 侧 `public` 不收敛 ⇒ 新增 `scripts/ci/converge_public_schema.sh` 并接进 CI seed 第 [3/4] 步，见 §8.3）、D-65（并发改动只改一半 ⇒ 集成+clippy 双红）、D-66（worktree 共享 `CARGO_TARGET_DIR` ⇒ 跨树复用产物，假红/假绿）、D-67（新增测试里的死常量让 clippy 红） |
| ⑩ 新发现且**未关闭**（等结构性修法） | 1 | **D-73**：`e2ee_audit_log.operation` 在 catalog 中**可空**而 `KeyAuditEntry.operation` 非 `Option`（唯一写入者恒写非空 ⇒ C40 已按 R4 断言 `AS "operation!"`）；**结构上应把 `operation` 收紧为 `NOT NULL`，并删掉与之恒等值、零读者（只被 `idx_e2ee_audit_log_action` 引用）的 `action` 列** —— 牵动迁移 + 基线指纹 + 三份 `BASELINE_SQL`（R10），属独立事项，见 §7.1 |

**去向**：阶段总结前关闭的 57 条逐条明细在 HISTORY §7.2；总结后关闭的 12 条（D-37 / D-57② / D-62 /
D-65 / D-66 / D-67 / D-68 / D-69 / D-70 / D-71 / D-72 / D-74）记在各自提交信息里（下次阶段总结时并入快照）；**未关闭 1 条（D-73）在 §7.1 逐条留档**。本表 ①–⑧ 是**发现时**
的归类（历史口径，不随修复变动），因此 D-57 仍计入 ⑥、D-37 仍计入 ④、D-62 已改判为"已修" ——
"还剩哪些没修"看结论行与 §7.1，不看桶号。
**结论：64 已关闭 / **0 未关闭** / 7 结构性例外 —— 本战役登记表已清空。**

### 0.5 阶段结论

1. 动态 SQL 已从**系统性风险**降为**局部清单**：275 处里 72 处有意保留，待收 **203 处** ——
   其中 **173 处是纯机械转换**，29 处是 D-14 结构性（`format!` 拼列清单）、1 处是跨函数传参。
2. **收益性质变了**：早期批次每批都在挖"真 schema 下必败"的硬缺陷（① 类 15 条）；
   现在批次以机械收敛为主，并顺手清理一类残留（C31 清 `FromRow` 死代码、C32 消手工 `Row::get`、
   C33 消 `PgRow` 泄漏与 10 处吞错、C34 消 `Row` 解码与死 derive；C40 又挖出 D-72/D-73/D-74：
   两个方向的可空性错配、`COALESCE($7, 0)` 的参数定型陷阱、以及一行恒等值的冗余审计列）。
3. **长期资产是规则与门禁，不是数字**：数字会被并发改动推动，R1–R13 与四道自证过的门禁才是
   "不再制造同类缺陷"的保证；本阶段新增的两条规则（宏实参须为调用点字面量、worktree 各自 target 目录）
   都来自实测而非推导。
4. **登记表只剩 1 条未关闭项（2026-09-26，C40 后）**：`D-01…D-74` 里 66 条已关闭、7 条转为结构性
   例外，**唯一未关闭的是 D-73**（`e2ee_audit_log` 的 `operation` 可空性 + 冗余 `action` 列收敛，
   属迁移链独立事项，见 §7.1）。剩下的**只有计划内的工作**（§8.1 的 203 处可转换残量）与 7 条
   **结构性例外**（工具/接口边界，不是缺陷）。这不等于战役结束 —— 收尾条件见 §8.4。
5. **D-68 的接线边界（写清楚，免得下次误判）**：`notifications` 现在的生产写入者是
   `PushNotificationService::send_notification`（"服务端决定推送"这一处，排队成功后记一条，
   同批接入 30 天保留期清理）。本仓**没有**按事件求值的推送规则引擎，`sync` 的
   `notification_count` 仍由 `events` + `read_markers` 现算 —— 因此 `/notifications` 是
   "服务端实际推送过的通知"的记录，不是"按规则应当通知"的推导结果；两者口径不同属**有意**，
   若要统一（把计数改为读 `notifications`）那是另一个需要排期的产品改造。

---

## 7. 仍存在的问题（唯一登记处）

**当前无未关闭项**（最近一次关闭：D-57②，2026-09-26）。只登记**未关闭项**与**结构性例外**；
已关闭项的去向见 §0.4 与各自提交信息。

### 7.1 汇总表

| 编号 | 是什么 | 在哪 | 为什么还没修 | 怎么修 |
|---|---|---|---|---|
| **D-73** | `e2ee_audit_log` 的审计动作列有**两份且恒等值**：`action`（`NOT NULL`、**零读者**、只被 `idx_e2ee_audit_log_action` 引用）与 `operation`（**可空**、是唯一读取路径）；而 `KeyAuditEntry.operation` 非 `Option` | `migrations/00000000_unified_schema_v12.sql:849-861`、`synapse-storage/src/e2ee_audit.rs` | C40 转换时实测：宏按 catalog 判定 `operation` 可空 ⇒ 与结构体字段冲突；唯一写入者（`log_key_operation`）恒写非空，故 C40 按 R4 先断言 `operation AS "operation!"` 并注明理由。**结构性修法牵动迁移 + 基线指纹 + 三份 `BASELINE_SQL` + 契约用例（R10）**，不属机械转换批次 | ① `ALTER TABLE e2ee_audit_log ALTER COLUMN operation SET NOT NULL;`（唯一写入者恒写非空）；② 删除 `action` 列与其索引（零读者；保留 `operation` 是因为它是 API 序列化键 `KeyAuditEntry.operation`，删它才是对外形状变更）；③ 同步 `EXPECTED_BASELINE_FINGERPRINT`，跑 R10 的①–④ |

> 已关闭项的去向见 §0.4 与各自提交信息；本节只留**未关闭项**（R13）。

### 7.2 逐条明细

（空 —— 明细在 HISTORY §7.2 或各提交信息里。）

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

- 合计 **74** 条（D-01…D-74）：**未关闭 1（D-73）**、
  **结构性例外 7**（D-13 / D-14 / D-18–D-22，有意不修）、**已关闭 66**（含 D-37 收敛、
  D-57② 收敛、D-62 修法①、D-68 接线落地、D-69/D-70/D-71/D-72/D-74 先修）。
- 本文档**只显示**未关闭项与结构性例外；已关闭项的明细在 HISTORY §7.2（冻结，不参与当前计数），
  阶段总结后关闭的 12 条（D-37 / D-57② / D-62 / D-65 / D-66 / D-67 / D-68 / D-69 / D-70 / D-71 / D-72 / D-74）在各提交信息里。
- "部分已修"指同一编号下仍有明确未做子项；结构性例外**不计入**待修，其约束力写在 §7.3 与 R1–R13。

### 7.5 处置约定（改 SQL / 查询前）

1. **先修再转，独立提交**（R12）—— 否则"编译器把改动证伪"这条证据链失效。
2. **登记唯一**（R13）—— 新问题只追加到 §7.1，不再开第二份清单。
3. **白名单只减不增**（R7）—— 允许保持动态的只有"动态标识符"与 `Vec<Option<T>>` 两类，
   且必须有 D-13/D-14 这样的登记条目。
4. **门禁必须自证能变红**（R11）—— 用故意违规证明它真的会失败，并把实验写进提交信息或本文档。

---

## 8. 优化方案

### 8.1 剩余可静态化清单（按实测，2026-09-26 C40 后）

**可转换残量 203 处** = **173 处字面量（机械转换）** + **29 处运行期拼装（D-14 结构性）**
加 **1 处跨函数传参（`param`）**。下表按**字面量**处数排前 14（表内数字是**可机械转换**的站点数；
纯 `runtime` 文件见下方结构性清单）：

| 文件 | 处数 | 门控 | 备注 |
|---|---|---|---|
| `synapse-federation/src/event_broadcaster.rs` | 8 | — | `synapse-federation`，注意广播路径 |
| `synapse-federation/src/key_rotation.rs` | 8 | — | `synapse-federation`，注意密钥轮换路径 |
| `synapse-storage/src/admin_media.rs` | 8 | — | 注意 U-3 的 hash 隔离查询；并发会话近期活跃 |
| `synapse-storage/src/email_verification.rs` | 8 | — | ⚠️ **需先补覆盖**：该文件只有 1 条 DB 用例 |
| `synapse-storage/src/event/dag.rs` | 8 | — | `event/` 同域（**动手前确认并发会话不在途**） |
| `synapse-storage/src/call_session.rs` | 7 | `voip-tracking` | 门控（见 `gated_module_test_matrix`）⇒ 单列一批更省来回 |
| `synapse-storage/src/delayed_events.rs` | 7 | — | 单表模块（延迟事件队列） |
| `synapse-storage/src/room_account_data.rs` | 7 | — | ⚠️ **需先修**：2 处 `PgRow` 泄漏（`get_room_account_data` / `get_room_vault_data` 返回 `Option<PgRow>`）+ 1 处 `.ok().flatten()` 吞错 |
| `synapse-storage/src/media/quarantine_stream.rs` | 6 | — | 与 `pruning` 同域（保留期流） |
| `synapse-storage/src/monitoring.rs` | 6 | — | 单表模块（监控采样） |
| `synapse-storage/src/event/search.rs` | 6 | — | `event/` 同域 |
| `synapse-e2ee/src/backup/service.rs` | 5 | — | `synapse-e2ee`，备份服务（与 C19b 的 `backup/storage.rs` 同域） |
| `synapse-storage/src/feature_flags.rs` | 5 | — | 单表模块 |
| `synapse-storage/src/filter.rs` | 5 | — | 单表模块（过滤器） |

> 紧随其后（各 4–5 处）：`qr_login.rs`(5)、`account_data/mod.rs`(4)、`audit.rs`(4)、
> `event/ephemeral.rs`(4)、`room_tag/mod.rs`(4)、`schema_health_check.rs`(4)。
> ⚠️ `event/pagination.rs` 的 6 处 literal **不在**本表：它与同文件的 9 处 runtime 一起属
> §0.2 的"结构性保留 15"，不是待做的机械转换。

> **门控列的判据**：整文件在 `#[cfg(feature = …)]` 下时，`cargo sqlx prepare` 必须 `--all-features`
> （R2 的教训），且 DB 往返要在带该 feature 的 CI 等价库上跑 ⇒ 门控文件单列一批更省来回。

> **D-14 结构性保留（0 处 literal，有意保持动态 —— 不是"还没做的机械转换"）**：
> `event/state.rs`(9)、`membership/mod.rs`(4)、`event/basic.rs`(3)、`state_groups.rs`(2)、
> `space/repository.rs`(2)、`event/batch.rs`(2)，以及混在其它文件里的 7 处
> （`src/server/database.rs` 3、`user/storage.rs` 2、`maintenance.rs` 2）—— 共 **29 处**，
> 与 §0.2 的 runtime 分项一致。它们的 SQL 文本由 `format!` 拼出（`{ROOM_EVENT_COLS}` /
> `{STATE_EVENT_OUTER_COLS}` / `ORDER BY` 方向等），而宏要求调用点字面量（R1）；
> **硬编码会把列清单复制多份**（铁律 2）⇒ 必须**先设计替代方案**再回收
> （候选：`query_file!` + 每查询一个 `.sql` 文件 —— 全仓尚无先例，属独立设计事项）。
> **C35–C40 均已完成**：`database_initializer/mod.rs`（15）、`burn_after_read.rs`（15）、
> `federation_queue.rs`（8）、`openid_token.rs`（7）、`event/{basic,redaction,batch}.rs`（25）、
> `ssss/storage.rs`（10）、`secure_backup/service.rs`（10）、`key_request/storage.rs`（9）、
> `sticky_event.rs`（5）、`federation_blacklist.rs`（7）、`e2ee_audit.rs`（7）
> —— 这些文件的生产区**可机械转换部分已全部归零**。

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

1. ✅ **`burn_after_read.rs`（15，门控 `burn-after-read`）已完成（2026-09-26，C36）** ——
   `prepare` 用 `--all-features`；**DB 往返必须显式带 feature**：
   `cargo nextest run -p synapse-storage --lib --features test-utils,burn-after-read -E 'test(/burn_after_read/)'`
   ⇒ **22/22 通过**（12 条 db_tests 走真 baseline schema，覆盖 upsert/批量/统计/往返等全部转换路径）。
   转换要点：`get_user_stats` 的三个聚合按 R4 断言 `AS "total_burned!"` 等（子查询 + `COALESCE`
   自身即非空保证）；`log_burned_event_batch` 的 `UNNEST(...)` 按 R5 把 `Vec<&str>` 改成 `Vec<String>`。
   ⚠️ **顺带补一个 D-25 类覆盖缺口**：该模块是 `#[cfg(feature = "burn-after-read")]` 门控的
   （`synapse-storage/src/lib.rs:205-206`），但此前**不在** `scripts/ci/gated_module_test_matrix` 里，
   于是"门控模块的测试过滤器必须命中"这道守卫从未检查过它 —— feature 未开时
   0 个用例会被静默当成通过。本批补行后，`check_gated_module_tests.sh burn_after_read`
   实测 58 tests run + OK（feature 缺失时会以"0 个用例"变红）。
2. **`database_initializer/mod.rs`（15 处→ 全部完成，该文件生产动态归零）** —— D-14 归属**已判**（实参全是
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
   - **C 类 / 4 处（utility 语句）**：`ensure_schema_migrations_table` 的 `CREATE INDEX`、
     `run_runtime_migrations` 里的 `SET statement_timeout`、`ROLLBACK` ×2。
     🔴 **已实测（C35b）：这三条文本全部 `cargo sqlx prepare` 成功、`describe.columns == []`、
     `query!(…).execute(…)` 正常编译 ⇒ 全仓"DDL / utility 语句无法用 query! 静态化"的旧说法
     被推翻**。真正不能宏化的只有两类：① `#[cfg(test)]`/`tests/` 内的语句（不进缓存，
     `--all-targets` 离线编译会失败，D-13/R9）；② SQL 文本不是编译期常量的语句（动态标识符 /
     `format!` 拼接，R7 白名单）。已同步修正 `AGENTS.md` R6 ④ 与
     `scripts/ci/sqlx_dynamic_ratio_baseline` 里的旧表述（含"DDL 无元数据"那句）。
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
   ✅ **C35b-0 已完成（先修 D-69）** —— `migration_lock_key` 的
   `hashtext(current_database() || ':' || current_schema())` 在 **`search_path` 为空或指向不存在
   的 schema** 时 `current_schema()` 为 NULL ⇒ `hashtext(NULL)` 为 NULL ⇒ 调用方
   `let lock_key: i64` 以 `UnexpectedNullError` 失败 ⇒ **运行时迁移在最需要它的场景
   （schema 尚未建立）反而跑不起来**。已先 `COALESCE(…, current_database() || ':')` 修掉，
   并补边界用例（`after_connect` 里 `SET search_path = ''` 的池 + 自证前提
   `current_schema() IS NULL`）；**RED 已实测**：回退修复后该用例报
   `ColumnDecode { index: "0", source: UnexpectedNullError }`，修复后 32/32 全绿。
   ✅ **C35b 已完成（2026-09-26，本批）** —— B 类 2 处（`migration_lock_key` / `try_acquire_migration_lock`，
   在 D-69 的 `COALESCE` 之后按 R4 断言 `AS "lock_key!"` / `AS "locked!"`）与 C 类 4 处
   （3 条文本：`CREATE INDEX` / `SET statement_timeout` / `ROLLBACK`×2 共享一条缓存条目）全部宏化
   ⇒ 该文件 15 处**全部完成、生产动态归零**；`dynamic_production` 384 → **378**、
   `static` 1084 → **1090**、`.sqlx` 1053 → **1058**（+5 / 0 删除）；模块用例 **32/32** 全绿。
   ⇒ **C35 三个提交（C35a-0 / C35a / C35b-0+C35b）全部落地**，无剩余子项。
   （2 处吞错已定性：**有意的 best-effort** —— 失败后没有任何调用方据此做出决定，且都不应阻断
   启动；已在源码就地图注，不单列缺陷。）

3. ✅ **C39（e2ee 三件套 29 处）已完成（2026-09-26）** —— `ssss/storage.rs`(10) +
   `secure_backup/service.rs`(10) + `key_request/storage.rs`(9) 全部宏化。
   实测：`cargo nextest run -p synapse-e2ee --lib --features test-utils` ⇒ **406/406**；
   集成 `-E 'test(/api_e2ee_advanced|key_backup|account_data_routes|ssss|secure_backup/)'`
   ⇒ **21/21**（覆盖 SSSS 与 secure_backup 的端到端路径，含 `api_e2ee_advanced` 的
   `/_matrix/client/v3/keys/backup/secure` 与 `key_backup_*` 套件）。
   **C39-0（先补覆盖）**：STEP 0 实测三者覆盖差异很大 —— SSSS / secure_backup 有大量端到端引用
   （19 / 1070 处），而 **`key_request/storage.rs` 的 9 处在 `tests/` 里零引用**且本文件没有
   `db_tests`（与同域 `olm/`/`megolm/`/`backup/` 三个 storage 不一致）；但该模块**是活的**
   （`wiring/e2ee.rs:83` → `routes/e2ee/{devices,keys}.rs` 的 `/room_keys/request*`）⇒ 不能按
   死代码删。先补 5 条真 baseline `db_tests`（覆盖 9 条路径）再转换。
   **四个非机械点**：① R4 —— `e2ee_secret_storage_keys.encrypted_key` / `.signatures` 列**可空**
   而 `SecretStorageKeyRow` 字段非 `Option` ⇒ 断言 `AS "…!"`，依据是**全部写入者恒写非空**
   （唯一生产 INSERT + 唯一测试夹具）；结构性替代是收紧 schema，属独立事项；
   ② R4 —— `COUNT(*)` / `1::bigint` / `COALESCE(is_fulfilled, FALSE)` 同理断言；
   ③ R5 —— `key.public_key` 的 `&Option<String>` ⇒ `.as_deref()`，`UNNEST($3::text[])` 的
   `Vec<&str>` ⇒ `Vec<String>`（与 C36 同型）；④ R6⑤ —— `restore_backup` 的元组投影改
   `query!` 按字段读，且带双引号别名的 raw string 开头必须 `r#"`（本批又实测踩到
   `no rules expected !` 一次）。

4. ✅ **C38（event 域 25 处）已完成（2026-09-26）** —— `event/basic.rs`(8) +
   `event/redaction.rs`(10) + `event/batch.rs`(7) 的字面量站点全部宏化；
   `cargo nextest run -p synapse-storage --lib --features test-utils -E 'test(/event::/)'`
   ⇒ **112/112 通过**（含 C38-0 先补的 6 条真 baseline 用例）。
   **STEP 0 的关键结论（修正了批次预估）**：这 4 个文件（含 `event/state.rs`）合计 39 处生产动态，
   但**只有 25 处可机械转换**；余下 14 处 = `{ROOM_EVENT_COLS}` / `{STATE_EVENT_OUTER_COLS}`
   的 `format!` 拼列清单（basic 3 + batch 2 + **state 9**）⇒ 属 D-14 结构性例外。
   ⇒ **`event/state.rs` 的可转换数为 0**，已从 §8.1 的"可转换"表移入结构性清单；
   §8.1 自此按 **literal** 处数排序（不再把 runtime 混进"可转换"数字）。
   **C38-0（先补覆盖）**：该域的 DB 往返集中在本模块的 `event/db_tests.rs`，但目标里有 6 个方法
   **零引用**（`find_event_ids_for_redaction` 的 4 个时间窗分支 + batch 的 5 个查询）⇒ 先补 6 条
   真 baseline 用例（夹具 `seed_event_row`）。踩坑：`events` 有 `ck_events_event_id_format`
   （`$…:<server>`），夹具事件 id 必须带 server name；事件 id 批量改写后**必须重跑 `cargo fmt`**
   （pre-commit 的 fmt 棘轮实测拦下了这次提交）。
   **两个新宏陷阱**（已写进 AGENTS.md **R6 ⑤**）：
   · **`query_as!` 不像 `FromRow` 那样忽略结果集里的多余列**：列名必须与结构体字段一一对应
     （多列 E0560 / 少列 E0063，即 D-22 的对称面）。`RoomEvent.processed_ts` 带
     `#[sqlx(rename = "processed_at")]`（D-19），而 `ROOM_EVENT_COLS` 时代写的是
     `origin_server_ts as processed_at` ⇒ 宏化时必须改写成 `AS "processed_ts"`（本次 3 处）。
   · **`query_as!` 不能构造元组**（它按字段构造结构体）⇒ 元组投影改 `query_scalar!`（单列）
     或 `query!`（多列、按字段读）；`redact_event_content` 的 `(String, Value)` 即如此改写。

4. ✅ **D-57②（seed 侧收敛 `public`）已完成（2026-09-26）** —— 新增
   `scripts/ci/converge_public_schema.sh`，并由 `scripts/ci/prepare_test_db.sh` 的
   **第 [3/4] 步**调用（每次 CI seed 都收敛一次）：
   - **参考集不维护清单**：上一步刚重建的模板 schema（同一份迁移、干净重建）就是 baseline 的对象集，
     逐对象比对 (kind, name) ⇒ baseline 变更无需改本脚本；
   - **只删多余对象**（table/view/matview/sequence，扩展拥有的对象永不作为候选），
     视图 → 物化视图 → 序列 → 表的顺序 + `IF EXISTS`，**不做** `DROP SCHEMA public CASCADE`；
   - **自证**：注入 `d57_probe_table`（含 PK 索引）+ `d57_probe_view` ⇒ report 精确列出 2 个候选、
     apply 删除后 `public 多余=0 缺失=0`、220 == 220、`rooms/events/users/notifications` 完好；
     再跑一次 0 删除（幂等）；安全栏杆实测——参考 schema 不存在（空参考会把整个 baseline 当"多余"）
     与库名不含 `test` 两种情形都**拒绝执行且未删任何对象**（EXIT=1）。
   - 端到端：跑一次完整 `prepare_test_db.sh`（探针预先注入）⇒ [3/4] 删掉探针、[4/4] 验证 220/220 通过。

5. ✅ **C40（`sticky_event.rs` 5 + `federation_blacklist.rs` 7 + `e2ee_audit.rs` 7 = 19 处）已完成（2026-09-26）** ——
   三文件生产动态 19 处全部宏化（各自生产区剩余动态 = 0）；三文件都无 feature 门控。
   **C40-0（先修 D-72，独立提交 `e016ce066`）**：`e2ee_audit_log.details` 是 `NOT NULL DEFAULT '{}'`，
   而 `log_key_operation` 把 `KeyEvent.details: Option<Value>` 直接绑成 `NULL` ⇒ **23502（不会回落列默认值）**；
   读侧 `KeyAuditEntry.details` 又声明成 `Option`（R4 的"反向不报错"⇒ 字段类型不能当可空性证据）。
   RED 已实测：新增用例 `test_log_key_operation_without_details_falls_back_to_column_default` 报
   `23502 null value in column "details" of relation "e2ee_audit_log"`；修复（`COALESCE($7, '{}'::jsonb)`
   + 读侧收紧为非 `Option`）后 `-E 'test(/e2ee_audit/)'` ⇒ **8/8**。
   **批次内两条新发现**：
   · **D-73（唯一未关闭项，见 §7.1）**：`e2ee_audit_log.operation` 在 catalog 中**可空**而
     `KeyAuditEntry.operation` 非 `Option`；唯一写入者恒写非空 ⇒ 本批按 R4 断言 `operation AS "operation!"`
     （"谁保证非空"写在结构体字段 doc 上），结构性修法（收紧 NOT NULL + 删零读者的恒等值 `action` 列）
     需走迁移链，单列条目。
   · **D-74（已关闭）**：`update_access_stats` 的 `COALESCE($7, 0)` 让 PG 把 `$7` 定型成 **int4** ⇒
     宏要求 `Option<i32>` 而 Rust 侧是 `response_time_ms: Option<f64>`（E0308 实测）；动态路径当时靠
     sqlx 显式发送 FLOAT8 才没暴露。改 `0::float8`（与列类型/实参一致，无行为变化），并补
     `test_update_access_stats_preserves_fractional_average`（100.5 入库、`(100.5+200.5)/2 = 150.5`）。
   其余非机械点：`COALESCE(blocked_by, 'system') AS "blocked_by!"`（COALESCE + 非空字面量恒非空，R4 ①）；
   `KeyEvent` 的 `&Option<String>` / `&Option<Value>` ⇒ `.as_deref()` / `.as_ref()`（R5）；
   带双引号别名的 raw string 一律 `r#"…#"#`（R6，本批又踩一次 `no rules expected !`）。
   验证：`cargo nextest run -p synapse-storage --lib --features test-utils
   -E 'test(/sticky_event/) or test(/federation_blacklist/) or test(/e2ee_audit/)'` ⇒ **45/45**；
   `federation_blacklist` 单跑 **28/28**（含新增浮点用例）。
   ⚠️ STAGE 0 已把 `room_account_data.rs`（7，2 处 `PgRow` 泄漏 + 1 处吞错）与
   `email_verification.rs`（8，仅 1 条 DB 用例）**排除**出本批：前者须先修、后者须先补覆盖（R12）。

### 8.4 收尾条件（何时可称"静态化战役结束"）

- `dynamic_production` 的**可机械转换部分（literal）归零**：275 → **101**
  （275 − 173 literal − 1 param = 101 = 测试基建 57 + 分页结构性 15 + **D-14 结构性 29**），
  或每个残留都有 §7.3 那样的登记条目；
- literal 逐文件表只剩 4 类（3 个测试基建文件 + `event/pagination.rs`）；
- ~~D-68 接线~~、~~D-37 收敛~~、~~D-62 修法①~~、~~D-57② 收敛~~ **均已落地**；§7 只剩
  **D-73**（`e2ee_audit_log` 的可空性/冗余列收敛，迁移链独立事项）；
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
