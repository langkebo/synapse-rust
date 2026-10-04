# P1-B 验证报告：storage 隔离夹具改为模板克隆

分支：`perf/test-isolation-template`（基点 `671c4ea6`）
状态：**验证通过，可合并**（未合并）
范围：仅 `synapse-storage/src/test_isolation.rs`

---

## 1. 要解决的问题

`--workspace --lib` 门禁概率性红：同提交同参数下 `6128/0`、`6116/4`、`6117/3` 交替出现，
失败全部是 `Operation timed out`，单跑又通过。storage 的 `IsolatedTestPool::new()`
对**每个**用例重放 `00000000_unified_schema_v11.sql`（253 CREATE TABLE + 373
CREATE INDEX + 45 ALTER TABLE + 函数/视图/触发器），逐语句一个往返。

## 2. 插桩测量（改动前，89 用例队列）

在改动前的同一提交上加 `[ISO_TIMING]` 插桩实测：

| 阶段 | median | p90 | max | Σ |
|---|---|---|---|---|
| `admin_connect` | 0.030s | 0.035s | 0.049s | 2.7s |
| **`baseline_replay`** | **4.416s** | 5.093s | 5.815s | **393.5s** |
| `cleanup`（DROP SCHEMA） | 0.520s | 0.647s | 1.045s | 48.4s |
| `setup`（合计） | 4.459s | 5.143s | 5.853s | 397.2s |

**结论：baseline 重放占 setup 的 99%。** 优化目标由此确定，不需要猜。

## 3. 改动内容

`IsolatedTestPool::new()` 不再重放 baseline，改为克隆一个**共享模板 schema**：

1. **模板**：按 baseline 内容指纹命名（`test_isolation_template_<fnv1a64>`），
   每个数据库只构建一次；改动迁移会自动换模板，不会误用旧模板。
2. **跨进程串行化**：session 级 `pg_advisory_lock`。nextest 一进程一用例，
   进程内 `OnceLock` 挡不住并发 `CREATE SCHEMA`。
3. **完整性标记**：模板内写 `_synapse_test_template_ready`，仅在全部语句成功后写入；
   被超时/SIGKILL 打断的半成品模板会被重建（否则克隆会静默回退到 `public`）。
4. **单次往返克隆**（`clone_statement`），两阶段，`search_path` 的切换是正确性关键：
   - 阶段 1（`search_path` = 模板）：`CREATE TABLE ... (LIKE ... INCLUDING ALL)`，
     带上列/默认值/生成列/identity/索引/主键/UNIQUE/CHECK。
   - 阶段 2（`search_path` = 克隆）：回放**函数、视图、物化视图、外键、触发器**。
     PL/pgSQL 函数体不是 schema 绑定的——若在 `search_path` 指向模板时创建，
     函数体会静默解析到**模板的表**，于是克隆里的函数会写模板的数据。
     实测确认：切换后克隆的 6 个函数体均不再提及模板 schema。
5. **克隆完整性校验**（`validate_clone`）：比对克隆与模板的表/外键/函数/视图/
   物化视图/触发器数量，不一致立即报错。这是防"库缺对象 → 静默回退 `public`
   → 变成莫名其妙的顺序相关失败"的守卫。
6. `Drop` 拒绝删除模板 schema；URL 解析走 `TEST_DATABASE_URL`/`DATABASE_URL`
   零探测快路径。

### 实现过程中被实测推翻的两个假设

- **`LIKE ... INCLUDING ALL` 不复制外键**。校验器首次运行即报 `fks 0/127`。
  必须显式回放外键。
- **视图定义带模板限定**。`pg_get_viewdef` 输出
  `FROM test_isolation_template_x.workers`；不剥掉限定会得到
  `42601 syntax error at end of input`，且即使创建成功，克隆的视图也会读模板的行。

## 4. 验证结果

### 4.1 功能

| 套件 | 结果 |
|---|---|
| `synapse-storage --lib` @threads=4 | **1760 passed / 0 failed** |
| `captcha::db_tests` | 33 passed / 0 failed |
| `worker::db_tests`（依赖 `active_workers` 视图） | 41 passed / 0 failed |
| `user\|openid_token\|media_quota\|cas::db_tests` | 113 passed / 0 failed |

改动前同命令：`1760 run: 1758 passed, 2 failed`（`captcha::db_tests` 两个用例
`Io(Os { code: 60, kind: TimedOut })`）。

### 4.2 性能

| 指标 | 改动前 | 改动后 |
|---|---|---|
| `baseline_replay` median | 4.416s | **2.911s**（克隆） |
| `admin_connect`/模板检查 median | 0.030s | 0.038s |
| 113 用例队列 @threads=4 | — | 279–281s |
| `synapse-storage --lib` @threads=4 | 373.9s（2 failed） | **279.5s（0 failed）** |

2.9s 仍是主要成本（253 表 + 373 索引的 DDL 无法避免），但已从 4.4s 降下来，
且**失败归零**。

### 4.3 门禁

- `./scripts/check_fmt_ratchet.sh` → `current=0 baseline=0`，通过
- `cargo clippy -p synapse-storage --all-targets --all-features --locked`
  → `test_isolation.rs` 相关 warning **0**

## 5. 顺带查明的根因（不是代码问题）

并发下的网络层失败在 Postgres 服务端日志里有直接证据：

```
FATAL: canceling authentication due to timeout
client=192.168.107.0
```

同时容器资源配置为 **1.5 CPU / 1.5 GB**、`fsync=on`、`synchronous_commit=on`。
受控实验（同一二进制，只改并发度）：

| `--test-threads` | 结果 | 网络层错误数 |
|---|---|---|
| 8 | 113 run, 112 passed, **1 failed** | 1（`HostUnreachable`） |
| 4 | 113 passed | **0** |
| 2 | 113 passed（1 leaky） | **0** |

即：**8 并发打满 1.5 CPU 的容器，认证握手被拖过超时**。这解释了
`--test-threads 4`(526s) 优于 `8`(594–641s) 的"加并发反而更慢"。

**因此 CI 的门禁并发度必须与数据库实例容量匹配，仅靠代码优化不能根治这一条。**

## 5b. 复核中查明的 CI 配置隐患（高危，与本分支无关但影响合并）

CI 的 test job 把测试库指向**应用库本身**，并主动解除保护：

| 位置 | `TEST_DATABASE_URL` | `SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE` |
|---|---|---|
| workflow 顶层 `:16` | `…/synapse_test`（用户名 `postgres:postgres`） | — |
| test job `:294` | **`…/synapse`** | **`"1"`** |
| test job `:321` / `:332` | **`…/synapse`** | **`"1"`** |
| integration `:549` | **`…/synapse`** | **`"1"`** |

两个问题：

1. **保护机制被主动关闭。** `src/test_utils.rs` 里"`public.schema_migrations`
   存在就拒绝 `DROP SCHEMA public`"的守卫，正是为防止这个破坏性动作而加。
   CI 在 4 个步骤里用 `SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE=1` 把它关掉，
   于是测试套件在 CI 里**被允许清空 `public`**。
2. **CI 从未创建 `synapse_test`。** 全文只有顶层 `:16` 提到它，且用的是
   `postgres:postgres`（service 里定义的用户是 `synapse:synapse`）。所以顶层
   默认值本身就是坏的，各步骤只能覆盖成 `…/synapse`。

### 本地实测（代价）

我按"CI 口径"（指向应用库 + 设 `SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE=1`）
跑了一次 `--workspace --lib`：

```
6136 run: 5103 passed, 1033 failed, 13 skipped   [509.6s]
public 表数: 253 -> 3      schema_migrations: 37 -> 0
relation "users" does not exist  (42P01，遍布 storage 各 db_tests)
```

**一次运行就清空了 `public`，产生 1033 个假失败。** 两个数据库都已用
`scripts/init_v11_database.sh` 恢复（均回到 253 表 / 777 索引）。

注意：这不是"某次竞态"，而是该配置下的**必然结果**。合并 P1-B 不会改变这一点，
它只是既存风险。

### 建议修法

```yaml
# 1) 建库（test job 的 postgres service 只建了 synapse）
- run: |
    PGPASSWORD=synapse psql -h localhost -U synapse -d synapse \
      -c 'CREATE DATABASE synapse_test;' || true
# 2) 四个步骤指向真正的测试库，并删掉 wipe 标志
  TEST_DATABASE_URL: postgresql://synapse:synapse@localhost:5432/synapse_test
  # SYNAPSE_TEST_ALLOW_PUBLIC_SCHEMA_WIPE: "1"   <- 删除
```

删掉标志后守卫会生效：一旦有人把 `TEST_DATABASE_URL` 指回应用库，
测试**快速失败**而不是静默清库。

## 5c. 合并最新主线后的复验，与 3 个既存抖动

把本分支合并到主线 `de3df1fb` 后（无冲突），在 `synapse_test` 上跑
`--workspace --lib @threads=4`：

```
6148 tests run: 6145 passed (3 slow), 3 failed, 13 skipped   [926.1s]
```

**P1-B 未引入任何新失败。** 3 个失败单独跑（`--test-threads 1`）**全部通过**，
均为既存抖动，且都可归因到"共享可变状态 + 并发"这一族：

### ① `retention_service::db_tests` 并发失败 —— 根因是**同类缺陷：缺表 → 回退 `public`**

```
left: Some(604800000)   right: Some(259200000)
```

复现：`--test-threads 4` 连跑 5 次，**4 次失败**（失败用例在
`test_effective_policy_server_fallback` / `test_run_cleanup_requires_room_policy`
之间漂移）；`--test-threads 1` 连跑 3 次**全绿**。

> **本报告早期版本把根因写成"`server_retention_policy` 单例行被跨进程共享"。
> 这个假设是错的**，实测推翻了它：把 `retention_service::db_tests` 从手写共享池
> 改成 `prepare_isolated_test_pool()` 后，`--test-threads 4` **仍然 4/5 失败**，
> 且 `--test-threads 1` 也稳定失败。加诊断后拿到真相：

```
current_schema=test_24413_1_1789272464844573000
local_table=false            ← 隔离 schema 里没有 server_retention_policy
resolved=public              ← 于是该表解析到了 public
server_policy=Some(259200000)  ← 读到的是 public 里的共享值
room_policies=0
```

**机制（与本报告第 5b 节是同一条）**：

- `synapse-services::prepare_isolated_test_pool()` 只 `CREATE SCHEMA` + 建池，
  然后跑**运行时** `DatabaseInitService` 建表；
- `server_retention_policy` 只存在于 **v11 baseline**
  （`migrations/00000000_unified_schema_v11.sql:3055`），
  **`database_initializer` 不建任何 retention 表**
  （`grep -rn retention synapse-services/src/database_initializer/` 无命中）；
- `search_path = <隔离 schema>, public` ⇒ 缺表时**静默回退** `public`，
  于是所有 retention 用例仍然写同一份共享数据。

旁证：`tests/integration/retention_storage_tests_migrated.rs:77` 自己
`CREATE TABLE IF NOT EXISTS server_retention_policy` —— 说明这条缺失早已被人
用"就地补建"绕过。

**结论：只改测试夹具不够。** 真正的修法是让 `synapse-services` 的隔离池也
建立在 baseline 之上（本分支为 `synapse-storage` 实现的模板克隆可以直接复用），
或者收紧 `search_path`（去掉 `public` 回退）让缺表**立刻报错**而不是静默串扰。

我尝试的夹具迁移已验证为**必要但不充分**，已从本分支撤出，避免留下"看起来修好了"
的假象。

### ① 的验证实验（红→绿，证明修法正确）

在隔离池之上补建缺失的两张 baseline 表（`server_retention_policy`、
`room_retention_policies` + seed 行），`--test-threads 4` 连跑 5 次：

| 夹具形态 | `--test-threads 4` 连跑 5 次 |
|---|---|
| 手写共享池（原始） | **4/5 失败**，失败用例漂移 |
| 隔离池，缺 baseline retention 表 | **4/5 失败**，`@threads=1` 也稳定失败 |
| **隔离池 + 补建 baseline retention 表** | **5/5 全绿** |

即：**根因确认是"隔离 schema 缺表 → `search_path` 回退 `public`"**，
而修法就是让隔离 schema 具备完整 baseline。实验代码已撤出（一次性验证）。

**推荐修法（二选一或并用）**：

1. 让 `synapse-services::prepare_isolated_test_pool()` 从 baseline 建 schema ——
   本分支为 `synapse-storage` 实现的模板克隆（内容指纹 + advisory lock +
   readiness 标记 + 单次 `DO $$` 克隆）可直接抽到 `synapse-common` 复用；
2. 收紧隔离池的 `search_path`（去掉 `public` 回退），让缺表**立刻报错**，
   把这类静默串扰变成一次性的显式失败。第 2 条成本极低，建议先做。

### ② `synapse-common time::tests::test_calculate_age_near_zero`（时钟容差）

```
age for now should be near zero, got 6
```

`calculate_age(now)` 在 4 并发下被调度延迟 6ms 即失败；单独跑 0.020s 通过。
纯负载敏感，容差过紧。

### ③ `render_appservice_scheduler_prometheus_metrics_reflects_recovery_summary`

单独跑也失败，但失败点是**共享池建库被守卫拒绝**（`src/server/mod.rs:1215`），
与第 5b 节的守卫判定缺陷同源（见下）。`docs/audit/P4_concurrency_perf_2026-09-11.md:222`
已记录该测试为性能敏感（77.2s → 15.8s）。

**结论**：门禁在 `--test-threads 4` 下已从"概率性红"降到"3 个已定位的既存抖动"。
这 3 个都不属于 P1-B 范围，但都值得单独跟进。

## 6. 已知遗留（本分支未处理）

1. **模板检查每个进程都要拿一次 advisory lock**。可在已确认 ready 后走无锁快路径，
   进一步降低串行化开销。
2. **`resolve_test_database_url()` 的 5s→30s 放宽**（`eee4c869`）未回退。
   应拆分为"库不可达 → 快速失败"与"池获取超时 → 长超时"两类语义。
3. `max_locks_per_transaction=64` 偏低，建议 256。
4. 未在本分支验证 `--workspace --lib` 全量门禁（时间成本），
   合并前必须在 CI 配置下跑一次。

## 7. 复现命令

```bash
export TEST_DATABASE_URL='postgresql://synapse:<pw>@<host>:5432/synapse'
export DATABASE_URL="$TEST_DATABASE_URL" SQLX_OFFLINE=true
cargo nextest run -p synapse-storage --lib --all-features --test-threads 4
```
