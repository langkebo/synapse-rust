# PROJECT_REMAINING_ISSUES 复核：13 项报项真实性核验 + 优化方案

> 输入：`docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md`（当前口径 = §0.2 + §22.3，§23 追加 E2EE 删除面）
> 方法：**不采信文档任何既有标记**，逐条回到源码取证（`路径:行号` 或可复现命令 + 实测输出）；
> 外部事实（Matrix 规范、上游 Synapse）用**主源**（spec 仓库 YAML / Synapse 源码与 CHANGES）核对。
> 日期：2026-09-25。核验基线：`opt/consolidated` @ `40f618356`（2026-09-25 11:06）+ **工作树在途改动**。

---

## 0. 基线与方法（先读这一节）

### 0.1 核验环境

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
git log --oneline -1          # 40f618356 docs(audit): 登记 D-48 ...
git status --short | wc -l    # 70 个文件处于 staged（含 §23 E2EE 删除面，未提交）
git worktree list             # 另有 .worktrees/c19b（同 HEAD）、/Users/ljf/...-worktree/invite-policy
```

**铁律（本轮踩过的坑，写入以免后人误判）**：

1. **所有全仓 grep 必须排除 `.worktrees/` 与 `target/`** —— 本仓存在与 HEAD 同步的第二份工作树
   （`.worktrees/c19b`，同 `40f618356`），不过滤会把同一份代码计两次。
2. **必须区分 HEAD 与工作树**：§23 的 E2EE 删除面**已 staged 但未提交**，
   故 `HEAD` 仍含 `verification/`、`device_trust/`、`verification_routes.rs`，
   而**工作树没有**。凡引用这些文件的行号，一律标注取自哪一侧。
3. **行号会漂移**：`migrations/00000000_unified_schema_v12.sql` 在工作树被删 105 行
   （§23 删表），§22 里的 `:2839` 是 HEAD 行号，工作树为 `:2750`。
4. **口径纪律**：本文件所有数字都附生成命令；凡文档既有数字与本轮实测冲突，以实测为准并标注。

### 0.2 一句话结论

**13 项报项中，8 项确认存在、2 项部分成立、2 项应降级/证伪、1 项因对象被删除而作废。**
另在核验过程中**新发现 6 项**（其中 2 项比报项本身更严重：出站 PDU 缺 `depth`/`auth_events`、
`sign_and_broadcast_event` 存在两份策略相反的实现）。

| 报项 | 文档标记 | 本轮判定 | 关键判据 |
|---|---|---|---|
| P0 `/send_join` PDU | 已修 + 残余 3 条 | **残余 3 条仍存在**，另新增 1 条更直接缺陷 | `synapse-storage/src/event/create.rs:14-19`（无图元数据列）；`synapse-services/src/room/messaging/service.rs:157-167`（出站 PDU 无 `depth`/`auth_events`） |
| 高 E2EE SAS 3 处偏离 | 🔴 | **⚪ 对象消失（作废）** | 工作树 `synapse-e2ee/src/verification/` 已删（staged）；全仓 0 命中 |
| 高 客户端撤回不级联 | 🔴 | **🔴 存在** | `synapse-web/src/routes/handlers/room/events.rs:954-1001` |
| 高 Content Scanner 零调用点 | 🔴 | **🔴 存在** | `synapse-services/src/content_scanner/service.rs:203` 起的 `#[cfg(test)]` 是唯一调用方 |
| 中 `dag.rs` 注释幻觉 | 🔴 | **🔴 存在**（另有 2 处同类） | `synapse-storage/src/event/dag.rs:203-205`；`get_state_dag_edges` 生产调用点 0 |
| 中 `msc2965/auth_issuer` 在册 | 🔴 | **🔴 存在** | `synapse-web/src/routes/assembly.rs:197` |
| 中 Profile 两条 | 🔴 | **🔴 存在**，且缺口比文档描述更大（缺整个 v1.16 稳定面） | `synapse-web/src/routes/handlers/extended_profile.rs:124`；`capability_governance.rs:507` |
| 中 MSC4502 / MSC4262 未收敛 | 🔴 | **❌ 证伪（维持 §22.4）**，本轮抽查未推翻 | MSC4502：`room.rs:53 → members.rs:387-424 → membership/service.rs:610-706 → membership/mod.rs:716-796`；MSC4262：`federation/edu.rs:31,628-702` + `user/storage.rs:783` + `sliding_sync_service/extensions.rs:254` |
| 中 Admin 媒体族缺口 | 🔴 部分 | **🔴 存在**；文档"上游 18"**无法复现**，本轮给出可核验的 15 条 | `synapse-web/src/routes/admin/media.rs:16-22` |
| 中 `animated` + 配额错误码 | 🔴 部分 | **🔴 两项均存在** | `animated` 全仓 0 命中；`synapse-services/src/media/mod.rs:250-252` 返回 400 |
| 低 v12/v13 不可创建 | 🔴 | **⚪ 设计取舍（非缺陷）** | `synapse-common/src/room_versions.rs:108-115` |
| 低 `search_index` 遗留表 | 🔴 | **🔴 存在**（+ 派生物 `INDEXES.md` 漂移） | baseline `:2750-2761` + 4 索引 `:3735-3738`；生产消费者 0 |
| 低 ledger `query_params` 无消费方 | 🔴 | **🟡 部分证伪**（字段死，但序列化被 fixture 钉住并被 `gen_route_table.py` 消费） | `route_ledger.rs:83-84,106-110` 调用点 0；`ledger_export.rs:156` + 6 份 fixture |

**新发现（8 项，文档未列）**：

| 编号 | 级别 | 新发现 | 判据 |
|---|---|---|---|
| **N-1** | **P0** | 出站 `/send` PDU **缺 `depth` 与 `auth_events`** —— 本地事件广播给远端时只有 `prev_events`，不是 v3+ 合法 PDU | `synapse-services/src/room/messaging/service.rs:157-167`（json! 仅 9 键） |
| **N-2** | 高 | `sign_and_broadcast_event` **两份实现、策略相反**（违反反冗余铁律 2）：messaging 版 fail-closed、membership 版 fail-open 且**无 room-version 感知的 `redacts` 放置** | `messaging/service.rs:131-151` vs `membership/service.rs:486-520` |
| **N-3** | 中 | `create_event` 的文档注释**自述"delegates here"是假的**：它有自己的 INSERT，从不调用 `create_event_with_graph` | `synapse-storage/src/event/create.rs:8-20` vs 注释 `:51-58` |
| **N-4** | 中 | `block_on_scan_failure=false`（fail-open）**只对 webhook 生效**；ClamAV 路径 4 条失败路径全部直接 `Err` | `content_scanner/service.rs:62-100`（对照 `on_webhook_failure` `:159-172`） |
| **N-5** | 中 | Spec v1.16 已稳定 MSC4133：缺稳定 `/{keyName}` 路由、`m.tz`、`M_PROFILE_TOO_LARGE`/`M_KEY_TOO_LARGE`；且 `/versions` 只声明到 **v1.14** | spec `profile.yaml:19,22,27,104-106,305-320`；仓库 0 命中 |
| **N-6** | 低 | `pdu.rs` 模块注释里的 event_id 格式仍是**错的**（`$<ts>_<rand>:<server>`，实际分隔符是 `$`）——正是 §22.1 N-5 已修正而此处未同步 | `synapse-web/src/routes/federation/pdu.rs:37` vs `synapse-common/src/crypto.rs:153` |
| **N-7** | 低 | `migrations/INDEXES.md` 与 baseline 漂移：baseline 为 `search_index` 建 **4** 条索引，文档列 **0** 条 | `grep -c search_index migrations/INDEXES.md` → 0；baseline `:3735-3738` |
| **N-8** | 低 | CI 契约 `TABLE_CONTRACTS["search_index"]` 漏列 GIN 索引 `idx_search_index_content_trgm`，且校验是**单向**的（只查"契约写的索引是否存在"）⇒ 该方向永远绿（铁律 8 同类） | `scripts/check_schema_contract_coverage.py:177-194` vs `:302-305` |

---

## 1. 逐条核验

### 1.1 ⚪ E2EE SAS 三处偏离 → **对象消失，整条作废**

§0.2 第 3 条与 §22.3 的"高｜E2EE SAS —— 5 个子项中 1 修 4 存"**均已过期**，§23 已自行声明作废，
但 §0.2（文档自称"唯一权威"）**没有同步**，仍把 SAS 列为"当前须处理"。

**判据**：

```bash
# 工作树：目录不存在（staged 删除）
git status --short synapse-e2ee/src/verification synapse-e2ee/src/device_trust
#   D  synapse-e2ee/src/verification/{mod,models,service,storage}.rs
#   D  synapse-e2ee/src/device_trust/{mod,models,service,storage}.rs
git status --short synapse-web/src/routes/verification_routes.rs   # D
# 偏差代码全仓 0 命中（排除 .worktrees/target）
grep -rn "verification.commitment\|% 64\|generate_decimal_from_emoji\|compute_shared_secret" --include=*.rs .
#   → 无输出
```

- HEAD 侧（未提交前）确有：`git show HEAD:synapse-e2ee/src/verification/service.rs` →
  `:146 compute_commitment`、`:149 hasher.update(b"verification.commitment")`、
  `:151 STANDARD.encode`（带 padding）、`:314 let idx = (byte as usize) % 64;`；
  `git show HEAD:synapse-web/src/routes/verification_routes.rs:405 generate_decimal_from_emoji`。
- **措辞纠正**：§23 表格写"`e2ee/devices.rs`（6 个 handler）"被读成"文件删除"；
  实测该文件**仍在**（16459 字节，status `M`），只有 6 个 handler 被删。
- **无悬挂引用**：`src/e2ee/mod.rs`、`synapse-e2ee/src/lib.rs`、`routes/assembly.rs`、
  `wiring/e2ee.rs`、`routes/context.rs`、`e2ee_audit/audit_service.rs`、`synapse-e2ee/Cargo.toml`
  的对应 re-export/字段/依赖均已同步移除；`cargo check -p synapse-web --locked` **exit 0**（实测 1m28s）。
- **7 张表 + 6 条索引**已随 baseline 删除，`EXPECTED_BASELINE_FINGERPRINT` 已同步为
  `e151e5956fb64914`（`tests/unit/test_isolation_unification_tests.rs:120`）。
- **跨仓 follow-up（本仓无法闭合）**：`CryptoDeviceAdapter.ts` **不在本仓**（`find` 为空），
  客户端改走 `m.key.verification.*` to-device 属 matrix-sdk-fork 的任务。

**结论**：不需要"修 SAS"，需要的是**收尾**：把在途删除提交入库，并把 §0.2/§22.3 的 SAS 行标注作废。

---

### 1.2 🔴 客户端撤回不级联（MSC3912）→ **存在，且实现层次本身不符合规范**

**文档结论正确**，但 §22.5 给的修法（"按 `redacts` 关系调 `cascade_redact_event`"）**不够**：
本仓已有的 cascade 是**存储层内容擦除 + 递归 BFS**，与 MSC3912 定义的语义不是同一种东西。

**判据（本仓）**：

| 事实 | 判据 |
|---|---|
| 客户端 `PUT /rooms/{roomId}/redact/{eventId}/{txnId}` 只撤单条 | `handlers/room/events.rs:920-1006`：`:954` 只读 `reason` → `:962-964` content 仅 `{"reason"}` → `:969-985` 建 1 条 `m.room.redaction` → `:990 redact_event_content(&event_id)` |
| 级联入口**只有管理端** | `admin/room/mod.rs:244`（路由）+ `:818`（唯一调用点，`AdminUser` 提取器 `:789`）；全仓 `cascade` 在 synapse-web 路由侧仅此 3 处 |
| 现有 cascade 是**递归 BFS（depth 5）** | `synapse-storage/src/event/cascade.rs:61 find_cascade_targets`、`:66 for _ in 0..max_depth` |
| 现有 cascade **擦内容、不产生 redaction 事件**（不联邦、不做逐事件鉴权） | `:105 cascade_redact_event` → `redaction.rs:88 redact_event_content`（`UPDATE events SET content=...`） |
| 关联查询**无索引** | `grep -n "idx_events_content_rel\|content->'m.relates_to'" migrations/00000000_unified_schema_v12.sql` → **无输出**（全表扫描） |
| `/versions` 未声明 MSC3912 | `capability_governance.rs:113-130 BASE_UNSTABLE_FEATURES` 无 `org.matrix.msc3912`；全仓 `msc3912` 0 命中（`.rs`） |

**主源（规范原文）**：MSC3912《Redaction of related events》规定的是**客户端面新增字段**：

- `PUT /_matrix/client/v3/rooms/{roomId}/redact/{eventId}/{txnId}` 的 **body 新增可选 `with_rel_types`**
  （未稳定前缀 `org.matrix.msc3912.with_relations`），值为关系类型列表，含 catch-all `"*"`。
- **缺省或空列表 ⇒ 只撤目标事件**；**只撤"与目标相关"的事件，不追父事件**；
  规范例子明确：撤 `$a` 会撤 `$b`（`$b` 是 `$a` 的 edit），但**不会**撤 `$c`（`$c` 是 `$b` 的 edit）⇒ **单层**。
- 逐事件权限不足者**忽略**（不报错）。响应体不变，可先返回主 redaction event_id、后台再撤其余。
- `/versions` 的 `unstable_features` 应含 `org.matrix.msc3912`。

来源：[proposals/3912-relation-based-redaction.md](https://raw.githubusercontent.com/matrix-org/matrix-spec-proposals/1b3176cfe105a8d572ce27052f3b9e5e44cc0d9c/proposals/3912-relation-based-redaction.md)。

**上游 Synapse（release-v1.161 实测源码）**：这是判断"该怎么做"的最强参照，且**上游也只做了一半**：

```python
# synapse/rest/client/room.py:1374-1377（release-v1.161）
with_relations = None
if self._msc3912_enabled and "org.matrix.msc3912.with_relations" in content:
    with_relations = content["org.matrix.msc3912.with_relations"]
    del content["org.matrix.msc3912.with_relations"]      # ← 必须从落库 content 中摘掉
...
:1407-1415
if with_relations:
    self.hs.run_as_background_process("redact_related_events",
        self._relation_handler.redact_events_related_to, ...)
```

- 开关：`synapse/config/experimental.py:160 self.msc3912_enabled = experimental.get("msc3912_enabled", False)`（**默认关**）。
- 上游读的是 `org.matrix.msc3912.with_relations`（**未稳定名**），稳定名 `with_rel_types` + `*` 的更新**尚未实现**
  （上游 issue [#15687](https://github.com/element-hq/synapse/issues/15687) 仍 open）。
- `handlers/relations.py:191-263 redact_events_related_to`：**单层**（`get_all_relations_for_event[_with_types]`）、
  对每个相关事件**新建一条真正的 `m.room.redaction` 事件**（`ratelimit=False`）、
  权限失败只 `logger.warning` 后继续、`"*"` 走全类型查询。
- 上游**未实现**"请求结束后新到事件补撤"（MSC 有此要求，上游明确缺，见 #15687）。

**影响**：本仓既没有客户端面（功能缺失），管理端那套又与规范语义冲突（递归 + 无 redaction 事件 + 无逐事件鉴权），
`docs/audit/2026-09-23-msc3912-cascade-redaction.md` 还自称 "✅ Implementation Complete"（该文 §Testing 自述
"Unit Tests (Disabled)"），属文档幻觉。

---

### 1.3 🔴 Content Scanner 零生产调用点 → **存在（三路取证一致）**

**判据**（MSC 口径教训"模块存在 / 配置接上 ≠ 功能在工作"）：

| 检查 | 结果 |
|---|---|
| 公开方法 | `content_scanner/service.rs`：`new:14`、`is_enabled:20`、`scan:25`、`scan_text:183`、`scan_media:193` |
| `new()` 调用方 | 生产 **1 处**：`wiring/core.rs:179`（构造） |
| `scan/scan_text/scan_media/is_enabled` 调用方 | **全部**落在 `service.rs` 自己的 `#[cfg(test)] mod tests`（`:203` 起）内 |
| 字段是否被读 | `CoreServices.content_scanner`（`wiring/core.rs:66`）**全仓无读取点**；`grep '\.content_scanner'` 只命中 `:180` 的**配置**字段 |
| 配置默认 | `ContentScannerConfig`（`synapse-common/src/config/mod.rs:241-242`）默认 `enabled:false`、`ClamAv`（`content_scanner/mod.rs:109-110`）；`docker/config/` 无该键 |
| 持久化 | migrations 无 scan/scanner 表；`synapse-storage/src/` 无 scanner 模块 |
| 是否接入上传/发消息 | 上传链 `media/mod.rs:65 → upload.rs:120 → :127 → :77 → media/mod.rs:284-305` **无扫描钩子**；`room/` 内 `scan` 0 命中 |

**"装配了但永不扫描"确实比纯缺失更危险**：配置可开、管理员以为已防护。
补充 N-4：即便接通，`block_on_scan_failure` 的 fail-open 语义**只在 webhook 路径生效**
（`on_webhook_failure:159-172` 是唯一消费点，ClamAV 的 4 条失败路径 `:62-100` 直接 `Err`）。

---

### 1.4 🔴 `dag.rs` 注释幻觉 → **存在（并发现 2 处同类）**

**判据**：`synapse-storage/src/event/dag.rs:203-205` 注释：

> "Used by `/send_join` (federation) to provide the full state DAG to joining servers, and by
> `/get_missing_events` to walk the state DAG when backfilling missing state events (MSC4242)."

实测调用点（排除 `.worktrees/target`）：

| 函数 | 生产调用点 | 测试 |
|---|---|---|
| `get_state_dag_edges`（`dag.rs:208`） | **0** | `event/db_tests.rs:2144` |
| `find_missing_event_ids`（`dag.rs:14`） | `federation/transaction.rs:322`、`room/backfill.rs:102` | db_tests 多处 |
| `get_missing_events_between`（`dag.rs:42`） | `federation/events.rs:66` | db_tests 多处 |
| **`get_prev_state_events`（`dag.rs:179`）** | **0**（新发现） | — |
| **`find_events_referencing_missing_state`（`dag.rs:237`）** | **0**（新发现；其注释同样声称被 `/get_missing_events` 使用） | — |

旁证：`grep -rn "prev_state_events\|state_dag" synapse-web/src/routes/federation/` → **0**，
即 `/send_join` 压根没消费 state DAG。同类事实：`create_state_event_with_dag`（MSC4242 写路径）
**生产调用点也是 0**（只有 `pdu.rs:16`、`models.rs:106` 的注释提到它）。

---

### 1.5 🔴 `msc2965/auth_issuer` 仍在册 → **存在**

- 注册：`synapse-web/src/routes/assembly.rs:196-199`（路径 `:197`，handler `:198`）；
  派生表 `derived_route_table_always.inc.rs:114`。**无** `/v1/auth_issuer`。
- `auth_metadata` **必须保留**：unstable `assembly.rs:193` + **stable v1** `assembly.rs:202`，
  同一 handler `auth_discovery.rs:47`。
- 上游证据（主源）：Synapse `release-v1.161/CHANGES.md:48` ——
  "Drop `GET /_matrix/client/unstable/org.matrix.msc2965/auth_issuer` endpoint which never ended up being used. (#20163)"；
  上游 **只删 issuer**，`auth_metadata` 未删 ⇒ 文档 §22.3 的"口径收窄"警告正确，照做。
- 删除代价极低：唯一消费者是 handler `get_auth_issuer`（`auth_discovery.rs:62`）；
  `oidc_available`/`build_oidc_discovery` 仍被 `get_auth_metadata` 使用，不会连带死。
  需同步：`tests/integration/api_auth_routes_tests.rs:247-254` + 全部派生产物（见 §4.5）。

---

### 1.6 🔴🟠 Profile：文档两条都成立，**且真实缺口比文档大**（缺整个 v1.16 稳定面）

**子项 A（停用用户写自定义字段应成功）— 成立**：
`synapse-storage/src/user/storage.rs:698`：

```sql
SELECT 1 AS "exists!" FROM users WHERE user_id = $1 AND is_deactivated = FALSE LIMIT 1
```

→ `user_exists` = "存在**且未停用**"。调用链
`handlers/extended_profile.rs:30-38 ensure_extended_profile_user_exists` →
`account_identity_service.rs:70-73` → `user_service.rs:112-116` → 上述 SQL；`:124` 在**所有权检查之前**就 404。
上游主源确认：`release-v1.161/CHANGES.md:36`（#20172）：

> "this now **succeeds for existing (e.g. deactivated) users** and returns a 404 error if the user does not exist"

**注意**：`user_exists` 有 **14 处生产调用点**（`membership/actions.rs:63`、`moderation.rs:37/183/352`、
`federation/edu.rs:238`、`auth_compat.rs:118`、`admin/room/management.rs:380/407/440/496/525` …）
⇒ **绝不能直接摘掉 SQL 里的 `AND is_deactivated = FALSE`**（会把停用判定泄漏到鉴权/成员路径），
必须**新增**一个"包含停用"的存在性查询，只给 profile 写路径用。

**子项 B（稳定 `/{keyName}` 未注册）— 成立，且已升级为规范缺口**：
- 仓库只声明 capability `m.profile_fields`（`synapse-services/src/capability_governance.rs:504-507`，属 `/capabilities`），
  `/versions` 只声明未稳定 flag `uk.tcpip.msc4133`（`capability_governance.rs:116`）；
  **稳定路由只到** `profile/{user_id}`、`/displayname`、`/avatar_url`（`assembly.rs:376-378`），
  泛化 `{key_name}` 只在 unstable（`assembly.rs:228-231`；派生表 `:269/:277/:285`）。
- **主源**：matrix-spec `data/api/client-server/profile.yaml:19` 已有
  `"/profile/{userId}/{keyName}"`，`:22` 明写 `x-changedInMatrixVersion: "1.16"`，
  即 **MSC4133 已进规范 v1.16**（PUT/GET/DELETE 三法），`keyName` 允许 `avatar_url|displayname|m.tz|自定义命名空间`
  （`:52-53`），并定义 `M_PROFILE_TOO_LARGE`（`:104-106`）、`M_KEY_TOO_LARGE`（`:106`）、
  总 profile 上限 **64 KiB**（`:27`）、`GET /profile/{userId}` 响应新增 `m.tz` 与 additionalProperties（`:305-320`）。
  ⚠️ **口径纠正**：MSC4133 提案文本提到 `/profile/{userId}` 的 `field=` 查询参数，
  但**合并进 spec 的版本里没有它**（`profile.yaml` 该端点只有 `userId` 路径参数）⇒ 本仓不存在该缺口，
  不得据提案文本立项。
- **仓库 0 命中**：`M_PROFILE_TOO_LARGE`、`M_KEY_TOO_LARGE`、`"m.tz"`、`GET /profile` 的 `field=`。
- **`/versions` 只到 v1.14**（`capability_governance.rs:79-104`）⇒ 声明 `m.profile_fields`
  （v1.16 capability）**同时**不声明 v1.16，客户端会得到自相矛盾的信号。
- 另：稳定 `PUT /displayname`、`/avatar_url` 也走 `ensure_active_user_exists`（`account_compat.rs:229,225` 一线），
  与 GET（对停用用户 200）**前后不一致**。

**结论**：本项的正确修法不是"二选一"这么简单，而是三选一（见 §3.1 T3.1）。

---

### 1.7 🔴 Admin 媒体端点族缺口 → **存在；文档"上游 18"不可复现，本轮给出可核验的 15 条**

**本仓实测**（`admin/media.rs:16-22`，`grep -c '"admin::media"'` = 7）：

| # | 本仓路由 |
|---|---|
| 1 | `GET /_synapse/admin/v1/media`（全局列举，**上游无此条**） |
| 2 | `GET /_synapse/admin/v1/media/{media_id}` |
| 3 | `DELETE /_synapse/admin/v1/media/{media_id}` |
| 4 | `GET /_synapse/admin/v1/media/quota`（**上游无此条**） |
| 5 | `GET /_synapse/admin/v1/users/{user_id}/media` ✅ 与上游一致 |
| 6 | `DELETE /_synapse/admin/v1/users/{user_id}/media` ✅ 与上游一致 |
| 7 | `GET /_synapse/admin/v1/quarantine_media/{media_id}/changes`（**形状与上游不同**） |

**上游主源**（`element-hq/synapse@release-v1.161/docs/admin_api/media_admin_api.md`，逐行实测）+ 交叉引用的
`user_admin_api.md:756,883`，共 **15** 条：

```bash
curl -sS https://raw.githubusercontent.com/element-hq/synapse/release-v1.161/docs/admin_api/media_admin_api.md \
  | grep -nE "^(GET|POST|DELETE) "
# 13 条（见下），另 2 条在 user_admin_api.md（GET/DELETE users/<user_id>/media，本仓已有）
```

| 上游端点（行号 = media_admin_api.md） | 本仓 | 缺口类型 |
|---|---|---|
| `GET /room/{room_id}/media`（:18） | ✗ | **真缺口**（房间级列举） |
| `GET /media/{origin}/{media_id}`（:49） | 有 `media/{media_id}` | **形状偏差**（缺 `{origin}` 段） |
| `POST /media/quarantine/{server_name}/{media_id}`（:112） | ✗ | **真缺口**（鉴权白名单 `admin_auth.rs:386` 已预留前缀） |
| `POST /media/unquarantine/{server_name}/{media_id}`（:133） | ✗ | **真缺口** |
| `POST /room/{room_id}/media/quarantine`（:154） | ✗ | **真缺口**（房间级隔离） |
| `POST /user/{user_id}/media/quarantine`（:186） | ✗ | **真缺口**（用户级隔离） |
| `POST /media/protect/{media_id}`（:217） | ✗ | **真缺口**（防隔离保护） |
| `POST /media/unprotect/{media_id}`（:237） | ✗ | **真缺口** |
| `GET /media/quarantine_changes?from=`（:267） | 有 `quarantine_media/{media_id}/changes` | **形状偏差**（上游是全局 + `from` 游标） |
| `DELETE /media/{server_name}/{media_id}`（:300） | 有 `DELETE media/{media_id}` | **形状偏差**（缺 `{server_name}` 段） |
| `POST /media/delete?before_ts=`（:331） | ✗ | **真缺口**（按时间批量删） |
| `POST /media/{server_name}/delete?before_ts=`（:339） | ✗ | **真缺口** |
| `POST /purge_media_cache?before_ts=`（:385） | ✗ | **真缺口**（远端缓存清理） |
| `GET/DELETE /users/{user_id}/media`（user_admin_api.md:756,883） | ✅ | 一致 |

**已具备但无路由的能力**（说明缺口是"没接线"而非"没能力"）：
`synapse-services/src/media/mod.rs:119 quarantine_media`、`:155 unquarantine_media`（仅测试调用 `:1071`）；
`synapse-storage/src/media/quarantine_stream.rs:14,42,52,74,154,180`；
`container.rs:490-493` 已装配 `with_quarantine_stream`；但 `admin_media_service.rs:12-85` 只暴露
`get_media_quarantine_changes`，**没有** quarantine/unquarantine 动作方法。

**文档"上游 18"**：唯一来源是 `API_COVERAGE_REPORT.md`（HEAD:137 / 工作树:155）的一个裸数字，
该表自带 `[人工口径·未机器复核]` 标注，无明细 ⇒ 不可核验。本轮以主源替换为 **15**。

---

### 1.8 🔴 `animated` + 媒体配额错误码 → **两项均存在**

**`animated`（未支持）**：
- 全仓 `grep -rin animated --include=*.rs`（排除 `.worktrees/target`）= **0 命中**。
- 参数解析：`routes/media/download.rs:263-268 thumbnail_request_params` 只读 `width/height/method`；
  handler `:440/:453` 用 `Query<Value>`；服务签名（`media/mod.rs:511-518`、`media_service.rs:405-412`）无该参数。
- **主源**：matrix-spec `data/api/client-server/content-repo.yaml:436-453` 定义 `animated` 查询参数，语义明确：
  "`true` → SHOULD 返回动画缩略图；`false` → MUST NOT；`true` 但素材无法动画（JPEG/PDF）→ SHOULD 视作 `false`"，
  响应体 `:497-502` 还区分了 `thumbnail`/`animated thumbnail` 两种 key。

**配额错误码（归因待议，但字面成立）**：
- 强制存在：`media/mod.rs:246-256 ensure_upload_allowed` → `:247 check_upload_quota` →
  `media_quota_service.rs:19-68`；`:250-252` 超限返回 `ApiError::bad_request(...)`。
- 实测响应：`ApiError::bad_request`（`error.rs:180-182`）→ kind=BadRequest → **HTTP 400 + errcode `M_BAD_JSON`**
  （`error.rs:63,822-824,858-864`；测试固化于 `media/mod.rs:893,950`）。
- `M_USER_LIMIT_EXCEEDED`：注册于 `error/code.rs:83-84`，注释明写 "**user account limit (MSC4335)**"，
  `:176` 映射 429；**生产使用点 0**（全仓只有注册表与测试）。
- 可用的候选码都已存在：`M_TOO_LARGE` 413（`code.rs:71,169`）、`M_RESOURCE_LIMIT_EXCEEDED` 403（`:75,171`）、
  `M_LIMIT_EXCEEDED` 429（`:27,147`）⇒ **不需要新造 errcode**。
- 结论：文档 §22.3 "该期望本身需要决策"的判断**正确**，本轮把它落成明确建议（见 §3.3 T3.4）。

---

### 1.9 ⚪ v12/v13 不可创建 → **设计取舍，应从"问题清单"移出**

`room_versions.rs:110-112` 注释自述"尚未完整实现 v12/v13 认证规则，降级为 parse+join+federate-only
可用，避免创建无法产生合规 PDU 的房间（fail-safe）"；`:114-115 stable_parse_only("12"|"13")`，
`stable_parse_only` ⇒ `can_create:false, can_join:true`（`:58-63`）；
`client_room_versions_capability:154-167` 过滤 `can_create` ⇒ `m.room_versions` 里不出现 v12/v13，
`/versions` 也不含房间版本。测试已钉住（`:239-241,:255-257`）。

**这不是缺陷**，是与本仓 PDU 能力（见 §2.1）一致的自觉取舍。**建议**：移入"已知取舍"章节，
并补一条守卫测试（断言 `can_create == false` 的理由常量存在），防止有人"顺手打开"。

---

### 1.10 🔴 `search_index` 遗留表 → **存在（+ 新的派生物漂移）**

- 工作树 baseline `:2750-2761` 仍 `CREATE TABLE IF NOT EXISTS search_index`，4 条索引
  `:3735-3738`（含 GIN `gin_trgm_ops`）；HEAD 行号为 `:2839`/`:3840-3843`（§22 引用的是 HEAD）。
- `synapse-storage/src/search_index.rs` **不存在**；生产 SQL 对 `search_index` 的引用 **0**
  （只有 `tests/integration/schema_contract_p0_tests_migrated.rs:336,1177-1312`）。
- 事件搜索走 `events` 表（`synapse-storage/src/event/search.rs`）；`config.search.search_index_name`
  是 Elasticsearch 索引名，与本表无关。
- **新发现（N-7，低）**：`migrations/INDEXES.md` 列出的 `idx_search_index_*` 条目数为 **0**，
  而 baseline 实际创建 4 条 ⇒ **索引文档与 baseline 已经不一致**（`INDEXES.md` 是人工维护的派生物）。
- **新发现（N-8，低，属铁律 8 同类）**：CI 契约 `scripts/check_schema_contract_coverage.py:177-194`
  只列了 **3** 条索引（`room/user/type`），漏了 GIN 的 `idx_search_index_content_trgm`
  （baseline `:3735`）；而该脚本的校验是**单向**的（`:302-305` 只检查"契约里写的索引是否存在"），
  **不会**发现"baseline 有而契约没写"⇒ 这个方向的门禁永远绿。
- 另有 CI 契约仍钉死该表：`scripts/check_schema_contract_coverage.py:177-194 TABLE_CONTRACTS["search_index"]`
  （被 `ci.yml:120`、`db-migration-gate.yml:73` 调用）。
- 删除需同步 **baseline 指纹**（硬规则，见 MEMORY/`test_isolation_unification_tests.rs:120`）。

---

### 1.11 🟡 ledger `query_params` → **部分证伪**

- **字段与 builder 确实是死的**：`route_ledger.rs:83-84 pub query_params`、`:106-110 with_query_params`，
  全仓调用点 **0**（只有定义）。
- **但"无消费方"不成立**：`ledger_export.rs:156` 把它序列化进导出产物，
  且 **6 份 fixture 逐字节钉住** 该字段（`tests/unit/ledger_export_tests.rs:136-179` 的
  `assert_fixture_matches` + `tests/unit/fixtures/ledger_export*/**.json`，
  当前 1033–1135 条全为 `[]`）；`tests/unit/placeholder_scan_tests.rs:224-255` 还钉了字面量
  `"query_params": []`；下游 `scripts/api_test/gen_route_table.py:64` 读取它并写入
  `docs/openapi/route-table.json`。
- **真实缺陷**是"**没有任何路径能产出非空值**"（字段+builder 是死代码），而不是"无消费方"。
- 决策影响契约：删除该字段会改动导出 schema ⇒ 需同步 `SCHEMA_VERSION`（`ledger_export.rs:45` = `"4"`）
  与 SDK lane pin（`LEDGER_SCHEMA_VERSION`）。

---

## 2. P0 残余复核（§22.2 的三条 + 本轮新增两条）

### 2.1 本地事件不落图元数据 → **成立**（并升级为两个独立缺陷）

**残余 ①：`create_event` 不写 `depth`/`prev_events`/`auth_events`/`origin`** —— 成立。
`synapse-storage/src/event/create.rs:14-15`（INSERT 列清单）对比 `:83-84`
（`create_event_with_graph` **有**这三列）。表结构允许（baseline `events.depth/prev_events/auth_events`
均为 NULL 列，`:343-345`）。

**影响链（实测，非推测）**：
- 建房走 `create_event`：`synapse-services/src/room/lifecycle/create.rs:76,116,167,188,216,238,308,350,382`
  ⇒ **本地新建房间的全部状态事件图元数据为 NULL**。
- `/send_join` 的 state/auth_chain 由 `routes/federation/pdu.rs::state_pdu` 投影，
  遇到 NULL 返回 `PduCompleteness::MissingGraphMetadata`（`:119-129`），
  `build_pdus` 遂走 `SignatureAction::RefuseIncomplete`（`:158-160`），
  **故意不发签名**（`:223-231`）。
- 结论：**远端服务器加入本仓本地创建的房间时，拿到的是一组无签名的 state PDU**，
  无法验签 ⇒ 联邦加入路径实质不可用。这不是"字段不齐"的美化描述，是 P0。

**残余 ②：入站 `signatures` 部分路径未覆盖** —— 成立（口径与 §22.2 一致）。
`/send` transaction 走 `create_event_with_graph`（`federation/transaction.rs:450,501`），
其 INSERT **不含** `signatures`/`hashes`；只有入站成员路径回填
（`federation/membership/mod.rs:238 → messaging/events.rs:507 → event/signature.rs:10`）。
⇒ 非成员入站事件再被转发时只剩本机签名。

**残余 ③：`event_id` 非 reference hash** —— 成立。
`synapse-common/src/crypto.rs:149-153`：

```rust
pub fn generate_event_id(server_name: &str) -> String {
    let timestamp = current_timestamp_millis();
    let mut bytes = [0u8; 18];
    rand::rng().fill_bytes(&mut bytes);
    format!("${}${}:{}", timestamp, URL_SAFE_NO_PAD.encode(bytes), server_name)
}
```

即 `$<ts>$<b64>:<server>`（§22.1 N-5 的口径修正**正确**）。
调用点 >40 处（`lifecycle/create.rs`、`membership/actions.rs`、`handlers/room/{events,state}.rs` …）。
v4+ 房间要求 `event_id = "$" + unpadded_base64(reference_hash)`，故 v11 对等端无法把本仓 PDU
当规范事件接受 ⇒ **独立于字段完备性的语义缺口**。

### 2.2 **新发现 N-1（P0）：出站 `/send` PDU 缺 `depth` 与 `auth_events`**

本地发消息后广播给远端的 PDU 在 `synapse-services/src/room/messaging/service.rs:157-167` 拼装，
**只有 9 个键**：`event_id/room_id/sender/user_id/type/content/origin_server_ts/origin/prev_events`
（+ 可选 `state_key`、`redacts`）。**没有 `depth`、没有 `auth_events`**。

对照仓库自己的规范纪律（`pdu.rs:19-23`）：

> "Substituting would be actively harmful: a peer that trusts a fabricated `prev_events: []`
> files the event as a DAG root and corrupts its own room graph."

即：**入站投影路径严格拒绝伪造图元数据，出站广播路径却直接省略两个必填键**——
两者对同一问题的处理自相矛盾，且出站那条是每个本地消息都走的路。

### 2.3 **新发现 N-2（高）：`sign_and_broadcast_event` 两份实现、策略相反**

| | `messaging/service.rs:131` | `membership/service.rs:486` |
|---|---|---|
| `prev_events` 取不到时 | **fail-closed**：`return Ok(())`，**不广播**（`:142-150`） | **fail-open**：`Vec::new()` 继续广播（`:496-505`），注释自述 "PDU may be incomplete" |
| `redacts` 放置 | 走 `apply_redacts`（`:174`），room-version 感知（v11+ 不写顶层） | **无条件**写顶层 `redacts`（`:531-533`） |
| `depth`/`auth_events` | 无 | 无 |

调用方：messaging 版被 `messaging/events.rs:204`、`messaging/messages.rs:241` 使用；
membership 版被 `membership/actions.rs:140,215,436`、`membership/moderation.rs:126,248,321,419` 使用。

这同时违反**反冗余铁律 2**（同一职责两份实现）与 pdu.rs 自述的安全立场（fail-open + 空 `prev_events`
正是"污染对端房间图"的那条路）。修复方式应是**收敛为一份**，而不是分别打补丁。

### 2.4 **新发现 N-3（中）：注释幻觉两处（与 §1.4 同类）**

1. `synapse-storage/src/event/create.rs:51-58` 注释称：
   "Callers without graph data … can continue to use `create_event`, **which delegates here** with
   empty arrays and depth 0." —— 实测 `create_event`（`:8-49`）**有自己的 INSERT，从不调用**
   `create_event_with_graph`。两者行为差异实质：前者图列为 **NULL**，后者为 `[]`/`0`；
   `pdu.rs:119` 的判定恰好区分 NULL 与 `[]`（`event_id_array` 对 NULL 与非数组都返回 None，
   但空数组会返回 `Some(vec![])` ⇒ **`[]` 会被判为 Complete 并签名**）。注释若被采信会导致
   错误实现（把本地事件改成"委托 + 空数组"，等于伪造 DAG 根）。
2. `synapse-web/src/routes/federation/pdu.rs:37` 仍写 `$<ts>_<rand>:<server>`
   （实际 `$<ts>$<b64>:<server>`，`crypto.rs:153`）—— §22.1 N-5 已修正文档，**代码注释未同步**。

### 2.5 **新发现 N-5（中）：MSC4133 已进规范 v1.16，本仓只到 v1.14**

见 §1.6 子项 B。要点：`profile.yaml:22` 明写 "1.16"，本仓 `/versions` 止于 v1.14
（`capability_governance.rs:79-104`），却声明 v1.16 的 capability `m.profile_fields`。

---

## 3. 优化方案

> **For agentic workers:** 本节是可直接执行的实施计划。若由 agent 执行，建议用
> `superpowers:subagent-driven-development`（每任务一个 fresh subagent + 任务间复核）。
> 步骤用 `- [ ]` 勾选跟踪。**禁止**在执行本计划时改动其他会话在途的 staged 文件（见 §3.0）。

**Goal:** 消除 §1/§2 中确认存在的缺陷，优先恢复联邦事件的规范可接受性（P0），其次补齐
MSC3912 客户端级联与 Content Scanner 接线（高），最后清理中低项与文档幻觉。

**Architecture:** 三类改动：①**写入路径统一**——把本地事件的图元数据（`depth`/`prev_events`/
`auth_events`）在创建期计算并落库，让出站广播与 `/send_join` 投影共用同一份真相；
②**同职责收敛**——两份 `sign_and_broadcast_event` 合成一份，扫面上传/发消息经单一钩子接入；
③**规范面补齐**——按 spec v1.16 / MSC3912 原文补路由与字段，同时删除上游已删面。

**Tech Stack:** Rust（axum / sqlx / tokio）、PostgreSQL 18（JSONB + GIN）、nextest、insta。

### Global Constraints

- 项目状态：**未发布、无外部用户、无生产数据 ⇒ 无兼容义务**（AGENTS.md 铁律 1）。
  不得为"兼容旧行为"保留双实现/旧字段；直接替换。
- **同一工作树同一时刻只允许一个写者**（铁律 9）。当前工作树有另一会话的 **70 个 staged 文件**
  （含 §23 E2EE 删除面）。执行者必须先 `git status --short` 确认，且**只 `git add` 逐路径**，
  禁止 `git add -A` / `git add .`。
- 提交前必过：`cargo fmt --all` + `./scripts/check_fmt_ratchet.sh`（baseline 0，`current>0` 与
  `current<0` 都失败）、`SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils
  --all-features --locked -- -D warnings`、`cargo test --workspace --all-features --locked`
  （`--workspace` 不可省）。
- 迁移只有一个真相源 `migrations/`；改 baseline 必须同步
  `tests/unit/test_isolation_unification_tests.rs:120 EXPECTED_BASELINE_FINGERPRINT`。
- **门禁必须自证能变红**（铁律 8）：本计划每个新增守卫测试都要附**变异自证**步骤。
- 每个任务结束时：`git diff --cached --stat` 复核 → 提交 → `git status --short` 确认未带走他人改动。

---

### 3.0 批次 B0：前置收尾（不写业务代码，先让基线一致）

| 任务 | 动作 | 验收 |
|---|---|---|
| **T0.1** | 由**在途会话的 owner** 把 §23 E2EE 删除面提交入库（本计划执行者**不得**代提交） | `git status --short` 中 `D synapse-e2ee/src/{verification,device_trust}/*`、`D .../verification_routes.rs` 消失；`cargo check -p synapse-web --locked` exit 0 |
| **T0.2** | 修正 `docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md`：§0.2 的第 3 条（SAS）标注 **⚪ 对象消失（见 §23）**，§22.3 的 SAS 表头加"整表作废"；§22.1 N-5 的 `$<ts>$<b64>` 口径回填到 §21.1 | `grep -n "SAS" PROJECT_REMAINING_ISSUES_2026-09-14.md` 中不再有"当前须处理"表述；不改动该文件的其他在途 hunk（该文件已 staged，**必须由 owner 合并**） |
| **T0.3** | 修正 N-6：`synapse-web/src/routes/federation/pdu.rs:37` 注释 `$<ts>_<rand>:<server>` → `$<ts>$<base64>:<server>` | `cargo fmt --all`；`./scripts/check_fmt_ratchet.sh` → `OK (0)` |
| **T0.4** | 修正 N-3①：`synapse-storage/src/event/create.rs:51-58` 注释删除"delegates here with empty arrays and depth 0"，改为事实描述（"`create_event` writes NULL graph columns; `create_event_with_graph` writes `[]`/`0`"） | 注释与 `:14`/`:83` 的 INSERT 逐字一致；无测试影响 |

### 3.1 批次 B1（P0）：联邦写入/广播路径的图元数据

**背景**：`/send_join` 无签名（§2.1残余①）与出站 PDU 缺 `depth`/`auth_events`（N-1）**同根**：
本地事件的图元数据从未被计算与持久化。因此**一个任务解决两个缺陷**。

#### Task 1：在创建期计算并落库 `depth`/`prev_events`/`auth_events`

> **⚠️ 执行修订（2026-09-25，实施中实测）** —— 原设计（给 `CreateEventParams` 加 3 个字段）
> 经实测有**两个问题**，已按下方新设计执行：
>
> 1. **爆炸半径**：`CreateEventParams {` 字面量全仓 **160 处**（`grep -rn "CreateEventParams {"`），
>    加必填字段要改 160 处（含 46 处 storage db_tests、15 处 test_mocks）——纯机械 churn，
>    且每处都要回答"该不该有图数据"，反而更容易错。
> 2. **缺少前置件**：`auth_events` 的正确性依赖规范的 **Auth events selection** 算法，
>    而全仓**没有任何实现**（`grep -rn "auth_events" synapse-services/src/room | grep fn` 为空）。
>    不先补它就无法产出合法 `auth_events`。
>
> **修订后的设计（三步，可分别提交）**：
>
> - **1a（已完成）** 新增 `synapse-services/src/room/state/auth_events.rs`：纯函数
>   `auth_types_for_event` / `select_auth_events` + `AuthStateSnapshot`。
>   算法与上游 Synapse `synapse/event_auth.py::auth_types_for_event`（release-v1.161）逐条对齐，
>   含 v9 无 restricted join rule、v10/v11 有。9 个已知答案单测（无需 DB）+ 3 个变异自证。
> - **1b（已完成，提交见分支）** 写入侧单一拦截点：`NotifyingEventWriter` 的模块文档已自证
>   "**Every service (messaging, membership, lifecycle, moderation, federation backfill) persists
>   through `Arc<dyn EventWriter>`**"（`synapse-services/src/notifying_event_writer.rs:11-13`）。
>   因此不改 160 处调用点，而是在该 seam 上加一层 `GraphMetadataWriter` 装饰器
>   （`synapse-services/src/graph_metadata.rs`）：
>   `create_event` → resolver 算 `(prev_events, auth_events, depth)` → 转调
>   `inner.create_event_with_graph(...)`；`create_event_with_graph` **原样透传**
>   （入站/backfill/建房已有图数据，不得被重算覆盖）。
>   依赖用**窄接口** `GraphMetadataSource`（4 个方法：forward_extremities / event_depths /
>   state_events / room_version）+ `StorageGraphMetadataSource` 适配真实
>   `Arc<dyn EventReader>` + `Arc<dyn RoomStoreApi>`；`depth` 走 `get_events_map`，**无需新增查询**。
>   装配点唯一（`wiring/rooms.rs`，Graph 在外、Notifying 在内，两条路径都仍会唤醒 sync）。
>   **失败策略 fail-closed**：房版本读不到 / 前驱事件读不到 / 无前驱 ⇒ 拒绝写入并返回错误，
>   绝不伪造 `[]`/`0`（与 `pdu.rs` 的立场一致）。
>   **事务边界（重要）**：装饰器**只在 `tx.is_none()` 时解析** —— 事务内的读看不到未提交行，
>   强行解析会产出错误的 `auth_events`/`prev_events`。事务调用方（建房批量写初始状态）
>   走原行为，由 1e 显式提供图数据。
>   8 个单测（无需 DB）+ 3 个变异自证（深度算术 / 自环过滤 / 装饰器改走 plain write ⇒ 各自转红）。
> - **1e（已完成，提交见分支）** 建房路径显式提供图数据：新增
>   `synapse-services/src/room/lifecycle/creation_graph.rs` 的 `CreationGraph`
>   （线性序列追踪：`prev_events` = 上一个事件、`depth` 递增、`auth_events` 走 1a 的选择算法，
>   且在**记录自身之前**做选择 ⇒ 事件永不自我授权），并在 `create_events.rs` 加
>   `write_creation_event(...)` 单一写法；`create.rs` 的 9 处 + `set_room_metadata` 2 处 +
>   `process_invites` 1 处全部由 `create_event` 改为 `create_event_with_graph`。
>   6 个纯单测 + 3 个变异自证（深度不递增 / 先记账再选 auth ⇒ 自授权 / prev 恒空 ⇒ 各自转红）。
>   **这一步才让新房间的 state 具备可签名条件**（装饰器在事务内不解析）。
> - **1c（待做）** storage 单写入口收敛：`create_event` 与 `create_event_with_graph` 目前是
>   **两条独立 INSERT**；抽成一个私有 helper，`create_event` 传 `None`（写 SQL `NULL`），
>   `create_event_with_graph` 传值，**公开签名不变**（避免 1a 之外的第二波 churn）。
> - **1d（待做）** `sign_and_broadcast_event` 收敛为一份并补 `depth`/`auth_events`
>   （见 N-1/N-2），改为复用已落库图字段而不是重新查 extremities。

**原设计（保留作对照，勿照抄）**：

**Files:**
- Modify: `synapse-storage/src/event/models.rs`（`CreateEventParams` 增加三个可选图字段）
- Modify: `synapse-storage/src/event/create.rs:8-49`（`create_event` 写这四列；统一走一条 INSERT）
- Modify: `synapse-services/src/room/messaging/events.rs`（`RoomMessagingService::create_event` 统一计算）
- Modify: `synapse-services/src/room/messaging/service.rs:131-214`（广播复用已落库图字段，删除二次查询）
- Modify: `synapse-services/src/room/membership/service.rs:486-560`（删除本文件实现，改调 messaging 版）
- Test: `synapse-storage/src/event/db_tests.rs`、`tests/unit/federation_state_pdu_tests.rs`

**Interfaces:**
- Produces: `CreateEventParams { depth: Option<i64>, prev_events: Option<Vec<String>>, auth_events: Option<Vec<String>>, .. }`
- Produces: `EventStorage::create_event` 落库图列（NULL 仅当调用方显式不传）
- Produces（合并后唯一实现）：`RoomMessagingService::sign_and_broadcast_event(&self, event: &RoomEvent) -> ApiResult<()>`

- [ ] **Step 1: 写失败测试（RED 1 —— 落库）**

`synapse-storage/src/event/db_tests.rs` 新增：

```rust
#[tokio::test]
async fn test_create_event_persists_graph_metadata() {
    let pool = setup_test_pool().await;
    let storage = EventStorage::new(pool.clone());
    let params = CreateEventParams {
        event_id: "$graph1:example.com".into(),
        room_id: "!r:example.com".into(),
        user_id: "@a:example.com".into(),
        event_type: "m.room.message".into(),
        content: serde_json::json!({"body": "x"}),
        state_key: None,
        origin_server_ts: 1_700_000_000_000,
        redacts: None,
        depth: Some(7),
        prev_events: Some(vec!["$p1:example.com".into()]),
        auth_events: Some(vec!["$a1:example.com".into()]),
    };
    storage.create_event(params, None).await.expect("create_event");
    let row: (Option<i64>, Option<serde_json::Value>, Option<serde_json::Value>) = sqlx::query_as(
        "SELECT depth, prev_events, auth_events FROM events WHERE event_id = '$graph1:example.com'",
    ).fetch_one(&*pool).await.expect("row");
    assert_eq!(row.0, Some(7));
    assert_eq!(row.1, Some(serde_json::json!(["$p1:example.com"])));
    assert_eq!(row.2, Some(serde_json::json!(["$a1:example.com"])));
}
```

- [ ] **Step 2: 运行并确认失败**

Run: `SQLX_OFFLINE=false cargo nextest run -p synapse-storage --lib -E 'test(test_create_event_persists_graph_metadata)'`
Expected: 编译失败（`CreateEventParams` 无 `depth` 字段）。

- [ ] **Step 3: 实现（GREEN 1）**

`CreateEventParams` 加三个字段；`create_event` 的 INSERT 改为与 `create_event_with_graph`
**同一条语句**（删掉 `create_event_with_graph` 的重复 INSERT，只保留一个写入口）：

```sql
INSERT INTO events (event_id, room_id, sender, user_id, event_type, content, state_key,
                    origin_server_ts, is_redacted, redacts, depth, prev_events, auth_events, origin)
VALUES ($1,$2,$3,$4,$5,$6,$7,$8,false,$9,$10,$11,$12,$13)
```

并在 `create_event_with_graph` 内改为构造带图字段的 `CreateEventParams` 后调用 `create_event`
（**消除第二份 INSERT**，满足铁律 2）。

- [ ] **Step 4: 运行确认通过**

Run: `SQLX_OFFLINE=true cargo nextest run -p synapse-storage --lib -E 'test(graph_metadata)'`
Expected: PASS。

- [ ] **Step 5: 写失败测试（RED 2 —— 出站 PDU 字段完备）**

`tests/unit/federation_state_pdu_tests.rs` 同级新增 `tests/unit/federation_outbound_pdu_tests.rs`：

```rust
#[test]
fn outbound_pdu_includes_depth_and_auth_events() {
    let event = room_event_fixture_with_graph(/*depth=*/ 7, /*prev=*/ &["$p"], /*auth=*/ &["$a"]);
    let pdu = build_outbound_pdu("example.com", &event);
    assert_eq!(pdu["depth"], serde_json::json!(7));
    assert_eq!(pdu["prev_events"], serde_json::json!(["$p"]));
    assert_eq!(pdu["auth_events"], serde_json::json!(["$a"]));
    // 反向断言：图元数据缺失时**不得**伪造（与 pdu.rs 的立场一致）
    let incomplete = room_event_fixture_without_graph();
    assert!(build_outbound_pdu("example.com", &incomplete).get("depth").is_none());
}
```

（实现时把 `service.rs` 里的 PDU 拼装抽成 `pub(crate) fn build_outbound_pdu(server_name, &RoomEvent) -> Value`，
便于单测；这是本次重构的关键 seam。）

- [ ] **Step 6: 实现（GREEN 2）** —— 抽 `build_outbound_pdu`，字段取自 `RoomEvent` 的
  `depth`/`prev_events`/`auth_events`（Task 1 Step 3 已保证有值），**取不到时按 `pdu.rs` 的同一立场处理**：
  不伪造，记 `federation_pdu_incomplete_total` 计数并跳过广播（与 messaging 版现有 fail-closed 一致）。

- [ ] **Step 7: 合并两份 `sign_and_broadcast_event`**

删除 `synapse-services/src/room/membership/service.rs:486-560` 的整份实现，让
`membership/actions.rs`、`membership/moderation.rs` 的 8 个调用点改调 messaging 版
（若字段不可达，用 `RoomMessagingService` 的引用注入，不要复制代码）。
验收：`grep -rn "fn sign_and_broadcast_event" --include=*.rs .` → **恰好 1 处**。

- [ ] **Step 8: 变异自证（铁律 8）**

分别制造 3 个变异并确认对应测试转红、还原后 sha256 一致：
① 把 `create_event` 的 `depth` bind 改成常量 `0`；② 从 `build_outbound_pdu` 删掉 `auth_events`；
③ 把 `RefuseIncomplete` 分支改成 `SignLocally`。
每次：`cargo nextest run --test unit -E 'test(federation_)'` 读到 nextest 的 `Summary [` 行并断言有 failed。

- [ ] **Step 9: 全量回归与提交**

```bash
cargo fmt --all && ./scripts/check_fmt_ratchet.sh
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
cargo nextest run --workspace --lib --all-features --locked --test-threads 4
cargo nextest run --test unit --features test-utils --locked --test-threads 4
git add synapse-storage/src/event/create.rs synapse-storage/src/event/models.rs \
        synapse-services/src/room/messaging/events.rs synapse-services/src/room/messaging/service.rs \
        synapse-services/src/room/membership/service.rs synapse-services/src/room/membership/actions.rs \
        synapse-services/src/room/membership/moderation.rs tests/unit/federation_outbound_pdu_tests.rs
git diff --cached --stat && git commit -m "fix(federation): persist local event DAG metadata and emit complete outbound PDUs"
```

#### Task 2：入站非成员事件的 `signatures`/`hashes` 回填（§2.1 残余②）

- **Files:** `synapse-web/src/routes/federation/transaction.rs:440-510`、
  `synapse-services/src/room/messaging/events.rs:507`、`synapse-storage/src/event/create.rs`
- [ ] Step 1: RED —— 在 `tests/integration` 加用例：投递一个 `/send` 事务（带远端 `signatures`），
  断言 `events.signatures` 落库且 `/send_join` 再投影时 `signature_action == KeepStored`。
- [ ] Step 2: GREEN —— 把 PDU 的 `hashes`/`signatures` 作为参数传入 `create_event_with_graph`
  并在 INSERT 中落库（与图元数据同一批字段），不再依赖事后 `update_event_signatures_and_hashes`。
- [ ] Step 3: 断言不再需要 `membership/mod.rs:238` 的专用回填（若无其他调用方则删除该方法，铁律 2）。
- [ ] Step 4: 变异自证 + 门禁同 Task 1 Step 8/9。

#### Task 3（决策项，**不建议本轮动手**）：`event_id` 改 v4+ reference hash

**为什么单列**：这是**语义级**改动，影响事件 ID 生成、事件去重、`stream_ordering` 下游、
以及所有以 `event_id` 为外键/缓存的路径（>40 处生成点 + 全库检索）。改动正确性依赖
canonical JSON + redaction 规则 + room version 判定齐备。

**建议**：
- 先做**可行性验证**（可独立立项）：在 `synapse-common/src/crypto.rs` 旁新增
  `compute_reference_hash(room_version, pdu_without_event_id) -> String`，用规范测试向量做**已知答案测试**
  （v4/v11 各一组），**不接线**。
- 只有当 v11 对等端互操作测试（`docker/` dev stack + 对端 Synapse）可复现通过后，才切换生成路径。
- **在本轮明确标注为"已知取舍"**：不修，也不假装已修；把 §2.1 残余③ 从"缺陷"改判为"未实现的互操作前提"。

### 3.2 批次 B2（高）

#### Task 4：MSC3912 客户端级联（规范形状）

**决策（已定，理由见 §1.2）**：
- **实现**：解析 `with_rel_types`（稳定名，按 MSC 原文）**并兼容** `org.matrix.msc3912.with_relations`
  （上游用的未稳定名，两条都读，避免客户端二分）。
- **语义按规范单层**（不递归）；`"*"` = 全部关系类型；缺省/空列表 = 只撤单条。
- **每条相关事件生成真正的 `m.room.redaction` 事件**（复用现有创建路径），逐事件 `can_redact_event`，
  权限不足**跳过并计数**，不影响响应。
- 主 redaction 立即返回；级联**同请求内联完成**（上限 `limit=1000`、关系类型数 ≤64）。
  *口径说明*：MSC 允许后台执行，上游用 `run_as_background_process`；内联便于测试与一致性，
  且本仓 `tokio::spawn` 有"panic 被吞"的历史坑（AGENTS.md），内联更安全。
- **不做**（与上游一致，明确登记）：请求结束后新到事件的补撤（MSC 要求、上游 issue #15687 未做）。

**Files:**
- Modify: `synapse-storage/src/event/cascade.rs`（新增带 rel_type 过滤的单层查询 + 索引）
- Modify: `synapse-services/src/event_redaction_service.rs`（新增 `cascade_redact_related`）
- Modify: `synapse-web/src/routes/handlers/room/events.rs:954-1001`
- Modify: `synapse-services/src/capability_governance.rs:113-130`（`("org.matrix.msc3912", true)`）
- Modify: `migrations/00000000_unified_schema_v12.sql`（GIN 索引）+ 指纹常量 + 派生产物
- Test: `synapse-storage/src/event/db_tests.rs`、`tests/integration/`

- [ ] **Step 1: RED（storage）** —— 用例：三个事件 `$a`（原消息）、`$b`（edit，`rel_type=m.replace`）、
  `$c`（reaction，`rel_type=m.annotation`）；`find_related_events_with_types("$a", ["m.replace"])`
  只返回 `$b`；传 `["*"]` 返回 `$b` 与 `$c`。
- [ ] **Step 2: GREEN（storage）** —— 新增：

```sql
SELECT event_id FROM events
WHERE room_id = (SELECT room_id FROM events WHERE event_id = $1)
  AND (
        (content->'m.relates_to'->>'event_id' = $1
         AND content->'m.relates_to'->>'rel_type' = ANY($2::text[]))
     OR (content->'m.relates_to'->'m.in_reply_to'->>'event_id' = $1
         AND 'm.in_reply_to' = ANY($2::text[]))
      )
  AND COALESCE(is_redacted, false) = false
  AND event_id <> $1
ORDER BY origin_server_ts ASC, stream_ordering ASC
LIMIT $3
```

  `"*"` 由调用方转成"去掉 rel_type 谓词"的变体（两个查询而非 `ANY` 里塞通配），避免 SQL 里做字符串特判。
- [ ] **Step 3: 索引（必须，否则每次撤回全表扫 `events`）** —— 在 baseline 追加：

```sql
CREATE INDEX IF NOT EXISTS idx_events_relates_to_gin ON events USING gin ((content->'m.relates_to'));
CREATE INDEX IF NOT EXISTS idx_events_in_reply_to_gin ON events USING gin ((content->'m.in_reply_to'));
```

  同步：`EXPECTED_BASELINE_FINGERPRINT`（先复算再替换，禁止照抄）、`migrations/INDEXES.md`、
  `scripts/check_schema_contract_coverage.py` 的 `TABLE_CONTRACTS`（若含 events 索引清单）。
- [ ] **Step 4: RED（路由）** —— 集成测试（`tests/integration/api_redaction_cascade_tests.rs`）：
  ① `with_rel_types: ["m.replace"]` 撤 `$a` ⇒ `$a`、`$b` 被撤且 `$c` **未**被撤；
  ② 同一请求断言**不为 `$c` 生成 redaction 事件**（对照 MSC 的 `$c` 例子）；
  ③ 权限不足者对他人事件的关系被忽略，响应仍 200；
  ④ 落库的 redaction content **不含** `with_rel_types`/`org.matrix.msc3912.with_relations` 键；
  ⑤ `GET /_matrix/client/versions` 的 `unstable_features["org.matrix.msc3912"] == true`。
- [ ] **Step 5: GREEN（路由）** —— 在 `redact_event` 中：

```rust
let relation_types = extract_relation_types(&mut body)?;   // 读两个 key，校验为字符串数组，随后从 body 移除
// ... 现有主 redaction 逻辑不变 ...
if let Some(types) = relation_types.filter(|t| !t.is_empty()) {
    let cascaded = ctx.event_redaction_service
        .cascade_redact_related(&room_id, &event_id, &redactor_user_id, &types)
        .await;   // 内部逐事件 can_redact_event + 真 redaction 事件 + 内容擦除 + appservice 派发
    metrics::counter!("msc3912_cascaded_redactions_total").increment(cascaded as u64);
}
Ok(Json(json!({ "event_id": new_event_id })))
```

- [ ] **Step 6: 管理端对齐** —— `admin/room/mod.rs:788` 的 `cascade_redact` 保留（管理员语义），
  但其实现改为**复用** Step 5 的服务方法，`max_depth` 字段标注为**本仓扩展**（MSC3912 无此概念），
  并在 `docs/audit/2026-09-23-msc3912-cascade-redaction.md` 头部加"与规范差异"小节，
  修正其"Implementation Complete / Unit Tests (Disabled)"表述。
- [ ] **Step 7: 变异自证** —— ① 把单层改成递归 ⇒ 测试②转红；② 删掉 `can_redact_event` 调用 ⇒ 测试③转红；
  ③ 保留 `with_rel_types` 在 content 里 ⇒ 测试④转红。
- [ ] **Step 8: 派生产物** —— `./scripts/api_test/export_ledger.sh` 与
  `gen_route_table.py --check` / `gen_client_yaml.py --check`（本任务不改路由，仅版本列表变化，
  但仍需跑一遍确认无漂移）。

#### Task 5：Content Scanner 接线（上传 + 发消息）

**Files:**
- Modify: `synapse-services/src/wiring/core.rs:66,179`（把 scanner 注入媒体与消息服务，而非只构造）
- Modify: `synapse-services/src/media/mod.rs:284-305`（`upload_media` 前扫描）
- Modify: `synapse-services/src/room/messaging/events.rs`（`create_event` 文本扫描）
- Modify: `synapse-services/src/content_scanner/service.rs:62-100`（修 N-4：ClamAV 也走失败策略）
- Create: `migrations/20260925xxxxxx_media_scan_results.sql`（或用 baseline——见 Step 5 决策）
- Test: `synapse-services/src/media/tests*`、`tests/integration/`

- [ ] **Step 1: RED（策略一致性，N-4）** —— 用例：`scanner_type=ClamAv` + `block_on_scan_failure=false`
  + 不可达 socket ⇒ `scan_media` 返回 `Ok(safe: true)` 而非 `Err`。
  Expected: FAIL（当前 `:62-70` 直接 `Err`）。
- [ ] **Step 2: GREEN** —— 把 ClamAV 的 4 条失败路径统一路由到 `on_scan_failure`
  （把 `on_webhook_failure` 重命名为 `on_scan_failure`，铁律 2：一份失败策略）。
- [ ] **Step 3: RED（上传接线）** —— 用 `wiremock` 起一个 webhook 扫描器返回 `{"safe": false, "threat_type":"virus"}`，
  调 `PUT /_matrix/media/v3/upload`，断言 **403 `M_FORBIDDEN`** 且媒体**未落库**
  （`SELECT count(*) FROM media` 不变）。
- [ ] **Step 4: GREEN（上传接线）** —— 在 `upload_media_common`（`upload.rs:127` 之后、
  持久化之前）调用 `scan_media(content_id, bytes, ContentType::Media)`；
  `safe=false` ⇒ `ApiError::forbidden`；`Err` ⇒ 由 `block_on_scan_failure` 决定放行或拒绝（沿用配置，不新增开关）。
- [ ] **Step 5: 决策 —— 扫描结果是否持久化**（**需 owner 决策**）：
  - 方案 a（推荐）：新增 `media_scan_results(media_id, safe, threat_type, scanned_at, scanner)` + 唯一约束，
    便于事后审计与"隔离已存在素材"。
  - 方案 b：不持久化，只在指标 + 审计日志留痕（最小改动）。
  两者都必须产出指标：`content_scans_total{result}`、`content_scan_failures_total`。
- [ ] **Step 6: 发消息扫描** —— `RoomMessagingService::create_event` 对 `m.room.message` 等
  文本事件调用 `scan_text`（文本提取复用 `extensible_events::extract_text_from_event_content`，
  不新增第二份提取实现）；`safe=false` ⇒ 403 + 不入库。
  **注意**：这会改变所有消息发送路径的行为，必须先跑全量集成回归确认无既有用例依赖
  "扫描器未接线"（`grep -rn "scan" tests/integration | grep -i message` 预检）。
- [ ] **Step 7: 配置与文档** —— `docker/config/homeserver.yaml` 增加**显式** `content_scanner: { enabled: false, ... }`
  （AGENTS.md：Search 类可选组件必须显式禁用而非缺省）；`docs/` 记录启用前置条件（ClamAV socket / webhook 可达）。
- [ ] **Step 8: 变异自证** —— ① 把上传钩子的判定反过来（safe=false 放行）⇒ Step 3 转红；
  ② 删掉 ClamAV 的策略路由 ⇒ Step 1 转红。

### 3.3 批次 B3（中）

#### Task 6：Profile —— 选定并落地一条路线（**需 owner 决策**）

| 路线 | 内容 | 代价 | 适用 |
|---|---|---|---|
| **A（推荐）** | 补 v1.16 稳定面：注册 `GET/PUT/DELETE /_matrix/client/v3/profile/{user_id}/{key_name}`（内部复用现有 extended_profile 实现）、支持 `m.tz`、`M_PROFILE_TOO_LARGE`/`M_KEY_TOO_LARGE`、64 KiB 总大小校验 | 大：需声明 v1.15/v1.16 版本（连带其全部变更）、重生成 ledger/fixture/openapi | 目标是 v1.16 合规 |
| **B** | 只修**语义**：profile 写路径改用"存在性含停用"的查询；capability 改名 `uk.tcpip.msc4133.profile_fields`（按 MSC 未稳定章节），保留 unstable 路由 | 小 | 暂不追 v1.16 |
| **C** | 撤销 `m.profile_fields` 声明，其余不动 | 最小 | 承认未实现 |

无论哪条，**子项 A（停用用户）都必须修**：

- [ ] Step 1: RED —— 集成用例：停用（`is_deactivated=true`）但存在的用户，管理员/本人 token 写自定义字段 ⇒ **成功**；
  不存在的用户 ⇒ 404。（对照上游 #20172。）
- [ ] Step 2: GREEN —— 在 `synapse-storage/src/user/storage.rs` **新增**
  `user_exists_including_deactivated(&self, user_id)`（`SELECT 1 FROM users WHERE user_id=$1`，**不带** deactivation 过滤）
  + trait 方法 + fake 实现；`extended_profile.rs:31` 改用它；**不动** `user_exists` 的 14 个调用点。
- [ ] Step 3: 一致化 —— 稳定 `update_displayname`/`update_avatar`（`account_compat.rs:229,225`）
  同样改走"存在性含停用"（与 GET 对停用用户 200 的行为对齐）；补用例钉住前后一致。
- [ ] Step 4: 路线 A 追加 —— 稳定路由 + `m.tz` + 两个 errcode + 总大小 64 KiB 校验；
  `/versions` 增补到 v1.16（**先核对 v1.15/v1.16 全部变更**，`VERSION_GAP_ANALYSIS` 口径）；
  重生成全部派生产物。
- [ ] Step 5: 路线 B/C 追加 —— `capability_governance.rs:507` 的 key 按决策改/删；
  同步 `tests/unit/` 里 capability 快照与 `docs/synapse-rust-vs-synapse-comparison.md:557,845`。

#### Task 7：Admin 媒体族补齐（按 §1.7 的 15 条清单）

- [ ] Step 1: 把上游 15 条**落成仓内可核验文件**（`docs/synapse-rust/ADMIN_MEDIA_ENDPOINT_PARITY.md`，
  含 `media_admin_api.md:行号`），删掉 `API_COVERAGE_REPORT.md` 里裸的"18"或改为引用该文件。
- [ ] Step 2: 补齐**真缺口**（9 条，按价值排序）：
  1. `POST /media/quarantine/{server_name}/{media_id}` + `unquarantine`（服务/存储已具备，只需 service 方法 + 路由 + 返回体对齐上游）
  2. `POST /room/{room_id}/media/quarantine`（房间级级联隔离）
  3. `GET /room/{room_id}/media`（房间级列举）
  4. `POST /user/{user_id}/media/quarantine`
  5. `POST /media/protect|unprotect/{media_id}`
  6. `POST /media/delete?before_ts=`、`POST /media/{server_name}/delete?before_ts=`
  7. `POST /purge_media_cache?before_ts=`
- [ ] Step 3: 修正**形状偏差** 3 条：`GET/DELETE media/{origin|server_name}/{media_id}`、
  `GET media/quarantine_changes?from=`（保留旧形状会违反铁律 1：无兼容义务，直接替换 + 重生成派生物）。
- [ ] Step 4: 鉴权：`admin_auth.rs:374 media_admin` RBAC 前缀已覆盖；`:386` 的 MFA 白名单从"预留"变"生效"，
  补守卫测试断言"白名单里的每条路径都有对应注册路由"（**反向验证**：临时删一条路由，测试必须转红）。
- [ ] Step 5: 每条新路由都要进 `route_ledger` + `*_route_manifest()` + 重生成 `derived_route_table_*.inc.rs`、
  `ROUTE_CONTRACT.md`、`route-table.json`、`client.yaml`、ledger fixtures、snapshot。

#### Task 8：缩略图 `animated`（按 spec 语义）

- [ ] Step 1: RED —— 集成用例：`GET .../thumbnail/...?animated=false` 对 GIF 素材必须**不返回**动画
  （按 spec `content-repo.yaml:436-453`：`false` ⇒ MUST NOT；`true` + 非动画素材 ⇒ 视作 `false`）。
- [ ] Step 2: GREEN —— `download.rs:263-268` 解析 `animated`（布尔，缺省 false）→
  透传到 `media/mod.rs:511-518` 与 `media_service.rs:405-412`；
  实现策略：`animated=false` 时若源为动画格式则取首帧（或走非动画转码路径）；
  `animated=true` 且无法动画 ⇒ 按 false 处理。**若缩略图实现目前不区分帧**，
  则本任务退化为"接受参数并诚实实现 false 分支"，`true` 分支按上游行为（返回静态最优）并由测试钉住。
- [ ] Step 3: 变异自证：把 `animated=false` 分支改成直通 ⇒ Step 1 转红。

#### Task 9：媒体配额错误码（**需 owner 决策**）

- [ ] Step 1: 决策 —— 单文件超限 ⇒ `M_TOO_LARGE`（413，`code.rs:71`，语义最贴）；
  总存储配额超限 ⇒ `M_RESOURCE_LIMIT_EXCEEDED`（403，`:75`）或 `M_LIMIT_EXCEEDED`（429，`:27`，可带 `retry_after_ms`）。
  **不采用** `M_USER_LIMIT_EXCEEDED`（`code.rs:83` 自述为 MSC4335 账户数上限，归因不符）。
- [ ] Step 2: RED —— 现有 `media/mod.rs:893,950` 断言 400，改为断言目标码 + 状态码。
- [ ] Step 3: GREEN —— `ensure_upload_allowed` 按超限类型返回对应 `ApiError`；`QuotaCheckResult` 增枚举
  `rejection: QuotaRejection::{FileTooLarge, StorageExceeded}`（不要把 string reason 当类型）。
- [ ] Step 4: 若 `M_USER_LIMIT_EXCEEDED` 最终**全仓仍无使用点**，按铁律 1 评估删除该变体
  （含 `code.rs:83,131,176,225` + `error.rs` 测试清单），避免"注册了但永不产生"的死码。

#### Task 10：`auth_issuer` 摘除 + `dag.rs` 幻觉清理

- [ ] **auth_issuer**：删 `assembly.rs:196-199` 的路由、`auth_discovery.rs:62` 的 handler、
  `tests/integration/api_auth_routes_tests.rs:247-254`、`tests/unit/assembly_route_tests.rs:403-405` 的断言；
  **保留** `auth_metadata`（unstable + v1 两条）；重生成派生表/ledger/快照/契约；净值 −1 路由。
- [ ] **dag.rs**：把 `:203-205` 注释改为"供 MSC4242 state-DAG 查询使用；当前**无生产调用点**"，
  并给 `get_state_dag_edges`/`get_prev_state_events`/`find_events_referencing_missing_state`
  和 `create_state_event_with_dag` 加 `#[allow(dead_code)]` + 同风格说明，
  **或**（更符合铁律 1）删除这三个查询与 MSC4242 写路径，若确定不打算做 MSC4242。
  补静态守卫：`tests/unit/doc_comment_claim_tests.rs` 断言"函数注释里出现的 `/` 路由路径必须能在
  `derived_route_table_*.inc.rs` 找到"（可反向验证：给注释里塞一个假路径，测试转红）。

### 3.4 批次 B4（低）

| 任务 | 动作 | 注意 |
|---|---|---|
| **T11 `search_index`** | 从 baseline 删表 + 4 索引；同步 `EXPECTED_BASELINE_FINGERPRINT`、`scripts/check_schema_contract_coverage.py:177-194`、`logical_checksum_tables.txt` + 生成器、`coverage_baseline.json`（若有对应文件）、`schema_contract_p0_tests_migrated.rs:336,1177-1312`、`synapse-storage/src/lib.rs:253` 的过期注释、**并修 `INDEXES.md` 漂移**（当前 0 条 vs 4 条）；顺带把 N-8 的**单向校验**补成双向（契约缺条目 ⇒ 转红） | 先反向验证：删一行索引 → `check_baseline_consolidation.py` 必须 exit 1；再给契约补上漏掉的 `idx_search_index_content_trgm` → 改双向后必须转红 |
| **T12 `query_params`** | 二选一：**(a)** 删除 `route_ledger.rs:83-84,106-110` + `ledger_export.rs:156` + 6 份 fixture 的字段 + `gen_route_table.py:64`，并把 `SCHEMA_VERSION` 4→5 与 SDK pin 同步（契约破坏，需跨仓通知）；**(b)** 保留并**真正使用**：给需要 query 参数的路由（如 `messages?dir/limit/from`、`thumbnail?width/height/method/animated`）填值，并加断言"声明了 query 参数的路由，其 handler 必须解析同名参数" | 若选 (b)，本任务与 Task 8 合并做，天然产生消费者 |
| **T13 v12/v13** | 从问题清单移入"已知取舍"；补守卫测试断言 `can_create == false` 且注释理由存在 | 与 §2.1 残余③ 联动：v12/v13 的 fail-safe 理由正是"产不出合规 PDU" |

---

## 4. 执行顺序、门禁与派生产物

### 4.1 建议顺序（依赖关系）

```
B0 (T0.1–T0.4 收尾/文档)
  └─ B1 Task1 (图元数据统一) ──> Task2 (入站签名)
        └─ B2 Task4 (MSC3912 客户端级联)   [依赖 Task1 的 redaction 创建路径一致性]
        └─ B2 Task5 (Scanner 接线)         [独立，可并行]
  └─ B3 Task6 (Profile) / Task7 (Admin 媒体) / Task8/9 (媒体) / Task10 (auth_issuer+dag)
  └─ B4 T11/T12/T13
Task3 (reference hash) —— 仅做可行性验证，不接线
```

### 4.2 每任务的完成定义（DoD）

1. `cargo fmt --all` + `./scripts/check_fmt_ratchet.sh` → `OK (0)`；
2. `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings` → exit 0；
3. `cargo nextest run --workspace --lib --all-features --locked --test-threads 4` 与
   `--test unit --features test-utils` 全绿；改了路由/迁移的另跑
   `cargo nextest run --profile ci --all-features --test integration --test-threads 1`；
4. 新增守卫**已变异自证**（记录了"变异 → 转红 → 还原 → sha256 一致"）；
5. 派生产物已重生成且 `--check` 通过（见 §4.3）；
6. `git add` 逐路径 + `git diff --cached --stat` 复核 + `git status --short` 确认未带走他人改动。

### 4.3 派生产物清单（改路由/版本/迁移时）

`synapse-web/src/routes/derived_route_table_{always,oidc,worker}.inc.rs`、
`docs/synapse-rust/ROUTE_CONTRACT.md`、`docs/openapi/{route-table.json,client.yaml}`、
`scripts/api_test/{ledger.json,handler_schemas.json,response_schemas.json}`、
`tests/unit/fixtures/ledger_export{,_sdk}/*.json`、`route_ledger_*.snapshot`、
`scripts/ci/{coverage_baseline.json,sqlx_*_baseline,ts_order_single_key_baseline,geiger_baseline.json}`、
`migrations/INDEXES.md`、`tests/unit/test_isolation_unification_tests.rs` 指纹常量、**SDK lane 的 `LEDGER_SCHEMA_VERSION` pin**。

### 4.4 本计划**不建议**现在做的项（明确登记，避免反复立项）

| 项 | 理由 |
|---|---|
| `event_id` 改 reference hash（Task 3） | 语义级改造 + 需真实对等端互操作验证；先做已知答案测试，不接线 |
| MSC3912"请求后新到事件补撤" | 上游 Synapse **也未实现**（issue #15687 open）；MSC 自身仍在演进 |
| 把 v12/v13 打开为可创建 | 与本仓 PDU 能力冲突，是自觉 fail-safe（§1.9） |
| Content Scanner 的 `scan_media` 全格式解码 | 扫描深度涉及解码/解压炸弹风险，属独立安全立项 |
| 逐条补齐上游 admin 面到 100% | 先补齐 §1.7 的 9 条真缺口即可；剩余以"上游文档面"为清单持续跟进 |

---

## 附录 A：本轮核验的可复现命令

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# A. E2EE SAS 对象消失
git status --short synapse-e2ee/src/verification synapse-e2ee/src/device_trust synapse-web/src/routes/verification_routes.rs
grep -rn "verification.commitment\|% 64\|generate_decimal_from_emoji\|compute_shared_secret" --include=*.rs .   # 0 命中

# B. 客户端撤回不级联 / 级联能力存在但仅管理端
grep -rn "cascade" --include=*.rs synapse-web/src/routes/ | grep -v derived_route_table
sed -n '954,1001p' synapse-web/src/routes/handlers/room/events.rs

# C. Content Scanner 零生产调用点
grep -rn "scan_media\|scan_text\|\.scan(" --include=*.rs . | grep -v "content_scanner/service.rs"
grep -rn "\.content_scanner" --include=*.rs .

# D. dag.rs 注释幻觉
sed -n '203,208p' synapse-storage/src/event/dag.rs
grep -rn "get_state_dag_edges\|get_prev_state_events\|find_events_referencing_missing_state" --include=*.rs .

# E. auth_issuer / auth_metadata
grep -rn "auth_issuer\|auth_metadata" --include=*.rs synapse-web/src/routes/assembly.rs

# F. Profile / MSC4133
grep -n "is_deactivated" synapse-storage/src/user/storage.rs | sed -n '1,5p'
grep -rn "M_PROFILE_TOO_LARGE\|M_KEY_TOO_LARGE\|\"m\.tz\"" --include=*.rs .    # 0 命中

# G. Admin 媒体
grep -n "route(" synapse-web/src/routes/admin/media.rs

# H. 出站 PDU / 两份 sign_and_broadcast_event
sed -n '156,175p' synapse-services/src/room/messaging/service.rs
grep -rn "fn sign_and_broadcast_event" --include=*.rs .

# I. 本地事件图元数据
sed -n '13,20p;82,89p' synapse-storage/src/event/create.rs

# J. event_id 格式
sed -n '148,154p' synapse-common/src/crypto.rs
```

## 附录 B：主源（本轮外部核对）

| 事实 | 主源 |
|---|---|
| MSC3912 客户端 `with_rel_types` / 单层语义 / `*` / 未稳定名 | [matrix-spec-proposals/3912](https://raw.githubusercontent.com/matrix-org/matrix-spec-proposals/1b3176cfe105a8d572ce27052f3b9e5e44cc0d9c/proposals/3912-relation-based-redaction.md) |
| Synapse 的 MSC3912 实现与开关默认值、后台执行、权限忽略、未做"后到补撤" | `element-hq/synapse@release-v1.161`：`synapse/rest/client/room.py:1374-1415`、`synapse/handlers/relations.py:191-263`、`synapse/config/experimental.py:160`；[issue #15687](https://github.com/element-hq/synapse/issues/15687) |
| MSC4133 已进 spec v1.16（`/profile/{userId}/{keyName}`、`m.tz`、两个 errcode、`field=`） | matrix-spec `data/api/client-server/profile.yaml:19,22,104-106,312-316` |
| 缩略图 `animated` 语义 | matrix-spec `data/api/client-server/content-repo.yaml:436-453,497-502` |
| 上游已删 `auth_issuer`、profile 停用用户修复 | `element-hq/synapse@release-v1.161/CHANGES.md:48`（#20163）、`:36`（#20172） |
| 上游 admin 媒体端点面（15 条） | `element-hq/synapse@release-v1.161/docs/admin_api/media_admin_api.md`、`user_admin_api.md:756,883` |
| 上游 MSC3912 未稳定名（稳定名待更新） | [element-hq/synapse#15687](https://github.com/element-hq/synapse/issues/15687)（open） |
