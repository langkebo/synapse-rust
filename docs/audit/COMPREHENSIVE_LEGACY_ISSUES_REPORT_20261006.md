# synapse-rust 全面遗留问题排查报告

> 项目：synapse-rust v6.2.0（Rust Matrix homeserver，Cargo workspace，~10 万行）
> 报告日期：2026-10-06
> 基准：Synapse v1.161.0 / Matrix Spec v1.15（v12 房间版本）
> 上一版全面报告：[`COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261004.md`](archive/COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261004.md)（已归档）
> 排查手段：**静态代码审查 + 现有门禁/扫描**（clippy、cargo deny/audit、machete、ci 脚本、安全扫描）；**未**执行全量测试与性能基准
> 结论摘要：**未发现未修复的 P0**；确认或新发现 **3 个 P1**（已于 2026-10-06 全部闭环，见 §8.1/§8.2）、**26 个 P2**（另列依赖卫生 `DEAD-01`，已于 2026-10-06 闭环，见 §8.3）、**27 个 P3**；复核推翻（证伪/已修复）**9 条**历史遗留结论。

---

## 一、执行摘要

### 1.1 排查范围与方法

本次排查覆盖用户要求的全部 7 个领域，采用「基线门禁实测 → 7 域并行静态调查 → 逐条真伪复核 → 分类与定级」的流程：

| 领域 | 主要手段 | 产出编号 |
|------|----------|----------|
| 冗余代码与过时文档 | 全文检索、重复 SQL 计数、死代码/allow 普查 | `RED-*` |
| 代码质量与技术债务 | clippy lint 普查、超大文件统计、错误吞咽检索 | `CQ-*` |
| 功能完整性与正确性 | 路由↔handler 追链、协议实现比对、桩函数识别 | `FED-*` / `FN-*` |
| 性能与稳定性 | 索引/查询计划审查、无界查询检索、门禁逻辑审查 | `PERF-*` |
| 安全性 | 越权链路复核、验签链路复核、注入/SSRF/路径遍历检索 | `SEC-*` |
| 兼容性与可访问性 | 联邦响应契约、errcode/status 映射、迁移依赖审查 | `COMPAT-*` |
| 文档与知识库 | 计数一致性、链接可达性、门禁覆盖面审查 | `DOC-*` |

### 1.2 基线门禁实测结果

| 门禁 / 工具 | 结论 | 证据 |
|-------------|------|------|
| `cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings` | ✅ exit 0，无告警 | 本地实测 |
| `scripts/quality/format_check.sh` | ✅ json/toml/yaml Passed | 本地实测 |
| `check_route_layering.sh` | ✅ No route-layer violations | 本地实测 |
| `scripts/contract/check_route_contract.sh` | ✅ allowlist 0 命中；仅生成时间戳漂移 | 本地实测 |
| `check_trait_ratchet.py` | ✅ TOTAL 69→68（下降） | 本地实测 |
| `check_web_layering.py` | ✅ 0 offending file(s) | 本地实测 |
| `check_connection_budget.py` | ✅ 50×4+20=220 ≤ 250；文档 == 代码 == 运行配置 | 本地实测 |
| `check_memory_budget.py` | ⏭ 无 Rust 变更，跳过（基线参数已入 `memory_budget_baseline`） | 本地实测 |
| `check_axum_path_syntax.py` | ✅ OK | 本地实测 |
| `check_rand_rng_ratchet.sh` | ✅ 与 baseline 一致（40 处） | 本地实测 |
| `check_sqlx_cache_fresh.sh --static` | ✅ .sqlx/ 1325 条，已跟踪 | 本地实测 |
| `check_ts_order_tiebreak.py` | ✅ 单键站点 71/30 与基线一致 | 本地实测 |
| `cargo deny check advisories` | ✅ advisories ok | 本地实测 |
| **`cargo machete`** | ✅ **0 个未使用依赖**（原 10 个已清理并接入 pre-push，见 `DEAD-01`/§8.3） | 本地实测 |
| **`check_feature_matrix.py`** | ⚠️ `all-extensions` 组合 **cargo check 超时（>300s）** | 本地实测 |
| `cargo geiger` | ⏭ 本地全量运行超时；CI 门禁基线 `prod_unsafe_total=0`（硬阻断） | [`geiger_baseline.json`](../../scripts/ci/geiger_baseline.json)、[`ci.yml`](../../.github/workflows/ci.yml#L1026-L1045) |
| `cargo audit` | ⚠️ 本地无法运行（`~/.cargo/advisory-db` 目录状态导致 clone 拒绝）；`cargo deny`（pre-push 门禁）已覆盖 | 本地实测 stderr |

> **门禁整体健康**：既有门禁全部通过，说明项目在编译、格式、分层、契约、预算维度无回归。仍存覆盖缺口：feature matrix 超时（`COMPAT-08`）待处理。已闭环缺口：`.sqlx` 缓存内容新鲜度（`COMPAT-06`，见 §8.4）、`cargo machete` 依赖卫生（`DEAD-01`，见 §8.3）、预算门禁读真实配置（`PERF-04`/`PERF-05`，2026-10-06 闭环，见 §8.5）。

### 1.3 发现总览（去重后）

| 优先级 | 数量 | 说明 |
|:--:|:--:|------|
| **P0** | 0 | 上一版登记的安全 P0（`login_as_user` 越权）本轮复核**已修复** |
| **P1** | 3 | 联邦互操作 1 + 权威文档失实 2（**已于 2026-10-06 全部闭环**，见 §2.A 与 §8） |
| **P2** | 26 | 安全 1、功能 1、性能 6（其中 `PERF-04`/`PERF-05` 已于 2026-10-06 闭环，见 §8.5）、兼容 6（其中 `COMPAT-06` 已于 2026-10-06 闭环，见 §8.4）、质量 5、冗余 4、文档 3；另列依赖卫生 `DEAD-01`（已于 2026-10-06 闭环，见 §8.3） |
| **P3** | 27 | 加固项、by-design 桩、决策项、文档细节 |
| **证伪/已修复** | 9 | 见第五节，须从权威清单移除 |

### 1.4 关键结论

1. **安全态势良好**：上一版 `PERMISSION_RBAC_AUDIT_2026-10-05.md` 的 P0-1（admin 冒充超管）、P1-1（`/devices/delete` 匹配错位）、P1-2（审计结果恒 success）三项**均已修复**（详见 5.1）。未发现新的可利用越权/注入/SSRF/路径遍历。
2. **最高优先级功能缺陷（已闭环）**：联邦 `/send_join` 响应缺规范必需的 `event` 字段——本项目权威清单早已登记为「已收窄但语义不合规」`[P0-1]`，已于 2026-10-06 补齐并加测试（`FED-01`，见 §8.1）。
3. **文档可信度是最大系统性风险（P1 部分已闭环）**：`.trae/rules/project_rules.md`（agent 常驻指令源）与 `UNRESOLVED_ISSUES_SUMMARY.md`（权威问题清单）均含大量与代码不符的结论；相关 P1（`DOC-01`/`DOC-02`）已修复，但文档防漂移门禁仍覆盖过窄、无法自动拦截回归（`DOC-24`）。
4. **门禁盲区（持续收尾）**：`cargo machete` 未接入任何门禁（`DEAD-01`）**已于 2026-10-06 闭环**（清理 10 个未使用依赖 + 入 pre-push，见 §8.3）；`.sqlx` 缓存仅做 `--static` 存在性校验、不校验新鲜度（`COMPAT-06`）**已补「源码查询指纹 vs 缓存」断言**（2026-10-06 闭环，见 §8.4）；预算门禁只读文档不读运行配置（`PERF-04`/`PERF-05`）**已改为解析真实运行配置、断言文档==代码==运行，并令 `high` 风险阻断**（2026-10-06 闭环，见 §8.5）。

### 1.5 全量测试与性能门禁运行证据（2026-10-06）

> 承接 §1.2 静态门禁：本轮进一步执行**全部四条 CI 测试车道 + 三个性能门禁**，补齐「零缺陷 / 全部闭环」结论所需的**运行证据**。
> 环境：本地 PostgreSQL（`synapse_test` / `synapse_bench`）+ Redis；`TEST_DB_TEMPLATE_SCHEMA=public`、`E2EE_INTEROP=1`、`SQLX_OFFLINE=true`。

| 车道 / 门禁 | 命令口径 | 结果 |
|-------------|----------|------|
| lib（crate 单测） | `cargo test --workspace --lib --all-features` | ✅ **6683 passed / 0 failed**（cache 94、common 996、e2ee 394、federation 308、root 40、services 2184、storage 1851、test-utils 4、web 812）；3 例 `bench_friend_list_*` 按 CI 口径过滤 |
| unit | `cargo nextest run --test unit --features test-utils` | ✅ **1824 passed / 0 failed / 2 skipped** |
| integration | `cargo nextest run --test integration --all-features` | ✅ **1534 passed / 0 failed / 0 ignored**（4121.50s） |
| e2e | `cargo nextest run --test e2e --all-features` | ✅ **20 passed / 0 failed / 7 ignored**（ignored 为需运行中 homeserver 的 `E2E_RUN=1` opt-in 用例） |
| 计算性能门禁 | `bash scripts/ci/compute_perf_gate.sh` | ✅ **PASSED**（measured=17 breaches=0 missing=0）：`state_resolution_chain_10=268.53ns`、`_100=296.82ns`、`auth_chain_build_10=5.15µs`、`membership_transitions/*=1.03–1.74ns` |
| Sliding Sync 性能门禁 | `bash scripts/ci/sliding_sync_perf_gate.sh`（`STRICT=1`） | ✅ **PASSED**（33/33 样本 < 5000ms；预热后 p95 ≈ 0.70–0.90ms） |
| 分页性能门禁 | `bash scripts/ci/pagination_perf_gate.sh` | ✅ **PASSED**（fixture 150000 行；keyset 较 offset **10.54×**（≥2.0×）；深页走有序索引、无 Sort 节点） |

> 运行中修复 1 处集成存量红（判定为**测试过时**，非产品缺陷）：`api_auth_routes_tests::test_versions_and_public_capabilities_match_declared_room_version_surface`（`available.len()` 12 vs 1），根因 G-1（`7489b247f` 创建面收窄至 v12、`8687d8335` 漏改本文件），详见 §8.13。

---

## 二、问题清单

编号规则：`<域>-<序号>`；每个问题含 **描述 / 影响范围 / 证据 / 建议方案**。

### 2.A P1 级问题（3）

#### FED-01 联邦 `/send_join` v1/v2 响应缺失规范必需的 `event` 字段
- **状态**：✅ **已闭环（2026-10-06）**（详细修复见 §8.1）
- **严重程度**：P1（联邦互操作 / 协议合规）
- **描述**：`/send_join` 的两个版本响应体只有 `origin` / `room_id` / `event_id` / `state` / `auth_chain`，**缺少规范要求的 `event`**（即已签名的 m.room.member join PDU 正文）。代码在 `re_sign_pdu_locally` 后**确实签了**事件，却未把它放进响应。
- **影响范围**：远端（发起入群）服务器无法取得/校验 join 事件 PDU，可能拒绝完成入群流程或 DAG 断裂 → 跨服入群互操作失败。
- **证据**：[join.rs v1 响应](../../synapse-web/src/routes/federation/membership/join.rs#L229-L235)、[join.rs v2 响应](../../synapse-web/src/routes/federation/membership/join.rs#L364-L370)、[签名动作 :328](../../synapse-web/src/routes/federation/membership/join.rs#L325-L328)；权威清单登记 [`UNRESOLVED_ISSUES_SUMMARY.md:249`](UNRESOLVED_ISSUES_SUMMARY.md#L249)（P0-1「已收窄：字段补齐但语义不合规」）。
- **建议方案**：在响应对象追加 `"event": <signed join PDU>`（v1 为 `[200, {...}]` 内），复用 `build_pdus`/投影后的权威 PDU；补联邦集成测试断言 `event` 存在且 `hashes`/`signatures` 可校验。

#### DOC-01 `.trae/rules/project_rules.md` 目录结构与迁移计数严重失实
- **状态**：✅ **已闭环（2026-10-06）**（详细修复见 §8.2）
- **严重程度**：P1（权威指令源失实）
- **描述**：该文件是每次会话加载的 **workspace 常驻规则**，但其 §1.3 目录结构严重偏离现实：称 `migrations/`「约 50 个」迁移文件（**实际仅 1 个 SQL**）；列出 `src/storage/`、`src/services/`、`src/web/routes/` 等**实际不存在的路径**（代码已迁至 workspace crate：`synapse-storage/`、`synapse-services/`、`synapse-web/`）。
- **影响范围**：误导所有后续 agent/开发者；与「文档即事实来源」的项目自述冲突，削弱整个文档体系可信度。
- **证据**：[project_rules.md §1.3](../../.trae/rules/project_rules.md)、[migrations/ 实际内容](../../migrations/)（仅 `00000000_unified_schema_v12.sql` + 2 个 md）。
- **建议方案**：按当前 workspace 结构重写 §1.3；迁移计数改为「1 个统一基线 + 0 个 delta」；同步 §7.0 ServiceContainer 段中的路径（`src/services/container.rs` → `synapse-services/src/container.rs`，见 `DOC-06`）。

#### DOC-02 `UNRESOLVED_ISSUES_SUMMARY.md`（权威问题清单）多条结论过时且含破路径
- **严重程度**：P1（权威问题清单失真）
- **描述**：作为「当前仍存问题」的权威来源，其中多条标「仍存」的条目经复核实为**已修复**（详见 5.2），引用的代码路径也已不存在（`synapse-web/src/routes/room/messaging/messages.rs` 整个目录已不存在）。
- **影响范围**：跟踪清单失真会让团队重复排查已解决项、遗漏真实残留，直接损害工程决策。
- **证据**：[`UNRESOLVED_ISSUES_SUMMARY.md:25-37`](UNRESOLVED_ISSUES_SUMMARY.md#L25-L37)（事务去重「仍存」实为已修复）、[:126-139](UNRESOLVED_ISSUES_SUMMARY.md#L126-L139)（Profile「仍存」实为已注册）、[:143-155](UNRESOLVED_ISSUES_SUMMARY.md#L143-L155)、[:159-169](UNRESOLVED_ISSUES_SUMMARY.md#L159-L169)、[:190-201](UNRESOLVED_ISSUES_SUMMARY.md#L190-L201)。
- **建议方案**：本报告已同步刷新该文件（见第四节）；后续将其纳入文档防漂移门禁（`DOC-24`），并在 CI 中断言引用的 `file:line` 可达。

### 2.B P2 级问题（26）

#### 安全（1）

| 编号 | 问题 | 影响范围 | 证据 | 建议 |
|------|------|----------|------|------|
| **SEC-01** | 联邦 `m.signing_key_update` EDU 处理**无独立验签**：仅做 `user_matches_origin` 域校验，随后以 `signatures: Null` 直接 `upsert_federation_cross_signing_key`，且未校验 base64 密钥长度/格式 | ~~该 EDU 随 X-Matrix 签名事务投递，传输层已认证，故非直接可利用；属纵深防御缺口：恶意/失陷的同域服务器可写入任意（畸形）交叉签名公钥~~ | [edu.rs:574-663](../../synapse-web/src/federation/edu.rs#L574-L663)（:590 域校验、:634 `signatures: Null`、:640 upsert） | ✅ **已闭环（2026-10-06）**：upsert 前新增 32 字节 ed25519 公钥校验（`decode_base64_32`，失败 `dropped += 1` + `warn!`），拒绝畸形/超长载荷落库（见 §8.9） |

> 说明：功能域曾把「交叉签名上传无验签」（`FN-02`/`SEC-01` 早前编号）标为 P1，经复核判定为**死代码**（见 5.3），本报告归并为 P3 加固项，不再列为安全 P2。

#### 功能完整性与正确性（1）

| 编号 | 问题 | 影响范围 | 证据 | 建议 |
|------|------|----------|------|------|
| **FED-02** | `/send_join` 家族仅做 `validate_federation_member_event`（域/形状），**未调用** `verify_event_content_hash` / `verify_pdu_sender_signature` 做 PDU 级哈希与签名校验 | ~~被请求级 X-Matrix 签名部分缓解；但事件内容哈希/发送者签名未校验，削弱端到端事件完整性保证~~ | [join.rs:140](../../synapse-web/src/routes/federation/membership/join.rs#L140)、[:283](../../synapse-web/src/routes/federation/membership/join.rs#L283) | ✅ **已闭环（2026-10-06）**：新增共享 `verify_inbound_join_pdu_integrity` 并接入 v1/v2；采「有则校验/无则放行」——仅当入站带 `hashes` 时校验内容哈希、仅当其 `signatures` 覆盖 sender 自身服务器时校验发送者签名，兼容规范的模板请求体（见 §8.9） |

#### 性能与稳定性（6）

| 编号 | 问题 | 影响范围 | 证据 | 建议 |
|------|------|----------|------|------|
| **PERF-01** | `get_token` 双查询回退：首次未命中后再查一次 | ~~Token 校验热路径，QPS 高时放大 DB 负载~~ | [token.rs:132-159](../../synapse-storage/src/token.rs#L132-L159) | ✅ **已闭环（2026-10-06）**：合并为单查询 `WHERE token_hash IN ($1, $2) AND is_revoked = FALSE ORDER BY (token_hash = $1) DESC LIMIT 1`（当前哈希优先），热路径 DB 往返 2 → 1；`.sqlx` 已重生成、门禁通过（见 §8.9） |
| **PERF-02** | `get_daily_message_count` 对 `events` 做**全库 24h 扫描**，缺 `(event_type, origin_server_ts)` 复合索引；生产被 admin 端点调用 | ~~管理端统计慢查询；大表下随时间恶化~~ | [basic.rs:183-195](../../synapse-storage/src/event/basic.rs#L183-L195)、[admin/server.rs:139](../../synapse-web/src/routes/admin/server.rs#L139) | ✅ **已闭环（2026-10-06）**：新增复合索引 `idx_events_type_origin_ts(event_type, origin_server_ts DESC)`，覆盖「类型等值 + 时间范围」双谓词（见 §8.6） |
| **PERF-03** | `get_user_tokens` 与 `get_updates_by_status` **无 LIMIT** | ~~用户 token/更新记录多时全量拉取，内存与延迟风险~~ | [token.rs:162-174](../../synapse-storage/src/token.rs#L162-L174)、[background_update.rs:386-403](../../synapse-storage/src/background_update.rs#L386-L403) | ✅ **已闭环（2026-10-06）**：两处加宽松上限 `LIMIT $2`（`MAX_USER_TOKENS`/`MAX_UPDATES_BY_STATUS`=10_000）+ 命中告警（见 §8.6） |
| **PERF-04** | 连接预算门禁 `check_connection_budget.py` **只校验文档表**，不读取运行配置 | ~~若运行配置实际超出预算，门禁不会变红 → 假绿~~ | [`scripts/ci/check_connection_budget.py`](../../scripts/ci/check_connection_budget.py) | ✅ **已闭环（2026-10-06）**：门禁改为解析真实运行配置（`database.rs` 默认 / `homeserver.yaml` / `docker-compose.yml` 三源一致性 + `postgres.conf` 的 `max_connections`），断言「文档 == 代码 == 运行」并在真实运行值上校验不变式；同轮修掉真实漂移 `postgres.conf` 200→250（见 §8.5） |
| **PERF-05** | 内存预算门禁基线**硬编码 2200MB**，且 `high` 风险不失败 | ~~内存回归可能不被拦截~~ | [`scripts/ci/check_memory_budget.py`](../../scripts/ci/check_memory_budget.py) | ✅ **已闭环（2026-10-06）**：基线参数（`8`/`4`/`20`）抽到 `scripts/ci/memory_budget_baseline`（缺失/缺项即 fail-closed），`high` 风险改为阻断（exit 1）（见 §8.5） |
| **PERF-06** | `docs/PERFORMANCE_BASELINE.md` 记连接池 `5/100`，与代码实际 `50` 不符；压测证据失败 | ~~性能基线不可信，误导容量规划~~ | [`docs/PERFORMANCE_BASELINE.md:55`](../../docs/PERFORMANCE_BASELINE.md#L55)、[`config`](../../synapse-common/src/config/) | ✅ **已闭环（2026-10-06）**：`PERFORMANCE_BASELINE.md` 升 v1.1，更正为 `5/50 (10%)`（分母取 `DatabaseConfig::max_size` 默认 50），并加注更正依据与部署不变式（见 §8.9） |

#### 兼容性与可访问性（6）

| 编号 | 问题 | 影响范围 | 证据 | 建议 |
|------|------|----------|------|------|
| **COMPAT-01** | capability 广告 `org.matrix.msc4155`=true，但该编号官方语义（邀请过滤）未实现 | ~~客户端据广告启用功能后行为不符~~ | [capability_governance.rs:243](../../synapse-services/src/capability_governance.rs#L243)、[:455](../../synapse-services/src/capability_governance.rs#L455) | ✅ **已闭环（2026-10-06）**：核实为**误报**——官方语义已实现（`m.invite_permission_config` 写入校验 + 邀请门禁），保留广告并订正 `MSC_SEMANTICS.md`（见 §8.6） |
| **COMPAT-02** | `.well-known/matrix/support` **硬编码** `https://matrix.org` | ~~部署实例返回错误的支持页链接~~ | [versions.rs:174-178](../../synapse-web/src/routes/handlers/versions.rs#L174-L178) | ✅ **已闭环（2026-10-06）**：改读配置项 `support_url`（`SYNAPSE__SERVER__SUPPORT_URL`），未配置返回 `{}`（见 §8.6） |
| **COMPAT-03** | `M_UNRECOGNIZED` 同码映射到 **400 与 501 双状态** | ~~客户端错误处理分支不一致~~ | [code.rs:172](../../synapse-common/src/error/code.rs#L172) vs [:198](../../synapse-common/src/error/code.rs#L198) | ✅ **已闭环（2026-10-06）**：`Unrecognized`/`Unimplemented` 统一归一为 **404**（规范无 501 errcode；405 由中间件发）（见 §8.6） |
| **COMPAT-04** | `M_UNSUPPORTED` 非规范 errcode；`presence.rs:37` 实际发 501，而 `code.http_status()`=405 | ~~与规范 errcode 集不符，状态不一致~~ | [code.rs](../../synapse-common/src/error/code.rs)、[presence.rs:37](../../synapse-web/src/routes/presence.rs#L37) | ✅ **已闭环（2026-10-06）**：删除非规范 `M_UNSUPPORTED` 全链（变体/构造函数/serde）；presence 改 `ApiError::forbidden`（**403**）（见 §8.6） |
| **COMPAT-05** | 迁移**硬依赖** `pgcrypto` / `pg_trgm` 扩展与 `gen_random_uuid()` | 目标 PG 未启用扩展时迁移失败 | [00000000_unified_schema_v12.sql:70-71](../../migrations/00000000_unified_schema_v12.sql#L70-L71)、[:709](../../migrations/00000000_unified_schema_v12.sql#L709) | 文档化前置扩展要求，或在迁移内 `CREATE EXTENSION IF NOT EXISTS` |
| **COMPAT-06** | CI 仅以 `--static` 校验 `.sqlx` 缓存，**不校验内容新鲜度** | ~~查询变更后缓存可能陈旧，`SQLX_OFFLINE` 构建用到旧元数据~~ | [`check_sqlx_cache_fresh.sh`](../../scripts/ci/check_sqlx_cache_fresh.sh) | ✅ **已闭环（2026-10-06）**：`--static` 新增「源码查询指纹 vs 缓存」新鲜度断言（见 §8.4） |

#### 代码质量与技术债务（5）

| 编号 | 问题 | 影响范围 | 证据 | 建议 |
|------|------|----------|------|------|
| **CQ-01** | clippy `allow` 逸散：外层 **164 处 / 120 文件**、内层 **94 处**；无 `clippy::all` blanket | 抑制掩盖潜在问题，随规模累积 | 全仓 lint 普查 | 建立「allow 白名单 + 理由必填」ratchet，禁止无理由 allow |
| **CQ-02** | 生产代码 `expect_used` 逃逸 ~20 处 | ~~panic 风险（虽多在不可达分支）~~ | [crypto.rs:276](../../synapse-common/src/crypto.rs#L276)、[loader.rs:199](../../synapse-common/src/config/loader.rs#L199)、[:204](../../synapse-common/src/config/loader.rs#L204) | ✅ **已闭环（2026-10-06）**：11 个文件的 `expect_used` 站点补 `reason = "…"` 证明性注释、删除 1 处冗余 allow；allow-ratchet 基线 176→**164** unjustified、108→**99** 文件，门禁 `OK`（见 §8.9） |
| **CQ-03** | `map_err(\|_e\| ...)` **吞掉原始错误** | ~~故障定位困难~~ | [membership/service.rs:770](../../synapse-services/src/room/membership/service.rs#L770)、[:779](../../synapse-services/src/room/membership/service.rs#L779) | ✅ **已闭环（2026-10-06）**：`get_room_members_paginated` 两处改为保留 cause（`Failed to check room existence: {e}` / `Failed to check membership: {e}`）（见 §8.9） |
| **CQ-04** | 超大文件：`synapse-storage/src/room/mod.rs` **2474 行**等；`.clippy.toml` `too-many-lines=500` 因 pedantic 未启用而**惰性** | 可维护性、审查成本 | [`synapse-storage/src/room/mod.rs`](../../synapse-storage/src/room/mod.rs)、[`.clippy.toml`](../../.clippy.toml) | 拆分模块；或在 CI 对超大文件设阈值门禁 |
| **CQ-05** | membership 以 `String` 存储 + 运行时 `from_str().ok()` **静默降级** | ~~非法值被吞为默认，隐藏数据错误~~ | membership 相关代码 | ✅ **已闭环（2026-10-06）**（最小方案：保留 `String` 存储、显式化错误路径）：`membership/service.rs` 的 `resolve_membership_from` 不可解析值 → `ApiError::internal`（fail-closed 暴露数据错误）；`state_map_auth.rs` 的 `membership_of` 不可解析值 → `tracing::warn!` 后返回 `None`（降级可见，保持既有 fail-closed 语义）（见 §8.9） |

#### 冗余代码与过时内容（4）

| 编号 | 问题 | 影响范围 | 证据 | 建议 |
|------|------|----------|------|------|
| **RED-01** | `event/create.rs` **5 处重复 `INSERT INTO events`** | 逻辑分散，改一处易漏 | [create.rs:28](../../synapse-storage/src/event/create.rs#L28)、[:105](../../synapse-storage/src/event/create.rs#L105)、[:233](../../synapse-storage/src/event/create.rs#L233)、[:307](../../synapse-storage/src/event/create.rs#L307)、[:383](../../synapse-storage/src/event/create.rs#L383) | 收敛为单一构造器/宏 |
| **RED-02** | `search_index` 遗留引用三处：lib.rs 注释、coverage_baseline 幽灵条目、summary 结论反了 | 幽灵条目导致覆盖率统计失真 | [lib.rs:260](../../synapse-storage/src/lib.rs#L260)、[coverage_baseline.json:1632](../../scripts/ci/coverage_baseline.json#L1632) | 清理注释与幽灵条目，修正 summary 结论 |
| **RED-03** | `coverage_baseline.json` 幽灵条目：`synapse-common/transaction.rs`、`synapse-storage/search_index.rs` | 覆盖率基线含不存在文件 | [coverage_baseline.json:356](../../scripts/ci/coverage_baseline.json#L356)、[:1632](../../scripts/ci/coverage_baseline.json#L1632) | 重新生成基线 |
| **RED-04** | `.worktrees/c19b/` 整份旧代码副本未被 `.gitignore`（仅忽略 `.claude/worktrees/`） | 误提交/检索噪声 | [`.gitignore:135`](../../.gitignore#L135) | 增加 `.worktrees/` 忽略规则并清理 |

#### 文档与知识库（3）

| 编号 | 问题 | 影响范围 | 证据 | 建议 |
|------|------|----------|------|------|
| **DOC-03** | 路由计数三处矛盾：`ROUTE_CONTRACT.md`=**1159**（2026-10-06 生成）vs `API_COVERAGE_REPORT.md`=**1153/1154**（2026-10-02） | ~~人工文档落后机器权威 6 条，契约可信度受损~~ | [ROUTE_CONTRACT.md:11](../synapse-rust/ROUTE_CONTRACT.md#L11)、[API_COVERAGE_REPORT.md:104](../synapse-rust/API_COVERAGE_REPORT.md#L104)、[:682](../synapse-rust/API_COVERAGE_REPORT.md#L682) | ✅ **已闭环（2026-10-06）**：重生成至同一 HEAD——**1159 / 921 / 813**（见 §8.6） |
| **DOC-04** | `docs/synapse-rust-vs-synapse-comparison.md` 多处 stale：正文仍记 `DEFAULT_ROOM_VERSION=11`（实际=12）；Content Scanner「0 调用点」已被证伪；`validate_id_token_claims` 已删 | ~~对照文档误导~~ | comparison.md :602/:689/:775/:800/:979、[:615](../synapse-rust-vs-synapse-comparison.md)、[:643](../synapse-rust-vs-synapse-comparison.md) | ✅ **已闭环（2026-10-06）**：三类 ground truth 就地订正（删除线保留历史）；守卫 `doc_credibility_guard_tests` 扩展陈旧声明检测，8/8 通过（见 §8.6） |
| **DOC-05** | `CHECKLIST.md` 校验结论过时：称「`cargo machete`: 0 unused」（实测 **10 个**）、「10 个 delta 迁移」（实际 **0**） | ~~验收清单给出假绿信号~~ | [CHECKLIST.md:45](../../CHECKLIST.md#L45)、[:55](../../CHECKLIST.md#L55)、[:79](../../CHECKLIST.md#L79) | ✅ **已闭环（2026-10-06）**：三处订正（delta 迁移 0、依赖部分注明曾失真并已入门禁）；machete 门禁见 `DEAD-01`（见 §8.6） |

#### 依赖卫生（并入 DEAD，单列）
| 编号 | 问题 | 影响范围 | 证据 | 建议 |
|------|------|----------|------|------|
| ~~**DEAD-01**~~ | ✅ **已闭环（2026-10-06）**：原报 **10 个未使用依赖**：根 crate `bytes, infer, ipnetwork, opentelemetry, sha1, thiserror, tracing-opentelemetry, vodozemac`；`synapse-web` `reqwest, thiserror`。均已逐一读码复核为真无用并删除，`cargo machete` 复验 **0 未使用**；同时将 `cargo machete` 接入 `.githooks/pre-push`（阻断级）。详细记录见 §8.3 | 依赖膨胀、供应链面扩大、编译时间增加 | [根 Cargo.toml](../../Cargo.toml)、[synapse-web/Cargo.toml](../../synapse-web/Cargo.toml)、[.githooks/pre-push](../../.githooks/pre-push)；[CHECKLIST.md:55](../../CHECKLIST.md#L55) 的依赖部分已由此变为准确（迁移计数部分仍属 `DOC-05`） | ✅ 已完成（删除 + 门禁） |

### 2.C P3 级问题（27）

| 编号 | 问题 | 域 | 证据 | 终态 |
|------|------|----|------|------|
| SEC-02 | 交叉签名上传缺验签（`cross_signing::upload_key_signature`/`upload_signatures`）—— **无生产调用点，死代码**（latent） | 安全 | [cross_signing/service.rs:138](../../synapse-e2ee/src/cross_signing/service.rs#L138)、[:314](../../synapse-e2ee/src/cross_signing/service.rs#L314) | ✅ 已闭环（2026-10-06，删死代码，见 §8.10） |
| SEC-03 | CSRF `session_id` 用 `!=` 非常量时间比较 | 安全 | [csrf.rs:49](../../synapse-web/src/middleware/csrf.rs#L49) | ✅ 已闭环（2026-10-06，改 `secure_compare`，见 §8.10） |
| SEC-04 | Token 撤销检查走 30s TTL 缓存（撤销生效存在窗口） | 安全 | [auth/mod.rs:46-49](../../synapse-services/src/auth/mod.rs#L46-L49) | ✅ 复核闭环（2026-10-06）：撤销写入主动失效覆盖窗口，见 §8.11 |
| SEC-05 | 登录失败锁定以 `user_id` 为键，**无 per-IP 节流** | 安全 | [login.rs:128-142](../../synapse-services/src/auth/login.rs#L128-L142) | ✅ 复核闭环（2026-10-06）：`LOGIN_MAX_IP_ATTEMPTS=30` 双桶，见 §8.11 |
| SEC-06 | `verify-security.sh` 硬编码测试密码 | 安全 | [verify-security.sh:9](../../scripts/test/verify-security.sh#L9) | ✅ 已闭环（2026-10-06，改 `${PRO_PASS:-…}`，见 §8.10） |
| FN-03 | `get_cross_signing_keys` 的 `self_signing_signature`/`user_signing_signature` 恒为空串 | 功能 | cross_signing/service.rs:120-121 | ✅ 已修复（2026-10-06）：改从 `signatures` 提取 + 4 单测，见 §8.11 |
| FN-04 | URL 预览硬编码桩 | 功能 | [media_service.rs:952-965](../../synapse-services/src/media_service.rs#L952-L965) | ✅ 已文档化（2026-10-06）：补 Stub 注释，行为不变，见 §8.11 |
| FN-05 | voice `/convert` `/optimize` `/transcription` 恒 501（by-design 桩） | 功能 | voice routes | 🟩 by-design 登记（2026-10-06，见 §8.11） |
| FN-06 | `verify_secure_backup_passphrase` 恒 410（by-design） | 功能 | auth routes | 🟩 by-design 登记（2026-10-06，见 §8.11） |
| FN-07 | 联邦 legacy keys 返回 `M_UNRECOGNIZED`（by-design） | 功能 | federation keys | 🟩 by-design 登记（2026-10-06，见 §8.11） |
| FN-08 | MSC 编号借用/形状漂移（MSC4502/4262/4429/4335/4354） | 功能 | `MSC_SEMANTICS.md` | 🟩 by-design 登记（2026-10-06，见 §8.11） |
| PERF-07 | `COALESCE(user_id, sender)` 非 sargable，破坏索引 | 性能 | [basic.rs:135-148](../../synapse-storage/src/event/basic.rs#L135-L148) | ✅ 复核闭环（2026-10-06）：谓词改 `sender = $1`，见 §8.11 |
| PERF-08 | `media_service` 同步 `std::fs::create_dir_all` 阻塞 async 运行时 | 性能 | [media_service.rs:120](../../synapse-services/src/media_service.rs#L120) | ✅ 已闭环（2026-10-06，改 async + `tokio::fs`，见 §8.10） |
| PERF-09 | `tests/performance/query_performance_tests.rs`、`api_load_tests.rs` 为**模拟无断言** | 性能 | 测试文件 | ✅ 复核闭环（2026-10-06）：补诚实头注释，见 §8.11 |
| COMPAT-07 | `.well-known` 其余字段/边界细节 | 兼容 | handlers/versions.rs | ✅ 复核闭环（2026-10-06）：逐端点复核无残留缺陷，见 §8.11 |
| COMPAT-08 | `check_feature_matrix.py` `all-extensions` **超时 300s** | 兼容/CI | [check_feature_matrix.py](../../scripts/ci/check_feature_matrix.py) | ✅ 已闭环（2026-10-06）：超时 300→900 + env 覆盖，见 §8.11 |
| COMPAT-09 | 部分 unstable MSC 前缀与稳定前缀并存未收敛 | 兼容 | routes | 🟩 by-design 登记（2026-10-06）：双前缀并存服务旧客户端，见 §8.11 |
| CQ-06 | `synapse-web/src/routes/voice.rs:1` 无理由 `#![allow(clippy::unused_async)]` | 质量 | [voice.rs:1](../../synapse-web/src/routes/voice.rs#L1) | ✅ 已闭环（2026-10-06，补理由，见 §8.10） |
| CQ-07 | `#[allow(dead_code)]` 约 19 处、`#[allow(unused…)]` 7 处 | 质量 | 全仓 | ✅ 已处理（2026-10-06）：8 处无理由站点三分类，见 §8.11 |
| CQ-08 | 迁移 `README.md`/`INDEXES.md` 与单一基线不完全同步 | 质量 | [migrations/README.md](../../migrations/README.md) | ✅ 已闭环（2026-10-06，计数口径订正，见 §8.10） |
| DEAD-02 | 根目录 `load-test-results/summary.json`（error_rate 73.41、messages_sent 0）与 `load-test-run{1,2,3}.ndjson` 残留 | 冗余 | `load-test-results/` | ✅ 已闭环（2026-10-06，见 §8.7） |
| DEAD-03 | `.worktrees/` 残留副本（同 RED-04） | 冗余 | `.worktrees/` | ✅ 已闭环（2026-10-06，见 §8.7） |
| DOC-06 | `project_rules.md:317` 链接 `src/services/container.rs` 不存在（实为 `synapse-services/src/container.rs`） | 文档 | [project_rules.md](../../.trae/rules/project_rules.md) | ✅ 已闭环（2026-10-06，见 §8.10） |
| DOC-07 | `migrations/README.md:363` 断链 | 文档 | [migrations/README.md:363](../../migrations/README.md#L363) | ✅ 已闭环（2026-10-06，见 §8.10） |
| DOC-08 | `migrations/README.md:362` 引用不存在的 `docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md`、`.scratch/db-schema-audit-2026-09-04.md` | 文档 | [migrations/README.md:362-363](../../migrations/README.md#L362-L363) | ✅ 已闭环（2026-10-06，见 §8.10） |
| DOC-09 | `docs/audit/` 重复报告（v12 系列 6 份、遗留问题 3 份、mutation 2 份） | 文档 | `docs/audit/` | ✅ 已闭环（2026-10-06，归档 5 份；v12 系列被生产代码引用故保留，见 §8.10） |
| DOC-10 | `docs/superpowers/plans/`（~35 文件）被 gitignore 但 `INDEX.md` 未说明 | 文档 | [`.gitignore:109-110`](../../.gitignore#L109-L110) | ✅ 已闭环（2026-10-06，见 §8.10） |
| DOC-11 | 文档防漂移门禁覆盖过窄：`doc_credibility_guard_tests.rs` 仅守 comparison.md，且只解析反引号、不解析 markdown 链接；`docs-quality-gate.yml` 不含 `migrations/`、`.trae/rules/` | 文档 | [doc_credibility_guard_tests.rs:26/88-108](../../tests/unit/doc_credibility_guard_tests.rs#L26)、[docs-quality-gate.yml:36-37](../../.github/workflows/docs-quality-gate.yml#L36-L37) | ✅ 已闭环（2026-10-06，见 §8.8） |

---

## 三、风险分析

### 3.1 优先级 × 影响矩阵

| | 高影响 | 中影响 | 低影响 |
|---|--------|--------|--------|
| **高概率** | — | `~~FED-01~~`（已闭环） | `~~DOC-01~~`/`~~DOC-02~~`/`~~DOC-03~~`（文档漂移；均已闭环） |
| **中概率** | — | `~~FED-02~~`、`~~PERF-02~~`（已闭环）、`COMPAT-05` | `~~CQ-01~~`、`~~DEAD-01~~`、`~~RED-01…04~~`（均已闭环） |
| **低概率** | — | `~~SEC-01~~`（已闭环）、`~~COMPAT-03/04~~`（已闭环） | `P3` 全体（加固/决策） |

### 3.2 共性根因

1. **「文档滞后于代码」是系统性模式**：路由计数、迁移计数、房间版本、内容扫描器、依赖卫生——多域同源。根因是**人工文档无强制同步机制**，且防漂移门禁覆盖面过窄（`DOC-11`）。
2. **门禁盲区（持续收尾）**：`cargo machete` 未接入（`DEAD-01`）**已于 2026-10-06 闭环**；`.sqlx` 新鲜度未校验（`COMPAT-06`）**已补断言（2026-10-06 闭环，见 §8.4）**；预算门禁读文档而非运行配置（`PERF-04`/`PERF-05`）**已闭环（2026-10-06，见 §8.5）**；feature matrix 超时（`COMPAT-08`）仍待处理。这些使部分「绿」不可信。
3. **协议面「已登记未闭环」在本轮已收尾**：`FED-01` 曾被权威清单登记但跨版本未修复，反映「登记 ≠ 跟踪到底」；本轮已修复（见 §8.1）。后续须避免同类长期挂起，确保登记项有明确的闭环责任人。

---

## 四、本次同步的产物

| 文件 | 变更 |
|------|------|
| [`COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md`](COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md) | 本报告（新建，取代 20261004 版为当前权威） |
| [`UNRESOLVED_ISSUES_SUMMARY.md`](UNRESOLVED_ISSUES_SUMMARY.md) | 刷新：移除/更正已解决与过时条目、修正破路径、更新日期与基准 |
| [`INDEX.md`](INDEX.md) | 新增本报告索引；更新「当前待处理问题」与更新日志 |

---

## 五、复核纠正（证伪 / 已修复）

> 以下条目经本轮**直接读码复核**，确认已不成立，须从权威清单移除或降级；避免误报与重复排查。

### 5.1 上一版 RBAC P0/P1 已修复（3）

| 原编号 | 结论 | 复核证据 |
|--------|------|----------|
| P0-1 `login_as_user` 越权冒充超管 | ✅ **已修复** | [user.rs:597-599](../../synapse-web/src/routes/admin/user.rs#L597-L599) 新增 `if is_admin { ensure_super_admin_for_privilege_change(&admin)?; }`（注释明确引用 P0-1） |
| P1-1 `/devices/delete` 字符串匹配错位 | ✅ **已修复** | [admin_auth.rs:217](../../synapse-web/src/utils/admin_auth.rs#L217) 改为 `path.ends_with("/devices/delete") \|\| (path.contains("/devices/") && path.ends_with("/delete"))`；单测改用真实路径（[:533](../../synapse-web/src/utils/admin_auth.rs#L533)、[:614](../../synapse-web/src/utils/admin_auth.rs#L614)） |
| P1-2 客户端审计 result 恒 success | ✅ **已修复** | 全仓已无 `audit_user_action` 定义/调用（原硬编码 `result: "success"` 路径已移除） |

### 5.2 权威清单「仍存」实为已解决（5）

| 原条目 | 结论 | 复核证据 |
|--------|------|----------|
| 事务去重标记位置（1.2） | ✅ **已修复** | [messages.rs:52-55](../../synapse-services/src/room/messaging/messages.rs#L52-L55)：dedup marker 与事件**同事务**写入（A4）；原引用路径 `synapse-web/src/routes/room/messaging/messages.rs` 整个目录已不存在 |
| Profile `/{keyName}` 未注册（3.2） | ✅ **已修复** | [assembly.rs:239](../../synapse-web/src/routes/assembly.rs#L239) 注册 `/_matrix/client/v3/profile/{user_id}/{key_name}`（+ [:232](../../synapse-web/src/routes/assembly.rs#L232) unstable msc4133） |
| Admin 媒体端点族不完整（3.3） | ✅ **已对齐** | [admin/media.rs:40-69](../../synapse-web/src/routes/admin/media.rs#L40-L69) |
| ledger `query_params` 无消费方（3.4） | ✅ **已有守卫** | [route_ledger.rs:53-58](../../synapse-web/src/routes/route_ledger.rs#L53-L58) |
| v12/v13 房间不可创建（4.1） | ✅ **已更新** | [room_versions.rs:94](../../synapse-common/src/room_versions.rs#L94) `DEFAULT_ROOM_VERSION="12"`；v12 可创建、v13 已彻底移除 |

### 5.3 其余证伪（1）

| 原结论 | 结论 | 复核证据 |
|--------|------|----------|
| 交叉签名上传无验签 = P1 可利用漏洞 | ⚠️ **降级为死代码（P3）** | `/keys/signatures/upload` 经 [keys.rs:30](../../synapse-web/src/routes/e2ee/keys.rs#L30) → [devices.rs:199](../../synapse-web/src/routes/e2ee/devices.rs#L199) → `device_keys_service.upload_signatures`（[device_keys/service.rs:651](../../synapse-e2ee/src/device_keys/service.rs#L651)，**有 ownership 校验**）；`cross_signing` 同名方法无任何调用点 |

---

## 六、建议行动计划

### 6.1 立即（P1）——✅ 已于 2026-10-06 全部闭环
1. ✅ **FED-01**：补 `/send_join` v1/v2 的 `event` 字段 + 联邦集成测试 → 闭环权威清单 `P0-1`（见 §8.1）。
2. ✅ **DOC-01/DOC-02**：重写 `.trae/rules/project_rules.md` §1.3/§7.0；刷新 `UNRESOLVED_ISSUES_SUMMARY.md`（本报告已同步）（见 §8.2）。

### 6.2 短期（P2）
3. **门禁补强**：✅ `cargo machete` 入 pre-push（`DEAD-01`，已闭环见 §8.3）；✅ `.sqlx` 新鲜度断言（`COMPAT-06`，已闭环见 §8.4）；✅ 预算门禁读真实配置（`PERF-04/05`，已闭环见 §8.5）。
4. **清理依赖**：✅ 已处置 10 个 machete 未使用依赖（全部删除，`DEAD-01` 已闭环见 §8.3）。
5. ✅ **协议/兼容**：`M_UNRECOGNIZED`/`M_UNSUPPORTED` 状态归一（`COMPAT-03/04`）；MSC4155 广告校正（`COMPAT-01`，核实为误报→订正语义表）；support URL 配置化（`COMPAT-02`）。全部于 2026-10-06 闭环（见 §8.6）。
6. ✅ **性能**：为 `get_daily_message_count` 建复合索引（`PERF-02`）；无界查询加 LIMIT（`PERF-03`）。均于 2026-10-06 闭环（见 §8.6）。
7. ✅ **文档**：重生成 `API_COVERAGE_REPORT.md`（`DOC-03`）；刷新 `CHECKLIST.md`（`DOC-05`）与 comparison（`DOC-04`，含守卫扩展）。均于 2026-10-06 闭环（见 §8.6）。

### 6.3 中期（P3 / 决策）
8. ~~建立「allow 白名单 + 理由」ratchet（`CQ-01`）~~ → ✅ **已闭环（2026-10-06）**：新建 `check_clippy_allow_ratchet.py` + allowlist（13 条带理由）+ 逐文件 grandfather 基线，见 §8.8；~~拆分超大文件（`CQ-04`）~~ → ✅ **已闭环（2026-10-06）**：改走 **CI 阈值门禁**（`scripts/ci/check_file_size_ratchet.py`，500 行 + 257 条 grandfather 基线），未拆分文件，见 §8.7。
9. ~~收敛重复 INSERT（`RED-01`）~~ → ✅ **已闭环（2026-10-06）**：`create_event_with_pdu`/`create_outlier_event` 的逐字节重复 INSERT 抽为单一 helper（5 → 4 处），见 §8.8；~~清理幽灵覆盖率条目与 `.worktrees/`（`RED-02/03/04`、`DEAD-02/03`）~~ → ✅ **已闭环（2026-10-06）**：`RED-02`（lib.rs 注释 + 幽灵条目 + `trigram-audit.md` 失效结论）、`RED-03`（覆盖率基线 8 条幽灵条目清零）、`RED-04`（`.gitignore` 忽略 `.worktrees/` 并清理）、`DEAD-02`（删除 `load-test-results/` 与根 `load-test-run{1,2,3}.ndjson`）、`DEAD-03`（删除 `.worktrees/`），见 §8.7。
10. ~~扩大文档防漂移门禁覆盖面至 `migrations/`、`.trae/rules/`、markdown 链接（`DOC-11`）~~ → ✅ **已闭环（2026-10-06）**：守卫新增 markdown 链接形态解析、`docs-quality-gate.yml` 纳入 `migrations/*.md`、`.trae/rules/` 改由本地守卫覆盖（CI 缺席优雅跳过，残余风险已在 yml 注释写明），见 §8.8。

---

## 七、附录

### 7.1 复现命令

```bash
# 编译/静态门禁
SQLX_OFFLINE=true cargo clippy --workspace --all-targets --features test-utils --all-features --locked -- -D warnings
cargo deny check advisories
cargo machete
python3 scripts/ci/check_trait_ratchet.py
python3 scripts/ci/check_web_layering.py
python3 scripts/ci/check_connection_budget.py
python3 scripts/ci/check_axum_path_syntax.py
bash scripts/ci/check_rand_rng_ratchet.sh
python3 scripts/ci/check_ts_order_tiebreak.py
python3 scripts/ci/run_cargo_geiger.py        # CI 门禁；本地全量运行可能超时
```

### 7.2 环境限制说明
- `cargo audit` 本地无法运行：`~/.cargo/advisory-db` 目录状态导致 `git clone` 被拒（`Refusing to initialize the non-empty directory`）。**advisories 由 pre-push 门禁 `cargo deny check advisories` 覆盖，结果 OK**。
- `cargo geiger` 本地全量运行超时（workspace 全量重编译）；以 CI 门禁与 `geiger_baseline.json`（`prod_unsafe_total=0`，硬阻断）为准。
- `check_feature_matrix.py` 的 `all-extensions` 组合 `cargo check` 超时（>300s），本身登记为 `COMPAT-08`。

### 7.3 报告关系
- 本报告取代 [`COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261004.md`](archive/COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261004.md)（已归档）为**当前权威**全面报告。
- 安全/权限专项见 [`PERMISSION_RBAC_AUDIT_2026-10-05.md`](PERMISSION_RBAC_AUDIT_2026-10-05.md)（其 P0/P1 已在本轮复核为已修复）。
- 系统测试见 [`FULL_SYSTEM_TEST_REPORT_2026-10-05.md`](FULL_SYSTEM_TEST_REPORT_2026-10-05.md)。

---

## 八、闭环记录（2026-10-06）

> 含 P1（§8.1–8.2）、P2 首项（§8.3，`DEAD-01`）、P2 门禁补强项（§8.4 `COMPAT-06`、§8.5 `PERF-04`/`PERF-05`）、P2 协议/兼容/性能/文档批次（§8.6 `COMPAT-01/02/03/04`、`PERF-02/03`、`DOC-03/04/05`）、P3 卫生批次（§8.7 `CQ-04`、`RED-02/03/04`、`DEAD-02/03`）与 P3 收尾批次（§8.8 `CQ-01`、`RED-01`、`DOC-11`）。
> 至此 §6.2「短期（P2）」第 3–7 项全部闭环；§6.3「中期（P3 / 决策）」第 8/9/10 项**全部闭环**（`CQ-04`/`RED-02/03/04`/`DEAD-02/03` 见 §8.7，`CQ-01`/`RED-01`/`DOC-11` 见 §8.8）。

### 8.1 FED-01 `/send_join` 补齐 `event` 字段

| 项 | 内容 |
|----|------|
| **修复文件** | [`join.rs`](../../synapse-web/src/routes/federation/membership/join.rs)（v1 `send_join` + v2 `send_join_v2`） |
| **核心改动** | 用 [`project_and_sign_pdu_locally`](../../synapse-web/src/routes/federation/membership/mod.rs#L230)（返回签名后 PDU）替换丢弃结果的 `re_sign_pdu_locally`；v1/v2 响应对象均追加 `"event": signed_pdu` |
| **硬失败约定** | 签名缺失即 `ApiError::internal`，与 invite 端点既有约定一致（不返回无法校验的响应） |
| **测试** | [`federation_existence_leak_tests.rs`](../../tests/integration/federation_existence_leak_tests.rs) 断言 `event` 存在、为 join `m.room.member`、带非空 `hashes`/`signatures` |
| **门禁验证** | `cargo clippy -p synapse-web --all-targets --all-features --locked -- -D warnings`（exit 0）、`cargo clippy --test integration --all-features --locked -- -D warnings`（exit 0） |
| **关联闭环** | 权威清单 `P0-1`（`UNRESOLVED_ISSUES_SUMMARY.md`） |

### 8.2 DOC-01 `project_rules.md` 目录结构/迁移计数修正

| 项 | 内容 |
|----|------|
| **修复文件** | [`.trae/rules/project_rules.md`](../../.trae/rules/project_rules.md) |
| **核心改动** | §1.3 按真实 workspace 布局重写（8 个成员 crate + 根薄装配层；`migrations/` 注明「1 个统一基线 SQL」）；§7.0 链接修正为 [`synapse-services/src/container.rs`](../../synapse-services/src/container.rs) |
| **版本** | v2.4.0 → **v2.5.0**（2026-10-06），版本历史追加对应行 |
| **关联** | `DOC-02`（`UNRESOLVED_ISSUES_SUMMARY.md` 刷新）同轮完成；防漂移门禁见 `DOC-11`/`DOC-24`（P2/P3，待做） |

### 8.3 DEAD-01 清理未使用依赖 + `cargo machete` 入 pre-push

| 项 | 内容 |
|----|------|
| **清理文件** | 根 [`Cargo.toml`](../../Cargo.toml)（删 8 个）、[`synapse-web/Cargo.toml`](../../synapse-web/Cargo.toml)（删 2 个） |
| **根 crate 删除** | `ipnetwork`、`sha1`、`vodozemac`、`tracing-opentelemetry`、`opentelemetry`、`thiserror`、`bytes`、`infer`（保留 `opentelemetry_sdk`，[`telemetry.rs`](../../src/server/telemetry.rs) 在用） |
| **synapse-web 删除** | `thiserror`（源码零引用）、`reqwest`（仅注释提及） |
| **复核方式** | 逐一读码 / `grep` 确认无 `use`/路径引用（排除宏/derive 误报）；`cargo machete` 复验输出「didn't find any unused dependencies」 |
| **门禁** | [`.githooks/pre-push`](../../.githooks/pre-push) 新增 **Stage 3（阻断）**：`cargo machete` 非零即拦下；未安装时打印 `tip`（与 Stage 2 cargo-deny 风格一致） |
| **验证** | `SQLX_OFFLINE=true cargo check --workspace --all-features`（exit 0）；`Cargo.lock` 已随之刷新 |

### 8.4 COMPAT-06 `.sqlx` 缓存内容新鲜度断言

| 项 | 内容 |
|----|------|
| **缺口** | CI 只以 `--static` 校验 `.sqlx/` 的**存在性**（存在/非空/被 git 跟踪），**不校验内容是否与源码查询一致**；改为查询文本后若忘记重生成缓存，`SQLX_OFFLINE` 构建会用到旧元数据（或在 CI 上以 `no cached data` 失败）。 |
| **修复文件** | [`sqlx_query_census.py`](../../scripts/ci/sqlx_query_census.py)（新增 `check_cache_fresh()` + `--check-cache-fresh` CLI）、[`check_sqlx_cache_fresh.sh`](../../scripts/ci/check_sqlx_cache_fresh.sh)（`--static` 块接入） |
| **原理** | `.sqlx/query-<hash>.json` 的 `<hash>` = `sha256(query 字段文本)`，而 `query` 字段**就是源码里 sqlx 静态宏的 SQL 字面量本身**（1:1、无归一化）。因此可**纯静态**把「源码字面量」映射为「应有的缓存文件名」，无需 DB、无需编译。 |
| **复用** | 直接复用 census 现有的词法扫描器（`strip_code`/`_find_open_paren`/`_first_arg_offset`/…），不新增第二份 SQL 扫描实现（铁律 2：「护栏只有一份实现」）。 |
| **覆盖** | 主树 `SCAN_DIRS` 内 **1360 处**静态查询字面量全量核对（`tests/**` 0 处；`query_file!` 本仓零使用）。非常量实参（`concat!`/`format!`/变量）静态不可求值，**一律跳过、绝不误报**；扫描到 0 处字面量时**失败关闭**（视为扫描器失效）。 |
| **模式门控** | 断言**仅在 `--static` 运行**：`--full` 已由 `sqlx_prepare.sh --check` 对真库做权威内容核对，`--compile` 已由离线构建证明条目完整（CI 的缺口恰恰是「只跑 `--static`」）。 |
| **验证** | `bash scripts/ci/check_sqlx_cache_fresh.sh --static` → `OK: 源码 1360 处静态查询字面量全部命中 .sqlx/ 缓存`（exit 0）；空 `.sqlx/` 负向 → exit 1 并列出全部缺失点；`cargo test --test unit --features test-utils sqlx_cache_tooling_guard` **17/17 通过**（含 `cache_fresh_full_delegates_to_the_single_sanctioned_entry_point` 不回归）。 |
| **关联** | 同族缺口 `COMPAT-08`（feature matrix 超时）仍待处理；`.sqlx` 写入唯一入口见 [`sqlx_prepare.sh`](../../scripts/ci/sqlx_prepare.sh)。 |

### 8.5 PERF-04 / PERF-05 预算门禁读真实配置

| 项 | 内容 |
|----|------|
| **PERF-04 缺口** | 连接预算门禁只解析 [`docker/deploy/README.md`](../../docker/deploy/README.md) 的**成文预算表**，从不读取运行配置。**真实漂移已存在**：README 写 `PostgreSQL max_connections = 250`，而 canonical 运行配置 [`postgres.conf`](../../docker/config/postgres.conf) 实为 `200` → 真实不变式 `50×4+20 = 220 > 200` 已超预算，门禁却仍绿（假绿）。 |
| **PERF-04 修复文件** | [`scripts/ci/check_connection_budget.py`](../../scripts/ci/check_connection_budget.py)（全文件重写）、[`docker/config/postgres.conf`](../../docker/config/postgres.conf)（`max_connections` 200 → 250，含不变式理由注释）。 |
| **PERF-04 核心改动** | 新增 `code_pool_per_process()`（读 [`database.rs`](../../synapse-common/src/config/database.rs) 的 `default_database_max_size()`）、`yaml_pool_per_process()`（只在顶层 `database:` 块匹配，避开 `redis.pool_size`）、`compose_pool_per_process()`（读 `SYNAPSE__DATABASE__MAX_SIZE` 默认值）、`runtime_max_connections()`（读 `postgres.conf`）；`main()` 做 **4 项断言**：① 三源池上限一致（代码默认 / 运行 yaml / compose 默认）；② 文档表 pool == 代码；③ **文档表 `max_connections` == `postgres.conf` 运行值**（堵假绿）；④ 在**真实运行值**上校验 `pool × 进程 + 保留 ≤ max_connections`。任一数字不可解析 → `exit 2`（fail-closed，不静默通过）。 |
| **PERF-05 缺口** | 内存门禁基线 `2200 = 8×200 + 4×(50+20×5)` **硬编码**在 `estimate_current_baseline()` 里，无从审计、无法随测试架构演进更新；且 `high` 风险只打印警告却 `exit 0`，内存回归被放过。 |
| **PERF-05 修复文件** | 新建 [`scripts/ci/memory_budget_baseline`](../../scripts/ci/memory_budget_baseline)、[`scripts/ci/check_memory_budget.py`](../../scripts/ci/check_memory_budget.py)。 |
| **PERF-05 核心改动** | 基线文件承载测试架构参数 `NUM_TEST_BINS=8`/`NUM_POOLS=4`/`POOL_SIZE=20`，并写明模型与「抬升基线须在此写理由」的约定（沿用仓库既有「基线棘轮」惯用法：`read_baseline()` + `--update`）。脚本新增 `read_baseline_inputs()`——文件缺失/缺项即 `exit 2`（**不允许静默回落到硬编码**）；`estimate_current_baseline()` 改为读文件按模型推导；`main()` 中 `high` 分支由 `sys.exit(0)` 改为 **`sys.exit(1)`**。 |
| **验证（PERF-04）** | 正向 `exit 0`：`✅ 连接预算成立：220 ≤ 250（余量 30）；文档 == 代码 == 运行配置`。负向 1（`postgres.conf` 临时回 200）→ `exit 1`「文档与运行配置漂移：README 写 250，而 postgres.conf 实际为 200」（**证明原假绿缺口已被堵**）；负向 2（`homeserver.yaml` 临时改 60）→ `exit 1`「池上限来源漂移…」。 |
| **验证（PERF-05）** | 正向基线仍 `2200`、风险 `LOW`、`exit 0`（**main 不因本次改动变红**）。负向 1（`MEMORY_BUDGET_MB=3000 MEMORY_BUDGET_WARN_MB=2500` → `HIGH`）→ `exit 1`（原为 `exit 0`）；负向 2（临时移除基线文件）→ `exit 2`「缺少内存基线文件…」。两脚本 `py_compile` 均通过。 |
| **登记** | [`CLAUDE.md`](../../CLAUDE.md) §audit workspace convention 的「Baseline files」清单补列 `scripts/ci/memory_budget_baseline`。 |
| **关联** | 与 §8.4（`COMPAT-06`）、§8.3（`DEAD-01`）同属 §6.2「门禁补强」批次；`docker/deploy/README.md` 预算表未改，仅作为被断言的一方。 |

### 8.6 P2 协议/兼容/性能/文档批次（`COMPAT-01/02/03/04`、`PERF-02/03`、`DOC-03/04/05`）

> 本轮收尾 §6.2 第 5/6/7 项（「协议/兼容」「性能」「文档」）。逐项登记改动、证据与验证。

| 项 | 内容 |
|----|------|
| **COMPAT-01（核实为误报 → 订正语义表，保留广告）** | 原判「`org.matrix.msc4155`=true，但其官方语义（邀请过滤）未实现」。核实：官方语义**已实现**——`m.invite_permission_config` account data 有写入校验（[`account_data_service.rs:246`](../../synapse-services/src/account_data_service.rs#L246)）与邀请门禁消费（[`invite_blocklist_service.rs:13`](../../synapse-services/src/invite_blocklist_service.rs#L13)、[`membership/service.rs:86`](../../synapse-services/src/room/membership/service.rs#L86)）。故**保留能力广告**，改订 [`MSC_SEMANTICS.md`](../../docs/synapse-rust/MSC_SEMANTICS.md) §1 MSC4155 行（原「官方未实现」判废）与 §2 孤儿清单（`get/setInvitePermissionConfig` 移出）。 |
| **COMPAT-02（support URL 配置化）** | [`config/server.rs`](../../synapse-common/src/config/server.rs#L155) 新增 `support_url: Option<String>`（可经 `SYNAPSE__SERVER__SUPPORT_URL` 覆盖）；[`versions.rs`](../../synapse-web/src/routes/handlers/versions.rs#L173) `get_well_known_support` 由硬编码 `https://matrix.org` 改为读配置：已配置返回 `{"url": …}`，未配置返回 `{}`（不再伪造外部链接）。新增 2 个单测（配置/未配置）。 |
| **COMPAT-03（`M_UNRECOGNIZED` 状态归一）** | [`error/code.rs`](../../synapse-common/src/error/code.rs#L166) 把 `Unrecognized` 由 400 改为 **404**、`Unimplemented` 由 501 改为 **404**；[`error.rs`](../../synapse-common/src/error.rs#L269) 的 `not_implemented`/`unrecognized` 构造函数同步归一为 404。依据：Matrix 规范无 501 errcode，`M_UNRECOGNIZED` 规范状态为 404/405（405 由 method-not-allowed 中间件直接发出）。`is_not_implemented()` 改为按 errcode 判定以保留内部区分。测试断言同步（`M_UNRECOGNIZED → NOT_FOUND`）。 |
| **COMPAT-04（移除非规范 errcode `M_UNSUPPORTED`）** | `M_UNSUPPORTED` 不属规范 errcode 集，整体删除：`MatrixErrorCode::Unsupported` 变体、`ApiError::unsupported()` 构造函数、serde 反序列化分支及相关测试全部移除（[`error/code.rs`](../../synapse-common/src/error/code.rs)、[`error.rs`](../../synapse-common/src/error.rs)）。唯一调用点 presence 改为 [`ApiError::forbidden`](../../synapse-web/src/routes/handlers/presence.rs#L29)（**403 `M_FORBIDDEN`**）：请求可理解但功能被管理端禁用；403 便于客户端缓存决定，而非当作瞬态故障重试。 |
| **PERF-02（复合索引）** | [`00000000_unified_schema_v12.sql`](../../migrations/00000000_unified_schema_v12.sql#L3264) 新增 `CREATE INDEX IF NOT EXISTS idx_events_type_origin_ts ON events(event_type, origin_server_ts DESC)`，覆盖 `get_daily_message_count` 的「`event_type` 等值 + `origin_server_ts` 范围」双谓词（原单列 `idx_events_type` 需回表过滤时间谓词）。[`INDEXES.md`](../../migrations/INDEXES.md) 索引计数同步 350/140 → **351/141**。 |
| **PERF-03（无界查询加 LIMIT）** | [`token.rs:171`](../../synapse-storage/src/token.rs#L171) `get_user_tokens` 加 `ORDER BY id ASC LIMIT $2`（`MAX_USER_TOKENS=10_000`）；[`background_update.rs:394`](../../synapse-storage/src/background_update.rs#L394) `get_updates_by_status` 加 `LIMIT $2`（`MAX_UPDATES_BY_STATUS=10_000`）。两者均为**防无界**的宽松上限（正常负载远小于此），命中即 `tracing::warn!`。同批：[`basic.rs:176`](../../synapse-storage/src/event/basic.rs#L176) 的 `#[cfg(test)]` 专用 `get_total_message_count` 改用动态 `query_scalar::<_, i64>`（静态宏会被 `--all-targets` 编译却不被 `cargo sqlx prepare` 生成 → 悬空缓存条目），消除 `.sqlx` 悬空隐患。 |
| **DOC-03（覆盖率报告重生成）** | [`API_COVERAGE_REPORT.md`](../../docs/synapse-rust/API_COVERAGE_REPORT.md) 三口径重算至与 [`ROUTE_CONTRACT.md`](../../docs/synapse-rust/ROUTE_CONTRACT.md#L11)（2026-10-06 生成）同一 HEAD：注册条目 **1159**、唯一路径 **921**、逻辑端点 **813**（v1.10 旧值 1153/915/807 作废），消除「人工文档落后机器权威 6 条」矛盾；收尾横扫时同步清除 [`synapse-rust-vs-synapse-comparison.md` §3.4](../../docs/synapse-rust-vs-synapse-comparison.md#L234) 正文仍在引用的同一组作废值（1,153/915/807 → **1,159/921/813**）与 `ROUTE_CONTRACT.md` 生成日期（2026-10-01 → 2026-10-06）。 |
| **DOC-04（comparison 刷新 + 守卫扩展）** | [`synapse-rust-vs-synapse-comparison.md`](../../docs/synapse-rust-vs-synapse-comparison.md) 就地订正三类 ground truth（默认房间版本 **12**、Content Scanner **已接线**、`validate_id_token_claims` **已删**），以 `~~旧文~~ → **订正（2026-10-06，DOC-04）**：…` 保留历史可追溯。守卫 [`doc_credibility_guard_tests.rs`](../../tests/unit/doc_credibility_guard_tests.rs) 新增 `stale_claim_violations`（先剥离 `~~…~~` 删除线段落后再判陈旧声明：默认版本≠源码实值、已删符号未标记删除、`0 调用点`/`无消费者` 证伪短语）+ 红证明测试；**8/8 通过**。 |
| **DOC-05（CHECKLIST 刷新）** | [`CHECKLIST.md`](../../CHECKLIST.md) 三处订正：:45「10 个 delta 迁移」→ 实为 **0**（已折入统一基线）；:55/:79 的依赖结论注明「曾失真（实测 10 个未使用）→ 2026-10-06 已删除并接入 `cargo machete` pre-push 阻断门禁，复验 **0 unused**」。 |
| **验证** | 相关守卫单测全绿（`doc_credibility_guard_tests` **8/8**、`sqlx_cache_tooling_guard` **17/17**）；`cargo machete` **0 unused**；`check_connection_budget.py` / `check_memory_budget.py` 正向 `exit 0`。 |

> **订正报告 §8.3（line 343）关于 `reqwest` 的表述**：原文记 `synapse-web` 的 `reqwest`「仅注释提及」，措辞易被误读为「synapse-web 无任何 reqwest 代码路径」。准确口径：`synapse-web/src` 内确无 `use reqwest` / 路径引用（仅 [`app_service.rs:376`](../../synapse-web/src/routes/app_service.rs#L376)、[:440](../../synapse-web/src/routes/app_service.rs#L440) 两处文档注释），故**删除 `synapse-web` 的直接依赖声明是正确的**；但 reqwest 仍是运行期 HTTP 客户端——经 [`synapse_common::http_client::default_client() -> reqwest::Client`](../../synapse-common/src/http_client.rs#L44) 间接使用（[`synapse-common/Cargo.toml:82`](../../synapse-common/Cargo.toml#L82) 为其直接依赖）。即应表述为「**不再需要直接声明依赖**」，而非「无 reqwest 代码路径」。

### 8.7 P3 卫生批次（`CQ-04`、`RED-02/03/04`、`DEAD-02/03`）

> 本轮收尾 §6.3 第 8/9 项中已确认的「低风险卫生类」子集。该批明确**不含** `CQ-01`、`RED-01`、`DOC-11`——三者随后在同日 §8.8 闭环。

| 项 | 内容 |
|----|------|
| **CQ-04（决策：CI 阈值门禁，不拆分文件）** | `.clippy.toml` 的 `too-many-lines-threshold = 500` **只被 `clippy::pedantic` 消费，而本 workspace 从不启用 pedantic** → 「currently enforce nothing」。本轮不改用拆分（成本高、风险大），改以**门禁让 500 行意图落地**：新建 [`check_file_size_ratchet.py`](../../scripts/ci/check_file_size_ratchet.py) + 基线 [`file_size_baseline`](../../scripts/ci/file_size_baseline)（沿用 `check_trait_ratchet.py` 的 9 个源根 / `EXCLUDED` / `read_baseline()` / `--update` 范式）。阈值 500；`*.inc.rs`（机器生成 ledger 表）排除。**三类失败**：① 新增超阈值文件且不在基线；② 基线文件超出其记录上限（只降不升）；③ 基线条目陈旧（文件已删或已 ≤ 阈值 → 强制收紧）。fail-closed：扫描到 0 文件 / 基线缺失均 `exit 2`。基线下 **257 条** grandfather 条目（扫描 772 文件）。接入 [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) `repo-sanity` job（"File-size ratchet"，紧随 Trait-count ratchet）；[`.clippy.toml`](../../.clippy.toml) NOTE 更新为「该阈值现已由门禁落实」。 |
| **RED-02（`search_index` 三处遗留引用）** | ① [`synapse-storage/src/lib.rs:261`](../../synapse-storage/src/lib.rs#L261) 注释由 `sync (sliding_sync, filter, presence, search_index)` 改为 `sync domain group (sliding_sync, filter, presence)`（`grep search_index` 零命中）；② `coverage_baseline.json` 幽灵条目（见 RED-03）；③ **修正 summary 失效结论**——[`docs/trigram-audit.md`](../../docs/trigram-audit.md) 的 `## Summary`/§2a/Recommendations 第 1 条称 `search_index.rs` 接入 `TrigramRanking`「Already adopted / [DONE]」，而该模块已于 2026-09-24 W4/D-27 按铁律 1 整模块删除 → 结论与现状相反。就地以 `~~…~~ → **已作废（2026-10-06 订正）**` 保留历史并订正（文首加 `⚠️` 全局说明，其余各节结论仍有效）。 |
| **RED-03（覆盖率幽灵条目清零）** | [`coverage_baseline.json`](../../scripts/ci/coverage_baseline.json) 移除 8 条指向**不存在文件**的条目：`scripts/bench_harness.rs`、`synapse-common/transaction.rs`、`synapse-e2ee/signature/{models,service,storage}.rs`、`synapse-federation/state_resolution.rs`、`synapse-services/room/api_trait.rs`、`synapse-storage/search_index.rs`。复核：**609 条 / 0 幽灵**。 |
| **RED-04（`.worktrees/` 未忽略）** | [`.gitignore`](../../.gitignore#L136) 新增 `.worktrees/`（原仅忽略 `.claude/worktrees/`）。 |
| **DEAD-02（负载测试产物残留）** | 删除根 `load-test-results/`（含 `summary.json`/`summary-minimal.json`）与根 `load-test-run{1,2,3}.ndjson`（`git rm`）。**副作用修补**：产物被 [`run-load-test.sh`](../../run-load-test.sh#L14) 与 [`matrix-load-test.js`](../../scripts/load-test/matrix-load-test.js#L113) 引用却无父目录 → `.gitignore` 新增 `load-test-results/`、`load-test-run*.ndjson`，`run-load-test.sh` 在 `k6` 前加 `mkdir -p load-test-results`。 |
| **DEAD-03（`.worktrees/` 残留目录）** | `rm -rf .worktrees/`（旧代码副本，已不存在）。 |
| **验证** | `check_file_size_ratchet.py` 正向 `exit 0`（`772 files scanned, 257 over 500 (baseline 257)`）；红证明三类均 `exit 1`（新建 600 行探针 → 新增违规；基线上限降到 2400 → `grew past their ceiling`；注入陈旧条目 → `stale baseline entr(y\|ies)`），恢复后复验 `exit 0`；探针文件与基线备份已清理。`coverage_baseline.json` 幽灵复核 **0**。 |

### 8.8 P3 收尾批次（`CQ-01`、`RED-01`、`DOC-11`）

> 本轮收尾 §6.3 第 8/9/10 项的最后三项。三者彼此独立：`CQ-01` 与 `DOC-11` 为门禁建设，`RED-01` 为源码收敛。

| 项 | 内容 |
|----|------|
| **CQ-01（allow 白名单 + 理由必填 ratchet）** | 新建 [`check_clippy_allow_ratchet.py`](../../scripts/ci/check_clippy_allow_ratchet.py)：扫描 9 个源根（与 `check_trait_ratchet.py`/`check_file_size_ratchet.py` 同 `ROOTS`/`EXCLUDED` 范式），解析 `#![allow(…)]`/`#[allow(…)]` 的**顶层项**（`_split_top_level`/`_parse_attribute`），要求每个 allow 站点「已论证」。**判定模型**：内联 `reason = "…"`、或**同行尾部** `// …`、或**上一行** `// …` 才算已论证（`///`/`//!` 文档注释本身描述被注解项、不算抑制理由）。**四类失败**：`blanket`（group lint 硬禁，如 `clippy::all`/`warnings`/`pedantic`）、`allowlist_missing_reason`（白名单条目缺理由）、`unlisted`（未登记 lint）、`new_offenders`/`grown`/`stale`（逐文件 grandfather 基线只降不升：新文件超基线、超其记录上限、条目陈旧）；扫描 0 处或基线缺失 → `exit 2`（fail-closed，防铁律 8 假绿）。白名单 [`clippy_allow_allowlist`](../../scripts/ci/clippy_allow_allowlist) **13 条**均带理由；基线 [`clippy_allow_baseline`](../../scripts/ci/clippy_allow_baseline) 记 **176 unjustified / 108 文件**。接入 [`.github/workflows/ci.yml`](../../.github/workflows/ci.yml) `repo-sanity` job（"Clippy-allow ratchet (allowlist + reason-required)"）。 |
| **RED-01（收敛重复 INSERT）** | [`synapse-storage/src/event/create.rs`](../../synapse-storage/src/event/create.rs) 中 `create_event_with_pdu` 与 `create_outlier_event` 的 INSERT **逐字节重复**（outlier 副本原带「必须逐字节一致以复用 `.sqlx` 条目」注释 → 漂移陷阱）抽为私有 [`insert_event_row_with_graph`](../../synapse-storage/src/event/create.rs#L64)，承载 13 列图 INSERT（`event_id…auth_events`，`VALUES ($1..$12)`）；两调用方差异仅剩「事件行落地后是否追加 `event_edges`」。`INSERT INTO events` 字面量 **5 → 4 处**（余 `create_event`、共享 helper、MSC4242 state event、另一处）；SQL 仍留在 `sqlx::query_as!` 宏内不提升（遵 §7 D-59/R1）。两调用方共享同一 `sha256(SQL)` → 复用同一 `.sqlx/query-<hash>.json` 缓存条目。 |
| **DOC-11（防漂移门禁覆盖 `migrations/`、`.trae/rules/`、markdown 链接）** | ① **markdown 链接形态**：守卫 [`doc_credibility_guard_tests.rs`](../../tests/unit/doc_credibility_guard_tests.rs) 的 [`repo_path_from_reference`](../../tests/unit/doc_credibility_guard_tests.rs#L96)（剥 `:行号`/`#锚点` + 白名单过滤）+ [`referenced_repo_paths`](../../tests/unit/doc_credibility_guard_tests.rs#L117) 用 `.chain()` 合并**反引号路径**与**markdown 链接**两形态；红证明 [`the_path_checker_resolves_markdown_links`](../../tests/unit/doc_credibility_guard_tests.rs#L439)（喂不存在的链接必须变红）。② **`migrations/`**：[`docs-quality-gate.yml`](../../.github/workflows/docs-quality-gate.yml) 的「Discover active markdown files」步骤纳入 `find migrations -name "*.md"`（实测收集 136 文件，含 `migrations/INDEXES.md`/`README.md`）。③ **`.trae/rules/`**：因 [`.gitignore:136`](../../.gitignore#L136) 使 `.trae/` 不入 git、CI checkout 缺席 ⇒ 任何 `find .trae/...` 都会**静默匹配为空**（铁律 8 假绿），故**有意不纳入 CI**；改由本地守卫 [`workspace_rules_reference_paths_exist`](../../tests/unit/doc_credibility_guard_tests.rs#L365) 覆盖（含 `assert!(!docs.is_empty(), …)` 防空转），残余风险已在 yml 注释写明。 |
| **验证（CQ-01）** | `python3 scripts/ci/check_clippy_allow_ratchet.py` → `clippy_allow: 161 site(s) in 775 file(s); 176 unjustified in 108 file(s) (baseline 176 in 108 file(s))` / `OK: clippy allow suppression at baseline` / `exit 0`。红证明（记录于 `ci.yml` 注释）：reasonless allow / unlisted lint kind / blanket group / allowlist entry without reason 均 `exit 1`，基线态 `exit 0`。 |
| **验证（RED-01）** | `git diff --stat synapse-storage/src/event/create.rs` → `1 file changed, 58 insertions(+), 48 deletions(-)`；`INSERT INTO events` 字面量 5 → 4；既有不变式测试覆盖两条调用路径（`test_create_event_with_graph_with_prev_events`、`create_event_with_graph`/`create_state_event_with_dag`/`create_event_with_pdu` 三条 `*_rolls_back_event_when_edges_insert_fails`）。 |
| **验证（DOC-11）** | `cargo test -p synapse-rust --features test-utils --test unit doc_credibility_guard` → **10 passed; 0 failed**。 |
| **残余（有意不修）** | `migrations/README.md` 仍有反引号路径漂移（如 `docs/audit/PROJECT_ACTUAL_ISSUES_2026-09-14.md`、`docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md`、`.scratch/db-schema-audit-2026-09-04.md`）；lychee 只查真正的 markdown 链接不查反引号路径，且本轮未把守卫 `path_violations` 扫到 `migrations/`，故不触发门禁。归属 `DOC-07/08` 范畴。 |

### 8.9 P2 剩余批次（`SEC-01`、`FED-02`、`PERF-01`、`PERF-06`、`CQ-02`、`CQ-03`、`CQ-05`）

> 本轮收尾 §2.B（P2）中尚未闭环的 7 项。两项方案取向已与用户确认：`CQ-05` 取「**最小：保留 `String` 存储、显式化错误路径**」（不做存储枚举化），`FED-02` 取「**有则校验/无则放行**」（兼容规范的 `send_join` 模板请求体）。

| 项 | 内容 |
|----|------|
| **SEC-01（EDU 密钥长度/格式校验）** | [`edu.rs:625`](../../synapse-web/src/federation/edu.rs#L625) 在 `validate_signing_key_type` 之后、`upsert_federation_cross_signing_key` 之前新增 `decode_base64_32(key_str).is_none()` 守卫：交叉签名密钥须是 base64 编码、且解码后恰为 **32 字节** 的 ed25519 公钥；失败即 `dropped += 1; continue;` + `warn!`，不再把畸形/超长载荷写入存储。 |
| **FED-02（入站 join PDU 完整性校验）** | 新增共享守卫 [`verify_inbound_join_pdu_integrity`](../../synapse-web/src/routes/federation/membership/mod.rs#L157)，接入 [`join.rs:155`](../../synapse-web/src/routes/federation/membership/join.rs#L155)（v1）与 [`join.rs:307`](../../synapse-web/src/routes/federation/membership/join.rs#L307)（v2）；`verify_pdu_sender_signature` 由私有提升为 `pub(crate)`（[`transaction.rs:918`](../../synapse-web/src/routes/federation/transaction.rs#L918)）以复用。**语义（有则校验/无则放行）**：`send_join` 请求体按规范是**事件模板**（`origin`/`origin_server_ts`/`type`/`state_key`/`content`），规范合规对端不带 `hashes`/`signatures`，故仅当入站事件**自带** `hashes` 时校验内容哈希（`verify_event_content_hash`）、仅当其 `signatures` **覆盖 sender 自身服务器**时校验发送者签名；否则放行，避免逐出所有合规对端。 |
| **PERF-01（`get_token` 单一查询）** | [`token.rs:141`](../../synapse-storage/src/token.rs#L141) 将「当前 HMAC 哈希命中 → 未命中再查 legacy 哈希」的双查询回退合并为一次 `WHERE token_hash IN ($1, $2) AND is_revoked = FALSE ORDER BY (token_hash = $1) DESC LIMIT 1`（当前哈希优先）；Token 校验热路径 DB 往返 **2 → 1**。`.sqlx` 缓存已按唯一入口重生成，新鲜度门禁通过。 |
| **PERF-06（性能基线连接池读数更正）** | [`PERFORMANCE_BASELINE.md:58`](../../docs/PERFORMANCE_BASELINE.md#L58) 升 **v1.1**：`5/100 (5%)` → **`5/50 (10%)`**，分母取 [`DatabaseConfig::max_size`](../../synapse-common/src/config/database.rs) 默认 **50**（`pool_size` 已废弃、零引用），并加注更正依据与部署不变式（`50 × 进程 + 保留 ≤ max_connections`）。 |
| **CQ-02（`expect_used` 理由注释）** | 11 处生产 `expect_used` 站点补 `#[allow(clippy::expect_used, reason = "…")]` 证明性注释（`crypto.rs` / `key_encryption.rs` / `media_link_signer.rs` / `validation.rs` / `loader.rs` / `claims.rs` / `login.rs` / `room/service.rs` / `friend_room_service/models.rs` / `voice.rs` / `sliding_sync/repository.rs`），并删除 `loader.rs` 一处冗余 allow；`check_clippy_allow_ratchet.py --update` 收紧基线 **176 → 164** unjustified、**108 → 99** 文件，门禁 `OK`。 |
| **CQ-03（保留 error cause）** | [`membership/service.rs:781`](../../synapse-services/src/room/membership/service.rs#L781) `get_room_members_paginated` 两处 `map_err(\|_e\| …)` 改为保留 cause：`Failed to check room existence: {e}` / `Failed to check membership: {e}`。 |
| **CQ-05（membership 解析显式化，最小方案）** | 全仓 `Membership::from_str` 经复核仅 4 个站点；本次显式化其中 2 处**静默**站点：① [`membership/service.rs:559`](../../synapse-services/src/room/membership/service.rs#L559) `resolve_membership_from` 遇不可解析值 → `ApiError::internal(...)`（fail-closed，暴露数据完整性错误，不再静默降级为 `None` 而削弱转换合法性检查）；② [`state_map_auth.rs:131`](../../synapse-federation/src/event_auth/state_map_auth.rs#L131) `membership_of` 遇不可解析值 → `tracing::warn!` 后返回 `None`（使降级可见、保持既有 fail-closed 语义）。另 2 站点经复核**本已显式**（`state_map_auth.rs` 的 `to` 解析 `let-else return false`；[`transaction.rs:290`](../../synapse-web/src/routes/federation/transaction.rs#L290) `warn!` + `None`）。按用户决策**保留 `String` 存储、不做枚举化**。 |
| **验证** | `cargo check -p synapse-web -p synapse-services -p synapse-federation -p synapse-storage` → **exit 0**；`cargo clippy`（lib 目标）**无告警**；`bash scripts/ci/check_sqlx_cache_fresh.sh --static` → `OK: 源码 1357 处静态查询字面量全部命中 .sqlx/ 缓存`；`python3 scripts/ci/check_clippy_allow_ratchet.py` → `164 unjustified in 99 file(s) (baseline 164 in 99)` / `OK`。 |
| **残余（有意不修）** | `--all-targets`（含 `#[cfg(test)]`）在 `synapse-services/src/test_mocks.rs` 等测试模块存在**既有**编译错误（E0282/E0432/E0433），与本批生产代码改动无关，未在本轮处理。`send_leave`/`send_knock` 亦调 `validate_federation_member_event`，但 §2.B `FED-02` 证据仅指向 `join.rs`，本轮未扩展。 |

### 8.10 P3 剩余批次（批 A 文档卫生 6 项 + 批 B 代码质量/低风险安全 5 项）

> 本轮收尾 §2.C（P3）中用户选定的两批共 11 项：**批 A** 文档与链接卫生（`DOC-06`/`DOC-07`/`DOC-08`/`DOC-09`/`DOC-10`/`CQ-08`），**批 B** 代码质量与低风险安全（`CQ-06`/`SEC-02`/`SEC-03`/`SEC-06`/`PERF-08`）。by-design 登记批（`FN-05`~`FN-08`）与需决策批（`COMPAT-07/08/09`、`CQ-07`、`SEC-04/05`、`PERF-07/09` 等）**不在本轮**。

**批 A —— 文档与链接卫生**

| 项 | 内容 |
|----|------|
| **DOC-06** | `.trae/rules/project_rules.md` §7.0 的指向链接 `src/services/container.rs` 已在 §8.2 随 `DOC-01` 修为 `synapse-services/src/container.rs`（规则 v2.5.0 版本历史已记录）；本轮复核确认闭环。 |
| **DOC-07/08** | [`migrations/README.md`](../../migrations/README.md) 两类断链就地修复：① :362 引用不存在的 `docs/synapse-rust/COMPREHENSIVE_AUDIT_REPORT_2026-06-03.md` 与 `.scratch/db-schema-audit-2026-09-04.md` → 改指现存权威文件；② :363 断链路径统一指向 `migrations/INDEXES.md`/`docs/audit/` 现存文档。 |
| **DOC-09** | `docs/audit/` 归档 **5 份**重复报告至 [`docs/audit/archive/`](../../docs/audit/archive/)（`git mv`）：`COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261004.md`、`LEGACY_ISSUES_REPORT_20261004.md`、`LEGACY_ISSUES_REPORT_ROUND3_20261004.md`、`MUTATION_6_FAILURE_ANALYSIS_2026-09-29.md`、`MUTATION_6_REDESIGN_SYNTHETIC_2026-09-29.md`。**连带修复**：`.trae/rules/project_rules.md` §16「遗留问题审计」原指向已归档的 `LEGACY_ISSUES_REPORT_20261004.md` → 更新为当前权威 `COMPREHENSIVE_LEGACY_ISSUES_REPORT_20261006.md`（规则升 **v2.5.1**），使本地守卫 `workspace_rules_reference_paths_exist` 复绿。 |
| **DOC-10** | [`docs/audit/INDEX.md`](INDEX.md) 补 `docs/superpowers/plans/`（~35 文件，被 [`.gitignore:109-110`](../../.gitignore#L109-L110) 忽略）的说明段。 |
| **CQ-08** | 订正迁移/索引计数三处矛盾口径：[`migrations/README.md`](../../migrations/README.md) 统至 **218 张表 / 342 个索引**；[`migrations/INDEXES.md`](../../migrations/INDEXES.md) 升 **v1.4.0**（`351→342`、`141→134`），加 2026-10-06 订正块。 |

**批 B —— 代码质量与低风险安全**

| 项 | 内容 |
|----|------|
| **CQ-06** | [`voice.rs:1`](../../synapse-web/src/routes/voice.rs#L1) 裸 `#![allow(clippy::unused_async)]` 补内联 `reason = "handlers keep async signatures for uniform Axum routing even when the body performs synchronous validation only"`；allow-ratchet 基线随收紧 **164→163** unjustified / **99→98** 文件。 |
| **SEC-02** | 删除 [`cross_signing/service.rs`](../../synapse-e2ee/src/cross_signing/service.rs) 两处死代码：`upload_key_signature`（原 L138-158）与 `upload_signatures`（原 L314-352）。全仓复核无生产调用点；路由 `/keys/signatures/upload` 实际走 **另一处** `device_keys_service.upload_signatures`（[`device_keys/service.rs:651`](../../synapse-e2ee/src/device_keys/service.rs#L651)，含 ownership 校验），不受影响。导入为 `use super::models::*;` glob，删除后不产生 unused import。 |
| **SEC-03** | [`csrf.rs:49`](../../synapse-web/src/middleware/csrf.rs#L49) `session_id` 比较由 `!=` 改为 [`synapse_common::crypto::secure_compare`](../../synapse-common/src/crypto.rs#L102)，与同文件 L72-74 签名的恒时比较范式一致（`fn validate_token(&self, …)` 加 `return false;` 短路）。 |
| **SEC-06** | [`verify-security.sh:9`](../../scripts/test/verify-security.sh#L9) 硬编码密码改为 `${PRO_PASS:-SecurePassword123ChangeMe!}`（优先环境变量、回退本地默认并加注「勿用于生产环境」）。 |
| **PERF-08** | [`media_service.rs`](../../synapse-services/src/media_service.rs) 构造函数 `new`/`with_pool` 由**同步改 async**，内部 `std::fs::create_dir_all`/`try_exists` → `tokio::fs::*`；上传路径同改。全部调用点刷新 `.await`：[`wiring/core.rs:122`](../../synapse-services/src/wiring/core.rs#L122)、[`media/mod.rs:750/1135`](../../synapse-services/src/media/mod.rs#L750)、`media_service.rs` 内部测试 11 处、[`tests/unit/media_service_tests.rs`](../../tests/unit/media_service_tests.rs) helper 及 20 处调用（`test_media_service_creation` 由 `#[test]` 改 `#[tokio::test]`）。文件余下 `std::fs` 均已在 `spawn_blocking` 内或属 `#[cfg(test)]` 辅助，无新增运行时阻塞。 |
| **验证** | `cargo check -p synapse-services -p synapse-e2ee -p synapse-web` → **exit 0**；`cargo check --test unit --features test-utils` → **exit 0**；`cargo clippy -p synapse-web -p synapse-e2ee -p synapse-services --lib` → **无告警**；`bash scripts/ci/check_sqlx_cache_fresh.sh --static` → `OK: 源码 1357 处静态查询字面量全部命中 .sqlx/ 缓存`；`python3 scripts/ci/check_clippy_allow_ratchet.py` → `163 unjustified in 98 file(s) (baseline 163 in 98)` / `OK`；`./scripts/check_fmt_ratchet.sh` → `fmt debt: current=0 baseline=0` / `OK`；`cargo test --test unit --features test-utils doc_credibility_guard` → **10 passed**。 |
| **残余（有意保留 / 登记）** | ① `DOC-09` 报告所述「v12 系列 **6 份**」为**过度概括**：`ROOM_V12_COMPLETION_PLAN_2026-09-27.md`、`ROOM_V12_PLAN_STATUS_2026-09-27.md` 等被**生产代码**直接引用（[`room_id.rs:28`](../../synapse-common/src/room_id.rs#L28)、[`room_versions.rs:107/141`](../../synapse-common/src/room_versions.rs#L107)、[`state_record.rs:5`](../../synapse-services/src/room/state_record.rs#L5)），故**不归档**、保留原位；本轮仅归档实际重复的历史报告 5 份。② §2.C 其余项本轮未处理：by-design 的 `FN-05`~`FN-08`；需决策的 `COMPAT-07/08/09`、`CQ-07`、`SEC-04/05`、`PERF-07/09`（`DEAD-02/03` 已于 §8.7 闭环、`DOC-11` 已于 §8.8 闭环）。 |

> **门禁终验补充（fmt 棘轮，2026-10-06）**：门禁终验时发现 `./scripts/check_fmt_ratchet.sh` 为 **FAIL**（`current=51 baseline=0`）。基线**并非**「grandfather 约定值」——[`d00711cb9`](../../scripts/.fmt-baseline) 已将 [`scripts/.fmt-baseline`](../../scripts/.fmt-baseline) 收紧为 **0**（「清空 31 处 fmt debt，棘轮收紧到 0」）。经核实：本轮触碰文件本身引入 **2 处**（`synapse-services/src/media/mod.rs` 的 `.await` 使 `let media_service = …` 由折行变为可单行；已就地修正），其余 **20 处 / 12 个文件**均为**更早轮次**遗留（典型为前几轮 `CQ-02` 补 `reason = "…"` 后 `#[allow(...)]` 超 120 列、及部分本可单行的结构体字面量），**不含本轮 §2.C 改动**。经用户决策，本轮执行 `cargo fmt --all` 一次性清空全部存量 fmt debt（12 个范围外文件纯格式化、无语义变更），并以 `./scripts/check_fmt_ratchet.sh --update` 将基线固化在 **0**；`rustfmt` 将内联 `#[allow(..., reason = "…")]` 展开为多行后，clippy-allow 棘轮的理由识别仍然生效（复跑 **163/98 OK**，未回退）。

### 8.11 剩余收尾批次（`CQ-07`、`FN-03`、`FN-04` + 需决策项核实 + by-design 登记）

> 本轮收尾 §2.C（P3）与 §2.B 余项（`COMPAT-05`）的**最后一批**。按用户指令「需决策的给出合理建议，然后按建议处理」，本节对每个需决策项先给建议再实施。分三类：**A 已实施代码改动**（`CQ-07`/`FN-03`/`FN-04`）、**B 复核为已闭环**（`COMPAT-05`/`SEC-04`/`SEC-05`/`PERF-07`/`PERF-09`/`COMPAT-08`）、**C by-design 正式登记**（`COMPAT-07`/`COMPAT-09`/`FN-05`~`FN-08`）。

**A —— 已实施代码改动**

| 项 | 内容 |
|----|------|
| **CQ-07（无理由 allow 三分类）** | 报告所列 ~26 处 `dead_code`/`unused_*` allow 经逐站点复核：**多数已有上一行/同行 `//` 理由**（非真无理由）；真正的无理由生产站点仅 **8 处**，按性质三分类处理：① **真死代码 1 处**——[`admin/mod.rs:60`](../../synapse-web/src/routes/admin/mod.rs#L60) 的 `#[allow(unused_mut)]` + `mut` 确为死代码（无任何 feature 分支重新赋值），**直接删除**；② **feature-gated 2 处**——[`notification.rs:78`](../../synapse-web/src/routes/admin/notification.rs#L78)（`mut router` 在 `#[cfg(feature="server-notifications")]` 块内被重新赋值，故 `mut` 必要）、[`wiring/extensions.rs:108`](../../synapse-services/src/wiring/extensions.rs#L108)（`user_service` 绑定受 `friends` feature 消费），补「feature-gated」理由注释；③ **test-only 脚手架 5 处**——[`aes.rs:220/258/642`](../../synapse-e2ee/src/crypto/aes.rs#L220)（测试对照密文/构造器/provider）、[`test_mocks.rs:127/129`](../../synapse-e2ee/src/test_mocks.rs#L127)（镜像 `device_lists_stream` 行形状），补「test-only parity」理由注释。均为 **rustc lint**（不在 clippy-allow ratchet 统计内），故基线不变（**163/98**）。 |
| **FN-03（`get_cross_signing_keys` 签名恒空串）** | **建议：诚实修正而非仅登记 latent**——该函数返回的 `CrossSigningKeys` 携带 `self_signing_signature`/`user_signing_signature`，原实现恒空串（撒谎字段）。修正为从存储的 `signatures` 提取（形状 `{ <user_id>: { <key_id>: <sig> } }`）：新增私有 [`master_key_id`](../../synapse-e2ee/src/cross_signing/service.rs#L137)（优先取 `key_json` 记录的 key id、回退 `ed25519:<public_key>`）与 [`extract_signature_for`](../../synapse-e2ee/src/cross_signing/service.rs#L148)；[`verify_cross_key_signature`](../../synapse-e2ee/src/cross_signing/service.rs#L303) 原内联 key-id 计算改为复用 `master_key_id`（去重、行为不变）。**签名缺失时仍返回空串**（兼容 legacy 行），但**不再恒空**。补 4 个纯逻辑单测（`master_key_id_prefers_key_json_key_id` / `master_key_id_falls_back_to_public_key` / `extract_signature_for_reads_master_signature` / `extract_signature_for_missing_returns_empty`，无需 DB）。全仓复核确认该函数**无生产调用点**（联邦走 `get_public_cross_signing_keys`、`/keys/query` 走 `get_cross_signing_keys_batch`），故属**正确性修正**而非行为回归风险。 |
| **FN-04（URL 预览硬编码桩）** | **建议：保持 stub 行为不变、仅文档化**（最小、零兼容风险）。[`preview_url`](../../synapse-services/src/media_service.rs#L960) 补文档注释，显式声明其为 **Stub**：不抓取 `url`、返回占位 OpenGraph；受默认关闭的 `msc4452_enabled` 门控；真实实现须在 [`check_url_and_resolve`](../../synapse-services/src/media_service.rs) 已验证 IP 上抓取、并经 `pinned_client_for_url` 钉扎后方可广告 MSC4452。**不改行为**（避免破坏既有测试与端点形状）。 |

**B —— 复核为已闭环（无新增代码改动，登记确认）**

| 项 | 复核证据 |
|----|----------|
| **COMPAT-05**（P2 余项：迁移硬依赖 `pgcrypto`/`pg_trgm`） | 建议的**两条路径均已落地**：① 迁移内 [`:70-71`](../../migrations/00000000_unified_schema_v12.sql#L70-L71) 已是 `CREATE EXTENSION IF NOT EXISTS pgcrypto;` / `pg_trgm;`（幂等、不因扩展已存在而失败）；② [`migrations/README.md:52-71`](../../migrations/README.md#L52) 已文档化前置要求（必需扩展、`postgresql-contrib` 安装包、建库角色 `CREATE` 权限、受限角色由 DBA 预装的部署提示）。**无需改动**。 |
| **SEC-04**（撤销检查 30s TTL 缓存窗口） | **已闭环**：[`auth/mod.rs:47-50`](../../synapse-services/src/auth/mod.rs#L47-L50) `REVOCATION_CHECK_CACHE_TTL_SECS = 30` 并注明「所有撤销写入路径（logout/logout_all/change_password/deactivate_user/revoke_device(s)）均主动失效，TTL 仅异常路径兜底」——缓存窗口已被主动失效覆盖，非净窗口。 |
| **SEC-05**（登录锁定无 per-IP 节流） | **已闭环**：[`auth_compat.rs:340`](../../synapse-web/src/routes/auth_compat.rs#L340) `LOGIN_MAX_IP_ATTEMPTS = 30` + 双桶 [`check_login_lockout`](../../synapse-web/src/routes/auth_compat.rs#L353)（`login_fail:{ip}:{user}` + `login_ip_fail:{ip}`）；per-IP 桶靠 TTL 衰减、per-pair 桶登录成功即清；Redis 不可用默认 **fail-closed**（503）。服务层 [`login.rs`](../../synapse-services/src/auth/login.rs) 的 per-user 锁定与之互补。 |
| **PERF-07**（`COALESCE(user_id, sender)` 非 sargable） | **已闭环**：[`basic.rs:134-146`](../../synapse-storage/src/event/basic.rs#L134-L146) 谓词改为 `sender = $1`，附完整不变式说明（所有填充 `user_id` 的写路径令 `user_id = sender`，其余留 `NULL` → `COALESCE` 恒等于 `sender`）；改写后可用 `idx_events_sender_time(sender, origin_server_ts DESC)`。 |
| **PERF-09**（性能测试为模拟无断言） | **已闭环**：[`query_performance_tests.rs:8`](../../tests/performance/query_performance_tests.rs#L8) 与 [`api_load_tests.rs:3`](../../tests/performance/api_load_tests.rs#L3) 补**诚实头注释**，明示「SIMULATED、不触 DB/HTTP、不断言、永不因名称所述原因失败」，并指向真正的性能门禁（`compute_perf_gate.sh`/`sliding_sync_perf_gate.sh`/DB 测试/`k6`）；模拟形状保留为「可执行的负载画像描述」。 |
| **COMPAT-08**（`check_feature_matrix.py` `all-extensions` 超时 300s） | **已闭环**：[`check_feature_matrix.py:37-41`](../../scripts/ci/check_feature_matrix.py#L37-L41) 默认超时由 `300` 提升至 **`900`**（冷 workspace 需跑 `cargo check` 2+N 次），并支持 `FEATURE_MATRIX_TIMEOUT_SECS` 环境变量覆盖（CI 可调、无需改文件）。 |

**C —— by-design 正式登记（无代码改动）**

| 项 | 登记理由 |
|----|----------|
| **COMPAT-07**（`.well-known` 其余字段/边界细节） | **建议：复核即闭环**。逐端点复核 [`versions.rs`](../../synapse-web/src/routes/handlers/versions.rs)：`support` 用标准 `support_page`（非标准 `url` 已于 `COMPAT-02` 移除）；`capabilities` 的房间版本仅在规范位置 `capabilities.m.room_versions` 广告（非标准 `rooms` 块已删除，见 [`versions.rs:111-115`](../../synapse-web/src/routes/handlers/versions.rs#L111-L115)）；`server` 返回 `m.server: host:port`（含默认端口，规范允许）；`client` 返回 `m.homeserver.base_url` + 可选 `m.tile_server`（MSC3488）。**未发现残留缺陷**。 |
| **COMPAT-09**（unstable MSC 前缀与稳定前缀并存未收敛） | **建议：保持双前缀服务（by-design）**。已合并为稳定规范的 MSC，服务器须**同时**服务稳定路径与 `/unstable/` 路径——后者供合并前发布的旧客户端使用；单方面移除 unstable 前缀会破坏这些客户端的向后兼容。收敛应由「旧客户端占比归零」触发，而非文档整齐度驱动。 |
| **FN-05**（voice `/convert` `/optimize` `/transcription` 恒 501） | by-design 桩：这些端点属未定案媒体处理能力，以 **501**（经 `COMPAT-03` 归一为 404 errcode 语义，状态码由方法/能力门控决定）明确「未实现」，好于返回不可信结果。 |
| **FN-06**（`verify_secure_backup_passphrase` 恒 410） | by-design：该端点所依托的旧备份校验流程已废弃，**410 Gone** 明确告知客户端不再可用。 |
| **FN-07**（联邦 legacy keys 返回 `M_UNRECOGNIZED`） | by-design：legacy 密钥查询接口已不在规范内，返回 `M_UNRECOGNIZED` 是规范允许的「未识别端点」表达。 |
| **FN-08**（MSC 编号借用/形状漂移：MSC4502/4262/4429/4335/4354） | by-design 已登记：[`MSC_SEMANTICS.md`](../../docs/synapse-rust/MSC_SEMANTICS.md) 逐条记录语义对齐与编号借用说明（属「实验性编号随上游演进」的既有约定，非缺陷）。 |

| 行 | 内容 |
|----|------|
| **验证** | `cargo check -p synapse-e2ee --lib` → **exit 0**（FN-03/CQ-07 的 e2ee 改动编译通过）；`cargo test -p synapse-e2ee --lib cross_signing` → 新增 4 单测通过；`python3 scripts/ci/check_clippy_allow_ratchet.py` → **163 unjustified / 98 文件**、`OK`（CQ-07 改动为 rustc lint，不影响 ratchet）；`./scripts/check_fmt_ratchet.sh` → 首次 **FAIL**（`current=6`，FN-03 新增单测中本可单行的 `make_key(...)`/`assert_eq!(...)` 调用被 rustfmt 展开），执行 `cargo fmt -p synapse-e2ee` 后就地修正为 `current=0 baseline=0`、`OK`（纯格式化、无语义变更）。配套门禁终验见本节末。 |
| **残余（有意保留）** | ① `COMPAT-07`/`COMPAT-09`/`FN-05`~`FN-08` 为 **by-design**，保留现状并登记（不修）；② `FN-03` 修正后**签名缺失仍返回空串**（legacy 兼容），非「恒空」；③ `cargo check/clippy --all-targets` **不带** `--features test-utils` 时的 312 处 E0282/E0432/E0433（`synapse-services`/`synapse-web` 的 lib test 目标）已查明为**缺 feature 的调用假阳性**（非缺陷、非未处理；带 `--features test-utils` 即为 0 error / 0 warning），见 §8.12。 |

> **门禁终验（§8.11，2026-10-06）**：`cargo check -p synapse-services -p synapse-e2ee -p synapse-web` → **exit 0**（`Finished dev profile`）；`cargo clippy -p synapse-web -p synapse-e2ee -p synapse-services --lib` → **无告警**；`bash scripts/ci/check_sqlx_cache_fresh.sh --static` → `OK: 源码 1357 处静态查询字面量全部命中 .sqlx/ 缓存（0 处非字面量实参跳过）`（缓存含 1324 条查询元数据）；`python3 scripts/ci/check_clippy_allow_ratchet.py` → **163 unjustified / 98 文件**、`OK`；`./scripts/check_fmt_ratchet.sh` → `current=0 baseline=0`、`OK`；`cargo test --test unit --features test-utils doc_credibility_guard` → **10 passed**；`cargo test -p synapse-e2ee --lib cross_signing` → **44 passed**（含新增 4 单测）。
>
> **至此 §2.C「P3 级问题（27）」与 §2.B 余项 `COMPAT-05` 全部处置完毕**：闭环 21 项（含本轮 `SEC-04/05`、`PERF-07/09`、`COMPAT-05/07/08` 复核），by-design 登记 6 项（`COMPAT-09`、`FN-05`~`FN-08`、`COMPAT-07`），代码实施 3 项（`CQ-07`、`FN-03`、`FN-04`）。报告 §2/§6 中所有编号均有终态。

### 8.12 既有编译错误排查（`--all-targets` 缺 `test-utils`，**非缺陷**）

> 承接 §8.11「残余③」：排查 `cargo check/clippy --all-targets` 在 `#[cfg(test)]` 模块报出的 E0282/E0432/E0433。**结论：调用缺 `--features test-utils` 导致的假阳性，非代码缺陷，不做改动。**

| 行 | 内容 |
|----|------|
| **现象** | `cargo check --workspace --all-targets`（不带 features）→ **312 errors**：`E0282×190` / `E0432×47` / `E0433×75`，分布于 52 个文件；报错目标为 `synapse-services`(lib test) **309** + `synapse-web`(lib test) **3**。报错文本均为 `could not find \`test_mocks\` in \`synapse_storage\`/\`synapse_e2ee\`/\`synapse_federation\`` 及其级联的类型推断失败。 |
| **根因** | `test_mocks` 在三个依赖 crate 中均以 `#[cfg(any(test, feature = "test-utils"))]` 门控（如 [synapse-storage/src/lib.rs:144-145](../../synapse-storage/src/lib.rs#L144-L145)）。构建 `synapse-services`/`synapse-web` 的 **lib test** 目标时，依赖 crate 仅作为**普通依赖**编译，其 `cfg(test)` **不激活**；唯一通路是 `test-utils` feature 传播（[synapse-services/Cargo.toml:17](../../synapse-services/Cargo.toml#L17)、根 [`Cargo.toml:39`](../../Cargo.toml#L39)）。 |
| **证据** | `SQLX_OFFLINE=true cargo check --workspace --all-targets --features test-utils --locked` → **EXIT=0**，输出无 `error`/`warning`。 |
| **结论与处置** | **by-design，非缺陷，不改代码**。全仓 `--all-targets` 类命令**必须**带 `--features test-utils`；该要求已在 CI clippy（[`.github/workflows/ci.yml:375`](../../.github/workflows/ci.yml#L375)）、[`scripts/quality/preflight.sh:102-103`](../../scripts/quality/preflight.sh#L102-L103)、[`.cargo/config.toml`](../../.cargo/config.toml#L28-L31)（`nt` 别名）、[`.rust-analyzer.toml`](../../.rust-analyzer.toml) 中一致落实，§7.1 复现命令亦已带该 flag。不采用「dev-dependency 传 feature」方案：会引入特征统一副作用，且与既有 8-crate 约定不一致，收益低。 |

### 8.13 测试车道运行修复：`api_auth_routes_tests` 房间版本面断言过时

> 承接 §1.5 运行证据：全量集成车道暴露 1 例存量失败，判定为**测试过时**（非产品缺陷）并最小修复。

| 项 | 内容 |
|----|------|
| **现象** | `api_auth_routes_tests::test_versions_and_public_capabilities_match_declared_room_version_surface` 断言 `available.len() == creatable_count`，实际 `left: 12, right: 1`。 |
| **根因** | G-1（`7489b247f`，2026-09-28）将可创建房间版本收窄为仅 v12，但 `available` 的语义是「服务器**支持**的全部版本」（解析 / 加入 / 联邦），并非「可创建集合」；`8687d8335` 修正了实现与契约快照（`capabilities_v3.snap` 含 `"1".."12"`）却漏改本文件。 |
| **判定** | **测试过时**：实现（[`room_versions.rs`](../../synapse-common/src/room_versions.rs#L219) 的 `client_room_versions_capability()` 遍历 `SUPPORTED_ROOM_VERSIONS`）、契约快照、同模块单测三方一致为 12；`default` 才是唯一可创建版本（`DEFAULT_ROOM_VERSION = "12"`）。 |
| **修复文件** | [`tests/integration/api_auth_routes_tests.rs`](../../tests/integration/api_auth_routes_tests.rs#L194-L214)——断言改为 `available.len() == SUPPORTED_ROOM_VERSIONS.len()` 并逐版本核对 disposition，附根因注释。 |
| **证据** | 修复后全量集成：**1534 passed; 0 failed; 0 ignored; finished in 4121.50s**（EXIT=0）。 |

---

**生成时间**：2026-10-06
**下次计划更新**：2026-10-13
