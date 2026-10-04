# 架构层问题（A1–A12）根治路线图

- 日期：2026-09-15
- 输入：`REDUNDANCY_OVERENGINEERING_AUDIT_2026-09-14.md` §2「架构层面的问题」（A1–A12）
- 基线：`main` @ `3739c9ba`（第一批零风险直删 + 第零批 3 项正确性修复 + §1–3 逐条修复 + CI 第三批已合入）
- 口径：审计报告 §2 的量化数字基于分叉点 `6ba7c457`，本文所有「现状」均按 `3739c9ba` 重新核验，标注「已消解」的项不再纳入路线。

---

## 0. 现状校准（报告基线 → 当前 HEAD 的增量）

| 报告条目 | 报告时状态 | 当前状态（已核验） |
|---|---|---|
| CFG-2/CFG-4/INF-1(部分)/INF-2/INF-5/INF-6/WEB-1 | 存在 | **已删除**（`src/web/api_doc/` 0 文件；`tables.rs` 不存在；`streaming.rs`/`filter.rs`/`src/security/` 已移除；`src/` 降至 64,356 行） |
| CFG-1 / STO-11 / STO-12 | 正确性缺陷 | **已修复**（附录 D.1） |
| A7 r0 兼容链 | 350/269/259 三倍扇出 | **仍然存在**：8 个 `*_NEST_PREFIXES` 常量、约 36 个文件含 `/r0` 字面量（`friend_room.rs` 55 处、`assembly.rs` 19 处） |
| A3 路由元数据 8 份 | 95 个 manifest 函数 | **仍然存在**：75 个路由文件含 manifest 引用，`assembly.rs` 内 7 个 manifest 函数 |
| A4 DI 手工复制 | context.rs 1,009 行 + auth.rs 932 行 | **仍然存在**（当前 1,000 + 932 行） |
| A5 存储 trait | 71 个 `*StoreApi` | **仍然存在**（66+ 文件定义 trait） |
| A6 双 DDL | runtime-ddl | **半边已消解**：`tables.rs` 已删；但「多份 schema 清单」仍在（`schema_health_check.rs` 111 处 + `schema_validator.rs` 50 处硬编码），且新增 `prevent_audit_delete` 触发器曾在非 public schema 复发（`86fc6cd0` 已修）——schema-blind 类缺陷仍活跃 |
| db-migration-gate | 18 个 echo 占位 | **仍存在**（`db-migration-gate.yml` 18 处 `TODO placeholder`） |

> 结论：A1–A12 中，由「死代码堆积」造成的症状大多已随第一批直删消解；**仍然完整成立的是结构性问题：A2、A3、A4、A5、A6（残留）、A7、A9、A10、A11、A12，以及 A1 的体量事实**。这正是本文路线要处理的集合。

---

## 1. 逐条根因分析（A1–A12）

### A1 根 crate 并非薄壳
- **现状**：`src/` = 64,356 行，其中 `src/web/` ≈ 88%；`synapse-services` ≈ 89k 行。HTTP 层与业务层体量相当的事实成立。
- **根本原因**：仓库按「binary + 库」而非「层次」组织——路由 handler、装配（assembly）、服务器生命周期（server/）、任务调度（tasks/）全部堆在根 crate，业务库 crate 反而是"被 import 的实现细节"。
- **影响范围**：所有跨层改动（新增一个端点要同时触碰路由文件、manifest、ledger、fixture）；测试车道分裂（根 crate `--test unit` 与 workspace `--lib` 两条路）。
- **依赖关系**：是 A2/A3/A4 的**容器**——不先把 web 层独立成 crate，任何"强制分层"的守卫都没有稳定的插入点。但它是**结果不是原因**：A3/A4 的元数据多副本才是根 crate 膨胀的机制。→ 只能作为末期收口，不能先行。

### A2 声明的 route→service→storage 分层未被强制
- **现状**：18 个路由文件直接 `use synapse_storage` 且不经 service。
- **根本原因**：分层只是文档约定（AGENTS.md），没有编译期或 CI 期的**机器强制**；而 A5 的 `Arc<dyn XStoreApi>` DI 风格让"路由直接持有 storage 句柄"在技术上可行且零摩擦。
- **依赖关系**：受 A5 制约（trait 收敛后路由直连变得不自然）与 A4 制约（context 字段即注入面）。
- **判定**：不是"把 18 个文件补 service"的问题，而是"让绕过不可见"的问题——守卫优先于搬迁。

### A3 路由元数据维护 5–8 份（最核心的根因）
- **现状**：Router 注册（994 次 `.route()`）、75 个文件的 manifest 函数、RouteLedger/RouteEntry、RouteModule trait、ledger_export bin + 1.9MB/6 个 JSON fixture、1,381 条快照、`docs/openapi/client.yaml`（72,924 行 tracked）。
- **根本原因**：**没有把 Router 当作唯一真相源**。manifest/ledger/fixture 都是"手抄的投影"，而投影本应由源单向生成。`assembly.rs` 的 6 个 manifest 函数与 42 次 `.merge()` 是两套独立枚举——它们会漂移是必然，不是偶然。
- **放大效应**：`friend_room.rs`（84 次 `.route(` vs 93 条手抄 manifest，93 条仅 31 个不同 suffix）、`key_backup.rs` 9 个 legacy 克隆 handler、尾斜杠别名 19 条——这些 WEB-5/6/18 全是 A3 的下游。
- **依赖关系**：是 A7 的**下游**（版本前缀扇出把每条重复放大 3 倍进 ledger/fixture），是 A1 的**上游**（manifest 手写副本占 web 层固定比例）。fixture 再生成依赖 ledger 重构完成的时点，顺序不能反。

### A4 DI 图手工复制 4 遍
- **现状**：`ServiceContainer → wiring/ → AppState → 11 个 context（259 字段）→ 121 个 FromRequestParts impl（其中 596 行逐字样板）`。
- **根本原因**：**没有抽象"认证来源"**——同一 extractor 逻辑对 3 个抽取器类型 × 7 个 context 类型手工笛卡尔积展开。新增一个服务要穿 4 层，导致 `Reserved/constructor parity` 死字段（INF-13/CFG-14）泛滥。
- **依赖关系**：与 A5 强耦合——context 字段爆炸的原因是每个 `Arc<dyn Trait>` 都要逐层 clone。A5 收敛 trait 后 A4 的字段数自然下降；**顺序必须先 A5 后 A4**，否则泛型化会被 259 个字段的 clone 拖住。
- **附带**：A4 是"绕过 service 层"（A2）的技术通路——context 暴露的就是 storage 句柄。

### A5 71 个存储 trait，69 个无多态价值
- **根本原因**：**把"可测试性"错误地等同于"trait 接口"**。仓库已有更好的机制（`test_mocks/` + `test-utils` feature 门控），但 trait-per-store 是更早的默认模板，新目录照抄五/六件套（`api.rs` + `repository.rs` + …）。
- **影响**：`Arc<dyn X>` 贯穿 DI 图 → 直接造成 A4 字段爆炸；trait/impl 分文件不产生抽象收益（STO-5 的 691 行连 `dyn` 都不出现）。
- **依赖关系**：是 A2（service 层被绕过的诱因：注入太容易）与 A4（clone 样板）的共同上游。**trait 收敛是结构性收益最大的单点之一，但必须渐进**：有真实 mock 消费者的 trait 要保留 seam（services 层的 4 个 auth trait 是合格反例参照，SVC-19 正面项）。

### A6 两套 schema 真相源（铁律 2）
- **现状**：`runtime-ddl` 第二套 DDL 已删（半边消解）；**残留问题升级为**：列级清单仍手工维护 3 份（`CORE_COLUMNS` 88 条 / `REQUIRED_COLUMNS` 13 条 / `migration_checks.rs` 表计数），且守卫 SQL 存在 schema-blind 复发模式——`prevent_audit_delete` 触发器（`86fc6cd0`）与 `typing` PK 守卫（`2026-09-14` 记录在 README）先后两次在非 public schema 下炸出 `multiple primary keys` / 判定失效。
- **根本原因**：baseline SQL 被**逐字应用到每个隔离 schema**，但其中的守卫语句用 `table_schema='public'` 判断存在性——这是"一份 SQL、两种执行上下文（public / 测试模板 schema）"的隐含假设未固化。
- **依赖关系**：与 STO-1（迁移链已被 v11 吸收但仍双文件维护）汇流；v12 重生成 baseline 时应一次性解决，属同一改动窗口。

### A7 兼容残留链：r0 别名 → 告警 → 静音配置
- **根本原因**：**为不存在的存量用户做向后兼容**（铁律 1 违规）。因果链自给自足：三倍扇出制造 489 条重复 ledger 条目 → 告警说"294 条该删" → 再加两个配置项静音告警。项目未发布、SDK 只走 v3/unstable。
- **影响**：路由面 ×3、A3 的每份投影 ×3、CI 的 feature×版本矩阵复杂度。
- **依赖关系**：是 A3 的**乘数**——**必须先拆 A7 再重建 A3 的生成链**，否则 fixture/快照再生成会把 1,381 条（含 489 条重复）固化进新管道。拆除影响对外契约：需先核对 SDK fork（@langkebo/matrix-js-sdk）字面路径（route-table 只增不减，不能作为调用判据——2026-09-14 Ledger v2 审计已确立此规则）。

### A8 两套 docker 部署目录漂移
- **根本原因**：**部署面没有单一真相源**，与 A6 同构（同一职责两份定义、无守卫、必然漂移）；`docker/deploy/config/homeserver.yaml` 甚至处于 tracked-but-gitignored 状态（TST-10），漂移无法被 CI 观察。
- **依赖关系**：与 CFG-11 `server` 伪 feature 纠缠（Dockerfile/deploy.sh/CI 三处的 feature 字符串需同步改）——A8 收敛目录时应一并裁定构建矩阵。

### A9 24 个错误类型 + 137 处 map_err
- **根本原因**：**缺一个跨层转换契约**（`impl From<ServiceError> for ApiError`）。每个域独立造 enum 本身是合理的领域建模，问题在于造完没有汇流机制，于是 137 处手工 `database_with_cause` 成为唯一通路——样板是缺失机制的代偿。
- **依赖关系**：A2/A4 的汇流点在 service→web 边界，错误汇流应与 DI 合并同期做，避免两次触碰同一批签名。

### A10 glob + allow(ambiguous) 维持可见性
- **根本原因**：**路径迁移成本被转嫁为永久压制**——为"legacy 根级路径继续可用"引入扁平 glob 与反向 re-export（`room/mod.rs` re-export crate 根的 DirectoryService），同一类型 2–3 条可达路径。
- **依赖关系**：与 A1 同族（门面壳）；独立性强，可任意窗口做，但与 A4/A5 同期可共享 import 改写工作量。

### A11 deny(missing_docs) 逼出 15,571 行零信息注释
- **根本原因**：门禁设计为**计数型**而非**内容型**——"每行都有注释"被当作"每行都有文档"，于是生成器式模板成为成本最低的通过方式。属铁律 8 的注释版。
- **依赖关系**：独立；但应放在所有代码合并动作**之后**（先稳定结构再补真实语义，避免给将被删除的代码写文档）。

### A12 仓库卫生
- **根本原因**：**产物入库无策略**——生成物（openapi yaml、schemas.json）、报告（308 md）、工作树副本（18GB backups、365k 行陈旧 worktree）与源码边界从未被机器划清。
- **依赖关系**：与 A3 共享一个决策：`docs/openapi/client.yaml` 是"生成物入库"，应由生成管道（A3 步骤③）顺带产出到 CI artifact 而非 tracked。

---

## 2. 根因聚类与依赖图

12 条问题可归并为 **4 个根因族**：

| 族 | 成员 | 病灶 |
|---|---|---|
| ①「手抄投影，无单一真相源生成管道」 | A3(←A7 放大)、A6残留、A8、A12 | 信息 3–8 份、守卫 schema-blind、生成物入库 |
| ②「手工笛卡尔积装配」 | A4、A9、（A1 膨胀机制） | DI 四层复制、137 处 map_err、context 字段爆炸 |
| ③「无收益抽象层」 | A5 → A2、A10、（INF-10 壳） | 69 个单实现 trait 强制 Arc<dyn>，绕过分层零摩擦 |
| ④「门禁与文档自指失效」 | A11、（§1.3 门禁）、A1 叙事与事实背离 | 计数型/恒绿/恒红门禁，AGENTS.md 描述与仓库实际不符 |

**硬依赖边（执行顺序约束）**：

```
拆 r0/compat 链(A7) ──必须早于──> ledger/fixture 生成管道重建(A3)
                                  （否则把 489 条重复固化进新管道）
trait 收敛(A5)        ──必须早于──> DI 泛型化(A4)
                                  （字段数先降，clone 样板才降得掉）
结构合并(②③族)        ──必须早于──> 注释重写(A11)
                                  （不给将删代码写文档）
守卫先行(A8/A12/分层lint) ──建议早于──> 对应的手动搬迁
                                  （先证明漂移可被拦截，再投入搬迁）
web 独立 crate(A1)    ──必须最后──>（是 ①②③ 收敛完成后的收口动作）
```

---

## 3. 分阶段优化路线

### 阶段 0 · 止血（1 个迭代，零/低行为变更）

> 目标：把「已确认正确性隐患」和「不可证伪门禁」清零，为后续大改提供安全网。

| # | 改动点 | 涉及模块 | 思路 | 成本 | 风险/兼容性 | 验证方式 |
|---|---|---|---|---|---|---|
| 0-1 | **push_rules 双写裁定**（STO-10/SVC-2） | `storage/push` vs `storage/push_notification`；`routes/push` vs `push_notification` | 产品裁定只保留标准 `/v3/pushrules`；非标准 `/r0/push/rules`、`/r0/push/devices` 本就"SDK 零调用点"（路由文件 `:355-362` 自认），并入 A7 拆除批次 | 裁定 ~1 天，合并 −700 行 | **对外契约变更**——项目未发布，可接受；需 SDK grep 复核字面路径 | 迁移前后 push 规则读写 db_test；ledger 比对 |
| 0-2 | **room_directory 双写裁定**（STO-13） | `storage/room/mod.rs` vs `storage/directory/` | 目录表读写归 `DirectoryStorage` 单所有；`RoomStorage` 侧方法删除或转发 | ~200 行 | `is_public` 语义归属需先定 | 目录列表/公开房间集成测试 |
| 0-3 | **retention no-op 显式化**（STO-15） | `retention_service.rs:390-422` | 恒返回空值的方法改为返回 `Err(Unsupported)` 并同步 admin 路由响应，或直接删端点（并入 SVC-0 裁定） | ~40 行 | 改变 admin API 响应 | 对应 admin 路由单测 |
| 0-4 | **ledger export 静默失败修复** | `ci.yml:737-746` | `\|\| echo warning` → `&& test -s docs/api-contract/route-table.json`，让导出真失败 | 3 行 | 可能暴露既有导出错误（好事） | 本地跑 `--bin synapse_ledger_export` |
| 0-5 | **db-migration-gate 18 个占位** | `db-migration-gate.yml` | 逐步骤要么实现要么删掉——按铁律 8，占位即红灯 | 分散 | 无 | 故意注入违规迁移证明门禁会红 |
| 0-6 | **schema-blind 守卫防复发 lint** | `migrations/*.sql` + 新 `scripts/check_schema_blind_guards.py` | 静态扫描 baseline/extensions：守卫不得含 `table_schema = 'public'` 字面量，必须 `current_schema()`/search_path；CI 挂为硬失败 | ~120 行脚本 | 无（只读检查） | 对现存 SQL 跑通；故意写一条 public 守卫证明会红 |
| 0-7 | **docker 双目录收敛**（A8） | `docker/` vs `docker/deploy/` | 裁定单目录（建议保 `docker/`），另一套删除；`homeserver.yaml` 的 gitignore 规则锚定；backups/ 经用户确认后移出版本工作区（**18GB 用户数据，不得自动删除**） | 半天 | 部署脚本路径引用需 grep 全量更新 | `docker compose config` 校验 + 部署演练（无 Docker 环境则记录为未验证） |
| 0-8 | **仓库卫生**（A12） | `.gitignore` / git index | `git rm --cached` 移除 `.scratch/`(97)、`coverage/`(3)、生成 JSON（`api_test/response_schemas.json` 63,784 行）；`git worktree remove` 陈旧副本 | 半天 | 无（保留磁盘文件，仅脱离跟踪） | `git ls-files -i -c --exclude-standard` 输出 0 |

**阶段验证门**：`cargo test --workspace --all-features --lib`（0 failed）+ `./scripts/check_fmt_ratchet.sh` + clippy `-D warnings` + 0-6 新守卫红/绿双向各证一次。

---

### 阶段 1 · 拆除兼容链 + 路由元数据单一真相源（2–3 个迭代，A7→A3）

> 目标：根治族①的路由侧。这是全路线**收益密度最高**的一段：净删约 1,500–2,500 行 Rust + 489 条 ledger 条目 + 约 7.5 万行 tracked 生成物。

**步骤 1a — 拆 r0/compat 链（A7，前置）**
- 改动点：8 个 `*_NEST_PREFIXES` 常量去 r0；`assembly.rs` 19 处 r0 nest；`create_*_compat_router`/`create_*_r0_only_router` 合并回正 router；删除 `suppress_r0_deprecation_warning`/`suppress_vendor_endpoint_warning` 配置字段及 homeserver.yaml 条目。
- 思路：**先盘点后拆除**——从 ledger 快照导出 1,381 条 → 按 `(method, suffix)` 去重得 892 个真实端点 → 对 489 条重复逐条确认 handler 同源 → 一次性删除扇出（同源即无行为差异）。`/v1` 保留策略：vendor 端点（`/_synapse/*`）本就用 `/v1`，仅删 client r0。
- 迁移成本：约 400–900 行删除 + ~20 个路由文件小改；1–2 周（含 fixture 重新生成一轮过渡）。
- 风险/兼容：**唯一的对外契约变更窗口**。Tjg 前端 + SDK fork 必须用字面路径 grep 复核（不看 route-table）。Complement 测试套若断言 r0 可达需同步。
- 验证：ledger diff 恰为 −489；SDK 侧跑一次端到端冒烟（登录/sync/发消息）；`route_ledger_default.snapshot` 重生成后人工抽查 admin/client 两域各 20 条。

**步骤 1b — manifest 自动派生（A3 核心）**
- 改动点：删除 95 个手写 `*_route_manifest()`（`assembly.rs` 内 7 个一并）；`RouteLedger::from_router(router)` 在 `main.rs` 装配完成后从 tower RouteTree（或 axum 的 `routes()` 枚举）一次性导出；保留 `RouteModule` trait 作为 feature→路由的**声明**机制（它有真实语义：profile 装配），但其 manifest 项由派生填充。
- 思路：把「注册即事实、清单即投影」固化为单向管道：`create_*_router() →(build)→ RouteLedger →(bin)→ route-table.json →(test)→ fixtures/snapshots`。手写点只剩 `.route()` 一处。
- 过渡态：若 axum 版本无法完整枚举（nest 前缀丢失），退而求其次——manifest 改为每个 router 构造时 `ledger.register(...)` 增量记录，删除独立的"抄写函数"。
- 迁移成本：**大**（约 1,741 行 manifest 删除 + 200 行派生器 + 75 个文件各删数行），2–3 周，需分 3–4 次提交。
- 风险：派生遗漏（某 router 未被 merge 即静默消失）→ 用「派生条目数 == 旧 manifest 并集数」做一次性对账测试；fixture 内容哈希大面积变化 → 一次性重基线并在 PR 描述中记录哈希变化原因（Ledger v2 审计已有此实践）。
- 验证：新增守卫测试：同一二进制启动两次导出的 ledger 逐字节相等（幂等）；golden 测试改为断言"派生结果 ⊇ SDK 声明消费的全部端点"（正向断言，替代手写清单）；`expand_under_prefixes` 单点化后 WEB-5/6/18 的别名放大自动消失。

**步骤 1c — 投影文件去 tracked 化（A12 的 A3 侧）**
- `docs/openapi/client.yaml`（72,924 行 tracked）**移出索引**：由 1b 的同一管道生成到 CI artifact/发布附件；SDK codegen 从 artifact 拉取。
- `route-table.json`（5,480 行）同理评估——它是 ledger 导出，保留 tracked 的代价是每次路由变更 diff 巨大；建议保留但明确"生成物，禁止手改"头部注释。
- 验证：CI 中 openapi 生成 job 成功 + 与旧 tracked 版做一次 diff 审计（应只反映 r0 拆除）。

---

### 阶段 2 · schema 与错误汇流（1–2 个迭代，A6 残留→A9）

> 目标：族①的存储侧 + 族②的边界侧。两者都是"多份手写清单/样板"，与阶段 1 的方法论一致（源生成 + 单向汇流）。

**步骤 2a — v12 baseline 重生成 + schema 真相源单点化（A6）**
- 改动点：把当前 `v11 + extensions` 重新生成式合并为 `00000000_unified_schema_v12.sql`（一次性，非手工编辑）；守卫 SQL 全部改为 `current_schema()` 相对判定（依赖阶段 0-6 的 lint 防复发）；`schema_health_check.rs` 的 88 条 CORE_COLUMNS 与 `schema_validator.rs` 的清单改为**由 v12 派生**（`include_str!` + 解析，推广 `baseline_tables.rs` 的正确范式到列级/索引级），删除硬编码；E.6 两个遗留 schema-blind 守卫一并处理。
- 迁移成本：中等（v12 生成有既有脚本 `build_sqlx_migration_source.py` 基础；派生器 ~200 行）；**必须同步更新内容哈希守卫** `baseline_fingerprint_...`（已知会触发，预期内）。
- 风险：sqlx `check_baseline_consolidation` / `migration_consistency` 门禁连锁；测试模板 schema 克隆路径依赖 v11 字面名（`schema-health-check.yml` 等，第三批刚改过 v10→v11）——**所有 v11 引用点需要 grep 清零**，这正说明版本字面名散落也是多真相源；路线：把 baseline 文件名收敛到一个常量/脚本变量。
- 兼容性：全新安装路径直接 v12；存量本地 dev/测试库按 README 的 `drop+recreate` 规程（无外部存量）。
- 验证：两个全新数据库分别应用 v11→v12 前/后（附录 E.4 的方法论已成熟），表/列/索引 `EXCEPT` 双向差集为空；schema-blind 守卫对非 public schema 的 db_test 真跑一次。

**步骤 2b — 错误单向汇流（A9）**
- 改动点：为每个 `*Error` 增加 `impl From<XError> for ApiError`（或带 HTTP 状态映射的 `IntoApiError`，复用已有的 `ServiceError::into_api_error()` 模式，推广到 24 个类型）；删除 137 处 `map_err(|e| ApiError::database_with_cause(...))` 样板，改为 `?` 直达；`worker/protocol.rs` 的 `ReplicationError` 等依附死子系统的错误类型随 INF-1 残留一并裁定。
- 迁移成本：低（每个 From 实现 10–20 行，删除处更多）；~1 周。
- 风险：不同转换的默认 HTTP 码差异（401 vs 403、500 vs 503）——必须对每个 From 写**路由级 golden 测试**锁状态码；`anyhow` 混用处保持原样不强并。
- 验证：新增 `error_conversion_tests.rs`：每个 From 变体 → 断言 `(status, errcode, body)`；现有 `ServiceError` 测试作模板。

---

### 阶段 3 · 装配与抽象收敛（3–4 个迭代，A5→A4→A2→A1）

> 目标：族③根治 + 分层强制 + 结构收口。顺序硬约束：**A5 先于 A4**。

**步骤 3a — 存储 trait 收敛（A5）**
- 改动点：71 个 `*StoreApi` 分三类处理：(i) **零 dyn 的 ~33 个**（STO-5 已点名 10 个 + 无 mock 的）→ 删 trait、消费者直接用具体类型（注入仍走 `Arc<ConcreteType>`）；(ii) 有 mock 消费者的 → trait 与 impl 合并到同一文件（保留 api.rs/repository.rs 的仅当 mock 真用 trait），删除跨文件签名转发；(iii) `MediaStorageBackend`/`UserStore` 等真实多实现 → 原样保留。`RoomServiceApi`/`SyncServiceApi`（SVC-10/11 单实现服务 trait）并入 (i) 评估。
- 迁移成本：**大但机械**——trait 声明 1,399 行 + 纯转发 impl 2,074 行的主要部分消失；调用面 `Arc<dyn XStoreApi>` → `Arc<XStorage>` 是类型替换，编译器全程引导；10% 人工裁定。3–4 周分批（按特性目录切，每批可独立编译验证）。
- 风险：测试缝丢失——**每个 trait 删除前先 grep `dyn` 与 mock 引用**（分类规则已保证）；sqlx 编译查询的面板在类型替换后不变。
- 验证：每批 `cargo check --workspace --all-features` + `cargo test --workspace --lib`；trait 计数脚本作为棘轮（基线只降不升，同 fmt ratchet 的零债务语义）。

**步骤 3b — DI 泛型化（A4）**
- 改动点：定义 `trait AuthSource { fn state(&self) -> &AppState; ... }`，11 个 context 各 impl 一次（~110 行）；`FromRequestParts<S> where S: AuthSource` **每类抽取器 1 个泛型 impl 取代 21 个**（−596 行）；context 字段按 3a 结果瘦身，剩余字段生成化（`FromRef` derive 或 build 脚本，消灭 343 行手工 clone）。`ServiceContainer → AppState` 合并为单一构造点。
- 迁移成本：中（extractors 重写 ~2 周，context 瘦身取决于 3a 深度）。
- 风险：泛型 impl 对 `FromRef` 链的 bound 传染（编译期可见，可控）；行为零变更（纯类型层）。
- 验证：现有 `extractors` 与 context 单测全绿；新增测试断言 `AdminUser` 与 `RoomContext` 两条路径的鉴权行为一致（防泛型化引入的语义合并错误）。

**步骤 3c — 分层强制（A2）**
- 改动点：新增 CI lint（可并入 `repo-sanity`）：`src/web/` 下禁止 `use synapse_storage::`（白名单：纯 DTO/类型 import，如 `RoomVersion` 等 common 化后移出 storage）——白名单应**趋向 0**，棘轮基线。18 个绕过 service 的路由文件按裁定补薄 service 或下沉逻辑。
- 迁移成本：lint 半天；18 个文件搬迁 ~1–2 周（可拆单文件小 PR）。
- 风险：部分 admin 路由直连 storage 是合理性能选择（读路径无业务规则）——若裁定保留，用显式 `#[allow(layer_bypass)]` 注释 + 清单文件记录，而非沉默。
- 验证：lint 红/绿双向证明（故意加一行 `use synapse_storage::...` 于路由文件）。

**步骤 3d — 根 crate 拆分（A1 收口）+ 可见性收敛（A10）**
- 改动点（A1）：`src/web/`（+ 其 fixture）独立为 `synapse-http` crate，根 crate 收缩为 **bin + wiring 壳（目标 <5k 行）**；`server/`、`tasks/` 归入 `synapse-runtime`。这是 ①②③ 全部收敛后的**机械搬迁**——因为元数据/字段/trait 已单源，搬迁不再是考古。
- 改动点（A10）：与 3d 同期做 import 路径唯一化：删除 `pub use x::*;` 扁平块与 16 处 `allow(ambiguous_glob_reexports)`、`prelude.rs` 兼容层；调用点一次性改规范路径（80+ 文件 import 变更，rustfmt 友好）。
- 迁移成本：高（跨 crate 移动涉及 `Cargo.toml`、feature 穿透、benchmark/bin 依赖）——4–6 周，**仅当阶段 1–3 前三步完成**才划算。
- 风险：feature 矩阵重构与 CFG-11（`server` 伪 feature）裁定强绑定：crate 拆分时 feature 声明应随能力走，不再用元 feature `all-extensions` 兜底；CI 的 `--no-default-features --features server` 叙事需同步修正（docker/Dockerfile、deploy.sh、complement、harness 5 处）。
- 验证：`cargo check --workspace --all-features` + docker 构建矩阵在具备 Docker 的环境补验（无法本地验证的项在 PR 中显式标注「环境外未验证」，禁默过）。

---

### 阶段 4 · 长期演进（门禁重设计，A11 + 防腐化）

| 改动点 | 思路 | 验证方式 |
|---|---|---|
| **A11：`deny(missing_docs)` 重设计** | 删除 15,571 行零信息模板；对**跨 crate 公共 API**（synapse-common/services/storage 的对外面）保留 `warn` + 内容审查（禁止自指 `See [x].`），对 crate 内部 `allow`。改写 `check_missing_docs_ratchet.py`：基线=真实待补项，只降不升 | 脚本对"自指/复读型"注释计为违规（红测试）；基线从 ~15.5k 逐步归零 |
| **注释棘轮** | 新注释不得为 `The \`x\` field.` 模式（CI grep 守卫） | 故意提交复读注释证明会红 |
| **测试文件 mod 守卫**（防 TST-4 复发） | CI 扫描 `tests/{unit,integration}/*.rs` 必须被 `mod.rs` 声明 | 新增未声明文件 → 红 |
| **职责级单源扫描** | 把 `test_isolation_unification_tests` Guard 7 从"仅 clone 子句"扩到 schema 创建/取池/删 schema/URL 解析（TST-1/2） | 同铁律 8 |
| **feature 矩阵真实化** | shipped（docker）=tested（CI）=default 三集合对齐后，每个 feature 至少一条 CI 车道真正开关切换 | 恒绿 feature 检测（`cargo hack --feature-powerset` 抽样） |

---

## 4. 里程碑与依赖顺序总览

```
阶段0 止血 ─────────┐ (0-1/0-2 裁定即 1a 的前置；0-6 lint 是 2a 的保险丝)
阶段1 A7→A3→投影去tracked ─┤ (fixture 重基线在此一次性完成)
阶段2 A6(v12) + A9 ───┤ (v11→v12 的字面名清零依赖阶段1 的教训：版本常量单源)
阶段3 A5→A4→A2→(A1+A10+CFG-11) ─┤ (3a 是 3b 的前置；3d 最后)
阶段4 A11 + 防腐守卫 ──┘ (代码量冻结后启动)
```

总删减预期：净 Rust/SQL/脚本 **−6,000~−9,000 行**，生成物 tracked **−75,000+ 行**，ledger 条目 −489，context 字段 −40% 以上，样板 −1,400 行（manifest 1,741 + extractor 596 + map_err 137 部分重叠计）。

---

## 5. 全局风险登记

1. **对外 HTTP 契约变更集中于阶段 1**——唯一可能"伤到"SDK 的窗口，拆前必须以 manager 源码字面路径复核（route-table 只增不减，不作判据）。
2. **fixture/快照大面积重基线**会掩盖真实回归——要求每次重基线附 diff 审计记录（哈希变化 + 抽样人工比对 ≥40 条），禁止直接 `cargo insta accept`。
3. **并发改同一个工作树**（本审计期间已观察到跨会话 stash 冲突）——阶段 3 的机械重写建议按特性目录切 PR，并避免多会话同时触碰 `wiring/`、`context.rs`。
4. **无 Docker 环境导致部署项无法本地验证**（0-7、3d 的构建矩阵）——必须在 PR 中显式标注未验证项，不得以"门禁未红"默过。
5. **sqlx 编译期查询 + v12 重生成**的连锁：v12 前 `cargo sqlx prepare` 类基线需同步，且 `.sqlx` 缓存目录当前未被 gitignore（应补）。

---

## 6. 一句话总结

该路线沿「先止血正确性缺陷 → 再切断信息多源（A7 拆乘数、A3/A6 建单源生成管道、A8 合部署面）→ 再收敛手工装配（A5 删无收益抽象、A4 泛型化 DI、A9 汇流错误、A10 路径唯一化）→ 用 A2 的编译期分层守卫锁住结构 → 最后以 A1 的 crate 拆分把『薄壳』从文档叙事变成物理事实 → A11/阶段 4 用内容型/可证伪门禁防腐」的单一依赖链推进：每一条架构缺陷要么被其根因族消除，要么被机器守卫永久拦截复发，故「2. 架构层面的问题」将被彻底解决而不是局部缓解。
