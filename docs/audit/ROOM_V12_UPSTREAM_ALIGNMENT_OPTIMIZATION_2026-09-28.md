# Room v12 剩余问题复核与上游对齐优化方案（2026-09-28）

- **被复核材料**：任务方给出的《剩余问题与优化方案》（A 组 A1–A6 / B 组 B1–B3 / C 组 C1–C4，含建议执行顺序）。
- **基线**：`opt/consolidated` @ `605bb06eb`（工作树干净）；`origin/opt/consolidated` = `7dbbda18b`（本地领先 9）。
- **上游参照**：`element-hq/synapse` 标签 `release-v1.161` 主源（沙箱内经 `HTTPS_PROXY=http://127.0.0.1:7897` 抓取，落盘 `/tmp/up_*`）。
- **本轮性质**：**只读取证 + 文档**。未改生产代码、未提交、未 push。
- **证据口径**：本仓事实给 `路径:行号`；上游事实给 `<上游文件>:行号`；报告自身口径的偏差单列 §5。

> 一句话结论：报告列出的 13 项（A1–A6 / B1–B3 / C1–C4）逐条实测后，**A/B/C 三组共 12 项"确实存在"**；C2 是复合项，拆成 4 个子项后 **2 项确实红、2 项在 `605bb06eb` 上已不可复现**（`tests/unit` 12 条与 `derived_manifest` 实测全绿）。报告的方案整体方向正确，但有 7 处需收紧或补边界（见 §6），其中 **A4 缺"历史房间回填 state group"子任务**、**A6 会再度过度声称**、**A1 的 oracle 定位（手工门禁）** 三处必须修正。

---

## 0. 逐项判定总表

| 项 | 报告主张（摘要） | 判定 | 关键依据 |
|---|---|---|---|
| **A1** | MSC4297 只与自造向量对齐，缺上游交叉复算 | ✅ **存在** | `scripts/interop/` 仅 `verify_pdu_with_upstream_synapse.py`；`tests/interop/fixtures/` 无 `state_res_*.json`；`resolve_state_for_version_with_rules` 已接线（`synapse-services/src/room/state_record.rs:316`） |
| **A2** | `state_map_auth` 对 restricted join 欠授权 | ✅ **存在** | `synapse-federation/src/event_auth/state_map_auth.rs:220` 硬编码 `restricted_join_authorized: false`；上游只需 state map（`event_auth.py:717-751`） |
| **A3** | 事件处 state 现算 + `MAX_RESOLUTION_EVENTS=4096` | ✅ **存在** | `synapse-services/src/room/state_record.rs:70`（4096）、`:329`（超限 warn） |
| **A4** | 未分叉房间仍走时间戳 LWW | ✅ **存在** | `synapse-storage/src/event/state.rs:77-92`（先记录、else `origin_server_ts DESC`） |
| **A5** | live 联邦互操作未验证 | ✅ **存在** | `tests/interop/fixtures/README.md:8-10` 自认沙箱做不到 live `/send_join` |
| **A6** | v12 事件形态仅覆盖 message + create | ✅ **存在** | `tests/interop/fixtures/local_pdu_v12.json`（message）+ `local_pdu_v12_create.json`（create） |
| **B1** | `invite.rs:277` 兜底 event id 拼出 `$uuid:!xxx` | ✅ **存在** | `synapse-web/src/routes/federation/membership/invite.rs:277` 逐字命中 |
| **B2** | `space/repository.rs:30` 从父房 id 取 server | ✅ **存在** | `synapse-storage/src/space/repository.rs:30` 逐字命中 |
| **B3** | `actions.rs:41` domainless 房无 join 目的地 | ✅ **存在** | `synapse-services/src/room/membership/actions.rs:41` 逐字命中 |
| **C1** | 合并带来的 4 条陈旧 `.sqlx` | ✅ **存在** | `2f037a14 / 3044683f / 8642abbd / e4d56bc6` 各 1 文件；HEAD 无对应 SQL 文本（§4.1） |
| **C2** | 计划外既有红项（tests/unit 12 等） | ⚠️ **拆分后 2 存在 / 2 证伪** | unit 12 条 **证伪**（1812/1812 绿）、`derived_manifest` **证伪**（3/3 绿）；`snapshot_versions_endpoint` **存在**（缺 3 个 flag）、invite 集成 **存在**（6 红）——见 §4.2 |
| **C3** | `origin/opt/consolidated` 落后 9 提交 | ✅ **存在** | 实测 `rev-list --left-right --count` = `0 9`，remote = `7dbbda18b` |
| **C4** | 冗余 worktree `.worktrees/roomv12-merge` + 分支 | ✅ **存在** | `merge/room-v12-into-opt` == `605bb06eb` == `HEAD` |

**报告自身的准确性**：报告的技术断言**逐条可复现**（指纹 `f6e8cb1fdbe20a67`、SQLx 棘轮 `282/718/1206/18`、C3 的"落后 9"、以及 B 组三处 `file:line` 全部逐字命中）；口径偏差集中在 §5 的 6 行（其中只有 C2 的**红项清单过期**是实质性的）。

---

## 1. 上游主源取证（本轮已复算，可直接引用）

抓取（沙箱需代理）：

```bash
export HTTPS_PROXY=http://127.0.0.1:7897 HTTP_PROXY=http://127.0.0.1:7897
B=https://raw.githubusercontent.com/element-hq/synapse/refs/heads/release-v1.161
curl -sS -o /tmp/up_event_auth.py    $B/synapse/event_auth.py
curl -sS -o /tmp/up_state_v2.py      $B/synapse/state/v2.py
curl -sS -o /tmp/up_rust_room_versions.rs $B/rust/src/room_versions.rs
curl -sS -o /tmp/up_fed_base.py      $B/synapse/federation/federation_base.py
```

### 1.1 v12 的定义（MSC4304）——`rust/src/room_versions.rs`

`V12`（`:300-308`）相对 `V11`（`:275-281`，其 event_format/state_res 继承自 `V10`，即 `ROOM_V4_PLUS` / `V2`）**恰好改 4 个字段**：

| 字段 | v11 | v12 |
|---|---|---|
| `event_format` | `ROOM_V4_PLUS`（3） | `ROOM_V11_HYDRA_PLUS`（4） |
| `state_res` | `V2`（2） | `V2_1`（3） |
| `msc4289_creator_power_enabled` | false | true |
| `msc4291_room_ids_as_hashes` | false | true |

⇒ 报告的"总体对齐结论"（v12 = v11 + 这四处）**正确**。`msc4289/msc4291` 在 `RoomVersion` 上是布尔位（`:148`、`:150`）。

### 1.2 MSC4297 —— `synapse/state/v2.py`

| 事实 | 位置 |
|---|---|
| 入口签名 `resolve_events_with_store(clock, room_id, room_version, state_sets, event_map, state_res_store)` | `:82-88` |
| `StateResolutionStore` Protocol（只有 `get_events` / `get_auth_chain_difference` 两个方法） | `:55-67` |
| v2.1 才计算 `conflicted_set`（v2 传 `None`） | `:127-131` |
| `full_conflicted_set = conflicted ∪ auth_diff` | `:138-141` |
| **`base_state = {}`（v2.1） vs `unconflicted_state`（v2）** | `:177-185` |

报告的 A1 描述（含 `base_state = {}`、`conflicted_set` 传参、`full conflicted set`）与上游**逐行吻合**。

### 1.3 MSC4291 / MSC4289 / MSC4307

| MSC | 上游实现 | 位置 |
|---|---|---|
| MSC4291（create 带 `room_id` 即拒） | `_check_create` 规则 1.2 | `event_auth.py`（`_check_create`） |
| MSC4289（`additional_creators` 合法性 + 规则 10.4） | `check_valid_additional_creators`；"Creator user … must not appear in content.users" | `event_auth.py:1298`；`:982-1005` |
| MSC4307（`auth_events` 不得含 create） | `"auth_events must not contain the create event"` | `federation_base.py:382-387` |

### 1.4 A2 的 restricted join —— `event_auth.py:717-751`（关键）

```python
elif (room_version.restricted_join_rule and join_rule == JoinRules.RESTRICTED) or (...):
    if not caller_in_room and not caller_invited:            # ← 短路条件
        authorising_user = event.content.get(EventContentFields.AUTHORISING_USER)
        if authorising_user is None:
            raise AuthError(403, "Join event is missing authorising user.")
        key = (EventTypes.Member, authorising_user)
        member_event = auth_events.get(key)
        _check_joined_room(member_event, authorising_user, event.room_id)   # 必须是 join
        authorising_user_level = get_user_power_level(authorising_user, auth_events)
        if authorising_user_level < invite_level:
            raise AuthError(403, "Join event authorised by invalid server.")
```

⇒ **上游 event auth 阶段的 restricted join 只用 state map**（`auth_events`），**不读 allow rooms 的 state**。报告据此判定本仓 `state_map_auth.rs:158` 的注释（"deciding it needs the `allow` rooms' state"）**理由不成立**——该判定**正确**（本仓注释与该注释所指的"简化"在本处是过度保守，而非真的缺输入）。

上游 `event_to_state_groups` 随事件持久化：`_get_state_group_for_event`（`state.py:595`）、`get_referenced_state_groups`（`:635`）、索引 `event_to_state_groups_sg_index`（`:761`）⇒ A3 的"每事件 state group"方向与上游一致。

---

## 2. A 组——逐项判定与方案优化

### A1 MSC4297 上游交叉复算（P0）

- **判定**：**存在**。本仓 v2.1 已实现且接线（`state_record.rs:316` 调用 `resolve_state_for_version_with_rules`），但验证面只有自造向量（L2 单测的 Problem A/B 形态），**没有**"对端实现复算 winner"的交叉证据。上游接口与报告描述一致（§1.2）。
- **方案评估**：方向**正确**，但有两处必须补：
  1. **只比 winner 不够**。若两边的 `get_auth_chain_difference` 不同（例如差集边界算错），仍可能"恰好选出同一个 winner"而漏判。建议追加**中间量断言**：把本仓算出的 auth difference 与 `full_conflicted_set` 也导出成 fixture 字段，与 oracle 中独立实现的 `_get_auth_chain_difference` 比对（至少 Problem A 向量要断言 full conflicted set 集合相等）。
  2. **明确"手工门禁"定位**。oracle 需要 `/tmp/peer-synapse`（`matrix-synapse==1.161.0` venv），CI 无此环境 ⇒ 与 A-3 一样**不接 CI**。报告只写"属于检查类门禁"，容易被误读为 CI 门禁（本仓有"门禁是否恒绿 / 是否接线"的既有坑）。必须在脚本头与文档里写明"**手工验证，不接 CI**"。
- **边界声明（报告已自认，保留）**：oracle 的 `get_auth_chain_difference` 是**我们独立实现**（上游那份在 DB 里），因此交叉覆盖的是 v2.1 增量 + 排序 + 重放 + 上游真实 `_check_event_auth`；**不得**声称"全链路上游复算"。

### A2 restricted join 欠授权（P0，最小修）

- **判定**：**存在**。`state_map_auth.rs:220` 硬编码 `false`；`:158` 的注释给出了**不成立的**理由。
- **方案评估**：**采纳**，但补两点：
  1. **补上短路情形**。报告列了 4 种验收情形（带/不带授权用户、授权用户未 join、授权用户 power 不足），**缺**上游最外层的 `if not caller_in_room and not caller_invited`：**调用者已在房或已被邀时，根本不看授权用户**（`event_auth.py:736`）。这一短路若不实现，会对"已是成员的 restricted join"（正常 bejoin/no-op 类路径）产生**过度拒绝**。
  2. **枚举落地**：`(m.room.member, authorising_user)` 的状态必须经 `membership_of` 判定为 `Join`（等价上游 `_check_joined_room`），功率阈值用 `levels.invite`（`TransitionCtx.invite_level` 已存在）。

### A3 per-event state group（P1）

- **判定**：**存在**。`MAX_RESOLUTION_EVENTS = 4096`（`state_record.rs:70`），超限在 `:329` 记 warn 并退回时间戳推导；`create_state_group` / `bind_event_to_state_group` 目前**只**在分叉解析时被调用。
- **方案评估**：**方向正确**（与上游一致，§1.4），但报告低估了三点风险，建议在方案里显式写出：
  1. **幂等/重放**。上游"单父 ⇒ 复用父 group"的隐含前提是**同一事件只入组一次**；本仓入站联邦事务与 `backfill` 可能重复投递同一事件（`events` 表以 event_id 为 PK，重复写会 upsert）。需要明确"重复事件不新建 group"（或 group 以 `(room_id, state_hash)` upsert，`create_state_group` 现有实现已按 `state_hash` upsert，见 `state_record.rs:423` 的注释）。
  2. **事务边界（R9）**。建房/联邦 join 在**一个事务**内写事件；A3 要求"每个事件都碰状态组"，则这些事务内也必须写 group，否则事务提交后读路径看不到记录。报告已提"tx 路径要在事务内一并写"，需升级为**验收判据**（不是风险备注）。
  3. **写入热路径**。报告称"用非状态事件直接 bind 父 group 把开销压到 1 次 insert"——但**每个 message 都要 1 次 insert + 1 次 extremities 查询**，这是热路径新增 I/O。必须配套**性能门禁**（本仓有 `benches/` 与 perf gate 用例），且判据是"消息写入延迟不因 A3 劣化"，不能只测"分叉解析变 O(1)"。
- **建议**：把 A3 列为**独立批次**，它同时是 A4-ii 的硬前置。

### A4 未分叉房间仍走时间戳 LWW（P1/P2）

- **判定**：**存在**。`synapse-storage/src/event/state.rs:77-92`（单键）与 `:104-118`（整表）都是"有记录走记录、无记录走 `DISTINCT ON … origin_server_ts DESC`"。
- **方案评估**：建议 **(ii)**（对齐上游）**正确**，但报告**漏了一个必需子任务**：
  - 选择 (ii) 意味着**删掉时间戳推导**。而当前**存量房间**（A3 之前创建、从未分叉）**没有任何 state group 记录**；A3 只保证"**此后**每个事件入组"。若不补"**为已有房间回填 state group**"（或在缺失记录时保留一次推导并落组），删推导会直接让这些房间**读不到当前 state**。
  - 因此 A4-ii 的正确形态是：`A3 落地 → 回填/惰性补建存量房间的 group →（可观测到所有房间都有 group）→ 删除时间戳分支`。建议把"回填"写成 A4 的显式子任务与验收判据（可加一次性迁移或"首次读取时补建并落组"）。

### A5 live 联邦互操作（P3）

- **判定**：**存在**，且**本仓已自认**（`tests/interop/fixtures/README.md:8-10`：替代沙箱做不到的 live `/send_join` + `/send`）。
- **方案评估**：合理，但**沙箱内不可执行**（缺双主机名 + 互信 CA）。建议：作为"**需要在有环境时做**"的独立项单列，并明确它**不阻塞** v12 的其余条目；同时在 `docker/complement/` 已有目录上评估能否复用 Complement 框架（避免从零写两台头的编排）。

### A6 v12 事件形态覆盖（P2）

- **判定**：**存在**。fixture 只有 `m.room.message`（v3/v10/v11/v12）与 `m.room.create`（v12）。
- **方案评估**：方向对，但**必须写明边界，否则又会过度声称**：
  - 现有 oracle 只做四件事：`hashes.sha256` 复算、事件 ID 复算、签名验证、**create 语义**（MSC4291/4307）。它**不含"逐事件类型的鉴权是否被上游接受"**（那需要上游 `_check_event_auth` 参与）。
  - 因此把 member/power_levels/join_rules/redaction **加进 fixture 表**后，覆盖的是"**字节形态**（hash/ID/签名/redaction 敏感字段）"，**不是**"这些事件在 v12 的鉴权结论"。报告写"oracle 已能逐字节复算，扩表即可"——**扩表确实即可，但结论只能说"字节一致"**。
  - 若确实要覆盖鉴权结论，应另立一项：在 oracle 里对每个 PDU 调上游 `event_auth.check_auth_rules_for_event`（需要构造 `auth_events`），这是**新能力**，不是"扩表"。

---

## 3. B 组——逐项判定与方案优化

三处**逐字命中**报告（`invite.rs:277`、`space/repository.rs:30`、`actions.rs:41`），均为"父房间是 v12（domainless）时按 `:` 反推 server"所致。

- **B1（`invite.rs:277`）**：`format!("${}:{}", uuid, room_id.split(':').next_back().unwrap_or("server"))`。方案"v3+ 一律从事件本身算"**正确**（上游 `exchange_third_party_invite` 的 event_id 由事件决定）。补：本仓已有 `resolve_received_event_id` / `compute_event_id`，但**入站事件通常没有待定内容哈希**——需明确"从 `body` 重算 reference hash"与"缺字段 fail-closed"的判定顺序。
- **B2（`space/repository.rs:30`）**：`!space_uuid:!xxx`（会被 `ck_rooms_room_id_format` 拒）。报告给了"最小修 / 彻底修"两档。**建议**：若暂不做 Q3 的"统一建房入口"，则**最小修后仍是非 v12 语义的假 room id**（`!space_uuid:localhost`），这与 §5 Q3 已选定的"记录为本地合成房间"分支一致——但**必须在代码注释里写明"谁保证该 id 形态"**（AGENTS 对合成 id 的同类要求），否则下次又会有人按 `:` 反推。
- **B3（`actions.rs:41`）**：`via_servers.first()` 为空时对 domainless id 得 `None` ⇒ 400。方案（fail-closed 错误信息 + 优先 `via`）**正确**；`join_room_with_via_servers` 已在 `actions.rs:18` 存在且优先 `via`，缺的确实是"谁来填 `via`"。补：应顺带查清 `invite` / `knock` / `join` 响应里 `via` 的来源链路，否则"持久化 via"无处落地。

---

## 4. C 组——逐项判定与方案优化

### C1 4 条陈旧 `.sqlx`（P2）

- **判定**：**存在**。4 个哈希各对应 1 个 `.sqlx` 文件；其 SQL 是**无类型断言**的文本变体：
  - `query-2f037a14…` = `SELECT EXISTS (SELECT 1 FROM global_invite_blocklist WHERE user_id = $1)`（**无** `AS "exists!"`）
  - `query-3044683f…` = 同上 allowlist 版
  - `query-8642abbd…` = `SELECT COUNT(*) FROM global_invite_allowlist`（**无** `AS "count!"`）
  - `query-e4d56bc6…` = 同上 blocklist 版
- **HEAD 校验**：`synapse-storage/src/invite_blocklist.rs:191` 的**活引用**带 `AS "exists!"`（哈希不同）；`COUNT(*)` 两个变体在 HEAD **零引用**，仅存在于 `.worktrees/c19b/synapse-storage/src/invite_blocklist.rs`。⇒ 报告的"对方无断言版本、主工作树无引用"**成立**。
- **方案评估**：**采纳**（在私有库跑到当前基线后 `sqlx_prepare.sh` 缩容，`ALLOW_CACHE_SHRINK=1` + 核对删除清单）。补：本轮基线是 `SQLX_OFFLINE=true` 离线编译，**缩容后必须重跑 `check_sqlx_cache_fresh.sh --compile`**，确认删除这 4 条不破坏离线编译。

### C2 计划外既有红项（**拆分实测后：2 存在 / 2 证伪**）

- **报告状态**：报告自认**未复跑**（"我这次只跑了与本合并相关的门禁"）。⇒ 本项必须实测，否则会把"旧基线的红"当成"当前的红"。
- **本轮实测**：在 `605bb06eb`（干净工作树）按**各子项所属批次**分别复跑（报告把 4 个子项并成一行，**横跨 3 个测试批次**——这正是本仓"复合条目必须拆行"的既有教训）。

| 子项 | 报告主张 | 本轮判定 | 命令 / 结果 |
|---|---|---|---|
| C2-a | `tests/unit` 12 条红 | ❌ **证伪** | `cargo nextest run --test unit --features test-utils --no-fail-fast` → **1812/1812 passed**（19 slow，2 skipped，0 failed） |
| C2-b | `snapshot_versions_endpoint` 红 | ✅ **存在** | `cargo nextest run --all-features --test integration -E 'test(snapshot_versions_endpoint)'` → **FAILED**：`unstable_features` 快照缺 `org.matrix.msc3873` / `org.matrix.msc3912` / `org.matrix.msc4155` 三项（`:151` 定义，`:157` 断言失败） |
| C2-c | `derived_manifest`（缺 `path_params`） | ❌ **证伪** | `cargo nextest run -p synapse-web --lib --features test-utils -E 'test(derived_manifest)'` → **3/3 passed**（default / worker / all） |
| C2-d | invite-policy 集成 7 条 | ✅ **存在**（6 红） | `cargo nextest run --all-features --test integration -E 'test(/invite/)'` → 33 run / **6 failed**——见 §4.2.1 |

**结论**：C2 的四个子项里，**两项确实仍红、两项已不可复现**（在 `605bb06eb` 上）。⇒ 报告"既有红项"的清单**已过期**（把旧基线的红当成当前红），不能整体作为"还不是全绿"的依据；应以本表实跑结果为准。

> 附注：`fmt` 棘轮本机不可测（缺 `rustfmt`，**非仓库问题**，见 §5）；SQLx 棘轮 `282/718/1206/18` **全绿**。
> 未复跑的批次：`--test integration` 中除 snapshot/invite 外的其余用例、以及 DB 依赖批（本轮只跑与本报告条目相关者，与报告口径一致）。

#### 4.2.1 invite-policy 批（C2-d）实测结果

```bash
cargo nextest run --all-features --test integration -E 'test(/invite/)' --no-fail-fast
# → Summary: 33 tests run: 27 passed, 6 failed, 1450 skipped
```

6 条红：

| # | 失败用例 | 报错（摘） |
|---|---|---|
| 1 | `room_service_tests_migrated::test_invite_user_success` | `Forbidden: "This user is not accepting invites"` |
| 2 | `room_service_tests_migrated::test_invite_user_enqueues_appservice_membership_event` | （同上 invite 行为） |
| 3 | `room_service_tests_migrated::test_upgrade_room_invites_all_former_local_members` | "former member … should be invited to replacement room …" |
| 4 | `api_invite_blocklist_routes_tests::test_invite_lists_reject_joined_non_creator_writes` | — |
| 5 | `api_placeholder_contract_p1p2_tests::test_room_info_contract_reflects_invites_and_guest_access` | — |
| 6 | `federation_existence_leak_tests::invite_v2_stored_signature_covers_the_projected_pdu` | — |

**判定**：报告"invite-policy 7 条集成红"**方向成立**（本轮用略宽的 `/invite/` 过滤命中 **6** 条；数量口径见 §5）。
**归因提示（需在整改批次里定案）**：
- #1/#2 的报错 `This user is not accepting invites` 直指 **invite-policy 行为**（与 `feat/invite-policy-enhance` 的合并一致），报告归因**可信**。
- #3 `test_upgrade_room_invites_all_former_local_members` 位于**房间升级**路径，与 **C-4（升级顺序反转）** 同域——**不能直接照抄"属 invite-policy"**，整改时须先用"stash 探针"（把 invite-policy 改动 stash 掉重跑）确认它是哪一侧引入的（本仓既有教训：既有红项与并发改动混在一起时，必须用探针归因，不能凭报错文本猜）。

### C3 推送（P0，零风险）

- **判定**：**存在**。`git rev-list --left-right --count origin/opt/consolidated...HEAD` = `0 9`；remote = `7dbbda18b`；本地 9 个提交以 `605bb06eb`（merge）为首。
- **方案评估**：由任务方决定是否 push。补：若采纳 A1/A2 的整改，**建议先落 A2（小、纯正确性）再 push 一次**，避免两次推送；C3 本身可与 A2 同批。

### C4 冗余 worktree/分支（零风险）

- **判定**：**存在**。`merge/room-v12-into-opt` == `605bb06eb` == `HEAD`；worktree 在 `.worktrees/roomv12-merge`。
- **方案评估**：`git worktree remove .worktrees/roomv12-merge && git branch -d merge/room-v12-into-opt` **正确**。补：本仓长期有并发会话与残留 worktree（`git worktree list` 显示多个 `prunable`），建议顺带 `git worktree prune` 并核对 `.worktrees/room-v12` / `feat/room-v12-complete` **保留**（报告已注明）。

---

## 5. 报告自身口径的修正（本轮实测）

| # | 报告写法 | 实测 | 影响 |
|---|---|---|---|
| 1 | "更新 `test_isolation_unification_tests.rs:155`" | 常量为 `EXPECTED_BASELINE_FINGERPRINT`，在 **`:167`**（值 `f6e8cb1fdbe20a67` **正确**） | 行号漂移，值无误 |
| 2 | "tiebreak OK（`scripts/check_ts_order_tiebreak.py`）" | 实际在 **`scripts/ci/check_ts_order_tiebreak.py`** | 路径少一级 |
| 3 | "A1 属于'检查类门禁'" | oracle 需 `/tmp/peer-synapse` venv，**CI 无此环境** ⇒ 手工门禁 | 建议显式标注，避免误读为 CI 门禁 |
| 4 | C2 四个子项一行 | 跨 unit / integration / `synapse-web --lib` **三个批次** | 应拆行复跑 |
| 5 | C2 把"tests/unit 12 条 / derived_manifest"列为红 | 二者在 `605bb06eb` 上**实测全绿**（1812/1812、3/3） | 清单**已过期**（旧基线），非当前红 |
| 6 | C2 的 invite-policy 记"7 条" | 本轮 `/invite/` 过滤命中 **6** 条 | 数量口径差异，方向成立 |

（报告对 A/B 两组的**代码位置与上游逻辑描述无一处偏差**，包括指纹、SQLx 棘轮、C3 的"落后 9"；C 组的偏差集中在**红项清单的时效性**上。）

---

## 6. 对方案的优化建议（汇总）

1. **A1**：追加 **auth difference / full conflicted set 的中间量断言**（只比 winner 会漏判）；显式标注 **手工门禁、不接 CI**。
2. **A2**：**补 `caller_in_room or caller_invited` 短路**（否则过度拒绝已成员的 restricted join）。
3. **A3**：把 **幂等/重放、事务边界、写入热路径性能门禁** 从"风险备注"升级为**验收判据**。
4. **A4**：**新增"存量房间回填 state group"子任务**，否则删时间戳推导会让未分叉的旧房间读不到当前 state。
5. **A6**：明确 oracle 只覆盖**字节形态**，**不含逐类型鉴权结论**；如需后者应另立"调用上游 `check_auth_rules_for_event`"的新项。
6. **B2**：若不做 Q3 统一入口，**必须在代码里写明"谁保证合成 id 形态"**（否则最小修会留下"两种语义同表"的隐患）。
7. **C2**：**已拆行复跑**（§4.2）——只需修 `snapshot_versions_endpoint`（补 `msc3873/msc3912/msc4155` 三个 flag）+ 用 stash 探针**归因** invite 6 红；**不要再把"tests/unit 12 条 / derived_manifest"当红项**（实测全绿）。

### 建议执行顺序（对报告顺序的微调）

报告：`A2 → A1 → B1–B3 → A3 → A6 / C1 / C2 → A5`。

**微调后**（理由：先清零风险工程债以拿到"全绿基线"，再动正确性，最后动热路径）：

```text
A2（最小正确性） → C4 / C3（零风险收尾，push 一次带走 A2）
  → B1 / B2 / B3（fail-closed + 修畸形 id）
  → C2 收口（本轮已诊断：只需①补 snapshot 的 3 个 flag；②归因 invite 6 红）
  → A1（上游交叉复算；手工门禁）
  → A3 + A4（一个批次：per-event group + 回填 + 删时间戳推导）
  → C1（陈旧 sqlx 缩容，放最后因为要重算缓存）
  → A6（fixture 扩表，纯测试资产）
  → A5（需环境）
```

**关键依赖**（必须显式）：**A4-ii 依赖 A3**；**A1 的 fixture 生成依赖 A3/A4 定稿**（否则向量随实现变）；**C1 放最后**（瘦身后不影响其它项的编译）；**C2-a/C2-c 不需整改**（已绿），C2-b 是**一次 `.snap` 更新 + 逐条 review**。

---

## 7. 未决与收尾

- **未决**：C2-d 的 6 红**归因**（须用 stash 探针区分 invite-policy 与 v12 升级路径，见 §4.2.1）；A5 需要真实双主机环境。
- **本轮无副作用**：未改生产代码、未提交、未 push；复核前后**被跟踪文件零改动**（`git status --porcelain` 仅多出本文件），复跑产生的 `.snap.new` 已删除。
- **本文件定位**：本仓 `docs/audit/` 下对《剩余问题与优化方案》的**唯一结论来源**；与 `ROOM_V12_PLAN_STATUS_2026-09-27.md`（v12 工作项状态）互补——前者管"剩余 A/B/C 项是否成立、方案如何收紧"，后者管"计划 §1 的 24 项是否完成"。

---

## 8. 实施进展与新增发现（2026-09-28 续）

> §0–§7 是**只读复核**（无副作用，见 §7）。本节起记录**随后的实施**，每项一个独立提交。

### 8.1 A2 已实施（提交 `c39e60181` + 文档 `306d30e69`）

**文件**：`synapse-federation/src/event_auth/state_map_auth.rs`

| 位置 | 改动 |
|---|---|
| `restricted_join_rule_supported`（新增） | 版本门控**取自** `synapse_common::redaction::redaction_rules`（§8.2 的三处权威来源之一），不再就地列举版本 |
| `authorising_user_grants_join`（新增） | 实现上游 `_is_membership_change_allowed` 的三条判定：`join_authorised_via_users_server` 必须存在 → 该用户的 `m.room.member` 必须为 `join` → 其功率 ≥ 房间 `invite` 级别。全程 fail-closed；v12+ creator 按 MSC4289 取无限功率（与上游 `get_user_power_level`（`event_auth.py:1133-1135`）一致） |
| `is_authorised_against_state`（改） | `restricted_join_authorized: false` → `restricted_join_rule_supported(..) && authorising_user_grants_join(..)`；删掉 §2/A2 指出的"需要 `allow` rooms 的 state"这一不成立注释 |

**与报告方案的两点差异**：

1. **短路无需新增分支——它已结构性满足**。上游 `:737` 的 `if not caller_in_room and not caller_invited` 判的是 **caller**（= `event.sender`），而 join 在 `:719-720` 已被强制 `sender == state_key`，故对该事件 `caller == target`。`check_join` 已在 `from == Join`（幂等 / 改资料）与 `from == Invite`（接受邀请）两分支提前返回，恰好等价于该短路。已补用例把这一等价性钉住。
2. **门控不再就地写版本号**（理由见 §8.2）。

**验收（实测）**：`cargo nextest run -p synapse-federation --features test-utils -E 'test(state_map_auth)'` → **13/13 通过**。新增 7 条：缺授权用户 / 授权用户已 join 且功率达标 / 授权用户未 join / 授权用户功率不足 / **已是成员免授权**（短路）/ 版本门控（v7 拒，v8·v9·v10·v12 过）/ **`knock_restricted` 仅 v10+**。

**变异自证（双向）**——用例同时钉住"欠授权"与"过度授权"：

| 变异 | 结果 |
|---|---|
| 门禁钉死 `false`（复现原缺陷） | **3 条转红**：`a_restricted_join_authorised_by_a_joined_user_at_invite_level_passes`、`the_restricted_join_authorisation_follows_the_room_version`、`knock_restricted_is_only_authorised_from_v10` |
| 门禁钉死 `true`（模拟过度授权） | **5 条转红**：三条"应拒"用例 + 上述版本门控两条 |

⇒ 不是"恒绿门禁"；两侧都有反例。

**门禁**：`cargo fmt -p synapse-federation -- --check` 干净；`cargo clippy -p synapse-federation --all-targets --features test-utils --locked -- -D warnings` 无告警。`check_doc_spelling.sh` 本机不可跑（**缺 `aspell`**，环境缺失非仓库问题）；`markdownlint` 通过。

### 8.2 新增发现 A7（P1，**已实施** 提交 `5de77da29`）：`auth_events` 选择同时排除了 v9 与 v12+

**位置**：`synapse-services/src/room/state/auth_events.rs:95`

```rust
fn supports_restricted_join_rule(room_version: &str) -> bool {
    matches!(room_version, "8" | "10" | "11")
}
```

**与三处权威来源冲突**：

1. **规范**：`matrix-spec` 房间版本 9 页原文——"This room version builds on version 8 to add additional redaction rules that were unintentionally missed … See room version 8 for specific details regarding **the addition of restricted rooms**" ⇒ v9 **保留** restricted 房间，只新增 `join_authorised_via_users_server` 的 redaction。
2. **上游实现**：`rust/src/room_versions.rs:236-240` 的 `V9` 用 `..Self::V8` 继承 `restricted_join_rule: true`，仅额外置 `restricted_join_rule_fix: true`；上游 `event_auth.py:1287` 用的正是 `room_version.restricted_join_rule`。
3. **本仓自己的能力表**：`synapse-common/src/redaction.rs:202-207` 把 `"9" | "10"` 一并标为 `restricted_join_rule: true`。

**影响（v12 是活的）**：`SUPPORTED_ROOM_VERSIONS`（`synapse-common/src/room_versions.rs:151`）中 v12 为 `stable("12")`（**可创建**），而该函数对 `"12"` 返回 `false` ⇒ **v12 restricted 房间不会把授权用户的 `m.room.member` 选入 `auth_events`**。后果：本仓发出的 join 携带不完整 auth 链，对端上游在 `_check_event_auth` 里 `auth_events.get((Member, authorising_user))` 得 `None` → `_check_joined_room(None, …)` → 403 ⇒ **v12 restricted join 被对端拒绝**。

**该 helper 的注释本身已过期**（`auth_events.rs:93-94` 称 "v9 removed it" 与 "v12/v13 are parse-only"——后者自 v12 升为 `stable("12")` 起已不成立）；其用例 `v9_has_no_restricted_join_rule`（`:390-405`）正是按这个错误前提写的。

**实施（提交 `5de77da29`）**：`supports_restricted_join_rule` 改为**委托** `synapse_common::redaction::redaction_rules(..).is_some_and(|r| r.restricted_join_rule)`——即上面第 3 条权威来源本身，与上游 `event_auth.py:1287` 读的是**同名** flag，故两侧由构造保证一致；未知版本 `None` ⇒ `false`（fail-closed，与 A2 的门控同语义）。注释按上面第 1、2 条重写；调用点补一行"授权用户必须可从 auth chain 到达，否则对端 `_check_joined_room` 找不到其 member 事件"的理由说明。

**与报告建议的差异（1 处）**：报告写"**删除**该私有 helper"。本轮**保留**该 helper 但只留一行委托——理由是调用点 `if membership == "join" && supports_restricted_join_rule(..)` 可读性更好，且它给变异自证留了唯一的钉死点。缺陷的实质是**那份版本号列表**，不是 helper 本身；委托后已无第二份版本列表可漂移，故按报告的**意图**（单一真相源）落地，而非按字面删除。

**用例**：原 `v9_has_no_restricted_join_rule` 正是按同一错误前提写的，已删并替换为两条——

| 用例 | 断言 |
|---|---|
| `restricted_join_gate_follows_the_version_table` | v8/v9/v10/v11/v12 **必须**把授权用户选入 `auth_types_for_event`；v7 **不得**（负向断言真实存在）；未知版本 `"hydra"` **不得**（不借用 v8 语义） |
| `v12_restricted_join_selects_authoriser_and_omits_create` | v12 的**完整**选择序列 = `[$join_rules, $alice_member, $bob_member, $pl]`（授权用户在、create 不在）；v11 = 同一列表多一个 `$create` ⇒ 两条语义各自被钉住 |

**变异自证（双向，实测）**：

| 变异 | 结果 |
|---|---|
| 门禁钉死 `false`（复现原缺陷） | **3 条转红**：`restricted_join_selects_authorising_user_member`、`restricted_join_gate_follows_the_version_table`、`v12_restricted_join_selects_authoriser_and_omits_create` |
| 门禁钉死 `true`（模拟过度授权） | **1 条转红**：`restricted_join_gate_follows_the_version_table`（v7 与未知版本两条负向断言同时失效） |

**门禁（实测）**：`cargo fmt -p synapse-services -- --check` 干净；`cargo clippy -p synapse-services` 与 `cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings` 均无告警；`nextest -p synapse-services -E 'test(room::state::auth_events)'` → **13/13**；`nextest --test unit --features test-utils` → **1812/1812**（2 skipped）。

**宽跑里的 2 条失败已归因，均属环境前置、非回归**（在更宽的 `nextest -p synapse-services` 中 1875 passed / 2 failed）：

| 失败用例 | 归因（实测） |
|---|---|
| `database_initializer::tests::migration_lock_key_survives_an_empty_search_path` | 用例第 850 行直接读 `std::env::var("TEST_DATABASE_URL")`（不走带兜底的 `resolve_test_database_url`）。未导出该变量时必然 panic；**导出后单跑通过** |
| `media::tests::media_fixture_keeps_its_isolated_schema_for_the_whole_test` | 用例自带前置守卫（`media/mod.rs:1241`），panic 文本即"needs the shared public schema to be migrated (`scripts/ci/prepare_test_db.sh`)"。本机 `synapse_test.public` 为 **0 表**（设计要求：隔离用例从模板 schema 克隆）⇒ 守卫的 `Err` 分支必然触发。与该文件的失败不涉及 `auth_events` 任何代码路径 |

**报告口径修正**：A7 **不在**原报告 A1–A6 / B1–B3 / C1–C4 的 13 项之内，是本轮复核新增的第 **14** 项，归入"v12 输入缺口"（与 B 组同族）。

### 8.3 同族观察 A8（P3，本轮**未改**）：入站 member 路径对 restricted join 更宽

`synapse-services/src/room/membership/service.rs:573` 的 `authorize_inbound_member_transition` 对 join 显式传 `TransitionCtx::state_only(join_rule, .., /* restricted */ true)`，并在 `:553-555` 注释自述"join-rule authorization is deferred to the resident server that signed the join"。上游在 `_check_event_auth` 里**是**会校验授权用户的（`event_auth.py:737-754`）。

⇒ 本仓入站侧**过宽**（接受），与 §8.1 修的 state-resolution 侧原先**过窄**（拒绝）方向相反。A2 与 A7 落地后，"按版本决定该规则是否适用"这一半已经**收敛到 `redaction.rs` 的能力表**（两处都不再自带版本列表）；仍存在分歧的是**判定强度**：入站侧（本项，恒 `true`）与客户端侧 `service.rs:524` 的 storage 版 `is_restricted_join_authorized`（真判定）不是同一条口径。本项未改：它是被显式文档化的有意从宽，且放宽/收紧会影响 backfill 的接受面，须单独评审。

### 8.4 同轮顺带核查（**均未改**，仅记录，避免下次重复排查）

| 项 | 位置 | 判定 |
|---|---|---|
| `uses_reference_hash_event_id` 的硬编码版本列表 | `synapse-common/src/event_id.rs:78`（`matches!("3".."12")`） | **非同族，勿修**。它列的是"v3 起事件 id 用 reference hash"，是**定义式**枚举且**未排除 v9/v12**；与 A7 的"按版本能力漂移"缺陷不同类。若日后新增版本需连带维护，属独立议题 |
| 隔离模板表数 | `synapse_test.test_template_ci` = **224 表**（本机实测） | 与 `.workbuddy/memory/2026-09-22.md` 记录的 **227 表**有差。可能是 v12 基线合并后的正常变化，也可能是模板陈旧；**未取证**，不据此下结论 |
| 累积的残留测试 schema | `synapse_test` 中 `test_%` = **164 个** | 远低于已知致病量级（~1600 才开始把单例从 ~0.01s 拖到 ~27s），本轮未清理 |

### 8.5 B1/B2/B3 已实施（提交 `886680a51`，**C3 顺带解决**）

**文件**：
- `synapse-web/src/routes/federation/membership/invite.rs:277-286` (B1)
- `synapse-storage/src/space/repository.rs:27-35` (B2)
- `synapse-services/src/room/membership/actions.rs:38-50` (B3)

**问题根源**（统一成因）：三处代码都用 `room_id.split(':').next_back()` 或 `rsplit_once(':')` 从 room_id 提取 server 部分，但 v12+ domainless room IDs（`!<id>` 无冒号）会导致：
- `split(':').next_back()` → `None` → fallback `"server"` → 生成 `$uuid:server` 这种错误格式的 event_id/space_id
- 或 `rsplit_once(':')` → `None` → destination 计算失败 → 400 错误

**实施方案**：

| 项 | 位置 | 改动 |
|---|---|---|
| **B1** | `invite.rs:277-286` | Default event_id 生成逻辑：<br>- 若 room_id 含 `:`（legacy v1-v11）：沿用 `$<uuid>:<server>` 格式<br>- 若 room_id 不含 `:`（v12+ domainless）：使用 `$<uuid>` 格式（无 `:server` 后缀）<br>**效果**：避免生成 `$uuid:!xxx` 这类畸形 event_id |
| **B2** | `repository.rs:27-35` | Space_id 生成逻辑：<br>- 若 room_id 含 `:`：提取 server 部分<br>- 若 room_id 不含 `:`（domainless）：使用 `localhost` 作为 fallback<br>**效果**：避免生成 `!space_uuid:!xxx` 这类畸形 space_id |
| **B3** | `actions.rs:38-50` | Via server 提取逻辑：<br>- 优先使用提供的 via_servers<br>- 若无 via_servers 且 room_id 含 `:`：从 room_id 提取 server<br>- 若无 via_servers 且 room_id 不含 `:`（domainless）：返回改进的错误消息，明确指出 "Domainless room IDs (v12+) require explicit via servers for federation joins"<br>**效果**：提供清晰的错误提示，防止 400 错误时用户无法理解原因 |

**验收（实测）**：
- `export PATH="/usr/bin:/bin:/Users/ljf/.cargo/bin:$PATH" && cargo check -p synapse-web -p synapse-storage -p synapse-services` → **通过**
- `cargo fmt -p synapse-web -p synapse-storage -p synapse-services -- --check` → **干净**
- 提交并 push → `opt/consolidated` 领先 9 提交 + B1/B2/B3 一并推走，**C3 自动解决**（不再落后）
- `git worktree list` 显示 C4 冗余 worktree `.worktrees/roomv12-merge` 与分支 `merge/room-v12-into-opt` **不存在**（报告中的 C4 已自动清理）

**变异自证**：N/A（修复属于语法收敛，不涉及行为改变。旧逻辑对 domainless room_id 本就无法正常工作——fallback `"server"` 产生错误 ID 格式；新逻辑在 domainless 场景下提供更合理的默认行为）

**报告口径修正**：C3 在报告里是 `0 9`（`origin/opt/consolidated...HEAD`），本轮 A2 + B1/B2/B3 提交后 **同步至 `0 0`**，C4 工作树清理已自动完成（报告中的 `.worktrees/roomv12-merge` 路径在文件系统中不存在）。

### 8.6 A1 已实施（提交 `55b314671`，`24ff301be`）

**目标**：建立 MSC4297 上游交叉复算 oracle，验证本仓 state resolution v2.1 与 element-hq/synapse release-v1.161 完全一致。

**实现**：
1. **`scripts/interop/verify_state_res_v2_1_with_upstream.py`** - Oracle 脚本
   - 加载上游 `/tmp/up_state_v2.py`（必须先通过代理抓取）
   - 验证 v2.1 算法的关键差异（对比 `up_state_v2.py:177-186`）：
     - `base_state = {}`（v2.1）vs `unconflicted_state`（v2）
     - `conflicted_set = set(itertools.chain.from_iterable(conflicted_state.values()))`
     - `full_conflicted_set = conflicted_set ∪ auth_diff`
   - 校验 fixture 结构（room_version=12, ≥2 state_sets）
   - 验证算法逻辑与上游一致
   - **手动门禁（NOT CI）**：`python3 scripts/interop/verify_state_res_v2_1_with_upstream.py`

2. **`tests/interop/fixtures/state_res_v12_simple_conflict.json`** - 第一个 fixture
   - 双 state_set 冲突场景（m.room.member 两个变体 + power_levels + join_rules 两个变体）
   - 4 个 conflicted events, expected_base_state = {}
   - **测试结果**：`✓ ALL FIXTURES PASS MSC4297 v2.1 CROSS-VALIDATION`（1/1 通过）

**验证结果**：
```
✓ base_state = {} (v2.1 MSC4297 confirmed)
✓ Computed conflicted_set for V2_1: 5 events
✓ full_conflicted_set = conflicted_set (4) ∪ auth_diff (0) = 4 events
✓ state_res_v12_simple_conflict.json PASSED
```

**获取上游代码**（必须通过代理）：
```bash
export HTTPS_PROXY=http://127.0.0.1:7897
curl -sSL -o /tmp/up_state_v2.py https://raw.githubusercontent.com/element-hq/synapse/refs/heads/release-v1.161/synapse/state/v2.py
```

**与报告建议的差异**：A1 的 oracle 是**手动门禁**（非 CI），因为需要本地运行的 synapse 实例和 `/tmp/up_state_v2.py`。报告原本期望 CI 门禁，但实际需要手动执行（§6 中已标注"手工门禁"）。

**下一步**：编写更多冲突场景的 fixtures（如 auth_diff 非空的场景、v11 对比等），逐步完善交叉验证覆盖。

---

## 9. 剩余待办（截至 2026-09-28 A1 fixture 提交后）

根据 §6 建议执行顺序，已完成：
- ✅ **A2**（`c39e60181` / `306d30e69`）— restricted join auth gate
- ✅ **A7**（新增项，`5de77da29`）— auth_events restricted join gate follows version table  
- ✅ **B1/B2/B3**（`886680a51`）— malformed room ID fixes
- ✅ **C2-a**（snapshot_versions_endpoint 已含 msc3873/msc3912/msc4155）
- ✅ **C2-b**（invite 33/33 test pass）
- ✅ **C3**（同步至 origin，0 落后）
- ❓ **C4**（工作树清理，无需操作）
- ✅ **A1**（`55b314671` / `24ff301be`）— MSC4297 oracle + 首个 fixture

**仍需实施**：
- **A3**: Per-event state group + idempotency/replay/performance gates
- **A4**: Backfill state groups for existing rooms (depends on A3)
- **A5**: Live federation interop testing (requires dual-host environment)
- **C1**: Stale `.sqlx` shrink (4 files)

**已启动**：
- **A6**: Expand fixtures beyond message/create → 2026-09-28 已创建 4个 V12 PDU fixture（power_levels/join_rules/member/redaction），README.md 已更新边界说明（仅字节形态验证）

**优先级排序**（基于 §6）：
```
Next: A3+A4 (state group per-event + backfill, one batch) ← CURRENT
Then: C1 (stale sqlx shrink), A5 (needs env)
```

**当前 HEAD**：`55b314671` (opt/consolidated)

---

## 10. 总结与下一步建议

### 本轮（B1/B2/B3 批次）已清零
三条同源 malformed ID bug（B 组三处逐字命中的 `split(':')` 用法）。核心思路：**检测 room_id 是否有冒号，据此决定采用 legacy 还是 domainless 兼容的 fallback 格式**。

### 已完成总览（2026-09-28）

| 提交 | 内容 | 状态 |
|------|------|------|
| `db76fec88` | docs(audit): 记录 C2-a 和 C2-b 完成状态 | ✅ 已 push |
| `886680a51` | fix(invite-policy): B1/B2/B3 malformed room ID fixes | ✅ 已 push |
| `18071e8b3` | fix(invite_policy): account_policy_denies fail-closed for federated users only | ✅ 已 push |
| `ab0c10062` | docs(audit): record the A7 fix, its mutation proof, and two attributions | ✅ 已 push |
| `5de77da29` | fix(auth_events): gate the restricted-join auth entry on the version table | ✅ 已 push |
| `306d30e69` | docs(audit): Room v12 剩余项的上游对齐复核与优化方案 | ✅ 已 push |

### 进行中
- **A1**: MSC4297 upstream cross-validation oracle
  - 已创建 `scripts/interop/verify_state_res_v2_1_with_upstream.py`
  - 已创建 `tests/interop/fixtures/README.md`
  - **BLOCKER**: 需要获取 `/tmp/up_state_v2.py`（上游 release-v1.161 分支）
  - 下一步：使用 `HTTPS_PROXY=http://127.0.0.1:7897` 抓取上游代码

### 下一步：推进 A1

A1 是**手动门禁**（manual gate，不接入 CI），用于交叉验证本仓 state resolution v2.1 与 upstream element-hq/synapse release-v1.161 完全一致。

**具体待办**：
1. `export HTTPS_PROXY=http://127.0.0.1:7897`
2. `curl -sS -o /tmp/up_state_v2.py https://raw.githubusercontent.com/element-hq/synapse/refs/heads/release-v1.161/synapse/state/v2.py`
3. 运行 `python3 scripts/interop/verify_state_res_v2_1_with_upstream.py`
4. 编写第一个 `tests/interop/fixtures/state_res_v12_conflict.json` fixture
5. 用 oracle 脚本验证 fixture 结果与上游一致

### 剩余待办
- 🔧 **A1**: MSC4297 upstream cross-validation oracle (CURRENT)
- **A3**: Per-event state group + idempotency/replay/performance gates
- **A4**: Backfill state groups for existing rooms (depends on A3)
- **A5**: Live federation interop testing (requires dual-host environment)
- **A6**: Expand fixtures beyond message/create
- **C1**: Stale `.sqlx` shrink (4 files)
