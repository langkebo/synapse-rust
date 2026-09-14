# synapse-rust 现存问题清单（审查验证版）

日期：2026-09-14（**第二轮复核**，见 §15 复核记录）
基线：`main` @ `d56a1d82`（首版基线 `e6ecda02`）
验证方式：**每条都附实测命令与实测结果**。未实测的明确标注 `[未验证]`。

---

## 0. 图例与验证环境

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

`docs/audit/P4_concurrency_perf_2026-09-11.md:222` 已记录该测试为性能敏感
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

## 6. 🔴 CI lint 门禁不覆盖 workspace —— **首版误标 ✅，实为未修（见 §15.1）**

> **第二轮复核更正**：本节首版标题写"✅ 已修复（`8509c52b`）"，是**误判**。
> `8509c52b` 只清掉了既有 warning（症状），**门禁的覆盖范围一个字没改**。
> §15.1 的变红实验证明：向 workspace crate 的 `#[cfg(test)]` 模块注入一处
> `clippy::bool_comparison`，CI 的 clippy 命令**退出 0、报 0 条错误**，
> 而加 `--workspace --all-targets` 后**退出 101、报 2 条**。
> 判定依据是 `AGENTS.md` 第 8 条：门禁类问题必须"故意制造违规能让它变红"才算修复。
> 以下保留本节原始记录（含首版的自相矛盾之处），仅更正状态。

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

> **修复（2026-09-14 完成）**：14 条 warning 全部清零（commit `8509c52b`）：
> - `assert_eq!(x, false/true)` → `assert!(x)` / `assert!(!x)`（5 处，key_request/secure_backup）
> - `1 * day_ms` → `day_ms`（3 处，pruning.rs）
> - `.bind(&user_id)` → `.bind(user_id)`（2 处，db_tests.rs）
> - 删除未使用 import `InMemoryToDeviceStorage`（to_device/service.rs）
> - field-assignment-outside-initializer → struct literal（2 处，key_rotation + cache tests）
> - `#[allow(dead_code)]` 标注保留的可复用测试辅助（voice.rs）
> - clippy `map().unwrap_or_else()` → `map_or_else()`（test_isolation guard）
>
> 验证：`cargo clippy --workspace --all-targets --all-features` **零 warning**。
> CI 门禁（`:217`）虽未加 `--workspace`，但已确保全 workspace 干净，可作为后续加入
> 门禁扩展的基线。

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

## 15. 第二轮复核记录（2026-09-14，基线 `d56a1d82`）

本节记录复核**推翻或修正首版结论**的地方，以及复核中新发现的问题。

### 15.1 🔴 **§6 的 ✅ 是错的**：clippy 门禁仍不覆盖 workspace 测试代码

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

### 15.2 🔴 **新发现**：共享模块的模板 schema 无限累积，且没有清理机制

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

### 15.4 复核中新出现的两类本地残留（由本轮验证产生，非产品缺陷）

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

## 16. 🔴 **新发现**：MSC4108 rendezvous 实现偏离规范（协议缺口）

首版 §12.2 只记录了"manifest 一致、`tags` 未验证"。复核对照 MSC4108 规范正文
（[4108-oidc-qr-login.md](https://raw.githubusercontent.com/matrix-org/matrix-spec-proposals/87f8317a902cd7bc5c2d2d225f71021b3a509e2d/proposals/4108-oidc-qr-login.md)）
后发现实现缺了多处**规范标记为 required** 的行为。

规范要求（insecure rendezvous 小节）：

> ##### Common HTTP response headers
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
| 🔴 P1 | **§6 clippy 门禁仍不覆盖 workspace 测试代码** | 门禁真实性 | 首版误标 ✅，**实为未修**。变红实验已证（§15.1） |
| 🔴 P1 | **§16 MSC4108 偏离规范 10 项** | 协议正确性 | 新发现；4 个既有测试固化了错误期望 |
| 🔴 P2 | **§15.2 共享模块模板 schema 无限累积** | 资源泄漏 | 新发现；根夹具有清理机制，共享模块没有 |
| 🟡 P2 | **§17 契约文档含未加 nest 前缀的路由；解析器有链式/nest 盲区** | 契约正确性 | 新发现：`/spaces/...` 15 条相对路径写入文档、0 条带前缀；`nest_map` 收集后从未使用 |
| 🟡 P3 | §7 契约链两条车道的 feature 集复杂度 | 架构 | 未动，需先统一 feature 集 |
| ⚪ P3 | §9 schema 残留清理 | 运维 | ✅ 已修复（`82921311`：CI 加 cleanup step + 脚本硬排除/独立保护/可配置标记目录）。CI 侧防 intra-job 累积；本地仍需手动 `--apply`（见 §9 说明） |
| ⚪ P3 | §10 存量债、§11 局限 | 技术债 | 新代码设禁，存量另立专项 |
| ✅ | §1 / §3 / §4 / §5 / §2 收敛 / §12 四条线索 | — | 已核实（§15.3） |

**核对方式说明**：本表把首版标 ✅ 但实为未修的 §6 降级为 🔴。
判定标准是 `AGENTS.md` 第 8 条——**门禁类问题只有在"故意制造违规能让它变红"
被实测证明后，才算修复**；清掉存量 warning 不等于门禁生效。

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
| P1 | §6 clippy 门禁覆盖 workspace | 门禁真实性 | ✅ 已修复（清 14 条 warning） | ❌ **误判**：只清症状，门禁仍不覆盖。见 §15.1 |
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
cargo clippy --workspace --all-targets --all-features --locked 2>&1 | grep -c '^warning'
# 变红实验：向 synapse-storage/src/voice.rs 的 #[cfg(test)] 内注入
#   if pool.is_closed() == true { return; }
# 然后对比：
#   cargo clippy --all-features --locked -- -D warnings                       # 退出 0（看不到）
#   cargo clippy --workspace --all-targets --all-features --locked -- -D warnings  # 退出 101（看得到）

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
