# 契约面优化方案（v1）全面复审与修订（2026-10-19）

> **复审对象**：`docs/audit/BACKEND_CONTRACT_SURFACE_VERIFICATION_2026-10-19.md` §4–§5 的方案（下称 v1）。
> **复审方式**：不重跑业务测试；对 v1 方案依赖的每个集成点逐一落到真实文件核验
> （`wire-format-waivers.json` 的 notice、`ledger_annotations.txt` 的语法与守卫、
> `check-wire-format.mjs` 的桶棘轮、`route-table.json` 的生成链与对账门禁、
> `extract_registered.py` 实测耗时 10.8s）。
> **结论**：v1 的方向（先补导出面、再收口 query、最后分桶请求体）与 P0 选型（sidecar）
> **成立**，但有 **2 条会直接导致门禁变红或跨仓退化的硬伤**（F2、F3）、
> **3 条完整性缺口**（F1、F4、F9）、**5 处工程细节缺失**（F5–F8、F10）。
> 本文给出修订后的 v2 方案；v1 文档的判定部分（§0–§3）不受影响。

---

## 1. 复审维度总表

| 维度 | v1 评价 | 主要问题 |
| --- | --- | --- |
| 功能完整性 | ⚠️ 有缺口 | N-1 的 51 条既有 struct 无 deny **没有批次认领**（F1）；17 条 ambiguous 路由未规定处置（F4）；`route-not-resolved:27` 桶未声明归属（F9） |
| 架构合理性 | ✅ 选型正确 | sidecar 不碰派生表/对账链是对的（见 §3.1 论证）；但 v1 把 A 桶登记进 waiver 文件**违反该文件自己的写入政策**（F2） |
| 性能 | ✅ 无运行时影响 | 全部为静态面；唯一成本是契约链 +~11s（extract_registered 实测 10.8s），可接受但可省（F7） |
| 可维护性 | ⚠️ 有隐患 | waiver 有 expires 语义会周期性变红（F2 连带）；跨仓 pin 滞后时 SDK 会**倒退**（F3）；缺变异自证（F6） |
| 风险 | ⚠️ 两处会咬人 | F2 落地即红、F3 在 pin 滞后窗口即倒退，都是"v1 照做必踩"级别 |

---

## 2. 缺陷清单（按严重度）

### F1 🔴 完整性：N-1 的 51 条没有批次

v1 §3 N-1 自己查出「159 条 typed struct 里 51 条没有 `deny_unknown_fields`」，并把验收判据定为
"deny 覆盖 108→159"——但 §4 的 P2 只排了 **B 桶 82 条 unknown-body** 的批次，
**51 条既有 struct 的 deny 补齐没有任何一个 P 认领**。108→159 的分子分母对不上：
159 是 typed struct 总数，B 桶改完只是让 typed 总数涨到 159+82=241，
那 51 条不加 deny，覆盖率永远卡在 108/241。

**v2**：新增 **P2-C**（见 §4），与 B 桶并行按面分批。

### F2 🔴 正确性：A 桶写进 `wire-format-waivers.json` 落地即红

v1 P2 Step 1 要把 A 桶 59 条登记进豁免表。但该文件头部 notice 原文：

> 「只登记**已回源确认的真缺陷**；**抽取器覆盖不到的情形一律落 wire-format-coverage.json 的桶，
> 不写在这里**」「台账里有但当前没有任何违规与之对应**也红**」

A 桶（协议即任意 JSON）恰好是"抽取器不该覆盖"而非"真缺陷"，且这些路由当前**不产生违规**
（只产生桶计数）——写进去**当场触发"无对应违规"红**。且每条 waiver 带 `expires`，
59 条永久豁免会周期性过期变红。v1 这一步**照做必失败**。

**v2**：A 桶不走 waiver，走**后端注解**（架构上也是正解——"body 是否开放对象"是后端事实，
不该由 SDK 侧正则猜）：

1. `ledger_annotations.txt` 扩展新键 `body_opaque=true`（语法本来就支持 `key=value` 扩展，
   **未知键会被拒绝**——加键要同步改解析器的 known-key 集，typo 即红的设计正好保护它）；
2. `check_annotation_fidelity` 加**正向**守卫：声明了 `body_opaque` 但 handler 不是
   `Json<Value>`/手动读 ⇒ 红。**反向**（手动读但没声明）不做硬红——否则 B 桶 82 条既有债
   当场把门禁打红；改为记入计数棘轮（只降不升），与仓库既有棘轮哲学一致；
3. sidecar 把 `body_opaque` 一并导出；SDK 把这 59 条归入**新桶** `backend-body-opaque`，
   `backend-body-unknown` 145→≈86。桶棘轮是"只降不升"（`check-wire-format.mjs:33`），
   降桶天然合法；新增桶需一次性 `--refresh` 基线。

### F3 🔴 正确性：P0-2 无条件删启发式，与"缺席退化"自相矛盾

v1 P0-2 第 1 条说 sidecar「缺席时退化为现状」，第 4 条却要**删掉**两条启发式兜底。
SDK 读的是**兄弟仓活文件**（`../synapse-rust`），`backendCommit` pin 冻结在 `9d2f152bb`——
pin 滞后窗口内 sidecar 不存在，启发式已删 ⇒ 归属能力**比今天还差**，B-4 的 2 条路由倒退。

**v2**：启发式**保留为 fallback**，仅当 sidecar 在场且该路由有 `exact` 条目时旁路；
删除动作推迟到「pin 已越过 sidecar 引入 commit 且稳定两周」后单独一批（P5），
届时有 `contractConflicts=0` 与桶计数双重判据兜底。

### F4 🟡 完整性：17 条 ambiguous 路由无处置口径

v1 sidecar 的 `resolution` 枚举含 `ambiguous`，但全文没说这 17 条（同一 `(method,path)`
挂多个 handler 候选）导出什么、SDK 怎么消费、验收怎么算。

**v2**：`ambiguous` 条目导出**候选列表** `candidates: [{file,fn},...]`；SDK 对它们维持
fail-closed（与现状等价）；验收口径改为
`collisionRefused ≤ (114 − sidecar 判为 exact 的撞车路由数)`，落地前先按真实导出重算，
不写死 60。

### F5 🟡 一致性：P1 标题"5 个 struct"与正文 8 个不符

v1 P1 表列了 8 个文件 8 个 struct（`KeyRotationCheckParams` 供 2 个 handler 共用，9 条路由）。
**v2** 修正为「8 个 struct / 9 条路由」。

### F6 🟡 方法学：全流程缺变异自证

仓库方法学要求「构造真实缺陷 → 确认 gate 失败 → revert」。v1 五个 P 都没有。

**v2** 每个 P 加变异自证（见 §4 各批验收），核心三条：
P0 把 sidecar 一条 `exact` 改成错的 `file` ⇒ SDK 门禁必须红；
P1 删一条 `query_params=` ⇒ `check_annotation_fidelity` 必须红；
P2 给 deny struct 多发一个字段 ⇒ 集成测试必须 400。

### F7 🟢 性能/维护：sidecar 生成不应二遍解析

`extract_registered.py` 全量实测 **10.8s**。v1 的 `gen_route_handlers.py` 若独立重跑
`load_sources()+Resolver`，契约链 +~11s。**v2**：脚本用 `importlib` 复用模块
（本次复核探针已验证可行），且 `--check` 走内存比对不落盘；总成本仍 ~11s 但只有一次解析。
另：**JSON 必须确定性**（键排序、无时间戳、`schema_version` 显式），否则新鲜度检查天天红。

### F8 🟢 门禁集成：两处职责混淆

- v1 让 `gen_derived_routes.py --check`「增加一项」查 sidecar —— 混淆了派生表生成器的单一职责。
  **v2**：sidecar 自带 `--check`，`check_route_contract.sh` 加**独立步骤**，失败信息独立可读。
- SDK 侧有 `check-contract-provenance.mjs`，新消费产物须登记来源，否则 provenance 门禁红。
  v1 未提。**v2** 列入 P0-2 任务清单。

### F9 🟢 范围声明：`route-not-resolved:27` 无人认领

基线里还有这个桶（27 调用点），v1 全文未提。它不属后端（是 SDK 侧路由解析），
但不声明就会被误认为遗漏。**v2**：显式标注「SDK 侧问题，本方案不含，另立票」。

### F10 🟢 跨仓协同：pin 推进流程缺失

P0-1 合并后 SDK 侧要：(a) bump `backendCommit` pin、(b) 分别在 sidecar **在场/缺席**两种状态下
跑门禁证明都绿。**v2** 写入执行顺序。

---

## 3. v1 复审后确认**成立**的部分（不动）

### 3.1 sidecar 选型是对的（备选已排除）

复审时认真评估了「不新增文件、把 handler 列并进 `docs/openapi/route-table.json`」的备选：

- `route-table.json` 由 `scripts/api_test/gen_route_table.py` 从**运行时 ledger 导出**生成，
  是「golden lane × default profile」的 VIEW，受 `check_route_surface_reconciliation.py`
  R1/R2 对账；handler 定义文件是**静态源码事实**（`extract_registered.Resolver`）。
  两种来源混进一个文件会污染 provenance（SDK 侧还有 `check-contract-provenance.mjs`），
  且动它必然触及对账门禁。**sidecar 独立产物 + 独立 provenance 是正确解。**
- `artifacts/registered_routes.json` 按模块分组、**未入 git**，不是契约面，不可用。

### 3.2 判定与分桶不受影响

v1 §0–§3（B-1..B-6 判定、N-1..N-3、A/B 分桶规则）本次复审未发现错误；
B-4 维持不动、P3 需用户裁决，均不变。

---

## 4. 修订后方案 v2（差异加粗）

### P0 导出 handler 归属（目标不变：B-1、B-4 一起消失）

**P0-1 后端**
- 新增 `scripts/contract/gen_route_handlers.py`：`importlib` 复用 `extract_registered`
  **（不二遍解析，F7）**；产出 `docs/synapse-rust/ROUTE_HANDLERS.json`，
  **确定性输出：键排序、无时间戳、显式 `schema_version`（F7）**；
  条目含 `resolution: exact|unresolved|ambiguous`，**ambiguous 带 candidates 列表（F4）**；
  **同时导出 `body_opaque` 注解（F2 依赖）**。
- 自带 `--check`（内存比对），**作为独立步骤进 `check_route_contract.sh`，
  不塞进 `gen_derived_routes.py`（F8）**。
- 同步扩展 `ledger_annotations.txt` 的 `body_opaque=true` 键 + 解析器 known-key 集 +
  `check_annotation_fidelity` **正向**守卫；反向债走计数棘轮**（F2）**。

**P0-2 SDK**
- 读 sidecar，缺席退化现状；`exact` 命中时按 `file` 精确归属；
  **两条启发式保留为 fallback，仅 sidecar 在场时旁路（F3）**；
  `body_opaque` 路由归入新桶 `backend-body-opaque`，一次性 refresh 桶基线**（F2）**；
  新产物登记 `check-contract-provenance`**（F8）**。
- **验收口径不写死**：`collisionRefused ≤ (114 − exact 命中数)`，按真实导出重算**（F4）**；
  `contractConflicts` 必须仍为 0；ban/kick/unban/invite/account_data 五条必须 HAS。
- **变异自证（F6）**：改错一条 sidecar `file` ⇒ SDK 门禁红 ⇒ revert。

### P1 收口 B-3（**8 个 struct / 9 条路由，F5**）

内容与 v1 相同（8 文件 8 struct + `query_params=` 注解 + Query struct 禁 deny 的硬约束）。
**加变异自证**：删一条注解 ⇒ fidelity 红 ⇒ revert**（F6）**。
验收命令不变（`check-query-param-coverage.mjs` 已核实存在）。

### P2 B-2 分桶收口（**A 桶改道，新增 P2-C**）

- **Step 1（替换 v1 的 waiver 路线）**：A 桶 59 条登记 `body_opaque=true` 注解，
  随 P0-1 的注解扩展落地；SDK 桶 145→≈86。**不写 waiver 表（F2）**。
- **Step 2**：B 桶 82 条分批同 v1（5 批，按调用频次排序，admin/vendor 面加 deny）。
- **Step 3（新增 P2-C，F1）**：N-1 的 51 条既有 struct 逐条三分类——
  admin/vendor 面 ⇒ 补 `deny_unknown_fields`；client/federation 面 ⇒ 保持开放 +
  rustdoc 写明理由；边界条目逐条裁决。验收判据从 v1 的「108→159」修正为
  「admin/vendor 面 typed struct 的 deny 覆盖率 = 100%」。
- 每批变异自证**（F6）** + v1 既有验收（clippy/nextest/门禁）。

### P3 N-2 双实现行为一致性

不变，**仍需用户裁决**「admin 面 ban/kick/unban 是否同步 friend-DM 房间成员状态」。

### P5（新增，后置）清理启发式

pin 越过 sidecar 引入 commit 且稳定后，删 P0-2 保留的两条 fallback**（F3 的尾巴）**。

---

## 5. 优化前后对比

| 项 | v1 | v2 | 差异原因 |
| --- | --- | --- | --- |
| A 桶 59 条去向 | 写 `wire-format-waivers.json` | 后端 `body_opaque` 注解 → sidecar → SDK 新桶 | waiver notice 禁止 + 落地即红（F2） |
| SDK 启发式兜底 | 立即删 | sidecar 在场才旁路，pin 稳定后才删（P5） | pin 滞后窗口会倒退（F3） |
| N-1 51 条 | 无批次 | P2-C 三分类补齐 | 验收数 108→159 永远达不到（F1） |
| ambiguous 17 条 | 枚举里有、处置没有 | 导出 candidates，SDK fail-closed，口径重算 | 验收数字无法核对（F4） |
| P1 规模表述 | "5 个 struct" | 8 struct / 9 路由 | 文内自相矛盾（F5） |
| 变异自证 | 无 | 每 P 必带 | 仓库方法学硬性要求（F6） |
| sidecar 生成 | 独立脚本（隐含二遍解析） | importlib 复用 + 确定性 JSON + 独立 `--check` 步骤 | 省 11s、防 diff 噪声、职责单一（F7/F8） |
| provenance 登记 | 未提 | P0-2 任务清单内 | 否则 SDK 门禁红（F8） |
| route-not-resolved:27 | 未提 | 显式声明 SDK 侧另立票 | 范围闭合（F9） |
| 跨仓 pin 推进 | 未提 | 执行顺序含 pin bump + 双状态验证 | 落地顺序可执行（F10） |
| 判定（§0–§3）、P0 选型、P1 内容、P2-B 分批、P3 | 维持 | 维持 | 复审确认成立（§3） |

## 6. 执行顺序（v2）

```
P0-1a 注解扩展(body_opaque) + P0-1b sidecar 生成/门禁   (后端, 同一批或两批)
  └─→ SDK pin bump + P0-2 (在场/缺席双状态验证 + 变异自证)
P1 query structs + 注解 (可与 P0 并行, 不互相依赖)
P2-A body_opaque 登记 (随 P0-1a) → P2-B 82 条分批 → P2-C 51 条补齐
P3 等裁决 (独立)    P5 pin 稳定后清理 fallback (后置)
route-not-resolved:27 → SDK 侧另立票 (F9)
```

**判绿标准不变**：只认门禁退出码（§5 of v1 的五条命令），另加
`python3 scripts/contract/gen_route_handlers.py --check`。

---

## 7. 复审边界

- F2 的「反向债走计数棘轮」是设计建议，棘轮初值需在 P0-1a 落地时按实测数写入
  （已于 §8.6 落地：`scripts/ci/check_deny_unknown_fields_ratchet.py`）。
- v2 未重新实跑 SDK 全量门禁组合（sidecar 尚不存在，跑了也是现状）；所有「必红/必退」
  结论来自对门禁源码与政策的静态阅读（引文均在 §2 各条内）。
- P2-C 的 51 条三分类清单未逐条列出，落地时先产分类表再动手。

## 8. P2-C 裁决与落地（2026-10-21）

v2 的 P2-C 写的是「N-1 的 51 条既有 struct 逐条三分类 —— **admin/vendor 面 ⇒ 补
`deny_unknown_fields`**」。落地时按实际存量重新量了一遍，结论要改一半。

### 8.1 存量（实测，不是估）

`synapse-web/src` 下 `pub struct` + `Deserialize` 共 **240** 个，其中 **86** 个没有
`deny_unknown_fields`（v2 说的"51 条"是 2026-10-19 的口径，两天里涨了）。按落在哪一面分：

| 面 | 数量 | 说明 |
|---|---|---|
| `routes/admin/**` | **15** | 本次处理对象（13 个 Query + 2 个嵌套 Body） |
| client / federation 面 | 66 | 协议规定"任意 JSON"或上游本就忽略未知字段 |
| 响应/工件结构（`module.rs`、`ledger_export.rs`、`push.rs` 的 `PushRule` 等） | 18 | 服务端自己构造，deny 无意义 |

**P1 状态同步更新**：B-3 已收口——全仓已无活的 `Query<HashMap>` / `Query<Value>`
（只剩 doc 注释在讲历史），`ledger_annotations.txt` 里 204 条 `query_params` 注解，
sidecar 的 `query: typed` 覆盖 203 条。

### 8.2 裁决：不是"一律补 deny"，而是"每条都要有决定"

原方案会让 admin 面比上游更严——`MediaPageQuery` 等 4 个 struct 的代码里就写着：

> Deliberately **not** `deny_unknown_fields`: the handlers used to take a bare
> `HashMap<String, String>` and ignored unknown parameters.

也就是说旧 handler 的语义**就是**忽略未知参数，上游 Synapse 同样如此。收紧会让现有
admin 工具（带 `_=` 缓存穿透、带未来参数、带前端表单多余字段）直接 **422** —— 这正是
这几批一直在收敛的"收紧过头"形态。所以按面分开：

| 面 | 裁决 | 依据 |
|---|---|---|
| **Body**（POST/PUT 的 JSON） | **默认 deny**，且**内层必须与外壳一致** | body 里拼错字段名 = 配置静默不生效，代价高 |
| **Query**（GET 的查询参数） | **保持开放**，但必须逐条登记 | 旧语义=忽略，上游=忽略；收紧只带来 422 |

### 8.3 本次落地

**① 修 2 处"严格外壳 + 宽松内核"**（自洽性缺陷，不是政策）：

| 外壳（已 deny） | 内层（原先没有） | 后果 |
|---|---|---|
| `ServerNoticeRequest` | `NoticeContent` | 外层承诺"未知字段会被拒绝"，实际在 content 里被静默丢弃 |
| `BatchCreateUsersRequest` | `BatchCreateUser` | 同上：`users[]` 每一项的拼错字段无声消失 |

**② 9 个静默默认的 Query 补显式说明**（0 行为变化）：`InviteListParams`、
`MediaQuarantineChangesQuery`、`GlobalMediaQuarantineChangesQuery`、`PurgeMediaCacheQuery`、
`RoomReportsQuery`、`RoomMembersQuery`、`RoomMessagesQuery`、`UserRoomsQuery`、
`AdminUsersV2Query` —— 每个加一行"有意开放未知字段"的 rustdoc，理由与已写明的 4 个一致。

**③ 登记表 + 双向断言**（`test-support/admin_dto_open_fields.txt` +
`synapse-web/src/routes/admin/dto_guard_tests.rs`）：

- admin 面每个 `Deserialize` struct 必须 **deny 或登记**，二者必居其一；
- 登记了却已 deny（或 struct 已删）⇒ 红（登记过期）。

**④ 变异自证**：从登记表删掉 `RoomMessagesQuery` ⇒ 红并精确报
`src/routes/admin/room/mod.rs:150 RoomMessagesQuery — no deny_unknown_fields, not registered` ⇒ 还原后绿。

### 8.4 守卫自己踩过的坑（值得记）

第一版判据是"属性块里**字符串**含 `deny_unknown_fields`"。而 4 个"有意不加"的 struct
文档里写的正是 *Deliberately **not** `deny_unknown_fields`* —— 于是它们被判成"已经加了"，
登记表被报 4 条"过期条目"。**按文本匹配代码属性时，必须排除注释行**：现在只看 `#[` 开头的真属性。

### 8.5 仍未做（与 v2 一致）

- **P2-B**（B 桶 82 条请求体加 struct）：跨仓（后端加 struct + SDK pin bump），本仓单方面做没有意义；
- **P3**（N-2 同名 handler 双实现的行为一致性）：**仍需用户裁决**——admin 面
  `ban`/`kick`/`unban` 是否要同步 friend-DM 房间的成员状态；
- **P5**（pin 稳定后删启发式 fallback）：后置。

### 8.6 deny 覆盖率的反向棘轮（2026-10-21）

§8.3 只解决"这一次补齐"。重构合文件、挪模块、删一个看似没用的属性，都会让覆盖静默
回退而没有任何测试会红——所以补一道**只许升不许降**的计数棘轮：

| 落点 | 作用 |
|---|---|
| `scripts/ci/check_deny_unknown_fields_ratchet.py` | 唯一实现（扫描、判红、`--update`、`--self-test`） |
| `scripts/ci/deny_unknown_fields_baseline` | 每行 `<count> <path>`，基线值由 `--update` 写入，**本文件不手写明细** |
| `.github/workflows/ci.yml`（Clippy-allow 之后） | 接线；缺基线/扫到 0 站点均 fail-closed |

**口径**（写在脚本 docstring，不复述）：容器级 `#[serde(… deny_unknown_fields …)]`
算 1 个站点；先剥注释与字符串（否则 §8.4 的 15 处"Deliberately **not**"会被算成已 deny）；
只扫与其它棘轮相同的 9 个生产源码根（`tests/`、`benches/` 里的属性是夹具，不计）；
按文件比对，**站点数下降或文件消失** ⇒ 红，上升 ⇒ 绿并提示 `--update` 收紧。

**关于"68 个文件"**：2026-10-21 实测，没有任何一种口径复现 68 ——
工作树含 token 75 文件 / tracked 74 文件 / 剥掉注释与字符串后 57 文件（含 `tests/`）/
其中落在 9 个生产源码根的 56 文件。差异不在"取整"，而在口径本身（是否含注释、
是否含 `tests/`、按站点还是按文件），且近 12 个提交的 tracked 文件数在 72–74 之间波动，
从未落到 68。
因此基线**以脚本口径跑出来的值为准**（`--update` 生成），不取任何口头数字：
这与第三批-1 把覆盖率三口径收敛到 `route_metrics.py` 是同一条原则——度量只能有一个来源。

**红证明**：删掉 `routes/admin/cleanup.rs` 唯一的那一个属性 → `178/56 → 177/55`、exit 1
且点名该文件；还原 → exit 0。`--self-test` 另含正样本（单行/组合/跨行写法）、三个负样本
（行注释、块注释、字符串字面量）、以及临时树端到端（删 1 个属性必翻红）。
