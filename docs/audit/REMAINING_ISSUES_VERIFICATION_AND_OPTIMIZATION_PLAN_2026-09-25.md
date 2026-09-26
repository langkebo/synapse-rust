# 剩余问题与优化方案（阶段版）

> **口径**：本文件**只保留当前仍然存在**的问题与对应优化方案。已收口的问题不再列出。
> 被裁掉的历史内容（13 项报项的逐条取证、已修项的完整证据链）在 git 历史里：
> ```bash
> git log --follow -- docs/audit/REMAINING_ISSUES_VERIFICATION_AND_OPTIMIZATION_PLAN_2026-09-25.md
> ```
> **基线**：`opt/consolidated` @ `ca9464da4`（2026-09-26）。
> **判据纪律**：每条都带 `路径:行号` 或可复现命令；凡数字都注明生成方式。
> 外部基准：Matrix spec v1.18、`element-hq/synapse` release-v1.161（互操作 oracle 用的是真实安装的
> `matrix-synapse==1.161.0`）。

---

## 1. 阶段总结

### 1.1 已收口（索引，不再展开）

| 主题 | 编号 | 提交 / 位置 | 结果 |
|---|---|---|---|
| 出站 PDU 单一组装 + v12 图字段 | M-1 / B1 / O-3 | `7c88cae1a`、`ba82b6080` | 一份 `build_pdu` + 一份 `sign_and_broadcast_event` |
| MSC4311 联邦邀请图字段 | O-2 | `f5a0dac15`、`e4e0f094f` | `depth`/`prev_events`/`auth_events` 齐备 |
| MSC3912 单层级联接线 | U-1 | 已落地 | 单层关联查询 + unstable 标志 + 后台 best-effort |
| 媒体 hash 级自动隔离 | U-3 | `4f7299e82`、`cbe6517c5` | `media_metadata.content_hash` + 索引 |
| Profile 错误码 | U-4 | `5e739ef8a` | — |
| 媒体配额错误码 | U-7 | `d70b4fc20` | 413 `M_TOO_LARGE` / 403 |
| ledger `query_params` | U-10 | `fd178425b` | 真填值 + 反向守卫 |
| `AuthEventBuilder` `@@user:server` 缺陷（逻辑） | U-16 | `b83cbcaac` | 逻辑已修；**删除/收敛仍待办（§2.2）** |
| 本机 `public` 未按 baseline 播种 | U-17 | 环境操作 | 已播种（222 表）；非代码缺陷 |
| 扫描失败状态码 502 | U-18 | `94fc91442` | 新增 `ApiErrorKind::BadGateway` |
| **出厂默认（扫描关闭）发消息恒 501** | U-20 | `a6a77ac03` | 11 条既有红转绿 + 定点回归用例 |
| 签名材料按房间版本 redact | U-21 | `fa337ade8` | 上游 `test_sign_minimal`/`test_sign_message` 逐字节通过 |
| U-13 第 1 步：reference hash 纯函数 | U-13 | `64ffc13a6` | 上游 v10/v3 已知答案向量通过 |
| U-13 第 2 步 Slice A/B/D/E：接线 | U-13 | `7890ba980`、`06c45b7b1`、`c783629b0`、`75ff09099` | 单一组装、出站无 `event_id`、写入口 finalize、客户端面消费、schema 约束放行 |
| `events.event_id` CHECK 放行 reference hash | U-13 | `75ff09099` | 旧约束只接受 `$opaque:domain` ⇒ v3+ 写路径整体不可用（P0，已修） |
| SQLx 静态化 C28–C32 | SQLx 战役 | 并发会话（`3ec02b1e9` 等） | 动态 432 / 测试 711 / 静态 1037 |
| 既有红门禁（与本批无关） | — | `ad90bf844`、`855708958` | clippy `let_underscore_future`、`bool_assert_comparison` |

### 1.2 阶段末门禁快照（本机实测）

| 门禁 | 命令 | 结果 |
|---|---|---|
| 格式棘轮 | `./scripts/check_fmt_ratchet.sh` | `current=0 baseline=0` ✅ |
| SQLx 比率棘轮 | `bash scripts/ci/check_sqlx_dynamic_ratio.sh` | 生产 `432<=432`、测试 `711<=711`、静态 `1037>=1037` ✅ |
| clippy（CI 口径，两档） | `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils [--all-features] --locked -- -D warnings` | `--all-features` 档 exit 0 ✅（在 `ca9464da4` 上实测）；**另一档（features-args 为空）需在干净树上补跑** —— 复跑时被并发会话 `sync_service/*` 的在途改动挡住（见 §1.3，不得把在途状态当提交态结论） |
| lib 全量 | `cargo nextest run --workspace --lib --all-features --locked --test-threads 4` | **6368 / 6368 passed, 0 skipped** ✅（在 `ca9464da4` 上实测） |
| unit 全量 | `cargo nextest run --test unit --features test-utils --locked --test-threads 4` | **未定格** ⚠️ —— 复跑时并发会话正在改 `sync_service/*`（工作树非干净，编译不过的是它的在途状态，不是提交态）。复跑前必须确认 `git status --short` 为空 |
| 集成（受影响面 + 既有红复核） | `--profile ci --all-features --test integration --test-threads 1` | 定点 15 例：8 过 / 7 红（红项全部是 §3 的既有家族） |
| 集成全量 | 同上（无过滤器） | **未跑**（时间预算 + 每轮新建数百 schema 使 DDL 超线性变慢）。不得当作通过 |
| 联邦互操作 | §2.1 U-13-S3 | **未跑**（本沙箱做不到 live，见 §5.2） |

### 1.3 本机验证前提（不遵守会得到假红）

```bash
# 1) public 必须按 baseline 播种（否则 schema_validator::db_tests 假红）
TARGET_SCHEMA=public RESET_PUBLIC=0 bash scripts/init_test_public_schema.sh
#    ⚠️ 不要用 RESET_PUBLIC=1：脚本头部记载它会 DROP SCHEMA public CASCADE，级联破坏模板索引
# 2) 长跑集成前清残留 schema（否则 DDL 超线性变慢）
bash scripts/cleanup_test_schemas.sh --apply
# 3) 改了 migrations/ 后重建两个 schema（public + test_template_ci）
bash scripts/ci/prepare_test_db.sh
# 4) CI 的 clippy 有**两档**矩阵（features-args 为空 / --all-features），两档都要跑：
#    只跑 --all-features 会漏掉「某个 cfg 门控模块在另一档不编译」这类缺陷
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
```

**注意（本机实测）**：本仓在同一 `target/` 上频繁切换 feature 组合与分支，
`synapse-common` 这类"刚新增模块"的 crate 可能留下**陈旧 artifact**，表现为下游报
`unresolved import synapse_common::pdu`（源码里该模块是无条件声明的）。
判据：`cargo check -p synapse-common --features test-utils` 单独看是绿的、且 `pdu.rs` 存在。
处置：`cargo clean -p synapse-common` 后重跑；不要据此改代码。

---

## 2. 仍然存在的问题

优先级口径：**P0** = 直接阻塞联邦互操作或数据正确性；**P1** = 明确的规范/实现缺口，有清晰修法；
**P2** = 反冗余与文档收口。

### 2.1 P0 —— U-13 第 2 步剩余接线：联邦互操作的最后一段

背景：Slice A/B/D/E 已把**本地产生**的事件身份切成 reference hash（v3+），
但**入站半边与邀请面还没跟上**。下面每条都是"同一份事件的两种算法不一致"，
后果一致：对等端（或我们自己）无法复现对方的 ID / 签名。

| 编号 | 问题 | 判据（实测） | 优化方案 |
|---|---|---|---|
| **U-13-R1** | **入站验签仍按"未 redact"的字节**，与已修好的签名半边不再自洽 | `synapse-web/src/routes/federation/transaction.rs:639` `verify_pdu_sender_signature`、`synapse-federation/src/signing.rs:277` `verify_pdu_signature_with_client` 都只去 `signatures`/`unsigned`；上游 `keyring.verify_event_for_server` 走 `redact_event(event).get_pdu_json()` | 把签名材料收敛成**一个**函数（`synapse-federation/src/signing.rs` 导出 `signature_material(room_version, event)`：`redact_event` → 去 `age_ts`/`unsigned` → v3+ 去 `event_id`），签名与验签**同一个实现**（铁律 2）。两个验签入口加 `room_version` 参数；`transaction.rs` 用 `ctx.room_service.state().get_room_version(room_id)` 解析，解析不到就**拒绝该 PDU**（不猜） |
| **U-13-R2** | **入站 `/send` 对无 `event_id` 的 v3+ PDU 编造 ID**，并据此落库 | `synapse-web/src/routes/federation/transaction.rs:108-111`：`map_or_else(\|\| format!("${}", generate_event_id(origin)), …)` ⇒ ① `$$…` 畸形；② v3+ 应按 reference hash 计算。该 ID 经 `create_event_with_graph` 落库（该路径保留调用方 ID）⇒ 与发送方 ID 不一致，后续 `prev_events` 全部悬空 | 在 `synapse-common/src/event_id.rs` 增加入站口径的唯一函数：v3+ **禁止**携带 `event_id`（上游 `Event` 实测抛 `v2/v3 events must not have an explicit event_id`），无字段则 `compute_event_id`；v1/v2 必须有字段。`transaction.rs` 先解析房间版本（房间行，或 `m.room.create` 的 `content.room_version`）再取 ID，解析失败则**该 PDU 记错并跳过** |
| **U-13-R3** | **`/invite` v2 的 body 形状两侧都不符** | 出站：`synapse-federation/src/client.rs:1046-1058` 把**事件本身**当 body 发出；上游是 `{"event": …, "room_version": …, "invite_room_state": […]}`。入站：`synapse-web/src/routes/federation/membership/invite.rs:97` `invite_v2` 直接把整个 body 当事件解析（`body.get("content")`）⇒ 上游发来的邀请必然 400 | 出站按上游包裹三层字段（`room_version` 必填）；入站读 `body["event"]` + `body["room_version"]`，并对 `invite_room_state` 做类型校验 |
| **U-13-R4** | **入站 `invite_v2` 强制要求顶层 `event_id`** | `synapse-web/src/routes/federation/membership/invite.rs:260` `validate_federation_invite_event` 在缺字段/与路径不符时返回 400；上游 servlet 明确**不比对**路径 ID（`# TODO(paul): assert that room_id/event_id parsed from path actually match`），且合规 v3+ 发送方**不会**带该字段 ⇒ 拒绝一切上游 v3+ 邀请 | v3+ 不再要求/比对顶层 `event_id`（用本地算出的 reference hash 作为事件 ID）；v1/v2 保留原校验 |
| **U-13-R5** | **出站联邦邀请 PDU 手搓且带 `event_id`** | `synapse-services/src/room/membership/federation.rs:511` `invite_user_via_federation` 手写 JSON 且含 `"event_id": event_id`；v3+ 的签名材料已把它剔掉 ⇒ 对等端按"收到的事件"重算必然验签失败 | 改为经 `synapse_common::pdu::build_pdu` 组装（v3+ 自动不带 `event_id`），用 `finalize_local_pdu` 取真实 ID，请求路径用该 ID |
| **U-13-R6** | **`state_pdu` 仍对 v3+ 输出 `event_id`** | `synapse-web/src/routes/federation/pdu.rs:86-88` 无条件 `pdu.insert("event_id", …)`。`send_join`/`send_leave` 响应里的 state/auth_chain 都走它 | 同一函数改为按房间版本决定是否输出（或直接复用 `build_pdu`），保持"单一组装" |
| **U-13-R7** | **F-03 `re_sign_pdu_locally` 对"手搓的部分 PDU"签名** | `synapse-web/src/routes/federation/membership/mod.rs:202` 的 6 个调用点（`join.rs:168,309`、`leave.rs:104,187`、`invite.rs:86,148`）只给 `event_id/room_id/sender/type/state_key/origin_server_ts/origin/content`，随后把 `hashes`+`signatures` 写回事件行；而事务/投影路径重新发出的是**完整 PDU**（含 depth/prev_events/auth_events）⇒ 对等端重算哈希必然不等 | 签名对象改为"从持久化行投影出的完整 PDU"（与出站事务路径**同一函数**），而不是手搓 dict；投影不完整则拒绝签名（沿用 `PduCompleteness`） |
| **U-13-R8** | **3pid 响应事件里把 `room_version` 当顶层 PDU 字段发出** | `synapse-web/src/routes/federation/membership/invite.rs:229`。redaction 会丢掉未知顶层字段（所以当前不破坏签名），但规范 PDU 无此字段 | 删除该字段；房间版本只属于 `/invite` 的 **body**，不属于事件 |
| **U-13-S3** | **第 3 步互操作门槛未跑** | 本沙箱限制见 §5.2（`/etc/hosts` 不可写、`sudo` 被禁、`*.localhost` 不解析、Docker Hub 不可达、本仓联邦客户端无自定义 CA/跳过校验开关） | 已备好替代 oracle：`scripts/interop/verify_pdu_with_upstream_synapse.py`（用真实 `matrix-synapse==1.161.0` 的 `compute_content_hash` / `redact_event_dict` / `verify_signed_json` 逐项复核我们产出的 PDU）+ 一个由本仓流水线生成的 fixture。**待办**：把脚本与 fixture 落到 `opt/consolidated`，并把运行输出记进本文件；live `/send_join`+`/send` 一旦有可解析双主机名与互信 CA 的环境再补 |

### 2.2 P1

| 编号 | 问题 | 判据（实测） | 优化方案 |
|---|---|---|---|
| **U-16** | `AuthEventBuilder` 是"第二份实现 + 死代码" | `synapse-services/src/room/auth.rs` 零生产调用点（只有自身 doc/测试）；B1 已按规范实现 `room::state::auth_events::select_auth_events` 并被 `lifecycle/creation_graph.rs` 使用 ⇒ 违铁律 1/2。其 `build_auth_events` 的 `_event_type`/`_state_key` 根本未参与选择，而规范要求按事件类型选择 | 删除 `AuthEventBuilder`，把 `messaging/events.rs` 的 v12 分支改为 `select_auth_events`。**注意**：v12 分支还有一处独立缺陷——`if room_version_str.as_str() >= "12"` 是字典序比较（`"2" >= "12"` 为真）会误命中；Slice E 已改为 `room_versions::room_version_at_least`，收敛时不要再引入第二份比较 |
| **U-2** | **R-1** `user_exists` 去掉停用过滤后的扩散复核 | `user/storage.rs` 的 `user_exists` 仍 `SELECT 1 FROM users WHERE user_id=$1`；生产调用点 **19** 处（`admin/room/management.rs` 5、`membership/moderation.rs` 3、`account_identity_service.rs` 3、`user_service.rs` 2，其余 6 处各 1） | 上游 #20172 只针对 profile 字段端点。拆两个谓词：`user_exists`（含停用）/ `active_user_exists`，逐点选定；auth、federation、moderation 三处必须先审 |
| **U-19** | U-1 的 MSC3912 实现 8 点缺口 | 见下表 | 见下表 |
| **U-8** | **R-2** HTTP 层端到端签名断言缺失 | `tests/integration/api_federation_transaction_tests.rs::test_send_transaction_with_signed_pdu_accepted` 未预建房间，且断言容忍 success/error 两种结果 ⇒ 验签半边实际没有被端到端钉住 | 建房间后再发事务，断言响应成功 **且** `events.signatures`/`hashes` 非空；再加一条"篡改 content 后必须被拒"的负例（与 U-13-R1 同批做，正好互为判据） |
| **U-5** | Admin 媒体族缺失 | `admin/media.rs` 内 `media/quarantine`、`unquarantine`、房间级媒体路由 **0 命中**（仅有 `quarantine_media/{media_id}/changes`） | 先把上游 15 条端点落成可核验清单文件，再按清单补齐 |
| **U-6** | 缩略图 `animated` 缺失 | 全仓 `.rs` **0 命中** | 阶段 1 参数支持 → 阶段 2 动画检测 → 阶段 3 WebP 编码 |
| **U-14** | 跨仓客户端接线 | `CryptoDeviceAdapter.ts` **不在本仓** | matrix-sdk-fork 侧改用 `m.key.verification.*` to-device；本仓无法闭合，只能记录 |

**U-19 明细（MSC3912 逐条对照）**

| MSC3912 要求 | 本仓实现 | 判定 |
|---|---|---|
| `with_rel_types`（稳定）+ `org.matrix.msc3912.with_relations`（unstable） | `handlers/room/events.rs` 两者都解析并从 content 剔除 | ✅ |
| 只撤"子"事件，绝不撤父事件 | `cascade.rs` 以 `content.m.relates_to.event_id = 目标` 匹配 | ✅ |
| `"*"` 通配任意 relation type | 通配分支**额外**匹配 `content.m.in_reply_to`（已废弃的 rich-reply 字段，不是 relation type） | ⚠️ 超范围匹配 |
| 空列表 ≡ 不级联（不是错误） | 对空数组返回 **400** | ❌ |
| 只撤"满足该 relation type 有效性要求"的事件 | 仅按 `event_id`+`rel_type` 匹配，无有效性校验 | ❌ |
| **无权限的事件必须被忽略** | 对命中事件直接 `redact_event_content(id, None)`，**无逐事件授权检查** ⇒ 可借 `with_rel_types` 清掉**他人**的子事件 | ❌ **授权缺口** |
| 后到事件补撤（联邦晚到命中事件必须补撤） | 未实现 | ❌ 缺失 |
| 撤红必须"以请求者名义"（可审计） | `redacted_by` 传 `None` | ❌ |
| `/versions` 声明 `org.matrix.msc3912` | `capability_governance.rs` 已声明 | ✅ |

附带：级联只清**本地** content，**不产生真正的 `m.room.redaction` 事件** ⇒ 对等端不会得知子事件被撤
（重新同步/回填可能把内容带回来）；错误被 `let _ = …` 吞掉且无指标；`content->'m.relates_to'`
上没有 GIN 索引 ⇒ 两条查询都是整房间扫描。

### 2.3 P2（反冗余 / 文档收口）

| 编号 | 问题 | 判据 | 优化方案 |
|---|---|---|---|
| **U-9** | 死代码 | `handlers/auth_discovery.rs` 的 `get_auth_issuer` 无路由引用；`dag.rs` 的 `get_state_dag_edges` / `get_prev_state_events` / `find_events_referencing_missing_state` 生产调用点 0 | 铁律 1：直接删（含无计划的 `create_state_event_with_dag`） |
| **U-11** | v12/v13 文档迁移 | `room_versions.rs` 仍 `stable_parse_only("12"/"13")`（设计使然） | 把条目移入"已知取舍"，并补一条 `can_create == false` 的守卫测试 |
| **U-12** | storage 单写入口收敛 | `synapse-storage/src/event/create.rs` 仍有 5 条 `INSERT INTO events` | 抽私有 helper；公开签名不变 |
| **U-15** | 审计文档漂移 | `PROJECT_REMAINING_ISSUES_2026-09-14.md` 的 §21.1/§22.3 仍把 `auth_issuer`、`dag.rs`、`search_index`、Content Scanner 列为"未修" | 逐条标注（同一事实已漂移两次），或在文首指向本文件 |

---

## 3. 已知红门禁（不属于上述清单，但会让集成批次红）

本机定点复核（`--test-threads 1`，15 例 8 过 7 红），7 条红**全部**属同一个既有家族：

| 测试 | 成因 |
|---|---|
| `api_route_ledger_tests::declared_route_manifest_entries_are_actually_wired` | 路由 manifest 里 app_service 的通配符代理路由（`/_matrix/{app,client}/v1/proxy/{as_id}/{*path}`）与 live router 不一致 |
| `api_route_ledger_tests::declared_route_manifest_full_snapshot_matches_default_state` | 同上（manifest 快照落后） |
| `api_route_ledger_tests::declared_route_manifest_full_snapshot_matches_worker_enabled_state` | 同上（worker 车道快照落后） |
| `api_route_snapshots_tests::snapshot_capabilities_v3` | `/capabilities` 快照落后（含 v12/动画缩略图等能力位） |
| `api_route_snapshots_tests::snapshot_versions_endpoint` | `/versions` 快照落后 |
| `api_auth_routes_tests::test_auth_issuer_returns_unrecognized_when_oidc_is_disabled` | `auth_issuer` 路由被 `76e5f9136` 摘除，用例仍断言旧行为（期待 400，实得 404）⇒ **摘路由时漏改用例** |
| `api_admin_room_lifecycle_tests::test_admin_room_lifecycle_management` | 管理端房间删除语义未与上游对齐 |

处置纪律（AGENTS.md R11）：**不得** `cargo insta accept`，也不得加 `#[allow]` 放宽；
按"先修再转"独立提交逐条修。其中 4 条快照/manifest 类必须先在 `--all-features` 下复现再改。

> 已被 U-20 修绿的既有红（不再是红）：`test_admin_room_history_purge`、
> `test_scanner_info_contract_is_not_empty_success`、`relation_is_allowed_for_members`、
> `test_global_thread_routes_return_real_data`、`test_room_context_rejects_non_member_and_admin_override`、
> `test_sync_filter_applies_room_timeline_matchers_before_limit`。

---

## 4. 仍然生效的关键决策（改代码前先读）

1. **事件身份只有一个来源**：本地事件由写入口（`GraphMetadataWriter` → `finalize_event_id` →
   `synapse_federation::event_finalize::finalize_local_pdu`）产出；v1/v2 保留服务器随机 ID，
   v3+ 为 reference hash。调用方**只能消费**返回值，不得自行 `generate_event_id` 再期望它是最终 ID。
2. **PDU 组装只有一份**：`synapse_common::pdu::build_pdu`（v3+ 不输出 `event_id`/`user_id`）。
   出站广播、写入口 finalize、入站投影都必须走它。
3. **签名材料 = 按房间版本 redact 之后的字节**：`redact_event(room_version, …)` → 去 `age_ts`/`unsigned`
   → v3+ 去 `event_id`。签与验必须共用同一个实现（U-13-R1 就是把验签半边并回来）。
4. **房间版本比较只允许数值**：`synapse_common::room_versions::room_version_at_least`；
   不可解析的版本 **fail-closed**。不得再写字符串比较或第二份 `parse::<u32>()`。
5. **房间版本解析不猜**：解析不到就拒绝（拒绝签名 / 拒绝该 PDU），不设默认值
   （join 模板原来的 `unwrap_or("10")` 已删）。
6. **v12/v13 仍 parse-only**：只做解析与能力声明，不放开创建；v12 的四项（MSC4289/4291/4297/4307）
   只有部分落地，放开前必须逐项复核。
7. **MSC3912 是单层语义**：只撤直接关联的子事件，不做递归；空列表不是错误（U-19 待修）。
8. **不升 `/versions` 到 v1.16**：只补错误码，不做版本面升级。

---

## 5. 复现与阻塞

### 5.1 一键复现（本机）

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 格式 / 棘轮
./scripts/check_fmt_ratchet.sh
bash scripts/ci/check_sqlx_dynamic_ratio.sh

# clippy（CI 两档都要；第二档才编译 integration target）
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings

# lib / unit
cargo nextest run --workspace --lib --all-features --locked --test-threads 4
cargo nextest run --test unit --features test-utils --locked --test-threads 4

# 集成（必须 --all-features；DB 前置见 §1.3）
cargo nextest run --profile ci --all-features --test integration --test-threads 1
```

### 5.2 互操作门槛的外部阻塞（实测，非推测）

- `docker manifest inspect matrixdotorg/synapse` 超时（Docker Hub 不可达），**无上游 Synapse 镜像**；
- `/etc/hosts` 不可写、`sudo` 被禁、`*.localhost` 在本机不解析 ⇒ 造不出两个可解析的服务器名；
- 本仓联邦客户端**没有**自定义 CA / 跳过校验的开关 ⇒ 无法信任自签 CA。

⇒ live `/send_join` + `/send` 在本沙箱**不可能**跑通。已落地/待落地的替代物见 **U-13-S3**：
用真实安装的 `matrix-synapse==1.161.0` 复算 `hashes` / `event_id` / `signatures`。
本机已有该环境：`/tmp/peer-synapse/bin/python`（`synapse 1.161.0`，直接读其
`compute_content_hash`、`redact_event_dict`、`verify_signed_json`）。

---

## 6. 不建议现在做

| 项 | 原因 |
|---|---|
| `event_id` 全量改造（把所有历史事件重算） | 未发布、无外部用户，没有需要迁移的数据；改造只增加风险 |
| MSC3912"后到事件补撤" | 需要事件流订阅 + 幂等补撤，属独立设计（先做 U-19 的授权/有效性缺口） |
| v12/v13 放开创建 | v12 四项未逐项复核，v13 语义未对齐上游 |
| scanner 全格式深扫 | 当前只有 text/media 面；深扫需要沙箱与配额设计 |
| Admin 面 100% 追平上游 | 先按 U-5 落清单，再按使用频率排序 |
