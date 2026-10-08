# 混合模块 55 条私有端点 —— 优化方案

> **性质**：**方案**（本文件不改任何代码，只给批次、判据与验收）
> **基线**：`main` @ `86b44d65c`（2026-10-08）
> **上游基准**：Matrix Client-Server API（稳定端点集）+ 项目自述的私有面规范位
> `/_matrix/vendor/v1`（`synapse-web/src/routes/assembly.rs:93-98`）
> **关联**：[`后端冗余清除与功能完善优化方案-2026-10-08.md`](../后端冗余清除与功能完善优化方案-2026-10-08.md) §3 Batch 4、
> [`前缀命名空间治理方案-2026-10-08.md`](../前缀命名空间治理方案-2026-10-08.md)、
> [`UNRESOLVED_ISSUES_SUMMARY.md`](./UNRESOLVED_ISSUES_SUMMARY.md)
> **方法**：先取证再动手。每条给「判据 + 路径证据 + 复现命令」；每批给「删除/迁移后能力由谁承接」与「变异自证」。

---

## 0. 结论先行

1. **55 条不是 55 个问题。** 按语义归属可分成 **4 类**。初稿估计"约 10 条是『同 handler 重复 /
   版本孪生 / 死别名』，可零风险删"—— **M0/M1 实测把这个估计修正为 1 条**（见 §2 类别 A 的
   A2/A3 改判）：同 handler 挂两条路径的多数是**刻意的探测位 / 兼容位**，不是死代码。
2. **消费者取证必须用严格判据。** 门禁自带的 `path_match()` 是**宽松**的（为兼容风格 2/3 而设计），
   用它枚举"某端点是否被 SDK 调用"会**假阳泛滥**：实测 SDK 的
   `buildMembershipChangePath()`（`/rooms/$room_id/$membership`，变量尾段）一处就"匹配"了 **27 条**
   后端字面量路径。判定消费者必须用**变量段只配变量段**的严格谓词（见 §1.2 复现命令）。
3. **严格判据下的实测结论**：SDK 源码字面量层面 **12 条有消费者 / 43 条无**；Tjg 侧另有一条
   **零消费的死常量**（`paths/room.ts` 的 `ANTI_SCREENSHOT` / `VAULT_DATA`）。
4. 批次：**M0 门禁补强**（零路由变化）与 **M1 删冗余面**（后端 + Tjg）—— **两者已执行完毕**
   （见 §3）；**M2 MSC 归位**（`sticky_events` / `pinned_events` 迁 unstable 前缀，须先裁定
   COMPAT-09 的"稳定/unstable 双前缀并存"口径）→ **M3 vendor 迁移**（跨仓）→ **M4 的 D1/D2 收尾裁定**
   —— 待续。
5. **顺带发现两处门禁盲区**（都不在 55 条清单内，但同源）：
   - **G-01 只覆盖 `/_matrix/client/{v1,v3}`，不含 `/unstable/`** ⇒
     `/_matrix/client/unstable/uk.half-shot.msc2666/user/mutual_rooms` 与 vendor 的
     `/user/mutual_rooms` **挂同一 handler**（`get_mutual_rooms`）却从未被守卫发现；
   - **`create_private` 的 v1/v3 版本孪生**（同 handler `create_private_room`）也无人看住。

---

## 1. 事实基线

### 1.1 构成

| 维度 | 值 | 取证 |
| --- | --- | --- |
| 混合模块 client 路由总数 | **104**（M1 前 105；`room.rs` 93 / `moderation.rs` 4 / `handlers/thread.rs` 7） | `scripts/contract/mixed_module_client_routes.txt` |
| 其中 **private** | **54**（M1 前 55；`room.rs` 53 / `handlers/thread.rs` 1） | 同上，第 3 列标签 |
| 其中 msc | 7 | 同上 |
| 其中 spec | 43 | 同上 |
| 门禁覆盖 | 判据 D 双向 ratchet（新增即红） | `test_extract_registered.py::check_standard_prefix_bucket` |

> ⚠️ 口径提醒：`LEDGER_CEILING = 5` 的口径**只统计 `WHOLESALE_PRIVATE_FILES`（整模块私有文件）**，
> 混合模块的私有面**从未**被它覆盖 —— 因此这 55 条既不进台账、也不受 ceiling 约束。

### 1.2 消费者取证（含方法论警告）

**严格谓词**（变量段只与变量段配对，字面量必须全等）：

```bash
python3 - <<'EOF'
import importlib.util, pathlib, collections, sys
sys.path.insert(0, "scripts/contract")
spec = importlib.util.spec_from_file_location("cov", "scripts/contract/check_sdk_route_coverage.py")
cov = importlib.util.module_from_spec(spec); spec.loader.exec_module(cov)

def strict_match(sdk_rel, backend):
    w, h = cov.segments(sdk_rel), cov.segments(backend)
    for start, seg in enumerate(h):
        if seg != w[0] or len(h) - start != len(w): continue
        if all((a == b) or (a == "{}" and b == "{}") for a, b in zip(w, h[start:])):
            return True
    return False
# （对 55 条 private 逐一跑 collect_sites(sdk) 的站点，见本方案 §2 的结论）
EOF
```

**结论**：

| 类别 | 条数 | 说明 |
| --- | ---: | --- |
| SDK 源码字面量有站点 | **12** | 集中在 `src/room-summary/sub-managers/*` 与 `src/room-member/index.ts` |
| SDK 无站点 | **43** → M1 后 **42** | 不代表"零消费者"，见下方 Tjg 与"假能力"讨论 |

> ⚠️ **为什么不能只看 SDK**：Tjg 有 `src/services/matrix/paths/room.ts` 这样的**裸路径常量表**
> 与 `RoomMessageQueuePanel.vue` 这样的 UI。判定"能否删"必须**三层都查**（后端 → SDK → Tjg），
> 且 Tjg 的常量**也要查消费点**（`ANTI_SCREENSHOT` / `VAULT_DATA` 就是"有定义、零消费"）。

### 1.3 Tjg 侧实测

| 项 | 结论 | 证据 |
| --- | --- | --- |
| `paths/room.ts` `ANTI_SCREENSHOT` | **死常量（零消费）** | 全仓仅定义处命中：`src/services/matrix/paths/room.ts:36` |
| `paths/room.ts` `VAULT_DATA` | **死常量（零消费）** | 同上 `:66` |
| `unread_count` | 有语义消费（本地账本对账） | `src/stores/domains/chat/sessionUnread.ts:17-32` |
| `message_queue` | 有 UI | `src/components/room/RoomMessageQueuePanel.vue` + i18n 键 |
| `turn_server` | 有引用 | 5 个文件（需逐条确认是走 SDK 还是裸调） |
| `external_ids` / `event_perspective` / `reduced_events` / `service_types` | **零命中** | 全仓 grep = 0 |

> 与 Batch 2 的 Tjg `MATRIX_EXTERNAL_SERVICES` 死常量**同型**：先有常量、后删消费点，
> 常量被留下。这类常量应在同一批次删除，否则会长期伪装成"有人在用"。

---

## 2. 分类清单与处置

> 分类依据：**语义归属**（是否属 Matrix 规范 / MSC / 项目私有扩展）+ **是否与既有端点重复** +
> **是否有消费者**。判为"语义重叠"的条目**需先逐条复核 handler 实现**再动手（本表给出复核入口）。

### 类别 A — 冗余面（同 handler 重复 / 版本孪生 / 死别名）：**直接删，零跨仓**

| # | 路径 | 判据 | 处置 |
| --- | --- | --- | --- |
| A1 | `POST /_matrix/client/v1/rooms/create_private` | 与 v3 版**同 handler** `create_private_room`（`room.rs:150-151`）；SDK 只打 v3（`RoomManager.ts:345,367` 用 `rp("/rooms/create_private")`），v1 三层零消费者 | ✅ **已删（M1）**，保留 v3 |
| A2 | `/_matrix/client/unstable/uk.half-shot.msc2666/user/mutual_rooms` | 与 `/_matrix/vendor/v1/user/mutual_rooms` **同 handler** `get_mutual_rooms` ⇒ 初判为死别名 | ❌ **改判为保留**：SDK 的 `server-capabilities/index.ts:424` 用它做 MSC2666 **特性探测**（`RoomManager.ts:1069` 注释亦区分"unstable 探测位 vs 稳定入口"）⇒ 已登记 `spp.MSC_KEEP` |
| A3 | `/_matrix/client/unstable/org.matrix.msc4156/threads/subscribed` | 与 vendor 的 `/threads/subscribed` **同 handler** ⇒ 初判为死别名（**M0 判据新报出**） | ❌ **改判为保留**：`handlers/thread.rs:191-196` 自述"仅为已发布客户端保留的兼容位，**不是** MSC4156 表面" ⇒ 已登记 `spp.MSC_KEEP` |

> **A2/A3 的改判过程值得记下来**：M0 判据（G-01 扩展到 `/unstable/`）先把这两条报红，
> 取证后才发现**"同一 handler 挂两条路径"不等于"死别名"** —— 一条是特性探测位、
> 一条是刻意兼容位。`spp.MSC_KEEP` 这个豁免入口本就是为这种情况留的。
> ⇒ **净删除只有 A1 一条**（−1），这与 §0 初稿"约 10 条可零风险删"的估计相差很大。

- **复现**：`grep -nE '\.route\(' synapse-web/src/routes/room.rs` 后按 handler 归组（见 §5 命令）。
- **承接**：无能力损失（保留的那条覆盖同一 handler）。

### 类别 B — MSC 归属但前缀位不规范：**归位到 unstable 前缀**

| # | 路径 | 问题 | 处置 |
| --- | --- | --- | --- |
| B1 | `GET/POST /_matrix/client/v3/rooms/{room_id}/sticky_events`、`DELETE .../sticky_events/{event_type}` | MSC4354；当前借稳定 v3 前缀 | 迁 `/_matrix/client/unstable/org.matrix.msc4354/...`（**与 COMPAT-09 的"稳定/unstable 双前缀并存"口径需先裁定**） |
| B2 | `GET/POST /_matrix/client/v3/rooms/{room_id}/pinned_events`、`DELETE .../{event_id}` | MSC3946；同上 | 同上 |
| B3 | `POST /_matrix/client/v1/rooms/{room_id}/threads/{thread_id}/unfreeze` | **不是** MSC3856 形状（MSC3856 只有 threads/replies/subscribe/unsubscribe） | 迁 vendor |
| B4 | `GET /_matrix/client/v3/rooms/{room_id}/thread/{event_id}`（单数 `thread`） | 非规范形状；MSC3440/3856 用的是 `/threads/{...}` | 迁 vendor 或删（**须先查消费者**） |

### 类别 C — 私有扩展，**实测有消费者**（迁 vendor，必须跨仓同批）

| # | 路径 | 消费者（严格判定） |
| --- | --- | --- |
| C1 | `GET /rooms/{room_id}/keys/{event_id}` | `src/room-summary/sub-managers/room-thread-manager.ts:56` |
| C2 | `GET /rooms/{room_id}/thread/{event_id}` | 同上 `:83` |
| C3 | `GET /rooms/{room_id}/receipts/{receipt_type}/{event_id}` | `room-event-operation-manager.ts:264` |
| C4 | `GET /rooms/{room_id}/fragments/{user_id}` | 同上 `:520` |
| C5 | `GET /rooms/{room_id}/device/{device_id}` | 同上 `:542` |
| C6 | `GET /rooms/{room_id}/event/{event_id}/url` | 同上 `:564` |
| C7 | `POST /rooms/{room_id}/translate/{event_id}` | 同上 `:594` |
| C8 | `POST /rooms/{room_id}/convert/{event_id}` | 同上 `:623` |
| C9 | `PUT /rooms/{room_id}/sign/{event_id}` | 同上 `:648` |
| C10 | `POST /rooms/{room_id}/verify/{event_id}` | 同上 `:677` |
| C11 | `DELETE /rooms/{room_id}/sticky_events/{event_type}` | 同上 `:785` |
| C12 | `POST /rooms/{room_id}/get_membership_events` | `src/room-member/index.ts:154` |

### 类别 D — 私有扩展，**SDK 无站点**（43 条）：**先分类，再决定删或迁**

| 子类 | 条目 | 建议 |
| --- | --- | --- |
| D1 **疑似语义重叠**（须逐条复核 handler 后决定删/收敛） | `GET /rooms/{}/visibility`、`PUT /rooms/{}/visibility`（规范位是 `GET/PUT /directory/list/room/{roomId}`）；`GET /rooms/{}/membership/{user_id}`（与 `/members`）；`GET /rooms/{}/invites`（与 `/members?membership=invite`）；`PUT /rooms/{}/room_keys/keys`（规范是 `PUT /room_keys/keys`，非 room-scoped）；`GET /rooms/{}/keys`、`keys/count`、`keys/version`、`POST keys/claim`（与 E2EE 全局 `/keys/*` 面）；`POST /rooms/{}/search`（与 `/search` + `/relations`）；`GET /rooms/{}/sync`、`timeline`、`message_queue`、`reduced_events`（与 `/sync`、`/messages`）；`POST /invite/{room_id}`（与 `POST /rooms/{room_id}/invite`）；`GET /rooms/{}/members/recent`（与 `/members`） | **先复核"是否只是另一条路径的等价写法"**；是 ⇒ 删重复的那份 |
| D2 **纯私有能力，无消费者**（Tjg 也零命中） | `anti_screenshot`(GET/PUT)、`vault_data`(GET/PUT)、`external_ids`、`event_perspective`、`reduced_events`、`service_types`、`rendered/`、`resolve`、`metadata`、`permissions`、`capabilities`、`notifications`、`version`、`retention`、`spaces`、`event/{event_id}/url` 之外的 `encrypted_events`、`device/{device_id}`、`room_keys/keys`、`POST rooms/create_private`(v3)、`POST rooms/{}/keys/claim` | **待裁定**：删（无任何消费者）还是迁 vendor（保留能力） |
| D3 **有 Tjg 侧使用但非 SDK 路径** | `unread_count`、`message_queue`、`turn_server`、`fragments`、`anti_screenshot`、`vault_data` | 迁 vendor（保留能力），Tjg 调用点同批改 |

---

## 3. 分批执行方案

> 跨仓顺序（工作区 `CLAUDE.md`）：**后端 → SDK → Tjg**；回滚反向。
> 每批"路由面变化"必须走完整契约链（后端冗余清除方案 §5.1 的 9 步）。

### M0 — 门禁补强（**零路由变化，可立即做**）

| 项 | 动作 | 自证 |
| --- | --- | --- |
| M0-1 | 扩展 `check_client_prefix_vendor_twins`：把 `/unstable/` 前缀也纳入"client 侧"（当前只比 `CLIENT_PREFIX_BASES` 的 v1/v3/r0） | 注入一条 `/unstable/x` ∩ vendor 孪生 ⇒ 必须红；实测当下就报出 A2 那条 |
| M0-2 | 新增判据「同一 handler 不得挂多条路径，除显式登记」 | 实测当下报出 A1（`create_private_room`）与 A2（`get_mutual_rooms`） |
| M0-3 | 冻结清单增加 `MIXED_MODULE_PRIVATE_COUNT = 55`（只减不增，与 `LEDGER_CEILING` 同型） | 注入一条私有 client 路由 ⇒ 必须红（判据 D 已覆盖，此处是**数值**上的护栏） |

### M1 — 删冗余面（后端 + Tjg，**零跨仓依赖**）✅ **已执行**

| 项 | 动作 | 结果 |
| --- | --- | --- |
| 后端 A1 | 删 `POST /_matrix/client/v1/rooms/create_private`（与 v3 同 handler 的版本孪生，SDK 只打 v3） | ✅ 注册路由 **1,035 → 1,034**；`route-table.json` 956 → **955** |
| 后端 A2/A3 | 两条 unstable 路径经取证**改判为保留**（有消费者 / 是兼容位），登记 `spp.MSC_KEEP` | ✅ 见 §2 类别 A |
| Tjg | 删 `ROOM.ANTI_SCREENSHOT` / `ROOM.VAULT_DATA`（零消费死常量）与其 **3 处**测试用例 | ✅ `paths` 套件 142 passed；`vue-tsc --noEmit` 无输出 |
| 契约链 | extract → **bootstrap 派生表** → 两车道 fixtures → `--check` 认证 → ROUTE_CONTRACT.md → route-table → 快照 | ✅ 派生表 `--check OK`（1036 rows）；集成快照 2 passed |
| 计数同步 | `comparison.md` 3 处 + `API_COVERAGE_REPORT.md` 6 处 | ✅ `doc_credibility` 通过 |

- **⚠️ 本轮踩到并确认了契约链的死锁与正确绕行**：改了路由面后直接跑
  `gen_derived_routes.py` 必然 `FIXTURE FIDELITY FAILED` —— 因为它写表前先
  `verify_fixtures()`，而 fixtures 是**派生表的下游**，两侧都改不动。
  脚本自带 `--bootstrap`（帮助文本写明了顺序）。**正确链路**：
  `extract → gen_derived_routes --bootstrap → 重导两条车道 fixtures → gen_derived_routes --check`（这一步才是认证）。
  我本轮先按"源码 → fixtures"的顺序跑了一遍 ⇒ **白跑一次**（fixtures 从旧派生表导出，仍含已删路由）。
- **Tjg 侧附带发现**：整个 `ROOM` 常量表（30+ 常量）**零生产消费**（只有 3 个测试与
  `paths/index.ts` 的重导出引用它），且它**未被 FT-120「无死路径常量」的全等断言覆盖**
  （该测试对 ROOM 只做 `not.toHaveProperty` 弱断言，对 MODERATION/AUTH/VOICE 才是全等断言）。
  ⇒ 建议单独立项：要么整表删除，要么补 FT-120 全等断言把它纳入治理。**本轮不动**（避免扩大范围）。

### M2 — MSC 归位（类别 B）

- 内容：`sticky_events`（MSC4354）、`pinned_events`（MSC3946）迁 `/_matrix/client/unstable/<msc>/...`；
  `unfreeze`、`thread/{event_id}` 迁 vendor。
- **前置裁定**：COMPAT-09 已登记"稳定/unstable 双前缀并存"为 by-design —— 本批若改变该口径，
  **须同步改 `MSC_SEMANTICS.md` 与 `COMPAT-09` 的登记**，不能只改代码。
- 消费者：`DELETE sticky_events/{event_type}`（C11）必须先迁 SDK；其余 sticky/pinned 的 SDK 站点需复核。

### M3 — vendor 迁移（类别 C + D3，**跨仓**）

- 内容：其余私有端点 → `/_matrix/vendor/v1`，**不留 client 别名**。
- 顺序：后端改挂载 → 契约链重生成 → SDK 改 `prefix:` 并重生成生成物 → Tjg 重打包 pin → 端到端冒烟。
- ⚠️ **这是发布面变更**：后端已迁、SDK 未迁的窗口内客户端会 404（与 Batch 3 尾巴同一个坑）。
  ⇒ **必须两端同批**，不能像 Batch 3 那样只做后端。
- 规模建议：按 SDK sub-manager 切分（`room-summary/*` 一批、`room-member/*` 一批），每批 ≤ 10 条。

### M4 — 类别 D1/D2 的收尾裁定

- D1：逐条复核 handler，判定"是否等价写法"。等价 ⇒ 删；不等价 ⇒ 迁 vendor。
- D2：**列出全部 43 条（M1 后 42）的三层消费者取证结果**，逐条在"删 vs 迁"上签字（本表给的是建议，不是结论）。

### 3.1 排期总表与跨仓前置（2026-10-08）

| 批次 | 后端 | SDK | Tjg | 前置 / 阻塞 |
| --- | --- | --- | --- | --- |
| **M0 门禁补强** | ✅ 已执行（`313438dc0`） | — | — | — |
| **M1 删冗余面** | ✅ 已执行（同批，净 −1） | — | ✅ 已执行（`7321f8ed`） | — |
| **M2 MSC 归位** | `sticky_events`(MSC4354) / `pinned_events`(MSC3946) 迁 unstable；`unfreeze`、`thread/{event_id}` 迁 vendor | 相应 `prefix:` 与生成物 | 重打包 + pin | ⚠️ **须先裁定 COMPAT-09** —— "稳定/unstable 双前缀并存"是已登记的 by-design，改它要同批改 `MSC_SEMANTICS.md`；**且 M2 不是零跨仓**：`thread/{event_id}` 有 SDK 消费者（`room-thread-manager.ts:83`） |
| **M3 vendor 迁移** | 其余约 40 条 → `/_matrix/vendor/v1` | 33+ 处 `prefix:` → `VendorPrefix` + codegen + `contract-sync` | 重打包 + pin | ⚠️ **SDK 目标分支未定**：Tjg pin 钉的是 `release/contract-entrypoint`（`653e6952f`），SDK 工作区在 `develop`（`d367d1518`），两者已分叉（`merge-base --is-ancestor` 为否）；切分支是工作区级动作，须先确认无其他写者 |
| **M4 D1/D2 收尾** | 逐条复核 handler + 三层取证签字 | — | — | M3 之后 |

**跨仓纪律（本仓已固化）**：

1. 顺序**单向**：后端 → SDK → Tjg；回滚反向。
2. **同一批次三仓一起做**（M3 尤其）：后端已迁、SDK 未迁的窗口期客户端会 404 ——
   Batch 3 就是这么留下尾巴的（29 条端点至今对 SDK 是 404，只由 `sdk_uncovered_allowlist.txt`
   的 SDK-BE-3 兜着）。
3. 每批后端侧走完整契约链（顺序见 §3 M1 记录里那条 `--bootstrap` 链路）。
4. 铁律 9：动手前确认目标仓没有其他写者；只 `git add` 自己的显式 pathspec。

---

## 4. 门禁与判据

```bash
# M0 之后每批必跑
PATH="/Users/ljf/.workbuddy/binaries/python/versions/3.13.12/bin:/usr/bin:/bin:$PATH" \
  bash scripts/contract/check_route_contract.sh; echo $?
PATH="/Users/ljf/.workbuddy/binaries/python/versions/3.13.12/bin:/usr/bin:/bin:$PATH" \
  python3 scripts/contract/check_sdk_route_coverage.py; echo $?
PATH="/usr/bin:/bin:$PATH" cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings
PATH="/usr/bin:/bin:$PATH" cargo nextest run --test unit --features test-utils
```

- **变异自证**（铁律 8）：每条新守卫都要有一次"注入缺陷 ⇒ 转红 ⇒ 撤回"的实测记录。
- **口径**：判断"私有面是否收敛"看**混合模块清单的 private 计数**，不要看注册路由总数
  （vendor 迁移会因版本折叠反而让总数上升，见前缀治理 Phase 3 的记录）。

---

## 5. 复现命令（可直接跑）

```bash
# 1. 当前 55 条的构成
grep -c 'private' scripts/contract/mixed_module_client_routes.txt

# 2. 同 handler 多路径（冗余面候选）
python3 - <<'EOF'
import re, pathlib, collections
src = pathlib.Path("synapse-web/src/routes/room.rs").read_text()
g = collections.defaultdict(list)
for m in re.finditer(r'\.route\(', src):
    depth, j = 1, m.end()
    while j < len(src) and depth:
        depth += (src[j] == '(') - (src[j] == ')'); j += 1
    body = src[m.end():j-1]
    pm = re.match(r'\s*"([^"]+)"\s*,\s*(.*)', body, re.S)
    if not pm: continue
    for h in re.findall(r'(?:get|put|post|delete)\(\s*([A-Za-z_][\w:]*)', pm.group(2)):
        g[h.split("::")[-1]].append(pm.group(1))
for h, ps in sorted(g.items()):
    if len(ps) > 1: print(h, sorted(set(ps)))
EOF
# 期望：create_private_room（v1+v3）、get_mutual_rooms（unstable+vendor）
```

---

## 6. 风险与待裁定

| 风险 | 触发 | 处置 |
| --- | --- | --- |
| 迁移窗口内 404 | 后端迁了、SDK 没迁 | **同批做**；不做"只做后端"的迁移（Batch 3 已留过这个尾巴） |
| 误删仍有消费者的端点 | 只看 SDK、漏 Tjg 裸调 | 三层取证 + 严格谓词；D2 逐条签字 |
| MSC 归位与 COMPAT-09 冲突 | 改 sticky/pinned 前缀 | 先裁定口径，再同批改 `MSC_SEMANTICS.md` |
| 删掉"唯一能力" | D1 误判为等价写法 | 复核 handler 实现，不按路径名猜 |

**待用户裁定（三选一即可推进）**：

1. **激进**：M1 + M2 先做（零跨仓），M3/M4 等 SDK 批次窗口统一做；
2. **保守**：只做 M0（门禁补强，零路由变化）+ M1，其余全部延后；
3. **彻底**：M1–M4 一次排期，按"后端 + SDK + Tjg 同批"执行（耗时最长，但一次收敛到位）。

---

**生成时间**：2026-10-08
**下次核对**：M0 落地后更新 §1 事实基线（private 计数）与 §2 分类表的状态列
