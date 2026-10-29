# 剩余问题与优化建议（2026-10-29 第八轮复核）

> **快照声明**：本文件全部结论基于提交 `96d64c34d`（工作区 clean，无未提交改动）。
> 任何"存在 / 不存在"的判断都与快照绑定 —— 本仓常有并发写入，隔几小时就可能失效。
> **引用本文件的任何结论时，请连同快照提交号一起引用。**

## 0. 取证口径

本文件遵守三条硬规矩，违反其一则结论作废：

1. **"未实现 / 缺失"必须先跑再写。** 本轮共推翻 8 条旧结论（见 §5），其中 3 条是把已实现当成缺失。
2. **数字一律现算**（`scripts/ci/check_doc_metrics.py --print`、`scripts/ci/check_file_size_ratchet.py`），
   不手抄、不转录他人口径 —— 不同口径的数字放在一起比较会得出相反结论（§5 有实例）。
3. **只列当前仍成立的问题**；已关闭项进 §2 并附提交号，避免下次重复排查。

## 1. 结论摘要

| 编号 | 级别 | 问题 | 状态 |
|---|---|---|---|
| **P1-1** | P1 | 状态决议仍保留 `origin_server_ts` 兜底路径 | **已关闭（`f3a91641a`）**：读路径 fail-closed，推导入口只给播种与逃生开关用（见 §2 R-8） |
| **P1-2** | P1 | 无 Redis 时拓扑校验只 `warn!`，不阻断启动 | **已关闭（`a68a93a08`）**：`write_forwarding` + 缺 `notify_fan_out` ⇒ 无条件 fatal，不看 `strict` |
| **P1-3** | P1 | 写归属守卫不校验 handler 行为，只看路由集合与 stream 归属 | **行为维度已补（`67ffd19bc`）**：sticky 补发 tick 的归属改为运行时门；**静态扫描已补**：`check_event_writer_routes.py`（52 条事件写路由 / 18 条约一条理由的例外） |
| **P1-4** | P1 | sticky catch-up 的**服务层零测试** | **已关闭（`67ffd19bc` + 本轮）**：`StickyCatchUpIo` 接缝 + 11 条用例（含跨 destination 轮转公平性） |
| **P2-1** | P2 | 写转发链路没有两进程 e2e | 未解决 |
| **P2-2** | P2 | sliding_sync 逐房 3 次查询（required_state / timeline / sticky） | **bench 已就位**：分档 [10,100,1000] + 6 项 `required_state`；基线数字需种子库（见 §4-C1） |
| **P2-3** | P2 | `events` 未分区、`room_memberships` 无归档（等量级） | **判据已落地**：`events-partition-trigger` 两条告警 + 两个面板（见 §4-C2）；分区本身仍等量级 |
| **P3-1** | P3 | 行数棘轮只报不压：`--update` 会把**新文件**直接纳入基线 | **已关闭（`92504e69f`）**：缩小即自动下压、条目消失自动删；`--update` 默认拒绝从未登记的新文件（`--allow-new` 为显式例外） |
| **P3-2** | P3 | 生产 517 行 `let _ =` / `unwrap_or_default()`（193 文件） | 未解决（登记项） |
| **P3-3** | P3 | 覆盖率基线存在**两代数字**（608 已过时 / 683 现行） | **已关闭**：`docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` 的 608 已就地标注为"bootstrap 时代、现行 683" |

## 2. 本轮（同日）已关闭

- **R-1 注销数据清理缺 5 张表 → `a4a3b0986`。**
  新增 `ACCOUNT_DEACTIVATION_PURGED_TABLES` 常量（12 张表）与逐表清零断言
  （`tests/integration/account_deactivation_purge_tests.rs` 遍历该常量、断言每表对该 user 为 0 行）。
  覆盖补齐：`room_tags`、`event_receipts` + `event_receipts_stream`（单事务）、`thread_read_receipts`、
  `presence`（删库并失效 `user_presence` 缓存 —— 修掉"注销后仍显示在线"）。
- **R-2 UG-7 sticky 联邦置顶 → `50ae32afb` + catch-up 批次。**
  ① 读取改为「本人行 ∪ 房级行」（`user_id NOT LIKE '@%'` 判据）；② 入站以 bare origin 作房级标记；
  ③ per-destination 对账（`revision` 水位 + `room_sticky_event_destinations.last_revision` 游标）
  与"加入即补发"（接在两个联邦 join handler 上）。
- **R-3 行数棘轮红 → 基线 `--update` 到 222。**
  注意这条是**被基线吸收**而不是被拆分消除的，机制缺口仍见 P3-1。
- **R-4 P1-4 sticky catch-up 服务层零测试 → `67ffd19bc`。**
  `run_catch_up_tick` / `backfill_to_destination` 与 io 之间插入 `StickyCatchUpIo`（8 方法，
  生产实现 `LiveStickyCatchUpIo` 只做薄适配），10 条用例：游标推进到最新 / `Ok(false)` 与 `Err`
  都不推进 / 整批 128 行续页（`row_fetches == [0, 128]`）/ 墓碑与过期行投影为移除 / seed 失败
  整轮跳过 / destination 列举失败零发送 / **非所有者零 io 调用** / backfill 归零重放 /
  backfill 本机短路与 reset 失败短路。内联测试按仓库惯例外移为 `sticky_catch_up/tests.rs`。
  变异自证 5 条（删归属门、两条分支误推进游标、去掉过期投影、seed 失败不 return），
  每次仅 1 条红、还原后 10/10 绿。
- **R-5 P1-3 单写者判据从静态改运行时 → `67ffd19bc`（仅 sticky tick 这一段）。**
  门放在共享自由函数 `run_catch_up_tick` 首行，`RoomServiceConfig.worker` 注入拓扑、
  `RoomService` 只存派生 `owns_events: bool`（判定仍委托 `owns_events_dispatch`）。
  守卫本身（`write_stream_map.txt` 双向棘轮）当时仍是静态的 —— 该缺口由下面的 R-6 关闭。
- **R-6 P1-3 残余①（静态扫描）→ 新增 `check_event_writer_routes.py` + 例外清单。**
  从真实路由表出发扫描每条路由 handler（含 3 层 helper 传递）是否调用"造事件"入口；命中即要求该
  `(method, path)` 在 `write_stream_map.txt` 里有一个非 `-` 的流。当前 52 条事件写路由、18 条例外
  （16 条 `record_audit_event` → `audit_events`、1 条 `space_service.invite_user` → `space_events`、
  1 条审计事件写入端点本身），逐条带理由。已接入 `preflight.sh`。
  变异自证 2 条：给 `reset_user_password` 插一次 `.create_event(` 且不登记 ⇒ 红；在例外清单里塞一条
  陈旧条目（`get_user GET /_synapse/admin/v1/users/{user_id}`）⇒ 红；两处还原后绿。
- **R-7 P1-4 残余②（跨 destination 轮转公平性）→ 第 11 条用例。**
  `FakeIo` 改为游标表以复刻 SQL 三语义，40 对 destination 两轮 tick 全部收敛、无重复、无饥饿。
- **R-8 P1-1 状态决议去掉 `origin_server_ts` 兜底（fail-closed）→ `f3a91641a`。**
  ① 读路径：`get_state_event` / `get_state_events` 无 `state_groups` 记录即 `Err`（`state.rs::missing_state_group`，
  `sqlx::Error::Protocol`），两条 `ORDER BY origin_server_ts DESC` 分支合并为「有记录读记录 / 无记录报错」；
  唯一逃生开关是 `SYNAPSE_ALLOW_STATE_TS_FALLBACK`（任意值即开，等价于旧的推导路径）。
  ② 推导入口改名为 `EventReader::derive_state_from_event_log`（真实实现 + 内存 mock 各一份），
  只允许两个调用方：写路径的一次性播种 `seed_from_current_state` 与逃生开关本身 ——
  播种若改回走 `get_state_events`，新房间的第一个状态写会撞 fail-closed 变成死锁式回归。
  ③ **变异时发现的真实缺口**：本地状态写原本**根本不维护记录**（只有 `create_event_with_graph` /
  `create_outlier_event` / 联邦 membership 三处调 `after_state_event`）⇒ 记录被冻结在建房那一刻，
  `m.room.history_visibility` 永远读到建房的 `shared`，改 `invited` 也不生效（历史泄漏）。
  补两块：`run_post_create_side_effects` 里对每次本地状态写调 `after_state_event`（失败只 `warn!`，
  不影响事件写入）；建房路径加幂等的 `StateRecord::seed_new_room`（建房整段走一个事务、
  不经过 `after_state_event`，不补则新房间 `GET /rooms/{id}/state` 直接报错）。
  ④ 回填脚本 `scripts/migration/backfill_state_groups.py` 同步修正：去掉硬编码测试库（改 `--database-url` /
  `DATABASE_URL`）、`state_hash` 改为与 Rust 逐字节一致（原先是 `str(...).encode().hex()[:64]` 的假 hash）、
  `event_to_state_groups` 只绑锚事件（原来把每个状态事件都绑进同一组，等于谎称它们都在该状态被决议）、
  `ON CONFLICT (state_hash)` 支持重跑。
  ⑤ 测试隔离：裸 SQL 造房间的夹具（`dedup_marker_failure_soft_fails_the_created_event`，没有状态事件也没有
  `state_groups`）在 A1 之后连事件都写不进去 ⇒ 用 `EnvGuard` 开逃生开关，并与断言 fail-closed 的
  `forked_state_is_resolved_and_served` 共用 `serial(state_ts_fallback)` 这把锁
  （`serial_test` 只互斥同 key 的用例，进程级环境变量会泄漏给并发用例）。
  ⑥ 变异自证 4 条：M1 `state_ts_fallback_allowed` 恒 `true` ⇒ 存储层用例红；M2 恒 `false` ⇒ 同一用例另一段红；
  M3 删建房 `seed_new_room` ⇒ `api_history_visibility` **9/9 红**；M4 关 `run_post_create_side_effects` 的维护块
  ⇒ **5/9 红**（invited / joined / room_sync / single_event / relations）。还原后全绿。
- **R-9 P1-2 无 Redis 阻断启动 → `a68a93a08`（并发写入者落地，本轮核对口径）。**
  `check_startup_topology` 把「`write_forwarding=true` 且 `missing_notify_fan_out` 非空」单列为
  `hard` 一类，产出 `fatal` 时**不看** `topology.strict`；`strict` 从此只管其余静态规则的升级。
  用例 `forwarding_without_redis_is_always_fatal` 对 `strict ∈ {true,false}` 双跑，断言 `fatal` 必须存在且
  其文本点名修法；`warnings_alone_are_not_escalated_by_strict` 钉住"`strict` 不再吞告警级之外的东西"。
  代价与 §4-A2 预告一致：开发/单机环境必须显式关 `worker.enabled`，不能"配着玩"。

## 3. 仍存在的问题

### P1-1 状态决议仍保留 `origin_server_ts` 兜底 —— **已关闭（`f3a91641a`，§2 R-8）**

**原现象**：房间没有 state group 记录时，读路径回落到 `ORDER BY origin_server_ts DESC LIMIT 1`。
**原证据**：`synapse-storage/src/event/state.rs` 的两处读取（`get_state_event` / `get_state_events`）
各有一个 `STATE_GROUP_REQUIRED` 环境变量分支，未设置时执行上述查询（该文件内命中 4 处）。
**原影响**：`origin_server_ts` 是发送方时钟，联邦事件可能带着偏旧时间戳后到 ⇒ 推导结果可能与 DAG 顺序相反。
**落地**：`STATE_GROUP_REQUIRED` 常量已删除（`grep -c` 应为 0），默认行为就是 fail-closed；
推导被降级为独立入口 `derive_state_from_event_log`，只给播种与 `SYNAPSE_ALLOW_STATE_TS_FALLBACK` 用。
判据与 4 条变异见 §2 R-8。
**残余（仍成立）**：`SYNAPSE_ALLOW_STATE_TS_FALLBACK` 仍在（回填窗口用），语义是"整进程回到旧推导"；
它不该长期留在生产配置里 —— 回填完成后应从部署清单消失，这一条没有机器门禁，靠发布说明约束。

### P1-2 无 Redis 时只告警，不阻断启动 —— **已关闭（`a68a93a08`，§2 R-9）**

**原现象**：`worker.write_forwarding=true` + `worker.enabled=true` + `redis.enabled=false` 时，
写入会在 writer 实例执行，而跨实例唤醒 `/sync` 长轮询只靠 Redis 扇出 ⇒ 落在 reader 的长轮询
要等客户端自己的 timeout（典型 30 s）才拿到数据。数据不丢，只是变慢。
**原证据**：`synapse-services/src/worker/topology_gate.rs` 的 `check_startup_topology` 把该情形放进
`validation.errors`（`valid=false`），但 `fatal` 只在 `config.topology.strict` 为真时才产出；
`src/server/mod.rs` 因此在 `valid=false` 时只 `warn!` 并继续启动。
**落地**：该情形已单列 `hard` 一类，`fatal` 不再受 `strict` 影响；`src/server/mod.rs` 见到 `fatal` 即拒绝启动。
**残余（仍成立）**：告警级的其余拓扑问题在非 strict 下仍只 `warn!`（这是设计意图 —— 它们不改变正确性，
只是慢或可观测性变差），因此**没有**"拓扑全静态校验"这回事，别按这个前提写发布检查单。

### P1-3 写归属守卫不校验 handler 行为

**现象**：`scripts/contract/write_stream_map.txt` 的双向棘轮校验的是
① `(method, path)` 集合 == ledger 的 client-surface 写集合；② 第三列 `stream` == `stream_for_write()` 的返回值。
**证据**：`tests/unit/worker_write_stream_tests.rs` 的模块文档自述 "The ratchet is **bidirectional**"。
**影响**：一个 handler 若**行为上**开始写事件表而没有新增路由，守卫不会红 ⇒ 多写者部署下"单写者"前提被悄悄打破，
症状是跨实例重复 fan-out（不是错误，是重复推送/顺序错乱）。
**已补（`67ffd19bc`，仅 sticky 补发这一段）**：周期 tick 的归属判据从"构造时读配置、静态可信"改为
**运行时门** —— 门在共享自由函数 `run_catch_up_tick` 首行（`if !io.is_events_writer() { return; }`），
非所有者实例连 seed 都不会调用，由 `non_writer_instance_is_inert` 断言 `calls() == []` 钉住。
判定本体仍委托唯一实现 `owns_events_dispatch(&WorkerConfig)`；`RoomService` 只存构造期派生的 `owns_events: bool`。
**已补（本轮，静态扫描这一半）**：新增门禁 `scripts/ci/check_event_writer_routes.py`，从真实路由表出发，
对每条路由的 handler（含 3 层 helper 传递）做静态扫描，命中"造事件"入口（`create_event` / `send_message` /
`join_room` / `invite_user` / `redact_event` …）即要求该 `(method, path)` 在 `write_stream_map.txt` 里
有一个非 `-` 的流；确属本地写（`audit_events` / `space_events`，共 18 条）逐条登记在
`scripts/contract/event_writer_allowlist.txt` 且**带理由**。登记项双向校验：漏登记 ⇒ 红，登记项陈旧 ⇒ 也红。
已接入 `preflight.sh` 契约段（只读源码 + 两份清单，不与 HEAD 比，不受契约链 SKIP 分支影响）。
**残余（仍成立）**：运行时门目前只覆盖 sticky tick **一处**，
其余"该只由 events owner 跑"的路径尚无同款断言，需逐个补。

### P1-4 sticky catch-up 的服务层零测试 —— **已关闭（`67ffd19bc` + 本轮补 ②）**

**原现象**：对账循环与"加入即补发"没有任何测试；`sticky_catch_up.rs` 内 `#[cfg(test)]` 计数为 0，
`tests/` 下检索三个入口名无命中。存储层另有 4 条（`synapse-storage/src/sticky_event/db_tests_destinations.rs`）。
**落地**：`run_catch_up_tick` / `backfill_to_destination` 与 io 之间插入 `StickyCatchUpIo`（8 方法），
生产实现 `LiveStickyCatchUpIo` 只做薄适配；内联测试按仓库惯例外移为 `sticky_catch_up/tests.rs`（`67ffd19bc` 落 10 条，本轮补第 11 条）。
覆盖：游标推进到最新 / `Ok(false)` 与 `Err` 都不推进 / 整批 128 行续页（`row_fetches == [0, 128]`）/
墓碑与过期行投影为移除 / seed 失败整轮跳过 / destination 列举失败零发送 / 非所有者零 io 调用 /
backfill 归零重放 / backfill 本机短路与 reset 失败短路。
**判据**：`cargo test -p synapse-services --lib --features test-utils sticky_catch_up` ⇒ 11 passed；
变异自证 5 条（删归属门、两条分支误推进游标、去掉过期投影、seed 失败不 return），每次仅 1 条红、还原后 10/10 绿；
本轮为第 11 条另做 3 条变异（`FakeIo::pending_destinations` 去掉 `LIMIT` ⇒ 40≠32 / tick 内加 `.take(1)` ⇒ 1≠32 /
过滤条件恒真 ⇒ 32≠40），均红并还原。
**残余（本轮已关闭）**：§4-A4 ②"轮转饥饿"（destination 数超每 tick 上限后多轮收敛）已由第 11 条用例
`destinations_beyond_the_per_tick_cap_rotate_without_starvation` 覆盖：40 对 `(room_id, destination)`
（= `CATCH_UP_DESTINATIONS_PER_TICK` + 8）两轮 tick 后 40 对全部推进到 `revision == 1`，且无重复消费。
为此把 `FakeIo` 的 destination 表示从 `Vec` 改成**游标表** `Mutex<HashMap<(room, destination), i64>>`，
复刻 `pending_sticky_destinations` 的三个语义（两谓词过滤 / `ORDER BY (room_id, destination)` / `LIMIT`）
与 `advance_cursor` 的 `GREATEST`（`*slot = (*slot).max(revision)`）—— 否则"多轮收敛"在 fake 里是假命题。

### P2-1 写转发链路没有两进程 e2e

**证据**：`tests/unit/worker_write_forwarding_two_instance_tests.rs` 的模块文档明写
"What this deliberately does **not** cover: two OS processes"（两个真实 HTTP 实例、真 TCP，但同进程）；
`scripts/test/worker_topology_smoke.sh` 里检索 `write_forwarding` / `转发` 无命中。
**影响**：进程边界（启动顺序、配置隔离、共享 secret、surface gate 与转发的组合）从未被真实验证。

### P2-2 sliding_sync 逐房 3 次查询

**证据**：`synapse-services/src/sliding_sync_service/filters.rs` 在两处房间循环里调用 `build_room_json`，
其内部依次查 required_state、timeline、sticky ⇒ 每房 3 次查询。
**影响**：房间数线性放大查询数；属"逐房装配"的性能特征，不是单行查询的病态 N+1。
**前置**：动手前必须先有带 `required_state` 的分档 bench 基线，否则无法证明改进。
**前置已就位（2026-10-30）**：`benches/performance_sliding_sync_benchmarks.rs` 的分档改为
`[10, 100, 1000]`，且 initial-sync 请求带 6 项 `required_state`（`m.room.{name,topic,avatar,canonical_alias,join_rules}` + `m.room.member/$ME`）。
此前 `required_state: None` 会让每房最贵的那次查询直接消失在测量里 —— 这也是"逐房 3 次"一直没被量化的原因。
**仍未做**：基线**数字**。bench 只在 `BENCHMARK_DATABASE_URL` 可达时跑，本地默认库 `synapse_bench` 不存在
（CI 的 `benchmark.yml` 才 provision），而 bench 本身不造房间数据 ⇒ 空库跑出来的数字没有意义。
取数方式：在 CI job 里带 artifact，或本地建迁移后的 `synapse_bench` 并灌 N 房 fixture。

### P2-3 `events` 未分区、`room_memberships` 无归档

**证据**：`migrations/00000000_unified_schema_v12.sql` 里 `events` 是普通 `CREATE TABLE`；
全文 `PARTITION` 仅 1 处（注释/待办）；`room_memberships` 为普通表，无归档表或分区。
**定性**：**等外部条件**（需要真实数据量判断收益）。上游对应做法本轮**未取证**，不写入方案依据。
**判据已落地（2026-10-30）**：`docker/deploy/prometheus/alerting-rules.yml` 新增
`events-partition-trigger` 组，两条告警 —— `EventsTablePartitionDue`（warning：`events` 行数 > 1e7
或体积 > 20 GiB）与 `EventsTablePartitionOverdue`（critical：> 5e7 行或 > 100 GiB），均 `for: 1h`；
数据源是 postgres_exporter 默认的 `pg_stat_user_tables`（Prometheus 的 `postgres` job 已在抓）。
面板加在 `docker/deploy/grafana/dashboards/postgresql.json`（行数 / 体积各一个），
指标名登记进 `scripts/ci/dashboard_metrics_allowlist`。
**阈值是 provisional**：依据是"单表还进得了 page cache、autovacuum 能在维护窗口内跑完"的量级上限，
按真实负载校准；**分区本身仍不排期**。
实测（2026-10-30，dev 库）：`events` = **1,356 行 / 89 MiB**，距 warn 线还有约 4 个数量级。

### P3-1 行数棘轮只报不压

**现象**：文件缩小或条目过期时，脚本只打印 "some entries are now under the threshold — run --update to tighten"，
不自动下压；反之 `--update` 会把**当前树**里所有超 600 行的文件写进基线。
**证据**：`scripts/ci/check_file_size_ratchet.py` 的 `stale` 分支只 `print`；本轮 `--update` 后
一个 813 行的**新文件**被直接纳入基线（`222 over 600 (baseline 222)`）。
**影响**：棘轮方向单一 —— 只挡新增，不逼收敛；"新文件超 600"这道门槛可被一次 `--update` 抹平。
**建议方向**：`over[p] < baseline[p]` 时自动下压；新文件（不在基线且在本次提交新增）禁止走 `--update`。
**已关闭（`92504e69f`）**：无硬违规时 gate **就地重写基线** —— 缩小下压（打印 `tightened <path>: was -> now`）、
条目消失或降到阈值以下自动删除；`--update` 默认**拒绝**"从未登记过"的超限文件（`--allow-new` 是显式例外并打 WARNING）。
判据：抽出 `plan_tightening` / `new_offenders_of` 两个纯函数做 6 条离线断言；真实树上首次运行即自动下压 2 条
（`container.rs` 769→767、`presence/mod.rs` 1450→1448）；造 601 行临时文件后 `--update` 退出 2 并列出该文件。

### P3-2 生产 517 行静默吞错

**证据**：`scripts/ci/check_doc_metrics.py --print` 实测 `SILENT_IGNORES_PROD_LINES=517` / `PROD_FILES=193`。
**定性**：登记项。其中已分档的 DB 吞错应逐个改成 `?` + 上下文；**不设总数配额**
（设配额会诱导写 `unwrap()` 来"减少计数"）。

### P3-3 覆盖率基线两代数字并存

**现象**：文档里同时存在 `608` 与 `683` 两个"基线条目数"，容易误读成"口径不一致"。
**实测**：`608` 是 bootstrap 时代的条目数（`TEST_COMPROMISE_AUDIT_2026-10-25.md` 记录的一次自毁事故
"608 条砍成 1 条"），`683` 是**现行**基线条目数（`docs/audit/REMAINING_ISSUES_SWEEP_2026-10-17.md` 的
"677 文件 / 683 基线"）。两者是**两代**，不是两套口径。
**建议方向**：历史数字加"（已过时，现行 683）"标注，或直接删掉只留现行值 + 脚本输出。
**已关闭**：`docs/audit/TEST_COMPROMISE_AUDIT_2026-10-25.md` §3.1.1 基线分布处首次出现的 608 已就地标注为
"**bootstrap 时代 608 个文件**；现行基线条目数为 **683**"，两个数字不再并列出现而无解释。

## 4. 优化建议（对照 Synapse v1.163.0）

> 上游依据优先取本仓已取证的 `docs/synapse-rust-vs-synapse-comparison.md` §19；
> 该文档**未取证**的上游行为，本文不写进方案。

### 批次 A（可立即做，改动小、收益直接）

**A1 状态决议去掉时间戳兜底（对应 P1-1）**
- 上游：`event_to_state_groups` 是唯一真相，没有"按时间戳推导"这条路。
- 改法：把 `STATE_GROUP_REQUIRED` 从"环境变量开关"改为默认行为 —— 无记录即播种，播种失败即 `Err`；
  删掉两条 `ORDER BY origin_server_ts DESC` 回落分支。
- 判据：① 两个环境变量分支合并为无分支；② 补一条"联邦乱序后状态仍按 DAG"的用例；
  ③ 回填脚本作为存量房的一次性动作写进发布说明。
- **状态：✅ 已做（`f3a91641a`）。** 两个分支合并为「有记录读记录 / 无记录 `Err`」；判据 ② 由
  `forked_state_is_resolved_and_served` 的"记录与推导给出相反答案"对照断言承担（推导仍选被 auth 规则拒绝的
  较新分支 ⇒ 结论只能来自记录）；判据 ③ 的回填脚本另修了三处失真（假 hash / 绑每个状态事件 / 硬编码测试库）。
  落地细节与 4 条变异见 §2 R-8。

**A2 Redis 前置条件改为硬失败（对应 P1-2）**
- 上游：多实例/worker 是部署前置条件，缺必要组件直接拒绝启动，不当降级处理。
- 改法：`check_startup_topology` 拆两档 —— `write_forwarding=true` 且 `missing_notify_fan_out` 非空
  ⇒ **无条件 fatal**（不看 `strict`）；`strict` 只管其余静态规则。
- 判据：新增用例"非 strict + 无 Redis + events 转发 ⇒ 启动失败"；`--validate-topology` 的退出码同口径。
- 代价：开发/单机环境必须显式关闭 `worker.enabled`，不能再"配着玩"。
- **状态：✅ 已做（`a68a93a08`）。** 见 §2 R-9。

**A3 单写者守卫补行为维度（对应 P1-3）**
- 改法：静态扫描 `synapse-web/src/routes/**` 里调用"造事件"入口（如 `create_and_send_event`、
  admin 的 membership 写入）的处理函数，要求其所在 `(method, path)` 已在 `write_stream_map.txt` 登记；
  白名单只放"确属本地写"的例外。
- 判据：变异自证 —— 给某个 admin handler 加一次事件写入但不登记 ⇒ 门禁必红；还原 ⇒ 绿。
- **状态：✅ 已做。** 脚本 `scripts/ci/check_event_writer_routes.py`（例外清单
  `scripts/contract/event_writer_allowlist.txt`），已接入 `preflight.sh` 契约段。
  `67ffd19bc` 落的"运行时归属门"与此是两条独立补法：运行时门证明"非所有者不跑"，
  静态扫描证明"该登记的没漏登记"—— 两者都需要，缺一仍有盲区。
- 实测口径：52 条事件写路由全部有非 `-` 声明；18 条例外逐条带理由（16 条审计表、1 条 space 表、1 条审计端点本身）；
  1 条 handler 静态不可解析（根状态页 closure，`GET /`）。

**A4 catch-up 补三条服务层测试（对应 P1-4）**
- ① 单 destination 有积压 ⇒ 一轮 tick 后游标推进到最新；✅ `tick_advances_cursor_to_last_confirmed_row`
- ② destination 数超过每 tick 上限 ⇒ 按 `ORDER BY room_id, destination` 轮转、无饥饿（跑多轮后全部收敛）；
  ✅ `destinations_beyond_the_per_tick_cap_rotate_without_starvation`
- ③ join 补发把 `last_revision` 重置为 0 ⇒ 下一轮全量重放、且不会重复消费同一条；
  ✅ `backfill_resets_then_replays_the_whole_room`（③ 的重放部分）+ `backfill_short_circuits_on_self_and_on_reset_failure`
- 判据：三条都不需要真联邦（用 fake io），断言游标值与广播次数 —— 已按此实现（`FakeIo` 记录 `Call` 序列）。
- **状态：✅ 三条全部覆盖。** `67ffd19bc` 落 10 条（①③），本轮补第 11 条（②，跨 destination 公平性），
  见 §3 P1-4 末段的落地说明与 3 条变异自证。

### 批次 B（需要一套多进程夹具，合并做）

**B1 两进程 e2e + 转发断言（对应 P2-1）**
- 改法：在 `scripts/test/worker_topology_smoke.sh` 里用 `docker/docker-compose.workers.yml` 起最少两个角色，
  写请求打到 reader、**用只读连接直查库**佐证数据由 writer 落库（不看 200，只看行）。
- 判据：脚本退出码 + DB 断言双绿；故意停掉 writer ⇒ 必红。

### 批次 C（有真实负载/数据再做）

**C1 sliding_sync 查询收敛（对应 P2-2）**：先建带 `required_state` 的分档 bench（10/100/1000 房），
再考虑把 required_state 与 sticky 改成按房列表批量取。
**状态**：bench 已改（`[10, 100, 1000]` + 6 项 `required_state`）；**基线数字仍缺**，
需种子库（见 §3 P2-2 的取数方式）。**查询收敛未动** —— 没有基线就不改。
**C2 `events` 分区 / 归档（对应 P2-3）**：先加"何时该分区"的判据（单表行数与表体积阈值 + 面板），
到阈值再立项。
**状态**：判据已落地（`events-partition-trigger` 两条告警 + 两个面板 + allowlist）；
**分区本身仍不排期**，等告警触发。
**C3 行数棘轮自收紧（对应 P3-1）**：`over[p] < baseline[p]` 自动下压；新增文件禁止 `--update` 纳入。
**状态**：已关闭（`92504e69f`）。

## 5. 本轮被证伪 / 撤回的旧结论

以下条目在以往清单里被写成"问题"，本轮逐条证伪，**请勿重复排查**：

| 旧结论 | 本轮实测 |
|---|---|
| UG-6 `/preview_url` 返回伪造 OpenGraph | **不成立**：`synapse-services/src/url_preview.rs` 每跳 `check_url_and_resolve` + DNS 固定客户端 + `no_redirect`，限额 5s / 2 跳 / 256 KiB |
| Room v12 A3「存储层未接线」 | **不成立**：`copy_forward → persist → bind_event_to_state_group` 在 `synapse-services/src/room/state_record.rs` 内成链；模块文档里那句 "today unused" 是**注释假象** |
| Worker W-B…W-E「全未做」 | **不成立**：`--validate-topology` 三态退出码、`topology.strict` 拒绝启动、`docker/docker-compose.workers.yml` + `docker/nginx/worker-routing.conf` 四角色 upstream |
| W-I ①③④⑥ 各缺口 | **不成立**：`missing_notify_fan_out` 三入口全接、`write_forwarding` 字段在拓扑响应里、`worker-write-forwarding` 告警组恰 2 条、`Body::wrap_stream` 已流式转发 |
| Room v12 C1（`.sqlx` 残留动态查询） | **不成立**：四个可疑哈希在 HEAD 与工作区均 0 命中 |
| UG-7「远端置顶本地成员看不到」 | **已随 `50ae32afb` 关闭**（仅对更早快照成立） |
| CODE-10「510/187 vs 517/193 差 1.4% 是老化」 | **跨口径无效**：权威口径就是 `--print` 的 517/193 |
| 覆盖率「608/627/683/684 四种口径并存」 | **误读**：608 与 683 是两代条目数；627 是另一指标（某条腿被比对的文件数）；684 与覆盖率无关 |

## 6. 复现命令

```bash
# 状态决议兜底是否还在（A1 之后应为 0 —— 常量已删，默认 fail-closed）
grep -c STATE_GROUP_REQUIRED synapse-storage/src/event/state.rs

# 逃生开关只剩一个，推导入口只该给"播种 + 逃生开关"用（逐个看调用链，别只看命中数）
grep -rn "STATE_TS_FALLBACK_ENV\|derive_state_from_event_log" --include=*.rs synapse-storage synapse-services synapse-common

# A1 存储层 2 条 + 服务层 1 条（fail-closed / 记录优先于推导 / 记录与推导分歧时只能读记录）
cargo test -p synapse-storage --lib -- state_ts_fallback
cargo test -p synapse-storage --lib -- test_current_state_prefers_the_resolved_state_record
cargo test -p synapse-services --lib --features test-utils -- forked_state_is_resolved_and_served

# A1 端到端：建房播种 + 本地状态写维护记录（M3/M4 变异都在这 9 条上体现）
cargo test --test integration --features "test-utils privacy-ext voice-extended voip-tracking beacons server-notifications" -- api_history_visibility

# 无 Redis 是否仍只告警（A2 之后：hard 一类不受 strict 影响，应为 fatal）
grep -n "let fatal = if !hard.is_empty()" -A 6 synapse-services/src/worker/topology_gate.rs
cargo test -p synapse-services --lib --features test-utils topology_gate

# catch-up 服务层用例（`67ffd19bc` 外移为兄弟文件 10 条，本轮补 ② ⇒ 应为 11）
grep -c '#\[tokio::test\]' synapse-services/src/room/sticky_catch_up/tests.rs
cargo test -p synapse-services --lib --features test-utils sticky_catch_up

# 运行时归属门是否仍在 tick 首行（应命中 `if !io.is_events_writer()`）
grep -n "is_events_writer" synapse-services/src/room/sticky_catch_up.rs

# 守卫本体（`write_stream_map.txt` 双向棘轮）的盲区自述仍在 —— 该盲区已由下面的门禁填上
sed -n '29,31p' scripts/contract/write_stream_map.txt

# 静态扫描门禁（§4-A3）：应为 `✅ 52 event-writing route(s) all declare a stream (18 exception(s), 1 ...)`
python3 scripts/ci/check_event_writer_routes.py
python3 scripts/ci/check_event_writer_routes.py --self-test   # 16 条判据自证
python3 scripts/ci/check_event_writer_routes.py --print       # 逐条打印识别出的路由 + writer + 调用链

# 两进程 e2e 缺口
grep -n "two OS processes" tests/unit/worker_write_forwarding_two_instance_tests.rs
grep -c "write_forwarding" scripts/test/worker_topology_smoke.sh

# 行数棘轮与覆盖度量（口径唯一来源）
python3 scripts/ci/check_file_size_ratchet.py
python3 scripts/ci/check_doc_metrics.py --print
```

## 7. 方法论备注（写给下一次复核的人）

1. **快照必须写明提交号。** 本仓有并发写入：同一份"存在问题"清单在 4 小时内被关闭了 2 条（P0 + UG-7）。
2. **两个门禁是"工作区函数"而非"提交快照函数"**：`check_doc_metrics.py` 的文件计数、
   `check_plan_numbers_fresh.sh` 的 `.sqlx` 条目数。任何一方在改文件数或重生成 `.sqlx` 时，
   **另一方的 preflight 必然红**，与它自己的改动无关；且文档数字只能与**同一提交**里的文件集一致。
3. **"未接线"类结论要看调用方，而不是看函数是否存在**（A3 的教训）；
   **"已实现"类结论要看它在哪个提交里**（UG-7 的教训）。
4. **数字自跑。** 本轮 8 条被推翻的结论里，有 2 条源于转录他人数字/口径。
5. **"当前问题"可能是"中间态"，必须同时记工作区是否可编译。**
   本次写文档期间，另一个写入者给 `RoomServiceConfig` 加了 `worker` 字段而构造点尚未跟上，
   工作区一度不可编译（`E0063`）—— 而 HEAD 本身是好的（HEAD 的该结构体没有 `worker` 字段）。
   依赖编译的门禁（包括文档可信度守卫）在这段时间必然红，**与文档内容无关**。
   ⇒ 记"存在问题"时要写清"是设计缺陷，还是半应用的编辑"。
