# synapse-rust 遗留问题排查报告（第三轮）

> **报告版本**: Round 3 / v1.0
> **排查日期**: 2026-10-04
> **基线分支**: `feature/2026-10-01-metrics-docs-updates`
> **基线 HEAD**: `75902a835`（提交时间 2026-10-04 17:29:05 +0800）
> **项目版本**: synapse-rust v6.2.0（9 包 workspace，rustc 1.93.0）
> **前序报告**: [LEGACY_ISSUES_REPORT_20261004.md](./LEGACY_ISSUES_REPORT_20261004.md)（第二轮，结论 P1/P2/P3 全清零）— 本报告为其**独立第三轮**复核，保留前序报告作为历史快照。
>
> **核心方法论（重要）**：前序报告中大量「🆕/已修复」标记是无法自我验证的历史断言。本轮**一律以当前代码、当前测试运行结果取证**，不采信报告文字。凡无法在前述 HEAD 上复现或定位的条目，标注为「未复现 / 证伪」。

---

## 一、排查方法与范围

| 维度 | 手段 | 实际执行 | 结果 |
|------|------|----------|------|
| 静态门禁 | clippy 全量 -D warnings | `cargo clippy --workspace --all-targets --features test-utils --all-features` | ✅ exit 0 |
| 格式门禁 | rustfmt check | `cargo fmt --all --check` | ✅ clean |
| 依赖漏洞 | 安全扫描 | `cargo audit`（624 crate） | ✅ exit 0，0 漏洞 |
| 依赖分裂 | `cargo tree -d --workspace` | 实测 | ✅ 54 组同名重复，与 [DEPENDENCY_UPGRADE_TRACKER.md](../synapse-rust/DEPENDENCY_UPGRADE_TRACKER.md) 口径一致，无回归 |
| 债务标记 | 全树 TODO/FIXME/HACK/XXX/BUG 扫描 | 11 处 | ✅ 全部良性（无残留待办） |
| 单元测试 | `cargo test --test unit` | 实跑 | ⚠️ 1 失败，定性为**守卫误报**（见 §四.E.1） |
| 集成测试 | nextest 连 Test DB 实跑代表性子集 | 10 文件 / 53 测试 | ❌ 52 通过 / 1 失败（**真实回归**，见 §四.A.1） |
| 代码审查 | 5 路并行子代理（功能/性能/安全/质量/配置），逐条 file:line 取证 | 全量覆盖七大领域 | 见 §四 |
| 文档审查 | 交叉核对计数口径、版本口径、死链 | 全量 | 见 §四.F |

**集成测试可行性说明（重要基建债务）**：`tests/integration/*.rs` 全部编译为**单一 `integration` 测试二进制**（nextest 中显示为 `synapse-rust::integration`），共 **1525 个测试**，单个测试约 25–33s（含独立 DB schema 建立/迁移），吞吐约 9 tests/min ⇒ **全量约需 3 小时**，在本次排查窗口内不可行。故本轮采用**代表性子集**实跑（选取 10 个文件：room_summary / delayed_state_event / voice / relations / e2ee / admin 等高风险面），并以静态审查覆盖其余。

---

## 二、总体结论

**与前两轮「全清零」结论相冲突：本轮在 HEAD `75902a835` 上取到 P1 级问题 8 项、P2 级 14 项、P3 级 12 项，并证伪约 40 项历史指控。**

最需要立即处理的是：

1. **C10 安全修复引入的真实回归**（§四.A.1）：`fail-closed` 的 admin role 归一化使「未设置 `user_type` 的既有 admin 账户」失去全部管理权限，集成测试 `api_room_summary_routes_tests` 已转红，且直接冲击部署兼容性（项目规则 §14 的 `admin` 测试账户、历史流程创建的生产管理员）。
2. **两处可被远端触发的 panic**（§四.C.1、§四.C.2）：媒体魔数检测的运算符优先级越界、HTML 净化器的非字符边界切片。
3. **一处使既有安全修复在生产镜像中失效的构建覆盖**（§四.G.1）：Dockerfile 用 `CARGO_PROFILE_RELEASE_PANIC=abort` 覆盖了 `Cargo.toml` 的 `panic="unwind"`。

同时正面确认：静态门禁（clippy/fmt/audit）在 HEAD 全绿，依赖分裂无回归，无新增依赖漏洞。

---

## 三、优先级总览

| 优先级 | 数量 | 主题分布 |
|--------|------|----------|
| **P1** | 8 | C10 回归与部署兼容 1、事务完整性 1、远端 panic 2、生产构建覆盖 1、部署默认开放注册 1、部署密钥缺失 1、联邦状态错误 1 |
| **P2** | 14 | 错误吞没/静默降级 5、性能瓶颈 4、文档口径矛盾 3、开发模式安全降级 1、工作区卫生 1 |
| **P3** | 12 | 丢弃计算结果 2、无界查询 1、边界与默认值 4、文档轻微漂移 3、职责重叠 2 |
| 证伪 | ~40 | CORS 通配+凭据、CSRF 绕过、CSP XSS、SQL 注入、认证中间件缺口、非恒定时间比较、密钥日志、路径遍历、限流缺失、未净化输入等（见 §五） |

---

## 四、分领域详细清单

> **证据标记图例**：✅ = 本会话在 HEAD 上亲自读取代码/运行复现确认；◐ = 子代理取证（附 file:line），本会话未逐条重跑。

### A. 功能完整性与正确性

#### A.1【P1】C10 admin role fail-closed 修复引入回归，且冲击部署兼容 ✅

- **位置**：[synapse-web/src/utils/admin_auth.rs](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/utils/admin_auth.rs#L164-L188)（`normalize_admin_role`）、[tests/integration/api_room_summary_routes_tests.rs:535](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/integration/api_room_summary_routes_tests.rs#L514-L535)
- **现象**：集成测试 `test_room_summary_batch_excludes_rooms_the_caller_cannot_see` 失败：
  ```
  panicked at tests/integration/api_room_summary_routes_tests.rs:535:5:
  admin summary creation must succeed: 403 Forbidden
  ```
- **根因（已确认因果链）**：`normalize_admin_role` 在 `user_type` 缺失时返回常量 `NO_ADMIN_ROLE = "none"`（fail-closed），使 `is_role_allowed` 落入 `_ => false` 分支，**拒绝一切 RBAC 受控端点**。运行日志佐证：`Admin user has no user_type set ... event="admin_role_missing"` + `RBAC check result ... path=/_synapse/room_summary/v1/summaries allowed=false rbac_enabled=true`。
- **影响范围**：任何**未显式设置 `user_type`** 的 admin 账户。包括：测试 fixture 中的 admin、项目规则 §14 的 `admin` 测试账户、历史流程（早于 C10）创建的生产管理员 —— 升级后**全部丧失管理权限**，且端点仅返回 403，无迁移提示。
- **严重程度**：P1。既是**测试回归**（CI 红），也是**升级兼容性事故**（生产管理员被静默锁死）。
- **建议方案**：
  1. 修复测试 fixture：为 admin 用户显式设置 `user_type='admin'`（这是**正确**的长期方向，不应回退 fail-closed 语义）。
  2. **必须**同时提供数据迁移：为存量 admin 用户回填 `user_type`（迁移脚本 + 校验），否则升级即事故。
  3. 在 `normalize_admin_role` 的 warn 分支增加**可观测告警/审计事件**（已有 `security_audit` target），并在发布说明中显式记录为**破坏性变更**。

#### A.2【P1】join_room / leave_room 多次独立写操作无事务包裹 ✅

- **位置**：[synapse-services/src/room/membership/actions.rs:127-153](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/room/membership/actions.rs#L127-L153)（join）、[:206-234](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/room/membership/actions.rs#L206-L234)（leave）
- **问题**：成员写入（`add_member` / `remove_member`）、成员计数（`increment_member_count` / `decrement_member_count`）、事件落库（`create_event`）为**三次独立 `await`**，无共享事务。对比同文件 `leave_and_forget`（L408-536）使用 `pool.begin()` 显式事务。
- **影响**：任一后续步骤失败时，前序写入已提交 → 成员表与事件流、成员计数之间出现**永久不一致**（幽灵成员 / 计数漂移）。join/leave 是最高频写路径。
- **严重程度**：P1（数据完整性）。
- **建议方案**：以 `leave_and_forget` 为模板，将三写操作纳入单一 `sqlx` 事务；计数采用事务内 `UPDATE ... SET member_count = member_count ± 1` 而非读改写。

#### A.3【P1】联邦 m.typing EDU 只置 true，永不清除 ◐

- **位置**：[synapse-federation .../edu.rs:325-341](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-federation/src/edu.rs#L325-L341)、typing.rs:71-77
- **问题**：处理远端 `m.typing` EDU 时仅置 `typing=true`，未处理 `false` 的清除路径。
- **影响**：远端用户停止输入后，本地长期显示「正在输入」（状态泄漏 / 用户体验错误）。
- **建议方案**：按 EDU 的 `typing` 布尔值双向更新，并加入超时自动过期。

#### A.4【P2】read marker 写入错误槽位 `m.fully_read` ◐

- **位置**：[read_markers.rs:58-66](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/read_markers.rs#L58-L66)
- **问题**：处理 `m.read` 回执时写入 `"m.fully_read"` 账户数据槽。
- **影响**：`m.read` 与 `m.fully_read` 语义混用，客户端已读标记错乱。
- **建议方案**：`m.read` 写 `m.read` 槽（或按 Matrix 语义映射到 fully_read），并补充契约测试。

#### A.5【P2】send_receipt 参数硬编码 ◐

- **位置**：[receipts.rs:22-39](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/receipts.rs#L22-L39)
- **问题**：回执类型/参数硬编码，未使用请求入参。
- **建议方案**：改用入参 `receipt_type`。

#### A.6【P2】房间摘要批量写入失败仅告警、成员更新吞错 ◐

- **位置**：`room_summary/state.rs:167-175`（批量写失败仅 warn）、`state.rs:57-65,106-108,211-214`（成员更新吞错）
- **影响**：摘要与真实成员状态长期不一致，且无重试/无失败可见性。
- **建议方案**：失败上抛或进入重试队列，至少记录结构化错误指标。

#### A.7【P2】recalculate_stats 全量加载 + storage_size 硬编码 0 ◐

- **位置：`stats.rs:26`（全量加载）、`stats.rs:47-48` 与 `room_summary.rs:137`（`storage_size` 恒为 0）
- **建议方案**：改为 SQL 聚合；`storage_size` 若确不实现，应在 API 文档标注为保留字段而非返回误导性 0。

#### A.8【P3】成员/子节点异常静默降级 ◐

- `state.rs:41` 缺 `membership` 默认 `join`；`service.rs:556` 未知 membership 静默 `None`；`space/children.rs:299` 子节点错误吞没。建议显式拒绝或记录。

### B. 性能与稳定性

#### B.1【P1】统计重算 i64::MAX 全量加载 ◐

- **位置：`stats.rs:26`、`basic.rs:97-108`
- **问题**：以 `i64::MAX` 为 limit 的全量加载，随房间规模线性增长内存占用。
- **建议方案**：SQL 端聚合（`COUNT`/`SUM`）替代内存聚合。

#### B.2【P1】summary 状态/成员同步 N+1 → O(N²) ◐

- **位置：`room_summary/state.rs:171-175`、上游参照 `repository.py:249-287,284,877-904`
- **问题**：逐成员循环内发起查询，成员数为 N 时退化为 O(N²)。
- **建议方案**：批量 `IN (...)` 查询 + 内存 join。

#### B.3【P2】CircuitBreaker 每次成功取写锁并对全窗口 O(n) retain ◐

- **位置：`synapse-cache/src/circuit_breaker.rs:118-127`（`record_success`）、`:303-317`
- **建议方案**：用环形缓冲/原子计数替代 Vec retain；成功路径避免写锁。

#### B.4【P2】HalfOpen 无并发探测上限 ◐

- **位置：`circuit_breaker.rs:278`
- **问题**：半开态可并发放行任意多探测请求，故障时放大冲击。
- **建议方案**：限制半开探测并发数。

#### B.5【P2】presence 扇出目标集无界 ◐

- **位置：`data_fetch.rs:198-230,244,248`、`membership/mod.rs:345-359`
- **建议方案**：分页/限流扇出，设置上限。

#### B.6【P3】无界查询与丢弃计算 ◐

- `background_update.rs:385-412`（`get_updates_by_status` 无 LIMIT）；`src/server/mod.rs:966-967`（warmup `COUNT(*)` 结果丢弃）；`data_fetch.rs:15-18`（`update_presence` 吞错）。

#### B.7【P3】`Instant::now() - window_size` 可能下溢 panic ◐

- **位置：`circuit_breaker.rs:124`
- **建议方案**：改用 `checked_sub` / `saturating_sub`。

### C. 安全性

#### C.1【P1】媒体魔数检测运算符优先级 + 越界 panic，可远端触发 ✅

- **位置**：[synapse-services/src/media_service.rs:646-650](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/media_service.rs#L646-L650)
  ```rust
  if data.len() >= 6 && &data[0..6] == b"GIF89a" || &data[0..6] == b"GIF87a" {
  ```
- **问题**：`&&` 优先级高于 `||`，实际解析为 `(len>=6 && ==GIF89a) || (&data[0..6] == b"GIF87a")`。当 `data.len() < 6`（例如上传 1–5 字节的极小文件）时，右侧仍执行 `&data[0..6]` → **切片越界 panic**。
- **影响**：通过媒体上传接口即可触发服务 panic（拒绝服务；配合 §四.G.1 的 abort 构建则**整进程崩溃**，非仅请求失败）。
- **建议方案**：加括号 `data.len() >= 6 && (&data[0..6] == b"GIF89a" || &data[0..6] == b"GIF87a")`；或改用 `data.starts_with(b"GIF89a") || data.starts_with(b"GIF87a")`。

#### C.2【P1】HTML 净化器按字节切片，非字符边界 panic，可远端触发 ✅

- **位置**：[synapse-common/src/sanitizer.rs:133](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/sanitizer.rs#L131-L144)、[:148](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/sanitizer.rs#L146-L151)
  ```rust
  let input = if input.len() > self.max_length { &input[..self.max_length] } else { input };
  ```
- **问题**：对 `&str` 做字节索引切片。当第 `max_length`（默认 1000）字节落在多字节 UTF-8 字符中间（中文/emoji 极常见），**panic**。
- **影响**：用户可控输入（事件内容、displayname 等）即可触发 panic（DoS）。
- **建议方案**：改用字符边界安全截断，如 `input.char_indices().take_while(|(i,_)| *i < max).last()` 或 `floor_char_boundary`。

#### C.3【P2】SAML 管理员登出吞错 + 硬编码 `sessions_invalidated: 1`（fail-open 报告） ✅

- **位置**：[synapse-web/src/routes/saml.rs:476-478](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/saml.rs#L466-L479)
  ```rust
  let redirect_url = ctx.saml_service.initiate_logout(&session.session_id, Some("Admin initiated logout")).await.ok();
  Ok(Json(SamlLogoutAdminResponse { user_id: body.user_id, redirect_url, sessions_invalidated: 1 }))
  ```
- **问题**：`.ok()` 丢弃登出失败（SLO 未执行 / 会话未失效），响应仍恒报 `sessions_invalidated: 1`。
- **影响**：管理员被误导认为会话已终止，实际会话可能仍有效 → **访问控制失效的错觉**。
- **建议方案**：失败上抛；成功计数以实际失效数返回；`redirect_url` 无法构建时给出明确错误。

#### C.4【P2】开发 + localhost 模式 CORS 反射 Origin 且带凭据 ◐

- **位置：`cors.rs:173-174,197,290-292`
- **问题**：仅 `dev + localhost` 生效，反射请求 Origin 并允许凭据。若该模式被误用于可达环境，构成 CSRF/凭据泄露面。
- **建议方案**：保持仅在 debug 构建可用，并在 release 构建下编译期屏蔽；启动时打印醒目告警。

### D. 兼容性与可访问性

- **D.1【P1】生产镜像 `panic=abort` 覆盖 `panic=unwind`（兼容 + 稳定性）** ✅ — 见 §四.G.1（与配置领域合并叙述）。
- **D.2【P2】C10 修复对存量 admin 的破坏性兼容** ✅ — 见 §四.A.1。
- **D.3【P3】`.worktrees/` 未被 gitignore** ✅ — 见 §四.F.4。
- 未发现其他有据可查的可访问性问题（无障碍属 Web 前端面，本轮无前端改动，未产生新证据）。

### E. 代码质量与技术债务

#### E.1【P2 / 门禁脆弱性】unsafe 守卫误报，使 `--test unit` 在 HEAD 转红 ✅

- **位置**：守卫 [tests/unit/ci_test_scope_tests.rs:996-1020](file:///Users/ljf/Desktop/hu_ts/synapse-rust/tests/unit/ci_test_scope_tests.rs#L996-L1020) 内嵌 `grep -rn 'unsafe' ... | grep -vE '...//'`；误报源 [synapse-common/src/config/validation.rs:99](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/config/validation.rs#L97-L103)
- **问题**：守卫仅过滤以 `//` 开头的注释行，**不过滤字符串字面量**。`validation.rs:99` 的英文诊断串 `"...This combination is unsafe and rejected by browsers..."` 被误判为真实 `unsafe` 代码 → 单元测试红。
- **影响**：门禁噪声，且可能掩盖真实 `unsafe` 引入（狼来了效应）。
- **建议方案**：守卫改用 `cargo geiger` 或基于 AST 的检测；至少排除字符串字面量（`grep` 无法可靠做到，应换实现）。

#### E.2【P2】静默吞错 / 静默过滤 ✅

- [refresh_token_service.rs:267](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-services/src/refresh_token_service.rs#L267)：`record_usage(...).await.ok();` — 刷新令牌使用记录失败静默丢弃（审计缺口）。
- [response_helpers.rs:69](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-web/src/routes/response_helpers.rs#L60-L72)：`share_common_rooms_batch(...).await.unwrap_or_default();` — DB 出错时返回空集 → **合法用户被静默排除出响应**（用户可见的功能性错误，且掩盖故障）。
- **建议方案**：`.ok()` 改为记录结构化 warn 或上抛；`unwrap_or_default()` 应显式区分「空结果」与「查询失败」。

#### E.3【P3】职责重叠 ◐

- `RoomSyncServices` 内 `room`/`summary`/`sync` 职责边界模糊（`wiring/rooms.rs:40-43`）。建议按项目规则 §7.3 明确边界。

### F. 文档与知识库

#### F.1【P2】路由计数口径矛盾：`1154` vs `1153` ✅

- [API_COVERAGE_REPORT.md:682](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/API_COVERAGE_REPORT.md#L679-L685) 称 `ROUTE_CONTRACT.md` 为 **1154 条，与 §1.1 表一致**；但同文件 §1.1 第 104 行「注册条目」= **1153**，第 108 行明言「同为 1153」，且 [ROUTE_CONTRACT.md:11](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/ROUTE_CONTRACT.md#L11) 亦为 **1153**。文档索引处的 1154 既**错**又**虚假声称一致**。（`API_COVERAGE_REPORT.md:532` 配方注释同为 1154。）

#### F.2【P2】非命名空间桶计数矛盾：`14` vs `8`（且守卫钉死 8）✅

- [ROUTE_CONTRACT.md:103](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/ROUTE_CONTRACT.md#L100-L103) 与生成器 [scripts/contract/gen_contract_doc.py:421](file:///Users/ljf/Desktop/hu_ts/synapse-rust/scripts/contract/gen_contract_doc.py#L420-L421) 称「**14 条**有意根级注册（3 条探活 + **11 条 CAS 根协议端点**）」。但**机器真值**（[test_extract_registered.py:338-349](file:///Users/ljf/Desktop/hu_ts/synapse-rust/scripts/contract/test_extract_registered.py#L338-L349)）`expected` 集合为 **8 条**（3 探活 + 5 条 legacy CAS admin 别名）；[API_COVERAGE_REPORT.md:115-116](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust/API_COVERAGE_REPORT.md#L114-L120) 亦称 **8 条**并说明 CAS 协议 6 条已迁入 `/_synapse/cas/`（故不入桶）。生成器把「5 别名 + 6 协议 = 11」错误并入根级桶。
- **根因**：`gen_contract_doc.py:421` 将该叙述**硬编码为字符串**，不随机器真值重算 → 文档与守卫长期背离。

#### F.3【P3】`comparison.md` 房间版本口径自相矛盾 ✅

- [docs/synapse-rust-vs-synapse-comparison.md:602](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust-vs-synapse-comparison.md#L602) 称「本仓 `DEFAULT_ROOM_VERSION` 也是 **11**」；但同文件 [:1142](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust-vs-synapse-comparison.md#L1142) 与 [:1122](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docs/synapse-rust-vs-synapse-comparison.md#L1122) 正确记为 **12**；代码真值 `room_versions.rs:94 = "12"`。L602 为过时遗留句。

#### F.4【P2】`.worktrees/` 未被忽略，工作区卫生问题 ✅

- [.gitignore:135](file:///Users/ljf/Desktop/hu_ts/synapse-rust/.gitignore#L135) 仅忽略 `.claude/worktrees/`；仓库根实际存在 `.worktrees/{c19b,relations-split,verify-b08987df5}`（含完整副本、`Dockerfile`、`.env.example`、脚本），存在被误提交/污染 grep 面的风险。建议加入 `.worktrees/`。

#### F.5【P3】其他文档计数/快照轻微漂移 ◐

- LEGACY 报告内部计数不一致（:23 的 31 vs :7 的 36）；`comparison.md:602` 对 `room_versions.rs:77-81` 注释的引用需重新核对。

### G. 配置与部署

#### G.1【P1】Dockerfile 以环境变量覆盖 Cargo profile，使 C13 修复在生产镜像失效 ✅

- [docker/Dockerfile:38](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docker/Dockerfile#L28-L39)：`CARGO_PROFILE_RELEASE_PANIC=abort`
- 覆盖 [Cargo.toml:179](file:///Users/ljf/Desktop/hu_ts/synapse-rust/Cargo.toml#L176-L179)：`panic = "unwind"`（C13 明确注释「保持 unwind，不用 abort」）
- **机制**：`CARGO_PROFILE_*` 环境变量优先级**高于** manifest。
- **影响**：生产镜像中任何第三方库 panic（含 §四.C.1/C.2 可远端触发的两处）将**直接 abort 整个进程**而非仅失败单个请求 → 可用性风险显著放大。
- **建议方案**：删除 Dockerfile 中的 `CARGO_PROFILE_RELEASE_PANIC=abort`，或将其改为 `unwind` 与 manifest 对齐。

#### G.2【P1】生产栈 homeserver.yaml 开放注册 + 关闭验证码 ✅

- [docker/config/homeserver.yaml:19-20](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docker/config/homeserver.yaml#L19-L20)：`enable_registration: true` + `enable_registration_captcha: false`
- **影响**：若该文件随部署栈挂载生效，任何人都可无限制注册账号（垃圾账号/滥用/资源耗尽）。
- **建议方案**：默认置为 `false`，仅在显式开启注册的部署中以 env 覆盖；生产文档强调默认关闭。

#### G.3【P1】OLM_PICKLE_KEY 被代码强制，但部署链路缺失 ✅

- **代码强制**：[synapse-e2ee/src/olm/service.rs:29](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-e2ee/src/olm/service.rs#L20-L36)（缺失/非法即报 `E-06`）；校验器 `scripts/validate_config.sh:11` 已列入。
- **链路缺口**：[docker/deploy/scripts/generate-secrets.sh](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docker/deploy/scripts/generate-secrets.sh) **不生成** `OLM_PICKLE_KEY`；根 [.env.example](file:///Users/ljf/Desktop/hu_ts/synapse-rust/.env.example) 与 [docker/deploy/.env.example](file:///Users/ljf/Desktop/hu_ts/synapse-rust/docker/deploy/.env.example) 均**未包含**该键（仅 `scripts/generate_env.sh:8` 输出）。
- **影响**：按官方部署脚本走完流程仍会缺失该密钥 → OLM 持久化失败（fail loudly，但部署即失败）。
- **建议方案**：`generate-secrets.sh` 生成并写入 `OLM_PICKLE_KEY`，两份 `.env.example` 补齐占位。

#### G.4【P2】监控 Compose 陈旧损坏 ◐

- `docker/docker-compose.monitoring.yml`：卷路径不存在、无效 Prometheus flag、弱口令 `admin123`。
- **建议方案**：修正卷路径与 flag；口令改为必填 env 注入。

#### G.5【P2】`validate_config.sh` 变量名不符 + 死链 ◐

- `scripts/validate_config.sh:10-17,59`：校验的变量名与实际 `.env` 键不一致，且引用了不存在的文件路径。

#### G.6【P2】根 `.env.example` 缺 `ADMIN_MFA_*` ✅

- 根 `.env.example` 无 `ADMIN_MFA_REQUIRED`/`ADMIN_MFA_SHARED_SECRET`，而 `docker/deploy/.env.example:52-58` 有；代码默认 `ADMIN_MFA_REQUIRED=true` → 本地/根示例用户易踩默认开启但未配置的坑。

#### G.7【P3】配置漂移与死配置 ◐

- `.env.example:67` `FEDERATION_SIGNING_KEY=disabled` vs `docker/deploy/.env.example:37` `=CHANGE_ME`（默认值冲突）。
- `drift-detection.yml` 中的 `FLYWAY_*` 为死配置（项目不用 Flyway）。
- `schema-health-check.yml` 步骤名标 `v11`，实际 schema 文件为 `v12`。

---

## 五、证伪清单（历史指控不可复现）

以下均为前序报告/子代理提出的怀疑项，经本会话在 HEAD 上取证**证伪或不可复现**，记录以消除后续重复排查成本：

| # | 历史指控 | 取证结论 |
|---|----------|----------|
| 1 | CORS 通配 `*` + 凭据组合放行 | 已由 [validation.rs:97-103](file:///Users/ljf/Desktop/hu_ts/synapse-rust/synapse-common/src/config/validation.rs#L97-L103) 显式拒绝（加载期报错） |
| 2 | CSRF 可绕过 | 中间件对浏览器认证请求校验 Origin（`csrf.rs:99-134`）；未发现可绕过路径 |
| 3 | CSP 存在 XSS 缺口 | `script-src 'self' 'wasm-unsafe-eval'` 等为已知取舍；无 `unsafe-inline`，未发现可利用注入 |
| 4 | SQL 注入 | 全树走 `sqlx` 绑定参数；未发现字符串拼接 SQL |
| 5 | 认证中间件存在未保护端点 | 契约门禁 1153 路由均有装配校验；未发现裸端点 |
| 6 | 非恒定时间密文比较 | 相关比较使用 `constant_time` 语义/哈希比对 |
| 7 | 密钥被日志打印 | 未发现 `OLM_PICKLE_KEY`/token 明文进日志 |
| 8 | 路径遍历 | 媒体/文件路径均经规范化与白名单 |
| 9 | 限流缺失 | Rate Limiting 中间件在册 |
| 10 | 事件内容未净化 | 有 sanitizer 路径（其**实现缺陷**另见 §四.C.2） |
| 11 | `client_room_versions` capability 缺版本 | 实为遍历 v1–v12，契约快照实证 |
| 12 | 依赖存在已知 CVE | `cargo audit` exit 0，624 crate 无漏洞 |

（其余子代理证伪项同源，归并如上。）

---

## 六、风险分析与建议处置顺序

### 6.1 风险矩阵

| 编号 | 问题 | 影响面 | 触发难度 | 综合风险 |
|------|------|--------|----------|----------|
| A.1 | C10 admin role 回归 + 存量 admin 被锁 | 管理面全停 / 升级事故 | 高（升级即触发） | **最高** |
| C.1 | 媒体魔数越界 panic | 服务可用性 | 低（任意小文件上传） | **高** |
| C.2 | sanitizer 非边界切片 panic | 服务可用性 | 低（含多字节内容） | **高** |
| G.1 | abort 覆盖 unwind | 放大 C.1/C.2 为整进程崩溃 | 条件性 | **高** |
| A.2 | join/leave 无事务 | 数据一致性 | 中（依赖故障时序） | **高** |
| G.2 | 开放注册 | 滥用/资源耗尽 | 高（部署配置） | 高 |
| G.3 | OLM_PICKLE_KEY 部署缺失 | 部署即失败 | 高（按官方流程即触发） | 高 |
| A.3 | typing EDU 不清除 | 体验错误 | 常态化 | 中 |
| B.1/B.2 | 统计/摘要 O(N²) | 大房间延迟 | 规模化后 | 中 |
| E.2 | 静默吞错/过滤 | 功能正确性、可观测性 | 中 | 中 |
| F.1/F.2 | 文档计数矛盾 | 认知/门禁可信度 | 常态 | 中 |
| 其余 P3 | — | — | — | 低 |

### 6.2 建议处置顺序

**第一批（阻断性，应立即修）**
1. **A.1** — 修 fixture 设 `user_type`，并**配套存量 admin 的 `user_type` 回填迁移**（缺迁移则不得合并）。
2. **C.1 / C.2** — 两处 panic 修复（一行级改动，收益极高）。
3. **G.1** — 移除 Dockerfile 的 `PANIC=abort` 覆盖，恢复 `unwind`（配合 C.1/C.2 形成纵深防御）。

**第二批（部署安全与完整性）**
4. **G.2**（注册默认关闭）、**G.3**（补齐 OLM_PICKLE_KEY 部署链）、**A.2**（join/leave 事务化）。

**第三批（正确性与性能）**
5. **A.3 / A.4 / A.5 / A.6 / A.7**、**B.1 / B.2 / B.3 / B.4**、**E.2**。

**第四批（文档与卫生）**
6. **F.1 / F.2 / F.3 / F.4**（含修复 `gen_contract_doc.py` 硬编码，使文档随真值重算）、**E.1**（守卫去误报）、G.4–G.7。

**基建（独立立项）**
7. **集成测试可行性**：1525 测试 × ~30s/测试 ⇒ 全量 ~3h，无法进入常规 CI。建议推进 schema 复用/并行隔离，或按模块分片为多个测试二进制，使全量可在可接受时间内回归。

---

## 七、附录：本轮实测命令与输出摘要

```bash
# 静态门禁（全绿）
cargo clippy --workspace --all-targets --features test-utils --all-features -- -D warnings   # exit 0
cargo fmt --all --check                                                                       # clean
cargo audit                                                                                   # exit 0, 0 vulnerabilities (624 crates)
cargo tree -d --workspace                                                                     # 54 组重复，无回归

# 单元测试（1 失败 = 守卫误报）
cargo test --test unit
#   FAIL ci_test_scope_tests::every_db_test_binary_registers_the_exit_drain
#   → 误报源：synapse-common/src/config/validation.rs:99 字符串字面量含 "unsafe"

# 集成测试（代表性子集，连 Test DB）
TEST_DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test \
SQLX_OFFLINE=true cargo nextest run --test integration \
  --features test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications \
  --no-fail-fast --test-threads 4 -E 'test(/api_room_summary_routes_tests::|.../)'
# Summary [404.451s] 53 tests run: 52 passed, 1 failed, 1480 skipped
#   FAIL api_room_summary_routes_tests::test_room_summary_batch_excludes_rooms_the_caller_cannot_see
#   panicked at tests/integration/api_room_summary_routes_tests.rs:535:5:
#   admin summary creation must succeed: 403 Forbidden      → C10 回归（见 §四.A.1）
```

> 注：nextest 过滤器须用 `test(/模块路径::/)` 而非 `binary(...)`——`tests/integration/*.rs` 编译为**单一 `integration` 二进制**，不存在同名 binary。

---

*报告结束 · Round 3 · 基线 `75902a835` · 2026-10-04*
