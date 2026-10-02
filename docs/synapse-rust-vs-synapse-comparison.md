# Synapse-Rust 与 Synapse (Python) 地址：https://github.com/element-hq/synapse 对比分析报告

> **状态（2026-10-01）**：**当前口径是 §18（v1.10 对照 Synapse v1.162.0）**。§1–§17 为历史轮次记录，
> 其中 §11.2/§11.3/§12.4/§12.5/§15.3 有若干已被后续提交修掉却仍写作"缺失"的**假缺口**，清单见 §18.5。
> 另：本文多处记"v12/v13 为 `stable_parse_only`、不可创建"，已过时：v12 是**唯一可创建**版本（G-1），
> 版本 13 已从能力表移除（Q5(b)）。v12 语义落地情况见 `docs/audit/ROOM_V12_PLAN_STATUS_2026-09-27.md`。

> **文档版本**: v1.10
> **更新日期**: 2026-10-01
> **更新说明**:
> - **v1.10 上游基线升级到 Synapse v1.162.0（2026-09-29 发布）+ 本仓差距复核（2026-10-01）**：
>   新增 **§18**（当前口径），按"① 上游变更要点 ② 逐项对照 ③ 分优先级建议 ④ 待验证清单"组织。
>   结论摘要：v1.162.0 无 Deprecations/Removals。本仓**已落地该版本的大部分对齐项**（原分支
>   `feature/2026-10-01-metrics-docs-updates`，tip `e1ffcb2ab`，2026-10-01 已合并进 `main`）：默认房间版本 12、OTK 单设备单算法 500 上限、
>   `rc_profile`、profile 空字段 `{}`、hierarchy `allowed_room_ids`、稳定 `M_UNKNOWN_DEVICE`、
>   非合规历史 user id 过滤、MSC4354 sticky EDU、缩略图异步（逐项证据与提交见 §18.3）。
>   **仍然存在的差距**（§18.4）按优先级为：① **委派认证/MAS 运行时未接线**（`with_mas_validator` 0 调用者）
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
>   **详细证据与命令见** `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md`。
> - v1.2（2026-09-22）：对齐基准从 Synapse v1.149.1 更新为 v1.161.0（2026-09-15 发布）；
>   修正 vodozemac 版本与 crate 计数；补充 MSC4140/3912/4242 状态与 v1.161 Bug 修复核查项。
> **生成日期**: 2026-09-16
> **对比对象**: synapse-rust (Rust 重写实现) vs element-hq/synapse (Python 原始实现)
> **对齐基准**: Synapse v1.162.0（2026-09-29 发布，`release-v1.162` CHANGES.md；v1.162.0 = rc1 + 无重大变更）

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
18. [v1.10 对照 Synapse v1.162.0（2026-10-01，当前口径）](#18-v110-对照-synapse-v11620-2026-10-01当前口径)

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
| **v1.10 上游升级 + 差距复核** | **2026-10-01** | **`opt/consolidated`；上游 Synapse `v1.162.0`** | **§18（当前口径）** |

> **§18 是唯一"当前状态"口径**。§1–§12 中与 §18 冲突的表述一律作废（§12.2 仅作历史对照），
> 假缺口清单见 §18.5。

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
| **synapse-rust** | - Cargo Workspace 编译时依赖检查，杜绝循环依赖<br- 单进程即可利用多核，部署简单<br>- Tower 中间件链类型安全，编译时验证 | - Worker 拓扑验证仍在建设中（`topology_validator.rs`）<br>- 模块化扩展点不如 Python 灵活<br>- 编译时间长（464K 行 Rust 代码全量编译） |

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

synapse-rust 的 HTTP 契约以机器抽取的 **`docs/synapse-rust/ROUTE_CONTRACT.md`**（2026-10-01 生成）为准：**1,154 条注册路由条目**（绝对 `(method, path)`，已解析 `.nest()` 前缀并去重），涉及 **65** 个含路由注册的模块文件；人工维护的 `docs/synapse-rust/API_COVERAGE_REPORT.md` 按三种口径记为 **注册条目 1,154 / 唯一路径 915 / 逻辑端点 807**——与前者**同源但口径不同**（后者折叠版本前缀并把同路径多方法合并），两者不可相加。路由文件分布在 **`synapse-web/src/routes/`** 下，共 144 个 `.rs` 文件。⚠️ 计数较 2026-09-22 的 1,165 / 66 **净减 11 条**：2026-09-25 E2EE 去服务端私钥重构删除 30 条（`verification_routes` 24 条 + `e2ee` 路由组 6 条），其后 profile `{user_id}/{key_name}` 新增 5 条（3 条来自 admin_media 新增路由，2 条来自 invite 路由的 POST 方法），`auth_issuer` 删除 1 条，2026-09-27 U-5 Admin 媒体端点族补全新增 5 条（房间级媒体列举/删除/隔离/解除隔离 + 媒体保护），2026-10-01 两条对齐/RFC 批次共新增 3 条 —— MSC4140 单事件端点（L1，`GET /_matrix/client/unstable/org.matrix.msc4140/delayed_events/{delay_id}`，与同路径既有 `POST` 合并计入口径）与 MSC3720 账户状态（客户端 `/_matrix/client/unstable/org.matrix.msc3720/account_status` + 联邦 `/_matrix/federation/unstable/org.matrix.msc3720/account_status`），**不是抽取器漂移**；2026-10-02 再补 spec 的 4 段关系路由（L-6）**+2**（1,152 → 1,154）——`GET` `…/relations/{eventId}/{relType}/{eventType}` 在 `v1`/`v3` 各一条，与同路径既有 `PUT` 合并进同一 `MethodRouter`，**唯一路径数不变**。

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
| **Worker 发现** | 静态配置 | 健康检查 + 拓扑验证（`worker/health.rs`, `worker/topology_validator.rs`） |
| **负载均衡** | 手动配置 Worker 类型分流 | 内置 load_balancer 模块 |
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
| **synapse-rust** | - 单进程即可利用多核，部署简单<br>- Feature Flag 控制编译，最小化二进制<br>- 内置负载均衡和健康检查<br>- TCP Worker 拓扑可动态扩展 | - Worker 拓扑仍在完善中<br>- 缺乏大规模生产验证<br>- 水平扩展方案成熟度待验证 |

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
| **API 参考** | 在线文档 | `docs/synapse-rust/ROUTE_CONTRACT.md`（1,154 条注册路由 / 65 个模块，机器抽取）+ ledger 导出契约；⚠️ 此前引用的 `docs/synapse-rust/api-reference.md` **不存在** |
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
| **核心 CS API** | ✅ 完整 | 路由面完整（`ROUTE_CONTRACT.md` 1,154 条注册路由）；按类别人工统计覆盖率 **80–97%**（`API_COVERAGE_REPORT.md`，2026-05-28 口径，非逐端点实测） | ⚠️ 未逐端点验证 |
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
| #20146 LiveKit SFU WebSocket URL | ✅ 弃用 `livekit_service_url` 并新增 SFU URL | ⚠️ `livekit_service_url` 本仓不存在（无可弃用）；曾经的 `LivekitConfig.ws_url` 是**从未被读取**的死配置，**本轮已删除**（`config/voip.rs` 现仅 `api_key`/`api_secret`/`host`）；`rtc/transports` 仍只返回 ICE。**差异未消除**：SFU 传输本身未实现 —— 现在至少不再有一个"看起来可配"的旋钮 | ⚠️ PARTIAL |

**v1.2 遗漏的上游条目（同基准期，v1.3 补记）**：

- **v1.157.2 安全版本**：6 High / 4 Moderate / 2 Low ELEMENTSEC 公告 —— 本报告 §7 安全对比**完全未提**，应逐条判定本仓同类性。
- **v1.158 默认房间版本改为 11（MSC4239）**：本仓 `DEFAULT_ROOM_VERSION` 也是 11，但 `room_versions.rs:77-81` 注释仍称"Synapse 默认 10，本项目刻意不同"——**注释已过时**；且 v12/v13 为 `stable_parse_only`（可 join/联邦，**不可创建**）。
- **v1.158 v12 房间修复**（并发建房 room ID 冲突、v12 第三方邀请）；**MSC4326** appservice 设备伪装稳定化（本仓 0 命中）；**MSC2409** appservice 短暂事件（本仓 12 处引用）；**MSC4186** 简化滑动同步（24 处）；**MSC4502** 房间成员查询（11 处）；**MSC4262/MSC4429** Profile 更新进 sync（21 处）——这些代码中已有引用的编号，本报告 v1.2 均未列出。
- **#20189** 联邦 `make_*` 请求缺少 `membership` 校验（安全修复）——见 §12.5 B2。

### 11.2 扩展功能

> ⚠️ **历史章节 · 当前口径见 §18**：本节状态列包含已被后续提交修掉的「缺失」条目（逐条作废清单见 §18.5(a)），判断现状请以 **§18.3** 为准，**不要**据本节排期。

| 功能 | Synapse (Python) | synapse-rust | 对齐状态 |
|------|-------------------|--------------|----------|
| **好友系统** | 无（标准 Matrix 无此功能） | ✅ 独有扩展（`friend_room_service/` 含 groups/models/sharding，完整实现 + 联邦好友同步 `synapse-federation/src/friend/`） | ✅ 完整实现 |
| **阅后即焚** | 无 | ✅ 独有扩展（`burn_after_read_service.rs` 完整实现，含 BURN_MAX_RETRY=5 死信处理 + 定时扫描器） | ✅ 完整实现 |
| **信标/位置** | 无标准实现 | ✅ 独有扩展（`beacon_service.rs` 完整实现，含配额限制、背压控制、缓存） | ✅ 完整实现 |
| **内容扫描** | 模块化扩展 | 🔴 **已装配但无消费者（2026-09-25 复核，"从未被构造/未接入 config"作废）**：`synapse-services/src/wiring/core.rs:66,179` 已构造 `ContentScanner` 并注入 `CoreServices`，配置项已接入（`synapse-common/src/config/mod.rs:242`，类型在 `synapse-common/src/content_scanner/mod.rs`）；**但** `scan`/`scan_text`/`scan_media` 在生产路径 **0 调用点**，且 `synapse-storage/` **无**存储模块、`migrations/` **无**相关表 ⇒ 配置打开也不产生任何扫描行为 | 🔴 已装配但空转（比纯缺失更隐蔽） |
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
| **LiveKit / RTC** | 有 | ⚠️ **PARTIAL**：`rtc/` 存在；`livekit_service_url` 本仓从未存在，`LivekitConfig.ws_url`（`config/voip.rs:92`）**声明但无读取点**（死配置），`rtc/transports` 仅返回 ICE | ⚠️ 能力不完整 |
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
| **OIDC/Builtin OIDC** | 有 | ⚠️ 路由完整（`routes/oidc/`），但 `synapse-services/src/oidc_service.rs:673` 的 `validate_id_token_claims` **从未被调用**（`#[allow(dead_code)]`，安全相关） | ⚠️ 存在未接线校验 |
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

6. **（v1.3 新增）协议正确性缺陷比"功能缺失"更值得优先处理**：本仓默认创建房间版本 11，
   但撤回事件仍按 v1–v10 的顶层 `redacts` 格式生成（`handlers/room/events.rs:959-982`），
   而 v11 消费方从 `content.redacts` 读取 → 本服务端发出的撤回可能在合规实现上不生效。
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
- Worker 拓扑验证仍在建设中，水平扩展方案成熟度待验证。
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
  - **Content Scanner 整模块孤儿**（无存储、无 config、无构造点），非"仅缺存储层"。
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
> （`docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md`、`docs/audit/PROJECT_REMAINING_ISSUES_2026-09-14.md`）
> **没有任何交叉引用**，构成第二份 backlog（违反 AGENTS.md 铁律 2/6 的精神）；同时把上游**实验性**的
> MSC4242/MSC4512 列为"P0 阻断性"，而漏掉了本次实测发现的协议正确性缺陷。
> v1.3 起改为：**只描述差异 + 指向权威清单**，每条给出证据与验收判据，不再给人日估算（估算无依据会掩盖不确定性）。
> 完整版本见 `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md` §7。

**分层原则**：A 类（门禁与事实，已完成）→ B 类（协议正确性，本轮实测发现，优先）→ C 类（功能补齐与收敛，按上游成熟度定级）→ D 类（把文档可信度变成守卫）。

**B 类：协议正确性（建议优先于任何新功能）**

| 编号 | 现象与证据 | 动作 | 验收判据 | 上游关联 |
|------|------------|------|----------|----------|
| B1 | 默认 v11，撤回仍写顶层 `redacts`（`room_versions.rs:89,113` vs `handlers/room/events.rs:959-982`） | 撤回创建按房间版本分支写 `content.redacts`；修正过期注释 | v11 房间 `content.redacts` == 目标 id；v10 仍在顶层；互操作冒烟通过 | v1.161 #19782 |
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
| OIDC `validate_id_token_claims` 接线 | 未列 | **高（安全）** | 死代码，声明校验未生效 |
| Content Scanner 装配或删除 | P0"缺存储层" | **中（决策项）** | 现状是孤儿模块；先决定是否上线该功能 |
| MSC4140 联邦（EDU） | ✅ 已对齐 | **中** | 单机真实，联邦缺失，需按草案确认是否必须 |
| 死字段清理（3 处 "Reserved/constructor parity"） | 未列 | **中** | 违反铁律 1 |
| MSC4242 State DAG | P0 阻断性 | **低（观察项）** | 上游实验性 + 无房间版本启用；先修正 `dag.rs` 不实注释 |
| MSC4512 AS 代理 | P0 阻断性 | **低（观察项）** | 上游实验性、opt-in |
| SMS 厂商专用实现（Twilio 等） | P1 | **低** | 已有 `SmsProvider` trait + 通用 HTTP provider |
| v12/v13 创建支持 | 未列 | **待决策** | 影响与"默认 v11 + v12 房间已在联邦中存在"的兼容边界 |

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
| **代码修复（Phase 1）** | **B1** v11+ 撤回目标写入 `content.redacts`（服务层唯一写入口）+ PDU 不再重复写顶层 `redacts`；**B8** `create_event_with_graph` 无事务分支改单事务；**B10a** `send_message` 传播 `origin_server_ts` 读取错误；**B5** 修正过期房间版本注释。验证证据见 `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md` §10（原计划文件位于 gitignored 的 `docs/superpowers/plans/`，不作为持久引用） |
| **代码修复（Phase 2）** | **B10b** 联邦 gap-fill 查重吞错；**B11** 默认搜索面纳入 `m.room.name`/`m.room.topic`；**B3** 举报端点 per-user `rc_reports` 限流（可配 + 守卫）；**B9** 事务去重标记失败时 soft-fail 已提交事件。证据见 `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md` §11–§12（原计划文件位于 gitignored 目录，不作为持久引用）。**Phase 2 全部完成**：B3/B4/B9/B10b/B10c/B11/B13；遗留缺陷另记：`scripts/api_test/scan_handler_schemas.py` 的 `ROOT` 为硬编码绝对路径（会写错工作树）、ledger `query_params` 字段无消费方 |
| **代码修复（Phase 3，安全）** | **C1** SAS 规范对齐：`derive_sas` 改用 HKDF-SHA256（`synapse-e2ee/src/verification/service.rs:106`，带已知答案测试 `derive_sas_matches_hkdf_sha256_known_answer`）、`confirm_sas` 以 `synapse_common::crypto::secure_compare` 校验 HMAC（`:367`）、删掉随机 SAS 兜底（fail-closed）；**C2** QR 不再伪造载荷，改为 `ApiError::unsupported`（`:409,423`）；**C3** 删除从未编译的 `synapse-e2ee/src/leak_detection/` 死模块；**C9** OIDC `id_token` 校验失败改 fail-closed（`ApiError::unauthorized`）+ 授权 URL 携带 `nonce` + 删除绕过校验的死函数（`validate_id_token_claims` 全仓 0 命中）。提交 `a6f797f3` / `fda1317f` / `db570918` / `c010135c`；复核记录见 `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md` §13 |
| **v1.4 代码取证复核（2026-09-22）** | **推翻本文档 5 处旧结论**（详见 §14.1）：① SAS 由"SHA256 派生 + 接受任意非空 MAC"→ 实为 `derive_sas` 已 HKDF（`verification/service.rs:105-113`）、`confirm_sas` 已校验 MAC（`:316-375`），但**仍 4 处偏离规范**；② QR 由"桩"→ 实为**显式 fail-closed** `M_UNSUPPORTED`（`:403-424`）；③ `leak_detection` 由"未编译死代码"→ 实为**目录已删除**；④ `create_event_with_graph` 半写窗口由"仍存在"→ 实为 **Phase 1 已修**（本地事务，`event/create.rs:111-148`）；⑤ `validate_id_token_claims` 由"从未被调用"→ 实为**已接线**，真实缺陷在 `routes/oidc/sso.rs:215-232`。**新增 3 条 P0**（§14.2）；§11.1 联邦协议由 ✅ 降为 PARTIAL；§7.2 SSSS 新增非合规判定。<br>（2026-09-23 交叉复核：①②③④ 与 3 条新 P0 均**独立复现属实**；**⑤ 已过时** —— Phase 3 C9 已把该函数整体删除，当前树 `validate_id_token_claims` 0 命中，`exchange_code` 改为 fail-closed；但其指出的 `routes/oidc/sso.rs:215-232` 提权面**仍然存在**。） |
| **本轮（2026-09-23）** | **文档可信度变成门禁**：新增 `tests/unit/doc_credibility_guard_tests.rs` —— 本文档（1）引用的仓库相对路径必须存在、不得引用 gitignored 的 `docs/superpowers/plans/`；（2）声明的 route 条目数与模块数必须等于 `docs/synapse-rust/ROUTE_CONTRACT.md`；两个纯谓词各有红证明（把 v1.2 的"伪造引用/过期计数"原文喂进去必须被判违规）。据此前置修正：§13 两处 gitignored 计划引用改为指向复核报告；`docs/*.md` 这类 glob 与"文档有意记录某文件不存在"的否定陈述分别按过滤/白名单处理。**代码优化**：删除 LiveKit `ws_url` 死配置（`synapse-common/src/config/voip.rs`，从未被读取）；关闭 `create_state_event_with_dag` 的半写窗口（B8 因两处插入逻辑重复而漏掉的并行路径，改为本地事务，红→绿证明见 `create_state_event_with_dag_rolls_back_event_when_edges_insert_fails`）；修 `synapse-storage/src/server_notification/repository.rs` 5 处 `created_by` 缺 `Some(..)`（**main 上 CI lib 批次仍因此红**）。**更正本报告自身的过度声明**：上一版把 E2EE SAS 写成"已按规范对齐"不成立 —— 经逐行复核（info 串缺公钥且 txn 占位、emoji 仅 6 个且 decimal 被丢弃、MAC 非 `hkdf-hmac-sha256.v2`、commitment 用全零密钥 HMAC），仍 **4 处偏离规范**，§7.2/§11.1 已按 v1.4 复核改写。**计数重测（口径改为可复现的 `git ls-files`）**：`.rs` **1,019 个 / 442,451 行**；`docs` **188**（其中 `*.md` 155）；`docker` **58**；`tests` **292**（unit 132 / integration 128 / e2e 5 / performance 5 / 其余 22）；`benches` 5；`migrations` 3；per-crate 未变（common 72 / federation 20 / web 162、routes 144 / 根 `src/` 38）。⚠️ 旧版对 `docs`/`docker`/`tests` 用 `find` 统计，会随 gitignore 与本地产物漂移（同一提交在不同 worktree 得出 221/212/230 与 188/58/292 两套数）—— 故统一改为 `git ls-files` |

---

## 14. v1.4 复核保留摘要 + v1.5 修复进度（2026-09-23 续）

### 14.1 v1.4 复核的 5 处旧结论更正（保留摘要）

| 旧结论（v1.3 及更早） | 实测结论 | 证据 |
|----------------------|----------|------|
| SAS 用 `SHA256(secret‖info)` 派生，非 HKDF | **已更正**：`derive_sas` 实为 HKDF-SHA256（无 salt、info 为上下文串、取 6 字节）——⚠️ 该实现已于 **2026-09-25 随去服务端私钥重构整模块删除**，本行仅存历史（见 §14.4 item 3） | `synapse-e2ee/src/verification/service.rs`（文件已删除） |
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
| 6 | ELEMENTSEC-2026-1740 / GHSA-6wjm-9p2x-gvpm | `multipart/form-data` Content-Type 大小写绕过 DoS 缓解 | ⚠️ **待深查** | 本仓无手写 Content-Type 检查（multipart 仅经 axum `Multipart`，`routes/voice.rs`）；有全局 body 上限，但解析器内部行为属上游 |
| 7 | ELEMENTSEC-2026-1714 / GHSA-qcjr-46gf-7f4r | `/get_event_auth` 缺已入房校验 | ✅ 不受影响 | 先 `validate_federation_origin_can_observe_room`，再按 `room_id` 限定取事件 |
| 8 | ELEMENTSEC-2026-1718 / GHSA-r66v-qhwx-8rg4 | `/timestamp_to_event` 缺成员校验 | ✅ 不受影响 | 格式校验后调 `validate_federation_origin_can_observe_room` |
| 9 | ELEMENTSEC-2026-1751 / GHSA-jhcg-5392-5mjw | Sliding Sync 畸形响应（非法房间名/头像/资料） | ✅ 不受影响（结构上不可能） | 相关字段均为 `Option<String>`，类型系统保证不会输出错误 JSON 类型 |
| 10 | ELEMENTSEC-2026-1703 / GHSA-vh4c-pqh4-w3wq | 多余路径段被忽略 → 限流绕过 | ✅ 应用层不受影响 / ⚠️ 代理层待查 | axum 0.8 精确 `{param}` 匹配、无通配路由，fallback 404 `M_UNRECOGNIZED` |
| 11 | ELEMENTSEC-2026-1760 / GHSA-hgcg-p9gx-fq5f | 尾随后缀被接受 → nginx 规范化代理绕过 | ⚠️ **待深查（代理配置）** | `docker/deploy/nginx/` 使用前缀 `location`，无 `merge_slashes`；需运维侧复核，Rust 代码内不可证 |

**结论**：需要动作或明确决策的是 #1（push rule 上限）与 #5（别名劫持）；需要代理/基础设施复核的是 #6 与 #11；其余 7 条本仓已有对应守卫。

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
| N1 | **Content Scanner 是"已装配但无消费者"** —— 旧判"从未被构造、未接入 config"不成立，但真相更值得警惕：模块被真实构造并接入配置，**却没有任何调用点** | `synapse-services/src/wiring/core.rs:66,179`（构造）+ `synapse-common/src/config/mod.rs:242`（配置项）；`scan`/`scan_text`/`scan_media` 在生产路径 0 调用点，`synapse-storage/` 无存储模块 |
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
| **高** | E2EE SAS 4 处偏离规范（info 串缺公钥且顺序错 / emoji 仅 6 个且 decimal 被丢弃 / MAC 非 `hkdf-hmac-sha256.v2` / commitment 非 SHA-256） | `/keys/device_signing/verify_*` 是私有非规范 REST 面，**形状阻塞**规范修法（见 §14.4 item 3）。⚠️ **2026-09-25 该面已整模块删除**，条目作废 |
| **高** | 客户端撤回不级联 | `synapse-web/src/routes/handlers/room/events.rs:990` |
| **高** | Content Scanner 空转（装配了但不扫描） | 见 §15.1 N1 |
| **中** | MSC4242 仅存储层，且 `dag.rs` 注释声称被 `/send_join`、`/get_missing_events` 使用（实测无调用点） | `synapse-storage/src/event/dag.rs` |
| **中** | 上游 1.161 已删的 `msc2965/auth_issuer` 本仓仍在册 | `docs/synapse-rust/ROUTE_CONTRACT.md` |
| **中** | Profile 三处偏差：稳定 `/{keyName}` 未注册、停用用户写自定义字段 404、account_data 非对象语义 | 路由在册清单 + `user/storage.rs` |
| **中** | MSC4502 / MSC4262 仍为 PARTIAL 且未收敛 | 各 8 个 `.rs` 命中 |
| **中** | Admin 媒体端点族部分缺失（本仓 14 条 vs 上游 18 条；房间级列举/删除已补全） | 缺失：用户媒体隔离、按日期删除、远程缓存清理、解除保护；见 `CURRENT_ISSUES_AND_PLAN.md` U-5 |
| **中** | 缩略图 `animated` 参数未支持；`M_USER_LIMIT_EXCEEDED` 未用于媒体限额 | 两处均在业务层 0 命中 |
| **低（决策）** | v12/v13 房间不可创建 | `synapse-common/src/room_versions.rs:114-115` `stable_parse_only` |
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
- **降级**：Content Scanner 由"孤儿模块、从未被构造" → "已装配但**无消费者**"。
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
> 逐条证据与命令清单见 `docs/audit/COMPARISON_REPORT_REVIEW_2026-09-22.md`。

## 16. v1.8 代码验证更正（2026-09-26）

### 16.1 方法论

本轮复核（2026-09-26）以 `opt/consolidated` @ `9e26ee31a` 为基线，对文档中"仍存在"的问题声明进行**源码级别复核**。每项判定均给出可复现的 `路径:行号` 或命令。

### 16.2 纠正的误述

| 编号 | 原文声明 | 经证实 | 证据 |
|----|------|---|---|
| U-19-1 | "级联撤回缺乏逐事件授权检查" | ❌ **误判**：`cascade_redact_related_events` 确实在行 134 调用 `can_redact_event` | `synapse-services/src/event_redaction_service.rs:134` |
| U-19-2 | "空 `with_rel_types` 返回 400" | ❌ **误判**：行 993 处空列表意为"不级联" | `synapse-web/src/routes/handlers/room/events.rs:993` |
| U-20-1 | "reaction 不写入 events 表，级联查不到" | ⚠️ **设计取舍非缺陷**：`event_relations` 独立存储是 MSC3912 规范形式，`find_related_events_single_layer` 正确读取 `events.content->'m.relates_to'`，且 `event_relations` 表有 GIN 索引 | `cascade.rs:71-91`、`migrations/00000000_unified_schema_v12.sql:3306` |
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
| 2 | OTK 单设备单算法 500 上限（400） | ✅ **已实现**（`d409bd89b`） | `synapse-e2ee/src/device_keys/service.rs:20-21,317-326`（整批拒绝 + `MatrixErrorCode::TooLarge`，测试 `:1028/:1039/:1055` 断言 400 且不落库） |
| 3 | `redis.username`（ACL）+ 必须配密码 | ✅ **已对齐（2026-10-02，M-1；第二轮复核更正）** | `synapse-common/src/config/database.rs:187-195`（`user:pass@` / `user@` / `:pass@` 三种连接串形状）；**启动校验已补**：`validation.rs:82-88` 拒绝「配了 username 却没配 password」（空白值按未配置处理，与连接串口径一致）；**env 插值已补**：`loader.rs:81-82` 让 `redis.username` 与 `password` 走同一条 `${VAR}` 路径。⚠️ `database.rs:307` 的 username-only **形状测试**已改名为 `redis_connection_url_username_only_shape_is_formatter_only` 并写明该配置被 `Config::validate` 拒绝、运行时不可达 —— 它钉的是纯函数行为，不再是"反例合法"的证据 |
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
| 16 | `synapse_storage_stream_current_position` | ✅ **已 per-stream（2026-10-02，L-1）** | 改成**同名多标签** gauge（`{stream="…"}`）：此前 `MetricsCollector.gauges` 以 name 为 key，同名不同标签会互相覆盖，故先补 `DynamicGaugeTemplate`（与既有 counter/histogram 模板同构）。标签集合 = `StreamPosition::ALL`（`events` / `to_device` / `device_lists` / `sliding_sync` / `quarantined_media` / `worker_events`），数据源是 `synapse-storage/src/stream_positions.rs` 的**单条 UNION ALL 查询**；刷新从 admin `/statistics` 的按需更新改为 `src/server/mod.rs` 30s 指标循环周期刷新；判据 V-12 落地两条集成测试（标签集合与登记表逐字一致 + 每个 stream 随写入推进，含红证明）。**未覆盖**：upstream 的 presence/typing/receipts/account_data/push_rules/e2ee/backfill/federation 在本仓**没有位置列**；`sync_stream_id`、`device_lists_outbound_pokes` 有列但**无生产写者**（恒 0）；`worker_events` **已接线**（P-5：worker 模式下由事件写入装饰器发布，单进程部署恒 0 属预期而非漂移）；`room_ephemeral.stream_id` 是调用方传的墙钟毫秒且被 UPSERT 覆盖（非单调）。**worker-local** 未做：本仓位置取自 DB（写者无关），per-stream 已足，而刷新循环只在 global-maintenance owner 中运行 ⇒ 按 `stream_writers` 过滤反而会让 worker 部署少几条序列；仪表盘/监控文档同步登记为 §18.4 L-1b。**⚠️ 本轮修复的真实缺陷（合流时保留）**：per-stream 落地后，`to_prometheus_format()`（`synapse-common/src/metrics.rs:747`）必须**按指标族去重** `# HELP`/`# TYPE`；动态模板（`DynamicGaugeTemplate::set`）把**每个标签组合**存成独立条目，逐 entry 输出会让同一族出现多份元数据行。`room_operations_total` / `cache_operations_total`（`synapse-common/src/server_metrics.rs:417,421`）早已如此。Prometheus 文本格式规定 _「Only one `HELP` line may exist for any given metric name」_（[exposition formats](https://next.prometheus.io/docs/instrumenting/exposition_formats/)），重复元数据行会让**整个 scrape 被拒绝**。修复与两条用例（`test_to_prometheus_format_dedups_help_and_type_for_dynamic_gauge_family` / `…_counter_family`）已随合流入库 |
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
| **M-4** | ~~第三方规则回调接入事件鉴权/消息发送路径~~ → **✅ 已交付（2026-10-02 合流后，D-1）**：`check_event_allowed()` 以 `EventAdmissionGate` + 共享踏板 `consult_event_admission` 落地（`synapse-services/src/module_service.rs:135-225`），本地两咽喉 + membership 本地/联邦写入点 + 房间创建序列（`synapse-services/src/room/lifecycle/create_events.rs:89`）+ burn-after-read 全部在状态变更前咨询；触发侧 `third_party_rules` 配置启动即注册；失败语义取 **fail-closed**（对齐上游 v1.49.0 #11033），内容改写仅本地生效（联邦 `allow_modification=false`）。**合流去重**：本仓只保留这一份实现（feature 分支的 `EventAdmissionGate`）。`c19b` 工作树里那份未提交的**第三份**实现（`ThirdPartyRuleRegistry` / `run_third_party_rules`）已**由该工作树的写者自行清理**（2026-10-02 18:07 起 `git status` 干净、无新提交、无 stash；`git grep ThirdPartyRuleRegistry` 在所有 ref 上只命中本文件的历史记载与抢救后的用例别名）。⚠️ 复核期间该写者一直在写入（`module_service.rs` mtime 16:24 → 17:03 → 17:29），按铁律 9 未打断在途工作 —— 事后核对证明这是对的：那份工作里**独有的集成用例已被抢救**（`tests/integration/third_party_rules_event_paths_tests.rs`，7 passed），否则会随清理一起消失 | 见 §18.3 #12、§18.7 | — | — |
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
| §12.4 Content Scanner"未装配" | 已装配，但**无消费者**（真缺口是"装了不扫"） | §15.4 L970；`synapse-services/src/content_scanner/*` |
| §12.4 App Service 登录"整体缺失" | AS 登录**已实现** | §12.5 L931 |
| §12.5 SAS"高" | 该面**已整模块删除**（去服务端私钥重构），条目作废 | §15.3 L944 同行的"作废"标注 |
| §12.5 QR"当前为桩" / `leak_detection`"未编译" / OIDC `validate_id_token_claims`"死代码" | 三者均**已删除** | §7.2 L387；§14.1 L71 |
| §12.5 MSC4140"无法联邦/EDU" | EDU **已实现** | §12.4 L718 |
| §12.5 MSC4242"❌ 缺失" | **已进 v12 图路径**（存储层），仅 HTTP 面缺（§18.3 #18）；**取证更正（2026-10-02）**：上游 #20133 是把 MSC4242 接进**既有**联邦端点（`/make_join`、`/send_join`、`/get_missing_events`、`/send` 目的地），**不新增路由**，受阻于 MSC4242 房间版本 + experiment（§18.4 L-2） | `synapse-storage/src/event/dag.rs:255-290` |
| §12.5 MSC4512"缺失" | 代理**已实现** | §12.5 L931 |
| §12.4"撤回格式与默认房间版本不匹配""MSC3912 未实现" | **均已修**（默认版本 12 + MSC3912 级联） | §15.3 L945/L719 |
| §15.3 "v12/v13 不可创建" | v12 **是唯一可创建**版本，v13 已移出能力表 | `room_versions.rs:94,119-152` |
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
| V-2 | **MSC4311 语义**（H-2）：入站对 stripped 与 full PDU 两种 `invite_room_state` **都接受**；宽限阈值行为正确；knock stripped state 进入客户端 `/sync` | **已落成用例**：`cargo nextest run -p synapse-web --lib --features test-utils -E 'test(msc4311)'`（3 passed，含 strict 红证明路径）与 `cargo nextest run -p synapse-services --lib --features test-utils -E 'test(knock_stripped)'`（含 knock 分类与 `knock_state` 键）。**仍需联调**：与 Synapse v1.162 双向 `/invite`、`/knock`，断言开关真被读取（把 `msc4311_strict_validation=true` 后旧形状被拒） |
| V-3 | `redis.username`：username-only 配置**启动失败**（对齐上游）；`${VAR}` 插值生效 | **已落成用例**：`cargo nextest run -p synapse-common --lib -E 'test(redis_username)'`（3 passed）与 `-E 'test(resolve_env_variables_resolves_redis_password)'`（1 passed）；另加 `env_override_needs_a_double_underscore_after_the_prefix` 钉死覆盖拼写必须是双下划线形式（README/CONTRIBUTING 原先写的单下划线形式静默不生效，已修正） |
| V-4 | MSC4222：opt-in 客户端拿 `state_after`（含空）、默认客户端形状不变、旧参数不再生效 | **已落成用例**：`cargo nextest run -p synapse-services --lib --features test-utils -E 'test(msc4222)'`（4 passed）与 `cargo nextest run -p synapse-web --lib --features test-utils -E 'test(msc4222)'`（2 passed）；`cargo nextest run --all-features --test integration -E 'test(sync_authenticated_initial)'` 默认路径快照零 diff |
| V-5 | `/room_summary` `join_rules` 新鲜度：join_rules 变更后立即查询返回新值 | **已落成用例**：`cargo nextest run --features test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,cas-sso,saml-sso --test integration -E 'test(summary_join_rule_follows)'` → 2 passed；红证明见 §18.3 #13 行（移除 state→summary 投影 / 移除创建补偿） |
| V-6 | OTK 上限边界：恰好 500 通过、501 拒绝、`M_TOO_LARGE`、不落库 | 复用 `synapse-e2ee/src/device_keys/service.rs:1028+` 的用例风格补边界 |
| V-7 | `allowed_room_ids` 语义：restricted / knock_restricted 房间返回 allow 房间集合；联邦面不新增泄漏 | `GET /_matrix/client/v1/rooms/{roomId}/hierarchy` 集成断言 + 联邦响应字段白名单测试 |
| V-8 | profile `{}` 变更的客户端影响 | grep SDK/前端是否把 `""` 当"未设置"；改后跑 SDK `spec/unit/account*.spec.ts` 与快照 |
| V-9 | 历史 user id 过滤不误伤本地用户 | 用非合规 localpart 构造 `m.device_list_update`；确认既有测试用户全部合规（`is_compliant_user_id_localpart`） |
| V-10 | 第三方规则接入事件鉴权后的失败语义（fail-open/closed）与性能 | **集成层已补齐（2026-10-02）**：`tests/integration/third_party_rules_event_paths_tests.rs` 7 用例驱动真实 HTTP 面（空规则不回归 / 本地拒绝 `403`+规则 reason / 规则 `Err` fail-closed / 本地改写落库 / **创建序列被 gate 拒绝** / 联邦 PDU 拒绝且不落库 / 联邦入站改写不生效）；服务层另有 20 用例。该层同时修掉了"发送路由把 403 包成 500"的缺陷。**性能：机制已查清，数字仍缺**。机制（读码确认）：`consult_event_admission`（`synapse-services/src/module_service.rs:190-240`）先走 `has_event_rules()` 快路径 —— **无规则时只取一次内存锁，可忽略**；**一旦注册了任何规则**，每次事件写入都会调 `event_reader.get_state_events(room_id)` 并**物化整房 state**（`synapse-storage/src/event/state.rs:116-143`：`current_state_group_id` + `state_events_of_group`，或退化路径的 `DISTINCT ON` 全表扫）来构造规则上下文，且**没有任何缓存**。⚠️ 这与已有的 `room_state:{room_id}`（TTL 300，失效由状态写入路径负责，`synapse-services/src/sliding_sync_service/state.rs`）**职责重复** ⇒ 是"同一职责只允许一份实现"的候选。**复用已交付（2026-10-03）**：抽出唯一实现 `synapse-services/src/room_state_cache.rs`（key / TTL / 失效契约的唯一登记处），`ModuleService::room_state_for_rules` 与 sliding-sync 共用**同一条目**；判据 `room_state_for_rules_is_served_from_the_shared_cache`（预置哨兵值 ⇒ 命中缓存即证明没有回源）与 `room_state_for_rules_populates_the_shared_cache_on_a_miss`。**仍需压测**：给出消息主路径 p99 与规则超时护栏数字（fail-closed 意味着**规则超时会拒消息**，不能靠推断） |
| V-11 | 递归 relations：`recurse=true` 不得 400；按 MSC 示例图（A←B/G 的 `m.thread`、A←D 的 `m.edit`、B←E 的 `m.annotation`，另有无关事件作对照）断言 `rel_type=m.thread` ⇒ `[B, G]`、`recurse=true&dir=f` ⇒ `[B, D, E, G]`、`recurse=true&dir=b&limit=2` ⇒ `[G, E]`；`origin_server_ts` 与 `stream_ordering` 刻意相反以证明拓扑序；深链第 6 跳缺席且 `recursion_depth=3`；`rel_type=m.annotation` ⇒ `[E]`（过滤作用于返回集）。⚠️ MSC 的**第 5 个示例**（`rel_type=m.annotation&event_type=m.reaction` ⇒ `[G, E]`）不可作判据：G 在图中只挂 `m.thread`，该期望与 MSC 自己的图矛盾且与 `dir=b&limit=2` 的结果逐字重复，属提案勘误 | 判据已落地：`cargo nextest run --profile ci --all-features --test integration msc3981`；`EXPLAIN (ANALYZE)` 证据在 §8.3 M-6 卡片（同形状夹具 50k 事件/50k 关系：基步与递归步的 `events` 都走 `events_pkey` 索引扫描，递归项 `Filter: (depth <= 3)` 循环 5 次后终止） |
| V-12 | stream 指标：per-stream/worker-local 口径（标签、刷新时机）与仪表盘一致性 | **2026-10-02 合流后重述**（原记的「2 条 series + `/metrics` 抓取时刷新」与测试名 `test_storage_stream_current_position_gauge` 均已作废 —— 那是被删除的第二份实现）：口径为 `StreamPosition::ALL` 的 **5 条** series、单一 `UNION ALL` 数据源、`src/server/mod.rs` 的 30s 循环刷新。判据：`tests/integration/stream_position_tests.rs` 的 `stream_labels_match_the_registry_exactly`（标签集合与登记表逐字一致）与 `every_stream_position_advances_with_writes`（随写入推进）—— **两条已实际跑通**（44 passed 批次内）；单元层另有 `synapse-common/src/server_metrics.rs::test_storage_stream_current_position_is_per_stream`。仪表盘：`monitoring/grafana/dashboards/grafana-dashboard-business.json`（L-1b 的 per-stream 面板）与 `docker/deploy/grafana/dashboards/storage-performance.json`；`tests/promql-queries.md` 同步。**worker-local 归属**未做且已裁定不做（理由见 §18.4 L-1） |
| V-13 | 本文档计数与假缺口清单 | 计数守卫已存在（`counts_match_the_route_contract`）；**§18.5(a) 条目不得再作为「现状」出现**已落成门禁：五个历史章节必须带 §18 指针 (`cargo nextest run --test unit --features test-utils -E 'test(doc_credibility_guard_tests)'`)。⚠️ **2026-10-02 第二轮更正**：本行此前记的"6 passed"是**假绿**——L-6 把 `ROUTE_CONTRACT.md` 更新到 1,154 条后，本文三处仍写 1,152，`counts_match_the_route_contract` 实为**判红**。合流时已同步三处；**"守卫存在"不等于"守卫是绿的"**，该守卫必须进提交前脚本才有效（见 §18.7.3 P-3） |

### 18.7 双线合流记录与剩余问题（2026-10-02）

> 本轮把两条并行线合流：`opt/consolidated` @ `f8c45b73d` 合入 `feature/2026-10-01-metrics-docs-updates` @ `418d32d8b`
> （两线在 `46fe964b9` 分叉、互不为祖先）。合流在 feature 分支侧进行（该树 `git status` 干净、`target` 已热），
> **11 个文件冲突**全部逐项裁定。

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
| **P-7** | 两条辅助文档的条目口径在两侧各写一版 | **已闭环（2026-10-02/03）**：`docs/audit/OPTIMIZATION_EXECUTION_PLAN_2026-09-15.md` 的 L-1/L-3 行与 `docs/synapse-rust/API_COVERAGE_REPORT.md` 的 #20097 / #20181 / L4 行均已按合并后的实现对齐；P-5 又把两处的 stream 口径从 5 条更新为 **6 条**（`worker_events` 接入） |
| **P-8** | 合流结果需完整门禁验证 | **已闭环（2026-10-02/03）**：clippy 两档 `EXIT=0`；`./scripts/check_fmt_ratchet.sh` current=0 baseline=0；契约 gate `EXIT=0`；doc 守卫 6 passed；定向用例累计 **105+ 条**全绿（gate 7 / relations+stream 44 / 发送路径 43 / storage relations 35 / room_summary 2 / unit 9）。⚠️ **完整 1522 条集成批次未跑完**（为提交多次让出 cargo 锁；跑到 432/1522 时只有两条既有失败，均已处置）⇒ 全量以 CI 为准 |

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
| **P-6** | 需联调 / 外部对端的验证未闭环；V-10 的 p99 数字仍缺 | V-2 与真 Synapse v1.162 双向 `/invite`、`/knock`；V-7 联邦面 `allowed_room_ids` 不新增泄漏；V-8 SDK 对 profile `{}` 的影响；V-10 准入钩子在**消息主路径**的性能与超时语义（**机制已查清**，详见 V-10 行：规则注册后每次事件写入都会**无缓存地物化整房 state**，而 `room_state:{room_id}` TTL 300 已提供同一份数据 ⇒ 是铁律 2 的重复实现） | 按 §18.6 逐项补判据。**V-10 的缓存复用：已交付（2026-10-03）**——对全部 `state_key: Some(...)` 生产写入点逐函数核对后确认：**没有任何绕过失效的写路径**。中心失效点在 `MessagingService::create_event` / `create_event_with_graph` / `create_outlier_event`（`synapse-services/src/room/messaging/events.rs:321,419,523`，`if state_key.is_some() { delete room_state:{room_id} }`），因此 `set_pinned_event_ids`（同文件 `:665`，走 `self.create_event`）、`upgrade_room` 的 `m.room.tombstone`、`friend_room_service::send_state_event_inner`（都经 `messaging().create_event`）**都被覆盖**；membership/moderation/federation 另有显式删除；建房序列由 `room/lifecycle/create.rs:540` 收尾删除。（审计中一度判定 `auth_events.rs:214` / `state_record.rs:623` / `actions.rs:577,876` 缺失效，复核后确认**均在 `#[cfg(test)]` 夹具内**，非生产写路径。）⇒ **已实施（2026-10-03）**：抽出唯一实现 `synapse-services/src/room_state_cache.rs`（`room_state_cache_key` / `ROOM_STATE_CACHE_TTL_SECS` / `cached_room_state`，失效契约在该模块的文档里登记），`sliding-sync` 的内联缓存删除并改为调用它 ⇒ **该职责回到一份实现**（铁律 2）。落地形态：`EventAdmissionGate` 增 `room_state_for_rules(room_id, reader)`，`ModuleService` 用共享缓存实现（miss 才回源），`FakeEventAdmissionGate` 直读 reader；因此 `consult_event_admission` 的 10 个调用点**一处未改**（reader 参数保留，只在 miss 时使用）。判据：`room_state_for_rules_is_served_from_the_shared_cache`（预置哨兵 ⇒ 命中即证明未回源）、`room_state_for_rules_populates_the_shared_cache_on_a_miss`、以及原有的 `sliding_sync_service::tests::room_state_cache_{hit_avoids_storage,invalidation_clears_on_write}` 全部通过。**仍未做**：p99 压测数字与 V-2/V-7/V-8 联调 —— fail-closed 意味着**规则超时会拒消息**，只能实测 |
| **P-13** | 两条 stream 位置列**不是「没有写者」，而是数据模型本身有问题**（原 P-5 的 ②③，仍未修） | ② `device_lists_outbound_pokes.stream_id`：只有 DELETE 路径、**没有 INSERT** ⇒ 该列永远不会推进；③ `room_ephemeral.stream_id`：调用方传的是**墙钟毫秒**且被 UPSERT 覆盖 ⇒ **非单调**，拿它做游标会在时钟回拨时倒退 | 修好前**不得**纳入 `StreamPosition::ALL` —— 否则「永远是平线 / 会倒退」的序列会让积压类告警永不触发或误触发，比缺指标更危险。两条都需要各自的数据模型修正（② 补写者或删列；③ 改用单调序列）|

#### 18.7.4 合流后的门禁清单（按序，缺一不可）

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
