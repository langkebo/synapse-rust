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
| **P1-1** | P1 | 状态决议仍保留 `origin_server_ts` 兜底路径 | 未解决 |
| **P1-2** | P1 | 无 Redis 时拓扑校验只 `warn!`，不阻断启动 | 未解决 |
| **P1-3** | P1 | 写归属守卫不校验 handler 行为，只看路由集合与 stream 归属 | **行为维度已补（`67ffd19bc`）**：sticky 补发 tick 的归属改为运行时门 |
| **P1-4** | P1 | sticky catch-up 的**服务层零测试** | **已关闭（`67ffd19bc`）**：`StickyCatchUpIo` 接缝 + 10 条用例 |
| **P2-1** | P2 | 写转发链路没有两进程 e2e | 未解决 |
| **P2-2** | P2 | sliding_sync 逐房 3 次查询（required_state / timeline / sticky） | 未解决 |
| **P2-3** | P2 | `events` 未分区、`room_memberships` 无归档（等量级） | 未解决（登记项） |
| **P3-1** | P3 | 行数棘轮只报不压：`--update` 会把**新文件**直接纳入基线 | 未解决（机制） |
| **P3-2** | P3 | 生产 517 行 `let _ =` / `unwrap_or_default()`（193 文件） | 未解决（登记项） |
| **P3-3** | P3 | 覆盖率基线存在**两代数字**（608 已过时 / 683 现行） | 未解决（口径） |

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
  守卫本身（`write_stream_map.txt` 双向棘轮）仍是静态的，缺口见 §3 P1-3 的"残余"。

## 3. 仍存在的问题

### P1-1 状态决议仍保留 `origin_server_ts` 兜底

**现象**：房间没有 state group 记录时，读路径回落到 `ORDER BY origin_server_ts DESC LIMIT 1`。
**证据**：`synapse-storage/src/event/state.rs` 的两处读取（`get_state_event` / `get_state_events`）
各有一个 `STATE_GROUP_REQUIRED` 环境变量分支，未设置时执行上述查询（该文件内命中 4 处）。
**影响**：`origin_server_ts` 是发送方时钟，联邦事件可能带着偏旧时间戳后到 ⇒ 推导结果可能与 DAG 顺序相反。
**缓解现状**：首次状态写即 `seed_from_current_state` 播种，另有一键回填脚本 `scripts/migration/backfill_state_groups.py`
⇒ 实际受影响面是"从未有过状态写的存量房"。
**为什么是 P1 而不是 P0**：有播种 + 回填 + 环境变量三条退路；但默认行为仍是"静默使用不保证的顺序"。

### P1-2 无 Redis 时只告警，不阻断启动

**现象**：`worker.write_forwarding=true` + `worker.enabled=true` + `redis.enabled=false` 时，
写入会在 writer 实例执行，而跨实例唤醒 `/sync` 长轮询只靠 Redis 扇出 ⇒ 落在 reader 的长轮询
要等客户端自己的 timeout（典型 30 s）才拿到数据。数据不丢，只是变慢。
**证据**：`synapse-services/src/worker/topology_gate.rs` 的 `check_startup_topology` 把该情形放进
`validation.errors`（`valid=false`），但 `fatal` 只在 `config.topology.strict` 为真时才产出；
`src/server/mod.rs` 因此在 `valid=false` 时只 `warn!` 并继续启动。
**影响**：部署看起来健康，但每条推送都晚到 —— **正确性测试发现不了**，这正是它该是启动错误的原因。
**触发面**：需三个条件同时成立，且 `write_forwarding` 默认 `false`，所以不是任意部署都会中。

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
**残余（仍成立）**：① 守卫本体（`write_stream_map.txt` 双向棘轮）仍是**静态**的，一个 handler 行为上新增事件写
而路由不变，门禁依旧不会红 —— 见 §4-A3；② 运行时门目前只覆盖 sticky tick **一处**，
其余"该只由 events owner 跑"的路径尚无同款断言，需逐个补。

### P1-4 sticky catch-up 的服务层零测试 —— **已关闭（`67ffd19bc`）**

**原现象**：对账循环与"加入即补发"没有任何测试；`sticky_catch_up.rs` 内 `#[cfg(test)]` 计数为 0，
`tests/` 下检索三个入口名无命中。存储层另有 4 条（`synapse-storage/src/sticky_event/db_tests_destinations.rs`）。
**落地**：`run_catch_up_tick` / `backfill_to_destination` 与 io 之间插入 `StickyCatchUpIo`（8 方法），
生产实现 `LiveStickyCatchUpIo` 只做薄适配；内联测试按仓库惯例外移为 `sticky_catch_up/tests.rs`（10 条）。
覆盖：游标推进到最新 / `Ok(false)` 与 `Err` 都不推进 / 整批 128 行续页（`row_fetches == [0, 128]`）/
墓碑与过期行投影为移除 / seed 失败整轮跳过 / destination 列举失败零发送 / 非所有者零 io 调用 /
backfill 归零重放 / backfill 本机短路与 reset 失败短路。
**判据**：`cargo test -p synapse-services --lib --features test-utils sticky_catch_up` ⇒ 10 passed；
变异自证 5 条（删归属门、两条分支误推进游标、去掉过期投影、seed 失败不 return），每次仅 1 条红、还原后 10/10 绿。
**残余**：§4-A4 原列的"轮转饥饿"（destination 数超每 tick 上限后多轮收敛）尚未有专门用例 —— 现有
`full_batch_continues_to_the_next_page` 只覆盖**单 destination 的分页**，不覆盖**跨 destination 的公平性**。

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

### P2-3 `events` 未分区、`room_memberships` 无归档

**证据**：`migrations/00000000_unified_schema_v12.sql` 里 `events` 是普通 `CREATE TABLE`；
全文 `PARTITION` 仅 1 处（注释/待办）；`room_memberships` 为普通表，无归档表或分区。
**定性**：**等外部条件**（需要真实数据量判断收益）。上游对应做法本轮**未取证**，不写入方案依据。

### P3-1 行数棘轮只报不压

**现象**：文件缩小或条目过期时，脚本只打印 "some entries are now under the threshold — run --update to tighten"，
不自动下压；反之 `--update` 会把**当前树**里所有超 600 行的文件写进基线。
**证据**：`scripts/ci/check_file_size_ratchet.py` 的 `stale` 分支只 `print`；本轮 `--update` 后
一个 813 行的**新文件**被直接纳入基线（`222 over 600 (baseline 222)`）。
**影响**：棘轮方向单一 —— 只挡新增，不逼收敛；"新文件超 600"这道门槛可被一次 `--update` 抹平。
**建议方向**：`over[p] < baseline[p]` 时自动下压；新文件（不在基线且在本次提交新增）禁止走 `--update`。

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

**A2 Redis 前置条件改为硬失败（对应 P1-2）**
- 上游：多实例/worker 是部署前置条件，缺必要组件直接拒绝启动，不当降级处理。
- 改法：`check_startup_topology` 拆两档 —— `write_forwarding=true` 且 `missing_notify_fan_out` 非空
  ⇒ **无条件 fatal**（不看 `strict`）；`strict` 只管其余静态规则。
- 判据：新增用例"非 strict + 无 Redis + events 转发 ⇒ 启动失败"；`--validate-topology` 的退出码同口径。
- 代价：开发/单机环境必须显式关闭 `worker.enabled`，不能再"配着玩"。

**A3 单写者守卫补行为维度（对应 P1-3）**
- 改法：静态扫描 `synapse-web/src/routes/**` 里调用"造事件"入口（如 `create_and_send_event`、
  admin 的 membership 写入）的处理函数，要求其所在 `(method, path)` 已在 `write_stream_map.txt` 登记；
  白名单只放"确属本地写"的例外。
- 判据：变异自证 —— 给某个 admin handler 加一次事件写入但不登记 ⇒ 门禁必红；还原 ⇒ 绿。
- **状态：未做（静态扫描这部分）**。`67ffd19bc` 只落了"运行时归属门"这一半（见 §3 P1-3 的已补/残余），
  与 A3 的静态扫描是两条独立补法：运行时门证明"非所有者不跑"，静态扫描证明"该登记的没漏登记"。

**A4 catch-up 补三条服务层测试（对应 P1-4）**
- ① 单 destination 有积压 ⇒ 一轮 tick 后游标推进到最新；✅ `tick_advances_cursor_to_last_confirmed_row`
- ② destination 数超过每 tick 上限 ⇒ 按 `ORDER BY room_id, destination` 轮转、无饥饿（跑多轮后全部收敛）；❌ 未覆盖
- ③ join 补发把 `last_revision` 重置为 0 ⇒ 下一轮全量重放、且不会重复消费同一条；
  ✅ `backfill_resets_then_replays_the_whole_room`（③ 的重放部分）+ `backfill_short_circuits_on_self_and_on_reset_failure`
- 判据：三条都不需要真联邦（用 fake io），断言游标值与广播次数 —— 已按此实现（`FakeIo` 记录 `Call` 序列）。
- **状态：`67ffd19bc` 落了 10 条，其中 ①③ 已覆盖，②（跨 destination 公平性）仍缺**，见 §3 P1-4 残余。

### 批次 B（需要一套多进程夹具，合并做）

**B1 两进程 e2e + 转发断言（对应 P2-1）**
- 改法：在 `scripts/test/worker_topology_smoke.sh` 里用 `docker/docker-compose.workers.yml` 起最少两个角色，
  写请求打到 reader、**用只读连接直查库**佐证数据由 writer 落库（不看 200，只看行）。
- 判据：脚本退出码 + DB 断言双绿；故意停掉 writer ⇒ 必红。

### 批次 C（有真实负载/数据再做）

**C1 sliding_sync 查询收敛（对应 P2-2）**：先建带 `required_state` 的分档 bench（10/100/1000 房），
再考虑把 required_state 与 sticky 改成按房列表批量取。
**C2 `events` 分区 / 归档（对应 P2-3）**：先加"何时该分区"的判据（单表行数与 `pg_total_relation_size` 阈值 + 面板），
到阈值再立项。
**C3 行数棘轮自收紧（对应 P3-1）**：`over[p] < baseline[p]` 自动下压；新增文件禁止 `--update` 纳入。

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
# 状态决议兜底是否还在（应为 4 处命中）
grep -c STATE_GROUP_REQUIRED synapse-storage/src/event/state.rs

# 无 Redis 是否只告警（fatal 仅 strict）
grep -n "config.topology.strict" synapse-services/src/worker/topology_gate.rs
sed -n '210,226p' src/server/mod.rs

# catch-up 服务层用例（`67ffd19bc` 后：外移为兄弟文件，应为 10）
grep -c '#\[tokio::test\]' synapse-services/src/room/sticky_catch_up/tests.rs
cargo test -p synapse-services --lib --features test-utils sticky_catch_up

# 运行时归属门是否仍在 tick 首行（应命中 `if !io.is_events_writer()`）
grep -n "is_events_writer" synapse-services/src/room/sticky_catch_up.rs

# 守卫本体是否仍是静态的（仍命中 ⇒ §4-A3 未做）
sed -n '16,22p' tests/unit/worker_write_stream_tests.rs

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
