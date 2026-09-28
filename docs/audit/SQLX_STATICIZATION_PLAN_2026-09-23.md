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
| `dynamic_production` | 1532（近似） | **242** | **−84.2%** |
| `static` | 61 | **1246** | +1185 |
| `dynamic`（总） | 2151 | **967** | −1184 |
| 静态占比 | 2.76% | **56.3%**（1246 / 2213） | +53.5pp |
| `.sqlx` 离线缓存 | 60 条 | **1214 条** | +1154 |
| literal（逐文件棘轮，处 / 文件） | 876 / 98 | **171 / 39** | −705 |
| `param` 传参（D-14 新棘轮，处 / 文件） | — | **1 / 1** | 新立棘轮（此前混在 `runtime`，两道棘轮都不管） |
| `runtime` 残差 / `query_builder` | — | 70 / 13 文件 · **18**（已入计数棘轮） | — |

> **并发增益不固化入棘轮（口径说明）**：C41 之前，`opt/consolidated` 上并存过并发批次的
> 增益与回退（U-5 的 +7 已被其批次计入基线、invite-policy 合并回退的 12 处由其 `4cc45d279`
> 转宏偿还）。C41 起点与 `opt/consolidated` 完全一致（HEAD = `18071e8b3`，无分叉），
> 因此各批**按实测值**直接收紧，不再留余量：C41 把 `BASELINE_DYNAMIC_PRODUCTION` 282 → 272、
> `BASELINE_STATIC` 1206 → 1216；C42 收到 **263 / 1225**；C43 再收到 **255 / 1233**；
> C44-0（先修删零引用方法）→ **254 / 1233**；D-79（federation 隔离池基建）只动测试区
> （`BASELINE_DYNAMIC_TEST_INFRA` 718 → 723）；D-80（隔离 clone 同形）的校验宏化后
> **254 / 1234**（`.sqlx` 1201 → 1202，详见 baseline 内同日注记）；C44（12 处宏化）收到
> **242 / 1246**（`.sqlx` 1202 → 1214，literal 退三行 → 171/39）；同批按 R8 给 `service.rs` 的
> 四个被转方法补了真 baseline 往返 ⇒ `BASELINE_DYNAMIC_TEST_INFRA` 723 → **725**（两处
> `#[cfg(test)]` 夹具：插 `rooms` 行满足真实 FK + 直接造一条 `backup_id_text IS NULL` 的行）。

### 0.2 残量结构（"还剩多少活"的准确说法）

| 组成 | 处数 | 性质 |
|---|---|---|
| **可静态化残量** | **170** | **140 处字面量**（纯机械转换）+ **29 处运行期拼装**（`format!` 拼列清单 / `ORDER BY` 方向等，**属 §7.3 D-14 结构性例外：需先设计替代方案，不能靠硬编码压数字**）+ **1 处跨函数传参**（`param`，把字面量内联到调用点即可转） |
| 测试基建（有意保留） | 57 | `synapse-test-utils/src/lib.rs` 28、`synapse-common/src/test_isolation.rs` 25、`test_schema_guard.rs` 4 |
| 结构性保留（有意） | 15 | `synapse-storage/src/event/pagination.rs`（9 runtime 游标/排序方向 + 6 literal） |
| **合计** | **242** | = 170 + 57 + 15 |

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

> ⚠️ **缓存相关的两条禁令（D-77，2026-09-26）**：本仓**只跑** `check_sqlx_cache_fresh.sh`
> 的 `--static` + `--compile` 两道；**禁止** `--full`（它对着真库逐条 describe，而共享
> `synapse_test.public` 会被并发会话的 D-57② 收敛清空 ⇒ 实测吐出 1443 个误导性
> E0282/E0277），**禁止**裸 `cargo sqlx prepare`（destination 就是 `.sqlx/` 且先清空再重写）。
> 要写缓存只有唯一入口：`DATABASE_URL=<已迁移库> bash scripts/ci/sqlx_prepare.sh`
> （前置检查 + 缩容回滚，见 AGENTS.md R2）。**环境里没有 `psql`** 时：用绝对路径
> `/opt/homebrew/opt/postgresql@15/bin/psql`，或用一个**已迁移好**的库 + `SQLX_PREPARE_SKIP_DB_CHECK=1`
> 跳过前置检查（`--check` 不允许跳；缩容回滚仍生效）。`cargo sqlx prepare` 自身不需要 psql。

### 0.4 缺陷发现总览（**81 条**；只给统计与去向，不逐条显示）

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
| ⑨ 阶段总结后新发现并已关闭 | 18 | **D-81**（`b38380d9b`（A3+A4）新增的 `tests/integration/state_groups_backfill_tests.rs` **从未定义**它自己调用的 `unique_id()` （每个测试模块都是文件本地助手，不是共享工具），且 `StateGroupStateEntry` 导入未使用 ⇒ 集成测试 target 编译失败（E0425 ×2 + unused import ×1），`--all-targets --all-features` 的 clippy 与 `check_sqlx_cache_fresh.sh --compile` **在 `opt/consolidated` 上双红** —— 而这两道正是 CI 的阻断门禁。修法：补齐本文件的 `unique_id()`（`AtomicU64` 计数器，与同批 `state_groups_idempotency_tests.rs` 同形）+ 删无用导入，见 §8.3）、**D-80**（隔离 clone 与模板不同形，**两处**：① phase 1d 显式跳过约束支撑的索引、且注释断言"`LIKE` 已保留 PRIMARY KEY 名"——实测**不成立**（`pk_users`→`users_pkey`、`uq_users_username`→`users_username_key`）；② 物化视图上的索引**根本没被搬运**（克隆里 `idx_rooms_summaries_mv_*` ×4 + `idx_public_room_directory_*` ×2 全缺）。修法：新增 phase 1e 按 `(table, contype, pg_get_constraintdef)` 配对后用 `ALTER TABLE … RENAME CONSTRAINT` 还原 PK/UNIQUE/EXCLUDE 名（CHECK 名本就保留）、phase 2 在建 matview 后按其模板索引 DDL 逐个重建、`validate_clone` 从"只比数量"改为**索引名字集合**比对（这正是它长期不可见的原因）。R11 自证：去掉 phase 1e ⇒ 扩展后的探针用例红；缺 phase 2 那段 ⇒ 新名字检查当场列出 6 个缺失名。C44-0 那条健康检查用例随之从"容忍 5 组"收紧回 `missing_indexes.is_empty()`）、**D-79**（`synapse-federation` 缺 per-test schema 基建 ⇒ 该 crate 的 DB 路径只能跑共享 `public`，`key_rotation.rs` 的自愈 DDL 分支无法安全构造，集成用例名字承诺"缺失后恢复"却没造场景。已补 crate 本地隔离池适配器（第 4 份 `BASELINE_SQL` 副本，纳入统一守卫 `FEDERATION` 清单）+ 两条**真构造场景**的 db_tests（先 `DROP TABLE` 再断言重建表与两条索引 / 配置表 + 默认值 + 回写往返），并按 R11 用"把自愈变 no-op"变异自证两条用例都变红；集成用例改名为 `test_load_or_create_key_persists_a_signing_key` 并指向新用例）、**D-78**（C42 把 `key_rotation.rs` 8 处判为"与迁移重复的死 DDL、应删"并登记为待裁定；C43 复核发现 `federation_service_tests_migrated.rs` 里有一条 `test_load_or_create_key_recovers_missing_signing_key_table` —— 自愈是**有意**行为，且该测试体从未构造"表缺失"（跑的是共享库、表本就在）⇒ **改判为保留自愈 + 宏化 8 处**，该测试名承诺的恢复场景从未被覆盖 ⇒ 另立 D-79）、**D-77**（`check_sqlx_cache_fresh.sh --full` 对着被收敛成 0 表的共享 `public` 会吐 **1443 个 E0282/E0277**（看起来像源码坏了），而裸 `cargo sqlx prepare` 会把 `.sqlx/` 清空 ⇒ 新增唯一入口 `scripts/ci/sqlx_prepare.sh`（前置检查 fail-fast + 缩容回滚），`--full` 委托给它并在 AGENTS.md R2/R8 明令禁止）、**D-76**（`scripts/init_test_public_schema.sh` 的 `RESET_PUBLIC` 默认 1 ⇒ **裸跑就 `DROP SCHEMA public CASCADE`** 重建共享 `synapse_test.public`；失败/中断即留下 0 表 ⇒ 默认改为 0（幂等 apply），重建需显式 opt-in）、**D-75**（`converge_public_schema.sh` 的 TOCTOU：删除清单在 apply 阶段**二次求值**，而 `prepare_test_db.sh` [2/4] 会 `DROP SCHEMA test_template_ci CASCADE` 重建参考集 ⇒ 参考为空时 public 全被判"多余"；事后不变量又用同一个已塌掉的参考集（两边同时塌成 0 ⇒ 恒过）。实测环境 `synapse_test.public` = **0 表**（本该 ≥200）⇒ 已冻结清单 + 参考稳定性复检 + 大删栏杆 + 非空不变量，见 §8.3）、**D-74**（`update_access_stats` 的 `COALESCE($7, 0)` 让 PG 把 `$7` 定型成 **int4**，宏因此要求 `Option<i32>` 而 Rust 侧是 `response_time_ms: Option<f64>`；动态路径靠 sqlx 显式发送 FLOAT8 才没暴露 ⇒ 改 `0::float8` 并补浮点往返用例，见 §8.3）、**D-72**（`e2ee_audit.rs` 两个方向同时错：`e2ee_audit_log.details` 是 `NOT NULL DEFAULT '{}'`，但 `log_key_operation` 会把 `KeyEvent.details = None` 直接绑成 `NULL` ⇒ 运行期 23502；读回结构体又把该列声明成 `Option` ⇒ 可空性反推失真。已按 R12 先用 RED 用例复现 23502，再 `COALESCE($7, '{}'::jsonb)` + 读侧收紧为非 `Option`，见 §8.3）、**D-71**（D-25 家族收口：23 个 `#[cfg(feature)] pub mod` 声明里有 **10 个带测试却不在** `scripts/ci/gated_module_test_matrix` ⇒ "过滤器必须命中"这道守卫对它们从未生效；补 10 行后全表 21 行实跑通过）、**D-70**（`e4bc400cb` 删掉 3 个埋点却漏收紧 `metric_instrumentation_baseline` ⇒ 埋点棘轮在 `opt/consolidated` 上**常驻红**；按 R11 独立收紧 15 → 12 并复跑门禁）、D-62（通知响应的 `profile_tag` 键取自 `notification_type` ⇒ 已按修法① 改成真列 + 独立 `notification_type` 键）、**D-68**（通知记录层没有生产写入者、也没有保留期清理 ⇒ 已按修法① 接线 `record_notification` + `prune_old_notifications`，边界见 §0.5、明细见提交信息）、**D-69**（运行时迁移的 advisory lock key 在 `search_path` 为空时因 `current_schema()` 为 NULL 而**必败** ⇒ 已先 `COALESCE` 并补边界用例，见 §8.3）、**D-57②**（seed 侧 `public` 不收敛 ⇒ 新增 `scripts/ci/converge_public_schema.sh` 并接进 CI seed 第 [3/4] 步，见 §8.3）、D-65（并发改动只改一半 ⇒ 集成+clippy 双红）、D-66（worktree 共享 `CARGO_TARGET_DIR` ⇒ 跨树复用产物，假红/假绿）、D-67（新增测试里的死常量让 clippy 红） |


**去向**：阶段总结前关闭的 57 条逐条明细在 HISTORY §7.2；总结后关闭的 19 条（D-37 / D-57② / D-62 /
D-65 / D-66 / D-67 / D-68 / D-69 / D-70 / D-71 / D-72 / D-73 / D-74 / D-75 / D-76 / D-77 / D-78 / D-79 / D-80）记在各自提交信息里（下次阶段总结时并入快照）；**无未关闭项**。本表 ①–⑧ 是**发现时**
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
4. **登记表再次清空（2026-09-26，D-80 后）**：`D-01…D-80` 里 73 条已关闭、7 条转为结构性例外，**无未关闭项**。
   D-80（隔离 clone 与模板不同形：约束名被 PG 改名 + matview 索引未搬运）已修复并加了名字集合门禁 —— 隔离库第一次与 `public` 同形。
   D-78（C42 曾判"运行时 DDL 是死代码"）已在 C43 **改判并关闭**：自愈是有意行为（有命名用例），已保留并宏化。D-73（`e2ee_audit_log` 的冗余 `action` 列 + 可空 `operation`）已按 R4 的
   "结构上能保证就收紧 schema"落地：删列 + `operation SET NOT NULL` + 基线指纹同步（见 §8.3）。
   剩下的**只有计划内的工作**（§8.1 的 203 处可转换残量）与 7 条**结构性例外**（工具/接口边界，
   不是缺陷）。这不等于战役结束 —— 收尾条件见 §8.4。
5. **环境事实：共享 `synapse_test.public` 会被并发会话改造，别把它当成稳定输入（D-75/D-76/D-77）**：
   它曾被收敛成 **0 表**（实测），于是 `.sqlx` 的 `--full` 抛出 1443 个误导性编译错误。三条修法都已落地
   （冻结删除清单 + 参考稳定性复检 + 大删栏杆；`RESET_PUBLIC` 默认改为非破坏性的 0；`.sqlx` 写入收敛到
   `scripts/ci/sqlx_prepare.sh`），并且**规则层面**已禁止 `--full` 与裸 `cargo sqlx prepare`（AGENTS.md
   R2/R8）。要缓存核对只用 `--static` + `--compile`；要真库核对就先备一个**私有/一次性**已迁移库。
6. **D-68 的接线边界（写清楚，免得下次误判）**：`notifications` 现在的生产写入者是
   `PushNotificationService::send_notification`（"服务端决定推送"这一处，排队成功后记一条，
   同批接入 30 天保留期清理）。本仓**没有**按事件求值的推送规则引擎，`sync` 的
   `notification_count` 仍由 `events` + `read_markers` 现算 —— 因此 `/notifications` 是
   "服务端实际推送过的通知"的记录，不是"按规则应当通知"的推导结果；两者口径不同属**有意**，
   若要统一（把计数改为读 `notifications`）那是另一个需要排期的产品改造。

---

## 7. 仍存在的问题（唯一登记处）

**当前无未关闭项**（最近一次关闭：D-81，2026-09-26；A3+A4 新增集成测试编译失败已先修）。只登记**未关闭项**与**结构性例外**；
已关闭项的去向见 §0.4 与各自提交信息。

### 7.1 汇总表

（空 —— `D-01…D-80` 已全部关闭或转为结构性例外；D-80 于 2026-09-26 关闭，明细见 §8.3。）

| 编号 | 是什么 | 在哪 | 为什么还没修 | 怎么修 |
|---|---|---|---|---|


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

- 合计 **81** 条（D-01…D-81）：**未关闭 0**、
  **结构性例外 7**（D-13 / D-14 / D-18–D-22，有意不修）、**已关闭 74**（含 D-37 收敛、
  D-57② 收敛、D-62 修法①、D-68 接线落地、D-69/D-70/D-71/D-72/D-74 先修、
  D-75/D-76/D-77 工具链事故先修、D-73 结构性收敛、D-78 改判收口、D-79 隔离池基建、D-80 隔离同形、
  D-81 A3+A4 集成测试编译失败先修）。
- 本文档**只显示**未关闭项与结构性例外；已关闭项的明细在 HISTORY §7.2（冻结，不参与当前计数），
  阶段总结后关闭的 19 条（D-37 / D-57② / D-62 / D-65 / D-66 / D-67 / D-68 / D-69 / D-70 / D-71 / D-72 / D-73 / D-74 / D-75 / D-76 / D-77）在各提交信息里。
- "部分已修"指同一编号下仍有明确未做子项；结构性例外**不计入**待修，其约束力写在 §7.3 与 R1–R13。

### 7.5 处置约定（改 SQL / 查询前）

1. **先修再转，独立提交**（R12）—— 否则"编译器把改动证伪"这条证据链失效。
2. **登记唯一**（R13）—— 新问题只追加到 §7.1，不再开第二份清单。
3. **白名单只减不增**（R7）—— 允许保持动态的只有"动态标识符"与 `Vec<Option<T>>` 两类，
   且必须有 D-13/D-14 这样的登记条目。
4. **门禁必须自证能变红**（R11）—— 用故意违规证明它真的会失败，并把实验写进提交信息或本文档。

---

## 8. 优化方案

### 8.1 剩余可静态化清单（按实测，2026-09-26 C44 后）

**可转换残量 170 处** = **140 处字面量（机械转换）** + **29 处运行期拼装（D-14 结构性）**
加 **1 处跨函数传参（`param`）**。下表按**字面量**处数排前 11（表内数字是**可机械转换**的站点数；
纯 `runtime` 文件见下方结构性清单）：

| 文件 | 处数 | 门控 | 备注 |
|---|---|---|---|
| `synapse-storage/src/event/dag.rs` | 8 | — | `event/` 同域；⚠️ **需先补覆盖**（无 in-file db_tests） |
| `synapse-storage/src/email_verification.rs` | 8 | — | ⚠️ **需先补覆盖**：仅 1 条 DB 用例（且用的是空隔离池，R9 禁止的形态） |
| `synapse-federation/src/event_broadcaster.rs` | 8 | — | ⚠️ **需先补覆盖**：无 in-file 测试，`recover_pending_from_db` / `cleanup_old_transactions` 零引用 |
| `synapse-storage/src/call_session.rs` | 7 | `voip-tracking` | 门控（见 `gated_module_test_matrix`）⇒ 单列一批更省来回 |
| `synapse-storage/src/delayed_events.rs` | 7 | — | ⚠️ **需先补覆盖**：7 个方法里 5 个**零引用**（list/restart/cancel/mark_sent/get_due_events） |
| `synapse-storage/src/room_account_data.rs` | 7 | — | ⚠️ **需先修**：2 处 `PgRow` 泄漏（`get_room_account_data` / `get_room_vault_data` 返回 `Option<PgRow>`）+ 1 处 `.ok().flatten()` 吞错 |
| `synapse-storage/src/media/quarantine_stream.rs` | 6 | — | 与 `pruning` 同域（保留期流）；⚠️ 需先核覆盖（仅 1 条用例） |
| `synapse-storage/src/monitoring.rs` | 6 | — | 单表模块（监控采样）；⚠️ 需先补覆盖（无 in-file 测试） |
| `synapse-storage/src/event/search.rs` | 6 | — | `event/` 同域；⚠️ 需先补覆盖（无 in-file db_tests，但 34 个集成文件引用） |
| `synapse-storage/src/event/ephemeral.rs` | 4 | — | `event/` 同域 |

> 紧随其后（各 4 处）：`account_data/mod.rs`(4)、`room_tag` 家族以外的 `room/` 子模块等。
> `synapse-e2ee/src/backup/service.rs`(4)、`synapse-storage/src/audit.rs`(4)、
> `synapse-storage/src/schema_health_check.rs`(4) 由 **C44** 归零退表（12 处宏化 + 两处
> `COUNT(*)`/`COALESCE` 的 R4 断言 + R5 数组参数改 owned + R6 ⑤ 元组投影改字段读，见 §8.3 第 14 条）。
> `key_rotation.rs`(8) 由 **C43** 归零退表（保留自愈 + 宏化，见 §8.3 第 10 条）；`qr_login.rs`(5)/`room_tag/mod.rs`(4) 已由 **C42** 归零退表；`feature_flags.rs`/`filter.rs`
> 由 **C41** 归零；`admin_media.rs`(15) 的 U-5 残量已由其批次**计入基线冻结**（`a13f57316`），
> 是否回收属该批次后续决定。
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
   🚫 缓存只跑 `--static` + `--compile`；**禁止** `--full` 与裸 `cargo sqlx prepare`（D-77）——
   写入唯一入口是 `scripts/ci/sqlx_prepare.sh`。
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

6. ✅ **D-75/D-76/D-77（共享 `public` 被清空 + `.sqlx` 入口的两条禁令）已完成（2026-09-26）** ——
   事故：`synapse_test.public` = **0 表**（本该 ~220；`prepare_test_db.sh` [4/4] 也断言 ≥200），
   于是 `check_sqlx_cache_fresh.sh --full` 吐出 **1443 个 E0282/E0277**（看起来像源码坏了）。
   三条独立缺陷（全部已修 + 自证）：
   - **D-75 `converge_public_schema.sh` TOCTOU**：删除清单在 rail 1 之后**又一次**在 apply 的
     heredoc 里现场求值 `$DIFF_SQL`，而 `prepare_test_db.sh` 的 [2/4] 步会
     `DROP SCHEMA test_template_ci CASCADE` 再花数十秒重建参考集 ⇒ 参考为空时 public 的**全部**
     对象都被判"多余"；更糟的是事后不变量（rail 5）也用**同一个已塌掉的参考集**求值，
     `extra=0 / missing=0` 两边同时退化成 0 ⇒ **恒过**（日志只有一行 `public tables=0 / 参考=220`）。
     **修法（结构性）**：① 删除清单**冻结**（落盘 + `\copy` 进临时表，apply 期间不再咨询参考 schema）；
     ② apply **前**复测参考表数，低于 rail 1 的值即拒绝执行、一个对象都不删；
     ③ **大删栏杆**：若清单执行后 public 的存活对象数会少于参考对象数 ⇒ 拒绝
     （`CONVERGE_ALLOW_MASS_DELETE=1` 才放行）；④ **非空不变量**：收敛后还要求参考仍 ≥ rail 1 的表数、
     public ≥ `MIN_TABLES`。
     真库复跑：`report`/`apply` 均 `多余=0 缺失=0 public 220 / 参考 220`；参考 schema 不存在时 rail 1 拒绝。
   - **D-76 `init_test_public_schema.sh` 破坏性默认**：`RESET_PUBLIC` 默认 1 ⇒ **裸跑就等于**
     `DROP SCHEMA public CASCADE` 重建共享 `synapse_test.public`，失败/被 Ctrl-C/超时打断即留下 0 表。
     **修法**：默认改为 **0**（幂等 apply），重置需显式 `RESET_PUBLIC=1`（注释里写清破坏性与恢复方式）。
   - **D-77 `.sqlx` 入口**：`--full` 对着 0 表 schema 的失败是**上千个误导性编译错误**；
     而裸 `cargo sqlx prepare` 的 destination 就是 `.sqlx/` 且**先清空再重写** ⇒ 一次误跑清空 1162 条缓存。
     **修法**：新增唯一入口 `scripts/ci/sqlx_prepare.sh`（`DATABASE_URL` 必须显式给出；解析到的 schema
     必须 ≥100 张 BASE TABLE 且含 `events`/`rooms`/`users`，否则 **fail fast 且不进入编译**；写入前快照、
     写完若条目数减少则打印被删清单并**回滚**，`ALLOW_CACHE_SHRINK=1` 才允许缩容）；
     `check_sqlx_cache_fresh.sh --full` 改为 `exec` 它的 `--check`（护栏只有一份实现）。
     补充（2026-09-26，psql-less 环境）：第二道前置检查依赖 `psql`；没有 `psql` 时可用绝对路径
     （`/opt/homebrew/opt/postgresql@15/bin/psql`）或 `SQLX_PREPARE_SKIP_DB_CHECK=1` 显式跳过
     （只跳过检查，快照 + 缩容回滚仍是硬不变量；`--check` 不允许跳过）。
     **环境已修复**：`RESET_PUBLIC=0 TARGET_SCHEMA=public` 幂等 apply ⇒ `public` 222 对象 / 220 BASE TABLE；
     `--full` 复跑 **exit 0**。
   - **自证（R11，全部实测）**：新增 `tests/unit/sqlx_cache_tooling_guard_tests.rs`（14 个用例，
     hermetic 假 `psql`/`cargo` + 假 `.sqlx`，不连库、CI unit 批次可跑）。**变异自证**：把冻结清单换回
     二次求值 ⇒ 红；删 rail 7a ⇒ 红；删大删栏杆 ⇒ 红；删 rail 8 ⇒ 红（这一条**最初没覆盖**，
     补了"参考在 apply 期间塌掉"的用例才变红 —— 顺带暴露第一版 stub 把 heredoc 短接、
     使"diff 只跑一次"断言**恒真**）；`RESET_PUBLIC` 默认改回 1 ⇒ 红；删 `sqlx_prepare.sh`
     前置检查 ⇒ 红。

7. ✅ **D-73（`e2ee_audit_log` 动作列收敛）已完成（2026-09-26）** —— 唯一未关闭项结清，
   §7 再次无待修项。改动（R4 的"结构上能保证就收紧 schema"分支）：
   - `migrations/00000000_unified_schema_v12.sql`：`CREATE TABLE` 去掉 `action`；新增一段幂等收敛块
     —— `DO` 块判断旧列是否还在并用它回填历史 NULL（`action` 是 NOT NULL，故 `SET NOT NULL` 在任何既有库上成立；
     直接 `UPDATE … SET operation = action` 会在**第二次**执行时 42703，因为本文件被
     `init_test_public_schema.sh`（`RESET_PUBLIC=0`）重复执行）+ `ALTER COLUMN operation SET NOT NULL`
     + `DROP INDEX IF EXISTS idx_e2ee_audit_log_action` + `DROP COLUMN IF EXISTS action`；删掉那条索引定义
     （否则全新库上 `CREATE INDEX … ON e2ee_audit_log(action)` 必然 42703）。
   - `synapse-storage/src/e2ee_audit.rs`：INSERT 去掉 `action`（列 + 一个 `&event.operation` 绑定，9 → 8 参数）；
     5 条 SELECT 去掉 `AS "operation!"` 断言（schema 现在自己保证非空），字段 doc 改为说明收敛。
   **R10 链（全部实测）**：① 先用**旧值自检哈希实现** —— 独立实现的 FNV-1a 64 对 HEAD 的 v12 逐字节算出
   `efd39fc561affd7a`（与常量吻合），改后复算得 `d36d33bfe358346c` 并同步
   `EXPECTED_BASELINE_FINGERPRINT`；② 守卫 5（`test_isolation_unification`）+ `migration_consistency`
   + `mod_guard` 合计 **25/25**；③ 断言该 schema 的契约用例：`migration_consistency_tests` 的折入索引清单
   只含 `idx_e2ee_audit_log_device` / `_room_event`（不含被删的 `_action`），无需改；④ `--static` / `--compile`
   + 两档 clippy 全绿。
   **验证**：`.sqlx` 6 删 6 增（5 个 SELECT 去断言 + 1 个 INSERT 少一列），走**新入口**
   `scripts/ci/sqlx_prepare.sh`（对私有库 `synapse_c19b_scratch`）；私有一次性库 `synapse_d73_test`
   跑 `prepare_test_db.sh`（`public`/`test_template_ci` 各 220 表）后：
   `-p synapse-storage --lib --features test-utils -E 'test(/e2ee_audit/)'` ⇒ **8/8**；
   `--profile ci --all-features --test integration --test-threads 1 -E 'test(/e2ee_audit/)'` ⇒ **11/11**
   （含 `audit_service_log_key_operation_round_trip` 与 cross-signing 验证路径）。
   ⚠️ 期间**共享 `synapse_test` 上出现跨会话死锁**（`ALTER TABLE … ADD CONSTRAINT` 的
   AccessExclusiveLock 与另一会话的 AccessShareLock 互等，`prepare_test_db.sh` 因 `ON_ERROR_STOP` 中止）——
   属并发 seed 的环境问题，不是本批代码缺陷；本批改用**私有库**完成全部验证，共享库事后核对
   `public`/`test_template_ci` 仍各 220 表且已是新 schema。
   ⇒ **新教训（已写进 AGENTS.md R10）**：schema 变更在**合并进 `opt/consolidated` 之前**不要施加到共享库
   —— 别的 worktree 还拿着旧基线，其 `CREATE INDEX … ON e2ee_audit_log(action)` 会 42703；要么先用私有库
   验证，要么合并后再动共享库。

8. ✅ **C41（`filter.rs` 5 + `feature_flags.rs` 5 = 10 处）已完成（2026-09-26）** ——
   两个单表模块的生产区动态归零，无 feature 门控。转换全是文档化的机械处理：`query_as!`
   按列名构造（`filters` / `feature_flags` / `feature_flag_targets` 的投影列与结构体字段
   一一对应且都是 NOT NULL ⇒ **无需任何 `AS "col!"` 断言**）；事务内的两条
   （`feature_flag_targets` 的 DELETE / INSERT）直接跑在 `&mut *transaction` /
   `&mut **transaction` 上；`request.status.as_deref().unwrap_or("draft")` 原样保留（R5）。
   覆盖率先行核对（STEP 0）：`filter.rs` 5 个方法逐一有调用者且 in-file `db_tests` 覆盖
   create/get/缺失/list/两种 delete；`feature_flags.rs` 的 create/update/get/replace_targets
   由 `feature_flags_storage_tests_migrated` + `api_feature_flags_tests` 端到端覆盖。
   **本批未新增缺陷**（无 `PgRow`、无吞错、无零调用者语句）。
   验证：`-p synapse-storage --lib --features test-utils -E 'test(/filter::/) or
   test(/feature_flags/)'` ⇒ **9/9**；集成 `-E 'test(/feature_flag/) or test(/filter/)'`
   ⇒ **131/131**。
   ⚠️ 过程中撞到一次**"库比树旧"**的假红：首次跑该集成子集有 3 条失败（`Failed to create room`），
   根因是私有库 `synapse_merged_test` 是在并发批次又改迁移（+52/−8）**之前** seed 的 ——
   用当前树重灌 `public` + `test_template_ci`（224 对象）后 3/3 转绿。教训与 R10 的
   "共享库不是稳定输入"同源，只是这次是自己的一次性库。

9. ✅ **C42（`qr_login.rs` 5 + `room_tag/mod.rs` 4 = 9 处）已完成（2026-09-26）** ——
   两个单表模块（MSC4388 二维码登录 / 房间标签）的生产区动态归零。
   两个 R6/R5 要点：① `RoomTag.order` 带 `#[sqlx(rename = "order_value")]`，`query_as!` 不认
   rename ⇒ SQL 改写 `order_value AS "order"`（raw string 用 `r#"…"#`）；②
   `get_qr_transaction` 原为 7 元组投影，`query_as!` 不收元组（R6⑤）⇒ 直接改
   `query_as!(QrTransaction, …)`（字段名与列名一一对应，顺带删掉手工映射闭包）。
   其余 7 处为 `query!` 的 INSERT/UPDATE/DELETE；三张表的列可空性与字段一致，无需断言。
   验证：storage `-E 'test(/qr_login/) or test(/room_tag/)'` ⇒ **17/17**；
   集成 `-E 'test(/room_tag/) or test(/qr_login/)'` ⇒ **2/2**（`room_tag_storage_tests_migrated`）。
   ⚠️ **同批 STEP 0 把一个候选排除出转换范围，并立了新条目**：
   `synapse-federation/src/key_rotation.rs` 的 8 处 literal 里 **7 处**是**与迁移重复的运行时
   DDL 引导**（3 组「`SELECT EXISTS(信息模式/pg_indexes)` + 条件 `CREATE`」 + 1 个
   `CREATE TABLE IF NOT EXISTS`）；`federation_signing_keys`（迁移 1719）、其两条索引
   （3545-3546）、`key_rotation_config`（2245）**都已由迁移创建** ⇒ 在任何已迁移库上那些
   `CREATE` 分支**不可达**，且该自愈路径**不受** `SYNAPSE_ENABLE_RUNTIME_DB_INIT` 管辖。
   按 R12（死代码先删再转）**不应**把这 7 处转成宏 ⇒ 登记为 **D-78**（§7.1，两条可选修法），
   该文件本批一行未动。这正是 R12「转换前先查死代码」省下的一次转换 + 一次 `.sqlx` 往返。

10. ✅ **C43（`key_rotation.rs` 8 处宏化）+ **D-78 改判** 已完成（2026-09-26）** ——
   本批是"**先核意图再动手**"的一次纠偏：C42 的 STEP 0 曾把该文件判为"7/8 是与迁移重复的死 DDL、
   应删除"并登记 D-78；C43 在准备删除时去核**覆盖证据**，发现
   `tests/integration/federation_service_tests_migrated.rs::test_load_or_create_key_recovers_missing_signing_key_table`
   —— **自愈是有意行为**（用例名明确承诺"缺失后恢复"）。因此**撤销删除**，改为**保留自愈 + 宏化 8 处**
   （`git checkout` 复原后重做）：
   · 3 处 `SELECT EXISTS(信息模式/pg_indexes)` ⇒ `query_scalar!` + `AS "exists!"`
     （无关系来源 ⇒ 宏推可空；EXISTS 恒 TRUE/FALSE、永不为 NULL ⇒ R4 断言理由成立）；
   · 4 处 DDL ⇒ `query!`（R6④：DDL/utility 可宏化，describe.columns == []）；
   · `load_rotation_config` 的 `interval_ms` ⇒ `query_scalar!`（与同函数另外三条同型，顺带去掉
     冗余的 `::<_, String>` 与闭包类型标注）。
   实测：`dynamic_production` 263 → **255**（−8）、`static` 1225 → **1233**（+8）、
   `.sqlx` 1193 → **1201**（+8）、literal 192/43 → **184/42**。
   验证：`-p synapse-federation --lib --all-features -E 'test(/key_rotation/)'` ⇒ **13/13**；
   集成 `-E 'test(/federation_service_tests_migrated/)'` ⇒ **5/5**（含上述恢复用例与
   `test_key_rotation_initialization`，真库往返穿过转换后的宏）。
   ⚠️ **同时发现并新立 D-79**：那条"恢复"用例**从未构造表缺失**（跑共享库、表本就在）⇒
   自愈 DDL 分支至今**零执行**；而 `synapse-federation` **没有 per-test schema 基建**
   （无 `IsolatedTestPool`/`BASELINE_SQL`，dev-deps 只有 `wiremock`），在共享 `public` 上删表
   会波及并发用例（D-57/D-75/D-76 的老坑）⇒ 不能靠"小心删一下"造场景。修法见 §7.1。
   教训：**"看起来是死代码"必须先核覆盖与命名意图**；本例里 C42 的判断本身也是一次
   "名字/直觉 ≠ 事实"的同型错误（只是方向相反）。

11. 🔧 **C44-0（先修 + 先补覆盖）已完成（2026-09-26）** —— 为 C44 的两件前置：
   - **先修（铁律 1 / R12）**：`synapse-e2ee/src/backup/service.rs::get_backup_count_per_room`
     全仓**零调用者**（仅定义处与 doc 链接；路由/服务/`tests/` 全无引用）⇒ 直接删除，
     而不是把它转成宏（省下一次转换 + 一条 `.sqlx` 条目）。该文件字面量 5 → **4**，
     `dynamic_production` 255 → **254**、literal 184 → **183**（文件数仍 42）。
   - **先补覆盖（R8/R9）**：`schema_health_check.rs` 的 4 条检查 SQL
     （`check_missing_tables` / `check_missing_columns` / `check_missing_indexes` /
     `check_field_naming_issues`）此前**只有 CI 的 `schema_health_check` 二进制在跑**，
     `cargo nextest` 侧一条都没有（模块 12 条用例全是纯函数）。新增 `db_tests`：
     ① 真隔离库上跑 `run_schema_health_check`，断言 `missing_tables` / `missing_columns` 为空、
     `baseline_drift` 在界内；② 反向自证：换成不存在的表后 `check_missing_tables` 必须报出来。
   - ⚠️ **这条新用例当场抓到一个真缺陷 → D-80**：隔离 clone 把 PK/UNIQUE 约束支撑的索引名
     改成 PG 默认名（模板 `pk_users` / `uq_users_username` / `pk_presence` /
     `uq_access_tokens_token_hash` / `uq_user_threepids_medium_address` → 克隆
     `users_pkey` / `users_username_key` / …），而 `synapse-common/src/test_isolation.rs`
     的文档与它自己的用例都承诺"精确还原"，`validate_clone` 只比数量故无人可见。
     因此该用例目前**只能容忍这 5 组**（并在注释里写明"D-80 修好后收紧为 `is_empty()`"）。
   - 验证：`-p synapse-storage --lib --features test-utils -E 'test(/schema_health_check/) or
     test(/backup/)'` ⇒ **14/14**。

12. ✅ **D-79（`synapse-federation` 补 per-test schema 基建）已完成（2026-09-26）** ——
   该 crate 此前**没有任何 DB 测试基建**（无 `IsolatedTestPool` / `BASELINE_SQL`，dev-deps
   只有 `wiremock`），所以 `key_rotation.rs` 的自愈 DDL 分支（"表缺失→重建"）**无法被构造**：
   在共享 `public` 上 `DROP TABLE` 会波及并发用例（D-57/D-75/D-76 的老坑），集成用例
   `…_recovers_missing_signing_key_table` 的名字承诺了该场景、测试体却只跑普通路径。
   处置：
   - 新增 `synapse-federation/src/test_isolation.rs`（crate 本地适配器，镜像
     `synapse-storage/src/test_isolation.rs`：`pub use` 共享 `IsolatedTestPool` + 自己的
     `include_str!` 基线副本），`lib.rs` 以 `#[cfg(test)] pub mod test_isolation;` 声明；
   - 该副本是**第 4 份** `BASELINE_SQL`，已纳入 `tests/unit/test_isolation_unification_tests.rs`
     的副本清单（新增 `FEDERATION` 常量并 chain 进指纹循环）—— 漏列就等于让第 4 份可以悄悄漂移；
   - `key_rotation.rs` 新增 `mod db_tests`（隔离 schema）：**先 `DROP TABLE`** 再调用自愈入口，
     断言 `federation_signing_keys` 表与**两条索引**都被重建、密钥可读（覆盖 C43 那 7 处 DDL 站点）；
     另一条覆盖 `key_rotation_config`：删表 → `load_rotation_config()` → 表重建 + 空表走默认值 +
     `set_rotation_config_value` 回写后重新加载能读到新值；
   - 集成用例改名为 `test_load_or_create_key_persists_a_signing_key`（诚实描述它实际测的是什么）
     并加注释指向真正构造恢复场景的隔离用例。
   **R11 自证**：把两个 `ensure_*` 改成 `return Ok(())`（no-op）⇒ 两条新用例**都变红**；
   恢复后 `-p synapse-federation --lib --all-features -E 'test(/key_rotation/)'` ⇒ **15/15**
   （13 + 2），集成 `-E 'test(/federation_service_tests_migrated/)'` ⇒ **5/5**，
   统一守卫 `test_isolation_unification`（含新增副本）⇒ **13/13**。

13. ✅ **D-80（隔离 clone 与模板同形）已完成（2026-09-26）** —— 由 C44-0 那条"真 baseline
   健康检查"用例当场暴露，修复过程中又暴露出**第二处**：
   - **① 约束名被 PG 改名**：`clone_table_chunk_statement` 的 phase 1d **显式跳过**约束支撑的索引，
     注释还断言"`LIKE` already preserves the PRIMARY KEY name"。用 psql 探针实测推翻：
     `CREATE TABLE c (LIKE t INCLUDING ALL)` 把 `pk_t` → `c_pkey`、`uq_t_v` → `c_v_key`
     （CHECK 名 `ck_t_w` **保留**）。而 `validate_clone` 只比**数量**（文档自述
     "a rename is invisible to it"）⇒ 隔离库与 `public` 长期不同形却无人可见。
   - **② 物化视图索引根本没搬运**：`LIKE … INCLUDING ALL` 只处理表，phase 1d/1e 也只在表上工作，
     `rooms_summaries_mv` / `public_room_directory` 上的 6 条索引
     （`idx_rooms_summaries_mv_{creator,members,public_activity,room_id}`、
     `idx_public_room_directory_{members,room_id}`）在克隆里**一条都不存在**。
   修法（`synapse-common/src/test_isolation.rs`）：
   - **phase 1e（新）**：按 `(table, contype, pg_get_constraintdef)` 配对（定义文本不含名字，
     `LIKE` 保留列序，故两侧同定义即同约束；`row_number()` 处理重复定义），对 `contype IN ('p','u','x')`
     执行 `ALTER TABLE … RENAME CONSTRAINT <clone> TO <template>` —— 约束改名会连带改索引名；
   - **phase 2**：建完 `CREATE MATERIALIZED VIEW` 后，按模板的索引 DDL 逐个重建
     （`pg_get_indexdef` 去掉模板限定名后直接执行，索引名保持模板的）；
   - **`validate_clone`**：新增**索引名字集合**比对（`EXCEPT` 差集，失败时列出缺失名字），
     并订正模块里被推翻的那句注释。
     ⚠️ 这条新校验落在 `synapse-common/src/test_isolation.rs` —— **无条件编译**的测试基建，
     按口径算**生产区**，因此它第一版写成 `sqlx::query_as(...)` 时 ratio 门禁当场红
     （`production=255 > 254`）。按 R1 改成 `query_scalar!`（单列 ⇒ R6①；`EXCEPT` 差集不给
     NOT NULL 透传 ⇒ R4② 断言 `AS "name!"`，理由是该列取自 `pg_class.relname`，catalog 名
     永不为 NULL）⇒ 只增静态：`static` 1233 → **1234**、`.sqlx` 1201 → **1202**、生产动态回到 254。
     这是本批唯一一次动到棘轮，且方向是收紧。
   **R11 自证（两处，均实测）**：去掉 phase 1e ⇒ 扩展后的探针用例
   `clone_preserves_index_and_unique_constraint_names` 变红（该用例的基线已补上**具名**约束
   `CONSTRAINT pk_unify_named` / `uq_unify_named_v` —— 旧探针用无名 PK + `CREATE UNIQUE INDEX`，
   两者恰好都能存活，所以缺陷漏了过去）；缺 phase 2 那段 ⇒ 新的名字比对当场列出 6 个缺失名
   （即 C44-0 用例的失败信息）。
   收紧：`schema_health_check::db_tests::baseline_satisfies_the_schema_health_checks` 从
   "容忍 5 组约束索引改名"改回 `assert!(result.missing_indexes.is_empty())` ⇒ **14/14**
   （真基线克隆上与 `public` 同形）。
   意义：隔离库第一次与 `public` **同形** ⇒ 跑在隔离库上的 `has_index_named` 类断言
   （集成里 24 处）从此不再"只在共享 schema 上碰巧为真"。

14. ✅ **C44（`backup/service.rs` 4 + `audit.rs` 4 + `schema_health_check.rs` 4 = 12 处宏化）已完成
   （2026-09-26）** —— 三个模块的生产区字面量动态 SQL 全部归零：
   - `synapse-e2ee/src/backup/service.rs`：2 条 `COUNT(*)` 计数（`sqlx::query` +
     `try_get::<i64,_>("count")`，列名/类型一路吞到运行期）⇒ `query_scalar!`（`COUNT(*)`
     无关系来源 ⇒ R4 ① 断言 `AS "count!"`，聚合恒一行且永不为 NULL）；2 条 8 列投影 ⇒
     `query_as!`（列清单本就显式 ✓ R3；`COALESCE(kb.backup_id_text, kb.version::text)` 同为
     无关系来源 ⇒ 断言 `AS "backup_id!"`，理由：`version` 是 `NOT NULL DEFAULT 1`）。
     转换后 `use sqlx::Row` 失去唯一使用者 ⇒ 随批删除（否则 clippy 红）。
   - `synapse-storage/src/audit.rs`：`get_event` / `insert_audit_event`（`RETURNING` 9 列全
     `NOT NULL`）⇒ `query_as!`；`set_config(...)` 是**单列 SELECT** ⇒ 无 `.execute()`（R6 ①）
     ⇒ `query_scalar!` + `fetch_one`（函数调用按可空推断 R6 ③，值本就要丢 ⇒ `let _ =`）；
     `DELETE … < $1` ⇒ `query!` + `.execute()`（`rows_affected()` 与 append-only 逃逸路径逐字保留）。
   - `synapse-storage/src/schema_health_check.rs`：两处 `= ANY($1)` 的参数是 `&[&str]`/`Vec<&str>`
     ⇒ 宏 `ty_match` 拒绝（R5）⇒ 改 owned `Vec<String>`；`unnest($1::text[], $2::text[])` 的
     **元组投影**不能用 `query_as!`（R6 ⑤）⇒ `query!` + 字段读；三处查询的列全来自**系统视图**
     （`information_schema.tables` / `pg_indexes` / `unnest` 结果集），PG 的 Describe **不给视图列
     透传 NOT NULL**，而这些列分别取自 `pg_class.relname`（catalog 名，永不为 NULL）⇒ R4 断言。
   实测：`dynamic_production` 254 → **242**（−12）、`static` 1234 → **1246**（+12）、
   `dynamic` 总数 977 → **965**、`dynamic_test` 723 不变、`.sqlx` 1202 → **1214**（+12）、
   literal 183/42 → **171/39**（三文件退表）；runtime 恒等式 `242 − 171 − 1 = 70` 仍成立
   —— 即 12 处**全部**转为静态，未产生新的运行期拼装。
   **同批补覆盖（R8/R9）**：`service.rs` 的四个被转方法是 **service 层**读路径，原有 DB 覆盖只到
   storage 层，集成用例用的是**手搭简化 schema**（`version` 无 `NOT NULL`、`first_message_index`
   可空 —— D-36 允许 D-46 藏身的形态）⇒ 新增 `backup::service::db_tests::
   service_reads_round_trip_on_the_migration_template`，在真 v12 基线上覆盖两条 `COUNT(*)` 的
   文本/数字回退/未命中三路、8 列投影、以及 **`backup_id_text IS NULL` 的行必须靠 `version::text`
   被找到并由 `COALESCE` 回填**（`create_backup` 永远写非空 `backup_id_text`，该分支只能直接造）。
   代价是测试区 +2 处夹具动态站点（`dynamic_test` 723 → 725，`BASELINE_DYNAMIC_TEST_INFRA` 同步上调，
   理由：`#[cfg(test)]` 内宏不进 `cargo sqlx prepare` ⇒ R9/D-13，无静态等价物）。
   **R11 自证**：把 `get_all_backup_keys` 的 `COALESCE(...)` 变异回 `kb.backup_id_text` ⇒ 默认离线
   模式下**编译期**即报 `SQLX_OFFLINE=true but there is no cached data for this query`（SQL 文本
   变了、缓存无对应条目），复原后即绿。

15. ✅ **D-81（A3+A4 新增集成测试编译失败）已先修（2026-09-26）** —— 与本批无关的**既有红门禁**，
   按 R11"先修再转"独立提交。`b38380d9b`（A3+A4，`opt/consolidated` 并入）新增的
   `tests/integration/state_groups_backfill_tests.rs` 调用了 `unique_id()` 却**从未定义它**
   —— `unique_id` 在本仓是**各测试文件自己的**小助手（`state_groups_idempotency_tests.rs`、
   `beacon_storage_tests_migrated.rs` 等各自定义一份），不是共享工具，所以编译器只报
   "cannot find function"（并提示若干同名函数"inaccessible"）。同文件还有一处未使用的
   `StateGroupStateEntry` 导入。后果：集成测试 target 编译失败 ⇒
   `cargo clippy --workspace --all-targets --features test-utils --all-features -- -D warnings`
   与 `bash scripts/ci/check_sqlx_cache_fresh.sh --compile` 双双红在分支上（两者都是 CI 阻断门禁）。
   修法（最小、不碰他人在途文件）：在**该文件内**补一份 `static TEST_COUNTER: AtomicU64` +
   `fn unique_id()`（计数器起点 10_000，避开同 target 其它文件的取值区间）+ 删无用导入。
   验证：`cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings`
   ⇒ exit 0；`check_sqlx_cache_fresh.sh --compile` ⇒ OK；fmt `current=0=baseline`。
   教训：新增测试文件"看起来编译过"的假象来自**只跑单 target 的窄命令**
   （`nextest run -p <crate>` 不会编译 root 的 integration target），
   必须跑 `--all-targets` 那一档才能发现。

### 8.4 收尾条件（何时可称"静态化战役结束"）

- `dynamic_production` 的**可机械转换部分（literal）归零**：242 → **101**
  （242 − 140 literal − 1 param = 101 = 测试基建 57 + 分页结构性 15 + **D-14 结构性 29**），
  或每个残留都有 §7.3 那样的登记条目；
- literal 逐文件表只剩 4 类（3 个测试基建文件 + `event/pagination.rs`）；
- ~~D-68 接线~~、~~D-37 收敛~~、~~D-62 修法①~~、~~D-57② 收敛~~、~~D-73 结构性收敛~~、
  ~~D-79 隔离池基建~~、~~D-80 隔离同形~~ **均已落地**；§7 **无未关闭项**（7 条结构性例外除外）；
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
