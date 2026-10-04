# synapse-rust API 覆盖率分析 (v1.10)

> **状态（2026-09-29）**：房间版本能力（v12/v13）记录**已于本版订正** —— 现**仅 v12 可创建**（G-1，`7489b247f`），
> v1–v11 为 `stable_no_create`（可 join/parse/federate、不可创建）、版本 13 已移除（Q5(b)，`c83e3faf9`）；
> **遗留项订正（2026-10-02）**：**MSC4297（state resolution v2.1）已实现并接线到生产路径** —— 写入接缝
> （`MessagingService::create_event_with_graph`、联邦加入 state 批次）在房间分叉时经
> `resolve_forked_state` → `resolve_state_for_version_with_rules` 重算（v12+ 从空 state map 起步 = v2.1
> Modification 1），故 v12 **不再**「声明领先实现」（见 §5.5 #20185 等价实现、§5.6 L4）。
> 见 `docs/audit/ROOM_V12_PLAN_STATUS_2026-09-27.md`。

> **对齐基准**：element-hq/synapse **v1.162.0**（发布于 2026-09-29，当前最新稳定版）；上游 `CHANGES.md` 已核对 1.157→1.162 全部条目。
> Matrix Specification 基线：**v1.19**（上游 v1.162 release notes 引用 `spec.matrix.org/v1.19`）。
> **本次复核日期**：2026-09-29；本表全部取证的仓库 HEAD 为 `74bb9c522`（`git log -1`）。
> v1.6 的取证 HEAD 为 `9e26ee31a`（2026-09-25，分支 `opt/consolidated`）；其后路由注册面经 §1.1 末段列出的
> 三批提交（`88001b4a9` → `d4e22f9ea` → HEAD）推进，故本版三口径计数整体上移。
> **v1.5 与 v1.4 的差别**：v1.4 的「三种口径」表是 **2026-09-21 快照**（1151 / 931 / 811），
> 本轮按 §8 配方在 `9e26ee31a` 上重算全部三张分类表（§1.1 / §二 / §三），并把 §五/§六 的
> 判定推进到当前 HEAD（MSC4512、MSC3912 级联、MSC4140 联邦 EDU、Content Scanner 装配、
> `rc_reports` 限流、AS 登录均已落地；两处上游条目判据见 §5.1/§5.2）。
> **v1.6 与 v1.5 的差别**：2026-09-25 的 **E2EE 去服务端私钥重构**删除了服务端侧 SAS / 设备信任的实现
> （`verification_routes` 24 条 + `e2ee` 路由组 6 条），路由三口径随之由 **1165 / 933 / 813** 变为
> **1135 / 903 / 795**，Client 分类表在「房间」「设备与密钥」两行相应下调；重算时**顺带修正**了 v1.5
> 分类表中「打印的配方复现不出打印的表」的 ±5 归类偏差（详见 §二 后的增量复核注）。
> **v1.7 与 v1.6 的差别**：v1.6 停在 HEAD `88001b4a9`（2026-09-25），此后路由注册面净增 **+14 注册条目
> / +10 唯一路径 / +10 逻辑端点**（1135/903/795 → **1149/913/805**）：**新增** profile 自定义字段稳定端点
> `GET/PUT/DELETE /_matrix/client/v3/profile/{user_id}/{key_name}`（+3/+1/+1）、**Admin 媒体端点族 10 条**
> （+10/+10/+10，见 §6.3）、`POST /_synapse/admin/v1/invite/{allowlist,blocklist}`（+2 注册条目，唯一路径此前已有 GET）；
> **删除**上游 1.161 已移除的 `GET /_matrix/client/unstable/org.matrix.msc2965/auth_issuer`（−1/−1/−1，`76e5f9136` 有意摘除，见 §5.1/§7-B4）；
> **迁移** CAS 协议 6 条由根级移入 `/_synapse/cas/`（净 0 条目，仅改路径分布）。逐项归因见 §1.1 末段。
> 此外，本版**订正**了 v1.6 遗留的过时记录：§5.1 / §6.2 / §7-B5 中「默认版本仍为 11 / v12·v13 不可创建」
> 已不成立 —— 现 `DEFAULT_ROOM_VERSION = "12"`（`room_versions.rs:94`）、**仅 v12 可创建**（G-1，`7489b247f`）、
> v13 已移除（Q5(b)），遗留项仅 MSC4297（state resolution v2.1，见 `docs/audit/ROOM_V12_PLAN_STATUS_2026-09-27.md`）。
>
> **v1.8 与 v1.7 的差别**：本版**只做对齐基准升级与增量判定，不重算三口径**（§1.1 的 1149 / 913 / 805 仍为
> v1.7 于 HEAD `74bb9c522` 的实测值；本批 L1 新增 1 条已注册路由（同路径已有 `POST`，故唯一路径/逻辑端点不变），
> 重算仅「注册条目」`+1` 得 **1150 / 913 / 805**，见 §1.1 末段）。变更：
> ① 顶部**对齐基准由 v1.161.0 升至 **v1.162.0**（2026-09-29），`CHANGES.md` 核对范围扩到 **1.157 → 1.162**；
> ② §五 标题与 intro 同步，**新增 §5.4 / §5.5**「v1.161 → v1.162 增量逐条判定」（Features 4 / Bugfixes 11 /
> Docs 5 / Internal 13，共 **33** 条；v1.162.0 正文为 "No significant changes since 1.162.0rc1"，条目全来自 1.162.0rc1）
> 与 **§5.6**「本批 M1–M6 / L1–L5 处理结果」收口表；
> ③ **订正两处旧判**：§5.1 MSC4140 行（`GET /delayed_events/{delay_id}` 单事件端点已落地 handler + 路由，
> `state_key` 行号由 `:94` 改为 `:123`）与 §5.2 MSC4222 行（原判「`state_after` = 0 / N/A」**与代码不符**，实际已支持，改判 PARTIAL）；
> ④ §八 复核命令同步 `ref=v1.162.0` 并新增 ⑥.1 关键事实核对清单（11 条 grep）。
> 判 N/A 的三项依据：M5 `soft_failed` 为事务去重专用、L2 `action_name` 全仓 0 命中、L3 `federation_domain_whitelist` 全仓 0 命中。
> **v1.9 与 v1.8 的差别**：本版随 `feature/2026-10-01-metrics-docs-updates` 合并进 `main`（2026-10-01）
> **重算了全部三张分类表**（§1.1 / §二 / §三），并同步 §八 配方里的注释值。取数 HEAD：合并后的 `main`
> （该分支 tip `e1ffcb2ab`，另加 MSC3720 账户状态两条路由）。三口径为 **1152 / 915 / 807**
> （v1.8 的 v1.7 口径 1149 / 913 / 805，叠加 L1 的 MSC4140 单事件端点 `+1 / 0 / 0`
> 与 MSC3720 `+2 / +2 / +2`）；本版**不重算**任何 MSC/功能判定（§五/§六 沿用 v1.8）。
>
> **v1.10 与 v1.9 的差别**：本版为 **2026-10-02 台账订正批**（随 D-4「M-5 提升为跨请求共享」与 D-5 收口，并含 D-6/D-7 两项订正），
> **不重算全部三口径**（唯一路径 915 / 逻辑端点 807 沿用 v1.9；**注册条目 1152 → 1154**，+2，见 ⑥；其后 **1154 → 1153**，−1，见 ⑦），余为判定口径订正：
> ① **MSC4297（state resolution v2.1）由「未实现 / v12 对该单项声明领先实现」订正为「已实现并接线到生产路径」**
> —— 写入接缝（`MessagingService::create_event_with_graph`、联邦加入 state 批次）在房间分叉时经
> `resolve_forked_state` → `resolve_state_for_version_with_rules` 重算（v12+ 从空 state map 起步 = v2.1
> Modification 1）；同步改**顶部状态注**、§5.1「v12/v13 房间可创建」行与 §6.2 行（原判据引自已被证伪的「本仓无状态决议路径」注释）。
> ② **§5.5 #20185** 由 **N/A 订正为「等价实现（已交付，M-5）」** —— 原「生产 0 调用点、仅 benches」判定**不实**，
> 实为生产路径上的冲突输入键控缓存（`ResolutionCache`，键＝`room_version`＋状态集合排序 `(key,event_id)` 投影
> ＋已加载事件 id 集合；进程内跨请求共享）；§5.6 L4 同步。
> ③ **§5.5 #20160** 由 **PARTIAL 收口为「维持等价（正式收口）」**（决策 D-5：整房 `room_state:{room_id}`
> 列表缓存 TTL 300 已属等价，不再新增 per-item 缓存）；§5.6 L4 同步。
> ④ **§5.4 #20182（D-7）** 由 **N/A 订正为「已对齐（等价实现）」** —— 原「本仓无 `recurse` ⇒ 不适用」判据已作废，
> M-6（`46fe964b9`）已交付 MSC3981 `recurse` 且 join 恰在递归 CTE 内（#20182 的修复形状）。
> ⑤ **§5.5 #20097 / §5.6 L4（D-6）** 明确 per-stream 口径：当时为**单 gauge（仅 `events` 一条、仅 admin `/statistics` 刷新）**，
> **per-stream / worker-local 完整口径（`{stream="…"}` 多标签）登记为独立批次 D-6b**；**2026-10-02 双线合流后**：口径统一为 `StreamPosition::ALL` 的 **6 条** series（`events` / `to_device` / `device_lists` / `sliding_sync` / `quarantined_media` / `worker_events`），数据源为单条 `UNION ALL`（`synapse-storage/src/stream_positions.rs`），由 `src/server/mod.rs` 的 **30s 指标循环**周期刷新（另一线的「`/metrics` 抓取时即时计算、2 条 series」实现已在合流时删除，避免双写同一指标）；另一线的 `to_prometheus_format()` family 去重修复随合流保留（见 §5.5 #20097、§5.6 L4）；#20133（MSC4242 serving）维持 D-3 已收口的「受阻待办」口径不变。
> ⑥ **§5.4 #20182 / §18.4 L-6（D-7/L-6）**：spec 的 4 段关系路由
> `GET /rooms/{roomId}/relations/{eventId}/{relType}/{eventType}` 已补齐（与同路径既有 `PUT` 合并进同一 `MethodRouter`，
> 第 4 段参数名取 `{event_type}`），`event_type` 过滤自此拥有 HTTP 入口；**注册条目 1152 → 1154**（`v1`/`v3` 各 `+1`），
> **唯一路径与逻辑端点不变**（该 4 段路径此前已由 `PUT` 占据）。
> 跨文档同批更正：`MSC_SEMANTICS.md` MSC4297 登记行、`synapse-rust-vs-synapse-comparison.md` §18.3 #17/#16 / §18.4 M-5/L-1 / §18.6 V-12、
> `docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md` M-5 执行卡 / L-1。
> ⑦ **§18.4 L-6 后续（关系写入端点拆分）**：spec 不定义关系写入（客户端应发带 `m.relates_to` 的普通事件），
> 原先 `PUT` 与 `GET` 共用同一 4 段字面路径（axum 路由层会归一化路径参数名，同一字面路径无法注册成两条 `.route()`），
> 于是第 4 段被迫叫 `{event_type}` 而 handler 当 `txn_id` 用。现按 ISSUE-13 把写入拆到
> `PUT /_matrix/vendor/v1/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}`，client 4 段路径只留 `GET`；
> `{txn_id}` 同时接上既有的 `room_event_txn_dedup` 耐久去重（此前只写日志，注释声称的幂等是假的）。
> 计数影响：`/_matrix/client` −2（v1/v3 的 PUT）、其他命名空间 +1（vendor PUT），**注册条目 1154 → 1153**；
> 唯一路径与逻辑端点口径未重算（本版沿用 v1.9/v1.10 的约定）。跨文档同批：`ROUTE_CONTRACT.md`、
> 6 份 ledger fixture、两条 route-ledger 快照、`docs/openapi/route-table.json`、`synapse-rust-vs-synapse-comparison.md`。
>
> **权威来源声明（三条，冲突时按此优先级）**：
> 1. **机器权威（路由）**：[`ROUTE_CONTRACT.md`](./ROUTE_CONTRACT.md) —— 由 `scripts/contract/extract_registered.py` 从真实 `.route()` 注册面抽取，生成于 2026-10-01，
>    并与 `derived_routes.rs` 派生表 + `tests/unit/fixtures/ledger_export/*.json` 双向对账（两份独立事实来源，差额必须为 0）。
> 2. **语义权威（MSC 编号）**：[`MSC_SEMANTICS.md`](./MSC_SEMANTICS.md) —— 本仓存在**借用 MSC 编号**承载非官方语义的情况（MSC4155 / MSC4204 / MSC3967），
>    按编号推断语义前必须先查表。
> 3. **人工权威（本文档）**：覆盖率分析。**与 1/2 冲突时以 1/2 为准。**
>
> ⚠️ **本文档为人工维护，不构成事实来源。** 凡本文档出现的数字，均标注其口径与可复现命令（见 §8）；
> 无法机器复核的（如上游文档章节端点计数）显式标注 `[人工口径·未机器复核]`。

---

## 一、路由实测口径（机器可复现）

### 1.1 三种口径，不可混用

`ROUTE_CONTRACT.md` 的抽取器已递归应用 `.nest()` 前缀，因此条目是客户端可直接拼接的**绝对路径**。
同一路径常因**多方法注册**（如 `/rooms/{room_id}/summary` 同时挂 GET/POST）与**多版本前缀**
（`v3` / `r0` / `v1` / `unstable/org.matrix.*`）而膨胀为多条。
故本文档统一给出三种口径，**任何表格都不得把不同口径的数字相加或相除**：

| 口径 | 含义 | 全部 | `/_matrix/client` | `/_synapse/admin` | 其他命名空间 |
|---|---|---|---|---|---|
| **注册条目** | 唯一 `(method, absolute_path)` 对 | **1153** | 639 | 289 | 225 |
| **唯一路径** | 去掉方法后的唯一 `absolute_path` | **915** | 491 | 228 | 196 |
| **逻辑端点** | 在上者基础上折叠版本前缀（`v3`/`r0`/`v1`/`unstable/*` → `vX`）后的唯一路径 | **807** | 385 | 226 | 196 |

- `ROUTE_CONTRACT.md` 总览现为 **1153**（2026-10-02 重生成，含 ⑦ 的关系写入端点迁移），与上表「注册条目」**同为 1153**（本版已把分类表刷新到同一 HEAD，
  不再留 v1.7 快照的 `+1` 差）；
  该清单本身无 `(method,path)` 重复，65 个含路由注册的模块文件 / 73 个 `registered_by` 标签同为该文件的总览数字；
  生成器（`scripts/contract/gen_contract_doc.py`）另报 **45 个分类**（只出现在它的 stdout，不写进正文）。
- 1149 → 913 的差额**不是漂移**，而是"同路径多方法"（如 `summary` 4 个方法）；
  913 → 805 的差额是"同路径多版本前缀"。
- 「其他命名空间」196（唯一路径）= `/_matrix/`（非 client，如 federation/app/key）144 + `/_synapse/`（非 admin）41 + `/.well-known/` 5 + 根级非命名空间 6。
  其中根级端点按 `(method,path)` 为 **8 条有意注册**（3 条探活 `GET /`·`/health`·`/_health` + 5 条 legacy CAS admin 根端点
  `GET|POST /admin/services`、`DELETE /admin/services/{service_id}`、`GET|POST /admin/users/{user_id}/attributes`），该桶由
  `test_extract_registered.py::check_non_namespace_bucket` 守卫钉死（出现新成员即转红）。CAS 协议 6 条
  （`/login`、`/logout`、`/p3/serviceValidate`、`/proxy`、`/proxyValidate`、`/serviceValidate`）已于 2026-09 迁入 `/_synapse/cas/`
  （commit `d4e22f9ea`），故根级唯一路径由 12 降为 6、`/_synapse` 非 admin 由 35 升为 41，该批之后其他命名空间合计为 195；
  本版随 MSC3720 联邦端点再 `+1`（`/_matrix` 非 client 143 → 144），即上表的 **196**。
- **相对 2026-09-21 快照（1151 / 931 / 811）与 2026-09-22 复核（1165 / 933 / 813）的差额**：2026-09-22 之前的
  +14 / +2 / +2 来自 MSC4512 AS 代理的两条 `any()` 路由、MSC3912 的 `POST /_synapse/admin/v1/rooms/{room_id}/cascade_redact`，
  并**减去** D-12 删除的 `GET /_synapse/admin/v1/event_reports/{id}/history`；
  之后的 **−30 / −30 / −18** 全部来自 2026-09-25 的 **E2EE 去服务端私钥重构**（删 `verification_routes` 24 条 +
  `e2ee` 路由组 6 条，见 `docs/synapse-rust-vs-synapse-comparison.md` §13）。逐项归因（`git log -S <path>`）属独立文档任务，本表只保证口径可复现。
- **相对 v1.6 基线（HEAD `88001b4a9`，1135 / 903 / 795）的差额 +14 / +10 / +10**（本版按 §8 配方实测，逐条 `comm` 对账）：
  **+3 / +1 / +1** = profile 自定义字段稳定端点 `GET/PUT/DELETE /_matrix/client/v3/profile/{user_id}/{key_name}`；
  **+10 / +10 / +10** = Admin 媒体端点族（10 条唯一路径，见 §6.3）；
  **+2 / 0 / 0** = `POST /_synapse/admin/v1/invite/{allowlist,blocklist}`（唯一路径此前已有 `GET`，仅多/少方法）；
  **−1 / −1 / −1** = 上游 1.161 已删的 `GET /_matrix/client/unstable/org.matrix.msc2965/auth_issuer`（`76e5f9136` 有意摘除）；
  **0 / 0 / 0** = CAS 协议 6 条由根级迁入 `/_synapse/cas/`（同方法数，仅改路径分布与命名空间归属）。
- **本批 L1 增补（v1.8 未重算口径）**：MSC4140 单事件端点 `GET /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`
  于 2026-10-01 落地 handler + 路由（route ledger 1130 → **1131** / worker 1141 → **1142**，`+1` 注册条目）。
  因该 `absolute_path` 此前已有 `POST` 注册，**唯一路径与逻辑端点均不变**；按 §8 配方实测重算三口径为 **1150 / 913 / 805**
  （即仅「注册条目」`+1`）。上表三口径为 v1.7 于 HEAD `74bb9c522` 的实测值（1149 / 913 / 805），差额即为本批 `+1 / 0 / 0`。
- **本批合并重算（2026-10-01，v1.9）**：上表三口径已整体刷新到合并后的 `main`，机器实测 **1152 / 915 / 807**
  （相对 v1.7 的 1149 / 913 / 805 为 **+3 / +2 / +2**）。两条来源：
  **①** MSC4140 单事件端点 `GET /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`（L1，上一批遗留未折入表内）：
  同 `absolute_path` 已有 `POST`，故**仅注册条目 `+1 / 0 / 0`**，按 §8.1 规则归「消息」；
  **②** MSC3720 账户状态：客户端 `POST /_matrix/client/unstable/org.matrix.msc3720/account_status`
  与联邦 `POST /_matrix/federation/unstable/org.matrix.msc3720/account_status`，两条新路径
  **`+2 / +2 / +2`** —— 客户端一条按 §8.1 兜底规则落「房间」，联邦一条落「其他命名空间」。

> 📌 **口径注意**：`/_matrix/client/v1/proxy/{as_id}/{*path}` 被归入「房间」是 §8.1 分类脚本
> **兜底规则**（`("房间", r".")`）的结果，不代表它是房间 API。引用分类数字时须知这一点。

> 🚨 **对历史版本的纠正**：v1.3 及更早版本声称"HEAD 真实注册路由条目约 **883**"、"Client ~237 / Admin ~174 / 总计 ~411"。
> 883 与 237/174 均**无机器来源**，且与 2026-09-21 的 `ROUTE_CONTRACT.md` 不符。本版一律改为上表实测值。
> （同批次缺陷亦记录在 [`docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md`](../audit/COMPARISON_REPORT_REVIEW_2026-09-22.md) §2/§3。）

### 1.2 分类归属规则

下表按**有序优先级规则**把每条路由唯一归属到一类（因此各类可相加 = 该命名空间总数）。规则见 §8 的 `classify_routes.py`。
关键归属约定：`/rooms/{id}/…` 的**全部**子资源（含 `send` / `messages` / `state` / `tags` / `account_data` / `receipt`）归**房间**，
仅 `/sendToDevice`、`msc4140`、`/rooms/{id}/event/…` 归**消息**；`/users/{id}/media` 归**用户管理**而非媒体。

---

## 二、Client API 分类统计（机器口径，synapse-rust 实测）

| 类别 | 逻辑端点 | 唯一路径 | 注册条目 | 说明 |
|------|---------:|--------:|--------:|------|
| **房间** | 206 | 251 | 311 | 含 join/knock/leave/invite/state/tags/relations/threads/summary/spaces；⚠️ 含 MSC4512 的 `/_matrix/client/v1/proxy/{as_id}/{*path}`（`any()` ⇒ 7 条注册条目，属兜底归类，见 §1.1 口径注意） |
| **设备与密钥** | 54 | 84 | 125 | `/devices`、`/keys/*`、`/room_keys/*`、`cross_signing`、`dehydrated_device`(MSC3814)。⚠️ `device_verification` / `device_trust` / `security/summary`，以及全部 `/keys/verification/*`、`/keys/device_signing/verify_*`、`/keys/qr_code/*` 已于 2026-09-25 随 E2EE 去服务端私钥重构删除（见 `docs/synapse-rust-vs-synapse-comparison.md` §13） |
| **认证** | 44 | 59 | 70 | login/logout/register/refresh/oidc/saml/cas/rendezvous(MSC4108)/account(password·3pid·deactivate)/MSC2965（仅剩 `auth_metadata`，`auth_issuer` 已于 2026-09 摘除，见 §5.1） |
| **用户** | 25 | 29 | 50 | profile/presence/user_directory/thirdparty/capabilities/account_data；含稳定端点 `{key_name}`（PUT/GET/DELETE，2026-09 新增） |
| **同步** | 20 | 24 | 31 | `/sync`、`notifications`、MSC3575 + simplified MSC3575、pushrules/pushers/push、to_device |
| **消息** | 20 | 24 | 32 | `sendToDevice`、MSC4140 delayed_events（含 2026-10-01 新增的单事件 `GET`）、`/rooms/{id}/event/…` |
| **搜索** | 8 | 10 | 12 | `/search` |
| **媒体** | 8 | 10 | 10 | `/media/*`、`/upload`、`thumbnail`、`preview_url` |
| **合计** | **385** | **491** | **641** | — |

## 三、Admin API 分类统计（机器口径，synapse-rust 实测）

| 类别 | 逻辑端点 | 唯一路径 | 注册条目 | 说明 |
|------|---------:|--------:|--------:|------|
| **房间管理** | 51 | 51 | 60 | rooms/retention/purge_room/purge_history/shutdown_room/spaces/room_stats/statistics/server_notices/jitsi/cleanup/cascade_redact(MSC3912) |
| **用户管理** | 48 | 50 | 70 | users/user_sessions/registration_tokens/register/account_validity/whois/whoami/account/invite（`invite/{allowlist,blocklist}` 2026-09 补 `POST`） |
| **安全** | 45 | 45 | 57 | event_reports(15)/reports/policy/audit/feature-flags/experimental_features/background_updates(17) |
| **服务器** | 45 | 45 | 60 | 未被前四类命中的 server/version/rate-limit-status/modules/appservices/telemetry/saml/cas/external_services 等 |
| **联邦** | 20 | 20 | 23 | `/federation`、`destinations` |
| **媒体** | 17 | 17 | 19 | `/media*`、`quarantine_media`、`purge_media_cache`、`media_callbacks`；**2026-09 新增 10 条**：`media/{delete,protect,unprotect,quarantine,unquarantine}`、`rooms/{room_id}/media[/{media_id},/quarantine,/unquarantine]`、`user/{user_id}/media/quarantine` |
| **合计** | **226** | **228** | **289** | — |

> **增量复核注（2026-09-25，v1.5）**：本节三张表（§1.1 三口径总表、§二 Client、§三 Admin）已**全部**
> 按 §8 配方在 `9e26ee31a` 上重算，不再保留任何 2026-09-21 快照。
>
> 相对 2026-09-21 快照，本表（Admin）内部**无净变化**（216 / 218 / 277），但两个类别互相抵消：
> **安全 −1**（46/46/58 → 45/45/57）：D-12 删除了 `GET /_synapse/admin/v1/event_reports/{id}/history`
> （`/stats` **保留**，改为对 `event_reports` 的实时聚合）；说明列的 `event_reports(16)` → `(15)`。
> **房间管理 +1**（50/50/59 → 51/51/60）：来自**已提交**的 `a421e7641`（MSC3912 cascade redaction
> 的 `POST /_synapse/admin/v1/rooms/{room_id}/cascade_redact`），非 D-12 变更。
>
> Client 侧在 2026-09-22 的 MSC4512 增量（房间 +1/+1/+7）之后，**再叠加一次净减**（全部来自 2026-09-25
> 的 E2EE 去服务端私钥重构，见 `docs/synapse-rust-vs-synapse-comparison.md` §13）：
>
> | 类别 | 逻辑端点 | 唯一路径 | 注册条目 | 变化来源 |
> |---|---:|---:|---:|---|
> | **房间** | −1 | −1 | −1 | 删 `GET /_matrix/client/v3/security/summary`（按 §8.1 规则落入兜底「房间」） |
> | **设备与密钥** | −17 | −29 | −29 | 删 24 条 `/keys/{verification/*,device_signing/verify_*,qr_code/*}`（折叠版本前缀后 12 条）+ 5 条 `device_trust` / `device_verification/*` |
> | **合计** | **−18** | **−30** | **−30** | 与 `ROUTE_CONTRACT.md` 的 1165 → 1135 同一批 |
>
> ⚠️ **重算时发现并修正一处旧表口径不一致**：上一版 Client 表把 `device_trust` / `device_verification/*`
> 计在「设备与密钥」，但 §8.1 **打印出的**脚本里「设备」规则只匹配
> `/devices|/keys|/room_keys|cross_signing|dehydrated_device`，这 5 条实际落入兜底「房间」——
> 即**打印的配方复现不出打印的表**（旧表：唯一路径 房间 251 / 设备 113；脚本给出 256 / 108）。
> 本次删除把这 5 条一并移除后，两种归法结果相同，因此上表数字现在可被 §8.1 脚本**逐字复现**。
>
> Client 其余六个类别（认证/用户/同步/消息/搜索/媒体）三个口径全部未变。

> **增量复核注（2026-09-29，v1.7）**：本节三张表（§1.1 三口径总表、§二 Client、§三 Admin）已在 HEAD `74bb9c522`
> 上按 §8 配方重算。相对 v1.6 基线（`88001b4a9`，1135 / 903 / 795），本表（Admin）净增 **+10 / +10 / +12**：
> **媒体 +10 / +10 / +10**（`media/{delete,protect,unprotect,quarantine,unquarantine}`、
> `rooms/{room_id}/media[/{media_id},/quarantine,/unquarantine]`、`user/{user_id}/media/quarantine`，2026-09 新增）；
> **用户管理 0 / 0 / +2**（`invite/{allowlist,blocklist}` 补 `POST`，唯一路径此前已有 `GET`）。
> Admin 内部其余四个类别（房间管理 / 安全 / 服务器 / 联邦）三个口径全部未变。
>
> Client 侧本版净变 **+3 / +1 / +1**：**用户 +1 / +1 / +3**（新增稳定端点
> `GET/PUT/DELETE /_matrix/client/v3/profile/{user_id}/{key_name}`）；**认证 −1 / −1 / −1**
> （摘除上游 1.161 已删的 `GET /_matrix/client/unstable/org.matrix.msc2965/auth_issuer`，`76e5f9136`）；
> 其余六个类别（房间/设备与密钥/同步/消息/搜索/媒体）三个口径全部未变。
>
> CAS 协议 6 条由根级整体迁入 `/_synapse/cas/`（`d4e22f9ea`）：**注册条目净 0**，仅改变路径分布——
> 根级唯一路径 12 → 6，`/_synapse` 非 admin 35 → 41。逐条 `comm` 对账见 §1.1 末段「差额归因」。

> **增量复核注（2026-10-01，v1.9）**：上述三张表已按 §8 配方在合并后的 `main` 上重算（v1.8 未重算，留在 v1.7 的
> `74bb9c522` 口径）。相对 v1.7，Client 净变 **+1 / +1 / +2**：**房间 +1 / +1 / +1**
> （MSC3720 客户端账户状态端点，按 §8.1 兜底规则落「房间」）、**消息 0 / 0 / +1**（MSC4140 单事件 `GET`，同路径已有 `POST`）；
> Admin 六个类别与合计**三口径全部未变**（226 / 228 / 289）；新增的联邦 MSC3720 路径不计入 §二/§三（它不是 client 也不是 admin，
> 只进 §1.1 的「其他命名空间」）。

> 「逻辑端点」低于「唯一路径」是因为 `/v1/*` 与 `/v2/*` 折回同一 `vX` 路径时的合并（Admin 侧表现在 `users` 族与 `rooms` 族）。

---

## 四、与上游逻辑端点对照（上游列为人工口径，仅供参考）

> ⚠️ **口径警告**：下表"上游 Synapse"列**不是**机器抽取结果，而是按 Synapse 官方文档（Client-Server API / Admin API）
> 章节端点索引做的**人工估计**，标注为 `[人工口径·未机器复核]`。由于上游口径是"文档章节端点"而本仓列是"折叠版本前缀后的注册路径"，
> **两侧覆盖率百分比只反映趋势，不构成精确结论**。要得到可辩护的覆盖率，必须把上游端点也用机器抽取（见 §7 优化建议）。
> 本节保留该表仅因历史延续性；**权威口径以 §二/§三 与 `ROUTE_CONTRACT.md` 为准**。

### Client API

| 类别 | synapse-rust（逻辑端点） | 上游 Synapse `[人工口径]` | 参考覆盖率 |
|------|------------------------:|-------------------------:|-----------:|
| 认证 | 44 | 35 | >100%（本仓含 CAS/SAML/OIDC/MSC2965 等非 C-S 标准项） |
| 房间 | 205 | 50 | 口径不可比 |
| 消息 | 20 | 40 | 口径不可比 |
| 媒体 | 8 | 20 | 口径不可比 |
| 用户 | 25 | 25 | ≈100% |
| 设备 | 54 | 18 | 口径不可比 |
| 同步 | 20 | 15 | 口径不可比 |
| 搜索 | 8 | 10 | 80% |

### Admin API

| 类别 | synapse-rust（逻辑端点） | 上游 Synapse `[人工口径]` | 参考覆盖率 |
|------|------------------------:|-------------------------:|-----------:|
| 用户管理 | 48 | 27 | 口径不可比 |
| 房间管理 | 51 | 33 | 口径不可比 |
| 服务器 | 45 | 17 | 口径不可比 |
| 媒体 | 17 | 18 | ≈94%（2026-09 补齐 10 条 admin 媒体端点后已收敛，见 §6.3） |
| 联邦 | 20 | 14 | 口径不可比 |
| 安全 | 45 | 10 | 口径不可比 |

**结论**：v1.3 表格中"认证 34/35（97%）""房间 46/50（92%）"等百分比，源于两个**互相不可比的口径**相除，
且本仓侧数字（34/46/38/18/23/17/13/8）已无机器来源。本版不再给出统一覆盖率百分比。

---

## 五、上游 v1.157 → v1.162 增量逐条判定

> 依据：`element-hq/synapse` 的 `v1.162.0` 标签 `CHANGES.md`（`gh api repos/element-hq/synapse/contents/CHANGES.md?ref=v1.162.0`）。
> 判定口径：**TRUE** = 代码支撑；**PARTIAL** = 部分成立/语义不符；**MISSING** = 不存在；**N/A** = 本仓无对应结构。
> §5.1–§5.3 覆盖 **1.157 → 1.161**（上一轮判定）；**§5.4–§5.5 为本轮新增，覆盖 1.161 → 1.162**。
> v1.162.0（2026-09-29）正文为 "No significant changes since 1.162.0rc1"，全部条目来自 **1.162.0rc1（2026-09-22）**：
> **4 Features / 11 Bugfixes / 5 Docs / 13 Internal**，共 33 条。

### 5.1 协议与端点（影响 API 契约）

| 上游条目 | 版本 | 本仓实测 | 证据 |
|---|---|---|---|
| **默认房间版本改为 11**（MSC4239，Matrix v1.14） | 1.158 | **TRUE（已推进到 12）** | `synapse-common/src/room_versions.rs:94` `DEFAULT_ROOM_VERSION = "12"`（O-1 Phase 2，对齐上游 v1.162.0rc1；1.158 的 v11 默认已被本仓越过）。注意 `MSC4239` 是 **v11** 的发布 MSC，勿与 v12（MSC4304）混 |
| v12/v13 房间可创建 | 1.158 | **PARTIAL（v12 已可创建；v13 已移除）** | 同上 `:151` `RoomVersionCapability::stable("12")`（`can_create = true`）；v1–v11 为 `stable_no_create`（`:120-150`）；`"13"` 不列入（`:114-118`，Q5(b)，上游规范稳定列表止于 v12、1.161 只识别 `1..12`）。**订正（2026-10-02）**：MSC4297（state resolution v2.1）**已实现并接线到生产路径**（分叉时 `resolve_forked_state` → `resolve_state_for_version_with_rules`，v12+ 空 state map 起步），v12 **不再**「声明领先实现」（`docs/audit/ROOM_V12_PLAN_STATUS_2026-09-27.md`） |
| 缩略图动画支持（`animated` 查询参数） | 1.158 | **MISSING** | `animated` 在 `synapse-web/src/routes/` 与 `synapse-services/` 中均 **0 命中** |
| MSC4335 媒体上传超限返回 `M_USER_LIMIT_EXCEEDED` | 1.158 | **PARTIAL** | 错误码已定义（`synapse-common/src/error/code.rs:83,131,225,292`），但**未见**媒体上传限额路径使用它（`synapse-services/` 0 命中） |
| **MSC3814 脱水设备 `/events` 端点由 POST 改为 GET + query** | 1.157 | **TRUE（已对齐，2026-09-25 复测）** | `ROUTE_CONTRACT.md` 在册为 `GET /_matrix/client/unstable/org.matrix.msc3814.v1/dehydrated_device/{device_id}/events`；契约产物（派生表 / fixture / 快照 / route-table / client.yaml）已同批再生成 |
| **删除 `GET /_matrix/client/unstable/org.matrix.msc2965/auth_issuer`** | 1.161 | **TRUE（已对齐，2026-09 摘除）** | 本仓已删除该端点（commit `76e5f9136`，见 `docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` D-90）；`/_matrix/client/unstable/org.matrix.msc2965/auth_metadata`（另一条 MSC2965 端点）仍在册 |
| **MSC4140 新增"获取单个延迟事件"端点** | 1.161 | **PARTIAL（联邦 EDU 已补齐 + GET 单事件端点已落地）** | 单事件端点 `GET /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}` 已**落地 handler 与路由**（`synapse-web/src/routes/delayed_events.rs:41-61,97-99`；2026-10-01，路由 ledger 已同步，worker ledger 亦在册）；**联邦 EDU 已实现** —— `synapse-federation/src/edu.rs:37,67,83` 的 `EduType::DelayedEvent` ⇄ `"m.delayed_event"`，消费点 `synapse-web/src/federation/edu.rs:631`。**仍缺**：schedule 路径把 `state_key` 硬编码为 `None`（`synapse-services/src/delayed_event_service.rs:123`；行号已随 L1 新增 `get` 方法漂移，旧报告 `:94` 作废） |
| **MSC4502 定向房间成员查询** | 1.160 | **PARTIAL（编号借用）** | 已核查（2026-10-04）：官方 MSC4502 = 新 CS 端点 `GET /_matrix/client/v3/rooms/{roomId}/is_joined`（query 恰取 `mxid` 或 `server_name`）+ appservice `scopes` + OAuth scope（*proposals*，Open / `needs-implementation`），本仓**均未实现**（全仓无 `is_joined`）；编号借给 `/members` 的 `at`/`dir`/`limit`/`membership`/`not_membership` 分页，落点 `synapse-web/src/routes/handlers/room/members.rs`、`synapse-storage/src/membership/mod.rs`（详见 `MSC_SEMANTICS.md` §1.1） |
| **MSC4262 / MSC4429 Profile 更新进 sync** | 1.159/1.160 | **PARTIAL（MSC4262 编号借用+形状漂移；MSC4429 legacy 半边未实现）** | 已核查（2026-10-04）：官方 MSC4262 = sliding sync **`profiles`** 扩展（响应 `extensions.profiles.users.{uid}` = `{updated, removed}`、`fields` opt-in），本仓扩展名写作 `profile_updates`、形状为 `{displayname,avatar_url,updated_ts}` ⇒ 名称+形状漂移；官方 MSC4429 = legacy `/sync` **顶层 `users` 对象**（filter `profile_fields.ids` opt-in），本仓未输出。落点 `synapse-services/src/sliding_sync_service/extensions.rs`、`synapse-services/src/user_service.rs`（详见 `MSC_SEMANTICS.md` §1.1） |
| **MSC4512 App Service 命名空间代理 / 联邦请求** | 1.161 | **TRUE（代理已实现，2026-09-25 复核；旧版判 MISSING 已作废）** | `synapse-web/src/routes/app_service.rs:722-723` 注册两条 `any()` 代理路由（`/_matrix/app/v1/proxy/{as_id}/{*path}`、`/_matrix/client/v1/proxy/{as_id}/{*path}`），handler `proxy_to_as` 做 AS 注册校验 + `hs_token` 鉴权 + hop-by-hop 头过滤 + 响应回传；`msc4512` 命中 2 个文件。**未做**：上游 #19977 的另一半（联邦侧代理请求） |
| MSC3861 实验性 auth delegation 移除 | 1.157 | **N/A** | 本仓以 MAS 稳定集成为准（`synapse-services/src/auth/mas_validator.rs`） |

### 5.2 安全 / 限流 / 错误码

| 上游条目 | 版本 | 本仓实测 | 证据 |
|---|---|---|---|
| **`rc_reports` 限流应用于房间举报端点** | 1.161 | **TRUE（已实现，2026-09-25 复核）** | `synapse-web/src/routes/directory_reporting.rs:235,293` 在 `report_room`/`report_user` 内按用户取桶（`take_rc_reports_token`，`:638-645`）；规则可配（`rate_limit.rc_reports`） |
| `M_APPSERVICE_LOGIN_UNSUPPORTED`（Matrix 1.17 稳定码） | 1.161 | **PARTIAL（根因已消除一半）** | **AS 登录已实现**：`synapse-web/src/routes/auth_compat.rs:455-468` 处理 `m.login.application_service`（as_token + 命名空间校验 + 设备物化 + 令牌签发）；但该**错误码本身** `M_APPSERVICE_LOGIN_UNSUPPORTED` 全仓 **0 命中**（上游把它用在 `POST /register` 的 `inhibit_login` 语义上，窗口不在 `/login`） |
| MSC4178 3PID `requestToken` 非法邮箱/国家码返回 `M_INVALID_PARAM` | 1.161 | **未核对** | — |
| MSC3866：`GET /_synapse/admin/v2/users` 在未启用时省略 approval 标记 | 1.161 | **未核对** | — |
| Profile 自定义字段：PUT/DELETE 返回 403 + `M_FORBIDDEN` | 1.161 | **已实现（另一触发路径）** | `account_compat.rs:205,237`（标准字段）、`extended_profile.rs:160,186`（自定义字段）在调用者 ≠ 目标用户时返回 `forbidden("Access denied")` |
| Profile 不存在用户写自定义字段返回 404 而非 500 | 1.161 | **TRUE（已对齐，2026-09）** | `synapse-storage/src/user/storage.rs:726` 的 `user_exists` 为纯行存在性判定（**对已停用账户亦为 true**，见 `:718-725` 注释，对齐上游 #20172）；`extended_profile.rs:62-73` 用它 ⇒ 仅"真正不存在"返回 404、"已停用但存在"写字段成功。需要"可操作账户"的调用点另用 `active_user_exists`（`:738`） |
| MSC4222 `/sync` 左房 `state_after` 成员泄漏修复 | 1.161 | **PARTIAL（订正：本仓已有 MSC4222 `state_after` 支持）** | ~~全仓 `state_after` / `MSC4222` = 0~~（旧判已作废）：`synapse-services/src/sync_service/types.rs:192,195,238`、`synapse-web/src/routes/handlers/sync.rs:35,80,164,185` 均有 `state_after` 透传；本仓**未做** v1.162 #20171 的「since 落在持久化批次内」worker 边界修复（见 §5.4） |
| MSC3912 关系性撤回（room version > 10 时 `redacts` 置入 `content`） | 1.161 | **PARTIAL（格式已修 + 级联已实现，客户端路径不级联）** | ① **格式**：Phase 1 起由服务层按房间版本注入 `content.redacts`（v11+），出站 PDU 不再重复写顶层 `redacts`，有回归用例锁定；② **级联**：`synapse-storage/src/event/cascade.rs` + `synapse-services/src/event_redaction_service.rs:58` + 端点 `POST /_synapse/admin/v1/rooms/{room_id}/cascade_redact`（深度默认 5、上限 10）；③ **缺口**：客户端撤回路径 `synapse-web/src/routes/handlers/room/events.rs:990` 仍只调 `redact_event_content`（**不级联**）—— 级联仅管理端可达 |
| MSC4242 State DAG（联邦客户端 + 存储） | 1.161 | **PARTIAL（仅存储层；受阻）** | 官方 MSC4242 = **"State DAGs"**（*proposals*，Open / In Review，`requires-room-version`、**尚无房间版本指派**）。本仓 `dag.rs` 不实注释**已修正（2026-10-02）**（如实陈述：无生产调用点、仅 `db_tests` 覆盖）；上游本身亦为 experimental，其 #20133 serving 是把 MSC4242 接进**既有**联邦端点、**不新增路由**，受阻于房间版本 + `experimental_features`（对照报告 §18.4 L-2） |
| **v1.157.2 安全版本**（ELEMENTSEC / GHSA） | 1.157.2 | **已判定（2026-09-23，11 条）** | 逐条"受影响/不受影响 + 证据"对照表见 [`../synapse-rust-vs-synapse-comparison.md`](../synapse-rust-vs-synapse-comparison.md) §14.5：3 条需动作/决策（push rule 上限、别名劫持、multipart Content-Type）、2 条需代理侧复核、其余 6 条本仓已有守卫。⚠️ 公告计数是 **11** 而非 12（三处交叉验证：Releases 正文 / tag `CHANGES.md` / advisory 列表） |

### 5.3 外部依赖类（非路由，但影响能力声明）

| 上游条目 | 本仓实测 | 证据 |
|---|---|---|
| 1.157 `exclude_rooms_from_presence` / `presence` 分节新增 `last_active_granularity`、`sync_online_timeout`、`idle_timeout` | **未核对** | 需核对 `synapse-common/src/config` 的 presence 段 |
| 1.157 存在感禁用后把此前在线用户标记为离线（#19948） | **需关注** | 本仓 presence 语义见 §6 相关条目 |
| 1.161 弃用 `matrix_rtc.livekit_service_url`，改用 SFU WebSocket URL | **PARTIAL（死配置）** | `LivekitConfig.ws_url`（`config/voip.rs:92`）**全仓无读取点**；`rtc/transports` 只返回 ICE |
| 1.158 `register_federation_callbacks(...)` 模块 API | **未核对** | — |

### 5.4 v1.161 → v1.162 增量：Features / Bugfixes（本轮新增）

> 上游 1.162.0rc1 的 **4 Features + 11 Bugfixes** 逐条判定（PR 号为上游 `element-hq/synapse` 编号）。

**Features（4）**

| 上游条目 | 本仓实测 | 证据 |
|---|---|---|
| #20130 默认房间版本提升到 `"12"` | **TRUE** | `synapse-common/src/room_versions.rs:94` `DEFAULT_ROOM_VERSION = "12"`（本仓此前已采用 v12 默认，见 §5.1 首行） |
| #20162 E2EE 一次性密钥每设备每算法上限 **500**，超限返回 `400` | **TRUE** | `synapse-e2ee/src/device_keys/service.rs:20-21` `MAX_ONE_TIME_KEYS_PER_ALGORITHM_PER_DEVICE = 500`、`:305-319` 计数并拒绝；单测 `:1010,1028,1039` |
| #20187 Redis 连接支持 `username`（Redis 6+ ACL 认证） | **TRUE** | `synapse-common/src/config/database.rs:149,189,192` 依 `username`/`password` 拼 `redis://[username[:password]@]host:port/`；单测 `:295,307` |
| #20218 客户端 profile 查询限流（`rc_profile`） | **TRUE** | `synapse-common/src/config/rate_limit.rs:95-97,110` 定义 `rc_profile`；`synapse-web/src/routes/account_compat.rs:99,119,140,206,238` 应用桶（桶函数 `:265,287`） |

**Bugfixes（11）**

| 上游条目 | 本仓实测 | 证据 |
|---|---|---|
| #19723 MSC4311 invite/knock 严格校验延后至 **2027-06-01** | **TRUE（以配置开关承载）** | `synapse-common/src/config/federation.rs:228-245` `msc4311_strict_validation`（默认 `false` ＝宽限期宽松，注释显式写 2027-06-01 截止）；联邦侧发全量 PDU 见 `synapse-services/src/room/membership/federation.rs:653` |
| #19768 第三方规则回调 `check_event_allowed()` 兼容 MSC4291 房间 | **TRUE** | `synapse-services/src/module_service.rs:145-167` `EventAdmissionGate` trait + `ModuleService` 实现；`:189-239` 共享踏板 `consult_event_admission`（`has_event_rules()` 快路径短路、读房间状态构建 `ThirdPartyRuleContext`、拒绝返 `403 M_FORBIDDEN`、`modified_content` 仅本地可改写）；本地两咽喉 `room/messaging/events.rs:212`（`create_event`，可改写）/:408（`create_event_with_graph` 联邦入站，不改写）；membership 本地/远端各写点 `room/membership/{actions,moderation,federation}.rs` 于**状态变更之前**挂载；v12/MSC4291 房间 id 仅作字符串查表 ⇒ 天然兼容；触发侧 `third_party_rules` 配置（`synapse-common/src/config/mod.rs:254,2111` `ThirdPartyRulesConfig`）经 `ModuleService::register_configured_third_party_rules`（`module_service.rs:737`）在 `synapse-services/src/wiring/admin.rs:240` 装配时注册进与执行面共享的同一 `Arc<ModuleService>` ⇒ 钩子生产可达（`has_event_rules()` 自启动即为真）；规则亦可携带 `modification`（`event_types` + 改写后的 `content`，`synapse-common/src/config/mod.rs:2135` `ThirdPartyRuleModification`）经 `SimpleThirdPartyRule::with_modification` 返回改写，使 `(True, dict)` 改写路径生产可达 |
| #20100 本地媒体缩略图异步打开 | **TRUE** | `synapse-services/src/media_service.rs:502` `tokio::fs::read(&thumbnail_path)`；`MediaStreamPayload` 持 `tokio::fs::File`（`media/mod.rs:65-71,487,523`） |
| #20115 丢弃不合规（grandfathered）历史 user_id 的联邦设备列表更新 | **TRUE** | `synapse-web/src/federation/edu.rs:108-116` `validate_device_list_update_content` 按 localpart 调 `is_compliant_user_id_localpart` 丢弃（注释引 PR #20115） |
| #20145 未设置的 displayname/avatar_url 不再当作 `null` 字段 | **TRUE** | `synapse-services/src/user_service.rs:458` 注释；`synapse-web/src/routes/account_compat.rs:154-163` `single_profile_field` 对 `null`/缺省/空串返回空对象 |
| #20154 `/hierarchy` 返回 `allowed_room_ids`（Matrix 1.15 起要求） | **TRUE** | `synapse-web/src/routes/handlers/search/hierarchy.rs:203-224` `annotate_allowed_room_ids`（注释引 #20154）；`synapse-services/src/room/summary/service.rs:97` `resolve_allowed_room_ids` |
| #20171 MSC4222 `state_after` 在 `since` 落在持久化批次内时漏发 state 事件 | **PARTIAL** | 本仓有 `state_after` 支持（`sync_service/types.rs:192,195,238`），但**未核对**该 worker 批次边界修复；同时**订正** §5.2 中"MSC4222 = 0"的旧判（已作废） |
| #20181 返回稳定 `M_UNKNOWN_DEVICE` 而非 MSC4326 unstable 前缀 | **部分（2026-10-02 更正）** | `synapse-common/src/error/code.rs:99,149,255` 定义了稳定码（相对 MSC4326 unstable 前缀确属稳定化）；但**设备 CRUD 不使用它** —— 规范 `device_management.yaml` 对 `GET`/`PUT`/`DELETE /_matrix/client/v3/devices/{deviceId}` 的 404 描述为 *"The current user has no device with the given ID"*，且该文件全无 `M_UNKNOWN_DEVICE`，故 `synapse-web/src/routes/device.rs::device_not_found_error()` 返回 `M_NOT_FOUND`。`M_UNKNOWN_DEVICE` 属 MSC4326 *appservice device masquerading*，本仓无该路径 ⇒ 该码**当前无端点使用**（详见对照报告 §18.3 #7 / §18.7.1 P-11） |
| #20182 大房间递归 `/relations` 慢（改为在递归查询内 join） | **已对齐（等价实现，2026-10-02）** | 原「本仓 `/relations` 无 `recurse` ⇒ N/A」判据已作废：**M-6（`46fe964b9`）已交付 MSC3981 `recurse`**（路由 `synapse-web/src/routes/relations.rs`，不稳定名 `org.matrix.msc3981.recurse`），存储层实现即**一条静态递归 CTE、`events` 在递归内 join**（按 `events.stream_ordering` 拓扑序、深度 0 基 `depth <= 3`）——正是上游 #20182 的修复形状（把 join 移进递归查询内） |
| #20200 `GET /profile/{userId}/{field}` 未设置字段返回空对象而非 `null` | **TRUE** | 同 #20145：`synapse-web/src/routes/account_compat.rs:153-163` `single_profile_field`；单测 `tests/unit/account_compat_route_tests.rs:280` |
| #20204 房间状态变更时「取消 soft-fail」MSC4354 Sticky Events | **N/A（仅报告）** | 上游语义（v1.162 PR #20204，标题 *un-soft-failing MSC4354 Sticky Events*）：sticky 事件对**当前状态**做 `check_state_dependent_auth_rules` 失败即记 soft-failed；当关键 auth 状态（`m.room.join_rules`/`m.room.power_levels`/`m.room.member`）变更时重算并 **un-soft-fail**（`compute_sticky_events_to_un_soft_fail` → `un_soft_fail_sticky_events_txn` + `StickyEventsStream`）。**本仓 MSC4354 无 soft-fail 维度**：sticky 仅以 `room_sticky_events.is_sticky` 落库（`synapse-storage/src/sticky_event.rs`）+ 联邦 EDU（`synapse-services/src/room/service.rs:661-684,739-799`），写入不做 auth 评估、亦无状态变更重算（`un_soft_fail`/`StickyEventsStream` **全仓 0 命中**）⇒ **无"取消 soft-fail"的对象**。⚠️ 本仓 `events.soft_failed` 系 **B-8 事务去重**专用（`synapse-storage/src/event/txn_dedup.rs`），与 MSC4354 **同名不同义**，勿混用 |

### 5.5 v1.161 → v1.162 增量：Docs / Internal（非 API 契约）

> 上游 1.162.0rc1 的 **5 Docs + 13 Internal** 逐条判定。

**Improved Documentation（5）**

| 上游条目 | 本仓实测 | 说明 |
|---|---|---|
| #20209 记录可委派认证在 worker 上可处理的路由 | **N/A** | 上游文档；本仓 worker 路由以 ledger 快照为准（`route_ledger_worker_enabled.snapshot`） |
| #20210 记录"获取单个延迟事件"端点可 worker 化 | **TRUE** | 本仓 worker ledger 已含该端点（`tests/integration/snapshots/route_ledger_worker_enabled.snapshot:113` `GET …/delayed_events/{delay_id}`） |
| #20217 修复文档构建工具输出的小警告 | **N/A** | 构建基建 |
| #20219 contributing docs 的过时 `poetry` 版本改为指向 `pyproject.toml` | **N/A** | Python 构建基建 |
| #20225 新增防火墙配置文档 | **N/A** | 上游运维文档 |

**Internal Changes（13）**

| 上游条目 | 本仓实测 | 说明 |
|---|---|---|
| #19979 把 logcontext 机制移植到 Rust | **N/A** | 本仓本就为 Rust；`logcontext`/`LoggingContext` **0 命中**，无 Python logcontext 对应结构 |
| #20011 Rust 代码改为单一处存放 per-homeserver 状态 | **N/A** | Python/Rust 桥接层结构重构，本仓无对应 |
| #20097 `synapse_storage_stream_current_position` 指标 | **已补（per-stream 多标签，2026-10-02 合流后）** | `synapse-common/src/server_metrics.rs` 的 `storage_stream_current_position` 为**同名多标签** gauge（`{stream=…}`），标签集合 = `StreamPosition::ALL` 的 6 条：`events` / `to_device` / `device_lists` / `sliding_sync` / `quarantined_media` / `worker_events`（登记表与查询由 `tests/integration/stream_position_tests.rs` 逐字守卫）；数据源是 `synapse-storage/src/stream_positions.rs` 的**单条 `UNION ALL` 查询**（`get_stream_positions`），由 `src/server/mod.rs` 的 30s 指标循环周期刷新（不再依赖 admin `/statistics` 被访问）。**配套修复（合流时并入）**：`MetricsCollector::to_prometheus_format()` 按 metric family 去重 `# HELP`/`# TYPE`（此前动态 counter 多系列重复输出，Prometheus 会拒绝整个 scrape）。**未覆盖**：presence/typing/receipts/account_data/push_rules/e2ee/backfill/federation 无位置列；`sync_stream_id`、`device_lists_outbound_pokes` 有列但无生产写者（恒 0）；`worker_events` 已接线（P-5，worker 模式下推进） |
| #20133 为未来 MSC4242 增加 HTTP serving 函数 | **MISSING（受阻）** | 取证更正（2026-10-02）：上游 #20133 是把 MSC4242 接进**既有**联邦端点（`/make_join`、`/send_join`、`/get_missing_events` 的状态 DAG 回溯、`/send` 目的地按 `prev_state_events` 计算），**不新增路由**；前置是 MSC4242 房间版本 + `experimental_features` opt-in，本仓 `SUPPORTED_ROOM_VERSIONS` 仅至 v12 ⇒ 无接线落点。本仓 MSC4242 仅到存储层（`prev_state_events`，`synapse-storage/src/event/create.rs:257-265`），无 HTTP serving 函数（详见对照报告 §18.4 L-2） |
| #20160 为"当前房间状态的单项"增加缓存 | **维持等价（正式收口，2026-10-02）** | 本仓有 `room_state:{room_id}` **整房 state 列表**缓存（`synapse-services/src/sliding_sync_service/state.rs:20-36`，TTL 300；`synapse-cache/src/local.rs:47-49,91-92` 命名空间 `room_state` 20_000/1200），非上游按 `(type,state_key)` 单项缓存；**已属等价、无需重复实现**（决策 D-5：维持等价、不新增 per-item 缓存） |
| #20161 即使标准 Complement 套件失败也在 CI 跑 in-repo Complement | **N/A** | CI/测试基建 |
| #20166 联邦传输代码重构（事务准备/完成分离） | **未核对** | 内部重构，无 API 契约影响 |
| #20185 为 state resolution 增加按 conflicted 事件键控的缓存 | **等价实现（已交付，M-5）** | 取证更正（2026-10-02）：此前"**生产 0 调用点**、仅 `benches/performance_federation_benchmarks.rs:41,64`"的判定**不实** —— `resolve_state_for_version_with_rules`（内含 `full_conflicted_set` / `conflicted_state_subgraph`）在**生产路径**上：`create_event_with_graph` → `StateRecord::after_state_event` → `resolve_forked_state` → `StateWalker::resolve`（`synapse-services/src/room/state_record.rs:192,212,471`）。本仓已新增按冲突输入键控的结果缓存（`ResolutionCache`）：键 = `room_version` + 状态集合排序 `(key,event_id)` 投影 + 已加载事件 id 集合；进程内跨请求共享（`Arc<Mutex>`，由 `room/service.rs` 创建、经两个 `ServiceConfig` 穿入），上限 64、超限清空；HIT/MISS 返回逐字节相同状态图（红证 `resolution_cache_is_shared_across_walks`） |
| #20193 改进测试中 `assertEqual` 集合不等错误的渲染 | **N/A** | 测试基建 |
| #20205 修复 `/room_summary` 返回过期 `join_rules` | **TRUE** | `synapse-services/src/room/summary/state.rs:90-92` 状态变更即 `update_summary`（写 `join_rules` 列，`synapse-storage/src/room_summary/repository.rs:131`）⇒ 摘要列随状态刷新 |
| #20207 修复 Schema Diff CI 对 fork PR 无法评论 | **N/A** | CI |
| #20216 docs 构建依赖移入 `pyproject.toml` 的 `docs` 组 | **N/A** | Python 构建基建 |
| #20224 `.dockerignore` 注释与私有分支对齐 | **N/A** | 构建基建 |

### 5.6 本批 M1–M6 / L1–L5 处理结果（收口）

> 本轮"M1–M6 剩余清单 + L1–L5"的最终处置。凡判 N/A 者，均给出"本仓无对应结构"的实测依据，不实现。

| 项 | 结论 | 依据 |
|---|---|---|
| **L1** MSC4140 `GET /delayed_events/{delayId}` | ✅ **已实现** | `synapse-web/src/routes/delayed_events.rs:41-61,97-99`；路由 ledger 快照同步（default 1131 / worker_enabled 1142），worker ledger `:113` 在册 |
| **L2** Admin scheduled tasks 端点（`action_name` 机制） | **N/A / 延后** | 本仓有内部调度器 `ScheduledTasks`（`src/server/mod.rs:115,320,339,372,379`），但 **`action_name` 全仓 0 命中** ⇒ 无 admin 列表/动作端点机制可挂靠，纯新增无落点 |
| **L3** `federation_domain_whitelist` 可空处理 | **N/A** | `federation_domain_whitelist` **全仓 0 命中** ⇒ 本仓无该配置面 |
| **L4** 内部性能项 | ✅ **已落地** | ① `synapse_storage_stream_current_position` **per-stream 多标签 gauge 已落地**（2026-10-02 合流后为 `StreamPosition::ALL` 的 5 条 series，单一 `UNION ALL` 数据源 + 30s 循环刷新；合流时删除了另一线的 2 条 series 抓取时实现；见 §5.5 #20097）；② current room state / state resolution 缓存已**等价实现**（§5.5 #20160 维持等价·已收口；#20185 等价实现＝M-5 冲突输入键控缓存，2026-10-02 更正原「N/A」判定）——避免重复，不再新增 |
| **L5** 文档对齐 v1.162.0 | ✅ **本轮完成** | 本文件顶部基准、§五 标题与 §5.4/§5.5 增量、§5.1/§5.2 两条订正、§八 命令、footer |
| **M5** 取消 soft-fail（MSC4354 Sticky Events） | **N/A（仅报告）** | 本仓 MSC4354 **不存在 sticky soft-fail 机制**：sticky 事件不做状态相关 auth 评估、无 soft-failed 记录、无状态变更重算（`un_soft_fail`/`StickyEventsStream` **全仓 0 命中**）⇒ **无对象可"取消"**。本仓 `events.soft_failed` 是 **B-8 事务去重**专用（`synapse-storage/src/event/txn_dedup.rs`），读取一律 `soft_failed = FALSE`，与 MSC4354 **同名不同义**，不可混同 |

---

## 六、本轮纠正的错误结论与仍缺失项

### 6.1 已实现、但历史版本文档判定为"缺失"（纠偏）

| 端点 | v1.3 文档 | 实测 |
|---|---|---|
| `GET/DELETE /_synapse/admin/v1/rooms/{room_id}/reports[/{report_id}]` | §三"缺失（待实现）" | **已实现**（`_synapse/admin/v1/rooms/{room_id}/reports`、`.../reports/{report_id}` 均在册；`synapse-web/src/routes/admin/report.rs`） |
| `GET /_synapse/admin/v1/quarantine_media/{media_id}/changes` | §三"缺失（待实现）" | **已实现**（在册） |
| `GET/DELETE /_synapse/admin/v1/reports[/{report_id}]` | 未提及 | **已实现**（在册，与 `event_reports` 并存） |
| `POST /_matrix/client/v3/keys/upload` 拒绝 `device_keys: null` | §三"缺失（待实现）" | `[未验证]`（本次未按代码复核，留待下一轮） |
| 事件举报 API（`event_reports` 全家族 15 条，含 `rate_limit/{user_id}/block`） | v1.3 完全未列 | **已实现且超出上游文档面**（2026-09-24：`/{id}/history` 已按 D-12 删除，16 → 15） |
| **`rc_reports` 专项限流** | §5.2 判 `MISSING` | **已实现**（`synapse-web/src/routes/directory_reporting.rs:235,293`，桶函数 `:638-645`） |
| **MSC4512 AS 命名空间代理** | §5.1 / §6.2 判 `MISSING` | **已实现**（`synapse-web/src/routes/app_service.rs:722-723` + handler `proxy_to_as`） |
| **AS 登录 `m.login.application_service`** | §5.2 / §6.2 判 `MISSING` | **已实现**（`synapse-web/src/routes/auth_compat.rs:455-468`） |
| **MSC4140 联邦 EDU** | §5.1 判缺失 | **已实现**（`synapse-federation/src/edu.rs:37,67,83`；消费点 `synapse-web/src/federation/edu.rs:631`） |
| **MSC3912 关系性级联撤回** | §5.2 / §6.2 判 `MISSING` | **部分实现**（`synapse-storage/src/event/cascade.rs` + `synapse-services/src/event_redaction_service.rs:58` + 管理端点；客户端撤回路径不级联） |
| **Content Scanner** | §11.2（对比报告）判"未装配、从未被构造、未接入 config" | **已装配且已接入生产路径**（`synapse-services/src/wiring/core.rs:66,180` 构造 + `synapse-common/src/config/mod.rs:242` 配置项；消费点 `synapse-web/src/routes/media/upload.rs:89,137` `scan_when_enabled`、`synapse-web/src/routes/handlers/room/events.rs:317` `scan_text_when_enabled`）——**"零调用点/孤儿模块"旧判已作废**；仍无持久化（扫描 verdict 不落库） |

> ⚠️ **§三 的"缺失清单"在 v1.3 中停更于 2026-05-28**，其"待实现"标记已不可作为缺失证据。
> 本版起：该清单每条必须附 `路径:行号` 或"在册证据"，否则不写入。
> 另注：上表后 6 行是 v1.5 新增 —— **同一批"缺失"判定在同一份报告的 §5.2/§6.2 与对比报告里出现过三种不同状态**，
> 说明"人工清单"本身是漂移源；判断能力是否存在应直接跑 §8 的命令。

### 6.2 结构性缺失（本仓无对应实现，需决策）

| 项 | 状态（2026-09-25 复核） | 影响 |
|---|---|---|
| **App Service 登录**（`m.login.application_service`） | ✅ **已实现**（`auth_compat.rs:455-468`）；~~整体缺失~~ | 仍缺 pushers、设备管理、虚拟用户以 C-S 身份调用、以及稳定错误码 `M_APPSERVICE_LOGIN_UNSUPPORTED`（全仓 0 命中） |
| **MSC4512** App Service 命名空间代理 | ✅ **代理已实现**（`app_service.rs:722-723`）；~~缺失~~ | 联邦侧代理请求（上游 #19977 的另一半）未做；上游整体仍 experimental + opt-in |
| **MSC3912 / v11 撤回格式** | 🟡 **格式已修**（v11+ 写 `content.redacts`）；级联**仅管理端可达** | "撤回一条消息不连带撤回其回复/表情"仍与上游行为不同（`handlers/room/events.rs:990` 不级联） |
| **v12/v13 房间创建** | ✅ **仅 v12 可创建**（2026-09-27 G-1，`7489b247f`；`room_versions.rs:151` `stable("12")`） | v1–v11 为 `stable_no_create`、v13 已移除（Q5(b)）；**订正（2026-10-02）**：MSC4297（state resolution v2.1）已实现并接线到生产路径（写入分叉时 `resolve_forked_state` → `resolve_state_for_version_with_rules`），已无遗留项 |
| **`rc_reports` 限流桶** | ✅ **已实现**（`directory_reporting.rs:235,293`）；~~缺失~~ | — |
| **MSC4140 联邦 EDU** | ✅ **已实现**（`edu.rs:37,67,83`）；~~缺失~~ | 仍缺：schedule 的 `state_key` 硬编码 `None`（`delayed_event_service.rs:94`） |
| **MSC3814 `/events` 方法** | ✅ **已对齐（GET）**；~~漂移（POST）~~ | — |
| **Content Scanner** | 🔴 **已装配但无消费者**：`wiring/core.rs:66,179` 已构造、`config/mod.rs:242` 已接入配置，但 `scan`/`scan_text`/`scan_media` 在生产路径 **0 调用点**，且无存储模块与表 | 配置打开也不产生任何扫描行为 —— "看起来已上线"的功能比缺功能更危险 |

### 6.3 Admin 媒体端点（2026-09 已补齐，差距基本收敛）

> **本版订正**：v1.6 及以前本节判定"本仓 admin 媒体类仅 **7** 条逻辑端点……**缺少**形如
> `GET/DELETE /_synapse/admin/v1/users/{user_id}/media` 的按用户媒体管理族"。该论断**已被证伪**——
> 2026-09 的一次提交（`a13f57316`，见 `docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` D-87）新增了
> **10 条** admin 媒体端点，本仓该类别现为 **17 逻辑端点 / 17 唯一路径 / 19 注册条目**，与上游人工口径
> （18）已基本持平（≈94%）。

本仓 admin 媒体面当前实测（`ROUTE_CONTRACT.md` 在册，可复现）：

| 端点 | 作用 |
|---|---|
| `GET /_synapse/admin/v1/media` | 媒体列举（按房间/用户过滤由 query 决定） |
| `GET\|DELETE /_synapse/admin/v1/media/{media_id}` | 单条媒体查询 / 删除 |
| `GET /_synapse/admin/v1/media/quota` | 媒体配额 |
| `POST /_synapse/admin/v1/media/delete` | 批量删除 |
| `POST /_synapse/admin/v1/media/{protect,unprotect}/{...}` | 保护 / 取消保护 |
| `POST /_synapse/admin/v1/media/{quarantine,unquarantine}/{server_name}/{media_id}` | 隔离 / 解除隔离 |
| `GET /_synapse/admin/v1/rooms/{room_id}/media[/{media_id}]` | 按房间列举 / 查询 |
| `POST /_synapse/admin/v1/rooms/{room_id}/media/{quarantine,unquarantine}` | 按房间隔离 / 解除隔离 |
| `POST /_synapse/admin/v1/user/{user_id}/media/quarantine` | 按用户隔离 |
| `GET /_synapse/admin/v1/quarantine_media/{media_id}/changes` | 隔离媒体变更流 |
| `POST /_synapse/admin/v1/purge_media_cache` | 清除媒体缓存 |
| `GET\|POST /_synapse/admin/v1/media_callbacks[/{callback_type}]` | 媒体回调（本仓扩展） |

> ⚠️ **口径说明**：上游人工口径列的是 `GET/DELETE /_synapse/admin/v1/users/{user_id}/media`
> （**复数** `users`）；本仓实现的是 `POST /_synapse/admin/v1/user/{user_id}/media/quarantine`
> （**单数** `user`）。二者语义相近但路径不同，故 ≈94% 是**趋势**而非逐字对齐；若要严格对齐上游运维面板，
> 仍可补一条复数形式的按用户媒体列举/删除端点（取决于产品是否需要）。

---

## 七、优化建议

> 原则：**不做人日估算**；每条给"现象 → 动作 → 验收判据"。与
> [`docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md`](../audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md)
> 和 [`PROJECT_REMAINING_ISSUES_2026-09-14.md`](../audit/PROJECT_REMAINING_ISSUES_2026-09-14.md) 交叉引用，**本文档不新开 backlog**。

### A. 文档可信度（本文档自身）

| 编号 | 动作 | 验收判据 |
|---|---|---|
| A1 | ✅ **v1.5 已完成**：路由总数改为 `ROUTE_CONTRACT.md` 实测（**1135 注册条目 / 903 唯一路径 / 795 逻辑端点**；2026-09-22 为 1165 / 933 / 813，2026-09-25 随 E2EE 去服务端私钥重构净减 30 条路由），删除无源的 883 / 411，并把 2026-09-21 快照全部重算 | §1 的每个数字都能由 §8 命令复现 |
| A2 | ✅ 本版已修正：删除"34/35（97%）"等不可比口径相除得到的覆盖率百分比，改为显式口径警告 | §四 表格不含未标注口径的百分比 |
| A3 | ✅ 本版已修正：章节编号重复（v1.3 出现两个"三、"）| 章节编号唯一 |
| A4 | 把"缺失清单"改为**带证据的清单**：每条必须有 `路径:行号` 或在册证据 | 评审清单项：无证据条目不得出现 |
| A5 | 引用其他人工文档时必须带时间戳（沿用 2026-09-22 复核口径） | 脚本化检查（建议 D 类） |

### B. 协议正确性（优先于实验性 MSC）

| 编号 | 动作 | 状态（2026-09-25） | 验收判据 | 关联 |
|---|---|---|---|---|
| B1 | **v11 撤回格式**：`room_version > 10` 时把目标 id 写入 `content.redacts` | ✅ **已修**（服务层按房间版本注入，出站 PDU 不再重复写顶层） | 新增测试：v11 房间撤回的 `content.redacts` == 目标 id；v10 仍在顶层 | 本报告 §5.2 首次指出 |
| B2 | **`rc_reports` 限流桶** | ✅ **已修**（per-user 桶 + 可配规则 + 守卫测试） | 命中限流返回 429 + `retry-after` | 上游 1.161 #20036 |
| B3 | **MSC3814 `/events` 改为 GET + query**，`next_batch` 末页返回 null | ✅ **已修**（契约产物同批再生成） | 契约测试断言方法为 GET、末页 `next_batch` 为 null | 上游 1.157 #19896 |
| B4 | **移除上游已删除的 `msc2965/auth_issuer`**（或明确记录为有意兼容） | ✅ **已移除**（`76e5f9136`，2026-09；残留 `auth_metadata` 属另一条 MSC2965 端点） | 决策记录；若保留需标注为"上游已删除的本仓扩展" | 上游 1.161 #20163 |
| B5 | **v12/v13 支持边界决策**：实现 v12 认证规则并放开创建，或明确记录"仅 join/federate" | ✅ **已决策并落地**（Q1=a：仅 v12 可创建；`room_versions.rs:151` `stable("12")`，v1–v11 `stable_no_create`，v13 移除） | 决策记录 + `capabilities.available` 仅 `"12"` 与之一致 | 上游 1.158 |
| B6 | **v1.157.2 安全公告同类性逐条判定** | ✅ **已做**（对比报告 §14.5，共 11 条：3 条需动作、2 条需代理侧复核、6 条已有守卫） | 对照表 + 结论 | 上游 1.157.2 |
| B7 | **Profile 语义对齐**（停用但存在的用户写自定义字段应成功；account_data 非对象 ⇒ 400 而非 500） | ✅ **已完成**（MSC4133 非对象已改 400；停用用户走行存在性判定 `user_exists`（`extended_profile.rs:62-73`，U-2/#20172）；稳定 `/{key_name}` GET/PUT/DELETE 已注册，2026-09） | 各状态码有测试断言 | 上游 1.161 #20172/#20149 |
| B8 | **客户端撤回走级联**（或显式声明不支持）：当前 `handlers/room/events.rs:990` 只撤回单条 | 🔴 **新增缺口** | 撤回有回复的消息后相关事件均被撤回；或文档显式声明不支持 | 本报告 §5.2 / MSC3912 |
| B9 | **Content Scanner 决策**：接线（存储 + 表 + 媒体上传调用点）或下线该装配 | 🔴 **新增缺口** | 配置打开后上传媒体确实被扫描（有测试）；或模块下线 | 本报告 §6.2 |

### C. 功能补齐（保留原方向，重新定级）

| 编号 | 项 | 原定级 | 建议定级 | 理由 |
|---|---|---|---|---|
| C1 | App Service 登录（含稳定错误码） | 未列 | **低（已实现，仅欠错误码）** | `m.login.application_service` 已落地；剩稳定错误码 `M_APPSERVICE_LOGIN_UNSUPPORTED` 与 pushers/设备管理 |
| C2 | MSC4502 / MSC4262 从 PARTIAL 收敛到完整或显式声明边界 | 未列 | **中** | 代码已有实现痕迹但未验证语义完整性（各 8 个 `.rs` 命中） |
| C3 | MSC4140 联邦 EDU | "已对齐" | **低（已实现，仅欠 `state_key`）** | EDU 已通；剩 `delayed_event_service.rs:94` 的 `state_key: None` |
| C4 | Admin 媒体端点族补齐 | 未列 | **已收敛（低）** | 2026-09 已补齐 10 条（§6.3，**17 vs 上游 18**，≈94%）；仅差"复数 `users/{user_id}/media` 列举/删除"语义，看产品是否需要 |
| C5 | Admin `stats` 接口接运维仪表盘 | 短期 | **中（保留）** | 保留 v1.3 方向 |
| C6 | MSC4242（State DAG） | P0 阻断性 | **低（观察项）** | 上游本身 experimental + opt-in；`dag.rs` 不实注释**已修正（2026-10-02）**；serving 受阻于 MSC4242 房间版本与语义定稿（§18.4 L-2；#20133 实为改既有联邦端点，非新增路由）。**MSC4512 已实现，从本行移出** |
| C7 | OIDC 完善 / Push 优化 / Worker 架构激活 | 中期 | **中（保留）** | 保留 v1.3 方向 |

### D. 把"可辩护的覆盖率"变成机器产物（建议新增）

| 编号 | 动作 | 验收判据 |
|---|---|---|
| D1 | 在 SDK/脚本侧增加**上游端点机器抽取**（从 `element-hq/synapse` 的 `synapse/rest/**` 或官方 OpenAPI 导出），产出与 `ROUTE_CONTRACT.md` 同构的清单 | 生成上游清单的脚本可复现；两侧口径一致后**才**允许输出覆盖率百分比 |
| D2 | 本文档的每个数字必须来自 §8 命令或 `ROUTE_CONTRACT.md` | 脚本对故意写错的数字能变红。**对比报告已落地对应门禁**（`tests/unit/doc_credibility_guard_tests.rs` 钉死 route 计数与路径存在性）；**本文档仍无**，是 §1.1 三次漂移（1151→1163→1165）无人拦截的直接原因。<br>⚠️ **本次审查发现并已修复机器门禁红态**：审查中 `EXTRACT_STRICT=1 python3 scripts/contract/extract_registered.py` 曾非 0 退出（`ledger_export_sdk 1 of 1149 labels wrong`：`GET /_synapse/cas/login` 期望 `cas` 实得 `assembly::auth_compat`）。根因：`scripts/contract/ledger_origins.txt` 的 qualifier `@/login` 在 CAS 端点迁入 `/_synapse/cas/`（`d4e22f9ea`）后失配，退回兜底规则（相对路径 `/login` 的 registrar 污染使 `assembly::auth_compat` 胜出）。**已修复**：qualifier 改为前缀 `@/_synapse/cas/`，门禁恢复 0 退出（`ledger_export 1065 labels reproduced`、`ledger_export_sdk 1149 labels reproduced`；本批 L1 落地后为 **1066 / 1150**）。这既证明门禁有效（能变红），也说明其红态本可由一行规则修正 |
| D3 | 文档中出现的 MSC 编号必须已登记在 `MSC_SEMANTICS.md` | ✅ **本版已落地门禁**（`tests/unit/msc_semantics_guard_tests.rs`）：纯谓词 + 红证明，**大小写不敏感**地强制本文档引用的每个 `MSC####` 都能在 `MSC_SEMANTICS.md` §1（语义分歧详表）/ §1.1（引用登记表）找到登记行，未登记即判红。<br>**本次补齐**：本文档共引用 **24** 个编号，原仅 **3** 个（MSC3967 / MSC4155 / MSC4204）已登记；其余 **21** 个已补入 `MSC_SEMANTICS.md` §1.1，官方标题按该表证据约定标注 `*仓库既有结论*`（**未**独立检索官方仓库，禁止据该列反推官方语义） |

---

## 八、复核命令（可复制）

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# ① 路由权威口径（机器生成于 2026-10-02）
grep -n '注册路由条目\|含路由注册的模块文件\|registered_by' docs/synapse-rust/ROUTE_CONTRACT.md | head

# ② 从 ROUTE_CONTRACT.md 抽取路由，复现 §1.1 的三种口径
#    ⚠️ 方法名写成 `[A-Z]+`、且**不要**锚定行尾：部分行带〔always / X 双档注册〕注解，
#       锚定 `$` 会静默少 21 条（实测 1129 ≠ 1150，是本表上一版漂移的一个来源）。
grep -oE '^- `[A-Z]+` `[^`]+`' docs/synapse-rust/ROUTE_CONTRACT.md \
  | sed -E 's/^- `([A-Z]+)` `([^`]+)`$/\1 \2/' > /tmp/mp.txt
wc -l < /tmp/mp.txt                                          # 1154 注册条目 (method,path)
awk '{print $2}' /tmp/mp.txt | sort -u > /tmp/paths.txt
wc -l < /tmp/paths.txt                                       #  915 唯一路径
grep -c '^/_matrix/client' /tmp/paths.txt                    #  491 唯一路径 client
grep -c '^/_synapse/admin' /tmp/paths.txt                    #  228 唯一路径 admin
# 其他命名空间 196 = /_matrix(非 client) 144 + /_synapse(非 admin) 41 + /.well-known 5 + 根级 6
grep -cE '^/(?!/_matrix/|/_synapse/|/\.well-known/)' -P /tmp/paths.txt 2>/dev/null \
  || awk '!/^\/_matrix\/|^\/_synapse\/|^\/\.well-known\//' /tmp/paths.txt | wc -l   # 6

# ③ 逻辑端点口径（折叠版本前缀）
python3 - <<'PY'
import re
paths=[l.strip() for l in open('/tmp/paths.txt') if l.strip()]
def norm(p):
    p=re.sub(r'^/_matrix/client/(v3|v1|r0|v[0-9]+|versions)/','/_matrix/client/vX/',p)
    p=re.sub(r'^/_matrix/client/(unstable|org\.matrix\.[a-z0-9._]*)/','/_matrix/client/vX/',p)
    p=re.sub(r'^/_matrix/client/vX/org\.[a-z0-9._]+/','/_matrix/client/vX/',p)
    p=re.sub(r'^/_synapse/admin/v[0-9]+/','/_synapse/admin/vX/',p)
    return p
L=sorted({norm(p) for p in paths})
print('逻辑端点(全部) =', len(L))                                                   # 807
print('逻辑端点(client) =', sum(1 for p in L if p.startswith('/_matrix/client')))   # 385
print('逻辑端点(admin)  =', sum(1 for p in L if p.startswith('/_synapse/admin')))   # 226
open('/tmp/logical_routes.txt','w').write('\n'.join(L)+'\n')
PY

# ④ 分类统计（§二/§三）：先把 §8.1 的脚本存为 /tmp/classify_routes.py
python3 /tmp/classify_routes.py /tmp/paths.txt          # 唯一路径口径
python3 /tmp/classify_routes.py /tmp/mp_paths_all.txt   # 注册条目口径（awk '{print $2}' /tmp/mp.txt > /tmp/mp_paths_all.txt）
python3 /tmp/classify_routes.py /tmp/logical_routes.txt # 逻辑端点口径

# ⑤ 上游基准与增量（v1.162.0 = 2026-09-29）
gh release list --repo element-hq/synapse --limit 8
gh api "repos/element-hq/synapse/contents/CHANGES.md?ref=v1.162.0" -H "Accept: application/vnd.github.raw" > /tmp/synapse_changes.md
# 1.162 + 1.161 两节（§5.4/§5.5 逐条判定的依据）
awk '/^# Synapse 1\.162\.0 \(/{f=1} f' /tmp/synapse_changes.md | awk '/^# Synapse 1\.160\.0 \(/{exit} {print}'

# ⑥ 关键事实核对（注释为本版 2026-09-29 实测值）
grep -n 'DEFAULT_ROOM_VERSION: &str\|stable_no_create\|stable("12")' synapse-common/src/room_versions.rs   # DEFAULT_ROOM_VERSION="12"（:94）；v1–v11 stable_no_create、v12 stable、v13 已移除
grep -rn 'rc_reports' --include='*.rs' . | grep -v '^./target'   # 已实现：directory_reporting.rs:235,293
grep -rli 'msc4512' --include='*.rs' . | grep -v '^./target'     # 2 个文件（AS 代理已实现）
grep -rli 'msc4502' --include='*.rs' . | grep -v '^./target' | wc -l   # 8 个文件（PARTIAL）
grep -rliE 'msc4262|msc4429' --include='*.rs' . | grep -v '^./target' | wc -l  # 8 个文件（PARTIAL）
grep -n 'msc2965' /tmp/paths.txt                                 # 仅剩 auth_metadata；auth_issuer 已摘除（76e5f9136）

# ⑥.1 v1.162 增量关键事实（§5.4/§5.5 依据；注释为本版实测值）
grep -rn 'MAX_ONE_TIME_KEYS_PER_ALGORITHM_PER_DEVICE' --include='*.rs' .   # 500（device_keys/service.rs:20）
grep -rn 'rc_profile' --include='*.rs' . | grep -v '^./target'            # config/rate_limit.rs:95 + account_compat.rs 应用
grep -n 'username' synapse-common/src/config/database.rs                  # Redis username → redis://user:pass@host（:149,189,192）
grep -rn 'M_UNKNOWN_DEVICE' --include='*.rs' . | grep -v '^./target'      # error/code.rs:99 + device.rs:272
grep -rn 'annotate_allowed_room_ids' --include='*.rs' .                   # hierarchy.rs:203（#20154）
grep -rn 'msc4311_strict_validation' --include='*.rs' .                   # config/federation.rs:238（宽限至 2027-06-01）
grep -rn 'storage_stream_current_position\|get_max_stream_ordering' --include='*.rs' .  # L4 指标 + 事件流位置
grep -rn 'check_event_allowed' --include='*.rs' . | grep -v '^./target'   # module_service.rs（trait+impl+踏板）/messaging/events.rs/membership/*.rs（#19768 已实现）
grep -rn 'register_configured_third_party_rules\|third_party_rules' --include='*.rs' . | grep -v '^./target'  # 触发侧接线：config/mod.rs + module_service.rs + wiring/admin.rs（#19768 生产可达）
grep -rn 'state_after' --include='*.rs' synapse-services/src/sync_service | wc -l  # MSC4222 支持存在（订正 §5.2 旧判）
grep -rn 'action_name' --include='*.rs' . | grep -v '^./target'           # L2：0 命中 ⇒ N/A
grep -rn 'federation_domain_whitelist' --include='*.rs' .                 # L3：0 命中 ⇒ N/A

# ⑦ 能力"是否真的接线"—— 只看模块/配置存在会得出错误结论
grep -rn 'EduType::DelayedEvent' --include='*.rs' synapse-federation/src/edu.rs     # MSC4140 联邦 EDU 已通
grep -rn 'content_scanner\.' --include='*.rs' synapse-services/src synapse-web/src  # 仅 wiring/core.rs（0 生产消费者）
grep -rn 'cascade_redact' --include='*.rs' synapse-web/src/routes/admin/room/mod.rs # MSC3912 仅管理端可达
grep -rn 'redact_event_content' --include='*.rs' synapse-web/src/routes/handlers/room/events.rs  # 客户端撤回不级联
```

### 8.1 分类归属脚本（`classify_routes.py`）

按**有序优先级**把每条路径唯一归属到一类，因此各类可相加 = 该命名空间总数。规则内置于脚本，
关键归属：`/users/{id}/media` → 用户管理（非媒体）；`/rooms/{id}/…` 的全部子资源 → 房间（仅 `sendToDevice`/`msc4140`/`/rooms/{id}/event/` → 消息）。

```python
#!/usr/bin/env python3
"""用法: python3 classify_routes.py <唯一路径文件>"""

import re, sys
from collections import Counter

CLIENT = [
    (
        "认证",
        r"/(login|logout|register|refresh|oidc|saml|cas|rendezvous)"
        r"|/account/(password|deactivate|3pid)|msc2965|msc4108|msc3882|/organizations",
    ),
    (
        "同步",
        r"/sync|/notifications|msc3575|/to_device|/pushrules|/pushers|/push$|/push/",
    ),
    (
        "设备",
        r"/devices|/keys|/room_keys"
        r"|/cross_signing|/dehydrated_device|msc3814",
    ),
    ("搜索", r"/search"),
    ("媒体", r"/media|/upload|/thumbnail|/preview_url"),
    (
        "用户",
        r"/profile|/presence|/user_directory|/thirdparty|/users|/capabilities|/account_data",
    ),
    (
        "消息",
        r"/rooms/[^/]+/(send|messages|receipt|typing|redact|report|read_markers)"
        r"|/sendToDevice|msc4140|/rooms/[^/]+/event/",
    ),
    ("房间", r"."),
]
ADMIN = [
    (
        "用户管理",
        r"/users|/user_sessions|/registration_tokens|/register|/account_validity"
        r"|/whois|/whoami|/account|/password_auth_providers|/user_stats|/invite",
    ),
    ("媒体", r"/media|/quarantine_media|/purge_media_cache|/media_callbacks"),
    ("联邦", r"/federation|/destinations|/server|/version|/rate-limit-status"),
    (
        "安全",
        r"/event_reports|/reports|/policy|/audit|/feature-flags|/experimental_features"
        r"|/background_updates",
    ),
    (
        "房间管理",
        r"/rooms|/retention|/purge_room|/purge_history|/shutdown_room|/spaces"
        r"|/room_stats|/stats|/statistics|/server_notices|/send_server_notice"
        r"|/jitsi|/cleanup|/config|/captcha",
    ),
    ("服务器", r"."),
]

lines = [l.strip() for l in open(sys.argv[1]) if l.strip()]
for key, rules in (("Client", CLIENT), ("Admin", ADMIN)):
    prefix = "/_matrix/client" if key == "Client" else "/_synapse/admin"
    c = Counter()
    for l in lines:
        if not l.startswith(prefix):
            continue
        for name, pat in rules:
            if re.search(pat, l):
                c[name] += 1
                break
    print(f"### {key} (总计 {sum(c.values())})")
    for name, n in c.most_common():
        print(f"  {name}: {n}")
```

---

## 九、关联文档

| 文档 | 用途 | 时效性 |
|---|---|---|
| [`ROUTE_CONTRACT.md`](./ROUTE_CONTRACT.md) | **路由机器权威**（逐模块 `(method, path)`） | 2026-10-02 生成（**1154** 条，与 §1.1 表一致） |
| [`MSC_SEMANTICS.md`](./MSC_SEMANTICS.md) | **MSC 编号语义唯一真相源**（含"借用编号"登记） | 2026-09-14 |
| [`ELEMENT_SYNAPSE_GAP_ANALYSIS_2026-07-28.md`](./ELEMENT_SYNAPSE_GAP_ANALYSIS_2026-07-28.md) | 对标 v1.156.0 的功能级差距分析 | 2026-07-28（基准已落后 5 个版本） |
| [`../audit/COMPARISON_REPORT_REVIEW_2026-09-22.md`](../audit/COMPARISON_REPORT_REVIEW_2026-09-22.md) | 对 `synapse-rust-vs-synapse-comparison.md` 的复核（含 v1.157–1.161 逐条实测） | 2026-09-22 |
| [`../audit/2026-09-23-msc3912-cascade-redaction.md`](../audit/2026-09-23-msc3912-cascade-redaction.md) | MSC3912 关系性级联撤回的实现说明（§5.2 / §6.2 引用） | 2026-09-23 |
| [`../audit/D-12_EVENT_REPORT_HISTORY_STATS_FIX_PLAN.md`](../audit/D-12_EVENT_REPORT_HISTORY_STATS_FIX_PLAN.md) | D-12（`event_reports` `/history` 删除 + `/stats` 静态聚合）的收口记录 —— **§三「安全 −1」的来源** | 2026-09-25 |
| [`../audit/PROJECT_REMAINING_ISSUES_2026-09-14.md`](../audit/PROJECT_REMAINING_ISSUES_2026-09-14.md) | 仓库级「现存问题清单」（本文档 §七 的建议归口于此，不新开 backlog） | 2026-09-14（主线基线；`opt/consolidated` 差异见其 §20 之后追加的轮次） |
| [`LEDGER_EXPORT_SCHEMA.md`](./LEDGER_EXPORT_SCHEMA.md) | ledger 导出格式与 SDK 契约同步口径 | — |

---

*创建日期: 2026-03-19*
*最后更新: 2026-10-02（v1.10：**2026-10-02 台账订正批**，唯一路径 915 / 逻辑端点 807 沿用 v1.9，注册条目 **1152 → 1154**（+2，L-6）——
订正 MSC4297（顶部状态注 / §5.1 / §6.2）、§5.5 #20185（N/A → 等价实现·M-5）与 §5.5 #20160（PARTIAL → 维持等价收口）；
补齐 spec 的 4 段关系路由 `GET …/relations/{eventId}/{relType}/{eventType}`（L-6，见 ⑥）；
跨文档同批更正 `MSC_SEMANTICS.md` / 对照报告 §18.3 #17·§18.4 M-5·L-6 / 优化执行计划 M-5 卡。
以下为 v1.9 的更新说明：随 `feature/2026-10-01-metrics-docs-updates` **合并进 `main`**，
**重算 §1.1 / §二 / §三 三张分类表**（三口径 **1152 / 915 / 807**）并同步 §八 配方注释值；
新入册 **MSC3720 账户状态** 客户端 + 联邦两条路由。不变更任何 MSC/功能判定。
以下为 v1.8 的更新说明：**对齐基准由 v1.161.0 升至 v1.162.0**（发布于 2026-09-29，当前最新稳定版），
`CHANGES.md` 核对范围由 1.157→1.161 扩到 **1.157 → 1.162**；顶部基准注、§五 标题与 intro、
§八 命令与新增的 ⑥.1 关键事实核对清单（11 条 grep）同步，并新增顶部「v1.8 与 v1.7 的差别」段。
**新增 §5.4 / §5.5**「v1.161 → v1.162 增量逐条判定」——v1.162.0 正文为 "No significant changes since 1.162.0rc1"，
条目全来自 **1.162.0rc1**（Features 4 / Bugfixes 11 / Docs 5 / Internal 13，共 **33** 条），逐条映射本仓证据；
**新增 §5.6**「本批 M1–M6 / L1–L5 处理结果」收口表（L1 ✅已实现 / L2 N/A 延后 / L3 N/A / L4 ✅部分落地 / L5 ✅本轮完成 / M5 N/A 仅报告）。
**订正两处旧判**：§5.1 MSC4140 行（`GET /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}` 单事件端点
已落地 handler + 路由，`state_key` 行号 `:94` → `:123`）与 §5.2 MSC4222 行（原判「全仓 `state_after` = 0 / N/A」
**与代码不符**，实际在 sync service 与 handler 中已透传支持，改判 **PARTIAL**）。
**本批 L1 落地**：MSC4140 单事件端点路由入册，route ledger 由 1130 → **1131**（worker 1141 → **1142**）；
**本版不重算 §1.1 三口径**（仍为 v1.7 的 1149 / 913 / 805；本批 L1 因该路径已有 `POST` 注册，重算仅「注册条目」`+1` 得 **1150 / 913 / 805**，见 §1.1 末段）。
**本批 L4 落地**：`synapse_storage_stream_current_position` 为 per-stream 多标签 gauge（2026-10-02 合流后统一为 `StreamPosition::ALL` 的 5 条 series + 30s 循环刷新，见 §5.5 #20097）。
v1.7：按 §8 配方在 HEAD `74bb9c522` 重算 —— 三口径 **1149 / 913 / 805**
（v1.6 基线 `88001b4a9` 为 1135 / 903 / 795，净 **+14 / +10 / +10**）；新增 profile 自定义字段稳定端点
`{key_name}`（+3/+1/+1）、Admin 媒体端点族 10 条（+10/+10/+10）、`invite/{allowlist,blocklist}` 补 `POST`（+2）；
摘除上游 1.161 已删的 `msc2965/auth_issuer`（−1，`76e5f9136`）；CAS 协议 6 条由根级迁入 `/_synapse/cas/`（净 0，
commit `d4e22f9ea`）；§四 对照表、§5.1、§6.3、§7 B4/B5/B7/C4/D2、§八 配方注释值与 §九 同步更新；
**另订正** v1.6 遗留的过时事实：房间版本能力（默认 `"12"`、仅 v12 可创建、v1–v11 `stable_no_create`、v13 已移除，
遗留项仅 MSC4297）与 Profile 语义（稳定 `{key_name}` 已注册、`user_exists` 对已停用账户返回 true）——
见 §5.1/§5.2/§6.2/§7-B5/B7 与顶部状态注；
另记录本次审查发现并**已修复**的机器门禁红态：`ledger_origins.txt` 的 `cas.rs::cas_routes` qualifier 由 `@/login` 改为 `@/_synapse/cas/` 后，`EXTRACT_STRICT=1` 恢复 0 退出（见 §7-D2）。
**§7-D3 一并落地**：新增门禁 `tests/unit/msc_semantics_guard_tests.rs`（纯谓词 + 红证明，大小写不敏感），并在
`MSC_SEMANTICS.md` 新增 §1.1「引用登记表」，补登记本文档引用的 **21** 个此前未登记编号（本文档共引用 24 个，
另 3 个 MSC3967 / MSC4155 / MSC4204 已在该文 §1）。
v1.6：随 E2EE 去服务端私钥重构复算 —— 三口径 1135 / 903 / 795，Client 分类表
房间 206/250/308、设备与密钥 54/84/125，并修正旧表"打印的配方复现不出打印的表"的 ±5 归类偏差；
v1.5 全表按 §8 配方在 HEAD `9e26ee31a` 重算 —— 当时三口径 1165 / 933 / 813 与 §二/§三
分类表全部替换 2026-09-21 快照；§五/§六 判定推进到当前 HEAD，MSC4512 / MSC3912 级联 / MSC4140 联邦 EDU /
Content Scanner 装配 / `rc_reports` / AS 登录六项从「缺失」改为实测状态，其中 Content Scanner 是"已装配但零消费者"；
修正 §8 抽取配方中会静默少 21 条的锚定错误，并补 §⑦「是否真的接线」核对命令）*
