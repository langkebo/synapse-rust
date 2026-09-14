# synapse-rust 现存问题清单（审查验证版）

日期：2026-09-14
基线：`main` @ `e6ecda02`
验证方式：**每条都附实测命令与实测结果**。未实测的明确标注 `[未验证]`。
本文档只做记录，**没有为本清单修改任何代码**。

---

## 0. 图例与验证环境

| 标记 | 含义 |
|---|---|
| ✅ | 本轮已修复（附修复提交） |
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

## 2. 🟡 测试基础设施：`clone_schema_from_template` 曾 3 份实现 → 现 1 份实现 + 2 份薄封装

**违反项目规则第 2 条（同一职责只允许一份实现）。** 修复进行中。

### 原始状态（实测）

```bash
$ grep -rn "fn clone_schema_from_template" --include=*.rs . | grep -v worktrees
synapse-common/src/test_isolation.rs:848   pub async fn clone_schema_from_template(pool, schema, template)   # 共享版
synapse-services/src/test_utils.rs:484     async fn clone_schema_from_template(database_url, template_name)  # 私有
src/test_utils.rs:1021                     async fn clone_schema_from_template(database_url, template_name)  # 私有
```

三份能力不同（实测各自特性计数）：

| 实现 | 数据复制 | 外键回放 | 完整性校验 | 索引名还原 |
|---|---|---|---|---|
| `synapse-common`（共享） | ✅ | ✅ | ✅ | ❌ → ✅（本轮补） |
| `src/test_utils.rs` | ✅ | ✅ | ❌ | ✅ |
| `synapse-services/src/test_utils.rs` | ❌ | ❌ | ❌ | ❌ |

### 本轮进展

**已完成（提交见括号）**

1. **共享模块补上"索引名 + UNIQUE 约束名归一化"**（`6a051fcb`）。
   实测 `LIKE ... INCLUDING ALL` 会把 `idx_t_v_named` 改名成 `t_v_idx`、
   把 `uq_c_pid_named` 改名成 `c_pid_key`（PRIMARY KEY 名保留）。而
   `has_index_named` 在 `tests/integration/schema_contract_p0_tests_migrated.rs`
   有 **24 个调用点**，`validate_clone` 又只比数量 → 不做这一步就切换会**静默破坏**
   这些断言。新增 Phase 1d 处理它，并加了测试
   `clone_preserves_index_and_unique_constraint_names`（**已反向验证**：
   把 Phase 1d 置为 no-op 后该测试变红）。
2. **`synapse-services` 那份私有实现改为委派并降为薄封装**（`ea1a3ddc`）。
   它原本是三者中最弱的（不复制 seed 行、不回放外键、无校验）。
   验证：retention 队列 7/7；services 完整 lib **2079 run / 2078 passed / 1 failed**
   （基线 2076 passed / 3 failed，**减少 2 个失败**；剩余 1 个为既存 media 失败）。

**剩余（未做，需决策）**

`src/test_utils.rs` 的那份（177 行）仍自实现。它比 services 那份强，且有**一处
共享模块不具备的行为**：

```rust
const SEED_REFERENCE_TABLES: &[&str] =
    &["server_media_quota", "server_retention_policy", "sync_stream_id"];
// 注释：the development-only `@admin:localhost` seed in `users` is intentionally
//       NOT copied — tests manage their own users and many assert an empty users table.
```

即：根 crate 车道用**显式 seed 白名单**，共享模块则复制**所有表**的数据。
直接在根 crate 委派，会把 `users` 表的数据（含开发种子）也复制进克隆，
**破坏"users 表应为空"的断言**。

因此收敛它需要先决定：

- **(a)** 给共享模块加可选的 seed 白名单参数（调用方传入；默认全表复制）——
  这个既有信息量又有约束力，且能同时服务两条车道；
- **(b)** 让根 crate 车道也接受全表复制，并修掉依赖空 `users` 的测试；
- **(c)** 保持两份实现，接受该分歧（不推荐，违反规则 2）。

推荐 **(a)**。注意根 crate 那份还带 `SCHEMA_POOL`（TRUNCATEd schema 复用）与
`SHARED_CLONE_SEMAPHORE`，这些是**调用侧**的关注点，不在 `clone_schema_from_template`
内部，可保留在原处。

### 守卫的盲区（仍未修）

`tests/unit/test_isolation_unification_tests.rs` 只断言"两份夹具**调用了**共享模块"，
**不检测**是否还存在第三份自实现。建议加一条静态断言：
"全仓 `fn clone_schema_from_template` 的定义只允许出现在 `synapse-common`"。


**三份能力不同**（实测各自的特性计数）：

| 实现 | 数据复制 | 外键回放 | 完整性校验 |
|---|---|---|---|
| `synapse-common`（共享） | ✅ | ✅ | ✅ |
| `src/test_utils.rs` | ✅ | ✅ | ❌ |
| `synapse-services/src/test_utils.rs` | ❌ | ❌ | ❌ |

`synapse-services` 那份能力最弱：**不复制 seed 行、不回放外键、不做校验**——正是
"缺表 → `search_path` 静默回退 `public`"的温床。

### 影响

- 该类 bug 需要修三次（历史上已经发生过：测试隔离统一前有三份实现，同一类缺陷修了多次）
- 新增的守卫测试 `tests/unit/test_isolation_unification_tests.rs` **只覆盖
  storage 与 services 的对外夹具**，**不检测**这两份遗留私有实现是否仍在

### 建议

把 `src/test_utils.rs` 与 `synapse-services/src/test_utils.rs` 的私有实现改为调用
`synapse_common::test_isolation`，或删除；并在守卫测试里加一条"全仓只允许一份
`clone_schema_from_template` 定义"的断言（可静态扫描源码）。

---

## 3. 🔴 测试确定性：两个既存失败

### 3.1 `test_calculate_age_near_zero` —— 时钟容差过紧

```rust
fn test_calculate_age_near_zero() {
    let now = current_timestamp_millis();
    let age = calculate_age(now);
    assert!(age <= 1, "age for now should be near zero, got {age}");
}
```

实测：`--test-threads 4` 下曾报 `got 6`；单独跑 0.020s 通过。
**容差仅 1ms，并发下调度延迟即失败。** 修法：放宽到合理范围（如 `<= 50`），
或改用单调时钟比较。

### 3.2 `render_appservice_scheduler_prometheus_metrics_reflects_recovery_summary`

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

---

## 4. 🔴 `media::tests` 的确定性失败（3 个）

实测（`--test-threads 1` 单独跑也会失败，**不是抖动**）：

```
FAIL media::tests::test_chunked_complete_can_be_downloaded_via_media_service   (media/mod.rs:1014/1026/982)
FAIL media::tests::test_delete_media_rolls_back_quota_usage                     (media/mod.rs:1054)
FAIL media::tests::test_ensure_media_not_quarantined_rejects_non_admin          (media/mod.rs:1275)
```

典型错误：

```
23503 insert or update on table "upload_progress" violates foreign key constraint
      "fk_upload_progress_user"
      Key (user_id)=(@chunk_tester:test.server) is not present in table "users".
      schema: Some("public")
```

**根因**：`prepare_media_test_pool`（`media/mod.rs:699`）自建一个**部分 schema**
（实测 9 张表），`search_path = <schema>, public`；当所需表不在该 schema 时
**静默回退到 `public`**，而 `public` 里没有该测试用户 → 外键违约。

且失败断言点会漂移（有时是外键、有时是 `Content-Disposition` 里出现 media-id 前缀），
说明是多个缺陷叠加。

**CI 现状**：主门禁用 `-E 'not test(/^media::tests::/)'` 排除这 13 个用例，
并由 `scripts/ci/check_media_exemption_still_needed.sh` 守卫。

**该守卫的缺陷（结构性的）**：判定逻辑是"连跑 3 次，**有任何一次失败** → 豁免仍必要
→ `exit 0`"。因为失败是确定性的，它**永远走"仍必要"分支**，自我收回分支不可达——
无法区分"串扰仍在"与"夹具坏了"。另外无 DB URL 时它也 `exit 0`（静默跳过）。

**建议**：让 media 夹具改用共享隔离池（`prepare_isolated_test_pool`），
然后移除豁免与守卫。这一步能同时消掉 3 个失败与守卫缺陷。

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

## 6. 🔴 CI lint 门禁不覆盖 workspace

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

## 8. 🟡 迁移文档 `ROUTE_CONTRACT.md` 的漂移门禁：**已验证有效**

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

## 9. 🔴 测试库 schema 残留（实时数据）

```bash
$ for db in synapse synapse_test; do psql ... -c "select count(*) from pg_namespace where nspname like 'test\_%' or nspname like 'media_test_%'"; done
synapse:      test_* 残留=388   public 表=253
synapse_test: test_* 残留=2     public 表=253
```

`synapse` 库累积 **388** 个残留 schema（每个含 200+ 表）。
`scripts/cleanup_test_schemas.sh` 需手动 `--apply`，**无自动调用路径**；
且它要求"存在 live 模板标记"才肯删，而标记路径基于 `CARGO_TARGET_TMPDIR`，
本地若用自定义 `CARGO_TARGET_DIR` 会找不到标记而中止（实测遇到过一次）。

**建议**：CI 加一步 `cleanup_test_schemas.sh --apply`；脚本的模板标记路径改为
可配置或按模板 schema 名反查（而非依赖本地 target 目录）。

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

## 12. `[未验证]` 待查线索

以下**未取得实测证据**，仅作为线索列出，不应据此决策：

1. `src/web/routes/friend_room.rs` 的"双前缀"问题（B-3）：实测该文件
   `with_module`/`with_status` 均为 0 处，但**未验证**它注册的路由是否存在
   重复前缀或与 spec 不符。
2. `msc4108_rendezvous.rs`（B-4）：同样 0 处标注，**未验证**其 `tags` 字段缺失
   是否构成契约问题。
3. B-10/B-11 的"剩余条目"：**未逐条验证**。相关根因（manifest 与 router 注册
   分两张表）存在，但具体缺哪些条目未实测。
4. `synapse-e2ee` 的 `unused import: crate::test_mocks::InMemoryToDeviceStorage`
   实测存在（clippy 报出），但**未验证**是测试代码还是生产代码。

---

## 13. 汇总：建议的处理顺序

| 优先级 | 问题 | 类型 | 状态 / 预估成本 |
|---|---|---|---|
| ~~P0~~ | ~~§1 CI 指向生产库 + wipe 标志~~ | 数据安全 | ✅ **已修复**（`00c0aad2`：一库两 schema + pin `TEST_DB_TEMPLATE_SCHEMA`，DROP public 结构性不可能） |
| P0 | §4 `media::tests` 确定性失败 | 测试正确性 | 🔴 夹具改用隔离池（本轮降到 1 个失败，仍未清零） |
| P1 | §2 `clone_schema_from_template` 多份实现 | 架构一致性 | 🟡 **2/3 已完成**：共享模块补齐索引名能力（`6a051fcb`）+ services 改为委派（`ea1a3ddc`）；剩 `src/test_utils.rs`，需先决定 seed 白名单方案 |
| P1 | §3 两个既存失败（时钟容差 / 守卫判据） | 测试确定性 | 🔴 守卫判据已随 §1 修掉（DB 名含 test）；剩时钟容差 1 处小改 |
| P1 | §6 clippy 门禁覆盖 workspace | 门禁真实性 | 🔴 加 `--workspace --all-targets`，再清 21 条 warning |
| P2 | §5 `status` 字段去留 | 冗余治理 | ✅ 已完成（`c5a5df0d`，删除，schema 3→4） |
| P2 | §9 schema 残留自动清理 | 运维 | 🔴 CI 加一步 |
| P3 | §7 两条车道的复杂度 | 架构 | 🟡 需先统一 feature 集 |
| P3 | §10 存量债、§11 局限 | 技术债 | ⚪ 新代码设禁，存量另立专项 |

---

## 14. 复现命令速查

```bash
# §1 CI 危险组合
grep -nB1 -A1 "SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE" .github/workflows/ci.yml

# §2 三分实现
grep -rn "fn clone_schema_from_template" --include=*.rs . | grep -v worktrees

# §3/§4 两个类别的失败
cargo nextest run --test unit --features test-utils -E 'test(/test_calculate_age_near_zero/)'
cargo nextest run -p synapse-services --lib --all-features --test-threads 1 -E 'test(/^media::tests::/)'

# §5 status 无消费方
grep -rn '\.with_status(' --include=*.rs src | wc -l
grep -rn 'sunset_at' --include=*.rs src | grep -v 'route_ledger.rs\|ledger_export.rs' | wc -l

# §6 clippy 覆盖
grep -n "cargo clippy" .github/workflows/ci.yml
cargo clippy --workspace --all-targets --all-features --locked 2>&1 | grep -c '^warning'

# §8 路由契约漂移门禁（有效）
bash scripts/contract/check_route_contract.sh

# §9 schema 残留
psql ... -c "select count(*) from pg_namespace where nspname like 'test\_%' or nspname like 'media_test_%'"
```
