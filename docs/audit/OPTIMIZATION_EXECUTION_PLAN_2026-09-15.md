# 优化执行方案（合并版）

- 日期：2026-09-15
- 输入：`ARCHITECTURE_REMEDIATION_ROADMAP_2026-09-15.md`（架构层 A1–A12 路线图）
  ＋ `PROJECT_ACTUAL_ISSUES_2026-09-14.md`（33 篇 audit × 代码复核总清单 §1–§8）
- 基线：`main` @ `2dff2f3d`（本方案所有数字均在此 HEAD 上**实测复核**，非引用文档）
- 口径：文档自身的 ✅/🔴 标记不可信（第二轮复核已验证多处假 ✅）。本方案每条都带**可复现命令**，
  执行时以命令输出为准。

---

## 0. 基线实测（必须先读）

### 0.1 工作树不是干净的 —— 3 个门禁同时为红

| 门禁 | 命令 | 实测 |
|---|---|---|
| 路由契约 | `bash scripts/contract/check_route_contract.sh` | **EXIT=1**（`PUT /_synapse/admin/v1/push/config` 未反映进 `ROUTE_CONTRACT.md`） |
| SQLx 动态比 | `bash scripts/ci/check_sqlx_dynamic_ratio.sh` | **EXIT=1**（1 项棘轮违规） |
| fmt 棘轮 | `./scripts/check_fmt_ratchet.sh` | **EXIT=1**（`synapse-storage/src/room/mod.rs:1066`） |

原因：工作树有 **39 个未提交文件**，是一批**未完成**的改动（最后写入 08:57，本方案核验时 09:04），
内容与路线图 0-2、0-7 同源：

- `0-2 room_directory 单写`：`RoomStorage::set_room_directory(false)` 由 `UPSERT is_public=false`
  改为 `DELETE`（对齐 admin 路径），并加了 db_test（`room/mod.rs` +50/−12）
- `0-7 docker 双目录收敛`（部分）：`docker/deploy/config/` **已删**，故 `scripts/check_config_consistency.py`
  与其测试 `tests/unit/config_consistency_gate_tests.rs` 一并删除、`ci.yml` 移除该步骤
- 其余：rate_limit 配置、push 路由测试、ledger fixture 微调

**另有 9 个 stash**，其中 `stash@{0}` 标注 `redundancy-audit WIP (separate session, do not lose)`，
且存在陈旧 worktree `.claude/worktrees/optimization+audit-2026-07` @ `488d9888`。
→ 执行前必须先处置（见 §1 决策项 D1）。

### 0.2 代码体量与结构（实测）

| 指标 | 实测值 | 说明 |
|---|---|---|
| `src/` 总行数 | **64,479** | 其中 `src/web/` = **56,213（87.2%）** |
| `src/web/routes/**` 引用 `synapse_storage` | **48 个文件** | A2 分层穿透 |
| `_route_manifest` 出现 | **68 个文件**；`assembly.rs` 内 **54 处** | A3 手抄投影 |
| `.route(` 注册点 | **979 处** | A3 唯一真相源 |
| `*NEST_PREFIXES` 常量 | **11 个**（roadmap 记 8，实测 11） | A7 版本扇出 |
| `trait *StoreApi` | **68 个** | A5 无收益抽象 |
| `src/web/routes/context.rs` | **1,000 行** | A4 DI 手工复制 |
| `impl …FromRequestParts`（`src/web/`） | **22 处** | A4 |
| `map_err(`（`src/` + `synapse-services/`） | **1,385 处** | A9（含非 `database_with_cause` 形态） |
| `migrations/*.sql` | **2 个**，`.undo.sql` **0 个** | §1.4/§1.5 的 20 个对象仍缺 |
| `#[allow(dead_code)]` | **22 处** | H-13（文档记 31，已过时） |

### 0.3 仍为红/失效的门禁（实测）

| 编号 | 实测证据 | 状态 |
|---|---|---|
| 2.1 覆盖率基线永远无法提交 | `.gitignore:44 artifacts/` ＋ `:45 !artifacts/coverage_baseline.json`，但 `git check-ignore -v artifacts/coverage_baseline.json` → **`.gitignore:44:artifacts/`**。git 不允许反向包含"被排除目录内的文件"，故 `:45` 的例外**无效**；`ci.yml:852` 的 `git add` 仍会失败 | **未修**（此前一轮记为"已修"是**假修复**） |
| 2.2 性能门禁纯 echo | `drift-detection.yml:346` `echo "::warning::Performance-baseline gate removed: …"`，仍先造 1000 万行 | **未修** |
| 2.3 db-migration-gate 占位 | `grep -c placeholder` = **20** | 部分（echo 已注释，语义仍是占位） |
| 2.5 集成测试静默跳过 | `grep -rn "integration test database is not available" tests/` → **18** | **未修** |
| 2.6 SQLx 棘轮 | EXIT=1 | **未修** |
| T-6 CI 指向应用库 | `ci.yml` 5 处 `TEST_DATABASE_URL=…:5432/synapse`（`:559/:577/:589/:598/:789`） | **未修** |

> ⚠️ 方法论教训（写进本方案，避免重复踩）：`grep 'a\|b'` 在本沙箱 zsh 下**静默返回空**
> （BRE 不支持 `\|`）；`grep -rn "x" --include=*.rs` 会被 zsh **展开 glob 后报错**。
> 检索一律用**专用 Grep 工具**或 `grep -E` / 加引号 `--include='*.rs'`。

---

## 1. 必须先裁定的决策项

> 这 3 项不是技术问题，是**产品/运维裁定**。不定则对应批次无法启动；其余批次可在裁定前先跑。

| ID | 决策 | 建议 | 影响批次 |
|---|---|---|---|
| **D1** | 未提交的 39 文件改动如何处置？ | **推荐"续做并收口"**：补 fmt、重生成 `ROUTE_CONTRACT.md`、说明 sqlx 基线，把 3 门禁转绿后提交；**不要 stash/discard**（会丢 0-2/0-7 的已完成工作） | B0 |
| **D2** | `docker/deploy/` 18GB 备份（19 份）如何处置？ | 需用户确认后**移出版本工作区**（不自动删）；目录收敛已在未提交改动中部分完成 | B0-7 |
| **D3** | MSC4108 `DELETE` 补头 + threepid 孤儿路由（S-9/S-12）是否本轮处理？ | 建议**本轮处理**：都是几十行的确定性修复，且 S-12 已有 7/10 缺 3 的明确差距 | B5 |

---

## 2. 遗留问题总账（合并两张表，去重后）

### 2.1 P0 —— 正确性 / 门禁诚信

| ID | 问题 | 来源 | 状态 |
|---|---|---|---|
| P0-1 | 3 门禁红（contract/sqlx/fmt）由未提交树引入 | 本方案实测 | **待收口** |
| P0-2 | 覆盖率基线 gitignore 例外无效（假修复） | §2.1 | **待修** |
| P0-3 | 18 处集成测试静默跳过 → ledger 端到端链从未在无 DB 下验证 | §2.5 | **待修** |
| P0-4 | CI 5 处集成测试指向应用库 `synapse` 且未 pin `TEST_DB_TEMPLATE_SCHEMA` | §2.6/T-6 | **待修** |
| P0-5 | §1.4 数据完整性约束缺失（FK/CHECK/UNIQUE 共 7 类） | §1.4 | **待折入** |
| P0-6 | §1.5 热点索引缺失（10 个，部分无等价物） | §1.5 | **待折入** |
| P0-7 | baseline 自身违反单一真相源：5 对重复索引 + 3 处硬编码 `public` | §1.6 | **待修** |
| P0-8 | 模板构建吞错（T-2）/ 根模板无条件清空 public（T-1） | §4 | **待修** |
| P0-9 | 性能门禁纯 echo（2.2）、db-migration-gate 20 处占位（2.3） | §2 | **待修** |

> 已修不再列：F-1/F-2/F-3（burn `42P10`、v12/v13 CHECK、audit append-only）、
> P-1..P-4（自建推送链 4 缺陷）、G-1（契约文档 921→918）、0-1/0-3/0-4/0-5/0-6/0-8。

### 2.2 P1 —— 架构冗余（A 系列）

| ID | 问题 | 实测 | 归属阶段 |
|---|---|---|---|
| A2 | 路由 48 文件穿透分层直连 storage | 48 | 阶段 3c |
| A3 | 68 文件手抄 `*_route_manifest()`；979 `.route()` vs 多份投影 | 68 / 979 | **阶段 1b** |
| A4 | DI 四层手工复制；`context.rs` 1,000 行；22 个 `FromRequestParts` | 1,000 / 22 | 阶段 3b |
| A5 | 68 个 `*StoreApi`，绝大多数零多态价值 | 68 | 阶段 3a |
| A6 | 列级清单手工维护 3 份 + schema-blind 守卫复发 | `schema_health_check.rs` / `schema_validator.rs` 均在 | 阶段 2a |
| A7 | 11 个 `*NEST_PREFIXES` 版本扇出 ×3 | 11 | **阶段 1a（最高收益）** |
| A9 | 1,385 处 `map_err(` 样板（无跨层转换契约） | 1,385 | 阶段 2b |
| A10 | 通配 re-export + `allow(ambiguous_glob_reexports)` | 见 roadmap | 阶段 3d |
| A11 | `deny(missing_docs)` 逼出零信息注释 | 见 roadmap | 阶段 4 |
| A1 | 根 crate 非薄壳：`src/web` 占 87.2% | 56,213/64,479 | 阶段 3d（最后） |

### 2.3 P2 —— 安全 / 协议 / 卫生

| ID | 问题 | 归属批次 |
|---|---|---|
| S-1 | `query_server_keys` 不校验自签名 | B5 |
| S-2 | `get_server_keys` 不校验 `server_name == destination` | B5 |
| S-4 | `quarantined_media_changes` 无界增长（全仓 0 条 DELETE） | B5 |
| S-9 | threepid 路由孤儿（已进契约文档、未进 ledger） | B5（D3） |
| S-12 | MSC4108 `DELETE` 204 缺 3 个 required 头 | B5（D3） |
| S-14 | 无测试校验"真实 router == ledger" | B3（与 A3 同批，正是 A3 的验证手段） |
| S-15 | MSC4108 响应头测试自证（构造本地数组断言） | B5 |
| H-5 | 陈旧 worktree `.claude/worktrees/optimization+audit-2026-07` | B0 |
| H-9 | `cargo doc` ~3,500 条 intra-doc 警告 | B6 |
| H-10 | god-file `friend_room_service/mod.rs` 1,833 行 | B6 |
| H-11 | mock 与 PG 语义漂移 2 处 | B6 |
| H-12 | `IsolatedTestPool` fallback 仍首选 `localhost:15432` | B0 |
| H-14 | `docker/db_migrate.sh` 优先宿主 `psql`，会改非 compose 栈的库 | B0（高危） |

---

## 3. 执行批次（按依赖顺序，每批可独立提交）

### B0 · 收口与止血（零/低行为变更）

> 目标：先把**红门禁清零**，让后续所有批次都有可信的安全网。

| # | 改动 | 验证 |
|---|---|---|
| B0-1 | **收口未提交树**（D1）：`cargo fmt --all` 修 `room/mod.rs:1066`；`bash scripts/contract/check_route_contract.sh` 重生成契约文档；`check_sqlx_dynamic_ratio.sh` 若为合理新增则下调 baseline 并写明理由 | 三命令 **EXIT=0**；`cargo check --workspace --locked` |
| B0-2 | **覆盖率基线真修**：`.gitignore` 的 `artifacts/` 改为 `artifacts/*`（保留 `!artifacts/coverage_baseline.json`），使例外生效 | `git check-ignore -v artifacts/coverage_baseline.json` **无输出**（未被忽略）；`git add --dry-run` 成功 |
| B0-3 | **CI 集成测试指向修正**：`ci.yml:559/577/589/598/789` 的 `…/synapse` → `…/synapse_test`，并补 `TEST_DB_TEMPLATE_SCHEMA=test_template_ci` | `grep -c 'TEST_DATABASE_URL.*:5432/synapse$'` = 0 |
| B0-4 | **静默跳过 fail-closed**：18 处 `Skipping: …; return;` 改为 `panic!`（当 `SYNAPSE_TEST_REQUIRE_DB=1`），CI 置该变量 | 本地不置变量仍可跳；CI 置变量后故意不建库 → **红**（铁律 8 自证） |
| B0-5 | **性能门禁去 echo**：`drift-detection.yml` 的性能 job 要么给出真实断言（迁移耗时/行数阈值），要么**整体删除**（省掉 1000 万行造数） | 删除后 workflow 语法通过；保留则故意注入劣化 → 红 |
| B0-6 | **db-migration-gate 20 处占位**：逐条实现或删除，不留 `placeholder` 字样 | `grep -c placeholder` = 0；故意注入违规迁移 → 红 |
| B0-7 | **H-14 高危**：`docker/db_migrate.sh` 优先宿主 `psql` 的逻辑加护栏（非 compose 栈目标时拒绝执行 / 显式 `--allow-host-psql`） | 在宿主 5432 上跑 `validate` → **拒绝**而非建库 |
| B0-8 | **H-5 / H-12**：`git worktree remove` 陈旧副本（先确认其分支 3 提交是否已并入）；`IsolatedTestPool` fallback 端口更新 | `git worktree list` = 1 条；该文件 env-first 行为不变 |
| B0-9 | **D2**：`docker/deploy/` 18GB 备份移出版本工作区（**需用户确认，不自动删**） | `du -sh docker/deploy` 回落 |

**批次验证门**：`cargo check --workspace --locked` + `./scripts/check_fmt_ratchet.sh` + clippy `-D warnings`
＋ B0-2/B0-4/B0-6 各自"能变红"证明。

---

### B1 · 拆 r0 兼容链（A7，A3 的前置乘数）

> 先拆乘数再建单源管道，否则 489 条重复会被固化进新管道。**这是唯一的对外契约变更窗口。**

| # | 改动 | 验证 |
|---|---|---|
| B1-1 | **盘点**：从 ledger 快照导出全量条目 → 按 `(method, suffix)` 去重 → 对每条重复确认 handler 同源 | 输出 `总条目 vs 去重条目 vs 重复数` 三方核对表 |
| B1-2 | **SDK 字面路径复核**：grep `@langkebo/matrix-js-sdk` **manager 源码**里的实际 URL 字面量（**不得**用 route-table 作判据——它只增不减） | 产出一份 `被 SDK 真实调用的 r0 端点清单`（预期为空） |
| B1-3 | **去 r0**：11 个 `*NEST_PREFIXES` 删除 `/_matrix/client/r0`；`assembly.rs` 的 r0 nest；`create_*_r0_only_router` 合并回正 router；删除 `suppress_r0_deprecation_warning` / `suppress_vendor_endpoint_warning` 配置字段 + `homeserver.yaml` 条目 | `grep -rn '"/_matrix/client/r0"' src/` = 0；ledger diff **恰为 −489** |
| B1-4 | **契约文档 + fixture 重基线**（一次性） | 人工抽查 admin/client 两域各 20 条；附哈希变化审计记录 |

**风险**：Complement 测试套若断言 r0 可达需同步；SDK 端到端冒烟（登录/sync/发消息）必须跑一次。

#### B1-1 / B1-2 前置核查结论（2026-09-15 实测，**推翻了本节的两条假设**）

**假设一被推翻：r0 不是 489 条，且不全是重复。**

数据源：`tests/unit/fixtures/ledger_export/{all,default,worker}.json`（去重键 = `method` + 版本剥离后的路径）。

| 快照 | 总条目 | r0 条目 | 非 r0 | client 域唯一 `(method,suffix)` | 冗余条目 | **r0-only（无 v3/v1 对应）** |
|---|---|---|---|---|---|---|
| `all.json` | 1319 | **283** | 1036 | 487 | 401 | **13** |
| `default.json` | 1294 | 276 | 1018 | 480 | 394 | 13 |
| `worker.json` | 1305 | 276 | 1029 | 480 | 394 | 13 |

即：删 r0 的收益是 **−270 条重复**（不是 −489）；而代价是 **13 个端点会整体消失**（它们不是兼容副本，是只挂在 r0 上的主端点）：

| 端点 | 注册处 | 性质 |
|---|---|---|
| `GET /r0/account/profile/{user_id}` | `assembly::account_r0_only` | 自定义路径（规格为 `/profile/{userId}`），r0-only |
| `PUT /r0/account/profile/{user_id}/displayname` | 同上 | 同上 |
| `PUT /r0/account/profile/{user_id}/avatar_url` | 同上 | 同上 |
| `GET /r0/directory/room/{room_id}/alias` | `assembly::directory_r0_only` | 自定义路径（规格为 `/directory/room/{roomAlias}`），r0-only |
| `PUT /r0/directory/room/{room_id}/alias/{room_alias}` | 同上 | 同上 |
| `DELETE /r0/directory/room/{room_id}/alias/{room_alias}` | 同上 | 同上 |
| `GET|POST /r0/friendships` | `friend_room` | 遗留别名（正路由为 `/_matrix/vendor/v1/friends`） |
| `GET|POST /r0/push/devices`、`DELETE /r0/push/devices/{id}`、`POST /r0/push/send` | `push_notification` | 无任何 v1/v3/vendor 对应 |
| `POST /r0/rooms/{room_id}/get_membership_events` | `room` | **SDK 正在调用**（见下） |

**假设二被推翻：`被 SDK 真实调用的 r0 端点清单` 不为空。**

对 `matrix-js-sdk`（fork）manager 源码逐处核实（**未使用 route-table 作判据**）：

| SDK 位置 | 路径 | 前缀 | 是否无条件 |
|---|---|---|---|
| `src/room-member/index.ts:159` | `POST /rooms/{roomId}/get_membership_events` | `ClientPrefix.R0` | **是** |
| `src/saml/index.ts:91,109,126,144,157,168,178,188` | `login/sso/redirect/saml`、`login/saml/callback` ×2、`logout/saml`、`logout/saml/callback`、`saml/metadata`、`saml/sp_metadata` | `ClientPrefix.R0` | **是**（8 处） |
| `src/e2ee/index.ts:359` | `sendToDevice` 版本映射表含 `r0: ClientPrefix.R0` | 参数化（默认 `v3`） | 否 |
| `src/captcha/index.ts:46` | `captchaPrefix("r0")` | 参数化（默认 `v3`） | 否 |
| `src/verification/index.ts:33` | `verificationPrefix("r0")` | 参数化（默认 `v1`） | 否 |

另：`Tjg/src/services/matrix/auth/MatrixAuthSaml.ts:59` 的 `samlLogout()` 走 `manager.logout()` → `ClientPrefix.R0` → `/_matrix/client/r0/logout/saml`，而 backend 的 `saml.rs` **只把 `/logout/saml` 与 `/logout/saml/callback` 注册在 r0**（v3 只有 5 条、缺这 2 条）→ **前端现有功能直接依赖 r0-only 端点**。

> 注：`matrix-js-sdk/src/friend/__generated__/route-table.ts` 里的 324 条 r0 路径是 codegen「既有条目 ∪ ledger」的残留，**不是调用证据**——friend 模块实际走 `/_matrix/vendor/v1/friends/*`（`src/friend/paths.ts` + `sub-managers/*`）。本节结论与项目记忆一致：**route-table 只增不减，不能作为实际调用判据。**

**推论**：B1-3「`grep -rn '"/_matrix/client/r0"' src/` = 0 且 ledger diff 恰为 −489」在**不修改 SDK**的前提下不可达，强推会让 SAML 登出、`get_membership_events` 等现网路径 404。B1 必须二选一：**后端 + SDK 同批迁移**，或**只拆零调用部分**。

---

### B2 · 路由元数据单一真相源（A3 核心）

| # | 改动 | 验证 |
|---|---|---|
| B2-1 | **派生器**：实现 `RouteLedger::from_router(router)`，在 `main.rs` 装配完成后从 Router 一次性导出；保留 `RouteModule` trait 作为 feature→路由的**声明**机制，其 manifest 项由派生填充 | 派生条目数 **== 旧 manifest 并集数**（一次性对账） |
| B2-2 | **删手抄**：删除 68 个文件的 `*_route_manifest()`（`assembly.rs` 内 54 处引用随之消失）；手写点只剩 979 处 `.route()` | `grep -rl '_route_manifest' src/` = 0 |
| B2-3 | **幂等守卫**：同一二进制启动两次导出的 ledger **逐字节相等** | 新增测试，故意引入 `HashMap` 迭代序 → 红 |
| B2-4a ✅ | **正向契约守卫（S-14）**：断言"真实 router ⊆ ledger"（即不存在已服务却未登记的端点） | 已完成：先量出真实缺口 —— 以 golden + sdk 两条 fixture 泳道的**并集**为完备性 oracle，`derived \ ledger` 实测 **22 条**（原先按 golden 单泳道算是 102 条，其中 80 条是 feature 门控噪声，把真缺口埋掉了）。22 条逐条核实为真后全部补进所属 manifest；现 `derived \ ledger = 0`、`ledger \ derived = 0` 双向闭合，并在 `EXTRACT_STRICT=1` 下成为硬门禁（原实现把这组差集**只打印不拦截**——见原注释"reports rather than enforced"）。守卫测试新增 `check_positive_contract`（含"谓词非空转"自检）。**附带**：同一工作窗内发现并修复了 S-16 —— `tests/integration/snapshots/route_ledger_{default,worker_enabled}.snapshot` 是**手改而非重生成**的（1378 行含 250 条完全重复、22 条生产上 404 的 v3 friends 声明），该集成快照用例在 `main` 上本来就**是红的**；已带库重生成至 1127/1138 并逐项对账闭合（见 `PROJECT_ACTUAL_ISSUES §13`）。**剩余**：B2-4b 的"SDK 声明消费的全部端点 ⊆ ledger"需解析 SDK manager 源码字面量，另立条目 |
| B2-4b | **SDK 侧正向守卫**：断言 SDK manager 源码里实际调用的端点 ⊇ 已被 ledger 覆盖 | 见 B2-4a 备注；需从 `matrix-js-sdk` fork 的 manager 源码提取 URL 字面量（**不得**用 route-table，它只增不减） |
| B2-5 | **投影去 tracked**：`docs/openapi/client.yaml`（72,924 行）移出索引 → 由同管道生成到 CI artifact；`route-table.json` 加"生成物禁止手改"头注释 | CI 生成 job 成功；diff 只反映 B1 的 r0 拆除 |

**过渡态**：若 axum 版本无法枚举 nest 前缀，退化为"每个 router 构造时 `ledger.register(...)` 增量记录"，
仍删除独立抄写函数。

---

### B3 · schema 单源 + 错误汇流（A6 残留 + A9）

| # | 改动 | 验证 |
|---|---|---|
| B3-1 | **v12 baseline 重生成**：`v11 + extensions` 合并为 `00000000_unified_schema_v12.sql`（脚本生成，非手工编辑） | 两个全新库分别应用 v11→v12 前后，表/列/索引 `EXCEPT` 双向差集为空 |
| B3-2 | **折入 P0-5/P0-6**：7 类完整性约束 + 10 个热点索引写入 v12 | `grep -c` 各对象名 = 1；旧 23 项模拟脚本输出 `ABSENT FROM BASELINE: 0` |
| B3-3 | **清 A6 残留**：`schema_health_check.rs` 的 `CORE_COLUMNS`、`schema_validator.rs` 清单改为由 v12 派生；`v11:4941/5005/5056` 三处硬编码 `public` 改 `current_schema()`；删 5 对重复索引 | schema-blind lint（B0 已建）0 error；非 public schema 下真跑一次 |
| B3-4 | **版本字面名单源**：baseline 文件名收敛到一个常量/脚本变量，`grep -rn 'v11'` 引用点清零 | 引用点 = 0（防下次 v12→v13 再散落） |
| B3-5 | **错误单向汇流（A9）**：为每个 `*Error` 加 `impl From<XError> for ApiError`，删除 `map_err(|e| ApiError::database_with_cause(...))` 样板；不同转换的 HTTP 码差异用**路由级 golden 测试**锁死 | 新增 `error_conversion_tests.rs`：每个 From → 断言 `(status, errcode, body)` |

---

### B4 · 装配与抽象收敛（A5 → A4 → A2 → A1）

> 硬约束：**A5 必须先于 A4**（字段数先降，clone 样板才降得掉）。

| # | 改动 | 验证 |
|---|---|---|
| B4-1 | **A5 trait 收敛**：68 个 `*StoreApi` 分三类 —— (i) 零 `dyn` 的删 trait、消费者用具体类型；(ii) 有 mock 消费者的 trait/impl 合并同文件；(iii) `MediaStorageBackend`/`UserStore` 等真实多实现保留 | 每批 `cargo check --workspace --all-features` + `--lib` 全绿；trait 计数脚本作棘轮（只降不升） |
| B4-2 | **删除前守卫**：每个 trait 删除前 grep `dyn` 与 mock 引用（分类规则保证） | 分类清单存档 |
| B4-3 | **A4 DI 泛型化**：定义 `trait AuthSource`，11 个 context 各 impl；`FromRequestParts<S> where S: AuthSource` 泛型 impl 取代逐字笛卡尔积；context 字段按 B4-1 结果瘦身 | 现有 extractor/context 单测全绿；新增"`AdminUser` 与 `RoomContext` 鉴权行为一致"断言 |
| B4-4 | **A2 分层强制**：CI lint 禁 `src/web/` 下 `use synapse_storage::`（白名单趋向 0，棘轮）；48 个文件补薄 service 或下沉 | 故意加一行 → 红；`grep -rl synapse_storage src/web/routes` 计数下降 |
| B4-5 | **A1 + A10 收口**：`src/web/` 独立为 crate，根 crate 收缩为 bin+wiring（目标 <5k 行）；同期删扁平 `pub use x::*` 与 `allow(ambiguous_glob_reexports)`，import 路径唯一化 | `cargo check --workspace --all-features`；docker 构建矩阵（无 Docker 则显式标注未验证） |

---

### B5 · P2 安全/协议（可穿插，与 B1–B4 无耦合）

| # | 改动 | 验证 |
|---|---|---|
| B5-1 ✅ | **S-1/S-2**：`query_server_keys` 补自签名校验；`get_server_keys` 补 `server_name == destination` 校验 | 已完成：两者收敛到唯一信任门禁 `validate_remote_server_keys`（先名字、后自签名），并把"校验 + 写入缓存"合并为 `admit_server_keys`，使安全属性可直接断言——**未通过校验的文档永远进不了 `key_cache`**。5 条新单测（含经真实 HTTP 响应字节的 `/key/v2/query` 路径）+ 变异自证（摘掉校验后 2 条缓存守卫转红）。`cargo test -p synapse-federation --lib` 187 passed，clippy 干净 |
| B5-2 ✅ | **S-4**：`quarantined_media_changes` 加保留期清理（复用既有 cleanup 骨架） | 已完成：复用 `synapse-storage/src/pruning.rs` 骨架新增 `QUARANTINED_MEDIA_CHANGES_RETENTION_DAYS = 30` + `prune_old_quarantined_media_changes`，并接入 `src/server/mod.rs` 既有定时 prune 循环（**不是**只加函数不调度）。db 测试：40 天前插 3 行、10 天前插 2 行 → 仅 3 行被清，且存活行 `MIN/MAX(created_ts)` 精确校验（防"删对数量但删错行"）。变异自证：比较符反向后转红 |
| B5-3 ✅ | **S-12（D3）**：MSC4108 `DELETE` 204 补 `Last-Modified`/`Cache-Control: no-store`/`Pragma: no-cache` | **重写 S-15 的自证测试**：改为真实调用 handler 断言响应头 —— 已落地（commit `32938763`） |
| B5-4 ✅ | **S-9（D3）**：threepid 孤儿路由 —— 要么补装配点，要么从 `ROUTE_CONTRACT.md` 删除 | 已完成（裁定为**删除代码本身**）：`create_threepid_router` 历史从未被装配（`git log -S` 在装配位置全空）、路径为裸 `/requestToken`（非 Matrix 规范形状）、且是未完成桩（不发信，注释声称返回 token 但响应结构体无该字段）；真实 3PID 端点在 `account_compat.rs` 已装配。删除模块 + 死 re-export + 仅服务于它的 `AuthContext::threepid_storage`。实测 模块 67→66、路由 1148→1146、"前缀之外" 16→14，两份 oracle 仍 0 缺口。守卫由「钉死缺陷」改为「钉死不变量」（精确断言该桶 == 14 条有意根级注册）。详见 `PROJECT_ACTUAL_ISSUES_2026-09-14.md` §11 |
| B5-5 ✅ | **S-13 附带**：`extract_registered.py` 链式方法只取第一个 method 的缺陷（`get().put().delete()` 只提取 GET） | 对 MSC4108 重新提取 → 4 条全出；并一并根治 `nest` 前缀传播、测试块截断、`id()` 缓存复用四个缺陷。18 项守卫 + 变异自证；两份独立 oracle（manifest / ledger_export）均 0 缺口。**超额完成**：S-13 记录的另两项子缺陷也一并消除（相对 `/spaces/...` 15→0、带前缀 0→48） |

---

### B6 · 长期防腐（阶段 4，代码量冻结后再启动）

| # | 改动 |
|---|---|
| B6-1 | **A11**：删除零信息模板注释；`check_missing_docs_ratchet.py` 改为**内容型**（自指 `See [x].` 计违规），基线从 ~15.5k 归零 |
| B6-2 | 测试文件 `mod` 守卫（防 TST-4 复发）；职责级单源扫描（TST-1/2） |
| B6-3 | feature 矩阵真实化（shipped == tested == default；`cargo hack --feature-powerset` 抽样） |
| B6-4 | H-9/H-10/H-11：`cargo doc` 警告清理、god-file 拆分、mock/PG 语义对齐 |

---

## 4. 依赖图与里程碑

```
B0 收口止血 ──┬──> B1 拆 r0（契约变更唯一窗口）──> B2 路由单源 ──> B3 schema v12 + 错误汇流
              └──> B5 安全/协议（独立，可并行）
B3 ──> B4 (A5 → A4 → A2 → A1/A10) ──> B6 防腐门禁
```

**预期收益**（roadmap 口径，B1–B4 完成后）：
净 Rust/SQL/脚本 **−6,000~−9,000 行**；生成物 tracked **−75,000+ 行**；ledger 条目 **−489**；
context 字段 **−40%**；样板 **−1,400 行**（manifest + extractor + map_err，部分重叠）。

---

## 5. 全局风险登记

1. **并发改同一工作树** —— 已实测：9 个 stash（含 `do not lose`）+ 39 文件未提交 + 陈旧 worktree。
   B4 的机械重写必须按特性目录切 PR，避免多会话同时触碰 `wiring/`、`context.rs`。
2. **B1 是唯一可能"伤到" SDK 的窗口** —— 拆前必须按 manager 源码字面路径复核，**不含** route-table。
3. **fixture 大面积重基线会掩盖回归** —— 每次重基线附 diff 审计（哈希 + 抽样 ≥40 条），
   禁止直接 `cargo insta accept`。
4. **无 Docker 环境**导致 B0-7/B4-5 的构建矩阵无法本地验证 —— PR 中**显式标注未验证项**，禁默过。
5. **sqlx 编译期查询 + v12 重生成**连锁 —— `.sqlx` 缓存目录当前未被 gitignore，应补。
6. **本节之外的每个"门禁通过"都不可信** —— 铁律 8：新增/修改的门禁必须用故意违规**自证能变红**。

---

## 6. 一句话总结

本方案 = roadmap 的「先切断信息多源（A7→A3）→ 再收敛手工装配（A5→A4→A9）→ 用编译期分层守卫锁住结构（A2）
→ 最后以 crate 拆分把"薄壳"变成物理事实（A1）」+ 总清单的「P0 正确性/门禁诚信必须先行」两条链的合并；
**B0 先把 3 个红门禁清零**，再按 `B1 → B2 → B3 → B4 → B6` 的单一依赖链推进，B5 可全程并行。
每一条缺陷要么被根因族消除，要么被**能自证变红**的机器守卫永久拦截复发。
