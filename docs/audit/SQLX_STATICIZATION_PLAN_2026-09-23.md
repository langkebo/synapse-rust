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
| `dynamic_production` | 1532（近似） | **184** | **−88.0%** |
| `static` | 61 | **1295** | +1234 |
| `dynamic`（总） | 2151 | **919** | −1232 |
| 静态占比 | 2.76% | **58.7%**（1295 / 2207） | +55.9pp |
| `.sqlx` 离线缓存 | 60 条 | **1263 条** | +1203 |
| literal（逐文件棘轮，处 / 文件） | 876 / 98 | **113 / 31** | −763 |
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
> `#[cfg(test)]` 夹具：插 `rooms` 行满足真实 FK + 直接造一条 `backup_id_text IS NULL` 的行）；
> C45-0（先修：删 3 处零调用者方法 + 消 1 处重复 INSERT/静默吞错 + 补真基线覆盖）收到
> **238 / 1246**（`.sqlx` 不变 1214，literal 退到 167，测试区 725 → 726）；C45（4 处宏化）收到
> **234 / 1250**（`.sqlx` 1214 → 1218，literal 退到 163/38，该文件生产区动态归零）；
> C46-0（先修 D-91/D-92 + 补覆盖，测试区 726 → 733）与 C46（8 处宏化）收到 **226 / 1258**
> （`.sqlx` 1218 → 1226，literal 退到 155/37，`event/dag.rs` 生产区动态归零）；
> C47-0（先修：删 1 处零调用者 trait 方法 + 补真基线覆盖）收到 **225 / 1258**
> （`.sqlx` 不变 1226，literal 退到 154/37）；C47（6 处宏化）收到 **219 / 1264**
> （`.sqlx` 1226 → 1232，literal 退到 148/36，`delayed_events.rs` 生产区动态归零）；
> C48-0（先修：删 3 个零调用者方法 + 把 R9 违规用例改成真基线覆盖）收到 **216 / 1264**
> （`.sqlx` 不变 1232，literal 退到 145/36，测试区 733 → 732）；C48（5 处宏化）收到
> **211 / 1269**（`.sqlx` 1232 → 1237，literal 退到 140/35，该文件生产区动态归零）；
> C49-0（先修 D-93：消 2 处 `PgRow` 泄漏 + 1 处吞错）收到 **208 / 1271**
> （`.sqlx` 1237 → 1239，literal 退到 137/35）；C49（4 处宏化）收到 **204 / 1275**
> （`.sqlx` 1239 → 1243，literal 退到 133/34，该文件生产区动态归零）；C50（门控批，7 处宏化）
> 收到 **197 / 1282**（`.sqlx` 1243 → 1250，literal 退到 126/33，该文件生产区动态归零）；
> C51（`event/search.rs` 6 处，单批）收到 **191 / 1288**（`.sqlx` 1250 → 1256，literal 退到
> 120/32，该文件生产区动态归零）；C52-0（补真基线生命周期覆盖，生产区不变）收到
> **191 / 1288**（`.sqlx` 不变 1256，literal 不变 120/32，测试区 732 → 733）；C52（6 处宏化）
> 收到 **185 / 1294**（`.sqlx` 1256 → 1262，literal 退到 114/31，该文件生产区动态归零）；
> C53-0（先修 D-94 两处吞错 + 补覆盖 + 登记 D-95/D-96）收到 **184 / 1295**
> （`.sqlx` 1262 → 1263，literal 退到 113/31，测试区 733 → 735）。

### 0.2 残量结构（"还剩多少活"的准确说法）

| 组成 | 处数 | 性质 |
|---|---|---|
| **可静态化残量** | **112** | **82 处字面量**（纯机械转换）+ **29 处运行期拼装**（`format!` 拼列清单 / `ORDER BY` 方向等，**属 §7.3 D-14 结构性例外：需先设计替代方案，不能靠硬编码压数字**）+ **1 处跨函数传参**（`param`，把字面量内联到调用点即可转） |
| 测试基建（有意保留） | 57 | `synapse-test-utils/src/lib.rs` 28、`synapse-common/src/test_isolation.rs` 25、`test_schema_guard.rs` 4 |
| 结构性保留（有意） | 15 | `synapse-storage/src/event/pagination.rs`（9 runtime 游标/排序方向 + 6 literal） |
| **合计** | **184** | = 112 + 57 + 15 |

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

### 0.4 缺陷发现总览（**96 条**；只给统计与去向，不逐条显示）

| 类别 | 条数 | 说明 |
|---|---|---|
| ① 真 schema 下必然失败 | 15 | 列名写错、INSERT 漏 NOT NULL 列、路由背靠不存在的表、`WHERE $2 != '[]'` 在 prepare 阶段必败、绑定类型必败… |
| ② 门禁自身失效或长期红 | 9 | 假绿（`--static`、`0 tests`、陈旧 `public`、魔数下界）与长期红（`--all-features` clippy、悬空夹具、契约未同步） |
| ③ 吞错 / 非确定性 / 缓存不生效 | 8 | `unwrap_or_default`、`.ok().flatten()`、`let _ = <future>`、缺决胜键、值恒为 0 |
| ④ 死代码 / 第二份实现 / 空壳 / 遗留 schema | 12 | 零调用者语句与包装、第二份写入实现、`RowNotFound` 空壳端点、删表后遗留 schema |
| ⑤ 可空性 / 解码类型不符 | 4 | 可空列配非 `Option` 字段、jsonb 解成 `Vec<String>` |
| ⑥ 覆盖缺口 / 测试基建假绿 | 2 | 静态化后无 DB 往返、自建 schema 掩盖写入端约束 |
| ⑦ 文档级 | 6 | 计数漂移、过时结论、误导性"规则"注释 |
| ⑧ 结构性例外（有意保留） | 8 | D-13 / D-14 / D-18–D-22 / **D-96**（宏无法 describe 可选扩展 `pg_stat_statements` 的关系），见 §7.3 |
| ⑨ 阶段总结后新发现并已关闭 | 31 | **D-94**（`monitoring.rs` 两处吞错：`pg_stat_statements_enabled` 的 `.unwrap_or(false)` 与慢查询三元组的 `.unwrap_or((0.0, 0, total_transactions))` 都把**数据库错误**降级成"没有数据"，监控报告因而静默失真 ⇒ 改 `?` 传播；三个聚合列在空集上的 `unwrap_or` 是真默认值，保留）、**D-93**（C49-0 删两处 raw-`PgRow` 方法 D-93 —— 该条当时漏记进本表，本次一并补上）、**D-91**（`synapse-storage/src/event/dag.rs` 两处 `serde_json::from_value(...).unwrap_or_default()` 把 `events.prev_state_events` 的**形状错误**静默降级成"没有前驱状态事件"，与"该列本就 NULL"不可区分 ⇒ 吞错。抽 `prev_state_events_from_json()` 改 fail-closed（`sqlx::Error::Decode`））、**D-92**（`get_forward_extremities_count` 读旧 JSONB 约定 `content->>'prev_event_id'`（现代写入只写 `event_edges` ⇒ 子查询恒空）+ 额外 `state_key IS NOT NULL` ⇒ 实际返回**状态事件数**，与 `get_forward_extremities_in_room` 的 DAG 叶节点不是同一概念（同一职责的第二份实现）；管理员端点 `forward_extremities` 字段长期报错数 ⇒ 改为同一 `event_edges` 定义）、**D-89**（**生产缺陷**：`event_edges.prev_event_id` 是 `NOT NULL`，而 P1-3 折入块给它加的 FK 动作是 `ON DELETE SET NULL` ⇒ 删任一父事件即 23502，**管理员删房间恒 500**（实测 `DELETE /_synapse/admin/v1/rooms/{room_id}` ⇒ `M_UNKNOWN`，服务端日志 `null value in column "prev_event_id" of relation "event_edges" violates not-null constraint`）。改为 `ON DELETE CASCADE`（派生 DAG 边随节点消失，与同表 `event_id` 侧一致），并让折入块先 `DROP CONSTRAINT IF EXISTS` 再 `ADD`，使 `init_test_public_schema.sh` 的**重放**也能替换旧定义。按 R10 复算基线指纹 `f6e8cb1fdbe20a67` → `5906517503a8cdc6`）、**D-90**（既有红①收口：`auth_issuer` 端点由 `76e5f9136` 有意摘除，用例却仍断言"存在但拒绝"（400 `M_UNRECOGNIZED`）⇒ 改为"废弃端点不得复活"的 **404** 守卫）、**D-88**（`api_widget_tests::test_create_widget_allows_joined_room_moderator` 的 helper 把**创建者**写进了 `m.room.power_levels.users`，而 `create_room` 建的是 v12 房间 ⇒ 撞上 MSC4289 rule 10.4（`synapse-services/src/auth/power_levels.rs:244-271`：v12+ 房间的 `users` 不许点名创建者，创建者权力本就是无上限的）⇒ 房主自己那条 PUT 得 **403 `M_FORBIDDEN: power_levels.users must not name a room creator`**，用例在看错的地方报红。**生产侧规则是对的，是用例侧违规** ⇒ helper 只写非创建者（moderator 50）并删掉多余的 `owner_user_id` 形参；R11 自证：把 moderator 的 PL 降到 0 ⇒ 用例在 widget 创建断言处真的红（证明该断言不是空跑））、**D-87**（重活门禁实测：`docs/openapi/route-table.json` 与两条 `route_ledger_*.snapshot` 停在 2026-09-25，而 U-5（`a13f57316`）新增的 10 条 admin media/invite 路由、`auth_issuer` 路由的删除（`76e5f9136`）都只落在 live router 里 ⇒ `declared_route_manifest_full_snapshot_matches_{default,worker_enabled}_state` 与 CI 的 `openapi-artifact` 两道 `--check` 都会红。按 CI 的产物流水线用固定时间戳重生成：快照 count 1120 → **1130**、`route-table.json` 1098 → **1130**（`client.yaml` 由未变的 `ledger.json` 生成，逐字节不变）。**未**使用 `cargo insta accept`，也**未**改断言）、**D-84**（D-79 给 `synapse-federation` 补的隔离池基建让 `key_rotation.rs::db_tests` 两处 `DROP TABLE` 进了 test 区，却**没加进 `scripts/ci/test_ddl_allowlist`** ⇒ D-36 守卫 A 在 `opt/consolidated` 上红；这正是"重活门禁"（`--test unit`）才跑得到的一道。已按名单的正统语义补两条键并写明理由：`DROP TABLE` 是**被测对象**（"表缺失 ⇒ 自愈重建"分支的唯一入口），且两条用例都跑在 `isolated_test_pool()` 的 per-test schema 上）、**D-85**（C45-0 删掉 `recover_pending_from_db` 后，它那条 `ORDER BY created_ts`（单键、无决胜键）一并消失，而 `scripts/ci/ts_order_single_key_baseline` 仍记着 `event_broadcaster.rs 1` ⇒ 该棘轮"基线比实测宽"必然红，要求 `--update` 收紧。已 `--update`（72 处 / 30 文件））、**D-86**（A3+A4 新增的 `state_groups_backfill_tests::test_all_v12_rooms_have_state_groups`：`rooms.room_version` 是 **TEXT**（`DEFAULT '6'`），裸比较 `room_version >= 12` 让 PG 把字面量定成 integer ⇒ `42883 operator does not exist: text >= integer` 必红；且那句 `v12_with_state_groups <= total_v12` 是**恒真**断言（左是右的子集计数）⇒ 属"不会失败的门禁"。已改写为限定在本用例自建房间上、且带**反向对照**的判定验证）、**D-83**（`bf90f430f`（B1-1b 本地事件自动补图元数据）新增了 DI 接缝 trait `GraphMetadataSource`（`synapse-services/src/graph_metadata.rs:103`，有生产实现 `StorageGraphMetadataSource` + 测试替身 `FakeSource` + 注入点 `Arc<dyn GraphMetadataSource>`）却**漏同步 `scripts/ci/trait_count_baseline`** ⇒ `repo-sanity` 的 trait 棘轮自该提交起在 `opt/consolidated` 上**常驻红**（66 → 67）。按既有先例（`InvitePolicyGate` 65 → 66）与 R11 独立收紧基线并写明理由，而不是放宽脚本；`*StoreApi` 33 == 33 未放宽，见 §8.3）、**D-82**（`synapse-federation/src/event_broadcaster.rs` 的 `send_batch` 把 `persist_transaction_to_db` 的 `INSERT` **复制了第二份**（铁律 2），副本还把 `event_type` 硬编码 `'m.room.event'`、`room_id` 恒 `NULL`，并用 `.ok()` 把写库错误**静默吞掉**（既无日志也不区分"没配库"与"写库失败"）⇒ 抽出共享 `persist_transaction_row` 并统一 `error!` 记录，见 §8.3）、**D-81**（`b38380d9b`（A3+A4）新增的 `tests/integration/state_groups_backfill_tests.rs` **从未定义**它自己调用的 `unique_id()` （每个测试模块都是文件本地助手，不是共享工具），且 `StateGroupStateEntry` 导入未使用 ⇒ 集成测试 target 编译失败（E0425 ×2 + unused import ×1），`--all-targets --all-features` 的 clippy 与 `check_sqlx_cache_fresh.sh --compile` **在 `opt/consolidated` 上双红** —— 而这两道正是 CI 的阻断门禁。修法：补齐本文件的 `unique_id()`（`AtomicU64` 计数器，与同批 `state_groups_idempotency_tests.rs` 同形）+ 删无用导入，见 §8.3）、**D-80**（隔离 clone 与模板不同形，**两处**：① phase 1d 显式跳过约束支撑的索引、且注释断言"`LIKE` 已保留 PRIMARY KEY 名"——实测**不成立**（`pk_users`→`users_pkey`、`uq_users_username`→`users_username_key`）；② 物化视图上的索引**根本没被搬运**（克隆里 `idx_rooms_summaries_mv_*` ×4 + `idx_public_room_directory_*` ×2 全缺）。修法：新增 phase 1e 按 `(table, contype, pg_get_constraintdef)` 配对后用 `ALTER TABLE … RENAME CONSTRAINT` 还原 PK/UNIQUE/EXCLUDE 名（CHECK 名本就保留）、phase 2 在建 matview 后按其模板索引 DDL 逐个重建、`validate_clone` 从"只比数量"改为**索引名字集合**比对（这正是它长期不可见的原因）。R11 自证：去掉 phase 1e ⇒ 扩展后的探针用例红；缺 phase 2 那段 ⇒ 新名字检查当场列出 6 个缺失名。C44-0 那条健康检查用例随之从"容忍 5 组"收紧回 `missing_indexes.is_empty()`）、**D-79**（`synapse-federation` 缺 per-test schema 基建 ⇒ 该 crate 的 DB 路径只能跑共享 `public`，`key_rotation.rs` 的自愈 DDL 分支无法安全构造，集成用例名字承诺"缺失后恢复"却没造场景。已补 crate 本地隔离池适配器（第 4 份 `BASELINE_SQL` 副本，纳入统一守卫 `FEDERATION` 清单）+ 两条**真构造场景**的 db_tests（先 `DROP TABLE` 再断言重建表与两条索引 / 配置表 + 默认值 + 回写往返），并按 R11 用"把自愈变 no-op"变异自证两条用例都变红；集成用例改名为 `test_load_or_create_key_persists_a_signing_key` 并指向新用例）、**D-78**（C42 把 `key_rotation.rs` 8 处判为"与迁移重复的死 DDL、应删"并登记为待裁定；C43 复核发现 `federation_service_tests_migrated.rs` 里有一条 `test_load_or_create_key_recovers_missing_signing_key_table` —— 自愈是**有意**行为，且该测试体从未构造"表缺失"（跑的是共享库、表本就在）⇒ **改判为保留自愈 + 宏化 8 处**，该测试名承诺的恢复场景从未被覆盖 ⇒ 另立 D-79）、**D-77**（`check_sqlx_cache_fresh.sh --full` 对着被收敛成 0 表的共享 `public` 会吐 **1443 个 E0282/E0277**（看起来像源码坏了），而裸 `cargo sqlx prepare` 会把 `.sqlx/` 清空 ⇒ 新增唯一入口 `scripts/ci/sqlx_prepare.sh`（前置检查 fail-fast + 缩容回滚），`--full` 委托给它并在 AGENTS.md R2/R8 明令禁止）、**D-76**（`scripts/init_test_public_schema.sh` 的 `RESET_PUBLIC` 默认 1 ⇒ **裸跑就 `DROP SCHEMA public CASCADE`** 重建共享 `synapse_test.public`；失败/中断即留下 0 表 ⇒ 默认改为 0（幂等 apply），重建需显式 opt-in）、**D-75**（`converge_public_schema.sh` 的 TOCTOU：删除清单在 apply 阶段**二次求值**，而 `prepare_test_db.sh` [2/4] 会 `DROP SCHEMA test_template_ci CASCADE` 重建参考集 ⇒ 参考为空时 public 全被判"多余"；事后不变量又用同一个已塌掉的参考集（两边同时塌成 0 ⇒ 恒过）。实测环境 `synapse_test.public` = **0 表**（本该 ≥200）⇒ 已冻结清单 + 参考稳定性复检 + 大删栏杆 + 非空不变量，见 §8.3）、**D-74**（`update_access_stats` 的 `COALESCE($7, 0)` 让 PG 把 `$7` 定型成 **int4**，宏因此要求 `Option<i32>` 而 Rust 侧是 `response_time_ms: Option<f64>`；动态路径靠 sqlx 显式发送 FLOAT8 才没暴露 ⇒ 改 `0::float8` 并补浮点往返用例，见 §8.3）、**D-72**（`e2ee_audit.rs` 两个方向同时错：`e2ee_audit_log.details` 是 `NOT NULL DEFAULT '{}'`，但 `log_key_operation` 会把 `KeyEvent.details = None` 直接绑成 `NULL` ⇒ 运行期 23502；读回结构体又把该列声明成 `Option` ⇒ 可空性反推失真。已按 R12 先用 RED 用例复现 23502，再 `COALESCE($7, '{}'::jsonb)` + 读侧收紧为非 `Option`，见 §8.3）、**D-71**（D-25 家族收口：23 个 `#[cfg(feature)] pub mod` 声明里有 **10 个带测试却不在** `scripts/ci/gated_module_test_matrix` ⇒ "过滤器必须命中"这道守卫对它们从未生效；补 10 行后全表 21 行实跑通过）、**D-70**（`e4bc400cb` 删掉 3 个埋点却漏收紧 `metric_instrumentation_baseline` ⇒ 埋点棘轮在 `opt/consolidated` 上**常驻红**；按 R11 独立收紧 15 → 12 并复跑门禁）、D-62（通知响应的 `profile_tag` 键取自 `notification_type` ⇒ 已按修法① 改成真列 + 独立 `notification_type` 键）、**D-68**（通知记录层没有生产写入者、也没有保留期清理 ⇒ 已按修法① 接线 `record_notification` + `prune_old_notifications`，边界见 §0.5、明细见提交信息）、**D-69**（运行时迁移的 advisory lock key 在 `search_path` 为空时因 `current_schema()` 为 NULL 而**必败** ⇒ 已先 `COALESCE` 并补边界用例，见 §8.3）、**D-57②**（seed 侧 `public` 不收敛 ⇒ 新增 `scripts/ci/converge_public_schema.sh` 并接进 CI seed 第 [3/4] 步，见 §8.3）、D-65（并发改动只改一半 ⇒ 集成+clippy 双红）、D-66（worktree 共享 `CARGO_TARGET_DIR` ⇒ 跨树复用产物，假红/假绿）、D-67（新增测试里的死常量让 clippy 红） |


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

**未关闭项：1 条（D-95，见 §7.1）**（最近一次关闭：D-94，2026-09-28）；D-01…D-96 其余已关闭或转结构性例外（D-96 见 §7.3）。只登记**未关闭项**与**结构性例外**；
已关闭项的去向见 §0.4 与各自提交信息。

### 7.1 汇总表



| 编号 | 是什么 | 在哪 | 为什么还没修 | 怎么修 |
|---|---|---|---|---|
| ~~既有红①~~（已收口，见 D-90） | `test_auth_issuer_returns_unrecognized_when_oidc_is_disabled` | `tests/integration/api_auth_routes_tests.rs` | — | 已改为"废弃端点不得复活"的 404 守卫 |
| ~~既有红②~~（已收口，见 D-89） | `test_admin_room_lifecycle_management` | `tests/integration/api_admin_room_lifecycle_tests.rs:98` | — | 根因不是 admin 语义，而是 `fk_event_edges_prev` 的 FK 动作（`SET NULL` on `NOT NULL`）⇒ 迁移改 CASCADE |
| **D-95** | `DatabaseMonitor::verify_data_integrity` 的**两条检查结构上不可能命中**：① `events.room_id` 无对应房间；② `room_memberships.user_id` 无对应用户。两条都是全表 `NOT EXISTS` 扫描 | `synapse-storage/src/monitoring.rs`（`verify_data_integrity`，转宏前的 304/325 两处）+ 调用方 3 处（含 admin 完整性端点） | `fk_events_room` 与 `fk_room_memberships_user` 都是外键（`ON DELETE CASCADE`）⇒ 孤儿行**根本无法插入**（C53-0 的覆盖用"插入孤儿必须被拒"钉住了这一点）⇒ 该方法恒报"0 违规 / 100 分"，是**不会失败的门禁**（铁律 8 的反面）。处置需要产品/路由决策（该 admin 端点是否仍有意义、要不要换成可被违反的不变量），**不在静态化批次里擅自删路由**（删路由要连带重生成 `route-table.json`/契约 fixture，见 D-87 的教训） | 二选一：① 删掉 `verify_data_integrity` 及其 admin 端点（连带路由契约重生成）；② 换成**能违反**的不变量（例如 catalog 层的 NOT NULL/CHECK 漂移、缺索引、序列不同步等），并补一条"构造违规 ⇒ 报告非空"的用例 |



> 已关闭项的去向见 §0.4 与各自提交信息；本节只留**未关闭项**（R13）。

### 7.2 逐条明细

（空 —— 明细在 HISTORY §7.2 或各提交信息里。）

### 7.3 结构性例外（有意，不修，但必须遵守）

这些不是待修缺陷，而是**当前工具/接口的边界**，决定批次里"哪些站点允许保持动态"。

| 编号 | 边界 | 出现位置 | 怎么办 |
|---|---|---|---|
| D-96 | **可选扩展的关系**：SQL 文本是编译期常量，但 `FROM pg_stat_statements` 指向的关系**只在装了该扩展的库里存在** ⇒ 宏在 `cargo sqlx prepare` 阶段无法 describe（实测 `relation "pg_stat_statements" does not exist`），整份离线缓存都建不起来 | `synapse-storage/src/monitoring.rs`（`get_performance_metrics` 的慢查询查询） | 保持动态；若将来把该扩展纳入 baseline（需要 superuser 与 `shared_preload_libraries`）或改成 `to_regclass` 探测 + `query_scalar` 计数，可回收 —— 属独立设计事项 |
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

- 合计 **96** 条（D-01…D-96）：**未关闭 1**（D-95，见 §7.1）、
  **结构性例外 8**（D-13 / D-14 / D-18–D-22 / D-96，有意不修）、**已关闭 87**（含 D-37 收敛、
  D-57② 收敛、D-62 修法①、D-68 接线落地、D-69/D-70/D-71/D-72/D-74 先修、
  D-75/D-76/D-77 工具链事故先修、D-73 结构性收敛、D-78 改判收口、D-79 隔离池基建、D-80 隔离同形、
  D-81 A3+A4 集成测试编译失败先修、D-82 重复 INSERT/静默吞错先修、
  D-83 trait 棘轮漏同步先修、D-84/D-85/D-86 重活门禁回补、D-87 路由契约同步、
  D-88 widget 用例按 MSC4289 规则修正、D-89 事件边 FK 动作修正、D-90 废弃端点用例收口、
  D-91 状态前驱解码改 fail-closed、D-92 极端点计数改与 `_in_room` 同定义、
  D-93 raw-`PgRow` 公共 API 收敛、D-94 `monitoring.rs` 两处吞错改 fail-closed）。
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

**可转换残量 112 处** = **82 处字面量（机械转换）** + **29 处运行期拼装（D-14 结构性）**
加 **1 处跨函数传参（`param`）**。下表按**字面量**处数排前 3（表内数字是**可机械转换**的站点数；
纯 `runtime` 文件见下方结构性清单）：

| 文件 | 处数 | 门控 | 备注 |
|---|---|---|---|
| `synapse-storage/src/monitoring.rs` | 5 | — | C53-0 已修 D-94 两处吞错、补真基线覆盖，并登记 D-95（完整性检查结构上不可命中）/D-96（`pg_stat_statements` 可选扩展 ⇒ 该站点按 R7 保持动态）⇒ 剩余 4 处由 C53 转换 |
| `synapse-storage/src/event/ephemeral.rs` | 4 | — | `event/` 同域 |

> 紧随其后（各 4 处）：`account_data/mod.rs`(4)、`room_tag` 家族以外的 `room/` 子模块等。
> `synapse-e2ee/src/backup/service.rs`(4)、`synapse-storage/src/audit.rs`(4)、
> `synapse-storage/src/schema_health_check.rs`(4) 由 **C44** 归零退表（12 处宏化 + 两处
> `COUNT(*)`/`COALESCE` 的 R4 断言 + R5 数组参数改 owned + R6 ⑤ 元组投影改字段读，见 §8.3 第 14 条）。
> `synapse-storage/src/media/quarantine_stream.rs`(6) 由 **C52-0 + C52** 归零退表（先补真基线
> 生命周期覆盖，再宏化 6 处：三处 6 列投影、一处 UPDATE、一处可空列 `query_scalar!`、
> 一处聚合 `MAX`）。
> `synapse-storage/src/event/search.rs`(6) 由 **C51** 归零退表（覆盖本就在同域的
> `event/db_tests.rs`；含两个 7 元组分支改 `query!`、DDL 宏化、`COALESCE` 逐列断言）。
> `synapse-storage/src/call_session.rs`(7) 由 **C50** 归零退表（门控批，单批转换；覆盖与调用者
> 本就无缺口，已在 `gated_module_test_matrix` 登记）。
> `synapse-storage/src/room_account_data.rs`(7) 由 **C49-0 + C49** 归零退表（先按 D-93 删两处
> raw-`PgRow` 方法、把一处 `.ok().flatten()` 吞错改宏，再宏化其余 4 处）。
> `synapse-storage/src/email_verification.rs`(8) 由 **C48-0 + C48** 归零退表（先删 3 个零调用者
> 方法、把 R9 违规用例改成真基线覆盖，再宏化其余 5 处）。
> `synapse-storage/src/delayed_events.rs`(7) 由 **C47-0 + C47** 归零退表（先删唯一零调用者的
> `list_delayed_events_for_user`、补真基线生命周期覆盖，再宏化其余 6 处）。
> `synapse-storage/src/event/dag.rs`(8) 由 **C46-0 + C46** 归零退表（先修 D-91/D-92、补
> `get_event_graph_fields` 等覆盖，再宏化 8 处）。`synapse-federation/src/event_broadcaster.rs`(8) 由 **C45-0 + C45** 归零退表
> （先删 3 处零调用者方法、消 1 处重复 INSERT（D-82）、补真基线覆盖，再宏化剩余 4 处）。
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

16. ✅ **C45-0（先修 + 先补覆盖）已完成（2026-09-26）** —— 为 C45 扫清三个前置：
   - **删零调用者方法（3 处动态站点，铁律 1）**：`recover_pending_from_db`（56 行，全仓只有它
     自己的 doc 链接，连 `start_batch_sender` 都不调它）、`get_pending_count`（只有 doc 链接）、
     `cleanup_old_transactions`（同理 —— `synapse-e2ee/to_device/storage.rs` 里的同名方法是
     **另一个模块**的）；连同只为前者存在的类型别名 `DbPendingRow` 一起删除。
   - **D-82（1 处动态站点，铁律 2 + 吞错）**：`send_batch` 的发送失败分支**复制**了一份
     `INSERT INTO federation_queue (...) RETURNING id`，与 `persist_transaction_to_db` 是同一
     职责的两份实现；副本还把 `event_type` 硬编码 `'m.room.event'`、`room_id` 恒 `NULL`，
     并用 `.ok()` 把写库错误**静默吞掉**。抽出自由函数
     `persist_transaction_row(pool, destination, txn)` 两处共用（`send_batch` 没有 `&self`，
     这正是副本的由来）⇒ 元数据恢复正确（EDU 批次记 `m.edu`、`room_id` 取首条 PDU），
     错误统一 `error!` 记录。
   - **补覆盖（R8/R9）**：本文件此前**没有任何 DB 测试**，而 C45 要转的 4 处全在
     `federation_queue` 写路径上 ⇒ 新增两条真基线用例（PDU/EDU 两种批次的元数据、
     `update_db_status` 的 sent/retry/兜底三分支逐列断言、发送失败仍落库且 `db_id` 回填 +
     "发送成功不落库"对照组）。为此给 `MockFederationClient` 补 `fail_send_transactions(bool)`
     —— 此前 mock 的 `send_transaction` **永远成功**，失败分支根本无法构造。
   实测：`dynamic_production` 242 → **238**（−4）、`static` 1246 不变、`dynamic_test` 725 → **726**
   （`queue_row` 夹具的读回查询）、literal 171 → **167**、`.sqlx` 1214 不变。
   验证：`-p synapse-federation --lib --all-features -E 'test(/event_broadcaster/)'` ⇒ **5/5**。

17. ✅ **C45（`event_broadcaster.rs` 4 处宏化，该文件生产区动态归零）已完成（2026-09-26）** ——
   C45-0 之后剩下的 4 处全在 `federation_queue` 写路径上：
   - `persist_transaction_row` 的 `INSERT ... RETURNING id` ⇒ `query_scalar!` + `fetch_one`
     （单列 ⇒ R3 显式列清单；`room_id: Option<String>` 不能以 `&Option<T>` 传入 ⇒ R5 `.as_deref()`）；
   - `update_db_status` 的三条 UPDATE ⇒ `query!` + `.execute(pool)`（无结果列，正是 R6 ① 说的
     `.execute()` 适用形态）；三分支语义逐字保留：sent 写 `sent_at`、retry 做
     `retry_count + 1` 并把状态拉回 `pending`、兜底把传入的状态名直接落库。
   实测：`dynamic_production` 238 → **234**（−4）、`static` 1246 → **1250**（+4）、
   `dynamic` 总数 964 → **960**、`.sqlx` 1214 → **1218**（+4）、literal 167/39 → **163/38**；
   恒等式 `234 − 163 − 1 = 70` 仍成立（未产生运行期拼装）。
   证据性质：C45-0 补的两条**真基线**用例在转换后跑的就是这四条宏
   （`-p synapse-federation --lib --all-features` ⇒ **308/308**，含 PDU/EDU 元数据、
   状态流转三分支、发送失败落库 + `db_id` 回填与"成功不落库"对照组）——
   即"转换不改变行为"由真库往返直接证明，而不是靠"编译过了"。

18. ✅ **D-83（trait 棘轮漏同步，`opt/consolidated` 上的既有红门禁）已先修（2026-09-28）** ——
   与本批（静态化）无关，按 R11「先修再转」独立提交。
   `bf90f430f`（B1-1b「本地事件自动补图元数据」）新增了
   `synapse-services/src/graph_metadata.rs:103` 的 `pub trait GraphMetadataSource: Send + Sync`，
   但没有同步 `scripts/ci/trait_count_baseline`（`TOTAL` 仍 66）⇒ `repo-sanity` 里的
   `python3 scripts/ci/check_trait_ratchet.py` 自该提交起在 `opt/consolidated` 上**常驻红**
   （实测 `TOTAL=67 (baseline 66)`）。这与 D-70（删埋点漏收紧埋点基线）是同型事故，
   只是方向相反：**棘轮只禁增，所以"有正当理由的新增"必须显式落账**。
   判定为"正当理由"的依据（与 2026-09-21 那条 `InvitePolicyGate` 完全同型的判据）：
   · 生产实现：`impl GraphMetadataSource for StorageGraphMetadataSource`（同文件 :151）；
   · 测试替身：`FakeSource`（同文件 :609，`impl` 在 :657），被 `FakeSource::room(...)` 一族用例
     使用 —— 用来构造"深度/前驱事件"这类在真库上很难摆出的图状态；
   · 注入点：`GraphMetadataResolver { source: Arc<dyn GraphMetadataSource> }`（:247），
     生产由 `synapse-services/src/wiring/rooms.rs:96` 注入具体实现。
   修法：`TOTAL` 66 → **67** 并写明来源、判据与"为什么不删接缝"；`STORE_API` 保持 33（未放宽）。
   验证：`python3 scripts/ci/check_trait_ratchet.py` ⇒ OK；同批复核了 `repo-sanity` 其余廉价门禁
   （schema 表/契约覆盖、迁移一致性、路由↔存储边界、workflow step schema、SQLx 两道、
   web 分层、路由分层、连接预算、密钥/产物三类、埋点与仪表盘可达性、内存预算、fmt、
   两档 clippy、`--compile`、literal 守卫）全部为绿；其中 `check_get_raw_usage.py` 与
   `check_missing_docs_ratchet.sh` 的"红"经核实是**本机环境假红**（前者的失败项**全部**落在
   `.worktrees/*` 的重复副本里 —— CI 检出没有该目录；后者把 `CARGO_TARGET_DIR` 指到与并发会话
   共享的默认 `target/` 导致 clippy 构建失败，换私有目录后 OK=0 debt）。

19. ✅ **D-84（D-79 新增的 test 区 DDL 未进白名单）已先修（2026-09-28）** —— 由 `--test unit`
   批次（CI 阻断）当场暴露：D-79 给 `synapse-federation` 补的隔离池基建让
   `synapse-federation/src/key_rotation.rs::db_tests` 的两条用例用 `DROP TABLE` 构造"表缺失"，
   而 `scripts/ci/test_ddl_allowlist` 没有对应键 ⇒ D-36 守卫 A
   （`test_ddl_guard_tests::no_unallowlisted_self_built_schema_in_test_regions`）红。
   修法：按名单的正统语义补两条 `path::mod::fn` 键并写明理由 —— 这里的 DDL 是**被测对象**
   （"表缺失 ⇒ 自愈重建"分支的唯一入口，共享 `public` 上做不到），且两条用例都跑在
   `test_isolation::isolated_test_pool()` 的 per-test schema 上，不碰共享 schema、
   不改迁移 baseline，因此 D-31 型"写入端漏列"仍会被真 catalog 证伪。
   验证：`--test unit -E 'test(/test_ddl_guard_tests/)'` ⇒ **9/9**（含
   `allowlist_entries_all_still_match_something`，保证键没有写错）。
20. ✅ **D-85（ts-order 单键棘轮基线过期）已先修（2026-09-28）** —— 同一批次暴露：
   C45-0 删掉 `recover_pending_from_db` 时，它那条 `ORDER BY created_ts ASC`（单键、无决胜键）
   一并消失，而 `scripts/ci/ts_order_single_key_baseline` 仍记着
   `synapse-federation/src/event_broadcaster.rs 1` ⇒ 棘轮"基线比实测**宽**"，
   `ts_order_tiebreak_tests::timestamp_ordering_tiebreak_ratchet_passes_on_current_tree` 红
   （`left: 1, right: 0`，消息明确要求 `--update` 收紧）。
   修法：`python3 scripts/ci/check_ts_order_tiebreak.py --update` ⇒ 72 处 / 30 文件（仅删该行）。
   **教训**：删除代码会**顺带修好**别的棘轮，而"只禁增"的棘轮把"变好"也判红 ——
   删代码的批次必须把这类基线一起收紧（本条已写进 C45-0 的教训）。
   验证：`check_ts_order_tiebreak.py` ⇒ OK；`--test unit -E 'test(/ts_order_tiebreak/)'` ⇒ **2/2**。
21. ✅ **D-86（A3+A4 新增的回填用例类型错误 + 恒真断言）已先修（2026-09-28）** —— 集成批次暴露：
   `state_groups_backfill_tests::test_all_v12_rooms_have_state_groups` 用
   `SELECT COUNT(*) FROM rooms WHERE room_version >= 12`，而 `rooms.room_version` 是
   **TEXT**（`DEFAULT '6'`）⇒ PG 把裸字面量定成 integer，报
   `42883 operator does not exist: text >= integer`，用例**必红**；同时那句
   `v12_with_state_groups <= total_v12` 是**恒真**断言（左边是右边的子集计数），
   即使类型修好也永远不可能失败 —— 又一条"看起来是门禁、实际不会红"的形态（铁律 8 的反面）。
   修法：改写为 `backfill_predicate_distinguishes_bound_from_unbound_v12_rooms` ——
   在**本用例自建**的两个 v12 房间上跑"与回填脚本同款"的判定
   （`room_version ~ '^[0-9]+$' AND room_version::int >= 12 AND NOT EXISTS(… state_groups …)`），
   断言"绑定了状态组的房间 ⇒ 0 条待回填"且**反向对照**"没有状态组的房间 ⇒ 恰好 1 条待回填"，
   全局计数只留作背景日志（并发用例会同时造房间，全局等值不是本用例能控制的不变量）。
   验证：`--test integration --all-features --test-threads 1
   -E 'test(/state_groups_backfill|state_groups_idempotency/)'` ⇒ **8/8**。

22. ✅ **D-87（路由契约 fixture 漏同步）已先修（2026-09-28）** —— 重活门禁实测的第二类收获：
   `tests/integration/snapshots/route_ledger_{default,worker_enabled}.snapshot` 与
   `docs/openapi/route-table.json` 都停在 2026-09-25，而此后 live router 上
   ① U-5（`a13f57316`）新增了 10 条 admin media/invite 路由、
   ② `auth_issuer` 路由被 `76e5f9136` 摘除
   —— 两者都只落在代码里，没同步进契约 ⇒ `declared_route_manifest_full_snapshot_matches_*`
   两条集成用例红（live 1130 vs 快照 1120），CI 的 `openapi-artifact` 两道 `--check`
   同样会红（`route-table.json` 缺这 10 条）。
   修法（严格照 CI 的产物流水线，**不用** `cargo insta accept`、**不改**断言）：
   1. `UPDATE_ROUTE_LEDGER_SNAPSHOTS=1` 重生成两条快照 ⇒ count 1120 → **1130**，
      新增的正是 U-5 那 10 条（`_synapse/admin/v1/{media,rooms/{room_id}/media,user/{user_id}/media}`
      与 `_synapse/admin/v1/invite/{allowlist,blocklist}`），无任何**消失**项；
   2. `cargo build --bin synapse_ledger_export` + 固定时间戳
      （`artifact_common.FIXED_TIMESTAMP`）导出 default-feature ledger ⇒
      `gen_route_table.py --ledger …` 重生成 `docs/openapi/route-table.json`
      （1098 → **1130** 路由；同时把 `auth_issuer` 的删除、若干 `query_params` 变化一并同步）；
      `gen_client_yaml.py --skip-export` 生成的 `client.yaml` 与提交版**逐字节相同**
      （它由未变的 `scripts/api_test/ledger.json` 生成 ⇒ 无差异）。
   验证：`gen_client_yaml.py --skip-export --check` ⇒ OK；
   `gen_route_table.py --check --ledger <fresh>` ⇒ OK；
   `--test integration --all-features --test-threads 1 -E 'test(/declared_route_manifest/)'`
   ⇒ **5/5**（且**不再**需要 `UPDATE_ROUTE_LEDGER_SNAPSHOTS`）。
   ⚠️ 同批实测确认**仍红**的两条本仓既有红（`auth_issuer` 用例、管理端房间删除语义）
   见 §7.1 —— 它们已在 2026-09-25 的计划文档里登记，本战役不擅自改。

23. ✅ **D-88（widget 权限用例违规）已修（2026-09-28）** —— 重活门禁实测留下的最后一条，
   定性为**用例侧缺陷**（生产行为是对的）：
   - **现象**：`api_widget_tests::test_create_widget_allows_joined_room_moderator` 在
     串行 + 一次性库下稳定红，红在 helper `set_room_power_levels` 的最后一行
     （`api_widget_tests.rs:183`）：房主 `PUT /_matrix/client/v3/rooms/{id}/state/m.room.power_levels`
     得 **403**，用例期待 200。看起来像"widget 创建权限坏了"，根因不在这里。
   - **根因（探针实测错误体）**：
     `{"errcode":"M_FORBIDDEN","error":"power_levels.users must not name a room creator (MSC4289 rule 10.4)"}`
     —— `create_room` 建的是 **v12** 房间，MSC4289 rule 10.4
     （`synapse-services/src/auth/power_levels.rs:244-271`）明令 v12+ 房间的
     `m.room.power_levels.users` **不许点名任何创建者**（创建者权力无上限，见
     `get_user_power_level`，显式列出既冗余又会被拒）。helper 却写了
     `users: { owner_user_id: 100, moderator_user_id: 50 }` ⇒ **创建者自己那条 PUT 被规则挡下**。
     该 helper 由 WIP 提交 `c04476ccf` 引入，全文件只有这一个调用者。
   - **修法**：helper 只写非创建者（`users: { moderator_user_id: 50 }`），删掉因此多余的
     `owner_user_id` 形参（调用点同步改用 `register_user`）；helper 上加了引用规则出处的
     文档注释，防止后来者把创建者写回去。
   - **验证**：`--test integration --all-features --test-threads 1 -E 'test(/api_widget_tests/)'`
     ⇒ **19/19**（修复前 18/19）。
   - **R11 自证（断言非空跑）**：把 helper 里 moderator 的层级从 50 变异成 **0** ⇒ 用例在
     **widget 创建**那条断言（`api_widget_tests.rs:328`）真的红 —— "50 级成员可创建、0 级不可"
     确实由该用例守着。

24. ✅ **D-89（`fk_event_edges_prev` 的 FK 动作错：`SET NULL` 打在 `NOT NULL` 列上）已修（2026-09-28）**
   —— 这是本轮重活门禁挖出的**生产缺陷**，也是"管理端删房间失败"这条既有红的真根因。
   - **现象**：`api_admin_room_lifecycle_tests::test_admin_room_lifecycle_management` 红在
     "删除应返回 200/202"。探针取到的是 **500**：`{"errcode":"M_UNKNOWN","error":"An internal error occurred"}`
     （通用错误体把真因遮住了）。
   - **根因（服务端日志）**：
     `Failed to delete room error=error returned from database: null value in column "prev_event_id" of relation "event_edges" violates not-null constraint`
     —— `event_edges.prev_event_id` 是 `NOT NULL`（建表语句），而 baseline 尾部 P1-3 折入块给它的 FK
     `fk_event_edges_prev` 动作是 **`ON DELETE SET NULL`** ⇒ 删任一父事件时 PG 执行 SET NULL 立刻 23502
     ⇒ `RoomStorage::delete_room`（批量删 events → 删 room）**第一步就失败**，管理员删房间恒 500。
     即"管理端删除语义未与上游对齐"这条旧结论**方向就错了**：不是语义问题，是 schema 缺陷。
   - **修法**：动作改为 **`ON DELETE CASCADE`**（`event_edges` 是**派生**的 DAG 边，节点不存在时边无意义；
     与同表 `fk_event_edges_event` 的 `event_id` 侧一致。P1 审计当初为"宽容孤儿 prev_event_id"选 SET NULL，
     但 FK 校验本就拒绝孤儿 —— 要宽容应当用 `NOT VALID`，而不是把 NOT NULL 列置空）；并把折入块从
     `IF NOT EXISTS … ADD` 改成 **先 `DROP CONSTRAINT IF EXISTS` 再 `ADD`**，因为
     `scripts/init_test_public_schema.sh` 是**逐个重放**迁移文件（不走 sqlx 的 applied-migrations 表），
     只加不换会让已有库永远留着旧动作。
   - **R10 连带**：独立复算 FNV-1a 64（先用旧字节 218573 自检哈希实现 ⇒ `f6e8cb1fdbe20a67` 吻合）
     得 `5906517503a8cdc6`（新字节 219632），同步 `EXPECTED_BASELINE_FINGERPRINT`。
   - **验证**：`--test unit` ⇒ **1812/1812**（含 `test_isolation_unification` 10/10、
     `migration_consistency` 12/12）；`--test integration --all-features --test-threads 1
     -E 'test(/api_admin_room_lifecycle_tests|federation_existence_leak_tests/)'` ⇒ **16/16**；
     `-p synapse-storage --lib --features test-utils -E 'test(/delete_room|event::dag/)'` ⇒ **7/7**；
     `--static`/`--compile` OK；clippy（`--all-targets --all-features`）exit 0；fmt `current=0=baseline`。

25. ✅ **D-90（既有红①：`auth_issuer` 用例断言已删端点）已收口（2026-09-28）** —— 测试侧缺陷：
   端点 `/…/org.matrix.msc2965/auth_issuer` 由 `76e5f9136`（对齐上游 Synapse 1.161）**有意删除**，
   而用例仍断言"端点存在、只是拒绝"（400 + `M_UNRECOGNIZED`）⇒ 实得 404、长期红
   （2026-09-25 计划文档记为"摘路由时漏改用例"）。
   修法：改名 `test_removed_auth_issuer_endpoint_is_not_served`，断言 **404**，并在注释里写清
   "为什么保留而不是删除" —— 这是"废弃端点不得复活"最便宜的一处显式守卫（路由加回来会变
   400/401/405 ⇒ 立刻红），全表守卫仍由 `api_route_ledger_tests` 的快照 + manifest 承担。
   验证：`--test integration --all-features --test-threads 1 -E 'test(/api_auth_routes_tests/)'` ⇒ **7/7**。
   至此 §7.1 的两条既有红全部收口 ⇒ 分支在"单元 + 集成 + 契约"三类阻断门禁上均无已知红项。

26. ✅ **D-91（`prev_state_events` 解码吞错）已先修（2026-09-28）** —— C46 的 `event/dag.rs`
   转换前处理的第一件**行为**问题：`get_prev_state_events` 与 `get_state_dag_edges` 都用
   `serde_json::from_value(json).unwrap_or_default()` —— 列里的 JSON 若不是"字符串数组"，
   就静默变成"没有前驱状态事件"，与"该列本就为 NULL"完全不可区分（吞错，与 D-33/D-72 同族）。
   修法：抽出 `prev_state_events_from_json(event_id, json)`，形状不对返回
   `sqlx::Error::Decode`（fail-closed），让脏数据在调用点可见，而不是被当成"这个事件没有状态
   前驱"继续参与 DAG 遍历。
27. ✅ **D-92（极端点计数是第二份、且过期的实现）已先修（2026-09-28）** —— 同一个文件的第二件：
   `get_forward_extremities_count` 读的是**旧 JSONB 内容约定** `content->>'prev_event_id'`，
   而现代写入路径（`create_event_with_graph` / `create_state_event_with_dag`）只写 `event_edges`
   —— 没有任何生产代码再往 `content` 塞该字段 ⇒ 子查询恒为空集、`NOT IN (空)` 恒真；
   它另外还要求 `state_key IS NOT NULL`，于是实际返回的是**房间里的状态事件数**，与
   `get_forward_extremities_in_room`（`event_edges` 上的 DAG 叶节点）不是同一个概念
   —— 同一职责的第二份实现（铁律 2）。后果：管理员端点
   `GET /_synapse/admin/v1/rooms/{room_id}/forward_extremities` 长期报错数。
   修法：改为与 `_in_room` **完全同一定义**（`event_edges` 上的 `NOT EXISTS`），两者从此永远一致。
   验证（C46-0 两项一起）：`-p synapse-storage --lib --features test-utils -E 'test(/event::/)'`
   ⇒ **117/117**；其中新增/替换的三条用例本身即判别性证据：
   · `test_get_forward_extremities_count_counts_dag_tips`：用**消息**事件（`state_key` 为 NULL）
     构造"链 + 分叉"，旧实现会给 0/0，新实现必须给 1/2，且与 `_in_room` 的长度一致；
   · `test_get_event_graph_fields_round_trip`：该函数此前**零覆盖**，现在覆盖"有图元数据 /
     走 plain 写入路径（三列全 None）/ 缺事件（None）"三种形态；
   · `test_malformed_prev_state_events_is_an_error`：D-91 的失败路径，形状不对必须
     `sqlx::Error::Decode`，而不是 `Ok(None)`。

28. ✅ **C46（`event/dag.rs` 8 处宏化，该文件生产区动态归零）已完成（2026-09-28）** ——
   C46-0 之后剩下的 8 处按查询形状分三类转换：
   - **单列标量 3 处**：`find_missing_event_ids`（`event_id = ANY($1)`；`&[String]` 可直接传）、
     `get_latest_event_ids_in_room`（原 `query_as::<_, (String,)>` 元组 ⇒ R6 ⑤ 改 `query_scalar!`）、
     `find_events_referencing_missing_state`（`prev_state_events ?| $2::text[]`）。
   - **递归 CTE 1 处**：`get_missing_events_between` 的 DAG 游走 —— `dag_walk.event_id` 由 `UNION`
     的输出列构成，PG **不给集合运算的输出列透传 NOT NULL**（R4 ②），而两个分支都取自
     `event_edges.prev_event_id`（NOT NULL）⇒ 断言 `AS "event_id!"`。
   - **`query!` 按字段读 3 处**：9 列 JSON 投影（原先一律 `row.get::<Option<T>>`，现在宏按真
     catalog 给 `T`/`Option<T>` —— 序列化结果逐字节相同，但列名/类型/可空性错配编译期即证伪）、
     `get_state_dag_edges`（元组 ⇒ R6 ⑤；`prev_state_events` 由 WHERE 保证非空 ⇒ 断言）、
     `get_prev_state_events`（可空列 + `fetch_optional` ⇒ `Option<Option<Value>>` 两层，R6 ②）。
   - **`COUNT(*)` 1 处**：D-92 已修好的极端点计数 ⇒ `query_scalar!` + `AS "count!"`（R4 ①）。
   顺带删掉失去唯一使用者的 `use sqlx::Row;`。
   实测：`dynamic_production` 234 → **226**（−8）、`static` 1250 → **1258**（+8）、
   `dynamic` 总数 967 → **959**、`.sqlx` 1218 → **1226**（+8）、literal 163/38 → **155/37**；
   恒等式 `226 − 155 − 1 = 70` 仍成立。证据性质：C46-0 的覆盖在转换后**穿过新宏跑真库**
   （`-p synapse-storage --lib --features test-utils -E 'test(/event::/)'` ⇒ **117/117**）。

29. ✅ **C47-0（先修：删零调用者方法 + 补真基线覆盖）已完成（2026-09-28）** —— 为 C47
   （`synapse-storage/src/delayed_events.rs`）扫清前置：
   - **删死代码（铁律 1）**：`DelayedEventStorageApi::list_delayed_events_for_user` 全仓
     **零调用者** —— 只有 trait 声明、impl、以及"为满足 trait 而存在"的 mock impl
     （`test_mocks/delayed_event.rs`）三处 ⇒ 三处一并删除（省下一次转换与一条 `.sqlx` 条目）。
     ⚠️ **同时订正 §8.1 的旧注记**：那里写"7 个方法里 5 个零引用"，实测只有这 1 个 ——
     `restart` / `cancel` / `mark_sent` 由 `synapse-services/src/delayed_event_service.rs` 调用，
     `get_due_events` 由 `src/server/mod.rs` 的分发循环调用（旧注记来自按名字粗匹配的误判，
     这也是"先侦察调用者再决定删/转"的又一例证）。
   - **补覆盖（R8/R9）**：本文件此前只有 `validate_delay_ms` 的纯单测，7 个 trait 方法
     **零 DB 覆盖**，而 `delayed_events` 的 `state_key` / `last_error` 可空、`retry_count` 是
     `INTEGER NOT NULL DEFAULT 0` —— 正是宏转换最容易搞错可空性的形状 ⇒ 新增一条真基线
     生命周期用例：create（整行 + `scheduled_ts = created_ts + delay_ms` + 合成 event_id）、
     get（命中/未命中）、get_due_events（`pending` + `scheduled_ts <= now`、排序、LIMIT、
     未到期取不到）、restart（重排且仍 pending）、mark_sent / cancel（状态机不可逆，
     重复调用返回 false，且已 sent/cancelled 的不再出现在 due 列表）—— **只用被测 API 构造数据**，
     因此测试区动态站点**不增**（733 不变）。
   实测：`dynamic_production` 226 → **225**、`static` 1258 不变、`dynamic` 总数 959 → **958**、
   literal 155/37 → **154/37**；恒等式 `225 − 154 − 1 = 70` 成立。
   验证：`-p synapse-storage --lib --features test-utils -E 'test(/delayed_event/)'` ⇒ **8/8**；
   clippy（storage crate，`--all-targets --all-features`）exit 0；fmt `current=0=baseline`。

30. ✅ **C47（`delayed_events.rs` 6 处宏化，该文件生产区动态归零）已完成（2026-09-28）** ——
   C47-0 之后剩下的 6 处按形状分两类：
   - **`query_as!` 3 处**：`create_delayed_event`（14 列 `RETURNING` 与 `DelayedEvent` 的 14 个
     字段一一对应 —— R6 ⑤；`state_key: Option<String>` 以 `.as_deref()` 传入 —— R5）、
     `get_delayed_event`（14 列 + `fetch_optional`）、`get_due_events`（14 列 +
     `ORDER BY scheduled_ts ASC` + `LIMIT $2`，两个 `i64` 绑定直传，R5 的 LIMIT 定型规则在这里
     天然满足）。
   - **`query!` + `.execute()` 3 处**：`restart_delayed_event`（`SET scheduled_ts = $2 + delay_ms`，
     两个绑定）、`cancel_delayed_event`、`mark_sent`（都是
     `UPDATE ... WHERE id = $1 AND status = 'pending'`，无结果列 ⇒ R6 ① 的 `.execute()` 正体）。
   实测：`dynamic_production` 225 → **219**（−6）、`static` 1258 → **1264**（+6）、
   `dynamic` 总数 958 → **952**、`.sqlx` 1226 → **1232**（+6）、literal 154/37 → **148/36**；
   恒等式 `219 − 148 − 1 = 70` 仍成立。证据性质：C47-0 的真基线生命周期用例在转换后
   **穿过新宏跑真库**（`-p synapse-storage --lib --features test-utils -E 'test(/delayed_event/)'`
   ⇒ **8/8**），其中状态机（pending → sent/cancelled 不可逆）与 `get_due_events` 的
   `pending + scheduled_ts <= now + ORDER BY + LIMIT` 语义都由断言钉住。

31. ✅ **C48-0（先修：删 3 个零调用者方法 + 修 R9 违规用例）已完成（2026-09-28）** ——
   为 C48（`synapse-storage/src/email_verification.rs`）扫清前置：
   - **删死代码（3 处站点，铁律 1）**：`verify_token`、`delete_token_by_id`、`get_token_by_email`
     全仓**零调用者**。两个易误判点：`verify_token` 有一处"看起来有调用"的**假阳性** ——
     `synapse-services/src/uia_service.rs::verify_token_stage` 是**另一个**方法；而
     `mark_token_used` 虽然文件外无人调用，却由本文件的 `validate_and_consume_token` 内部调用
     ⇒ **保留**（"排除定义文件后再 grep"才看得出来）。
   - **修 R9 违规用例**：原 `test_delete_token_by_id_removes_verification_session` 在
     `prepare_empty_isolated_test_pool()` 造的**空 schema** 上手写
     `CREATE TABLE email_verification_tokens`，而那份 DDL **漏掉了真 schema 的
     `token TEXT NOT NULL UNIQUE`** —— D-31 家族"手搭夹具掩盖真约束"的教科书形态
     （手写版本里重复 token 会静默成功）；它还"拿不到库就 `warn!` + `return`"，门禁看不出它没跑。
     改为 `crate::test_isolation::isolated_test_pool()` 克隆真 v12 模板、只用被测 API 造数据，
     并按名单规则**同时删除** `scripts/ci/test_ddl_allowlist` 的对应条目
     （`allowlist_entries_all_still_match_something` 拦住"只删代码留条目"）。
     新用例把 `UNIQUE(token)` 变成**可失败**断言，并覆盖 create / get_by_id（可空列两侧）、
     `validate_and_consume_token` 的四条拒绝路径与成功路径、`claim_used_token` 的
     "取走即物理删除 + 不可重放"、`cleanup_expired_tokens` 只清过期行。
   实测：`dynamic_production` 219 → **216**、`static` 1264 不变、`dynamic` 总数 952 → **948**、
   `dynamic_test` 733 → **732**、literal 148/36 → **145/36**；恒等式 `216 − 145 − 1 = 70` 成立。
   验证：`-p synapse-storage --lib --features test-utils -E 'test(/email_verification/)'` ⇒ **5/5**；
   `test_ddl_guard_tests` ⇒ **9/9**；`check_ts_order_tiebreak.py` / `check_trait_ratchet.py` ⇒ OK。

32. ✅ **C48（`email_verification.rs` 5 处宏化，该文件生产区动态归零）已完成（2026-09-28）** ——
   C48-0 之后剩下的 5 处：
   - `create_verification_token` 的 `INSERT ... RETURNING id`（单列）⇒ `query_scalar!` + `fetch_one`，
     顺带删掉只为它存在的包装结构体 `TokenIdRow`（铁律 1）；
   - `mark_token_used` / `cleanup_expired_tokens` 两条无结果列语句 ⇒ `query!` + `.execute()`
     （R6 ①；后者保留 `rows_affected()` 语义）；
   - `get_verification_token_by_id` / `claim_used_token` 的 8 列投影 ⇒ `query_as!`
     （`claim_used_token` 是 `DELETE ... RETURNING`，同一份 8 列清单与 `EmailVerificationToken`
     一一对应 —— R6 ⑤）。
   实测：`dynamic_production` 216 → **211**（−5）、`static` 1264 → **1269**（+5）、
   `dynamic` 总数 948 → **943**、`.sqlx` 1232 → **1237**（+5）、literal 145/36 → **140/35**；
   恒等式 `211 − 140 − 1 = 70` 仍成立。
   证据性质：C48-0 的真基线用例在转换后**穿过新宏跑真库** ⇒
   `-p synapse-storage --lib --features test-utils -E 'test(/email_verification/)'` ⇒ **5/5**，
   其中 `UNIQUE(token)` 拒绝重复、四条拒绝路径（密钥/token/过期/已用过）、
   `claim_used_token` 的"取走即物理删除 + 不可重放"、`cleanup_expired_tokens` 只清过期行
   都由断言钉住。

33. ✅ **C49-0（先修 D-93：消 2 处 `PgRow` 泄漏 + 1 处吞错）已完成（2026-09-28）** ——
   为 C49（`synapse-storage/src/room_account_data.rs`）扫清前置：
   - **D-93（存储层把行类型暴露成公共 API）**：trait + impl + 委托 + mock 四处的
     `get_room_account_data(...) -> Result<Option<PgRow>, _>` 与
     `get_room_vault_data(...) -> Result<Option<PgRow>, _>` 把 `sqlx::postgres::PgRow` 直接
     暴露给服务/路由层；**mock 里两者都是 `unimplemented!()`**，报错信息自己写着
     "use get_room_account_data_content" —— 代码本身已承认这是不可用的接缝。
     实测调用者：`get_room_account_data` 全仓**无外部调用者**（唯一消费者是本文件的
     `get_room_account_data_content`；`sliding_sync_service/extensions.rs` 那条同名调用是
     **另一个** 2 参数方法）；`get_room_vault_data` **零调用者**（路由里的同名
     `get_room_vault_data` 是 **web handler**，取数据走 `get_room_account_data_with_ts`）。
     ⇒ 两者连同 trait 条目一并删除（铁律 1），`content` 的查询内联到自身（新写的 SQL 直接用宏，R1）。
   - **吞错（D-33/D-72 同型）**：`get_room_account_data_with_ts` 的
     `row.try_get::<Option<i64>, _>("updated_ts").ok().flatten()` 把解码失败静默变成 `None`，
     与"该列本就是 NULL"不可区分（schema 里 `updated_ts` 是 `BIGINT NOT NULL`）。
     这条的"先修"与转换是同一件事 —— 改成 `query!` 后手工解码整条消失、可空性由真 catalog
     在编译期钉死，返回值仍 `Option<i64>`（`Some(ts)`），成功路径行为逐字一致。
   - 顺带删掉失去唯一使用者的 `use sqlx::Row;`，并订正 mock 里已过时的
     "Raw-`PgRow` methods are intentionally unsupported"。
   实测：`dynamic_production` 211 → **208**、`static` 1269 → **1271**、`dynamic` 总数
   943 → **940**、`.sqlx` 1237 → **1239**、literal 140/35 → **137/35**；
   恒等式 `208 − 137 − 1 = 70` 成立。
   验证：`-p synapse-storage --lib --features test-utils -E 'test(/room_account_data/)'` ⇒ **11/11**
   （含共用该表的 `sliding_sync` 两条）。

34. ✅ **C49（`room_account_data.rs` 4 处宏化，该文件生产区动态归零）已完成（2026-09-28）** ——
   C49-0 之后剩下的 4 处：
   - `list_room_account_data` / `list_room_account_data_batch` 的
     `query_as::<_, RoomAccountDataRecord>` ⇒ `query_as!`（投影 `data AS content` 与结构体字段
     `content` 同名 —— R6 ⑤ 要求逐列对应；后者 `room_id = ANY($2)` 的 `&[String]` 直传）；
   - `upsert_room_account_data` 的 `INSERT ... ON CONFLICT ... DO UPDATE` ⇒ `query!` + `.execute()`
     （无结果列，R6 ①）；
   - `delete_room_account_data` 的 `DELETE` ⇒ `query!` + `.execute()`（保留 `rows_affected()` 语义）。
   实测：`dynamic_production` 208 → **204**（−4）、`static` 1271 → **1275**（+4）、
   `dynamic` 总数 940 → **936**、`.sqlx` 1239 → **1243**（+4）、literal 137/35 → **133/34**；
   恒等式 `204 − 133 − 1 = 70` 仍成立。
   证据性质：既有 db_tests（`room_account_data::db_tests` 8 条 + 共用该表的 `sliding_sync` 2 条）
   在转换后**穿过新宏跑真库** ⇒ **11/11**。

35. ✅ **C50（`call_session.rs` 7 处宏化，该文件生产区动态归零）已完成（2026-09-28）** ——
   本批是**门控批**：`#[cfg(feature = "voip-tracking")] pub mod call_session;`，且已在
   `scripts/ci/gated_module_test_matrix:41` 登记（D-71 家族无缺口 ⇒ 那 10 条 db_tests 在 CI 上
   确实执行）。覆盖与调用者都无缺口（8 个方法全有生产调用者、10 条真基线 db_tests）
   ⇒ **无 C50-0，单批转换**：
   - `create_session` 的 `INSERT ... RETURNING *` ⇒ `query_as!`：**R3 要求把星号展开为显式列清单**
     （12 列与 `CallSession` 的 12 个字段一一对应）；`callee_id` / `offer_sdp` 是 `Option<String>`
     ⇒ R5 `.as_deref()`；
   - `get_session`（12 列）/ `get_candidates`（6 列）⇒ `query_as!`；
   - `update_state` / `set_answer` / `add_candidate` / `cleanup_expired` 四条无结果列语句
     ⇒ `query!` + `.execute()`（R6 ①；`cleanup_expired` 保留 `rows_affected()` 语义）。
   实测：`dynamic_production` 204 → **197**（−7）、`static` 1275 → **1282**（+7）、
   `dynamic` 总数 936 → **929**、`.sqlx` 1243 → **1250**（+7）、literal 133/34 → **126/33**；
   恒等式 `197 − 126 − 1 = 70` 仍成立。
   验证：`-p synapse-storage --lib --all-features -E 'test(/call_session/)'` ⇒ **10/10**
   （门控模块必须带 `--all-features` 才编译得到，见 D-25 的教训）。

36. ✅ **C51（`event/search.rs` 6 处宏化，该文件生产区动态归零）已完成（2026-09-28）** ——
   覆盖本就在**同域**的 `event/db_tests.rs` 里（`search_room_messages_admin` /
   `search_joined_room_events` / `search_postgres_messages` / `create_postgres_fts_index` /
   `search_room_postgres_messages` 均有用例），且无死代码 ⇒ **单批转换**：
   - `search_room_messages_admin` 的 5 列 JSON 投影 ⇒ `query!` 按字段读（顺带删掉函数内的
     `use sqlx::Row;`）；
   - `search_postgres_messages` 的**两个 7 元组分支** ⇒ R6 ⑤（`query_as!` 不能构造元组）
     改 `query!` + 组装元组；
   - `create_postgres_fts_index` 的 DDL ⇒ `query!` + `.execute()`（R6 ④，仍走 autocommit，
     `CONCURRENTLY` 语义不变）；
   - `fail_if_fts_index_invalid` ⇒ `query_scalar!` + `fetch_optional`；
   - `search_room_postgres_messages` ⇒ `query_as!(RoomEvent, …)`：五处 `COALESCE(…)` 按 R4 ①
     断言（第二实参保证非空），并把 `processed_at` 别名改成**真实字段名** `processed_ts`
     —— R6：`query_as!` 不认 `#[sqlx(rename)]`。
   ⚠️ **两个转换期实测的坑**（都写进代码注释）：
   ① 断言别名是**真列名**：写成 `::float8 as "rank!"` 后，同一条 SQL 里的 `ORDER BY rank`
      在 prepare 阶段报 `column "rank" does not exist`（正是 R6 记录的形态）⇒ 排序改成重复
      `ts_rank(...)` 表达式；
   ② `ts_rank(...)` 返回 **real**，直接与 `$3` 比较会让 PG 把参数定型成 `real`、宏要求 `f32`，
      而本方法游标参数与 `::float8 as "rank!"` 输出都是 `f64`（**D-74 同族**：动态路径靠隐式
      放宽，宏把参数定型暴露出来）⇒ 显式 `$3::float8`，同时保住 API 与比较精度。
   实测：`dynamic_production` 197 → **191**（−6）、`static` 1282 → **1288**（+6）、
   `dynamic` 总数 929 → **923**、`.sqlx` 1250 → **1256**（+6）、literal 126/33 → **120/32**；
   恒等式 `191 − 120 − 1 = 70` 仍成立。
   验证：`-p synapse-storage --lib --features test-utils -E 'test(/search/)'` ⇒ **31/31**。

37. ✅ **C52-0（补 `quarantine_stream.rs` 真基线覆盖）已完成（2026-09-28）** —— 为 C52 扫清前置：
   该文件此前只有一个"能构造结构体"的纯单测，6 个方法（`record_media_quarantine_change` /
   `get_quarantined_media_changes` / `get_changes_by_media` / `set_media_quarantine_status` /
   `get_media_quarantine_status` / `get_current_stream_id`）**零 DB 覆盖**；而
   `quarantined_media_changes` 六列全 `NOT NULL`、`media_metadata.quarantine_status` 可空
   —— 正是宏转换最容易搞错可空性的形状。
   新增一条真基线生命周期用例（`isolated_test_pool()`，R9）覆盖：record 的 stream_id 严格递增、
   `> since` / 升序 / LIMIT、按 media_id 过滤、空表 `get_current_stream_id` = 0 与插入后取 MAX、
   以及 `media_metadata` 侧的四种语义（`NULL` / `'quarantined'` / `'clean'` / 缺行）与
   `set_media_quarantine_status` 的命中、幂等重复、缺行返回 false。
   代价：测试区 +1 站点（`media_metadata` 的夹具 INSERT；`#[cfg(test)]` 内宏不进
   `cargo sqlx prepare` ⇒ R9/D-13，无静态等价物）；生产区不变。
   验证：`-p synapse-storage --lib --features test-utils -E 'test(/quarantine_stream/)'` ⇒ **2/2**。

38. ✅ **C52（`media/quarantine_stream.rs` 6 处宏化，该文件生产区动态归零）已完成（2026-09-28）**
   —— C52-0 补上真基线覆盖后，6 处按形状分三类转换：
   - `record_media_quarantine_change` / `get_quarantined_media_changes` / `get_changes_by_media`
     的 6 列投影 ⇒ `query_as!`（列清单与 `QuarantinedMediaChange` 六字段一一对应，R6 ⑤）；
   - `set_media_quarantine_status` 的 `UPDATE` ⇒ `query!` + `.execute()`（无结果列，R6 ①，
     保留 `rows_affected()` 语义）；
   - `get_media_quarantine_status` ⇒ `query_scalar!` + `fetch_optional`（可空列 ⇒
     `Option<Option<String>>` 两层，R6 ②）；`get_current_stream_id` 的 `MAX(stream_id)` ⇒
     `query_scalar!`（聚合、无关系来源 ⇒ 宏推 `Option<i64>`；**空表返回 NULL 是正常语义**，
     `Result` 仍由 `?` 传播 —— `unwrap_or(0)` 只落在那个可空值上，不是吞错，与本战役
     反复处理的"吞掉 DB 错误"形态无关）。
   实测：`dynamic_production` 191 → **185**（−6）、`static` 1288 → **1294**（+6）、
   `dynamic` 总数 924 → **918**、`.sqlx` 1256 → **1262**（+6）、literal 120/32 → **114/31**；
   恒等式 `185 − 114 − 1 = 70` 仍成立。
   验证：`-p synapse-storage --lib --features test-utils -E 'test(/quarantine_stream/)'` ⇒ **2/2**
   （C52-0 的覆盖在转换后**穿过新宏**跑真库）。

39. ✅ **C53-0（先修 D-94 两处吞错 + 补 `monitoring.rs` 覆盖 + 登记 D-95/D-96）已完成（2026-09-28）**
   —— 为 C53 扫清前置，同时登记两条新发现：
   - **D-94（吞错，D-33 同型，2 处，已修）**：`get_performance_metrics` 里
     `pg_stat_statements_enabled` 的 `.fetch_one(…).await.unwrap_or(false)` 把**任何数据库错误**
     降级成"扩展未启用"（监控指标静默少一半）；慢查询三元组的
     `…map(…).unwrap_or((0.0, 0, total_transactions))` 把**查询错误**降级成"没有慢查询"。
     两处都改 `?` 传播；三个聚合列在空集上返回 NULL ⇒ 那一层的 `unwrap_or` 是**真默认值**，保留。
     `EXISTS(...)` 按 R4 ① 断言 `AS "exists!"`（无关系来源，恒 TRUE/FALSE）。
   - **D-96（R7 结构性例外，新登记）**：慢查询那条 SQL 的 `FROM pg_stat_statements` 指向
     **可选扩展** —— baseline 迁移不创建它，本机与 CI 的库都没有该关系 ⇒ 宏在
     `cargo sqlx prepare` 阶段无法 describe（实测 `relation "pg_stat_statements" does not exist`），
     **整份离线缓存都建不起来**。这是"SQL 文本是编译期常量、但关系是否存在取决于运行环境"，
     与 R6 ④ 列出的两类并列的第三种不可宏化情形 ⇒ 该站点保持动态（代码注释 + §7.3 均已登记）。
   - **D-95（新登记，未关闭）**：`verify_data_integrity` 的两条检查
     （`events.room_id` 无对应房间、`room_memberships.user_id` 无对应用户）**结构上不可能命中**
     —— `fk_events_room` 与 `fk_room_memberships_user` 都是外键（`ON DELETE CASCADE`），
     孤儿行根本无法插入 ⇒ 两条全表 `NOT EXISTS` 永远返回空，方法恒报"0 违规 / 100 分"，
     是**不会失败的门禁**。处置需要产品/路由决策（该 admin 端点是否仍有意义、要不要换成能被违反的
     不变量），且删路由要连带重生成契约 fixture（D-87 的教训）⇒ **不在静态化批次里擅自动它**，
     只登记 + 用测试把真守卫（外键）钉住。
   - **覆盖（R8/R9）**：本文件此前**没有任何测试** ⇒ 新增真基线用例覆盖 4 个方法
     （`check_connection`、连接池状态、性能指标、完整健康状态、完整性报告），并用
     "插入孤儿事件 / 孤儿成员关系**必须被外键拒绝**"证明 D-95 的结构性事实。
   实测：`dynamic_production` 185 → **184**、`static` 1294 → **1295**、`dynamic_test` 733 → **735**
   （+2 夹具）、`dynamic` 总数 918 → **919**、`.sqlx` 1262 → **1263**、literal 114/31 → **113/31**。
   ⚠️ 同时补记 **D-93**（C49-0）当时漏进 §0.4/§7.4 计数的一次笔误 —— 本批把两处计数一并订正为
   **96 条 = 已关闭 87 / 未关闭 1（D-95）/ 结构性例外 8（含 D-96）**。
   验证：`-p synapse-storage --lib --features test-utils -E 'test(/monitoring/)'` ⇒ **1/1**。

### 8.4 收尾条件（何时可称"静态化战役结束"）

- `dynamic_production` 的**可机械转换部分（literal）归零**：184 → **101**
  （184 − 82 literal − 1 param = 101 = 测试基建 57 + 分页结构性 15 + **D-14 结构性 29**），
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
