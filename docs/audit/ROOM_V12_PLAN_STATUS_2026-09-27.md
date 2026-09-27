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
| **A-2** | 冻结 Q1–Q8 产品决策（写进 §5 或 ADR） | ❌ **未做** | 全仓 `grep Q1..Q8 / 决策落定 / ADR` 在 `docs/audit/*2026-09-2*` 无命中；§5 全部仍标"建议（供讨论，不是结论）" |
| **A-3** | v12 一致性 fixture 与 oracle 骨架 | ❌ **未做** | `tests/unit/u13_interop_fixture_tests.rs:105` 仍是 `for room_version in ["3", "10", "11"]`；无 v12 fixture 文件 |
| **B-1** | 建立入站事件鉴权入口（单一实现） | ✅ **完成** | `synapse-federation/src/event_auth/rules.rs`（282 行，`check_inbound_event_auth` / `InboundEventAuth` / `ResolvedAuthEvent`）；入口壳与版本分派齐备 |
| **B-2** | 规则 3.5：`auth_events` 同房校验 | ✅ **完成（已接线）** | `ce4078969`；**生产调用点** `synapse-web/src/routes/federation/transaction.rs:378`（解析 auth_events → `check_inbound_event_auth`，失败即 reject + `security_audit` 日志）；`rules.rs:152` `enforces_auth_events_room_rule` 只对 v12+ 生效；单测含"跨房拒绝/同房通过/无法解析拒绝/pre-v12 不定义该规则" |
| **C-1** | create 事件身份 finalize（G-08） | ✅ **完成** | `7d79982c9`：`write_creation_event` 改走 `create_event_with_pdu`（会 finalize），占位 ID 仅用于 v1/v2；验收测试 `tests/integration/room_service_tests_migrated.rs:5175-5196`（v11 create id 长 44、无 `:`、等于对端复算值、图已重指向 final id） |
| **C-2** | room_id 推导（`$`→`!`）与创建流程重排 | ✅ **完成**（`e2b8266b3`） | 新增唯一 helper `room_id::room_id_from_create_event_id`；`create_room` 先定稿 create → 派生 room_id → 写 rooms 行；验收测试 `test_create_room_v12_room_id_is_the_create_event_id`。**注意：D-6 是它的硬前置**（见下） |
| **C-3** | 无域名 room ID 语法收敛（G-17..G-23、G-20） | ✅ **完成**（`6036c4cb8`/`b089e0323`/`16ee8208f`） | ✅ 语法：`synapse-common/src/room_id.rs`（单实现）+ 两个校验器委托；6 处联邦守卫改用 `is_well_formed_room_id`；`room_id.contains(':')` 生产代码 **0** 处<br>✅ 本地性（G-21）：`b089e0323` `MembershipService::room_locality`<br>✅ DB CHECK 放宽为两形态（`16ee8208f`）+ 指纹 `16d86ee4035cd351`（顺带修掉 HEAD 上的既有红项）+ 真 DB 契约用例<br>❌ 未收敛：`invite.rs:277`（产出 `$uuid:!xxx`）、`space/repository.rs:30`、`actions.rs:41`（join 目的地） |
| **C-4** | 创建侧不写 `predecessor.event_id`；升级顺序反转 | ❌ **未做** | `synapse-services/src/room/service.rs:549` 仍写 `predecessor`（含 `event_id`）；`:496` 注释仍锚定 tombstone 的 event ID |
| **C-5** | `CreateRoomConfig.room_id` 逃逸口处置 | ❌ **未做** | `service.rs:41` `pub room_id: Option<String>` 仍在；`admin/notification.rs` / `space/repository.rs` 的合成房间未动 |
| **D-1** | 规则 1.2：v12 create 带 `room_id` 则拒绝（无 `room_id` 时推导房间身份） | ✅ **完成**（`227a7228d`） | `validate_inbound_transaction_pdu` 接收 `room_version`/`event_id`：v12+ create 带 `room_id` 即拒、无则用 `room_id_from_create_event_id` 推导；5 单测 + 变异自证；联邦事务集成 14/14 |
| **D-2** | 规则 2：room_id 必须是已接受 create 事件 ID | ❌ **未做** | 依赖 C-2，无 `room_id→create` 反查 |
| **D-3** | 规则 2.5 / MSC4307 的 v12 收敛 | 🟡 **B-2 已覆盖** | 与 B-2 同一实现；`rules.rs` 已版本分派，无独立待办 |
| **D-4** | 本地 `auth_events` 不再包含 create（G-28） | ✅ **完成**（`b7cf472b4`） | `auth_types_for_event` 改为 `if !room_version_at_least(room_version, 12)` 才加 create；3 个新用例 + 变异自证（反转阈值 → 5 红） |
| **D-5** | 裁定 auth chain / auth difference 是否含 create | ❌ **未做** | `synapse-web/src/routes/federation/pdu.rs:141-157` 五类型清单原样；计划自标【待核验】 |
| **D-6** | 出站 create PDU 省略 `room_id`（G-10） | ✅ **完成**（`e2b8266b3`） | `build_pdu` 对 v12+ create 不写 `room_id`；**它是 C-2 的硬前置**（见 §2.5） |
| **E-1** | `additional_creators` 校验（规则 1.4） | ❌ **未做** | 全仓 `grep additional_creators` **0** 命中（与计划 G-31 一致） |
| **E-2** | 创建者集合 + 无限 PL（G-32/33/35） | ❌ **未做** | `auth/power_levels.rs:73` 仍 `resolve_room_creator -> Option<String>`（单个）；无 `i64::MAX` 哨兵 |
| **E-3** | 规则 10.4：PL 的 `users` 不得含创建者 | ❌ **未做** | 无该检查；`rules.rs:120` 注释把它列为待加规则 |
| **F-1** | 状态决议接线决策（G-38 零调用者） | 🟡 **已决策（A-2 Q6b），未执行** | 裁定：**不接线**；删除 `StateResolutionService` / `resolve_state_v2` 死实现，v2.1 只在 `resolve_state_with_auth_chain` 上演进（该函数目前亦仅被 bench 调用，去留需在 F-2 一并处理） |
| **F-2** | v2.1 三处修改 | ❌ **未做** | `grep "conflicted state subgraph"` / `"iterative auth"` 在 src **0** 命中 |
| **F-3** | v1–v11 兼容边界 | ❌ **未做** | 依赖 F-2 |
| **G-1** | 能力表收敛：仅 v12 可创建 | 🟡 **部分**（Q5 已落，`c83e3faf9`） | ✅ 版本 13 已移除（`stable_parse_only` 一并删除）；❌ v1–v11 仍 `can_create = true`，待收敛（Q1(a)：只禁创建，保留 join/federate） |
| **G-2** | 连带面清单（含联邦 `m.room_versions` 补测） | ❌ **未做** | 收敛本身未做；联邦 `/version` 的 `m.room_versions` **仍无内容断言** —— `tests/integration/api_federation_tests.rs:279` 只有 `assert_eq!(json["capabilities"]["m.room_versions"]["default"], DEFAULT_ROOM_VERSION)`（比常量，不校验 `available`/`unstable_features`），且该行来自 `5aac642c1`（2026-09 初的 cas_service 提交），**不是** v12 工作所加。计划 G-05 的结论未被推翻 |
| **H-1** | 逐份更正文档（含额外 6 份） | 🟡 **部分完成** | `d3a12ca73`（`docs/room-version-12-13-correction`，已并进本分支历史）改了 `CURRENT_ISSUES_AND_PLAN.md` 与 `REMAINING_ISSUES_...2026-09-25.md`；`V12_ROOM_VERSION_..._PLAN.md:21` 已自我更正 MSC4239 误引<br>❌ `AUDIT_SUMMARY_2026-09-12.md`、`DB_REVIEW_2026-09-17.md` 最近提交仍是 markdownlint 批（**未加 superseded 横幅**）；`docs/synapse-rust/` 与 `docs/audit/O1_PHASE1_...` 未核 |
| **H-2** | Q1–Q7 结论落档 | ❌ **未做** | 依赖 A-2 |

**计数（批次 2 后）**：✅ 完成 10（A-1、A-2、B-1、B-2、C-1、C-2、C-3、D-1、D-4、D-6）｜🟡 部分 3（D-3、G-1+G-2、H-1）｜❌ 未做 11（F-1 已决策未执行）。

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
