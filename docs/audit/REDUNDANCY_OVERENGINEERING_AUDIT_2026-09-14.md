# 冗余与过度开发审查报告（架构层面 + 逐领域清单）

- 日期：2026-09-14
- 范围：整个 workspace（根 crate `src/` + 6 个 workspace crate + `tests/` + `migrations/` + `scripts/` + 仓库卫生）
- 方法：**只读审查，未修改任何源文件**。6 路并行深挖（web 路由层 / feature-cfg / services / storage / 基础设施 / 测试与门禁）+ 主审独立复核。
- 取证口径：所有结论基于 `wc -l`、`grep -rn` 引用计数、`git ls-files`、`du`；标注「**已独立核验**」的条目由主审亲自复跑命令确认。

---

## 1. 执行摘要

### 1.1 量化基线

| 指标 | 数值 |
|---|---|
| 生产源码（7 个 crate 的 `src/`） | **355,111 行**（`main` @ `6ba7c457`，含 §15.2 + §16 + fmt 修复）；清理分支 `optimization/redundancy-cleanup-2026-09-14` 已删 149 文件 → **315,121 行**（未合入 main，见附录 C） |
| 根 crate `src/` | 78,637 行（其中 `src/web/` 占 **69,790** 行 = 88.8%） |
| `tests/` | 99,802 行 |
| 文档注释行 | 29,749 行（占生产源码 **8.4%**） |
| 其中**零信息**注释（自指 `See [x].` + `The \`x\` field.`） | **15,571 行**（占全部文档注释 **52%**） |
| `#[cfg(feature = …)]` 站点 | **1,009** 处 |
| 取反门控 `cfg(not(feature = …))` = 双实现 | **52** 处 |
| `#[allow(dead_code)]` | **151 处**（main 实测，126 处集中在 `api_doc/schemas.rs`）→ 清理分支删除 `api_doc/` 后降至 25–34 处（附录 C.5 验证 clippy **0 warning**）；口径区分见附录 F |
| cargo feature 总数 | **19** 个 |
| storage 层 trait 数 | **71** 个（其中 **69** 个只有 ≤1 个非 mock 实现） |
| 错误类型枚举 | **24** 个 |
| 路由装配点 | `.route()` 994 次 / `*_manifest()` 函数 95 个 / 1,741 行 |
| 迁移 | 78 个 SQL / 12,616 行 / 530 个 `CREATE TABLE` / 835 个 `CREATE INDEX` |
| tracked markdown | **308 个文件 / 54,743 行**，其中 166 个是 audit/report/plan/summary/review 类 |
| 近 30 天提交 | 500 个（占全部 1,367 个提交的 37%），其中 86 个为纯文档提交 |

### 1.2 一句话结论

**这个项目的问题不是"某处多写了几行工具函数"，而是四类系统性结构冗余：**

1. **同一份信息被手工维护 3–5 遍**（路由元数据、DI 依赖图、schema、错误类型、模块可见性）；
2. **为"未发布项目"保留了大量向后兼容层**（r0/v1 路由别名、legacy 密码/token 哈希、`all-extensions` 元 feature、`backward-compatibility` prelude），而这恰恰是本仓库 `AGENTS.md` 铁律第 1 条明令禁止的；
3. **整套从未被装配的投机子系统**（worker 集群抽象 ≈1.8k 行、runtime-ddl 第二套 DDL ≈1.3k 行、openapi-docs ≈12.8k 行、`src/security/`、`web/streaming.rs`）；
4. **门禁与文档自身的通胀**（15.5k 行零信息注释用于满足 `deny(missing_docs)`；1.9MB 路由 ledger fixture；2.3MB 生成式 OpenAPI；308 个 tracked md；一处 CI 步骤因 `--exclude synapse_worker` 指向不存在的 package 而**从未真正运行过**）。

**已识别的可直接删减规模（六路深挖全部完成）：约 50,500 行 Rust/脚本/SQL + 约 137,000 行生成/固化产物 + 78 个迁移文件中的 41 个，另有 108 个跟踪状态与 gitignore 规则违规。**

其中**已确认从未运行**的测试代码（约 6,000 行）与**已确认从未装配/从未调用**的生产代码（约 12,000 行）合计近 18,000 行 —— 这是本次审查最值得注意的一类：它们不产生 bug，但持续制造"这里已经测过 / 已经实现"的错误信心。

**另有 5 处不是"多余"而是"危险"的正确性隐患**，优先级应高于任何删减：
- **STO-11**：`UserStorage::set_account_data` 写入 `user_account_data` 表，而读取一律走 `account_data` 表 ⇒ 写进去的数据无人能读（静默丢弃）。
- **STO-12**：两个同名 `delete_events_before` 的过滤条件语义相反（一个只删非状态行，一个只删远端事件），调用方极易选错。
- **STO-10 / SVC-2**：`push_rules` 同表被两套 storage/service 以不同方法名与返回类型双写。
- **STO-13**：`room_directory` 同表被 `RoomStorage` 与 `DirectoryStorage` 双写，`is_public` 可被互相覆盖。
- **CFG-1**：release 构建下 Olm pickle key 的宽松入口返回全零密钥，而生产 E2EE 路径（`session.rs:44/:77`）调用的正是它。

### 1.3 门禁可证伪性问题（铁律第 8 条）

本仓库自己的铁律第 8 条要求"任何检查类门禁必须用故意制造的违规证明它真的会失败"。审查发现**当前存在 1 个红灯门禁、4 类恒过/空转门禁、1 个空门禁**：

| 门禁 | 位置 | 状态 | 证据 |
|---|---|---|---|
| **fmt ratchet** | `scripts/check_fmt_ratchet.sh`，`ci.yml:214` | 审查时红（提交态）→ **已修复**（见§15-16连带） | 审查时实测 `fmt debt: current=2 baseline=0` → `exit=1`；offending 为 `tests/unit/test_isolation_unification_tests.rs:619`（2 处），`git diff HEAD` 为空 ⇒ 未格式化代码被提交。**已修复**：§15.2 与 §16 代码重构导致连带格式漂移，由 `cargo fmt --all` 修复并以 commit `b0e8ed9e` 提交至 main，脚本现输出 `fmt debt: current=0 baseline=0`（`exit=0`）。附录 C.5 原 `9f68b4b3` 仅修复工作树中独立 fmt 问题（未合并至 main），main 分支的修复为本轮。 |
| supply-chain | `ci.yml:140` → `scripts/ci/supply_chain_gate.sh` | 未动（需产品决策） | repo-sanity job 内**无 `cargo install`**；工具缺失时打印 "not installed, skipping" 后 `exit 0`（脚本 `:60-62,:94-96,:98`） ⇒ 标准 runner 上**恒绿**；真正执行的是 `ci.yml:662`（仅 PR/schedule/main-push）。develop 分支等于没有该门禁。 |
| performance-baseline | `drift-detection.yml:346-383` | **已修复（第三批）** | **已移除**（commit `9e847a42`）：4 个目标迁移全部不存在（`git ls-files \| grep -i performance_indexes` 无输出），原本每轮 `::warning::Missing … (skipped)`、`failed=0` ⇒ **100% 空转**。第三批将整段 for 循环替换为一句 `::warning::` 声明"该门禁随冗余清理已移除"（commit `9e847a42`）。 |
| schema-health-check | `schema-health-check.yml:73` | **已修复（第三批）** | **已修** `_v10.sql` → `_v11.sql`（commit `9e847a42`）：原本引用不存在文件、每次 push/PR **必红**；现指向实际存在的 `00000000_unified_schema_v11.sql`。 |
| db-migration-gate | `db-migration-gate.yml` | `grep -c "P1-8 placeholder"` = **18** 个仅含 `echo '::warning::TODO…skipped'` 的步骤（真实 `cargo test` 行被注释掉） | 18 个步骤只产出警告 |
| `cargo test --doc` | `TESTING.md:96,389,407,511` 仍按根 crate 口径记载 | 根 crate Rust doctest 块 = **0**（工作区 7 个块中 5 个 `ignore` + 2 个 `no_run`） | 空门禁；CI 已在 `ci.yml:275-277` 修正为 `--workspace`，文档未同步 |

另有 `.github/workflows/ci.yml:739` 的**坏步骤（已修复）**：

```yaml
# 原状态（已修复）
- name: Build wasm (if applicable)
  if: matrix.profile.name == 'all-extensions'
  run: cargo build --release --workspace --exclude synapse_worker --locked || true
```

`--exclude` 接受 **package name**，而 workspace 内不存在名为 `synapse_worker` 的 package（`Cargo.toml:2` 的 package 名为 `synapse-rust`；`synapse_worker` 只是 `src/bin/synapse_worker.rs` 的 bin target 名）。该命令在 resolve 阶段即失败，并被 `|| true` 吞掉 —— **该步骤从未执行过任何构建**。步骤名 `Build wasm` 与命令（Rust release build）亦不符。**已独立核验**（`grep -n "^name" Cargo.toml */Cargo.toml` 无 `synapse_worker`）。

**已修复**（commit `9e847a42`）：改为 `cargo build --release --bin synapse_worker --locked`（无 `|| true`），确保实际构建并能在失败时可见。

> 推论（沿用 `AGENTS.md` 铁律 8）：check 类门禁若长期全绿（或长期必红），先怀疑它没在工作。本仓库同时存在这两种失效形态。

---

## 2. 架构层面的问题

以下 12 条是**根因级**问题，第 3 节的逐条冗余大多是它们的症状。

### A1. 根 crate 并非"薄壳"，HTTP 层体量已与业务层相当

- 根 crate `src/` = 78,637 行；`synapse-services` = 94,188 行；`synapse-storage` = 112,975 行。
- 其中 `src/web/` = **69,790 行**（根 crate 的 88.8%），`src/web/routes/` = 51,882 行 / 143 个文件。
- `AGENTS.md` 声称 `src/services/`、`src/storage/`、`src/common/` 是"只允许 re-export 的薄壳"——但 `src/services/` **目录根本不存在**，`src/storage/mod.rs`（66 行）自述其存在理由是"服务测试套件、benchmark、二进制 crate 仍走这个 facade"，`src/cache/mod.rs`（4 行）与 `src/worker/mod.rs`（2 行）是**零消费者**的纯 glob 转发壳（`src/worker/mod.rs` 已独立核验：`grep -rn "crate::worker::" src tests benches` = 0）。
- 结论：项目实际是「一个巨型 web crate + 6 个库」，而不是 AGENTS.md 描述的「薄 HTTP 壳 + 厚业务层」。

### A2. 声明的 `route → service → storage` 分层未被强制

- **18 个路由文件直接 `use synapse_storage` 且完全不 `use synapse_services`**（已独立核验），例如：
  `src/web/routes/app_service.rs`、`feature_flags.rs`、`admin/room/mod.rs`、`admin/notification.rs`、`admin/audit.rs`、`admin/token.rs`、`admin/report.rs`、`admin/retention.rs`、`event_report.rs`、`rendezvous.rs`、`delayed_events.rs`、`qr_login_token.rs`、`background_update.rs`、`sliding_sync.rs`、`handlers/room/events.rs`、`handlers/room/state.rs`、`extractors/auth.rs`、`space/types.rs`。
- 另有 47 个路由文件引用 storage 类型（`grep -rlE "synapse_storage::|Storage\b" src/web/routes` = 47）。
- 正面：`src/web` 内**没有**裸 `sqlx::query`（已独立核验为 0），说明"不越过 storage 写 SQL"这条守住了；被打破的是"必须经过 service"。
- 有服务层存在的意义被削弱：`synapse-services/src/rendezvous_service.rs`（17 行）是一个**纯 re-export**，文件头自述「更高层业务规则将在后续批次加入；目前唯一职责是公开 re-export」——这正是 `AGENTS.md` 铁律 1 禁止的"先留着以后可能有人用"的占位。

### A3. 路由元数据被维护成 5–8 份表示

一次路由变更需要同步的位置：

| # | 表示 | 规模 | 位置 |
|---|---|---|---|
| 1 | 真实 `Router` 注册 | 994 次 `.route()` | 各 `create_*_router()` |
| 2 | 模块级 `*_route_manifest()` | **95 个函数 / 1,741 行** | `src/web/routes/*.rs` |
| 3 | `RouteLedger` / `RouteEntry` | 419 行 | `route_ledger.rs` |
| 4 | `RouteModule` trait + `ProfileFlags` | 394 行 | `route_module.rs` |
| 5 | SDK 契约导出 | 278 行 + 262 行 bin | `ledger_export.rs` / `src/bin/synapse_ledger_export.rs` |
| 6 | ledger fixture | **68,446 行 / 1.9 MB / 6 个 JSON** | `tests/unit/fixtures/ledger_export{,_sdk}/` |
| 7 | 路由快照 | 1381 条 | `tests/integration/snapshots/route_ledger_*.snapshot` |
| 8 | 生成的 OpenAPI | **72,924 行 / 2.3 MB** | `docs/openapi/client.yaml`（tracked） |

- `assembly.rs` 内部还有 **6 个 manifest 函数**（`base_route_manifest`、`declared_route_manifest_for`、`declared_route_manifest_for_profile`、`top_level_inline_manifest`、`assembly_compat_manifest`、`vendor_route_manifest`），与同文件 `create_router` 里的 42 次 `.merge()` 是两套独立枚举。
- 具象证据（**已独立核验**）：`src/web/routes/friend_room.rs` 中 `create_friend_router` 有 **84 次 `.route(`**，`friend_route_manifest` 又手抄了 **93 条路径字面量**；两者必须人工保持一致。
- `RouteEntry` 自身已有 3 个自证冗余的字段（**已独立核验**）：`query_params` 填充率 **0/1381**、`with_auth` 调用 **1 次**、`total_entries ≡ unique_tuples`（`assembly_route_tests.rs:51,335` 用测试断言其恒等）。`ledger_export.rs:29-49` 曾因同样理由删除过 `module`/`status` 字段，这两个是遗漏。
- `route_ledger.rs:133` 对每条路径做 `Box::leak`，把 1381 条路径**永久泄漏**为 `&'static str`。

### A4. 依赖注入图被手工复制 4 遍

```
ServiceContainer (23 个顶层字段，嵌套分组)
  → wiring/{core,admin,rooms,e2ee,sso,accounts,federation,extensions}.rs (1,633 行)
  → AppState
  → src/web/routes/context.rs: 11 个 context struct / 259 个 pub 字段 / 1,009 行
  → 11 个 impl FromRef<AppState> 共 332 行机械 .clone()
```

- `AdminContext` 一个结构就有 **58 个字段**，`RoomContext`/`FederationContext` 各 37 个。
- 121 个 `FromRequestParts` 实现 = 3 个抽取器类型 × 7 个 context 类型，`extractors/auth.rs` 因此膨胀到 932 行；把其中 596 行是**近乎逐字相同**的 `from_request_parts` 样板。一个泛型 impl + 一个 `trait AuthSource` 可以替代。
- **这套手工 DI 是本仓库"reserved / constructor parity"死字段泛滥的结构性根因**：新增一个服务必须穿 4 层，为了不改构造签名就留下从不读取的 `Arc` 字段（见 3.3 INF-13 / CFG-14）。

### A5. 存储层 trait-per-store：71 个 trait，69 个无多态价值

- `synapse-storage` 定义 **71 个 `pub trait *StoreApi`**；其中 **69 个只有 1 个非 mock 实现**（仅 `MediaStorageBackend` 有 3 个、`UserStore` 有 2 个）。**已独立核验**。
- 12 个特性目录采用固定五/六件套：`api.rs`（trait 声明）+ `repository.rs`（唯一实现）+ `models.rs` + `mod.rs` + `db_tests.rs` + `tests.rs`。把 trait 声明与唯一实现拆到两个文件，不产生任何抽象收益。
- 代价是 `Arc<dyn XStoreApi>` 贯穿整个 DI 图，并直接导致 A4 的 context 字段爆炸。

### A6. 两套 schema 真相源（违反铁律 2）

- 权威源：`migrations/`（78 个 SQL / 12,616 行 / 530 个 `CREATE TABLE` / 835 个 `CREATE INDEX`）。
- 第二套：`synapse-services/src/database_initializer/tables.rs` **1,249 行 / 35 个 `CREATE TABLE` + 44 个 `CREATE INDEX`**，整文件 `#[cfg(feature = "runtime-ddl")]`。
- `runtime-ddl` feature **没有任何 CI / docker / Makefile / 脚本启用过**（只在 `--all-features` 下编译）。**已独立核验**。
- 值得注意：`docs/artifacts` 里 2026-09-03 的代码审查报告 P1-5 曾把「数据库双重 DDL 定义」标为"✅ 已修复"，但现状两份定义都还在——**该"已修复"结论不成立**。
- 反例（正确范式）：`synapse-storage/src/baseline_tables.rs`（178 行）用 `include_str!` 在编译期解析 `00000000_unified_schema_v11.sql` 得到表清单，这是**单一真相源的正确做法**；但 `schema_health_check.rs` 另外硬编码了 111 处「关键列」字面量，`schema_validator.rs` 又有 50 处，列级 schema 仍是多份。

### A7. 兼容残留链：r0/v1 路由别名 → 弃用告警 → 抑制告警的配置

这是铁律 1 的完整违规链路：

1. **路由面按版本前缀三倍扇出**：`/_matrix/client/v3` 350 处、`/v1` 269 处、`/r0` **259** 处（已独立核验）。
   - ledger 快照 1381 条去掉前缀后只剩 **892 个不同的 `(method, suffix)`，即 489 条（35%）是同 handler 的重复注册**，其中 r0 占 294 条。
   - `assembly.rs:616-619` 把同一个 `create_auth_compat_router()` 分别 nest 到 r0 与 v3；`:658-663` account 实例化 3 次；`:684-690` directory 同理。
   - 11 个 `*_NEST_PREFIXES` 常量里 **10 个含 r0**。
2. **代码里自认该删**：`route_ledger.rs:210` 统计 `r0_route_count`，`assembly.rs:393` 打印 "scheduled for removal"。
3. **为静音自己的告警再加配置**：`synapse-common/src/config/server.rs:297,309` 定义 `suppress_r0_deprecation_warning` / `suppress_vendor_endpoint_warning`，唯一用途是关掉上面这条"294 条 r0 该删"的告警。**已独立核验**。
4. 另有 59 个 `compat` 命名的项目（`create_*_compat_router`、`*_compat_relative_routes`、`account_compat.rs`、`auth_compat.rs`、`assembly_compat_manifest`）与 2 个 `create_*_r0_only_router`。

> 项目未发布、无外部客户端，`/r0`（Matrix 早已废弃的版本前缀）没有任何必需理由。整条链可一次性拆除。

### A8. 两套 docker 部署目录，配置已漂移

- `docker/` 与 `docker/deploy/` 各自持有 `docker-compose.yml`、`config/homeserver.yaml`、`config/rate_limit.yaml`，**三份配置全部不一致**（`diff -q` 均报 differ），行数 248 vs 247。**已独立核验**。
- `docker/deploy/` 目录实测 **18 GB**，其中 `backups/` 有 19 个 `synapse_backup_*` 时间点副本（未 tracked，但在工作区内，且是 `grep -r` 反复超时的原因）。
- 这与已经修复的"迁移双副本漂移"（`2b16dc3c` 删除 `docker/deploy/migrations/`）是**同一类问题的另一个实例，且尚未处理**。

### A9. 错误类型 24 个 + 137 处 `map_err` 样板

- workspace 内 `pub enum *Error` 共 **24 个**（`ApiError`/`ServiceError`/`CacheError`/`ConfigError`/`CryptoError`/`FederationClientError`/`StateResolutionError`/`ReplicationError`/`TranslationError`/`LivekitError`/`TagsError`/`MasError`/…）。
- `synapse-common/src/error.rs` 单文件 **2,113 行**；`synapse-services/src/error.rs` 另有一套带领域注释分组的 `ServiceError`。
- **没有任何 `impl From<ServiceError> for ApiError`**（已独立核验），因此 137 处 `map_err(|e| ApiError::database_with_cause(...))` 手工样板成为唯一转换途径。
- 24 个错误类型里，`worker/protocol.rs:419 ReplicationError`（worker 未部署）、`translation_service.rs:347 TranslationError`（翻译是私有的非 Matrix 扩展）等的存在只是过度切分的产物。

### A10. 模块可见性靠 glob + `allow(ambiguous_glob_reexports)` 维持

- workspace 内 **197 处 `pub use …::*;`**；其中 **16 处**触发 `ambiguous_glob_reexports` 并被显式 `#[allow]` 压制 —— 即这些 glob **确实存在名字冲突**。
- 具象冲突（**已独立核验**）：`synapse-services/src/room/mod.rs:47-48`
  ```rust
  pub use crate::directory_service::{DirectoryRoom, DirectoryService};
  pub use crate::typing_service::{TypingService, TypingUser};
  ```
  把**定义在 crate 根**的两个服务反向 re-export 进 `room`，只为让 `lib.rs` 的 `pub use room::*;` 覆盖"legacy flat re-export"。结果是同一个类型有两个可达路径（`synapse_services::directory_service::DirectoryService` 与 `synapse_services::room::DirectoryService`），这正是 `allow(ambiguous_glob_reexports)` 要压制的东西。
- `synapse-services/src/prelude.rs:1` 自述 **"Backward-compatibility prelude"**；`lib.rs:178` 注释 **"backward-compatibility flat re-exports"** / **"keep the legacy root-level paths working"** —— 逐字的铁律 1 违规。

### A11. `deny(missing_docs)` 逼出 15,571 行零信息注释

- 各 crate 均声明 `#![deny(missing_docs)]`。为通过该门禁产生的注释：
  - `/// The \`X\` field.` / `/// The \`X\` struct.` 形式：**10,059 行**
  - `/// See [\`x\`].` 自指转发：**5,512 行**
- 存在**连续两行完全相同**的注释，例如 `src/tasks/mod.rs:47-48` 两行 `/// See [\`new\`].`、`src/server/mod.rs:899-900` 两行 `/// See [\`metrics_collector\`].`（**已独立核验**）。
- 这类注释的信息量为零，且会把 Rustdoc 的"缺文档"信号彻底淹没 —— 门禁形式上是绿的，实质上是空的（铁律 8 的另一种形态）。累计约 4.4% 的生产源码行是这种噪声。

### A12. 仓库卫生：生成物、报告与陈旧工作树混入工作区

| 对象 | 体量 | git 状态 | 问题 |
|---|---|---|---|
| `docker/deploy/` | **18 GB**（19 份 `backups/`） | 21 个文件 tracked，backups 未 tracked | 工作区被备份副本污染；`grep -r` 反复超时 |
| `.claude/worktrees/optimization+audit-2026-07/` | **365,355 行 Rust / 39 MB** | 未 tracked（gitignored） | 已注册的旧 worktree（commit `488d9888`），体量≈全部生产源码；会污染 grep/IDE/索引（本审第一轮 grep 即命中其陈旧 `ci.yml`） |
| `docs/` + 根目录 md | 308 个 tracked md / 54,743 行 | tracked | 166 个是 audit/report/plan 类 |
| `.scratch/` | **97 个 tracked md** | tracked | 代理临时工作区入库 |
| `coverage/` | 3 个 tracked md | tracked（`.gitignore:98` 已忽略目录） | 先提交后忽略的残留 |
| `artifacts/` | 5.3 MB / 57 项（48 md + 27 log） | 0 tracked（正确） | 仅工作区噪声 |
| `docs/openapi/client.yaml` | 72,924 行 / 2.3 MB | tracked | 生成物入库（见 A3/CFG-3） |

- 近 30 天 500 个提交中 86 个（17%）为纯文档提交；工程吞吐有相当比例消耗在报告迭代而非代码。
- **建议**：`git worktree remove .claude/worktrees/optimization+audit-2026-07`；把 `docker/deploy/backups/` 移出版本库工作区；`.scratch/` 与 `coverage/` 的 tracked md 从索引移除。

---

## 3. 逐领域冗余与过度开发清单

> 置信度：high = 引用计数为 0 或已独立核验；medium = 需保留部分语义，删除前需 `cargo check --all-targets`。

### 3.1 WEB — HTTP / 路由层（`src/web/`，69,790 行）

| ID | 标题 | 关键证据 | 可删 LOC | 置信 |
|---|---|---|---|---|
| WEB-1 | **整个 `directory.rs` 是死模块** | `directory.rs:1 #![allow(dead_code)]`；9 个 pub 项（`:98/:156/:181/:210/:231/:289`）零调用；未被任何 router 引用；活实现走 `directory_reporting.rs`（其 `:17/:27/:31` 助手与 `directory.rs:32/:42/:46` 逐字相同）。**已独立核验**：`remove_room_alias`/`search_public_rooms` 的"21/13 处引用"指向同名但不同作用域的函数（`RoomService::remove_room_alias`、`DirectoryService::search_public_rooms`、storage 方法），`directory.rs` 的路由处理器调用点为 0 | 334 | high |
| WEB-2 | **版本前缀扇出：489/1381 条冗余，r0 占 294** | `route_ledger_default.snapshot` 1381 条 → 去前缀后 892 个不同 `(method,suffix)`；`assembly.rs:616-619` 同一 router nest r0+v3；`:658-663` account ×3；`:684-690` directory ×3；11 个 `*_NEST_PREFIXES` 中 10 个含 r0 | 400–900 | high |
| WEB-3 | ledger 是路由元数据的第三份手写副本 | 95 个 manifest 函数 / 1,741 行；`push.rs` 一个文件内四份编码（`:13` 真实注册 / `:46` relative 清单 / `:64` absolute 清单 / `:78` manifest）；手写 1381 条 vs 实际 994 次 `.route(`；`route_ledger.rs:133` 每条路径 `Box::leak` | 600–1500 | med-high |
| WEB-4 | **"类型化 context" 制造 939 行纯样板** | `context.rs` 1,009 行（259 字段 + 343 行 `FromRef` 机械 clone）；`extractors/auth.rs:203-799` 596 行近乎逐字相同的 `FromRequestParts<XContext>`；`auth.rs:195-201` 注释自认是迁移中间态 | 550–800 | high |
| WEB-5 | `friend_room.rs` 手抄多份 manifest | 1,177 行；`:22` `create_friend_router` 323 行 / 84 次 `.route(`；manifest `:343-449` 手工分块重抄 r0/v1/v3/vendor；93 条 manifest 仅 31 个不同 suffix（67% 重复）。同仓已有 `expand_under_prefixes`（`key_backup.rs` 已用）本文件不用 | 200–300 | high |
| WEB-6 | `key_backup` 用 9 个克隆 handler 实现第二套 URL 编码 | `:37` 自认 "Legacy/MSC-compatibility"；9 个 `*_legacy`（`:365/:405/:443/:486/:529/:567/:599/:629/:660`）共 66 行，与 spec 版仅差 `Query<VersionQuery>` vs `Path<RoomId>`（**版本号被当 RoomId 解析**）；9 条 legacy 路径 ×3 前缀 = 27 条 ledger 条目 | 110 | high |
| WEB-7 | 35 处内联 limit 解析，专用 `Pagination` 提取器零消费者 | `extractors/pagination.rs:4-6/:48`（98 行）唯一引用是 `mod.rs:45` 的 re-export 本身；35 处内联且上限/默认值有 6+5 种；另有两套常量 `DEFAULT_PAGE_LIMIT`/`MAX_PAGE_LIMIT` 与 `constants.rs:23 MAX_PAGINATION_LIMIT`；3 个同名 `fn default_limit()` | 133 | high |
| WEB-8 | MSC4108 rendezvous 两套并存，共 10 条路由 | `rendezvous.rs:23-33`（v1，JSON + 自定义头）vs `msc4108_rendezvous.rs:28-37`（unstable，text/plain + ETag）；同名函数成对；`assembly.rs:563-564` 同时 merge；项目自审 `BACKEND_ROUTE_ISSUES_AUDIT_2026-09-13_REVISED.md:151-153` 已确认分裂且 SDK 只用 unstable | 165 | med-high |
| WEB-9 | `dm.rs` 用 `#[cfg]` 双实现同一 DM 职责 | `:360-386`（friends，委托 friend_room_service）vs `:386-416`（not(friends)，用 room_service 重新实现）；`:441-455` vs `:457-488` `load_dm_partner_info` 两处定义；两份行为**已漂移** | 70–110 | medium |
| WEB-10 | `routes/mod.rs` 268 行同义反复测试 | 547 行 = 生产 280 + 测试 268；6 个 `#[cfg(test)]` 共 16 个 "structure" 测试只断言**局部字面量数组**（如 `assert_eq!(shared_paths.len(), 8)`），不引用任何生产符号 → 删掉被测 router 仍全绿（违反铁律 8） | 368 | high |
| WEB-11 | `admin/room/management.rs` 6 个纯转发 handler，且丢弃鉴权结果 | `:278/:290/:302/:319/:330/:342` body 均为 `Ok(Json(x_internal(...).await?))`；签名为 `_admin: AdminUser` —— 提取只为副作用、值被丢弃；全仓同类候选 218 个 | 45（推广 150–250） | high/medium |
| WEB-12 | 2 个中间件完全无引用 | `middleware/security.rs:12 logging_middleware`（32 行）、`:99 metrics_middleware`（14 行）引用数 0；活的是 `:114 request_debug_middleware` 与 `:195 request_id_middleware` | 46 | high |
| WEB-13 | 单行/影子转发 helper 链 | `resolve_request_id` 4 个入口同一实现（`utils/auth.rs:10` 真 + `admin/audit.rs:159`/`feature_flags.rs:160`/`telemetry.rs:379` 转发）；`middleware/auth.rs:15-17 extract_token` 1 行转发；`federation_auth.rs:327-345 canonical_federation_request_bytes` **遮蔽同名真实现**只加 tracing；15 处 `<XContext as FromRef<AppState>>::from_ref(&state)` 样板 | 100 | high |
| WEB-14 | ledger 自身 3 处已自证冗余的字段 + 兼容开关 | `query_params` 填充率 **0/1381**、`with_auth` 调用 **1 次**、`total_entries ≡ unique_tuples`；`assembly.rs:391-403` 的 `SUPPRESS_R0_DEPRECATION_WARNING` 唯一作用是静音"294 条 r0 该删"。同文件 `ledger_export.rs:29-49` 曾因同样理由删过 `module`/`status`。**已独立核验** | 40 | high |
| WEB-15 | `room_access.rs` 的 `_ctx`/`_admin` 孪生 helper | `:42 ensure_room_member_ctx` vs `:77 ensure_room_member_admin` body 逐字相同；`:59` vs `:94` 同样；文件头已抽出类型无关内核 `is_member_via`（`:10`）却又加同义壳 | 35 | high |
| WEB-16 | 16 个 >800 行 god module | `friend_room.rs` 1177（27 handler/11 struct/1 router）、`federation/keys.rs` 1147、`admin/user.rs` 1074、`key_backup.rs` 1014（**43 handler**）、`context.rs` 1009、`worker.rs` 976（**3 router**）、`room_summary.rs` 927（**4 router**）、`module.rs` 921（24 handler/19 DTO）。对照：`space.rs` 已示范正确拆分（`space/` 5 文件） | 结构性 | medium |
| WEB-17 | token 响应 4 种手写形状且已漂移 | 单一来源 `formatting.rs:5 format_token_response`（`:19` 含 `well_known`）；但 `auth_compat.rs:571-576` 手写刷新响应**缺 `well_known`**；`:31-41` 第三个形状（guest）；测试里第 4 份镜像 `tests/unit/formatting_route_tests.rs:20-25`（注释自认 "Mirror of"） | 45 | med-high |
| WEB-18 | 9 处尾斜杠别名路由放大成 19 条 ledger 条目 | `account_data.rs:14`、`push.rs:15 vs :16`、`room.rs:35/:61/:109`、`assembly.rs:456 vs :458-461`；axum 精确匹配，一条 `NormalizePathLayer` 可顶替 | 20 + 19 条目 | med-high |

**W-E 已核实"真正独立、不应合并"的候选对**（防误伤）：`module.rs` vs `route_module.rs`（功能路由 vs 装配 trait，仅命名易混）、`space.rs` vs `space/`（正确范式）、`room.rs` vs `handlers/room/`（router vs 实现）、`account_compat.rs` vs `handlers/`、`auth_compat.rs` vs `handlers/auth_discovery.rs`、`response_helpers.rs` vs `formatting.rs`；`ensure_room_view_access`（`handlers/room/mod.rs:39`，53 处调用）是有价值封装，勿删。

### 3.2 CFG — feature 开关 / cfg 死代码 / 兼容残留

**feature 启用矩阵（要点）**

| feature | 启用者 | cfg 站点 | 判定 |
|---|---|---|---|
| `server` | ci/docker/deploy 各 3 处 | **0** | **伪 feature**：Rust 源码中 `feature = "server"` 出现 **0 次**（已独立核验），`assembly.rs:13` 无条件 `use axum::…`，永远关不掉；却支撑着 `--no-default-features --features server` 的"最小构建"叙事 |
| `runtime-ddl` | **无任何处**（仅 `--all-features`） | 13 | **死**（1,249 行第二套 DDL） |
| `openapi-docs` | **无任何处**（仅 `--all-features`） | 444 | **死**（12,836 行） |
| `builtin-oidc` | 仅 `scripts/run_local_coverage.sh:62` | 30（含 8 处取反） | **CI 中实际死**（1,456 行）；开关两侧从未被 CI 真正切换测试 |
| `performance-tests`（synapse-services） | 无 | 0 | 幽灵 feature |
| `all-extensions` | ci/ledger-export/deploy | 5 | 元 feature，`Cargo.toml:52` 注释自认 **"default for backwards compatibility"** |
| `core-private-chat` | default/docker/ci | **0** | 纯别名，不门控任何代码 |
| `external-services`/`builtin-oidc`（synapse-storage） | — | **0** | 穿透式 plumbing，不门控该 crate 任何代码 |

> **shipped ≠ tested**：`docker/Dockerfile:5` 构建 `server,core-private-chat,widgets,external-services,voice-extended,cas-sso,saml-sso,friends`；CI `COV_FEATURES`（`ci.yml:821`）却包含 `beacons,voip-tracking,server-notifications,privacy-ext` 而 docker 不含；`default` 又包含 `beacons`。**发布二进制、默认测试道、覆盖率测量是三套不同 feature 集**，19 个开关的每一种真实组合都未被验证。

| ID | 标题 | 关键证据 | 可删 | 置信 |
|---|---|---|---|---|
| CFG-1 | **Olm pickle key 双实现；release 桩返回全零密钥** | `synapse-e2ee/src/olm/service.rs:106-117` `#[cfg(not(debug_assertions))] fn get_pickle_key()` 在 `PICKLE_KEY` 未初始化时 `tracing::error!` 后返回 `static ZERO: [u8;32]`；而**生产路径** `synapse-e2ee/src/olm/session.rs:44`（`load_sessions`）与 `:77`（`persist_sessions`）调用的正是这个宽松版；`:100-104` 的注释声称"body is unreachable because the function is gated behind `cfg(debug_assertions)`"——**该注释与事实相反**（release 分支就是 `cfg(not(debug_assertions))`）。是否返回全零取决于无关的初始化顺序（`service.rs:170/:211` 先跑才会初始化 `PICKLE_KEY`）。debug 分支 `:77-96` 还重复实现了 `decode_pickle_key_from_env`（`:27-41`） | ~50 | high（已独立核验） |
| CFG-2 | `src/web/api_doc/` 12,836 行纯文档桩 | 434 个函数体是 `unreachable!("This function exists only for OpenAPI documentation purposes")`；每个文件既有文件级 `#![cfg(feature="openapi-docs")]` **又**有 434 个冗余的行级同款 `#[cfg]`；`mod.rs:29` 真实现 vs `:631` 空 router 桩，`assembly.rs:600` 无条件 merge；`README.md:159` 宣传的 `/_swagger` 在任何真实构建中都 404；`schemas.rs` 1,777 行 / 126 处 `allow(dead_code)` | **12,836** + 434 属性 + 2 个可选依赖 | high |
| CFG-3 | **同一职责两份 OpenAPI** | (a) Rust/utoipa `src/web/api_doc/` 标注 364 个路径；(b) `scripts/api_test/generate_openapi.py` + **tracked** `docs/openapi/client.yaml`（**72,924 行 / 2.3 MB / 704 路径**，**已独立核验**）+ `index.json` + `refresh_openapi_specs.py` + `scan_handler_schemas.py`。(a) 只覆盖约一半路由且在所有真实构建中被编译掉 → 会静默漂移的第二真相源 | ~1,000 行 Python + 72,924 行生成物（或反向删 12.8k Rust） | high |
| CFG-4 | `runtime-ddl` 死 feature + 第二套 DDL | `database_initializer/tables.rs` 1,249 行 / 35 `CREATE TABLE` + 44 `CREATE INDEX`，整文件门控；`mod.rs:155/:740/:749/:900/:915/:940`、`schema_validator.rs:159/:166/:189` 亦门控。**已独立核验**：无任何 CI/docker/Makefile/脚本传递该 feature。注：2026-09-03 审查报告把此项标为"已修复"，**该结论不成立** | ~1,280 + 4 处 feature 声明 | high |
| CFG-5 | 旧 SHA-256 密码哈希兼容路径 | `config/security.rs:38-40` `allow_legacy_hashes`（唯一消费者是 `validation.rs:51-58` 的一条 DEPRECATED 告警）；`crypto.rs:43-88` legacy 校验分支、`:115 verify_password_legacy`（**0 生产调用者**）、`:120 is_legacy_hash`、`:125/:130 migrate_password_hash*`；登录时重哈希 `auth/login.rs:93-103`；配置面 `docker/config/homeserver.yaml:139`。未发布项目 ⇒ 不可能存在 legacy 哈希 | ~130 + 1 配置字段 | high |
| CFG-6 | 旧 token 哈希双查询 | `crypto.rs:237-248` `hash_token_legacy` + `verify_token_hash` 恒双试；`WHERE token_hash IN ($1,$2)` 打在唯一索引列上：`storage/token.rs:147/:194/:266/:285/:305/:334`、`refresh_token_service.rs:57/:277/:380/:425`、`auth/session.rs:122`。代价是**热认证路径上每次 cache miss 多一次索引查询 + 一次 HMAC** | ~120 + 每查询一次往返 | high |
| CFG-7 | `builtin-oidc` 1,456 行从未被 CI 当作开关测试 | 30 个 cfg 站点含 8 处取反（`route_module.rs:200-204`、`wiring/sso.rs:34-38,81-85`、`context.rs:917-918`）；仅 `run_local_coverage.sh:62` 启用，不在 `ci.yml:821`/`test.yml:73` 的 `COV_FEATURES`、不在 `Cargo.toml:272` required-features、不在 docker | 1,456 或补进 CI 矩阵 | high |
| CFG-8 | "backward-compatibility prelude" + 扁平 glob | `synapse-services/src/prelude.rs:1`（39 行，自述 Backward-compatibility）与 `synapse-storage/src/prelude.rs:1`（41 行）；**无任何生产代码 `use …::prelude::*`**，唯一消费者是 `tests/unit/prelude_module_tests.rs:4,60`（即"测试死代码的死代码"）；`synapse-services/src/lib.rs:170-199` 与 `synapse-storage/src/lib.rs:237-262` 扁平 glob 块注释自认 "keep the legacy root-level paths working"；**16 处** `#[allow(ambiguous_glob_reexports)]` 压制真实名字冲突 | ~200 + 16 处压制 | high |
| CFG-9 | `Option<()>` 特性占位字段（同字段两份定义） | `room/messaging/service.rs:32-34` + `:63-65`、`room/service.rs:107-110`、`wiring/sso.rs:37-38` + `:85`（`let builtin_oidc_provider: Option<()> = None;`）、`context.rs:917-918`（`:918 pub builtin_oidc_provider: Option<()>`） | ~20 + 简化 5 个结构 | high |
| CFG-10 | `synapse-services` 幽灵 feature `performance-tests` | `synapse-services/Cargo.toml:20` 零引用、零 cfg 站点 | 1 | high |
| CFG-11 | `server` 伪 feature | 见上表；`feature = "server"` 在全部 Rust 源码中出现 **0 次**（已独立核验），却支撑 `docker/Dockerfile:5`、`deploy.sh:533`、`ci.yml:723` 三处"最小构建" | 3 处声明 + 1–2 个构建矩阵项 | high |
| CFG-12 | "为向后兼容而默认"的元 feature 与零门控别名 | `Cargo.toml:52` 注释逐字违规；`:38 core-private-chat` 零 cfg 站点；`synapse-storage/Cargo.toml:29,30` 两个穿透 feature | ~15 + 6 处声明 | high |
| CFG-13 | SDK 契约随 feature 变化：手写排除清单 + 双份 fixture 车道 | `tests/unit/ledger_export_tests.rs:123-234` 5 个 golden 测试包在 `#[cfg(not(any(feature = "all-extensions", "voice-extended", …)))]`（8 元素手写清单，注释自认"新增扩展 feature 时需同步"）；`fixtures/ledger_export/{default,worker,all}.json` 与 `fixtures/ledger_export_sdk/{default,worker,all}.json` 两条平行车道 | ~60 + 一条车道（6 文件） | med-high |
| CFG-14 | "Reserved for future use / constructor parity" 死字段 | 7 个结构带 `#[allow(dead_code)] // Reserved fields for future use`：`admin_registration_service.rs:19/:26`、`admin_security_service.rs:19/:22`、`friend_room_service/models.rs:338/:343`、`room/lifecycle/service.rs:17/:24`、`room/membership/service.rs:35/:42`、`room/service.rs:128/:154`、`room/state/service.rs:14/:22`；`self.user_service` 在 7 处**读取 0 次**（**已独立核验**）。另 `user_service.rs:24/:73`（"unused in production"）、`worker/manager.rs:17/:19` | ~40 + 9 处压制 | high |
| CFG-15 | 零生产调用者的兼容构造函数 / 转发别名 | `src/tasks/mod.rs:49-59` `pub fn new` 自述 "Backwards-compatible constructor … New code should use from_config"；全仓唯一构造点 `server/mod.rs:306` 用 `from_config`；`config/server.rs:580` `get_event_server_name()` 是 `get_server_name()` 的纯转发别名（引用仅声明处与测试）。两者 `pub` ⇒ 死代码 lint 永不触发 | ~25 | high |
| CFG-16 | 双份服务器标识配置 + 掩盖死代码的 allow | `config/server.rs:20 pub name` 与 `:95 pub server_name`，`:547` 带"平滑迁移配置格式"回退；`directory.rs:1` 对活模块的 `#![allow(dead_code)]`；`synapse-services/src/lib.rs:190` `#[allow(deprecated, …)]` 而 `src/media` 无 deprecated 项；`synapse-storage/src/voice.rs:380-382` 助手自述 "Dead-code: kept … for future voice db_tests rather than deleted"；`synapse-e2ee/src/crypto/aes.rs:219/:257/:641` | ~12 + 4 处压制 | med-high |

### 3.3 INF — 共享基础设施 / worker / cache / federation / 薄壳

| ID | 标题 | 关键证据 | 可删 LOC | 置信 |
|---|---|---|---|---|
| INF-1 | **worker 集群抽象层无任何生产装配** | `worker/manager.rs:17-28` `#[allow(dead_code)]` 结构含 `bus/stream_manager/load_balancer/health_checker` 四个 `Option`，字段注释 "Reserved for future cluster rollout"；`manager.rs:157-161` 自述 "BUS wire-up … intentionally not wired in admin.rs today — multi-instance cluster replication rolls out as a follow-up"；唯一生产构造点 `wiring/admin.rs:333` **不调用任何 `.with_*`**（全仓非 worker 目录命中 0）；因此 `manager.rs` 中 12 处 `if let Some(lb/hc/bus/sm)` 分支恒为 false；`WorkerBus::new` 只出现在测试；`src/bin/synapse_worker.rs`（561 行）完全不用这套抽象；**docker-compose 只有 3 个 service（无 worker）**（已独立核验） | **约 1,820**（bus 796 / health 363 / stream 334 / load_balancer 250 的非测试部分 + manager 死分支）。若连 `protocol.rs`+`tcp.rs` 收敛可达 ~2,520 | high |
| INF-2 | **`QueryCache` 第三份缓存实现，零生产使用者** | `synapse-cache/src/query_cache.rs` **961 行**（自带 `QueryCacheConfig` per-namespace TTL、`CacheStats`、独立失效逻辑）；引用仅 `synapse-cache/src/lib.rs:66` 与 `synapse-services/src/lib.rs:309` 两处 re-export（**已独立核验**）；与 `CacheManager`(986) + `LocalCache`/`NamespaceCache` + `RedisCache` 职责重叠 | 961 | high |
| INF-3 | `src/worker/mod.rs` 零消费者死薄壳 | 全文 2 行 `pub use synapse_services::worker::*;`；`crate::worker::` 命中 0（**已独立核验**）；真实消费者全部直接写 `synapse_services::worker::…` | 3 | high |
| INF-4 | `init_logging` 双实现，`synapse-common` 那份被遮蔽且从未执行 | `synapse-common/src/logging.rs`（79 行）与 `src/common/logging.rs`（79 行）近乎逐字节相同（连 `",sqlx::query=warn,sqlx_core=warn,hyper=info,tower_http::trace=info"` 都一致）；唯一调用点 `src/server/telemetry.rs:39` 走 root 版；`synapse_common::init_logging` 被 `src/common/mod.rs:50` 遮蔽（**已独立核验**） | 79 | high |
| INF-5 | `src/security/`（477 行）零生产调用者 | `invite_signature.rs` 167 + `device_binding.rs` 117 + `invite_cross_service_tests.rs` 180；`grep -rn "invite_signature\|device_binding\|sign_invite\|DeviceBinding"` 排除自身后 = **0**（**已独立核验**） | 297（生产）/ 477（含测试） | high |
| INF-6 | `src/web/streaming.rs` + `filter.rs`（335 行）零消费者 | 全仓引用 0（**已独立核验**：命中的是无关的 `SyncResponseFilter`）；只被 `src/web/mod.rs:15,19` 的 `pub use …::*;` 挂出。附带缺陷：`streaming.rs:45` 名为 streaming 实则 `tokio::fs::read` 全量入内存 | 335 | high |
| INF-7 | synapse-common 三个零引用工具模块 | `collections.rs` 181（6 个 pub 项全 0 调用，均可由 `Vec::with_capacity` 等一行替代）、`early_exit.rs` 117（除 re-export 外 0）、`nonce_cache.rs` 203（0）。**已独立核验**三者仅有 `lib.rs` re-export | 501 | high |
| INF-8 | `macros.rs` 7 个宏中 5 个零使用，却被 crate 根公开承诺 | 调用点：`map_internal` 48 / `map_database` 176 / `impl_api_error` **0** / `map_bad_request` **0** / `map_forbidden` **0** / `map_not_found` **0** / `map_unauthorized` **0**；5 个死宏仍写在 `src/lib.rs:48-50` 的公共契约与 `src/common/mod.rs:11-13` 的二次 re-export（注释自认 "Also covered by the glob above"） | ~62 + 2 行 re-export | high |
| INF-9 | `TaskMetricsCollector` / `CollectedMetrics` 写入后从未被读取 | `src/tasks/mod.rs:397-423` / `:425-471`（含 30 行 JSON 拼装）；接线 `server/mod.rs:116` 字段、`:310` 构造、`:900` 访问器；`collect_all()`/`to_json()`/访问器**调用点均为 0**（**已独立核验**） | ~75 + ~6 | high |
| INF-10 | `src/e2ee/` 非测试内容 100% 转发；`src/federation/mod.rs` 同类 | `src/e2ee/` 866 行中 `vodozemac_interop_tests.rs` 占 753（`E2EE_INTEROP=1` 才跑），余 113 行约 110 行是 `pub use`；`megolm/{models,service,storage}`、`device_keys/…`、`key_rotation/service` 均为 2 行 facade，自述 "Thin facade re-exporting from synapse_e2ee"；`src/federation/mod.rs` 23 行纯 `pub use`。前提：`Cargo.toml:59-64` 已直接依赖这些 crate，测试可直接跨 crate import ⇒ 每个转发行都能被等价 import 替代，同一符号被暴露两次 | ~150 + 二次 re-export ~15 | high（冗余事实）/ medium（需改 ~80 文件 import） |
| INF-11 | cache 的 key/TTL 策略表 90% 是死的 | `synapse-cache/src/strategy.rs` 312 行：`CacheKeyBuilder` 21 个构造器，实际只用 3 个（`user_presence` 12 次、`ip_rate_limit` 1、`federation_origin_rate_limit` 1）；`CacheTtl` 14 个只用 1 个。未使用的 key builder 描述的命名空间与真实调用点（散落字符串字面量）事实脱钩 —— 正是"缓存读写字面量不一致"类 bug 的温床 | ~250 | high |
| INF-12 | 常量表六成是死的，且被硬编码字面量绕过 | `synapse-common/src/constants.rs` 32 个常量中 **19 个零引用**；同时 `src/web/routes/validators.rs:28` 写死 `user_id.len() > 255` 而 `MAX_USERNAME_LENGTH = 255` 零引用；`:55` 同样。常量成了装饰，"改限制要改两处且不会同步" | ~50 | high |
| INF-13 | `#[allow(dead_code)]` 与 "Reserved" 字段成灾 | 全仓 143 处 `#[allow(dead_code)]`；服务层明确"为将来预留"的字段见 CFG-14；另 `synapse-common/src/config/mod.rs:62` `// Re-exports for backward compatibility`（后跟 40+ 行 re-export）、`push_rules.rs:251` `// Backward compat: map old rule IDs…` | ~150（8 个 reserved 字段块） | med-high |
| INF-14 | 限流至少 4 套，token bucket 双实现 | (1) `web/middleware/rate_limit.rs:13`（`:59-66` 双分支选桶，`:109` 调 cache）；(2) `federation_rate_limit.rs:22` 自建 `federation_endpoint_bucket`（`:101`）；(3) `synapse-cache/src/manager.rs:879` 本地 moka bucket（`:970/:981`）**vs** `remote.rs:347` Redis-Lua bucket —— 同一算法两份实现；(4) 业务自建 `event_report_service.rs:271`、`admin_security_service.rs:11/:57`。配置侧也两套：`rate_limit_config.rs`(1,227) 与 `config/rate_limit.rs`(221，纯别名)，且 `select_endpoint_rule`(:601) 与 `select_endpoint_rule_runtime`(:607) 是同构双实现 | ~80–150 | medium |
| INF-15 | 缓存族普查：10 个 Cache 结构体 + 10 处 moka builder + 9 个散落 TTL 常量 | `LocalCache`/`NamespaceCache`/`RedisCache`/`FederationSignatureCache`/`QueryCache`/`RegexCache`/`ReplayProtectionCache`/`FederationNonceCache`/`FriendListSortCache`/`UrlPreviewCache`；统一失效通道 `CacheInvalidationManager`(481 行) 仅被 2 处使用，其余 8 个各靠 moka TTL。**这是 `AGENTS.md` 已知坑「`set_raw` 写 L1+L2 / 同步 `get_raw` 只读 L1」的结构性根因** | 961 已计入 INF-2 | high（普查）/ medium（合并） |
| INF-16 | `FederationNonceCache` 与 `ReplayProtectionCache` 同职责双实现，前者零使用者 | `nonce_cache.rs:28,37`（203 行，moka future::Cache）全仓 **0 引用**；`security.rs:30,46 ReplayProtectionCache::check_and_record` 已接线于 `routes/state.rs:6,28,87`、`middleware/federation_auth.rs:189` | 203 | high |
| INF-17 | `Validator`(725) 与 `routes/validators.rs`(332) 职责重复 + 死函数 | `synapse-common/src/validation.rs:45 Validator` 含 13 个 validate_*；`src/web/routes/validators.rs` 独立实现 9 个；room_id 校验两边都有（web 侧 131 次引用）；`validate_server_name` **0 调用**；该文件 `:5-10` 注释自称"互补而非重复" | ~60（彻底统一需改 ~150 行调用点） | medium |
| INF-18 | 文档噪声：5,512 行 `See [x].` + 10,059 行样板，含连续重复行 | `src/tasks/mod.rs:47-48` 两行 `/// See [\`new\`].`（`:346-347/:352-353/:358-359/…` 成对重复）；`src/server/mod.rs:899-900` 两行 `/// See [\`metrics_collector\`].`（**已独立核验**） | ~2,000+（保守） | high（计数）/ medium（删除范围） |

### 3.4 SVC — `synapse-services` 服务层（94,188 行 / 201 文件）

| ID | 标题 | 关键证据 | 可删 LOC | 置信 |
|---|---|---|---|---|
| SVC-0 | **一套没有插件、也没有接线的"动态模块/插件系统"** | 三处合计 **3,360 行 / 88 个公开函数**：`synapse-services/src/module_service.rs`（1,111 行 / 36 fn）、`synapse-storage/src/module.rs`（1,328 行 / 27 fn，含 `modules` 表 CRUD + `execution_logs` + `error_count`/`last_error`）、`src/web/routes/module.rs`（921 行 / 25 fn）。核心判定（**已独立核验**）：`check_spam` / `check_third_party_rules` —— 这套 API 存在的**全部意义**（Synapse 的 spam-checker / third-party-rules 模块接口）—— 在**事件管线中调用点为 0**（`grep -rn "check_spam\|check_third_party_rules" --include=*.rs src synapse-*` 排除其自身 service 与 route 后 = 0）。即：一个模块注册表，既没有内置模块实现，也没有任何代码会在发事件时咨询它 | **~3,360** | high |
| SVC-1 | **整条推送投递链路在生产路径上不可达** | `push/service.rs:22-23` 的 `push_gateway`/`queue` 两个 `Option` 字段**只写不读**（`self.queue` 全仓 3 次命中全是写：`:130`、`:176-177`）；`push/service.rs:148 initialize_providers()` 是全仓唯一给 `fcm_provider/apns_provider/webpush_provider` 赋值处（`:152/:160/:171`），而**全仓调用点为 0**（唯一命中是 `:147` 的自指文档注释）。构造点 `wiring/admin.rs:280` 之后从不调用它 ⇒ 三个 provider 恒 `None`，`service.rs:344-365` 恒走 `else { self.send_fcm_fallback(...) }`；`send_with_retry` 的 4 处命中全在死分支里；`with_fcm_provider`/`with_apns_provider`/`with_push_gateway`/`with_queue` 外部调用点均为 0。`push/` 模块合计 **3,288 行**（service 986 + queue 481 + gateway 441 + providers fcm/apns/webpush/mod 1,366）。**已独立核验**：`grep -rn "initialize_providers"` 仅命中定义与其文档注释 | **~1,600**（+ ~550 测试） | high |
| SVC-2 | **推送规则两套并行实现（服务+存储+DTO+路由）** | 标准：`src/web/routes/push.rs:18-22` `/pushrules` → `ctx.client_push_service`；非标准重复：`src/web/routes/push_notification.rs:335-337` `/_matrix/client/r0/push/rules` → `ctx.push_notification_service`；两套存储 trait（`storage/push/mod.rs:48 upsert_push_rule` vs `storage/push_notification.rs:312 create_push_rule`）；**三处 `PushRule` 定义**（`storage/push_notification.rs:53`、`src/web/routes/push.rs:119`）；`push_notification.rs:331-333` 的 `/r0/push/devices` 亦重复标准 `/v3/pushers`。`routes/push_notification.rs:355-362` 自认其 7 条 r0 路由 "have zero call sites in the SDK" | ~700 | high |
| SVC-3 | **`LivekitClient`（SFU 集成）完全死代码** | `rtc/sfu.rs:150`（583 行 / ~445 生产 LOC）含 `create_room`/`delete_room`/`list_rooms`/`list_participants`/`mute_published_track`/`create_access_token`；唯一构造点 `wiring/extensions.rs:165`，存于 `rtc/mod.rs:96`，唯一访问器 `rtc/mod.rs:137 sfu()` —— 而全仓 `\.sfu\b` 仅 1 处命中（访问器自身），`src/web/` 零引用 | ~460 | high |
| SVC-4 | `extensible_events.rs` 255 行模块零调用 | 模块唯一公开项 `extract_text_from_event_content`（`:35`）；全仓仅 2 处引用且均非调用：`lib.rs:96-97` 的 `pub mod` 声明 + `capability_governance.rs:118-120` 的一句注释 | 255 | high |
| SVC-5 | 零使用者的纯转发壳（route 层绕过） | `event_service.rs`(14 行，自述 "the current scope is the type-re-export shim only")、`rendezvous_service.rs`(17 行)。**关键**：`src/web/routes/rendezvous.rs:14` 直接 `use synapse_storage::rendezvous::{…}`，**绕过了这个"为了维持分层而存在"的壳** | 31 | high |
| SVC-6 | `shutdown.rs` 全文只有测试，无任何生产项 | `synapse-services/src/shutdown.rs` 33 行 = doc 注释 + `#[cfg(test)] mod tests`；真实关闭逻辑用 `tokio_util::sync::CancellationToken`（`src/tasks/mod.rs:116`），与本模块无关 | 33 | high |
| SVC-7 | `IdentityService` 13 个公开方法中 9 个零调用 | 完全无调用：`get_user_three_pids:63`、`add_three_pid:68`、`remove_three_pid:74`、`request_3pid_verification:164`、`check_3pid_validity:209`、`hash_lookup:242`、`invite_3pid:275`、`get_trusted_servers:321`、`with_test_base_url:33`；连带 `identity/storage.rs:26 add_three_pid`、`:48 remove_three_pid` 仅服务这些死方法 | ~250 | high |
| SVC-8 | **领域分组壳 + `prelude` + `lib.rs` 三重 glob，制造三套等价路径** | `lib.rs:36-168` 逐个 `pub mod`，`:181-196` 又 `pub use account::*; pub use admin::*; …`，而分组模块本身只再导出（`admin.rs:24`、`event/mod.rs:19`、`infra/mod.rs:30`、`account/mod.rs:27`、`application/mod.rs:13`、`identity/mod.rs:17`、`push/mod.rs:15`、`sync/mod.rs:26`）。结果 `AdminAuditService` 有 **3 条可达路径**（root / `admin::` / `admin_audit_service::`）。`infra/mod.rs:22-29` 是 4 条整模块 glob。`prelude.rs`(40 行) 自述 `Backward-compatibility prelude` + 10 处 `allow(ambiguous_glob_reexports)`；`lib.rs:178-180` 自认 "backward-compatibility flat re-exports … keep the legacy root-level paths working"。**生产侧真正使用分组路径的只有 8 处** | ~225 | high |
| SVC-9 | 一行/数行的转发薄壳文件 | `worker/storage.rs` **全文 1 行**（`pub use synapse_storage::worker::WorkerStoreApi;`）；`worker/types.rs`(8 行)、`sync_service/push_rules.rs`(9 行，自述 "exists to preserve the historical … import path … new code should import from `synapse_common::push_rules` directly")、`event_broadcaster_trait.rs`(26 行) | ~44（4 文件） | high |
| SVC-10 | `RoomServiceApi`：单实现 trait + 24 个纯转发方法 | `room/api_trait.rs:17`（190 行）；全仓 `impl RoomServiceApi for` **恰好 1 处**（`:97`），方法体全是同名转发。**且并未真正打破循环**：`room` 已直接引用 `friend_room_service`（`room/service.rs:705`、`room/mod.rs:60`），同 crate 模块互引无需 trait 中介；构造顺序也不构成障碍（`wiring/rooms.rs:102` 先建出具体类型再强转） | ~130 | med-high |
| SVC-11 | `SyncServiceApi`：单实现 trait，唯一消费者是容器字段声明 | `sync_service/api_trait.rs:9`（101 行，6 方法全转发，`:54` 是唯一 impl）；唯一引用 `wiring/rooms.rs:33 pub sync_service: Arc<dyn SyncServiceApi>`；`test_mocks.rs` 中**无**该 trait 的 mock ⇒ **连测试缝都不是** | ~101 | medium |
| SVC-12 | `module_service.rs` 单文件混装 5 个不相关服务 + 3 个 trait | 见 SVC-1（模块系统整体）之外的两点：`AccountValidityService`（`:672`，账户有效期）与模块系统无语义关系却同文件，且只在 `routes/module.rs:682-716` 被调用、未接入认证/注册链路；`PasswordAuthProviderTrait`（`:150`）**生产实现 0 个**（唯一实现在 `tests/unit/module_tests.rs:331`）；`SpamChecker`/`ThirdPartyRule` 各只有 1 个同文件实现，且无生产者注册它们 | ~120 + 拆文件 | medium |
| SVC-13 | `admin_server_service.rs`：45 行零增值包装 | 全文 45 行 2 个方法，各 2–3 行，只是包了 `synapse_common::health::DatabaseHealthCheck` 与 `synapse_storage::schema_validator::SchemaValidator`；文件头自认 "intentional exception to the service→storage pattern"；调用点 3 处只取 bool/Vec<String>；且直接持有 `Arc<PgPool>` ⇒ 既不可 mock 也不可替换 | ~51 | med-high |
| SVC-14 | God object：4 个服务类方法数 48–60 | `impl RetentionService` **60 个方法**（`retention_service.rs:136`）、`impl SamlService` 57（`saml_service.rs:151`）、`impl SearchService` 48、`impl MediaDomainService` 48、`impl ThreadService` 48、`impl MediaService` 46、`impl FriendRoomService` 36（`friend_room_service/mod.rs:135`，**1,834 行 = 全 crate 最大生产文件**）。对照：`room/` 已按 membership/messaging/state/lifecycle 拆分，`retention`/`friend_room` 未做 | ~300–500（拆分估计） | medium |
| SVC-15 | `media_quota_service` 注入两个 context 却无任何路由读取 | `ctx.media_quota_service` 全仓仅 2 处且都在构造侧（`context.rs:669`、`:865`），**无任何方法调用**。服务本体非死（`media/mod.rs:247/:259/:272/:565`），仅注入面冗余 | ~6 | high |
| SVC-16 | `PolicyService` 4 个 convenience 包装中 3 个只是转发，1 个参数被吞 | `check_room_create:125`/`check_room_join:130`/`check_room_invite:135`/`check_content_send:142` 全部转调 `check_policy`（底层有 12 个调用点）；`check_content_send` **零外部调用**；`check_room_invite(&self, _room_id, inviter, invitee)` **把 `room_id` 丢弃**（`_` 前缀）—— 需裁定是策略语义还是缺陷 | ~22 | medium |
| SVC-17 | 模块粒度双峰：42 个文件 < 100 行 vs 1,834 行的单体 | 201 文件 / 94,188 行，median 371；**<100 行 42 个 / 1,636 行**，其中 26 个非 `mod.rs`（`worker/storage.rs` 1 行、`worker/types.rs` 8、`sync_service/push_rules.rs` 9、`event_service.rs` 14、`rendezvous_service.rs` 17、`admin.rs` 23、`event_broadcaster_trait.rs` 26、`shutdown.rs` 33、`auth/token_auth.rs` 34、`prelude.rs` 39、`auth/room_auth.rs` 42、`admin_server_service.rs` 45…）。内联 `#[cfg(test)] mod tests` 占 **28.1%**（`sync_service/filter.rs` 74%、`room/membership/mod.rs` 69%） | 与 SVC-5/8/9 叠计 | high |
| SVC-18 | **排除项：经核验不构成冗余的候选**（防误伤） | `client_push_service.rs` vs `push/` 职责不同（配置读写 vs 投递）；`media_service.rs` vs `media/mod.rs` 是**装饰器分层**（8 个同名方法全部委托，另 15 个 domain-only 方法）；**不存在 `admin/` 目录**，8 个 `admin_*_service.rs` 互不重复；`sync_helpers.rs` 是真实共享逻辑（`/sync` 与 sliding sync 共用 Client 格式转换）；`sliding_sync_service/` 是独立实现（MSC4186 txn_id 幂等、连接 TTL/LRU）；`feature_flag_service` / `policy_service` / `capability_governance` **三者正交**；`wiring/`（1,633 行）有真实价值（`wiring/rooms.rs:76-80` 的 `NotifyingEventWriter` 装饰、`:102-132` 的配置装配），不是 `pub use` 堆叠 | 0 | high（正面） |
| SVC-19 | 服务层 trait 使用相对克制（**正面项**） | services 仅 12 个 `pub trait`：`auth/` 的 4 个（`TokenAuth`/`RoomAuth`/`CredentialAuth`/`MasTokenValidator`）与 `RegistrationTokenApi`、`SmsProvider` 均有**真实测试替身或多实现**，是合格 seam。真正的过度抽象只有 5 个：`RoomServiceApi`、`SyncServiceApi`、`SpamChecker`、`ThirdPartyRule`、`PasswordAuthProviderTrait`（合计约 260 行样板）。**对照 storage 的 71 个 trait / 69 个单实现 —— 过度抽象集中在持久层，不在服务层**。测试脚手架门控也正确（`lib.rs:286-295` 三处均为 `#[cfg(any(test, feature = "test-utils"))]` 且 `test-utils` 不在 default） | — | high（正面） |

### 3.5 STO — `synapse-storage` 持久层（112,975 行 / 218 文件）

| ID | 标题 | 关键证据 | 可删/可收敛 | 置信 |
|---|---|---|---|---|
| STO-1 | **36 个增量迁移 + 36 个 undo 已被 v11 baseline 全量吸收，属纯重复文件** | 项目自带门禁实跑自证（**已独立核验**）：`python3 scripts/check_baseline_consolidation.py` → `✅ 00000000_unified_schema_v11.sql 已吸收全部 36 个增量迁移的对象。`；`scripts/build_sqlx_migration_source.py:43-45` 注释 `# All timestamp-based migrations are superseded by v8 baseline`，`selected = [latest_baseline] + [latest_extension] + V*` ⇒ 36 个时间戳迁移**根本不在 CI 的 sqlx 前向链里**；`migrations/README.md:90-95` 自述"每次新增时间戳迁移后必须把幂等增量同步折入 v11 尾部"；`baseline_tables.rs:28` 用 `include_str!` 只读 v11。实测 `migrations/` 有 **72 个时间戳 `.sql` + 36 个 `.undo.sql`**。v11 里已有 `---- 折入自 <文件名> ----` 反向标记（如 `:5050` 对应仍存在的 `20260813165000_create_login_tokens.sql`） | **1,937 行 / 72 文件** | high |
| STO-2 | `00000001_extensions_v10.sql` 的 15 张表 100% 已在 v11 baseline，feature 门控是空转 | `comm -12` 比对 v11 的 253 张表与 extensions 的 15 张表 → **15/15 全部重叠**，extensions 独有表 = 0。`migrations/extension_map.conf` 头部自称「real feature gating」，但 `migrations/README.md:85-88` 自认"map 只能表达整个文件↔特性集合…要实现按特性表裁剪需把文件拆回 per-feature **并从 baseline 中移除这些表**，属结构性变更，尚未进行"。文件头亦自述 "no schema changes from v8" | 258（1 文件）+ map 条目 | high |
| STO-3 | `migrations/archive/` 携带 195 KB / 4,954 行的 v8 死副本 | 4 文件：`00000000_unified_schema_v8.sql`(177,879 B)、`00000001_extensions_v8.sql`、`20260605120000_…_v8.sql`、`20260606120000_…_v8.sql`。**已独立核验**：无任何脚本读取（`build_sqlx_migration_source.py:28` 用非递归 `glob("*.sql")`；`docker/db_migrate.sh:434` 用 `-maxdepth 1`） | **4,954** | high |
| STO-4 | **四层互相重叠的 schema 校验，表/列清单手工维护 3 份** | `schema_health_check.rs:42 CORE_COLUMNS` = **88 条 (表,列)** 覆盖 21 表 + `:162 REQUIRED_INDEXES` 15 条；`schema_validator.rs:40 REQUIRED_TABLES` 11 张 + `:54 REQUIRED_COLUMNS` 13 条 + `:169 columns_to_add` 7 条 + `:192 indexes` 4 条（**另一份独立手工清单**）；`baseline_tables.rs:107 baseline_tables()` 编译期解析 v11 得 253 张表（**这份是正确方向**）；`migration_checks.rs:140 count_public_tables()` 是第 4 个表计数入口。`comm -12` 显示前两者共享 15 个完全相同的字面量。且 `schema_validator.rs:159/:166/:189` 的 `repair_missing_columns`/`create_missing_indexes` 全挂在 `#[cfg(feature = "runtime-ddl")]`（非默认 feature，见 CFG-4） | ~250 | high |
| STO-5 | **71 个 `*StoreApi` trait 中 69 个只有 1 个非 mock 实现；33 个连 mock 都没有** | **已独立核验**：逐 trait 统计 `impl X for`，只有 `MediaStorageBackend`(3) 与 `UserStore`(2) 有真实多态，其余 **69 个 ≤1 个非 mock 实现**。其中 **33 个连 mock 都没有**（`ApplicationServiceStoreApi`/`BeaconStoreApi`/`CallSessionStoreApi`/`CaptchaStoreApi`/`ChunkedUploadStoreApi`/`DelayedEventStorageApi`/`E2eeAuditStoreApi`/…/`VoiceStoreApi`），trait 声明体合计 1,399 行 + impl 块 2,074 行 = **3,473 行纯样板**。典型：`state_groups.rs:74-135` 是 60 行签名，`:487` 的 impl 是 82 行逐字 `self.method(...).await` 转发。**其中 10 个从未以 `dyn` 出现**（`grep -rE "dyn [A-Za-z_:]*<Trait>"` = 0）：`E2eeAuditStoreApi`、`FederationQueueStoreApi`、`MatrixRTCStoreApi`、`ModerationLogStoreApi`、`ModerationStoreApi`、`OAuthClientStoreApi`、`SearchIndexStoreApi`、`StateGroupStoreApi`、`UrlPreviewStoreApi`、`VoiceStoreApi` ⇒ 691 行**零分发价值** | 691（零 dyn 的 10 个）～ 3,473（全部收敛为具体类型） | high（计数）/ medium（全删需评估 DI 风格） |
| STO-6 | **`performance.rs` 292 行完全死代码** | `performance.rs:52 pub struct PerformanceMonitor`：全仓 `PerformanceMonitor::new` **0 命中**（唯一提及是 `infra/mod.rs:32` 的 re-export）；`:243 #[macro_export] macro_rules! timed_query`：全仓 `timed_query!` 仅命中其自身文档注释，**0 次真实调用**；`get_query_metrics`/`get_pool_stats`/`check_pool_health`/`record_query`/`reset_metrics` 排除本文件后 0 命中。实际在用的监控入口是 `monitoring.rs` 的 `DatabaseMonitor`（`telemetry_service.rs:375`、`src/tasks/mod.rs:254`） | 292 | high |
| STO-7 | `test_mocks/` 33 文件：3 个 mock 零调用（1,099 行）+ 1,186 行"给 mock 写的测试" | 引用数 0（排除自身与 `mod.rs`）：`test_mocks/space.rs` → `InMemorySpaceStore`（**504 行**）、`test_mocks/registration_token.rs` → `InMemoryRegistrationTokenStore`（**399**）、`test_mocks/admin_federation.rs` → `InMemoryAdminFederationStore`（**196**）。对照其余 mock 确有价值（`InMemoryEventStore` 24 个使用文件、`InMemoryMemberStore` 25 个）。另 `test_mocks/tests.rs` = **1,186 行**，测的是测试替身自身行为 | 1,099（+ 可再减 ~900） | high |
| STO-8 | **25 个 domain-group 转发 shim + 根级双路径别名 + 一个只验证"两条路径同类型"的测试** | 纯转发 `mod.rs`（无 `sqlx::query`、无 `pub async fn`、无 `impl`）**22 个文件 / 468 行**（`oidc/mod.rs` 12、`registration_token/mod.rs` 12、`sliding_sync/mod.rs` 12、`cas/mod.rs` 13、`e2ee/mod.rs` 13、`thread/mod.rs` 15、`account/mod.rs` 17、`user/mod.rs` 21、`admin/mod.rs` 21、`sync/mod.rs` 25、`infra/mod.rs` 57…）；`lib.rs:243-262` 同时保留 flat 声明与 domain glob，注释直言 `keep the legacy root-level paths working`；`prelude.rs:1-2`「Backward-compatibility prelude」(41 行) 全仓无调用；`tests/unit/storage_admin_domain_refactor_tests.rs`（**256 行 / 24 个 `test_*_path_identity`**）唯一目的是断言两条路径是同一类型。实际消费量极低（`application::` 3 次、`e2ee::` 3、`oidc::` 3、`admin::` 6、`sync::` 6，对比 flat 路径遍布全仓） | 468 + 41 + 256 ≈ **765** | high |
| STO-9 | **`user_store_fake.rs` 705 行测试替身未加 cfg 门控，被编进生产二进制** | `lib.rs:162-163 pub mod user_store_fake;` **无条件**，`:232 pub use user_store_fake::FakeUserStore;` 也无条件；文件内除 `:436` 的 test mod 外无 `#![cfg]`。对照所有其它 mock 都在 `lib.rs:149-151 #[cfg(any(test, feature = "test-utils"))] pub mod test_mocks;` 之下。**已独立核验**（与 TST-3 同源，TST-3 侧另含 `synapse-common` 的 1,478 行） | 705（移出生产图） | high |
| STO-10 | `push/mod.rs` 与 `push_notification.rs` 双实现同时写 `push_rules` | 同职责方法各写一份：`push/mod.rs:315 get_user_push_rules` vs `push_notification.rs:532 get_user_push_rules`、`push/mod.rs:233 delete_push_rule` vs `push_notification.rs:549 delete_push_rule`、`push/mod.rs:201 upsert_push_rule` vs `push_notification.rs:496 create_push_rule`。两条链都活着：`wiring/core.rs:152` 构造 `push::PushStorage`（供 `client_push_service`），`wiring/admin.rs:277` 构造 `push_notification::PushNotificationStorage`（供 `push/service.rs`）。不同方法名、不同返回类型（`sqlx::Error` vs `ApiError`），规则集/启用态可被交错覆盖。**与 SVC-2 是同一条问题的两层**（路由层 + 存储层） | ~200 | high |
| STO-11 | **`user_account_data` 只写不读：静默数据丢弃（正确性隐患）** | v11 同时建两张同义表：`:1930 account_data (data_type TEXT, content JSONB)` 与 `:1953 user_account_data (event_type TEXT, content TEXT)`。**同一个 `UserStorage` 内写一张、读另一张**（**已独立核验**）：`user/storage.rs:876 INSERT INTO user_account_data (user_id, event_type, content, created_ts)`，而 `:896 SELECT content FROM account_data WHERE user_id=$1 AND data_type=$2`。`user_account_data` 全仓仅 4 处引用（上述写入 + 其自身 `db_tests.rs:658/663/665`）；生产读路径一律走 `account_data`（`account_data_service.rs`、`client_push_service.rs:146`、`push/service.rs:535`）。⇒ `UserStorage::set_account_data` 是**只被自己测试调用的死写路径，写进去的数据无人能读** | ~40 + 整表 | high |
| STO-12 | **两个同名 `delete_events_before`，过滤语义相反（正确性隐患）** | `retention.rs:363 delete_events_before(&self, room_id, cutoff_ts) -> i64`：`DELETE … AND state_key IS NULL`（**只删非状态行**）；`event/basic.rs:40 delete_events_before(&self, room_id, timestamp, dry_run) -> u64`：`DELETE … AND COALESCE(NULLIF(NULLIF(BTRIM(origin),''),'undefined'),'self') != 'self'`（**只删远端事件**）。第三条：`event/basic.rs:180 delete_room_events` 无条件全删。调用方分别是 `retention_service.rs:361` 与 room purge-history 路径，签名与过滤条件互不兼容 ⇒ 极易选错（保留策略删除 vs 历史清除） | ~45（收敛为带显式策略参数的一条实现） | high |
| STO-13 | **`room/mod.rs` 是 1,453 行生产的 god module，且与 `directory/`、`room_account_data/` 双写同一张表** | `room/mod.rs` 共 2,267 行，`#[cfg(test)]` 首现于 `:1454` ⇒ **1,453 行生产代码 / 48 个 `pub async fn` / 65 处 `sqlx::query`**，触达 9 张表。与专用模块重叠：`room/mod.rs:1049 INSERT INTO room_directory (room_id, is_public, added_ts)` **vs** `directory/mod.rs:108 INSERT INTO room_directory (… name, topic, join_rule, member_count)` —— 两个类写同一张表，`DirectoryStorage` 强制 `is_public=true` 而 `RoomStorage::set_room_directory` 可写 `false`；`room/mod.rs:1086 set_room_account_data` vs `room_account_data.rs:197 upsert_room_account_data`；`room/mod.rs:709/:729 increment/decrement_member_count` vs `membership/mod.rs`（52 处 `room_memberships` 查询）。另 21 处 `room_memberships` 查询散落在 `room/mod.rs`(13) 与 `room/admin.rs`(8) 而非集中在 `membership/` | ~300（迁走目录与账户数据两段） | high |
| STO-14 | **v11 baseline 中 22 张表无任何 Rust 引用（含 6 张已删功能的 AI/OpenClaw 表）+ 2 个索引重复定义** | **已独立核验**：`grep -c "ai_messages\|ai_conversations\|ai_connections\|ai_generations\|ai_chat_roles\|openclaw" migrations/00000000_unified_schema_v11.sql` = **69**，而全仓 Rust 引用 = **0**。6 张 AI 表连续定义于 `:3340-3422`，另有触发器 `:4518-4535` 与 `COMMENT ON` `:4602-4657`。`migrations/README.md:108-115` **自认**「v11 baseline 仍包含 `openclaw_connections`/`ai_conversations`/`ai_connections` 三张表及其触发器。openclaw 源码已于 commit `67e66bf4` 彻底删除…新装实例会建出死表。」此外 22 张零引用表还包括 `voice_messages`、`user_reputations`、`typing_stream`、`security_events`、`room_stats_current`、`receipts_linearized`、`reaction_aggregations`、`presence_stream`、`password_history`、`ip_blocks`、`federation_inbound_events`、`destination_retry_timings` 等。重复索引：`:3570` 与 `:4066` 各定义一次 `idx_rooms_name_trgm`，`:3571` 与 `:4067` 重复 `idx_rooms_canonical_alias_trgm` | ~150（AI 段）+ 4 个重复索引行 + 其余死表 | high |
| STO-15 | `retention.rs` 的 4 个 `FromRow` struct 与 5 个 no-op 方法，对象表已被 DROP | `migrations/20260909010000_drop_stale_legacy_tables.sql:47-53` 已 `DROP TABLE … deleted_events_index / retention_cleanup_logs / retention_cleanup_queue / retention_stats`，但 `retention.rs:45/:72/:97/:112` 的 4 个 `#[derive(FromRow)]` struct 仍在；`retention_service.rs:390/:400/:407/:413/:419` 的对应方法即注释级 no-op（`// No-op: cleanup queue table has been removed` / `Ok(0)` / `Ok(None)` / `Ok(vec![])`）—— 表已删、struct 无 `FromRow` 目标、方法恒返回空值，三层死代码，且 `get_*` 的固定空返回会**掩盖真实错误** | ~120 | high |
| STO-16 | `monitoring.rs` 与 `performance.rs` 各自维护一套连接池/性能指标 DTO | `monitoring.rs:20-33 ConnectionPoolStatus { total_connections, idle_connections, busy_connections, max_connections, connection_utilization }` vs `performance.rs:13-23 PoolStatistics { …, active_connections, …, utilization_percent }` —— 字段一一对应；`monitoring.rs:35-54 PerformanceMetrics` vs `performance.rs:27-42 QueryMetrics`。生产由 `monitoring.rs` 提供（`lib.rs:299-301 Database::get_performance_metrics()`）；`performance.rs` 那套依附于 STO-6 的死模块 | ~60 | high |
| STO-17 | **迁移引用与门禁整体陈旧** | `migration_checks.rs:109-112` 仍判 `if name.starts_with("00000000_unified_schema_v10") { return None; }`，而实际文件是 `_v11.sql` ⇒ **该分支永不命中**（碰巧被 `name[..14].parse::<i64>()` 失败兜住），`:21`/`:69` 注释同样写 v10；`baseline_tables.rs:148-150` 断言消息仍写 "suspiciously low for v10"。更严重：`drift-detection.yml:344-359` 的迁移性能门禁遍历 **4 个全部不存在的路径**（`migrations/archive/2026…_performance_indexes*.sql` 不存在；`docker/deploy/migrations/…` 目录已在 `2b16dc3c` 删除）⇒ 全部 `continue`，仅打印 warning，`failed` 保持 0 ⇒ **永久绿灯**（与 §1.3 的空转门禁同类） | ~30 + 15 行 CI | high |
| STO-18 | `EventReader`/`EventWriter` 附带 418 行逐方法纯转发 `impl` | `event/reader.rs` 597 行 = `:9-299` trait 声明（40+ 方法）+ `:301-597` 的 `impl EventReader for EventStorage`，**296 行全是 `self.method(...).await`**；`event/writer.rs` 252 行中 `:130` 起为同构转发块 **122 行**。三个实现中只有 `EventStorage` 这份完全无逻辑 —— 它本身已有全部同名 inherent 方法。trait 的真实价值只在 `NotifyingEventWriter` 装饰器这一条接缝 | 418 | medium（接缝有真实用途，问题在样板量） |

### 3.6 TST — 测试基建 / CI / 脚本 / 文档

**先回答本仓库自己提出的问题**：`AGENTS.md` 铁律 2 把"测试隔离曾有三份实现"列为已修正反例。实测**只被部分根治**——`clone SQL` 已单源（由 `tests/unit/test_isolation_unification_tests.rs:717` 的 Guard 7 强制），但 schema 的**创建 / 取池 / 删除 / URL 解析**仍有 5 个 fixture 文件、约 4,246 行、6–7 个 URL 解析器，且**两份 schema 契约已实际漂移**。同一个 bug 仍然要在多处修。

| ID | 标题 | 关键证据 | 可删/可收敛 | 置信 |
|---|---|---|---|---|
| TST-1 | **测试隔离只部分根治：schema 生命周期仍是 5 份并行实现** | 共享核心 `synapse-common/src/test_isolation.rs`(1,119) + `test_schema_guard.rs`(359)；仍是独立实现的 fixture：`src/test_utils.rs`(1,626) + `synapse-services/src/test_utils.rs`(707) + `synapse-storage/src/test_utils.rs`(206) + `synapse-storage/src/test_isolation.rs`(228)。「创建测试 schema」独立实现 **5–6 份**；「删除测试 schema」独立实现 **5 份**，其中 `IsolatedTestPool::Drop`（`storage/test_isolation.rs:187-226`）自己起线程 + join 做 DROP，**未接入共享 janitor**。Guard 7 的扫描范围（`:747`）显式 `continue` 掉不含 `/src/` 的文件，**只覆盖 clone 子句** | 合并 fork ≈ 900–1,200 | high |
| TST-2 | **测试库基础设施原语重复且已漂移** | 测试库 URL 解析器 **6–7 份**（`src/test_utils.rs:1509,1547`、`services/test_utils.rs:600,630`、`storage/test_utils.rs:57,89`、`storage/test_isolation.rs:35,56`、`storage/lib.rs:319`、`services/test_config.rs:19`、`tests/common/mod.rs:12`），**fallback 列表互不相同** ⇒ 同一台机器上不同 crate 的测试可能连到不同数据库；`fn next_test_schema_name()` 三处**逐字相同**（`src/test_utils.rs:1572`、`services/test_utils.rs:655`、`storage/test_utils.rs:179`）；`ensure_test_schema_contract` 两份且已漂移（services 比 root **多一条** `ALTER TABLE events ADD COLUMN IF NOT EXISTS reference_image TEXT;`，`diff` 输出 `10a11`） | ~250 + 消除已存在 drift | high |
| TST-3 | **生产构建混入 2,183 行 test-only 代码** | `synapse-common/src/lib.rs:95,99` 的 `pub mod test_isolation;` / `test_schema_guard;` **无任何 cfg**（注释自辩 "Compiled unconditionally"）；`synapse-storage/src/lib.rs:163` `pub mod user_store_fake;` 同样无 cfg，且 `:232` 把它 re-export 成**公开生产 API**（`user_store_fake.rs` 705 行，全部调用点都在 `#[cfg(test)]` 内）。对照：storage `lib.rs:147`、e2ee `lib.rs:44`、federation `lib.rs:46`、services `lib.rs:286,289,294`、root `lib.rs:30` **都正确门控** —— 只有这 3 处漏了 | 应门控（生产构建 −2,183） | high |
| TST-4 | **22 个测试文件从未被编译（5,982 行死测试）** | `tests/unit/mod.rs` 104 个 `mod` vs 125 个 `.rs` ⇒ **21 个未声明**（`application_service_tests` 314、`background_update_tests` 301、`event_report_tests` 318、`federation_signature_cache_tests` 430、`friend_groups_tests` 200、`module_tests` 362、`new_features_tests` 415、`qr_login_tests` 26、`rate_limit_config_tests` 331、`refresh_token_tests` 281、`registration_token_tests` 312、`retention_tests` 327、`room_cache_tests` 278、`room_summary_tests` 363、`saml_tests` 163、`security_regression_tests` 254、`security_tests` 312、`service_tests` 265、`sticky_event_tests` 36、`storage_tests` 203、`worker_tests` 445）；`tests/integration/mod.rs` 114 vs 115 ⇒ `api_qr_login_tests.rs` 46。**已独立核验：21 + 1 = 22 个文件，逐条命令复核**。部分内容已被内联同名模块重写而未删（`api_optimized_features_tests.rs:31 mod retention_tests {`、`new_features_tests.rs:3 mod storage_tests {`） | **5,982 / 22 文件** | high |
| TST-5 | **门禁空转或常红** | 详见 §1.3：fmt ratchet 当前**红**（已独立核验）、supply-chain 在 repo-sanity **恒过**、performance-baseline **100% 空转**、db-migration-gate **18 个 echo 占位**、schema-health-check 引用**不存在的 v10 文件**（必红） | 修 2 处 fmt；删/修 ~300 行 YAML | high |
| TST-6 | **CI 重复计算同一信号** | 全工作区测试每次 main/develop push 至少跑 **3 次**（`ci.yml:303/315`、`:611` 的 `cargo insta test --all-features` 重跑所有 target、`:822/825` 的 `llvm-cov --workspace`）；clippy **~34 次/轮**（`ci.yml:217` 4 leg + `:329` 2 leg + `check_missing_docs_ratchet.py:174-193` 对 7 个 crate 各一次且 `ci.yml:336` 无 `if:` 门控 ⇒ 4×7=28）；**格式化 3 套实现**（`check_fmt_ratchet.sh`、`format-governance.yml:43 format_check.sh` 为严格超集/权威、`format-drift-tracking.yml:49` 调同一个 `format_check.sh` 且报告硬编码 `--compliance-status pass`）；`test.yml`（87 行）整份是 `ci.yml` coverage job 的手工克隆且结果被丢弃 | 删 `test.yml` + `format-drift-tracking.yml` + 重复 job ≈ **500 行 YAML** | high |
| TST-7 | **scripts 囤积：56 项 / 166,956 行（38% 是两个生成 JSON）** | `api_test/response_schemas.json` **56,741 行** + `handler_schemas.json` **7,043 行** = 63,784（生成物）；真实代码脚本 102 个 / 28,985 行。**15 个真孤儿**（scripts 内外引用数均为 0）：`comprehensive_test.py` 1,923、`schemathesis_pathparam_test.py` 432、`replace_generic_db_errors.py` 321、`t1_{auth_compat,room,sliding_sync,sync}` 682、`fixture_converge_{apply,dryrun}` 209、`stress_quota_check.py` 163、`extract_routes.py` 118、`parse_coverage.py` 112、`inspect_coverage.py` 25 + 2 个 ignored 生成物（`fullstack-redo/lib/common.sh:24` 引用的 `run_t1_all.sh` 根本不存在）。**重复门禁实现**：missing-docs 有 3 份，CI 实跑的 baseline = `6`，无人调用的 `quality/check_missing_docs_ratchet.sh` baseline = **13833**（相差 2300 倍）；`parse_coverage.py`/`inspect_coverage.py` 读已废弃的 tarpaulin JSON 且零消费者；format 家族 4 份。**11 处悬空引用**（`ci_backend_validation.sh:213,225`、`Makefile:91-99`、`TESTING.md:96,112,113,128,409`、`.github/pull_request_template.md:16`） | 孤儿 3,985 + 废弃门禁 ~352；生成 JSON 63,784 行移出 scripts | high |
| TST-8 | **`.scratch/` 整目录已入库且未被 gitignore** | `git ls-files .scratch` = **97 个文件**；`git check-ignore -v .scratch` 无输出。内容为 12 个审计主题目录（`api-route-audit-2026-09-04`、`backend-issues-2026-09-06`、`e2ee-audit-2026-09-04`、`worker-audit-2026-09-04`…），与 `docs/audit/` 形成**第二份审计真相源**（铁律 4） | **97 文件 / ~7,909 行 md** | high |
| TST-9 | **预先摆放的 test mock 大量零调用** | storage `test_mocks/` 33 文件 / 10,121 行中，`InMemoryAdminFederationStore`(196)、`InMemoryRegistrationTokenStore`(399)、`InMemorySpaceStore`(504) **外部引用 0 且连 `mod tests` 都没有** = 1,099 行纯死代码；services `test_mocks.rs` 的 `FakeTokenAuth`/`TestSyncContext`/`MockSyncServiceDepsBuilder` 外部引用 0 仅自测；146 个 mock 公开项中 **18 个零外部调用**。`AGENTS.md` 自标这些是 "pre-positioned / scaffolding pending SYNC-1..6 / FED-1..4" ⇒ 正是铁律 1 禁止的"先留着以后可能有人用" | ~1,279 | high |
| TST-10 | **11 个 tracked-but-ignored 文件 + 6 个派生目录未忽略** | `git ls-files -i -c --exclude-standard` = **11**（`coverage/phase2-summary.md` 等 3 个、`.superpowers/sdd/task-8-9-report.md`、`.workbuddy/memory/*`×3、`.trae-html-share-packages/…zip`、`docker/config/homeserver.yaml`、`docker/deploy/config/homeserver.yaml`、`docker/nginx/ssl/server.crt`）；未忽略目录：`.scratch/`、`audit-verification-2026/`、`project-audit-2026/`、`thumbnails/`、`.hypothesis/`、`.ruff_cache/`。`.gitignore:22` 的 `homeserver.yaml` **未锚定**，误伤 2 份合法 docker 配置 | 11 个跟踪状态 + 6 条 gitignore | high |
| TST-11 | **测试夹具复制粘贴；共享 helper 零消费者** | `fn test_pool()` 出现 **59 处**，其中 **53 处 body 逐字相同**（同一 expect 字符串出现 53 次）；裸 SQL：`INSERT INTO users` 在 **52 个文件 / 95 处**、`INSERT INTO rooms` 39 处；`create_room` 定义 **60 处**、`create_user` 11、`create_test_user` 11（签名各异）。规范实现 `tests/common/mock_db.rs:95 create_test_user` 在 tests/ 下 **`mock_db::` 零命中**，且 `tests/common/mod.rs` 根本没声明它 | 53 个重复体 ≈ 265；统一再减 ~200 | high |
| TST-12 | 44 个 `_migrated` 集成测试（32,629 行 = integration 套件 55%）是迁移命名残留 | `ls tests/integration \| grep -c "_migrated"` = 44；44 个全部在 `mod.rs` 有声明（是活代码），但后缀只记录"曾经发生过的迁移" | 0（应重命名/合并） | high（计数）/ medium（合并） |
| TST-13 | **文档囤积与失效链接** | `docs/` 152 个 md / 55,603 行；`AGENTS.md:188` 指向 `docs/synapse-rust/MATRIX_SYNAPSE_AUDIT_AND_OPTIMIZATION_PLAN_2026-05-29.md`（**不存在**，`docs/INDEX.md:32` 已标为删除）；`docs/INDEX.md` 落后 HEAD 5 周且 §六 5 个链接全失效；`CLAUDE.md` 约定的 4 个 `docs/audit/00_*_baseline.log` **全部不存在**；错误的 `BACKEND_ROUTE_ISSUES_AUDIT_2026-09-13.md` 与其 `_REVISED` 版并存；`docs/archive/redundant-summaries/` 13 文件命名即冗余（`FINAL.md`/`FINAL_REPORT.md`/`FINAL_SUMMARY.md`/`COMPLETE_SUMMARY.md`/…）；根 `overview.md`(258) 实为 `# T10 MSC2666 完成报告` 且无任何索引引用 | ~12,000–20,000 | high（计数/链接）/ medium（取舍） |
| TST-14 | **`AGENTS.md` 与 `CLAUDE.md` 已分叉** | 238 vs 379 行；**102 行逐字相同**（占 AGENTS 50.7%）；最大重复块是**整节** `AGENTS.md:98-125 ≡ CLAUDE.md:61-88`（`## High-level architecture` 全节）。`CLAUDE.md:121-122` 自称铁律"只保留同步摘要"，实测 `AGENTS.md:54-97` vs `CLAUDE.md:119-150` 仅 **7/26 行（26%）** 逐字相同，CLAUDE 版**丢失 7 处反例**（测试隔离三份实现、`cargo machete`、`migrations/` 单一真相源、`vendor/pastey`、薄壳禁令、retention `max-threads=1`、`check_fmt_ratchet.sh` 计数恒 0 连续 21 个 CI 运行）⇒ **它声称要避免的双源漂移已经发生** | ~100–150 行 | high |
| TST-15 | `TESTING.md` 记录 3 个幽灵门禁，其声称的 CI 入口不被任何 workflow 调用 | `grep -rn "run_ci_tests" .github/workflows` → **无输出**（`ci.yml:303/315/582/611` 把其内部命令全部内联重实现），而 `TESTING.md:96,111` 称其为"当前 CI 等价默认测试入口"；`TESTING.md:96,115,393` 引用的 `detect_shell_routes.sh`、`detect_unwired_route_candidates.sh`、`run_e2ee_observability_gate.sh`、`quality-evidence` job **全部不存在**；覆盖率口径仍写 tarpaulin（CI 实为 `llvm-cov`） | 文档层 | high |
| TST-16 | **生成的 HTML 报告 bundle 入库** | `audit-verification-2026/` 与 `project-audit-2026/` 各 5 个 tracked 文件（含 ~1.0 MB `echarts.min.js` + 3 个 TTF + HTML），`git check-ignore` 对两目录无输出；`.gitignore:47` 已忽略同类 `test-suite-report/`，但 `.trae-html-share-packages/test-suite-report/test-suite-report.html.zip`（520 KB）仍被跟踪，内容正是被忽略的同一 bundle | **10 文件 / 2.6 MB + 520 KB** | high |
| TST-17 | **`tests/common/` 有 3 个未声明子模块（292 行死文件）** | `tests/common/mod.rs:1 #![allow(dead_code)]`，`:2-3` 只声明 `http_mock`/`snapshots`；`fixtures.rs`(140)、`mock_db.rs`(110)、`assertions.rs`(42) **无任何声明**且零使用 ⇒ 本应收敛夹具的公共目录**自身是死的**，并被 `allow(dead_code)` 静音 | 292 / 3 文件 | high |
| TST-18 | 快照目录分裂；根 `overview.md` 误置 | 31 个 `.snap` 中 `tests/unit/snapshots` **20**、`tests/integration/snapshots` 11，而规则文档规定"Snapshots live under `tests/integration/snapshots/`" ⇒ **65% 不在规定位置** | ~10 文件位置 | high（事实）/ medium（优先级） |

### 3.7 说明

§3.4 / §3.5 由主审与对应并行深挖合并而成；两者结论方向一致，主审已对其中最关键的条目（SVC-1 推送链路不可达、SVC-0 模块系统零调用、STO-1 迁移全量折入、STO-11 写读不同表、STO-14 死表与 0 Rust 引用）逐条复跑命令确认。

---

## 4. 删减优先级建议

### 第零批：正确性隐患优先排期（不是删代码，而是修缺陷）

| 项 | 问题 | 位置 |
|---|---|---|
| STO-11 | `set_account_data` 写 `user_account_data`，读走 `account_data` ⇒ 静默丢弃 | `synapse-storage/src/user/storage.rs:876` vs `:896` |
| STO-12 | 两个同名 `delete_events_before` 过滤语义相反 | `retention.rs:363` vs `event/basic.rs:40` |
| STO-10 / SVC-2 | `push_rules` 被两套 storage/service 双写 | `storage/push/mod.rs` vs `storage/push_notification.rs`；`routes/push.rs` vs `routes/push_notification.rs` |
| STO-13 | `room_directory` 被两个 storage 双写，`is_public` 可互相覆盖 | `room/mod.rs:1049` vs `directory/mod.rs:108` |
| CFG-1 | release 下 Olm pickle key 宽松入口返回全零密钥，生产路径正在调用它 | `synapse-e2ee/src/olm/service.rs:106` + `session.rs:44/:77` |
| STO-15 | 已 DROP 表的 no-op 方法恒返回空值，掩盖真实错误 | `retention_service.rs:390-422` |

### 第一批：零风险直删（引用计数为 0，无行为变化）

| 项 | LOC |
|---|---|
| CFG-2 `src/web/api_doc/` + CFG-3 二者取一的重复 OpenAPI | 12,836 + 72,924 生成行 |
| CFG-4 `runtime-ddl` + `tables.rs`（第二套 DDL） | ~1,280 |
| INF-1 worker 集群抽象（bus/health/stream/load_balancer） | ~1,820 |
| INF-2 `QueryCache` | 961 |
| INF-7 `collections.rs`/`early_exit.rs`/`nonce_cache.rs` | 501 |
| INF-6 `web/streaming.rs` + `filter.rs` | 335 |
| WEB-1 `directory.rs` | 334 |
| INF-16 `nonce_cache.rs`（与 INF-7 同一文件，勿重复计） | — |
| INF-5 `src/security/` | 297 |
| INF-11 `strategy.rs` 死 key/TTL | ~250 |
| WEB-12 2 个死中间件 + WEB-7 死 `Pagination` 提取器 | 46 + 98 |
| INF-8 5 个死宏 / INF-9 `TaskMetricsCollector` / CFG-10/11/15 | ~170 |
| **TST-4 22 个未编译测试文件** + **TST-9 死 mock** + **TST-7 孤儿脚本** + **TST-17 `tests/common` 死文件** | **5,982 + 1,279 + 3,985 + 292** |
| **SVC-1 推送投递链路（provider/queue/gateway 不可达）** | **~1,600** |
| **SVC-3 `LivekitClient`** + **SVC-4 `extensible_events.rs`** + **SVC-6 `shutdown.rs`** + **SVC-9 转发薄壳** | **460 + 255 + 33 + 44** |
| **SVC-0 无插件无接线的模块系统**（需先确认对外路由契约） | **~3,360** |
| **STO-1/2/3 已被 baseline 全量吸收的迁移与 v8 死副本** | **1,937 + 258 + 4,954** |
| **STO-6 `performance.rs`** + **STO-7 死 mock** + **STO-8 shim/prelude/身份测试** | **292 + 1,099 + 765** |
| **小计** | **约 50,500 行 Rust/脚本/SQL + 约 137,000 行生成物 + 78 个迁移文件中的 41 个** |

### 第二批：结构性合并（需跑 `cargo check --all-features --all-targets` 验证）

1. **拆掉兼容链**（A7）：删除 `/r0` 全部路由别名 → 删除 `suppress_r0_deprecation_warning`/`suppress_vendor_endpoint_warning` 配置 → 删除 `*_compat_router`/`*_r0_only_router`。预计 −400~900 行路由代码 + −489 条 ledger 条目。
2. **收敛路由元数据**（A3）：让 manifest 由 Router 构建期自动导出（或由 ledger 单向生成 Router），消除 95 个手写 manifest / 1,741 行；删除 `RouteEntry.query_params`/`auth`/`total_entries`；`Box::leak` 改为 `Cow` 或索引表。预计 −600~1,500 行 + 6 个 fixture 文件。
3. **合并 DI 层**（A4）：用一个 `trait AuthSource` + 泛型 `FromRequestParts` 取代 3×7 手工 impl（−596 行）；context 只保留真正需要的字段，或改为持有 `Arc<ServiceContainer>`。
4. **消除 legacy 认证兼容**（CFG-5/CFG-6）：单哈希 + Argon2-only，−250 行并去掉热路径上每次多一次索引查询。
5. **拆除 `src/*/mod.rs` 转发壳与扁平 glob**（A10/CFG-8/INF-10）：−200 行 + 16 处 `allow` 压制，模块路径唯一化。
6. **`deny(missing_docs)` 落地方式重做**（A11）：删除自指 `See [x].` 与 `The \`X\` field.` 样板，改为对真正需要说明的 pub 项写一句语义描述。

### 第三批：仓库与流程卫生

1. **先修红灯**：`cargo fmt --all` 修掉 `tests/unit/test_isolation_unification_tests.rs:619` 的 2 处格式债（当前主干 fmt 门禁为红，见 §1.3）。
2. `git worktree remove .claude/worktrees/optimization+audit-2026-07`（365,355 行 Rust / 39 MB 陈旧副本）。
3. 清理 `docker/deploy/backups/`（18 GB / 19 份副本）；决定 `docker/` 与 `docker/deploy/` 只保留一套（当前三份配置已漂移）。
4. 从索引移除：`.scratch/`(97) 、`coverage/`(3)、`audit-verification-2026/`(5)、`project-audit-2026/`(5)、`.trae-html-share-packages/…zip`、`.superpowers/sdd/`；`docs/openapi/client.yaml` 改为 CI 生成不入库；补 `.gitignore` 并修正未锚定的 `homeserver.yaml` 规则（TST-8/10/16）。
5. ✅ **门禁整治（第三批次，已完成）**：`ci.yml:739` 死步骤已修复（`--bin synapse_worker --locked`，去 `|| true`）；`schema-health-check.yml:73` v10→v11 已修；`drift-detection.yml:346-383` 空转 performance-baseline 门禁已移除（被冗余清理删掉的目标文件）；`test.yml` 与 `format-drift-tracking.yml` 已删除。以 `format-governance.yml` 为唯一格式化权威（TST-5/6）。
6. 修复 11 处悬空脚本/文档引用（TST-7）；按 workflow 实际内容重写 `TESTING.md` 主门禁（TST-15）；收敛 `AGENTS.md`/`CLAUDE.md` 双源漂移（TST-14）。
7. 加两条静态守卫：`tests/{unit,integration}` 下每个 `.rs` 必须有对应 `mod` 声明（防 TST-4 复发）；职责级"只允许一个定义"扫描（把 Guard 7 从"仅 clone 子句"扩展到模板创建/取池/删 schema/URL 解析，防 TST-1/2 复发）。

---

## 5. 明确不应删除的（防误伤）

审查中特意核实为**设计正确、不应改动**的部分：

- `synapse-storage/src/baseline_tables.rs`（178 行）：用 `include_str!` + 编译期解析 `migrations/00000000_unified_schema_v11.sql` 得到表清单 —— **单一真相源的正确范式**（应推广到列级校验，而非删除）。
- `synapse-storage/src/migration_checks.rs` 的拆分理由（把 `_sqlx_migrations` 校验从编排逻辑中分离）。
- `src/web/routes/space.rs` + `space/`：模块根 + 子模块的正确拆分范式（对照 `friend_room.rs` 的 1,177 行单体）。
- `ensure_room_view_access`（`handlers/room/mod.rs:39`，53 处调用）：有价值的具名封装。
- `RouteLedger` 的**存在理由**（防 `Router::merge` 静默覆盖）是成立的 —— 该删的是它的**手写重复**，不是这个机制本身。
- E2EE / federation 的核心实现（`synapse-e2ee` 24,501 行、`synapse-federation` 10,386 行）体量与职责匹配，不在本次冗余范围内。

---

## 附录 A：本次审查的取证命令（可复现）

```bash
# 生产源码规模
find src synapse-*/src -name '*.rs' -exec cat {} + | wc -l          # 355111

# 零信息文档注释
grep -rcE "^ */// (The \`[A-Za-z_]+\` (field|struct|enum|module|function|method)\.|See \[\`)" \
  --include=*.rs src synapse-*/src | awk -F: '{s+=$2} END{print s}'

# storage trait 多态性
python3 - <<'EOF'   # 见 §3.5：71 trait，69 个 <=1 非 mock impl
EOF

# 直连 storage 而不经 service 的路由文件
for f in $(grep -rl "use synapse_storage" src/web/routes --include=*.rs); do
  grep -q "use synapse_services" "$f" || echo "$f"; done | wc -l    # 18

# 从未启用的 feature
grep -rn 'feature = "server"' --include=*.rs src synapse-*/src | wc -l   # 0

# CI 死步骤
grep -n "exclude synapse_worker" .github/workflows/ci.yml                 # 747
grep -n "^name" Cargo.toml */Cargo.toml | grep -c synapse_worker          # 0
```

## 附录 B：与既有审计文档的关系

本仓库 `docs/audit/` 下已有 30+ 份按主题（P1 安全 / P2 协议 / P3 数据层 / P4 CI / P5 工程）编排的报告。本报告**不重复**其安全与协议结论，只做**冗余与过度开发**这一横切视角，并纠正其中一处不成立的结论：

- `artifacts/synapse-rust-code-review-2026-09-03.md:162` 将「数据库双重 DDL 定义（runtime-ddl vs migrations）」标为「✅ 已修复」，但 `tables.rs`（1,249 行）与 `runtime-ddl` feature 至今完整存在（见 CFG-4）。

---

# 附录 C：第一批执行记录（2026-09-14）

在第零批之后，**第一批"零风险直删"已执行完毕**，提交于隔离 worktree（分支
`optimization/redundancy-cleanup-2026-09-14`，提交 `a0f2819d`）。

## C.1 执行结果

| 指标 | 数值 |
|---|---|
| 变更文件 | 189 |
| 删除行 | **39,432** |
| 新增行 | 508 |
| 整文件删除 | **149** |

## C.2 已删除项（按原报告 ID）

| ID | 内容 | 行数 |
|---|---|---|
| CFG-2 | `src/web/api_doc/`（12,836）+ `utoipa`/`utoipa-swagger-ui` 依赖 + README `/_swagger` 说明 | ~12,900 |
| CFG-4 | `runtime-ddl` 第二套 DDL：`tables.rs` + 4 处 Cargo feature 声明 | ~1,280 |
| INF-1 | worker `bus.rs` + `stream.rs`（真正无消费者部分） | ~1,460 |
| INF-2 | `synapse-cache/src/query_cache.rs` | 961 |
| INF-7/16 | `collections.rs`、`early_exit.rs`、`nonce_cache.rs` | 501 |
| INF-6 | `web/streaming.rs`、`web/filter.rs` | 335 |
| WEB-1 | `web/routes/directory.rs`（未接入任何 router） | 355 |
| WEB-7/12 | `Pagination` 提取器 + 2 个死中间件 | 144 |
| INF-5 | `src/security/`（**未在 lib.rs 声明，游离于编译单元之外**） | 477 |
| INF-9 | `TaskMetricsCollector`/`CollectedMetrics` 死链 | ~85 |
| INF-8/CFG-15 | 5 个零调用宏、`ScheduledTasks::new`、`get_event_server_name` | ~90 |
| SVC-3 | `rtc/sfu.rs`（`LivekitClient`） | 583 |
| SVC-5 | `event_service.rs`、`rendezvous_service.rs`（**同样未在 lib.rs 声明**） | 31 |
| STO-6 | `synapse-storage/src/performance.rs` | 292 |
| SVC-15 | `media_quota_service` 死注入（2 处 context 各 3 行） | 8 |
| TST-4 | **22 个从未被编译的测试文件** | 5,982 |
| TST-9 | 3 个零引用 storage mock | 1,099 |
| TST-17 | `tests/common/{fixtures,mock_db,assertions}.rs`（未声明） | 292 |
| TST-7 | 13 个孤儿脚本 + 4 个无消费者的重复门禁脚本 | ~4,300 |
| STO-1/3 | 72 个已被 v11 全量吸收的增量/undo 迁移 + `migrations/archive/` | ~6,900 |

## C.3 执行中被否决的项（引用计数复验后**不能删**）

原报告列为"零风险"，但动手前的引用计数复验推翻了其中 3 项，**已保留**：

| 项 | 否决理由 |
|---|---|
| CFG-4 的 `00000001_extensions_v10.sql` | 被 `test_utils.rs:225`、`test_isolation_unification_tests.rs:53,619` 用 `include_str!` 引用，且 `migration_consistency_tests.rs:19` 断言其存在 |
| INF-1 的 `health.rs` + `load_balancer.rs` | `tests/integration/worker_task_recovery_tests.rs`（725 行 / 8 测试）真实注入并断言 LB 行为（测试名即含 `_removes_worker_from_lb_candidates`）；删除会移除**被测试覆盖的逻辑**，不属零风险 |
| SVC-9 的 `worker/storage.rs`/`types.rs`、`event_broadcaster_trait.rs` | 在 crate 内部有真实消费者，删除需改写多处 import，收益 9–26 行，性价比不足 |

## C.4 需产品决策而暂缓的项

| 项 | 暂缓理由 |
|---|---|
| **SVC-0** 模块系统 3,360 行 | 其路由是**对外 HTTP 契约**的一部分；删除会影响 SDK ledger 与 `_synapse/admin/v1/modules` 客户端。应先裁定是否保留该 API 面 |
| **SVC-1** 推送投递链路 ~1,600 行 | 需二选一：接线 `initialize_providers()` 并删 fallback，或整体删除 provider/queue/gateway。属产品能力取舍 |
| **CFG-11** `server` 伪 feature | 不是死代码而是**构建配置重构**：需同步改 `docker/Dockerfile`、`docker/complement/Dockerfile`、`docker-compose.yml`、`run_element_web_browser_harness.sh`、`deploy.sh` 与 CI 矩阵，且无法在无 Docker 环境下验证 |
| SVC-13/16、WEB-11 等 <60 行微壳 | 需逐处改写调用点，收益极小，留待第二/三批结构性合并一并处理 |

## C.5 验证证据

| 门禁 | 命令 | 结果 |
|---|---|---|
| 格式 | `./scripts/check_fmt_ratchet.sh` | **current=0 baseline=0，OK**（同时修掉了主干原有的 2 处格式债，该门禁此前为**红**） |
| 格式 | `cargo fmt --all -- --check` | exit 0 |
| 编译 | `cargo check --workspace --all-features --all-targets --locked` | 0 error / 0 warning |
| Lint | `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | **0 error / 0 warning** |
| 单测 | `cargo test --test unit --all-features --locked` | **1915 passed / 2 failed** |
| 迁移单源 | `python3 scripts/check_migration_consistency.py` | `status: ok`，`primary_forward_files: 2` |
| 迁移折入 | `python3 scripts/check_baseline_consolidation.py` | `✅ 已吸收全部 0 个增量迁移的对象` |

**2 项失败为既有红灯，非本次引入**：`sqlx_ratio_gate_tests` 报
`dynamic=1466 > baseline=1443`。已在**主工作树 HEAD（未含本次改动）**复跑同一门禁，
输出**完全一致**（1466 / 1443），确认与本批删除无关；本次改动未新增任何 SQL
（扫描计数前后均为 1466）。该棘轮基线最后更新于 2026-09-12（1443），
其后提交使其漂移到 1466。详见 §1.3 关于门禁可证伪性的同类问题。

## C.6 并发冲突说明（重要）

执行过程中发现**另一个 agent 会话正在同一工作树并发操作**：它以
`git stash` 收走了本批 A/B 阶段成果（`stash@{0}: redundancy-audit WIP
(separate session, do not lose)`），并在 17:18 提交了 `9875d8ff`。

处置：本批全部工作在**隔离 worktree** 中完成 ——
`.worktrees/redundancy-cleanup`（分支 `optimization/redundancy-cleanup-2026-09-14`，
基线 `9875d8ff`）。主工作树的未提交状态未被本批修改。

**另外记录一项既有数据丢失**：会话开始时主工作树存在 `synapse-storage/src/voice.rs`
的未提交改动；该改动不在任何近期 stash 中（`stash@{0}` 及 `stash@{4..8}` 均不含），
现已被并发会话的 checkout 丢弃。非本次操作所致，但请留意。

---

# 附录 D：第一批收尾 + 第零批正确性修复（2026-09-14）

## D.1 已修复的正确性隐患

报告 §4「第零批」列为**优先级高于任何删减**的 5 项，已修复 3 项：

### D.1.1 CFG-1 — Olm pickle key release 全零回退（安全）

- **缺陷**：生产 E2EE 路径 `synapse-e2ee/src/olm/session.rs:44`（`load_sessions`）
  与 `:77`（`persist_sessions`）调用宽松版 `get_pickle_key()`；该函数在 release
  构建下于 `PICKLE_KEY` 未初始化时 `tracing::error!` 后返回
  `static ZERO: [u8; 32]`，即把 Olm 会话以全零密钥加解密 —— 写出的数据不可恢复。
  原注释声称该分支 "unreachable because gated behind `cfg(debug_assertions)`"，
  与实际**相反**（release 分支正是 `cfg(not(debug_assertions))`）。
- **修复**：两处改调 `get_pickle_key_strict()?`；删除宽松入口（debug 随机回退、
  release 全零桩、`generate_random_pickle_key`）。
- **验证**：`cargo test -p synapse-e2ee --lib --all-features` = 429 passed / 0 failed。

### D.1.2 STO-11 — `UserStorage::set_account_data` 静默丢数据

- **缺陷**：该方法 `INSERT INTO user_account_data(event_type, content)`，而**所有**
  读路径（`get_account_data_content`、`AccountDataService::get_account_data`）读的是
  `account_data(data_type, content)` —— 写进去的数据无人能读。
- **核实**：其唯一调用者是它自己的 db_test；路由用的是
  `AccountDataService::set_account_data` → `upsert_account_data_content` → `account_data`
  （正确）。故这是纯死写路径。
- **修复**：删除该方法及其 db_test。`user_account_data` 表随之零引用，列入待 DROP 清单。

### D.1.3 STO-12 — 两个同名删除方法语义相反

- **缺陷**：`RetentionStorage::delete_events_before(room_id, cutoff) -> i64` 只删
  **本地非状态**消息（`state_key IS NULL` 且排除 4 类状态事件）；而
  `EventStorage::delete_events_before(room_id, ts, dry_run) -> u64` 只删**远端**事件
  （`origin != self`）。同一张 `events` 表、相反过滤条件、仅凭名字极易选错。
- **修复**：按实际语义重命名为 `delete_local_messages_before` /
  `delete_remote_events_before`（纯重命名，编译器与既有测试共同验证）。
  `AuditStorage::delete_events_before` 作用于 `audit_events` 表、语义无歧义，保持原名。
- **未采用**原报告的"合并为带 `scope` 参数的单一实现"：那会改变行为（本地+远端删除
  合一），需业务裁定；重命名已消除实际风险且零行为变更。

## D.2 未修复的第零批项（需产品决策）

| 项 | 为何不能零风险修 |
|---|---|
| SVC-2 / STO-10 `push_rules` 双写 | 需先裁定保留哪套 push-rule 实现（标准 `pushrules` 还是 `/_matrix/client/r0/push/rules`），涉及路由契约与存储表 |
| STO-13 `room_directory` 双写 | `RoomStorage::set_room_directory` 可写 `is_public=false`，`DirectoryStorage` 强制 `true`；合并需裁定语义归属 |
| STO-15 retention no-op 方法掩盖错误 | 这些方法有 admin 路由调用，删除即移除/改变 admin API 面 |

## D.3 本批（a0f2819d + 7cb25947 + 21cc11c1）累计

| 指标 | 数值 |
|---|---|
| 变更文件总数 | ~200 |
| 删除行 | **≈ 39,990** |
| 整文件删除 | **152** |

## D.4 一次验证口径失误（已修正，值得记录）

`a0f2819d`（G 阶段迁移清理）**引入了一处测试回归**：
`migration_checks::tests::discover_finds_baseline_migrations` 断言"至少 5 个时间戳
迁移文件存在"——正是被 v11 baseline 吸收后删除的那批。

**根因是验证口径不完整**：当时只跑了 `cargo test --test unit`（根 crate 的 unit 目标）
与各 crate 的 `--lib` **部分**，没有跑 `cargo test --workspace --lib`，因此
`synapse-storage` 的 lib 测试从未执行。

**修正**：`21cc11c1` 重写该测试以锁定真正需要的不变量（返回值全为 14 位时间戳 /
有序 / 去重，且 baseline 与 extensions 文件必须被排除），并补齐完整验证：

```bash
TEST_DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test \
  cargo test --workspace --all-features --lib --locked -- --test-threads=4
```

结果：`synapse-common` 869 / `synapse-storage` 1757 / `synapse-services` 2025 /
`synapse-cache` 89，**0 failed**。唯一一次
`test_schema_guard::…released_pool_triggers_cleanup…` 超时为 DB 并行负载下的已知
flaky（隔离复跑两次均通过，见仓库「已知坑 #10」）。

> 教训与 §1.3 同源：**声明"已验证"之前必须确认验证覆盖了所有目标**。本仓库的
> `--test unit` 与 `cargo --workspace --lib` 是两条不同车道，只跑前者会漏掉全部
> workspace crate 的单元测试。

## D.5 待办清单（本次未做，按优先级）

1. **DROP `user_account_data` 表**（现零引用）+ 报告 STO-14 的另外 22 张零引用表，
   其中 6 张 AI/OpenClaw 表带触发器与注释（`migrations/README.md` 已自认）。
2. **第零批剩余 3 项**：`push_rules` 双写、`room_directory` 双写、retention no-op
   方法（均需产品裁定，见 D.2）。
3. **第一批暂缓**：SVC-0 模块系统 3,360 行、SVC-1 推送链路 ~1,600 行、
   CFG-11 `server` 伪 feature（构建配置重构，需 Docker 环境验证）。
4. **第二批结构性合并**：拆 `/r0` 兼容链（−400~900 + −489 ledger 条目）、
   收敛路由元数据（−600~1,500）、合并 DI 层泛型化（−596）、
   legacy 认证兼容（−250 且去掉热路径每次多一次索引查询）、
   `deny(missing_docs)` 落地方式重做（−15,571 行零信息注释）。

---

# 附录 E：零引用表 DROP（2026-09-14）

## E.1 范围与结果

删除 **23 张 Rust 全仓零引用表**（`\b<表名>\b` 引用计数为 0）：

`user_account_data`、`voice_messages`、`user_reputations`、`typing_stream`、
`security_events`、`room_stats_current`、`room_parents`、`receipts_linearized`、
`reaction_aggregations`、`presence_stream`、`password_history`、
`openclaw_connections`、`migration_audit`、`ip_blocks`、`federation_inbound_events`、
`federation_blacklist_config`、`event_forward_extremities`、`destination_retry_timings`、
`ai_messages`、`ai_generations`、`ai_conversations`、`ai_connections`、`ai_chat_roles`

| 一并删除的对象 | 数量 |
|---|---|
| `CREATE TABLE` 块 | 23 |
| 显式 `CREATE INDEX` | 25 |
| `COMMENT ON COLUMN` | 54 |
| `pg_constraint` FK 补丁（DO 块内） | 4 |
| AI 触发器 `DO` 块 | 3 |
| **合计行数** | **442**（baseline 400 + extensions 25 + 文档） |

## E.2 两处必须一并处理、否则清理不彻底的地方

1. **`voice_messages` 同时存在于 `00000001_extensions_v10.sql`** —— 只删 baseline
   会让启用 `voice-extended` 的部署继续建出该死表。两处都已删除。
2. **`extension_map.conf` 的说明是错的** —— 它声称
   `voice-extended -> voice_messages`，而 `synapse-storage/src/voice.rs` 实际读写的是
   `voice_usage_stats`（在 baseline 中）。已改正。这也解释了为何 `voice_messages`
   会是零引用：功能的真实表从来不是它。

## E.3 未采用原 README 的"留待 v12"方案

`migrations/README.md` 原称这些死表"因迁移文件遵循 append-only，暂不清理，待 v12"。
该推理不成立：

1. `build_sqlx_migration_source.py` 只选 baseline + extension + `V*` 迁移 ——
   时间戳命名的 `DROP TABLE` 迁移**根本不会被执行**，"留待 v12"在实践中等于永久不清。
2. 项目**未发布、无外部用户、无生产数据**（`AGENTS.md` 铁律 1），append-only 的
   前提（保护存量部署）不存在。

## E.4 验证方法（迁移改动唯一可靠的证明）

在**两个全新数据库**中分别把改动前/后的 baseline 应用到 `public`，再比对：

| 检查 | BEFORE（原始 baseline） | AFTER（改后） | 结论 |
|---|---|---|---|
| 应用退出码 / ERROR 数 | 0 / 0 | 0 / 0 | 双方均可干净应用 |
| `public` 表数 | **255** | **232** | 差恰好 **23** |
| 表名 `EXCEPT` 双向差集 | — | only_before=**23** / only_after=**0** | 除这 23 张外 schema 完全一致 |
| 索引数（`pg_indexes`） | 764 | 710 | −54 = 25 显式 + 29 随表删除的主键/唯一索引 |
| baseline + extensions 组合应用 | — | 0 error；`voice_messages` 不存在、`voice_usage_stats` 保留 | 扩展路径同样干净 |

> **为何必须用全新数据库**：共享测试库的 `public` 中仍残留这些表，会把
> baseline 里的 schema-blind 守卫"喂饱"，从而掩盖问题（见 E.6）。

## E.5 门禁与测试

| 门禁 | 结果 |
|---|---|
| `./scripts/check_fmt_ratchet.sh` | current=0 / baseline=0 **OK** |
| `cargo clippy --workspace --all-targets --all-features -D warnings` | **0 error / 0 warning** |
| `cargo test --test unit --all-features` | 1910 passed / 2 failed（既有 sqlx 棘轮红灯） |
| `check_migration_consistency.py` | `status: ok`，`primary_forward_files: 2` |
| `check_baseline_consolidation.py` | ✅ |
| `test_isolation_unification_tests` 全部 8 个守卫 | 通过 |
| `migration_consistency_tests` / `migration_replayability_guard_tests` | 通过 |

**一处由本次改动引起、已修复的测试**：
`baseline_fingerprint_is_v11_then_extensions_with_no_separator` 用**内容哈希**锁定
v11++extensions 的拼接，任何迁移编辑都会触发。已更新常数（`7c3a8965…` →
`8e0da5c4…`）并补充说明：该守卫的真正职责是抓**错误拼接**（分隔符
`a05fa4488475fe1d` / 反转 `4137af770181767b`），而非阻止合法的迁移编辑。

## E.6 顺带发现的两个既有 baseline 缺陷（**未修**）

两者都与本次删除无关，但在全新数据库上暴露：

1. **`-- typing composite PK` 守卫硬编码 schema**
   ```sql
   IF NOT EXISTS (SELECT 1 FROM information_schema.table_constraints
                  WHERE table_schema = 'public' AND table_name = 'typing'
                    AND constraint_name = 'pk_typing') THEN
       ALTER TABLE typing ADD CONSTRAINT pk_typing PRIMARY KEY (user_id, room_id);
   ```
   `typing` 的建表语句**已内联** `CONSTRAINT pk_typing PRIMARY KEY`。当 baseline 被
   应用到非 `public` schema（测试隔离正是逐 schema 应用）时，守卫查的是
   `public.typing`（不存在）→ 判定"需要添加"→ `ALTER TABLE typing` 经 search_path
   命中刚建好的目标 schema 表 → **`multiple primary keys for table "typing"`**。
   在共享库上因 `public.typing` 已存在而长期被掩盖。
2. **`-- user 表其他 user_id 字段` 的 DO 循环**：同样硬编码 `table_schema = 'public'`，
   且是**全文件唯一没有 `IF NOT EXISTS` 守卫**的约束块 —— 对同一 schema 重复执行
   必然报 `constraint ck_<t>_user_id_format ... already exists`。

两项都应在 v12 重构时改为按**实际目标 schema**（`current_schema()` 或 search_path
首项）判定，而非写死 `public`。已记入 `migrations/README.md`。

## E.7 待办（更新自 D.5）

1. ~~DROP `user_account_data` + STO-14 的 22 张零引用表~~ —— **本附录已完成**。
2. **第零批剩余 3 项**（需产品裁定）：`push_rules` 双写、`room_directory` 双写、
   retention no-op 方法对应的 admin 端点去留。
3. **第一批暂缓**：SVC-0 模块系统 3,360 行、SVC-1 推送链路 ~1,600 行、
   CFG-11 `server` 伪 feature。
4. **baseline 剩余欠债**（本次未动）：`events.reference_image` 死字段、
   `idx_rooms_name_trgm`/`idx_rooms_canonical_alias_trgm` 各重复定义两次、
   E.6 的两个 schema-blind 守卫。
5. **第二批结构性合并**：拆 `/r0` 兼容链、收敛路由元数据、合并 DI 层、
   legacy 认证兼容、`deny(missing_docs)` 落地方式重做。

---

## 附录 F：文档口径声明（2026-09-14 澄清）

本报告包含两个**互不相同的审计范围**，引用时请先确认所指：

### F.1 `main` 分支（当前权威基线）

- 分支指向 commit **`6ba7c457`**（HEAD = `b0e8ed9e` style: cargo fmt fix debt）。
- `main` 上**尚不存在**第一批删除（`a0f2819d` 等），`api_doc/`、`directory.rs` 仍在，
  `#[allow(dead_code)]` 计数仍为 **151 处**，`src` 生产源码 **355,111 行**。
- `main` 在 9875d8ff 与清理分支分叉（`git merge-base main optimization/redundancy-cleanup-2026-09-14` = `9875d8ff`），
  两分支此后**各自独立演进**：`main` 独有 4 个提交（§15.2/§16/fmt 修复），清理分支独有 11 个提交（含 a0f2819d 等）。
- 本附录 E 中所有「第一批已删除」条目描述的是**清理分支**的产物，**不是** `main` 的当前状态。
- 本附录 F 是 `main` 上对该分叉事实的正式记录。引用清理分支的规模数据（−39,990 行 / 315,121 行 / 149 文件删除）时，应标注来源分支；不可直接作为 `main` 的当前指标。

### F.2 清理分支（`optimization/redundancy-cleanup-2026-09-14`）

- 基线 `9875d8ff`（HEAD 偏移 11 个 commit）。
- 第一批删除以 commit **`a0f2819d`**（附录 C）为主体：189 文件变更、删除 39,432 行、新增 508 行、整文件删除 149 个。
- 验证（附录 C.5）：fmt/cargo check/clippy 全绿；`cargo test --test unit --all-features` 1915 passed / 2 failed（既有 sqlx 棘轮红灯）。
- 与 `main` 的差异不在本报告记录范围（参见该分支的独立审计）。

