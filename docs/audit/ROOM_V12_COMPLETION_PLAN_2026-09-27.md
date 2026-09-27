# Room Version 12 完成度缺口分析与实施计划

- **文档日期**：2026-09-27
- **工作树**：`/Users/ljf/Desktop/hu_ts/synapse-rust/.worktrees/room-v12`
- **分支 / 基线**：`feat/room-v12-complete` @ `3856f2961`（工作树干净）
- **本轮性质**：**规划文档**。未改动任何生产代码，未 `git commit`，未联网。
- **范围**：Matrix room version 12（MSC4304）在本仓的完成度。

## 0. 证据口径（先读，避免把推测当实测）

本文所有结论标注证据类型：

| 标记 | 含义 |
|---|---|
| `【实测】` | 本会话在 `3856f2961` 上实际执行的 `grep` / `sed` / `git` 命令所得。给出 `路径:行号`。 |
| `【上游】` | 直接引用本轮已核验的上游规范/MSC 事实（由任务方提供，本轮**未**重新联网复核）。 |
| `【待核验】` | 无法在本仓或已给材料中确认，明确标注为不确定。 |

**必须避免的名词混淆（本轮实测确认）**：`migrations/00000000_unified_schema_v12.sql` 里的 "v12" 是**数据库 schema 版本**，与 Matrix room version 12 **无关**。全仓 `grep` 该文件名会产生大量假阳性引用，本文不使用它们作为房间版本证据。

**基线状态一句话**：本仓 `DEFAULT_ROOM_VERSION = "12"` 且 `"12"` 可创建，但 v12 的四个定义性 MSC（4289/4291/4297/4307）**没有一个是实现的**；已落地的只有 v12 红action 规则中的 `room_ids_as_hashes` 一项，以及事件 ID 的 v3+ reference-hash 纯函数（后者在**房间创建路径上未被使用**）。

---

## 1. 上游规范要求（已核验，可直接引用；不要重新联网研究）

> 本节内容为任务方已核验的上游事实，按 MSC 归纳。`【上游】`。规范页为 `matrix-spec` 的 `content/rooms/v12.md`，其规则编号按 MSC4304 的编号引用。

### 1.1 MSC4304 —— room version 12 的定义

- **v12 = room v11 基线 + MSC4289 + MSC4291 + MSC4297 + MSC4307**。
- 规范中 **v12 是 stable**，且规范要求 **"Servers SHOULD use room version 12 as the default room version"**。
- 引用：MSC4304 <https://github.com/matrix-org/matrix-spec-proposals/blob/main/proposals/4304-room-version-12.md>；规范页 `content/rooms/v12.md`。

### 1.2 MSC4291 —— room ID = create 事件的哈希

引用：<https://github.com/matrix-org/matrix-spec-proposals/blob/main/proposals/4291-room-ids-as-hashes.md>

- room ID = `m.room.create` 事件的 **event ID**，把 sigil `$` 换成 `!`。
  例：create event id `$31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM` ⇒ room id `!31hneApxJ_1o-63DmFrpeqnkFfWppnzWso1JvH3ogLM`。
- room ID 语法被**收窄**为 **`!` + 43 个 unpadded urlsafe base64 字符**，**不再含 `:` 域名部分**。
- **`room_id` 字段从 `m.room.create` 事件里整个移除**（create 事件上省略）；但在**非联邦 API 上重新引入** —— 与 `event_id` 在联邦上缺失、其余 API 上补齐同理。
- `auth_events` **不再包含 create 事件**（由 room_id 隐含）。
- 规范 v12 auth 规则变化：
  - **1.2** 改为"create 事件若带 `room_id` 则拒绝"；
  - **新增规则 2**："若 event 的 room_id 不是一个已被接受的 create 事件的 event ID（sigil 换成 `!`）则拒绝"；
  - **删除旧规则 2.4**；
  - **新增规则 2.5**："auth_events 中任何条目的 room_id 与当前事件不符则拒绝"（与 MSC4307 重叠）。
- **`content.predecessor.event_id` 被废弃**（升级房间的 chicken-and-egg）；客户端 **MUST NOT** 期待它存在。
- unstable 前缀是 `org.matrix.hydra.11`（与 MSC4289/MSC4297 同期）。

### 1.3 MSC4289 —— 显式赋予创建者特权

- **创建者 = `m.room.create` 的 `sender` 加上 create content 里的 `additional_creators`**（若存在；必须是字符串数组，每个都要通过与 sender 相同的 user ID 校验，否则拒绝 —— 规范 v12 规则 **1.4**）。
- 创建者拥有**无限 power level**，**不能被 `m.room.power_levels` 降权**，也**不能出现在 power_levels 的 `users` 里**（规范 v12 规则 **10.4**：若 `users` 含 create 的 sender 或任一 `additional_creators` 则拒绝）。
- 创建后不可更改。

### 1.4 MSC4297 —— State Resolution v2.1

相对 v2 的三处修改：

1. **迭代鉴权检查（iterative auth checks）改为从空 state map 开始**（v2 是从 unconflicted state map 开始）。
2. 新增 **conflicted state subgraph** 定义：conflicted state set 中任意两事件之间沿 `auth_events` 的**所有路径的并集，含端点**。
3. **full conflicted set = conflicted state set ∪ conflicted state subgraph ∪ auth difference**。

上游另有 `state-res-2.1` implementer's guide 可供引用【上游】。

### 1.5 MSC4307

- 校验 `auth_events` 中每个事件的 `room_id` 与当前事件一致 —— 即规范 v12 规则 **3.5**。

### 1.6 本仓已具备的 v12 相关实现（避免把"全缺"写成结论）

- **v12 红action 规则已实现**：`synapse-common/src/redaction.rs:210` —— `"12" => Some(RedactionRules { room_ids_as_hashes: true, ..RedactionRules::updated() })`；`redaction.rs:327-331` 在 redact `m.room.create` 时把 `room_id` 从允许字段里剔除（注释即"room_id is derived from the event ID in MSC4291 rooms"）。测试：`redaction.rs:764`。`【实测】`
- **v3+ reference-hash 事件 ID 纯函数已实现**：`synapse-common/src/event_id.rs:77-79` 的版本集合**已含 `"12"`**；`compute_event_id`（`:125`）、`resolve_received_event_id`（`:147`）。`【实测】`
- **`ck_events_event_id_format` 已放行 reference hash 形态**：`migrations/00000000_unified_schema_v12.sql:4714-4722`（DROP 后重声明；`OR event_id ~ '^\$[a-zA-Z0-9._=+/-]{43}$'`）。`【实测】`

> 也就是说：v12 缺的不是"红action 与 hash 函数"，而是**这四项 MCS 的行为规则与接线**。

---

## 2. 本仓缺口清单（逐条带 `路径:行号` 证据）

### 2.1 能力声明面

| # | 现状 | 缺口 | 证据 |
|---|---|---|---|
| G-01 | `DEFAULT_ROOM_VERSION = "12"`；`RoomVersionCapability::stable("12")`（`can_create: true`） | MSC4289/4291/4297/4307 **均未实现**；声明领先实现 | `synapse-common/src/room_versions.rs:89`、`:113`【实测】 |
| G-02 | `resolve_room_version` 对 v1–v12 全部返回 `Some` | 未收敛；"仅 v12"需改这里与其守卫 | `room_versions.rs:160-167`【实测】 |
| G-03 | 守卫测试把"v1–v12 全可创建、v13 parse-only"钉死 | 收敛 `can_create` 会直接打红这些断言 | `room_versions.rs:239-260`（`:248` 断言 1..12 可创建）、`:262-287`（`:269-270` 断言 `available.len() == 12`）【实测】 |
| G-04 | `/capabilities` 快照固定 `"default": "11"`、`available` 仅 1–11 | **当前基线已是红项**：快照 2026-09-18 接受，v12 创建 2026-09-25 开启后未重接受 | `tests/integration/snapshots/integration__api_route_snapshots_tests__capabilities_v3.snap:29-44`；测试体 `tests/integration/api_route_snapshots_tests.rs:161-176`（`:170-174` 只脱敏 `unstable_features`）【实测】 |
| G-05 | 联邦 `/version` 也发 `m.room_versions` | 该表面**无任何断言**（`test_federation_version` 只断言 `server.version`） | `synapse-web/src/routes/federation/mod.rs:263`、`federation/events.rs:407-421`；测试 `tests/integration/api_federation_tests.rs:138`【实测】 |
| G-06 | `/versions` | **实测确认不含房间版本**（只有 `versions` + `unstable_features`），故 `/versions` **不在**连带面内 —— 与任务书预期不同 | `synapse-services/src/capability_governance.rs:247-280`【实测】 |
| G-07 | route ledger / SDK 契约 | **实测确认零房间版本引用**；只有"增删路由"才需要动 `SCHEMA_VERSION="4"` 与两条 fixture | `synapse-web/src/routes/ledger_export.rs:45`（`room_version`/`capabilities` 在 ledger 中 0 命中）【实测】 |

### 2.2 事件身份 —— MSC4291 的**前置阻塞**（任务书未列，实测新发现）

| # | 现状 | 缺口 | 证据 |
|---|---|---|---|
| G-08 | 房间创建路径用 **legacy** `generate_event_id`（`$<millis>$<rand>:<server>`）产生 create 及全部初始状态事件的事件 ID；`GraphMetadataWriter::create_event_with_graph` 是**不做 finalize 的旁路**（直接 `inner`） | create 事件的 ID 必须先是 reference hash，才可能"room_id = create event id"。**这是 MSC4291 创建侧的第一道阻塞**，也意味着 v11/v12 房间目前落库的本地事件 ID 非 spec 形态 | `synapse-services/src/room/lifecycle/create_events.rs:34`（`let event_id = generate_event_id(...)`）；`synapse-services/src/graph_metadata.rs:490-511`（`create_event_with_graph` 直接 `self.inner...`）；对比同一文件 `:402-486` 的 `create_event` / `create_event_with_pdu` 才调用 `finalize_event_id`（`:359-388`）；`synapse-web/src/routes/federation/pdu.rs:37-39` 的注释亦自认"event_id 不是 v4+ reference hash"【实测】 |
| G-09 | reference hash / `$`→`!` 的覆盖：纯函数有，房间身份推导没有 | 全仓 **0** 个"从 create 事件推导 room_id"的 helper（`grep -rni "room_id_from\|derive_room_id\|hash_room_id"` 0 命中；`event_id.rs` 内亦无 sigil 替换） | `synapse-common/src/event_id.rs:77-158`【实测】 |
| G-10 | `build_pdu` 与 `state_pdu` **无条件**写入 `room_id` | v12 create PDU 应省略 `room_id`（联邦形态）；出站、签名材料、投影三处一致地要处理 | `synapse-common/src/pdu.rs:89`；`synapse-web/src/routes/federation/pdu.rs:103`；签名材料经版本红act（`synapse-federation/src/signing.rs:211-222`）故 hash 侧"偶然正确"，线格式仍不合规【实测】 |

### 2.3 MSC4291 —— 创建侧

| # | 现状 | 缺口 | 证据 |
|---|---|---|---|
| G-11 | `create_room` 在 create 事件**之前**就定下 room_id | 必须先定稿 create 事件（含 content）→ reference hash → event id → room_id | `synapse-services/src/room/lifecycle/create.rs:26`（`config.room_id.clone().unwrap_or_else(\|\| self.generate_room_id())`）、`:425-426`（`fn generate_room_id` → `generate_room_id(&self.server_name)`）【实测】 |
| G-12 | `generate_room_id` 生成 **18 字节随机数** + `:server` | MSC4291 创建侧完全缺失；此函数在 v12 语义下无用途 | `synapse-common/src/crypto.rs:145-148`【实测】 |
| G-13 | `build_create_event_content` 产出 `{creator, room_version, ...}`，**不含 `room_id`**（这点是对的），但也不支持 `additional_creators` | 需决定 create content 的最终字段集；`creator` 字段与 v11+/MSC4289 的创建者定义（sender + additional_creators）并存会歧义【待核验：v11 起 `content.creator` 是否应缺席，本轮无法联网核实】 | `synapse-services/src/room/lifecycle/create.rs:509-511`；客户端提供的 `creator` 已在 handler 层剥除 `synapse-web/src/routes/handlers/room/management/create.rs:181-185`【实测】 |
| G-14 | 升级路径：先建 tombstone（用**旧**的 room_id 与旧格式 event id），再以**抢占式 room_id** 建新房 | v12 下顺序必须反转：create 事件不含 `predecessor.event_id`，先定稿 create 事件 → room_id → 再建 tombstone（`replacement_room` 指向新 room_id）。现有注释与断言都锚定 `predecessor.event_id` | `synapse-services/src/room/service.rs:494-560`（`:504` `let new_room_id = generate_room_id(...)`、`:549-554` `predecessor: { room_id, event_id }`）；测试 `tests/integration/room_service_tests_migrated.rs:782-783`、`:796-841`【实测】 |
| G-15 | `CreateRoomConfig.room_id` 抢占式逃逸口 | v12 下不可能预先知道 room_id；铁律 1 视角下需要删除或严格限域 | `synapse-services/src/room/service.rs:76`；使用点 `create.rs:24-26`【实测】 |
| G-16 | 绕过 Room 生命周期的**内部合成房间** | `!server_notice_<uuid>:<server>` 与 `!space_<uuid>:<server>` 直接格式化 room_id 与事件 ID，既非 v12 语义也不经过任何校验 | `synapse-web/src/routes/admin/notification.rs:350-355`（room_id + 3 个 `$uuid:server` 事件 ID）；`synapse-storage/src/space/repository.rs:30`【实测】 |

### 2.4 MSC4291 —— 无域名 room ID 的语法与校验收敛（**穷举**）

生产代码中会拒绝"无冒号 room ID"的位置：

| # | 位置 | 形态 | 证据 |
|---|---|---|---|
| G-17 | 客户端侧主校验 `validate_room_id` | `room_id[1..].rsplit_once(':')` 失败即 `invalid_input`；**110 个调用点、18 个文件** | `synapse-web/src/routes/validators.rs:49-73`（关键 `:60`）【实测：`grep -rn "validate_room_id(" synapse-web/src \| grep -v "pub fn" \| wc -l` = 110】 |
| G-18 | `synapse-common` 侧校验 | 正则 `^![a-zA-Z0-9._=\-]+:[a-zA-Z0-9.-]+$`；回落正则同类 | `synapse-common/src/validation.rs:62`、`:182-190`、`:326`、`:380-385`。**实测：`Validator::validate_room_id` 与 `ValidationContext::validate_room_id` 在生产代码中均无调用者**（唯一命中是对自身的封装）→ 属"测试/死代码"面 |
| G-19 | 联邦路由硬校验 | `!room_id.starts_with('!') \|\| !room_id.contains(':')`，**6 处** | `synapse-web/src/routes/federation/events.rs:431`、`:481`；`federation/membership/join.rs:254`；`federation/membership/invite.rs:25`、`:271`；`federation/membership/leave.rs:137`【实测，已穷举】 |
| G-20 | DB CHECK 约束 | `ck_rooms_room_id_format CHECK (room_id ~ '^![a-zA-Z0-9._=+./-]+:[a-zA-Z0-9.-]+$')` —— 无冒号 id 触发 **23514**，**建房必败** | `migrations/00000000_unified_schema_v12.sql:4694-4697`（`:4691-4698` 为幂等 DO 块）【实测，全仓唯一命中】 |
| G-21 | "远端/本地"判定依赖冒号 | `server_name_from_id(id) = id.rsplit_once(':').map(\|(_, s)\| s)` → 无冒号 v12 room id 返回 `None` ⇒ `is_remote_id` 为 `false` ⇒ **远端 v12 房间被当成本地房间** | `synapse-services/src/room/membership/service.rs:205-224`；调用点 `actions.rs:41`（join 目的地）、`:161`（leave 目的地）【实测】 |
| G-22 | 用 room_id 反推 server 的拼接 | `invite.rs:277` `format!("${}:{}", uuid, room_id.split(':').next_back().unwrap_or("server"))` —— 对 v12 room id，`next_back()` 返回**整串** `!xxx`，产出 `$uuid:!xxx` | `synapse-web/src/routes/federation/membership/invite.rs:277`【实测】 |
| G-23 | 测试侧的"必须含冒号"断言 | 至少 10 处，其中 `tests/unit/msc_tests.rs` 在 MSC 语境下把 `!room:server` 当唯一合法形态；另有 `tests/unit/pinned_route_tests.rs:106` 显式断言"缺 `:` 必须被拒" | `tests/unit/msc_tests.rs:32-40`、`:54-59`；`tests/unit/pinned_route_tests.rs:104-108`；`synapse-common/src/crypto.rs:504-507`；`synapse-services/src/room/service.rs:761-765`；`synapse-web/src/routes/validators.rs:224-236`；`synapse-web/src/routes/burn_after_read.rs:334-378`；`tests/integration/invite_blocklist_tests_migrated.rs:179-186`【实测，抽样穷举】 |

### 2.5 MSC4291 —— 入站/校验侧

| # | 现状 | 缺口 | 证据 |
|---|---|---|---|
| G-24 | **本仓没有任何"规范 auth rules"引擎**。入站 PDU 只做：sender↔origin 匹配、`origin` 校验、`origin` 是否有房内成员、power-level 写权限、成员状态机迁移 | 规则 **1.2 / 2 / 2.5** 无处落点：v12 需要一个"事件鉴权"入口来拒绝"create 带 room_id"、拒绝"room_id 不是已接受 create 事件 ID 的事件"、校验 auth_events 同房 | `synapse-web/src/routes/federation/transaction.rs:681-704`（`validate_inbound_transaction_pdu`：只查 `room_id`/`sender`/`type` 存在性与 sender↔origin）、`:245-272`（create 跳过 origin-in-room；其余走 `verify_state_event_write`）、`:278-322`（成员状态机）；`synapse-services/src/auth/power_levels.rs:174-193`【实测】 |
| G-25 | `validate_inbound_transaction_pdu` **强制**顶层 `room_id` | v12 create PDU 按 MSC4291 省略 `room_id` ⇒ 直接被拒（400 Missing room_id） | `synapse-web/src/routes/federation/transaction.rs:682-686`【实测】 |
| G-26 | `inbound_pdu_room_version`：create 由 `content.room_version` 定版本（好），但非 create 仍要 `room_id` 查库 | v12 下 create 的**房间身份**必须由 event id 推导后回填 | `transaction.rs:727-743`（`:737-742` 要求 `room_id`）【实测】 |
| G-27 | 入站 `auth_events` 只被**收集**用于图字段写入，逐条内容从不校验 | MSC4307 / 规则 3.5 / 规则 2.5 全缺 | `transaction.rs:349-355`、`:498-511`、`:569`（`create_event_with_graph`）【实测】 |
| G-28 | 本地 `auth_events` **选择**仍包含 `m.room.create` | v12 必须不选它（由 room_id 隐含） | `synapse-services/src/room/state/auth_events.rs:106-116`（`:113` 固定加入 create）；测试把 v11 行为钉死 `auth_events.rs:216-229`；`creation_graph.rs:126-129` 断言 create 出现在 auth_events【实测】 |
| G-29 | `is_auth_chain_member` 用**五类型清单**（含 `m.room.create`） | 该清单与规范的 auth chain 定义（auth_events 传递闭包）本就不一致（模块自认），v12 下是否仍含 create 需裁定 | `synapse-web/src/routes/federation/pdu.rs:141-157`（`:142` 自认"the real definition is the transitive closure ... out of scope here"）【实测】 |
| G-30 | `predecessor.event_id` 仍被写入、被断言 | 需按 MSC4291 移除写入与断言；升级顺序反转（同 G-14） | `synapse-services/src/room/service.rs:549-554`；`tests/integration/room_service_tests_migrated.rs:782-783`、`:836-840`【实测】 |

### 2.6 MSC4289 —— 创建者特权

| # | 现状 | 缺口 | 证据 |
|---|---|---|---|
| G-31 | `additional_creators` **全仓 0 命中**（src/tests/docs 均无） | create content 校验（必须是字符串数组、每个通过与 sender 相同的 user ID 校验）完全缺失 | `grep -rn "additional_creators" .` 0 命中【实测】 |
| G-32 | 创建者"兜底 100"只在**没有** power_levels 事件时生效 | 有 PL 事件时创建者可被写成任意值（含 0）⇒ "无限 PL / 不可降权"没有落点 | `synapse-services/src/auth/power_levels.rs:7-40`（`:14-16` 非成员 −1；`:20-30` 先看 `users`/`users_default`；`:34-38` 仅当 PL content 缺席才 `return Ok(100)`）【实测】 |
| G-33 | `resolve_room_creator` 返回**单个** creator，且优先读 `content.creator` | 需改为"创建者集合"（sender ∪ additional_creators），并处理 `content.creator` 的取舍 | `power_levels.rs:73-93`（`:80-82` 先读 `content.creator`）【实测】 |
| G-34 | `verify_power_levels_change` 只检查"不得越权升降"，**不拒绝** `users` 中出现创建者 | 规则 10.4 缺失：PL 的 `users` 含 create sender 或任一 additional_creator 必须拒绝 | `power_levels.rs:196-215`（遍历 `users` 只比较数值）【实测】 |
| G-35 | 无"无限 PL"表示 | 所有比较都是数值（`verify_room_admin` 等）⇒ 需要引入 `i64::MAX` 之类的哨兵或显式"creator"分支 | `power_levels.rs:7-40`、`:355-370` 等【实测】 |

### 2.7 MSC4297 —— State Resolution v2.1

| # | 现状 | 缺口 | 证据 |
|---|---|---|---|
| G-36 | 存在名为 `resolve_state_v2` 的函数 | **零调用者**（定义外仅自身单测）⇒ 死代码 | `synapse-federation/src/event_auth/state_resolution.rs:367`；全仓 `grep` 调用点 0【实测】 |
| G-37 | 实际生效的"状态决议"是 **timestamp/event_id 裁决的 auth_events BFS**，不是 v2 也不是 v2.1 | v2 的 unconflicted/conflicted 分离、mainline ordering、reverse topological power ordering、auth difference 与 full conflicted set 的组合都只部分存在或未接线 | `synapse-federation/src/event_auth/state_resolution.rs:106-160`（`resolve_state_with_auth_chain`，`:135-146` 纯时间戳+event id 裁决）；入口 `synapse-federation/src/state_resolution.rs:83`；`StateResolutionService::resolve`（`:77`）【实测】 |
| G-38 | `StateResolutionService` **零调用者**（未在 workspace 任何地方实例化/调用） | ⇒ 本仓**状态决议根本没有接线**（不只是"没实现 v2.1"）。这比任务书预期更严重 | `synapse-federation/src/state_resolution.rs` 全文件；`grep -rn "StateResolutionService" .` 仅自身【实测】 |
| G-39 | v2.1 三处改动全缺 | `grep -rni "iterative\|conflicted state subgraph\|v2\.1"` 在 src 下 0 命中（仅 `room_versions.rs:81` 的散文注释） | 【实测】 |
| G-40 | `calculate_auth_difference` 存在 | 仅被自身单测调用（`:673`、`:684`），无生产调用点 | `synapse-federation/src/event_auth/state_resolution.rs:284`【实测】 |
| G-41 | 无任何 v2.1 测试向量 | 上游 implementer's guide 的向量未落地；本仓 v2 测试只覆盖 `detect_conflicts` / `calculate_state_id` / `calculate_auth_difference` 等辅助函数 | `state_resolution.rs:500-687`【实测】 |

### 2.8 MSC4307 / 规则 3.5

| # | 现状 | 缺口 | 证据 |
|---|---|---|---|
| G-42 | `auth_events` 逐条 `room_id` 与当前事件比对 | **完全不存在**（入站只收集 ID；`AuthStateSnapshot` 只存 `(type,state_key)→event_id`） | 同 G-27；`synapse-services/src/room/state/auth_events.rs:59-100`【实测】 |
| G-43 | 无 MSC4307 专项测试 | 最近似的测试是第三方邀请的 room_id mismatch，语义不同 | `synapse-web/src/routes/federation/membership/invite.rs:551`【实测】 |

### 2.9 DB / 迁移

| # | 现状 | 缺口 | 证据 |
|---|---|---|---|
| G-44 | `rooms.room_id TEXT NOT NULL` + PK，22 个 `REFERENCES rooms(room_id)`（含 `events`） | room_id 语义变更（无域名、= create event id）后需逐一复核；无冒号 id 被 CHECK 拒（同 G-20） | `migrations/00000000_unified_schema_v12.sql:234-270`（`:235`、`:250`）、FK 行 `:280, 324, 366, 406, 436, 469, 482, 494, 509, 521, 530, 583, 593, 616, 631, 2343, 2667, 2684, 2697, 2723, 3015, 4058`【实测，22 条】 |
| G-45 | schema 指纹守卫 | 任何 CHECK/列变更必须重算 | `tests/unit/test_isolation_unification_tests.rs:151`（`EXPECTED_BASELINE_FINGERPRINT = "efd39fc561affd7a"`）【实测】 |
| G-46 | `ck_rooms_room_version_valid` 已是 `^[0-9]+(\.[0-9]+)*$`（放行 v12+ 联邦 join） | 无需改；注释已解释为何不用 1..11 白名单 | `migrations/00000000_unified_schema_v12.sql:266-267`（注释 `:257-265`）【实测】 |
| G-47 | `events.event_id` 两种形态 CHECK 已放行 reference hash | 无需改；这是 MSC4291 的有利条件 | `migrations/00000000_unified_schema_v12.sql:4714-4722`【实测】 |

### 2.10 测试、快照与 fixture

| # | 现状 | 缺口 | 证据 |
|---|---|---|---|
| G-48 | 集成测试以 v9/v10/v11/v1 显式建房 | 收敛"仅 v12"后这些用例会失败，需迁移或限定范围 | `tests/integration/room_service_tests_migrated.rs:751`、`:807`、`:866`（`"9"`）、`:4989`、`:5098`（`"11"`）、`:5061`（`"1"`）、`:279`（`"10"`）；`tests/integration/federation_existence_leak_tests.rs:567`、`:735`（`"11"`）、`:863`（`"10"`）、`:680`（参数化）【实测】 |
| G-49 | U-13 oracle fixture 覆盖 v3/v10/v11 | 缺 v12 fixture（reference hash + 无 room_id 的 create PDU + 签名材料） | `tests/unit/u13_interop_fixture_tests.rs:105`（`for room_version in ["3", "10", "11"]`）【实测】 |
| G-50 | v13 声明与实现冲突 | `stable_parse_only("13")` 声明"可 parse/join/federate"，但 `redaction_rules("13") == None`、`uses_reference_hash_event_id("13") == false`（测试显式断言 `"13"` 报错）⇒ v13 实际连 parse 都不成立 | `room_versions.rs:114` vs `synapse-common/src/redaction.rs:751`、`synapse-common/src/event_id.rs:327-328`【实测】 |
| G-51 | state-res 相关的性能门禁与用例 | 改造状态决议会牵动 bench 与 perf gate | `benches/performance_federation_benchmarks.rs:16-64`；`tests/unit/perf_gate_honesty_tests.rs:118-120`【实测，来自并行审计】 |

### 2.11 配置面（实测纠正）

| # | 现状 | 结论 | 证据 |
|---|---|---|---|
| G-52 | 任务书/并行审计把 `server.default_room_version` 当作"死配置字段" | **实测更准确**：整个 `RoomsConfig`（含 `default_room_version`）被**块注释**掉，即死代码文本，不是活字段、不构成房间版本默认面的第 4 个 surface | `synapse-common/src/config/mod.rs:1670`（`/*`）… `:1725`（`*/`），结构体在 `:1693`、字段在 `:1697`、默认函数在 `:1715-1719`；`grep -rn "RoomsConfig"` 仅命中注释内【实测】 |

### 2.12 文档（一句话结论 / 每份）

> 方法：`【实测】` 逐文件读取 + `grep`。结论中的 TRUE/FALSE 以上游事实为准。

| 文件 | 要不要改 | 一句话结论 |
|---|---|---|
| `docs/audit/V12_ROOM_VERSION_AND_ANIMATED_THUMBNAIL_IMPLEMENTATION_PLAN.md` | **要改** | L18 把 `MSC4239` 当 v12 定义（错，那是 v11 release），L41 的"差距：无（v12 已完全对齐上游）"与 L594-599 的"✅"是**过度声明**，须改为 MSC4304 四项未实现；**注意**：该文件的更正已在未合并分支 `docs/room-version-12-13-correction`（worktree `.worktrees/roomver-docs`，commit `d3a12ca73`），本分支基线**没有**那些更正 ⇒ **不要在本计划中改它**，合并后不冲突但需与本计划同步 |
| `docs/audit/REMAINING_ISSUES_VERIFICATION_AND_OPTIMIZATION_PLAN_2026-09-25.md` | **要改** | U-11（L184）把"v12 升 stable"记为"已修复"、L227-228/L284 仍写"v12/v13 仍 parse-only…放开前必须逐项复核"，与现状（已放开创建）矛盾，须改写为"已声明 stable/default 但四项 MSC 未实现" |
| `docs/audit/CURRENT_ISSUES_AND_PLAN.md` | **要改** | ❌7（L92-97）只讲 v13 parse-only，**漏掉 v12 的过度声明**；应新增一条 ❌ |
| `docs/audit/AUDIT_SUMMARY_2026-09-12.md` | **要改** | L8/L79/L83/L157/L212 记录的"v12/v13 降级为 parse-only / ✅ 已解决（fail-safe）"已被 O-1 反转，需加 superseded 横幅 |
| `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md` | **要改** | §5.1（L218-222）与决策 B6（L271、L399）的证据行号已漂（`DEFAULT` 现为 12、仅 v13 parse-only），且 B6 实际是"未实现 auth rules 就放开创建"，需记为决策结果与后果 |
| `docs/audit/DB_REVIEW_2026-09-17.md` | **要改（仅状态注记）** | §13.6.2（L568-587）与 L757 说"`/capabilities` 丢掉 v12/v13（未裁定）"，现 v12 已 `can_create=true` ⇒ `available` 会含 v12，需更新为"已由 O-1 反转，但协议语义问题仍未解" |
| `AGENTS.md`（`### Protocol declaration discipline`，L435-438）+ `CLAUDE.md` | **不改规则本体** | 规则 L436/L438 正是 v12 当前违背的那一条（"Room-version capability must match actual event/auth behavior"）；CLAUDE.md 无此节且刻意只同步铁律，无需改；本轮**未**发现中文短语"协议声明纪律"（0 命中）【实测】 |
| `README.md`（`## 文档` L158-169） | **不改 v12 文本** | README 索引的是 `docs/synapse-rust/`，**`docs/audit` 零索引**（`grep audit README.md` 0 命中）；`docs/INDEX.md` §六 是过期部分清单。若要本计划可发现，需另加索引条目 |
| 额外（不在任务书清单，但同样是过度声明）| **要改/要注记** | `docs/audit/O1_PHASE1_V12_IMPLEMENTATION_DETAILS.md`（L9 "v12 现在是可创建的"、L376 spec 链接错为 v1.10）、`docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md`（L83/L1206/L1435/L1453/L1483 "v12/v13 不可创建"）、`docs/audit/P2_protocol_contract_2026-09-11.md`（L21/L46/L104-105 过度声明的源头）、`docs/synapse-rust-vs-synapse-comparison.md`（L574/L723/L768/L945）、`docs/synapse-rust/API_COVERAGE_REPORT.md`（L173-174/L241/L279/L357）、`docs/audit/v12-pdu-graph-fields-integration.md`（L154/L156 反而**诚实**地记录了 v12 路径未验签，与 V12 计划"差距：无"冲突） |

---

## 3. 实施计划

### 3.1 与任务书 1–7 的映射与排序差异

任务书给的排序是：① MSC4291 创建侧 → ② MSC4291 入站侧 → ③ MSC4289 → ④ MSC4297 → ⑤ MSC4307 → ⑥ 只允许 v12 → ⑦ 文档。

**本计划的排序不同**，理由如下（每条都是实测证据驱动）：

1. **把 MSC4307（规则 3.5）提到最前**，与入站 auth 校验入口一起做。原因：实测 G-24 表明"入站鉴权规则引擎"在本仓**不存在**；规则 1.2 / 2 / 2.5 / 3.5 必须共用同一个新入口。先做规则 3.5（最小、可独立验收）等于先把入口立起来，后续三项是往同一入口加分支，避免"先做房间身份、再回头建引擎"的返工。
2. **把"事件身份 finalize"作为 MSC4291 创建侧的第一颗子任务**（G-08）。原因：`create_events.rs:34` 用 legacy ID、且装饰器对 `create_event_with_graph` 是旁路（`graph_metadata.rs:490-511`）。不先修这条，"room_id = create event id"在物理上不可能成立。
3. **无域名 room ID 的语法收敛（G-17..G-23、G-20）与创建侧同批**，因为它横跨 route/service/storage/DB 四层，单独做会被 DB CHECK 或 110 个调用点截断，必须先"语法放行"再"语义切换"。
4. **MSC4289 放在 MSC4297 之前**。原因：MSC4297 的 iterative auth checks 会调用鉴权规则，而 v12 的鉴权规则包含"创建者无限 PL"；先落 MSC4289 才能给 v2.1 的 auth check 提供正确语义。
5. **"只允许 v12"（⑥）放到协议实现之后**。原因：G-03/G-04/G-48 表明收敛 `can_create` 会同时打红守卫、快照与约 10 个建房用例；在 v12 行为尚未正确前收敛，等于把"声明领先实现"换成"声明领先实现 + 测试全红"。
6. **文档（⑦）最后**，但它包含一项**无依赖的前置修复**（G-04 快照），放在阶段 A。

### 3.2 阶段与工作项

每一项给出：**目标 / 触及模块 / 依赖 / 验收判据 / 风险**。

---

#### 阶段 A —— 基线止血与前置（无协议语义，可立即做）

**A-1 修复已红的 `/capabilities` 快照**
- **目标**：`snapshot_capabilities_v3` 恢复绿。
- **触及模块**：`tests/integration/snapshots/integration__api_route_snapshots_tests__capabilities_v3.snap`、`tests/integration/api_route_snapshots_tests.rs:161`。
- **依赖**：无。**必须在 A-2 之前**，否则 A-2 之后无法区分"原本就红"与"新红"。
- **验收判据**：`INSTA_UPDATE=no` 下该用例通过；`.snap` 中 `default` 与 `available` 与 `client_room_versions_capability()` 逐字段一致（当前应含 `"12": "stable"`）。
- **风险**：低。仅测试数据。但需按 CLAUDE.md 规则逐条 review 新快照，不得 `cargo insta accept` 盲接。

**A-2 产品决策落定（见 §5）**：Q1（"仅 v12"范围）、Q2（`CreateRoomConfig.room_id` 存废）、Q3（内部合成房间）、Q5（v13 声明）。
- **目标**：把会影响 A/B/C 阶段设计的决策冻结，避免中途返工。
- **依赖**：无。
- **验收判据**：决策以文字落进本文件 §5 或独立 ADR。
- **风险**：若 Q1 选"连 join/federate 一起关"，阶段 G 的波及面显著变大（`can_join_room_version`/`can_federate_room_version` 的消费点、联邦 join 路径、`ck_rooms_room_version_valid` 的意义）。

**A-3 v12 一致性 fixture 与 oracle 骨架**
- **目标**：为 MSC4291/MSC4307 建立"上游可复算"的固定向量（沿用 `tests/unit/u13_interop_fixture_tests.rs` 的 oracle 模式，新增 v12）。
- **触及模块**：`tests/unit/u13_interop_fixture_tests.rs`、`tests/unit/fixtures/`（新增 v12 fixture）。
- **依赖**：无（本项只加测试资产）。
- **验收判据**：新增 v12 fixture 至少覆盖：create PDU（无 `room_id`、无 `event_id`）→ reference hash → event id → room_id（`$`→`!`）；含 `room_id` 的 create PDU 必须被判非法。
- **风险**：上游 Synapse 的 v12 oracle 是否可离线复算【待核验】；若不可，退化为"本仓自洽 + 规范文字推演"，必须在文档里标注该限制。

---

#### 阶段 B —— MSC4307（规则 3.5）与入站鉴权入口

**B-1 建立入站事件鉴权入口（单一实现）**
- **目标**：新建一个按 room_version 分派的鉴权检查函数/模块（铁律 2：不得出现第二份）。它接收 `(room_version, event PDU, auth_events 解析结果, 房间状态)`，返回 accept/reject。
- **触及模块**：新模块（建议 `synapse-federation/src/event_auth/rules.rs`），由 `synapse-web/src/routes/federation/transaction.rs` 调用。
- **依赖**：无（结构先行，规则可先只有 v1–v11 的恒真分支 + v12 的真规则）。
- **验收判据**：入口存在且被 transaction 路径调用；有"入口被调用"的用例（避免建了不接线，重演 G-38 的死代码模式）。
- **风险**：**高**——本仓历史上出现过"实现了但零调用者"（`resolve_state_v2`、`StateResolutionService`）。必须用铁律 8 的方式自证：故意让某条 v12 规则失败，观察集成用例变红。

**B-2 规则 3.5：`auth_events` 中每个事件的 `room_id` 必须与当前事件一致**
- **目标**：实现并接线。
- **触及模块**：B-1 的模块；`transaction.rs:349-355`（收集 auth_events 后校验）。
- **依赖**：B-1。
- **验收判据**：单测（同房通过 / 跨房拒绝）+ 真 baseline DB 往返用例（AGENTS R8 第 4 条）；变异自证（把 room_id 比较改成恒真 → 用例必须红）。
- **风险**：**auth_events 的 room_id 需要额外读库**（当前只存 ID）；批量读取需注意 N+1 与 SQLx 静态化（R1/R2）。跨房 auth_events 在 v1–v11 是否也应拒绝，需与上游一致【待核验】，默认只在 v12 生效以免影响既有房间。

---

#### 阶段 C —— MSC4291 创建侧 + 无域名 room ID 语法收敛

**C-1 create 事件身份 finalize（G-08）**
- **目标**：房间创建路径产生的事件 ID 对 v3+ 走 reference hash；v1/v2 保持 legacy。
- **触及模块**：`synapse-services/src/room/lifecycle/create_events.rs:34`、`synapse-services/src/room/lifecycle/create_events.rs:23-57`、`synapse-services/src/graph_metadata.rs:490-511`（`create_event_with_graph` 旁路是否改为"仅对 create/初始状态事件 finalize"需设计）、`synapse-common/src/event_id.rs`。
- **依赖**：A-3（fixture）。**这是 C-2 的硬前置。**
- **验收判据**：v11/v12 房间的 create 事件落库 ID 形如 `$` + 43 字符 urlsafe base64（`ck_events_event_id_format` 已放行，G-47）；v1/v2 仍为 `$<ts>$<rand>:<server>`；真 baseline DB 往返通过。
- **风险**：**高**。`create_event_with_graph` 同时服务**入站联邦**与 backfill（注释 `graph_metadata.rs:329-333` 明确"必须对 origin 的图数据保持字节忠实"）—— 不能简单地在装饰器里统一 finalize，否则会把远端 PDU 的既有身份重算。设计上必须区分"本地创建"与"入站持有 origin 图数据"两个调用族。

**C-2 room_id 推导（`$`→`!`）与创建流程重排**
- **目标**：先定稿 create 事件全部字段（含 content、signatures、hashes）→ reference hash → event id → room_id；再写 rooms/events 行。
- **触及模块**：`synapse-common/src/event_id.rs`（新增唯一 helper）、`synapse-services/src/room/lifecycle/create.rs:26`、`:509-511`、`synapse-services/src/room/lifecycle/create_events.rs:23-57`。
- **依赖**：C-1。
- **验收判据**：给定固定输入，room_id 与 fixture 一致；`rooms` 行 room_id == create 事件 event_id 的 `!` 形态；重复创建同内容【注意】：create 含 `origin_server_ts`，天然唯一。
- **风险**：**高**。创建事务内 create 事件必须在写 `rooms` 行**之前**定稿，而 `ck_rooms_room_id_format`（G-20）此时未必已放行 ⇒ C-3 必须与 C-2 同批（或 C-3 先行放行语法）。

**C-3 无域名 room ID 的语法收敛（G-17..G-23、G-20）**
- **目标**：`!` + 43 字符 urlsafe base64（v12）与 `!opaque:server`（v1–v11）**同时**被接受；校验按"房间版本"而非"是否含冒号"分派。
- **触及模块**（按依赖顺序）：
  1. `synapse-common/src/validation.rs:62`（正则按版本分派或放宽为"`!` + `[A-Za-z0-9._=+/-]+` 可选 `:server`"）、`:326`（回落正则）、`:182-190`、`:380-385`；
  2. `synapse-web/src/routes/validators.rs:49-73`（**110 个调用点 / 18 文件**需确认哪些拿到 room_version；无版本的入口只能做"形态宽松"校验）；
  3. 6 处联邦硬校验（G-19）；
  4. `migrations/00000000_unified_schema_v12.sql:4694-4697`（改为"两种形态任一"或删除该 CHECK）、重算 `EXPECTED_BASELINE_FINGERPRINT`（G-45）；
  5. `synapse-services/src/room/membership/service.rs:205-224` + `actions.rs:41`/`:161`（G-21：无冒号时不得把远端房当本地）；
  6. `synapse-web/src/routes/federation/membership/invite.rs:277`（G-22）、`synapse-storage/src/space/repository.rs:30`（G-16）。
- **依赖**：无（可与 C-1/C-2 并行，但 C-2 需要它先行落 DB/校验）。
- **验收判据**：
  - `validate_room_id("!" + 43 字符)` 通过、`"!room:server"` 仍通过、`"!x"`（短且无冒号）仍拒绝；
  - DB 层插入无冒号 room_id 成功（真 baseline 往返，AGENTS R9）；
  - 6 处联邦入口对无冒号 room_id 返回非 400；
  - `grep -rn "room_id.contains(':')"` 生产代码 0 命中；
  - 每个新门禁按铁律 8 自证能变红（放一个无冒号 id 的探针）。
- **风险**：**高**。① 110 个调用点里有些拿不到版本（如 `pinned.rs`、`relations.rs`），只能放宽为"形态校验"，语义校验下沉到 service；② 放宽容器可能让非法 id 通过到 DB/联邦层，需以"收紧到语法正则"而非"删除校验"来抵消；③ DB CHECK 放宽后，v1–v11 的 `!opaque:server` 与 v12 的 `!hash` 都在同一列，破坏了"room_id 可反推 server"的隐含不变量（G-21/G-22 只是其中两处显式体现）。

**C-4 创建侧不再写 `predecessor.event_id`；升级顺序反转（G-14、G-30）**
- **目标**：v12 升级：定稿新房 create 事件（`predecessor.room_id` only）→ room_id → 旧房 tombstone（`replacement_room` = 新 room_id）。
- **触及模块**：`synapse-services/src/room/service.rs:494-560`、`:549-554`；`synapse-web/src/routes/handlers/room/management/upgrade.rs`；测试 `tests/integration/room_service_tests_migrated.rs:782-841`。
- **依赖**：C-2。
- **验收判据**：v12 升级后新 create content **不含** `predecessor.event_id`；tombstone 的 `replacement_room` 与新 room_id 一致；v1–v11 升级行为保持（若仍允许）。
- **风险**：中。旧房 tombstone 与新房间的写入顺序变化会影响"两面都失败"的原子性（当前是 tombstone 先行、失败即中止）。需确认跨房间事务边界与失败回滚语义。

**C-5 `CreateRoomConfig.room_id` 逃逸口的处置（G-15、G-16）**
- **目标**：按 A-2/Q2 决策删除或严格限域。
- **触及模块**：`synapse-services/src/room/service.rs:76`、`create.rs:24-26`、`synapse-web/src/routes/admin/notification.rs:350-355`、`synapse-storage/src/space/repository.rs:30`。
- **依赖**：A-2、C-2。
- **验收判据**：若删除：`grep RoomId` 中无"调用方提供 room_id"的路径；内部合成房间改走统一建房入口（或明确记录为"非 v12 语义的本地合成房间"）。
- **风险**：中。server-notice / space 是既有功能，改动面在 admin 与 space 两个域；若选择"保留但记录例外"，必须写清谁保证其 ID 形态（AGENTS R4 的同类要求）。

---

#### 阶段 D —— MSC4291 入站侧（规则 1.2 / 2 / 2.5）+ 出站形态

**D-1 规则 1.2：create 带 `room_id` 则拒绝**（v12）
- **目标**：入站 v12 create PDU 含顶层 `room_id` ⇒ 拒绝。
- **触及模块**：B-1 的模块；`transaction.rs:681-704`（当前强制要求 room_id，需按 `type == m.room.create && version == 12` 分支放行并推导）。
- **依赖**：B-1、C-3。
- **验收判据**：含 `room_id` 的 v12 create 被拒；不含的被接受且房间身份由 event id 推导。
- **风险**：中。`validate_inbound_transaction_pdu` 是 `transaction.rs` 的核心前置（G-25），改它会牵动 `results[]` 错误形态与既有测试。

**D-2 规则 2：room_id 必须是"已被接受的 create 事件 ID（`$`→`!`）"**
- **目标**：非 create 事件的 `room_id` 必须能对应到一个已接受 create 事件。
- **触及模块**：B-1 的模块（需要 room_id→create event id 的反查，或用 `!`→`$` 后查 events 表）。
- **依赖**：D-1、C-2。
- **验收判据**：伪造 room_id（`!` + 随机 43 字符）被拒；真 room_id 通过。需要 DB 往返用例。
- **风险**：**高**。反查是每个入站事件一次额外查询（性能）；且 v1–v11 房间的 room_id 无法用同一规则，必须按版本分派。需要缓存策略，但不能演变成第二份"房间存在性"实现（铁律 2）。

**D-3 规则 2.5 / MSC4307 的 v12 收敛**
- **目标**：与 B-2 合并为同一实现，v12 下强制。
- **依赖**：B-2、D-1。
- **验收判据**：与 B-2 共用用例，v12 打开、v1–v11 行为不变。

**D-4 本地 `auth_events` 不再包含 create（G-28）**
- **目标**：`auth_types_for_event` 按版本去掉 `m.room.create`。
- **触及模块**：`synapse-services/src/room/state/auth_events.rs:106-116`；`creation_graph.rs`；`auth_events.rs:216-229` 测试；`creation_graph.rs:126-129` 断言。
- **依赖**：C-2（create 的 event id 语义确定后）。
- **验收判据**：v12 下任意事件的 `auth_events` 不含 create event id；v11 行为不变（或用例显式分版本）。
- **风险**：中。`creation_graph` 的线性假设与断言会变；且"auth_events 不含 create"与"状态决议仍需 create"之间的边界需明确（见 D-5）。

**D-5 裁定 auth chain / auth difference 是否含 create（G-29）**
- **目标**：给出结论并统一 `is_auth_chain_member`、状态决议的 create 处理。
- **依赖**：D-4、阶段 F。
- **验收判据**：结论写入代码注释 + 测试；若变更类型清单，需有"变更后仍满足规范定义（传递闭包）"的论证。
- **风险**：**高**。这是规范语义题，本轮无法联网核实 MSC4291 对 auth chain 的精确措辞【待核验】。

**D-6 出站 create PDU 省略 `room_id`（G-10）**
- **目标**：`build_pdu` / `state_pdu` 对 v12 `m.room.create` 不写 `room_id`；签名材料与联邦广播同步。
- **触及模块**：`synapse-common/src/pdu.rs:80-89`、`synapse-web/src/routes/federation/pdu.rs:95-145`、`synapse-federation/src/signing.rs:211-222`。
- **依赖**：C-2。
- **验收判据**：v12 create 的出站 PDU 无 `room_id`；对端（oracle fixture）能复算 event id/room id；本仓入站能接受自己的 PDU（往返用例）。
- **风险**：中。`state_pdu` 的 `room_version: Option<&str>` 在版本读不到时保留遗产形态（现有设计），需确认 create 房间尚未落库时版本可从 content 取得（`transaction.rs:727-735` 已是这个逻辑）。

---

#### 阶段 E —— MSC4289 创建者特权

**E-1 create content 的 `additional_creators` 校验（规则 1.4）**
- **目标**：必须为字符串数组；每个元素通过与 sender 相同的 user ID 校验；否则拒绝。
- **触及模块**：`synapse-services/src/room/lifecycle/create.rs:509-511`（写入侧）、入站校验侧（B-1 的模块，规则 1.4 属 create 事件鉴权）、`synapse-web/src/routes/handlers/room/management/create.rs:181-185`（是否允许客户端通过 `creation_content` 传 `additional_creators`，需与 Q2 一并裁定）。
- **依赖**：B-1、C-2。
- **验收判据**：非数组 / 非字符串元素 / 非法 user ID ⇒ 拒绝；合法 ⇒ create content 落库且创建者集合 = sender ∪ additional_creators。
- **风险**：中。写入侧与校验侧必须共用同一校验函数（铁律 2）。

**E-2 创建者集合 + 无限 PL（G-31、G-32、G-33、G-35）**
- **目标**：`resolve_room_creator` → `resolve_room_creators`（集合）；`get_user_power_level` 对创建者返回"无限"（哨兵或显式分支），且 PL 事件无法降权。
- **触及模块**：`synapse-services/src/auth/power_levels.rs:7-40`（`:36` 的 `Ok(100)`）、`:44-56`（`get_joined_user_power_level`）、`:73-93`（`resolve_room_creator`）、所有以数值比较的下游（`verify_room_admin` `:355`、`can_kick_user` `:368`、`can_ban_user` `:425`、`can_unban_user` `:483`、`can_redact_event` `:550`）。
- **依赖**：E-1。
- **验收判据**：PL `users: {creator: 0}` 被构造出来后，创建者仍能执行 100 级操作；创建者不出现在 `users` 里；非创建者数值语义不变。**必须**有一个"把无限 PL 降级为 100 后用例变红"的变异自证。
- **风险**：**高**。① 哨兵（`i64::MAX`）会参与现有算术/比较，需全量审计（`power_levels.rs` 972 行 + 其它消费点）；② `can_kick_user` 有"创建者不可被踢"的既有特例测试（`power_levels.rs:856` 区域），与"无限 PL"可能重复或冲突；③ 创建者集合意味着多处签名从 `Option<String>` 改为集合，波及面比单点大。

**E-3 规则 10.4：power_levels 的 `users` 不得含创建者（G-34）**
- **目标**：`verify_power_levels_change`（及入站对应检查）在 v12 下拒绝此形态。
- **触及模块**：`synapse-services/src/auth/power_levels.rs:196-215`；入站侧在 B-1 的模块。
- **依赖**：E-2。
- **验收判据**：含 create sender 或任一 additional_creator 的 PL 事件被拒；v1–v11 不变。
- **风险**：中。当前建房时**自己**写入 `power_levels.users = {creator: 100}`（`create.rs:137-142`）⇒ 若规则 10.4 在创建序列内也生效，**建房会自拒**。必须把"创建序号内的首个 PL 事件"设计为特殊路径（规范上 v12 的初始 PL 不应包含创建者）。

---

#### 阶段 F —— MSC4297 state resolution v2.1

**F-1 状态决议的接线决策（G-38）**
- **目标**：先裁定"是否把状态决议接入服务路径"，因为它当前**零调用者**——这是比"实现 v2.1"更大的问题。
- **触及模块**：`synapse-federation/src/state_resolution.rs`（`StateResolutionService`）、`synapse-web/src/routes/federation/mod.rs:13`（re-export）。
- **依赖**：A-2。
- **验收判据**：给出书面结论：要么接线（并说明在哪个入站/room-join 路径调用），要么按铁律 1 删除死实现。
- **风险**：**高**。若选择接线，等于新增一条从未在生产路径运行过的算法，回归面极大；若选择删除，则"v12 的 v2.1"落地方式需要重新定义（可能只在 `resolve_state_with_auth_chain` 上演进）。**这是我建议单独决策、不要与其它项捆绑的一项。**

**F-2 v2.1 三处修改（G-36、G-37、G-39）**
- **目标**：① iterative auth checks 从**空 state map** 开始；② 新增 conflicted state subgraph（auth_events 路径并集，含端点）；③ full conflicted set = conflicted ∪ subgraph ∪ auth difference。
- **触及模块**：`synapse-federation/src/event_auth/state_resolution.rs:367-508`（`resolve_state_v2`）、`:284-310`（`calculate_auth_difference`，当前仅测试调用）、`:106-160`（若走演进路线）。
- **依赖**：F-1、E-2（auth check 需要创建者语义）、D-4（auth_events 形态确定）。
- **验收判据**：新增 v2.1 测试向量（上游 implementer's guide【上游】+ 自制冲突图）；三个改动各有一个"退回 v2 行为即失败"的用例。
- **风险**：**高**。① 该函数当前无调用者，任何行为改动都缺少端到端证据；② `sort_by_reverse_topological_power` 的 power 提取（`:312-364`）与 mainline 计算（`chain.rs:160`）都是近似的，v2.1 的 subgraph 定义要求精确的 `auth_events` 图遍历；③ 性能门禁（G-51）会随复杂度变化。

**F-3 v1–v11 的兼容边界**
- **目标**：明确 v2.1 只对 v12 生效，还是替换全局。
- **依赖**：F-2。
- **验收判据**：按版本分派的显式测试；若全局替换，需要 v1–v11 的回归向量。
- **风险**：**高**（铁律 1 说"无向后兼容义务"，但 v1–v11 仍是可 join/federate 的房间，行为变更会改变既有房间的状态裁决结果）。

---

#### 阶段 G —— "只允许 v12"（`can_create` 收敛）

**G-1 能力表收敛**
- **目标**：`can_create: false` for v1–v11 与 v13；仅 v12 为 `true`。
- **触及模块**：`synapse-common/src/room_versions.rs:92-115`（改用类似 `stable_parse_only` 的新构造，或为 v1–v11 新增"不可创建"构造）、`resolve_room_version`（`:160-167`）。
- **依赖**：C、D、E 完成（v12 行为正确）——**否则是把过度声明换成"过度声明 + 不可用"**。
- **验收判据**：`resolve_room_version(Some("11")) == None`；`resolve_room_version(None) == Some("12")`；`client_room_versions_capability().available` 只含 `"12"`。
- **风险**：中。

**G-2 连带面清单（**实测修正了任务书的预期**）**
- **`/capabilities`**：由 `capability_governance.rs:485` → `client_room_versions_capability()` 驱动，随能力表自动收敛。**无独立改动**。
- **`/versions`**：**实测不含房间版本**（G-06）⇒ **不在连带面内**。
- **`m.room_versions` 能力表**：`room_versions.rs:92-115`、`:170-196`。
- **联邦 `m.room_versions`**：`federation/mod.rs:263`、`federation/events.rs:407-421` ⇒ 若 `can_federate` 一起关，此处会变；且**当前无断言**（G-05）⇒ 本阶段应补测。
- **守卫测试**：`room_versions.rs:207/212/224/239/262/289`（`:248`、`:252`、`:269-270`、`:293` 必改）；`tests/integration/api_auth_routes_tests.rs:158-213`（自洽，随能力表自动通过）。
- **快照**：`capabilities_v3.snap`（A-1 已处理，本阶段需再更新 `available` 为仅 12）。
- **route ledger / SDK 契约**：**实测不需要动**（G-07），除非本阶段新增/删除路由。
- **集成测试**：G-48 的 10 处非 12 建房用例必须迁移到 v12 或改为"断言非 12 不可创建"。
- **风险**：中。真正的风险不在能力表，而在"收敛后 v1–v11 房间是否还能 join/federate"（Q1）。注意**区分**：
  - **"仅禁止创建"**：v1–v11 仍 `can_join/can_parse/can_federate = true` ⇒ 既有联邦房间不受影响，只需改 `can_create` 与其守卫/快照/用例。
  - **"完全不支持"**：需同时关 `can_join/can_federate`，并处理 `federation/membership/mod.rs:47` 的 `can_federate_room_version` 拒绝路径（会让 v1–v11 联邦 join 直接 400 `M_INCOMPATIBLE_ROOM_VERSION`）；`ck_rooms_room_version_valid`（G-46）的存在理由随之消失。
- **验收判据（若仅禁创建）**：联邦 join 一个 v11 远端房间仍成功；本仓建房只产出 v12。

---

#### 阶段 H —— 文档连带面

**H-1** 按 §2.12 表格逐份更新（含额外发现的 6 份）。**注意**：`V12_ROOM_VERSION_AND_ANIMATED_THUMBNAIL_IMPLEMENTATION_PLAN.md` 的更正已在 `d3a12ca73`（分支 `docs/room-version-12-13-correction`）——**本阶段不要改它**，合并后不冲突但需与本计划的 §2.12 结论对齐。
- **依赖**：G（能力面定稿后写才不返工）。
- **验收判据**：`grep -rn "MSC4239" docs/ | grep -i "v12"` 不再把 MSC4239 当 v12 定义；每份被改文档标题下有一行状态（TRUE/已反转）。
- **风险**：低。仅文本。

**H-2** 决策落档：把 §5 的 Q1–Q7 结论写入相应文档（尤其 Q1 需要写进 `AGENTS.md` 纪律所要求的"capability 与行为一致"论证）。
- **风险**：低。

---

### 3.3 关键路径与并行建议

- **关键路径**：A-1 → A-3 → C-1 → C-2 → (C-3 ‖ C-4 ‖ D) → E → F → G → H。
- **可并行**：A-1（快照）与 A-3（fixture）无关；C-3（语法收敛，横跨 route/storage/DB）可以在 C-1/C-2 期间由另一人在独立 worktree 推进，但它与 C-2 共享 DB 迁移 ⇒ **迁移文件必须一个写者**（AGENTS 铁律 9）。
- **建议拆分 worktree**（铁律 9 + `CARGO_TARGET_DIR` 不得跨树共享）：
  - WT-1：A + C + D（MSC4291 主线，含迁移）
  - WT-2：B（MSC4307 + 鉴权入口）
  - WT-3：E + F（创建者特权与状态决议）
  - WT-4：G + H（能力收敛与文档）
- **B 与 E 有接口耦合**（鉴权入口要被 E 填充），建议 WT-2 先落入口壳，WT-3 以 patch 方式填规则。

---

## 4. 风险与测试策略

### 4.1 最大风险（排序）

1. **"实现了但没有调用者"是本仓的既有模式**（实测 G-36/G-38：`resolve_state_v2` 与 `StateResolutionService` 双零调用者；`Validator::validate_room_id` 生产零调用者）。MSC4297 改造的最大风险不是算法写错，而是写完后**依然不接线**。⇒ 每一项都必须有"端到端被调用"的证据，不能只有单元测试。
2. **入站鉴权引擎从零开始**（G-24）：规则 1.2/2/2.5/3.5/10.4 都需要它。这是一个**新子系统**，而任务书把它描述为"规则接线"。工作量与回归面被低估的可能性最大。
3. **创建路径事件身份 finalize 与"入站字节忠实"需求冲突**（G-08 的 `create_event_with_graph` 双职）。改错会静默改变入站联邦事件的 ID。
4. **DB CHECK 与 22 个 FK**（G-20/G-44）：`ck_rooms_room_id_format` 是硬阻塞；放宽后"room_id 可反推 server"的隐含不变量在全仓至少 2 处被显式使用（G-21/G-22），其余是隐式的。
5. **110 个 `validate_room_id` 调用点**（G-17）：多数入口拿不到 room_version，只能做形态校验 ⇒ 语义校验下沉 service，容易漏。
6. **能力收敛掩盖行为未实现**：若先做阶段 G，会得到"只声明 v12，但 v12 仍不符规范"的更差状态（且测试全红）。
7. **`i64::MAX` 无限 PL 的数值污染**（G-35）：972 行的 `power_levels.rs` 及下游大量数值比较。
8. **v13 声明与实现冲突**（G-50）：`stable_parse_only("13")` 但红act/event id 都不认 13。若本轮统一清理能力表，必须一并裁定，否则"能力表与实现一致"这条纪律只对 v12 成立。

### 4.2 不确定处（明确标注，不编）

- MSC4291 对 **auth chain / auth difference 是否仍含 create** 的精确措辞（G-29 / D-5）【待核验】。
- `content.creator` 在 v11/v12 create content 中是否应缺席（G-13）【待核验】。
- 规则 3.5 是否也应约束 v1–v11（默认只在 v12 生效是保守选择）【待核验】。
- 上游 Synapse 是否有可离线复算的 **v12** oracle（A-3）【待核验】。
- 状态决议在本仓"无调用者"是否意味着入站冲突状态实际上由**存储层覆盖**决定（`synapse-storage/src/state_groups.rs:361 resolve_state_for_group` 是另一条路径）—— 本轮未展开，需在 F-1 决策前查清。

### 4.3 测试策略

- **TDD**（CLAUDE.md 强制）：每个行为改动先写失败用例；`cargo nt -p <crate> <test> -P tdd`。
- **oracle 互操作**：沿用 U-13 的"上游 Synapse 复算 fixture"机制，新增 v12 fixture（create PDU 无 room_id/event_id → hash → room_id；含 room_id 的 create 必须被拒）。
- **真 baseline DB 往返**（AGENTS R8 第 4、R9）：room_id 语法收敛、auth_events 同房校验、创建者无限 PL 都必须有 DB 级用例，禁止只写纯构造用例（本仓历史上"纯构造通过、真 schema 失败"已发生）。
- **门禁自证能变红**（铁律 8 / R11）：每一项新门禁（无冒号 room_id 探针、跨房 auth_events 探针、PL 含创建者探针、把无限 PL 改成 100 的变异）都必须实际观察到红，并记录实验命令。
- **SQLx（R1–R13）**：本计划会新增/修改大量查询（auth_events room_id 批量读取、room_id→create 反查、rooms 插入）。每批必须跑 R8 的四道门禁 + `check_sqlx_cache_fresh.sh --compile`；不得使用 `--full`。
- **迁移连带**（R10）：改 `ck_rooms_room_id_format` ⇒ 重算 `EXPECTED_BASELINE_FINGERPRINT`（G-45）+ 跑 `test_isolation_unification` + 同步所有断 schema 的契约用例 + R8 第 2/3 条。
- **快照**：A-1 先修绿；G 阶段再更新一次；`INSTA_UPDATE=no` + `.snap.new` 检查。
- **两档 clippy**：`--all-features` 那档不可省（历史事故：测试没在 `--all-features` 下编译过）。
- **性能门禁**：F 阶段需重跑 `benches/performance_federation_benchmarks.rs` 相关项与 perf gate 用例（G-51）。

---

## 5. 需要产品决策的点（Open Questions）

| # | 问题 | 选项 | 影响 | 建议（供讨论，不是结论） |
|---|---|---|---|---|
| **Q1** | "只允许 v12"的范围 | (a) 仅禁止**创建** v1–v11（join/parse/federate 保持）；(b) 完全不支持 v1–v11（join/federate 也关） | (a) 只动 `can_create` 与其守卫/快照/10 个建房用例；(b) 还要动 `can_join/can_federate`、联邦 join 拒绝路径（`federation/membership/mod.rs:47`）、`ck_rooms_room_version_valid` 的存在理由，且既有联邦 v11 房间不可 join | 倾向 (a)：本仓铁律只说不承担**向后兼容**，而"能 join 别人的 v11 房间"是**互操作**而非兼容层；(b) 会让本仓无法进入大量现存联邦房间 |
| **Q2** | `CreateRoomConfig.room_id` 抢占式逃逸口 | (a) 删除（v12 下不可能预知 room_id）；(b) 保留但仅供内部合成房间 | 涉及 `admin/notification.rs`（server notice）与 `space/repository.rs`（space 房间）；这些"房间"的 create 事件若有，其 ID 目前是伪造的 `$uuid:server` | 倾向 (a) + 把内部合成房间改为走统一建房入口；若成本过高则 (b) 必须写明"谁保证这些 ID 的形态" |
| **Q3** | 内部合成房间（server notice / space）是否纳入 v12 语义 | (a) 纳入（走真 create 事件）；(b) 明确记录为"本地合成房间，非 v12 协议房间" | (a) 改造成本；(b) 会留下"同一 `rooms` 表里两种语义"的长期不一致 | 需要产品判断 server notice 是否需要联邦可达 |
| **Q4** | 收敛 `can_create` 后 `/capabilities.available` 只列 v12，客户端（Element）行为变化是否接受 | (a) 接受；(b) 同时声明 `can_join` 的版本以另行告知 | 只列 v12 会让客户端在"建房"UI 里只看到 12；影响面小但用户可感知 | 倾向 (a)，并在文档里写明这是规范 SHOULD 的方向 |
| **Q5** | `stable_parse_only("13")` 与实现冲突（G-50） | (a) 保留声明并补齐 13 的红act/event id；(b) 从能力表移除 13；(c) 保留并加注释说明 13 目前只用于"数据库列可存放" | 现今 `redaction_rules("13") == None`、`uses_reference_hash_event_id("13") == false`，"可 parse"是假的 | 倾向 (b) 或 (c)，但需与 MSC4304 之后的 v13 路线图对齐【待核验 v13 的上游定义】 |
| **Q6** | 状态决议（MSC4297）落地方式 | (a) 把 `StateResolutionService` 接线进服务路径并实现 v2.1；(b) 不接线，按铁律 1 删除死实现，只在 `resolve_state_with_auth_chain` 上演进；(c) 另起一份实现 | (a) 新增一条从未跑过的关键路径；(b) 需要重新定义"v2.1 落地"的验收；(c) 违反铁律 2 | **必须单独决策**。当前"零调用者"这一事实本身需要先解释（`state_groups.rs:361` 是否是实际使用的另一条路径）——本轮未查清【待核验】 |
| **Q7** | 是否允许客户端在 `creation_content` 传 `additional_creators` | (a) 允许（则 handler 需停止剥除该键）；(b) 仅服务端/应用服务可设置 | 影响 MSC4289 的写入侧边界与 `handlers/room/management/create.rs:181-185` 的黑名单 | 需要产品判断；若 (b)，需定义可信来源 |
| **Q8** | 本计划的 3.2 阶段顺序是否接受（尤其把 MSC4307 提前、把"只允许 v12"推后） | — | 见 §3.1 的六条理由 | 若坚持任务书原序，请特别确认"入站鉴权入口从零建"这一事实（G-24）被纳入 ①②的工作量 |

---

## 附：本轮新增发现（任务书未列，均带证据）

1. **G-08 房间创建路径用 legacy 事件 ID，且装饰器对 `create_event_with_graph` 是旁路** —— MSC4291 创建侧的隐藏前置阻塞。`create_events.rs:34` + `graph_metadata.rs:490-511`。
2. **G-20 `ck_rooms_room_id_format` 是硬阻塞**（无冒号 room_id 触发 23514）。`migrations/00000000_unified_schema_v12.sql:4694-4697`。
3. **G-04 `/capabilities` 快照当前已是红项**（钉 `default: "11"`、无 `"12"`）。`...capabilities_v3.snap:29-44`。
4. **G-36/G-38 状态决议双死代码**：`resolve_state_v2` 零调用者；`StateResolutionService` 零调用者 ⇒ 本仓状态决议**未接线**，比"没实现 v2.1"更严重。
5. **G-24 不存在入站 event-auth rules 引擎** ⇒ 规则 1.2/2/2.5/3.5/10.4 需要新建子系统，而非"补分支"。
6. **G-21 无冒号 room id 会让远端房间被判定为本地**（`server_name_from_id` 返回 `None`）。`membership/service.rs:205-224`。
7. **G-22/G-16 用 room_id 反推 server 的两处拼接在 v12 下产出畸形 ID**（`$uuid:!xxx`、`!server_notice_...`）。`invite.rs:277`、`admin/notification.rs:350-355`、`space/repository.rs:30`。
8. **G-50 v13 声明与实现冲突**：`stable_parse_only("13")` vs `redaction_rules("13") == None` / `uses_reference_hash_event_id("13") == false`。
9. **G-52 `RoomsConfig`（含 `default_room_version`）整段被块注释**（`config/mod.rs:1670`–`:1725`）⇒ 是死代码文本，不构成房间版本默认面的第 4 个 surface（纠正并行审计的"死字段"表述）。
10. **G-06/G-07 任务书预期的两个连带面实际不在**：`/versions` **不发**房间版本；route ledger / SDK 契约**零**房间版本引用 ⇒ 只有增删路由才需要动 `SCHEMA_VERSION`。
11. **G-05 联邦 `/version` 的 `m.room_versions` 完全无断言**（`api_federation_tests.rs:138` 只查 `server.version`）。
12. **G-48 约 10 处集成用例以 v9/v10/v11/v1 建房**，收敛 `can_create` 会直接打红：`room_service_tests_migrated.rs:279/751/807/866/4989/5061/5098`、`federation_existence_leak_tests.rs:567/680/735/863`。
