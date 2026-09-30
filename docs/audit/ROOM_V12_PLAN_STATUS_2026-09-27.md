# Room v12 计划完成情况同步（2026-09-27）

- **被同步文档**：`docs/audit/ROOM_V12_COMPLETION_PLAN_2026-09-27.md`（计划基线 `3856f2961`）
- **工作树 / HEAD**：`.worktrees/room-v12` @ `81b80091d`
- **核验方式**：`git log 3856f2961..HEAD` 逐提交 + 对每个工作项 grep/读码实测；每条给 `路径:行号`
- **本轮性质**：**只读核验 + 状态文档**。未改代码、未联网。

> 结论先行：**24 个工作项里完成 3 项、部分完成 2 项**（A-1、B-1、B-2、C-1 完成；B2a 的两段语法收敛属 C-3 的前半、G-2 的联邦声明断言已补）。
> 计划 §3.3 的关键路径 `A-1 → A-3 → C-1 → C-2 → …` **在 A-3 处断档**：C-1 已先行完成，但 A-3（v12 fixture）未做。

---

## 1. 状态总表（计划 §3.2 的 24 项）

| 项 | 目标（摘要） | 状态 | 实测证据 |
|---|---|---|---|
| **A-1** | 修复已红的 `/capabilities` 快照 | ✅ **完成** | `94d72a29f`；`integration__api_route_snapshots_tests__capabilities_v3.snap:33-34` 含 `"12": "stable"`、`:44` `"default": "12"` |
| **A-2** | 冻结 Q1–Q8 产品决策（写进 §5 或 ADR） | ✅ **完成** | `6cb305409`（Q1=a/Q2=a/Q5=b/Q6=b 落档）+ 本文件 **§3**（Q6 改选 (ii)、Q7 修订）与 **§5**（逐决策落点表）；Q1/Q2/Q4/Q5 已落进代码（`7489b247f` / `c83e3faf9`）⇒ 不再是"建议（供讨论，不是结论）" |
| **A-3** | v12 一致性 fixture 与 oracle 骨架 | ✅ **完成**（`20789c476`） | 新增 v12 **message** fixture（`local_pdu_v12.json`，domainless room id）与 v12 **create** fixture（`local_pdu_v12_create.json`：无 `room_id`/`event_id` + `derived_room_id`）；`u13` 逐字节复核循环纳入 `"12"`；oracle 增加上游 v12 复算：`_check_create` 接受并由事件 ID 推出同一 `!`+43 room id、**带 `room_id` 的 create 被拒**、`auth_events` 含 create 被拒（MSC4307）。两条新检查均有变红自证，详见 §4.10 |
| **B-1** | 建立入站事件鉴权入口（单一实现） | ✅ **完成** | `synapse-federation/src/event_auth/rules.rs`（282 行，`check_inbound_event_auth` / `InboundEventAuth` / `ResolvedAuthEvent`）；入口壳与版本分派齐备 |
| **B-2** | 规则 3.5：`auth_events` 同房校验 | ✅ **完成（已接线）** | `ce4078969`；**生产调用点** `synapse-web/src/routes/federation/transaction.rs:378`（解析 auth_events → `check_inbound_event_auth`，失败即 reject + `security_audit` 日志）；`rules.rs:152` `enforces_auth_events_room_rule` 只对 v12+ 生效；单测含"跨房拒绝/同房通过/无法解析拒绝/pre-v12 不定义该规则" |
| **C-1** | create 事件身份 finalize（G-08） | ✅ **完成** | `7d79982c9`：`write_creation_event` 改走 `create_event_with_pdu`（会 finalize），占位 ID 仅用于 v1/v2；验收测试 `tests/integration/room_service_tests_migrated.rs:5175-5196`（v11 create id 长 44、无 `:`、等于对端复算值、图已重指向 final id） |
| **C-2** | room_id 推导（`$`→`!`）与创建流程重排 | ✅ **完成**（`e2b8266b3`） | 新增唯一 helper `room_id::room_id_from_create_event_id`；`create_room` 先定稿 create → 派生 room_id → 写 rooms 行；验收测试 `test_create_room_v12_room_id_is_the_create_event_id`。**注意：D-6 是它的硬前置**（见下） |
| **C-3** | 无域名 room ID 语法收敛（G-17..G-23、G-20） | ✅ **完成**（`6036c4cb8`/`b089e0323`/`16ee8208f`） | ✅ 语法：`synapse-common/src/room_id.rs`（单实现）+ 两个校验器委托；6 处联邦守卫改用 `is_well_formed_room_id`；`room_id.contains(':')` 生产代码 **0** 处（本轮复测）<br>✅ 本地性（G-21）：`b089e0323` `MembershipService::room_locality`<br>✅ DB CHECK 放宽为两形态（`16ee8208f`）+ 指纹 `16d86ee4035cd351`（顺带修掉 HEAD 上的既有红项）+ 真 DB 契约用例<br>⚠️ **本项验收判据之外的已知残留**（不是 C-3 的五条判据之一，故不影响本项 ✅）：`invite.rs:277` 的**兜底** event id 用 `split(':').next_back()`、`space/repository.rs:30` 用同一写法从父房间 id 取 server、`actions.rs:41` 从 room id 取 join 目的地。其中前两处在**父房间是 v12（domainless）输入**时会拼出 `$uuid:!xxx` / `!space_uuid:!xxx` 这类畸形 id（属于"合成房间"这条独立决策 Q3 的落点，见 §5；`actions.rs` 则本就无法从 domainless id 取目的地，需 `via`）。 |
| **C-4** | 创建侧不写 `predecessor.event_id`；升级顺序反转 | ✅ **完成**（`10aecb7d3`） | v12+ 先建新房（派生 id）再 tombstone，`predecessor` 只含 `room_id`；v1–v11 保持原顺序与 `event_id`；新增 v11→v12 验收测试 |
| **C-5** | `CreateRoomConfig.room_id` 逃逸口处置 | ✅ **完成**（`7489b247f`，随 G-1） | 字段 + `create_room` 预分配分支 + below-v12 升级分支全部删除；`create_room` 显式断言"可创建版本必须由 create 事件派生 id"。合成房间（server notice / space）**不用**该字段，故不受影响（Q3 仍另计） |
| **D-1** | 规则 1.2：v12 create 带 `room_id` 则拒绝（无 `room_id` 时推导房间身份） | ✅ **完成**（`227a7228d`） | `validate_inbound_transaction_pdu` 接收 `room_version`/`event_id`：v12+ create 带 `room_id` 即拒、无则用 `room_id_from_create_event_id` 推导；5 单测 + 变异自证；联邦事务集成 14/14 |
| **D-2** | 规则 2：room_id 必须是已接受 create 事件 ID | ✅ **完成**（`c073c9625`） | v12+ 在 B-1 接缝强制：期望值由**真实 create 事件 id** 经 `room_id_from_create_event_id` 反推（非字符串拼装），故非参照哈希的 create id 也被拒；`create_event_id` 未知 ⇒ **fail-closed 拒绝**；新增 `MessagingService::get_room_create_event_id`（一次定点 state 查询，不用会省略 event_id 的投影 PDU）。顺带把接缝移出 `!auth_events.is_empty()` 守卫——此前空 `auth_events` 会**跳过全部** v12 规则 |
| **D-3** | 规则 2.5 / MSC4307 的 v12 收敛 | ✅ **完成** | 与 B-2 同一实现（`event_auth::rules` 的规则 3.5，v12+ 强制）；版本范围由 `pre_v12_versions_do_not_define_rule_3_5` 与 `a_numeric_version_above_twelve_inherits_the_v12_rules` 显式覆盖 |
| **D-4** | 本地 `auth_events` 不再包含 create（G-28） | ✅ **完成**（`b7cf472b4`） | `auth_types_for_event` 改为 `if !room_version_at_least(room_version, 12)` 才加 create；3 个新用例 + 变异自证（反转阈值 → 5 红） |
| **D-5** | 裁定 auth chain / auth difference 是否含 create | ✅ **完成**（`e933e362c`） | 结论（由本仓自身事实推出，无需引规范原文）：auth chain 是 `auth_events` 的**传递闭包**，而 D-4 已把 create 移出 v12 的 `auth_events` ⇒ **v12+ 闭包不可能含 create**。`is_auth_chain_member` 改为按版本分派；版本从调用方**已持有**的 state 记录中读取（`room_version_from_state`，零额外查询） |
| **D-6** | 出站 create PDU 省略 `room_id`（G-10） | ✅ **完成**（`e2b8266b3`） | `build_pdu` 对 v12+ create 不写 `room_id`；**它是 C-2 的硬前置**（见 §2.5） |
| **E-1** | `additional_creators` 校验（规则 1.4） | ✅ **完成**（`a659f9f7d`） | v12+ create 的 `additional_creators` 必须为合法 user ID 数组；user-id 语法抽为唯一实现 `validation::is_well_formed_user_id` 并让 `Validator` 委托；7 用例 + 变异自证 |
| **E-2** | 创建者集合 + 无限 PL（G-32/33/35） | ✅ **完成**（`6f0c1735a`） | `room_creators_and_version` 一次读 create 事件返回（集合, 版本）；`resolve_room_creators`；v12+ 创建者返回 `CREATOR_POWER_LEVEL = i64::MAX` 且**排在读 PL 之前**（不可降权）；踢/封保护改用集合；版本未知时 fail-closed 不授予无限 |
| **E-3** | 规则 10.4：PL 的 `users` 不得含创建者 | ✅ **完成**（`6f0c1735a` + `fcc57e0f2` + 接线修复 `ab59f4286`） | 客户端 `verify_power_levels_change` 拒绝；**修复**：规范路径是 `PUT /state/m.room.power_levels`（`ensure_room_state_write_access`），原先只接了非规范的 `/send` 路径 ⇒ 规则实际未生效，现已接上（同时补上原本缺失的升降权/同级检查）；入站经 B-1 接缝，读创建者失败 **fail-closed** |
| **F-1** | 状态决议接线决策 + 接线 | ✅ **完成**（`424aaec54` + `62d98ac49`） | 决策：v2.1 实现在 `resolve_state_v2`（见 F-2）+ 查清/接线冲突状态路径。**查清**（§4.6：本仓无状态决议路径，原为 `DISTINCT ON … origin_server_ts DESC` 纯时间戳 LWW）。**层次由任务方拍板 `(A) 入站写入时`**（2026-09-28 第 12 轮）。**读半边** `424aaec54`（§4.9）：resolver 结果统一可消费 + 当前 state 以 `state_groups` 记录为准。**写半边** `62d98ac49`（§4.11）：`StateRecord` 在两个状态写入接缝后维护记录 —— 分叉时逐 extremity 求事件处 state 并用 `resolve_state_for_version_with_rules` 解析回写；有记录时前向拷贝完整状态；无分叉无记录时行为不变。含**服务接缝端到端**用例与变红自证 |
| **F-2** | v2.1 三处修改 | ✅ **完成**（五片：`a5d32166c`/`45d48b69b`/`0e207f202`/`de9270a02`/`70e6ffd38`） | Modification 2（subgraph）+ 3（full conflicted set）+ auth difference 修正 + Modification 1（`iterative_auth_checks`，起始 map 为参数）+ 解析器重组（空起始重放 → 叠加 unconflicted）+ 忠实排序（reverse topological power ordering / mainline ordering）+ **真实 `_check_event_auth`**（`state_map_auth`，复用 membership_transition/room_creator/E 组结论）。已知简化：restricted join 在本谓词下**欠授权**（需 allow rooms 的 state）；上游 implementer's guide **无机器可读向量** ⇒ 向量来自 MSC 原文的 Problem A/B 场景 |
| **F-3** | v1–v11 兼容边界 | ✅ **完成**（`9836c3be7`） | `resolve_state_for_version`：**v12+ 用空起始 map（v2.1）**、**v1–v11 用 unconflicted 起始（v2）**；边界用例在**同一冲突/同一谓词**下断言 v11 否决、v12 放行；另有 `"1"/"11"/"12"/"13"/不可解析标识` 的分派范围用例 |
| **G-1** | 能力表收敛：仅 v12 可创建 | ✅ **完成**（`7489b247f`） | 新增 `RoomVersionCapability::stable_no_create`：v1–v11 可 join/parse/federate、不可创建；`resolve_room_version` 对 v1–v11 返回 `None`；`/capabilities.available` 仅 `"12"`（快照已审阅更新）；联邦 join 走存储层 `room_storage.create_room`，**不受影响** |
| **G-2** | 连带面清单（含联邦 `m.room_versions` 补测） | ✅ **完成**（`8687d8335`） | 补测后暴露**真实协议缺口**：联邦 `m.room_versions` 发的是扁平 `{version:{"status":…}}` + 手工插入 `default`，**不符规范形状** `{default, available:{v:status}}`；已修正，且 `available` 由 `can_federate` 派生（v1–v11 可联邦但不可创建，故与客户端集合**刻意不同**）；新增 `GET /_matrix/federation/v1` 用例（此前零覆盖） |
| **H-1** | 逐份更正文档（含额外 6 份） | ✅ **完成**（`72072e175`） | 10 份文档标题下加状态行（U-22、❌7、AUDIT_SUMMARY、DB_REVIEW、O1_PHASE1、PROJECT_REMAINING、P2_protocol_contract、synapse‑vs‑synapse comparison、API_COVERAGE、v12‑pdu‑graph‑fields），统一指向本文为唯一现状来源；判据 `grep MSC4239 + v12` 不再把 MSC4239 当 v12 定义（余下命中是本计划的纠错说明与该文件自我更正）；新增行无 markdownlint 违规 |
| **H-2** | Q1–Q7 结论落档 | ✅ **完成**（`c56d9d161`） | 本文 §3/§5 记录 Q1–Q8 结论**及其落点**（代码/提交/doc）；README 文档索引新增房间 v12 计划与状态条目（此前 `docs/audit` 零索引 ⇒ 不可发现） |

**计数（2026-09-28，F-1 完成后）**：§1 表中**全部工作项均为 ✅**（含此前被漏计的 **A-3**、一并更正的 **A-2** 行，以及本轮的 **F-1** 写半边）｜🟡 0｜❌ 0。

> ⚠️ **计数口径说明（2026-09-28）**：原"完成 23（… 实为 24 项中的 23 项）"一行**本身自相矛盾** ——
> 它列了 24 个名字却说 23 项，且**漏掉了 §1 表里的 A-3**；A-2 行也与其它 ✅ 行矛盾。
> A-3 已做完（`20789c476`，§4.10）、A-2 行已更正、F-1 两半均已落地（§4.9 / §4.11）。
> **计划基线 §3.2 的项数（24）与 §1 表的行数并不一致**，因此不以分数表述，只报逐项状态。
>
> **仍未修、与本计划无关的既有红项**（不在 §1 任何一项内）：`tests/unit` 的 12 条、
> `snapshot_versions_endpoint`、`derived_manifest` fixture、invite-policy 的 7 条集成用例，
> 以及本文件 §4.9.4 记录的 SQLx/顺序/死代码门禁债（**已在本轮之前的独立提交中偿还**）。

> **D-2/D-5 完成（2026-09-28）**：`c073c9625`（规则 2）、`e933e362c`（auth chain 不含 create）。
> **仅剩 F-2/F-3**（MSC4297）：落点 `resolve_state_v2` 已定，但 §4.6 已证明本仓**没有**状态决议路径，
> 故 F-2 = 实现 v2.1 算法 **+ 从零决定并搭建冲突状态路径**；其验收判据依赖上游 v2.1 implementer's guide 向量
> （计划标【上游】），需联网核验后才可实施 —— 建议单独一轮、甚至先就"冲突状态路径放在哪一层"再决策一次。
>
> **H-1/H-2 + G-2 完成（2026-09-28）**：`72072e175`（10 份文档更正）、`c56d9d161`（决策落点 + README 索引）、`8687d8335`（联邦 `m.room_versions` 形状修正 + 补测）。
> 剩余：**D-2**（v12 非 create 事件的 room_id→create 反查，需改入站热路径）、**D-5**（auth chain 是否含 create【待核验】）、**F-2/F-3**（按 (ii) 落 `resolve_state_v2`，须先建冲突状态路径——§4.6）。
>
> **G-1 + C-5 + E-3 接线修复完成（2026-09-27）**：`ab59f4286`（E-3 规范路径接线）、`7489b247f`（G-1 能力收敛 + C-5 删逃逸口 + 测试迁移）。
> 剩余：**D-2/D-5**、**F-2/F-3**（按 (ii)，落点 `resolve_state_v2`，但需先建冲突状态路径——见 §4.6）、**H-1/H-2** 文档。
>
> **C-4 完成（2026-09-27）**：`10aecb7d3`。**C-5 与 G-1 强耦合**（见 §4.4），应同批执行。
>
> **E 组完成（2026-09-27）**：`a659f9f7d`（E-1 规则 1.4）、`6f0c1735a`（E-2 无限创建者 + E-3 客户端）、`fcc57e0f2`（E-3 入站 + 唯一创建者实现）。
> 剩余：**C-4/C-5**（升级顺序反转 + 删逃逸口）、**F 组**（Q6b：删 `StateResolutionService`/`resolve_state_v2`，v2.1 在 `resolve_state_with_auth_chain` 演进；注意该函数届时仅剩 bench 调用，去留需一并裁定）、**G-1**（v1–v11 收敛 `can_create` + 约 10 处用例迁移）、**H-1/H-2** 文档。
>
> **批次 2 完成（2026-09-27）**：`227a7228d`（D-1 入站规则 1.2）、`6cb305409`（A-2 决策落档：Q1=a / Q2=a / Q5=b / Q6=b）、`c83e3faf9`（Q5 移除 v13）。
> 剩余：C-4（升级顺序反转）、C-5（删逃逸口 + 合成房间）、E 组（MSC4289）、F 组（MSC4297，按 Q6b 删死实现 + v2.1 向量）、G-1（v1–v11 收敛 can_create）、H 文档。
>
> **批次 1 完成（2026-09-27）**：`16ee8208f`（C-3 DB 放宽 + 指纹 + 契约）、`b7cf472b4`（D-4）、`e2b8266b3`（D-6 + C-2）。
> 关键修正：**计划把 D-6 排在 C-2 之后是错的 —— D-6 是 C-2 的硬前置**（详见 §2.5）。

> `D-3` 之所以算"部分"：它的内容（规则 2.5 与 MSC4307 合并为同一实现）已由 B-2 落地并从 v12 起强制，没有独立待办；但它对 v1–v11 的行为边界**尚未有显式回归向量**，故不记为"完成"。

---

## 2. 未完成任务清单（按依赖层级）

### 第 0 层 —— 无依赖、可立即做

- **A-2**（产品决策 Q1–Q8）：它挡住 C-5（Q2/Q3）、G-1 的范围（Q1）、D-5（规范措辞）、F-1/Q6。**建议先落 Q1/Q2/Q3/Q5/Q6 五条**，其余可延。
- **A-3**（v12 fixture/oracle）：只加测试资产；计划自标【待核验】上游 Synapse 是否可离线复算 v12，若不可则退化为"本仓自洽 + 规范推演"并注明。
- **F-1**（状态决议接线决策）：计划明确要求**单独决策**，且要先查清 `synapse-storage/src/state_groups.rs:361 resolve_state_for_group` 是否才是实际生效路径。

### 第 1 层 —— MSC4291 创建侧（本目标 B2b/B3）

- **C-3 的 DB 部分（硬前置）**：放宽 `ck_rooms_room_id_format` → 重算 `EXPECTED_BASELINE_FINGERPRINT` → 同步 `tests/unit/msc_tests.rs`、`tests/integration/invite_blocklist_tests_migrated.rs` 的 schema 断言（R10）。
- **C-2**（依赖 C-1 ✅、C-3 DB）：新增唯一 helper "create event id → room id"；创建流程重排为"先定稿 create → 算 id → 写 rooms 行"。
- **C-4**（依赖 C-2）：废弃 `predecessor.event_id`、反转升级顺序。
- **C-5**（依赖 A-2 Q2/Q3、C-2）：`CreateRoomConfig.room_id` 处置；合成房间（server notice / space）改走统一建房入口或明确记录例外。

### 第 2 层 —— MSC4291 入站侧 + 出站形态

- **D-1**（依赖 B-1 ✅、C-3）：规则 1.2 —— v12 create 带 `room_id` 即拒；同时放行"create 无 room_id"并推导房间身份。
- **D-2**（依赖 D-1、C-2）：规则 2 —— 反查 room_id→create 事件；注意性能与铁律 2（不得成为第二份"房间存在性"实现）。
- **D-4**（依赖 C-2）：`auth_types_for_event` 按版本排除 `m.room.create`。
- **D-5**（依赖 D-4、F-3）：裁定 auth chain / auth difference 是否含 create（【待核验】）。
- **D-6**（依赖 C-2）：出站 create PDU 省略 `room_id`（`pdu.rs:89` + `state_pdu` + 签名材料）。

### 第 3 层 —— MSC4289 创建者特权

- **E-1**（依赖 B-1、C-2）→ **E-2**（依赖 E-1，风险最高：972 行 `power_levels.rs` 的数值污染）→ **E-3**（依赖 E-2；注意建房时自己写的首个 PL 事件含 `users:{creator:100}`，规则 10.4 生效会**自拒**，需特殊路径）。

### 第 4 层 —— MSC4297 状态决议

- **F-1**（第 0 层已列）→ **F-2**（v2.1 三处修改）→ **F-3**（v1–v11 边界）。风险最高的一组：当前 `resolve_state_v2` 与 `StateResolutionService` **双零调用者**，任何行为改动都缺端到端证据。

### 第 5 层 —— 能力收敛与文档

- **G-1**（依赖 C/D/E 全部完成）：v1–v11 + v13 `can_create: false`；同步守卫、快照、约 10 处非 v12 建房用例。
- **G-2**：随 G-1 收尾（联邦 `m.room_versions` 断言已补，其余连带面待收敛）。
- **H-1 / H-2**：依赖 G-1 / A-2 定稿。

---

## 3. 下一步工作计划（建议顺序）

### 批次 1（本目标续作，单写者，预计 1 个会话）

1. **C-3 DB 放宽**（迁移 + 指纹 + 契约用例同步），并**自证门禁能变红**：真 baseline DB 往返插入无冒号 room id，确认 23514 消失。
2. **C-2**：唯一 helper + 创建流程重排；验收 = 给定 fixture 输入，room_id == create event id 的 `!` 形态，且 `rooms` 行在 create 定稿之后写。
3. **D-4**：`auth_types_for_event` 按版本排除 create（v12 生效、v11 不变），同步 `auth_events.rs:216-229` 与 `creation_graph.rs:126-129` 的断言。
4. 每批收尾跑 R8 四道 + 两档 clippy + `check_fmt_ratchet.sh`。

> **前置提醒**：本工作树当前**有两个写者**（本轮已发生 `git stash` 卷走未提交改动、`stash pop` 冲突污染 4 个文件两次）。批次 1 开工前必须协调为单写者；`migrations/` 更是必须单写者（铁律 9）。

### 批次 2（可并行 / 另开 worktree）

- **WT-2**：**D-1 + D-6**（入站规则 1.2 与出站 PDU 形态，依赖 C-2 但可先写壳与测试）。
- **WT-4**：**A-2 决策落档 + G-1** 的**前置准备**（能力表收敛的测试迁移清单、快照差异盘点），真正改 `room_versions.rs` 要等批次 3。

### 批次 3（依赖完成后再动）

- **E 组**（MSC4289）→ 再 **F 组**（MSC4297，先 F-1 决策）→ 最后 **G-1**（收敛 `can_create`）→ **H-1/H-2** 文档收口。

### 不建议现在做的

- **G-1 收敛**：会得到"只声明 v12、但 v12 仍不符规范 + 测试全红"的更差状态（计划 §4.1 风险 6）。
- **F-2**：在 F-1 未裁定接线方式前动手，等于给一个零调用者的函数加行为，无法验证。
- **把 A-1 之外的快照再动**：G-1 会再改一次，避免二次接受。

---

## 4. 核验用的关键命令（供下一轮复算）

```bash
cd .worktrees/room-v12
# 计划基线以来的提交
git log --oneline 3856f2961..HEAD

# 完成项的接地证明
grep -n "check_inbound_event_auth" synapse-web/src/routes/federation/transaction.rs   # B-2 接线
sed -n 5175,5196p tests/integration/room_service_tests_migrated.rs                     # C-1 验收
grep -n "12" tests/integration/snapshots/*capabilities_v3.snap                        # A-1

# 未完成项的接地证明
sed -n 4710,4712p migrations/00000000_unified_schema_v12.sql                           # C-3 DB 未放宽
grep -n "EXPECTED_BASELINE_FINGERPRINT: &str" tests/unit/test_isolation_unification_tests.rs
grep -rn "additional_creators" --include=*.rs . | wc -l                                # E-1 = 0
grep -rn "conflicted state subgraph" --include=*.rs . | wc -l                          # F-2 = 0
sed -n 92,116p synapse-common/src/room_versions.rs                                     # G-1 未收敛
sed -n 108,116p synapse-services/src/room/state/auth_events.rs                         # D-4 未改
sed -n 85,92p synapse-common/src/pdu.rs                                                # D-6 未改
```

---

## 2.5 批次 1 的实现发现：D-6 是 C-2 的硬前置（计划排序有误）

计划 §3.2 把 **D-6**（出站 create PDU 省略 `room_id`）放在阶段 D、排在 **C-2**（room_id 推导）之后。
实测证明这个顺序反了：**不先做 D-6，C-2 在物理上不可能成立**。

机制（`synapse-common/src/pdu.rs` + `event_id.rs`）：

1. `finalize_local_pdu` 先算 content hash，再把它写进 `hashes`，**然后**才算 event id；
2. 红action 白名单**保留 `hashes`**（`redaction.rs` 的 `allowed` 列表）；
3. content hash 是对**未红action的整份 PDU** 取的，其中**包含 `room_id`**。

⇒ 只要 `build_pdu` 还在 v12 create 上写 `room_id`，event id 就会**通过 `hashes` 间接依赖 `room_id`**，
而 MSC4291 又要求 `room_id` 由 event id 推导 —— 循环定义。

诊断过程的关键证据（同一函数、同一 `parts`）：

```
direct=Ok("$abBwO9...")        ← compute_event_id(build_pdu(&parts))
via_finalize=$honiPdN...       ← finalize_local_pdu(&parts).event_id
```

两者不等；而两侧 PDU **逐字段只差 `room_id`**。单独测 `compute_event_id`（不含 `hashes`）会得到
"room_id 无关"的结论 —— 这正是本项一开始被误判为"派生失败"的原因：**必须走完整 finalize 路径才能
观察到该泄漏**。回归测试因此固定在 `synapse-federation/src/event_finalize.rs`（走完整路径），
而不是 `compute_event_id` 的纯函数单测。

**因此 §3.2 的阶段顺序应修正为**：`C-3(DB) → D-6 → C-2 → D-4 → …`；D-6 不应留在阶段 D。

---

## 3. A-2 决策落档（2026-09-27，任务方裁定）

> 计划 §5 的 Q1–Q8 以本表为**结论**（原文档的"建议（供讨论）"至此作废）。

| # | 决策 | 影响与后续工作项 |
|---|---|---|
| **Q1** | **(a) 仅禁止创建**：v1–v11 的 `can_create = false`，但 `can_join` / `can_parse` / `can_federate` 保持 | G-1 只改 `can_create` 与其守卫/快照/约 10 处建房用例；本仓仍可加入现存 v11 联邦房间（互操作，非兼容层）。`ck_rooms_room_version_valid` 的存在理由保持 |
| **Q2** | **(a) 删除 `CreateRoomConfig.room_id` 逃逸口**；房间升级改为"**先建新房（派生 id）→ 再 tombstone 旧房**" | 需要 C-4（升级顺序反转）+ C-5（删除逃逸口与内部合成房间改造）。`room/service.rs:41/536`、`admin/notification.rs`、`space/repository.rs` |
| **Q3** | 内部合成房间（server notice / space）纳入统一建房入口（随 Q2 一起） | C-5 |
| **Q4** | 接受 `/capabilities.available` 收敛为仅 v12 | G-1 后更新快照 |
| **Q5** | **(b) 从能力表移除 `"13"`** | `room_versions.rs:114` 删 `stable_parse_only("13")`；同步其守卫与 `redaction_rules("13")` 的 fail-closed 断言 |
| **Q6** | **(b) 不接线**：按铁律 1 删除 `StateResolutionService` 与 `resolve_state_v2` 死实现；v2.1 只在 `resolve_state_with_auth_chain` 上演进 | F-1/F-2/F-3 的验收改为"现有裁决路径满足 v2.1 语义 + 测试向量" |
| **Q7** | **不允许**客户端在 `creation_content` 传 `additional_creators`（服务端/应用服务专属） | E-1 写入侧边界；handler 黑名单保持并补文档 |
| **Q8** | 接受计划的阶段排序**但按 §2.5 修正**：`C-3(DB) → D-6 → C-2 → D-4 → D-1 → C-4 → C-5 → E → F → G-1 → H` | 见 §2.5 |

---

## 4. E 组（MSC4289）执行分析与下一步

### 4.1 已完成

- **E-1 规则 1.4（`a659f9f7d`）**：`InboundEventAuth` 增加 `event_type`/`content`（文档已约定的扩展点），
  `check_inbound_event_auth` 对 v12+ create 校验 `content.additional_creators`（非数组 / 非字符串 /
  非法 user ID 均拒；缺省合法）。user-id 语法抽为唯一实现
  `synapse_common::validation::is_well_formed_user_id`，`Validator::validate_matrix_id` 委托它
  （删掉 `matrix_id_regex`）。7 个新用例 + 变异自证 + 联邦集成 14/14。

### 4.2 E-2 / E-3 的实测消费面（下一轮直接照此执行，不必重新摸排）

- `get_user_power_level` / `get_joined_user_power_level` 的消费点**全部在**
  `synapse-services/src/auth/power_levels.rs`（`:153/:175/:202/:330/:356/:369-370/:426-427/:484-485/:525/:551`），
  外加 `room/membership/service.rs:253/:272` 只有**注释**提到 `resolve_room_creator`。
  ⇒ 改动面被限制在该文件 + 成员服务注释。
- `resolve_room_creator` 的调用点：`power_levels.rs:32`（`get_user_power_level`）、`:405`（踢人保护）、
  `:463`（封禁保护）。
- **现有顺序就是 G-32 的缺陷**：`users[user]` / `users_default` 在**创建者兜底 100 之前**返回，
  所以一条 PL 事件可以把创建者降权到 0。

### 4.3 E-2 的关键设计约束（避免返工）

1. **必须按房间版本门控**：unlimited 只对 **v12+** 生效；v1–v11 保持现有"兜底 100"语义，
   否则会改变既有房间的授权结果（v1–v11 仍可 join/federate，见 Q1(a)）。
2. **创建者判定要排在读取 PL 之前**（v12+），否则 PL 仍能降权 —— 这就是规则 10.4 的另一面。
3. **`i64::MAX` 哨兵**：消费点全是比较（`actor > target`、`actor >= threshold`），未见算术；
   落地时仍需对 `power_levels.rs` 内每处比较做一次确认。现有测试断言的 `== 100`
   （`:711`、`:743`）针对的应是 v10/v11 房间，需确认其房间版本后再决定是否改为版本条件断言。
4. **创建者集合只读一次 create 事件**：`resolve_room_creators` 与 `get_room_version` 都读
   `m.room.create`；应合并为一个私有 helper（返回 `(creators, version)`）以避免每次授权两次状态读取。
   集合 = `content.creator`（v1–v10）∪ 事件 `sender`（v11+ 的创建者）∪ `additional_creators`。
5. **E-3（规则 10.4）**：`verify_power_levels_change` 在 v12+ 拒绝 `users` 含任一创建者。
   ⚠️ 建房时自己写的首个 PL 事件含 `users: {creator: 100}`（`create.rs` 的 power_levels 构造），
   若该规则在创建序列内也生效会**自拒** —— 规则 10.4 只应作用于**入站** PL（经 `event_auth::rules`），
   或创建序列显式走特殊路径。

---

## 4.4 C-5 与 G-1 的耦合（实测：必须同批执行）

**C-5 不能单独做**。`CreateRoomConfig.room_id` 目前仍被 **below-v12 升级分支**需要：
v1–v11 的 `predecessor.event_id` 要求 tombstone 先写、而 tombstone 的 `replacement_room`
又要求新房 id 已知 ⇒ 必须预分配。删掉逃逸口会直接打断 v9→v10 这类升级路径
（`test_upgrade_room_predecessor_names_the_persisted_tombstone` 就是它的守卫）。

**G-1 让 C-5 变简单**：`can_create = false` for v1–v11 之后，`resolve_room_version(Some("10"))`
返回 `None` ⇒ below-v12 的建房/升级分支**不可达**，按铁律 1 随 `config.room_id` 一起删除，
`upgrade_room` 只剩 v12 分支（C-4 已就位）。

**同批必须处理的连带面（实测清单）**：

| 连带项 | 位置 | 处理 |
|---|---|---|
| 能力表 | `synapse-common/src/room_versions.rs:118-119` | v1–v11 改用"可 join/parse/federate、不可 create"的构造（`stable_parse_only` 已随 Q5 删除，需新增语义正确的构造或直接内联标志位） |
| 守卫 | 同文件 `:235-300` | `resolve_room_version(Some("1".."11")) == None`；`available` 只含 v12；断言 `available.len() == 1` |
| 快照 | `tests/integration/snapshots/*capabilities_v3.snap` | `available` 收敛为仅 `"12"`（并复核 `unstable_features` 漂移，那是并发会话的面） |
| 建房用例（G-48） | `tests/integration/room_service_tests_migrated.rs:279/751/807/866/4989/5061/5098`、`federation_existence_leak_tests.rs:567/680/735/863` | 迁移到 v12，或改为"断言非 12 不可创建" |
| v1/v9/v10/v11 升级用例 | `room_service_tests_migrated.rs` 的 `test_upgrade_room_*`、`test_create_room_keeps_the_server_assigned_create_id_for_v1_rooms` | G-1 后这些路径不可达 ⇒ 随 below-v12 分支一起删除（保留 v11→v12 与 v12→v12） |
| below-v12 升级分支 | `synapse-services/src/room/service.rs` 的 `else` 分支 + `predecessor.event_id` | 删除（C-5） |
| 逃逸口与合成房间 | `CreateRoomConfig.room_id`、`admin/notification.rs`、`space/repository.rs` | 删除字段；合成房间若需保留则必须写明谁保证 id 形态，或改走统一建房入口 |

> ⚠️ 该批同改 `migrations/` 之外的**大量测试**属预期；`migrations/` 本批不动，故 R10 指纹不涉及。

---

## 4.5 F-1 决策与实测的矛盾（执行前必须澄清）

A-2 的 **Q6(b)** 裁定："不接线，按铁律 1 删除死实现，只在 `resolve_state_with_auth_chain`
上演进 v2.1"。但本轮实测与该前提不符：

| 事实 | 证据 |
|---|---|
| `StateResolutionService` 全仓（含 bench/tests）**零引用** | `grep -rn "StateResolutionService\|ResolutionResult\|events_to_data\|StateResolutionError" --include=*.rs .` 除自身文件外 0 命中 |
| `resolve_state_v2` **零引用** | `grep -rn "resolve_state_v2"` 除自身文件外 0 命中 |
| **`resolve_state_with_auth_chain` 也零生产引用** | 唯一调用者是 `state_resolution.rs:83`（即被删的 service）；删除 service 后仅 `benches/performance_federation_benchmarks.rs:41/64` 还在调 |
| 计划 §F-2 把 v2.1 的落点写成 `resolve_state_v2`（`:367-508`） | 计划原文；与 Q6(b) 的"删 `resolve_state_v2`"**直接冲突** |
| `synapse-storage/src/state_groups.rs:361 resolve_state_for_group` 不是 v2 实现 | 它是沿 DAG 边递归取某 state_group 的**存储层** helper，且只被测试调用 |

**结论**：本仓**没有任何一条生产路径**在做状态决议；`resolve_state_with_auth_chain` 与
`resolve_state_v2` 都是未接线的实现。因此"在 `resolve_state_with_auth_chain` 上演进 v2.1"
目前等于**给一个只被 bench 调用的函数加行为**，无法端到端验证（正是计划 §4.1 风险 1 描述的模式）。

**建议（待任务方确认，未执行删除）**：二选一
- (i) 维持 Q6(b) 字面：删 `StateResolutionService` + `resolve_state_v2`，`resolve_state_with_auth_chain`
  保留为 v2.1 落点；则 F-2 的验收只能是"该函数的单测/向量"（无端到端证据），需明确接受这一点；
- (ii) 修正 F-2 落点为 `resolve_state_v2`（计划原文），把它按 v2.1 三处修改补齐并**接线到冲突状态路径**
  （需先查清本仓实际由谁决定冲突 state，可能根本没有这条路径），验收才可能有端到端证据。

（本轮**未**删除任何状态决议代码。）

---

## 4.6 F-1 接线调查结论（已按 (ii) 定方向；实测本仓**没有**状态决议路径）

任务方在 4.5 的二选一中选了 **(ii)：v2.1 实现在 `resolve_state_v2`（计划原文），并查清/接线冲突状态路径**。
本轮把"查清"做完，结论比 4.5 更严重：

| 调查项 | 实测结果 |
|---|---|
| `create_state_group`（写 state group 的唯一入口） | **零生产调用者**（`grep` 在 `synapse-services`/`synapse-web`/`src/` 0 命中）⇒ **state-group 流水线整体未接线** |
| `resolve_state_for_group` | 只被测试调用（`tests/integration/state_groups_storage_tests_migrated.rs:460/495/530`） |
| **当前房间 state 实际怎么来的** | `synapse-storage/src/event/state.rs:46-60`：`SELECT DISTINCT ON (event_type, state_key) … FROM events … ORDER BY event_type, state_key, origin_server_ts DESC` |
| 结论 | 本仓的"状态决议"是**纯 `origin_server_ts` last-write-wins**，既不是 v2 也不是 v2.1；冲突时**不做** auth chain / power ordering / iterative auth checks |

⇒ F-2 的"接线"实际是**从零建一条冲突状态路径**，而不是把已有实现接上一个调用点。这正是计划 §4.1 风险 1 / F-1 提示的最大回归面，
也是计划把 F-1 列为"必须单独决策"的原因。**执行 F-2 前需要先做两件事**：

1. 找到（或新增）**冲突状态**的判定入口：当前 `DISTINCT ON … origin_server_ts DESC` 位于存储层读路径，
   要接 v2.1 必须先在"入站事件写入时"或"读 state 时"分出"存在多个并发分支"的场景（本仓目前连
   `prev_events` 分叉检测都没有用于 state 决策）。
2. 明确 v2.1 的输入面：`resolve_state_v2(events)` 接收的是 `Vec<serde_json::Value>`（PDU 列表），
   需要 `auth_events` 图与 room version；当前调用链上没有任何地方持有这三者。

**本轮未执行删除、也未改状态决议代码**（4.5 的建议 (i) 已被 (ii) 取代：不再删 `resolve_state_v2`）。

---

## 5. A-2 决策（Q1–Q8）的落点 —— H-2 可追溯性

| 决策 | 结论落在哪里（代码/文档） |
|---|---|
| **Q1**（仅禁止创建，join/federate 保持） | 代码：`synapse-common/src/room_versions.rs` 的 `RoomVersionCapability::stable_no_create` 与其上方的 capability↔behaviour 论证注释；本文件 §3 |
| **Q2**（删 `CreateRoomConfig.room_id`；升级改为先建新房再 tombstone） | `7489b247f`（G-1+C-5）；`synapse-services/src/room/service.rs::upgrade_room` 的顺序注释；`create.rs` 的派生不变量断言 |
| **Q3**（内部合成房间纳入统一建房入口） | ⚠️ **按"记录为本地合成房间"这一被允许的分支收口，未改走统一入口**（2026-09-28 复核）。计划 C-5 的验收判据原文允许二选一："内部合成房间改走统一建房入口（**或明确记录为"非 v12 语义的本地合成房间"**）"。现状：`admin/notification.rs:350` 与 `space/repository.rs:30` **不经过** `CreateRoomConfig`（直接拼 id + 直接落库），即它们是**非 v12 语义**的本地合成房间 ⇒ 走的是记录分支。<br>⚠️ **但这条分支有一个未关闭的具体缺口**：两处用 `room_id.split(':').next_back()` 从**父**房间 id 取 server，**父房间是 v12（domainless）时会拼出畸形 id**（`space/repository.rs:30` 的 `!space_uuid:!xxx` 会被 CHECK 拒；`invite.rs:277` 的兜底 event id 同理）。修法很局部（改 `rsplit_once` + 补 v12 父房间用例），但属 Q3 的后续落点，**不在计划 §1 任何一项的验收判据内** |
| **Q4**（接受 `available` 仅列 v12） | `7489b247f`：快照 `integration__api_route_snapshots_tests__capabilities_v3.snap` |
| **Q5**（移除不存在的版本 13） | `c83e3faf9`；`room_versions.rs` 注释 + `redaction.rs` 的 fail-closed 说明 |
| **Q6**（(ii) v2.1 实现在 `resolve_state_v2`，并查清/接线冲突状态路径） | 本文件 §4.5（矛盾记录）与 §4.6（接线调查：本仓无状态决议路径） |
| **Q7**（客户端不得在 `creation_content` 传 `additional_creators`） | E-1（`a659f9f7d`）只做**入站校验**；写入侧边界（handler 黑名单）保持原样 |
| **Q8**（阶段顺序，含 D-6 在 C-2 之前） | 本文件 §2.5 |

---

## 4.7 F-2 可执行性核验（2026-09-28，联网核实上游原文后）

上游原文已取到并逐条核对（[MSC4297 raw](https://raw.githubusercontent.com/matrix-org/matrix-spec-proposals/refs/heads/kegan/msc4297/proposals/4297-state-resolution-v2_1.md)）。三项修改的正确表述是：

1. **Modification 1**：state res **step 2 的 iterative auth checks 从"空 state map"开始**（v2 是从 unconflicted state map 开始），
   依据是 iterative auth checks 定义中"所需 `(event_type, state_key)` 不在 state 中时，改用该事件 `auth_events` 里的对应状态事件"。
   ⚠️ 原文明确："They do **not** modify how conflicted events are sorted nor do they modify the iterative auth checks."
2. **Modification 2**：新增术语 **conflicted state subgraph** —— "从 conflicted state set 中的一个事件出发沿 `auth_events` 边可到达
   同一集合中的另一事件；所有这样的**路径并集（含端点）**"。
3. **Modification 3**：**full conflicted set = conflicted state set ∪ conflicted state subgraph ∪ auth difference**。

### 实测：F-2 在本仓**当前基座上不可执行**（不是"改三处"）

| 事实 | 证据 |
|---|---|
| `resolve_state_v2` **是近似实现，不是忠实的 v2** | `state_resolution.rs:367-515`：只做了 v2 step 1（unconflicted/conflicted 划分）+ "每个冲突键按 (sender power, ts, mainline 序号, event id) 取第一个赢家"；**没有 iterative auth checks**、**没有计算/使用 auth difference**，`mainline` 只作 tiebreak |
| ⇒ Modification 1 **无落点** | "iteration auth checks 从空 map 开始"要求先存在 iterative auth checks；本仓没有 |
| ⇒ Modification 2/3 无消费者 | subgraph 只有在"重放并逐条鉴权"时才有意义；当前没有重放 |
| 上游**无机器可读测试向量** | implementer's guide 为散文（且本次抓取失败）；检索未发现向量仓库。计划把 F-2 验收判据定为"上游 implementer's guide 的向量"，**该判据无法直接落地** |
| 该函数**零调用者** | `grep` 全仓（除自身/单测/bench）0 命中 ⇒ 任何行为改动都无端到端证据（计划 §4.1 风险 1） |

### 因此 F-2 有两个可执行的解释（需任务方选一个，本轮未动代码）

- **(A) 真做**：先补一个**忠实的 v2**（mainline ordering + reverse topological power ordering + iterative auth checks + auth difference），
  再叠加 v2.1 三项修改；测试用**自洽冲突图向量**（按 MSC 的 Problem A/B 场景构造，并在测试里标注"自构造、非上游向量"）。
  规模 = 重写 state resolution 核心；安全关键；仍无端到端证据（零调用者）。F-3 再按版本分派。
- **(B) 按铁律 1 收敛**：F-1 的查清已证明本仓**没有**状态决议路径，`resolve_state_v2` 零调用者且是近似品 ⇒ 删除它，
  并把"MSC4297 未实现"如实写入 v12 能力声明（AGENTS「协议声明纪律」要求 capability 与行为一致）。
  代价：本轮 goal 的 F-2/F-3 以"**不适用**（本仓无状态决议路径）"结案，而不是"实现了 v2.1"。

---

## 4.8 F-2 进展（2026-09-28，按任务方选 (A) 真做）

任务方选择 **(A) 从零实现忠实的 v2 + v2.1**。已落地**第一个可独立验证的切片**
（`a5d32166c`）：**v2.1 的集合选择半边**。

| v2.1 项 | 状态 | 证据 |
|---|---|---|
| **Modification 2** conflicted state subgraph | ✅ 实现（按 MSC 字面：从每个 conflicted 事件沿 `auth_events` 走链、携路径，落到 conflicted 事件时整条路径入集，含端点） | `conflicted_state_subgraph`；2 用例（含"中间的未冲突事件必须入集""单个 conflicted 事件时子图即自身"） |
| **Modification 3** full conflicted set = conflicted ∪ subgraph ∪ auth difference | ✅ 实现（并**驱动** `resolve_state_v2`：先算每个 state set 的 full auth chain → auth difference → full conflicted set，再排序） | `full_conflicted_set`；1 用例 |
| auth difference 定义修正 | ✅ 按规范 `∪C_i − ∩C_i`；**删除**旧实现多插的"differing 事件的 `auth_events`"（那只会加回两链共有的项，使差集偏大） | 1 用例 + 变异自证（恢复旧行为即红） |
| **Modification 1** iterative auth checks 从**空 state map** 开始 | 🟡 **机制已做**（`45d48b69b`）：`iterative_auth_checks` 实现重放机制（含"缺失键从事件自身 `auth_events` 传递性回填、被拒事件不得回填"），**起始 state map 作为参数** ⇒ v2/v2.1 的差异成为调用方决策（F-3 的分派点）；MSC 的 **Problem A** 形态已有用例断言两种起始 map 的不同结果。<br>❌ 仍缺：解析器尚未围绕该重放重组（排序 + `_check_event_auth` 真实实现） | §4.7/§4.8 |

**变异自证**（铁律 8）：子图项退回 v2（不加子图）⇒ 2 红；auth difference 恢复旧写法 ⇒ 1 红；均复原后 27/27。
`synapse-federation --lib` 240/240；fmt 0/0；clippy（all-targets/all-features `-D warnings`）EXIT=0。

### 剩余（F-2 收尾 + F-3）

1. **鉴权接缝**：为 state resolution 提供 `is_authorised(event, state) -> bool`（本仓无规范 auth-rules 引擎；`event_auth::rules` 是为**入站单事件**设计的，不接 state map）。
2. **Modification 1**：iterative auth checks 以空 state map 起步（v2 从 unconflicted 起步）；其"所需键不在 state 时改用该事件 `auth_events` 中的对应状态事件"规则由接缝承担。
3. **重组 `resolve_state_v2`**：mainline ordering + reverse topological power ordering + 上述 iterative checks；当前实现仍是"每键按 (power, ts, mainline 序, event id) 取第一个"，非忠实 v2。
4. **F-3**：按版本分派（v2.1 仅 v12+），并为 v1–v11 保留 v2 回归向量。
5. ⚠️ **接线仍未决**（§4.6）：`resolve_state_v2` 零调用者，故以上全部仍无端到端证据。

### 4.8.1 第二片（`45d48b69b`）

`iterative_auth_checks` 已实现，鉴权谓词（规范里的 `_check_event_auth`）作为**参数注入**（本仓无面向 state map 的
auth-rules 引擎，避免复制一套规则）。4 个新用例 + 变异自证（清空回填栈 ⇒ 3 红）。`synapse-federation --lib` 244/244。

**F-2 剩余**：① 真实 `_check_event_auth`（或把 `event_auth::rules` 扩展成可对 state map 判定）；
② 重组 `resolve_state_v2`：mainline ordering + reverse topological power ordering + 用 full conflicted set 调
`iterative_auth_checks`（v2.1 传空起始 map）；③ **F-3** 版本分派 + v1–v11 的 v2 回归向量。

### 4.8.2 第三片（`0e207f202`）+ F-3（`9836c3be7`）

- **解析器重组**：`resolve_state_v2` 现按算法执行 —— full conflicted set → 排序 → `iterative_auth_checks` **空起始 map** 重放 → 叠加 unconflicted（spec step 5）。签名新增**注入谓词**（本仓无面向 state map 的 auth-rules 引擎）；零调用者，故签名变更无代价。
  两个行为随之改变：赢家改由**重放授权**决定（非排序第一个）；"仅一侧有的键"也走重放（不再直接采纳）。
- **F-3 分派**：`resolve_state_for_version(room_version, …)` —— v12+ 空起始（v2.1）、v1–v11 unconflicted 起始（v2）。边界用例在**同一冲突/同一谓词**下断言 v11 否决、v12 放行；另有标识符范围用例。

**F-2 真正剩余（2 项）**：
1. **真实 `_check_event_auth`**：谓词目前由调用方注入（测试里用"发送者必须是 joined 成员"）。需要面向 state map 的规范 auth rules（`event_auth::rules` 是单事件入站的，不适用）。
2. **忠实排序**：mainline ordering 与 reverse topological power ordering 目前共用同一个比较器（sender power → ts → mainline 位置），是近似。

⚠️ 不变：`resolve_state_for_version` **零调用者** ⇒ 以上均无端到端证据；§4.6 的接线决策仍未拍板。

### 4.8.3 第四片（`de9270a02`）与 F-2 的**唯一**剩余项

**已接线**：auth 组用 `reverse_topological_power_ordering`（Kahn + 规范 tie-break：sender power desc → ts asc → event id asc），
非 auth 组用 `mainline_ordering`（`mainline_depth_of` 取 auth 链中**最深**的 mainline 祖先，无祖先记 0）。
旧 `sort_by_reverse_topological_power` 已零调用者 ⇒ 按铁律 1 **删除**（54 行）。3 个排序用例 + 2 个变异自证。

**F-2 唯一剩余：真实 `_check_event_auth`**。现状是**调用方注入谓词**（测试里用"发送者必须是 joined 成员"）。要补齐需实现
规范的 event auth rules 对 **state map** 求值，范围（按规范 §Authorization rules 归纳）：

1. `power_level` 解析：从 state 里的 `m.room.power_levels` 取 `users` / `users_default` / `events` / `state_default`（含 v12 的 MSC4289 创建者无限 PL —— 可复用 E 组结论）；
2. `m.room.create`：无 auth_events、房间版本合法、v12 的 `additional_creators` 规则（可复用 E-1 的 `is_well_formed_user_id`）；
3. `m.room.member`：join / invite / leave / ban / knock 各自的门槛与状态机规则（**可复用纯函数 `synapse_common::membership_transition::is_legal`** 与 `TransitionCtx`，这是本仓既有的"深模块"接缝）；
4. 其余状态事件：`events[type]` / `state_default` 门槛；`m.room.redaction` 的 redact 门槛；
5. v12 专属：规则 1.2 / 2 / 3.5 / 10.4（**已实现于 `event_auth::rules`，但那是单事件入站形态**，需抽出可对 state map 复用的部分，避免第二份实现 —— 铁律 2）。

⚠️ 该谓词仍**无调用者**（`resolve_state_for_version` 零调用），故即便补齐也仍无端到端证据；§4.6 的接线决策是它生效的前提。

---

## 4.9 F-1 接线：层次拍板 **(A) 入站写入时** + 读半边落地（2026-09-28）

§4.6 列出的三个层次（**入站写入时** / 读 state 时 / 新建 state-group 流水线）中，任务方在第 12 轮
选定 **(A) 入站写入时**：事件落库后判定"该房间是否已分叉"，分叉则用 v2.1 重算受影响键的状态并
**回写**；读 state 以该记录为准。

### 4.9.1 本轮实测的既有事实（为什么这条接线比"接一个调用点"大）

| 事实 | 证据 |
|---|---|
| 本仓**没有**状态决议路径 | §4.6：读 state 是 `DISTINCT ON … origin_server_ts DESC` 纯时间戳 LWW |
| 四张 state-group 表**早已存在且结构完整** | `migrations/00000000_unified_schema_v12.sql:2351-2386`（`state_groups` / `state_group_edges` / `event_to_state_groups` / `state_group_state`）⇒ "回写"**不需要新表、不需要新迁移** |
| 但**没有任何生产代码读写它们** | `grep -rn "state_group" --include=*.rs synapse-services/src synapse-web/src src/` 只命中 `src/storage/mod.rs` 的 re-export；`create_state_group` 零生产调用者 |
| resolver 的结果**不可消费**（接线的硬前置） | `resolve_state_with_start` 对 unconflicted 项写入完整 state event（含 `event_id`），对**重放**项只写入 content ⇒ 重放赢家没有 `event_id`，既无法回写也无法与 unconflicted 项比较 |

### 4.9.2 本切片落地（`424aaec54`，F-1 的**读**半边）

1. **resolver 结果统一可消费**：新增 `EventData::to_state_event_value()`，unconflicted 与重放
   两条路径都返回**完整 state event**。新用例 `every_resolved_entry_identifies_its_winning_event`
   断言"每条结果都有 `event_id`"；原 `replay_decides_the_conflicted_winner_not_the_ordering`
   改为从 `content.join_rule` 读 —— 旧断言读的正是那个缺陷形态（裸 content）。
2. **当前 state 以"已决议记录"为准**：`EventStorage::get_state_events` / `get_state_event`
   先取房间最新的 `state_groups` 行，存在则从 `state_group_state` 返回（同一入口服务"整表"与
   "单键"两种投影，共用一条静态 SQL）；不存在才回落到时间戳推导。
   DB 用例 `test_current_state_prefers_the_resolved_state_record`：同一 key 两个候选，无记录时
   时间戳新者胜；写入记录后**较旧**的候选必须胜出；且记录是**全量**当前态（记录未覆盖的键
   不出现在结果里），不是叠加在时间戳推导之上的补丁。

### 4.9.3 写半边（**已落地**：`62d98ac49`，见 §4.11）

落点在两个**状态事件写入接缝**之后的维护步骤 —— `MessagingService::create_event_with_graph`
（本地 `/send`、入站联邦事务、`backfill`、invite 共用）与联邦加入的 state 批次
（后者走存储层写入并在**一个事务**内提交，不经过前者）：

- 房间 forward extremities > 1（分叉）⇒ 对每个 extremity 求"事件处 state"（沿 `event_edges`
  走 DAG；多父节点处用 `resolve_state_for_version_with_rules` 递归）→ 解析 → 以新 group 回写
  （`create_state_group` + `set_state_entries` + `add_state_group_edges` + `bind_event_to_state_group`）；
- 否则若房间**已有**记录 ⇒ **前向拷贝**完整状态（读路径只服务 group 自身的行，故不能只写增量）；
- 从未分叉、也从未有记录的房间维持时间戳推导（即行为完全不变）。

计划 §F-1 原评"风险：高"（从未在生产路径跑过的算法 + 一条新的关键路径）已由 §4.11 的
端到端用例与变红自证承担；`tx: Some(..)` 的调用方（建房/升级）在事务内不维护，由联邦加入路径
在 commit 后单独维护。

### 4.9.4 同批偿还的既有门禁债（`4cc45d279`，独立提交）

`656a54699`（invite-policy 合并）带进 `invite_blocklist.rs` 却未同步门禁，HEAD 上这些门禁
**本就是红的**（非 room v12 引入）：SQLx 生产动态 275 → 287、测试动态 716 → 718、
`check_ts_order_tiebreak.py` `0 → 4`、clippy `dead code`。按 R11 独立提交修掉：12 处生产字面量
全部宏化（生产动态回到 **275 = 基线**）、4 处 `ORDER BY created_ts DESC` 补 `, user_id ASC`、
删零调用者夹具、删未使用形参；并把一条会随棘轮进展变假的守卫判据
（`dynamic > 1000`，而棘轮终点是 0）改为直接断言扫描面含 workspace crate 路径。
`BASELINE_DYNAMIC_TEST_INFRA` 716 → **717**（两处必要的 test 夹具，带归因）、
`BASELINE_STATIC` 1193 → **1206**、`BASELINE_DYNAMIC` 991 → **992**。

**本批门禁实测**：`check_sqlx_dynamic_ratio.sh` OK（275/717/1206/18）；
`check_sqlx_cache_fresh.sh --compile` OK；`check_ts_order_tiebreak.py` OK（73/31）；
clippy 两档 `-D warnings` EXIT=0；fmt `current=0 baseline=0`；
`synapse-federation --lib` 260/260；`synapse-storage --lib`（event:: + invite_blocklist）130/130；
unit 守卫（literal / ratio / tiebreak）41/41。

⚠️ 与本批无关、**仍未修**的既有红项（与前一版状态文档所列一致）：`tests/unit` 的 12 条、
`snapshot_versions_endpoint`、`derived_manifest` fixture、以及 invite-policy 的 7 条集成用例。

---

## 4.10 A-3：v12 一致性 fixture + 上游 oracle 实测（`20789c476`，2026-09-28）

计划的 A-3 原判风险是"上游 Synapse 的 v12 oracle 是否可离线复算【待核验】，若不可则退化为
'本仓自洽'"。**本轮核验结果：可离线复算，无需退化。**

| 事实 | 证据 |
|---|---|
| 本沙箱的 `matrix-synapse==1.161.0`（`/tmp/peer-synapse`）**认识 room v12** | `KNOWN_ROOM_VERSIONS` 含 `'12'`（与 `11`/`org.matrix.hydra.11` 等并列） |
| 它已实现 MSC4291 | `RoomVersion.msc4291_room_ids_as_hashes`；`synapse/event_auth.py` 的 `_check_create` 规则 1.2：`if "room_id" in event: AuthError(403, "Create event has a room_id")` |
| 它也实现了 MSC4289 / MSC4307 | `msc4289_creator_power_enabled=True`；`federation_base.event_from_pdu_json` 拒绝 `auth_events` 含 create 事件（`400 auth_events must not contain the create event`） |
| ⇒ 另一分支的占位 skeleton（`e6311f5b3`，placeholder hashes、`u13` 仍只验 3/10/11）**本分支不需要** | 本分支的 v12 流水线已落地（C/D 阶段），可以产出**真实** v12 字节并交由上游复算 |

### 4.10.1 落地内容

- **fixture（由真实流水线生成，`u13` 逐字节复核）**
  - `local_pdu_v12.json`：v12 **message** PDU；`room_id` 用 domainless 形态
    （`!` + 43 字符 URL-safe base64，**无 `:server`**）——沿用 legacy 拼写就不是 v12 向量。
  - `local_pdu_v12_create.json`：v12 **create** PDU；PDU 内**无 `room_id`**（D-6）、
    **无 `event_id`**（v3+ 线形态）；envelope 记录 `derived_room_id`。
  - `u13` 的复核循环由 `["3","10","11"]` 扩为 `["3","10","11","12"]`，并新增断言：
    `$` + 43 URL-safe、`derived_room_id == "!" + event_id[1..]`、
    与 `room_id_from_create_event_id` 一致、`is_domainless_room_id` / `is_well_formed_room_id` 为真。
- **oracle（`scripts/interop/verify_pdu_with_upstream_synapse.py`）新增第 4、5 步**，
  判据全部来自**上游自己的实现**：
  - **MSC4291**：上游 `_check_create` 接受我们的 create ⇒ 且 `event_from_pdu_json` 推出的
    `room_id`/`event_id` 与我们的 `derived_room_id`/`event_id` 一致；
    **带 `room_id` 的 create（即使是"正确"的那个 id）必须被上游拒绝**。
  - **MSC4307**：`auth_events` 含 create 事件的 PDU 必须被上游拒绝；同一次运行先断言
    "未改动 PDU 能构建"作**正向对照**，使该拒绝可归因（不是"恰好构建失败"）。
- **实测**：v3/v10/v11 三条旧 fixture 行为**不变**；v12 两条全 PASS；
  `synapse_federation --lib` 无关、`unit` 的 `u13_interop_fixture` 2/2。

### 4.10.2 变红自证（铁律 8）

| 变异 | 结果 |
|---|---|
| 把 `room_id` 加进 v12 create，并**重算 hashes / 事件 ID / 签名**（使第 1–3 步全过） | oracle 仍以 ``FAIL: a v12 m.room.create PDU must not carry `room_id` (MSC4291)`` **退出 1** ⇒ 该检查真的在拦人，不是被上游 hash/ID 检查"顺便"挡住 |
| 篡改 envelope 的 `derived_room_id` | ``FAIL: fixture records derived_room_id=… but `!` + event_id[1:] is …``，退出 1 |
| 把 create 事件 ID 写进 fixture 的 `auth_events` | 第 5 步的**正向对照**先失败，退出 1（正常 fixture 下正向对照通过、构造变体被拒 ⇒ 两侧都被执行到） |

三条实验记进 `tests/interop/fixtures/README.md`。oracle 仍是**手工门禁**（需要
`/tmp/peer-synapse` 这个 venv，CI 无此环境），与 `REMAINING_ISSUES_…_PLAN_2026-09-25.md`
的 U-13-S3 记法一致：只剩**传输层**（双主机名 + 互信 CA）未验证。

### 4.10.3 剩余

计划的**唯一**剩余项回到 **F-1 的写半边**（§4.9.3）：分叉检测 + v2.1 解析 + 回写 group。

---

## 4.11 F-1 写半边：状态决议接线完成（`62d98ac49`，2026-09-28）

计划 §F-1 的验收判据是"**要么接线（并说明在哪个入站/room-join 路径调用）**，要么按铁律 1
删除死实现"。本节给出接线结论与证据。

### 4.11.1 调用路径（验收判据点名的那一半）

| 接缝 | 覆盖的写入 | 何时维护 |
|---|---|---|
| `MessagingService::create_event_with_graph`（`synapse-services/src/room/messaging/events.rs`） | 本地 `/send`、**入站联邦事务**（`synapse-web/.../federation/transaction.rs`）、`backfill`、联邦 invite | 事件已提交（`tx.is_none()`）且是状态事件时 |
| `MembershipService::join_room_via_federation`（`synapse-services/src/room/membership/federation.rs`） | **联邦 join** 的 state 批次 | 事务 commit **之后**逐条维护（走查必须看得见已提交行） |

两者都调用同一个 `StateRecord::after_state_event`（唯一实现）。房间创建/升级在事务内写事件，
不维护 —— 其记录在**第一次分叉**时由解析一并产生。

### 4.11.2 维护语义

- **分叉**（forward extremities > 1）⇒ 逐 extremity 沿 `prev_events` 求"事件处 state"
  （多父节点处递归到同一入口）→ `resolve_state_for_version_with_rules`（v12+ v2.1 / v1–v11 v2）
  → 结果整体回写为新 `state_groups`（+ `state_group_state` + `state_group_edges`
  - `event_to_state_groups`）。
- **有记录、无分叉** ⇒ 前向拷贝：新 group 携带**完整**状态（读路径只服务 group 自身的行，
  只写增量会让记录陈旧）。
- **无分叉、无记录** ⇒ 无操作：房间继续走事件日志的时间戳推导 —— 与接线前**逐字相同**。
- 只有**状态事件**付代价（一次 extremities 查询）；message 在第一次查询前返回，消息热路径不变。
- 失败**尽力而为**：事件已落库，不能回滚 ⇒ 记 `warn` 并退化为时间戳推导（与相邻的
  签名/哈希补写同策略）。

### 4.11.3 读路径一致性

`get_state_events` / `get_state_event`（§4.9.2）之外，`get_state_events_by_type` 也改为优先记录：
记录是**每个键**的当前状态，若只有整表投影走记录，一个冲突的**单例类型**
（`m.room.join_rules` / `m.room.encryption` / `m.room.server_acl`）仍可能从败方分支读出。
历史态读取（`get_state_events_at_or_before`）与增量读取（`batch` / `since_batch`）**不动**。

### 4.11.4 端到端证据（计划 §4.1 要求的"被调用"证明）

| 用例 | 断言 |
|---|---|
| `the_service_write_seam_maintains_the_record` | **通过服务接缝**写两个分叉 topic ⇒ 记录出现，且读路径服务**授权分支** `$topic_a` |
| `forked_state_is_resolved_and_served` | 无记录时时间戳推导选中较新但**未授权**的 `$topic_b`；维护后必须改为 `$topic_a`；记录是**完整**状态（create/member 键在） |
| `later_state_events_are_folded_into_the_record` | 合并分支后再写 topic ⇒ 记录跟随更新且仍是完整状态 |
| `unforked_rooms_keep_the_event_log_derivation` | 无分叉无记录的房间**不产生**记录，读路径仍走事件日志 |

**变红自证（铁律 8）**：把 `after_state_event` 的分叉分支置为恒不进入（`if false`）⇒
前三条**全部 FAIL**（`left: Some("$topic_b")` / `right: Some("$topic_a")`），第四条仍 PASS
（它本就不依赖解析）。复原后 4/4 通过。这证明用例判的是**解析结果**，不是"恰好通过"。

### 4.11.5 已知边界（下一批可收紧处）

1. **事件处 state 是现算的**：每次解析沿 DAG 走一遍（上限 `MAX_RESOLUTION_EVENTS = 4096`，
   超限则报错并退化为时间戳推导），不像上游那样**每事件持久化 state group**。
   `event_to_state_groups` 表仍是这条路的规模化形态（也解释了为什么它此前无人使用）。
2. `restricted join` 在 `state_map_auth` 下**欠授权**（需 allow rooms 的 state）——
   F-2 已登记，非本次引入。
3. 记录只覆盖**已接线接缝**写入的状态事件；绕过这两个接缝直接写 `events` 的代码路径
   （若将来出现）不会维护记录 —— 届时读路径会退回时间戳推导，而**不会**读到过期记录
   （记录不会被错误地当作最新态继续服务，除非它已经是最新 group；见边界 1 的规模化修法）。
