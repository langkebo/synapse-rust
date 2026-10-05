# synapse-rust 现存问题清单（审查验证版）

> **状态（2026-09-28）**：本文关于"v12/v13 不可创建"的记录已过时 —— v12 现在是**唯一可创建**版本
> （G-1），版本 13 已移除（Q5(b)）。MSC4291（创建侧 C-1/C-2、入站 D-1、升级 C-4）、MSC4289（E-1/E-2/E-3）、MSC4307（B-2）均已落地；**仅 MSC4297（State Resolution v2.1）未落**（本仓当前无状态决议路径）。 见 `docs/audit/ROOM_V12_PLAN_STATUS_2026-09-27.md`。

日期：2026-09-14（**第五轮复核**，见 §23 本轮收口）
基线：`opt/consolidated` @ `af2df7913`（首版基线 `e6ecda02`，第四轮基线）
验证方式：**每条都附实测命令与实测结果**。未实测的明确标注 `[未验证]`。

## 轮次指针（先读这张表，再决定信哪一节）

| 轮次 | 日期 | 基线 | 章节 | 说明 |
|---|---|---|---|---|
| 首版 | 2026-09-14 | `main` @ `e6ecda02` | §1–§14 | 首版清单与排序 |
| 第二轮 | 2026-09-14 | `main` @ `d56a1d82` | §15–§20 | 复核 + 汇总（§18） |
| 第三轮 | 2026-09-25 | `opt/consolidated` @ `9e26ee31a` | §21 | 全量回源码重判 |
| 第四轮 | 2026-09-25 | `opt/consolidated` @ `af2df7913` | §22 | 逐条复核 13 项报项（SAS 三处仍偏离等） |
| **第五轮** | **2026-09-25** | **`opt/consolidated` @ HEAD（E2EE 去服务端私钥重构）** | **§23** | **删设备验证私有面；连带清除纸面门禁、棘轮死条目、文档过时结论** |

> **当前口径只有一个**：`§0.2 严重度分布` + `§22.3 仍然存在`（含 §23 的判定）是唯一"当前状态"来源；
> 其余章节（含 §18 / §21.1 / §22）均为**历史快照**，保留用于追溯，不得直接引用其状态标记。
> 判据一律可复现：`路径:行号` 或"命令 + 期望输出"。

---

## 0. 图例与验证环境

### 0.1 图例

| 标记 | 含义 |
|---|---|
| ✅ | 已修复（附修复提交）。**注意**：标 ✅ 的前提是"缺陷本身已消失"；仅清掉症状而门禁仍不覆盖的，标 🔴 并在 §15 说明 |
| 🔴 | 确认存在，需修 |
| 🟡 | 确认存在，但需先决策（属产品或架构取舍） |
| ⚪ | 存在但不建议现在动（成本/风险不匹配） |
| `[未验证]` | 未取得实测证据，仅为待查线索 |

**验证环境**（所有命令在此环境下执行）：

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
export SQLX_OFFLINE=true CARGO_TARGET_DIR=/tmp/audit_target
export TEST_DATABASE_URL="postgresql://synapse:<pw>@<container-ip>:5432/synapse_test"
export DATABASE_URL="$TEST_DATABASE_URL"
unset SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE          # 绝不设置
# 并发一律 --test-threads 4（容器仅 1.5 CPU，8 线程会触发服务端认证超时）
```

### 0.2 严重度分布（第四轮口径，唯一权威）

复核基线 `opt/consolidated` @ `af2df7913`（2026-09-25）。**13 项报项的判定汇总**：

| 严重度 | 项数 | 仍存在 | 部分 | 证伪 | 已修复 |
|---|---|---|---|---|---|
| P0 | 1 | 0 | 0 | 0 | **1** |
| 高 | 3 | 2 | 1 | 0 | 0 |
| 中 | 6 | 2 | 3 | **1** | 0 |
| 低 | 3 | 3 | 0 | 0 | 0 |
| **合计** | **13** | **7** | **4** | **1** | **1** |

（"部分"= 同一项内子结论分裂；"证伪"= 该缺陷不存在。逐条依据见 §22.3 / §22.4。）

**当前仍须处理的项**（按严重度，明细见 §22.3）：

> ⚠️ **本清单已过期，当前口径以
> [`REMAINING_ISSUES_VERIFICATION_AND_OPTIMIZATION_PLAN_2026-09-25.md`](./REMAINING_ISSUES_VERIFICATION_AND_OPTIMIZATION_PLAN_2026-09-25.md)
> §0.3 + §5 为唯一来源**（该文档逐条实测 main，并给出 §6 决策记录）。
> 相对本清单已失效的条目：第 2 项 Content Scanner **已接线**（`routes/media/upload.rs:87,128`、
> `handlers/room/events.rs:306`）；第 5 项 `auth_issuer` 路由**已摘除**；第 6 项稳定
> `/{keyName}` **已注册**（`assembly.rs:231`）、停用用户语义**已改**；第 9 项 `search_index`
> **已删表**（`00271cf91`）。保留本清单仅为追溯。

1. 高｜客户端撤回不级联（MSC3912 级联仅管理端可达）；
2. 高｜Content Scanner 零生产调用点（装配了但永不扫描）；→ **已修，见上方指针**
3. ~~高｜E2EE SAS **三处**仍偏离（emoji 映射、decimal 算法、MAC 派生）——另两处已修；~~
   **作废：服务端 SAS 实现已随 §23 删除（对象消失，非"已修复"）；且原描述本身有误
   （N-1/N-2 实测：emoji 数量已对、算法仍错；decimal 系从 emoji 反推）**；
4. 中｜`dag.rs` 注释声称的被 `/send_join`、`/get_missing_events` 使用，实测无生产调用点；
   → 注释**已改**；死查询仍未清（见新文档 §5 U-9）
5. 中｜`msc2965/auth_issuer` 仍在册（上游 1.161 已删该端点）；→ **路由已摘除**（handler 成死码）
6. 中｜Profile：停用用户写自定义字段 404、稳定 `/{keyName}` 未注册（account_data 非对象已修为 400）；
   → 两条**均已改**；但 `user_exists` 去过滤的扩散需复核（新文档 §5 R-1/U-2）
7. 中｜Admin 媒体端点族真缺口（`media/quarantine|unquarantine` POST、房间级媒体列举/删除）；
8. 中｜缩略图 `animated` 参数未支持；媒体配额拒绝未使用 `M_USER_LIMIT_EXCEEDED`（归因待议）；
   → 错误码已决策为 `M_TOO_LARGE`(413) / `M_RESOURCE_LIMIT_EXCEEDED`(403)（新文档 §6.4）
9. 低｜v12/v13 不可创建（**fail-safe 设计使然**）、`search_index` 遗留表、ledger `query_params` 无消费方。
   → `search_index` **已删表**；`query_params` 已决策为"真填值 + 反向守卫"（新文档 §6.5）

---

## 1. ✅ 数据安全：CI 把测试指向生产库并主动解除保护

**已修复**（提交 `00c0aad2`：一库两 schema + TEST_DB_TEMPLATE_SCHEMA 钉模板）

### 证据

```bash
$ grep -nE "SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE|TEST_DATABASE_URL: postgresql" .github/workflows/ci.yml
292:          SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE: "1"
294:          TEST_DATABASE_URL: postgresql://synapse:synapse@localhost:5432/synapse
319:          SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE: "1"
321:          TEST_DATABASE_URL: postgresql://synapse:synapse@localhost:5432/synapse
330:          SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE: "1"
332:          TEST_DATABASE_URL: postgresql://synapse:synapse@localhost:5432/synapse
547:          SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE: "1"
549:          TEST_DATABASE_URL: postgresql://synapse:synapse@localhost:5432/synapse
```

- 4 个步骤同时具备"**指向应用库 `synapse`**"+"**打开 wipe 开关**"这一危险组合
- 该开关的作用是**关闭** `src/test_utils.rs` 里"`public.schema_migrations` 存在就拒绝
  `DROP SCHEMA public`"的保护——即 CI 主动允许测试清空 `public`
- CI **从未创建** `synapse_test`：

```bash
$ grep -nE "createdb|CREATE DATABASE|synapse_test" .github/workflows/ci.yml
16:  TEST_DATABASE_URL: postgresql://postgres:postgres@localhost:5432/synapse_test
```

只有顶层 `:16` 提到它，而 service 定义的用户是 `synapse:synapse`
（`POSTGRES_USER: synapse`），顶层却用 `postgres:postgres` → **该默认值本身是坏的**，
所以各步骤只能覆盖成应用库。

### 实测后果

按 CI 口径（指向应用库 + 开 wipe）跑一次全量门禁：

```
6165 run: 5103 passed, 1033 failed, 13 skipped
public 表数: 253 -> 3        schema_migrations: 37 -> 0
relation "users" does not exist (42P01)   遍布 storage 各 db_tests
```

**一次运行即清空 `public`，产生 1033 个假失败。** 这不是竞态，是该配置下的必然结果。

### 建议修法（采纳"分两步 + 一库"，2026-09-14 已实施 ✅）

**文档建议的修法（改指测试库 + 删 wipe 标志）是必要不充分**——它消除了"指向应用库+解除保护"，
但未解决一个更深的共存矛盾：storage `db_tests` 直连 `public` 需要 256 表，而 services/root 的
`prepare_shared_test_pool` 首次调用会经 `init_template_schema` **DROP SCHEMA public** 重建模板——
两者在同一 nextest 步骤共享一个库必然串扰，这正是历史 "1033 假失败 → public 253→3" 的**机制根因**
（不是竞态，是结构性的）。用户确认采纳"分两步 + 一库"。

**最终方案（已落地）**：

1. **seed 步骤** `scripts/ci/prepare_test_db.sh`：一次性把迁移灌进两个 schema——
   `public`（storage 直连）+ `test_template_ci`（services/root 克隆模板）。
   模板用 `PGOPTIONS='-c search_path=test_template_ci,public'` 让非 schema 固定的
   `CREATE TABLE IF NOT EXISTS` 落进模板 schema（已实测：254 表 ✓）。
   最后校验两个 schema 均 ≥200 表，否则 fail-fast。

2. **所有测试步骤设 `TEST_DB_TEMPLATE_SCHEMA=test_template_ci`**（Test & Lint 的
   lib/media-guard/unit 三步 + integration 的 admin-registration 步）：
   该 env 让 root `prepare_shared_test_pool` 走 `ensure_template_schema_exists`
   **verify-only 路径，永不进入 `init_template_schema`**——`DROP SCHEMA public`
   在 CI 中**结构性不可能发生**，而非仅靠守卫拦截。

3. **守卫强化**（`src/test_utils.rs`）：数据库名含 `test` 视为可重建测试库，
   无论是否已带 `public.schema_migrations`（旧判据会连 `synapse_test` 也拒绝，
   见 §3.2）。同时顶层凭据 `postgres:postgres` → `synapse:synapse`。

4. **`SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE` 在 CI 中零出现**（grep 验证）。

### 修复后验证（实测）

```bash
$ grep -nE "SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE|TEST_DATABASE_URL: postgresql" .github/workflows/ci.yml
16:  TEST_DATABASE_URL: postgresql://synapse:synapse@localhost:5432/synapse_test
# 4 个测试步骤全部指向 synapse_test + TEST_DB_TEMPLATE_SCHEMA: test_template_ci；
# WIPE 标志 0 处。
```

```bash
# seed 后两 schema 共存
$ bash scripts/ci/prepare_test_db.sh
==> public: 255 tables; test_template_ci: 254 tables
==> synapse_test ready: public + test_template_ci coexist.

# 根 §3.2 测试（prepare_shared_test_pool + pin）→ 通过，且 public 不被 DROP
$ cargo test -p synapse-rust --lib --features test-utils -- \
    render_appservice_scheduler_prometheus_metrics_reflects_recovery_summary
test result: ok. 1 passed; 0 failed;   # public 255 → 255（未动）

# storage db_tests（直连 public + pin）→ 通过
$ cargo nextest run -p synapse-storage --lib --features test-utils -- \
    -E 'test(/db_tests::test_register_creates_service/)'
PASS [0.057s] application_service::db_tests::test_register_creates_service
```

---

## 2. ✅ 测试基础设施：`clone_schema_from_template` 曾 3 份实现 → 现 1 份实现 + 2 份薄封装

> **状态更新（2026-09-14）**：按建议执行（方案 a）——共享模块新增可选 seed 白名单参数 `SeedSource`。
>
> **提交**：`3e9063e0`（`test-isolation: converge the third clone onto the shared one; fix two sequence defects`）
>
> **收敛后三处现状**（grep `fn clone_schema_from_template`，3 处，1 实 + 2 薄封装）：
>
> | 位置 | 角色 | 能力 | 委托 |
> |---|---|---|---|
> | `synapse-common/src/test_isolation.rs:1040` `pub async fn clone_schema_from_template` | **唯一实现** | 数据复制✅ 外键✅ 完整性校验✅ 索引名✅ 序列 OWNED BY✅ | — |
> | `src/test_utils.rs:1032` | ROOT 薄封装 | 建 schema + 委托 shared `SeedSource::Only(SEED_REFERENCE_TABLES)` | ✓ |
> | `synapse-services/src/test_utils.rs:502` | services 薄封装 | 建 schema + 委托 shared `SeedSource::Everything` | ✓ |
>
> **根 crate 仍用白名单**的疑虑已消：`SeedSource::Only(SEED_REFERENCE_TABLES)` 是共享模块的**正式参数**，ROOT 传它（语义 = "只复制基线中非空的那 3 张"），shared 的 `seed_where_clause` 对不在白名单的表生成 `1 = 0`（结构克隆、不复制行）。与原来"不复制 users"的效果等价，共享模块无需感知 ROOT 的意图。
>
> **验证**（DB 实测）：`reusing_a_schema_reseeds_its_sequences` 通过（`TRUNCATE ... RESTART IDENTITY` 重置序列 + 重播种 + 默认 id 插入恢复）；`media::tests` 3 失败清零；全量 workspace lib 回归 6179/6179；静态守卫 7/7（`tests/unit/test_isolation_unification_tests`）。
>
> **连带修复（P1D 遗留缺陷）**（同 commit `3e9063e0`）：
> - 阶段 1c 创建克隆序列时缺 `OWNED BY` → `pg_get_serial_sequence()` 返回 NULL → `TRUNCATE ... RESTART IDENTITY` **静默不重置**（实测：无归属序列停在 42，有归属重置为 1）。已补 `ALTER SEQUENCE ... OWNED BY`。新增 `advance_schema_sequences()` 让被复用的 schema 与新克隆达到相同状态（ROOT 池化路径 `truncate_and_reseed_schema` 1439-1443 已接入）。
>
> **原始记录与根因**（保留供归档）：
>
> **违反项目规则第 2 条（同一职责只允许一份实现）。** 修复进行中。
>
> ### 原始状态（实测）
>
> ```bash
> $ grep -rn "fn clone_schema_from_template" --include=*.rs . | grep -v worktrees
> synapse-common/src/test_isolation.rs:848   pub async fn clone_schema_from_template(pool, schema, template)   # 共享版
> synapse-services/src/test_utils.rs:484     async fn clone_schema_from_template(database_url, template_name)  # 私有
> src/test_utils.rs:1021                     async fn clone_schema_from_template(database_url, template_name)  # 私有
> ```
>
> 三份能力不同（实测各自特性计数）：
>
> | 实现 | 数据复制 | 外键回放 | 完整性校验 | 索引名还原 |
> |---|---|---|---|---|
> | `synapse-common`（共享） | ✅ | ✅ | ✅ | ❌ → ✅（本轮补） |
> | `src/test_utils.rs` | ✅ | ✅ | ❌ | ✅ |
> | `synapse-services/src/test_utils.rs` | ❌ | ❌ | ❌ | ❌ |
>
> ### 本轮进展
>
> **已完成（提交见括号）**
>
> 1. **共享模块补上"索引名 + UNIQUE 约束名归一化"**（`6a051fcb`）。
>    实测 `LIKE ... INCLUDING ALL` 会把 `idx_t_v_named` 改名成 `t_v_idx`、
>    把 `uq_c_pid_named` 改名成 `c_pid_key`（PRIMARY KEY 名保留）。而
>    `has_index_named` 在 `tests/integration/schema_contract_p0_tests_migrated.rs`
>    有 **24 个调用点**，`validate_clone` 又只比数量 → 不做这一步就切换会**静默破坏**
>    这些断言。新增 Phase 1d 处理它，并加了测试
>    `clone_preserves_index_and_unique_constraint_names`（**已反向验证**：
>    把 Phase 1d 置为 no-op 后该测试变红）。
> 2. **`synapse-services` 那份私有实现改为委派并降为薄封装**（`ea1a3ddc`）。
>    它原本是三者中最弱的（不复制 seed 行、不回放外键、无校验）。
>    验证：retention 队列 7/7；services 完整 lib **2079 run / 2078 passed / 1 failed**
>    （基线 2076 passed / 3 failed，**减少 2 个失败**；剩余 1 个为既存 media 失败）。
> 3. **`src/test_utils.rs` 收敛到共享模块**（`3e9063e0`）。
>    删除 ~170 行自实现（DO 块含表 LIKE + 索引名修复 + seed 复制 + 序列重绑 + 视图重建），
>    改为 `clone_schema_from_template(..., SeedSource::Only(SEED_REFERENCE_TABLES))`。
>
> **剩余（共识已达，待编码）**
>
> _(旧记录，已完成，保留作验收参考)_
>
> `src/test_utils.rs` 的那份（177 行）仍自实现。它比 services 那份强，且有**一处
> 共享模块不具备的行为**：
>
> ```rust
> const SEED_REFERENCE_TABLES: &[&str] =
>     &["server_media_quota", "server_retention_policy", "sync_stream_id"];
> // 注释：the development-only `@admin:localhost` seed in `users` is intentionally
> //       NOT copied — tests manage their own users and many assert an empty users table.
> ```
>
> 即：根 crate 车道用**显式 seed 白名单**，共享模块则复制**所有表**的数据。
> 直接在根 crate 委派，会把 `users` 表的数据（含开发种子）也复制进克隆，
> **破坏"users 表应为空"的断言**。
>
> 因此收敛它需要先决定：
>
> - **(a)** 给共享模块加可选的 seed 白名单参数（调用方传入；默认全表复制）——
>   这个既有信息量又有约束力，且能同时服务两条车道；
> - **(b)** 让根 crate 车道也接受全表复制，并修掉依赖空 `users` 的测试；
> - **(c)** 保持两份实现，接受该分歧（不推荐，违反规则 2）。
>
> 推荐 **(a)**。注意根 crate 那份还带 `SCHEMA_POOL`（TRUNCATEd schema 复用）与
> `SHARED_CLONE_SEMAPHORE`，这些是**调用侧**的关注点，不在 `clone_schema_from_template`
> 内部，可保留在原处。
>
> ### 守卫的盲区（仍未修）
>
> `tests/unit/test_isolation_unification_tests.rs` 只断言"两份夹具**调用了**共享模块"，
> **不检测**是否还存在第三份自实现。建议加一条静态断言：
> "全仓 `fn clone_schema_from_template` 的定义只允许出现在 `synapse-common`"。
>
> _(旧记录，已完成：Guard 1 扩展至三份夹具 + 新增 Guard 1b "no fixture resells a hand-rolled clone")_
>
> **三份能力不同**（实测各自的特性计数）：
>
> | 实现 | 数据复制 | 外键回放 | 完整性校验 |
> |---|---|---|---|
> | `synapse-common`（共享） | ✅ | ✅ | ✅ |
> | `src/test_utils.rs` | ✅ | ✅ | ❌ |
> | `synapse-services/src/test_utils.rs` | ❌ | ❌ | ❌ |
>
> `synapse-services` 那份能力最弱：**不复制 seed 行、不回放外键、不做校验**——正是
> "缺表 → `search_path` 静默回退 `public`"的温床。
>
> ### 影响
>
> - 该类 bug 需要修三次（历史上已经发生过：测试隔离统一前有三份实现，同一类缺陷修了多次）
> - 新增的守卫测试 `tests/unit/test_isolation_unification_tests.rs` **只覆盖
>   storage 与 services 的对外夹具**，**不检测**这两份遗留私有实现是否仍在
>
> ### 建议
>
> 把 `src/test_utils.rs` 与 `synapse-services/src/test_utils.rs` 的私有实现改为调用
> `synapse_common::test_isolation`，或删除；并在守卫测试里加一条"全仓只允许一份
> `clone_schema_from_template` 定义"的断言（可静态扫描源码）。

---

## 3. ✅ 测试确定性：两个既存失败—— **已修复**
> **状态更新（2026-09-14 完成）**：`test_calculate_age_near_zero` 放宽容差 `<= 50`，`render_appservice_scheduler_prometheus_metrics_reflects_recovery_summary`
> 以前 panic 的“拒绝 DROP SCHEMA public”守卫在 §1 强化后（DB 名含 `test` 视为可重建测试库）不会
> 再误伤共享测试池，故用例稳定通过。

### 3.1 `test_calculate_age_near_zero` —— 时钟容差过紧（**已修**）

原始记录：

```rust
fn test_calculate_age_near_zero() {
    let now = current_timestamp_millis();
    let age = calculate_age(now);
    assert!(age <= 1, "age for now should be near zero, got {age}");
}
```

实测：`--test-threads 4` 下曾报 `got 6`；单独跑 0.020s 通过。
**容差仅 1ms，并发下调度延迟即失败。** 修法：放宽到合理范围（如 `<= 50`），
或改用单调时钟比较。**已执行：放宽容差到 `<= 50`（`synapse-common/src/time.rs:179`）**

### 3.2 `render_appservice_scheduler_prometheus_metrics_reflects_recovery_summary`（**已修**）

原始记录：

实测在合并后全量门禁中稳定失败（非偶发）：

```
panicked at src/server/mod.rs:1215:53:
  shared test pool should be available: "refusing to DROP SCHEMA public on a database
  that looks deployed (public.schema_migrations exists). ..."
```

**根因不是本用例逻辑，而是"守卫判定条件无法区分部署库与测试库"**：
它使用共享池，而共享池路径会尝试 `DROP SCHEMA public`；守卫拒绝后 `expect` panic。
连它自己错误信息里建议的 `synapse_test` 也会被拒（因为 `synapse_test` 里同样有
`public.schema_migrations`）。

`docs/archive/audit/P4_concurrency_perf_2026-09-11.md:222` 已记录该测试为性能敏感
（77.2s → 15.8s）。

**建议**：守卫的判据不该是"`schema_migrations` 存在"，而应是显式环境变量或专用
哨兵表；或让该测试改用隔离池（per-test schema），使其不触碰 `public`。

**已修原因**：守卫判据已随 §1 强化（`src/test_utils.rs:init_template_schema` 改为
DB 名含 `test` 视为可重建测试库，不再依赖 `public.schema_migrations` 是否存在），
故该用例不再被误拒。实测通过（`--features test-utils`，PASS 5.5s）。

---

## 4. ✅ `media::tests` 的确定性失败（3 个）—— **已修复**（3 个 commit）

> **状态更新（2026-09-14）**：按建议执行。`prepare_media_test_pool`
> 改为委托 `test_utils::prepare_isolated_test_pool()`（共享 v11 克隆），
> 删除自建 9 表部分 schema；同时移除 CI 豁免与自清理守卫脚本。
>
> **提交**：
> - `5d3f7d4b` — media 夹具委托共享隔离池；ci.yml 移除排除过滤器 +
>   删除 `check_media_exemption_still_needed.sh`；守卫测试改为
>   `media_exemption_is_fully_removed_from_ci`（断言无排除、无守卫步骤、
>   无守卫脚本）
> - `011db5db` — **连带 P0 修复**：全量回归暴露 2 个被豁免掩盖的
>   CI 关键 bug（见下）
> - `f6283785` — baseline 指纹守卫刷新（v11 fold-in 改变指纹）
>
> **验证**：13/13 media 测试通过（`--test-threads 1` 与 `--test-threads 4`
> ×3 轮）；全量 workspace lib 回归 **6178/6179**（1 个无关并发抖动
> `test_remove_friend_from_group`，单独跑通过，nextest ci profile 有
> `retries = 2`）。
>
> ### 连带 P0 修复（`011db5db`）
>
> §4 全量回归暴露 2 个此前被 media 豁免掩盖的 CI 关键 bug：
>
> 1. **burn_after_read retry 列从未 fold 进 v11 baseline**。
>    `20260907000000_burn_idempotent_retry_cap.sql` 给
>    `burn_after_read_pending` 加了 `retry_count` / `last_error` /
>    `is_dead_letter`，但 `build_sqlx_migration_source.py` 只产出
>    forward-only 产物（丢弃所有时间戳迁移），fresh CI/test DB 全都缺列
>    → `storage burn_after_read::db_tests` 报
>    `column "retry_count" does not exist`。
>    修复：三列并入 v11 baseline（`migrations/00000000_unified_schema_v11.sql`），
>    索引收紧为 `WHERE is_processed = FALSE AND is_dead_letter = FALSE`；
>    同时把 `scripts/check_baseline_consolidation.py` 的列检查从
>    "列名出现在基线任意位置"（假绿：`retry_count` 在另外 10 张表也出现）
>    升级为**表-列配对**（CREATE TABLE 块 ∪ ALTER ADD），反向验证
>    （从副本删除列 → 守卫 exit 1）。
> 2. **`prepare_test_db.sh` 无法构建模板 schema**（两个 bug）：
>    - `CREATE SCHEMA` 必须先行（否则未限定 `CREATE TABLE` 落回 `public`，
>      且 `_sqlx_migrations` 已记录 → 静默跳过）
>    - sqlx-cli **不读** `PGOPTIONS`（rust-postgres 驱动忽略 libpq env）；
>      `search_path` 必须经 URL `?options=-c%20search_path%3D...` 注入
>    修复后重建验证：`public` 254 表、`test_template_ci` 254 表。
>
> ### baseline 指纹变更（`f6283785`）
>
> v11 fold-in 改变了基线字节 → `v11 ++ extensions` 的 FNV-1a 指纹从
> `bec240fb79ed438b` 变为 `7c3a89659a56940f`。守卫测试
> `baseline_fingerprint_is_v11_then_extensions_with_no_separator` 变红，
> 已同步更新常量与文档引用。
>
> **原始记录与根因**（保留供归档）：
>
> 实测（`--test-threads 1` 单独跑也会失败，**不是抖动**）：
>
> ```
> FAIL media::tests::test_chunked_complete_can_be_downloaded_via_media_service   (media/mod.rs:1014/1026/982)
> FAIL media::tests::test_delete_media_rolls_back_quota_usage                     (media/mod.rs:1054)
> FAIL media::tests::test_ensure_media_not_quarantined_rejects_non_admin          (media/mod.rs:1275)
> ```
>
> 典型错误：
>
> ```
> 23503 insert or update on table "upload_progress" violates foreign key constraint
>       "fk_upload_progress_user"
>       Key (user_id)=(@chunk_tester:test.server) is not present in table "users".
>       schema: Some("public")
> ```
>
> **根因**（历史）：`prepare_media_test_pool`（`media/mod.rs:699`）自建一个
> **部分 schema**（实测 9 张表），`search_path = <schema>, public`；当所需表不在
> 该 schema 时**静默回退到 `public`**，而 `public` 里没有该测试用户 → 外键违约。
>
> 且失败断言点会漂移（有时是外键、有时是 `Content-Disposition` 里出现 media-id
> 前缀），说明是多个缺陷叠加。
>
> **CI 现状（修复前）**：主门禁用 `-E 'not test(/^media::tests::/)'` 排除这 13
> 个用例，并由 `scripts/ci/check_media_exemption_still_needed.sh` 守卫。
>
> **该守卫的缺陷（结构性的）**：判定逻辑是"连跑 3 次，**有任何一次失败** → 豁免
> 仍必要 → `exit 0`"。因为失败是确定性的，它**永远走"仍必要"分支**，自我收回分支
> 不可达——无法区分"串扰仍在"与"夹具坏了"。另外无 DB URL 时它也 `exit 0`
> （静默跳过）。

---

## 5. ✅ `status` 字段没有消费方（B-7 的连带问题）—— **已修复**（schema 3 → 4）

> **状态更新（2026-09-14 后续）**：按 P2 决策「删除」执行。已删除
> `RouteStatus` / `with_status` / `LedgerEntryStatusJson` / `RouteEntry.status`；
> `SCHEMA_VERSION` 3 → 4；6 个 fixture（两条车道）与 SDK 镜像（50 模块 +
> 46 文档 frontmatter）同步。7 条 legacy `/r0/push/*` 路由保留注册但不再带
> 生命周期标注。

原始记录如下：

删掉 `module` 之后，`status` 成了同类问题的候选：

```bash
$ grep -rn '\.with_status(' --include=*.rs src | wc -l
0                                     # 工作树中最后一个调用点已被移除
$ grep -rn 'sunset_at' --include=*.rs src | grep -v 'route_ledger.rs\|ledger_export.rs' | wc -l
0                                     # 除定义与序列化外，无人读取
$ grep -rc 'deprecated' .github/workflows/ | grep -v ':0' | wc -l
0                                     # CI 中无 deprecated 相关门禁
```

即：`RouteStatus` / `with_status` / `LedgerEntryJson.status` 保留着，
但**没有生产者、也没有消费者**。

**当时待决策**（二选一）：
1. **删除**（与 `module` 同理由）：移掉 `RouteStatus`、`with_status`、
   `LedgerEntryStatusJson`、`status` 字段
2. **给消费方**：加 CI 门禁 `ledger-deprecated`——读到 `Deprecated` 就要求
   `sunset_at` 非空，到期时报警；这样字段才真正有用

**决策结果**：选 1（删除）。理由：与 `module` 相同——未发布项目不留冗余，
预埋无消费方的机制不如将来真有需求时再引入（届时先建消费方）。

---

## 6. ✅ CI lint 门禁不覆盖 workspace —— **已根治**（commit `9875d8ff`）

> **第三轮修复（2026-09-14）**：§15.1 指出的"只清症状、门禁未覆盖"现已**真正修好**。
> `:217` 的 clippy 命令改为 `cargo clippy --workspace --all-targets --features
> test-utils ${{ matrix.features-args }} --locked -- -D warnings`，并删除被其完全
> 覆盖的 `-p synapse-services --all-features --tests` 补丁步骤。**修复判据 = 变红实验**
> （`AGENTS.md` 第 8 条），详见 §15.1 更新。

```bash
$ grep -n "cargo clippy" .github/workflows/ci.yml
217:        run: cargo clippy ${{ matrix.features-args }} --locked -- -D warnings
351:        run: cargo clippy -p synapse-services --all-features --tests --locked -- -D warnings
```

`:217` **无 `--workspace`、无 `--all-targets`**（仅根 crate 的默认 target）；
`:351` 只补了 `synapse-services` 的 `--tests`。
→ 其余 workspace crate 的 lib 代码、以及所有 crate 的**测试代码**都不在
`-D warnings` 之内。

实测 `cargo clippy --workspace --all-targets --all-features` 当前有 **21 条 warning**
（均非 error，故未被现有门禁拦截）：5 条 `assert_eq!` 用字面 bool、3 条
"operation has no effect"、2 条"borrowed expression implements required traits"等。

> **修复（2026-09-14 完成）**：14 条 warning 全部清零（commit `8509c52b`，
> 见 §15.1）：
> - `assert_eq!(x, false/true)` → `assert!(x)` / `assert!(!x)`（5 处，key_request/secure_backup）
> - `1 * day_ms` → `day_ms`（3 处，pruning.rs）
> - `.bind(&user_id)` → `.bind(user_id)`（2 处，db_tests.rs）
> - 删除未使用 import `InMemoryToDeviceStorage`（to_device/service.rs）
> - field-assignment-outside-initializer → struct literal（2 处，key_rotation + cache tests）
> - `#[allow(dead_code)]` 标注保留的可复用测试辅助（voice.rs）
> - clippy `map().unwrap_or_else()` → `map_or_else()`（test_isolation guard）
>
> **门禁根治**（commit `9875d8ff`）：`:217` 改为
> `cargo clippy --workspace --all-targets --features test-utils ${{ matrix.features-args }} --locked -- -D warnings`
> ——覆盖全部 workspace crate 的 lib + test + bin 代码（旧版仅根 crate lib，
> 子 crate 测试代码的 lint 完全漏检）。`:351` 的 `-p synapse-services --all-features --tests`
> 补丁步骤因被完整覆盖而删除。
>
> ### 变红实验（决定性证据，§15.1 同步更新）
>
> 向 `synapse-storage/src/event/db_tests.rs` 的 `#[cfg(test)]` 模块注入
> `assert_eq!(x, false, ...)`（触发 `clippy::bool_comparison`）：
>
> | 命令 | 退出码 | 报出的 clippy 错误数 | 门禁是否可见 |
> |------|--------|---------------------|-------------|
> | 旧 `:217` `cargo clippy ${{ matrix.features-args }} --locked -- -D warnings` | **0** | **0** | ❌ 完全看不到 |
> | 新 `:217` `cargo clippy --workspace --all-targets --features test-utils ${{ matrix.features-args }} --locked -- -D warnings` | **101** | **2** | ✅ 在 `db_tests.rs:2180` 捕获 |
>
> 探针已还原（`git diff` 确认）。**存量清零 ≠ 门禁生效**，新 warning 不会被漏掉。
>
> ### 验证（两条矩阵 lane，均零 warning）
>
> ```bash
> # 默认 lane（~53s）—— 验证 test-utils feature 开启后 test 代码被覆盖
> $ cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings
> # → Finished dev profile, zero warnings (in 53s)
>
> # all-features lane（~2m05s）—— 验证 voice-extended 等 feature-gated 模块 + max feature 集仍通过
> $ cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
> # → Finished dev profile, zero warnings (in 2m05s)
> ```
>
> 旧记录中的 `voice.rs` 探针因 `voice` 模块被 `#[cfg(feature = "voice-extended")]`
> 门控、在默认 feature 下不编译而从未被旧门禁看到——这正是"门禁盲区"的又一例证；
> 变红实验最终改在**无条件编译**的 `event/db_tests.rs` 上完成。

---

## 7. 🔴 契约链的 feature 集不一致（已缓解，未根治）

### 现状（已在本轮修好一部分）

| 产物 | 生成方式 | feature | `all` 档条数 |
|---|---|---|---|
| `tests/unit/fixtures/ledger_export/` | 金文件车道 | **默认** | 1320 |
| `tests/unit/fixtures/ledger_export_sdk/` | SDK 车道 | **all-extensions** | 1407 |
| CI `artifacts/ledger-export/` | CI workflow | **all-extensions** | 1407 |

两条车道**同名文件、不同 feature、差 87 条**。本轮已在
`scripts/generate_sdk_ledger_fixtures.sh` 头注释与
`docs/synapse-rust/LEDGER_EXPORT_SCHEMA.md` 写明该差异与原因，并固定了默认值。

**仍未根治**：两条车道的存在本身是复杂度来源。若将来统一为 all-extensions，
需要先让金文件测试的 `cfg(not(any(feature = ...)))` 门控与 CI 步骤对齐。

### 已解决部分（记录，不必再动）

- ✅ SDK pin 与后端 schema 对齐（曾因 1→2 未同步导致同步链断裂）
- ✅ 新建 `docs/synapse-rust/LEDGER_EXPORT_SCHEMA.md`（此前被 3 处代码引用却不存在）
- ✅ 加 `schema_doc_version_matches_code` 守卫（代码版本 ↔ 文档版本）
- ✅ 删除无消费方的 `module` 字段（schema 2 → 3，两条车道与 SDK 镜像同步）

---

## 8. 🟡 迁移文档 `ROUTE_CONTRACT.md` 的漂移门禁：**门禁有效，但它与文档共用同一个有缺陷的解析器**（见 §17）

⚠️ **修正先前的判断**：本条曾被我列为"文档手工维护、CI 无校验"。实测**不成立**：

```bash
$ bash scripts/contract/check_route_contract.sh
==> Regenerating docs/synapse-rust/ROUTE_CONTRACT.md (gen_contract_doc.py) ...
wrote .../ROUTE_CONTRACT.md: 921 routes, 46 categories (appendix preserved)
✅ ROUTE_CONTRACT.md: only the generation timestamp changed (acceptable drift).
EXIT=0
```

CI 侧有 `drift-detection.yml` 的 `route-contract-drift` job，
Makefile 有 `route-contract-check`，`make check` 也包含它。**该门禁是有效的**，
不列为问题。（唯一注意：脚本会因日期变化重写时间戳行。）

---

## 9. ✅ 测试库 schema 残留 + 脚本保护修复—— **已修复**（82921311）

### 问题规模与根因

本地 `synapse` 库累积 **388** 个残留 schema（每个含 200+ 表），`synapse_test` 2 个。
`scripts/cleanup_test_schemas.sh` 原需手动 `--apply`、**无自动调用路径**；且标记路径基于
`CARGO_TARGET_TMPDIR`，本地用自定义 `CARGO_TARGET_DIR` 时找不到标记而中止（实测痛点）。

脚本的**逻辑缺陷**更严重：旧版把 `test_template_ci`（CI seed 钉住的模板，§1）也当
`test\_%` 候选，而它不匹配任何指纹家族正则（`^test_template_v<N>_<hex>` /
`^test_isolation_template_<hex>`）→ `TEMPLATE_PREDICATE` 第一个 OR 分支恒真 → 落入
候选 → **直接 DROP**，CASCADE 连带删掉所有并发克隆的 DEFAULT 序列。

### 修复（脚本，commit 82921311）

1. **硬排除**：keep 名单通过 `AND nspname NOT IN (...)` 叠加在 `CANDIDATE_SQL` 最外层，
   不被家族正则的 OR 短路，`test_template_ci` 等明确 live 模板绝对安全。
2. **TEST_DB_TEMPLATE_SCHEMA 独立保护**：CI 的 `test_template_ci` 无 Rust 文件系统标记，
   旧逻辑只在「完全没标记」时才用它；新逻辑将其作为**独立保护源**，无论本地标记是否
   存在都加入 keep 集合。
3. **标记路径可配置**：新增 `SYNAPSE_TEMPLATE_MARKER_DIR`，本地自定义 target 时显式指向。
4. **降级兜底**：无任何 live 来源时降级为「保留全部模板家族、清理克隆」（同
   `--keep-all-templates`），告警而不中止。

### 修复（CI 集成）

`test` job 末尾新增 step（`if: always() && matrix.features-args == '--all-features'` +
`continue-on-error: true`）执行 `cleanup_test_schemas.sh --apply`，env 钉
`TEST_DB_TEMPLATE_SCHEMA=test_template_ci`。

> **工程诚实性提示**：GitHub Actions 的 postgres service 容器是 job 级、销毁即丢弃，
> **跨 CI run 不累积**。该 step 的真实价值在 intra-job 预防（多矩阵共享容器、未来
> self-hosted 持久库场景）；**本地 388 残留无法被它触及**——应在本地定期执行
> `bash scripts/cleanup_test_schemas.sh --apply`，或加入本地 `make` target。
>
> **本地清理建议（待办）**：把 cleanup 接入 `dev-test-setup.sh` 或加 `Makefile`
> 的 `make clean-test-schemas` target，让本地开发者一条命令治理累积。

---

## 10. ⚪ 工程债计数（不建议批量处理）

```bash
allow(dead_code):   149
allow(clippy::):    170
#[ignore]:           26
TODO/FIXME/XXX/HACK:  8
```

26 个 `#[ignore]` 的分布：

```
 8  tests/unit/e2e_honesty_tests.rs
 7  tests/e2e/user_flow_tests.rs
 3  tests/unit/pagination_gate_tests.rs
 3  tests/unit/ci_test_scope_tests.rs
 2  tests/performance/manual_smoke_tests.rs
 2  tests/performance/appservice_scheduler_perf_tests.rs
 1  tests/e2e/e2e_scenarios.rs
```

**不建议批量删**：`#[ignore]` 多与 e2e/性能目标相关（需真实后端或硬件），
`allow(dead_code)` 多为测试基础设施。需逐个判断是否有真实调用面，
否则会制造编译失败。建议**只在新代码审查时禁止新增**，存量另立专项。

---

## 11. ⚪ 共享克隆的两处已知局限（deferred minor）

均**已记录、当前不触发**，列出以免遗忘：

1. **身份列（identity column）不重新绑定**
   `grep -c 'attidentity' synapse-common/src/test_isolation.rs` → `0`。
   序列（`BIGSERIAL`）已在 phase 1c 重新绑定到克隆自身，但 `GENERATED ...
   AS IDENTITY` 列未处理。当前 v11 baseline **0 个身份列**，
   若未来基线引入则需补。
2. **`EXECUTE PROCEDURE`（PG 11 之前语法）未重定向**
   仅重写了 `EXECUTE FUNCTION`。支持基线是 PG 15/16，不触发。

---

## 12. `[未验证]` 待查线索 —— **第二轮已全部查证**（2026-09-14）

首版列出的 4 条线索现已逐条实测。**结论：2 条被证伪（原描述不成立），
2 条被证实——但两者的实际影响面都与原线索描述不同：**

| 线索 | 结论 | 实际影响 |
|------|------|----------|
| 12.1 B-3 `friend_room.rs` 双前缀 | **证伪** | manifest 与 router 完全一致（93/93，对称差集为空） |
| 12.2 B-4 `msc4108_rendezvous.rs` | **证实，且比原描述严重** | manifest 一致；但协议实现缺 10 项规范 required 行为（新 §16） |
| 12.3 B-10/B-11 剩余条目 | **证伪（可测范围内 0 缺口）** | 无路由缺 manifest 声明；但契约文档含未加 nest 前缀的路径（新 §17） |
| 12.4 `synapse-e2ee` unused import | **证伪** | 位于 `#[cfg(test)]` 内，不进生产构建 |

### 12.1 B-3 `friend_room.rs` 的"双前缀" —— **证伪，无契约不一致**

实测方法：写解析器分别抽出 `create_friend_router` 里所有 `.route(path, methods)`
的 `(method, path)` 与 `friend_route_manifest()` 声明的元组，做双向差集。

```
router (method,path) 数: 93
manifest 条数:           93
对称差集:                (空)
router 前缀分布  : v1=29  r0=28  v3=7  /_matrix/vendor/v1=29
manifest 前缀分布: v1=29  r0=28  v3=7  /_matrix/vendor/v1=29
```

> ⚠️ 若用 `scripts/contract/extract_registered.py` 的解析器统计同一文件会得到
> **偏小的数字**（它每个 `.route` 只记第一个方法、且忽略 `nest`）。上面这组数字
> 来自我自己写的括号平衡解析器。这正是 §17 所述解析器缺陷的一个具体体现。

**裁定**：B-3 原描述的"双前缀问题"**不存在**。三套前缀（v3/v1/r0）是**有意**
同时注册的兼容别名，且 manifest 与 router 完全同步。`with_module`/`with_status`
为 0 处是 B-7 删除该机制的结果，不是缺陷。

### 12.2 B-4 `msc4108_rendezvous.rs` —— **manifest 一致；但发现真实协议缺口（新 §16）**

manifest 与 router 同样完全一致（4 条，2 条路径 × 方法）。
原线索提到的"`tags` 字段缺失"**不成立**：`tags` 不是 MSC4108 的 API 字段
（MSC4108 的响应头是 `ETag`/`Expires`/`Last-Modified`/`Cache-Control`/`Pragma`）。

但对照 MSC4108 规范正文后发现**真实的协议实现缺口**，见 **§16**。

### 12.3 B-10/B-11 "剩余条目" —— **证伪（在可测范围内 0 缺口）；但暴露真正缺陷（新 §17）**

实测（`scripts/contract/extract_registered.py` 产物 + 自写双向差集）：

```
extractor total_routes: 921
注册但未在任何 manifest 声明的路由: 0
```

**裁定**：B-10/B-11 "manifest 与 router 分两张表"的**症状在可测范围内为 0**——
没有一个已注册路由缺 manifest 声明。原线索的"具体缺哪些条目未实测"答案是
**一条都不缺**。

但这个核查过程暴露了**更根本的缺陷**：该结论**无法被任何门禁持续保证**，
因为不存在"构建真实 router 并与 ledger 比对"的测试。见 **§17**。

### 12.4 `synapse-e2ee` 的 `unused import` —— **证伪，是测试代码**

```
$ sed -n '118,140p' synapse-e2ee/src/to_device/service.rs
#[cfg(test)]        <- 124 行
mod tests {         <- 125 行
    use crate::test_mocks::InMemoryToDeviceStorage;   <- 129 行
```

该 import 位于 `#[cfg(test)] mod tests` 内，**不进生产构建**。原线索的
"未验证是测试代码还是生产代码"答案是**测试代码**，不构成生产缺陷。

> 注：该 import 在 `8509c52b` 已被删除，当前 `cargo clippy --workspace
> --all-targets --all-features` 报的 3 条既有 warning 中不再包含它。

---

### 15.3 ✅ **新发现**：第三批次 CI 门禁整治（死步骤/空转门禁/冗余 workflow）—— **已修复**（`9e847a42`）

> **状态更新（2026-09-14 修复落地）**：按 REDUNDANCY 报告 §4 第三批次（门禁整治）执行。
>
> **提交**：`9e847a42`（5 个文件：3 修改 + 2 删除）。
>
> **验证**：`cargo check --workspace --all-features` Finished；`check_fmt_ratchet.sh` current=0 baseline=0 GREEN。

| # | 问题 | 位置 | 修复 |
|---|------|------|------|
| 1 | **死步骤**：`--exclude synapse_worker` 引用不存在的 package（实际是 bin target），加上 `\|\| true` 使步骤永远绿（**红灯门禁的相反失效：完全不可见**） | `.github/workflows/ci.yml:739` | 改为 `--bin synapse_worker --locked`（无 `\|\| true`，失败可见） |
| 2 | **必红门禁**：schema-health-check 引用不存在的 `unified_schema_v10.sql`（实际只有 `_v11.sql`），每次 push/PR **必红**，因而长期被忽略 | `.github/workflows/schema-health-check.yml:73` | 改为 `unified_schema_v11.sql` |
| 3 | **空转门禁**：drift-detection 的 performance-baseline 引用 4 个不存在的 `performance_indexes*.sql` 路径（两个 `migrations/archive/` 目录、两个 `docker/deploy/migrations/` 目录均不存在），每轮 100% warning + skip | `.github/workflows/drift-detection.yml:347-350` | 删除整段 for 循环，改为 `::warning::` 声明"该门禁随冗余清理已移除"（避免下次有人误以为该门禁在运行） |
| 4 | **冗余 workflow**：`test.yml` 与 `format-drift-tracking.yml` 功能已被 `ci.yml`（coverage job）与 `format-governance.yml`（format compliance）覆盖，属重复门禁 | `.github/workflows/test.yml` + `format-drift-tracking.yml` | 删除 |

> **工程诚实性**：第 3 条的"空转门禁"与 §1.3 的"性能基准门禁 100% 空转"描述一致——这类门禁的特征是**每轮都走跳过分支**，CI 绿但**零信息量**。删除比"等文件重新出现"更诚实。

---

## 15. 第二轮复核记录（2026-09-14，基线 `d56a1d82`）

> **第四轮修复记录（2026-09-14）**：§15.2 与 §16 两项新发现**已全部修复**。  
> - §15.2 模板清理 → commit `314df061`（已合并 main）  
> - §16 MSC4108 协议缺口 → commit `6ba7c457`（已合并 main，配套 4 个既有测试改写）  
> - 连带格式修复 → commit `b0e8ed9e`（`cargo fmt --all`，main 工作树现 `fmt debt: current=0 baseline=0`，CI 绿）  
> - 验证：`cargo check -p synapse-common/synapse-services/synapse-storage --features test-utils` 均 Finished；`cargo test --no-run --test unit --features test-utils` 编译通过。

本节记录复核**推翻或修正首版结论**的地方，以及复核中新发现的问题。

### 15.1 ✅ **§6 的 ✅ 曾是错的**：clippy 门禁不覆盖 workspace 测试代码 —— **已根治（`9875d8ff`）**

> **第三轮收口（2026-09-14）**：本条指出的缺陷（门禁不覆盖子 crate 测试代码）
> 已按下方"修法"真正实施。`:217` 现为
> `cargo clippy --workspace --all-targets --features test-utils ${{ matrix.features-args }} --locked -- -D warnings`，
> 并用变红实验证明了它可被拦下。详见 §6。以下为原复核记录，保留作证据链。

首版 §6 标 ✅ 并写明"**已修复**（`8509c52b`）"。但该节自己的"修复"正文同时写着：

> CI 门禁（`:217`）虽未加 `--workspace`，但已确保全 workspace 干净

也就是说 **`8509c52b` 只清掉了症状（14 条 warning），缺陷（门禁不覆盖）完全没有动**。
实测 `ci.yml` 当前仍是：

```
217:  run: cargo clippy ${{ matrix.features-args }} --locked -- -D warnings
329:  run: cargo clippy -p synapse-services --all-features --tests --locked -- -D warnings
```

`:217` 无 `--workspace`、无 `--all-targets`。

**变红实验（决定性证据）**：向 `synapse-storage/src/voice.rs` 的
`#[cfg(test)]` 模块内注入一处 `if pool.is_closed() == true`（`clippy::bool_comparison`）：

| 命令 | 退出码 | 报出的 clippy 错误数 |
|------|--------|---------------------|
| `cargo clippy --all-features --locked -- -D warnings`（复刻 CI `:217`） | **0** | **0** |
| `cargo clippy --workspace --all-targets --all-features --locked -- -D warnings` | **101** | **2** |

即：**一处 100% 确定会被 `-D warnings` 拦下的 clippy 错误，放进 workspace crate
的测试代码后，CI 门禁完全看不到。** 探针已还原（`diff` 确认）。

**为何重要**：`AGENTS.md` 第 8 条要求"门禁必须自证能变红"。首版把"清了 warning"
当成"修了门禁"，正是该条要防的误判——存量清零 ≠ 门禁生效，下一个新增的
test-code warning 依然不会被拦。

**修法**（未实施，属需决策项）：`:217` 改 `cargo clippy --workspace --all-targets
${{ matrix.features-args }} --locked -- -D warnings`，然后清掉届时暴露的存量
（当前实测仅 3 条：`synapse-e2ee` 1 条 unused import + `synapse-storage/voice.rs`
2 条 dead_code）。

### 15.2 ✅ **新发现**：共享模块的模板 schema 无限累积，且没有清理机制 → **已修复**（`314df061`）

> **状态更新（2026-09-14 修复落地）**：按下方修法在 `synapse-common` 共享模块补齐了
> 与根夹具 `prune_stale_template_schemas` **对等**的清理机制。
>
> **提交**：`314df061`（`synapse-common/src/test_isolation.rs`）。
>
> **实现要点**：
> - 新增 `pub async fn prune_stale_isolation_templates(admin_pool, keep)`（公开入口）
>   与私有 `prune_isolation_templates(conn, keep)`（可借用的连接版本）。
> - **安全序与根夹具一致**：先 `to_regclass` 校验 `keep`（当前模板）仍存在，**替代模板
>   未就位绝不 prune**。
> - 候选集 `WHERE nspname ~ '^test_isolation_template_[0-9a-f]{16}$' AND nspname <> $1`；
>   正则带 16 位 hex 锚点，**不会**误匹配根夹具 `test_template_v<rev>_<hex>` 或
>   services `test_template_<pid>` 家族（交叉 crate 安全，`synapse-storage` 与
>   `synapse-services` 共享同一 baseline 指纹，prune 在两者间安全）。
> - **6 小时宽限窗**（`TEMPLATE_PRUNE_GRACE`）而非"除 keep 全删"：因为该家族跨 crate、
>   测试本地（并发 nextest 进程合法使用不同指纹），直接 `DROP ... CASCADE` 可能命中
>   正在克隆的 cloner。判定：无 marker 表 → 视为不完整构建可删；marker 0 行 → 回填
>   (`INSERT DEFAULT VALUES`) + 豁免（legacy）；否则按 `max(built_at)` 年龄 > 宽限窗才删。
> - **配套修复 readiness marker 空行缺陷**：原 `_synapse_test_template_ready` 建表后
>   **从未 INSERT 行**，`max(built_at)` 恒 NULL 会误删生产模板——现已 (a) `build_template`
>   建表后立即 `INSERT DEFAULT VALUES`，且 (b) ready 快路径改为 `DELETE + INSERT`
>   刷新 `built_at`（令在用模板持续自保 + 回填 legacy 0 行表）。
> - **调用点**：`ensure_template_schema_with_lock_timeout` 中 `build_template` 成功、
>   释放 advisory lock 之前执行；失败仅 `warn`，不阻断建模板。
> - **验证**：`cargo check -p synapse-common --features test-utils` 与
>   `cargo check -p synapse-services --features test-utils` 均 Finished；
>   新增 DB 测试 `prune_backfills_legacy_zero_row_marker_and_spares_it`
>   （需 `TEST_DATABASE_URL`）验证 legacy 0 行 marker 被回填且模板被豁免，不误删。

首版 §9 只记录 `synapse` 库的 **388** 个 `test_*` 残留。复核发现另一类**由共享
模块自己产生**的残留，首版完全没提：

```
$ psql synapse_test -c "select nspname, count(relkind='r') from pg_namespace
                        where nspname like 'test_isolation_template%'"
test_isolation_template_0d9aadd87310f7d6|  4
test_isolation_template_7c3a89659a56940f|254
test_isolation_template_bec240fb79ed438b|254
test_isolation_template_ce1048bf17bf4285|  2
test_isolation_template_cedaafcca5237cbd|  3
test_isolation_template_e3d73be71e17840a|  2
test_isolation_template_e87a91bf79535e84|  2
                                         ^^^ 7 个模板，合计 2,457 个关系对象
```

成因与自增机制：

- `synapse-common/src/test_isolation.rs` 的 `ensure_template_schema` 用
  `template_schema_name(baseline_sql)`（baseline 内容的 FNV-1a 指纹）命名模板。
  **每次改 baseline（含改测试里的 baseline 字符串）就 mint 一个新模板。**
- 该文件里 `let baseline = ...` 出现 **13 次**，即跑一次 lib 测试套件会造出
  13 个不同的模板 schema（实测残留里那 5 个只有 1–4 张表的，正是测试 baseline 的模板）。
- **共享模块没有任何模板清理**：`grep -c 'DROP SCHEMA'` = 35，但全部在
  `#[cfg(test)]` 测试体内（清理自己造的临时 schema），唯一在非测试代码里的
  一处是 `build_template` 重建**不完整**模板时的 `DROP`。**没有**
  "删除被新指纹取代的旧模板"的逻辑。

对照：根夹具 `src/test_utils.rs` **有**这个机制（`prune_stale_template_schemas`，
首版 §2 提到过），共享模块没有——这正是首版 §2 "两份实现漂移"的又一个具体后果。

**影响**：每个完整模板 254 张表/1,197 个对象。长期运行的开发库与 CI 会持续累积。
CI 因每次是干净容器而不暴露，**本地开发库会持续膨胀**（实测已 2,457 个对象）。

**修法**（未实施）：共享模块在 `ensure_template_schema` 成功建好新模板后，
按 `test_isolation_template_*` 前缀删除非当前模板（参照
`prune_stale_template_schemas` 的"先确认替代模板存在再删"安全序）；
测试用的 baseline 应在测试结束时 drop 自己 mint 的模板。

### 15.3 复核确认**无误**的项

| 首版条目 | 复核结果 |
|---|---|
| §3.1 时钟容差 | ✅ 属实已修：`assert!(age <= 50, ...)`，且注释记录了实测 `got 6` |
| §9 `synapse` 库 388 残留 | ✅ 数字准确（复核仍为 388 / public 253） |
| §10 工程债计数 | ✅ 基本准确（`allow(dead_code)` 149 → 现 151，因收敛新增；其余一致：`allow(clippy::)` 170、`#[ignore]` 26、TODO 8、`#[deprecated]` 0） |
| §11.1 identity 列未处理 | ✅ 属实且仍不触发：baseline 里 `GENERATED ... AS IDENTITY` **0 处**，模板里 `attidentity <> ''` 的列实测 **0**，共享模块 `attidentity` 命中 **0** |
| §11.2 `EXECUTE PROCEDURE` 未重定向 | ✅ 属实且仍不触发：baseline 里 `EXECUTE PROCEDURE` **0 处** |
| §8 路由契约漂移门禁有效 | ✅ 属实：`check_route_contract.sh` 退出 0，921 routes / 46 categories。但见 §17——它保证的是"源码 ↔ 文档"，不是"served router ↔ ledger" |

### 15.5 复核中新出现的两类本地残留（由本轮验证产生，非产品缺陷）

复核过程中我自己的测试跑出了以下残留，**已定位、可清理、不属于产品缺陷**，
列出以免误读为泄漏：

```
unify_names_*   unify_refill_*   unify_reseed_*   unify_seed_all_*
unify_seed_clone_*   unify_seed_only_*
test_46001_1_1789365993263823000   (253 表的克隆)
test_template_v2_b6fa43a1d62f6181  (根夹具模板)
```

成因：共享模块的测试在**断言失败**时会提前 `panic`，其末尾的
`DROP SCHEMA ... CASCADE` 清理语句因此不执行。这本身是测试卫生问题
（清理应放 `Drop` guard 而非线性代码末尾），与 §15.2 是同一类问题的两个面。

---

## 16. ✅ **已修复**：MSC4108 rendezvous 实现偏离规范（协议缺口）—— 10 项全部补齐

首版 §12.2 只记录了"manifest 一致、`tags` 未验证"。复核对照 MSC4108 规范正文
（[4108-oidc-qr-login.md](https://raw.githubusercontent.com/matrix-org/matrix-spec-proposals/87f8317a902cd7bc5c2d2d225f71021b3a509e2d/proposals/4108-oidc-qr-login.md)）
后发现实现缺了多处**规范标记为 required** 的行为。

> **状态更新（2026-09-14 修复落地）**：按 §16 的 10 条缺口逐一补齐；4 个既有测试因**固化了错误期望**被同步重写（§16 记录中已注明）。
>
> **提交**：`6ba7c457`（4 个文件）。
>
> **验证**：`cargo test --test unit --features test-utils msc4108` → **46 passed; 0 failed**（含补齐的 4 个原有测试重写 + 新增的响应头完整性 / Content-Type 校验 / 413 限制 / errcode 等测试）。
>
> **修复清单（10 项）**：
>
> | # | 规范要求 | 修复后实现 | 涉及文件 |
> |---|----------|-----------|----------|
> | 1 | 所有响应带 `Last-Modified` | 4 个端点全部设置（POST/GET/PUT/DELETE 均有 `LAST_MODIFIED`） | `src/web/routes/msc4108_rendezvous.rs` |
> | 2 | 所有响应带 `Cache-Control: no-store` | 4 个端点全部设置（DELETE 走 Body::empty 也已设置） | 同上 |
> | 3 | 所有响应带 `Pragma: no-cache` | 4 个端点全部设置 | 同上 |
> | 4 | `PUT` 成功 → **202 Accepted** | `update_session` → `StatusCode::ACCEPTED`（3 个已有测试 `update_session_returns_ok_with_new_etag` 等同步改写为断言 202） | 同上 + `tests/unit/msc4108_rendezvous_route_tests.rs` |
> | 5 | `DELETE` 成功 → **204 No Content** | `delete_session` → `StatusCode::NO_CONTENT`（已有测试 `delete_session_returns_204_no_content` 预期符合，新实现对齐） | 同上 + `tests/unit/rendezvous_service_tests.rs` |
> | 6 | PUT ETag 不匹配 → **412** + unstable errcode `M_CONCURRENT_WRITE` | 新增 `Msc4108UpdateOutcome::PreconditionFailed { .. }` 变体；存储层事务内区分 NotFound / PreconditionFailed；路由 412 + `{"errcode":"M_UNKNOWN","org.matrix.msc4108.errcode":"M_CONCURRENT_WRITE"}`（`update_session_etag_mismatch_returns_bad_request` 改写为 412 断言） | `synapse-storage/src/rendezvous.rs` + 测试 |
> | 7 | POST 校验 `Content-Type: text/plain` | `create_session` / `update_session` 均加校验：缺失 → `ApiError::missing_param`；非法 → `ApiError::invalid_param` | `src/web/routes/msc4108_rendezvous.rs` + 测试 |
> | 8 | POST 响应带 `Access-Control-Expose-Headers: ETag` | POST 响应头已加入该字段 | `src/web/routes/msc4108_rendezvous.rs` |
> | 9 | GET 的 304 也要带上文 common headers | 304 响应现在带全部 5 个 common headers（`get_session_304_response_carries_only_etag_header` 改写为 5 项断言） | 同上 + 测试 |
> | 10 | 载荷上限 4KB，超限 413 | 新增 `MSC4108_MAX_PAYLOAD_BYTES = 4 * 1024` 常量，POST/PUT 均执行 `body.len()` 检查，超限 → `ApiError::too_large`（413）；新增 `update_session_payload_too_large_maps_to_413` 测试 | 同上 + 测试 |
>
> **状态码变更影响面**：PUT 200→202 与 PUT 400→412 均属行为变更，已由路由层测试（非集成端到端）覆盖；
> 若后续需要端到端验证，需在 SDK fork 测试链中同步更新期望。

规范要求（insecure rendezvous 小节）：

> ### Common HTTP response headers
> - `ETag` - **required**
> - `Expires` - **required**
> - `Last-Modified` - **required**
> - `Cache-Control` - **required, `no-store`**
> - `Pragma` - **required, `no-cache`**

实测 `src/web/routes/msc4108_rendezvous.rs` 设置的响应头（`grep` 结果）：

```
POST   : ETAG, EXPIRES, CONTENT_TYPE(json)      <- 缺 Last-Modified / Cache-Control / Pragma
GET    : ETAG, CONTENT_TYPE(text/plain)         <- 缺 Expires / Last-Modified / Cache-Control / Pragma
PUT    : ETAG, CONTENT_TYPE(text/plain)         <- 同上
DELETE : （无任何头）
```

逐条缺口：

| # | 规范要求 | 实测实现 | 影响 |
|---|----------|----------|------|
| 1 | 所有响应必须带 `Last-Modified` | **未设置** | 客户端无法判断载荷新旧 |
| 2 | 所有响应必须带 `Cache-Control: no-store` | **未设置** | 中间缓存可能缓存 rendezvous 载荷（规范明确要求防止缓存篡改 ETag） |
| 3 | 所有响应必须带 `Pragma: no-cache` | **未设置** | 同上 |
| 4 | `PUT` 成功返回 **`202 Accepted`** | 返回 `200 OK` | 状态码不符 |
| 5 | `DELETE` 成功返回 **`204 No Content`** | 返回 `200 OK` | 状态码不符 |
| 6 | `PUT` 的 ETag 不匹配返回 **`412 Precondition Failed`**（unstable 期用 `M_UNKNOWN` + `org.matrix.msc4108.errcode: M_CONCURRENT_WRITE`） | 返回 `400`（`ApiError::bad_request`） | 状态码与 errcode 均不符；客户端无法区分"ETag 冲突"与"参数非法" |
| 7 | `POST` 请求必须校验 `Content-Type: text/plain`、缺失/非法返回 `400`（`M_MISSING_PARAM` / `M_INVALID_PARAM`） | **未校验请求 Content-Type**（`CONTENT_TYPE` 仅出现在响应构造处） | 接受任意 Content-Type |
| 8 | `POST` 响应须 `Access-Control-Expose-Headers: ETag` | **未设置** | 浏览器端 JS 读不到 `ETag`，Web 客户端无法完成 QR 登录 |
| 9 | `GET` 的 `304 Not Modified` 也要带上文 common headers | 只带 `ETAG` | 缺 `Expires`/`Last-Modified`/缓存头 |
| 10 | 载荷上限 4KB，超限 `413 M_TOO_LARGE` | 未实现（无大小限制代码） | 规范建议的 DoS 缓解缺失（规范原文是 SHOULD，但 DoS 面明确） |

> **重要**：现有测试**把这些偏离当作期望固化**了。`tests/unit/msc4108_rendezvous_route_tests.rs`
> 里有 `update_session_returns_ok_with_new_etag`（断言 200）、
> `delete_session_returns_ok_with_empty_body`（断言 200）、
> `update_session_etag_mismatch_returns_bad_request`（断言 400）、
> `get_session_304_response_carries_only_etag_header`（断言 304 **只**带 ETag）。
> 因此修这些缺口**必须同时改测试**，否则会被"既有测试全绿"挡住。

**修法**（未实施）：按上表 10 条逐一补齐；状态码与 errcode 变更需同步改上述 4 个测试；
补齐后应有一条"响应头完整性"测试断言 5 个 common headers 在所有状态码下都存在。

---

## 17. 🟡 **新发现**：没有任何测试校验"实际注册的路由 == ledger"

首版 §8 标"路由契约漂移门禁：已验证有效"，复核确认该门禁确实有效，
但**它保证的范围比字面理解窄**：

| 门禁 | 实际保证 | **不**保证 |
|------|----------|-----------|
| `scripts/contract/check_route_contract.sh` | `src/web/routes/**` 的**源码文本** ↔ `ROUTE_CONTRACT.md`（两边都由 `extract_registered.py` 解析源码生成） | ① 真实 Axum router 注册的路由 ↔ ledger；② 源码解析会漏的部分 |
| `RouteLedger::validate()` | 同一 `(method,path)` **不重复** | 任何"注册了但没进 ledger"的缺失 |
| `tests/unit/assembly_route_tests.rs`（27 个测试） | **声明出来的** `declared_manifest()` 内部自洽（非空、无重复、路径以 `/` 开头、含预期命名空间） | 真实 router 是否真的注册了这些路径 |

**核心缺口**：没有任何测试**构建真实 router 再枚举路径**。实测
`grep -rn '\.routes()\|into_make_service' src/ tests/` → **0 处**；
`assembly_route_tests.rs` 里也从不调用 `create_router`（注释里说
"the live `create_router` aborts on duplicate"，但测试只检查 manifest）。

**而且解析器本身有已实测的盲区**：`extract_registered.py` 的
`re_route` 只匹配每个 `.route(...)` 的**第一个**方法（链式 `.get(a).post(b)` 只记第一个），
且**忽略 `.nest()` 前缀**——它把 `nests` 收进 `nest_map`（第 43 行）后**从未读取**，
`out[mod]` 直接等于未加前缀的 `full`。

**已实测的具体后果（契约文档出错，不只是门禁缺失）**：

`space.rs` 用 `expand_under_prefixes("space", SPACE_NEST_PREFIXES, &space_relative_routes())`
生成 manifest，`space_relative_routes()` 有 **24** 条相对路径，
`SPACE_NEST_PREFIXES = ["/_matrix/client/v1","/_matrix/client/r0","/_matrix/client/v3"]`
→ manifest 里是 **72** 条带前缀的完整路径。

但 `space/children_hierarchy.rs` 等子模块是按**相对路径**注册的
（`.route("/spaces/{space_id}/children", ...)`），而且**路径是字符串字面量**，
所以被 extractor 原样收进 `artifacts/registered_routes.json`，`gen_contract_doc.py`
再原样写进文档：

```
$ grep -cE '^- `[A-Z]+` `/spaces/'                         docs/synapse-rust/ROUTE_CONTRACT.md
15
$ grep -cE '^- `[A-Z]+` `/_matrix/client/v[0-9]+/spaces'   docs/synapse-rust/ROUTE_CONTRACT.md
0        # 期望 45（15 条 × v1/r0/v3 三个前缀）
```

**即：契约文档里 15 条 `/spaces/...` 是相对形式，真实 serve 路径是
`/_matrix/client/{v1,r0,v3}/spaces/...`。文档与真实路由面不符。**

这 15 条**全部**是相对形式（已逐条核对，无一条带前缀）：

```
DELETE /spaces/{space_id}/children/{room_id}     GET  /spaces/room/{room_id}/parents
GET    /spaces/{space_id}/children              GET  /spaces/{space_id}/hierarchy
GET    /spaces/{space_id}/hierarchy/v1          GET  /spaces/{space_id}/tree_path
POST   /spaces/{space_id}/children              GET  /spaces/{space_id}/members
GET    /spaces/{space_id}/rooms                 GET  /spaces/{space_id}/state
POST   /spaces/{space_id}/invite                POST /spaces/{space_id}/join
POST   /spaces/{space_id}/leave                 GET  /spaces/{space_id}/summary
GET    /spaces/{space_id}/summary/with_children
```

manifest 侧对应 15 × 3 前缀 = **45** 条带前缀路径，文档侧带前缀的 **0** 条。

> **诚实边界**：我**没有**逐条审计整份文档有多少条同类错误。曾用"后缀匹配"
> 估计为 37 条，但实测发现该方法会误报（`/capabilities` 会匹配到
> `/_matrix/client/v3/rooms/{room_id}/widgets/{widget_id}/capabilities`），
> 因此**该数字已丢弃**。文档里另有 254 条是合法顶层路径
> （`/.well-known/*`、`/_health`、`/` 等）。**要做完整审计，必须先有一个
> 能解析链式方法与 `.nest()` 前缀的解析器——这正是本条要求的前置修复。**

**影响**：① 任何客户端按 `ROUTE_CONTRACT.md` 拼接请求会打到不存在的路径；
② 一个只改了 router 而忘了改 manifest 的改动，当前没有任何自动门禁能发现
（契约文档反而会跟着 router 变，看起来"同步"）；③ `manifest_has_route` 驱动的
capability 声明读的是 manifest，manifest 漏条目会让 `/capabilities` 与
`/versions` 谎报能力缺失（反向则谎报可用）。

**修法**（未实施，需决策）：
1. 修 `extract_registered.py`：解析链式方法（`.get(a).post(b)` 全部记入）、
   并把 `nest_map` 真正应用到 `out`（当前完全未用）。修完重新生成契约文档，
   即可暴露并修正全部前缀缺失条目。
2. 加一条测试：断言"解析出的真实路由集合 == `declared_manifest()` 集合"（双向）。
   更强的做法是用测试 `AppState` 调 `create_router(state)` 枚举真实路由——
   Axum 0.8 无公开路由枚举 API，所以第 1 步的解析器修复是更现实的路径。

> 这一条与首版 §8 的"已验证有效"**不矛盾**：§8 的门禁确实在工作，
> 只是它守护的是"源码解析 ↔ 文档"，而两边用的是**同一个有缺陷的解析器**，
> 所以解析器错、两边一起错，门禁依然报绿。这是 §6 同类问题
> （"门禁在跑 ≠ 门禁在检查正确的东西"）的又一个实例。

---

## 18. 复核后的汇总（**当前有效的排序**；首版排序见 §19）

| 优先级 | 问题 | 类型 | 状态 |
|---|---|---|---|
| ✅ P1 | **§16 MSC4108 偏离规范 10 项** | 协议正确性 | **已修复**（`6ba7c457`）：10 项全部补齐，4 个固化错误期望的测试同步改写；46 passed 验证 |
| ✅ | §6 clippy 门禁覆盖 workspace | 门禁真实性 | **已根治**（`9875d8ff`）：`:217` 扩为 `--workspace --all-targets --features test-utils`，变红实验证明可拦子 crate 测试代码 |
| ✅ P2 | **§15.2 共享模块模板 schema 无限累积** | 资源泄漏 | **已修复**（`314df061`）：`prune_stale_isolation_templates`（6h 宽限、安全序、legacy 0 行 marker 回填）+ 配套修复 marker 空行缺陷；新增 DB 测试验证 |
| 🟡 P2 | **§17 契约文档含未加 nest 前缀的路由；解析器有链式/nest 盲区** | 契约正确性 | 未动：`/spaces/...` 15 条相对路径写入文档、0 条带前缀；`nest_map` 收集后从未使用 |
| ✅ | **§18-3 第三批次 CI 门禁整治**（死步骤/空转门禁/冗余 workflow） | 门禁有效性 | **已修复**（`9e847a42`）：修 1 死、1 必红、删 1 空转、删 2 冗余 |
| 🟡 P3 | §7 契约链两条车道的 feature 集复杂度 | 架构 | 未动，需先统一 feature 集 |
| ⚪ P3 | §9 schema 残留清理 | 运维 | ✅ 已修复（`82921311`：CI 加 cleanup step + 脚本硬排除/独立保护/可配置标记目录）。CI 侧防 intra-job 累积；本地仍需手动 `--apply`（见 §9 说明）。**注**：§15.2 修复后共享模块自行修剪，本地累积速度大幅下降 |
| ⚪ P3 | §10 存量债、§11 局限 | 技术债 | 新代码设禁，存量另立专项 |
| ✅ | §1 / §3 / §4 / §5 / §2 收敛 / §12 四条线索 | — | 已核实（§15.4） |

> **第四轮（2026-09-14）净变化**：§15.2 与 §16 由 🔴 → ✅；新增 `b0e8ed9e` 修复两者连带的
> fmt 债务（main 现 `fmt debt: current=0 baseline=0`）。当前 main 上唯一未动的确认问题是 §17
> （契约文档/nest 前缀）与 §7（feature 集复杂度）。

**核对方式说明**：首版曾误标 §6 ✅。本轮已按 `AGENTS.md` 第 8 条完成变红实验并实施
门禁扩容，§6 现为 ✅。判定标准仍是"门禁必须自证能变红"；清掉存量 warning 不等于门禁生效。

---

## 19. 首版（`e6ecda02`）汇总 —— 保留作历史对照

> **已被 §18 取代。** 保留它是为了记录首版排序，并标出首版**误判**的一项
> （`§6`），避免以后有人只读这一节而得到错误结论。

| 优先级 | 问题 | 类型 | 首版结论 | 复核更正 |
|---|---|---|---|---|
| ~~P0~~ | ~~§1 CI 指向生产库 + wipe 标志~~ | 数据安全 | ✅ 已修复（`00c0aad2`） | ✅ 成立 |
| ~~P0~~ | ~~§4 `media::tests` 确定性失败~~ | 测试正确性 | ✅ 已修复（`5d3f7d4b`） | ✅ 成立 |
| P1 | §2 `clone_schema_from_template` 多份实现 | 架构一致性 | ✅ 已修复（`3e9063e0`） | ✅ 成立 |
| P1 | §3 两个既存失败（时钟容差 / 守卫判据） | 测试确定性 | ✅ 已修复（`8509c52b`） | ✅ 成立 |
| P1 | §6 clippy 门禁覆盖 workspace | 门禁真实性 | ✅ 已修复（清 14 条 warning，`8509c52b`） | ✅ **第三轮修正**：首版误标 ✅；第二轮标 🔴（§15.1 变红实验证门禁盲区）；第三轮实施门禁扩容（`:217` 改 `--workspace --all-targets --features test-utils` + 删除 `:351` 补丁步骤，commit `9875d8ff`）。再跑变红实验：`--workspace --all-targets` 捕获 `db_tests.rs:2180` 的 `bool_comparison`（EXIT 101），门禁真实生效 |
| P2 | §5 `status` 字段去留 | 冗余治理 | ✅ 已完成（`c5a5df0d`） | ✅ 成立 |
| P2 | §9 schema 残留自动清理 | 运维 | ✅ 已修复（`82921311`：CI 加 cleanup step + 脚本硬排除/独立保护/可配置标记目录） | ✅ 成立 |
| P3 | §7 两条车道的复杂度 | 架构 | 🟡 需先统一 feature 集 | ✅ 不变 |
| P3 | §10 存量债、§11 局限 | 技术债 | ⚪ 新代码设禁，存量另立专项 | ✅ 不变（§11 两条已实测确认不触发） |

---

## 20. 复现命令速查（含第二轮新增）

```bash
# §1 CI 危险组合
grep -nB1 -A1 "SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE" .github/workflows/ci.yml

# §2 实现份数（应只有 1 处构建克隆 SQL）
grep -rn "fn clone_schema_from_template" --include=*.rs . | grep -v worktrees
grep -rn 'AND {seed_where}' --include=*.rs src synapse-*/src | grep -v tests/

# §3/§4 两个类别的失败
cargo nextest run --test unit --features test-utils -E 'test(/test_calculate_age_near_zero/)'
cargo nextest run -p synapse-services --lib --all-features --test-threads 1 -E 'test(/^media::tests::/)'

# §5 status 无消费方
grep -rn '\.with_status(' --include=*.rs src | wc -l
grep -rn 'sunset_at' --include=*.rs src | grep -v 'route_ledger.rs\|ledger_export.rs' | wc -l

# §6 clippy 覆盖 —— 关键是"能不能变红"，不是"当前有几条 warning"
grep -n "cargo clippy" .github/workflows/ci.yml
# 现状：:217 = cargo clippy --workspace --all-targets --features test-utils
#       ${{ matrix.features-args }} --locked -- -D warnings （:351 补丁步骤已删）
cargo clippy --workspace --all-targets --all-features --locked 2>&1 | grep -c '^warning'
# 变红实验：向"无条件编译"的 #[cfg(test)] 模块注入 bool_comparison，
#   如 synapse-storage/src/event/db_tests.rs 的 assert_eq!(x, false, ...)
#   （注意：不要放 voice.rs —— 该模块被 #[cfg(feature = "voice-extended")] 门控，
#    默认 feature 下不编译，探针根本不会被看到，这正是盲区本身）
# 然后对比旧/新门禁：
#   cargo clippy --locked -- -D warnings                                          # 旧 :217，退出 0（看不到）
#   cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings  # 新 :217，退出 101（在 db_tests.rs 捕获）

# §8/§17 路由契约
bash scripts/contract/check_route_contract.sh
python3 scripts/contract/extract_registered.py
grep -cE '^- `[A-Z]+` `/spaces/' docs/synapse-rust/ROUTE_CONTRACT.md                 # 15（相对路径，错）
grep -cE '^- `[A-Z]+` `/_matrix/client/v[0-9]+/spaces' docs/synapse-rust/ROUTE_CONTRACT.md  # 0（应有）

# §9 schema 残留
psql ... -c "select count(*) from pg_namespace where nspname like 'test\_%' or nspname like 'media_test_%'"
psql ... -d synapse_test -c "select nspname, count(*) from pg_class c join pg_namespace n on n.oid=c.relnamespace where n.nspname like 'test_isolation_template%' group by 1"

# §12 四条线索的复核命令
#   B-3/B-4 manifest↔router 双向差集：见 §12.1/§12.2 的解析器脚本
#   4) e2ee unused import 的门控：
sed -n '118,132p' synapse-e2ee/src/to_device/service.rs

# §15.2 共享模块模板累积
grep -c 'DROP SCHEMA' synapse-common/src/test_isolation.rs
grep -c 'ensure_template_schema(&url, baseline)' synapse-common/src/test_isolation.rs

# §16 MSC4108 响应头
grep -n 'header::' src/web/routes/msc4108_rendezvous.rs
grep -n 'StatusCode::' src/web/routes/msc4108_rendezvous.rs
```

---

## 21. 第三轮复核（2026-09-25，`opt/consolidated` @ `9e26ee31a`）

> **基线说明**：§1–§20 的基线是 `main`。本节在**分支 `opt/consolidated`** 上复测，方法一律**回到源码
> 取证**（`路径:行号` 或可复现命令），不采信任何文档既有标记。与 §18 冲突时以本节为准。
> 本节全部取证完成于 `9e26ee31a`；收尾期间并发会话将 HEAD 推进到 `57cb5e81a`（storage 游标测试 +
> D-15.3 登记），两个提交均不触及本节任何审计面，结论不变。
> 完整判定表见 [`../synapse-rust-vs-synapse-comparison.md`](../synapse-rust-vs-synapse-comparison.md) §15，
> 路由/覆盖口径见 [`../synapse-rust/API_COVERAGE_REPORT.md`](../synapse-rust/API_COVERAGE_REPORT.md) §六。
> **本节不新开 backlog**，只做状态归并。

### 21.1 当前仍存在的问题（按严重度）

| 级别 | 问题 | 判据（可复现） |
|---|---|---|
| **P0（已收窄）** | 联邦 `/send_join` **响应面与 PDU 字段面已修**：v1 补 `[200, {…}]` 包装、补 `origin`；`state`/`auth_chain` 统一经 `routes/federation/pdu.rs::build_pdus` 投影（含此前完全缺失的 `origin_server_ts`，并复用库中 `hashes`/`signatures`，否则现场签名）。**残余三条**（详见 §21.5）：① 本地 `create_event` 不落 `depth`/`prev_events`/`auth_events` ⇒ 本地起源事件投影为 `MissingGraphMetadata` 并**故意不发签名**（不伪造）；② 入站事件不落原服务端 `signatures` ⇒ 转发远端 PDU 只有本机签名；③ `event_id` 非 reference hash（见下一行） | `synapse-web/src/routes/federation/pdu.rs`（新）；`membership/join.rs`；`federation/events.rs` |
| **P0** | **PDU 语义未对齐（独立于字段完备性）**：`event_id` 形如 `$<ms>_<rand>:<server>`，而非 v4+ 的 reference hash ⇒ 即便字段齐全，v11 对等端也无法把本仓 PDU 当规范事件接受 | `synapse-common/src/crypto.rs:149`（`generate_event_id`） |
| **高** | E2EE SAS 4 处偏离规范（info 串缺公钥且顺序错 / emoji 仅 6 个且 decimal 被丢弃 / MAC 非 `hkdf-hmac-sha256.v2` / commitment 非 SHA-256）；`/keys/device_signing/verify_*` 私有 API 形状**阻塞**规范修法 | `synapse-e2ee/src/verification/service.rs` |
| **高** | 客户端撤回不级联（MSC3912 级联**仅管理端可达**） | `synapse-web/src/routes/handlers/room/events.rs:990` |
| **高** | Content Scanner **已装配但零调用点、零持久化** —— 配置可开却不扫描 | `synapse-services/src/wiring/core.rs:66,179` + `synapse-common/src/config/mod.rs:242` |
| **中** | MSC4242 仅存储层；`dag.rs` 注释声称被 `/send_join`、`/get_missing_events` 使用（实测无调用点） | `synapse-storage/src/event/dag.rs` |
| **中** | 上游 1.161 已删的 `msc2965/auth_issuer` 本仓仍在册 | `docs/synapse-rust/ROUTE_CONTRACT.md` |
| **中** | Profile 三处偏差：稳定 `/{keyName}` 未注册、停用用户写自定义字段 404、account_data 非对象语义 | 路由在册清单（`docs/synapse-rust/ROUTE_CONTRACT.md`） |
| **中** | MSC4502 / MSC4262 仍未收敛（PARTIAL） | 各 8 个 `.rs` 命中 |
| **中** | Admin 媒体端点族缺失（本仓 7 vs 上游文档面 18） | `../synapse-rust/API_COVERAGE_REPORT.md` §6.3 |
| **中** | 缩略图 `animated` 参数未支持；`M_USER_LIMIT_EXCEEDED` 未用于媒体限额 | 业务层 0 命中 |
| **低** | v12/v13 房间不可创建（待决策） | `synapse-common/src/room_versions.rs:114-115` |
| **低** | `search_index` 遗留表（D-39） | baseline 仍有该表 |
| **低** | ledger `query_params` 字段无消费方 | `synapse-web/src/routes/route_ledger.rs` |

### 21.2 本轮证伪（不要重复排查）

| 曾记录的问题 | 实测结论 |
|---|---|
| `scripts/api_test/scan_handler_schemas.py` 硬编码绝对路径、会写错工作树 | **证伪**：`ROOT = Path(__file__).resolve().parents[2]`，是正确的相对推导 |
| `update_pool_metrics` 是死调用点 ⇒ 连接池指标恒 0 | **证伪**：现有周期宿主 `src/tasks/mod.rs:206`、`src/server/mod.rs:405` |
| `scripts/load-test/` 与 `scripts/test/perf/` 两份 k6 重叠、待二选一 | **证伪**：两个目录**都不存在** |

### 21.3 本轮新修复（已完成，勿再立项）

- **D-42**：`synapse-storage/src/event/create.rs` 三处边插入守卫 `WHERE $2 != '[]'` —— `$2` 已被
  `unnest($2::text[])` 定为 `text[]`，PG 把 `'[]'` 当数组字面量 ⇒ 语句在 **prepare 阶段**必报
  `22P02`（**与参数取值无关**）。改为 `cardinality($2) > 0`（`cardinality(NULL)` ⇒ NULL ⇒ 语义等价）。
  此前被两条"期望报错"的回滚用例掩盖（因错误的原因通过）。
- **D-12**：删 `GET /_synapse/admin/v1/event_reports/{id}/history`（对齐 Element），`/stats` 改为对
  `event_reports` 的静态实时聚合；连带重生成全部路由派生产物。
- **D-15.2 / D-15.5**：sliding_sync 游标与 12 个 namespace/统计方法的 DB 往返用例。
- **能力落地**（本轮复核确认）：MSC4140 联邦 EDU、MSC4512 AS 命名空间代理、MSC3912 级联撤回
  （存储/服务/管理端点）、`rc_reports` 专项限流、AS 登录 `m.login.application_service`、
  Content Scanner 装配、MSC3814 `/events` 改 GET。

### 21.4 本轮口径教训（会重复踩）

- **"模块存在 / 配置接上"≠"功能在工作"**：Content Scanner 同时满足前两者却零调用点。
  判据必须落到**调用点**（`grep -rn '<service>\.' <生产目录>`），而不是模块或配置项是否存在。
- **"响应里有字段"≠"字段可用"**：`/send_join` 补了 `state`/`auth_chain`，但条目不是可验签 PDU。
  判据要落到**内容形状**（`hashes`/`signatures` 是否存在），而不是键名是否存在。
- **同一份文档内同一事实出现三种状态**（"缺失"在 §5.2/§6.2 与对比报告各写一次）——人工清单本身
  是漂移源。计数与存在性一律用可复现命令，且每个数字只能有一个来源。
- **变异自证脚本自己也会假红/假绿**：本批的 `mutation_self_proof.sh` 初版用
  `grep -E 'Summary:'` 解析 nextest 结果，而 nextest 打印的是 `Summary [`（**"Summary" 后面没有
  冒号**）⇒ 解析永远为空 ⇒ 连"基线全绿"都被读成"没拿到 Summary 行"，退出码恒为非 0。
  虽然三个变异其实**都按预期转红**（人工读日志可见），但脚本自身的结论不可用。
  修法是先用**已知绿**与**已知红**两份日志离线验解析器，再跑真实变异。
  教训：`-D warnings` 那样"门禁存在但永远绿"是既有教训，这里多了个对称面——
  **自证脚本存在但永远红**同样是失效的守卫。

### 21.5 P0/`/send_join` 收口批次（2026-09-25）

**做了什么**：把联邦四条状态发射路径（`/send_join` v1+v2、`/state`、`/get_room_auth`、
`/get_event_auth`）里各自内联的 5–6 键手工 JSON，统一到一个共享投影器
`synapse-web/src/routes/federation/pdu.rs`。

- **新增 `federation/pdu.rs`**：`state_pdu()` 产出 `event_id`/`room_id`/`sender`/`type`/`content`/
  `origin_server_ts`/`origin`（+`state_key`/`unsigned`）；`depth`/`prev_events`/`auth_events`
  **三者齐备才插入**，否则**省略**并返回 `PduCompleteness::MissingGraphMetadata`——
  填 `[]`/`0` 会让对端把事件当成 DAG 根、污染其房间图，属主动作恶。
  `signature_action()` 的判定表：不完整 **优先** 拒签（`RefuseIncomplete`），完整且库中
  `hashes.sha256` + 非空 `signatures` 齐备 ⇒ `KeepStored`（逐字节原样附着），否则 `SignLocally`。
- **v1 响应补 `[200, {…}]` 二元组包装**，v1/v2 均补 `origin`。
- **`StateEvent` +4 字段**（`prev_events`/`auth_events`/`signatures`/`hashes`，均
  `Option<serde_json::Value>` + `#[serde(default)]`），`STATE_EVENT_{OUTER,INNER}_COLS` 同步；
  连带同步 22 处字面量构造点（纯机械补 `None`）。
- 签名**仅内存**，不落库——同一条投影器同时服务 GET 读路径（`/state`、`/get_event_auth`），
  GET 不应长出写副作用。

**为什么不是"全修"**：本批**只补字段完备性与现场签名**，不碰写入路径。本地事件走
`EventStorage::create_event`（`synapse-storage/src/event/create.rs:14` 的 INSERT 列清单**无**
`depth`/`prev_events`/`auth_events`/`origin`），因此**本地起源**事件的 PDU 仍会被判为
`MissingGraphMetadata` 并**故意以无签名形态发出**（附 `federation_pdu_incomplete_total` 计数与一条
`tracing::warn`）。要真正闭合需把 PDU 图元数据补进创建期写入路径——那是另一项工程，
且涉及 76 个 `create_event` 调用点，不在本批范围。

**验证**（全部在本机实测；`<file>` 路径为工作树）：

| 门禁 | 命令 | 结果 |
|---|---|---|
| 编译 | `cargo check -p synapse-web --features test-utils` | EXIT 0 |
| 权威 clippy | `cargo clippy --workspace --all-targets --features test-utils --locked -- -D warnings` | EXIT 0 |
| 本批守卫 | `cargo nextest run --test unit --features test-utils -E 'test(federation_state_pdu)'` | 9/9 PASS |
| **变异自证** | `/tmp/gate/mutation_self_proof.sh` | 基线 9/9 绿 → **M1**（用常量伪造 `depth:0`/`prev_events:[]`/`auth_events:[]`）打红 2 例 → **M2**（删掉「不完整则拒签」分支）打红 1 例 → **M3**（删掉 `origin_server_ts` 插入）打红 2 例 → 每次从备份还原后 `sha256` 一致、无残留 → 收尾 9/9 绿。脚本按 `Summary [` 行**断言**每个变异确实转红，退出码 0 |
| fmt 棘轮 | `./scripts/check_fmt_ratchet.sh` | `OK (0)` |

**新守卫**：`tests/unit/federation_state_pdu_tests.rs`（9 例）钉住
（a）完整记录必含全部必填键、（b）缺图元数据**必须省略而非伪造**、（c）非数组图元数据算缺失、
（d）`origin` 归一化、（e）签名判定表四种边界、（f）stored 对逐字节附着、
（g）投影结果可被 `sign_and_hash_event` 签名且 `verify_event_content_hash` 通过、
（h）auth chain 的 5-type 规则。

---

## 22. 第四轮复核（2026-09-25，`opt/consolidated` @ `af2df7913`）

### 22.0 方法与基线

**触发**：用户给出一份 13 项报项清单（P0×1 / 高×3 / 中×6 / 低×3），要求
"**根据项目实际先确定以上问题是否存在**，更新本文档"。该清单与 §21.1 的表基本同源。

**方法**：不采信任何既有标记，逐条**回到源码取证**（`路径:行号`，或可复现命令 + 实际输出）。
外部事实（Matrix 规范原文、上游 Synapse 行为）用**主源**核对：

```bash
# Matrix 客户端-服务端规范 v1.11（本机代理 127.0.0.1:7897）
curl -sS -o spec.html https://spec.matrix.org/v1.11/client-server-api/
# 上游 Synapse 1.161 变更日志
curl -sS -o syn161.md https://raw.githubusercontent.com/element-hq/synapse/release-v1.161/CHANGES.md
```

**基线**：`af2df7913`（即本文件上一条 P0 收口提交）。工作树有**并发会话**的在建文件
（`synapse-e2ee/src/verification/service.rs` +42 行、`tests/integration/mod.rs`、
未跟踪的 `tests/integration/api_verification_relay_tests.rs`），**未纳入本轮判定**；
凡涉及 SAS 的结论一律取 **HEAD 版本**（`git show HEAD:…`），与工作树 WIP 无关。

**环境侧新事实（会影响取证，写入本条以免误判）**：本仓除主工作树外还有一份
**并发 worktree `.worktrees/c19b/`**，其内容与 HEAD 同步（同样含 `pdu.rs`）。
任何全仓 `grep` 若不过滤该目录，会把同一份代码计两次 ⇒ **本轮所有检索均排除 `.worktrees/` 与 `target/`**。

### 22.1 本轮新发现

| 编号 | 级别 | 新发现 | 判据 |
|---|---|---|---|
| **N-1** | 高 | SAS 的 `emoji` **不是 6 个**（§21.1 说"仅 6 个"已过期）：`derive_sas` 返回 `[u8; 6]`，代码产出 **7 个**——但用 `byte % 64` 逐字节取模，**不是**规范要求的"前 42 bits 切 7×6bits"。即**数量对了、算法仍错** | 规范 §SAS method: emoji；`verification/service.rs:114-118, 310-322` |
| **N-2** | 高 | SAS `decimal` 的偏差比"被丢弃"更严重：服务端**从 emoji 反推** decimal（`generate_decimal_from_emoji`），而规范要求 **5 字节切 3×13bits（各 +1000）**。两套算法不可能互为逆运算 ⇒ 即使 emoji 修对，反推仍错 | 规范 §SAS method: decimal；`verification_routes.rs:184, 405-412` |
| **N-3** | 中 | 媒体配额**确被强制**（§21.1 隐含的"没强制"不成立）：`ensure_upload_allowed` → `check_upload_quota` → `ApiError::bad_request`。真问题只是**错误码**：超限返回 400，未用 `M_USER_LIMIT_EXCEEDED` | `synapse-services/src/media/mod.rs:246-253, 293` |
| **N-4** | 中 | `M_USER_LIMIT_EXCEEDED` 的**归因可疑**：该码在本仓注册为 MSC4335「服务器达到**用户账户数**上限」 | `synapse-common/src/error/code.rs:83`；`error.rs:1881`（映射 429） |
| **N-5** | 低 | `event_id` 格式的**描述错**（§21.1 写 `$<ms>_<rand>:<server>`）：实际分隔符是 `$`，即 `$<ts>$<b64>:<server>` | `synapse-common/src/crypto.rs:149-153` |

### 22.2 已修复（不要再重测）

| 项 | 判定 | 证据 |
|---|---|---|
| **P0｜`/send_join` 不合规** | ✅ **已修复**（`af2df7913`） | 见下 |
| 中｜MSC4502 / MSC4262 "未收敛" | ❌ **证伪** | 见 §22.4 |
| 中｜Profile `account_data` 非对象 ⇒ 500 | ✅ **已修** | `handlers/extended_profile.rs:48-55` 返回 `ApiError::bad_request`（400） |

**P0 收口的取证（逐条可复现）**：

```bash
# 1) 手工拼装函数已被删除（0 命中）
/usr/bin/grep -rn "serialize_state_event_minimal" --include='*.rs' .   # → 无输出
# 2) 四条发射路径全部改走共享投影器（8 处调用点，均在 pdu.rs 之外）
/usr/bin/grep -rn "build_pdus(" --include='*.rs' . | /usr/bin/grep -v worktrees
#   events.rs:25,93,665,666   join.rs:200,201,338,339
# 3) v1 补回了 [200, {…}] 二元组包装
/usr/bin/grep -n "json!(\[" synapse-web/src/routes/federation/membership/join.rs  # → 205
```

**关于"缺 `event`"这一半**：v2 `/send_join` 不返回 `event` **不是缺陷**——规范中该字段仅在
"房间版本支持 restricted join rules"时必需，本仓不产此类房间。**不要再把它立项**。

**P0 的残余三条（本轮复核后仍成立，但口径需收窄）**：

1. **本地 `create_event` 不落图元数据** —— 成立。`synapse-storage/src/event/create.rs:15-19`
   的 INSERT 列清单为 `event_id, room_id, sender, user_id, event_type, content, state_key,
   origin_server_ts, is_redacted, redacts`，**无** `depth`/`prev_events`/`auth_events`/`origin`。
   对比入站路径 `create_event_with_graph`（同文件 `:83-84`）**有**这三列——所以本地起源事件的
   PDU 会被判 `MissingGraphMetadata`。
2. **入站事件不落远端 `signatures`** —— **需收窄**：入站**成员**事件已回填
   （`synapse-web/src/routes/federation/membership/mod.rs:238` →
   `update_event_signatures_and_hashes`，落库在 `event/signature.rs:10`）；但
   `create_event_with_graph` 的 INSERT 本身**不含** `signatures`/`hashes`，
   故其它入站路径（`/send` transaction 等）是否回填**未逐条确认** ⇒ 本条改为
   "**部分路径未覆盖**"，而不是"完全没落"。
3. **`event_id` 非 v4+ reference hash** —— 成立（描述见 N-5）。这是**独立于字段完备性**的
   语义缺口：即便字段齐全，v11 对等端也无法把本仓 PDU 当规范事件接受。

### 22.3 仍然存在（合并清单，按严重度；含"部分"中的未修子项）

**高｜E2EE SAS**（§22.3 原表，对象已随 §23 删除）**→ 整体作废**：

`synapse-e2ee/src/verification/` 已整目录删除，`verification_routes.rs` 与 `e2ee/devices.rs` 同删，
SAS 的 ECDH/SAS/MAC 全部由**客户端**计算，服务端私钥永不离开客户端，设备验证回归规范
to-device 流程。§22.3 表里 `info` / `commitment` / `emoji` / `decimal` 四列的"❌ 仍错"
是**对象消失**，不是"已修复"——文档口径应统一收口。

⚠️ **但注意**：同路径的**遗留文档**（§7.2 / §11.1 / §14 各处写着"E2EE SAS 仍 4 处偏离规范"、
"QR 为桩"）已被 §23 的文书同步逐条标注作废，并登记进守卫的
`HISTORICAL_NEGATIVE_MENTIONS` 白名单。文档与代码对齐。

**高｜客户端撤回不级联** —— 仍存在。客户端路径 `handlers/room/events.rs:920`（`redact_event`）
只调 `redact_event_content`（`:990`）撤单条；MSC3912 级联入口只有管理端
`admin/room/mod.rs:244,788`（`/_synapse/admin/v1/rooms/{room_id}/cascade_redact`）。

**高｜Content Scanner 零生产调用点** —— 仍存在。`ContentScanner` 有 `scan` / `scan_text` /
`scan_media` 三个公开方法（`content_scanner/service.rs:25,183,193`），但全仓只有
**它自己的单测**在调用；生产侧仅 `wiring/core.rs:179` 的构造，无消费者。

**中｜`dag.rs` 注释声称的调用点不存在** —— 仍存在。注释写明
"Used by `/send_join` (federation) … and by `/get_missing_events`"（`event/dag.rs:203-205`），
但 `get_state_dag_edges` 的引用**只有 `db_tests.rs`**（`:2059,2144`），生产 0 调用点。
（同文件的 `find_missing_event_ids` / `get_missing_events_between` **确有**生产调用点——
`federation/transaction.rs:322`、`federation/events.rs:66`——注释对这两个没说错。）

**中｜`msc2965/auth_issuer` 仍在册** —— 仍存在。本仓注册于
`routes/assembly.rs:197`（+ `derived_route_table_always.inc.rs:114`）。
**上游权威证据**（1.161 `CHANGES.md:48`）：`Drop GET
/_matrix/client/unstable/org.matrix.msc2965/auth_issuer endpoint which never ended up being
used. (#20163)`。
⚠️ **口径须收窄**：上游**只删了 `auth_issuer`**，**`auth_metadata` 未删**
（同文件 `:612` 还专门为它加了缓存）⇒ 本文档不应把两者一起当"已删"。

**中｜Profile 三处偏差 —— 3 个子项中 1 修 2 存**：

| 子项 | 判定 | 判据 |
|---|---|---|
| `account_data` 非对象语义 | ✅ 已修（返回 400） | `handlers/extended_profile.rs:54` |
| 停用但存在用户写自定义字段应成功 | ❌ **仍错** | `user_exists` 的 SQL 带 `AND is_deactivated = FALSE`（`user/storage.rs:698`）⇒ 停用用户被判"不存在"，写路径 `extended_profile.rs:124` 直接 404。**上游 1.161 `CHANGES.md:36`（#20172）**明确："this now **succeeds for existing (e.g. deactivated) users** and returns a 404 error if the user does not exist" |
| 稳定 `/{keyName}` 未注册 | ❌ **仍错** | 派生路由表里 `/_matrix/client/v3/profile/{user_id}`、`/avatar_url`、`/displayname` 都在，**没有**泛化 `{key_name}`；泛化版只在 `unstable/uk.tcpip.msc4133` 下（`derived_route_table_always.inc.rs:269`）。同时 `/versions` 已声明 `m.profile_fields`（`capability_governance.rs:507`）⇒ **声明与注册面不一致** |

**中｜Admin 媒体端点族缺口 —— 部分成立（已降级）**：

- 本仓 `registered_by == "admin::media"` 实测**恰为 7 条**（`admin/media.rs:16-22`
  四条 + `quarantine_media/{media_id}/changes` + `users/{user_id}/media` 的 GET/DELETE）——
  文档的"7"这个数字**可复现**。
- 但"**上游 18**"在仓内**只有数字、无明细**（`API_COVERAGE_REPORT.md:137`，且该表自带
  `[人工口径·未机器复核]` 标注）⇒ **不可核验**。
- **点名示例被证伪**：§6.3 举的"缺 `GET/DELETE /_synapse/admin/v1/users/{user_id}/media`"
  **实际已注册**（`admin/media.rs:20-21`），且在 §21 基线 `9e26ee31a` 上就已存在。
- **真缺口**（仓库侧 0 命中）：`POST .../media/quarantine/{server_name}/{media_id}`、
  `POST .../media/unquarantine/{server_name}/{media_id}`、房间级媒体列举/删除。
  旁证：鉴权白名单已为**不存在**的路由预留了路径
  （`utils/admin_auth.rs:386` 的 `/media/quarantine` 前缀）⇒ 缺口真实存在。

**中｜缩略图 `animated` / 媒体配额错误码 —— 部分**：

- `animated`：全仓 `.rs` **0 命中** ⇒ **仍存在**（未支持）。
- 配额强制：**已实现**（`media/mod.rs:246-253` 的 `ensure_upload_allowed`，被
  `upload_media` `:293` 调用），超限返回 `ApiError::bad_request`（400）。
  **未**使用 `M_USER_LIMIT_EXCEEDED` ⇒ 字面成立；但见 N-4，该码语义是账户数上限，
  "应用于媒体限额"这一期望本身**需要决策**（判为**降级**：口径待议，不是纯缺陷）。

**低｜三项**：

| 项 | 判定 | 判据 |
|---|---|---|
| v12/v13 房间不可创建 | **仍存在，但属设计使然** | `room_versions.rs:114-115` 为 `stable_parse_only("12"\|"13")`；同文件 `:108-113` 注释写明理由是"避免创建无法产生合规 PDU 的房间（fail-safe）"。**不是缺陷，是取舍** ⇒ 建议从"问题清单"移入"已知取舍" |
| `search_index` 遗留表（D-39） | **仍存在** | baseline `migrations/00000000_unified_schema_v12.sql:2839`（+ 4 个索引 `:3840-3843`）仍建表；而 `synapse-storage/src/search_index.rs` **文件已不存在** ⇒ 表无代码消费 |
| ledger `query_params` 无消费方 | **仍存在** | `route_ledger.rs:84` 定义字段、`:107` 有 builder `with_query_params`，但该 builder **零调用点**（全仓仅它自己的定义）；`ledger_export.rs:156` 只把它序列化进导出 fixture，无任何校验/断言消费 |

### 22.4 证伪 / 降级 / 口径修正

| 原结论 | 本轮判定 | 证据 |
|---|---|---|
| 中｜**MSC4502 未收敛（PARTIAL）** | ❌ **证伪** | 端到端完整：客户端路由 `routes/room.rs:53` → handler 解析全部 MSC4502 参数（`handlers/room/members.rs:387-424`）→ 服务层鉴权 + `limit+1` 续页（`room/membership/service.rs:610-706`）→ 存储层游标分页（`membership/mod.rs:716-796`）；另有 `/sync` 的 `not_membership` 消费面（`sync_service/mod.rs:406,423-428`） |
| 中｜**MSC4262 未收敛（PARTIAL）** | ❌ **证伪** | 双向全链路：EDU 类型（`synapse-federation/src/edu.rs:31`）+ 出站广播（`user_service.rs:220,226`）+ 入站处理（`web/src/federation/edu.rs:628-702`，含 origin 校验）+ 落库（`user/storage.rs:783`）+ 消费方 sliding sync `profile_updates`（`sliding_sync_service/extensions.rs:254`）+ 装配（`container.rs:512`） |
| 中｜Admin 媒体族"缺失 **users/{user_id}/media**" | ❌ **证伪** | `admin/media.rs:20-21` 已注册；§22 基线之前就已在 |
| 高｜SAS「`info` 串缺公钥且顺序错」 | ❌ **已修**（描述过期） | `service.rs:44-47` 与规范逐字一致 |
| 高｜SAS「`commitment` 非 SHA-256」 | ❌ **描述错** | 它**是** SHA-256（`service.rs:146-152`）；真错在哈希输入与 base64 padding |
| 高｜SAS「`emoji` 仅 6 个」 | ❌ **描述过期** | 现产出 7 个（`service.rs:310`）；真错在 `byte % 64` 映射 |
| P0｜`event_id` 形如 `$<ms>_<rand>:<server>` | ⚠️ **口径修正** | 实为 `$<ts>$<b64>:<server>`（`crypto.rs:153` 的格式串是 `"${}${}:{}"`） |
| 中｜`msc2965/auth_issuer`「1.161 已删」 | ⚠️ **口径收窄** | 上游只删了 `auth_issuer`（#20163），`auth_metadata` 未删 |
| P0 残余②｜入站事件"不落"远端 `signatures` | ⚠️ **口径收窄** | 入站**成员**路径已回填（`federation/membership/mod.rs:238`）；其余入站路径未逐条确认 ⇒ "部分路径未覆盖" |
| 中｜媒体配额"未强制" | ❌ **证伪**（真问题只是错误码） | `media/mod.rs:246-253,293` |
| 低｜v12/v13 不可创建 | ⚠️ **降级为设计使然** | `room_versions.rs:108-115` 注释自述 fail-safe |

> **方法论教训（本轮）**：§21.1 的这一行把 **5 个子项塞进一行**，其中 1 个已修、1 个描述错、
> 1 个数量过期。**复合条目必须拆成子项逐一给判据**——否则一个已修的子项会永久"污染"整行，
> 让读者以为全都没修（本轮实测正是如此）。同类风险条目：Profile 三项、`animated`+错误码
> （两项合写）。已在本轮 §22.3 全部拆开。
>
> **第二条教训**：`MSC编号 + 命中文件数` **不是收敛度判据**。MSC4502/4262 各命中 8 个文件，
> 但两条都是端到端完整的（§22.4）。命中数只能证明"有人提过这个编号"。

### 22.5 建议的执行顺序

> **⚠️ 第 1 项作废（2026-09-25）**：其前提"服务端存在 SAS 实现"已不成立
> （`synapse-e2ee/src/verification/` 整目录删除，见 §23）。**不要执行第 1 项**。

1. ~~**SAS 三处修正**（高，本仓自有代码、无外部依赖）：`emoji` 改 42-bits 切分、`decimal` 改
   5 字节 / 13 bits（并用 `SasRepresentation::Decimal` 返回，别从 emoji 反推）、
   `commitment` 改 `sha256(pubkey ‖ canonical_json(start_event))` + **unpadded** base64。
   三者可一轮改完，且都能写**已知答案测试**（规范给了逐位公式与 emoji 表）。
2. **客户端撤回接级联**（高）：`handlers/room/events.rs:990` 之后按
   `redacts` 关系调 `event_redaction_service.cascade_redact_event`（服务/存储层已具备）。
3. **Content Scanner 接线**（高）：在媒体/消息落库前调 `scan_media`/`scan_text`，
   否则"装配了但永不扫描"比纯缺失更危险（配置可开、管理员以为已防护）。
4. **Profile 两条**（中）：`user_exists` 与实际存在性解耦（停用用户也算存在）；
   注册稳定 `/{keyName}` 或撤销 `m.profile_fields` 声明——**二选一，不要维持不一致**。
5. **Admin 媒体缺口**（中）：补 `media/quarantine|unquarantine` POST（鉴权白名单已预留）
   与房间级媒体端点；**先把"18 条"的来源清单落成可核验的文件**，否则缺口无法收敛。
6. **`dag.rs` 注释**（中）：改注释或补调用点——注释声称的调用点不存在，属"文档幻觉"，
   代价极低但会误导后续审计。
7. **低三项**：`msc2965/auth_issuer` 直接摘除（上游已删）；`search_index` 表删或标注废弃
   （需同步 baseline 指纹，见 MEMORY.md 硬规则）；v12/v13 从"问题"移入"已知取舍"。

---

## 23. 第五轮：E2EE 设备验证「去服务端私钥」重构（2026-09-25，`opt/consolidated`）

§22.5 第 1 项给出的是"SAS 三处修正（emoji / decimal / commitment）"的**修法**。实际执行时
判定该路径不可取：偏离项位于本仓**私有非规范 REST 面**（`/keys/device_signing/verify_*` 的
`mac` 是单字符串、`keys` 是 `key_id → 公钥值` 映射、**无算法字段**），规范
`hkdf-hmac-sha256.v2` 的 key-list MAC 在该形状内无法表达。故改为**按规范删面**：设备验证回归
客户端 to-device 流程，服务端只中继、只存交叉签名。

### 23.1 删除面（整模块移除）

| 违规面 | 路径 | 违规行为 |
|---|---|---|
| SAS 验证 | `synapse-web/src/routes/verification_routes.rs`（12 条 `.route()` 声明，经 v1/v3 双 `nest` 展开为 **24 条绝对路由**）+ `synapse-e2ee/src/verification/`（4 文件） | `accept_sas` 在服务端生成 X25519 私钥；`generate_sas` 代算 ECDH 与 SAS；`confirm_sas` 用服务端私钥判 MAC 后置 `VerificationState::Done` |
| 设备信任审批 | `synapse-web/src/routes/e2ee/devices.rs`（6 个 handler）+ `synapse-e2ee/src/device_trust/`（4 文件） | 服务端生成设备密钥对并算 MAC；`respond_to_verification` 由**同一 user 的任意客户端**审批即置 `DeviceTrustLevel::Verified` |

连带删除：`e2ee/keys.rs` 的 6 条 v3-only 路由、`assembly.rs` 的 router merge、
`synapse-e2ee/src/lib.rs` 与 `src/e2ee/mod.rs` 的再导出、`wiring/e2ee.rs` 与
`routes/context.rs` 的字段、`e2ee_audit/audit_service.rs` 对 `DeviceTrustStorage` 的依赖
（含 `mark_device_verified` / `mark_device_unverified`），以及 baseline 中 7 张表 + 6 条索引。

**保留**：`key_rotation_log` 表与 `key_rotation` 模块（另有生产消费者）、`E2eeAuditStorage`、
`CrossSigningVerificationService`（其 `is_verified` 只从**交叉签名**推导）、`/keys/query`、
`/keys/signatures/upload`、`/keys/device_signing/upload`。

### 23.2 前提的红证明

重构的前提是"删掉旧面后，规范流程仍可用"。按铁律 8 先加测试、后删除：

- `tests/integration/api_verification_relay_tests.rs::verification_to_device_events_are_relayed_verbatim`
  —— 删除前后均 **PASS**：`PUT /sendToDevice/m.key.verification.start/{txn}` 的事件在
  `/sync` 的 `to_device.events` 中原样出现（`sender` / `content` 逐字段相等）。
- 同文件 `server_side_sas_endpoints_are_gone` —— 删除前 **FAIL**（旧 handler 返回 422
  `missing field from_device` 而非 404，证明端点当时确实存活），删除后 **PASS**。

### 23.3 连带发现的七个真实缺陷

1. **测试容器 to-device 限额恒为 0**（**已修**）：`synapse-services/src/test_config.rs` 用
   `ServerConfig::default()` 再 `..Default::default()`，而 `ServerConfig` 是
   `#[derive(Default)]`（`#[serde(default = "...")]` 只在反序列化路径生效）⇒
   `to_device_max_recipients = 0` / `to_device_max_payload_bytes = 0`，**任何**
   `PUT /sendToDevice` 都必然 400 `M_BAD_JSON`。该缺陷此前从未被发现，因为没有任何测试
   走过 to-device 路径。已在 `build_test_config()` 中显式给值。
2. **X25519 无贡献性检查**（**随模块删除作废**）：`compute_shared_secret` 直接接受低阶点公钥
   （RFC 7748 §6.1），共享密钥退化为全零，而该密钥发送方自己就能算出 ⇒ 配合调用方自选的
   `peer_pubkey`，任何已认证用户都能在**无真实设备参与**下通过 `confirm_sas`。本轮曾加
   `was_contributory()` 修复；随 `verification` 模块删除，该攻击面已不存在。
3. **baseline 指纹守卫**（**已同步**）：删表改变了
   `migrations/00000000_unified_schema_v12.sql` 的字节内容，须同步
   `test_isolation_unification_tests::EXPECTED_BASELINE_FINGERPRINT`
   （`a58420543eb97db2` → `e151e5956fb64914`，独立复算 FNV-1a 64 并先用旧值自检）。
4. **`scripts/api_test/export_ledger.sh` 自 H-6 起恒失败**（**已修**）：脚本固定传
   `--features server,core-private-chat,…`，而 `server` 特性已被 H-6 删除（注释原文：
   "零 `#[cfg(feature = "server")]` 门控"，纯死标志）⇒ cargo 直接报
   `the package 'synapse-rust' does not contain this feature: server`。
   叠加第二个 bug：`PROFILE="${1:-default}"` 会把 `--output=X` 当成 profile，
   于是 `--output=` 形式调用必然失败。两个 bug 使 README 里
   "路由有增删时请重新执行 `./export_ledger.sh` 刷新 `ledger.json`" 这条路径
   **自 2026-08 起无法执行** ⇒ `ledger.json` 冻结在 2026-08-12（1292 条，含本次删除的
   全部端点），由它生成的 `docs/openapi/client.yaml` 同样过期（89 处已删端点）。
   已修两处并重新导出（**1292 → 1096 条**）+ 重生成 `client.yaml`
   （`gen_client_yaml.py --skip-export --check` 复验通过）。
5. **`docs/openapi/route-table.json` 与 CI 口径不符（既有红）**（**已修**）：该产物的
   CI 门禁（`.github/workflows/ci.yml` 的 `openapi-artifact` 作业）是"用
   `cargo build --bin synapse_ledger_export`（**默认特性**）+ `--profile=default` 的
   新鲜导出重新生成并逐字节比对"。HEAD 提交的是 **1146** 条，而同一构建在删面之前产
   **1063** 条（证据：`tests/unit/fixtures/ledger_export/default.json` 由同一条装配路径
   产出，HEAD 版恰为 1063）——差额 **83** 正是 feature-gated 模块（CAS / SAML /
   ExternalServices / Voice…），即该文件当时被**全扩展导出**覆盖过。CI 只在 `main` 与
   PR 触发，而本仓工作在 `opt/consolidated`，故该红从未被跑到。本次重生成后为 **1033** 条，
   与新鲜导出逐字节一致（`gen_route_table.py --check --ledger <fresh>` 通过）。
6. **`gen_contract_doc.py` 把两个派生表计数写死**（**已修**）：`ROUTE_CONTRACT.md` 那句
   "这类孪生行正是派生表 **1168** 行去重为 **1166** 条"里的两个数是**硬编码**，而同一句
   下一行的"当前共 N 条双档注册"是算出来的 ⇒ 路由面每增删一次，同文档的总览（动态）与这句
   （写死）就背离。实测本轮开工时它已比真实值多 1（真实 1167 → 1165）。已改为从已解析的
   `profile_rows` 求和 + `len(profiles_of)`，重生成后为 **1137 行去重为 1135 条**。
7. **`scripts/test/api-integration_test.sh` 的 5 个 "Verification Routes" 用例自诞生起就是纸面门禁**
   （**已删**）：第 78 节（用例 225–229）用 `curl -s … && pass … || skip …` 打
   `/_matrix/client/v0/keys/request_verification` 与
   `/_matrix/client/v0/keys/verification/request/{id}/{accept,complete,cancel}`。两个独立缺陷
   叠加使它们**恒 `pass`**，而报告里一直以"通过"出现：
   - **端点从未存在**：被删的服务端路由只有 **v1/v3 双 `nest`**（`verification_routes.rs`
     的 `compat_router`），而用例写的是 **v0**。取证：`git grep -c "_matrix/client/v0" HEAD -- synapse-web/`
     **零命中**，`git show HEAD:…/derived_route_table_always.inc.rs | grep -c "_matrix/client/v0/"`
     也是 **0** ⇒ v0 形态在本仓历史上从未注册过。
   - **退出码骗过了断言**：`curl -s` 对 HTTP **404 仍返回 0**，所以 `&& pass` 命中、`|| skip`
     分支永远走不到。把二者叠加，这 5 行"测试"在任何时刻都不可能失败。

   已整节删除并留说明注释。同批清理 `scripts/api_test/errcode_validator.py` 里 `/keys/qr_code`、
   `/keys/verification` 两条**死规则** —— `_find_rule` 匹配不到会静默跳过（不报错），但会虚增
   `total_rules`，并让人以为这两族端点仍在服务面上。

### 23.4 与 §22 的关系

§22.1（N-1/N-2 的 emoji / decimal 算法错）、§22.3（SAS 5 子项：info 已修 / 派生已修 /
commitment 仍错 / emoji 仍错 / decimal 仍错）以及 §22.5 第 1 项，**均以"服务端存在 SAS 实现"
为前提**。该前提已不成立：`synapse-e2ee/src/verification/` 已删除，故上述条目**全部作废**
—— 不是"已修复"，而是"对象消失"。

仍有效的相关遗留只有一个：**客户端接线**。`Tjg` 的 `CryptoDeviceAdapter.ts` 仍短路指向已删的
`/device_verification/request`，本轮删除后该路径会返回 404；改用
`m.key.verification.*` to-device 属**独立后续任务**，不在本次范围（本次仅服务端）。

**跨仓 follow-up（本仓无法闭合）**：本轮刷新了两个被下游消费的契约产物 ——
`docs/openapi/route-table.json`（1146 → 1033）与 `docs/openapi/client.yaml`
（704 → 477 个 path）。`matrix-sdk-fork` 的 `src/__generated__/route-table.ts` 与任何按
`client.yaml` 代码生成的调用方**都是过期产物**，须在那两个仓按各自生成器重跑；本仓的
`gen_route_table.py --check` / `gen_client_yaml.py --check` 只保证**本仓内**的
"提交的产物 == 可复现的产物"，管不到下游。

### 23.5 门禁与派生产物同步

删路由与删表触发以下派生产物重生成：`derived_route_table_*.inc.rs`、
`docs/synapse-rust/ROUTE_CONTRACT.md`、`docs/openapi/route-table.json`、
`docs/openapi/client.yaml`、`scripts/api_test/{ledger,handler_schemas,response_schemas}.json`、
ledger fixtures（default 与 sdk 两条 lane）、`route_ledger_*.snapshot`。

手工同步：两个 `api-integration_test.sh` 的已删端点用例、
`check_schema_contract_coverage.py` 的 6 条 `TABLE_CONTRACTS`、`logical_checksum_tables.txt`
与其生成器、`scripts/ci/` 的四份基线（`coverage_baseline.json` /
`ts_order_single_key_baseline` / `sqlx_literal_production_baseline` /
`sqlx_dynamic_ratio_baseline` 的注释）、`db-migration-gate.yml` 的历史说明注释、
`doc_credibility_guard_tests` 守卫的文档计数，以及上文的 baseline 指纹常量。

其中三件派生物**不只是刷新**，还修掉了既有缺陷（见 §23.3 第 4/5 项与 §23 开头的分类偏差）：

- `scripts/api_test/export_ledger.sh`：修 `server` 死特性 + `$1` 抢占 profile 两个 bug；
- `scripts/api_test/ledger.json`（**1292 → 1096**）与 `docs/openapi/client.yaml`：随脚本修复首次真正刷新；
- `docs/openapi/route-table.json`（**1146 → 1033**）：首次与 CI 的"默认特性新鲜导出"口径对齐。

文书口径同步：`docs/synapse-rust/ROUTE_CONTRACT.md`（**1165 → 1135 条 / 66 → 65 模块**）、
`docs/synapse-rust/API_COVERAGE_REPORT.md`（v1.6，三口径 **1135 / 903 / 795**，并修正 Client
分类表"打印的配方复现不出打印的表"的 ±5 归类偏差）、
`docs/synapse-rust-vs-synapse-comparison.md`（v1.8，另把 §7.2/§11.1/§14 各处仍写着"E2EE SAS
仍 4 处偏离规范 / QR 为桩"的**旧结论逐条标注作废**，并把两条已删模块路径按守卫的
`HISTORICAL_NEGATIVE_MENTIONS` 白名单显式登记）。

**基线卫生（同批顺手清掉的死条目，两类都不报错）**：

- `scripts/ci/coverage_baseline.json`：6 条 `path` 指向已删文件（3 条 `device_trust/*`、
  2 条 `verification/*`、1 条 `routes/verification_routes.rs`）。不报错的原因是棘轮里
  `if cur is None: continue` —— 基线有、lcov 报告没有的路径会被静默跳过。
- `scripts/ci/sqlx_literal_production_baseline`：2 行。一行是同一批删除的
  `verification/storage.rs`（8 处）；另一行 `rendezvous.rs`（16 处）**与本批无关** —— 该文件
  早已全部改用 `sqlx::query!` / `query_as!` **宏**，而生成命令用的 `DYNAMIC_RE` 按定义
  排除 `!` 形态（`scripts/ci/sqlx_query_census.py:57-61`），故这一行是 C 系列静态化之后
  **忘了下调**的历史松弛值，实测恒为 0。字面量棘轮只遍历**实测**站点，所以死行既不报错也不生效。
- 同批给上述生成命令补上 **`LC_ALL=C sort`**：默认 locale 下 `_` 与 `/` 的次序不同，同一份
  实测表会出现两种行序（实测 `room/models.rs` 与 `room_account_data.rs` 互换），在逐行
  diff 里会伪装成"抽取器漂移"。
- 清理后两张表都与重跑的实测**逐行一致**（字面量 **534 处 / 79 文件**）。
--- 第五轮已修 / 已同步 ---
● baselines/指纹：migrations/00000000_unified_schema_v12.sql (7 张 E2EE 表删 + 6 条索引删) → expected 228 → 221；EXPECTED_BASELINE_FINGERPRINT a584... → e151...
