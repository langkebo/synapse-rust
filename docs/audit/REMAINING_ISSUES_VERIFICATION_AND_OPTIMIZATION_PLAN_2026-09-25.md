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
| U-13 入站半边：验签按版本 redact（R1） | U-13 | `16c654dcc` | 签名/验签共用 `signature_material`；新增"按旧材料签的名必须验不过"用例 |
| U-13 入站半边：事件 ID 由 PDU 推导（R2） | U-13 | `16c654dcc` | `resolve_received_event_id`（v3+ 禁止携带 ID、无字段则算 hash；v1/v2 必需）；`/send`、gap-fill、`/backfill` 三处接线 |
| U-13 `/invite` v2 body 形状（R3） | U-13 | `16c654dcc` | 出站包 `{event, room_version, invite_room_state}`；入站读 `body["event"]` 并校验版本一致 |
| U-13 入站不再强制顶层 `event_id`（R4） | U-13 | `16c654dcc` | v3+ 由 reference hash 推导；持久化改走显式图路径，回的是落库后的 ID |
| U-13 出站联邦邀请 PDU 单一组装（R5） | U-13 | `16c654dcc` | `build_pdu` + `finalize_local_pdu`，不再手搓、不再带 `event_id` |
| U-13 3pid 事件不再带顶层 `room_version`（R8） | U-13 | `16c654dcc` | 版本只属于 `/invite` body |
| U-13 第 3 步：oracle 互操作门槛 | U-13-S3 | `c43e68b81` | 真实 `matrix-synapse==1.161.0` 复算 v3/v10/v11 fixture 的 hashes/ID/签名，全 PASS；含反向自证 |
| U-13-R6：`state_pdu` 按版本输出 `event_id` | U-13 | `10ac3253f` | v3+ 不再发（`/send_join`、`/state`、`/get_room_auth`、`/get_event_auth`）；`build_pdus` 按 room_id 缓存版本解析 |
| U-13-R7：本地重签覆盖对端真正收到的 PDU | U-13 | `8a7185e05` | `re_sign_pdu_locally` 改为投影持久化行后再签；投影不完整/版本读不到则留空；6 个手搓 dict 删除；DB 级用例 + 变异自证 |
| 测试夹具 `KeyRotationManager` 从未初始化（签名静默 no-op） | — | `8a7185e05` | `setup_federation_app` 现显式 `initialize(...)`；此前"签名测试"实际没测到 |
| U-19 级联逐事件授权 + 空列表不再 400 | U-19 | `975c379f9` | 每个子事件先过既有 `RoomAuth::can_redact_event`（未授权跳过并记 `security_audit` 日志）；`with_rel_types: []` 不再 400，等价于不级联 |
| **客户端撤回恒 500（P0，既有）** | — | `3e304b69c` | 处理器把**用户 ID** 写进 `events.redacted_by`，而该列是到 `events.event_id` 的自引用外键（`fk_events_redacted_by`）⇒ 每次客户端撤回都违反外键。改为写撤回事件自身的 ID |
| U-16 删除/收敛 | U-16 | `4cce7d782` | `room/auth.rs` 整体删除（含 4 条钉错行为的用例），v12 写路径改走 `select_auth_events`；零调用点 ⇒ 违铁律 1/2 的重复实现消失 |
| 并发会话留下的编译 + clippy 红门禁 | — | `cb68220e6` | MSC4222 的 `sync` 第 8 参数未同步两个集成测试；`map_or`/未用绑定 4 条 |
| `user_exists` 双谓词拆分 | U-2 | `bb69ad7a0` | 拆成 `user_exists`（行存在，含停用；供 profile 字段端点与用户名可用性）与 `active_user_exists`（仅未停用；auth/federation/moderation 必须走它）。19 处生产调用点逐点选定并列表；`AccountIdentityService::ensure_active_user_exists` 曾错误地调用 `user_exists`（本轮修复）|
| HTTP 层端到端签名断言 | U-8 | `3495c6892` | 真建 v11 房间，PDU 经生产流水线（`build_pdu` → `finalize_local_pdu` → `sign_and_hash_event`）组装；断言 per-PDU `success`（不再容忍 success/error）与**落库** `hashes`/`signatures` 逐字等于所发。两条负例：篡改 `origin_server_ts`（redaction 保留字段）+ 重算内容哈希 ⇒ 必须被 sender-signature 拒绝；v3+ 携带显式 `event_id` ⇒ 必须拒绝 |
| v≤11 写路径持久化图字段 | U-13-R9 | `11bf5455d`、`113765e9b` | 已知房间版本统一走图感知写入；`create_event_with_pdu` 成为**唯一**图写路径（event 行 + `event_edges` 同一本地事务），`create_event_with_graph` 改委托适配器（铁律 2）。**注意**：`11bf5455d` 单独合并会丢掉 `event_edges`（见 §1.4），两者必须一起 |
| 本地事件身份在调用方事务路径 finalize + 消费 | U-13-R10 | `b6b923d8a`、`31b475710` | tx 路径按**调用方连接**读房间版本后 finalize（`room_version_in_tx`）；`send_message` 的 relation 索引/beacon/返回值改用写入口返回的 ID；`knock`/`voip` 不再回占位 ID、`voip` 的错误不再被 `let _ =` 吞掉；清除 `$$…` 占位；`room_memberships.event_id` 不再伪造（缺 ID 即 NULL）|
| 专用 reaction 路由写入 `events` | U-20（reaction 写入口）| `29ac9b316` | `send_annotation`/`send_reference`/`send_replacement` 改走 `MessagingService::create_event`（单一写入口），`event_relations` 只作索引 ⇒ 真实 reaction 对 MSC3912 级联可见；`m.relates_to.key` 优先于顶层 `body` |

### 1.2 阶段末门禁快照（本机实测）

| 门禁 | 命令 | 结果 |
|---|---|---|
| 格式棘轮 | `./scripts/check_fmt_ratchet.sh` | `current=0 baseline=0` ✅ |
| SQLx 比率棘轮 | `bash scripts/ci/check_sqlx_dynamic_ratio.sh` | 生产 `432<=432`、测试 `711<=711`、静态 `1037>=1037` ✅ |
| clippy（CI 口径，两档） | `SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils [--all-features] --locked -- -D warnings` | `--all-features` 档 exit 0 ✅（在 `ca9464da4` 上实测）；**另一档（features-args 为空）需在干净树上补跑** —— 复跑时被并发会话 `sync_service/*` 的在途改动挡住（见 §1.3，不得把在途状态当提交态结论） |
| lib 全量 | `cargo nextest run --workspace --lib --all-features --locked --test-threads 4` | **6368 / 6368 passed, 0 skipped** ✅（在 `ca9464da4` 上实测） |
| unit 全量 | `cargo nextest run --test unit --features test-utils --locked --test-threads 4` | **未定格** ⚠️ —— 复跑时并发会话正在改 `sync_service/*`（工作树非干净，编译不过的是它的在途状态，不是提交态）。复跑前必须确认 `git status --short` 为空 |
| 集成（受影响面 + 既有红复核） | `--profile ci --all-features --test integration --test-threads 1` | 定点 15 例：8 过 / 7 红（红项全部是 §3 的既有家族） |
| 集成（本批受影响面） | 同上，过滤 `api_federation_transaction` \| `federation_existence_leak` | **11/11 绿**（`16c654dcc`） |
| U-13 定点单测 | `-p synapse-federation --lib`（signing/verify）/ `-p synapse-common --lib`（event_id）/ `-p synapse-services --lib`（backfill） | 32/32、17/17、3/3 |
| 集成全量 | 同上（无过滤器） | **未跑**（时间预算 + 每轮新建数百 schema 使 DDL 超线性变慢）。不得当作通过 |
| 联邦互操作（oracle） | `tests/unit/u13_interop_fixture_tests.rs` + `scripts/interop/verify_pdu_with_upstream_synapse.py` | **PASS**：真实 Synapse 1.161.0 复算 v3/v10/v11 三个 fixture 的 `hashes`/事件 ID/签名，exit 0；篡改字节后 exit 1（能变红）。live `/send_join`+`/send` 仍被 §5.2 阻塞 |

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

### 1.4 U-2 / U-8 / U-13-R9(+R10) / U-20 并入后的复测（2026-09-27，本机实测）

四条分支并入 `opt/consolidated` 的落点：U-2 `9efc01f8a`、U-13-R9 部分 `03e1cc920`、
U-20 `7ab85a87f`（由并发会话先行并入）；U-8 `111a4df28`、U-13-R9 完整 `dcec344e2`
（本轮补并，含 `113765e9b` 的边修复与 `b6b923d8a`/`31b475710` 的 ID 收口）。
合并后 tip：`dcec344e2`，工作树 `git status --short` 为空。

| 门禁 | 命令 | 结果 |
|---|---|---|
| 格式棘轮 | `./scripts/check_fmt_ratchet.sh` | `current=0 baseline=0` ✅ |
| 编译 | `SQLX_OFFLINE=true cargo check --workspace --all-targets --all-features --locked` | exit 0 ✅ |
| clippy（两档） | 同 §1.3 两条命令 | 两档均 exit 0 ✅ |
| SQLx 比率棘轮 | `bash scripts/ci/check_sqlx_dynamic_ratio.sh` | 生产 `323<=323`、测试 `716<=716`、静态 `1146>=1145`、QB `18<=18` ✅ |
| `.sqlx` 缓存 | `check_sqlx_cache_fresh.sh --static` / `--compile` | 1114 条、被跟踪、离线构建通过 ✅ |
| 四单元定点回归 | `--profile ci --all-features --test integration --test-threads 1`，13 例 | **13/13 通过**（U-8 三条签名断言、R9 图字段/边、R10 tx finalize + knock + voip、U-2 `add_member` 投影、U-20 级联 reaction）|

⚠️ **两条环境性红项（非代码缺陷，复跑前先读）**：
1. `check_sqlx_cache_fresh.sh --full`（需 `DATABASE_URL` 指向**已迁移**库）在本机**必然失败**：
   共享 `public` schema 现为 **0 表**（并发会话 `406ab07ef` 把 `public` 收敛进 CI seed，
   与 §1.3 第 1 条的历史前提相反）。`cargo sqlx prepare --check` 以默认 `search_path` 连库 ⇒
   宏全部无表可用（实测 `synapse-storage` 1410 个错误）。权威判据改用 `--compile`（离线缓存完整性），
   它对这次合并是绿的。
2. `synapse-services` 的 `media_fixture_keeps_its_isolated_schema_for_the_whole_test` 需要
   `public` 里有 `upload_progress` 才能跑（`media/mod.rs:1239` 的守卫），同样被上一条挡住。
   排除该环境用例后 `-p synapse-services --lib --all-features` 实测 **2082/2082 通过**。

---

## 2. 仍然存在的问题

优先级口径：**P0** = 直接阻塞联邦互操作或数据正确性；**P1** = 明确的规范/实现缺口，有清晰修法；
**P2** = 反冗余与文档收口。

### 2.1 P0 —— U-13 第 2 步剩余接线：联邦互操作的最后一段

背景：Slice A/B/D/E 把**本地产生**的事件身份切成 reference hash（v3+），
入站半边与邀请面已由 `16c654dcc` 补齐（R1–R5、R8 见 §1.1），R6/R7 亦已收口
（`10ac3253f`、`8a7185e05`）；写入面缺口 **U-13-R9** 与由它暴露的 ID 消费残留
**U-13-R10** 已收口（见 §1.1）。**只剩**仍未跑的 live 传输层门槛。

| 编号 | 问题 | 判据（实测） | 优化方案 |
|---|---|---|---|
| **U-13-S3** | **live 互操作仍未跑**（oracle 已落地并通过） | 本沙箱限制见 §5.2。已落地替代门槛（`c43e68b81`）：`tests/unit/u13_interop_fixture_tests.rs` 用固定输入跑真实流水线并与提交的 fixture 逐字节比对；`scripts/interop/verify_pdu_with_upstream_synapse.py` 用真实 `matrix-synapse==1.161.0` 复算三个 fixture 的 `hashes`/事件 ID/签名（v3/v10/v11 全 PASS，篡改即 FAIL） | 只剩**传输层**：需要可解析的双主机名 + 互信 CA（本仓联邦客户端目前没有自定义 CA/跳过校验开关）。有该环境时补 `/send_join` + `/send` 实测并把输出记进本文件 |

### 2.2 P1

| 编号 | 问题 | 判据（实测） | 优化方案 |
|---|---|---|---|
| **U-19** | U-1 的 MSC3912 实现缺口：**已修 2 项**（逐事件授权、空列表语义），**余 5 项**：晚到事件补撤、级联的 `redacted_by` 审计归属（仍传 `None`）、不为子事件发出真正的 `m.room.redaction`（对等端不知情）、`content->'m.relates_to'` 缺 GIN 索引（两条查询都是整房间扫描）、`"*"` 分支越界匹配已废弃的 `m.in_reply_to` | 见下表（❌ 行中已修的两项标 ✅） | 见下表 |
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
| **U-11** | ~~v12/v13 文档迁移~~ | ✅ **已修复**（2026-09-26 复核）：`room_versions.rs:253` 存在 `!can_create_room_version("13")` 守卫测试；v12 已升为 `stable`（line 113），v13 仍 `stable_parse_only`（line 114）。原"补守卫测试"要求已满足 | — |
| **U-12** | ~~storage 单写入口收敛~~ | ⚠️ **仍存在（但非阻塞）**：`synapse-storage/src/event/create.rs` 仍有 5 条 `INSERT INTO events`（`create_event` / `create_event_with_pdu` / `create_event_with_graph` / `create_state_event_with_dag` / `upsert_power_levels_event`）。未提取私有 helper 收敛，但各函数公开签名保持不变，不影响外部调用。优先级：低（P2） | — |
| **U-15** | ~~审计文档漂移~~ | ⚠️ **已知超代文档**：`PROJECT_REMAINING_ISSUES_2026-09-14.md` 的 §21.1/§22.3 仍错误列出 `auth_issuer`、`dag.rs`、`search_index`、Content Scanner 为"未修"。但该文件顶部已有**失效声明**（lines 62-68）明确指向本文件为唯一权威来源。漂移原因是旧文件未同步更新，不影响本文件的有效性。**处置**：在本文件文首已注明"口径：只保留当前仍然存在的问题"，旧文件仅保留作 git 历史追溯 | — |

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

⇒ live `/send_join` + `/send` 在本沙箱**不可能**跑通。已落地的替代物见 **U-13-S3**（oracle 已 PASS）：
用真实安装的 `matrix-synapse==1.161.0` 复算 `hashes` / `event_id` / `signatures`。
本机已有该环境：`/tmp/peer-synapse/bin/python`（`synapse 1.161.0`，直接读其
`compute_content_hash`、`redact_event_dict`、`verify_signed_json`）。

```bash
# 复跑 oracle（三个 fixture 都必须 PASS；篡改任一字节必须 FAIL 且 exit 1）
for v in 3 10 11; do
  /tmp/peer-synapse/bin/python scripts/interop/verify_pdu_with_upstream_synapse.py \
      tests/interop/fixtures/local_pdu_v$v.json || echo "ORACLE FAILED v$v"
done
```

---

## 6. 不建议现在做

| 项 | 原因 |
|---|---|
| `event_id` 全量改造（把所有历史事件重算） | 未发布、无外部用户，没有需要迁移的数据；改造只增加风险 |
| MSC3912"后到事件补撤" | 需要事件流订阅 + 幂等补撤，属独立设计（先做 U-19 的授权/有效性缺口） |
| v12/v13 放开创建 | v12 四项未逐项复核，v13 语义未对齐上游 |
| scanner 全格式深扫 | 当前只有 text/media 面；深扫需要沙箱与配额设计 |
| Admin 面 100% 追平上游 | 先按 U-5 落清单，再按使用频率排序 |
