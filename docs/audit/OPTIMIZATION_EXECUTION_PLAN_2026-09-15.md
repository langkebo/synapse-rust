# 优化执行方案（合并版）

- 日期：2026-09-15
- 输入：`ARCHITECTURE_REMEDIATION_ROADMAP_2026-09-15.md`（架构层 A1–A12 路线图）
  ＋ `PROJECT_ACTUAL_ISSUES_2026-09-14.md`（33 篇 audit × 代码复核总清单 §1–§8）
- 基线：`main` @ `2dff2f3d`（本方案所有数字均在此 HEAD 上**实测复核**，非引用文档）
- 口径：文档自身的 ✅/🔴 标记不可信（第二轮复核已验证多处假 ✅）。本方案每条都带**可复现命令**，
  执行时以命令输出为准。

---

## 0. 基线实测（必须先读）

> **本节是 `main @ 2dff2f3d` 时点的基线快照，不是当前状态。** 下面几节的"红"已随 B0/B5/B2-4a
> 落地而清零；要判断当前状态请看 §2 各表的 ✅ 与 §3 各批次的实测列，或直接跑该行给出的命令。

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
| **D4** 🆕 | **B2-1 的处方机制已证不可行，改走哪条路？**（详见下方「B2-1 可行性复核」） | **推荐 B 路**：让已验证的源码提取器成为派生源并按 `#[cfg(feature)]` 建模 profile；A 路（注册期录制）代价 933 处 `.route()` + 还要包 `get()/post()`，且遗漏即"静默不登记"，比现状更危险 | **B2-1 / B2-2 / B2-3** |

---

## 2. 遗留问题总账（合并两张表，去重后）

### 2.1 P0 —— 正确性 / 门禁诚信

| ID | 问题 | 来源 | 状态 |
|---|---|---|---|
| P0-1 ✅ | 3 门禁红（contract/sqlx/fmt）由未提交树引入 | 本方案实测 | **已收口**：实测 contract **EXIT=0**（1146 routes）、sqlx **EXIT=0**（1484/61 棘轮到顶未越）、fmt **EXIT=0**（debt 0/0） |
| P0-2 ✅ | 覆盖率基线 gitignore 例外无效（假修复） | §2.1 | **已真修**：`.gitignore:44` 已是 `artifacts/*`（非 `artifacts/`），`git add --dry-run artifacts/coverage_baseline.json` 成功。注：`git check-ignore -v` 仍打印 `:45:!…` 并 **exit 0**，那是 git 在报告"最后命中的是负向规则"，**不能作为判据** —— 判据是 `git add --dry-run` |
| P0-3 ✅ | 18 处集成测试静默跳过 → ledger 端到端链从未在无 DB 下验证 | §2.5 | **已收口**：18 处 `return` 收敛为唯一入口 `tests/integration/mod.rs::skip_or_fail_without_db()`；`integration_tests_required()` 把 `CI` 视为"必须跑"，故 CI 下缺库即 `panic!` 而非静默通过（全仓仅剩 1 处匹配字符串，位于该函数内） |
| P0-4 ✅ | CI 5 处集成测试指向应用库 `synapse` 且未 pin `TEST_DB_TEMPLATE_SCHEMA` | §2.6/T-6 | **已收口**：`grep -c 'TEST_DATABASE_URL.*:5432/synapse$' .github/workflows/ci.yml` = **0**；`ci.yml:16` 全局 `TEST_DATABASE_URL=…:5432/synapse_test`，各集成步骤另 pin `TEST_DB_TEMPLATE_SCHEMA=test_template_ci` |
| P0-5 | §1.4 数据完整性约束缺失（FK/CHECK/UNIQUE 共 7 类） | §1.4 | **待折入**（B3-2） |
| P0-6 | §1.5 热点索引缺失（10 个，部分无等价物） | §1.5 | **待折入**（B3-2） |
| P0-7 | baseline 自身违反单一真相源：5 对重复索引 + 3 处硬编码 `public` | §1.6 | **待修**（B3-3） |
| P0-8 | 模板构建吞错（T-2）/ 根模板无条件清空 public（T-1） | §4 | **待修**（B3） |
| P0-9 ✅ | 性能门禁纯 echo（2.2）、db-migration-gate 20 处占位（2.3） | §2 | **已修**：`drift-detection.yml` 已无 `Performance-baseline gate removed`；`db-migration-gate.yml` 的 `grep -c placeholder` 从 20 降到 3，且这 3 处是**真实测试文件名**（`api_placeholder_contract_p0/p1p2_tests`，"placeholder contract" 是领域概念，不是空壳步骤），故该门禁已无占位 |

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
| S-1 ✅ | `query_server_keys` 不校验自签名 | B5（见 B5-1） |
| S-2 ✅ | `get_server_keys` 不校验 `server_name == destination` | B5（见 B5-1） |
| S-4 ✅ | `quarantined_media_changes` 无界增长（全仓 0 条 DELETE） | B5（见 B5-2） |
| S-9 ✅ | threepid 路由孤儿（已进契约文档、未进 ledger） | B5（见 B5-4，裁定为删除代码本身） |
| S-12 ✅ | MSC4108 `DELETE` 204 缺 3 个 required 头 | B5（见 B5-3） |
| S-14 ✅ | 无测试校验"真实 router == ledger" | B3 → **已提前到 B2-4a 完成**（22 条缺口闭合 + `EXTRACT_STRICT=1` 硬门禁） |
| S-15 ✅ | MSC4108 响应头测试自证（构造本地数组断言） | B5（见 B5-3，改为真实调用 handler 断言响应头） |
| S-16 ✅ | 集成快照 `route_ledger_*.snapshot` 是手改而非重生成（1127 vs 1378） | B2-4a 附带修复（见 `PROJECT_ACTUAL_ISSUES §13`） |
| H-5 ✅ | 陈旧 worktree `.claude/worktrees/optimization+audit-2026-07` | B0（见 B0-8） |
| H-9 | `cargo doc` ~3,500 条 intra-doc 警告 | B6 |
| H-10 | god-file `friend_room_service/mod.rs` 1,833 行 | B6 |
| H-11 | mock 与 PG 语义漂移 2 处 | B6 |
| H-12 ✅ | `IsolatedTestPool` fallback 仍首选 `localhost:15432` | B0（见 B0-8） |
| H-14 ✅ | `docker/db_migrate.sh` 优先宿主 `psql`，会改非 compose 栈的库 | B0（见 B0-7，已加护栏 + `SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL=1` 显式放行） |
| H-15 🆕 | **`suppress_key_server_warning` 是只写字段（H-12/B1-3 同类的残留）**：声明在**两个**结构体里（`ServerConfig` `server.rs:103` + `FederationConfig` `federation.rs:58`）、`homeserver.yaml:113` 有键、**22 处结构体字面量**在写它，但全仓**无任何读取**（`grep -rn '\.suppress_key_server_warning'` = 0）。同批还发现 `ServerConfig` 另有 12 个字段同样从不以 `.字段` 形式被读取（`expire_access_token`、`serve_server_wellknown`、`soft_file_limit`、`max_image_resolution` 等），成因待判（可能是经统一访问器/反序列化间接消费）。**本条只是记录，不在 B1-3 内夹带处理** | 待办：先判定"间接消费"还是"真死字段"，再统一处置 |
| H-16 🆕 | **SDK 侧死代码：`AccountManager.setGuestAccess` 打非标路径**（B2-4b 实测发现）。`src/account/index.ts:351` 请求 `PUT /rooms/$roomId/guest_access`，**后端无此路由**——Matrix 规范里访客准入只有 state event `m.room.guest_access`，该端点纯属非标。同一 SDK 里另有三条**正确**实现：`RoomManager.setGuestAccess`（`RoomManager.ts:1106`，走 state event）、`client.setGuestAccess`（`client.ts:2914`，委托 RoomManager）、`client-room-access.ts:15`（同样委托 state event）。即 `client` 门面已指向正确实现，AccountManager 那份是**不可达的重复实现**。已确认 Tjg 前端未调用（`grep -rn 'setGuestAccess' Tjg/src` 只命中无关的 `getGuestAccessToken`） | 待办：**SDK fork 侧删除该方法**（约 12 行，无调用方）。**不要**为它开后端路由——那等于往 v3 命名空间塞私有路径，正与 ISSUE-13 相悖。删除后 `sdk_uncovered_allowlist.txt` 对应条目会变 stale，检查器会提醒删掉 |
| H-17 🆕 | **后端缺规范端点：`GET /_matrix/client/v3/auth/{authType}/fallback/web`**（B2-4b 实测发现）。SDK `getFallbackAuthUrl`（`src/account/index.ts:290`）拼 `/auth/$loginType/fallback/web` 交给 `http.getUrl()`，后者用客户端默认前缀 `ClientPrefix.V3`（`src/client.ts:841`）→ 实际 URL 就是 C-S 规范定义的 fallback 认证页面路径。后端只在 `assembly.rs:588` 注册了 `/_matrix/static/client/login/`（handler `auth_compat::login_fallback_page`，**已在 ledger 中登记**），全仓 `grep 'route("/...fallback'` = 0，即规范路径**确实不存在**。**判断**：SDK 是对的、后端是缺的；不是"后端多出端点"（那条 B2-4a 已守着） | 待办：实现该端点（需一张 HTML 页 + session 承接逻辑，属**功能开发**而非契约守卫范围；且前端登录流程目前不走 fallback 认证，不阻塞）。**不要**让 SDK 改调 `/_matrix/static/client/login/`——那是免检的静态页位置，不是客户端该硬编码的契约路径。实现后删除对应豁免条目 |
| H-18 🆕 | **`ProfileFlags::saml_enabled` 从不被读取**（B2-1 §3.4 实测发现）。三个字段里 `oidc_enabled`（`route_module.rs:199`）与 `worker_enabled`（`:236`）都在 `manifest_for_profile` 里被读，`saml_enabled`（`:37` 声明、`:52` 由 `from_state` 写入、`ledger_export.rs:113-114` 用于构造 profile、`ledger_export.rs:273-274` 有断言）**没有任何 `manifest_for_profile` 读它**。原因是 SAML 路由本就经 `oidc::oidc_enabled()`（`oidc/mod.rs:154`：`oidc_service.is_some() \|\| builtin_oidc_provider.is_some() \|\| saml_enabled`）折进了 `oidc_enabled` —— 所以 profile 维度上"开 SAML"等价于"开 OIDC"，独立字段是冗余的。**注意与 H-15 的区别**：H-18 不是"死字段"（它确实影响 SAML 路由是否随 OIDC 一起出现，只是通过间接路径），而是**新增了一个字段却没有任何消费点直接读它**这一表象 | 待办：与 H-15 一批处理。可选：在 `ProfileFlags` 文档里写明"`saml_enabled` 只作为 `from_state` 的输入、经 `oidc_enabled` 间接生效"，或把 `oidc_enabled` 拆成 `oidc_enabled \|\| saml_enabled` 让语义显式。**不要**在没有消费点的情况下继续新增 profile 字段 |

---

## 3. 执行批次（按依赖顺序，每批可独立提交）

### B0 · 收口与止血（零/低行为变更）

> 目标：先把**红门禁清零**，让后续所有批次都有可信的安全网。

| # | 改动 | 验证 |
|---|---|---|
| B0-1 ✅ | **收口未提交树**（D1）：`cargo fmt --all` 修 `room/mod.rs:1066`；`bash scripts/contract/check_route_contract.sh` 重生成契约文档；`check_sqlx_dynamic_ratio.sh` 若为合理新增则下调 baseline 并写明理由 | 三命令 **EXIT=0** ✅ 实测：contract 0（1146 routes）、sqlx 0（1484/61）、fmt 0（debt 0/0）；`cargo check --workspace --all-features --locked` 通过 |
| B0-2 ✅ | **覆盖率基线真修**：`.gitignore` 的 `artifacts/` 改为 `artifacts/*`（保留 `!artifacts/coverage_baseline.json`），使例外生效 | 实测 `.gitignore:44` = `artifacts/*`，`git add --dry-run artifacts/coverage_baseline.json` **成功**。⚠️ 原定判据 `git check-ignore -v` **无输出**是错的：命中负向规则时它照样打印并 **exit 0**，判据只能用 `git add --dry-run` |
| B0-3 ✅ | **CI 集成测试指向修正**：`ci.yml:559/577/589/598/789` 的 `…/synapse` → `…/synapse_test`，并补 `TEST_DB_TEMPLATE_SCHEMA=test_template_ci` | `grep -c 'TEST_DATABASE_URL.*:5432/synapse$' .github/workflows/ci.yml` = **0** ✅；`ci.yml:16` 另有全局 `TEST_DATABASE_URL=…:5432/synapse_test`，各集成步骤均 pin 模板名 |
| B0-4 ✅ | **静默跳过 fail-closed**：18 处 `Skipping: …; return;` 改为 `panic!` | 实测：18 处已收敛为唯一入口 `tests/integration/mod.rs::skip_or_fail_without_db()`，全仓仅剩 1 处匹配字符串（就在该函数内）。⚠️ 实现**没有**引入 `SYNAPSE_TEST_REQUIRE_DB`，而是复用既有 `integration_tests_required()`（`CI` 存在即视为必须跑）—— 更符合"一个职责一份实现"，且 CI 天生置位，无需额外配置 |
| B0-5 ✅ | **性能门禁去 echo**：`drift-detection.yml` 的性能 job 要么给出真实断言，要么**整体删除** | `grep -rn "Performance-baseline gate removed\|::warning::Performance" .github/workflows/drift-detection.yml` **无匹配** ✅ |
| B0-6 ✅ | **db-migration-gate 20 处占位**：逐条实现或删除，不留 `placeholder` 字样 | 从 20 → **3**，且这 3 处是**真实测试文件名**（`api_placeholder_contract_p0_tests.rs` / `api_placeholder_contract_p1p2_tests.rs`，"placeholder contract" 是领域概念），门禁已无空壳步骤。故"`grep -c placeholder` = 0"这个判据本身错——它会把真测试名当占位 |
| B0-7 ✅ | **H-14 高危**：`docker/db_migrate.sh` 优先宿主 `psql` 的逻辑加护栏（非 compose 栈目标时拒绝执行 / 显式 `--allow-host-psql`） | 实测已落地：`host_psql_target_is_implicit_loopback()` 判定「隐式 loopback 目标」→ 拒绝，除非 `SYNAPSE_DB_MIGRATE_ALLOW_HOST_PSQL=1`；显式给出的 `DATABASE_URL` / `DB_HOST` 一律放行 |
| B0-8 ✅ | **H-5 ✅ / H-12 ✅ 均已关闭**：分支 `optimization/audit-2026-07` **不是 3 个提交而是 157 个**。已完成按主题甄别（`docs/audit/H5_STALE_WORKTREE_TRIAGE_2026-09-15.md`），结论是**没有需要移植的对象**：`git cherry` 的"157 未合并"是 patch-id 假象（`main` 用重写方式落地），内容级比对显示 **86% 的新增行已在 main**、57/157 提交零残留、**OPT-001…031 逐项核验 31/31 已在 main**（含 OPT-020/024 已吸收进 baseline 迁移）、13/13 删除已生效。另有**安全发现**：该 worktree 暂存区含一把真实 Ed25519 私钥（7 月分支的 `.gitignore` 缺 `*.key`；`main` 的 `.gitignore:18-19` 已有），已 `git rm --cached` 处置。**收尾已执行**：私钥已备份到 `~/Desktop/hu_ts/synapse-rust-h5-archive-2026-09-15/`；worktree 已移除；分支归档为 `archive/optimization-audit-2026-07`（157 提交仍可达）；**`git worktree list` 实测 = 1 条，工作区 0 个残留改动、0 个未跟踪文件**。**H-12 已修**：`15432` 是**死端口**（`nc -z localhost 15432` → 无监听；dev compose 现在发布的是 `${DB_EXPOSE_PORT:-5432}:5432`），且旧链还把**应用库** `…/synapse` 当兜底（本机该库根本不存在，`psql -d synapse` → `database "synapse" does not exist`）。已把 5 份 Rust fallback 链统一为「`5432` + `synapse_test`」，清掉 6 个脚本里的 `15432` 默认值，并把 7 处测试里硬编码的 `…:15432/synapse_test`、`…:5432/synapse` 归正；新增静态守卫 `tests/unit/test_db_url_convention_tests.rs`（6 项，含变异自证） | `git worktree list` = 1 条 ✅；H-12 守卫 6/6 ✅、变异注入后 2 项转红 ✅；该文件 env-first 行为不变 ✅ |
| B0-9 | **D2**：`docker/deploy/` 18GB 备份移出版本工作区（**需用户确认，不自动删**） | `du -sh docker/deploy` 回落 |

**批次验证门**：`cargo check --workspace --locked` + `./scripts/check_fmt_ratchet.sh` + clippy `-D warnings`
＋ B0-2/B0-4/B0-6 各自"能变红"证明。

---

### B1 · 拆 r0 兼容链（A7，A3 的前置乘数）

> 先拆乘数再建单源管道，否则 489 条重复会被固化进新管道。**这是唯一的对外契约变更窗口。**

| # | 改动 | 验证 |
|---|---|---|
| B1-1 ✅ | **盘点**：从 ledger 快照导出全量条目 → 按 `(method, suffix)` 去重 → 对每条重复确认 handler 同源 | 已完成，三方核对表见下节：`all` 1319 条 / client 域唯一 487 / 冗余 401 / **r0-only 13** |
| B1-2 ✅ | **SDK 字面路径复核**：grep `@langkebo/matrix-js-sdk` **manager 源码**里的实际 URL 字面量（**不得**用 route-table 作判据——它只增不减） | 已完成：清单**非空**（推翻了"预期为空"），逐处见下节。SDK 侧已随 `039c2b2ec`（feat(b1): migrate saml/room-member/push r0 paths to v3）迁到 v3；复检 `grep -rn 'ClientPrefix.R0' matrix-js-sdk/src/` = **0**（仅生成的 `__generated__/route-table.ts` 仍留 r0 条目——正是本行警告的"只增不减"，不得用作判据） |
| B1-3 ✅ | **去 r0**：`*NEST_PREFIXES` 删除 `/_matrix/client/r0`；`assembly.rs` 的 r0 nest；`create_*_r0_only_router` 合并回正 router；删除 `suppress_r0_deprecation_warning` / `suppress_vendor_endpoint_warning` 配置字段 + `homeserver.yaml` 条目 | 实测：`'"/_matrix/client/r0"'` = **0**、`create_*_r0_only_router` = **0**、`NEST_PREFIXES` 内 r0 = **0**。⚠️ 判据"ledger diff 恰为 −489"是**错的**——B1-1 已实测拆 r0 的收益是 **−270 条重复**、代价是 13 条 r0-only 需改挂（实为改名 `*_extra`）。**B1-3 遗留已补齐（commit 见下）**：`suppress_vendor_endpoint_warning` 字段（**从未被任何代码路径读取**：告警点直接读 env var）+ `homeserver.yaml` 条目 + `docker-compose.yml` env 透传 + 每次启动必发的 `tracing::warn!` 全部删除；8 处仍声称 "under r0 + v3" 的**陈旧注释**（`assembly.rs` ×5、`friend_room.rs` ×2、`space.rs` ×1）一并更正 |
| B1-4 ✅ | **契约文档 + fixture 重基线**（一次性） | 已完成：`ROUTE_CONTRACT.md` 1146 routes / 46 categories 与源码一致（`EXTRACT_STRICT=1` EXIT=0）；两种泳道 fixture 均已重生成（golden `all` 1065、sdk `all` 1146） |

**风险**：Complement 测试套若断言 r0 可达需同步；SDK 端到端冒烟（登录/sync/发消息）必须跑一次。

> **B1-3 的教训（新增）**：删除一个前缀时，**注释不会自己跟着改**。本次 `grep '"/_matrix/client/r0"'`
> 早已是 0，但 `assembly.rs` 里 5 处 `expand_under_prefixes` 的上方注释仍写着 "under r0 + v3"，
> 而紧随其后的前缀数组只有 `v3`。这类"注释声称的契约 ≠ 代码注册的契约"会直接误导复核
> （本次复核一度据此判断"r0 仍在服务"）。判据只能取**机器生成的** `ROUTE_CONTRACT.md` / ledger，
> 不是注释。

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

> **⚠️ B2-1 处方机制可行性复核（2026-09-15，实测）—— `from_router` 不可行，见 §3.1。**
> 下表 B2-1 原文的机制（读装配后的 `axum::Router`）在本仓锁定的 **axum 0.8.9** 上无法实现。
> 在 D4 裁定前**不要**按原文动工。

| # | 改动 | 验证 |
|---|---|---|
| B2-1 🔄 | **派生器**：原处方（`RouteLedger::from_router(router)`）**已证不可行**（axum 0.8.9 无路由枚举 API，见 §3.1），D4 裁定改走 B 路：让源码提取器成为派生源。**第 1 步已完成**（§3.4）：提取器现有 per-lane（`#[cfg]`，特征集读自 `Cargo.toml`）与 per-profile（从 `merge_into` 读出的运行时 flag guard）建模，能**逐条精确复现六组 fixture 集合**（golden 1047/1058/1065、sdk 1127/1138/1146） | 六组集合精确相等，已进 `EXTRACT_STRICT=1` 硬门禁；`test_extract_registered.py` 39 项检查 + 4 项变异自证。**源码级变异实测**：改了 `mod voice` 的 cfg 后 union 门禁四项指标全绿（1146/0/0/0）而新门禁 EXIT=1 点名三条 profile —— 证明"承诺在编译不出它的泳道里"这类谎 union 数学上看不见。**剩余**：生成派生物 → 删 244 处手抄 → B2-3 幂等守卫（§3.3 第 2/3/5 步） |
| B2-2 ✅ | **删手抄**：删除全部手抄 `*_route_manifest()` 助手；手写点只剩 979 处 `.route()` | **实测（2026-09-16）**：删除 **67 个** `*_route_manifest()` 定义 + 随之孤儿的 `*_relative_routes()` 助手与 `*_NEST_PREFIXES` 常量，代码净 −6422 行（88 文件，+2151 / −8573）。判据按字面口径修订为 `grep -rn 'fn [a-z_0-9]*_route_manifest(' src/ \| wc -l` = **1**（只剩 `derived_route_manifest`）—— 原写法 `grep -rl '_route_manifest' src/ = 0` 在 `derived_route_manifest` 命名确定后**数学上不可能为 0**，故收紧为"除派生器外无 `*_route_manifest()` 函数"（另：`declared_route_manifest_for*` 两个访问器重命名为 `declared_ledger_for(_profile)`，并新增 `declared_ledger_all()`）。**验证**：`cargo fmt --check` 0 diff；`python3 scripts/contract/gen_derived_routes.py --check` 绿（1148 行、6 组 fixture 全复现）；`cargo test --lib --features test-utils web::routes` **450 passed / 0 failed**；`cargo test --test unit --features test-utils` 相关切片 **521 passed / 0 failed**。详见 §3.6 |
| B2-3 | **幂等守卫**：同一二进制启动两次导出的 ledger **逐字节相等** | 新增测试，故意引入 `HashMap` 迭代序 → 红 |
| B2-4a ✅ | **正向契约守卫（S-14）**：断言"真实 router ⊆ ledger"（即不存在已服务却未登记的端点） | 已完成：先量出真实缺口 —— 以 golden + sdk 两条 fixture 泳道的**并集**为完备性 oracle，`derived \ ledger` 实测 **22 条**（原先按 golden 单泳道算是 102 条，其中 80 条是 feature 门控噪声，把真缺口埋掉了）。22 条逐条核实为真后全部补进所属 manifest；现 `derived \ ledger = 0`、`ledger \ derived = 0` 双向闭合，并在 `EXTRACT_STRICT=1` 下成为硬门禁（原实现把这组差集**只打印不拦截**——见原注释"reports rather than enforced"）。守卫测试新增 `check_positive_contract`（含"谓词非空转"自检）。**附带**：同一工作窗内发现并修复了 S-16 —— `tests/integration/snapshots/route_ledger_{default,worker_enabled}.snapshot` 是**手改而非重生成**的（1378 行含 250 条完全重复、22 条生产上 404 的 v3 friends 声明），该集成快照用例在 `main` 上本来就**是红的**；已带库重生成至 1127/1138 并逐项对账闭合（见 `PROJECT_ACTUAL_ISSUES §13`）。**剩余**：B2-4b 的"SDK 声明消费的全部端点 ⊆ ledger"需解析 SDK manager 源码字面量，另立条目 |
| B2-4b ✅ | **SDK 侧正向守卫**：断言 SDK manager 源码里实际调用的端点 ⊆ 已被 ledger 覆盖（方向与 B2-4a 相反：B2-4a 防"后端偷偷多出端点"，本条防"后端欠 SDK 端点"） | 已完成：`scripts/contract/check_sdk_route_coverage.py`（SDK 缺席时只报告并跳过，`SDK_CONTRACT_STRICT=1` 升为硬失败）。**判据**：只读 manager 源码的 `encodeUri("…")` 字面量（117 处），**不读** `__generated__/route-table.ts`——后者"只增不减"，B1-2 已实测它在 r0 拆除后仍留着 r0 条目。**匹配**：首段锚定对齐（`path_match`），不猜前缀——原型期用"取附近 `prefix:`"会把后一个请求的 `VendorPrefix` 错配到前一个 v3 路径上，假阴假阳都有；改为「以 SDK 相对路径首段在 ledger 里找落点，落点后剩余段数必须相等，逐段比对（`{}` 通配、字面量必须相同）」后，未覆盖数从宽松匹配的 8 条收敛到**2 条**。**实测**：117 站点中 35 个能解出唯一 method（其余 48 处是 `client-*-requests.ts` 一族的纯路径构造器，method 由调用方给），方法校验 **0 不匹配**。**变异自证 5 项**：① 伪造端点未被拒 → 红；② 谓词退化回"段数相等整段比较"→ 自检红；③ allowlist 塞假条目 → 报 stale 红；④ 条目缺理由 → 解析期报错；⑤ 在 SDK 副本里注入不存在的调用 + 把 `Method.Post` 改成 `Delete` → 分别按 `文件:行号` 精确报出"未覆盖"与"方法不匹配"。**门禁接线**：`check_route_contract.sh` 新增该步骤（`bash scripts/contract/check_route_contract.sh` 现 4 段全绿）。**不依赖 SDK 的部分刻意前置**：谓词自检 + 豁免清单卫生检查（被豁免的形状必须**仍然不被 ledger 服务**——否则后端补上端点后豁免会静默吃掉真实不一致）在任何环境都跑；CI 只 checkout 本仓、拿不到 SDK，这两项是那时唯一的护栏，且 SKIPPED 横幅明写"本次结论不覆盖 SDK ⊆ ledger"。**结论**：`SDK 调用 ⊆ ledger` 当前成立，2 条已核实缺口进豁免清单并各带 follow-up（见 H-16/H-17） |
| B2-5 | **投影去 tracked**：`docs/openapi/client.yaml`（72,924 行）移出索引 → 由同管道生成到 CI artifact；`route-table.json` 加"生成物禁止手改"头注释 | CI 生成 job 成功；diff 只反映 B1 的 r0 拆除 |

**过渡态**：若 axum 版本无法枚举 nest 前缀，退化为"每个 router 构造时 `ledger.register(...)` 增量记录"，
仍删除独立抄写函数。

#### §3.1 B2-1 可行性复核：`RouteLedger::from_router(router)` 走不通（2026-09-15 实测）

原文要求「实现 `RouteLedger::from_router(router)`，在 `main.rs` 装配完成后从 Router 一次性导出」。
实测结论：**本仓锁定的 axum 0.8.9 不提供任何公开的路由枚举能力**，该机制在不 unsafe 摸私有布局的
前提下无法实现。证据（`~/.cargo/registry/src/*/axum-0.8.9/src/routing/mod.rs`）：

```
pub struct Router<S = ()> { inner: Arc<RouterInner<S>> }   // 字段私有
struct RouterInner<S> { path_router: …, fallback_router: … } // 结构体私有
// Router 的全部 pub fn：
//   new / without_v07_checks / route / route_service / nest / nest_service / merge /
//   layer / route_layer / has_routes / fallback / fallback_service /
//   method_not_allowed_fallback / reset_fallback / with_state / as_service /
//   into_service / into_make_service / into_make_service_with_connect_info
```

没有任何 `routes()` / 迭代器 / 反射入口；唯一的自省是 `has_routes() -> bool`。

**第二重阻断（更致命）**：即使自写"注册期录制器"包住 `.route()`，`MethodRouter` 同样**不暴露
已注册的方法集合** —— `.route(p, get().put())` 到底注册了哪些 method 读不出来。要拿全就必须连
`get()/post()/put()/delete()/…` 一起包。即 A 路的真实代价不是"改 933 处 `.route()`"，而是
"让 933 处注册语句全部改写成自定义 DSL"。

**当前规模（实测）**：`_route_manifest` 出现在 **72 个文件 / 244 个调用点**，`.route()` **933 处**。
运行时 ledger 的来源是 `ledger_export::build_artifact()` → `declared_route_manifest_for_profile(flags)`
（**读 manifest，不读 Router**）；per-profile（default / worker / all）粒度完全依赖 manifest +
`ProfileFlags`。

**两条可行路线**（D4 二选一）：

| | A 路：注册期录制 | B 路：源码提取器成为派生源 |
|---|---|---|
| 机制 | 自写 `Router` 包装 + `.route()/get()/post()/nest()/merge()` 全量改写为录制 DSL | 让 `extract_registered.py`（已存在、已通过 2 个独立 oracle）成为唯一派生源；删除 244 处 manifest |
| 改动面 | **933 处注册语句** + 重新实现 nest 前缀传播 + 重新实现 method 集合抽取 | **0 处注册语句**；`extract_registered.py` 增加 `#[cfg(feature)]` 的 **per-profile** 建模（现为"递归进 cfg 块取并集"）；重接 `ledger_export` 与 fixture 生成管道 |
| 新增失效模式 | **遗漏包装即静默不登记** —— 而 ledger 正是"完整性"的判据，这比现状（显式手抄）更危险 | 解析器又有新盲区（但已有 20 项守卫 + 变异自证兜底） |
| 与 B2-2 判据 | 需大量改写后才能谈"删手抄" | 直接满足 `grep -rl '_route_manifest' src/` = 0 |
| 风险 | 高（大范围重写 + 新失效模式） | 中（解析器增强 + 管道重接，均有现成守卫） |

**附带收益（B 路的证据基础）**：S-13 已把解析器做到了"链式方法全记入 + nest 前缀传播 + 测试块
切除 + 两份独立 oracle 0 缺口"，且有 20 项守卫与变异自证。也就是说**"从源码派生"这条链在本仓
已被证明可靠**，而"从 Router 派生"这条链被库 API 证明不可行。B 路缺的那一块（per-profile cfg）
是可测的增量，不是未知领域。

#### §3.2 侦察结论（2026-09-15，D4 裁定走 B 路后）——两点改变风险评级

**① `assembly.rs:24` 早就写明了同一件事。** `base_route_manifest()` 的文档注释第一句是：

> "This is the substitute for the axum route-walker API we don't have — see R4 / O2 in
> `docs/synapse-rust/SPEC_ALIGNMENT_PLAN_2026-05-01.md`."

即：**团队在 2026-05 就知道没有 route-walker，并明确把 manifest 定位成它的替代品**。
B2-1 的处方（`from_router`）等于要求把已经存在的"替代品"换成它替代的那件不存在的东西。
这不是新信息，只是写处方时没回看这条注释。→ 结论不变，但**理由更硬**：B 路不是退让，
而是回到团队原本的架构选择上继续做（消掉替代品里的"手抄"部分）。

**② 存在一个独立 oracle，且它恰好就是缺的那个能力。** 侦察 `tests/integration/api_route_ledger_tests.rs`
发现：它 boot 真实 `Router`，对**每一条**声明的 `(method, path)` 发一个 `PATCH` 请求，断言返回
**405 且 `Allow` 头包含所声明的方法**；返回 404 即判定"manifest 在说谎"。

```
// Why PATCH? It is reserved by RFC 5789 but unused by every endpoint we
// register, so axum's MethodRouter will always answer with 405 + Allow
// when the route exists.
```

这解决了 §3.1 里"枚举不了"的死结：**列举做不到，但"逐条问存在性"做得到**。于是 B 路的安全性
不依赖"解析器不可能有盲区"，而依赖三层组合：

| 层 | 防线 | 抓什么 |
|---|---|---|
| 1 | 提取器 20 项守卫 + 变异自证 | 解析器已知缺陷类别 |
| 2 | `unresolved` 棘轮（`extract_unresolved_allowlist.txt`，现 19 条） | **新出现的**未知构造 —— 解析器有新盲区时必须显式登记，不会静默漏 |
| 3 | **405 探测**（每声明一条就探一条） | 派生表里的**假阳**（解析器凭空造出的路由） |

**③ 必须诚实记录的一处能力让渡。** 现状下 B2-4a 的 `derived \ ledger = 0` / `ledger \ derived = 0`
是**两个独立源**对账（提取器 vs 手抄 manifest）。删掉 manifest 后 ledger 即派生结果，这条双向对账
退化为自指。**"后端有路由但派生表没有"这一方向（解析器漏读）不再有独立 oracle 兜底** —— 第 1、2 层
守卫是它仅剩的防线。这是 B 路明确付出的代价，不是被忽略的细节。
（`ledger_export` fixture 也源自 manifest，因此它从来不是"独立"的第二个 oracle；真正的独立性
一直只来自提取器的不存在性 —— 而现在它成了唯一源。）

#### §3.3 B 路执行计划（已裁定，按此动工）

1. ✅ **提取器加 per-lane / per-profile 建模**（2026-09-15 完成，见 §3.4）。
   现在是"递归进 `#[cfg(feature)]` 块取并集"，已改为记录 cfg 谓词并按 feature 集求值；
   对账判据 **六组集合全部精确一致**：golden 1047/1058/1065、sdk 1127/1138/1146。
2a. ✅ **`registered_by` 可派生**（2026-09-15 完成，commit `8ef20296`）。
    32 条 origin 规则（`ledger_origins.txt`）+ 路径规则 + 注册点归属，
    复现两条泳道共 2211 个标签（零豁免清单）。
    每条路由的标签源被 `test_extract_registered.py` 锁住：抽掉 swimlane 规则 → 2 个标签漂移转 RED。
2b. ✅ **per-row `#[cfg]` 门可派生**（2026-09-15 完成，commit `26730b2c`）。
    `Resolver.cfg_of` 记录每条路由注册时的最窄 `#[cfg]` 作用域；`gate_of(row)` =
    narrowest-in-function-scope ∪ module-gates-of-all-registrars；
    gate-filtered union 复现两泳道（default 1065、all-extensions 1146）。
    抽掉模块门 → golden 1066 vs 1065 转 RED。
2c. ⏳ **生成可 include! 的 Rust 派生物**：由提取器产出一份 `include_str!`/`include!` 可用的
    路由表（每条带 `#[cfg(..)]` + profile guard + `registered_by` + 注解），
    供 `base_route_manifest()` / `declared_route_manifest_for_profile()` 读取；
    运行时 `create_router` 的重复检测继续用这份表。
3. ⏳ **删手抄**：删除 244 处 `*_route_manifest()`（72 文件）与 `assembly.rs` 的 37 处 `ledger.extend(...)`；
   含 `top_level_inline_manifest()` 与 `assembly_compat_manifest()`。
   判据：`grep -rl '_route_manifest' src/` = 0。
   ⚠️ **前置项（2026-09-15 侦察，§3.5）**：`auth`（1 条）、`rate_limit_exempt`（5 条，sync + sliding_sync）
   与 `query_params`（0 条）属于**手写注解**，不能从 `.route()` 调用推演，必须建立 `ledger_annotations.txt`
   策略表 + 提取器 fidelity 守卫，否则删表后 middleware  exempt-paths 归零 + `auth=user` 丢失。
4. ⏳ **保住 per-profile 粒度**（§3.4 已把它从"猜"变成"读"）。
   `ProfileFlags { oidc_enabled, worker_enabled, saml_enabled }` 是**运行时配置**，源码读不出来。
   侦察修正了本条的两处原假设：
   - 受 flag 影响的**不是整模块，而是两个 router 构造根**：`oidc::create_oidc_router`
     （`oidc_enabled`）与 `worker::create_worker_body_router`（`worker_enabled`）。
     同模块内 `create_oidc_fallback_router` / `create_worker_admin_router` 是**总是合并**的，
     所以 `oidc/mod.rs` 10 条里有 2 条、`worker.rs` 15 条里有 4 条属于 Always。
   - 三个 profile **单调**（`default ⊆ worker ⊆ all`，实测违反数 = 0），
     于是"该路由在哪个 profile"可以压成一个 `{Always, Oidc, Worker}` 三值标注，
     而不是对 `manifest_for_profile` 的分支做通用求值。
   - `ProfileFlags::saml_enabled` **从不被任何 `manifest_for_profile` 读取**
     （SAML 路由是经 `oidc::oidc_enabled()` 折进 `oidc_enabled` 的）—— 记为 H-18。
5. ⏳ **B2-3 幂等守卫**：同一输入两次导出逐字节相等；故意引入 `HashMap` 迭代序则转红。
6. ⏳ 跑满门禁：两个方向的契约门禁、契约文档重生成、ledger 单测 11/11、405 探测集成测试、
   fixture 双向对账。

#### §3.4 B2-1 第一步：提取器已经能复现全部六组 (lane × profile) 集合（2026-09-15）

**做了什么。** `extract_registered.py` 原先只承认一个"并集"视角：`#[cfg(feature = "…")]`
被当作噪声丢掉（`strip_leading_attrs` 直接略过属性），运行时 `ProfileFlags` 更是完全不存在。
于是它只能回答"源码里一共有多少条路由"（1146），回答不了"`default` 特性编译下、`worker` 档配置下
到底服务多少条"。现在两条轴都建模了：

| 轴 | 载体 | 读法 |
|---|---|---|
| **lane**（编译期） | `#[cfg(feature = "…")]`，出现在 ① `mod` 声明 ② `fn` 定义 ③ 语句/块 | `Cargo.toml` 的 `[features]` 求传递闭包：`golden` = `default`；`sdk` = `default + all-extensions`（cargo 语义是叠加而非替换） |
| **profile**（运行时） | `route_module.rs::*::merge_into` 里 `if <flag> { router.merge(…) }` | 解析 `merge_into` 的 `if` 分支识别出被门控的 router 构造根；**不是手写清单** |

**为什么 lane 必须在 `mod` 声明上把关。** 试过只按 `fn` 级 cfg 过滤：`#[cfg(feature = "voice-extended")]
impl RouteModule for VoiceModule` 被丢掉后，`voice::create_voice_router` 就**没有静态调用方**了，
于是被提升为新的 root，把 27 条路由**加进**了根本编译不出它们的 golden 泳道 —— 过滤器反而变成放大器。
`mod` 声明是唯一覆盖整个条件面的位置。

**为什么 profile guard 要在"路由产生点"归属。** `route_module.rs::merge_into` 本身也是一个 root
（`create_router` 通过 trait 动态派发调用它，静态看不见调用方）。实测：把 11 个 `merge_into` root
整个排除会丢掉 **194 条**路由 —— 因为 `friend_room` / `voice` / `cas` / `saml` / `widgets` 等模块的
构造器**只**经由它被静态调用到。所以 guard 不能挂在 root 上，必须挂在"产生这条路由的那个构造根"上，
且允许多个 guard 并存（`/.well-known/openid-configuration` 同时被总是合并的 fallback router 与
OIDC-only router 服务 → 判定为 Always）。

**判据（六组，逐条精确相等，无豁免清单）。**

```
ledger_export      default 1047  worker 1058  all 1065
ledger_export_sdk  default 1127  worker 1138  all 1146
```

对面是**手写 manifest** 产出的 fixture，所以这是"两套独立实现互证"，不是自证；
且这套对账已接进 `EXTRACT_STRICT=1`（`check_route_contract.sh` 每轮都跑）。

**新门禁抓到了旧门禁看不见的东西（源码级变异，实测）。**

| 变异 | 旧（union）门禁 | 新（B2-1）门禁 |
|---|---|---|
| 把 `mod voice` 的 `#[cfg(feature = "voice-extended")]` 改成 `widgets` | **完全无感**：`router-derived 1146`、`declared-not-derived 0`、`ledger NOT derived 0`、`derived-not-in-ledger 0` | EXIT=1，三条 profile 全部 FAIL（golden 1085 vs 1058 等） |
| 把 `OidcModule::merge_into` 的条件改成 `true` | 无感（并集不变） | EXIT=1，点名 `GET /_matrix/client/v3/oidc/authorize` 等 4 组集合多出 8 条 |

含义：**"某条路由被承诺在一个编译不出它的泳道里"和"被承诺在一个永不合并它的 profile 里"
这两种谎，union 视角在数学上不可能看见。** 这正是 B2-1 要消掉的那类不一致。

**同时新增的守卫。** `test_extract_registered.py` 现 52 项检查（原 22 项）+ 6 项变异自证：

- cfg 谓词双向可判别（`feature = "voice-extended"` 在 golden 关、sdk 开；`not(feature = "friends")` 两泳道都关；
  `all/any/not` 组合与 Rust 语义一致；未知裸 flag 判为关；`features=None` 的并集模式恒真）；
- `merge_into` 读出的 guard 集合 **恰为** `{create_oidc_router: oidc_enabled, create_worker_body_router: worker_enabled}`；
- 每条派生路由都必须带 guard 记录（漏记会被静默当成 Always，所以单列一项检查）；
- 每个泳道 `default ⊆ worker ⊆ all`；
- `registered_by` 与 32 条 origin 规则全量对齐（2211 标签，零豁免；抽掉 swimlane 规则 → 2 标签漂移转 RED）；
- `gate_of` 为每条路由产出 gate，且 gate-filtered union 复现两泳道（default 1065、all-extensions 1146；
  抽掉模块门 → golden 1066 vs 1065 转 RED；有门文件内 `#[cfg]` 块仍被门控 `/rooms/{room_id}/call/{call_id}` 为 `voip-tracking`；
  共享路径 `/login` 保留 `cas-sso`）。

**副作用：无。** union 侧输出逐字未变（`router-derived 1146` / `manifest 1077` / 双向 0 / `unresolved 19` /
非 Matrix 命名空间 14），`ROUTE_CONTRACT.md` 无漂移，SDK 覆盖检查仍 2 条豁免全命中。

#### §3.5 step 2c 前置侦察：派生物必须携带的手写注解（2026-09-15）

**结论：删 244 处 manifest（step 3）之前，派生表除 `(method, path, gate, registered_by)`
外，还必须携带两类"源码里推不出来"的手写注解。** §3.3 第 2 步原假设"提取器的三元组几乎够用"
（见收盘备忘）经实测被推翻——`RouteEntry` 有 3 个 `new()` 默认关闭的 builder 字段，它们承载的
是**作者意图**，与"注册了哪些路由"正交，`.route()` 字面量里根本没有这些信息。逐个量化：

| 字段 | 全仓来源 | 条数 | 消费者 | 删表后果 |
|---|---|---|---|---|
| `rate_limit_exempt` | `sync.rs:68` `GET /v3/sync`（1）+ `sliding_sync.rs:55` 4 个 POST sync | **5** | `assembly.rs:405` 从 ledger 收集 → 中间件 `rate_limit.rs:35` 跳过 IP 限流 | **限流回归**：这 5 条从"自带 per-user 限流"退回"双重限流"，静默改变行为 |
| `auth` | `delayed_events.rs:128` `POST .../delayed_events/{delay_id}` `.with_auth("user")` | **1** | `ledger_export.rs:157` → 导出进 fixture/SDK 产物 | **契约漂移**：导出少一个 `auth:"user"` 字段（该字段有真实消费者，非 `module`/`status` 那类零消费项） |
| `query_params` | 无任何 `.with_query_params()` 调用 | **0** | 同上，`ledger_export.rs:156` | 无（恒空，可安全省略或留默认 `&[]`） |

**判据（与 step 2a 同构）**：`registered_by` 靠 `ledger_origins.txt` 32 条规则复现，
注解也必须有一张对账表——提议 `ledger_annotations.txt`，每行 `METHOD PATH<TAB>field=value`，
当前恰 6 行（5 exempt + 1 auth）。fidelity 守卫：提取器按此表为派生路由贴上注解，
要求与真实 manifest 产出的 `rate_limit_exempt_paths`（可运行时枚举）和导出 `auth` 字段逐条相等。
无独立 oracle 的方向（同 §3.2 的让渡）：exempt/auth 若只存在于这张新表，就与 manifest 同源，
故其正确性靠"变异自证"兜底——抽掉某行 → 派生 exempt 集少 1 转红。

**因此 step 3 的判据要加一条**：`grep -rn 'with_auth\|with_rate_limit_exempt' src/` = 0
之前，`ledger_annotations.txt` 必须已落地并被 fidelity 守卫覆盖，否则删 manifest 会静默丢注解。

---

#### §3.6 B2-2 删手抄：实测记录（2026-09-16）

**做了什么。** 删掉全部手抄投影，让 `derived_routes.rs` 成为路由元数据的唯一来源：

| 类别 | 数量 | 说明 |
|---|---|---|
| `*_route_manifest()` 定义 | **67** | 含 `assembly.rs` 的 `base_` / `assembly_compat_` / `vendor_` / `top_level_inline_` 四个，以及 `oidc_fallback_manifest` / `oidc_route_manifest_for` 两个近亲访问器 |
| 随之孤儿的 `*_relative_routes()` 助手 | **17** | 只被被删的 manifest 调用（如 `space_relative_routes`、`room_v3_only_relative_routes`） |
| 随之孤儿的 `*_NEST_PREFIXES` 常量 | **10** | 同上 |
| 净行数 | **−6422**（88 文件，+2151 / −8573；含派生表重排） | |

`RouteModule` trait 只剩 `merge_into()`：`manifest_for_profile()` / `manifest_for()`
两个方法及其 11 处 impl 一并删除——元数据不再由装配层声明。

**访问器重命名。** `declared_route_manifest_for` → `declared_ledger_for`、
`declared_route_manifest_for_profile` → `declared_ledger_for_profile`，并新增
`declared_ledger_all()`（最宽 profile，供能力门控与契约测试取全量）。理由：它们返回的是
`RouteLedger` 而非"manifest"，且新名字让 B2-2 的判据可以字面求值。

**判据修订（必须记录的一处口径修正）。** 原判据 `grep -rl '_route_manifest' src/ = 0`
在派生器被命名为 `derived_route_manifest` 之后**数学上不可能为 0**。修订为：

```bash
grep -rn 'fn [a-z_0-9]*_route_manifest(' src/ | wc -l   # = 1（仅 derived_route_manifest）
```

**注解安全。** §3.5 要求的 `ledger_annotations.txt` 已先行落地：5 条 `rate_limit_exempt`
与 1 条 `auth` 在删 manifest **之前**就搬进了注解表，所以删除没有丢注解。实测佐证：
`route_ledger::tests::collect_exempt_paths_from_real_manifests` 仍断言 6 条 sync 路径豁免，绿。

**顺带修掉的一处门禁互锁（P0 级）。** `cargo fmt` 与 `gen_derived_routes.py --check`
原本**互斥**：生成器按手排格式输出并做字节比较，而 rustfmt 会把 `RouteEntry::new(...)`
压成单行、把 `#[cfg]` 缩进一格——任一方跑过，另一方就红。现 `emit()` 在写盘前把输出过一遍
同一份 rustfmt（`rustfmt.toml` + edition 2021），两个门禁同时为绿：
`cargo fmt --check` 0 diff、`gen_derived_routes.py --check` up to date。

**一处测试语义迁移。** `sync_rate_limit_config_tests::sync_routes_are_still_exempt_from_the_generic_ip_limiter`
原来是"扫 `src/web/routes/sync.rs` 源码文本含 `with_rate_limit_exempt(true)`"。helper 删除后
该断言必然失败，已改为直接断言派生 ledger 的 `rate_limit_exempt` 集合——比扫源码更靠近
限流中间件真正读的东西。

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
| B4-1 ⏳ | **已完成 (i) 桶 10/10 + 棘轮落地**（2026-09-15）。分类存档见 `B4_1_TRAIT_CLASSIFICATION_2026-09-15.md`：真实分布为 (i) 10、(ii-a) `dyn`+单生产 impl 无 mock **32**、(ii-b) mock 接缝 17、(iii) 多实现 7。**注意原文"68 个"与实际不符**（本批前 `*StoreApi` = 66，现 56；总 `pub trait` 96 → 86） | 棘轮 `python3 scripts/ci/check_trait_ratchet.py`：`TOTAL=86 STORE_API=56` 基线写入 `scripts/ci/trait_count_baseline`；注入探针 trait 实测 **EXIT=1**、移除后 EXIT=0。`cargo clippy -p synapse-storage -p synapse-services --all-targets --all-features -- -D warnings` = 0 警告。**未验证**：仓库级 `cargo check --workspace` / `cargo test --test unit` 被其它会话的 `src/web/routes/**` codemod 阻塞（E0603/E0432 全在该批文件），本批未触碰 |
| B4-1b ⏳→✅(19/23) | **`dyn` + 单一生产 impl + 无 mock → `Arc<具体类型>`**。起始 23 个（修正分类后；原文 32 是把带 mock 的 9 个误判进来了），**已转 19 个**：`FeatureFlag`/`QrLogin`/`Privacy`/`Beacon`/`PushNotification`/`Retention`/`Captcha`/`AdminFederation`/`CallSession`/`MediaQuota`/`RegistrationToken`/`EventReport`/`Space`/`Saml`/`ChunkedUpload`/`FederationBlacklist`/`FriendRoom`/`ApplicationService`/`StickyEvent`。顺带删除 **7 个"只为装 trait 而存在"的 `api.rs` 空壳文件** | `cargo clippy -p synapse-storage -p synapse-services --all-targets --all-features -- -D warnings` = 0；`cargo test -p synapse-services --lib` **1987 passed / 0 failed**；`cargo test -p synapse-storage --lib` **1759 passed / 0 failed**（`public` 重灌后；此前 906 failed 全是 T-1 空 `public` 导致的 42P01）。**63 文件 / +111 / −2901 行**；棘轮基线收紧到 `TOTAL=67 STORE_API=37` |
| B4-1 记账 | `AuthSource`/`AdminAuthSource`（B4-3 引入）使 `pub trait` +2，已按棘轮规矩在 `scripts/ci/trait_count_baseline` 写明理由：`*StoreApi` 未增加（33），B4-1+B4-3 整体 `pub trait` 96 → 65、`*StoreApi` 66 → 33 | 棘轮 `OK: trait counts at baseline` |
| B4-1c ✅ | **剩 4 个转换已完成**（`InviteBlocklist`/`Module`/`RendezvousMessage`/`EmailVerification`）—— 并发 codemod 于 `6f06eb0c` 落地后，`src/web/routes/context.rs` 不再被改，一次性转换 | workspace `clippy --all-targets --all-features -D warnings` = 0；`cargo test -p synapse-services --lib` 1987 passed / 0 failed；−408 行 / 12 文件；棘轮 67/37 → **63/33** |
| B4-1d 🆕 | **A5 大头剩余**：`MemberStoreApi`(21 文件)、`RoomStoreApi`(12)、`AccountDataStoreApi`(10) 等 dyn 面很大的 trait —— 它们**有 mock**（属 mock 接缝，保留 trait 本身），但消费者侧的 `Arc<dyn …>` 可改为 `Arc<具体类型>` 并在测试处**向下转型/注入具体 mock 类型** | 需单独裁定：收益（去 trait object）与代价（失去现成的 mock 注入点）权衡 |
| B4-2 | **删除前守卫**：每个 trait 删除前 grep `dyn` 与 mock 引用（分类规则保证） | 分类清单存档 |
| B4-2 ✅ | 判据落地为**三重检查**（全名残留 = 0 且无 `dyn`、无泛型约束、无测试消费者），10 个 trait 删除后全名 `grep` 残留均为 0；4 个只为"扁平路径==分组路径"而存在的迁移期测试（`*_store_api_path_identity`）随 trait 一并删除 | 见存档 §2 表 + §4 验证矩阵 |
| B4-3 ✅ | **A4 DI 泛型化已完成**（2026-09-15）。新增 `src/web/routes/auth_source.rs`：`trait AuthSource`（`token_auth` + `admin_audit_service`）与 `trait AdminAuthSource: AuthSource`（`user_service` + `security_config`），为 9 个 context + `AppState`（AuthSource）、4 个（AdminAuthSource）各实现一次。`extractors/auth.rs` 的 **21 个逐字重复 impl → 3 个泛型 impl**（`AuthenticatedUser`/`OptionalAuthenticatedUser` 用 `S: AuthSource`，`AdminUser` 用 `S: AdminAuthSource`） | `auth.rs` **−645 行**、`admin_auth.rs` −25 行、新模块 +117 行，净 **−553 行**。
**最终验证（真实树，非隔离副本）**：`cargo test --test unit --features test-utils` = **1821 passed / 0 failed**；
`cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` = **0 警告**；
`cargo check -p synapse-rust --all-features` = 0。
（中途曾因另一会话正在改写 `derived_routes.rs` 而无法编译，故先用"该文件回退到 HEAD"的隔离副本验证过一遍：
`cargo check` = 0、unit 1812 passed / 9 failed（9 个全是副本未包含 `docker/deploy/` 的断言）。
其改动落地后已在真实树复跑并全绿。）**注意**：`AdminAuthSource` 按当前能力集精确实现（只有原本支持 `AdminUser` 的 4 个 + AppState），故不新增任何 extractor×context 组合 |
| B4-3a ✅ | **连带消除鉴权路径的重复实现**：`authorize_admin_request`（基于 `AppState` 的 83 行版本）与 `authorize_admin_from_services`（参数化版本）是近似逐字重复。先收敛为薄适配器、再随泛型化失去最后一个调用点后**整体删除**，管理员鉴权从此只有一份实现 | 见 B4-3 行；`cargo test --test unit --features test-utils` 全绿（含 `admin_auth` 自带 10 个单测） |
| B4-4 | **A2 分层强制**：CI lint 禁 `src/web/` 下 `use synapse_storage::`（白名单趋向 0，棘轮）；48 个文件补薄 service 或下沉 | 故意加一行 → 红；`grep -rl synapse_storage src/web/routes` 计数下降 |
| B4-4 ✅(门禁半) | **A2 分层门禁已落地**：`scripts/ci/check_web_layering.py` + `scripts/ci/web_layering_allowlist.txt`，接入 `ci.yml` repo-sanity。**白名单语义**（不是裸计数）：白名单外的文件若引用 `synapse_storage` → 红；白名单里已不再引用的条目 → 也红（列表只能变短）。注释会被剥离，故文档里提一句不算违规 | 实测：当前 **49** 个文件入白名单、gate EXIT=0；三种 RED 自证 —— ①新增探针文件引用 storage → EXIT=1 并点名；②删掉一条有效条目 → EXIT=1；③塞一条不存在的路径 → EXIT=1 报 stale |
| B4-4-迁移 ⚠️ | **48/49 个文件的"补薄 service 或下沉"** 仍未做，且必须等 `src/web/routes/**` 的 manifest codemod 落地（同一批文件） | 每下沉一个文件就从白名单删一行；`grep -rl synapse_storage src/web \| wc -l` 下降 |
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
