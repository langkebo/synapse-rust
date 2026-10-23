# Synapse-Rust 与 Synapse (Python) 地址：https://github.com/element-hq/synapse 对比分析报告

> **状态（2026-10-16）**：**当前口径是 §19（v1.11 对照 Synapse v1.163.0）**。§18 为上一轮口径（v1.10 对照
> v1.162.0），§1–§17 为更早的历史轮次记录。§11.2/§11.3/§12.4/§12.5/§15.3 有若干已被后续提交修掉却仍写作
> "缺失"的**假缺口**，清单见 §18.5；§19.6 另给出**对 §18 自身口径**的作废清单（含 §18.4 中已过时的两条）。
> 另：本文多处记"v12/v13 为 `stable_parse_only`、不可创建"，已过时：v12 是**唯一可创建**版本（G-1），
> 版本 13 已从能力表移除（Q5(b)）。v12 语义落地情况见 `docs/audit/ROOM_V12_PLAN_STATUS_2026-09-27.md`。

> **文档版本**: v1.11
> **更新日期**: 2026-10-16
> **v1.11 更新说明**:
> - **上游基线升级到 Synapse v1.163.0（2026-10-14 发布；rc1 2026-10-06）**，新增 **§19**（当前口径）。
>   本轮**不再以既有文档为基线**，而是回到上游 `v1.162.0...v1.163.0` 的 147 个变更文件逐项取证，
>   再对本仓源码逐条实测。结论摘要：
>   - **已对齐（含超前）**：Matrix 版本宣告（本仓已到 `v1.17`，上游本版才补 `v1.14`/`v1.15`）、
>     MSC4354 sticky 的到期语义与联邦 EDU、MSC4140 专用调度端点、OTK 单算法 500 上限、
>     profile 未设置字段返回 `{}`、`/relations` 的 `recurse`、stream 位置指标。
>   - **当时识别出的真实差距（P0/P1；①–⑦ 已于 2026-10-16/17 全部修复）**：① **账户注销是"只置标志位"**——不退出房间、不清推送器、不取消 MSC4140
>     延迟事件（上游 #20247 只修了延迟事件这一条，本仓整条注销管线都缺）；② **`/preview_url` 曾是桩**：
>     不抓取远端 HTML，返回**伪造的 OpenGraph**（含拼出来的 `og:image`），上游同版本正在收紧该字段
>     （#20211）；**2026-10-22 已实现真实抓取**（钉扎 IP + 逐跳 SSRF 校验 + 体积/重定向上限，
>     只输出 `og:title`/`og:description`，`og:image` 待媒体镜像与隔离检查，见 §19.5.1 B2）；③ **应用服务（MSC2409）临时数据投递未接线**（`AppServiceEphemeralBatch` 只有类型与
>     载荷构造函数，没有从回执流取数的生产调用方），上游 #20108 修的是"超过 100 条回执被永久跳过"；
>     ④ **lazy-load 成员缓存只记 user_id、不记事件 id 与响应位置**（上游 #20289 的同类缺陷，且本仓
>     缓存持久化到 `device_storage`，丢失窗口更宽）；⑤ **MSC4075（MatrixRTC 邀请/来电）推送规则缺失**——
>     既无默认规则（`synapse-common/src/push_rules.rs`），也未宣告 `org.matrix.msc4075`；
>     ⑥ **MSC4354 的 `org.matrix.msc4354` 未在 `/versions.unstable_features` 宣告**——实现存在但客户端
>     发现不了；⑦ **profile 更新缺 `LEFT_ROOM` 语义**（上游 #20203：离开最后一个共享房后要告知客户端
>     删除本地资料），本仓原以 `users.updated_ts` 轮询替代，无法表达"不再共享房间"（**已修 2026-10-17**：
>     `profile_updates.users[user] = null`，见 §19.5.1 R9）。
>   - **修复进度（2026-10-17 更新，逐项见 §19.5.1）**：上列 ①（A1 注销管线）、②（B2 `preview_url`）、
>     ③（B3/UG-4 AS 临时数据投递：`deliver_pending_receipts` 分页 + `reached_token` 循环已接线）、
>     ④（B1 lazy-load 位置维度）、⑤（A4 MSC4075 规则）、⑥（A2 MSC4354 宣告）**已落地**；
>     ⑦（profile `LEFT_ROOM` 语义）**已落地**（2026-10-17，零迁移）。
>   - **另（§18.8.1「P1 功能缺口/安全面」5 项，2026-10-16 分支 `fix/p1-functional-security-gaps`）**：
>     ① 房间别名静默改指、③ push rule 数量/体积上界、④ AS 设备管理免 UIA、⑤ MSC4354 标准请求面
>     （`/send` 面）**已修**；② AS 临时数据投递**已接线**（2026-10-17）。逐项见 §19.5.1 的
>     「P1 实施记录」。
>   - **结构性不适用**（附本仓理由，非"没找到"）：Python/twisted/poetry/GIL（#20246/#20252/#20267/
>     #20259/#20354）、`user_ips` 表（本仓无该表）、thread 级角标计数（本仓计数为房间粒度，无跨房
>     `thread_id` 污染面）、MAS 委派的前提下 Admin API 拒建用户（#20241；MAS 运行时**已接线**，见 §19.6(a) 更正）。
>   - 另附 **§19.6 口径更正**：§18.4 的两条声明已过时（recurse 已实现、MSC4140 专用端点已实现），
>     以及 §18 的证据栅格中 v1.15 那句"hierarchy 的 `allowed_room_ids` 未返回"**已作废**。
> - **v1.10 上游基线升级到 Synapse v1.162.0（2026-09-29 发布）+ 本仓差距复核（2026-10-01）**：
> **更新说明**:
> - **v1.10 上游基线升级到 Synapse v1.162.0（2026-09-29 发布）+ 本仓差距复核（2026-10-01）**：
>   新增 **§18**（当前口径），按"① 上游变更要点 ② 逐项对照 ③ 分优先级建议 ④ 待验证清单"组织。
>   结论摘要：v1.162.0 无 Deprecations/Removals。本仓**已落地该版本的大部分对齐项**（原分支
>   `feature/2026-10-01-metrics-docs-updates`，tip `e1ffcb2ab`，2026-10-01 已合并进 `main`）：默认房间版本 12、OTK 单设备单算法 500 上限、
>   `rc_profile`、profile 空字段 `{}`、hierarchy `allowed_room_ids`、稳定 `M_UNKNOWN_DEVICE`、
>   非合规历史 user id 过滤、MSC4354 sticky EDU、缩略图异步（逐项证据与提交见 §18.3）。
>   **仍然存在的差距**（§18.4）按优先级为：① ~~**委派认证/MAS 运行时未接线**~~ → **已接线**（2026-10-17 更正：`container.rs` 的 `build_mas_validator` → `with_mas_validator` 已注入，见 §19.6(a)）
>   ② **MSC4311 宽限开关是死配置 + knock stripped state 缺失** ③ `redis.username` 缺上游的"必须配密码"启动校验
>   ④ MSC4222 `since` 落批次内的边界 ⑤ room_summary `join_rules` 反规范化列可陈旧
>   ⑥ 第三方规则回调未接入事件鉴权 ⑦ 状态决议缓存缺失 ⑧ `/relations` 无 `recurse`（MSC3981）
>   ⑨ stream-position 指标口径 ⑩ MSC4242 无 HTTP 面（上游 #20133 实为把 MSC4242 接进**既有**联邦端点，非新增路由；受阻于 MSC4242 房间版本 + experiment）。
>   同时给出 **§18.5 假缺口作废清单**（§12.4/§12.5/§15.3 中"已修却写缺失"的 10+ 条，
>   以及本轮初稿按旧基线误判、已更正的 6 项）与 **§18.6 待验证点**。
> - **v1.9 全面审查（2026-09-27，HEAD `7db9d57db`，分支 `opt/consolidated`）**：系统性回到源码逐条复核
>   文档中"仍存在"的问题声明，**纠正 1 处误判**：
>   1. **U-19-R2 的"`events` 表缺 `content` GIN 索引"为误判**：`idx_events_content_gin` 索引已存在于
>      `migrations/00000000_unified_schema_v12.sql`（2026-09-27 核查确认）。
>   **确认已解决问题**：U-19-R4（`redacted_by` 审计追踪已在 `5d1bfdc3f` 修复）、U-2/U-13-R9/U-20
>   （已在之前提交中修复）。**仍存问题更新**：U-5 Admin 媒体端点从 9 条增至 14 条（工作区未提交，
>   完成度 78%，缺 4 条：用户隔离、按时间删除、远程缓存清理、解除保护）。
>   **路由契约漂移**：ROUTE_CONTRACT.md 未及时更新，`admin/media.rs` 显示 11 条而非实际 14 条，需重新生成。
>   **刷新计数**：`.rs` 1,040 个 / 455,233 行；SQLx 静态 1,192 / 动态 1,009（静态化率 54.2%）。
> - **v1.8 E2EE 去服务端私钥重构（2026-09-25）**：删除**全部**"服务端参与 SAS 密码学 / 服务端替客户端
>   宣布已验证"的实现，回归 Matrix 规范形态 —— SAS 的 ECDH/HKDF/MAC 全在客户端算、**私钥永不离开
>   客户端**，服务端只做 `PUT /sendToDevice/{event_type}/{transaction_id}` 的中继。删除面：
>   `synapse-web/src/routes/verification_routes.rs`、`synapse-e2ee/src/{verification,device_trust}/`、
>   baseline 中 7 张 `device_trust*/verification_*/e2ee_security_events` 表、`CrossSigningVerificationService`
>   的 device-trust 依赖、6 个 `/device_verification|/device_trust|/security/summary` 端点。
>   **路由计数随之 1,165 → 1,135、模块 66 → 65（净减 30 条，全为主动删除）**；§3.4/§7 相应改写。
>   详见 `docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md` §23。
> - **v1.7 P0-1 收窄（2026-09-25，分支 `opt/consolidated`）**：联邦 `/send_join` 的**响应面与 PDU
>   字段面已修** —— v1 补 `[200, {…}]` 二元组包装、v1/v2 补 `origin`；四条发射路径
>   （`/send_join` v1+v2、`/state`、`/get_room_auth`、`/get_event_auth`）里各自内联的手工 JSON
>   统一收敛到新模块 `synapse-web/src/routes/federation/pdu.rs`，产出含 `origin_server_ts` 的真实
>   PDU，并复用库中 `hashes`/`signatures`、否则以本机密钥**现场签名**；`StateEvent` 补 4 个
>   JSONB 字段。**⚠️ 这不是"已合规"**：本批**未动写入路径**——本地事件仍走
>   `create_event`（INSERT 无 `depth`/`prev_events`/`auth_events`），故本地起源 PDU 仍判
>   `MissingGraphMetadata` 并**故意不发签名**（不伪造 DAG 位置）。§15.3 现为**两行 P0**：
>   第一行"已收窄（残余三条）"、第二行"`event_id` 非 reference hash"。详见
>   `docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md` §21.5。
> - **v1.8 代码复核（2026-09-26，HEAD 待更新，分支 `opt/consolidated`）**：系统性回到源码逐条复核
>   文档中"仍存在"的问题声明，**纠正 3 处误述**：
>   1. **U-19 级联撤回的"逐事件授权缺失"为误判**：`synapse-services/src/event_redaction_service.rs:134`
>      确实在每次红事前调用 `can_redact_event` 检查权限；
>   2. **U-19 的"空列表返回 400"为误判**：`synapse-web/src/routes/handlers/room/events.rs:993` 的空列表
>      现意为"不级联"而非错误；
>   3. **U-20 的"reaction 不写入 events 表"为设计取舍**：通过 `event_relations` 表独立存储关系
>      是 MSC3912 规范形态，`cascade.rs` 的 `find_related_events` 正确读取
>      `events.content->'m.relates_to'`。
>   **剩余真实问题**（经代码验证确认）：U-19-R4（`redacted_by=None` 失去审计追踪，
>   `event_redaction_service.rs:149`）、U-19-R2（`events` 表缺 `content` GIN 索引）、
>   U-13-R9（v≤11 写路径不持久化图字段，`create_event.rs:25-47`）。
>   **新增 §16** 记录本轮代码验证方法与更正清单。另**全面刷新计数**（`.rs` 1,030 个 / 447,508 行；
>   `docs` 203；`tests` 300；per-crate 见 §2.2）并**修正两处严重失真的数字**：
>   §3.2 的 SQLx 静态化比例实为 **静态 806 / 动态 1396（36.6% / 63.4%）**，而非本文档长期写的
>   "static 61 / dynamic 2147 ≈ 2.8%"（旧计数含 turbofish 与注释误算，已由棘轮计数器修复后重测）；
>   §3.4 引用的 `API_COVERAGE_REPORT.md` 逻辑端点实为 **813**（旧版 883 无机器来源；2026-09-25 起随路由删除降至 **795**）。
>   **新增 §15** 记录 v1.6 判定表与"仍然存在"清单（当时口径：P0-1 是唯一未修 P0，**已被 v1.7 收窄**）。
> - **v1.5（2026-09-23 续）**：P0 收口与规范对齐。**P0-2 OIDC 回调提权已修**
>   （回调路径写入并复用 OIDC 绑定 `fe35fb0a`；账号接管判定抽成纯函数并补判定表用例 `0a633b89`）；
>   **P0-3 `soft_failed` 无读路径过滤已修**（`53c43a48` 时间线无游标分支 + `7d968f6d` 其余全部
>   消费者读取面 + 行为回归用例）；**SSSS 对齐 `m.secret_storage.v1.aes-hmac-sha2`**
>   （HKDF + AES-256-CTR + encrypt-then-MAC + 16 字节 IV/bit 63 清零，含 NIST SP 800-38A F.5.5
>   已知向量，`a2375743`）。**P0-1 联邦 `/send_join` 的 `state`/`auth_chain` 仍阻塞**：取证发现
>   根因是本地创建的事件从不落 PDU 图元数据（无 `depth`/`prev_events`/`auth_events`/签名），
>   修法需补整条创建期 PDU 管线，详见 §14.4。
>   **文档可信度变成门禁**：`tests/unit/doc_credibility_guard_tests.rs` 校验本文档引用的仓库相对
>   路径必须存在、声明的 route/模块计数必须等于 `docs/synapse-rust/ROUTE_CONTRACT.md`（两个纯谓词
>   各有红证明）。计数口径统一改为 `git ls-files`（`.rs` 1,019 个 / 442,451 行；`docs` 188；
>   `docker` 58；`tests` 292；`benches` 5；`migrations` 3），避免 `find` 随 gitignore 与本地产物漂移。
> - **v1.4 代码取证复核（2026-09-22）**：不再以既有文档为基线，改为**逐条回到源码取证**
>   （证据形式仅 `路径:行号` + 可复现命令）。本轮**推翻本文档 5 处旧结论**（SAS/QR/`leak_detection`/
>   `create_event_with_graph`/OIDC 校验接线），并**新增 3 条 P0**：联邦 `/send_join` 缺
>   `state`/`auth_chain`、OIDC 回调提权、`soft_failed` 读路径无过滤；§11.1 联邦协议由 ✅ 降为
>   PARTIAL，§7.2 SSSS 新增非合规判定。**v1.5 已修正其中两条 P0 的判定**（P0-2/P0-3 已修）
>   并对 `validate_id_token_claims` 再作更正（Phase 3 C9 已整体删除该函数）。
>   §14.1 保留 5 处更正的摘要并标注最新状态，§14.2 为 3 条 P0 的当前状态。
> - **v1.3 复核（2026-09-22）**：逐条实测 v1.2 的 ✅ 声明与全部计数，修正 10+ 处计数/版本错误、
>   1 处**伪造引用**（`docs/synapse-rust/api-reference.md` 不存在）、特性片段中已删除的 `server` 特性、
>   "编译时 SQL 验证""静态链接 + musl"等高估；§11 增补实测判定（E2EE SAS/QR/泄漏检测、MSC4140、
>   MSC3912 归因、v1.161 八项）；§12.5 重写为指向权威清单的方案。
>   **详细证据与命令见** `docs/audit/archive/COMPARISON_REPORT_REVIEW_2026-09-22.md`。
> - v1.2（2026-09-22）：对齐基准从 Synapse v1.149.1 更新为 v1.161.0（2026-09-15 发布）；
>   修正 vodozemac 版本与 crate 计数；补充 MSC4140/3912/4242 状态与 v1.161 Bug 修复核查项。
> **生成日期**: 2026-09-16
> **对比对象**: synapse-rust (Rust 重写实现) vs element-hq/synapse (Python 原始实现)
> **对齐基准**: Synapse v1.163.0（2026-10-14 发布，`release-v1.163` CHANGES.md；v1.163.0 = rc1 + Redis/pip 启动修复）

## 目录

1. [项目概览](#1-项目概览)
2. [架构对比](#2-架构对比)
3. [技术实现对比](#3-技术实现对比)
4. [性能指标对比](#4-性能指标对比)
5. [可扩展性对比](#5-可扩展性对比)
6. [可维护性对比](#6-可维护性对比)
7. [安全特性对比](#7-安全特性对比)
8. [用户体验对比](#8-用户体验对比)
9. [开发效率对比](#9-开发效率对比)
10. [资源利用率对比](#10-资源利用率对比)
11. [业务对齐度对比](#11-业务对齐度对比)
12. [总结结论](#12-总结结论)
13. [v1.3 复核修正记录](#13-v13-复核修正记录2026-09-22)
14. [v1.4 复核保留摘要 + v1.5 修复进度](#14-v14-复核保留摘要--v15-修复进度2026-09-23-续)
15. [v1.6 本轮复核（2026-09-25）](#15-v16-本轮复核2026-09-25)
16. [v1.8 代码验证更正（2026-09-26）](#16-v18-代码验证更正2026-09-26)
17. [2026-09-27 行动清单](#17-2026-09-27-行动清单)
18. [v1.10 对照 Synapse v1.162.0（2026-10-01）](#18-v110-对照-synapse-v11620-2026-10-01当前口径)
19. [v1.11 对照 Synapse v1.163.0（2026-10-16，当前口径）](#19-v111-对照-synapse-v11630-2026-10-16当前口径)

---

> **轮次指针**（先看这张表，再决定信哪一节）：

| 轮次 | 日期 | 复核基线 | 章节 |
|------|------|----------|------|
| v1.2 生成 | 2026-09-16 | — | §1–§12（初版叙述） |
| v1.3 复核 | 2026-09-22 | `main` | §13 + §11/§12 局部改写 |
| v1.4 代码取证 | 2026-09-22 | — | §14.1–§14.2 |
| v1.5 P0 收口 | 2026-09-23 | — | §14.2–§14.6 |
| v1.6 复核 | 2026-09-25 | `opt/consolidated` @ `9e26ee31a` | §15 |
| v1.8 代码验证 | 2026-09-26 | `opt/consolidated` @ `9e26ee31a` | §16–§17 |
| v1.10 上游升级 + 差距复核 | 2026-10-01 | `opt/consolidated`；上游 Synapse `v1.162.0` | §18 |
| **v1.11 上游升级 + 现状重判** | **2026-10-16** | **`main` @ `86ee4e957`；上游 Synapse `v1.163.0`** | **§19（当前口径）** |

> **§19 是唯一"当前状态"口径**。§18 降级为上一轮口径；§1–§18 中与 §19 冲突的表述一律以 §19 为准
> （历史章节仍保留原文，其"当前口径"指针按既有门禁指向 §18.5，改指针会破坏
> `tests/unit/doc_credibility_guard_tests.rs` 的 `historical_sections_point_at_the_current_scope`，
> 故**不修改**历史指针，由本节统一覆盖）。§19.6 给出对 §18 自身口径的作废清单。

---

## 1. 项目概览

### 1.1 基础信息

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **项目来源** | element-hq/synapse (官方参考实现) | 独立重写项目 |
| **主要语言** | Python 3.10+ | Rust (Edition 2021, MSRV 1.93) |
| **当前版本** | v1.162.0 | v6.2.0 |
| **许可证** | AGPL-3.0 | AGPL-3.0-only |
| **代码规模** | 未独立测量（本报告未复核上游行数） | **447,508 行 Rust（1,030 个 .rs 文件）**（实测：`git ls-files '*.rs' \| wc -l` 与 `git ls-files -z '*.rs' \| xargs -0 wc -l \| tail -1`，2026-09-25） |
| **数据库** | PostgreSQL / SQLite | PostgreSQL (sqlx 0.8，Cargo.lock 实锁 0.8.6) |
| **缓存** | Redis (tx-redis) | Redis (deadpool-redis 0.20，Cargo.lock 实锁 0.20.0) |
| **异步运行时** | Twisted reactor | Tokio（Cargo.toml 要求 1.49，Cargo.lock 实锁 **1.53.1**） |

### 1.2 项目定位

- **Synapse (Python)**: Matrix 协议的官方参考实现，经过多年生产验证，功能最完整，但性能受限于 Python 语言特性。
- **synapse-rust**: 以 Synapse **v1.161.0**（2026-09-15）为对齐基准的 Rust 重写实现，目标是在保持协议兼容性的同时，利用 Rust 语言优势提升性能和资源效率。

---

## 2. 架构对比

### 2.1 整体架构

| 架构维度 | Synapse (Python) | synapse-rust |
|----------|-------------------|--------------|
| **进程模型** | 多进程 Worker 模式（generic_worker, federation_sender, synchrotron 等） | 单进程多模块 + Worker TCP 拓扑（可配置） |
| **Web 框架** | Twisted Web (treq/twisted.web) | Axum 0.8 + Tower 中间件栈 |
| **模块化方式** | Python 包层级（synapse.rest, synapse.storage, synapse.federation 等） | Cargo Workspace：`members` 8 个 crate + 根 crate（共 9 个编译单元） |
| **中间件** | Twisted inlineCallbacks 装饰器 | Tower 中间件（CORS, Auth, RateLimit, CSRF, FederationAuth, Security） |
| **配置管理** | YAML + Python argparse | config 0.14 (YAML) + 结构化配置 |
| **可观测性** | Prometheus client + structured logging | OpenTelemetry 0.31 + tracing-subscriber + jemalloc profiling |

### 2.2 Workspace 模块化（synapse-rust 独有优势）

synapse-rust 采用 Cargo Workspace：`[workspace] members` 声明 8 个 crate，另加根 crate（`src/`）共 9 个编译单元。下表文件数为 2026-09-25 实测（`git ls-files '<crate>/*.rs' | wc -l`）：

| Crate | 文件数 | 职责 |
|-------|--------|------|
| `synapse-common` | 73 | 公共工具、加密原语、配置、错误定义 |
| `synapse-cache` | 12 | 缓存层（Redis 连接池 + 本地缓存） |
| `synapse-storage` | 208 | 数据访问层（PostgreSQL 持久化） |
| `synapse-e2ee` | 58 | 端到端加密（Megolm, device keys, key rotation） |
| `synapse-federation` | 21 | 联邦协议（事件传输, EDU, 成员同步） |
| `synapse-services` | 206 | 业务逻辑层 |
| `synapse-web` | 162（其中 `src/routes/` 144） | HTTP 路由层（Axum + Tower 中间件） |
| `synapse-test-utils` | 2 | 测试夹具与共享测试基础设施 |
| 根 crate (`src/`) | 39 | 服务器启动、Worker 管理（根包全部 .rs 含 tests/benches 共 344） |

**优势对比**:
- synapse-rust: 编译时模块隔离，每个 crate 可独立编译测试，依赖关系明确
- Synapse: Python 包层级灵活但边界模糊，运行时才发现导入错误

### 2.3 优势与劣势

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 成熟的多进程 Worker 架构，生产验证<br>- 灵活的 Python 模块系统，快速迭代<br>- 丰富的模块化扩展点（spam checker, third-party rules 等） | - Worker 进程间通信复杂，部署门槛高<br>- Python 包边界模糊，容易产生循环依赖<br>- 单进程内 GIL 限制并行度 |
| **synapse-rust** | - Cargo Workspace 编译时依赖检查，杜绝循环依赖<br- 单进程即可利用多核，部署简单<br>- Tower 中间件链类型安全，编译时验证 | - Worker 拓扑**静态校验 + 运行时对账**均已成型（`synapse-services/src/worker/topology_validator.rs`、`synapse-services/src/worker/runtime_topology.rs`），可用 `worker.topology.strict` 拒绝启动、也可 `--validate-topology` 在部署前拿退出码<br>- **请求分流**：路由准入自 2026-10-17 起**可执行**（`worker.surface_gate`，默认关、master 不受限，见 `synapse-services/src/worker/surface_gate.rs`）；但参考栈里的 worker 进程仍只开 `/metrics`，真分流要以**根二进制**按角色起实例（见 §19.9.1 与 W-F 实施记录）<br>- 模块化扩展点不如 Python 灵活<br>- 编译时间长（464K 行 Rust 代码全量编译） |

---

## 3. 技术实现对比

### 3.1 异步并发模型

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **并发模型** | Twisted reactor (event loop) + inlineCallbacks | Tokio async/await (work-stealing scheduler) |
| **线程利用** | 单线程事件循环（GIL 限制） | 多线程 work-stealing 线程池 |
| **CPU 并行** | 需要 Worker 进程才能利用多核 | 单进程内 async + 线程池自动并行 |
| **I/O 并发** | 高（Twisted async I/O 成熟） | 高（Tokio async I/O + 零拷贝） |

### 3.2 数据库访问层

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **驱动** | psycopg2 / txpostgres | sqlx 0.8（Cargo.lock 实锁 0.8.6） |
| **连接池** | Twisted DBACP | sqlx 内置连接池 + deadpool-redis |
| **迁移** | 增量 SQL 迁移文件 | **1 个**统一 Schema 文件（`migrations/00000000_unified_schema_v12.sql`） |
| **查询安全** | 运行时检查 | ⚠️ **以运行时为主**：棘轮实测 **静态 1,192 / 动态 1,009**（`bash scripts/ci/check_sqlx_dynamic_ratio.sh`，2026-09-27；`ratio=0.458`），即 **54.2%** 调用点走 `sqlx::query!`/`query_as!` 编译期校验，动态占 **45.8%**。棘轮基线 `BASELINE_STATIC=1,193`（只许升）/ `BASELINE_DYNAMIC_PRODUCTION=275`、`BASELINE_DYNAMIC_TEST_INFRA=716`（只许降），见 `scripts/ci/sqlx_dynamic_ratio_baseline`。静态化比例仍是**待收紧的债务**。<br>⚠️ **本文档此前长期写的"static 806 / dynamic 1,396 ≈ 36.6%"是 2026-09-25 的旧口径**：经 C38/C39/C40 批量宏化后静态调用点已增至 1,192（+386），动态降至 1,009（-387） |

### 3.3 加密实现

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **Olm/Megolm** | Python binding (libolm C 库) | vodozemac >=0.10.0（Cargo.toml 实际锁定 >=0.10.0，纯 Rust 实现） |
| **密钥签名** | Python + PyNaCl | ed25519-dalek 2.0 (纯 Rust) |
| **哈希算法** | hashlib (Python stdlib) | sha2 0.10 + hmac 0.12 (Rust crates) |
| **密码存储** | bcrypt (Python binding) | Argon2 (项目规则指定) |

### 3.4 路由覆盖

synapse-rust 的 HTTP 契约以机器抽取的 **`docs/synapse-rust/ROUTE_CONTRACT.md`**（2026-10-23 生成）为准：**979 条注册路由条目**（2026-10-09 删 1 条**恒 400 的拒绝型路由** `POST /_matrix/vendor/v1/rooms/{room_id}/widgets/{widget_id}/send`，见 `docs/前缀命名空间治理方案-2026-10-08.md` §13.21.6 第 5 项；其 SDK 调用点已于 C6 第六批删除；2026-10-08 M1 在前缀治理 Phase 3 的 1,035 之上再删 1 条 —— `POST /_matrix/client/v1/rooms/create_private`，与 v3 同 handler 的版本孪生，见 [`MIXED_MODULE_PRIVATE_ROUTES_PLAN_2026-10-08.md`](audit/MIXED_MODULE_PRIVATE_ROUTES_PLAN_2026-10-08.md)）（绝对 `(method, path)`，已解析 `.nest()` 前缀并去重），涉及 **66** 个含路由注册的模块文件；人工维护的 `docs/synapse-rust/API_COVERAGE_REPORT.md` 按三种口径记为 **注册条目 979 / 唯一路径 779 / 逻辑端点 760**——与前者**同源但口径不同**（后者折叠版本前缀并把同路径多方法合并），两者不可相加。路由文件分布在 **`synapse-web/src/routes/`** 下，共 144 个 `.rs` 文件。⚠️ 计数较 2026-09-22 的 1,165 / 66 **净减 11 条**：2026-09-25 E2EE 去服务端私钥重构删除 30 条（`verification_routes` 24 条 + `e2ee` 路由组 6 条），其后 profile `{user_id}/{key_name}` 新增 5 条（3 条来自 admin_media 新增路由，2 条来自 invite 路由的 POST 方法），`auth_issuer` 删除 1 条，2026-09-27 U-5 Admin 媒体端点族补全新增 5 条（房间级媒体列举/删除/隔离/解除隔离 + 媒体保护），2026-10-01 两条对齐/RFC 批次共新增 3 条 —— MSC4140 单事件端点（L1，`GET /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`，与同路径既有 `POST` 合并计入口径）与 MSC3720 账户状态（客户端 `/_matrix/client/unstable/org.matrix.msc3720/account_status` + 联邦 `/_matrix/federation/unstable/org.matrix.msc3720/account_status`），**不是抽取器漂移**；2026-10-02 再补 spec 的 4 段关系路由（L-6）**+2**（1,152 → 1,154）——`GET` `…/relations/{eventId}/{relType}/{eventType}` 在 `v1`/`v3` 各一条，与同路径既有 `PUT` 合并进同一 `MethodRouter`，**唯一路径数不变**。随后同一批把关系写入端点按 ISSUE-13 拆到 `PUT /_matrix/vendor/v1/rooms/{room_id}/relations/{event_id}/{rel_type}/{txn_id}`（client 4 段路径只留 `GET`，`{txn_id}` 接上 `room_event_txn_dedup` 真幂等）：client 注册条目 −2、vendor +1，**净 −1**（1,154 → 1,153）；2026-10-04 补 Admin 媒体端点 `POST /_synapse/admin/v1/purge_media_cache` **+1**（1,153 → 1,154）；2026-10-05 P3-1 补全 Admin 媒体端点族 6 条（`GET`/`DELETE` `/_synapse/admin/v1/media/{server_name}/{media_id}`、`GET /_synapse/admin/v1/media/quarantine_changes`、`GET /_synapse/admin/v1/room/{room_id}/media`、`POST /_synapse/admin/v1/room/{room_id}/media/quarantine`、`POST /_synapse/admin/v1/media/protect/{media_id}`），同时移除 `admin/server.rs` 中重复注册的 `POST /_synapse/admin/v1/purge_media_cache`（改由 `admin/media.rs` 唯一注册，消除 axum `Overlapping method route` panic）：**净 +5**（1,154 → 1,159）；2026-10-08 ISSUE-13 前缀命名空间治理 Phase 2 batch 1 删除 **77 条 client 前缀死别名**（`friend_room` 36 / `voice` 18 / `burn_after_read` 14 / `key_rotation` 9 —— 全部已有 `/_matrix/vendor/v1` 孪生，按 AGENTS.md 铁律 1「未发布项目无向后兼容义务」直接删除，无 `#[deprecated]` 过渡态），**净 −77**（1,159 → **1,082**；唯一路径 921 → 858、逻辑端点 813 → 766，客户端三条口径分别 −77 / −63 / −47，admin 与其他命名空间零变化）；2026-10-08 同日前缀治理 **Phase 2 batch 2** 把 **78 条私有端点从 client 前缀迁到 `/_matrix/vendor/v1`**（`space` 44 / `widget` 18 / `room_summary` 16；`move-to-vendor`，即新增 vendor 挂载并移除 client 注册）：其中 `space` 的 44 条是 22 条路径被 `nest` 到 v1+v3 两次，迁移后收敛为 22 条，`widget` 还合并了同 handler 的重复路径 `POST /widgets/create`（−1），MSC2946 `spaces/{space_id}/hierarchy[/v1]`（4 条）与 MSC3266 `GET /rooms/{room_id}/summary`（1 条）作为**规范端点留在 client 前缀**；**净 −23**（1,082 → **1,059**；唯一路径 858 → 838、逻辑端点 766 → **766 不变** —— 版本前缀折叠后每个路径仍是 1 条逻辑端点，可作一致性自检；client 注册条目 −78、vendor +55，admin 与其他命名空间口径零变化）。2026-10-08 同日前缀治理 **C6 反冗余批次**删除 4 条 admin 面的**同 handler 重复挂载**（`GET /_synapse/admin/v1/room/{room_id}/media`、`POST /_synapse/admin/v1/room/{room_id}/media/quarantine`、`POST /_synapse/admin/v1/federation/destinations/{destination}/reset`、`POST /_synapse/admin/v1/rooms/{room_id}/delete`；四条都保留了本仓自身形态：复数 `rooms/`、`reset_connection`、`DELETE /rooms/{room_id}`），**净 −4**（1,059 → **1,047**；唯一路径 838 → 834、逻辑端点 766 → 762，admin 三条口径 295/233/231 → 291/229/227，client 与其他命名空间零变化）。2026-10-08 同日 **C6 第二批**删除 SDK `DeviceKeysManager`/`E2EEManager` 仍在用的非规范别名 `POST /_matrix/client/{v1,v3}/keys/signatures`（规范形态是 `/keys/signatures/upload`，两者同 handler；SDK 侧三条重复入口收敛为一条并改打规范路径），**净 −2**（1,055 → **1,047**；唯一路径 834 → 832、逻辑端点 762 → 761，client 三条口径 484/368/298 → 482/366/297）。2026-10-08 同日 **C6 第三批**删除 admin 面第 5 条重复挂载 `POST /_synapse/admin/v1/users/{user_id}/devices/delete` —— 它与上游形态 `.../logout` 挂同一 handler，而该 handler **不读请求体**、调 `logout_all` 撤销全部会话；SDK `AdminUserManager.deleteUserDevices(userId, deviceIds)` 正打这条并按「只删列出的设备」对外承诺 ⇒ 实为**假能力（静默升级为全量登出）**，同批删除后端别名 + SDK 方法 + Tjg 门面链（无 UI 消费者；按设备删除请用 `deleteUserDevice` 的 `DELETE .../devices/{device_id}`，整体登出请用 `logoutUser` 的 `.../logout`）。**净 −1**（1,053 → **1,047**；唯一路径 832 → 831、逻辑端点 761 → 760，admin 三条口径 291/229/227 → 290/228/226）。2026-10-08 同日 **C6 第四批**删除 `cas.rs` 里自我声明的 `legacy_admin_routes`（根级 `/admin/services`、`/admin/services/{service_id}`、`/admin/users/{user_id}/attributes` 共 **5** 条注册）与其专用的 `legacy_cas_admin_alias_deprecation_middleware`（该中间件只服务这个 router，正文自述 "Legacy CAS admin aliases under /admin are deprecated"）⇒ **净 −5**（1,052 → **1,047**；唯一路径 831 → 828、逻辑端点 760 → 757，「其他命名空间」三条口径 280/237/237 → **275/234/234** —— 因为它们挂在**根级**、不属于 client 也不属于 `/_synapse/admin`）。2026-10-08 同日 **前缀死别名 + 非规范命名空间收敛**（本批后端侧）：删除 **6 条 client 前缀死别名**（`GET /_matrix/client/v1/external_services/health`、`PUT`/`DELETE /_matrix/client/v1/external_services/{service_id}`、`GET /_matrix/client/v3/my_rooms`、`POST /_matrix/client/v3/search_rooms`、`POST /_matrix/client/v3/search_recipients`）—— 每一条都在 `/_matrix/vendor/v1` 下有**同 method 孪生**且挂**同一 handler**，属死别名；同时删除 **5 条 `/_matrix/admin/v1/external_services*`**（`_matrix/admin` 不在 Matrix 规范枚举的命名空间内，且这 5 条与 `/_synapse/admin/v1/external_services*` **同 handler**、是其**严格子集**，缺 `/{as_id}/health` 与 `/{as_id}/health/check`）⇒ **净 −11**（1,047 → **1,036**；唯一路径 828 → **820**，逻辑端点未复算；client 注册条目 −6、「其他命名空间」−5，admin 三条口径零变化）。同批新增守卫 `check_client_prefix_vendor_twins`（判据：client 前缀 ∩ vendor 孪生 ⇒ 必须删除或在 `MSC_KEEP` 登记）与 `mutation#9`，并修复了两份集成快照的漏刷（各 −12 条，见 `docs/后端冗余清除与功能完善优化方案-2026-10-08.md` §2.0）。2026-10-08 同日 **前缀命名空间治理 Phase 3（第三批）** 把最后 **29 条私有端点从 client 前缀迁到 `/_matrix/vendor/v1`**（`handlers/thread.rs` 14 / `dm.rs` 5 / `push_notification.rs` 4 / `moderation.rs` 3 / `room.rs` 3），**不留 client 别名**（AGENTS.md 铁律 1）。其中 `handlers/thread.rs` 是**混合模块**：23 条里 MSC3856 规范的 9 条（房间线程列表/详情、回复、订阅、解冻）留在 client 前缀；`moderation.rs` 的 `PUT .../report/{event_id}/score` 原先在 `v1`/`v3` 各挂一份、迁到 vendor 后合为**一条**（vendor 只有一个版本）⇒ **净 −1**（1,036 → **1,035**）。⚠️ 唯一路径 820 → **821**、逻辑端点 749 → **753** 这两个口径**反而上升**，原因是 vendor 路径落入「其他命名空间」桶（client 桶 476/361/290 → **447/336/268**，其他命名空间 270/231/231 → **298/257/257**）⇒ **判断私有面是否收敛必须看 client 桶，不能看总数**。同批台账 34 → **5** 条、`LEDGER_CEILING` 14 → **5**。2026-10-09 同日 **第十八轮：SDK 封装对账的「28 条真缺口」逐条回源 → 删 3 条重复别名**（`MSC4155` 线程列表桩、`MSC4156` 线程订阅桩、`GET /_matrix/vendor/v1/friends/request/received`）。前两条是 v1/vendor 主路径已封装的 unstable 兼容桩（后端 `handlers/thread.rs` 自述「仅为已发布客户端的向后兼容」），第三条是后端自打 `deprecation` + `link: …incoming; rel="successor-version"` + `sunset` 头的 predecessor 死别名（规范路径 `friends/requests/incoming` 已封装）；三条经 SDK/Tjg 实测**零消费者**，按 AGENTS.md 铁律 1 直接删除，无过渡态。**净 −3**（1,030 → **1,027**；唯一路径 817 → **814**、逻辑端点 749 → **747**，client 注册条目 −2 / 其他命名空间 −1，admin 零变化）。同批清掉 `MSC_KEEP`、`ledger_annotations.txt`、`extract_registered.py` 人工注解与 `mixed_module_client_routes.txt` 的对应登记，并重刷两条 fixture lane 与两份 ledger snapshot。2026-10-14 实现 **MSC4140 专用调度端点** `PUT /_matrix/client/unstable/org.matrix.msc4140/rooms/{room_id}/delayed_event/{event_type}/{txn_id}`（上游 changelog **19354**，对应 `docs/audit/UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md` §11.7 的原方案），**净 +1**（1,027 → **1,028**；模块数与唯一路径数不变），同批重刷三张派生表、两条 fixture lane（6 份 JSON + `manifest.json`）、两份 ledger 集成快照、`docs/openapi/route-table.json`（948 → **949**，golden × default 坐标）与 `docs/synapse-rust/ROUTE_CONTRACT.md`。2026-10-14 补齐 **MSC4140 的另外 4 条管理/列表端点**（上游 `synapse/rest/client/delayed_events.py` release-v1.161 的 `CancelDelayedEventServlet` / `RestartDelayedEventServlet` / `SendDelayedEventServlet` / `DelayedEventsServlet`：`POST …/delayed_events/{delay_id}/{cancel,restart,send}` 与 `GET …/delayed_events`），**净 +4**（1,028 → **1,032**；模块数与唯一路径数不变），同批重刷三张派生表、两条 fixture lane 与两份 ledger 集成快照、`route-table.json`（949 → **953**）与 `ROUTE_CONTRACT.md`。2026-10-15 把 **MSC3814 `dehydrated_device` 族**（6 条路由）从 `assembly::create_router` 直挂改为独立 sub-router（`handlers/dehydrated_device.rs::create_dehydrated_device_router`，见 `scripts/contract/ledger_origins.txt`），使 ledger 的 `registered_by` 成为独立 key ⇒ **路由总数 1,032 不变**（同一批 `.route()` 站点仅换挂载点），但**含路由注册的模块文件 65 → 66**（新增一个 `registered_by` 桶，SDK codegen 因而获得独立契约表并纳入 manager-codegen 管辖）；同批重刷三张派生表、两条 fixture lane 与两份 ledger 集成快照、`route-table.json`（953 不变）与 `ROUTE_CONTRACT.md`。2026-10-16 补 **Q1 per-user 邀请名单端点 4 条**（`POST|DELETE /_synapse/admin/v1/invite/{blocklist,allowlist}/{user_id}`：per-user 幂等增删 + `reason`/`created_by` 审计列；决策与实测见 `docs/audit/INVITE_PERUSER_POLICY_PROPOSAL_2026-10-16.md`），**净 +4**（1,032 → **1,036**；含路由注册的模块文件数不变 —— 4 条挂在既有的 `admin::server` 桶下；同批重刷三张派生表、两条 fixture lane、两份 ledger 集成快照、`route-table.json`（953 → **957**）与 `ROUTE_CONTRACT.md`）。2026-10-17 补 **MSC4512 App Service 联邦代理桥接 2 条**（`POST /_matrix/client/v1/appservice/fed_proxy` + unstable 孪生 `POST /_matrix/client/unstable/io.element.msc4512/appservice/fed_proxy`，见 §19.5.1 C1），**净 +2**（1,036 → **1,038**；含路由注册的模块文件数不变 —— 2 条挂在既有的 `app_service` 桶下；同批重刷三张派生表、两条 fixture lane（6 份 JSON + `manifest.json`）、两份 ledger 集成快照、`route-table.json`（957 → **959**）与 `ROUTE_CONTRACT.md`）。2026-10-18 交付 **B-9 一次性邀请 2 条**（`POST /_matrix/vendor/v1/invites/one-time` 签发 + `POST /_matrix/vendor/v1/invites/claim` 认领，均为 vendor 扩展，见 `synapse-web/src/routes/one_time_invite.rs`；claim 只接受 POST，避免链接预览/爬虫触发 GET 提前烧掉 token）与 **B-5 房间级 retention PUT 1 条**（`PUT /_matrix/vendor/v1/rooms/{room_id}/retention`，挂在既有的 `room` 桶下），**净 +3**（1,038 → **1,041**；含路由注册的模块文件数 66 不变 —— 2 条挂既有的 `vendor` 桶、1 条挂既有的 `room` 桶；同批重刷三张派生表、两条 fixture lane（6 份 JSON + `manifest.json`）、两份 ledger 集成快照、`route-table.json`（959 → **962**）与 `ROUTE_CONTRACT.md`）。2026-10-19 按 SDK 后端问题台账补齐 **5 条管理/登录端点**（A-2 `GET /_synapse/admin/v1/users/{user_id}/shadow_ban`、A-3 `DELETE /_synapse/admin/v1/feature_flags/{flag_key}`、A-4 `POST /_synapse/admin/v1/join/{room_id_or_alias}`、A-5 `POST /_matrix/client/v1/login/get_token`、admin 用户查询 `GET /_synapse/admin/v1/users/{user_id}/admin`），**净 +5**（1,041 → **1,046**；含路由注册的模块文件数 66 不变 —— 5 条全部挂在既有桶下；同批重刷三张派生表、两条 fixture lane（6 份 JSON + `manifest.json`）、`route-table.json`（962 → **967**）与 `ROUTE_CONTRACT.md`）。2026-10-20 按 SDK 后端问题台账补齐 **2 条**（B-31 voice 媒体删除 `DELETE /_matrix/vendor/v1/voice/{media_id}`、B-32 注销 legacy 别名 `POST /_synapse/admin/v1/deactivate/{user_id}`），**净 +2**（1,046 → **1,048**；含路由注册的模块文件数 66 不变 —— 2 条挂在既有桶下；同批重刷三张派生表、两条 fixture lane（6 份 JSON + `manifest.json`）、`route-table.json`（967 → **968**，golden × default 坐标只含后者）与 `route-table-sdk.json`（**1048**，sdk × all-extensions 坐标））。

> **2026-10-22（A3 第一批：`client/v1` 双注册退役）**：路由条目 **1,048 → 997（净 −51）**。
> `key_backup.rs` / `e2ee/keys.rs` / `relations.rs` 三处都是"同一个 router 挂 v1 与 v3"，
> 删掉 v1 那一半对 v3 调用方零影响；涉及 `room_keys/*` 36 条、`keys/*` 8 条、`rooms/*` 5 条、
> `sendToDevice/*` 2 条。这 34 条去重路径**全部有同 handler 的 v3 孪生**，且与"无孪生"名单
> （规范指定 v1 的 MSC3266/3856/3916/4108/2965 + 私有扩展共 31 条）**交集为空**。
> 同批重刷三张派生表、两条 fixture lane、两份 ledger 集成快照、`route-table{,-sdk}.json`、
> `API_COVERAGE_REPORT.md` 的 5 个生成块与 `ROUTE_CONTRACT.md`；退役面棘轮由 `112/79` 收紧到
> `61/45`（判据见 `docs/audit/A3_v1_deprecation_strategy.md`）。
>
> **2026-10-22（A3 第二批：A 族清零）**：路由条目 **997 → 975（净 −22）**。
> 删的是 `client/v1` 上最后一批"历史双注册"：`account/*` + `profile/*`（18 条，
> `create_account_compat_router()` 原样挂在 v3）、`rooms/{id}/context`（1 条）、
> `rooms/{id}/report/{event_id}`（1 条）、`state/m.room.power_levels/` 的 GET/PUT（2 条）。
> 同批新增 `scripts/contract/v1_spec_keep_baseline.txt`：**8 条规范指定 v1 的端点
> （MSC2946 hierarchy ×3、MSC3856 threads/{id}、v1/v3 presence ×3、v1.11 media/config）
> 虽然另有 v3 别名，v1 必须保留** —— 此前"有孪生即可删"的推断会把它们一起扫掉。
> 终态：`client/v1` 39 条 = 规范正典 28 + 私有扩展 11，可退队列 **0**。
>
> **2026-10-23（A3 第四批：relations v1 恢复）**：路由条目 **975 → 979（净 +4）**。
> 第一批按「有 v3 孪生即可删」把 `relations.rs` 的 v1 挂载整体退掉，但 matrix-spec
> `relations.yaml` 的 `servers[0].variables.basePath.default = /_matrix/client/v1` ——
> 三个 `/relations` 读端点（`GET /rooms/{roomId}/relations/{eventId}[/{relType}[/{eventType}]]`）
> 属**规范指定 v1**（B 族），v3 只是本仓别名；本批恢复 `/_matrix/client/v1` nest（v3 保留）。
> 因此「有 v3 孪生」不足以构成删除依据，必须以规范 `basePath.default` 为准（判据已写入
> `docs/audit/A3_v1_deprecation_strategy.md`）。同批重刷三张派生表、两条 fixture lane、
> 两份 ledger 集成快照、`route-table{,-sdk}.json`、`API_COVERAGE_REPORT.md` 的 5 个生成块
> 与 `ROUTE_CONTRACT.md`（client 桶注册条目 329 → 333、唯一路径 242 → 246）。

> ⚠️ 上一版本此处写"**656 个 API 端点，覆盖 48 个功能模块**，来源为项目 API 参考文档"。本轮复核确认：仓库内**不存在** `docs/synapse-rust/api-reference.md`，该数字无法在仓库中定位来源，且与上述两份权威清单均不一致，已删除。引用端点数量时请以 `ROUTE_CONTRACT.md` 为准。

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 协议实现最完整，所有 MSC 均已落地<br>- libolm 经过多年安全审计<br>- 增量迁移支持平滑升级 | - Python binding 存在 FFI 开销<br>- 迁移文件过多，维护成本高<br>- 运行时 SQL 错误，难以提前发现 |
| **synapse-rust** | - sqlx 提供编译期 SQL 校验能力（当前 静态 1,192 / 动态 1,009，即 54.2% 静态；棘轮单向收紧中）<br>- vodozemac 纯 Rust 实现，无 FFI 开销<br>- 统一 Schema 简化迁移管理<br>- 类型安全的路由定义（Axum macros） | - vodozemac 相对 libolm 生态成熟度较低<br>- 统一 Schema 对增量升级不友好<br>- **45.8% 的 SQL 调用点未走编译期校验** |

---

## 4. 性能指标对比

### 4.1 语言层面性能

| 指标 | Python | Rust | 倍数 |
|------|--------|------|------|
| **CPU 密集计算** | ~6.7s (1 亿次循环) | ~0.34s (1 亿次循环) | ~20x |
| **内存基线** | ~80-300MB (运行时 + 依赖) | ~5-15MB (二进制) | ~15-20x |
| **启动时间** | 2-5 秒 | 毫秒级 | ~100x |
| **GC 停顿** | 有（分代 GC，p99 抖动） | 无（所有权模型，零 GC） | - |
| **多核利用** | 1 核（GIL 限制） | 全部核心 | N 核倍数 |

> **数据来源**: Rust vs Python 性能基准测试（2026 年），以及 synapse-rust 项目内置 benchmark 数据。

### 4.2 服务器层面性能

| 指标 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **空闲内存** | 300-500 MB | 预期 50-100 MB（jemalloc 精细化分配） |
| **活跃负载内存** | 2-4 GB（联邦房间 + 活跃用户） | 预期 200-500 MB（无 GC 开销，零拷贝） |
| **峰值内存** | 6+ GB（大型联邦房间，如 #matrix:matrix.org） | jemalloc profiling 支持（MALLOC_CONF 配置） |
| **p99 延迟** | 波动大（GC 停顿 + GIL 竞争） | 稳定可预测（无 GC，work-stealing 调度） |
| **编译优化** | 无 JIT（CPython 解释执行） | LTO + codegen-units=1 + panic=abort |

### 4.3 Benchmark 基础设施

synapse-rust 在 `Cargo.toml` 中声明了 5 个 criterion 基准测试套件（`grep -c '\[\[bench\]\]' Cargo.toml`）：

1. `performance_api_benchmarks.rs` — API 端点吞吐量
2. `performance_federation_benchmarks.rs` — 联邦事务处理
3. `performance_sliding_sync_benchmarks.rs` — 滑动同步性能（`required-features = ["test-utils"]`）
4. `performance_membership_benchmarks.rs` — 成员状态操作
5. `performance_pagination_benchmarks.rs` — 分页查询（v1.2 遗漏）

以及 `tests/performance/` 目录下的负载测试和冒烟测试。

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 生产环境大量真实数据验证<br>- 性能瓶颈已有社区分析和优化方案 | - GIL 是结构性限制，无法通过优化消除<br>- GC 停顿导致 p99 不稳定<br>- 内存利用率低（Python 对象模型开销） |
| **synapse-rust** | - 无 GIL，多核并行处理<br>- 无 GC，p99 延迟稳定可预测<br>- jemalloc + profiling 精细化内存管理<br>- 内置 benchmark 基础设施完善 | - 缺乏生产环境大规模验证数据<br>- 实际性能数据待实测确认<br>- 编译时间较长影响迭代速度 |

---

## 5. 可扩展性对比

### 5.1 水平扩展

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **Worker 模型** | 多进程 Worker（generic_worker, federation_sender 等），通过 Redis 共享状态 | TCP Worker 拓扑（`worker/tcp.rs`, `worker/manager.rs`, `worker/load_balancer.rs`） |
| **Worker 发现** | 静态配置 | 健康检查（运行时，心跳驱动）+ 拓扑**静态**校验；**运行时对账（声明 vs 注册）未做** ⇒ 见 [docs/audit/WORKER_TOPOLOGY_VERIFICATION_AUDIT_2026-10-16.md](docs/audit/WORKER_TOPOLOGY_VERIFICATION_AUDIT_2026-10-16.md) |
| **负载均衡** | 手动配置 Worker 类型分流 | 内置 `load_balancer` 模块，已接进生产装配（`synapse-services/src/wiring/admin.rs` 的 `apply_load_balancing`），**默认关闭**（`worker.load_balancing.enabled: false`）；开启后按策略在健康 worker 间选主，关闭时行为与改造前逐字节相同 |
| **Redis 共享** | tx-redis（Twisted binding） | deadpool-redis 0.20（连接池 + tokio-rustls） |

### 5.2 垂直扩展

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **多核利用** | 需要 Worker 进程，每进程 1 核 | 单进程 async + 线程池自动利用全部核心 |
| **内存扩展** | Python 对象开销大，内存利用率低 | Rust 零开销抽象，内存利用率高 |
| **连接池** | Twisted DBACP（进程内） | sqlx 连接池 + deadpool-redis（进程内 + 跨 Worker） |

### 5.3 功能可扩展性

synapse-rust 通过 Feature Flag 控制模块编译：

```toml
[features]
# 实际取值（Cargo.toml）：`server` 特性已于 H-6 删除（全仓零 gate，属死特性）
default = ["core-private-chat", "widgets", "external-services", "beacons"]
core-private-chat = ["friends", "burn-after-read", "synapse-web/core-private-chat"]
friends = ["synapse-services/friends", "synapse-web/friends"]
saml-sso = ["synapse-services/saml-sso", "synapse-web/saml-sso"]
widgets = ["synapse-services/widgets", "synapse-web/widgets"]
burn-after-read = ["synapse-services/burn-after-read", "synapse-web/burn-after-read"]
# ... 共 12 个可选扩展模块；另有元特性 all-extensions 一次开启全部
```

> ⚠️ 上一版本片段写作 `default = ["server", "core-private-chat", ...]`，其中 `server` 已不存在；且实际特性同时向 `synapse-services` 与 `synapse-web` 两个 crate 转发，不只是 `synapse-services`。

这允许编译最小化二进制文件，适应不同部署场景。

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - Worker 模式成熟，大规模部署验证<br>- 模块系统（Pluggable Modules）灵活扩展<br>- 社区有大量运维经验 | - Worker 部署复杂，需要额外进程管理<br>- 每进程 GIL 限制，垂直扩展效率低<br>- 配置繁琐 |
| **synapse-rust** | - 单进程即可利用多核，部署简单<br>- Feature Flag 控制编译，最小化二进制<br>- 内置负载均衡和健康检查<br>- TCP Worker 拓扑可动态扩展 | - Worker 控制面（注册 / 心跳 / 任务 / 命令 / 复制位点 / 队列消费）已可跑、可对账、可告警；**但 HTTP 请求分流未做**（worker 进程不提供 Matrix API）⇒ 水平扩展对请求面仍不成立<br>- 缺乏大规模生产验证 |

---

## 6. 可维护性对比

### 6.1 代码质量保障

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **类型系统** | mypy (可选，非强制) | Rust 类型系统（编译时强制） |
| **Linter** | flake8 + black + isort | Clippy（workspace 级别强制） |
| **Lint 规则** | 风格检查为主 | `unwrap_used = "deny"`, `redundant_clone = "deny"`, `unused_async = "deny"` 等 |
| **错误处理** | 异常（运行时检查） | thiserror 2.0 + Result 类型（编译时检查） |
| **测试框架** | pytest + Twisted trial | 内置 test harness（unit, integration, e2e, performance） |

### 6.2 测试覆盖

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **测试文件数** | ~500+ 测试文件 | 300 个文件（`git ls-files tests \| wc -l`，其中 `.rs` 238 个；另有各 crate 内联 `#[cfg(test)]` 模块） |
| **测试分层** | Unit + Integration + System tests | 4 层：`tests/unit`(111 文件) + `tests/integration`(116) + `tests/e2e`(3) + `tests/performance`(5) |
| **Mock 方式** | mock + patch | mockall 0.13 + wiremock 0.6 + insta 快照（Cargo.lock 实锁 1.48.0） |
| **属性测试** | 无 | quickcheck 1.1（仅用于 `synapse-common/src/validation.rs` 的输入校验，覆盖面有限） |
| **基准测试** | 基本无 | criterion 0.5（5 个 benchmark 套件） |
| **快照测试** | 无 | insta 1.41+（JSON 响应形状锁定） |
| **测试工具** | 共享 fixtures | `synapse-test-utils` 独立 crate（当前仅 2 个 .rs 文件） |

### 6.3 文档与配置

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **文档文件数** | 官方文档站 (matrix-org.github.io) | 203 个文件（`git ls-files docs \| wc -l`，其中 `.md` 167 个） |
| **API 参考** | 在线文档 | `docs/synapse-rust/ROUTE_CONTRACT.md`（979 条注册路由 / 66 个模块，机器抽取）+ ledger 导出契约；⚠️ 此前引用的 `docs/synapse-rust/api-reference.md` **不存在** |
| **Docker 配置** | docker-compose 示例 | 58 个文件位于 `docker/`（`git ls-files docker \| wc -l`，另有 3 个 `Dockerfile`：`docker/`、`docker/complement/`、`docker/deploy/alert-handler/`） |
| **数据库标准** | 无统一标准 | `DATABASE_FIELD_STANDARDS.md` 字段命名规范 |

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 官方文档站完善<br>- 社区 Wiki 和 FAQ 丰富<br>- 多年积累的最佳实践 | - mypy 非强制，类型安全不完整<br>- 测试以 mock 为主，集成测试不足<br>- 代码风格统一但运行时错误仍多 |
| **synapse-rust** | - 编译时类型安全 + Clippy 强制 Lint<br>- 4 层测试体系 + 属性测试 + 快照测试<br>- 统一数据库字段标准<br>- 文件化 API 参考覆盖全部端点 | - 文档规模大但质量参差不齐<br>- 测试覆盖虽广但真实联调测试不足<br>- 编译时间长影响开发迭代 |

---

## 7. 安全特性对比

### 7.1 认证与授权

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **密码哈希** | bcrypt | Argon2（可配置成本参数） |
| **Token 管理** | Access/Refresh Token | Access/Refresh Token + JWT (HS256) |
| **管理员注册** | 共享密钥 | HMAC-SHA256 签名验证 |
| **SSO** | SAML + OIDC + CAS | SAML + OIDC + CAS (Feature Flag 控制) |
| **速率限制** | 有（per-endpoint） | RateLimit 中间件 + Federation RateLimit |

### 7.2 加密安全

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **E2EE 引擎** | libolm (C 库 binding) | vodozemac `>=0.10.0`（Cargo.lock 实锁 **0.11.0**，纯 Rust；Megolm/Olm 走 `GroupSession`/`InboundGroupSession`/`Account`/`Session` + 加密 pickle） |
| **密钥轮转** | 有 | `synapse-e2ee/src/key_rotation/`（1017 行）+ `synapse-federation/src/key_rotation.rs`（1157 行） |
| **跨设备验证** | 有（成熟） | ✅ **规范形态（2026-09-25 去服务端私钥重构）**：服务端**不再参与 SAS 密码学、不再替客户端宣布"已验证"**。原两条违规面已**整模块删除** —— ① `synapse-web/src/routes/verification_routes.rs`（12 条私有 `/_matrix/client/{v1,v3}` 路由：`/keys/device_signing/verify_{start,accept,key_agreement,mac,done}`、`/keys/device_signing/requests`、`/keys/qr_code/{show,scan}`）连同 `synapse-e2ee/src/verification/`（`accept_sas` 在服务端生成 X25519 私钥、`generate_sas` 代算 ECDH 与 SAS、`confirm_sas` 用服务端私钥判 MAC 后置 `Done`）；② `e2ee/devices.rs` 的 `device_verification/{request,respond,status}`·`device_trust`·`security/summary` 6 个 handler 连同 `synapse-e2ee/src/device_trust/`（服务端生成密钥对，且由同一 user 的任意客户端审批后即置 `DeviceTrustLevel::Verified`）。设备验证回归规范流程：客户端经 `PUT /sendToDevice/m.key.verification.*` 互发事件、从 `/sync` 的 `to_device.events` 取走（`tests/integration/api_verification_relay_tests.rs` 以"事件原样中继 + 被删端点返回 404"锁定）；交叉签名仍由 `/keys/signatures/upload`、`/keys/device_signing/upload` 与 `e2ee/cross_signing/` 承载，只读报告 `CrossSigningVerificationService` 的 `is_verified` 已**只**从交叉签名推导。客户端侧 SAS 密码学由 `matrix-sdk-crypto-wasm` 承担（本次仅服务端，客户端接线为独立任务） |
| **密钥备份** | 有 | `synapse-e2ee/src/backup/` + `synapse-web/src/routes/e2ee/backup.rs`（`synapse-common/src/secure_backup` 摘要派生另有实现，`ssss/service.rs:250` 的 curve25519 路径从密文自身派生 AES 密钥，非 ECDH） |
| **SSSS** | 有 | ✅ **已对齐规范（2026-09-23 修复，commit `a2375743`）**：`e2ee/ssss/service.rs` 现按 matrix-spec v1.19 `m.secret_storage.v1.aes-hmac-sha2` 实现——HKDF-SHA256（salt = 32 个 0 字节，输出 64 字节拆 AES key/MAC key；密钥校验 info 为空串、加密 info 为 secret name）、**AES-256-CTR**（128 位大端计数器）+ **encrypt-then-MAC**（HMAC-SHA-256 over 密文）、16 字节 IV 且 **bit 63 清零**、`iv/ciphertext/mac` 一律无填充 base64；新增 `decrypt_secret` 先验 MAC 再解密。MSC2697 `curve25519-aes-sha2`（从密文自身派生 AES 密钥、公钥取密文前 32 字节）**已删除**，`create_key`/`encrypt_secret` 对其 fail-closed 400。测试 27 项含 **NIST SP 800-38A F.5.5 CTR-AES256 已知向量**、篡改密文 → 403、错误 secret name → 403、非 32 字节密钥拒绝 |
| **泄漏检测** | 有 | ❌ **能力不存在**：`synapse-e2ee/src/leak_detection/` 目录**已删除**（`glob 'synapse-e2ee/src/leak_detection/**'` 无结果），仓库内无替代实现 |
| **内存安全** | Python 管理但 binding 可能有漏洞 | Rust 所有权模型 + `zeroize`（当前仅 `synapse-e2ee` 依赖） |

### 7.3 网络安全

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **CORS** | Twisted CORS middleware | tower-http CORS 中间件 |
| **CSRF** | 有 | 专用 CSRF 中间件 (`middleware/csrf.rs`) |
| **IP 黑名单** | 有 | 有（`middleware/security.rs`） |
| **联邦认证** | 有 | `middleware/federation_auth.rs` + 签名验证 |
| **联邦速率限制** | 有 | `middleware/federation_rate_limit.rs` 独立中间件 |

### 7.4 安全审计

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **审计历史** | 多年安全审计，CVE 记录完整 | 新项目，审计历史有限 |
| **依赖安全** | pip-audit / safety | cargo-audit + cargo-deny + RUSTSEC 跟踪 |
| **内存安全** | C binding 可能有内存漏洞 | Rust 内存安全（编译时保证）+ `zeroize` 清理敏感数据 |
| **漏洞修复** | 成熟的 CVE 响应流程 | 主动替换不安全依赖（如 paste → pastey fork） |

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 安全审计历史长，CVE 记录完善<br>- libolm 经过专业密码学审计<br>- 生产环境安全事件响应经验丰富 | - C binding 可能引入内存安全漏洞<br>- bcrypt 不如 Argon2 抗 GPU/ASIC 破解<br>- Python 运行时类型安全问题 |
| **synapse-rust** | - Rust 编译时内存安全保证<br>- Argon2 密码哈希（抗 GPU/ASIC）<br>- 主动跟踪 RUSTSEC 并替换不安全依赖<br>- `zeroize` 清理敏感数据<br>- E2EE 跨设备验证走**规范形态**（交叉签名 + 密钥备份由服务端承载；SAS/QR 密码学回归客户端 `m.key.verification.*` to-device，服务端只中继 —— 2026-09-25 删除服务端 SAS/设备信任面） | - vodozemac 审计历史短于 libolm（但已升级至 >=0.10.0，Soatok 2026-02 DH 贡献性问题已修复） |

---

## 8. 用户体验对比

### 8.1 服务器端用户体验（API 延迟）

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **同步延迟** | 大型房间 sync 可达数秒 | 预期亚秒级（无 GC 停顿） |
| **消息发送** | ~50-200ms（含 DB 写入） | 预期 ~10-50ms |
| **联邦延迟** | 受 Python 序列化影响 | reqwest 0.12 + 零拷贝序列化 |
| **Sliding Sync** | MSC3886 实现存在 | 独立 `sliding_sync_service` 模块 + benchmark |

### 8.2 部署体验

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **部署方式** | pip install + YAML 配置 | 单二进制 + YAML 配置 |
| **Docker 镜像** | ~400-600MB (Python + deps) | ~50-100MB (静态链接二进制) |
| **启动速度** | 2-5 秒 | 毫秒级 |
| **配置复杂度** | 高（Worker 配置 + 主进程配置） | 低（单进程 + 可选 Worker） |
| **依赖管理** | pip + poetry (虚拟环境) | Cargo (lockfile + workspace) |

### 8.3 运维体验

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **日志** | structured logging | tracing-subscriber (env-filter, JSON, fmt) |
| **指标** | Prometheus client | OpenTelemetry 0.31 (metrics + logs) + tracing-opentelemetry |
| **内存诊断** | 有限（Python tracemalloc） | jemalloc profiling（MALLOC_CONF 控制，jeprof 符号化） |
| **健康检查** | /health 基本检查 | 健康检查 + schema_health_check 独立工具 |

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 运维经验积累丰富，社区文档完善<br>- Worker 模式可精细控制资源分配<br>- 故障排查有成熟方法论 | - 部署复杂度高（多进程管理）<br>- Docker 镜像体积大<br>- 启动慢影响滚动更新 |
| **synapse-rust** | - 单二进制部署简单<br>- Docker 镜像体积小<br>- 毫秒级启动，快速滚动更新<br>- jemalloc profiling 精细化内存诊断<br>- OpenTelemetry 原生集成 | - 运维经验积累不足<br>- 故障排查方法论待完善<br>- 缺乏社区运维文档 |

---

## 9. 开发效率对比

### 9.1 开发体验

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **语言学习曲线** | 低（Python 易上手） | 高（Rust 所有权 + 生命周期） |
| **编译反馈** | 无编译步骤（解释执行） | 编译时全面检查（类型 + 借用 + Clippy） |
| **迭代速度** | 快（修改即运行） | 慢（全量编译 + LTO） |
| **IDE 支持** | 完善（PyCharm, VSCode + Pylance） | 良好（rust-analyzer, 但大型项目较慢） |
| **错误定位** | 运行时堆栈追踪 | 编译时错误 + 运行时 panic 回溯 |

### 9.2 编译与构建

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **构建时间** | 秒级（无编译） | 分钟级（8 crate workspace + 编译时 SQL 验证 + LTO） |
| **Profile** | 无 | dev (opt-level=1) / test (opt-level=0) / release (LTO) / bench |
| **交叉编译** | 不需要 | ⚠️ **不成立**：`rust-toolchain.toml` 只声明 `x86_64-unknown-linux-gnu`，全仓无 `musl` 目标；`docker/Dockerfile` 使用 `distroless/cc-debian12` 并**显式抽取缺失的动态库**（`runtime-libs` 阶段），即产物是 glibc **动态链接**，不是静态链接 |
| **增量编译** | 不需要 | 支持（incremental=true for dev/test） |

### 9.3 代码组织

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **路由文件数** | ~60 REST servlet 文件 | 144 个路由文件（`synapse-web/src/routes/`） |
| **服务文件数** | ~80 handler 文件 | 206 个服务文件（`git ls-files 'synapse-services/*.rs'`，口径同 §2.2） |
| **存储文件数** | ~50 store 文件 | 208 个存储文件（`synapse-storage/src/`） |
| **依赖注入** | Python dictionary-based HomeServer | Rust trait-based wiring（`wiring/` 模块） |

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - Python 易上手，开发速度快<br>- 无编译等待，即时反馈<br>- 社区贡献门槛低 | - 运行时错误多，调试成本高<br>- 类型安全不完整（mypy 非强制）<br>- 重构风险高（缺乏编译时保障） |
| **synapse-rust** | - 编译时全面保障，运行时错误少<br>- 重构安全（编译器保障）<br>- Clippy 强制代码质量 | - Rust 学习曲线陡峭<br>- 编译时间长影响迭代<br>- 社区贡献门槛高 |

---

## 10. 资源利用率对比

### 10.1 内存利用率

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **空闲基线** | 300-500 MB | ~5-15 MB（二进制本身） |
| **运行时基线** | 300-500 MB（Python 运行时 + 依赖） | ~50-100 MB（预估，jemalloc 分配） |
| **活跃负载** | 2-4 GB | ~200-500 MB（预估） |
| **内存分配器** | Python pymalloc | tikv-jemallocator 0.6（profiling feature） |
| **内存泄漏排查** | 困难（Python GC 管理） | MALLOC_CONF + jeprof 符号化 |
| **对象开销** | ~232 bytes/对象（Python 对象模型） | ~48 bytes/对象（Rust struct，零开销抽象） |

### 10.2 CPU 利用率

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **核心利用** | 1 核/进程（GIL） | 全部核心（Tokio work-stealing） |
| **上下文切换** | 频繁（Twisted reactor + 线程切换） | 少（async task 调度） |
| **序列化开销** | json.dumps（Python 实现） | serde_json（SIMD 优化） |
| **正则匹配** | Python re 模块 | regex 1.10（Rust 原生） |

### 10.3 磁盘与网络

| 维度 | Synapse (Python) | synapse-rust |
|------|-------------------|--------------|
| **Docker 镜像** | ~400-600 MB | 预期 ~50-100 MB（⚠️ 本轮未实测：无可用镜像产物） |
| **二进制大小** | N/A（解释执行） | 未实测（`release` 为 LTO + `strip=false` + `panic=abort`，保留行号表供 jeprof 符号化） |
| **链接方式** | N/A | glibc **动态链接**（`distroless/cc-debian12` + 抽取 libssl3 等动态库），**非静态链接** |
| **网络序列化** | Python json | serde_json + bytes（零拷贝） |
| **HTTP 压缩** | 无 | tower-http compression-gzip |

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 资源使用模式已被充分理解<br>- 可通过 Worker 分配资源配额 | - 内存利用率极低（Python 对象开销）<br>- CPU 利用不充分（GIL）<br>- Docker 镜像体积大 |
| **synapse-rust** | - 内存利用率高（5-10x 节省）<br>- CPU 全核利用<br>- Docker 镜像极小<br>- jemalloc 精细化内存管理<br>- 零拷贝网络序列化 | - 实际资源数据待大规模验证<br>- jemalloc profiling 增加少量开销 |

---

## 11. 业务对齐度对比

### 11.1 Matrix 协议覆盖

> **对齐基准**: Synapse v1.162.0（2026-09-29 发布，`release-v1.162` CHANGES.md；v1.162.0 = rc1 + 无重大变更）。v1.161→v1.162 增量逐条判定见 §18。
>
> ⚠️ **证据口径（v1.3 起）**：本表每一行的状态必须有 `路径:行号` 或可复现命令支撑；无法证实的标 `[未验证]`。
> MSC 编号语义以 `docs/synapse-rust/MSC_SEMANTICS.md` 为唯一真相源（该表登记了 MSC4155/4204/3967 等**借用编号**），
> 不得按编号推断语义。本表覆盖的 MSC 少于代码中实际出现的标识（`grep -rhoE 'msc[0-9]{4}'` 可查），
> 属"抽查"而非"全覆盖"。

| MSC / 功能 | Synapse (Python) v1.161 | synapse-rust v6.2.0 | 对齐状态 |
|------------|--------------------------|----------------------|----------|
| **核心 CS API** | ✅ 完整 | 路由面完整（`ROUTE_CONTRACT.md` 979 条注册路由）；按类别人工统计覆盖率 **80–97%**（`API_COVERAGE_REPORT.md`，2026-05-28 口径，非逐端点实测） | ⚠️ 未逐端点验证 |
| **联邦协议** | ✅ 完整 | ⚠️ **PARTIAL（2026-09-25 重判）**：`synapse-federation/` 模块存在，`send_*` 已按 `expected_membership` 校验；**`/send_join` 现已返回 `state` + `auth_chain`**（`synapse-web/src/routes/federation/membership/join.rs:209-212`（v1）、`:357-361`（v2）—— 旧版"仅回 `event_id`/`room_id`"已作废）。**但仍不合规**：① 响应体缺规范必需的 `event`（已签名的 join 事件），且 v1 缺 `[200, {…}]` 二元素数组包装；② `state`/`auth_chain` 条目是**手工拼装的 JSON**（`{event_id, sender, type, content, state_key}`，见 `synapse-services/src/room/messaging/events.rs:74-85`），**无 `hashes`/`signatures`/`depth`/`prev_events`/`auth_events`** ⇒ 合规远端无法验签、不能当 PDU 使用（根因见 §14.4）；③ `make_join` 模板仍缺 `origin`/`origin_server_ts`/`room_id`；④ 入房/离房路径**未调用房间 ACL 检查** | ⚠️ 部分对齐 |
| **E2EE** | ✅ 完整（libolm） | ✅ **服务端侧已对齐规范（v1.8 重判，2026-09-25）**：Megolm/Olm、交叉签名、密钥备份**真实**。原 v1.4 的「设备信任**真实**」与「SAS 已 HKDF 但仍 4 处偏离规范 / QR 为显式 fail-closed」**两条评价均已作废** —— 服务端参与的 SAS/QR/设备信任实现连端点一并**整模块删除**（`synapse-web/src/routes/verification_routes.rs`、`synapse-e2ee/src/verification/`、`synapse-e2ee/src/device_trust/`），设备验证回归规范的客户端 `m.key.verification.*` to-device 中继；`leak_detection` 模块**已删除**（能力缺失）；SSSS 已于 2026-09-23 对齐 `aes-hmac-sha2`（见 §7.2） | ⚠️ 服务端侧对齐；**客户端 SAS 接线为独立任务**（本仓不含客户端源码） |
| **Sliding Sync** | ✅ 完整 | ✅ 完整（独立 `sliding_sync_service/` 模块 + benchmark；另有 `msc4186` 简化滑动同步引用） | ✅ 已对齐 |
| **MSC3030** (Timestamp to event) | ✅ | ✅ | ✅ 已对齐 |
| **MSC2776** (Presence list) | ✅ | ✅（代码中无 `MSC2776` 标识，按路由 `presence.rs` 判定） | ✅ 已对齐 |
| **MSC2654** (Read markers) | ✅ | ✅ | ✅ 已对齐 |
| **MSC3079** (VoIP) | ✅ | ✅ | ✅ 已对齐 |
| **MSC3882** (QR login) | ✅ | ✅ | ✅ 已对齐 |
| **MSC3886** (Sliding sync) | ✅ | ✅ | ✅ 已对齐 |
| **MSC3983** (Thread) | ✅ | ✅（`thread_service.rs` + `synapse-storage/src/thread/`） | ✅ 已对齐（实现较精简） |
| **MSC3245** (Room summary) | ✅ | ✅ 完整（`synapse-services/src/room/summary/` + `synapse-storage/src/room_summary/`，单实现） | ✅ 完整对齐 |
| **MSC4380** (Invite shield) | ✅ | ✅ | ✅ 已对齐 |
| **MSC4354** (Sticky Event) | ✅ | ✅（`sticky_event.rs` 服务 + 存储） | ✅ 已对齐 |
| **MSC4261** (Widget API) | ✅ | ✅ | ✅ 已对齐 |
| **MSC4140** (Cancellable Delayed Events) | ✅（v1.143 起；v1.161 仅新增"查询单个延迟事件"端点） | ⚠️ **PARTIAL（联邦 EDU 已补齐，2026-09-25 复核）**：单机链路真实（`delayed_event_service.rs` + `synapse-storage/src/delayed_events.rs` + 调度器 `src/server/mod.rs` + 所有权 fail-closed）；**联邦 EDU 已实现** —— `synapse-federation/src/edu.rs` 的 `EduType::DelayedEvent` ⇄ `"m.delayed_event"`，消费点 `synapse-web/src/federation/edu.rs`。**仍缺**：schedule 的 `state_key` 硬编码 `None`（`synapse-services/src/delayed_event_service.rs:94`） | ⚠️ 单机 + EDU 已通，`state_key` 未填 |
| **MSC3912 / v11 撤回格式** | ✅（v1.161 #19782：room version > 10 时 `redacts` 放入 `content`） | ⚠️ **PARTIAL（格式已对齐 + 级联已实现，2026-09-25 复核）**：① 格式：`RoomMessagingService::create_event` 按房间版本注入 `content.redacts`（v11+），出站 PDU 不再重复写顶层 `redacts`，有回归用例锁定；② **级联已实现**：`synapse-storage/src/event/cascade.rs`（`find_related_events` / `find_cascade_targets` BFS / `cascade_redact_event`）+ `synapse-services/src/event_redaction_service.rs:58` + 端点 `POST /_synapse/admin/v1/rooms/{room_id}/cascade_redact`（深度默认 5、上限 10）；`MSC3912` 代码标识现命中 5 个 `.rs`。**仍缺**：级联**仅管理端可达** —— 客户端撤回 `synapse-web/src/routes/handlers/room/events.rs:990` 仍只调 `redact_event_content`（单条） | ⚠️ 格式+级联已实现，客户端不级联 |
| **MSC4242** (State DAGs) | ⏸ **不接线（2026-10-02 裁定，L-2）** | ⚠️ **仅存储层**（`event/dag.rs:179/208/237` + `create.rs:161`），无服务/联邦/路由/房间版本启用。**MSC 未定稿**：`proposals/4242-state-dags.md` **不在** matrix-spec-proposals 的 `main`（枚举 310 个提案无 `*4242*`，raw URL 404），仅存在于 PR #4242（open/unmerged/`needs-implementation`，**无 room version 指派**、无 `x-addedInMatrixVersion`）；其 HTTP 面**不是 CSAPI**，只有既有联邦路由上的 `state_dag` 旗标与新字段（`/get_missing_events`、`/send_join`），且只在未指派的 `org.matrix.msc4242.12` 下有意义；上游联邦侧 PR **#19425 已关闭未合并**。本地 `synapse-web/` 零命中。`dag.rs` 里"被 `/get_missing_events` 使用"的注释**已修**（`find_events_referencing_missing_state` 无生产调用者；同文件另两处同类注释核实为真） | ⏸ 不接线（语义未定稿；上游联邦 PR 关闭未合并） |
| **MSC4512** (Application Services Proxy) | ✅ v1.161 实验性（#19972 代理命名空间 + #19977 联邦请求） | ✅ **代理已实现（2026-09-25 复核，旧判"未实现"作废）**：`synapse-web/src/routes/app_service.rs` 的 `proxy_to_as`（AS 注册校验 + `hs_token` 鉴权 + hop-by-hop 头过滤 + 响应回传），注册两条 `any()` 路由 `/_matrix/app/v1/proxy/{as_id}/{*path}` 与 `/_matrix/client/v1/proxy/{as_id}/{*path}`；**未做**联邦侧代理请求（#19977 的另一半）。另注：`module_service.rs` 实际不止 spam/3P/auth，还含模块 CRUD、媒体与 account_data 回调、account validity | ⚠️ 代理已对齐，联邦侧缺失 |

**v1.161.0 上游条目对齐情况**（v1.3 已逐条实测，不再使用"待核查"）：

| 上游条目 | Synapse v1.161 | synapse-rust 实测 | 状态 |
|----------|----------------|-------------------|------|
| #20148 DB 宕机时新事件无法持久化 | ✅ 修复（根因：每实例 state-group persisted 标记过期） | **该机制在本仓不存在 ⇒ 上游具体 bug N/A**；但同类风险存在：`create_event_with_graph` 先 INSERT `events` 再于**事务外** INSERT `event_edges`（`synapse-storage/src/event/create.rs:112-142`），联邦入库/补洞/backfill 使用 | ⚠️ PARTIAL |
| #20119 `event_search` 跳过 `m.room.topic` | ✅ 修复 | 在线默认搜索面已修（见 §Search 行）：未传 `filter.types` 时含 `m.room.name`/`m.room.topic`（`event/search.rs:84`）。原 `synapse-storage/src/search_index.rs` 的**重建索引帮手含 `m.room.topic` 但整模块无调用者**（死代码），已于 2026-09-24 W4/D-27 按铁律 1 删除；`search_index` 表本身成为无人读写的遗留表（见 B8） | ⚠️ PARTIAL（死代码已清） |
| #20169 `/sync` 左房成员泄漏（MSC4222 `state_after`） | ✅ 修复 | **✅ 已实现（2026-09-26 O-6）**：`/sync` 端点新增 `state_after` 查询参数，左房 state events 按 `origin_server_ts > state_after_ts` 过滤（`response.rs` build_sync_response）；`SyncServiceRequest`/`BuildSyncResponseRequest` 透传该参数；3 个守卫测试覆盖 leave 过滤 / join 不过滤 / None 全量保留 | ✅ 已对齐 |
| #20149/#20172 Profile 500 | ✅ 修复 | 不存在用户 → 404 已对；account_data 非 JSON 对象仍 500（`extended_profile.rs:47-50`）；**已停用但存在用户写自定义字段返回 404**（`user/storage.rs:663-673`），与上游"应成功"相反；稳定 `GET /_matrix/client/v3/profile/{userId}/{keyName}` 未注册（仅 `uk.tcpip.msc4133`） | ⚠️ PARTIAL |
| #20173 Profile PUT/DELETE 400→403 | ✅ 修复 | ✅ 已正确返回 403 + `M_FORBIDDEN`（`account_compat.rs:195-197,224-226`）；上游触发配置（`enable_set_displayname` 等）本仓不存在 | ✅ 已对齐 |
| #20036 房间举报端点 `rc_reports` 限流 | ✅ 修复 | ✅ **已实现（Phase 2）**：路径中间件无法表达 `/rooms/{room_id}/report`（只有精确/前缀匹配），故在 `report_room`/`report_user` 内按用户取桶（`ratelimit:rc_reports:{user_id}`），规则可配（`rate_limit.rc_reports`，默认 1/s、burst 10），并有"配置键被删即变红"的守卫测试 | ✅ 已对齐 |
| #20180 `M_APPSERVICE_LOGIN_UNSUPPORTED` | ✅ 稳定化 | ⚠️ **部分**：`m.login.application_service` 已实现（Phase 2：as_token + 排他命名空间校验 + 设备物化 + 令牌签发，测试覆盖 200/403/401）。但该**错误码本身经复核属 `POST /register`**（MSC4190：appservice 未传 `inhibit_login=true`），不属 `/login`；上游 diff 本地无法取证，故未臆造该码 | ⚠️ 登录已实现，错误码待定 |
| #20146 LiveKit SFU WebSocket URL | ✅ 弃用 `livekit_service_url` 并新增 SFU URL | ⚠️ `livekit_service_url` 本仓不存在（无可弃用）；`LivekitConfig`（`ws_url`/`api_key`/`api_secret`/`host`）**全部字段从未被读取**，已随审计 RD-11 **整体删除**（`config/voip.rs` 不再有该结构体）；`rtc/transports` 仍只返回 ICE。**差异未消除**：SFU 传输本身未实现 —— 现在至少不再有一个"看起来可配"的旋钮 | ⚠️ PARTIAL |

**v1.2 遗漏的上游条目（同基准期，v1.3 补记）**：

- **v1.157.2 安全版本**：6 High / 4 Moderate / 2 Low ELEMENTSEC 公告 —— 本报告 §7 安全对比**完全未提**，应逐条判定本仓同类性。
- **v1.158 默认房间版本改为 11（MSC4239）**：~~本仓 `DEFAULT_ROOM_VERSION` 也是 11，但 `room_versions.rs:77-81` 注释仍称"Synapse 默认 10，本项目刻意不同"——**注释已过时**；且 v12/v13 为 `stable_parse_only`（可 join/联邦，**不可创建**）。~~ → **订正（2026-10-06，DOC-04）**：本仓 `DEFAULT_ROOM_VERSION` 已为 **12**（`synapse-common/src/room_versions.rs:94`，随 O-1 对齐上游 v1.162.0rc1），**v12 是唯一可创建版本**，v13 已移出能力表；旧注释亦已修正。
- **v1.158 v12 房间修复**（并发建房 room ID 冲突、v12 第三方邀请）；**MSC4326** appservice 设备伪装稳定化（本仓 0 命中）；**MSC2409** appservice 短暂事件（本仓 12 处引用）；**MSC4186** 简化滑动同步（24 处）；**MSC4502** 房间成员查询（11 处）；**MSC4262/MSC4429** Profile 更新进 sync（21 处）——这些代码中已有引用的编号，本报告 v1.2 均未列出。
- **#20189** 联邦 `make_*` 请求缺少 `membership` 校验（安全修复）——见 §12.5 B2。

### 11.2 扩展功能

> ⚠️ **历史章节 · 当前口径见 §18**：本节状态列包含已被后续提交修掉的「缺失」条目（逐条作废清单见 §18.5(a)），判断现状请以 **§18.3** 为准，**不要**据本节排期。

| 功能 | Synapse (Python) | synapse-rust | 对齐状态 |
|------|-------------------|--------------|----------|
| **好友系统** | 无（标准 Matrix 无此功能） | ✅ 独有扩展（`friend_room_service/` 含 groups/models/sharding，完整实现 + 联邦好友同步 `synapse-federation/src/friend/`） | ✅ 完整实现 |
| **阅后即焚** | 无 | ✅ 独有扩展（`burn_after_read_service.rs`）：**用户级时间注册表**而非服务端 redact（房间事件只有一份共享副本，redact 会导致「一人读即全员删」）；`schedule_burn` 重复调用取 `LEAST` 保证不顺延；`burn_after_ms` 边界校验 **1s–7d**（越界返回 400，历史遗留越界存储值读时回退 60s 默认）；`BURN_MAX_RETRY=5` 死信处理 + 5s 定时扫描器；pending 保留 7 天 / burn log 保留 30 天，由小时级 `purge_history` 清理；`/burn/pending` 在保留窗口内同时返回已处理行并暴露 `is_processed` / `is_dead_letter`，离线客户端仍可获知消息已焚 | ✅ 完整实现 |
| **信标/位置** | 无标准实现 | ✅ 独有扩展（`beacon_service.rs` 完整实现，含配额限制、背压控制、缓存） | ✅ 完整实现 |
| **内容扫描** | 模块化扩展 | ~~🔴 **已装配但无消费者（2026-09-25 复核，"从未被构造/未接入 config"作废）**：`synapse-services/src/wiring/core.rs:66,179` 已构造 `ContentScanner` 并注入 `CoreServices`，配置项已接入（`synapse-common/src/config/mod.rs:242`，类型在 `synapse-common/src/content_scanner/mod.rs`）；**但** `scan`/`scan_text`/`scan_media` 在生产路径 **0 调用点**，且 `synapse-storage/` **无**存储模块、`migrations/` **无**相关表 ⇒ 配置打开也不产生任何扫描行为~~ → **订正（2026-10-06，DOC-04）**：✅ **已装配且已接入生产路径** —— 装配 `synapse-services/src/wiring/core.rs:66,180`，消费 `synapse-web/src/routes/media/upload.rs:89,137`（`scan_when_enabled`）与 `synapse-web/src/routes/handlers/room/events.rs:327`（`scan_text_when_enabled`），是否实际扫描由 `content_scanner` 配置开关决定 | ✅ 已接线（开关决定是否扫描） |
| **短信推送** | 有（通过 Push Gateway） | ✅ `SmsProvider` trait（`sms_provider/mod.rs:16`）+ 三种实现：`NoopSmsProvider`（默认桩）、**通用 `HttpSmsProvider`（`:48`）**、`AliyunSmsProvider`（`aliyun.rs:43`）；工厂支持 `"aliyun"/"http"/"generic_http"` → 缺 Twilio 等厂商专用实现，但可用通用 HTTP 接入 | ✅ 已实现（厂商专用实现缺） |
| **VoIP Tracking** | 无 | ✅ Feature Flag 控制（`rtc/` 模块 + `voip/` 路由） | ✅ 已实现 |
| **服务器通知** | 有 | ✅ `server_notification_service.rs` 完整实现 | ✅ 完整实现 |
| **隐私扩展** | 无标准 | ✅ Feature Flag `privacy-ext`（存储层 `synapse-storage/src/privacy.rs` 985 行 + `user_privacy_settings` 表；服务逻辑在 `synapse-services/src/account_identity_service.rs:8-52` 与 `wiring/extensions.rs:51`，**不存在** `synapse-services/src/privacy.rs`） | ✅ 已实现 |
| **应用服务** | 完整（Pluggable Modules） | ⚠️ **部分实现（AS 登录与 MSC4512 代理已补齐，2026-09-25 复核）**：AS 注册/命名空间正则/虚拟用户/事务投递（含 `hs_token`）/调度**真实**（`application_service/`）；**AS 登录已实现**（`synapse-web/src/routes/auth_compat.rs:455-468`：as_token + 排他命名空间校验 + 设备物化 + 令牌签发）；**MSC4512 代理已实现**（见 §11.1）；**仍缺** pushers、设备管理、AS 以虚拟用户身份调用 C-S（客户端提取器只做 token 校验）、稳定错误码 `M_APPSERVICE_LOGIN_UNSUPPORTED`（全仓 0 命中）；`external_service.rs` 属私有桥接扩展（`/_synapse/external/*`），**不是** Matrix AS API | ⚠️ 实现不完整 |
| **延迟事件** | 有 | ⚠️ **PARTIAL**：单机链路完整 + **联邦 EDU 已实现**（见 §11.1 MSC4140）；`state_key` 仍硬编码 `None` | ⚠️ 可用，`state_key` 未填 |
| **关系性撤回** | 有（v1.161+） | ⚠️ **已按房间版本写 `content.redacts`，且级联撤回已实现**（`synapse-storage/src/event/cascade.rs` + 管理服务层 + 管理端点）；**剩余缺口**（经 2026-09-26 代码复核确认）：<br>- `cascade_redact_related_events` 调用 `redact_event_content(&target_id, None)` 失去审计追踪（P0）<br>- 客户端 `/redact` 路径仍只撤单条不级联（设计取舍，非 bug）<br>- v≤11 本地事件写路径（事务包装下）不持久化图字段，导致 `\\/send_join` PDU `MissingGraphMetadata`<br>详见 §16.1 | ⚠️ 格式 + 级联已实现，客户端不级联 + 审计缺口 |
| **房间升级** | 有 | ✅ `handlers/room/management/upgrade.rs` 完整实现（`upgrade_room` + `get_room_version`） | ✅ 完整实现 |
| **Space** | 有 | ✅ `synapse-web/src/routes/space/` 完整实现（children_hierarchy/lifecycle_query/membership_state/summary/types） | ✅ 完整实现 |
| **Thread** | 有 | ✅ `thread_service.rs` + `synapse-storage/src/thread/` + `handlers/thread.rs` | ✅ 已实现（相对精简） |
| **LiveKit / RTC** | 有 | ⚠️ **PARTIAL**：`rtc/` 存在；`livekit_service_url` 本仓从未存在，`LivekitConfig`（`ws_url`/`api_key`/`api_secret`/`host`）从未被读取，已随审计 RD-11 整体删除（`config/voip.rs` 不再有该结构体）；`rtc/transports` 仅返回 ICE | ⚠️ 能力不完整 |
| **Dehydrated Devices** | 有 | ✅ **已对齐（Phase 2）**：`/events` 改为 `GET` + query 参数，`next_batch` 空页返回 `null`（storage `Option<i64>` → service → handler 三层同步）；契约产物（derived 表、6 fixture、2 快照、ROUTE_CONTRACT、route-table、client.yaml、api_test 输入）已再生成；端到端测试断言 GET 200 + `events: []` + `next_batch: null` + 旧 POST 返回 405 | ✅ 已对齐 |
| **Rendezvous** | 有 | ✅ `rendezvous.rs` + `msc4108_rendezvous.rs` 完整实现 | ✅ 完整实现 |
| **Key Rotation** | 有 | ✅ `synapse-e2ee/src/key_rotation/` + `synapse-federation/src/key_rotation.rs` 完整实现 | ✅ 完整实现 |
| **事件报告** | 有 | ✅ `event_report_service.rs` + `synapse-storage/src/event_report/`（1662 行）；admin 举报端点**已实现**（`admin/report.rs:20-24`）——`API_COVERAGE_REPORT.md:126-127` 把它列为"缺失"是**过时信息** | ✅ 完整实现 |
| **背景更新** | 有 | ✅ `background_update_service.rs` + `synapse-storage/src/background_update.rs` 完整实现 | ✅ 完整实现 |

### 11.3 SDK 与前端生态

> ⚠️ **历史章节 · 当前口径见 §18**：本节状态列包含已被后续提交修掉的「缺失」条目（逐条作废清单见 §18.5(a)），判断现状请以 **§18.3** 为准，**不要**据本节排期。

| 维度 | Synapse (Python) | synapse-rust | 对齐状态 |
|------|-------------------|--------------|----------|
| **官方 SDK** | matrix-js-sdk, matrix-ios-sdk, matrix-android-sdk2 | 独立 matrix-js-sdk 封装层 | ✅ 已对齐（已知 Bug：URL 重复前缀等） |
| **前端集成** | Element Web/Desktop/iOS/Android | TJG 前端（Vue 3 + Tauri 跨平台） | ✅ 已对齐 |
| **Admin API** | 完整且文档化 | ⚠️ 覆盖较广（`synapse-web/src/routes/admin/` 含 audit/cleanup/federation/media/notification/policy/register/report/retention/room/management/security/server/token/user），但 `API_COVERAGE_REPORT.md`（2026-05-28 口径）按类别为 **89–94%**，非"完整对齐"；举报端点**已实现**（旧"缺失"清单过时） | ⚠️ 接近对齐 |
| **扩展端点** | 无标准 | ✅ `/_matrix/vendor/v1/` 私有扩展（`external_service.rs`，属私有桥接而非 Matrix AS API） | ✅ 已实现 |
| **OIDC/Builtin OIDC** | 有 | ~~路由完整（`routes/oidc/`），但 `synapse-services/src/oidc_service.rs:673` 的 `validate_id_token_claims` **从未被调用**（`#[allow(dead_code)]`，安全相关）~~ → **订正（2026-10-06，DOC-04）**：✅ 路由完整（`routes/oidc/`）；该 `validate_id_token_claims` 及配套死代码校验逻辑已**整体删除**（Phase 3 C9，全仓 0 命中） | ✅ 已对齐 |
| **SAML/CAS SSO** | 有 | ✅ Feature Flag 控制（`saml.rs` + `cas.rs`） | ✅ 已对齐 |
| **Key Backup** | 有 | ✅ `synapse-web/src/routes/e2ee/backup.rs` 完整实现 | ✅ 已对齐 |
| **Push Notifications** | 有 | ✅ `push/` + `push_notification.rs` + `client_push_service.rs` 完整实现 | ✅ 已对齐 |
| **Relations** | 有 | ✅ `relations_service.rs` + `synapse-storage/src/relations/`（不含 `content.redacts` 与级联撤回，见 §11.1） | ⚠️ 部分对齐 |
| **Search** | 有 | ⚠️ **默认搜索面已修（Phase 2）**：未传 `filter.types` 时改为 `IN (m.room.message, m.room.name, m.room.topic)`（`event/search.rs:84`，内容按 `content::text` LIKE 匹配，无需回填）；**已清（2026-09-24，W4/D-27）**：`synapse-storage/src/search_index.rs` 整模块（`SearchIndexStorage` 等，1239 行 / 8 处动态 SQL）无任何生产调用者，已按铁律 1 删除；**仍存**：`search_index` 表的 partial GIN 索引谓词未同步、表已无人读写（删表需独立迁移决策，见 B8） | ⚠️ 部分对齐 |
| **Webhooks/App Services** | 完整 | ⚠️ `app_service.rs` 提供 AS 管理/事务/命名空间；**AS 登录（`m.login.application_service`）已于 Phase 2 实现**；`external_service.rs` 是私有桥接扩展，**不能**算作 Matrix AS API；仍缺 pushers/设备管理/虚拟用户调用 C-S/MSC4512 | ⚠️ 部分对齐 |

**优势与劣势**:

| | 优势 | 劣势 |
|---|------|------|
| **Synapse** | - 协议覆盖最完整，所有 MSC 均已实现<br>- 官方 SDK 生态完善<br>- 与 Element 客户端深度集成<br>- 社区贡献和 Bug 修复活跃<br>- v1.161 新增 MSC4512（实验性）、MSC4242 联邦客户端（实验性）与 MSC4140 单事件查询端点 | - 不支持业务定制扩展<br>- 好友/阅后即焚等需要外部桥接<br>- 缺乏内置短信推送 |
| **synapse-rust** | - 好友系统/阅后即焚/信标等独有扩展实现完整（本轮实测为真）<br>- 通用 `HttpSmsProvider` + trait 接缝，便于接第三方短信<br>- Feature Flag 控制功能裁剪<br>- Room Summary 单实现架构清晰<br>- Space/Thread/Rendezvous/Key Rotation/事件报告/背景更新完整 | - **撤回格式与默认房间版本不匹配（v11 默认却用 v10 顶层 `redacts`）**——协议互操作缺陷<br>- **泄漏检测为未编译死代码**（原列的两条 —— "E2EE SAS 派生非规范、QR 为桩" —— 已于 2026-09-25 随服务端 SAS/QR 面整模块删除而**不再适用**）<br>- **MSC4140 无联邦/EDU**<br>- **`MSC3912`（关系性撤回）未实现**<br>- Content Scanner 模块未装配（孤儿）、LiveKit `ws_url` 死配置<br>- 无 appservice 登录、无 `rc_reports` 专项限流、Dehydrated `/events` 端点方法落后上游<br>- 生产路径仍有半写窗口、事务去重标记在事件事务外、4 处吞 DB 错误<br>- State DAGs (MSC4242) / App Service 代理 (MSC4512) 缺失（上游均为**实验性**） |

---

## 12. 总结结论

### 12.1 综合评估矩阵

| 评估维度 | Synapse (Python) | synapse-rust | 优势方 |
|----------|-------------------|--------------|--------|
| **架构设计** | 成熟但复杂 | 模块化 + 类型安全 | synapse-rust |
| **技术实现** | 协议完整但 binding 有开销 | 纯 Rust 实现无 FFI | synapse-rust |
| **性能指标** | 受 GIL 和 GC 限制 | 无 GIL 无 GC，多核并行 | synapse-rust |
| **可扩展性** | Worker 模式成熟但复杂 | 单进程多核 + Feature Flag | synapse-rust |
| **可维护性** | Python 灵活但类型安全弱 | Clippy + 编译时保障 | synapse-rust |
| **安全特性** | 审计成熟但 C binding 有风险 | Rust 内存安全 + Argon2 | synapse-rust |
| **用户体验** | 运维经验丰富 | 部署简单 + 快速启动 | 平手 |
| **开发效率** | Python 快速迭代 | 编译保障但迭代慢 | Synapse |
| **资源利用** | 内存/CPU 利用率低 | 高效利用 | synapse-rust |
| **业务对齐** | 协议完整但无定制扩展 | 协议大面积对齐 + 独有扩展，但存在**协议正确性缺陷**（v11 撤回格式、MSC4140 无联邦）与若干未实现项 | Synapse（对齐质量更高） |

### 12.2 核心发现

1. **性能是 synapse-rust 最显著的优势**：Rust 无 GIL、无 GC，单进程可利用多核，内存利用率预期提升 5-10 倍。这对于大型联邦房间（如 60,000 成员的 #matrix:matrix.org）场景尤为关键。

2. **安全是 synapse-rust 的结构性改进**：Rust 编译时内存安全保证 + Argon2 密码哈希 + `zeroize` 敏感数据清理，从语言层面消除了整类内存安全漏洞。但 vodozemac 的密码学审计历史短于 libolm。

3. **Synapse 的核心优势在于成熟度**：多年的生产验证、安全审计、社区运维经验、以及与 Element 客户端的深度集成，是 synapse-rust 短期内无法复制的。

4. **synapse-rust 的业务扩展能力是独有价值**：好友系统、阅后即焚、信标位置、内容扫描、阿里云短信等私有扩展功能，使该项目不仅仅是协议重写，而是面向特定业务场景的增强实现。

5. **开发效率是 synapse-rust 的主要代价**：Rust 学习曲线陡峭、编译时间长、社区贡献门槛高。Python 的快速迭代能力在原型开发和社区贡献方面仍有优势。

6. ~~**（v1.3 新增）协议正确性缺陷比"功能缺失"更值得优先处理**：本仓默认创建房间版本 11，但撤回事件仍按 v1–v10 的顶层 `redacts` 格式生成（`handlers/room/events.rs:959-982`），而 v11 消费方从 `content.redacts` 读取 → 本服务端发出的撤回可能在合规实现上不生效。~~ → **订正（2026-10-06，DOC-04）**：默认创建房间版本已为 **12**；撤回事件已**按房间版本**写入 `content.redacts`（v11+）或顶层 `redacts`（v≤10），上述旧行为已修。
   同类还有：`leak_detection` 为未编译死代码（原列的"E2EE SAS 派生未用 HKDF、QR 验证为桩"已于 2026-09-25 随服务端 SAS/QR 面整模块删除而不再适用）。
   这些是"已经声称支持、实际不符合规范"的项，风险高于"尚未实现"的 MSC4242/MSC4512（上游均实验性）。

7. **（v1.3 新增）数据一致性存在已知窗口**：`create_event_with_graph` 在无事务时先写 `events`
   再于事务外写 `event_edges`（`synapse-storage/src/event/create.rs:112-142`），联邦入库/补洞/backfill 走该路径；
   消息发送的 txn 去重标记在事件提交之后写入（`messages.rs:288-305`），标记失败时客户端重试可能产生重复事件。

8. **（v1.3 新增）"文档声称"与"代码实现"之间的漂移需要机制约束**：本次更新出现
   `api-reference.md`（不存在的文件）被当作来源、MSC4140 被标为 v1.161 新增、
   `include_redundant_members` 被当成 MSC4222 修复证据等。建议按 §12.5 D 类建立
   "数字必须可复现 / MSC 编号必须在语义表登记"的守卫。

> ⚠️ **§12.2 是 v1.3 时点的叙述**，保留作历史对照。其中第 6/7 条的判定已随 Phase 1/2/3 与后续各轮复核变化：
> 撤回格式已按房间版本写入 `content.redacts`、`soft_failed` 读路径已全面过滤、`leak_detection` 已删除、
> QR 为显式 fail-closed。**当前口径只看 §15 与 §12.4**；§12.2 不再单独维护。

### 12.3 适用场景建议

| 场景 | 推荐 | 原因 |
|------|------|------|
| **大规模生产部署** | synapse-rust | 性能和资源效率优势显著 |
| **快速原型开发** | Synapse | Python 迭代速度快 |
| **安全敏感场景** | synapse-rust | Rust 内存安全 + Argon2 |
| **资源受限环境** | synapse-rust | 内存占用低，Docker 镜像小 |
| **业务定制需求** | synapse-rust | 好友/阅后即焚/信标等扩展功能 |
| **社区贡献项目** | Synapse | 贡献门槛低，文档完善 |
| **合规审计要求** | Synapse | 安全审计历史长，CVE 记录完整 |
| **跨平台部署** | synapse-rust | 单二进制 + distroless 容器；⚠️ 依赖 glibc 动态库，**不是**静态链接，跨平台需按目标平台重新编译 |

### 12.4 风险提示

> ⚠️ **历史章节 · 当前口径见 §18**：本节状态列包含已被后续提交修掉的「缺失」条目（逐条作废清单见 §18.5(a)），判断现状请以 **§18.3** 为准，**不要**据本节排期。

> v1.3 起，风险条目只保留**实测存在**或**明确未验证**的项；已证伪的旧条目直接删除（不保留"可能"表述）。

- synapse-rust 的性能数据多为**预期值**（§4/§10 中"预期"字样均指未实测），缺乏大规模生产实测验证；
  Docker 镜像与二进制体积本轮**未实测**（无可用产物）。
- **vodozemac 已升级**：`Cargo.toml` 要求 `>=0.10.0`，`Cargo.lock` 实锁 **0.11.0**。
  2026-02 Soatok 披露的 DH 贡献性问题（Olm 接受恒等元导致共享密钥全零）已在 0.10.0 修复；
  相关历史公告可引 `GHSA-c3hm-hxwf-g5c6`（CVE-2024-34063）与 `GHSA-j8cm-g7r6-hfpq`（CVE-2024-40640）。
  —— 上一版本写作"CVE-2026-XXXX 系列"属占位符，不应出现在正式报告。
- SDK 封装层存在已知 Bug（URL 重复前缀、batch 接口不存在等）。
- Worker 拓扑：**静态配置校验 + 运行时对账（声明 vs 注册）均已成型**，且已有强制力（`worker.topology.strict`）、部署前离线入口（`--validate-topology`）、参考多进程栈与端到端烟测、指标/告警/面板；**请求分流的前提已于 2026-10-17 补齐（W-F）**——路由归属不再是注释级声明：`worker.surface_gate` 让非 master 实例对不属于自己角色的路径返回 404 `M_NOT_FOUND`；但**参考栈的进程选型未变**（worker 仍是只开 `/metrics` 的任务消费者），要分流必须以根二进制按角色起实例；⇒ 水平扩展对**控制面**成立、对**请求面**不成立（独立审计与实施记录见 [docs/audit/WORKER_TOPOLOGY_VERIFICATION_AUDIT_2026-10-16.md](docs/audit/WORKER_TOPOLOGY_VERIFICATION_AUDIT_2026-10-16.md)）。
- **协议正确性风险（v1.4 代码取证重排，优先级最高）**：
  - **【P0｜2026-09-25 重判：部分修复】联邦 `/send_join` 响应**：字段已补 —— `synapse-web/src/routes/federation/membership/join.rs:209-212`（v1）与 `:357-361`（v2）现返回 `state` + `auth_chain`，旧判"仅返回 `event_id`/`room_id`"**已作废**；**但仍不合规**：① 缺规范必需的 `event`（已签名 join 事件），v1 还缺 `[200, {…}]` 包装；② `state`/`auth_chain` 条目是手工拼装（`{event_id, sender, type, content, state_key}`，见 `synapse-services/src/room/messaging/events.rs:74-85`），**无 `hashes`/`signatures`/`depth`/`prev_events`/`auth_events`** ⇒ 合规远端**仍无法验签**。根因（本地事件不落 PDU 图元数据）未动，见 §14.4。**"合规远端无法完成入房"的结论依然成立。**
  - **【新 P0】OIDC 回调提权**：`routes/oidc/sso.rs:215-232` 仅按 `localpart` 命中本地用户即签发令牌，未校验 OIDC subject 绑定；可接管任意同名账号（含 admin）。同仓 `routes/oidc/provider.rs:187-201` 已有正确检查。（2026-09-23 独立复核：属实，两条路径语义不一致。）
  - ~~**【新 P0】`soft_failed` 读路径无过滤**~~ → **已修（2026-09-23，commit `53c43a48` + `7d968f6d`）**。`53c43a48` 只补了 `/messages` 的**无游标**分支；`7d968f6d` 补齐全部面向客户端的读取面：`get_room_events_paginated_cursor` 的**两个带游标分支**（`/messages` 真实生产路径，回归用例证明修复前会返回 loser）、`get_room_events_after_stream_ordering`（sliding sync）、`find_event_by_timestamp`/`find_event_id_by_timestamp`（MSC3030）、`get_room_events_batch_inner`（`/sync`，谓词置于 ROW_NUMBER 之前）、`has_room_events_since`、`get_room_message_counts_batch`、四个 `search_*`、`get_unread_counts(_batch)`、sliding sync bump_stamp、`count_sent_messages`。**行为回归**用例 `test_soft_failed_events_hidden_from_all_consumer_read_paths`（`synapse-storage/src/event/db_tests.rs`）：写入 winner(loser) 后逐个读取方法断言 loser 不出现、winner 仍在；先红后绿。**未覆盖（有意）**：DAG/prev_events/auth/state-resolution/联邦/redaction 目标查找等**内部**读取面——它们必须看到该行；`friend_room` 与 relations 读的是 state/`event_relations`，不属于 soft-fail 事件类型（仅 `send_message_with_txn` 调用 `mark_event_soft_failed`）。**顺带修掉** `search_postgres_messages` 的 `ts_rank`(real) → `f64` 解码缺陷（生产 postgres provider 会 `ColumnDecode` 失败）。
  - **E2EE**：**服务端侧 2026-09-25 已回归规范形态** —— 服务端 SAS/QR/设备信任的实现连端点一并删除（原"4 处偏离规范""QR 为显式 fail-closed 不支持"两条结论随之作废，见 §14.4 item 3），密码学归客户端 `m.key.verification.*` to-device；SSSS 已于 2026-09-23 对齐 `aes-hmac-sha2`；`leak_detection` 模块已删除（详见 §7.2）。
  - **MSC4140**：无 EDU/联邦。
  - ~~撤回格式 × 房间版本~~ → **Phase 1 已修**（服务层按房间版本注入 `content.redacts`，PDU 不再重复写顶层）；关系性级联撤回仍未实现（见 §11.1 MSC3912 行）。
  - ~~Dehydrated device `/events` 仅 POST~~ → **Phase 2 已修**（GET + query 参数，`next_batch` 空页返回 `null`）。
- **数据一致性（2026-09-23 逐条复核后的状态）**：
  - ~~`create_event_with_graph` 半写窗口~~ → **Phase 1 已修**：无调用方事务时改用本地事务包裹 `events` + `event_edges`（`event/create.rs:111-148`，代码注释标 B8）。
  - **同一窗口的"另一半"**：`create_state_event_with_dag` 在无调用方事务时同样是先落 `events`、再于事务外插 `event_edges`（B8 因两处插入逻辑重复而漏掉）→ **2026-09-23 已修**（改本地事务）；红证明 `create_state_event_with_dag_rolls_back_event_when_edges_insert_fails`（先证明修复前确有孤立行，再证明修复后整笔回滚）。⚠️ 两处插入逻辑仍是重复实现，建议抽成单一 helper（铁律 2）。
  - txn 去重标记仍在事件提交之后（`room/messaging/messages.rs`）；Phase 2 的补偿（标记失败即 soft-fail 已提交事件）**因上一条 P0（读路径不过滤 `soft_failed`）而实际无效** —— 本项保持"未解决"。
  - ~~生产路径吞 DB 错误：`messages.rs:32` 的 `unwrap_or(0)`、`federation/transaction.rs:358-362` 的 `.ok().flatten()`、`membership/federation.rs:191-216/251-268` 的 warn-and-drop~~ → **2026-09-23 逐条复核确认均已修**：前两处全仓 0 命中；第三处改为 `return Err` + 事务回滚（`membership/federation.rs:219`）。v1.4 复核文本称这三处"仍存在"属**过时**，此处按实测更正。
- **未实现/未装配风险**：
  - ~~**Content Scanner 整模块孤儿**（无存储、无 config、无构造点），非"仅缺存储层"。~~ → **订正（2026-10-06，DOC-04）**：已装配并接入生产路径（`synapse-services/src/wiring/core.rs:66,180`；消费 `synapse-web/src/routes/media/upload.rs:89,137`、`synapse-web/src/routes/handlers/room/events.rs:327`），是否扫描由 `content_scanner` 开关决定。
  - **App Service 登录整体缺失**（无 `m.login.application_service` / `M_APPSERVICE_LOGIN_UNSUPPORTED`），
    以及 pushers、设备管理、虚拟用户调用 C-S、MSC4512 代理缺失。
  - ~~**`rc_reports` 专项限流缺失**~~ → **Phase 2 已修**（handler 内 per-user 桶 + 可配规则 + 守卫测试）；**LiveKit `ws_url` 仍为死配置**；稳定 `/_matrix/client/v3/profile/{userId}/{keyName}` 未注册。
- **未验证/待决策**：v12/v13 房间能否从"可 join/联邦"推进到"可创建"；`MSC4186/4262/4502/2409` 等代码中已出现的编号
  其语义是否与官方一致（须查 `MSC_SEMANTICS.md`，当前未登记）。
- **对齐基准更新建议**：当前最新稳定版为 v1.161.0（2026-09-15）。后续审查须记录 tag + CHANGES 链接 + 复核日期；
  **安全版本（如 v1.157.2 的 12 条 ELEMENTSEC 公告）必须逐条纳入 §7**，不得省略。

### 12.5 优化建议（v1.3 重写）

> ⚠️ **历史章节 · 当前口径见 §18**：本节状态列包含已被后续提交修掉的「缺失」条目（逐条作废清单见 §18.5(a)），判断现状请以 **§18.3** 为准，**不要**据本节排期。

> **口径变更**：上一版本按 P0/P1/P2 + **人日估算**排列，且与仓库既有权威清单
> （`docs/audit/archive/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md`、`docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md`）
> **没有任何交叉引用**，构成第二份 backlog（违反 AGENTS.md 铁律 2/6 的精神）；同时把上游**实验性**的
> MSC4242/MSC4512 列为"P0 阻断性"，而漏掉了本次实测发现的协议正确性缺陷。
> v1.3 起改为：**只描述差异 + 指向权威清单**，每条给出证据与验收判据，不再给人日估算（估算无依据会掩盖不确定性）。
> 完整版本见 `docs/audit/archive/COMPARISON_REPORT_REVIEW_2026-09-22.md` §7。

**分层原则**：A 类（门禁与事实，已完成）→ B 类（协议正确性，本轮实测发现，优先）→ C 类（功能补齐与收敛，按上游成熟度定级）→ D 类（把文档可信度变成守卫）。

**B 类：协议正确性（建议优先于任何新功能）**

| 编号 | 现象与证据 | 动作 | 验收判据 | 上游关联 |
|------|------------|------|----------|----------|
| B1 | ~~默认 v11，撤回仍写顶层 `redacts`~~ → **已修（2026-10-06，DOC-04）**：默认创建版本已为 **12**，撤回已按房间版本写入 `content.redacts`（v11+）或顶层 `redacts`（v≤10）（`room_versions.rs:89,113` vs `handlers/room/events.rs:959-982`） | 撤回创建按房间版本分支写 `content.redacts`；修正过期注释 | v11 房间 `content.redacts` == 目标 id；v10 仍在顶层；互操作冒烟通过 | v1.161 #19782 |
| B2 | 联邦 `make_*`/`send_*` 的 `membership` 校验：**`send_*` 已校验**（`federation/membership/mod.rs:128-137` 按 `expected_membership` 比对），`make_join` 为无 body 的 GET | 按上游 diff 复核 `make_knock`/`make_leave` 是否同源；同源则补测 | `send_leave` 带 `membership=join` → 400 | v1.161 #20189（作用面 `[未验证]`） |
| B3 | 举报端点无 `rc_reports` 专项限流（`directory_reporting.rs:226-266`） | 补限流桶（客户端举报端点同理） | 429 + `retry-after` 有测试 | v1.161 #20036 |
| B4 | Dehydrated `/events` 仅 POST + body 游标（`assembly.rs:216-217`） | 改为 GET + query；`next_batch` 末页可为 null | 契约测试断言 GET 与 null | v1.157 #19896 |
| B5 | `create_event_with_graph` 半写窗口（`event/create.rs:112-142`） | 并入同一事务或给出补偿 | 注入 `event_edges` 失败后无孤立 `events` 行 | 新增 |
| B6 | txn 去重标记在事件事务外（`messages.rs:288-305`） | 标记与事件同事务，或"标记先行 + 幂等回填" | 注入标记失败后重试，房内仅 1 条事件 | 新增 |
| B7 | 4 处吞 DB 错误 | 改错误传播/fail-closed | 每点有失败用例证明返回错误 | CLAUDE.md 已知坑 |
| B8 | ~~搜索索引死代码~~（**已删**，2026-09-24 W4/D-27：`search_index.rs` 整模块无调用者，按铁律 1 删除）+ 在线仅 `m.room.message`（`event/search.rs:170,210,311`） | 接线或删除 ✅ 已删；统一索引事件类型集合；`search_index` 遗留表删否待决 | 主题可默认搜到（已做），或明确声明不支持 | v1.161 #20119 |
| B9 | Profile：停用用户自定义字段 404、稳定路由缺失、非对象 500 | 分开"存在/停用"判定；注册稳定路由或声明不支持；非对象→400 | 三条各自断言状态码 | v1.161 #20149/#20172 |
| B10 | v1.157.2 的 12 条 ELEMENTSEC 公告未做同类性判定 | 逐条产出"受影响/不受影响 + 证据" | 对照表 + 结论 | v1.157.2 |

**C 类：功能补齐与收敛（重新定级）**

| 项 | 原定级 | 新定级 | 理由 |
|----|--------|--------|------|
| E2EE SAS 对齐 HKDF + 真实 MAC 校验 | 未列（误判 ✅ 完整） | ~~**高**~~ **已作废** | 2026-09-25 去服务端私钥重构：服务端 SAS 实现已整模块删除，"对齐规范"不再需要（见 §14.4 item 3）；密码学归客户端 |
| E2EE QR 实现或声明未实现 | 未列 | **高** | 当前为桩（复用公钥 + 空签名），文档称"完整"属误报 |
| `leak_detection` 接入或删除 | 误判"已实现" | **高** | 未编译 + 启用即编译失败 + 桩计数 + schema 列缺失（铁律 1） |
| ~~OIDC `validate_id_token_claims` 接线~~ → **已作废（2026-10-06，DOC-04）** | 未列 | **已闭环** | 该函数及配套死代码已**整体删除**（Phase 3 C9），"声明校验未生效"随之消失 |
| ~~Content Scanner 装配或删除~~ → **已闭环（2026-10-06，DOC-04）** | P0"缺存储层" | **已闭环** | 已装配并接入生产路径（见 §11.2 / §12.4），是否扫描由 `content_scanner` 开关决定 |
| MSC4140 联邦（EDU） | ✅ 已对齐 | **中** | 单机真实，联邦缺失，需按草案确认是否必须 |
| 死字段清理（3 处 "Reserved/constructor parity"） | 未列 | **中** | 违反铁律 1 |
| MSC4242 State DAG | P0 阻断性 | **低（观察项）** | 上游实验性 + 无房间版本启用；先修正 `dag.rs` 不实注释 |
| MSC4512 AS 代理 | P0 阻断性 | **低（观察项）** | 上游实验性、opt-in |
| SMS 厂商专用实现（Twilio 等） | P1 | **低** | 已有 `SmsProvider` trait + 通用 HTTP provider |
| ~~v12/v13 创建支持~~ → **已定（2026-10-06，DOC-04）** | 未列 | **已定** | v12 是唯一可创建版本（`synapse-common/src/room_versions.rs:94`），v13 已移出能力表 |

**D 类：把"文档可信度"变成守卫**

1. 文档中的端点/文件/模块数必须来自 `ROUTE_CONTRACT.md` 或可复现命令，并用**故意写错的数字**证明检查会变红（铁律 8）。
2. 文档中的 `MSC\d+` 必须出现在 `MSC_SEMANTICS.md`；新增编号须同时登记语义。
3. 引用人工文档（如 `API_COVERAGE_REPORT.md`）必须带时间戳；过期条目不得作为"缺失"证据。
4. 每条 ✅ 必须带 `路径:行号` 或测试名；无法证实的写 `[未验证]` 并说明阻塞原因。

**实施建议**

1. **先 B 后 C**：B 类中 B1/B2/B5/B6/B7 属正确性与数据一致性，优先于任何新 MSC 开发。
2. 每个修复附**能变红的测试**（注入失败、版本分支、篡改 MAC 等），避免"报通过的门禁未必在工作"。
3. 修复后同步更新本文档 §11/§12.4 与 `MSC_SEMANTICS.md`（若涉及编号语义）。
4. 持续对齐：每季度复核一次，**必须包含上游安全版本**；记录 tag + CHANGES 链接 + 复核日期。

---

## 13. v1.3 复核修正记录（2026-09-22）

| 类别 | 修正内容 |
|------|----------|
| 门禁 | 修复 docs-quality-gate 拼写回归（`.aspell.ignore.txt` +10 词；修复前 `check_doc_spelling.sh` exit 1，修复后 exit 0） |
| 伪造引用 | 删除 `docs/synapse-rust/api-reference.md`（文件不存在）与 "656 端点 / 48 模块"；改挂 `ROUTE_CONTRACT.md`（1,166 条 / 66 模块） |
| 计数 | .rs 1020 / 439,130 行；workspace members 8 + 根 = 9；common 72、federation 20、web 162（routes 144）、根 `src/` 38；tests 230（106/113/3/5）；benches 5；docs 221；docker 212；migrations 1 |
| 版本 | tokio 实锁 1.53.1；vodozemac 实锁 0.11.0；insta 实锁 1.48.0；quickcheck 仅用于输入校验 |
| 事实 | 删除"sqlx 编译时验证"（static 61 / dynamic 2147 ≈ 2.8%）；删除"静态链接 + musl"（gnu + distroless 动态库）；修正 `server` 特性（已删除）、`web/routes/e2ee/backup.rs` 路径 |
| §7.2/§11.1 | E2EE 由 "✅ 完整" 降为 PARTIAL（SAS 非 HKDF、QR 桩、泄漏检测未编译）；MSC4140 由 "✅ 已对齐" 降为 PARTIAL（无联邦/EDU）；MSC3912 行由"待验证"改为明确的 v11 撤回格式缺陷 |
| §11.2/§11.3 | Content Scanner 由"缺存储层"改为"未装配孤儿模块"；SMS 补 `HttpSmsProvider`；privacy 路径纠正；Dehydrated `/events` 标注端点漂移；Search/Relations/Admin/App Service 降级为部分对齐 |
| §11.1 v1.161 表 | 8 项"待核查"全部实测判定；其中 #20169 的旧 ✅ 属误判（机制不同）；#20036、#20180 确认缺失 |
| §12.4 | 风险改为只保留实测/明确未验证项；CVE 占位符替换为真实 GHSA/CVE 编号 |
| §12.5 | 重写为 A/B/C/D 分层，删除人日估算，改为指向权威清单并按验收判据验收 |
| **代码修复（Phase 1）** | **B1** v11+ 撤回目标写入 `content.redacts`（服务层唯一写入口）+ PDU 不再重复写顶层 `redacts`；**B8** `create_event_with_graph` 无事务分支改单事务；**B10a** `send_message` 传播 `origin_server_ts` 读取错误；**B5** 修正过期房间版本注释。验证证据见 `docs/audit/archive/COMPARISON_REPORT_REVIEW_2026-09-22.md` §10（原计划文件位于 gitignored 的 `docs/superpowers/plans/`，不作为持久引用） |
| **代码修复（Phase 2）** | **B10b** 联邦 gap-fill 查重吞错；**B11** 默认搜索面纳入 `m.room.name`/`m.room.topic`；**B3** 举报端点 per-user `rc_reports` 限流（可配 + 守卫）；**B9** 事务去重标记失败时 soft-fail 已提交事件。证据见 `docs/audit/archive/COMPARISON_REPORT_REVIEW_2026-09-22.md` §11–§12（原计划文件位于 gitignored 目录，不作为持久引用）。**Phase 2 全部完成**：B3/B4/B9/B10b/B10c/B11/B13；遗留缺陷另记：`scripts/api_test/scan_handler_schemas.py` 的 `ROOT` 为硬编码绝对路径（会写错工作树）、ledger `query_params` 字段无消费方 |
| **代码修复（Phase 3，安全）** | **C1** SAS 规范对齐：`derive_sas` 改用 HKDF-SHA256（`synapse-e2ee/src/verification/service.rs:106`，带已知答案测试 `derive_sas_matches_hkdf_sha256_known_answer`）、`confirm_sas` 以 `synapse_common::crypto::secure_compare` 校验 HMAC（`:367`）、删掉随机 SAS 兜底（fail-closed）；**C2** QR 不再伪造载荷，改为 `ApiError::unsupported`（`:409,423`）；**C3** 删除从未编译的 `synapse-e2ee/src/leak_detection/` 死模块；**C9** OIDC `id_token` 校验失败改 fail-closed（`ApiError::unauthorized`）+ 授权 URL 携带 `nonce` + 删除绕过校验的死函数（`validate_id_token_claims` 全仓 0 命中）。提交 `a6f797f3` / `fda1317f` / `db570918` / `c010135c`；复核记录见 `docs/audit/archive/COMPARISON_REPORT_REVIEW_2026-09-22.md` §13 |
| **v1.4 代码取证复核（2026-09-22）** | **推翻本文档 5 处旧结论**（详见 §14.1）：① SAS 由"SHA256 派生 + 接受任意非空 MAC"→ 实为 `derive_sas` 已 HKDF（`verification/service.rs:105-113`）、`confirm_sas` 已校验 MAC（`:316-375`），但**仍 4 处偏离规范**；② QR 由"桩"→ 实为**显式 fail-closed** `M_UNSUPPORTED`（`:403-424`）；③ `leak_detection` 由"未编译死代码"→ 实为**目录已删除**；④ `create_event_with_graph` 半写窗口由"仍存在"→ 实为 **Phase 1 已修**（本地事务，`event/create.rs:111-148`）；⑤ `validate_id_token_claims` 由"从未被调用"→ 实为**已接线**，真实缺陷在 `routes/oidc/sso.rs:215-232`。**新增 3 条 P0**（§14.2）；§11.1 联邦协议由 ✅ 降为 PARTIAL；§7.2 SSSS 新增非合规判定。<br>（2026-09-23 交叉复核：①②③④ 与 3 条新 P0 均**独立复现属实**；**⑤ 已过时** —— Phase 3 C9 已把该函数整体删除，当前树 `validate_id_token_claims` 0 命中，`exchange_code` 改为 fail-closed；但其指出的 `routes/oidc/sso.rs:215-232` 提权面**仍然存在**。） |
| **本轮（2026-09-23）** | **文档可信度变成门禁**：新增 `tests/unit/doc_credibility_guard_tests.rs` —— 本文档（1）引用的仓库相对路径必须存在、不得引用 gitignored 的 `docs/superpowers/plans/`；（2）声明的 route 条目数与模块数必须等于 `docs/synapse-rust/ROUTE_CONTRACT.md`；两个纯谓词各有红证明（把 v1.2 的"伪造引用/过期计数"原文喂进去必须被判违规）。据此前置修正：§13 两处 gitignored 计划引用改为指向复核报告；`docs/*.md` 这类 glob 与"文档有意记录某文件不存在"的否定陈述分别按过滤/白名单处理。**代码优化**：删除 LiveKit `ws_url` 死配置（`synapse-common/src/config/voip.rs`，从未被读取）；关闭 `create_state_event_with_dag` 的半写窗口（B8 因两处插入逻辑重复而漏掉的并行路径，改为本地事务，红→绿证明见 `create_state_event_with_dag_rolls_back_event_when_edges_insert_fails`）；修 `synapse-storage/src/server_notification/repository.rs` 5 处 `created_by` 缺 `Some(..)`（**main 上 CI lib 批次仍因此红**）。**更正本报告自身的过度声明**：上一版把 E2EE SAS 写成"已按规范对齐"不成立 —— 经逐行复核（info 串缺公钥且 txn 占位、emoji 仅 6 个且 decimal 被丢弃、MAC 非 `hkdf-hmac-sha256.v2`、commitment 用全零密钥 HMAC），仍 **4 处偏离规范**，§7.2/§11.1 已按 v1.4 复核改写。**计数重测（口径改为可复现的 `git ls-files`）**：`.rs` **1,019 个 / 442,451 行**；`docs` **188**（其中 `*.md` 155）；`docker` **58**；`tests` **292**（unit 132 / integration 128 / e2e 5 / performance 5 / 其余 22）；`benches` 5；`migrations` 3；per-crate 未变（common 72 / federation 20 / web 162、routes 144 / 根 `src/` 38）。⚠️ 旧版对 `docs`/`docker`/`tests` 用 `find` 统计，会随 gitignore 与本地产物漂移（同一提交在不同 worktree 得出 221/212/230 与 188/58/292 两套数）—— 故统一改为 `git ls-files` |

---

## 14. v1.4 复核保留摘要 + v1.5 修复进度（2026-09-23 续）

### 14.1 v1.4 复核的 5 处旧结论更正（保留摘要）

| 旧结论（v1.3 及更早） | 实测结论 | 证据 |
|----------------------|----------|------|
| SAS 用 `SHA256(secret‖info)` 派生，非 HKDF | **已更正**：`derive_sas` 实为 HKDF-SHA256（无 salt、info 为上下文串、取 6 字节）——该实现已于 **2026-09-25 随去服务端私钥重构整模块删除**，本行仅存历史（见 §14.4 item 3） | `synapse-e2ee/src/verification/service.rs`（文件已删除） |
| `confirm_sas` 接受任意非空 MAC | **已更正**：校验 MAC 非空、要求 `keys`/`peer_pubkey`、`secure_compare` 比对，不符 403 | 同上 |
| QR 验证为桩（复用同一公钥 + 空 `signature`） | **已更正**：显式 fail-closed，返回 `M_UNSUPPORTED` | 同上 |
| `leak_detection` 是"未在 `lib.rs` 声明"的死代码 | **已更正**：整个目录**已删除**，即该能力不存在 | `glob 'synapse-e2ee/src/leak_detection/**'` 无结果 |
| OIDC `validate_id_token_claims` 从未被调用（死代码） | **已更正**：该函数已被调用；真实缺陷在回调侧未校验 subject 绑定。**2026-09-23 再更正**：Phase 3 C9 已删除该函数，`exchange_code` 改为 fail-closed | `synapse-web/src/routes/oidc/sso.rs` |

### 14.2 v1.4 新增 3 条 P0 的当前状态

| # | 问题 | 状态（2026-09-23） |
|---|------|--------------------|
| P0-1 | 联邦 `/send_join` 响应缺 `state`/`auth_chain` | 🟡 **部分修复（2026-09-25 重判，见 §15.1）**：字段已补 —— `synapse-web/src/routes/federation/membership/join.rs:209-212`（v1）与 `:357-361`（v2）现返回 `state` + `auth_chain`。**但仍是 P0**：响应缺 `event`、v1 缺 `[200,{…}]` 包装、且 `state`/`auth_chain` 条目无 `hashes`/`signatures`/`depth`/`prev_events`/`auth_events`（手工拼装）⇒ 合规远端无法验签。**旧判"仅回 `event_id`/`room_id`"已作废**，但"合规远端无法完成入房"的结论**依然成立** |
| P0-2 | OIDC 回调提权（按 localpart 签发令牌，无 subject 绑定） | ✅ **已修**：回调路径写入并复用 OIDC 绑定（`fe35fb0a`），账号接管判定抽成纯函数并补判定表用例（`0a633b89`） |
| P0-3 | `soft_failed` 无任何读路径过滤 | ✅ **已修**：`53c43a48`（时间线无游标分支）+ `7d968f6d`（其余全部消费者读取面 + 行为回归用例），详见 §12.4 |

### 14.3 本轮（2026-09-23 续）已完成并验证

| 项 | 内容 | 提交 | 验证 |
|----|------|------|------|
| 1 | `soft_failed` 其余消费者读取面（pagination 带游标分支 / batch(`/sync`) / search 四条路径 / unread / sliding sync / 消息计数）+ 行为回归用例；顺带修 `ts_rank` 的 `real`→`f64` 解码缺陷 | `53c43a48`、`7d968f6d` | `cargo nextest run -p synapse-storage --lib -E 'test(soft_failed_events_hidden)'` 先红（cursor 分支返回 loser）后绿 |
| 3（一半） | **SSSS 对齐 `m.secret_storage.v1.aes-hmac-sha2`**：HKDF（salt=32×0，info=空串/secret name）、AES-256-CTR（128 位大端计数器）、encrypt-then-MAC、16 字节 IV 且 bit 63 清零、无填充 base64、`decrypt_secret` 先验 MAC；删除不可解的 MSC2697 curve25519 路径（改为 fail-closed 400）；`create_key`/`encrypt_secret`/`decrypt_secret` 改关联函数 | `a2375743` | `synapse-e2ee --lib` SSSS 27 项全绿，含 **NIST SP 800-38A F.5.5 CTR-AES256 已知向量**、篡改密文→403、错误 secret name→403、非 32 字节密钥拒绝 |
| 5（B2） | **签名前校验 `make_join`/`make_leave` 模板**（上游 #20189 / 规范 PR #2284）：新增纯函数 + join/leave 接线，失败 400 且不取签名密钥、不调 `send_*` | `cd7b97bb`（被并发写入者合并） | 8 项表驱动单测 + 端到端：mock 返回 `membership:"ban"` ⇒ 400 且 `send_join_call_count()==0` |
| 5（B9c） | MSC4133 非对象文档：写入口加形状守卫（400）、读取面 `internal`→`bad_request`；data type 常量收敛到一处 | `7fc49e06` | 3 项单测 + 集成 `test_msc4133_non_object_document_is_bad_request_not_server_error` 实跑通过（21.7s） |

### 14.4 遗留阻塞与后续计划

**item 2（P0-1 `/send_join` 的 `state`/`auth_chain`）—— 根因与最小正确实现**

本轮取证发现该缺陷比"补两个响应字段"深：本仓**本地创建的事件从来不落 PDU 图元数据**。

- 房间创建（`synapse-services/src/room/lifecycle/create.rs` 的 `m.room.create`/`power_levels`/`join_rules`/`member` 等）走**普通 `create_event`**：`depth`/`prev_events`/`auth_events` 为 `NULL`，且**完全没有签名/哈希**（该文件不调用 `sign_and_broadcast_event`）。
- `sign_and_broadcast_event`（`room/messaging/service.rs`）虽然把 `prev_events` 放进被签名的 PDU，但只回写 `signatures`/`hashes`，**不回写** `depth`/`prev_events`/`auth_events`。
- 因此 `state` 数组里的多数事件既无 `auth_events` 也无 `depth`，而 `hashes.sha256` 覆盖这些字段——**事后补造会作废哈希与远端签名**，不能只在响应侧修。
- 存储里也没有 auth 边表：`event_edges` 的 `is_state` 区分的是"房间 DAG vs MSC4242 状态 DAG"，**不是 auth vs prev**；auth 边只存在于 `events.auth_events` JSONB。

最小正确实现（建议单独分支/里程碑）：① 为本地事件补齐创建期 PDU 管线——选 `auth_events`（按房间版本的 auth 事件选择规则）+ 算 `depth` + 签名/哈希 + 用 `create_event_with_graph` 落库，覆盖房间生命周期、状态、消息、成员等**全部**本地写入口；② 新增 storage 全 PDU 读取（`depth/prev_events/auth_events/hashes/signatures/unsigned`）与 auth 链闭包查询；③ 统一 `serialize_full_pdu`；④ `/send_join` v1 返回 `[200, {...}]`（v1 **必须**是二元素数组）、v2 返回裸对象，二者均含 `state`/`auth_chain`/`event`/`members_omitted:false`；⑤ 顺带修 `make_join` 模板（补 `origin`/`origin_server_ts`）与 `SendJoinResponse.origin` 改为 `Option`（v1.14 起规范已删除响应中的 `origin`，当前客户端结构体把它当必填，解析真实 Synapse 响应会失败）。

**item 3（SAS 4 处偏离）—— 已随"去服务端私钥重构"消解（2026-09-25）**

原结论是"规范修法被私有 API 形状阻塞"：`/keys/device_signing/verify_*` 是本仓私有的非规范 REST 面，其 `mac` 是单个字符串、`keys` 是 `key_id → 公钥值` 的映射且**没有算法字段**，规范 `hkdf-hmac-sha256.v2` 的 key-list MAC 无法在其形状内表达。**该阻塞已按"删除而非修补"处理**：`synapse-web/src/routes/verification_routes.rs` 与 `synapse-e2ee/src/verification/`（连同 `device_trust` 面）**整模块删除**，服务端不再计算 SAS 的 info 串 / emoji / decimal / MAC / commitment。因此四项偏离**不再适用于本仓** —— 它们描述的服务端实现已不存在；SAS 密码学全部移到客户端（`matrix-sdk-crypto-wasm`）经 `m.key.verification.*` to-device 完成，服务端只中继。

**item 5 其余子项**

- **B8（搜索死代码）**：`synapse-storage/src/search_index.rs`（`SearchIndexStorage` 等，1239 行）**无任何生产调用者**，已于 2026-09-24 W4/D-27 按铁律 1 **整模块删除**（连带回收 8 处生产动态 SQL，棘轮 `dynamic_production` 706 → 694），故本文不再引用该路径。**遗留**：`search_index` **表**在模块删除后已无任何生产读写方（仅 `tests/integration/schema_contract_p0_tests_migrated.rs` 断言其形状），删表需新增前向迁移并同步 schema-contract 用例与 SDK fixture —— 已登记为 §7 D-39，属独立 schema 决策。
- **B9(a)/(b)**：停用用户在 MSC4133 上返回 404 而标准 `/v3/profile` 返回 200（两面对"存在 vs 停用"判定不一致）；稳定 `/_matrix/client/v3/profile/{userId}/{keyName}` 未注册而 capability 已声明 `m.profile_fields`。**未做**（(b) 需重生成 ledger/快照/契约 fixture，或改为不再声明该 capability）。
- **B10**：见 §14.5 对照表。
- **C 类死字段**：实测为 **7 处**（不是 3 处）——`room/lifecycle/service.rs`、`room/state/service.rs`、`room/membership/service.rs`、`room/service.rs`（`event_writer`，仅 `burn-after-read` 特性下有 1 个读取点）、`friend_room_service/models.rs`、`admin_registration_service.rs`、`admin_security_service.rs` 的 `user_service`/`event_writer`，均为 `#[allow(dead_code)]` + "Reserved/constructor parity"。另见 `user_service.rs` 的 `event_reader` + `set_event_reader`（0 调用者）。**未做**（需删除字段 + 构造参数 + wiring + 测试构造点，并新增可红的守卫测试）。

### 14.5 B10：Synapse v1.157.2 ELEMENTSEC 公告对照（2026-07-28，共 **11** 条而非 12）

> 计数三处交叉验证一致（GitHub Releases API 正文、tag `v1.157.2` 的 `CHANGES.md`、仓库 advisory 列表 `patched_versions: ["1.157.2"]`）。
> 均无 CVE 编号。"12"的来源疑为 ELEMENTSEC-2026-1740 描述中交叉引用的旧公告 `GHSA-rfq8-j7rh-8hf2` 被一并计数。

| # | 公告 | 组件 | 本仓判定 | 主要证据 |
|---|------|------|----------|----------|
| 1 | ELEMENTSEC-2026-1071 / GHSA-fp53-rw9v-hcf9 | push rules 数量/体积无上界 → 磁盘/内存耗尽 | ⚠️ **可能受影响/待深查** | push rule 走独立表（`client_push_service.rs` → `synapse-storage/src/push/mod.rs`），未发现 per-user 条数上限；唯一的 64KB 限制在 account-data 路径，pushrules 路由不经过 |
| 2 | ELEMENTSEC-2024-1520 / GHSA-rgv2-84w7-5j9p | to-device EDU 发送方伪造 | ✅ 不受影响 | `synapse-web/src/federation/edu.rs` 要求 `user_matches_origin(sender, origin)`，否则丢弃 EDU |
| 3 | ELEMENTSEC-2026-1717 / GHSA-27p5-4f45-gx76 | ✅ **已实现（2026-10-02，M-1）** | ✅ 不受影响（有纵深防御备注） | 路由先 `validate_federation_origin_can_observe_room`；storage 最终查询 `WHERE room_id = $1 AND event_id = ANY($2)`（CTE 本身未按房间限定，建议补注释） |
| 4 | ELEMENTSEC-2026-1721 / GHSA-95fh-hv8c-chvq | 联邦错误回传导致客户端销毁加密状态 | ✅ 不受影响 | `From<FederationClientError> for ApiError` 一律映射为 500 `M_UNKNOWN`，远端状态码被丢弃 |
| 5 | ELEMENTSEC-2026-1729 / GHSA-cjh7-rcpx-xpf8 | 房间别名重定向 | ⚠️ **本地可劫持（确认）** | `synapse-storage/src/room/mod.rs` 的 `ON CONFLICT (room_alias) DO UPDATE SET room_id = EXCLUDED.room_id` 会静默改指；路由无"别名已被占用"预检。联邦向量待深查 |
| 6 | ELEMENTSEC-2026-1740 / GHSA-6wjm-9p2x-gvpm | `multipart/form-data` Content-Type 大小写绕过 DoS 缓解 | ✅ **不受影响（2026-10-17 裁决）** | 绕过的前提是"先按 `Content-Type` 字面量比较、再决定是否解析"——本仓没有这道门：`Content-Type` 只参与 mime 解析（大小写不敏感），内存由**请求体上限**界定。⚠️ 复核中同时发现一处**真实缺陷（已修）**：`/voice/upload` 原先没有自己的 `DefaultBodyLimit` ⇒ 落到 axum 的 **2 MiB** 默认值，而 `/voice/config` 的 `max_size_bytes` 与 handler 校验都按 `server.max_upload_size`（默认 50 MiB）算 ⇒ **2 MiB 以上的音频在传输层被拒，且状态码是 400 `M_BAD_JSON`**（限制经 `Multipart` 报成解析失败）—— 客户端会误判成请求格式错误，比 413 更难诊断。现按 `max_upload_size + 64 KiB`（封装开销）显式设界；三条回归见 §19.10 |
| 7 | ELEMENTSEC-2026-1714 / GHSA-qcjr-46gf-7f4r | `/get_event_auth` 缺已入房校验 | ✅ 不受影响 | 先 `validate_federation_origin_can_observe_room`，再按 `room_id` 限定取事件 |
| 8 | ELEMENTSEC-2026-1718 / GHSA-r66v-qhwx-8rg4 | `/timestamp_to_event` 缺成员校验 | ✅ 不受影响 | 格式校验后调 `validate_federation_origin_can_observe_room` |
| 9 | ELEMENTSEC-2026-1751 / GHSA-jhcg-5392-5mjw | Sliding Sync 畸形响应（非法房间名/头像/资料） | ✅ 不受影响（结构上不可能） | 相关字段均为 `Option<String>`，类型系统保证不会输出错误 JSON 类型 |
| 10 | ELEMENTSEC-2026-1703 / GHSA-vh4c-pqh4-w3wq | 多余路径段被忽略 → 限流绕过 | ✅ 应用层不受影响 / ⚠️ 代理层待查 | axum 0.8 精确 `{param}` 匹配、无通配路由，fallback 404 `M_UNRECOGNIZED` |
| 11 | ELEMENTSEC-2026-1760 / GHSA-hgcg-p9gx-fq5f | 尾随后缀被接受 → nginx 规范化代理绕过 | ✅ **应用层不适用（2026-10-17 裁决）** | 上游成因是部分端点**容忍后缀**（`/path/extra` 命中 `/path`）；本仓 axum 0.8 精确段匹配（`{param}` 只吃一段，通配需显式注册 `{*wildcard}`），未匹配即落兜底 404 `M_UNRECOGNIZED`。代理层：nginx 规范化后的 URI 就是转发给后端的 URI（`proxy_pass` 未带 URI 重写），后端仍按精确路由判定 ⇒ 规范化不产生额外暴露；敏感面（admin）另有 token 门。回归守卫 `test_extraneous_path_suffix_and_trailing_slash_are_not_routed`（含 `..` 归一化形态），见 §19.10 |

**结论**：需要动作或明确决策的是 #1（push rule 上限）与 #5（别名劫持），两条均已在 §19.5.1 修掉；#6 与 #11 已于 2026-10-17 裁决为**不受影响**（#6 的复核顺带揪出一处 body 上限缺陷并修掉，见 §19.10）；其余 7 条本仓已有对应守卫。**本表已闭环**。

### 14.6 并发写入者事故记录（流程）

本轮作业期间，另一个 agent 会话（`.workbuddy`，Phase 3 closeout / SIGTERM 内存门禁 / E2EE v2 优化）在**同一工作树、同一分支**上持续 `git add` 提交，把本轮在途改动至少 3 次扫进其提交：`bc1501f9`（"format db_tests.rs"，含本轮的 soft_failed 回归用例）、`d6c55ed8`（"T2EE-001 cross-signing keys"，含本轮 SSSS 的 `models.rs`/`Cargo.toml`/`Cargo.lock`）、`cd7b97bb`（"Sync: 处理并发会话遗留的 federation 相关变更"，含本轮 B2 校验）。内容未丢，但**提交信息与内容不符**，违反 AGENTS.md 铁律 9（同一工作树同一时刻只允许一个写者）。本轮已通过"每完成一项立即逐路径 `git add` + 提交"把暴露窗口压到最小；后续并行作业必须改用独立 `git worktree`。

---

## 15. v1.6 本轮复核（2026-09-25）

### 15.0 方法与基线

- **基线**：分支 `opt/consolidated` @ `9e26ee31a`（本章全部取证完成于该提交）；上游对齐基准 Synapse **v1.161.0**（2026-09-15）。
- **基线后位移（不影响结论）**：收尾期间并发会话把 HEAD 推进到 `57cb5e81a`
  （`908ee4b35` storage 游标测试 `+64`、`57cb5e81a` D-15.3 登记，改
  `synapse-storage/src/event_report/db_tests.rs` 与 `docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md`）；
  两个提交都**不触及本轮审计面**（路由注册 / baseline schema / 服务实现），故 §15 全部结论在 `57cb5e81a` 上同样成立。
- **方法**：不采信本文档任何既有 ✅/🔴，全部**回到源码取证**；每条给 `路径:行号` 或可复现命令。
  计数一律 `git ls-files`；路由一律以 `docs/synapse-rust/ROUTE_CONTRACT.md`（2026-09-25 重新生成，
  `bash scripts/contract/check_route_contract.sh` EXIT=0，仅时间戳漂移）为准。
- **口径优先级**：本章与 §12.4 冲突时以本章为准；§12.2 为 v1.3 历史叙述，不再单独维护。

### 15.1 本轮新发现（旧文档没有的）

| # | 发现 | 证据 |
|---|------|------|
| N1 | ~~**Content Scanner 是"已装配但无消费者"** —— 旧判"从未被构造、未接入 config"不成立，但真相更值得警惕：模块被真实构造并接入配置，**却没有任何调用点**~~ → **订正（2026-10-06，DOC-04）**：已装配**且已接线**，消费点为 `synapse-web/src/routes/media/upload.rs:89,137` 与 `synapse-web/src/routes/handlers/room/events.rs:327` | ~~`synapse-services/src/wiring/core.rs:66,179`（构造）+ `synapse-common/src/config/mod.rs:242`（配置项）；`scan`/`scan_text`/`scan_media` 在生产路径 0 调用点，`synapse-storage/` 无存储模块~~ → 装配 `synapse-services/src/wiring/core.rs:66,180`，是否扫描由 `content_scanner` 开关决定 |
| N2 | **MSC3912 级联撤回只到管理端** —— 存储/服务/管理端点齐备，但客户端撤回路径不级联 | `synapse-web/src/routes/handlers/room/events.rs:990` 只调 `redact_event_content` |
| N3 | **`/send_join` 的 `state`/`auth_chain` 是手工拼装的非 PDU JSON** —— "有字段"≠"可验签"，本轮把 P0-1 从"缺字段"重判为"字段在但内容不可用" | `synapse-services/src/room/messaging/events.rs:74-85` 只拼 `{event_id, sender, type, content, state_key}`，无 `hashes`/`signatures`/`depth`/`prev_events`/`auth_events` |
| N4 | **SQLx 静态化比例被本文档长期低报约 13 倍** | 实测 静态 806 / 动态 1396（`bash scripts/ci/check_sqlx_dynamic_ratio.sh`，`ratio=0.634`）；旧文写的 61 / 2147 来自有缺陷的计数器（漏算 turbofish、误算注释） |
| N5 | **`API_COVERAGE_REPORT.md` 的逻辑端口径实为 795**（旧引 883 无机器来源；2026-09-25 起为 注册条目 1,135 / 唯一路径 903 / 逻辑端点 795） | §3.4 与 `docs/synapse-rust/API_COVERAGE_REPORT.md` §1.1 |

### 15.2 已修复（不要再重测）

| 项 | 证据 |
|----|------|
| P0-2 OIDC 回调提权 | `synapse-web/src/routes/oidc/sso.rs:215-244`（按 `issuer + subject` 绑定授权，fail-closed） |
| P0-3 `soft_failed` 读路径无过滤 | `53c43a48` + `7d968f6d`（覆盖全部客户端读取面 + 行为回归用例） |
| SSSS 对齐 `m.secret_storage.v1.aes-hmac-sha2` | `a2375743`（含 NIST SP 800-38A F.5.5 已知向量） |
| 撤回按房间版本写 `content.redacts` | Phase 1；v11 用例锁定 |
| MSC4140 联邦 EDU | `synapse-federation/src/edu.rs:37,67,83` ⇄ `"m.delayed_event"`；消费点 `synapse-web/src/federation/edu.rs` |
| `rc_reports` 专项限流 | `synapse-web/src/routes/directory_reporting.rs:235,293`（桶函数 `:638-645`） |
| AS 登录 `m.login.application_service` | `synapse-web/src/routes/auth_compat.rs:455-468` |
| MSC4512 AS 命名空间代理 | `synapse-web/src/routes/app_service.rs:722-723` + `proxy_to_as` |
| MSC3912 级联（存储/服务/管理端点） | `synapse-storage/src/event/cascade.rs`、`synapse-services/src/event_redaction_service.rs:58` |
| MSC3814 `/events` 改 GET | `docs/synapse-rust/ROUTE_CONTRACT.md` 在册为 GET |
| LiveKit `ws_url` 死配置 | `synapse-common/src/config/voip.rs:83` 注释确认该字段已删 |
| `synapse-storage/src/search_index.rs` 死模块 | 已整模块删除（`search_index` **表**仍在，属 D-39 待决） |
| D-42 边插入守卫 `$2 != '[]'`（`text[]` 上必报 `22P02`） | `synapse-storage/src/event/create.rs` 改 `cardinality($2) > 0`（3 处） |

### 15.3 仍然存在（按严重度，当前口径）

> ⚠️ **历史章节 · 当前口径见 §18**：本节状态列包含已被后续提交修掉的「缺失」条目（逐条作废清单见 §18.5(a)），判断现状请以 **§18.3** 为准，**不要**据本节排期。

| 严重度 | 项 | 证据 / 判据 |
|--------|----|-------------|
| **P0（已收窄）** | 联邦 `/send_join`：**响应面已修**（v1 补 `[200,{…}]` 包装、v1/v2 补 `origin`，`state`/`auth_chain` 统一走 `routes/federation/pdu.rs::build_pdus` 真实 PDU 投影）。**残余**：① 本地 `create_event` 不落 `depth`/`prev_events`/`auth_events` ⇒ 本地起源事件仍判 `MissingGraphMetadata` 并**故意不发签名**；② 入站事件不落原服务端 `signatures`；③ 无 `event` 字段（本仓不产 restricted-join 房间，规范允许） | 见 §15.1 N3；写入路径根因见 §14.4；收口记录见 `docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md` §21.5 |
| **P0（新登记）** | PDU **语义**未对齐：`event_id` 为 `$<ms>_<rand>:<server>` 而非 v4+ reference hash ⇒ 字段齐全也不被 v11 对等端接受 | `synapse-common/src/crypto.rs:149` |
| ~~**高**~~ **已作废** | ~~E2EE SAS 4 处偏离规范（info 串缺公钥且顺序错 / emoji 仅 6 个且 decimal 被丢弃 / MAC 非 `hkdf-hmac-sha256.v2` / commitment 非 SHA-256）~~ → **2026-09-25 该面已整模块删除**（`/keys/device_signing/verify_*` 私有非规范 REST 面随去服务端私钥重构一并删除） | 见 §14.4 item 3 |
| **高** | 客户端撤回不级联 | `synapse-web/src/routes/handlers/room/events.rs:990` |
| ~~**高**~~ **已作废** | ~~Content Scanner 空转（装配了但不扫描）~~ → **已接线（2026-10-06，DOC-04）** | 见 §15.1 N1 订正 |
| **中** | MSC4242 仅存储层，且 `dag.rs` 注释声称被 `/send_join`、`/get_missing_events` 使用（实测无调用点） | `synapse-storage/src/event/dag.rs` |
| **中** | 上游 1.161 已删的 `msc2965/auth_issuer` 本仓仍在册 | `docs/synapse-rust/ROUTE_CONTRACT.md` |
| **中** | Profile 三处偏差：稳定 `/{keyName}` 未注册、停用用户写自定义字段 404、account_data 非对象语义 | 路由在册清单 + `user/storage.rs` |
| **中** | MSC4502 / MSC4262 仍为 PARTIAL 且未收敛 | 各 8 个 `.rs` 命中 |
| ~~**中**~~ **已闭环** | ~~Admin 媒体端点族部分缺失（本仓 14 条 vs 上游 18 条；房间级列举/删除已补全）~~ → **已补齐（2026-10-06，DOC-04）**：`2df0b8fa8` 补齐并去重，本仓 **22** 逻辑端点已超上游人工口径 18（见 `docs/synapse-rust/API_COVERAGE_REPORT.md` §6.3） | 用户媒体隔离、按日期删除、远程缓存清理、解除保护均已注册 |
| **中** | 缩略图 `animated` 参数未支持；`M_USER_LIMIT_EXCEEDED` 未用于媒体限额 | 两处均在业务层 0 命中 |
| ~~**低（决策）**~~ **已定** | ~~v12/v13 房间不可创建~~ → **订正（2026-10-06，DOC-04）**：v12 **是唯一可创建**版本（`synapse-common/src/room_versions.rs:94`），v13 已移出能力表 | `synapse-common/src/room_versions.rs:152` `RoomVersionCapability::stable("12")` |
| **低** | `search_index` 遗留表（D-39） | baseline 仍有该表 |
| **低** | ledger `query_params` 字段无消费方 | 仅 `synapse-web/src/routes/route_ledger.rs` 定义 |

### 15.4 证伪 / 降级 / 口径修正

- **证伪**：`scripts/api_test/scan_handler_schemas.py` 的"硬编码绝对路径（会写错工作树）" —— 实为
  `Path(__file__).resolve().parents[2]`，是**正确的相对推导**。
- **证伪**：`update_pool_metrics` 是"死调用点 ⇒ 指标恒 0" —— 现有周期宿主
  （`src/tasks/mod.rs:206`、`src/server/mod.rs:405`）。
- **证伪**：`scripts/load-test/` 与 `scripts/test/perf/` 两份 k6 重叠 —— 两个目录**都不存在**。
- **降级**：P0-1 由"缺 `state`/`auth_chain` 字段" → "字段已在但**非可验签 PDU**"（严重度不变，判据换）。
- **收窄（2026-09-25 收口批次）**：P0-1 的**响应面与字段面已修** —— v1 补 `[200,{…}]` 包装、补
  `origin`；四条发射路径统一到 `synapse-web/src/routes/federation/pdu.rs`，`state`/`auth_chain` 现在是
  含 `origin_server_ts` 的 PDU，并复用库中 `hashes`/`signatures`、否则**现场签名**。**未动的三条**
  已单独登记（见 §15.3 两行 P0）：本地写入路径不落图元数据、入站事件不落原签名、`event_id` 非
  reference hash。⚠️ **不要再把"字段在"当成"已合规"**——本批只闭合了字段与签名，未闭合**语义**。
- **降级**：~~Content Scanner 由"孤儿模块、从未被构造" → "已装配但**无消费者**"。~~ → **再订正（2026-10-06，DOC-04）**：Content Scanner 已**装配且已接线**（消费点见 §15.1 N1 订正）。
- **口径修正**：SQLx 静态化 2.8% → 36.6%（计数缺陷）；路由逻辑端点 883 → 813
  （2026-09-25 E2EE 去服务端私钥重构删 30 条路由后为 **795**，见 §3.4）；
  `docs`/`tests`/`docker` 计数由 `find` 改为 `git ls-files`（同一提交在两个 worktree 会给出两套数）。

### 15.5 建议执行顺序

1. **`/send_join` PDU 语义收口**：v1 数组包装与真实 PDU 投影**已完成**（`routes/federation/pdu.rs`），
   剩下的是**创建期 PDU 管线** —— 让 `EventStorage::create_event` 落 `depth`/`prev_events`/
   `auth_events`，否则本地起源事件仍无法产出完整 PDU（等价于 §14.4）。随后才是 `event_id`
   改 reference hash（v11 对等端的硬门槛，见 §15.3 第二行 P0）。
2. **Content Scanner 决策**（接线或下线）：现状是"配置看起来能开"，会误导运维与安全评估。
3. **客户端撤回级联接上**，或显式声明不支持。
4. **SAS 私有 API 形状重构**（其余 SAS 偏离修法的前置）。
5. 其余按 §12.5 的 B/C 表；文档侧按 §12.5 D 类继续把"数字必须可复现"变成守卫。

---

> **声明**: 本报告基于 synapse-rust v6.2.0 工作树（`HEAD 9e26ee31a`，分支 `opt/consolidated`）与 Synapse v1.161.0
> （2026-09-29，`release-v1.162` CHANGES.md）编写；v1.162.0 对本仓的逐项对照见 §18。**性能与资源数据凡标"预期/未实测"者均未经过生产验证**，
> 不得作为容量规划依据。所有结论遵循"代码优先"原则；凡未实测项显式标注，不以"需确认"充当结论。
> **审查方法**：以可复现命令与 `路径:行号` 为唯一证据形式——
> 计数与版本来自 `git ls-files` / `Cargo.toml` / `Cargo.lock`；
> 路由与模块口径来自 `docs/synapse-rust/ROUTE_CONTRACT.md`；
> MSC 语义以 `docs/synapse-rust/MSC_SEMANTICS.md` 为准；
> 上游条目来自 `element-hq/synapse` `release-v1.161` CHANGES.md；
> 逐条证据与命令清单见 `docs/audit/archive/COMPARISON_REPORT_REVIEW_2026-09-22.md`。

## 16. v1.8 代码验证更正（2026-09-26）

### 16.1 方法论

本轮复核（2026-09-26）以 `opt/consolidated` @ `9e26ee31a` 为基线，对文档中"仍存在"的问题声明进行**源码级别复核**。每项判定均给出可复现的 `路径:行号` 或命令。

### 16.2 纠正的误述

| 编号 | 原文声明 | 经证实 | 证据 |
|----|------|---|---|
| U-19-1 | "级联撤回缺乏逐事件授权检查" | ❌ **误判**：`cascade_redact_related_events` 确实在行 134 调用 `can_redact_event` | `synapse-services/src/event_redaction_service.rs:134` |
| U-19-2 | "空 `with_rel_types` 返回 400" | ❌ **误判**：行 993 处空列表意为"不级联" | `synapse-web/src/routes/handlers/room/events.rs:993` |
| U-20-1 | "reaction 不写入 events 表，级联查不到" | **设计取舍非缺陷**：`event_relations` 独立存储是 MSC3912 规范形式，`find_related_events_single_layer` 正确读取 `events.content->'m.relates_to'`，且 `event_relations` 表有 GIN 索引 | `cascade.rs:71-91`、`migrations/00000000_unified_schema_v12.sql:3306` |
| U-19-R2 | "`events` 表缺 `content` GIN 索引" | ❌ **误判**：索引已存在 (`idx_events_content_gin`, 2026-09-27 核查) | `migrations/00000000_unified_schema_v12.sql` 含 `CREATE INDEX IF NOT EXISTS idx_events_content_gin ON events USING GIN (content jsonb_path_ops);` |

### 16.3 仍存真实问题（列举，等待处理）

| 编号 | 问题 | 严重度 | 证据 |
|----|------|---|---|
| U-19-R4 | `cascade_redact_related_events` 传 `None` 给 `redact_event_content` 失去审计追踪 | P0 → ✅ 已修复 (commit `5d1bfdc3f`) | 现在传递 `redaction_event_id` 给 `redact_event_content`，见 `event_redaction_service.rs:125-172` |
| U-19-R2 | `events` 表 `content` GIN 索引 ✅ 已存在 | - | `migrations/00000000_unified_schema_v12.sql` 含 `idx_events_content_gin` |
| U-13-R9 | v≤11 写路径不持久化图字段 (`depth`/`prev_events`/`auth_events`) | P0 → ✅ 已修复 (commit `11bf5455d`) | `synapse-services/src/room/messaging/events.rs:171-243` 现在所有 v≥1 走 `create_event_with_pdu` |
| U-3 | v≤11 端点 `knock.rs`/`voip.rs` 占位 event_id | P0 → ✅ 已修复 (commit `31b475710`) | 现在消费 `stored.event_id`，测试 `federation_existence_leak_tests::knock_room_returns_the_id_of_the_persisted_row` |
| U-2 | `user_exists` 停用过滤语义不完整 | P1 → ✅ 已修复 (commit `bb69ad7a0`) | `synapse-storage/src/user/storage.rs:704-734` 拆分为 `user_exists`（含停用）与 `active_user_exists`（排除停用） |
| U-5 | Admin 媒体端点族不完整（14 条） | P1 → ✅ 已补全 (2026-09-27) | 新增 4 条端点：用户隔离、按策略删除、清除缓存、解除保护；共 18 条，与上游 v1.161 对齐 |
| U-6 | 缩略图 `animated` 边缘问题 | P1 | `download.rs`、`media/mod.rs` |

### 16.4 代码复核命令

以下命令可验证本章节所有结论：

```bash
# 1. 验证 U-19 级联授权检查
grep -n "can_redact_event" synapse-services/src/event_redaction_service.rs

# 2. 验证空列表处理
awk 'NR==993' synapse-web/src/routes/handlers/room/events.rs

# 3. 验证 event_relations GIN 索引
grep "GIN" migrations/00000000_unified_schema_v12.sql | grep event_relations

# 4. 验证 v≤11 写路径差异
awk 'NR==25,47' synapse-storage/src/event/create.rs

# 5. 验证 events 表是否缺 GIN 索引
grep "CREATE INDEX" migrations/00000000_unified_schema_v12.sql | grep -i "events.*gin\|gin.*events"
```

---

## 17. 2026-09-27 行动清单

基于上述代码验证，建议的后续行动：

1. **紧急**：修复 U-19-R4（`redacted_by=None`） → 传递 `redaction_event_id` 给 `redact_event_content`（已在 5d1bfdc3f 修复）
2. **高优先级**：v≤11 写路径持久化图字段（联邦 PDU 语义收口的最后一步）
3. **中优先级**：Admin 媒体端点族补全（剩余 4 条：用户隔离、按时间删除、远程缓存清理、解除保护）
4. **低优先级**：缩略图 animated 边缘问题

---

## 18. v1.10 对照 Synapse v1.162.0（2026-10-01，当前口径）

> **本节是"当前口径"**。§1–§17 是历史轮次记录；与本节冲突的表述一律以本节为准
> （尤其是 §11.2/§11.3/§12.4/§12.5/§15.3 中已被后续提交修掉、却仍写成"缺失"的条目，
> 见 §18.5 的作废清单）。

### 18.1 方法与基线

- **上游基线**：`element-hq/synapse`，`v1.162.0`（2026-09-29 发布）。取数方式（可复现）：
  ```bash
  git init /tmp/synapse-upstream && cd /tmp/synapse-upstream
  git remote add origin https://github.com/element-hq/synapse.git
  git fetch --depth 1 origin refs/tags/v1.162.0:refs/tags/v1.162.0 refs/tags/v1.161.0:refs/tags/v1.161.0
  git show v1.162.0:CHANGES.md | awk '/^# Synapse 1\.161\.0/{exit} {print}'   # v1.162.0 全部条目
  git diff --stat v1.161.0 v1.162.0                                            # 153 文件 / +8,223 / −1,739
  git diff v1.161.0 v1.162.0 -- synapse/config/ docs/usage/configuration/config_documentation.md
  ```
- **本仓基线**：`opt/consolidated` @ `f8c45b73d` 与 `feature/2026-10-01-metrics-docs-updates` @ `418d32d8b`
  **已于 2026-10-02 合流**（反向合并：`opt/consolidated` 合入 feature 分支）。两条线此前**各自独立实现**了
  同一批条目（详见 §18.7），合流时已逐项去重，**不留双实现**。
  v1.162.0 的对齐提交都在这些分支上：`18a07b2c1`（metrics 模板）、`08d014626`（MSC4354 sticky EDU）、
  `d409bd89b`（OTK 上限 / historical user_id / `M_UNKNOWN_DEVICE`）、`ede782382`（MSC4140 GET /
  stream gauge / `redis.username` / profile 空字段 / hierarchy `allowed_room_ids`）、
  `ee3181c26`…`cf52e113f`（第三方事件准入钩子 D-1）、`aee2f098e`（M-5 跨请求共享决议缓存）、
  `b30bf1997`（L-3 worker 显式路径集）、`cf845cb35`+`2695d5706`（L-6 relations 4 段路由）。
  §18.3 表中标 `e1ffcb2ab` 的证据行取自当时的分支 tip；**2026-10-02 第二轮复核**按 `f8c45b73d` 逐行复算，
  更正了 #3（`redis.username` 已对齐）与 #12（第三方钩子已实现，非缺口）。
  ⚠️ **不要**用旧的 `.worktrees/c19b` @ `64a015a8d` 判定 v1.162 的对齐状态
  （本文 §18 初稿曾按 c19b 误判 6 项为缺失，已在 §18.3/§18.5 更正）。
- **判定口径**：`路径:行号` 或可复现命令；找不到即写"❌ 缺"而不用"待确认"充当结论；无法在本轮证实的写
  "⚠️ 待验证"并在 §18.6 列出验证方法。
- **v1.162.0 无 Deprecations/Removals**（上游 upgrade 文档零 diff），故本节不含废弃项处置。

### 18.2 v1.162.0 变更要点（按类归纳）

| 类 | 变更 | 上游模块 / 配置 / 接口 |
|---|---|---|
| 功能 | 默认房间版本 11 → **12** | `synapse/config/server.py::DEFAULT_ROOM_VERSION`；`default_room_version`（config_documentation 标注 _Changed in 1.162_） |
| 功能 | 单设备**单算法一次性密钥上限 500**，超限整批拒绝（400） | `synapse/handlers/e2e_keys.py::MAX_ONE_TIME_KEYS_PER_ALGORITHM_PER_DEVICE`；`POST /keys/upload` |
| 功能 | Redis **ACL 用户名**（Redis 6+），且用户名必须同时有密码 | `synapse/config/redis.py::redis_username`；`redis.username` |
| 功能 | 客户端 **profile 查询限流** `rc_profile`（默认 1/s、burst 500；已认证按用户、否则按 IP） | `synapse/config/ratelimiting.py`；`synapse/rest/client/profile.py`；`GET /profile/{userId}[/{field}]` |
| 修复 | MSC4311 邀请/敲击：**联邦侧改发完整 PDU**（`invite_room_state`），客户端面仍用 stripped state；严格校验推迟到 **2027-06-01** | `synapse/handlers/federation.py`、`rust/.../msc4311_stripped_state`；`PUT /_matrix/federation/v2/invite/{roomId}/{eventId}` |
| 修复 | 第三方规则回调 `check_event_allowed()` 支持 MSC4291 房间 | `synapse/events/...`；第三方规则模块回调 |
| 修复 | 本地缩略图**异步打开**（`defer_to_thread`），提升并发 | `synapse/media/...` |
| 修复 | 丢弃**非合规历史 user id** 的联邦 `m.device_list_update` EDU | `synapse/handlers/device.py::is_compliant_user_id_localpart` |
| 修复 | 未设置的 displayname/avatar_url **不再物化为 `null`**（`GET /profile/{userId}/{field}` 返回 `{}`，修 1.135.0 引入的回归） | `synapse/storage/databases/main/profile.py` |
| 修复 | 客户端 `GET /_matrix/client/v1/rooms/{roomId}/hierarchy` 返回 **`allowed_room_ids`**（Matrix 1.15 起必需；此前被 `for_client` 剥掉） | `synapse/handlers/room.py` |
| 修复 | MSC4222 `state_after`：`since` 落在事件持久化批次内时不再漏状态事件（worker 场景） | `synapse/handlers/sync.py`、`sliding_sync/*` |
| 修复 | 返回**稳定 `M_UNKNOWN_DEVICE`**（原 `ORG.MATRIX.MSC4326.M_UNKNOWN_DEVICE`） | `synapse/api/errors.py` |
| 性能 | 递归 `/relations`：把 `events` **join 进递归 CTE**（原为递归后再 join 全表扫描，大房间秒级） | `synapse/storage/databases/main/relations.py` |
| 修复 | MSC4354 Sticky Events：房间状态变化时**取消 soft-fail**（联邦更可靠；配合 MSC4242 用 `prev_state_events`） | `synapse/storage/databases/main/sticky_events.py`、`federation/sender/*` |
| 内部 | Rust 化 logcontext；单一 per-homeserver Rust 状态；新增 `synapse_storage_stream_current_position` 指标；MSC4242 HTTP 脚手架；当前房间状态缓存；状态决议缓存（按冲突事件）；per-destination 事务 prepare/complete 拆分；`/room_summary` 陈旧 `join_rules` 修复；CI/文档若干 | `rust/src/logging/context.rs`、`synapse/storage/controllers/state.py`、`synapse/state/*` 等 |

### 18.3 逐项对照（本仓现状；原证据基线 `e1ffcb2ab`，2026-10-02 按 `f8c45b73d` 逐行复核）

| # | v1.162.0 项 | 本仓状态 | 证据（`e1ffcb2ab`） |
|---|---|---|---|
| 1 | 默认房间版本 12 | ✅ **已对齐** | `synapse-common/src/room_versions.rs:94` `DEFAULT_ROOM_VERSION="12"`；`:119-152` `SUPPORTED_ROOM_VERSIONS`=1..12（v1–v11 `stable_no_create`、v12 可创建）；v13 有意移出 |
| 1b | `available` 列出多版本 | ⚠️ **有意不同**：只列**可创建**版本（仅 `"12"`），上游列 11/12 | `room_versions.rs:210-218` |
| 2 | OTK 单设备单算法 500 上限（400） | ✅ **已实现**（`d409bd89b`） | `synapse-e2ee/src/device_keys/service.rs:20-21,317-326`（整批拒绝 + `MatrixErrorCode::TooLarge`，测试 `synapse-e2ee/src/device_keys/service/tests.rs:283/294/315` 断言 400 且不落库） |
| 3 | `redis.username`（ACL）+ 必须配密码 | ✅ **已对齐（2026-10-02，M-1；第二轮复核更正）** | `synapse-common/src/config/database.rs:187-195`（`user:pass@` / `user@` / `:pass@` 三种连接串形状）；**启动校验已补**：`validation.rs:82-88` 拒绝「配了 username 却没配 password」（空白值按未配置处理，与连接串口径一致）；**env 插值已补**：`loader.rs:81-82` 让 `redis.username` 与 `password` 走同一条 `${VAR}` 路径。`database.rs:307` 的 username-only **形状测试**已改名为 `redis_connection_url_username_only_shape_is_formatter_only` 并写明该配置被 `Config::validate` 拒绝、运行时不可达 —— 它钉的是纯函数行为，不再是"反例合法"的证据 |
| 4 | `rc_profile` 覆盖 profile 查询 | ✅ **已实现** | `synapse-common/src/config/rate_limit.rs:95-97,110`（`per_second 1`、`burst_size 5`）；`synapse-web/src/routes/account_compat.rs:99,119,140`（GET）与 `:206,238`（PUT）。⚠️ 默认 burst 与上游 500 不同 |
| 5 | 未设置 profile 字段返回 `{}` | ✅ **已对齐**（`ede782382`） | `synapse-web/src/routes/account_compat.rs:155-165` `single_profile_field` → `{}`；`synapse-services/src/user_service.rs:459-473` `build_profile_json` 省略未设置键 |
| 6 | hierarchy 返回 `allowed_room_ids` | ✅ **已实现**（`ede782382`） | `synapse-web/src/routes/handlers/search/hierarchy.rs:128,198` → `annotate_allowed_room_ids`（`:203-224`），经 `resolve_allowed_room_ids`（`synapse-services/src/room/summary/service.rs:97`） |
| 7 | 稳定 `M_UNKNOWN_DEVICE` | ✅ **已按规范更正（2026-10-02，撤回了 `d409bd89b` 越界的那半）** | `synapse-common/src/error/code.rs:100,149,201` 仍定义该码（Matrix 错误码词汇表成员），但**设备 CRUD 不再使用它**：`synapse-web/src/routes/device.rs` 的 `device_not_found_error()` 返回 `M_NOT_FOUND`。依据是规范本身 —— `matrix-org/matrix-spec` 的 `data/api/client-server/device_management.yaml` 对 `GET`/`PUT`/`DELETE /_matrix/client/v3/devices/{deviceId}` 的 404 描述为 _"The current user has no device with the given ID"_，且该文件**从未出现 `M_UNKNOWN_DEVICE`**。`M_UNKNOWN_DEVICE` 属 MSC4326（_appservice device masquerading_），本仓无该路径 ⇒ 该码当前**没有任何端点使用**（`synapse-common/src/error/code.rs` 已加注释防止被再次误用）。**原「✅ 已实现」判定作废**：把它推广到普通设备 CRUD 属编号语义越界，并让 `api_device_routes_tests::test_get_device_returns_not_found_for_other_users_device` 长期判红 |
| 8 | MSC4311：联邦侧 full PDU + 宽限到 2027-06-01 | ✅ **已收口（2026-10-01，决策 D-1 同批）** | `federation.msc4311_strict_validation` 由 **0 读取点**变为真实生效：入站 `/invite` 在持久化任何房间行之前调用 `validate_invite_room_state_shape`（`synapse-web/src/routes/federation/membership/invite.rs`）—— 宽限期（默认 `false`）同时接受 stripped 与 full PDU，`true` 要求每条都带 `event_id`+`sender`+`origin_server_ts`，否则 400 并写明开关与 2027-06-01 期限。**口径更正**：该开关原文档写"PDU 图字段校验"，但 invite 的 `depth`/`prev_events`/`auth_events` 自 P0-1 起已**无条件必需**（拒绝在伪造 DAG 位置上持久化）；MSC4311 仍处过渡期的部分是 **stripped-state 载荷形状**，开关改为 gate 它（配置注释已同步）。另一半：客户端 `/sync` 新增 **`rooms.knock`** 段（此前 `knock` 会员关系落进 catch-all ⇒ 被当成 **joined** 房间渲染），`SyncRoomSection::Knock` + `knock_state`（与 `invite_state` 同一 stripped 形状、同一 fail-closed 规则）。证据：`msc4311_grace_period_accepts_both_shapes`、`msc4311_strict_rejects_stripped_state`、`msc4311_strict_requires_all_full_pdu_markers`、`test_room_sections_knock_membership_maps_to_knock`、`knock_stripped_state_uses_knock_state_key_and_keeps_create`、`insert_room_stripped_state_populates_the_target_map`；`/sync` 形状变化已同步 insta 快照 |
| 9 | 丢弃非合规历史 user id 的设备列表 EDU | ✅ **已实现**（`d409bd89b`） | `synapse-common/src/validation.rs:73` `is_compliant_user_id_localpart`；`synapse-web/src/federation/edu.rs:13,115` 在 `validate_device_list_update_content` 中丢弃 |
| 10 | MSC4222 `state_after` | ✅ **已按规范实现（2026-10-01；原为编号借用）** | 接受 `?use_state_after=true` 与 `?org.matrix.msc4222.use_state_after=true`（不稳定名优先，响应字段镜像它）；opt-in 后房间段**省略 `state`**、返回 `state_after`（不稳定名为 `org.matrix.msc4222.state_after`），**空也返回**；内容 = 上次同步 → 本次 timeline 末尾（本地状态窗口 `stream_ordering > since` 上界无界，天然满足，无需第二次查询）；**删除了非规范的 `?state_after=<event_id>` 参数与左房时间戳过滤**（生产不可达的死分支）。证据：`msc4222_opt_in_replaces_state_with_state_after`、`msc4222_unstable_opt_in_mirrors_the_unstable_field_name`、`msc4222_state_after_is_present_even_when_empty`、`msc4222_default_keeps_state_and_never_emits_state_after`（services）、`msc4222_use_state_after_accepts_both_spellings`、`msc4222_use_state_after_requires_a_truthy_value`（web）；默认路径回归：`sync_authenticated_initial` 快照**零 diff** |
| 11 | MSC4354 sticky 事件 un-soft-fail | ⚪ **已裁定不跟随（2026-10-01，决策 D-2）**（EDU 已通；本仓无 soft-fail 对象） | 存储 `synapse-storage/src/sticky_event.rs` + `migrations/00000000_unified_schema_v12.sql:609`；客户端路由 `synapse-web/src/routes/sticky_event.rs`；`/sync` 注入 `sync_service/response.rs:238-259`；EDU 出入站 `synapse-federation/src/edu.rs:38-40,71,88`、`web/federation/edu.rs:645,820+`、`synapse-services/src/room/service.rs:659-684`（`08d014626`）。**`un_soft_fail` 0 命中**；本仓 `API_COVERAGE_REPORT.md` 把上游 #20204 的语义声明为 N/A ⇒ 需产品决策 |
| 12 | MSC4291 + 第三方 `check_event_allowed()` | ✅ **已实现（2026-10-02 合流后，D-1）** | 钩子落地为 `EventAdmissionGate` + 共享踏板 `consult_event_admission`（`synapse-services/src/module_service.rs:135-225`），在**状态变更前**咨询：本地两咽喉（`synapse-services/src/room/messaging/events.rs:1365`）与 membership 本地/联邦写入点，并覆盖房间创建序列（`synapse-services/src/room/lifecycle/create_events.rs:89`）与 burn-after-read 的 `m.room.redaction`。触发侧 `third_party_rules` 配置经 `ModuleService::register_configured_third_party_rules` 启动注册；规则可携 `modification`，异常 **fail-closed**（对齐上游 v1.49.0 #11033）；改写仅作用于本地事件（联邦入站 `allow_modification=false`）。用例 20/20 通过（`event_admission_gate_fails_closed_when_a_rule_errors`、`create_event_denied_by_admission_rule_returns_forbidden`、`register_configured_third_party_rules_wires_config_into_the_gate` 等）。**集成层已补（2026-10-02）**：`tests/integration/third_party_rules_event_paths_tests.rs` 的 7 个用例驱动**真实 HTTP 面**（客户端发送路由 + 签名联邦 `/send`），全部通过 —— 并因此暴露一个**真实缺陷，已修**：发送路由把准入门的 `403 M_FORBIDDEN` 一律包成 `500 M_UNKNOWN`（`synapse-services/src/room/messaging/messages.rs` 里包住 `create_event` 的 blanket `map_err(... internal_with_cause ...)`），**策略拒绝被误报成服务器故障**；现在只对非客户端错误加 "Failed to send message" 上下文。该集成层另钉死两条语义：① 房间创建序列同样被 gate 覆盖（规则可拒绝 `m.room.create`）；② 联邦入站 PDU **不应用** `modified_content`（PDU 已由源服务器签名 + 内容哈希，改写会与签名失配）。本仓 MSC4291 = room-v12 的 create-id 规则（`synapse-federation/src/event_auth/rules.rs:14,60,64,150`） |
| 13 | `/room_summary` 陈旧 `join_rules` | ✅ **已核对并加回归网**（2026-10-01） | 反规范化列 `synapse-storage/src/room_summary/repository.rs:59,131`（`join_rules AS join_rule`）。**`should_update_summary = tx.is_none()`（`room/messaging/events.rs:182`）跳过的只是「事务内写入的就地刷新」，而今天唯一的事务内状态写入是 createRoom 的 `initial_state`，它有两条独立补偿：①提交前把 `m.room.join_rules` 显式投影进 summary 请求（`room/lifecycle/create.rs:435-470,503`）；②`create_summary` 末尾的 `synchronize_room_snapshot`（`room/summary/service.rs:169` → `state.rs:90-92`）从**已提交状态**重算 `join_rule`。其余状态写入（客户端 `PUT /state`、成员变更、联邦入站）`tx=None`，就地刷新。两条集成用例锁定该不变量并已做**变异红证明**：`api_room_summary_routes_tests.rs::summary_join_rule_follows_client_state_write`（移除 state→summary 投影即红）、`…_initial_state_join_rules`（同时移除两条补偿即红）。**残留契约**：未来新增事务内状态写入时，调用方必须在提交后自行触发 summary 刷新（先例即 createRoom） |
| 14 | 递归 `/relations`（MSC3981 `recurse`） | ✅ **已实现（2026-10-02，M-6）** | 规范（_proposals_：`proposals/3981-relations-recursion.md`，**spec v1.10 起稳定**）：`recurse`（不稳定名 `org.matrix.msc3981.recurse`）为 true 时纳入「关系的关系」；事件**始终**按拓扑序（与同 `dir` 的 `/messages` 一致），分页与 limit 也作用于该序；传了 `recurse` 就必须回 `recursion_depth`；`/versions` 广告 `org.matrix.msc3981`。本仓现状：两条路径（recurse on/off）共用**一条静态递归 CTE**（`$6 = TRUE` 短路递归项 ⇒ 缺省路径只出直连），排序键与 keyset 游标改在 `events.stream_ordering` 上 ⇒ 合规客户端不再 400、序为拓扑序；深度口径与上游一致（0 基 `depth <= 3`，上报 3）。**两处披露**：① 过滤语义取**参考实现**（先递归、后过滤返回集）——MSC 正文那句「过滤同时剪枝中间节点」与 MSC 自己的第 5 个示例自相矛盾，Synapse 的 CTE 是前者，引用见 `MSC_SEMANTICS.md` §1.1；② ~~`event_type` 过滤无 HTTP 入口~~ → **已补（L-6，2026-10-02）**：spec 的 4 段 `GET …/{relType}/{eventType}` 已注册，契约链同步，判据见 §18.4 L-6 |
| 15 | 本地缩略图异步 | ✅ **已具备** | `synapse-services/src/media_service.rs:517-520`（解码/缩放在 `spawn_blocking`；另见 `:869`） |
| 16 | `synapse_storage_stream_current_position` | ✅ **已 per-stream（2026-10-02，L-1）** | 改成**同名多标签** gauge（`{stream="…"}`）：此前 `MetricsCollector.gauges` 以 name 为 key，同名不同标签会互相覆盖，故先补 `DynamicGaugeTemplate`（与既有 counter/histogram 模板同构）。标签集合 = `StreamPosition::ALL`（`events` / `to_device` / `device_lists` / `sliding_sync` / `quarantined_media` / `worker_events`），数据源是 `synapse-storage/src/stream_positions.rs` 的**单条 UNION ALL 查询**；刷新从 admin `/statistics` 的按需更新改为 `src/server/mod.rs` 30s 指标循环周期刷新；判据 V-12 落地两条集成测试（标签集合与登记表逐字一致 + 每个 stream 随写入推进，含红证明）。**未覆盖**：upstream 的 presence/typing/receipts/account_data/push_rules/e2ee/backfill/federation 在本仓**没有位置列**；`sync_stream_id` 只有 seed 写入（全仓无读/写者）；`device_lists_outbound_pokes` 在生产代码里**既无 INSERT 也无 SELECT** ⇒ **已按铁律 1 整表删除**（2026-10-03，见 §18.7.1 P-13）；`worker_events` **已接线**（P-5：worker 模式下由事件写入装饰器发布，单进程部署恒 0 属预期而非漂移）；`room_ephemeral.stream_id` **不是 stream 位置** —— 它是按房维度的新鲜度排序键（UPSERT 覆盖、仅用于 `ORDER BY stream_id DESC`），服务层从不读取它、也不做游标 ⇒ **永久排除**。**worker-local** 未做：本仓位置取自 DB（写者无关），per-stream 已足，而刷新循环只在 global-maintenance owner 中运行 ⇒ 按 `stream_writers` 过滤反而会让 worker 部署少几条序列；仪表盘/监控文档同步登记为 §18.4 L-1b。**本轮修复的真实缺陷（合流时保留）**：per-stream 落地后，`to_prometheus_format()`（`synapse-common/src/metrics.rs:747`）必须**按指标族去重** `# HELP`/`# TYPE`；动态模板（`DynamicGaugeTemplate::set`）把**每个标签组合**存成独立条目，逐 entry 输出会让同一族出现多份元数据行。`room_operations_total` / `cache_operations_total`（`synapse-common/src/server_metrics.rs:417,421`）早已如此。Prometheus 文本格式规定 _「Only one `HELP` line may exist for any given metric name」_（[exposition formats](https://next.prometheus.io/docs/instrumenting/exposition_formats/)），重复元数据行会让**整个 scrape 被拒绝**。修复与两条用例（`test_to_prometheus_format_dedups_help_and_type_for_dynamic_gauge_family` / `…_counter_family`）已随合流入库 |
| 17 | 当前状态缓存 / 状态决议缓存 | ✅ **已对齐（2026-10-01 更正 + 补齐）** | sliding-sync 整房状态缓存 `room_state:{room_id}` TTL 300（`synapse-services/src/sliding_sync_service/state.rs:20-36`）；**更正**：MSC4297 v2.1 本体**已在生产路径上**（`create_event_with_graph` → 提交后 `StateRecordBuilder` → `resolve_state_for_version_with_rules` → `full_conflicted_set` → `conflicted_state_subgraph`），此前据一条过时注释判该函数为死代码**属误判**（注释已修并加「不得当死代码删」告诫）；**补齐**：新增按冲突事件集合为键的结果缓存，**作用域＝进程内跨请求共享**（`ResolutionCache`，`Arc<Mutex<…>>`，`synapse-services/src/room/state_record.rs:92-135`；由 `room/service.rs` 创建并 clone 穿入 messaging/membership 配置，跨 walk 共享由 `resolution_cache_is_shared_across_walks` 锁定，见 §18.4 M-5） |
| 18 | MSC4242 HTTP 服务函数 | ⏸ **已裁定不接线（2026-10-02，L-2）** | `synapse-storage/src/event/dag.rs:255-290`、`synapse-storage/src/event/create.rs:257-281`；无路由/handler/service（上游 #20133 是"为未来 MSC4242 加 HTTP serving 函数"） |
| 19 | 委派认证（MSC3861/MAS） | ✅ **已接线（2026-10-01，决策 D-1）** | 接线点 `auth/mas_validator.rs::build_mas_validator`（配置 → `OidcMasTokenValidator`），由 `container.rs` 在 `AuthService` 构造后按配置注入；启动期 `Config::validate()` 拒绝半配置（`mas.enabled` 必须有 `issuer_url` + `client_id`）；`verify_access_token` 在 `client_id` 非空时**校验 audience**（本仓无 introspection 绑定，`aud` 是唯一阻止跨客户端令牌混用的锚点）；`auth/token.rs` 既有 fail-closed 语义（MAS 返回 `Err` ⇒ 直接失败，**不回落**本地 HS256；`Ok(None)` 才回落）。证据：`build_mas_validator_requires_full_configuration`、`mas_rejection_never_falls_back_and_non_mas_falls_through`、`access_token_with_wrong_audience_is_rejected`（含"关掉校验即红"的变异证明）、`validate_rejects_mas_enabled_without_issuer_or_client_id` |
| 20 | 单个 delayed event GET + worker 归属 | ✅ **已实现**（端点）+ **显式路径集与两道守卫（2026-10-02，L-3，`b30bf1997`）** | `synapse-web/src/routes/delayed_events.rs:41-61,98`、`synapse-services/src/delayed_event_service.rs`；worker 归属原先只有前缀级 `/_matrix/client/*`（`synapse-storage/src/worker/models.rs:81-100`），**L-3 已在 worker 拓扑校验器侧补显式路径集**：`synapse-services/src/worker/topology_validator.rs` 的 `DELAYED_EVENTS_WORKER_PATHS`（枚举自 route ledger）+ 精确匹配闸门 `may_serve_delayed_events_route`（非前缀），消费方 `WorkerResponse.delayed_events_paths`（`synapse-web/src/routes/worker.rs`），两道守卫见 `tests/unit/worker_delayed_events_ownership_tests.rs`；**未改 `RouteEntry`**（`route_ledger.rs:77-95` 仍无 worker 字段），契约链零改动 |

**净结论（2026-10-02 合流后重述）**：**v1.162.0 的 4 条功能 4/4 已落地**（房间版本 / OTK 上限 / `rc_profile` / `redis.username`——第二轮复核把 #3 由「部分」更正为「已对齐」）；
13 条修复里 **9 条已落地或已按规范重做**（profile 空字段 / `allowed_room_ids` / `M_UNKNOWN_DEVICE` / 历史 user id / 缩略图异步 / MSC4311 双形状 / MSC4222 `state_after` / room_summary `join_rules` / MSC4291+第三方准入钩子），
**3 条为有意的功能面/口径差异**（递归 relations 的过滤语义取参考实现、MSC4354 的 un-soft-fail 裁定不跟随、`available` 只列可创建版本），
**1 条仅存储层且已裁定不接线**（MSC4242，L-2）；内部项里 stream 指标**已 per-stream 且修掉 `# HELP`/`# TYPE` 重复缺陷**，状态决议缓存**已升级为进程内跨请求共享**。

**本轮复核另发现的自身问题（已修，见 §18.7）**：
① **文档计数守卫曾判红** —— 本文件三处写 1,152 而 `ROUTE_CONTRACT.md` 为 1,154（L-6 新增 2 条）⇒ `tests/unit/doc_credibility_guard_tests.rs::counts_match_the_route_contract` 失败；合流时已同步（**"守卫存在"不等于"守卫在跑"**）。
② **两条并行线各自实现同一批条目**（M-4 / M-5 / L-1 / L-6），外加第三份未提交的准入钩子实现；已于 2026-10-02 合流并按 §18.7 去重。

### 18.4 真实剩余差距（分优先级）

**高（正确性 / 互操作 / 死代码）**

| ID | 改动内容 | 影响范围 | 兼容性风险 | 依赖 |
|---|---|---|---|---|
| **H-1** | ~~委派认证（MSC3861/MAS）运行时未接线~~ → **✅ 已收口（2026-10-01，决策 D-1＝接线）**：`build_mas_validator` 成为唯一接线点并由 `container.rs` 注入；启动期拒绝半配置；`client_id` 非空时校验 audience；fail-closed 回落语义有用例锁定。剩余可选项：`mas_rest_client.rs` 的 admin REST 面仍无调用方（与 token 校验无关，单独评估） | 见 §18.3 #19 | — | — |
| **H-2** | ~~MSC4311 宽限开关是死配置 + knock stripped state 缺失~~ → **✅ 已收口（2026-10-01）**：开关接入入站 `/invite` 的 `invite_room_state` 形状校验（宽限双形状 / 严格只收 full PDU，含 2027-06-01 期限文案）；`/sync` 新增 `rooms.knock` + `knock_state`（并修掉 knock 被当作 joined 渲染的缺陷）。**遗留**：与真 Synapse v1.162 的双向 `/invite`、`/knock` 互操作验证仍需在联调环境跑（V-2 下半段，本地无法覆盖） | 见 §18.3 #8 | — | — |

**中（规范/一致性/性能）**

| ID | 改动内容 | 影响范围 | 兼容性风险 | 依赖 |
|---|---|---|---|---|
| **M-1** | ~~`redis.username` 补上游的「username 必须配 password」启动校验 + env 解析~~ → **✅ 已交付（2026-10-02，M-1）**：`synapse-common/src/config/validation.rs:82-88`（配了 username 却没 password ⇒ 启动返回 `Err`）+ `loader.rs:76`（username 与 password 走同一条 `${VAR}` 插值路径）；判据 `validate_rejects_redis_username_without_password`、`env_override_needs_a_double_underscore_after_the_prefix` | `synapse-common/src/config/database.rs`、`loader.rs` | — | 需确认上游 `password_path` 等字段是否也要一并对齐（本仓无） |
| **M-2** | ~~MSC4222 批次边界~~ → **✅ 已收口（2026-10-01）**：取证确认上游那处"批次边界"修复在本仓无对象（逐事件唯一 `stream_ordering` + 状态窗口上界无界），真正缺口是 MSC 被借用 ⇒ 已按规范形状重做并删除借用参数与左房过滤 | 见 §18.3 #10 | — | — |
| **M-3** | `/room_summary` 的 `join_rules` 反规范化列刷新时机 —— **本轮已收口，无行为改动**（复核结论：现状由两条独立机制保证新鲜，见 §18.3 #13）。交付物是**回归网 + 契约**：两条集成用例（含变异红证明）+ 未来事务内状态写入的补偿义务记录 | `tests/integration/api_room_summary_routes_tests.rs`、`room/messaging/events.rs:182` 的契约注释 | **无**：测试与注释 | 无 |
| **M-4** | ~~第三方规则回调接入事件鉴权/消息发送路径~~ → **✅ 已交付（2026-10-02 合流后，D-1）**：`check_event_allowed()` 以 `EventAdmissionGate` + 共享踏板 `consult_event_admission` 落地（`synapse-services/src/module_service.rs:135-225`），本地两咽喉 + membership 本地/联邦写入点 + 房间创建序列（`synapse-services/src/room/lifecycle/create_events.rs:89`）+ burn-after-read 全部在状态变更前咨询；触发侧 `third_party_rules` 配置启动即注册；失败语义取 **fail-closed**（对齐上游 v1.49.0 #11033），内容改写仅本地生效（联邦 `allow_modification=false`）。**合流去重**：本仓只保留这一份实现（feature 分支的 `EventAdmissionGate`）。`c19b` 工作树里那份未提交的**第三份**实现（`ThirdPartyRuleRegistry` / `run_third_party_rules`）已**由该工作树的写者自行清理**（2026-10-02 18:07 起 `git status` 干净、无新提交、无 stash；`git grep ThirdPartyRuleRegistry` 在所有 ref 上只命中本文件的历史记载与抢救后的用例别名）。复核期间该写者一直在写入（`module_service.rs` mtime 16:24 → 17:03 → 17:29），按铁律 9 未打断在途工作 —— 事后核对证明这是对的：那份工作里**独有的集成用例已被抢救**（`tests/integration/third_party_rules_event_paths_tests.rs`，7 passed），否则会随清理一起消失 | 见 §18.3 #12、§18.7 | — | — |
| **M-5** | 状态决议结果缓存（按冲突事件集合为键） → **✅ 已交付（2026-10-01）**：`StateWalker` 内新增结果缓存，键 = 状态集合的 `(key, event_id)` 投影（排序归一） + 已加载事件 id 集合（`events` 只插入不覆盖 ⇒ id 集合即内容标识）；上限 64、超限清空（正确性从不依赖命中）；HIT/MISS 计数器供用例断言 | `synapse-services/src/room/state_record.rs` | **低**：纯性能，不改判定结果 —— 用例断言 HIT 与 MISS 返回**逐字节相同**的状态图；**红证明**：把状态集合从键里去掉 ⇒ 用例红（`(3, 1)` vs `(2, 2)`，即错误命中） | **✅ 作用域已升级为进程内跨请求共享**（`aee2f098e`，2026-10-02 合流保留）：`Arc<Mutex<ResolutionCache>>` 跨 walk 复用，跨房共享靠键补 `room_version` 槽位 |
| **M-6** | ~~`/relations` 支持 `recurse`~~ → **✅ 已交付（2026-10-02）**：`recurse` + 不稳定名接线（此前 `deny_unknown_fields` ⇒ 合规客户端 400）；存储层**一条静态递归 CTE**同时服务两条路径（`events` 在递归内 join，`$6 = TRUE` 短路递归项），排序与 keyset 分页改在 `events.stream_ordering`（**两条路径都**拓扑序，MSC 要求「无论 recurse 取值」）；深度与上游一致（0 基 `depth <= 3`，上报 `recursion_depth = 3`）；`/versions` 广告 `org.matrix.msc3981`(`.stable`)；`.sqlx` 增量同批（R2）；判据 V-11 已落地 | `synapse-storage/src/relations/mod.rs`（`MSC3981_RECURSION_DEPTH`、`OrderedEventRelation`、`get_relations`）、`synapse-services/src/relations_service.rs`、`synapse-web/src/routes/relations.rs`、`synapse-services/src/capability_governance.rs` | **低**（已验证）：缺省路径的**结果集**不变（仍只出直连），但排序键由 `origin_server_ts` 换成 `stream_ordering` —— 这是 MSC 的硬性要求（「始终拓扑序」），已同步本节措辞与测试；过滤语义取参考实现而非 MSC 正文，披露见 `MSC_SEMANTICS.md` §1.1 | 无 |

**低（可观测/配置/文档/跟随上游）**

| ID | 改动内容 | 影响范围 | 兼容性风险 | 依赖 |
|---|---|---|---|---|
| **L-1** | ~~stream position 指标做成 per-stream / worker-local~~ → **✅ 已交付（2026-10-02，L-1）**：指标改为**同名多标签** gauge（`{stream=…}`；先补 `DynamicGaugeTemplate` 修掉「同名互相覆盖」）+ 5 个 stream 的单一 `UNION ALL` 数据源 + 30s 周期刷新（不再依赖 admin `/statistics` 被访问）；判据 V-12 两条集成测试（含变异红证明）。**未实现** worker-local 归属过滤，理由见 §18.3 #16（本仓位置取自 DB，刷新循环只在 global-maintenance owner 跑，过滤反而会让 worker 部署少序列） | `synapse-common/src/server_metrics.rs`、`synapse-web/src/routes/admin/server.rs`、各 stream 读侧 | — | ✅ **已闭合（2026-10-02 合流）**：stream 全集 = `StreamPosition::ALL`（5 条，见 §18.3 #16），数据源单一 `UNION ALL`（`synapse-storage/src/stream_positions.rs`）。合流时**删除了另一线的第二份实现**（抓取时 `refresh_storage_stream_positions` + 2 条序列），只保留 30s 循环这一份；另一线的 `# HELP`/`# TYPE` 去重修复已并入 |
| **L-2** | ~~MSC4242 的 HTTP 服务函数~~ → **⏸ 已裁定不接线（2026-10-02）**：提案不在 `main`（PR #4242 open/unmerged/`needs-implementation`）、**无房间版本指派**、上游联邦侧 PR #19425 关闭未合并，且其"HTTP 面"只是既有联邦路由上的 `state_dag` 旗标（非 CSAPI）⇒ 现在实现＝落地未定稿规范。随本裁定修掉 `dag.rs` 的不实注释（唯一一处假注释） | 本文件 + `synapse-storage/src/event/dag.rs` | — | — |
| **L-3** | **✅ 已交付（2026-10-02，`b30bf1997`）**：显式路径集 `DELAYED_EVENTS_WORKER_PATHS`（枚举自 route ledger，非手抄）+ 精确匹配闸门 `may_serve_delayed_events_route`（明确不是前缀）+ 真实消费方 `WorkerResponse.delayed_events_paths` + 两道守卫（A 每条路径必须已注册；B 所有含 `delayed_events` 的已注册路由必须被覆盖；含变异红证明与精确性负控）；**零契约链改动**（未给 `RouteEntry` 加字段） | `synapse-services/src/worker/topology_validator.rs`、`synapse-services/src/worker/mod.rs`、`synapse-web/src/routes/worker.rs`、`tests/unit/worker_delayed_events_ownership_tests.rs` | 无（D-3=② 已落地） | — |
| **L-4** | ~~MSC4354 状态变化时 un-soft-fail~~ → **⚪ 已裁定不跟随（2026-10-01，决策 D-2）**：本仓无 soft-fail 机制（`un_soft_fail` / `StickyEventsStream` 全仓 0 命中），"跟随"＝新建整套 soft-fail（新功能而非对齐），且会与本仓 `events.soft_failed`（B-8 事务去重，**同名不同义**）混淆。结论落档即可，无代码改动 | 本文件 + `MSC_SEMANTICS.md` §1.1 | — | — |
| **L-5** | ~~本文档去陈旧化~~ → **✅ 已收口（2026-10-01）**：§11.2/§11.3/§12.4/§12.5/§15.3 五个历史章节各插入一条「当前口径见 §18」指针（正文按约定不改写），并把它变成门禁 (`doc_credibility_guard_tests::historical_sections_point_at_the_current_scope` + 纯谓词红证明，含「章节被改名 ⇒ 守卫失效」的检测) | 本文件、`tests/unit/doc_credibility_guard_tests.rs` | — | — |
| **L-6** | ✅ **已完成（2026-10-02）**：spec 稳定路由 `GET /_matrix/client/{v1,v3}/rooms/{roomId}/relations/{eventId}/{relType}/{eventType}` 已注册（与 PUT 合并为同一条 MethodRouter，4 段参数名统一取 `{event_type}`），`event_type` 过滤端到端打通；三个读路由（2/3/4 段）收敛到单一 `relations_response`。契约链全同步：标注 + `EXPECTED_ANNOTATIONS` + 派生表（1156 行）+ `ROUTE_CONTRACT.md`（1154 路由）+ 6 份 ledger fixture（golden=default、SDK=all-extensions 两车道分别真导出）+ `docs/openapi/route-table.json`（1050→1052）+ 两条 route-ledger 快照 | `synapse-web/src/routes/relations.rs`、`scripts/contract/*`、`docs/*`、`tests/*` | **低**（已验证）：判据见下 | 判据：`-E 'test(route_ledger)'` **15 passed**（带/不带 `UPDATE_ROUTE_LEDGER_SNAPSHOTS` 各一次）、`-E 'test(ledger_export)'` **7 passed**、`check_route_contract.sh` **提交后 EXIT=0**、clippy 两档 0、fmt 0/0、sqlx/ts_order/trait 棘轮 OK。HTTP 往返判据已补（`relations_event_type_route_filters_over_http`：匹配 / 不匹配 / 3 段 三断言 + 变异红证明） |
| **L-1b** | ✅ **已完成（2026-10-02）**：文档半边（`docs/monitoring/monitoring-ops-guide.md` 的 per-stream 小节 + `API_COVERAGE_REPORT.md` 三处旧描述）+ **Grafana 面板**（`monitoring/grafana/dashboards/grafana-dashboard-business.json` 新增 "Stream 位置（per-stream）" timeseries，`expr=synapse_storage_stream_current_position`、`legendFormat={{stream}}`，刷新周期写在面板描述里） | `monitoring/grafana/dashboards/grafana-dashboard-business.json`、`docs/monitoring/*`、`docs/synapse-rust/API_COVERAGE_REPORT.md` | — | — |

### 18.5 本轮作废 / 更正的历史声明（"假缺口"）

**(a) 历史正文里"已修却仍写缺失"的条目**（会误导排期）：

| 原声明（节 · 行） | 实际状态 | 证据 |
|---|---|---|
| §11.2 / §12.4 Content Scanner"未装配" / §15.3"孤儿模块" | **已装配且已接入生产路径**（旧"无调用点/孤儿"判定作废）：媒体上传与房间消息文本两路扫描均已接线，是否实际扫描由 `content_scanner` 配置开关决定 | 装配 `synapse-services/src/wiring/core.rs:66,180`；消费 `synapse-web/src/routes/media/upload.rs:89,137`（`scan_when_enabled`）、`synapse-web/src/routes/handlers/room/events.rs:327`（`scan_text_when_enabled`） |
| §12.4 App Service 登录"整体缺失" | AS 登录**已实现** | §12.5 L931 |
| §12.5 SAS"高" | 该面**已整模块删除**（去服务端私钥重构），条目作废 | §15.3 L944 同行的"作废"标注 |
| §11.3 / §12.5 QR"当前为桩" / `leak_detection`"未编译" / OIDC `validate_id_token_claims`"死代码" | 三者均**已删除** | §7.2 L387；§14.1 L71 |
| §12.5 MSC4140"无法联邦/EDU" | EDU **已实现** | §12.4 L718 |
| §12.5 MSC4242"❌ 缺失" | **已进 v12 图路径**（存储层），仅 HTTP 面缺（§18.3 #18）；**取证更正（2026-10-02）**：上游 #20133 是把 MSC4242 接进**既有**联邦端点（`/make_join`、`/send_join`、`/get_missing_events`、`/send` 目的地），**不新增路由**，受阻于 MSC4242 房间版本 + experiment（§18.4 L-2） | `synapse-storage/src/event/dag.rs:255-290` |
| §12.5 MSC4512"缺失" | 代理**已实现** | §12.5 L931 |
| §12.4"撤回格式与默认房间版本不匹配""MSC3912 未实现" | **均已修**（默认版本 12 + MSC3912 级联） | §15.3 L945/L719 |
| §15.3 "v12/v13 不可创建" | v12 **是唯一可创建**版本，v13 已移出能力表 | `room_versions.rs:94,119-152` |
| §11.1 "本仓默认房间版本 11 / 注释过时" | **已订正**：本仓默认创建版本已为 **12**（v12 唯一可创建）；旧注释亦已修 | `synapse-common/src/room_versions.rs:94,152` |
| §15.3 "Admin 媒体缺 4 条" | §16.3 L1022 记**已补全 18/18**，但 §17 L1054 仍写"剩余 4 条" | §16.3 L1022 vs §17 L1054 |
| §17"紧急：U-19-R4" | 同行括注已写"已在 `5d1bfdc3f` 修复" | §16.3 L1017 |
| TOC | §16 重复、§17 缺失（本轮已修） | 本文件目录 |

**(b) §18 初稿自身的更正（按旧基线 `c19b`@`64a015a8d` 误判为"缺失"，实已在 `e1ffcb2ab` 落地）**：

| 初稿误判 | 实际（`e1ffcb2ab`） | 提交 |
|---|---|---|
| OTK 500 上限缺失 | 已实现 | `d409bd89b` |
| `redis.username` 完全没有 | 字段/URL 已有，仅缺校验与 env | `ede782382` |
| `rc_profile` 缺失 | 已实现（burst 5 vs 上游 500） | 同一批 |
| profile 空字段返回 `""` | 已改 `{}` | `ede782382` |
| hierarchy 无 `allowed_room_ids` | 已实现 | `ede782382` |
| `M_UNKNOWN_DEVICE` 缺失 / 历史 user id 未过滤 | 均已实现 | `d409bd89b` |
| MSC4354 完全缺（含 EDU） | EDU/存储/注入已实现，仅 un-soft-fail 缺 | `08d014626` + `5df920848` |
| stream 指标缺失 | gauge 已注册（口径不同） | `18a07b2c1` |

> 处理约定：正文（§1–§17）**不改写**（保留历史轨迹），以 §18 为唯一现状口径。
> **2026-10-01 起这条约定由门禁强制**：§11.2 / §11.3 / §12.4 / §12.5 / §15.3 五个历史章节的标题后
> 必须紧跟一条「当前口径见 §18」的指针，否则
> `doc_credibility_guard_tests::historical_sections_point_at_the_current_scope` 判红
> （谓词 `stale_section_violations`；红证明 `the_stale_section_checker_rejects_a_missing_pointer`
> 同时覆盖「章节被改名 ⇒ 守卫静默失效」这一形态）。后续若彻底重构，应把 §11–§12 的「当前状态」列
> 整体替换为指向 §18 的引用，而不是继续叠加轮次章节。

### 18.6 需要额外验证或测试的关键点

| # | 验证点 | 方法 / 判据 |
|---|---|---|
| V-1 | **MAS/委派认证接线**（H-1）：MAS 令牌可用、非法令牌 fail-closed | **已落成用例**：`cargo nextest run -p synapse-services --lib --features test-utils -E 'test(/access_token_with_wrong_audience\|mas_\|build_mas_validator/)'`；`cargo nextest run -p synapse-common --lib -E 'test(/mas_enabled_without_issuer/)'`。接线点复核改为**行为断言**（`build_mas_validator` 四态）而非 grep —— grep 正是上次漏掉它的原因 |
| V-2 | **MSC4311 语义**（H-2）：入站对 stripped 与 full PDU 两种 `invite_room_state` **都接受**；宽限阈值行为正确；knock stripped state 进入客户端 `/sync` | **已落成用例**：`cargo nextest run -p synapse-web --lib --features test-utils -E 'test(msc4311)'`（3 passed，含 strict 红证明路径）与 `cargo nextest run -p synapse-services --lib --features test-utils -E 'test(knock_stripped)'`（含 knock 分类与 `knock_state` 键）。**联调已跑（2026-10-03，跨实现 A=synapse-rust v12 / B=上游 Synapse v1.162.0；栈见 `docker/federation-test/`、判据 `scripts/federation-test/test_federation_v2v7.sh`，结果 `artifacts/federation-interop/v2v7_result.json`）**：`V-2a` A→B 邀请 ✅、`V-2c` 入站 knock ✅、`V-2d` ✅（strict=false 走 DAG 守卫、strict=true 走形状校验 ⇒ **开关真被读取**）、`V-7a`/`V-7b` 见 V-7 行。**`V-2b` B→A 邀请 ✅（`c13d0ebec` 后联调 failures=0）—— 四层问题逐层剥开、全部修掉**：① 上游 `403 Not signed by synapse-b.federation.test` —— 入站 invite 回显的 PDU 只有本机签名（**P-15，已修**）；② 修完签名后暴露内容哈希不一致（v3+ PDU 多带非规范 `origin`，上游会静默 redact —— **P-16，已修**，修后 B 日志不再有 `tampered/redacting`，且 A→B/V-2c/V-2d/V-7 全部无回归）；③ 邀请在 A 的 `/sync` 里根本不被收集（`get_sync_rooms` 排除 `invite`/`knock` —— **P-17，已修**，本地邀请现已在 `/sync` 的 `rooms.invite` 里带 `invite_state`）；④ 首次接触的联邦邀请没有落 `invite_room_state`，按 MSC4311 fail-closed 被移出 invite 段（**P-18，已修**：入站在邀请事件的 `unsigned.invite_room_state` 保存发送方给的 stripped state、`assemble_room_stripped_state` 优先取它；出站补 `m.room.create`；`c13d0ebec`）。**判据自身的前置修复**：Python 3.13 `create_default_context()` 默认带 `VERIFY_X509_STRICT`，测试 CA 缺 `keyUsage=keyCertSign` 时V-2c/2d/V-7b 三个探针全部 `status=0`（与产品无关），已在 `generate-tls-cert.sh` 补齐 CA/叶证书扩展 |
| V-3 | `redis.username`：username-only 配置**启动失败**（对齐上游）；`${VAR}` 插值生效 | **已落成用例**：`cargo nextest run -p synapse-common --lib -E 'test(redis_username)'`（3 passed）与 `-E 'test(resolve_env_variables_resolves_redis_password)'`（1 passed）；另加 `env_override_needs_a_double_underscore_after_the_prefix` 钉死覆盖拼写必须是双下划线形式（README/CONTRIBUTING 原先写的单下划线形式静默不生效，已修正） |
| V-4 | MSC4222：opt-in 客户端拿 `state_after`（含空）、默认客户端形状不变、旧参数不再生效 | **已落成用例**：`cargo nextest run -p synapse-services --lib --features test-utils -E 'test(msc4222)'`（4 passed）与 `cargo nextest run -p synapse-web --lib --features test-utils -E 'test(msc4222)'`（2 passed）；`cargo nextest run --all-features --test integration -E 'test(sync_authenticated_initial)'` 默认路径快照零 diff |
| V-5 | `/room_summary` `join_rules` 新鲜度：join_rules 变更后立即查询返回新值 | **已落成用例**：`cargo nextest run --features test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,cas-sso,saml-sso --test integration -E 'test(summary_join_rule_follows)'` → 2 passed；红证明见 §18.3 #13 行（移除 state→summary 投影 / 移除创建补偿） |
| V-6 | OTK 上限边界：恰好 500 通过、501 拒绝、`M_TOO_LARGE`、不落库 | 复用 `synapse-e2ee/src/device_keys/service/tests.rs:283+` 的用例风格补边界 |
| V-7 | `allowed_room_ids` 语义：restricted / knock_restricted 房间返回 allow 房间集合；联邦面不新增泄漏 | `GET /_matrix/client/v1/rooms/{roomId}/hierarchy` 集成断言 + 联邦响应字段白名单测试。**联调已跑（2026-10-03，同上跨实现栈）**：`V-7a` 客户端 `/hierarchy` ✅ —— A 建 public space + `join_rule=restricted` 且 `allow=[{type:m.room_membership, room_id:<space>}]` 的子房，子房条目带 `allowed_room_ids` 且包含该 space；`V-7b` 联邦面 `/_matrix/federation/v1/hierarchy/<space>` ✅ —— 200 且响应里**没有** `allowed_room_ids` 键（该字段是客户端专用投影，不泄漏）。**V-7a 首跑命中真实缺陷（新记 P-14，已修）**：`allow[].room_id` 被 `room/join_rules.rs` 自带的「必须带冒号」校验全部丢弃，而本仓建房产出的正是 room v12 无域名 ID ⇒ `allowed_room_ids` 恒为空，且 `membership/service.rs` 的 restricted join 授权闸门**永不授权**；已删掉第二份语法、统一委托 `synapse_common::room_id::is_well_formed_room_id`（提交 `97fd35130`，红证明 `extract_allow_accepts_domainless_room_v12_ids` 先 `left: []` 后绿） |
| V-8 | profile `{}` 变更的客户端影响 | **仓库内部分已闭环（2026-10-03）**：`GET /profile/{userId}/{field}` 未设置字段返回 `{}` 的契约（`account_compat.rs::single_profile_field`，`ede782382`）此前**只有假覆盖** —— `tests/unit/account_compat_route_tests.rs` 的两条用例自造 `json!({})` 再断言它自己没有 `displayname` 键，从不调用被测函数（把实现整体删掉它们仍然绿）；现由模块内 `single_profile_field_tests` 真覆盖（已设置 / 空串 / `null` / 缺键 / 非字符串 / 只投影被请求字段），外部两条假用例已删除并指路。**仓库外部分无法在此验证**：本仓没有 TypeScript SDK/前端代码，`spec/unit/account*.spec.ts` 与相关快照不存在 ⇒ 「是否把 `""` 当未设置」必须在 SDK 仓库核对，不能声称已验证 |
| V-9 | 历史 user id 过滤不误伤本地用户 | 用非合规 localpart 构造 `m.device_list_update`；确认既有测试用户全部合规（`is_compliant_user_id_localpart`） |
| V-10 | 第三方规则接入事件鉴权后的失败语义（fail-open/closed）与性能 | **集成层已补齐（2026-10-02）**：`tests/integration/third_party_rules_event_paths_tests.rs` 7 用例驱动真实 HTTP 面（空规则不回归 / 本地拒绝 `403`+规则 reason / 规则 `Err` fail-closed / 本地改写落库 / **创建序列被 gate 拒绝** / 联邦 PDU 拒绝且不落库 / 联邦入站改写不生效）；服务层另有 20 用例。该层同时修掉了"发送路由把 403 包成 500"的缺陷。**性能：机制已查清，数字仍缺**。机制（读码确认）：`consult_event_admission`（`synapse-services/src/module_service.rs:190-240`）先走 `has_event_rules()` 快路径 —— **无规则时只取一次内存锁，可忽略**；**一旦注册了任何规则**，每次事件写入都会调 `event_reader.get_state_events(room_id)` 并**物化整房 state**（`synapse-storage/src/event/state.rs:116-143`：`current_state_group_id` + `state_events_of_group`，或退化路径的 `DISTINCT ON` 全表扫）来构造规则上下文，且**没有任何缓存**。这与已有的 `room_state:{room_id}`（TTL 300，失效由状态写入路径负责，`synapse-services/src/sliding_sync_service/state.rs`）**职责重复** ⇒ 是"同一职责只允许一份实现"的候选。**复用已交付（2026-10-03）**：抽出唯一实现 `synapse-services/src/room_state_cache.rs`（key / TTL / 失效契约的唯一登记处），`ModuleService::room_state_for_rules` 与 sliding-sync 共用**同一条目**；判据 `room_state_for_rules_is_served_from_the_shared_cache`（预置哨兵值 ⇒ 命中缓存即证明没有回源）与 `room_state_for_rules_populates_the_shared_cache_on_a_miss`。**p99 已测（2026-10-03，manual target，非 CI）**：`tests/performance/manual_smoke_tests.rs::admission_gate_send_latency_p50_p99`（`--features performance-tests,test-utils`），每组 40 次真实 `PUT /rooms/{room}/send/m.room.message`，先跑未注册规则的基线、再注册一条永远放行的规则并预热共享 `room_state` 缓存。**在裸 `create_router` 上测过两轮**（无规则 p50=3.26/2.68、p99=11.84/11.07 ms；规则开 p50=3.72/3.66、p99=5.35/8.26 ms；Δp99=−6.49/−2.81 ms），**随后把该 target 也切到生产装配并复测**（无规则 p50=6.00、p99=34.65 ms；规则开 p50=4.43、p99=13.76 ms；Δp99=−20.89 ms）⇒ 结论不变（规则路径没有可观测额外 p99，基线的 max==p99 是首个冷样本），但**绝对值差了一档**：裸装配少算的正是那六层（指标/Trace/超时/body 上限）的固定开销，p50 从 2.7ms 级变成 6ms 级 —— 这就是「测量必须测生产装配」的实例。用例只做结构性断言（每次发送必须 200），不做绝对延迟预算 —— 共享机器上必然 flaky。**仍缺**：规则**超时**护栏数字（fail-closed 意味着规则超时会拒消息，只能实测；本次探针的规则是同步立即返回，未覆盖超时路径）  **规则挂起/超时已量清并修复（2026-10-03）**：规则**没有自己的超时** —— 唯一的时限是生产层 `request_timeout_middleware`（`src/server/router.rs::build_router` 挂载；配置项 `server.request_timeout_secs` / `server.long_poll_request_timeout_secs`，缺省 30s/90s），所以挂起的规则会吃掉整个请求预算、客户端拿到与规则无关的 `408 M_REQUEST_TIMEOUT`（对入站联邦 `/send` 还会被来源反复重试）。本条第一次记成「该中间件从未接进路由器」，是只 grep `synapse-web/src` 导致的错误结论，已在 §18.7.1 P-19 更正。**已修**：`third_party_rules.rule_timeout_ms`（默认 2s）经 `EventAdmissionGate::event_rule_timeout()` 交给 `consult_event_admission`，超时按 fail-closed **拒绝**（`403`，reason 含 `no verdict within {n}ms`），写入之前返回。实测 `admission_gate_hanging_rule_is_refused_by_the_rule_deadline`：规则睡 30s，发送 **2.01s** 返回 `403 M_FORBIDDEN`、`/messages` 0 条。**覆盖情况**：装配缺口已闭环 —— 集成/手工压测夹具都经 `build_router` 装配，`production_request_timeout_bounds_a_hung_write` 直接断言该层（1s 预算 ⇒ 504 `M_REQUEST_TIMEOUT`）。 |
| V-11 | 递归 relations：`recurse=true` 不得 400；按 MSC 示例图（A←B/G 的 `m.thread`、A←D 的 `m.edit`、B←E 的 `m.annotation`，另有无关事件作对照）断言 `rel_type=m.thread` ⇒ `[B, G]`、`recurse=true&dir=f` ⇒ `[B, D, E, G]`、`recurse=true&dir=b&limit=2` ⇒ `[G, E]`；`origin_server_ts` 与 `stream_ordering` 刻意相反以证明拓扑序；深链第 6 跳缺席且 `recursion_depth=3`；`rel_type=m.annotation` ⇒ `[E]`（过滤作用于返回集）。⚠️ MSC 的**第 5 个示例**（`rel_type=m.annotation&event_type=m.reaction` ⇒ `[G, E]`）不可作判据：G 在图中只挂 `m.thread`，该期望与 MSC 自己的图矛盾且与 `dir=b&limit=2` 的结果逐字重复，属提案勘误 | 判据已落地：`cargo nextest run --profile ci --all-features --test integration msc3981`；`EXPLAIN (ANALYZE)` 证据在 §8.3 M-6 卡片（同形状夹具 50k 事件/50k 关系：基步与递归步的 `events` 都走 `events_pkey` 索引扫描，递归项 `Filter: (depth <= 3)` 循环 5 次后终止） |
| V-12 | stream 指标：per-stream/worker-local 口径（标签、刷新时机）与仪表盘一致性 | **2026-10-02 合流后重述**（原记的「2 条 series + `/metrics` 抓取时刷新」与测试名 `test_storage_stream_current_position_gauge` 均已作废 —— 那是被删除的第二份实现）：口径为 `StreamPosition::ALL` 的 **5 条** series、单一 `UNION ALL` 数据源、`src/server/mod.rs` 的 30s 循环刷新。判据：`tests/integration/stream_position_tests.rs` 的 `stream_labels_match_the_registry_exactly`（标签集合与登记表逐字一致）与 `every_stream_position_advances_with_writes`（随写入推进）—— **两条已实际跑通**（44 passed 批次内）；单元层另有 `synapse-common/src/server_metrics.rs::test_storage_stream_current_position_is_per_stream`。仪表盘：`monitoring/grafana/dashboards/grafana-dashboard-business.json`（L-1b 的 per-stream 面板）与 `docker/deploy/grafana/dashboards/storage-performance.json`；`tests/promql-queries.md` 同步。**worker-local 归属**未做且已裁定不做（理由见 §18.4 L-1） |
| V-13 | 本文档计数与假缺口清单 | 计数守卫已存在（`counts_match_the_route_contract`）；**§18.5(a) 条目不得再作为「现状」出现**已落成门禁：五个历史章节必须带 §18 指针 (`cargo nextest run --test unit --features test-utils -E 'test(doc_credibility_guard_tests)'`)。**2026-10-02 第二轮更正**：本行此前记的"6 passed"是**假绿**——L-6 把 `ROUTE_CONTRACT.md` 更新到 1,154 条后，本文三处仍写 1,152，`counts_match_the_route_contract` 实为**判红**。合流时已同步三处；**"守卫存在"不等于"守卫是绿的"**，该守卫必须进提交前脚本才有效（见 §18.7.3 P-3） |

### 18.7 双线合流记录与剩余问题（2026-10-02；闭环复核至 2026-10-03）

> 本轮把两条并行线合流：`opt/consolidated` @ `f8c45b73d` 合入 `feature/2026-10-01-metrics-docs-updates` @ `418d32d8b`
> （两线在 `46fe964b9` 分叉、互不为祖先）。合流在 feature 分支侧进行（该树 `git status` 干净、`target` 已热），
> **11 个文件冲突**全部逐项裁定。
>
> 2026-10-03 增补：把「需外部对端」的判据真正跑了 —— A = synapse-rust（room v12 无域名 ID）、
> B = 上游真 Synapse v1.162.0，跨实现双栈见 `docker/federation-test/`，判据脚本
> `scripts/federation-test/test_federation_v2v7.sh`，结果 `artifacts/federation-interop/v2v7_result.json`。
> 六项全数转绿，**沿 V-2b 一路剥出五个产品缺陷**：P-14（`allow[].room_id` 语法与本仓 v12 room ID 矛盾）与
> P-15（入站邀请回显的 PDU 发送方无法验签）已修；修完 P-15 才暴露 P-16（v3+ PDU 多带 `origin` ⇒ 对端静默 redact，已修）
> 与 P-17（`/sync` 永远看不到 invite/knock，已修）；**P-18**（入站邀请不落 `invite_room_state`）随后也已修（`c13d0ebec`），V-2b 收口、联调 failures=0。
> 另在 V-10 的「超时语义」上发现 **P-19**：规则**没有自己的超时**，挂起即吃掉整个请求预算（生产那层 `request_timeout` 是接了的 —— 我第一次只 grep `synapse-web/src` 就断言「从未接进路由器」，是错的，已在 §18.7.1 更正）。规则级超时已交付；顺带暴露的**测试装配缺口**（集成/压测走 `create_router`，六个生产层全被绕过）也已基本闭环（夹具改经 `build_router`/`production_app` 装配，见 §18.7.1 P-19）。

#### 18.7.1 问题闭环状态

| ID | 原问题 | 处置 / 证据 |
|---|---|---|
| **P-1** | 两条线各自实现同一批 backlog 条目（违反铁律 2） | 已完成反向合流；11 个冲突文件逐项取版本（§18.7.2） |
| **P-1b** | `c19b` 工作树里还有**第三份**未提交的准入钩子实现 | **已闭环（由该工作树的写者自行清理）**：2026-10-02 18:07 起 `git status` 干净、`HEAD` 仍是 `f8c45b73d`（无新提交）、`stash` 为空，`git grep ThirdPartyRuleRegistry` 在所有 ref 上只命中历史记载与用例别名。复核期间该写者仍在写入（mtime 16:24 → 17:03 → 17:29），按铁律 9 未打断 —— 事后证明正确：其**独有的集成用例已被抢救**（见 P-10），否则会随清理消失 |
| **P-2** | `/metrics` 同族重复输出 `# HELP`/`# TYPE`（Prometheus 拒绝整个 scrape） | family 去重修复 + 两条用例随合流入库（`synapse-common/src/metrics.rs`） |
| **P-3** | 文档计数守卫判红（1,152 vs 1,154） | 三处已同步；见 §18.6 V-13；且已把该守卫纳入 `scripts/quality/preflight.sh`（含红证明） |
| **P-4** | 同一条目两份实现（M-4 / M-5 / L-1 / L-6） | 各取一份，见 §18.7.2 |
| **P-5** | worker 事件总线**半接线**：读侧已暴露（`GET .../worker/events` → `WorkerManager::get_events_since`），但写侧**没有生产者** ⇒ `worker_events` 恒空、接口永远静默返回空列表 | **已按 (a) 补生产者（2026-10-02）**：新增 `synapse-services/src/worker/event_sink.rs`（`WorkerEventSink` trait + `WorkerManagerEventSink`），由 `NotifyingEventWriter` 在**每次已提交**的事件写入后发布（复用与唤醒通知同一个 `autocommit` 边界 ⇒ 事务内不发布，worker 不会被指向它还看不见的行）；接线在 `synapse-services/src/container.rs`，**仅当 `worker.enabled`** 时注入 （单进程部署不产生额外 INSERT）。`WorkerStorage::add_event` 改为**幂等**（`ON CONFLICT (event_id) DO UPDATE`）⇒ 重复发布返回原行、保留原 `stream_id`，worker 不会把同一事件当成新位置重放。**保留期**：新增 `prune_old_worker_events`（7 天，挂进既有 30s 维护循环）—— 接线前该表**没有任何清理路径**，不补就会变成无界追加表。**指标同步**：`worker_events` 从 `StreamPosition::ALL` 的排除名单移入登记表（第 6 条序列），`stream_positions.rs` 的排除理由与 SQL 同步更新。判据：`tests/integration/worker_event_bus_tests.rs`（幂等 + 保留期）与 `notifying_event_writer` 的单元用例（提交后发布一次 / 无 sink 不发布） |
| **P-9** | **集成层才暴露的缺陷**：客户端发送路由把准入门的 `403 M_FORBIDDEN` 包成 `500 M_UNKNOWN`（策略拒绝被当成服务器故障） | **已修**：`synapse-services/src/room/messaging/messages.rs` 只对非客户端错误加 "Failed to send message" 上下文；由 `tests/integration/third_party_rules_event_paths_tests.rs` 的 `test_m4_deny_rule_rejects_local_send_with_m_forbidden` 钉死（修前 500，修后 403 且带规则 reason） |
| **P-10** | 服务的 lib/unit 层从不覆盖真实 HTTP 拒签语义，故 P-9 长期不可见 | **已补**：`tests/integration/third_party_rules_event_paths_tests.rs` 7 用例（真实 HTTP：本地发送 + 签名联邦 `/send`），含空规则回归、`403`+reason、fail-closed、本地改写落库、**创建序列被 gate 拒绝**、联邦 PDU 拒绝不落库、联邦入站改写不生效。同时用 DB 往返验证了合流的两处争议裁定：`stream_position_tests::{stream_labels_match_the_registry_exactly, every_stream_position_advances_with_writes}` 与 `relations_service_tests_migrated::test_get_relations_filtered_by_event_type` |
| **P-11** | **集成批次有一条长期判红的测试**（`d409bd89b` 起，两条线上都存在，**非本次合流引入**）：`tests/integration/api_device_routes_tests.rs::test_get_device_returns_not_found_for_other_users_device` 断言 `M_NOT_FOUND`，而代码返回 `M_UNKNOWN_DEVICE` |证据链：① 该提交把 `get/update/delete` 三处的 `ApiError::not_found(...)`（`M_NOT_FOUND`）**替换**为 `unknown_device_error()`（`M_UNKNOWN_DEVICE`），但 `git show --stat d409bd89b` 显示它**只加了单元测试、没动这个集成测试**（该测试最后一次变更是更早的 `52b59c8f3`）；② 仓库因此自相矛盾 —— `synapse-web/src/routes/device.rs` 的单元测试钉 `M_UNKNOWN_DEVICE`，同一行为的集成测试钉 `M_NOT_FOUND`，二者不可能同绿；③ **规范判据（本轮新增取证）**：`matrix-org/matrix-spec` 的 `data/api/client-server/device_management.yaml` 对这三个端点的 404 描述是 _"The current user has no device with the given ID"_，且**全文件无 `M_UNKNOWN_DEVICE`**；该码属 MSC4326 _Device masquerading for appservices_（`MSC_SEMANTICS.md` §59），本仓无 masquerading 路径 **已按 A 执行**：设备 CRUD 恢复 `M_NOT_FOUND`（`device_not_found_error()`），`M_UNKNOWN_DEVICE` 保留为错误码词汇但无端点使用（`code.rs` 加注防误用），单元测试改为 `device_crud_404_uses_m_not_found`。判据：`cargo nextest --profile ci --all-features --test integration api_device_routes` ⇒ **6 passed**（含原判红用例）|
| **P-12** | **L-6 加了 `/versions` 能力却没更对应快照** ⇒ `api_route_snapshots_tests::snapshot_versions_endpoint` 长期判红（同样**非**合流引入） | **已修**：取证显示 `tests/integration/snapshots/integration__api_route_snapshots_tests__versions_endpoint.snap` 在**两个父提交里都没有** `org.matrix.msc3981`（快照最后一次变更是更早的 `18071e8b3`），而 L-6 让 `/versions` 广告 `org.matrix.msc3981` + `org.matrix.msc3981.stable`（§18.3 #14 已记录该行为）⇒ 快照补齐这两行。**教训**：L-6 的交付说明只提了"两条 route-ledger 快照"，漏了这条能力快照 —— 能力集合变更必须扫**全部**受影响快照，不能只列自己记得的那几个 |
| **P-7** | 两条辅助文档的条目口径在两侧各写一版 | **已闭环（2026-10-02/03）**：`docs/audit/archive/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md` 的 L-1/L-3 行与 `docs/synapse-rust/API_COVERAGE_REPORT.md` 的 #20097 / #20181 / L4 行均已按合并后的实现对齐；P-5 又把两处的 stream 口径从 5 条更新为 **6 条**（`worker_events` 接入） |
| **P-8** | 合流结果需完整门禁验证 | **已闭环（2026-10-02/03）**：clippy 两档 `EXIT=0`；`./scripts/check_fmt_ratchet.sh` current=0 baseline=0；契约 gate `EXIT=0`；doc 守卫 6 passed；定向用例累计 **105+ 条**全绿（gate 7 / relations+stream 44 / 发送路径 43 / storage relations 35 / room_summary 2 / unit 9）。⚠️ **完整 1522 条集成批次未跑完**（为提交多次让出 cargo 锁；跑到 432/1522 时只有两条既有失败，均已处置）⇒ 全量以 CI 为准 |
| **P-13** | `device_lists_outbound_pokes` **整表是死脚手架**（原 P-5 的 ②） |生产代码里该表**既没有 INSERT 也没有 SELECT**：全仓唯一引用是 `synapse-storage/src/pruning.rs` 的 pruning 函数（对永远为空的表做 DELETE，还挂在 30s 维护循环上）与 baseline 的建表语句；联邦 device-list 更新实际走 **EDU 发送路径**（`synapse-web/src/federation/edu.rs`），与本表无关 **已按铁律 1 删除（2026-10-03）**：baseline 的建表 + 3 索引 + FK、`prune_sent_device_lists_outbound_pokes` + 其 retention 常量 + 2 条测试、`src/server/mod.rs` 的 `prune_step!`、`.sqlx` 中那条 DELETE 的缓存条目全部移除；`baseline_tables()` 是编译期解析 baseline 得到的，自动随之减少一表。R10 判据：`EXPECTED_BASELINE_FINGERPRINT` 重算为 `ba6e35d1379ddd74`（用 test 里同一 FNV-1a64 实现先复现旧值自检）、`test_isolation_unification` 等 80 passed、pruning 13 passed、`check_sqlx_cache_fresh.sh --compile` OK（1325 条）、SQLx 四道棘轮 OK、`database_integrity`/`db_schema_smoke`/`schema_contract` 通过|
| ~~**P-13b**~~ | ~~`room_ephemeral.stream_id` 非单调 ⇒ 拿它做游标会倒退~~ **【本轮撤回：前提不成立】** |取证：两个读查询（`synapse-storage/src/event/ephemeral.rs:91,133`）只用它做 **`ORDER BY stream_id DESC`** 排序，**没有任何 `stream_id > since` 的游标过滤**（过期判定走 `expires_at`）；`RoomEphemeralEvent.stream_id` 在服务层**从不被读取**（`response.rs` 里的 `t.stream_id` 全是 **sync token**，不是 ephemeral 事件）。它的写入由调用方提供并被 UPSERT 覆盖（typing 场景传的是 `now`），语义就是「同房内按新鲜度排序」 **不是缺陷，无需修**；`StreamPosition::ALL` 对它的排除是**永久正确**的，只需把排除理由从「非单调」改成「它不是 stream 位置」（已在 `stream_positions.rs` / `server_metrics.rs` 更正）|
| **P-14** | **`allow[].room_id` 的语法校验与本仓自己的 room-ID 语法矛盾**（跨实现联调 V-7a 命中，2026-10-03） | `synapse-services/src/room/join_rules.rs` 自带一份「必须含 `:`」的 `is_valid_matrix_id`，而本仓建房用的是 room v12 / MSC4291 的**无域名** ID（`!` + 43 位 URL-safe base64，见 `synapse-common/src/room_id.rs`）⇒ `allow` 里本机自建房间 ID 全被丢弃。后果有二：① `/summary` 与 `/hierarchy` 的 `allowed_room_ids` **恒为空**（客户端无法判断哪个 space 的成员资格可进 restricted 房）；② `membership/service.rs:595` 的 **restricted join 授权闸门永不授权**。上游对照：`synapse/handlers/event_auth.py::get_rooms_that_allow_join` **不做任何语法校验**（只要求 `room_id` 是字符串）。**已修（`97fd35130`）**：删掉第二份语法、统一委托 `synapse_common::room_id::is_well_formed_room_id`（铁律 2）；红证明 `extract_allow_accepts_domainless_room_v12_ids` 先 `left: []` 后绿，`extract_allow_follows_the_canonical_room_id_grammar` 钉住两个方向；`-p synapse-services --lib -E 'test(extract_allow) \| test(join_rules) \| test(summary_projection)'` 25 passed；重建镜像后 V-7a 联调转绿 |
| **P-15** | **入站联邦邀请回显的 PDU 发送方无法验签**（跨实现联调 V-2b 命中，2026-10-03） | 现象：B（上游 Synapse）建房邀请 A 用户时，B 收到 A 的 v2 invite 响应后复验失败 —— `synapse.federation.federation_client:1380` 的 `_check_sigs_and_hash` 抛 `Not signed by synapse-b.federation.test`，客户端 API `403 M_FORBIDDEN`。取证：A 库里该 invite 行 `events.signatures` **只有 A 自己的 key**；`create_outlier_event`（`synapse-storage/src/event/create.rs:230`）与 `create_event_with_pdu`（`:105`）的 INSERT 列清单里没有 `signatures`/`hashes`，`CreateEventParams` 也没有这两个字段 ⇒ 收到的签名材料写入时就丢了，随后 `project_and_sign_pdu_locally` 走 `SignLocally` 用本机密钥重签。**已修**：invite_v2 在落库后、投影前，用**与 `/send` 事务入站同一条机制**（`synapse_common::event_utils::signature_material` + `update_event_signatures_and_hashes`）把发送方的 `hashes`/`signatures` 写回该行 ⇒ 投影取 `KeepStored`，响应复述发送方签名。红证明 `invite_v2_keeps_the_origin_signature_in_the_row_and_the_response`（修前 `got {"localhost": …}`，修后响应里的签名能通过 `verify_strict`）。**修复不能只做这一半 —— 见 P-16（内容哈希）与 P-18（stripped state）** |
| **P-16** | **v3+ PDU 携带非规范 `origin` 字段 ⇒ 对端内容哈希校验失败并静默 redact**（同轮联调发现，2026-10-03） | 现象（修 P-15 后立刻暴露）：本机投影把 `origin` 加到 v3+ PDU 上，而 `hashes.sha256` 覆盖的是**未 redact** 的事件，于是对端重算内容哈希必然不等 —— 上游 `federation_base._check_sigs_and_hash` 在此情形下**只 redact、不报错**（`check_event_content_hash` 失败 ⇒ `prune_event`），所以表面上"邀请成功"而内容是残缺的。取证：`state_pdu` / `build_pdu` 无条件插入 `origin`（room v3 起该字段已从 PDU 移除；redaction 也会丢掉它，所以**签名一直是好的**，只有内容哈希错），`verify_event_content_hash(&echoed)` 在同一条测试里先红（`expected … computed …`）。**已修**：`build_pdu` 与 `state_pdu` 统一用既有谓词 `synapse_common::pdu::event_id_is_a_pdu_field`（v1/v2）决定是否发出 `origin`（铁律 2：一处谓词，两个装配点）；5 个 v3+ interop fixture 按测试自带的 `U13_WRITE_INTEROP_FIXTURE=1` 重新生成。判据：新增断言 4（红→绿）+ `verify_event_content_hash` 通过；重建镜像后联调重跑，B 的日志里**不再出现** `tampered/redacting`，且 V-2a/V-2c/V-2d/V-7a/V-7b 全部保持绿（无回归） |
| **P-17** | **`/sync` 永远看不到 invite / knock**（修 P-15 后 V-2b 的下一层，2026-10-03） | `SyncService::sync` 的房间集合**唯一**来自 `RoomMemberStorage::get_sync_rooms`，而该查询只有 `membership IN ('join','leave')`（`include_leave=false` 时只有 `'join'`）⇒ 被邀请/敲门的用户 `/sync` 里什么都没有；`room_sections_from_memberships` 的 `invite`/`knock` 分支与 `response.rs` 里整套 `invite_state`/`knock_state` 渲染**全部不可达**（H-2 交付的 `rooms.knock` 因此端到端不生效）。单元测试看不见它，因为它们直接把 membership 数组喂给 `room_sections_from_memberships`，绕过了查询。**已修**：两条查询改为 `('join','invite','knock'[,'leave'])`（`ban` 刻意不纳入：section 映射里没有 `ban` 分支，纳入会落进 catch-all 渲染成已加入）。红证明 `test_get_sync_rooms_includes_invite_and_knock`（修前 `got ["join"]`）；端到端 `api_room_sync_tests::test_global_sync_shows_an_invited_room`（本地邀请 ⇒ `rooms.invite[room_id]` 且 `invite_state` 带 `m.room.create`）；SQLx 缓存按 R2 重新生成（2 条替换，仍 1325 条） |
| **P-18** | **入站联邦邀请不落 `invite_room_state`** ⇒ 首次接触的邀请仍进不了 `/sync`（修 P-17 后 V-2b 的第三层，2026-10-03；**已修**） | 取证：A 库里 B 发来的邀请房间**只有 1 条事件（邀请本身）、0 条 `m.room.create`**（`!BbR61lR0nKA0KNBqi2a6x1DZWvWl0RwAQHUKgAyjQ_c`）；`invite_v2` 只用 `invite_room_state` 取 join_rule 与做形状校验（`invite.rs:130,188`），**从不持久化**；而 `assemble_room_stripped_state` 按 MSC4311 **fail-closed**：没有 `m.room.create` ⇒ 返回 `None` ⇒ 该房间被移出 invite 段。反方向同一根因：A 出站邀请的 `invite_room_state` 也缺 `m.room.create`，上游 B 日志 `WARNING - Continuing anyway but failed to validate invite_room_state … Stripped state must include m.room.create event (MSC4311)`（容忍但点名）。**已修（`c13d0ebec`）**：① 入站 —— `invite_v2` 在签名写回之后把发送方给的 stripped state 投影（`sanitized_invite_room_state`：只留 `type/state_key/content/sender?`，全空即 `None`）落进邀请事件的 `unsigned.invite_room_state`（新增 `EventWriter::update_event_unsigned`；`unsigned` 不在签名材料、不在内容哈希内 ⇒ 后写不破坏发送方签名）；`assemble_room_stripped_state` 改为**优先取该 payload**、仅在缺失时才回退既有的 fail-closed 投影；② 出站 —— 邀请/建房路径的 `invite_room_state` 补齐 `m.room.create`（`invite_room_state_for()`，`synapse-federation/src/client.rs`/`client_api.rs`），上游不再报 `Stripped state must include m.room.create`。判据：`invite_v2_supplied_stripped_state_reaches_the_invitees_sync` 与 `invite_room_state_tests::*`；跨实现联调 `scripts/federation-test/test_federation_v2v7.sh` **failures=0**，V-2b 转绿 |
| **P-19** | **规则没有自己的超时**：一条挂起的规则吃掉整个请求预算（2026-10-03；本条结论已更正一次） | **第一次写错了、当场更正**：我最初记的是「`request_timeout_middleware` 从未接进路由器」——那是只 grep 了 `synapse-web/src` 得出的错误结论。实际上**生产是接了的**：`src/server/router.rs::build_router` 在 `create_router` 之上挂了 `RequestBodyLimitLayer` / HTTP-RED 指标 / `request_debug` / **`request_timeout`** / `TraceLayer` / 413→`M_TOO_LARGE` 六层（`src/server/mod.rs:331` 调用）。准确的事实有三条：① **规则级超时确实不存在** —— 挂起的规则会吃掉整个请求预算（非长轮询 30s / 长轮询 90s+），客户端最后拿到的是与规则无关的 `408 M_REQUEST_TIMEOUT`；入站联邦 `/send` 上，来源服务器还会带着同一事件反复重试，每次再占满一个请求槽；② **测试从未跑过那一层**：集成套件与手工压测都用 `synapse_web::create_router` 装配（`tests/integration/mod.rs:408,555,609`），六个生产层全被绕过 —— 这也是探针第一次跑等满 600s 被强杀的原因；③ fail-closed 当时是**偶然成立**的（闸门在写入之前，请求被取消 ⇒ 事件不落库），不是设计。**①已修（2026-10-03）**：`third_party_rules` 增 `rule_timeout_ms`（默认 `DEFAULT_THIRD_PARTY_RULE_TIMEOUT_MS` = 2s），`EventAdmissionGate::event_rule_timeout()` 把它交给 `consult_event_admission`，超时按 **fail-closed 拒绝**（`403`，reason 含 `no verdict within {n}ms`）——与规则返回 `Err` 同语义，且写入之前就返回。实测 `admission_gate_hanging_rule_is_refused_by_the_rule_deadline`：规则睡 30s，发送在 **2.01s** 返回 `403 M_FORBIDDEN`（`no verdict within 2000ms`），`/messages` 里 0 条消息。**②基本闭环（2026-10-03）**：`build_router` 已在 `src/server/mod.rs` 重导出；① `tests/integration/server_router_assembly_tests.rs` 用**生产装配**断言 HTTP-RED 指标层在工作、裸 `create_router` 不工作（反差即证据）；② 同文件用 receipts 路由（`body: String`，axum 把长度错误报成 **413**，而 `Json` 会包成 400）端到端断言 body 上限 + 413→`M_TOO_LARGE` 改写；③ **集成夹具与手工压测夹具都改为经 `build_router` 装配**（`tests/integration/mod.rs::production_app`、`tests/performance/manual_smoke_tests.rs::production_app`），因此那 1500+ 条用例与其请求现在都跑真实层栈；④ 压测数字复测后确认：切到生产装配后 p50 从 2.7ms 级升到 6ms 级（见 §18.6 V-10）—— 裸装配的测量偏低。**③ 请求超时也成了配置并可断言（2026-10-03）**：`server.request_timeout_secs` / `server.long_poll_request_timeout_secs`（`Option<u64>`，缺省 30s/90s）取代了那两个**未文档化的环境变量** （`REQUEST_TIMEOUT_SECS` / `LONG_POLL_REQUEST_TIMEOUT_SECS` —— 没有配置文件、文档或脚本提过它们，且每个请求都读一次环境）；中间件改经 `from_fn_with_state(RequestTimeouts)` 取预算，`build_router` 在装配时从配置填入，样例配置也写明了这两个键。判据 `production_request_timeout_bounds_a_hung_write`：把 `server.request_timeout_secs=1`（规则截止仍是容器默认 2s，所以先触发的是**请求**层）、注册一条挂起规则后发消息 ⇒ **1s 级**返回 `504 GATEWAY_TIMEOUT` + `M_REQUEST_TIMEOUT`（`Request processing exceeded server timeout of 1s`）—— 顺带钉住"超时是 504 不是 408"这一容易误判的细节。**唯一残留**：整体迁移的全量套件结论（见 §18.7.3） |

#### 18.7.2 逐项取版本裁定

| 条目 | 取自 | 理由 |
|---|---|---|
| 第三方准入钩子（M-4） | feature | 唯一完整实现（fail-closed + `modification` + 房间创建序列 / burn-after-read 覆盖） |
| M-5 决议缓存作用域 | feature | 进程内跨请求共享，跨 walk 共享已有用例锁定 |
| L-1 stream 指标 | opt/consolidated | 5 stream + 单一 `UNION ALL`、后台负载有界；**删除** feature 的抓取时实现（双写同一指标），并叠加 feature 的 family 去重修复 |
| L-6 relations 4 段路由 + `event_type` | opt/consolidated | 其契约链（ledger / 派生表 / fixture / `ROUTE_CONTRACT.md`）已同步，反向取会丢；SQL 取 `COALESCE(e.event_type, '')` 版本（`count_relations` 注释已引用该口径） |
| L-3 worker 显式路径集 | opt/consolidated | feature 无此实现 |
| L-1b Grafana 面板 | opt/consolidated | feature 无此实现 |
| MSC4242 注释 / L-2 裁定 | opt/consolidated | 取证更细（MSC 未定稿 + 上游联邦 PR 关闭未合并） |

**合流期发现并修掉的 3 处"静默"破坏**（`git merge` 未报冲突，但结果本身不成立 ——
这是"以 auto-merge 成功当作正确"的典型反例，必须按编译与用例复核，而不是看合并是否报错）：

1. `RelationQueryParams.event_type` 被两侧**各加一次** ⇒ 同名重复字段（E0124）；已去重为一个。
2. `src/server/mod.rs` 同时保留**两份** stream 刷新（opt/consolidated 的 30s 循环 + feature 的抓取时）⇒ 双写同一指标；已删除抓取时那一份。
3. `DynamicGaugeTemplate::set` 两侧**签名不同**（`(value, labels)` vs `(labels, value)`）；统一取 opt/consolidated 的 `(labels, value)`，并同步修正遗留调用点与文档示例。

#### 18.7.3 仍存在的问题（下一步）

| ID | 问题 | 影响 | 建议动作 |
|---|---|---|---|
| **P-6** | 需联调 / 外部对端的验证未闭环；V-10 的 p99 与超时语义仍缺 | **2026-10-03 收盘**：① V-10 的**缓存复用**已交付（唯一实现 `synapse-services/src/room_state_cache.rs`；失效契约审计确认**没有绕过失效的生产写路径**）；② **p99 已实测**（`admission_gate_send_latency_p50_p99` 两次运行，规则路径无可观测额外 p99）；③ **V-2a/V-2c/V-2d/V-7a/V-7b 联调闭环**（跨实现栈）；④ **V-10 的超时语义已量清**：不存在规则级超时，唯一的候选层 `request_timeout_middleware` 从未接进路由器 ⇒ 挂起即无界（P-19）；⑤ **V-8 的仓库内部分已补**：`GET /profile/{userId}/{field}` 未设置字段返回 `{}` 的契约此前只有**假覆盖**（外部用例断言自造的 `json!({})`），现由 `account_compat::single_profile_field_tests` 真覆盖；SDK/前端侧不在本仓（无 TS 代码），无法在此验证 | 剩余一项见下（P-19 超时层；P-18 stripped state 已闭环，见 §18.7.1）。**V-8 的 SDK 侧**需在 SDK 仓库核对：是否把 `""` 当"未设置"（本仓现在省略键），并跑其 `spec/unit/account*.spec.ts` 与快照 —— 本仓没有这些文件，不能声称已验证 |
| ~~**P-18**~~ | ~~入站联邦邀请不落 `invite_room_state` ⇒ 首次接触的邀请进不了 `/sync`（V-2b 的最后一层，P-15/P-16/P-17 修完后的现状）~~ **【已闭环，2026-10-03】** | 被邀请的本地用户**看不到**来自其他服务器的邀请（`/sync` 的 invite 段按 MSC4311 fail-closed 丢弃无 `m.room.create` 的房间）；反方向 A 出站邀请的 `invite_room_state` 也缺 `m.room.create`，上游只能 WARNING 容忍。证据见 §18.7.1 P-18（A 库中该房间 0 条 create + 上游日志原文） | **已按原建议落地（`c13d0ebec`）**，但实现比原设想更简：不新造 stripped-state 行，而是把发送方给的 payload 原样存到邀请事件的 **`unsigned.invite_room_state`**（`unsigned` 不进签名材料/内容哈希 ⇒ 安全后写），投影侧 `assemble_room_stripped_state` **优先取它**、仅在缺失时才回退既有的 fail-closed 逻辑 —— 因此**没有绕过** fail-closed（原建议 ⑤ 仍成立）；出站侧由 `invite_room_state_for()` 补齐 `m.room.create`。判据：`invite_v2_supplied_stripped_state_reaches_the_invitees_sync` + `invite_room_state_tests::*`；联调 `test_federation_v2v7.sh` **failures=0**。详见 §18.7.1 P-18 |
| **P-19** | 准入钩子的超时护栏（已交付）与测试装配缺口（部分闭环） | **规则级超时已交付（2026-10-03，见 §18.7.1 P-19）**：`third_party_rules.rule_timeout_ms`（默认 2s）+ `EventAdmissionGate::event_rule_timeout()` + `consult_event_admission` 的 `tokio::time::timeout`，超时 fail-closed 拒绝（403，reason 点名 `no verdict within {n}ms`），实测 2.01s 返回且不落库。**装配缺口基本闭环（2026-10-03）**：`build_router` 重导出 + `server_router_assembly_tests`（HTTP-RED 指标层的生产/裸装配反差）+ receipts 路由上的 body 上限与 413→`M_TOO_LARGE` 端到端断言；集成夹具与手工压测夹具（含 worker replication 那个自建 app 的文件）都改为 `production_app()` 装配 ⇒ 1500+ 条用例现在跑真实层栈，`request_debug`/`TraceLayer`/指标层随之每条请求都被执行。**另外**：`request_timeout` 也已配置化并可断言 —— `server.request_timeout_secs` / `long_poll_request_timeout_secs` （缺省 30s/90s，样例配置写明）取代未文档化的环境变量，`production_request_timeout_bounds_a_hung_write` 用 1s 预算实测 `504 M_REQUEST_TIMEOUT`。**仍未做**：迁移后的**全量**集成套件结论（本轮已先跑 59 条高风险用例：metrics/too_large/payload/rate_limit/timeout/media_routes 全绿，手工 perf target 18 条全绿，装配级用例 3 条全绿） | **建议**：把 `build_router` 变成测试可用的入口（`src/server/mod.rs` 里 `pub use router::build_router`，或让 `TestContext` 走同一个装配函数），让至少一条集成用例跑**真实层栈**；迁移时注意长轮询路由（`/sync`、`/events`）走 90s 或 `timeout+15` 分支、媒体上传走 body 上限分支 —— 先加一条「带层栈的冒烟」，再决定是否整体切换，别一次性把 1500 条集成用例换到新装配上 |

#### 18.7.4 合流后的门禁清单（按序，缺一不可）

> ⚠️ 本地跑集成/手工压测前先清一次累积的隔离 schema：
> `DATABASE_URL=... bash scripts/cleanup_test_schemas.sh --apply`。
> 2026-10-03 实测：库里有 **869 个残留 schema**，外加一条被强杀后仍挂着的
> `DO $do$ …`（克隆序列）—— 单个 4.6s 的用例被拖成超过 nextest 的 120s 终止线，
> 两条用例「全都超时」。terminate 掉那条后端并换到新库（`prepare_test_db.sh` 重建
> `public` + `test_template_ci`）后恢复 4.6s。**先怀疑环境，再怀疑代码。**

```bash
# 1) 编译与静态检查（两档不可互相替代）
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
# 2) 本次冲突涉及模块的用例
cargo nextest run -p synapse-common --lib -E 'test(/prometheus|stream_position|dedups/)'
cargo nextest run -p synapse-services --lib --features test-utils -E 'test(/admission|relations/)'
cargo nextest run -p synapse-web --lib --features test-utils -E 'test(/relations/)'
# 3) 文档守卫 + 契约链（守卫谓词已在合流时复算通过）
cargo nextest run --test unit --features test-utils -E 'test(doc_credibility_guard_tests)'
bash scripts/contract/check_route_contract.sh
# 4) 格式
./scripts/check_fmt_ratchet.sh
```

**跨实现联调（非 CI，需 Docker + 两个栈，2026-10-03 首次跑通）**：

```bash
cd docker/federation-test
docker compose -p synapse-federation-test-a --env-file .env.a -f docker-compose-synapse-a.yml up -d   # 若镜像缺失，先 build synapse-rust
docker compose -p synapse-federation-test-b --env-file .env.b -f docker-compose-synapse-b.yml up -d   # 上游 Synapse v1.162.0
bash scripts/federation-test/test_federation_v2v7.sh   # 期望 failures=0（V-2a/2b/2c/2d、V-7a/7b 六项全绿；P-18 已修，见 §18.7.1）
```

⚠️ A 栈镜像必须**由当前 HEAD 重建**：`docker compose ... build synapse-rust`。用旧镜像会得到假结论
（本轮第一次跑用的 2026-10-01 镜像里还没有 H-2 的 `msc4311_strict_validation` 代码，V-2d 的 strict 分支根本不存在）。
判定探针不要只看脚本的 ok/fail：B 的日志要 grep `tampered|redacting`（P-16 的表现是"邀请成功但内容被 redact"），
A 的库要查 `events.signatures` 是否含发送方域名（P-15）。

**手工压测（非 CI，`--features performance-tests,test-utils`）**：

```bash
# p50/p99：规则注册前后各 40 次真实 send
TEST_DATABASE_URL=... cargo nextest run --features performance-tests,test-utils \
    --test performance_manual admission_gate_send_latency --nocapture
# 规则挂起：断言"规则级超时 ⇒ fail-closed 403 且不落库"（P-19；默认 2s 截止）
TEST_DATABASE_URL=... cargo nextest run --features performance-tests,test-utils \
    --test performance_manual admission_gate_hanging_rule --nocapture
```

### 18.8 复核：§1–§18 遗留问题的实际状态（2026-10-16）

> 本节对正文与 §18 中所有以「问题 / 差距 / 待办 / 待确认」口吻出现的声明做了**逐条读码复核**：
> 先定位承载该声明的源码路径，再直接读实现（不以 grep 命中数代替语义核对），
> 并区分「机制存在」与「生产路径已接线」。口径同 §18.5 —— 正文不改写，以本节为最新复核结论。

#### 18.8.1 复核确认仍然存在的问题

| 级别 | 问题 | 复核结论（证据） |
|---|---|---|
| ~~**P0**~~ | ~~账户注销语义不完整（UG-1；上游 #20247）~~ → **已修（2026-10-17）** | 新增 `synapse-services/src/account_deactivation_service.rs` 把注销做成完整流水线：取消 MSC4140 延迟事件（fail-fast，最先）→ 退房并拒绝 pending invite/knock → 删全部 pusher → 清理账户作用域数据（账户数据/房间账户数据/推送规则/线程订阅/3PID/E2EE 备份）→ 撤销 token/设备并置停用位（最后）。原先「只置停用位 + 删 token/设备」的 `deactivate_user` 已不再是生产注销路径 |
| ~~**P0**~~ | ~~重发远程 PDU 缺发送方签名与哈希~~ → **原判定作废（2026-10-17 复核）** | 三条入站路径均已回写上游 `signatures` / `hashes`（统一谓词 `synapse_common::event_utils::signature_material` + `MessagingService::update_event_signatures_and_hashes`）：`/send`（`synapse-web/src/routes/federation/transaction.rs:712-736`）、`backfill`（`synapse-services/src/room/backfill.rs:344`）、入站邀请（`synapse-web/src/routes/federation/membership/invite.rs:238-245`）。`SignatureAction`（`synapse-web/src/routes/federation/pdu.rs:58-68`）实为 `KeepStored` / `SignLocally` / `RefuseIncomplete`（**无** `EmitUnsigned`），`KeepStored` 即「原样投射发送方签名」态；DB 回归 `inbound_pdu_signature_material_round_trips`（`synapse-storage/src/event/db_tests.rs:691-751`）锁定落库-读回闭环。**仅剩真实残余**：`event_id` 仍是 `$<ts>$<base64>:<server>`，非 v4+ 参考哈希 |
| **P1** | ~~房间别名可被静默改指（ELEMENTSEC-2026-1729）~~ → **已修（2026-10-16，见 §19.5.1）** | `synapse-storage/src/room/mod.rs` 的 `set_room_alias` 改为 `ON CONFLICT (room_alias) DO NOTHING`（0 行受影响 ⇒ `sqlx::Error::RowNotFound`），不再 `DO UPDATE SET room_id = EXCLUDED.room_id`；服务层把它翻成 **409 `M_ROOM_IN_USE`** —— 已存在的别名不再被静默改指，对齐上游"失败而非改指"语义。证据：`synapse-services/src/room/state/aliases.rs`、回归 `tests/integration/room_alias_storage_tests_migrated.rs` |
| **P1** | App Service（MSC2409）临时数据投递未接线（UG-4） | ✅ **已修（2026-10-17，见 §19.5.1）**：`synapse-services/src/application_service/transaction.rs`（+289 行）落地 EDU/回执投递接线 —— `AppServiceScheduler` 的 `deliver_transaction_with_edus` 生产调用方现从回执流取数并填 `AppServiceEphemeralBatch.receipts`；新 seam `RoomStoreApi::get_latest_receipt_position()` / `get_receipts_since(from_exclusive, to_inclusive, limit)`（`synapse-storage/src/room/api.rs`，模型 `ReceiptStreamEntry` 见 `room/models.rs`）提供按流位置分页；`synapse-services/src/wiring/admin.rs` 以 `.with_room_store(...)` 注入 `RoomStorage`。① 配置键口径同前（`appservice_receives_ephemeral` 认 `de.sorunome.msc2409.push_ephemeral` 与 `receive_ephemeral`）；② 现按上游规范写的 bridge opt-in 后**能收到 `m.receipt` EDU** |
| **P1** | App Service pushers / 设备管理端点缺失 | **设备管理面已修（2026-10-16，MSC4190/MSC4326，见 §19.5.1）**：AS 用它自己的 token 调用客户端设备端点（`synapse-web/src/routes/device.rs`）即可管理所代理虚拟用户的设备 —— `appservice_id` 非空时**跳过 UIA**，`PUT /devices/{deviceId}` 走 upsert（201 新建 / 200 更新），未知设备用 MSC4326 稳定码 `M_UNKNOWN_DEVICE`。**pusher 面已解（2026-10-17 复核结论）**：上游 Matrix AS API **无专属 pusher 端点**，AS 经 `m.login.application_service` 登录取得带 `appservice_id` 的普通 access token 后，直接调用通用 C-S 端点 `POST /_matrix/client/v3/pushers/set` / `GET /_matrix/client/v3/pushers`（`synapse-web/src/routes/push.rs`）即管理虚拟用户 pusher ⇒ 原「pusher 面仍缺」判定**属误报**，回归测试 `test_appservice_pusher_management_via_login`（`tests/integration/api_appservice_p1_tests.rs`）锁定该链路 |
| **P1** | App Service 联邦侧代理缺失 | ✅ **已修（2026-10-17，MSC4512，见 §19.5.1）**：新增 `synapse-web/src/routes/msc4512_proxy.rs` —— ① 统一 fallback（`routes/assembly.rs`）按注册文件 `proxy_prefix` 动态分派 C-S（`/_matrix/client/...`）与 S-S（`/_matrix/federation/...`）代理，命中后授权（C-S 复用 `utils::auth` 的 token 校验；S-S 复用 `middleware::federation_auth::authenticate_federation_request`）并转发到 `proxy_url`，转发前后双向剥离 hop-by-hop 头与入站 `Authorization`，改注入 AS `hs_token` 与 `X-Matrix-User-Identifier`（C-S）/ `X-Matrix-Origin`（S-S）；② 发起侧新端点 `POST /_matrix/client/v1/appservice/fed_proxy`（+ unstable 孪生 `POST /_matrix/client/unstable/io.element.msc4512/appservice/fed_proxy`）实现规范全部错误码（`M_FEDPROXY_DESTINATION_DENIED` / `M_FEDPROXY_PATH_NOT_ALLOWED` / `M_FEDPROXY_CONNECTION_FAILED`）；③ 策略对齐上游 Synapse：`ALLOWED_PROXY_PREFIXES = {"rtc/livekit"}` 白名单 + `experimental.msc4512_enabled` 默认 false。配置沿用 `AppServiceConfigFile` 的 `#[serde(flatten)] extra` ⇒ **零迁移** |
| **P1** | MSC4242 仅存储层、HTTP 面未接线 | 存储层已进 v12 图路径（`synapse-storage/src/event/dag.rs`），HTTP 面缺；§18.4 L-2 已裁定不接线（受阻于 MSC4242 房间版本 + experiment） |
| **P1** | ~~push rule 数量 / 体积无上界（ELEMENTSEC-2026-1071）~~ → **已修（2026-10-16，见 §19.5.1）** | `synapse-services/src/client_push_service.rs` 落地三道上界：`MAX_PUSH_RULE_ID_LEN = 300`、`MAX_PUSH_RULE_SIZE = 1024`（序列化动作长度，对齐上游 `_push_rule_size_for_limits`）、`MAX_PUSH_RULES_PER_USER = 10_000`；越界一律 **413 `M_TOO_LARGE`**，且**更新已有规则时放行**（不把已达上限的用户锁死） |
| ~~**P1**~~ | ~~lazy-load 成员缓存缺 sync 位置维度~~ → **已修（2026-10-17，分支 `feat/b3-msc2409` 已并入 main）** | `synapse-services/src/sync_service/lazy_load.rs` 的成员状态现在按 sync 流位置记账：`start_request(since)` 先丢弃 `sent_at <= since` 的记录（稳态下直接跳过扫描）、`restore` 只复用 `last_sent_position <= since` 的持久化行、`mark_sent(..., response_position)` 把「本次响应位置」写进内存条目并持久化 `last_sent_position`。即：客户端换用更旧的 `since` 时不会再漏发该位置之后的成员变更 |
| **P1** | ~~缺 MSC4354 标准请求面（设计性分歧）~~ → **已修（2026-10-16，方案 ⓐ 补上游面，见 §19.5.1）** | `PUT /rooms/{roomId}/send/{eventType}/{txnId}` 现读 `?org.matrix.msc4354.sticky_duration_ms=<ms>`（亦接受短名 `sticky_duration_ms`），把 sticky TTL 落进事件的客户端投影 `unsigned.msc4354_sticky_duration_ttl_ms`（上限 1h = `MAX_STICKY_DURATION_MS`，非整数/非正/越界 → 400 `M_INVALID_PARAM`）。**`/send` 与 `/state` 两面均已接线（`/state` 面 2026-10-17 补齐，见 §19.5.1）**：状态写路径 `send_state_event` / `put_state_event` / `put_state_event_empty_key` / `put_state_event_no_key`（`synapse-web/src/routes/handlers/room/state.rs`，+40 行）现同样解析 `msc4354_query_sticky_duration_ms` 并在持久化后 `apply_msc4354_sticky_ttl` 落 `unsigned` 投影。私有 `POST /rooms/{roomId}/sticky_events` 路由**保留**（per-user 置顶语义不变）；`delayed_events.rs` 的专用调度端点**也已接受**该字段（2026-10-17）：值随行持久化，由 dispatcher 在**发送时**写进 `unsigned` —— 上游 #19354 就是这么做的（`add(sticky_duration_ms=…)` + `_send_event()` 注入），详见 `docs/audit/UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md` §11.18。两套语义的归属见 §19.5.1：**`/sync` 的 sticky 标记由私有 `room_sticky_events` 表驱动**，`/send`、`/state` 的查询参数只落 `unsigned` 投影 |
| **P2** | SQLx 动态查询债务 | 基线文件 `scripts/ci/sqlx_dynamic_ratio_baseline` 仍在并受 ratchet 门禁约束 |
| **P2** | ~~MSC4502 / MSC4262 覆盖度~~ → **两条均已补齐（2026-10-17 复核）** | **MSC4262**：`LEFT_ROOM` 语义落地（`profile_updates.users[uid] = null`），见 §19.5.1 R9。**MSC4502 已复核为「端到端完整」**（不再标「部分实现」）：`GET /rooms/{id}/members` 的 handler 解析 `membership` / `not_membership` / `from` / `dir` / `at`（`synapse-web/src/routes/handlers/room/members.rs:364-386`）→ 服务层 `limit + 1` 探测续页（`synapse-services/src/room/membership/service.rs:799`）→ 存储层按 `user_id` 游标分页（`synapse-storage/src/membership/mod.rs:831,852`，`>`/`<` 双向）；另有 `/sync` 的 `not_membership` 消费面。（`PROJECT_REMAINING_ISSUES_2026-09-14.md` §22.4 早已证伪「未收敛」，本轮只是在当前 HEAD 上重跑链路） |
| **P2** | ~~ELEMENTSEC #6（multipart Content-Type）/ #11（nginx `merge_slashes`）~~ → **已闭环（2026-10-17）** | #6 **结构性不适用** —— `multer 3.1.0` 用 `mime::Mime` 比较（大小写不敏感）+ 流式解析 + body 上限，无「按字符串前缀判 Content-Type」面。⚠️ 同一轮复核**顺带发现并修掉**一处真实缺陷：`/voice/upload` 未设 `DefaultBodyLimit` ⇒ 落到 axum 的 **2 MiB** 默认值，而 `/voice/config` 与 handler 都按 `server.max_upload_size` 算 ⇒ 2 MiB 以上的音频在传输层被拒且报成 400 `M_BAD_JSON`。现显式设界为 `max_upload_size + 64 KiB`，四条回归见 §19.10。#11 **应用层不适用**（axum 精确段匹配 + 404 兜底 `M_UNRECOGNIZED`，新增回归 `test_extraneous_path_suffix_and_trailing_slash_are_not_routed`，含 `..` 归一化形态）；仓库内 nginx 配置均无 `merge_slashes`（worker 分流未启用）⇒ 余下部分是代理层运维项，不改代码 |
| **P2** | §18.7.3 P-6 / P-19 残留 | P-19 迁移后的**全量**集成套件结论仍未跑完（本轮仅覆盖高风险 59 条 + 手工压测 18 条 + 装配级 3 条） |

#### 18.8.2 复核中被证伪的条目（应从问题清单移除）

| 原声明 | 证伪依据 |
|---|---|
| MSC4140 `state_key` 被硬编码为 `None` | `synapse-web/src/routes/delayed_events.rs:172-176` 的 `parse_schedule_request` 完整解析 absent / null / 具名等形态，并按 `state_key.is_some()` 分流 state / message 两条写路径；`synapse-storage/src/delayed_events.rs:154-185` 持久化 `state_key` 列。`state_key: None` 仅出现在测试夹具（`synapse-services/src/delayed_event_service.rs:145`）⇒ 端到端支持，**原判定属误报** |

#### 18.8.3 复核确认为已解决 / 已过时的条目

| 条目 | 现状 |
|---|---|
| send_join 真实 PDU 投影 | 已实现 |
| MSC4140 联邦 EDU | 已实现（§12.4 L718） |
| OIDC 提权 | 已修 |
| MSC3912 级联 + 审计 | 已实现（§15.3 L945 / L719） |
| rc_reports 限流 | 已实现 |
| 半写窗口事务 / txn 去重同事务 | 均已修 |
| 稳定 profile `/{keyName}` 路由 | 已实现 |
| 默认房间版本 | 默认创建版本已为 **12**，v13 已移出能力表（§11.1 / §15.3 的旧声明已作废） |
| LiveKit `ws_url` 死配置 | 已删除 |
| 缩略图 `animated` 支持 | 已实现（`synapse-web/src/routes/media/download.rs:174`） |
| `m.login.application_service` | 已实现（`synapse-web/src/routes/auth_compat.rs:455-468`） |
| `search_index` 遗留表 | 已从 SQL 移除 |

#### 18.8.4 待人工决策项

| 项 | 状态 |
|---|---|
| ~~AS 错误码 `M_APPSERVICE_LOGIN_UNSUPPORTED`~~ → **已实现（2026-10-17）** | 复核发现缺的不是一个字符串：`POST /register` 原先**完全不认 AS token**（`extract_as_token` 只在 `/login` 用），`inhibit_login` 全仓 0 命中 ⇒ **MSC4190 的注册面整体缺失**。本轮按「实现」落地：`/register` 认 as_token → 要求 `inhibit_login: true`（否则 400 `M_APPSERVICE_LOGIN_UNSUPPORTED`）→ 校验 `users` 命名空间（任一条目，不要求 exclusive）→ 只建用户、**不签发 token/设备**。详见 §19.11 |

---

## 19. v1.11 对照 Synapse v1.163.0（2026-10-16，当前口径）

### 19.1 方法与基线

**上游基线**：`element-hq/synapse` tag **`v1.163.0`（2026-10-14 发布）**，对比区间
`v1.162.0`（2026-09-29）→ `v1.163.0`，共 **50 个提交 / 147 个变更文件**。取数方式：

```bash
# 1) 确认最新版本与发布时间
curl -sS "https://api.github.com/repos/element-hq/synapse/releases?per_page=5" \
  | python3 -c 'import json,sys;[print(r["tag_name"],r["published_at"]) for r in json.load(sys.stdin)]'
# 2) 版本级 changelog（raw.githubusercontent 对大文件会超时，改用 jsDelivr）
curl -sS "https://cdn.jsdelivr.net/gh/element-hq/synapse@release-v1.163/CHANGES.md"
# 3) 变更文件清单（含增删行数）
curl -sS "https://api.github.com/repos/element-hq/synapse/compare/v1.162.0...v1.163.0"
# 4) 逐文件取两版内容后本地 diff（权威判据是 diff，不是 changelog 措辞）
curl -sS "https://cdn.jsdelivr.net/gh/element-hq/synapse@release-v1.162/<path>" -o old
curl -sS "https://cdn.jsdelivr.net/gh/element-hq/synapse@release-v1.163/<path>" -o new
```

**本仓基线**：`main` @ `86ee4e957`（2026-10-16 工作区干净；注册路由条目 **1,036**，含路由模块 **66**，
以 `docs/synapse-rust/ROUTE_CONTRACT.md` 为准）。

**判定原则**（与前几轮一致，此处重申）：
1. **不采信模块名、注释与既有文档的自述**，只认 `路径:行号` 级别的源码实测；
2. 上游是 Python / twisted / poetry 实现的条目，**先判"本仓是否有对应形态"**，无形态即标
   「结构性不适用」并写出理由，不用"没找到"代替结论；
3. **已实现但未宣告**、**已宣告但未实现**都算差距（前者客户端发现不了，后者是最危险的过度承诺），
   两者分列；
4. 全部"不存在 / 未接线"类结论在写入前都在本 HEAD 上实跑过检索命令。

⚠️ 本机 `grep` 被 toybox 替换，BRE 扩展 `\|` 与 `\b` **静默返回空**（`-E` 也救不了 `\b`）——
本轮所有检索一律走 `Grep` 工具（ripgrep），裸 `grep` 的结果不作为判据。

### 19.2 上游 v1.163.0 变更全量归纳

**Features（6 条）**

| 编号 | 内容 | 影响面 |
|---|---|---|
| #20137 / #20286 | 宣告支持 Matrix **v1.14** / **v1.15**（`rust/src/handlers/versions.rs` 的 `versions` 数组延长） | 互操作承诺 |
| #20165 | MSC4354 Sticky Events **补齐**：对错过 EDUs 的远端 homeserver 做 catch-up | 联邦一致性 |
| #20227 | 新增 **MSC4075**（MatrixRTC 邀请/通知）推送规则：`rust/src/push/base_rules.rs` **+171 行**（含 6 条单测），`rust/src/push/mod.rs` 加 `msc4075_enabled` 门控 | 通知/来电 |
| #20230 | 任务调度器并发可配：新增 `synapse/config/task_scheduler.py`，`task_scheduler.max_concurrent_tasks`，默认 **5**，并拒绝 `bool` / 非正整数 | 运维/并发 |
| #20247 | 账户注销时**取消该用户的 MSC4140 延迟事件**（放在注销流程最前） | 安全/数据 |

**Bugfixes（12 条，剔除 Python-only 的 #20354）**

| 编号 | 内容 | 影响面 |
|---|---|---|
| #20108 | 应用服务回执：单次流更新 > 100 条回执时会被永久跳过 —— 改为**分页取到 `reached_token`** 并循环投递，首次投递取最新（`BACKWARDS`）、后续取最旧（`FORWARDS`） | AS 集成 / 数据丢失 |
| #20186 | MSC4354 sticky 在 `/sync` 与 sliding sync 改用**分片令牌**，避免同一事件出现在多个 sync 响应 | 同步正确性 |
| #20203 | 用户离开最后一个共享房后，告知客户端**删除本地缓存资料**（`profile_updates` 流新增 `LEFT_ROOM`，`record_profile_updates_txn` 从「单用户」改为「集合 + `target_users`」） | 隐私/陈旧数据；**✅ 已修（2026-10-17，零迁移）**：`MemberStoreApi::get_users_no_longer_sharing_rooms_since` 基于 `room_memberships.updated_ts` 推导「不再共享房间」集合，`build_profile_updates_extension` 输出 `users[uid] = null` |
| #20211 | `/preview_url`：媒体被**哈希自动隔离**时不得再回 `og:image`；并改为"直接丢弃下载物 + 删缓存文件（含空父目录）"而非落库后返回永远 404 的 `mxc://` | 内容安全 |
| #20231 | `limit_profile_requests_to_users_who_share_rooms` 依赖认证 ⇒ 未同时开启 `require_auth_for_profile_requests` 时**拒绝启动** | 安全（静默失效） |
| #20232 | 持久化**前**被 redact 的 sticky event 不得 sticky | 正确性 |
| #20233 | 新加入房间时把该房**全部** sticky events 下推 legacy `/sync` | 客户端一致性 |
| #20237 | 推送角标计数：摘要匹配键从 `thread_id` 改为 `(room_id, thread_id)`，修掉**跨房污染** | 通知计数 |
| #20241 | MAS 委派认证时 Admin API **拒绝**创建用户（403 `M_FORBIDDEN`） | 认证边界 |
| #20274 | `auth` 为 `null` 或非对象时，需要用户交互认证的端点**返回 500** → 改为 400 | API 健壮性 |
| #20289 | lazy-load 成员 + 旧 `since` 令牌时 membership 事件丢失 —— 引入 `LazyLoadedMembersCache`，**记录每个成员事件被发送时的 `next_batch` 位置**，请求开始时忘掉 `since` 之后记录的条目 | 同步正确性 |
| #20262 | Docker/worker：`GET` 单个延迟事件端点支持 worker | 部署 |

**Removals（1 条）**

| 编号 | 内容 |
|---|---|
| #20256 | **移除"可发送 sticky 状态事件"的能力** —— MSC4354 删掉了状态事件的 sticky 语义，`/send` 与 `/state` 上的 `sticky_duration_ms` 查询参数一并删除 |

**Internal / 性能 / 可观测（12 条，剔除纯 CI 与 Python 依赖类）**

| 编号 | 内容 | 对本仓的含义 |
|---|---|---|
| #19301 | URL 预览 HTML 解析换 `beautifulsoup4` | 实现形态不适用，#20211 的**语义**适用 |
| #20095 | 新增 `synapse_notifier_wait_for_stream_token_timeouts` 指标（按滞后 stream 打标签）+ 超时日志打印**滞后流明细** | 可观测缺口 |
| #20285 | `user_ips` 降低 prune 频率（减轻 DB 压力） | 本仓无该表 |
| #20252 / #20267 | 完成 Rust future / 从 Rust 调 Python 时**不在 tokio worker 线程取 GIL** | 本仓纯 Rust，**不适用** |
| #20253 | `cryptography` 46.0.7 → 50.0.1（CVE-2026-69248 / 69249 / 69247，官方称均**不影响** Synapse）+ `pyOpenSSL` 26.0.0 → 26.4.0 | **CVE 编号不适用于 Rust 依赖**，但"按版本对齐安全公告"的做法须在本仓用 `cargo deny` 复现 |
| #20259 / #20255 / #20258 / #20270 / #20293 | Twisted 25.5.0→26.4.0、dependabot 改月度合并、`PATTERNS` 类型注解、Complement go 依赖、`sytest` 构建依赖 | 构建/工具链，**不适用** |
| #20283 | 管理 API 删房 + `new_room_user_id` 时忽略挂起用户加入失败 | 边缘语义 |

**v1.162.0 遗留项（本轮一并复核，避免"只追最新版"造成漏检）**：状态决议缓存（#20185）、当前房间
状态单点缓存（#20160）、`/relations` 递归查询内联 join（#20182，大房 10–100x）、本地缩略图异步打开
（#20100）、`synapse_storage_stream_current_position`（#20097）、`/room_summary` 的 `join_rules`
陈旧（#20205）。

### 19.3 逐条对照（本仓实测）

判定标记：**✅ 已对齐** / **🟡 部分** / **❌ 差距** / **⚪ 结构性不适用**

| 上游条目 | 本仓现状（实测位置） | 判定 | 处置 |
|---|---|---|---|
| #20137 / #20286（宣告 v1.14/v1.15） | `CLIENT_API_VERSION_SUPPORT` 已含 `v1.1`…**`v1.17`**，且每条都有 `CLIENT_API_VERSION_EVIDENCE` 证据行 + 守卫（`synapse-services/src/capability_governance.rs`） | **✅ 已对齐（超前两版）** | 无 |
| #20165（sticky 联邦 catch-up） | EDU 类型已注册（`synapse-federation/src/edu.rs` 的 `EduType::StickyEvent` ↔ `org.matrix.msc4354.sticky_event`），出站广播在 `synapse-services/src/room/service.rs::broadcast_sticky_event_update`；**未实现"对错过 EDU 的远端补发"** | **🟡 部分** | §19.5 批次 C |
| #20227（MSC4075 推送规则） | 原本：`synapse-common/src/push_rules.rs` 的默认规则集共 19 条 `rule_id`，**无** MSC4075 两条；全仓检索 `4075` **0 命中**。**本轮已实现**（见 §19.5.1）：两条规则 + `experimental.msc4075_enabled` 开关（默认 **false**，与上游同向），`/versions` **不**宣告 `org.matrix.msc4075`（上游也不宣告 —— 见 §19.6(b)） | **✅ 已修（本轮）** | §19.5.1 |
| #20230（`task_scheduler.max_concurrent_tasks`） | 已落地：`synapse-common/src/config/task_scheduler.rs::TaskSchedulerConfig`（默认 5，拒绝 `0`/非正数），由 `synapse-services/src/wiring/admin.rs` 注入调度器 | **✅ 已修（2026-10-17）** | §19.5.1 |
| #20247（注销时取消延迟事件） | 注销链路：`synapse-web/src/routes/account_compat.rs` → `synapse-services/src/registration_service.rs::deactivate_account` → `synapse-services/src/auth/account.rs::deactivate_user`。该函数只做：置 `is_deactivated`、删 token、撤销 refresh token、删设备、清缓存；**全仓无 `cancel_all` / `cancel_for_user` 形态**，延迟事件零处理 | **❌ 差距（P0 之一，未做）** | §19.5 A1 |
| #20108（AS 回执分页） | `synapse-services/src/application_service/transaction.rs::deliver_pending_receipts` 从 `event_receipts_stream` 分页取数（每批 `MAX_RECEIPTS_PER_TRANSACTION`），按 `reached_token` 循环直至 `>= new_token`；首投只播种位点。`process_pending_for_service` 每 tick 调用 | **✅ 已修（2026-10-17）** | §19.5.1 |
| #20186（sticky 分片令牌） | 本仓 sticky 是 `(room_id, user_id, event_type)` 唯一键的表 + EDU 广播（`synapse-storage/src/sticky_event.rs`），同步侧未引入分片位置概念；本仓亦无 `MultiWriterStreamToken` 对应类型 | **❌ 差距（P1，形态不同）** | §19.5 批次 C |
| #20203（离开末个共享房 ⇒ 删除本地资料） | 本仓**没有** `profile_updates` 流（`migrations/00000000_unified_schema_v12.sql` 无该表）；sliding sync 的 `profile_updates` 扩展以 `users.updated_ts` 轮询实现（`synapse-services/src/sliding_sync_service/extensions.rs`、`synapse-storage/src/user/storage.rs::get_user_profiles_updated_since`）⇒ **无法表达 `LEFT_ROOM`**（"不再共享房间"不是一次资料写入） | **❌ 差距（P1，隐私）** | §19.5 批次 B |
| #20211（隔离媒体不回 og:image） | 本仓 `/preview_url` 原本是**桩**且返回**伪造的 OpenGraph**（`og:image` 由 `format!("{url}/preview.png")` 拼出）；**本轮已收口**（方案①，见 §19.5.1）：只回 `url`，删掉全部伪造字段与"Preview unavailable"合成分支。媒体隔离机制本身存在（`synapse-services/src/media/mod.rs`、`synapse-services/src/media/quarantine_tests.rs`） | **✅ 已修（本轮）** | §19.5.1 |
| #20231（profile 限制需认证） | 本仓该配置项**不存在**（`require_auth_for_profile_requests` / `limit_profile_requests_to_users_who_share_rooms` 均 0 命中）；等价能力走自研的 `user_privacy_settings`（`synapse-storage/src/privacy.rs::can_view_profile`，`contacts` 模式 = 同房 join），接线在 `synapse-web/src/routes/account_compat.rs::enforce_profile_visibility` | **⚪ 结构性不适用（等价物存在，缺"启动期一致性校验"这一形态）** | 登记，不排期 |
| #20232（持久化前 redact 不得 sticky） | 本仓无"持久化批次"概念；撤回即失效已有（`synapse-storage/src/event/redaction.rs` 与 §10.1 的既有判定） | **✅ 已对齐（无对应形态）** | 无 |
| #20233（新加入时下推全部 sticky） | 本仓 sticky 不注入 timeline；新加入房间的补发未实现 | **❌ 差距（P2）** | §19.5 批次 C |
| #20237（角标跨房污染） | 本仓未读计数为**房间粒度**（`synapse-services/src/room/summary/stats.rs::increment_unread_notifications`、`synapse-services/src/room/messaging/history_visibility.rs` 的可见性下界），全仓无 `unread_thread_notifications` 等 thread 级计数 ⇒ 不存在"`thread_id` 跨房比较"这一步 | **⚪ 结构性不适用** | 无 |
| #20241（MAS 下 Admin API 拒建用户） | MAS/委派认证运行时未接线（§18.4 ① 仍在册）⇒ 前提不成立；本仓 `is_admin` 建用户路径在 `synapse-web/src/routes/admin/user.rs` | **⚪ 结构性不适用（前提未落地）** | 与 MAS 接线同批 |
| #20274（`auth` 为 null/非对象 ⇒ 500） | 本仓 UIA 的 `auth` 是 `&Value`，`validate_auth` 全程用 `auth.get(...)` 取值（`synapse-services/src/uia_service.rs`），缺键即落到 `build_uia_response` 的 `M_UIA_REQUIRED` 分支；调用点如 `synapse-web/src/routes/device.rs::require_password_uia` | **✅ 已对齐（形态上不受该缺陷影响）** | 见 §19.7 V-19-1 |
| #20289（lazy-load + 旧 since 丢成员） | 原本：`synapse-services/src/sync_service/lazy_load.rs` 的缓存值是 **`HashSet<String>`（仅 user_id）**，并持久化到 `device_storage`；**没有事件 id、没有响应位置** ⇒ 客户端"没收到"与"已收到"无法区分，丢失即永久。**本轮已按上游语义重做**（见 §19.5.1）：条目改为 `成员 → (event id, 承载它的响应位置)`、每次请求按 `since` 修剪、初始同步不信任任何持久化行；持久化的位置由基线新增**可空**列 `last_sent_position` 承载。**同一修法的第二半也在本轮落地**："窗口外成员变更不回放"这条规则原先按 `timeline_limited` 一刀切，压到了 `state_after` 块上 —— 而 `state_after` 不叠 timeline，那条变更对它**永久丢失**（见 §19.5.1 的 B1b） | **✅ 已修（本轮）** | §19.5.1 |
| #20262（worker 支持取单个延迟事件） | 已在册：`synapse-services/src/worker/topology_validator.rs` 的 `DELAYED_EVENTS_WORKER_PATHS` 枚举了 7 条（含 `GET .../delayed_events/{delay_id}`），`docs/synapse-rust/ROUTE_CONTRACT.md` 同形 | **✅ 已对齐** | 无 |
| #20256（移除 sticky 状态事件） | 本仓 `set_sticky_events`（`synapse-web/src/routes/sticky_event.rs`）取 `event_type` + `event_id`，**本轮已加守卫**：目标事件带 `state_key` ⇒ 400 且不落库（纯谓词 `is_state_event` + 正负控单测）。另一处口径已更正：上游的 MSC4354 请求面是 `/send` 的 `org.matrix.msc4354.sticky_duration_ms` 查询参数，本仓**没有**那个面（见 §19.4.1 UG-10 与 §19.6(b)） | **✅ 已修（本轮）** | §19.5.1 |
| #20095（notifier 等待超时指标） | 全仓检索 `synapse_notifier` **0 命中**（本仓无 Twisted notifier 的"等 worker 追上流位置"机制）⇒ 该指标在本仓无对应观测对象 | **⚪ 结构性不适用（§19.6(b) 改判）** | 无 |
| #20285（`user_ips` 降频清理） | 全仓检索 `user_ips` **0 命中**（`.rs` 与 `.sql` 皆无） | **⚪ 结构性不适用（无该表）** | 无 |
| #20097（stream 当前位置指标） | 已在册：`synapse-common/src/metrics.rs` 与 `synapse-common/src/server_metrics.rs` 的 `synapse_storage_stream_current_position`（per-stream 标签），数据源 `synapse-storage/src/stream_positions.rs` | **✅ 已对齐** | 无 |
| #20182（`/relations` 递归 join 提速） | `recurse` 双拼写已实现（`synapse-web/src/routes/relations.rs::recurse_flag`），递归 CTE 在 `synapse-storage/src/relations/mod.rs`，已有 `EXPLAIN (ANALYZE)` 证据（§8.3 M-6 卡片） | **✅ 已对齐** | 无 |
| #20185 / #20160（状态决议 / 当前状态缓存） | 状态决议结果缓存已交付（`synapse-services/src/room/state_record.rs`，§18 的 M-5）；**"当前房间状态单点查询缓存"未单独建** —— 但本仓有共享 `room_state` 缓存（`synapse-services/src/room_state_cache.rs`，§18 的 V-10 已复用） | **✅ 已对齐（复用同一实现）** | 无 |
| #20100（本地缩略图异步） | 本仓缩略图走 `synapse-services/src/media/mod.rs::get_thumbnail` → 媒体服务；未见阻塞式打开 | **✅ 形态上已对齐** | 见 §19.7 V-19-3 |
| #20246（tracing "Closing scope" 误报） | 该告警来自上游 twisted/tracing 桥接层 | **⚪ 结构性不适用** | 无 |
| #20252 / #20267（GIL）、#20259 / #20354（Twisted / Redis 客户端库）、`beautifulsoup4`、#20255 / #20258 / #20270 / #20293 | Python / 依赖 / CI 专属 | **⚪ 结构性不适用** | 无 |

### 19.4 真实剩余差距

#### 19.4.1 功能（按优先级）

> ID 前缀 `UG-`（Upstream Gap），**只在 §19 内引用** —— 与文档其他章节的 `G-1`（房间版本）、
> `U-n`（遗留问题）、`M-n`/`L-n`（优化项）**不是同一套编号**。

| ID | 差距 | 为什么是真差距 | 证据 |
|---|---|---|---|
| **UG-1（P0）** | **账户注销只置标志位**：不退出/踢出房间、不清推送器、不解除 3PID、不取消 MSC4140 延迟事件 | **已修（2026-10-16，见 §19.5.1）**：`synapse-services/src/account_deactivation_service.rs` 落地完整管线；退房已改为**异步可重试**（UG-11，2026-10-17） | `synapse-services/src/account_deactivation_service.rs`；`synapse-services/src/auth/account.rs` |
| **UG-11（P2，UG-1 的残余）** | 注销的**账户级数据面**：threepid 解绑/删除、账户数据（`account_data` + `room_account_data`）与推送规则清理、线程订阅与 E2EE 备份键删除、用户目录摘除、**待处理邀请/敲击的拒绝**；以及**退房的异步可重试模型** | **已全部落地（2026-10-17，见 §19.5.1「A1 残余收尾」与 R11）**：① 数据面 —— `StorageUserDataPurger`（六条 `user_id` 全量 `DELETE`）+ 既有 `remove_threepids_by_user`，接在不可逆 teardown 之前；② 待处理邀请/敲击 —— `get_pending_rooms()` 驱动自我 `leave`（即上游 `reject_invite`/`retract_knock`），且**保留在同步路径**（与上游一致）；③ 退房 —— 新增 `user_pending_deactivations` 表 + `UserDeactivationStorage` + `AccountDeactivationService::process_pending_parts()`（上游 `_user_parter_loop`），注销请求只入队后立即返回，服务器启动时与每个维护 tick（或入队 `Notify`）排空，进程重启可恢复。**用户目录**为 dead schema（全仓无 Rust 读写），行为缺口已由每次目录查询的 `is_deactivated` 过滤满足 ⇒ 不写无效 `DELETE` | 见 §19.5.1 的 A1 实施记录与 R11 |
| **UG-2（P1）** | **MSC4075 默认推送规则缺失** | **已修（2026-10-16，见 §19.5.1 A4）**：`synapse-common/src/push_rules.rs` 落地两条 MatrixRTC 规则（`notify` + `set_tweak: sound=ring`，含 `invite_for_me` 的具体 user id 与 `invite_for_room` 的权限门），并穿到 `/pushrules`、`/sync`、`account_data` 三条读取面；开关 `experimental.msc4075_enabled` 默认 `false`（与上游同向）。要真正来电响铃需显式开启该键 | `synapse-common/src/push_rules.rs` |
| **UG-3（P1）** | **MSC4354 实现存在但未在 `/versions.unstable_features` 宣告** | **已修（2026-10-16，见 §19.5.1 A2）**：`msc4354_capability()` 按**路由面**推导（而非硬写 `true`）+ `build_client_versions` 插入 `org.matrix.msc4354`；正负控判据见 A2 | `synapse-services/src/capability_governance.rs` 的 `BASE_UNSTABLE_FEATURES` |
| **UG-4（P1）** | **AS（MSC2409）临时数据投递未接线** | **已修（2026-10-17）**：`ApplicationServiceManager::deliver_pending_receipts` 从回执流分页取数并按 `reached_token` 循环投递（首投只播种位点、不回溯），`process_pending_for_service` 每个 tick 调用；配置键解析见 §19.5.1 的配置对齐。残余形态见 P-5 | `synapse-services/src/application_service/transaction.rs`、`scheduler.rs` |
| **UG-5（P1）** | **lazy-load 成员缓存缺少"客户端是否收到"的位置维度** | **已修（2026-10-16，见 §19.5.1）**：条目改为 `成员 → (event id, 承载它的响应位置)`，每次 `/sync` 请求按 `since` 修剪"位置晚于 since"的条目；初始同步清空并**不信任**持久化行。持久化的位置落在基线新增的可空列 `last_sent_position` 上（可空 ⇒ 与"真实位置 0"可区分；存量行 `NULL` 即"未记录"、一律不抑制） | `synapse-services/src/sync_service/lazy_load.rs`；`synapse-storage/src/device/mod.rs` |
| **UG-6（P1）** | **`/preview_url` 返回伪造的 OpenGraph** | **已修（2026-10-16，见 §19.5.1 B2）**：`preview_url` 只回 `url`（2026-10-22 起改为**真实抓取**：`synapse-services/src/url_preview.rs`），删掉拼出来的 `og:image` 与占位 `title`/`description`，也删掉了路由层"Preview unavailable"的合成分支。**仍为不抓取的桩**：返回 `{url}` 是"无伪造数据"的诚实形态，不是完整抓取实现；DNS-rebinding 钉扎在将来真正抓取时才需要 | `synapse-web/src/routes/media/preview.rs`、`synapse-services/src/media_service.rs` |
| **UG-7（P2）** | MSC4354 联邦 catch-up（#20165）与新加入房间补发（#20233）缺 | **已实施（2026-10-17，见 §19.5.1「批次 C 实施记录」，C1 改最小闭环）**：C1 复用 `federation_queue` 做启动/定期重放（重放**已落库事务**，非 per-destination 位点增量补发）；C2 收敛为「`/sync` sticky 集合 == `GET .../sticky_events` 集合」（**按用户置顶语义不变**）；C3 写「单写者假设」注释 + 唯一键单行回归。**残余**：无逐 destination 的 sticky 增量补发；本仓 sticky 仍是**按用户**置顶，非上游的房级广播 | `synapse-services/src/room/service.rs`、`synapse-federation/src/edu.rs` |
| **UG-8（P2）** | `task_scheduler.max_concurrent_tasks` 缺 | **已修（2026-10-17，见 §19.5.1）**：`synapse-common/src/config/task_scheduler.rs` 落地 `TaskSchedulerConfig { max_concurrent_tasks }`（默认 5，自定义反序列化拒绝 `0` 与非正数），由 `synapse-services/src/wiring/admin.rs` 注入应用服务调度器 | `synapse-common/src/config/task_scheduler.rs`、`synapse-services/src/wiring/admin.rs` |
| **UG-9（P2）** | `synapse_notifier_wait_for_stream_token_timeouts` 缺 | worker 等不到流位置时只有日志没有指标 ⇒ 无法告警**（§19.6(b) 已改判：本仓无 Twisted notifier 的"等 worker 追上流位置"机制，该指标无观测对象 ⇒ ⚪ 结构性不适用）** | `src/server/mod.rs` 与 `synapse-common/src/metrics.rs` 的指标清单 |
| **UG-10（P1）** | **缺 MSC4354 的标准请求面**：上游用 `/send?org.matrix.msc4354.sticky_duration_ms=…` 把事件本身标记为 sticky，并把 sticky TTL 放在事件的 `unsigned` 上；本仓原先**不读这个查询参数**，改用自研的 `POST/DELETE/GET .../rooms/{room_id}/sticky_events`（`(room_id, user_id, event_type)` 唯一键的按用户置顶表） | **已修（2026-10-16，方案 ⓐ 补上游面，见 §19.5.1）**：`PUT /rooms/{roomId}/send/{eventType}/{txnId}` 现读 `?org.matrix.msc4354.sticky_duration_ms=<ms>`（含短名 `sticky_duration_ms` 回退），命名空间字段优先，非整数/`<=0`/越界 → 400 `M_INVALID_PARAM`，并把 TTL 写入事件的 `unsigned.msc4354_sticky_duration_ttl_ms` 投影。~~**残余**：上游的 `/state` 面本仓**未接线**~~ → **订正（2026-10-17，`a3664aca7`）**：`/state` 面**已接线**（四条写路径，与 `/send` 同一解析器）；既有私有 `sticky_events` 路由**保留**；`/sync` 的 sticky 标记仍由私有 `room_sticky_events` 表驱动，`/send` 的查询参数只落 `unsigned` 投影 | §19.5 C4（**已决策 ⓐ**：补齐上游面）；`/state` 面残余**已订正**（见 §19.7 V-19-9） |

#### 19.4.2 性能

| ID | 项 | 本仓现状 | 与上游差距 |
|---|---|---|---|
| **P-1** | `/relations` 递归查询 | 递归 CTE 已在递归内 join `events`，有 50k 事件夹具的 `EXPLAIN` 证据 | **无差距**（#20182 语义已覆盖） |
| **P-2** | 状态决议 / 当前状态缓存 | 决议缓存 + 共享 `room_state` 缓存 | **无差距**（#20185 / #20160 语义已覆盖） |
| **P-3** | 缩略图/媒体打开 | 未见阻塞式打开 | 形态上对齐（待验证，见 §19.7 V-19-3） |
| **P-4** | lazy-load 成员过滤 | 缓存命中率高但**正确性有洞**（UG-5）；若为修正确性改成"每次都回源"，会把缓存查询退化成每房一次 `device_storage` 往返（`get_lazy_loaded_members` 已带 `LruCache` 兜底，故回源只在进程内未命中时发生） | 修 UG-5 时必须**保留**进程内 `LruCache`，只把条目结构从 `HashSet` 换成 `(event_id, next_batch)` 映射 |
| **P-5** | AS 回执投递 | **已接线（2026-10-17，UG-4）**：`deliver_pending_receipts` 照抄上游语义 —— 从 `event_receipts_stream` 分页取数（`MAX_RECEIPTS_PER_TRANSACTION`）、按 `reached_token` 循环并在 `reached_token >= new_token` 退出（不会忙循环）；首投只播种 `read_receipt_position`、不回溯历史。由 `process_pending_for_service` 每 tick 调用，失败非致命 | 结构已对齐上游 |
| **P-6** | DB 维护 | 本仓无 `user_ips` 表 ⇒ #20285 无对象；但**后台清理任务本身**是否按"降低频率"的同一思路评估过，未取证 | 无需动作 |

#### 19.4.3 安全

| ID | 项 | 本仓现状 | 判定 |
|---|---|---|---|
| **S-1** | Rust 依赖安全公告 | `cargo deny check advisories` → **`advisories ok`**（2026-10-16 实测；`deny.toml` 在仓，pre-push 同款门禁） | **无未处理公告**（⚠️ 该结论依赖本地 advisory 库的抓取时间，见 V-19-7） |
| **S-2** | 上游 #20253 的 CVE（`cryptography` 三枚） | 均为 Python 包 CVE，**不映射**到本仓 Rust 依赖 | 不适用 |
| **S-3** | profile 请求限制的静默失效（#20231） | 本仓配置项不存在，不会"配了不生效"；等价策略在 `synapse-storage/src/privacy.rs::can_view_profile` 且已有调用点（`synapse-web/src/routes/account_compat.rs`） | **无该风险** |
| **S-4** | 隔离媒体的 `og:image`（#20211） | 被 UG-6 覆盖（桩实现根本不查隔离状态） | 由 UG-6 承接 |
| **S-5** | 注销用户仍可能触发已排期的延迟事件 | 上游视为安全问题（"不要在注销流程进行中发出"）而优先取消；本仓未做 | **由 UG-1 承接（P0）** |
| **S-6** | 账号枚举 / 第三方规则 / SSRF 面 | 前几轮已收口（§18.3 #21、§18.7.1）；本轮无新增面 | 无 |

### 19.5 可执行优化方案

> 每批**独立提交**、独立验证；改路由字面量或默认规则集时按 §5 的"连带重生成"清单同步契约链
> （标注 → 3 张派生表 → 6 份 ledger fixture → `docs/synapse-rust/ROUTE_CONTRACT.md`
> → `docs/openapi/route-table.json`），**只做一半是既有最高频的红**。

**批次 A —— 正确性/安全（P0+P1，建议先做）**

1. **A1（UG-1）注销语义补全**（在 `synapse-services/src/registration_service.rs::deactivate_account`
   内按固定顺序编排，任一步失败按"已完成步骤可重入"设计）：
   `cancel_all_for_user(user_id)`（**先做**，对齐上游注释"不要让延迟事件在注销过程中发出"）
   → 离开全部房间（发 `leave` 成员事件，走既有 membership 服务，避免绕过事件鉴权）
   → 删除该用户全部 pusher → 清未读计数 → 删设备与 token（**已有，顺序挪到最后**）。
   验收：① 延迟事件不再触发；② 用户不再是任何房的 `join`；③ 服务重启后状态可复现（不依赖内存）。
   **变异自证**：注释掉 `cancel_all_for_user` ⇒ 必须有用例变红。
2. **A2（UG-3）MSC4354 能力宣告**：在 `BASE_UNSTABLE_FEATURES` 增加 `org.matrix.msc4354`
   （取值与该功能真实可用性一致，**不要**写成常量 `true` 而绕过配置），同时核对 sticky 路由
   实际注册面（当前挂在 `/_matrix/client/unstable/org.matrix.msc4354/...`）。
   验收：`GET /_matrix/client/versions` 含该键；`capability_governance` 的单测覆盖三态。
3. **A3（#20256 的剩余一半）sticky 状态事件守卫**：`set_sticky_events` 在写入前拒绝
   `state_key` 存在的目标事件（400），错误码与文案按"该能力已从 MSC4354 移除"给出。
   验收：状态事件请求 ⇒ 400 且不落库；普通 `m.room.message` 不受影响（负控）。
4. **A4（UG-2）MSC4075 默认推送规则**：在 `synapse-common/src/push_rules.rs` 的 override 段、
   `.m.rule.is_user_mention` **之前**插入两条：
   - `org.matrix.msc4075.rule.rtc.invite_for_me`：`event_match(type = org.matrix.msc4075.rtc.notification)`
     ＋ `event_property_contains_type(content.m.mentions.user_ids, UserId)` ⇒
     `["notify", {"set_tweak":"sound","value":"ring"}]`
   - `org.matrix.msc4075.rule.rtc.invite_for_room`：同上 type ＋
     `event_property_is(content.m.mentions.room = true)` ＋ `sender_notification_permission(room)` ⇒ 同上动作

   两条都用 `default: true` / `enabled: true`；并加配置开关与"开关关闭 ⇒ 不下发"的行为断言。
   **迁移注意**：`default_push_rules_for_user` 被 `synapse-web/src/routes/push_rules.rs` 与
   `synapse-services/src/sync_service` 共用，改默认集必然波及推送规则快照类用例，须同批更新。
5. **A5（S-1）安全公告纳入例行**：把 `cargo deny check advisories` 的实跑输出（版本 + 时间）
   记入审计文档；**不要**只写"已检查"。

**批次 B —— 数据正确性与隐私（P1）**

1. **B1（UG-5）lazy-load 缓存结构升级**：把条目从 `HashSet<String>` 改为
   `{ user_id -> (event_id, sent_at_room_position) }`，并在每次 `/sync` 请求开始时按 `since`
   丢弃"位置晚于 `since`"的条目（对齐上游 `LazyLoadedMembersCache::start_request`）。
   **必须**同时保留进程内 `LruCache`（避免正确性修复退化成每房一次 DB 往返，见 P-4）。
   验收：构造"客户端拿到响应但没保存 token 就重登"的场景 ⇒ 成员事件必须重发；
   红证明：把 `start_request` 改成不丢弃 ⇒ 用例变红。
2. **B2（UG-6）`/preview_url` 收口（两选一，须显式决策）**
   - **方案 ①（推荐，低风险）**：维持桩，但**只回 `url`**，删掉全部伪造字段
     （`title` / `description` / `og:*` / `matrix:image:size`），并保持端点默认关闭
     （现状 `experimental.msc4452_enabled = false`）。
   - **方案 ②**：实现真实抓取 —— 必须走已验证 IP 的**钉扎客户端**（`check_url_and_resolve`
     返回的 `verified_ips` → 钉扎），遵守重定向次数与体积上限，HTML 解析走独立限流，
     并在媒体命中"哈希隔离"时**既不落库也不回 `og:image`**（对齐 #20211）。
   - ⚠️ 无论哪条：**不得**保留 `format!("{url}/preview.png")` 这类拼造字段。
3. **B3（UG-4）AS 回执投递接线**：新增"从回执流取数 → 填 `AppServiceEphemeralBatch.receipts`
   → 经 `synapse-services/src/application_service/scheduler.rs` 投递"的生产路径，
   并照抄上游三要素：① 每 AS 记 `read_receipt` 流位置；② 单次取数**有上限**并返回
   `reached_token`；③ 外层 `while` 直到 `reached_token >= new_token` 才停（**退出条件必须钉死**，
   否则退化为忙循环）。首次投递取最新（`BACKWARDS`）以免回填全历史。
   验收：构造单个流位置 >100 条回执 ⇒ 全部投递且不重复；负控：位置不推进时不进入无限循环。
   - **本轮状态（2026-10-16）：范围收窄，未接线**。经裁定，本轮只做「**配置键对齐**」——
     `appservice_receives_ephemeral(service)` 现认 4 个键（上游键 `de.sorunome.msc2409.push_ephemeral`、
     mautrix 键 `receive_ephemeral`、命名空间 `de.sorunome.msc2409`、`ephemeral`），
     使"按上游键配置的 AS"不再被判为不接收 ⇒ 上述三要素（分页 / `reached_token` 循环 /
     退出条件）**仍未实现**，`deliver_transaction_with_edus(…, None)` 依传 `None`。残余见 P-5。

**批次 C —— 联邦与同步一致性（P2）**

1. **C1（UG-7a）sticky 联邦 catch-up**：记录"本机已向某远端同步到哪个 sticky 位置"，
   远端重连/回执成功后补发缺失 EDU；**不要**用"每次广播全量 sticky"代替（会放大联邦流量）。
2. **C2（UG-7b）新加入房间补发 sticky**：新成员加入时，把该房当前全部 sticky 事件按其
   `event_type` 投影进该成员的同步结果（本仓 sticky 不注入 timeline，故投递点选在同步装配侧
   而非事件流侧），并保证与 `GET .../sticky_events` 的可见集合一致。
3. **C3（#20186 的形态差异）**：本仓 sticky 表是单机唯一键模型。若**将来**要上多写者/多 worker
   部署，再引入"分片位置"（`instance_name` + 每写者位置）；**现在不做**，只把
   "同一 sticky 事件可能重复出现在多个 sync 响应"这一风险写成注释与回归用例，
   避免后续无人知道上游为何改成分片令牌。
4. **C4（UG-10）已决策 ⓐ 补上游面（2026-10-16 实施）** 二选一，二者不能只做一半：
   - **方案 ⓐ 补上游面（已采纳）**：`/send` 读 `org.matrix.msc4354.sticky_duration_ms`，
     把 sticky TTL 落进事件的客户端投影（`unsigned`），并让现有 `sticky_events` 路由继续服务私有置顶；
     风险＝两套 sticky 语义并存，必须写清"哪一个决定 `/sync` 里的 sticky 标记"。
     - **实施范围**：`PUT /rooms/{roomId}/send/{eventType}/{txnId}` 解析
       `?org.matrix.msc4354.sticky_duration_ms=<ms>`（命名空间字段优先、短名 `sticky_duration_ms` 回退），
       非整数 / `<=0` / 超 `MAX_STICKY_DURATION_MS` → 400 `M_INVALID_PARAM`，落盘写入
       事件 `unsigned.msc4354_sticky_duration_ttl_ms` 投影（`update_event_unsigned`）。
     - **语义裁定（哪一套决定 `/sync`）**：`/sync` 里的 sticky 标记**仍由私有 `room_sticky_events` 表驱动**
       （per-user 置顶模型）；`/send` 的查询参数**只落 `unsigned` 投影**，不写 `room_sticky_events`，
       故**不改变 `/sync` 的 sticky 归属**。两套语义并存、互不覆写，避免既有一致性被打破。
     - ~~**残余**：上游的 `/state` 面本仓**未接线**（该查询参数只在 `/send` 解析），登记于 §19.7 V-19-9。~~
       **订正（2026-10-17，`a3664aca7`）**：`/state` 面**已接线** —— `state.rs` 的四条写路径
       （`put_state_event` / `put_state_event_empty_key` / `put_state_event_no_key` / `send_state_event`）
       都调 `msc4354_query_sticky_duration_ms`，与 `/send` 同一解析器（同一组边界与 400 语义）。
       护栏见 `tests/integration/api_sticky_state_event_tests.rs`（3 条，覆盖三种写入形态）。
   - **方案 ⓑ 归位私有（未采纳）**：承认现有路由是非上游扩展，把它从
     `/_matrix/client/unstable/org.matrix.msc4354/...` 迁到 `/_matrix/vendor/v1/...`
     （本仓既有惯例，ISSUE-13 已对 3 条路由这么做过），并**撤销** A2 的 `org.matrix.msc4354`
     宣告 —— 否则等于用一个 MSC 号宣告了与 MSC 不同的东西。
   - 决策输入：HuLa 客户端的置顶 UI 用的是什么形态（若它只用私有路由，ⓑ 更诚实；
     若还要与上游/Element 互通，必须 ⓐ）。**本轮裁定：选 ⓐ（对齐上游面）**。

**批次 D —— 运维与可观测（P2，低成本）**

1. **D1（UG-8）**：新增 `task_scheduler` 配置段，键名 `max_concurrent_tasks`，默认 5，
   **拒绝布尔与非正整数**（上游为此写了显式分支 —— YAML 的 `true` 在弱类型语言里会被当 `1`，
   本仓 Rust 侧同样要把"能反序列化成整数但语义是布尔"的输入挡掉）。
2. **D2（UG-9）**：新增 `synapse_notifier_wait_for_stream_token_timeouts`（按滞后的 stream
   打标签）+ 超时日志带上**滞后流的当前值与目标值**（上游为此写了 `_describe_lagging_streams`，
   因为"等不到"的现场只有这一行日志能还原）。
3. **D3**：把本批新增指标补进 monitoring 面板与规则白名单的**同一提交**（本仓既有教训：
   Prometheus 不自动重载规则、面板指标名是静态正则校验，漏一处即静默失配）。

**批次 E —— 文档与门禁（与 A–D 同步）**

1. 每批完成后更新 `CHANGELOG.md` 与 `docs/audit/INDEX.md`，并复跑 §19.8 的验证清单。
2. 本文档计数与 `docs/synapse-rust/ROUTE_CONTRACT.md` 的一致性由
   `tests/unit/doc_credibility_guard_tests.rs` 守卫，改动路由面时**必须**同批重生成。

### 19.5.1 本轮实施记录（2026-10-16）

| 项 | 状态 | 交付物与判据 |
|---|---|---|
| A2（UG-3）MSC4354 能力宣告 | ✅ 已实施 | `synapse-services/src/capability_governance.rs` 新增 `msc4354_capability()`（**路由面推导**，非硬写常量）+ `build_client_versions` 插入 `org.matrix.msc4354`；判据 `test_msc4354_is_declared_from_the_sticky_route_surface_only` —— **正负控**：注册 sticky 写路由 ⇒ `true`，同 config 下不注册 ⇒ `false`（硬写 `true` 或 `false` 都不可能同时满足） |
| A3（#20256 剩余一半）sticky 状态事件守卫 | ✅ 已实施 | `synapse-web/src/routes/sticky_event.rs`：纯谓词 `is_state_event` + 400 分支（不落库）；判据 `test_state_event_is_detected_by_a_string_state_key` 与 `test_timeline_event_is_not_a_state_event`（**负控**：`state_key: null`、缺键、非字符串一律不得被判为状态事件，否则普通置顶消息会被误拒） |
| A4（UG-2）MSC4075 默认推送规则 | ✅ 已实施（**默认关闭，与上游同向**） | `synapse-common/src/push_rules.rs`：两条规则 + `MSC4075_RTC_RULE_IDS` + `remove_msc4075_rtc_rules`；开关 `experimental.msc4075_enabled`（默认 `false`）穿到 `/pushrules`、`/sync`、`account_data` 三条读取面（`synapse-web/src/routes/push_rules.rs`、`synapse-web/src/routes/push.rs`、`synapse-web/src/routes/account_data.rs`、`synapse-services/src/sync_service/data_fetch.rs`）。判据 6 条：规则存在性、动作精确等于 `["notify", {"set_tweak": "sound", "value": "ring"}]`、`invite_for_me` 带**具体 user id**（不是 `value_type` 模板）、`invite_for_room` 带房间通知权限门、**顺序必须排在 `.m.rule.is_user_mention` 之前**、开关关闭后已持久化的默认行也停发。开关留 false 是**有意**的：上游 `experimental.msc4075_enabled` 默认也是 `False`；要真正拿到来电响铃把该键置 `true` |
| A5（安全公告） | ✅ 已做 | `cargo deny check advisories` → `advisories ok`（2026-10-16 实跑）；advisory 库新鲜度仍列为 V-19-7 |
| B2（UG-6）`/preview_url` 收口 | ✅ 已实施（方案①：先止损）→ **2026-10-22 升级为真实抓取（方案②）** | 2026-10-16 先只回 `url`（删掉 `title`/`description`/`og:*`/`matrix:image:size` 与拼出来的 `og:image`；`synapse-web/src/routes/media/preview.rs` 同时删掉"Preview unavailable"合成分支（该分支原先也能产出 OpenGraph 形状的伪造字段）。**无需重生成 api 台账**：`scripts/api_test/response_schemas.json` 把该端点记为 403（默认关闭），schema 是错误形状 |
| A1（UG-1）注销语义 | ✅ **已实施**（2026-10-16） | 新增 `synapse-services/src/account_deactivation_service.rs`（见下方实施记录）；路由改调它；`RegistrationService::deactivate_account`（只有"置标志位"的旧入口）**已删除**，避免两条路径通向同一状态变更 |
| B1（UG-5）lazy-load 缓存位置维度 | ✅ **已实施**（2026-10-16） | 缓存条目改为 `成员 → (event id, 承载它的响应位置)` + 每请求按 `since` 修剪（上游 `LazyLoadedMembersCache::start_request` 语义）；持久化位置走基线新增的**可空**列 `last_sent_position`（见下方实施记录） |
| B1b（#20289 第二缺陷）`state_after` × 截断时间线 | ✅ **已实施**（2026-10-16） | 判据收敛为一个纯函数 `state_block_describes_the_timeline_start(limited, state_after) = limited && !state_after`；`LazyLoadMembersRequest` 新增 `state_after`，两处调用点接 `use_state_after`。判据：真值表 4 项 + 走**异步** `apply_lazy_load_members` 的正反两控（`timeline_limited_replays_membership_changes_only_for_state_after`） |

**A1 实施记录（2026-10-16）**

管线顺序（与上游 v1.163.0 的差异都在注释里写明理由）：

1. `DelayedEventService::cancel_all_for_user()` —— **最先**且 fail-fast（对齐上游注释"不要让它们在注销过程中发出"）。
   新增方法复用 `list_pending_delayed_events_for_user` + `cancel_delayed_event`，**不新增 SQL**：
   单行取消路径就是客户端 `action: cancel` 走的同一条路，写第二份 bulk `UPDATE` 会在"什么算 pending"上漂移。
2. 退出全部已加入房间 **并拒绝待处理的邀请/敲击** —— **逐房 best-effort**（列出失败仍是致命，见下）。
   走 `RoomService::membership()`（状态机 + 事件鉴权 + summary 刷新），不绕过去。
3. `PushStoreApi::delete_all_pushers_for_user()` —— fail-fast。**新增静态 SQL**
   `DELETE FROM pushers WHERE user_id = $1`（`pushers` 表无外键指向 `devices`，删设备不会连带删）。
4. `StorageUserDataPurger::purge()` —— 账户数据 / 房间账户数据 / 推送规则 / 线程订阅 / threepid /
   备份键，fail-fast（**2026-10-17 补入，见下方「A1 残余收尾」**）。
5. `CredentialAuth::deactivate_user()` —— 置标志位 + 撤 token + 删设备，**最后**（不可逆步骤放最后，见下）。

**两处刻意的顺序/容错设计**（都有单测锁定）：

- **不可逆步骤放最后**：上游是先标记注销、再**异步**退房（`add_user_pending_deactivation` + 后台循环重试）。
  本仓没有那张重试表，所以把顺序对调 —— 否则"已注销但仍在房间里"会永久卡住，且没有任何机制补完。
  判据 `irreversible_teardown_is_the_last_step`（**只看终态无法区分顺序**，故断言调用日志）。
- **列表失败致命、单房失败容忍**：列出房间失败是瞬时（DB 抖动）⇒ `Err` 让用户重试；
  单房退出失败通常是永久的（坏房、鉴权拒绝、远端不可达）⇒ 记 `warn` 并继续，
  否则用户永远注销不掉。判据 `a_failed_room_listing_aborts_before_the_irreversible_step` 与
  `a_single_unpartable_room_does_not_block_deactivation`。

**数据面连带（唯一需要动 SQLx 的地方）**：`.sqlx/` 新增 1 条
（`query-27e9366f…json`，`DELETE FROM pushers WHERE user_id = $1`，参数 `Text`、无返回列）；
`scripts/ci/sqlx_dynamic_ratio_baseline` 的 `BASELINE_STATIC` **1354 → 1361**（本批 +1，
其余 +6 是本文件上次钉版 2026-10-15 之后其它已提交批次累积的未钉版实测值，已在该文件注释里注明）；
`docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` 的数字块同步（`BASELINE_STATIC=1361`、
`SQLX_CACHE_ENTRIES=1328 → 1329`），由 `scripts/ci/check_plan_numbers_fresh.sh` 核验（已跑通）。

⚠️ **`.sqlx` 生成的坑（实测）**：在 `synapse_sqlx_scratch` 上跑 `scripts/ci/sqlx_prepare.sh` 时，
`cargo sqlx prepare` 先清空 `.sqlx/` 再逐条 describe，本轮那次 describe 中途失败
（`synapse-storage` 12 处 E0282），脚本按设计回滚了快照 —— 但磁盘上仍一度少了 186 个条目，
且被环境的批量删除屏障拦了一下。处置：`git checkout -- .sqlx` 恢复已跟踪条目，
保留脚本已写出的**那 1 个**新条目（已核对内容正确），再用 `cargo check`（`SQLX_OFFLINE=true`）
证实缓存完整。

**A1 残余收尾（UG-11，2026-10-17）**

上一批把管线的**骨架**补齐后，登记了六项账户级残余（UG-11）。本轮按裁定「**数据清理 5 项，
退房保持同步**」收尾——只补"数据面"，不动"退房的同步/异步"（见文末残余）。

1. **账户级数据清理**：新增 `synapse-services/src/account_deactivation_service.rs` 的
   `UserDataPurger` seam + 生产实现 `StorageUserDataPurger`，接在管线**第 4 步**（清推送器之后、
   不可逆 teardown 之前），fail-fast。**为什么是"全量删"而不是复用既有 delete**：既有入口都是
   **键形**的（`delete_account_data` 要 `(user_id, data_type)`、`delete_push_rule` 要
   `(user_id, scope, kind, rule_id)`），而注销者手上**只有 user_id**、并不知道该用户有哪些
   `data_type`/规则 ⇒ 六条 `WHERE user_id = $1` 的静态 `DELETE` 是本场景唯一可行的形状。
   级联表不单独删：`backup_keys` 随 `key_backups`（FK `ON DELETE CASCADE`）、
   `secure_backup_session_keys` 随 `secure_key_backups` 走。
2. **threepid**：复用**既有** `ThreepidStorage::remove_threepids_by_user()`（无需新增 SQL）。
3. **待处理邀请/敲击的拒绝**：`RoomParting` / `MembershipService` 扩一个
   `get_pending_rooms()`，`part_rooms` 的目标集合由 `joined` 改为 **`joined ∪ pending`**（二者
   按 membership 取值天然不交）。**不必发明新迁移**：一次自我 `leave` 从 `invite`/`knock` 出
   发就是上游的 `reject_invite` / `retract_knock`，`synapse_common::membership_transition`
   已放行该转换。
4. **用户目录摘除——为什么不做**：`user_directory` 表在本仓**没有任何 Rust 读写**（dead schema），
   上游删该行想达到的用户可见效果（"搜不到这个已注销用户"）本仓已由目录查询的
   `COALESCE(is_deactivated, FALSE) = FALSE` 过滤**天然满足**。对一张没人读的表写 `DELETE`
   是**有动作无效果**，故刻意省略，改为在模块文档里写明"机制就是那个标志位、由第 5 步置上"。
   这与"不写无效 SQL"一致，也是本轮**没有**把它算进"5 项数据清理"的原因。

**判据**（`account_deactivation_service.rs` 的 `#[cfg(test)] mod tests`，全离线）：
`deactivate_rejects_pending_invites_and_knocks`（pending 房也进 part 目标集）、
`deactivate_purges_account_data_and_reports_the_counts`（purge 在 teardown **之前**且计数回填
`DeactivationReport.purged`）、`a_failed_purge_aborts_before_the_irreversible_step`（purge 失败
⇒ `Err` 且**未**调用不可逆步），以及被同步更新的顺序/容错不变式（`irreversible_teardown_is_the_last_step`、
`deactivate_cancels_…`、`deactivate_is_a_noop_…`、`a_failed_room_listing_…`）。

**数据面连带**：`.sqlx/` 新增 **6** 条（上述六条 `DELETE`），经唯一入口 `scripts/ci/sqlx_prepare.sh`
重生成；`scripts/ci/sqlx_dynamic_ratio_baseline` 的 `BASELINE_STATIC` **1361 → 1367**；
`docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` 数字块同步（`BASELINE_STATIC=1367`、
`SQLX_CACHE_ENTRIES=1328 → 1334`），由 `scripts/ci/check_plan_numbers_fresh.sh` 核验（已跑通）。

⚠️ **一处环境事实（本轮踩到，供后人省事）**：重生成时**不能**用 `test_template_ci` schema ——
它是**旧迁移的副本**（`global_invite_blocklist` 只建了 `id/user_id/created_ts`，缺
`reason`/`created_by`），`cargo sqlx prepare` 会对不上列的查询吐一批**误导性的** E0282
（看起来像源码坏了）。正解是用一个**与当前基线一致**的已迁移库（本轮用 scratch 库
`synapse_prepare.public`，218 表、列齐），prepare 一次通过、无缩容。

**UG-11 的退房模型（2026-10-17 后续收尾，见 R11）**：上游"退房"是**异步可重试**的
（`add_user_pending_deactivation` 表 + 后台 `_user_parter_loop`）。上一批按裁定先保持同步；
本轮已改为同一模型 —— 注销只入队后立即返回，服务器启动时与每个维护 tick 排空，进程重启可恢复。
详见 R11。

**B1 实施记录（2026-10-16）**

上游语义（逐行取证 `synapse/handlers/sync.py` v1.163）：缓存按 `(user, device)` 一份，
条目 = `成员 → (最后发出的 event id, 承载它的那次响应的 next_batch 位置)`，
内部上限 100、`ExpiringCache` 30 分钟；`start_request(since)` 在 `since` 缺失时**全清**、
否则删掉位置晚于 `since` 的条目；过滤判据是"变更过的成员无条件发，其余看
`was_sent(member, event_id)`"。

本仓落地（与上游的每处差异都写了理由）：

1. 缓存键仍是 `(user, device, room)`（比上游细一档，且与持久化表的粒度一致）；
   `start_request` 因此在 `apply_lazy_load_members` 里按房调用。
2. `mark_sent` 记的位置是**本次响应的 `next_batch`**，并且该位置在房间循环**之前**就算好
   （整账户路径与单房路径各自 `next_event_stream_id`）—— 记下的数必须与发给客户端的 token
   是同一个，否则修剪规则会拿位置去比一个语义不同的 token。
3. **只记录真正交付的成员**：上游对过滤后的 `state_ids` 与 `timeline_state` 记；本仓原先会把
   _被自己抑制掉的_成员也写回缓存，那会让"成员变更"永久被吞（旧 event id 盖住新 event id）。
4. **交接集合按成员去重**：同一成员可能同时出现在 timeline 与 state 块里，而持久化是**一条**
   `UNNEST … ON CONFLICT DO UPDATE` —— PostgreSQL 会拒绝对同一冲突行更新两次。这是本轮单测
   抓出来的真缺陷：不去重时持久化会直接报错。
5. 持久化行的位置落在基线新增的**可空**列 `last_sent_position` 上：不用 `DEFAULT 0`，因为
   真实位置**可以是 0**（空房/静默房的 `next_batch`），"未记录"必须能与它区分。写入跳过无位置的
   条目；加载只用"位置 ≤ `since`"的行（`NULL` 与"晚于 `since`"都不抑制）；初始同步（无 `since`）
   **整条读路径跳过** —— 对齐上游 `start_request(None)` 的"新客户端必须收到全部成员"。
6. 缓存仍是进程内 `LruCache`（P-4 的要求）：回源只在进程内未命中时发生一次，不会退化成
   每房一次 DB 往返。

数据面连带：基线改动 = 加列 + 1 条 `ADD COLUMN IF NOT EXISTS`（+1062 字节），
`EXPECTED_BASELINE_FINGERPRINT` 按 R10 第①条独立复算为 `0bea2dcc031b8c94`
（先用改动前的常量 `c1894a91182775f0` 对 235048 字节自检哈希实现，通过后才取新值）；
两条查询文本变更 ⇒ `.sqlx` 条目按 `sha256(query 原文)` **离线手写**（实测条目内的 `hash`
字段就是该值，`check_sqlx_cache_fresh.sh --static` 正是按此核对 ⇒ 无需
`cargo sqlx prepare` / describe，绕开 A1 那次缓存事故）；静态查询总数不变（1361），棘轮基线不动。

判据（全部离线可跑）：`LazyLoadedMemberState` 的 8 条状态机用例（位置修剪、初始同步全清、
事件 id 感知、`NULL` 位置不抑制、恢复行晚于 token 不抑制、恢复行被更旧的 token 修剪、
位置上界只前移），加上既有过滤用例（含 limited 分支的负控）全部通过。

**P1（功能缺口/安全面）实施记录（2026-10-16，分支 `fix/p1-functional-security-gaps`）**

承接 §18.8.1「P1（功能缺口/安全面）」5 项，逐项交付与判据如下（区分「已修 / 部分修复 / 仍缺」，
不夸大）：

| 项 | 状态 | 交付物与判据 |
|---|---|---|
| ① 房间别名可被静默改指（ELEMENTSEC-2026-1729） | ✅ 已修 | `synapse-storage/src/room/mod.rs::set_room_alias` 的 INSERT 改为 `ON CONFLICT (room_alias) DO NOTHING`，0 行受影响时返回 `sqlx::Error::RowNotFound`（不再静默覆写既有别名）；`synapse-services/src/room/state/aliases.rs::is_alias_taken` 据此（`RowNotFound \|\| is_unique_violation()`）映射 `ApiError::conflict_with(MatrixErrorCode::RoomInUse)` ⇒ **409 `M_ROOM_IN_USE`**。判据：单测 `set_room_alias_conflict_maps_to_409_room_in_use`（**负控**：空闲别名仍返回 200/成功，仅有主才 409） |
| ② AS（MSC2409）临时数据投递未接线（UG-4） | ✅ 已修（2026-10-17 补齐投递面，见下方本轮记录） | 配置键对齐（2026-10-16）：`synapse-services/src/application_service/transaction.rs::appservice_receives_ephemeral` 现认 4 个键（上游键 `de.sorunome.msc2409.push_ephemeral`、mautrix 键 `receive_ephemeral`、命名空间 `de.sorunome.msc2409`、`ephemeral`），使"按上游键配置的 AS"不再被误判为不接收。**投递接线（2026-10-17 补齐）**：回执流生产路径已接线（详见下方本轮记录），按上游规范 opt-in 的 AS 现可收到 `m.receipt` EDU |
| ③ push rule 数量 / 体积无上界（ELEMENTSEC-2026-1071） | ✅ 已修 | `synapse-services/src/client_push_service.rs`：`MAX_PUSH_RULE_ID_LEN = 300`、`MAX_PUSH_RULE_SIZE = 1024`（单条序列化体积）、`MAX_PUSH_RULES_PER_USER = 10_000`（计数上界）；`ensure_push_rule_within_limits` 越界 ⇒ **413 `M_TOO_LARGE`**（`MatrixErrorCode::TooLarge`）；计数上界在新增路径生效，更新已有规则时**放行**（避免存量越界账号被锁死）。判据：同文件 `mod tests` 的 `test_upsert_push_rule_rejects_rule_at_count_limit` / `..._at_size_limit` / `..._at_limit` 与对应 `accepts_..._at_*_limit` 正控 |
| ④ AS 设备管理免 UIA（MSC4190/MSC4326） | ✅ 已修（设备面 + pusher 面） | `synapse-web/src/routes/device.rs`：`DELETE /devices/{deviceId}` 与 `PUT /devices/{deviceId}` 在 `auth_user.appservice_id.is_some()`（`synapse-common/src/claims.rs` 提供）时**跳过 UIA**；`PUT` 走 upsert（201 新建 / 200 更新），未知设备 → `M_UNKNOWN_DEVICE`。判据：`tests/integration/api_appservice_p1_tests.rs::test_appservice_device_management_skips_uia_msc4190`。**pusher 面（2026-10-17 复核）**：上游 AS API 无专属 pusher 端点，AS 用自己的 token 打通用 `POST /_matrix/client/v3/pushers/set` / `GET /_matrix/client/v3/pushers` 即管理虚拟用户 pusher；回归 `test_appservice_pusher_management_via_login` 锁定 |
| ⑤ 缺 MSC4354 标准请求面（UG-10，方案 ⓐ） | ✅ 已修（`/send` + `/state` 两面） | `synapse-web/src/routes/handlers/room/mod.rs`：`msc4354_query_sticky_duration_ms` 解析 `?org.matrix.msc4354.sticky_duration_ms=<ms>`（命名空间优先、短名 `sticky_duration_ms` 回退），非整数 / `<=0` / 超 `MAX_STICKY_DURATION_MS` → 400 `M_INVALID_PARAM`；`apply_msc4354_sticky_ttl` 经 `update_event_unsigned` 写事件 `unsigned.msc4354_sticky_duration_ttl_ms` 投影，在 `synapse-web/src/routes/handlers/room/events.rs` 的 `/send` 路径接线。**`/state` 面（2026-10-17 补齐）**：`synapse-web/src/routes/handlers/room/state.rs` 的 4 个状态写路径（`send_state_event` / `put_state_event` / `put_state_event_empty_key` / `put_state_event_no_key`，+40 行）同样解析该参数并在持久化后 `apply_msc4354_sticky_ttl`。判据：`tests/integration/api_delayed_event_schedule_tests.rs` 的 3 个用例（含越界/回退/投影断言）。**延迟事件面（2026-10-17 补齐）**：`synapse-web/src/routes/delayed_events.rs` 的专用端点现接受 body/query 两种位置，值存 `delayed_events.sticky_duration_ms`，由 dispatcher 在发送时落 `unsigned`（§11.18）；**残余**：`/sync` 的 sticky 标记仍由私有 `room_sticky_events` 表驱动，查询参数只落 `unsigned` 投影 |

**数据面连带**：本轮无新增/修改 `sqlx::query!` / `query_as!` 的静态 SQL 文本命中缓存条目变更之外的
describe；`.sqlx/` 缓存按 `scripts/ci/sqlx_prepare.sh`（`ALLOW_CACHE_SHRINK=1`）复核，条目
**1329 → 1326**（清理 3 条失去引用的旧条目），全 workspace `cargo check`（`SQLX_OFFLINE=true`）通过。

**P1/P2 续作实施记录（2026-10-17，分支 `fix/p1-functional-security-gaps`）**

承接 §18.8.1 剩余项与 §19.5 批次 C/UG 项，本轮交付 R2–R7 六组（判据与残余如实登记）：

| 项 | 状态 | 交付物与判据 |
|---|---|---|
| R2 = B3 + P-5 + UG-4：AS（MSC2409）临时数据投递接线 | ✅ 已修（投递面） | `synapse-services/src/application_service/transaction.rs`（+289 行）把 `deliver_transaction_with_edus(…, Some(ephemeral))` 的**生产调用方**接上：从回执流取数填 `AppServiceEphemeralBatch.receipts`。新 seam：`RoomStoreApi::get_latest_receipt_position()` / `get_receipts_since(from_exclusive, to_inclusive, limit)`（`synapse-storage/src/room/api.rs`；模型 `ReceiptStreamEntry{id(BIGSERIAL 流位置),room_id,user_id,event_id,receipt_type,ts,data}` 见 `synapse-storage/src/room/models.rs`，含 `get_latest_receipt_position`/`get_receipts_since` 的 SQL 实现）；`synapse-services/src/wiring/admin.rs` 以 `.with_room_store(Arc<dyn RoomStoreApi> = RoomStorage::new(pool))` 注入。判据：`tests/unit` 事务/调度器单测（含 ephemeral 分支）+ 既有 AS 集成套件 |
| R3 = §18.8.1 L1428（pusher 面） | ✅ 结论：原判定属误报 | 复核上游 Matrix AS API **无专属 pusher 端点**；AS 经 `m.login.application_service` 登录拿到带 `appservice_id` 的普通 access token，直接调通用 C-S 端点 `POST /_matrix/client/v3/pushers/set` / `GET /_matrix/client/v3/pushers`（`synapse-web/src/routes/push.rs`）。判据：新增回归 `tests/integration/api_appservice_p1_tests.rs::test_appservice_pusher_management_via_login`（+151 行：注册 AS → 建虚拟用户 → AS 登录 → `POST /pushers/set` 200 → `GET /pushers` 匹配 pushkey → `kind:null` 删除 200 → `GET /pushers` 空） |
| R4 = §18.8.1 L1429（AS 联邦侧代理）= MSC4512 | ✅ 已修 | ① 代理侧：新增 `synapse-web/src/routes/msc4512_proxy.rs`——统一 fallback（`routes/assembly.rs` 由 `router.fallback(...)` 接管）按注册文件 `proxy_prefix` 动态分派 C-S（`/_matrix/client/(unstable/[^/]+\|v[^/]+)/{prefix}/…`）与 S-S（`/_matrix/federation/...`）请求，授权后转发到 `proxy_url`；双向剥离 hop-by-hop 头（复用 `app_service.rs::is_hop_by_hop_header`，改 `pub(crate)`）与入站 `Authorization`，改注入 `Authorization: Bearer <hs_token>` 及 `X-Matrix-User-Identifier`（C-S）/ `X-Matrix-Origin`（S-S）。② 发起侧：`POST /_matrix/client/v1/appservice/fed_proxy`（+ unstable 孪生 `/_matrix/client/unstable/io.element.msc4512/appservice/fed_proxy`，均注册于 `create_app_service_router`）实现规范全部校验与错误码（`M_FEDPROXY_DESTINATION_DENIED` / `M_FEDPROXY_PATH_NOT_ALLOWED` / `M_FEDPROXY_CONNECTION_FAILED` 等）；`synapse-common/src/error/code.rs` 补码。③ 策略对齐上游 Synapse：`synapse-services/src/application_service/proxy.rs`（新建）含 `ALLOWED_PROXY_PREFIXES = {"rtc/livekit"}` 白名单、`ProxyScheme`、`validate_proxy_config_value`（`proxy_prefix`/`proxy_url` 必须成对）、`resolve_proxy_target`；`experimental.msc4512_enabled` 默认 `false`。配置沿用 `AppServiceConfigFile` 的 `#[serde(flatten)] extra` ⇒ **零迁移**。判据：`msc4512_proxy` 4 单测 + `application_service::proxy` 21 单测全通过；`cargo check -p synapse-web --all-features` 通过。**契约链连带**：2 条新路由 ⇒ 重生成三张派生表 + 两条 fixture lane（golden 3 份 + sdk 3 份 + `manifest.json`）+ 两份 ledger 集成快照 + `docs/openapi/route-table.json`（957 → **959**）+ `ROUTE_CONTRACT.md`（**1,038 路由 / 66 模块**） |
| R5 = §18.8.1 L1433 / UG-10：MSC4354 标准请求面 `/state` 残余 | ✅ 已修 | `synapse-web/src/routes/handlers/room/state.rs`（+40 行）：`send_state_event` / `put_state_event` / `put_state_event_empty_key` / `put_state_event_no_key` 四路径均取 `msc4354_query_sticky_duration_ms(&query)?`（持久化前校验，越界 400）并在持久化后 `apply_msc4354_sticky_ttl(&ctx, &event_id, duration_ms).await?` 落 `unsigned` 投影。判据：既有 `/send` 3 用例 + `/state` 面复用同一助手函数 |
| R7 = UG-8 + §19.5 D1：`task_scheduler` 配置段 | ✅ 已修 | 新增 `synapse-common/src/config/task_scheduler.rs`（`max_concurrent_tasks` 等键），`synapse-services/src/wiring/admin.rs` 改为 `ApplicationServiceScheduler::with_max_services_per_tick(manager, config.task_scheduler.max_concurrent_tasks)` ⇒ 上游 `task_scheduler` 并发上限有了对应物（见 V-19-8 改判） |
| R6 = C1 + C2 + C3（§19.5 批次 C）= UG-7：sticky 联邦 catch-up + 新加入房间补发 sticky | ✅ 已实施（**C1 改最小闭环**） | 见下方「批次 C 实施记录（2026-10-17）」。C1 复用 `federation_queue` 做启动/定期重放（**零迁移、零 `.sqlx` 变更**）；C2 收敛为「`/sync` sticky 集合 == `GET .../sticky_events` 集合」并由构造保证；C3 以注释 + 唯一键单行回归用例登记 |
| R9 = ⑦ / 上游 #20203：profile `LEFT_ROOM`（MSC4262） | ✅ 已修（2026-10-17，零迁移） | ① 存储 seam：`MemberStoreApi::get_users_no_longer_sharing_rooms_since(user_id, since_ts)`（`synapse-storage/src/membership/api.rs`），`RoomMemberStorage` 以 `room_memberships.updated_ts` 为变更信号推导「不再共享房间」集合（`synapse-storage/src/membership/mod.rs`；运行时 SQL，避免 `.sqlx` 重生成）；② `ban_member` 补写 `updated_ts`（否则封禁不触发 LEFT_ROOM）；③ 服务层 `build_profile_updates_extension` 把该集合以 `users[uid] = null` 合入 `profile_updates`（已有更新优先，避免覆盖新资料）；④ `InMemoryMemberStore` 同步实现，保证服务层单测可用。判据：`membership::db_tests::test_left_room_*` 2 条（双向语义 + 仍共享房排除 + 仅 invite 不泄漏）；且 `profile_updates.users` 非空即触发 non-idle sync（`sliding_sync_service/mod.rs`） |
| R10 = M-04：`KeyRotationConfig::max_session_age_days` 不生效 | ✅ 已修（2026-10-17，配置为唯一真源） | 原缺陷：该配置被读/写/API 暴露，但 `key_rotation/service.rs` **无读者**，真实寿命来自 `MEGOLM_SESSION_MAX_AGE_DAYS` 环境变量（默认 7 天）⇒ 同一策略两个真源。修复：`synapse-e2ee/src/vodozemac_megolm.rs` 把全局 `OnceLock` 改为 `AtomicI64`（0 = 未配置）+ 独立 `OnceLock` 引导 fallback，新增 `set_session_max_age_days(days)`；`synapse-e2ee/src/key_rotation/service.rs` 新增 `apply_session_max_age`，在 `new` / `new_with_db_config` / `reload_config` / `update_config` 四处注入 ⇒ 持久化配置**覆盖**环境变量，env 仅在配置装载前作为进程引导值。判据：`vodozemac_megolm::tests::session_max_age_days_follows_rotation_config`（12 passed）+ `key_rotation::service::db_tests`（6 passed，含持久化配置回读） |
| R11 = UG-11 残余：注销退房改为**异步可重试** | ✅ 已修（2026-10-17，新增 1 张表） | 上游 `deactivate_account()` 只写 `user_pending_deactivations` 并由 `_user_parter_loop` 后台排空，本仓原先是同步逐房 ⇒ 超大账号会让注销请求变长、且无重启恢复。落地：① 迁移新增 `user_pending_deactivations(user_id PK, created_ts)`；② `synapse-storage/src/user_deactivation.rs` 提供入队/取队首/删行（运行时 SQL，避开 `.sqlx` 重生成）；③ `AccountDeactivationService` 新增 `PendingDeactivations` seam + `process_pending_parts()`（队列 FIFO，**列表失败保留行并停止本次 pass**，**单房失败记日志后仍删行**，与上游一致）；`deactivate()` 改为「同步拒绝 invite/knock → 入队 joined 退房 → pushers → purge → 不可逆 teardown → `Notify` 唤醒」；④ `src/server/mod.rs` 独立后台任务：启动先排空一次（重启恢复）＋ 每个 `background_tasks_interval` tick ＋ 入队 `Notify` ＋ shutdown 退出。判据：`account_deactivation_service::tests` 11 条（含 `process_pending_parts_keeps_the_row_when_listing_fails` / `process_pending_parts_drains_every_queued_user` / `deactivate_wakes_the_background_drain_loop`）+ `user_deactivation::db_tests`（FIFO、幂等、删除语义） |

**本轮数据面连带**：R4 零迁移（`extra` → `config` JSONB），无新增 `.sqlx` 条目；R2 新增的两条
receipt 流查询已入 `.sqlx` 缓存。契约链按 `scripts/contract/check_route_contract.sh` 全链重生成后通过。
R6 亦**零迁移、零 `.sqlx` 变更**（C1 复用已入缓存的 `get_all_pending`）。

**批次 C 实施记录（2026-10-17）**

1. **C1（UG-7a）sticky 联邦 catch-up —— 偏离 §19.5 C1 原方案，改用最小闭环**：
   - **原方案**：为每个 destination 记录"本机已同步到哪个 sticky 位置"（per-destination 位点表），
     远端重连/回执成功后**补发缺失 EDU**。
   - **本轮裁定（用户答复）**：**最小闭环 —— 复用 `federation_queue` 做启动（及定期）重放**，
     把已落库的 `status='pending'` 行在启动/每轮 tick 重新装载并重放。**零迁移、零 `.sqlx` 变更**，
     不新增 per-destination 位点表。
   - **实现**：`synapse-federation/src/event_broadcaster.rs` 新增
     `EventBroadcaster::replay_pending_transactions()` —— 用 `FederationQueueStorage::get_all_pending()`
     读回 `status='pending'` 行，反序列化回 `FederationTransaction` 塞回内存重试队列，交给既有
     `retry_pending_transactions()` 补发。**幂等**：内存队列里已存在的 `db_id` 跳过，重复调用不重装；
     **不可反序列化（结构漂移）或超 `MAX_PENDING_QUEUE_ENTRIES` 被淘汰的行标 `failed`**，
     不留永远无法推进的 `pending` 行（淘汰/标失败在释放写锁后做，因 `update_db_status` 是 async）。
     接线在 `src/server/mod.rs` 后台维护循环的**启动**处与**每轮 tick**（`retry_pending_transactions` 之前）。
     未配联邦客户端或未配 pool 时直接返回 0（避免把同一批行反复装载又反复失败）。
   - **判据**：`event_broadcaster.rs::mod db_tests` 的
     `replay_pending_transactions_requeues_persisted_rows`（回装 + `db_id`/`transaction_id`/`next_retry_at`
     正确 + **幂等负控**：二次调用返回 0 且队列仍 1 条）与
     `replay_pending_transactions_marks_unreplayable_rows_failed`（结构漂移行标 `failed`，不空转）。
   - **残余（如实登记）**：这是"重启/重连后重放**已落库事务**"的 catch-up，与上游"per-destination
     sticky 位点 + 增量补发"**语义不等价** —— 本仓 sticky 不注入 timeline、也不逐 destination 记位点，
     故**未实现逐 destination 的 sticky 增量补发**。若将来确需，须引入 per-destination 位点（§19.5 C1 原方案）。
2. **C2（UG-7b）新加入房间补发 sticky —— 保守读法（用户裁定）**：
   - **裁定**：只保证「**新成员 `/sync` 的 sticky 集合恒等于其 `GET .../sticky_events` 集合**」，
     **不改现有语义**、不在加入路径补发。
   - **事实依据**：`/sync` 的 sticky 注入（`synapse-services/src/sync_service/response.rs`，MSC4354 段）
     与 `GET .../sticky_events`（`synapse-web/src/routes/sticky_event.rs`）**共用同一** per-user 读取
     `StickyEventStorage::get_all_is_sticky_events(room_id, user_id)`（过期过滤 `is_sticky=true AND
     (expires_at IS NULL OR expires_at > now)` 在该读取内完成）⇒ 两路集合**恒等由构造保证**；
     新成员两路皆空，无需在加入路径补发。
   - **落地**：`response.rs` 的 MSC4354 注入注释追加 C2 段，显式声明"同源 ⇒ 恒等；新成员无需补发"。
   - **判据**：`tests/integration/api_sticky_event_tests.rs::sticky_view_is_identical_for_sync_and_get_and_empty_for_new_members`
     （pinned 用户读到既有集合；new_member 读到空集）。
3. **C3（#20186 的形态差异）**：按 §19.5 C3「现在不做，只写注释与回归用例」落地。
   - `synapse-storage/src/sticky_event.rs::set_is_sticky_event` 注释新增「单写者假设（C3）」：
     唯一键 `(room_id, user_id, event_type)`（`ON CONFLICT … DO UPDATE`）保证单写者模型下同键恒一行；
     多写者/分片是被本轮**未做**的边界，届时需按分片位置去重或改 per-room 广播抄本。
   - **判据**：`api_sticky_event_tests.rs::sticky_event_unique_key_keeps_one_row_per_room_user_type`
     （同 `(room, user, type)` 写两次 ⇒ 仅 1 行且为后写值）。

**本轮未做（如实登记）**：
- **R8 = UG-11 = §19.5.1 A1 残余**：**已收尾（2026-10-17，见 §19.5.1「A1 残余收尾」）**——threepid 解绑、账数据/推送规则清理、线程订阅与备份键删除、待处理邀请/敲击拒绝**均已落地**；用户目录摘除以 `is_deactivated` 过滤满足（dead schema，不写无效 `DELETE`）。**仅剩残余**：退房仍为**同步逐房**（上游为异步可重试的 `add_user_pending_deactivation` + 后台 parting 循环），已登记于 UG-11 行。
- **R1（P0 PDU 保留发送方签名/哈希）**：~~本轮范围明确不含（用户答复未选）~~ → **后续已落地（2026-10-17 复核确认）**：`/send`、`backfill`、入站邀请三条路径均回写上游 `signatures` / `hashes`，见 §18.8.1「重发远程 PDU」行与 `synapse-storage/src/event/db_tests.rs:691-751`。

### 19.6 本轮作废 / 更正的历史声明

**（a）对 §18 自身口径的更正**（§18 是 2026-10-01 的判定，下列各项在其后已落地）：

| §18 原声明 | 实际状态（本 HEAD 实测） | 证据 |
|---|---|---|
| §18.4 ⑧「`/relations` 无 `recurse`（MSC3981）」 | **已实现**：`recurse` 稳定名 + `org.matrix.msc3981.recurse` 拼写均被读取，缺省为 `None` 而非 `false` | `synapse-web/src/routes/relations.rs` |
| §18.4 ②「MSC4311 宽限开关是死配置 + knock stripped state 缺失」 | 已按 §18.6 V-2 落地（跨实现联调 V-2b / V-2c 已绿） | §18.6 V-2 行 |
| §18.4 ①「委派认证/MAS 运行时未接线」 | **已作废**：运行时已接线 —— `build_mas_validator` 是唯一入口（含缺 audience 锚点时的第二道防线），返回值在容器装配阶段经 `with_mas_validator` 注入 `AuthService` | `synapse-services/src/container.rs:258-281`；`synapse-services/src/auth/mas_validator.rs`（`build_mas_validator_requires_full_configuration`） |
| §18 证据栅格中 v1.15 行「GAP: hierarchy 的 `allowed_room_ids` … 未返回」 | **已作废**：`allowed_room_ids` 已实现并返回 | `synapse-services/src/room/join_rules.rs::extract_allowed_room_ids` |
| §18.4 ⑩ / §18.5(b)「MSC4140 专用调度端点未实现」 | **已作废**：`PUT .../org.matrix.msc4140/rooms/{room_id}/delayed_event/{event_type}/{txn_id}` 等 **6 条路由 / 7 个方法**已注册（含上游同形的 `cancel` / `restart` / `send` 子路径与 `GET .../delayed_events` 列表） | `synapse-web/src/routes/delayed_events.rs:352-357` |

**（b）对本轮初稿的自我更正**：本轮起草时曾把「注销时延迟事件未取消」写成"MSC4140 未实现"，
取证时发现 MSC4140 本身**已完整实现**（7 条路由 + 服务 + 存储 + 联邦 EDU），真实缺口只是
**注销路径没有取消钩子**。这条更正本身即是 UG-1 的证据形态：**"功能存在"与"生命周期接上了"是两件事**。

实施 A3 / A4 时又取证出**两条口径错误**，一并记录（都是"看起来已对齐、实际不是"）：

| 初稿声明 | 实况（上游 v1.163.0 取证） | 处置 |
|---|---|---|
| §19.3 #20256 行「'查询参数'这一半已天然对齐」 | **错**。上游的 MSC4354 **客户端请求面就是** `/send` 上的 `org.matrix.msc4354.sticky_duration_ms` 查询参数：`synapse/rest/client/room.py` 的非状态处理器仍解析它、并把 sticky 字段写进事件（`event_dict[StickyEvent.EVENT_FIELD_NAME]`），客户端再从事件的 `unsigned` 读 sticky TTL。被 #20256 删掉的只是 **`/state`** 那一份。上游**没有** `.../sticky_events` 路由；本仓那条是**非上游扩展** | 新增 **UG-10**（见 §19.4.1）；A3 只覆盖"不得 sticky 状态事件"这一半，**不声称**请求面已对齐 |
| §19.3 #20227 行「也未宣告 `org.matrix.msc4075`」 | **上游也不宣告它**：`rust/src/handlers/versions.rs` 的 `UnstableFeatureMap` 里没有 `msc4075` 字段，上游只把它当作推送规则 + 配置开关 | A4 **不做**能力宣告（做了反而是过度承诺）；该子句从差距描述里删掉 |

> 两条更正的共同形态：**"名字像对上了"不等于"形状对上了"**。前者是路径前缀相似（`/rooms/{id}/sticky_events`
> 看起来就像 MSC4354 的客户端面），后者是 MSC 号在 `/versions` 里的缺席（其实上游也不宣告）。
> 判据只能是上游源码本身，不能是名称相似度。

### 19.7 待核实项（附可复现命令与预期）

| # | 验证点 | 命令 / 方法 | 预期 |
|---|---|---|---|
| V-19-1 | #20274：`auth` 为 `null` 时 UIA 端点是否 500 | 对 `DELETE /_matrix/client/v3/devices/{deviceId}` 发 `{"auth": null}` 与 `{"auth": "x"}`，看状态码 | 期望 400/401 + `M_UIA_REQUIRED`；若出现 500 则该项升级为差距 |
| V-19-2 | UG-3：`org.matrix.msc4354` 是否真的未宣告 | `curl -s /_matrix/client/versions` 看 `unstable_features` | 当前预期**不含**该键（若已含则本轮结论有误） |
| V-19-3 | #20100：缩略图是否阻塞事件循环 | 对 `GET /_matrix/client/v1/media/thumbnail/...` 并发压测，观察 p99 与线程池 | 期望无 worker 饥饿 |
| V-19-4 | UG-4：AS 临时数据是否真的零投递 | 配一个 `ephemeral: true` 的 AS，发一条回执，抓 AS 侧 HTTP | **已修（2026-10-17）**：当前预期**能收到** `m.receipt` EDU（投递面已接线，见 §19.5.1 本轮记录）；若仍收不到则本修有误 |
| V-19-5 | UG-1：注销后延迟事件是否仍发出 | 建用户 → 排期一条 30s 延迟消息 → 立即注销 → 等 60s | 当前预期**会发出**（缺陷复现）；修后必须不发出 |
| V-19-6 | UG-1：注销用户是否仍在房间成员里 | 注销后查 `room_memberships` / 当前状态里的 `m.room.member` | 当前预期仍为 `join` |
| V-19-7 | S-1：安全公告库新鲜度 | `cargo deny check advisories` 的输出是否表明拉到了最新 DB；必要时先 `cargo deny fetch` | 避免"用陈旧库得出 advisories ok" |
| V-19-8 | #20230：本仓是否有与上游 `task_scheduler` 对应的调度器 | `Grep "task_scheduler"` 全仓；再看 `synapse-services/src/application_service/scheduler.rs` 的容量/退避选项是否就是那个"并发上限"的对应物 | 若无对应物 ⇒ 该条改判「结构性不适用」而不是"缺一个配置键"（**本轮已按此改判**，见 §19.6(b)） |
| V-19-9 | UG-10：上游标准请求面在本仓被完全忽略 | 对 `PUT /_matrix/client/v3/rooms/{roomId}/send/m.room.message/{txn}?org.matrix.msc4354.sticky_duration_ms=60000`（及 `/state` 面同参数）发一次，读回事件看 `unsigned` 是否有 sticky TTL | **已修**：当前预期**有** `unsigned.msc4354_sticky_duration_ttl_ms`（`/send` 与 `/state` 两面均已接线，见 §18.8.1 与 §19.5.1 本轮记录）；若仍无则本修有误 |
| V-19-10 | A4：开关真被读取（不是"加了但没接线"） | 把 `experimental.msc4075_enabled` 置 `true`，`GET /_matrix/client/v3/pushrules` 应出现两条 `msc4075` 规则；置 `false` 应消失 | 两态都要测：只测一态无法区分"接线了"与"恒为某值" |

### 19.8 验证与门禁

```bash
# 0) 环境前置：清隔离 schema（残留会把 4.6s 用例拖过 120s 终止线，见 §18.7.3）
DATABASE_URL=... bash scripts/cleanup_test_schemas.sh --apply
# 1) 编译与静态检查（两档不可互相替代；照抄 pre-push 那一行）
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
# 2) 本轮涉及模块的用例
cargo nextest run -p synapse-common --lib -E 'test(/push_rules|default_push/)'
cargo nextest run -p synapse-services --lib --features test-utils -E 'test(/lazy_load|capability|deactivat/)'
cargo nextest run -p synapse-web --lib --features test-utils -E 'test(/sticky|preview|profile/)'
# 3) 文档守卫 + 契约链（本文件任何计数/路径改动都必须过这两条）
cargo nextest run --test unit --features test-utils -E 'test(doc_credibility_guard_tests)'
bash scripts/contract/check_route_contract.sh
# 4) 依赖安全
cargo deny check advisories
# 5) 格式（真实门禁是棘轮脚本，不是裸 rustfmt）
./scripts/check_fmt_ratchet.sh
```

**⚠️ 本轮不做的三件事**（有意，写入以防后续被当成遗漏）：
① 不跟随 #20186 的分片令牌改造（本仓无多写者流模型，改造面大于收益，见批次 C3）；
② 不为对齐 #20285 而引入 `user_ips` 表（那是新增功能，不是对齐）；
③ 不因 #20231 新增 `limit_profile_requests_to_users_who_share_rooms` 配置项
（本仓已有语义更细的 `user_privacy_settings`，加第二个开关会制造"同一职责两份实现"）。

### 19.9 Worker 拓扑验证现状重判（2026-10-16，独立审计）

本文 §2.3 / §5.1 / §12.4 三处把本仓 Worker 拓扑写作「仍在建设中」。本轮独立审计（明细见
[docs/audit/WORKER_TOPOLOGY_VERIFICATION_AUDIT_2026-10-16.md](docs/audit/WORKER_TOPOLOGY_VERIFICATION_AUDIT_2026-10-16.md)）重判如下 —— **方向对、粒度错**：`topology_validator.rs` 不是半成品，
它是**已接线到启动路径的静态配置校验器**；真正缺的是**运行时对账、强制力与参考部署**。

| 判定对象 | 原措辞 | 重判 |
|----------|--------|------|
| 拓扑**静态校验** | "仍在建设中" | ✅ **已成型**：`synapse-services/src/worker/topology_validator.rs` 的 10 条规则（主进程必需 / `instance_map_key` 唯一 / 前缀冲突 / 未知实例 / replication 前置 / secret 存在性 / writer 归属已知性与类型白名单），`src/server/mod.rs:208` 启动时执行。上游 `synapse/config/workers.py` 只有**配置解析期**校验，**没有**注册表、健康检查、拓扑端点 ⇒ 本仓在建模上超前 |
| 校验的**强制力** | （未提） | ❌ 只 `warn!` + 继续启动（`src/server/mod.rs:210-215`）；上游同类错误是 `ConfigError`（进程不启动） |
| **运行时对账**（声明 vs 注册） | （未提） | ❌ 缺失：`validate_worker_config` 只吃静态配置，从不与 `WorkerManager::get_active` 比对 ⇒ "声明了却没起 / 注册了却不在配置 / 心跳已停"三类事故都不可见 |
| `GET /_synapse/worker/v1/topology` | （未提） | ❌ 返回**硬编码**的 `WorkerTopologySummary::baseline()`（`State` 未被使用）⇒ 运维看到的是"支持哪些类型"，不是"集群现在什么样" |
| **负载均衡** | "内置 load_balancer 模块" | ⚠️ 模块在，但 `enable_load_balancer` 全仓**只有测试调用点** ⇒ 生产恒走 `select_worker_fallback`，该声名只对测试为真 |
| 参考多进程部署 / 端到端烟测 | "水平扩展方案成熟度待验证" | ❌ 成立：`docker/config/homeserver.yaml` 的 `worker.enabled: false`、`docker/docker-compose.yml` 只有单应用服务、`docker/nginx/` 无任何 conf ⇒ 多进程拓扑**从未端到端跑过** |

**修正后的措辞**：本仓应写「**拓扑静态校验已成型；运行时对账、强制力与参考部署未做**」。
完善方案见审计文档 §4 的批次 **W-A…W-E**（**W-A 为纯读、无 schema/启动/响应字段变更，建议先做**；
W-B 为严格模式 + 离线 `--validate-topology` 入口）。本节改动只涉及文档，仍须过 §19.8 的
文档守卫与契约链两条门禁。

### 19.9.1 W-A…W-E 实施结果与"请求面仍单点"的复核（2026-10-16 收口）

§19.9 列出的缺口**已全部整改**，五批各自成 commit（分支 `feat/w-topology-completion`）：

| 原缺口 | 现状 |
|--------|------|
| 拓扑非法不阻断启动 | `worker.topology.strict: true` ⇒ 校验有 error 即 `Err` 拒绝启动（默认 false，存量行为零变化） |
| 没有运行时对账 | `synapse-services/src/worker/runtime_topology.rs` 的 `evaluate_runtime_topology` 产出 `missing` / `stale` / `unexpected` / `unowned_streams` / `writer_violations`；端点 `…/topology/validate` 新增 `runtime` 字段 |
| `/topology` 返回硬编码 | 新增 `live` 字段（真实注册表快照），既有 `workers` / `deployment_presets` 经 `#[serde(flatten)]` 原样保留 |
| 负载均衡只有测试接线 | `wiring/admin.rs` 的 `apply_load_balancing` 接进生产装配（**默认关**），策略映射有编译期保证 |
| 零参考多进程部署 | `docker/docker-compose.workers.yml` + `docker/nginx/` 参考 + `scripts/test/worker_topology_smoke.sh` + 部署一致性守卫 |
| 无拓扑指标 / 告警 | `synapse_worker_topology_valid` / `_registered_total` / `_missing_instances` / `_stale_instances` / `_unowned_streams` / `_instance_last_heartbeat_age_seconds{instance}` + 3 条告警 + 2 块面板 |
| 部署前无法判 | `--validate-topology`，退出码 0/1/2（根 bin 与 `synapse_worker` bin 都接） |

**复核发现的新事实（原先三处文档都没写，属本轮实测）**：

1. **HTTP 请求分流在服务端不存在**：worker 进程（`synapse_worker`）只 mount `/metrics`；
   全仓没有任何生产代码按 `WorkerType::owned_route_prefixes` 裁剪自己的路由，
   配置项 `worker.worker_app` 已于 2026-10-17 **删除**（它从未被读取，见 §19.12）。⇒ `docker/nginx/` 里"把 `/_matrix/client/*/sync`
   打到 sync_worker"今天只会 502；本仓 worker 拓扑真正覆盖的是**控制面与后台任务**。
   已登记为独立批次 **W-F**（按角色收敛 HTTP 面），它是"请求级水平扩展"的唯一前置。
2. **镜像原先不含 worker 二进制**：`docker/Dockerfile` 只 build/install `synapse-rust` 与
   `healthcheck` ⇒ 容器化的多进程栈**根本起不来**（`/app/synapse_worker` not found）。已补。
3. **`stream_writers` 目前是纯声明面**：全仓没有按它决定"谁来写这条流"的代码
   （与 `instance_map` 不同——后者至少被拓扑校验与运行时对账消费）。⇒ 声明它不会改变写入行为。
   > **2026-10-22 更新（W-I）**：默认仍是纯声明面，但**不再是"只能声明"**。
   > `worker.write_forwarding: true` 时，非 writer 实例会把该路的写**原样转发**给配置的 writer
   > （`synapse-web/src/middleware/write_forwarding.rs`）。本版本开六路
   > （typing / to_device / account_data / receipts / presence / push_rules）；
   > `events` / `device_lists` **配成非默认值直接启动报错**，不再静默无效。
   > 另需纠正一条此处隐含的理由：本仓 `events.stream_ordering` 是 DB 序列
   > （`nextval('events_stream_ordering_seq')`），多写者**不会**打乱流顺序 ——
   > 单写者要防的是提交后 fan-out（联邦广播 / appservice / 房间摘要）被多实例各做一遍。
   > 设计见 `docs/audit/WORKER_STREAM_WRITE_FORWARDING_DESIGN_2026-10-22.md`。

⇒ 因此本仓**不应**再写作「Worker 拓扑仍在建设中」；准确表述是
「**控制面已成型并可观测；请求面的准入已可执行（W-F，2026-10-17），但参考栈的进程选型尚未切到根二进制实例**」。

---

### 19.10 ELEMENTSEC-2026-1740 / -1760 裁决（2026-10-17）

> 这两条是 §14.4 安全表里最后两个「待深查」。本轮按**公告原文**核对本仓承载面，并把结论钉成
> 回归。两个公告同批发布、同在 Synapse **1.157.2** 修复。

#### 19.10.1 CVE-2026-102592 —— `multipart/form-data` 大小写绕过 → 内存耗尽

**上游成因**：对恶意 multipart 请求的既有缓解按 `Content-Type` **字面量**判断，改变大小写
（`Multipart/Form-Data`）即可绕过该门，请求进入解析器后整包缓冲进内存。

**本仓判定：不受影响。** `routes/voice.rs` 的 multipart 解析走 `axum::extract::Multipart`
（`multer`），`Content-Type` 只参与 `mime::Mime` 解析（大小写不敏感），**没有**「先比较字符串、
再决定是否解析」的门 ⇒ 不存在可绕过的检查；内存由请求体上限界定。

**但复核顺带揪出一处真实缺陷（已修）**：`/voice/upload` 原先**没有**自己的 `DefaultBodyLimit`，
而 `Multipart` 读的正是这个扩展 ⇒ 上限落到 axum 的默认 **2 MiB**；与此同时 `/voice/config` 的
`max_size_bytes` 与 handler 的 `content.len() > max_size` 都按 `server.max_upload_size`
（默认 **50 000 000** 字节）算 ⇒ **2 MiB 以上的语音在传输层被拒，且状态码是 400 `M_BAD_JSON`**（`Failed to parse
multipart data`）—— 客户端会误判成格式错误，比 413 更难诊断，按宣告上传必失败。与 §19.4 里反复出现的形态同类：配置/宣告一个值，实际生效另一个值。

**修法**：`voice_multipart_body_limit(max_upload_size) = max_upload_size + 64 KiB`
（multipart 封装开销：boundary、每个字段头、收尾），在 `create_voice_router` 上显式
`.layer(DefaultBodyLimit::max(...))`。之所以要加开销：恰好等于上限的音频加上封装必然超限。

**回归**（`tests/integration/voice_routes_tests.rs`）：

| 用例 | 断言 |
|---|---|
| `test_voice_upload_accepts_body_above_the_axum_default_limit` | `max_upload_size = 4 MiB` 时 2.5 MiB 音频 → **200**（修复前 **400 `M_BAD_JSON`**，变异自证见 §19.10.4） |
| `test_voice_upload_rejects_body_above_the_configured_limit` | 带 `Content-Length` 的超限体 → **413 `M_TOO_LARGE`** |
| `test_voice_upload_body_limit_is_content_type_case_insensitive` | 同一个超限体、`Content-Type: Multipart/Form-Data` → 仍 **413**（CVE 的绕过机制在本仓不存在） |
| `test_voice_upload_streamed_body_without_content_length_is_still_rejected` | 无 `Content-Length` 的超限体 → 仍被拒（现状 400，见 §19.10.3） |

#### 19.10.2 CVE-2026-102597 —— 容忍尾随后缀 → 代理规则绕过

**上游成因**：部分端点接受路径尾部的额外数据（`/path/extra` 命中 `/path`）。当服务位于会
**先规范化路径**的反向代理之后（nginx 把 `/a/../b` 变成 `/b`）时，这类宽松匹配会让请求落到
代理规则本意未覆盖的端点上。公告自评 Low，且「不应可直接利用」（敏感端点仍需 token）。

**本仓判定：应用层不适用。** axum 0.8 精确段匹配（`{param}` 只吃一段，通配必须显式
`{*wildcard}`），未匹配即落兜底 404 `M_UNRECOGNIZED`；路由面另有 `api_route_ledger_tests`
的全表快照。代理层：`docker/deploy/nginx/**` 的 `proxy_pass` 未带 URI 重写 ⇒ 规范化的结果就是
转发给后端的 URI，后端仍按精确路由判定，规范化不产生额外暴露；admin 面另有 token 门。
公告给的两条 workaround 中，「不要用规范化路径做路由」对本仓不成立（我们不依赖代理规则保护
端点），「敏感端点独立监听」属部署选项，不构成代码缺陷。

**回归**：`api_auth_routes_tests::test_extraneous_path_suffix_and_trailing_slash_are_not_routed`
—— 以下五条必须 404 + `M_UNRECOGNIZED`：`/versions/extra`、`/versions/`、
`/register/and/more`、`/versions/../versions`（归一化形态）、
`/_synapse/admin/v1/server_version/extra`。

#### 19.10.3 残余（登记，不修）

流式（**无 `Content-Length`**）的超限请求体：上限层在读取过程中中止，`Multipart` 把它报成
解析失败，handler 映射为 **400 `M_BAD_JSON`** 而不是 413 `M_TOO_LARGE`。**不是安全问题**
（读取在到达上限时即停，内存有界），属状态码分类问题；真实客户端对定长 body 一定带
`Content-Length`，走 413 分支（修复前，**带** `Content-Length` 的请求也是这个 400 症状——那是 axum
默认 2 MiB 经同一条路径报出来的，见 §19.10.1）。要改需在 handler 识别「长度超限」这一解析错误并改写状态码，
依赖上游错误文案，收益不抵脆弱性 ⇒ 已用用例钉住现状，等 axum/multer 暴露结构化错误再说。

#### 19.10.4 验证（全部实跑）

```bash
cargo nextest run --test integration \
  --features 'test-utils voice-extended voip-tracking' -j 1 \
  -E 'test(/voice_upload_|extraneous_path_suffix/)'
```

- 修复后：**5/5 通过**（4 条 voice + 1 条路径后缀）。
- **变异自证**（把层去掉 ⇒ 回到修复前状态）：
  - 删掉 `.layer(DefaultBodyLimit::max(...))` ⇒ 正向用例 `0 passed / 1 failed`，实得
    **400 `M_BAD_JSON` "Failed to parse multipart data"**（`max_upload_size` 设为 4 MiB、
    音频 2.5 MiB，全局层不拦，只有 axum 的 2 MiB 默认值会拦）—— 这就是修复前的真实症状。
  - 换成 `DefaultBodyLimit::disable()` ⇒ 正向用例**通过**：证明请求体本身没有问题，
    缺陷纯粹是"上限被 2 MiB 默认值遮蔽"，不是解析/存储能力不足。
  - 从备份还原 ⇒ `1 passed`（未用 `git checkout`，避免连带回滚同文件里的未提交改动）。

---

### 19.11 MSC4190 注册面 + Room v12 A4 首写播种（2026-10-17）

两件都是上一轮"遗留项复核"（§18.8.1 / `ROOM_V12_UPSTREAM_ALIGNMENT_OPTIMIZATION_2026-09-28.md` §11）
给出的建议，本轮按建议实施。

#### 19.11.1 MSC4190：应用服务注册虚拟用户

**此前状态**：`POST /register` 不认 as_token，`inhibit_login` 全仓 0 命中 ⇒ 该错误码没有存在的地方，
AS 只能靠 `/login` + `m.login.application_service` 为**已存在**的用户换 token，无法创建虚拟用户。

**实现**（四处）：

| 面 | 改动 |
| --- | --- |
| 错误码 | `MatrixErrorCode::AppserviceLoginUnsupported` → `M_APPSERVICE_LOGIN_UNSUPPORTED`（400），含 `as_str` / `http_status` / `from_str` 三处映射 + `ApiError::appservice_login_unsupported(...)` |
| 命名空间门 | `ApplicationServiceManager::is_interested_in_user` —— 与 `can_login_as` 的区别是**不要求 `exclusive`**（上游 `is_interested_in_user` 口径：注册只要落在服务的任一 `users` 命名空间；`exclusive` 只在登录/冒充时要求） |
| 服务层 | `CredentialAuth` trait 新增 `register_without_login`（默认实现返回 `M_UNRECOGNIZED`，供测试替身继承），`AuthService` 覆盖为"`create_user` + displayname，**不建设备、不签 token**" |
| 路由 | `register` 增加 `HeaderMap`：token 属于某 AS ⇒ 走 AS 分支（`inhibit_login` 必须为 true；`username` 必填；命名空间校验；响应只有 `user_id` + `well_known`） |

**判据**（`tests/integration/api_appservice_p1_tests.rs`，3 条）：

- `test_appservice_register_user_without_login` —— 200、`user_id` 正确、**响应里没有 `access_token`/`device_id`**，
  且新用户可由 `GET /profile/{user_id}` 查到、`displayname` 已落库；
- `test_appservice_register_requires_inhibit_login` —— 缺字段与显式 `false` 两态都必须是
  **400 `M_APPSERVICE_LOGIN_UNSUPPORTED`**（不是泛化的 `M_BAD_JSON`）；
- `test_appservice_register_rejects_user_outside_namespace` —— 命名空间外 403。

**已知取舍**：token 存在但**不是** AS token 时按普通注册继续（避免把带用户 token 的客户端误判成 AS）；
代价是"AS 用了错误的 as_token"会得到 `Password required` 而不是 401。此口径写在函数注释里。

#### 19.11.2 Room v12 A4：首写时播种

**缺陷**：`copy_forward` 在"无记录"分支直接 `Ok(())` ⇒ **从未分叉的房间永远没有 state group**，
读路径永远走 `DISTINCT ON … origin_server_ts DESC`。而 `origin_server_ts` 是发送方时钟，
联邦事件可带着偏旧时间戳后到 ⇒ 该推导会给出**与 DAG 顺序相反**的当前状态。

**改法**：该分支改为 `seed_from_current_state` —— 用 `EventReader::get_state_events` 取一次当前状态
（单查询、与房间历史长度无关；不用 `StateWalker::state_at`，那要沿 `prev_events` 递归整条历史）
落成 group 0，之后一律 copy-forward。

**判据**（`state_record::db_tests::first_state_write_seeds_a_record_for_an_unforked_room`）：
线性 DAG 上让**第二条** topic 的 `origin_server_ts` **更旧**（DAG 上是后继），断言读路径返回第二条 ——
时间戳推导会选第一条，只有"记录接管"才能拿到第二条；同时断言首个状态写恰好播 1 条记录、
事件已绑定、记录含 create/member/topic。

**残余**：并发的首个状态写会各播一份（`state_hash` upsert + 取 `id` 最大者胜出），
与既有 copy-forward 同类竞态；彻底消除需按房间串行化状态写。

#### 19.11.3 MSC4242 存储面已整体清空（2026-10-17，L-1）

MSC4242（state DAG）至今没有房间版本指派，上游实现 PR 已关闭未合并。按"不留半个功能"分两步清完：

| 步骤 | 内容 |
| --- | --- |
| 第一步（本文件早前） | 删 `find_events_referencing_missing_state`（无生产调用者）及其唯一用例 |
| 第二步（2026-10-17） | 删 `create_state_event_with_dag`（`event/create.rs`，零调用方）与 `get_prev_state_events` / `prev_state_events_from_json`（`event/dag.rs`，零调用方），连同三个 `db_tests`（持久化/回滚/malformed-json）与五处已失效的引用注释 |
| 第三步（2026-10-17） | 从基线移除 `events.prev_state_events` 列与两个 partial index（`idx_events_prev_state_events` GIN、`idx_events_state_dag_room`）—— 既然没有写入方也没有读取方，留着只会让 `events` 的每次插入多维护两个索引，并让 schema 看起来像"半个功能"（先例：`search_index` 表的移除） |

基线因此从 `237691` 降到 `236554` 字节，模板指纹 `65b6701ed356b3d1 → 16a162d3c01f87ce`
（`test_isolation_unification_tests` 的常量已同步，5 处 `include_str!` 副本全部指向同一文件）。
若 MSC4242 定稿，重新折入那条迁移即可 —— git 历史保留原实现与全部测试。
