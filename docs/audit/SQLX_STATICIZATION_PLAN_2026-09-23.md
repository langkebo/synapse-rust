# SQLx 静态化优化方案（2026-09-23）

> **口径与实测**：本文所有数字由 `bash scripts/ci/check_sqlx_dynamic_ratio.sh`
> 与一次同正则的逐文件重测得出，命令与分布见 §1。**这不是"SQL 注入债"**——
> `sqlx::query("… WHERE id = $1").bind(x)` 仍是参数化查询；真正的代价是
> **编译器不再校验 SQL 文本、列名、列类型与可空性**。
>
> 本文是 backlog，不是已完成的结论；引用路径取自当前工作树。

---

## 0. 现状与目标摘要

| 指标 | 当前 | 说明 |
|------|------|------|
| `dynamic` | **2151** | 棘轮上限，不得增加 |
| `static` | **61** | `query!` 34 + `query_as!` 20 + `query_scalar!` 7 |
| 静态占比 | **2.76%** | `61 / 2212` |
| **生产动态（近似）** | **1532** | 首个 `#[cfg(test)]` 之前的行数 |
| **测试基础设施（近似）** | **619** | 其余；多数原理上无法宏化 |
| `.sqlx` 离线缓存 | **60 条** | 部分缓存；CI 部分 job 已 `SQLX_OFFLINE=true`（`ci.yml:532`、`:570`） |
| `format!` 拼 SQL | **147** 处直接 + **9** 处 `let sql = format!` | 主要插值"列清单/排序方向"等标识符 |
| `QueryBuilder` | **14** 个构造点 / 26 处引用 | 合法动态（`push_bind` 仍参数化） |

**目标（现实值，不是 100%）**：把**生产路径、非动态标识符**的查询静态化到 100%，
即 `BASELINE_DYNAMIC_PRODUCTION` 单向降到 0；测试基础设施与 DDL 类动态 SQL 走
书面白名单，不再掩盖生产债务。每批同时下调 dynamic、上调 static。

---

## 1. 实测分布（可复现）

命令（扫描面与棘轮脚本一致：根 `src/` + 各 workspace crate 的 `src/`）：

```bash
bash scripts/ci/check_sqlx_dynamic_ratio.sh
# => dynamic=2151 static=61 total=2212 ratio=0.9724
```

按目录（行计数，镜像脚本正则）：

| 目录 | dynamic | static |
|------|---------|--------|
| `synapse-storage/src` | 1713 | 52 |
| `synapse-e2ee/src` | 175 | 0 |
| `synapse-common/src` | 147 | 0 |
| `synapse-services/src` | 55 | 0 |
| `synapse-test-utils/src` | 33 | 0 |
| `synapse-federation/src` | 22 | 9 |
| `src`（根 crate） | 6 | 0 |
| `synapse-web/src` / `synapse-cache/src` | 0 | 0 |

生产/测试近似切分（按每个文件首个 `#[cfg(test)]` 分界）：
**生产 ≈ 1532，测试基础设施 ≈ 619**。该方法对"测试模块不在文件末尾"或
"用 `#[cfg(any(test, feature = …))]`"的文件有误差，**Phase A 必须换成可复现的
块级扫描并重测**（见 A1）。

生产动态 Top 目标（`dyn / prod`）：

```
127/ 25  synapse-common/src/test_isolation.rs
 76/ 76  synapse-storage/src/event/db_tests.rs      (全为测试)
 65/ 48  synapse-storage/src/room/mod.rs
 56/ 44  synapse-storage/src/device/mod.rs
 50/ 15  synapse-storage/src/captcha.rs
 49/ 43  synapse-storage/src/membership/mod.rs
 48/ 48  synapse-storage/src/server_notification/repository.rs
 45/ 45  synapse-storage/src/user/storage.rs
 41/ 41  synapse-storage/src/space/repository.rs
 41/ 41  synapse-storage/src/application_service/repository.rs
 38/ 35  synapse-storage/src/room/admin.rs
 37/ 37  synapse-storage/src/thread/storage.rs
 32/ 32  synapse-storage/src/saml/repository.rs
 32/ 32  synapse-storage/src/room_summary/repository.rs
 29/ 29  synapse-storage/src/worker/repository.rs
 26/ 26  synapse-storage/src/friend_room/repository.rs
 26/ 26  synapse-e2ee/src/device_keys/storage.rs
```

---

## 2. 影响（为什么值得做）

1. **运行时才暴露的解码/类型错误**。实证：`synapse-storage/src/event/search.rs`
   的 `search_postgres_messages` 用 `f64` 解码 `ts_rank(...)`，Postgres 返回
   `real`(float4) ⇒ 生产 `/search`（postgres provider）必然 `ColumnDecode` 失败。
   `query_as!` 编译期即可拒绝。同类：`bool`↔`i32`、`NOT NULL`↔`Option<T>`、列改名。
2. **迁移风险放大**。改一列要人肉扫 1532 个生产调用点；编译门禁帮不上，
   只能靠 `schema_health_check.rs`（启动期）与 integration（晚而宽）。
3. **手写元组类型静默漂移**。`query_as::<_, (String, i64, Value)>` 不校验列顺序，
   `SELECT *` 加列/换序会错位而不报错。
4. **少了一道针对 `format!` 拼值的护栏**。当前 147+9 处 `format!` 基本用于标识符，
   但没有门禁保证以后不会把可绑定值拼进去。
5. **文档可信度**。只能声明"安全敏感模块已静态化"，不能声明"编译期验证"。

---

## 3. 方案

### Phase A — 让刻度可信（不改查询行为，最高性价比）

**A1. 重写计数器 `scripts/ci/check_sqlx_dynamic_ratio.sh`**
- 计数前**剥掉注释与字符串字面量**（现在 `grep -vE ':.*//!|:.*///'` 只挡行内
  doc comment，散文里的 `sqlx::query(` 仍会被计入 —— 基线文件已登记此缺陷）。
- 统计**出现次数**而非匹配行数（`wc -l` 会漏同一行两处）。
- 覆盖 turbofish（`query_as::<…>(`、`query_scalar::<…>(`）与
  `QueryBuilder`（单列，不计入 static/dynamic，只报数）。
- **按 production / `#[cfg(test)]` 分区计数**：逐字符扫描，`#[cfg(test)]` 之后
  进入 test 区；`#[cfg(any(test, feature = …))]` 归入 test 区。输出
  `dynamic_production=… dynamic_test=… static=…`。
- 明确排除与包含面（保持现行：`tests/`、`benches/`、`artifacts/` 不入扫描）。

**A2. 基线文件改为三键**（`scripts/ci/sqlx_dynamic_ratio_baseline`）
`BASELINE_DYNAMIC_PRODUCTION`、`BASELINE_DYNAMIC_TEST_INFRA`、`BASELINE_STATIC`；
初值取 A1 重测结果（当前近似值 1532 / 619 / 61）。保留该文件既有的
"每次调整写理由"体例。

**A3. `.sqlx` 新鲜度门禁**
- 新增 `scripts/ci/check_sqlx_cache_fresh.sh`：`cargo sqlx prepare --workspace`
  后 `git diff --exit-code -- .sqlx`。
- 规则写进 CONTRIBUTING/README：**新增任何 `query!` 必须同 PR 提交缓存**，
  否则 `SQLX_OFFLINE=true` 的 job 会 `no cached data`。
- 与 A1 的 `--all-features` / `#[cfg(test)]` 口径问题一并说明（见 §4 陷阱）。

**A4. 门禁红证明**（新增 `tests/unit/sqlx_ratchet_guard_tests.rs`）
- 插入 `sqlx::query(` → 脚本必须 FAIL；
- 插入 `sqlx::query!` → static 必须 +1；
- 把 `sqlx::query(` 写进注释/字符串 → **不得**计数；
- 删除一列名（临时 migration）→ 已静态化的模块**编译必须红**。

**A5. `format!` 拼值守卫**
1. 先出**审计清单**：147 处 `query*(&format!` + 9 处 `let sql = format!` +
   14 处 `QueryBuilder`，逐处标注插值内容（标识符/常量/排序枚举 vs 绑定值）。
2. 再加 `tests/unit/sqlx_format_guard_tests.rs`：命中 `format!` 作为 SQL 文本且
   插值参数不在 allowlist 时 FAIL；允许的插值必须带
   `// sqlx-format-allow: <reason>` 标记（**不用行号型白名单**，会随
   `cargo fmt` 漂移）。

### Phase B — 冻结新增（规则先于迁移）

- **B1** CI 判定改为：`dynamic_production` 不得增加、`static` 不得减少；
  `dynamic_test_infra` 增加必须逐条写理由。
- **B2** 规则文档：新增 storage/service 代码必须用 `query!`/`query_as!`/
  `query_scalar!`；动态仅限 DDL、动态标识符、`= ANY($1)`、`QueryBuilder`。
- **B3** 立即回收死查询（0 调用者，已实测）：
  `get_latest_events_for_rooms`、`get_room_message_counts_batch`、
  `get_events_since_stream_ordering`、`get_room_events_by_stream_range`；
  并按铁律 1 评估删除 `synapse-storage/src/search_index.rs`（整模块无生产调用者，
  其 `let sql = format!` ×2 与若干 query 一并回收）。
- **B4** 目标值：`dynamic_production` 从实测起点单向降，不接受"持平"。

### Phase C — 按"风险 × 改动量"分批静态化（每批一个 PR）

排序原则：安全敏感 > 手写元组类型 > 数值/布尔/可空列 > 其余。

| 批次 | 目标模块 | 生产动态 | 理由 |
|------|----------|----------|------|
| C1 | `synapse-storage/src/user/storage.rs` | 45 | 身份/停用/管理员判定 |
| C2 | `synapse-storage/src/device/mod.rs` | 44 | E2EE 设备与一次性密钥 |
| C3 | `synapse-storage/src/membership/mod.rs` | 43 | 成员/权限 |
| C4 | `synapse-storage/src/refresh_token/mod.rs`、`token.rs` | 增量 | 令牌（已是静态化样板，补齐同模块剩余） |
| C5 | `synapse-storage/src/openid_token.rs` | 7 | 令牌 |
| C6 | `synapse-storage/src/server_notification/repository.rs` | 48 | 面广 |
| C7 | `synapse-storage/src/space/repository.rs`、`application_service/repository.rs` | 41 + 41 | 面广、元组多 |
| C8 | `synapse-storage/src/room/mod.rs`、`room/admin.rs` | 48 + 35 | 热路径 |
| C9 | `synapse-storage/src/saml/repository.rs`、`room_summary/repository.rs`、`thread/storage.rs`、`worker/repository.rs` | 32/32/37/29 | 面广 |
| C10 | `synapse-e2ee/src/device_keys/storage.rs` 等 E2EE 存储 | 26+ | E2EE 安全 |
| C11 | `synapse-common/src/test_isolation.rs` | 25 | 待 A1 分区后重估 |

**每批验收判据（缺一不可）**
1. `SQLX_OFFLINE=true cargo check --workspace --all-features --locked` 通过
   （使用提交的 `.sqlx`）；
2. **漂移红证明**：临时改一列名 → 该模块编译 FAIL，改回后通过；
3. 该模块 integration 通过；
4. 同步 `BASELINE_DYNAMIC_PRODUCTION` 下调、`BASELINE_STATIC` 上调。

### Phase D — 结构性收敛（收益最大、需设计）

- **D1 typed repository 层**：为高频聚合引入少量 `query_as!` 函数
  （如 `EventRow::page`、`UserRow::by_id`、`DeviceRow::list_for_user`），
  调用点改走它 —— 同时减少调用点数量与漂移面，符合铁律 2。
- **D2 测试夹具收敛**：`test_isolation` / `test_utils` 的多份副本合并
  （基线文件记录有 3–4 份），能搬到 `tests/` 的探针搬走（不入扫描面）。
- **D3 固化"必须动态"白名单**：DDL、`CREATE/DROP SCHEMA`、`set_config`、
  故障注入、`pg_*` catalog 探针、动态标识符。

---

## 4. 陷阱与反例（仓库已踩过）

- **计数器不看注释**（基线文件登记的 +1 假增长）：别再用行计数。
- **`#[cfg(test)]` 内的宏不进 `cargo sqlx prepare`**，`--all-targets` 又会因测试
  目标缺 feature 报 E0432 ⇒ 测试夹具**不能**强行宏化（已实测）。
- **行号型 allowlist 会随 `cargo fmt` 漂移**（`scripts/shell_routes_allowlist.txt`
  前车之鉴）⇒ 用标记注释/函数级匹配。
- **不要为降计数删掉刻意的双向断言探针**（"存在/不存在"两次查询是刻意设计）。
- **动态标识符无法宏化**：列清单用常量（如 `ROOM_EVENT_COLS`），`IN (…)` 用
  `= ANY($1)`，排序用枚举分支 `format!` + 白名单标记。

---

## 5. 工作量与顺序

| 阶段 | 量级 | 风险 | 前置 |
|------|------|------|------|
| A（刻度可信） | 小 | 无行为变更 | 无 |
| B（冻结新增） | 极小 | 无 | A |
| C（分批迁移） | 每批 20–50 处，机械 | 低（有红证明） | A、B |
| D（结构收敛） | 中 | 需设计评审 | A |

**建议顺序**：A → B → B3（回收死查询）→ C1–C3（安全敏感）→ C6–C10 → D。
每个 C 批次独立 PR、独立降基线，禁止大爆炸式一次重写 1532 处。

---

## 执行结果（2026-09-23）

> 本节记录 Phase A / B / B3 / C1–C10 / D1 的实测结果、可核验提交与**残差债务**。
> 所有计数由 `python3 scripts/ci/sqlx_query_census.py`（`check_sqlx_dynamic_ratio.sh`
> 的唯一计数实现）同源产出；每批的基线调整理由与逐条覆盖（nullability 覆盖、
> `&Option<T>` → `.as_deref()`、`AS "col!"` 别名等）写在
> `scripts/ci/sqlx_dynamic_ratio_baseline` 的对应段落，本节只列数字、提交与结论。
>
> 口径：`dynamic_production` = `#[cfg(test)]` 块外、且不由 `#[cfg(test)] mod x;`
> 引入的文件里的动态 `sqlx::query*` 调用；`static` = `query!`/`query_as!`/
> `query_scalar!`/`query_file!` 宏站点（当前全部落在生产侧，`static_test = 0`）。

### 1. 批次表

| 阶段 / 批次 | 目标 | `dynamic_production` | `static` | 提交（短哈希，可 `git log -1 --format=%H <subject>` 核验） |
|---|---|---|---|---|
| **A**（刻度可信） | A1 计数器重写为 `sqlx_query_census.py`（分区 + 词法剥离 + 按出现次数）；A2 基线改三键；A3 `.sqlx` 新鲜度门禁；A4 棘轮守卫测试 | 1532（旧近似）→ **1455**（实测） | 61 | `2e9c3d11d`（A1/A2/A3）；`d7742bdf9`（A4 守卫自修，16 项全绿） |
| **B**（冻结新增） | B1 棘轮语义改为「生产动态不得增、静态不得减」；B2 规则文档；B4 单向收紧 | 1455 | 61 | `2e9c3d11d` |
| **B3**（回收死查询） | 删除 4 个 0 调用者死查询：`get_latest_events_for_rooms`、`get_room_message_counts_batch`、`get_events_since_stream_ordering`、`get_room_events_by_stream_range` | 1455 → **1451** | 61 | `2e9c3d11d` |
| **C1** | `synapse-storage/src/user/storage.rs` | 1451 → **1411**（自身 -42；另 1 处 `search_directory_users` 运行期回退，基线按实测收到 1410） | 61 → **103** | `84613d811` |
| **C2** | `synapse-storage/src/device/mod.rs` | 1411 → **1367** | 103 → **147** | `a25bb7b41`（⚠️ 被并发会话的 `git add -A` 卷入其「MSC3912 cascade redaction」提交，故文件名与提交信息不符） |
| **C3** | `synapse-storage/src/membership/mod.rs` | 1367 → **1328**（-39）；门禁实测 1329（并发在途 `search_index.rs` +1） | 147 → **185**（+38：39 调用点 / 38 宏站点） | `90fe9c8a4` + `7faf3f5a7` |
| **C6** | `synapse-storage/src/server_notification/repository.rs` | 1329 → **1281**（-48） | 185 → **233** | `1b90c6b07`（主体）+ `0a96478a1`（LEFT JOIN 可空性修正）+ `6825dbd60`（.sqlx + 收紧） |
| **C7** | `synapse-storage/src/space/repository.rs` + `application_service/repository.rs` | 1281 → **1200**（-80） | 233 → **313** | `c8871fe76` + `0af599d40` + `eaaf829f9` / `d926cb271`（.sqlx + 收紧） |
| **C8** | `synapse-storage/src/room/mod.rs` + `room/admin.rs` | 1200 → **1117**（-83） | 313 → **396** | `0e1716643` + `6741e8e51` |
| **C9** | `saml/repository.rs` + `room_summary/repository.rs` + `thread/storage.rs` + `worker/repository.rs` | 1117 → **990**（-127） | 396 → **523** | `cbe718ff6`（saml）/ `3cee36c30`（room_summary）/ `b9c2f52a7`（thread）/ `30d9a7f91`（worker）+ `0d57b807d`（可空性加固）+ `69d7a3363`（.sqlx + 收紧） |
| **C10** | `synapse-e2ee/src/device_keys/storage.rs` | 990 → **964**（-26） | 523 → **549** | `1157099d4` + `4ff9b62d7` |
| **D1**（本次） | 生产区**字面量**动态 SQL 棘轮守卫（新增 census 模式 + 守卫测试 + 基线） | 964（不变） | 549（不变） | `23eeef31f` |

**累计（A 之后 → C10 收口）**：

| 指标 | 起点 | 终点 | 变化 |
|---|---|---|---|
| `dynamic_production` | **1451** | **964** | **-487（-33.6%）** |
| `static` | **61** | **549** | **+488** |
| `dynamic`（总） | 2147 | **1660** | -487 |
| `dynamic_test` | 696 | **696** | 0（测试夹具按 §4 陷阱不得宏化，未被静默计入生产） |
| 静态占比 | 2.76%（61/2212） | **24.9%（549/2209）** | +22.1pp |

> ⚠️ 逐批 Δ 之和不严格等于累计值：C1/C3/C6 的基线按**门禁实测值**设定，而同期并发
> 会话的在途改动（`event/cascade.rs` +2、`search_index.rs` +1）曾一度计入基线；
> C3 的 39 个动态调用点只对应 38 个宏站点。以 `sqlx_dynamic_ratio_baseline`
> 各段的实测数字为准。

**门禁命令（唯一实现，全部可复现）**：

```bash
# 1) 计数与分区（本次 D1 新增 --list-production-dynamic 模式）
python3 scripts/ci/sqlx_query_census.py            # key=value 摘要（含生产/测试分区）
python3 scripts/ci/sqlx_query_census.py --json     # 机器可读
python3 scripts/ci/sqlx_query_census.py --list-production-dynamic . | grep ':literal$'
# 2) 棘轮：生产动态不得增、静态不得减（唯一入口，内部调用上面的 census）
bash scripts/ci/check_sqlx_dynamic_ratio.sh
# 3) .sqlx 离线缓存新鲜度：cargo sqlx prepare --workspace 后 git diff --exit-code -- .sqlx
bash scripts/ci/check_sqlx_cache_fresh.sh
# 4) 守卫测试
cargo nextest run --test unit sqlx_ratio_gate_tests
cargo nextest run --test unit sqlx_dynamic_literal_guard_tests
```

### 2. DDL 结论修正（C10 学到）

本文 §4 与旧基线曾写「DDL 不可用 `query!` 静态化」，**该结论需收窄**：生产 DDL
**可以**静态化。C10 把 `device_keys/storage.rs` 的 4 处 `CREATE TABLE` /
`CREATE INDEX` 转成 `query!` 并通过真实 DB 校验（`SQLX_OFFLINE=false cargo check
-p synapse-e2ee --all-features` + worktree 内运行期实跑 `create_tables()`）。机制：

- Postgres 的 SQL 层 `PREPARE` 只接受 SELECT/INSERT/UPDATE/DELETE/MERGE/VALUES，
  但 sqlx 走的是**扩展查询协议**的 Parse/Describe —— utility statement 被接受，
  Describe 返回 `NoData`；
- `sqlx-macros-core` 的 `query!` 在「输出列全为 void」（`all(|it|
  it.type_info().is_void())`）时退化为普通 `Query`，故 `query!(...).execute(...)`
  正常工作；4 条 DDL 缓存条目均为 `"columns": []` / `"parameters": {"Left": []}`，
  离线可用。

**「DDL 不可静态化」现在只适用于 `#[cfg(test)]` 内的宏**：`cargo sqlx prepare`
（默认 target/feature 集）不收集 `#[cfg(test)]` 中的宏，`--all-targets` 又会因测试
目标缺 feature 报 E0432，因此测试夹具里的建表/故障注入 DDL 仍必须保持动态。

### 3. 残差动态清单（生产区 964 处）

`--list-production-dynamic` 把 964 处逐条分成两类：**876 处 `literal`（字面量）**
与 **88 处 `runtime`（实参不是字符串字面量）**。876 处 `literal` 分布在 98 个文件，
已作为 **D1 棘轮基线**逐文件登记（`scripts/ci/sqlx_literal_production_baseline`）；
它们主要是**从未进入 C1–C10 迁移范围**的生产模块（`friend_room/repository.rs` 26、
`module.rs` 25、`sliding_sync/repository.rs` 22、`registration_token`/`media_quota`/`cas`
各 20、`event_report`/`beacon` 各 19、`threepid`/`push_notification`/
`background_update`/`admin_federation`/`key_rotation`/`backup` 各 18 …），
属于 C11+ 的工作量，不在 Phase D（结构性收敛）范围内。

以下只详列 **88 处 `runtime`**（Phase B2 明确允许的残差类别）与其中的分类器盲区。

#### 3.1 真正运行期拼装（41 处 / 12 文件）

| 文件 | 站点（`path:line`） | 为什么静态化不了 / 收紧方向 |
|---|---|---|
| `src/server/database.rs` | 43, 44, 45 | `SET statement_timeout = '<n>s'` 等 3 条 GUC 设置。PG 的 `SET` 是 utility statement，**不接受绑定参数**，超时值只能 `format!` 内联（值域由 `format_pg_timeout` 钳死为 `'<int>s'`）。收紧方向：GUC 名与单位本就固定，可在 `after_connect` 里按 3 个已知值分支出静态 SQL —— 属独立的小重构。 |
| `synapse-common/src/transaction.rs` | 66 | 通用事务执行器 `run_in_transaction(statement: &str)`，SQL 文本由**调用方**传入；静态化等于删除这个 API（其调用者各自静态化后才可回收）。 |
| `synapse-storage/src/event/basic.rs` | 91, 112, 129 | `sqlx::query_as(&format!(…))`：拼 `ROOM_EVENT_COLS` 列清单 + 可选 `WHERE` 片段。列清单本就是常量，可下沉进静态 SQL 字面量；条件片段可改 `$n::text IS NULL OR …`。 |
| `synapse-storage/src/event/batch.rs` | 13, 30 | 同上（`ROOM_EVENT_COLS` + `origin_server_ts`/`stream_ordering` 两种排序）。 |
| `synapse-storage/src/event/pagination.rs` | 18, 33, 47, 62, 91, 311, 329, 343, 362 | 同上；含分页游标方向与 `ORDER BY` 方向。收紧方向：方向用 `CASE` 或两条静态 SQL 表达。 |
| `synapse-storage/src/event/state.rs` | 26, 45, 68, 93, 123, 156, 189, 214, 260 | 状态事件查询的列清单/条件拼装。 |
| `synapse-storage/src/state_groups.rs` | 329, 390 | 列清单与多表 `DELETE` 的**表名集合**（动态标识符）。 |
| `synapse-storage/src/membership/mod.rs` | 752, 764, 776, 787 | `get_room_members_paginated_with_profiles` 的 4 种游标分支（`not_membership` ± `from_user_id`、有/无游标）用 `format!` 拼 `WHERE`/`ORDER BY`；收紧方向已在 C3 段登记（`$n::text IS NULL OR …` + `CASE` 排序）。 |
| `synapse-storage/src/space/repository.rs` | 572, 626 | `search_spaces` 的 `TrigramRanking` 相似度表达式在运行期拼装（`&sql`）；收紧方向：固化为两条静态 SQL。 |
| `synapse-storage/src/user/storage.rs` | 990, 1234 | `search_users` / `search_users_with_presence` 的 `&sql`（`format!` 拼 `WHERE`/`ORDER BY`）；C1 段已登记。 |
| `synapse-storage/src/maintenance.rs` | 96, 140 | `VACUUM ANALYZE {table}` / `REINDEX INDEX {index}`：**标识符**（表名/索引名）无法绑定，且 `VACUUM`/`REINDEX` 不接受参数 —— 属「必须动态」白名单（对应 §3 Phase D 的 D3 白名单立项）。 |
| `synapse-storage/src/search_index.rs` | 161, 179 | `let sql = format!(…)` 两条。该模块**全仓无生产调用者**（B3 已登记按铁律 1 整体删除，仅被配置字段 `search_index_name` 与测试引用同名表），删除即回收 2 处 —— 应删除而非静态化。 |

#### 3.2 测试基础设施（无条件编译，32 处 / 3 文件）

| 文件 | 站点 | 说明 |
|---|---|---|
| `synapse-common/src/test_isolation.rs` | 593, 606, 613, 628, 674, 677, 686, 690, 694, 707, 714, 725, 1730, 1731, 1758, 1790 | 该模块在 `synapse-common/src/lib.rs:92` 被**无条件编译**（注释自述 "Compiled unconditionally"），故按口径计入生产区。SQL 文本含 schema 名/模板名等动态标识符（`CREATE/DROP SCHEMA {schema}`、`pg_namespace` 探测、跨 schema 外键修复），标识符不可绑定。 |
| `synapse-common/src/test_schema_guard.rs` | 288, 345 | 同上（schema 守卫基建）。 |
| `synapse-test-utils/src/lib.rs` | 429, 430, 487, 680, 755, 779, 943, 947, 1064, 1344, 1719, 1800, 1821, 1843 | `synapse-test-utils` 是独立 crate，其 `src/` 在 `SCAN_DIRS` 内；同型动态标识符。 |

> 这 32 处「生产区」其实是测试基建。若把它们移出扫描面（例如要求 `test_isolation`
> 走 `#[cfg(any(test, feature = "test-utils"))]` 门控），`dynamic_production` 可直接
> 再降 32 —— 这属于 D2（测试夹具收敛）的结构性收益，见 §4。

#### 3.3 分类器盲区：名义 `runtime`、实为字面量文本（15 处）

| 文件 | 站点 | 实参 | 说明 |
|---|---|---|---|
| `synapse-storage/src/event/create.rs` | 23, 36, 94, 112, 121, 139, 210, 229, 237, 252, 271, 279 | `query` / `insert_event_query` / `insert_edges_query` / `insert_room_edges_query` / `insert_state_edges_query` | 全部是**同文件 `let … = r"…"` 局部字面量绑定**（L13/L86/L182/L194/L202）。守卫只看实参 token 形态，故判为 `runtime`。收紧方向：把 SQL 直接写进 `query!` 调用点（宏要求字面量在调用点，`let` 绑定不满足），可回收 12 处。 |
| `synapse-storage/src/presence/mod.rs` | 274, 305 | `const PRESENCE_SELECT_BY_USER: &str`（L21 字面量） | 同型。 |
| `synapse-storage/src/event_report/repository.rs` | 319 | `const REPORT_RATE_LIMIT_SELECT_FOR_UPDATE: &str`（L16 字面量） | 同型。 |

> ⚠️ **这是 D1 守卫的已知假阴性**：`let sql = "SELECT …"; sqlx::query(&sql)` 会被判为
> `runtime` 从而绕过棘轮。当前**刻意不做**「同文件 const/let 字面量绑定」解析，因为
> 本守卫是**棘轮**（基线已锁住这 15 处）且任务书把残差类别定义为
> `&sql` / `&query` / `&format!(…)` 的运行期实参；把它记为收紧方向而不是隐藏。
> 实现要点：在 `iter_dynamic_sites` 里对 `path` 建立 `name → 是否字面量绑定` 表，
> 实参为裸标识符时查表即可（15 处会据此从 `runtime` 变 `literal`，基线相应 +15）。

#### 3.4 C1–C10 期间发现的死代码 / 缺陷方法（**不在 964 残差计数内**，但仍然存在）

| 符号 | 位置 | 结论 | 证据 |
|---|---|---|---|
| `get_rooms_with_member_counts` | `synapse-storage/src/room/mod.rs:1393` | **0 调用者**（全仓只有定义与 `/// See [...]`），按铁律 1 应删除；非 trait 方法，无动态分发路径 | `grep -rn 'get_rooms_with_member_counts' --include=*.rs .`（排除 `/target/`）仅命中 `room/mod.rs:1392-1393` |
| `WorkerStorage::get_statistics`（SQL 版） | `synapse-storage/src/worker/repository.rs:707`（函数定义 `:693`） | SELECT 里 **12 个列不存在**：`worker_name`/`worker_type`/`status`/`host`/`port`/`last_heartbeat_ts`/`started_ts`/`cpu_usage`/`memory_usage`/`active_connections`/`requests_per_second`/`average_latency_ms`/`queue_depth`/`pending_commands`/`active_tasks`；`worker_statistics` 实际只有 `id, worker_id, total_messages_sent, total_messages_received, total_errors, last_message_ts, last_error_ts, avg_processing_time_ms, uptime_seconds, created_ts, updated_ts`。代码内 doc 引用的迁移 `20260812120000_worker_statistics_load_metrics` **不在 `migrations/`**。C9 有意保留动态（`query!` 会在编译期拒绝），未修。它是 `worker/repository.rs` **唯一残留**的动态站点（`--list-production-dynamic` 报 `:707:literal`，已进 D1 基线） | `migrations/00000000_unified_schema_v12.sql` 的 `worker_statistics` DDL；`repository.rs:693` 起的 `NOTE(C9)`；⚠️ `NOTE(C9)` 自述 "No in-tree callers" **不准确**：`impl WorkerStoreApi for WorkerStorage`（`worker/api.rs:93`/`:227`）把它挂上 trait，`WorkerManager::get_statistics`（`synapse-services/src/worker/manager.rs:663`）消费它，生产装配见 `synapse-services/src/wiring/admin.rs:384`，路由为 `/_synapse/worker/v1/statistics`（`synapse-web/src/routes/worker.rs:695`）⇒ 该端点运行必然 `42703`。**`[未验证：未实跑该路由/该 SQL]`** |
| `RoomSummaryStorage::add_members_batch` / `set_states_batch` | 定义 `room_summary/repository.rs:296` / `:564`；动态站点 `:332` / `:579` | 绑定**逐元素可空数组** `Vec<Option<String>>` / `Vec<Option<i64>>`：sqlx-postgres 的 `param_type_for_id` 只注册了非空元素数组（`Vec<String>`/`&[String]`），**没有 `Vec<Option<T>>` 映射**，`query!` 以 E0308 拒绝。注意这两处的 **SQL 文本是字面量**（故在 D1 基线里按 `literal` 登记，`room_summary/repository.rs = 2`），动态的只是绑定参数类型。收紧方向：七个并行数组换成 `jsonb_to_recordset($n)` 单参形态 | 代码内 `NOTE(C9)`；`--list-production-dynamic` 报 `room_summary/repository.rs:332:literal`、`:579:literal` |
| `DeviceKeyStorage::create_tables` | `synapse-e2ee/src/device_keys/storage.rs:233` | 手写 DDL **缺 `fallback_used` 列**（权威 schema `migrations/00000000_unified_schema_v12.sql:673` 有该列，`:3423` 的部分索引 `idx_device_keys_fallback` 还依赖 `fallback_used = FALSE`），与真源不一致；且**0 调用者**（全仓无 `.create_tables(`）。建表真源是 `migrations/`，按铁律 1 该方法应删除 | `grep -rn '\.create_tables(' --include=*.rs .` 无命中；`grep -rn fallback_used` 见上 |
| `search_index` 模块 | `synapse-storage/src/search_index.rs` | B3 已登记：无生产调用者，按铁律 1 应整体删除而非静态化。该模块共 **8 处**生产动态：6 处 `literal`（106, 216, 223, 233, 268, 271，已进 D1 基线）＋ 2 处 `runtime`（161, 179，`let sql = format!` ×2，即 §3.1 末行）—— 删除即一次回收 8 处 | 全仓唯一的模块引用是 `synapse-storage/src/sync/mod.rs:10` 的 `pub use crate::search_index::{…}` **再导出**，而该再导出本身无人消费：`SearchIndexStorage` 的全仓外部引用数 = 0（`grep -rn '\bSearchIndexStorage\b'` 排除自身与 `sync/mod.rs` 后无命中） |

> 上表每条均已用 `grep` / 读源码核验；唯一标注 `[未验证]` 的是
> `worker/get_statistics` 端点的**运行期**失败（未连库实跑，仅静态读码 + 迁移列对照）。
> 本节未发现其他无法核验的条目。

### 4. Phase D 其余两项的状态

**D2 status（测试夹具收敛）——未做，仍立项。**
范围：`test_isolation` / `test_utils` 的**多份副本**合并（根 crate `src/test_utils.rs`、
`synapse-common/src/test_isolation.rs`、`synapse-services/src/test_utils.rs`、
`synapse-storage/src/test_utils.rs`、`synapse-storage/src/test_isolation.rs`；基线文件
已登记 3–4 份分叉，并留有「同一类 schema 泄漏 bug 要修三次」的事故记录），以及把能
搬到 `tests/` 的探针搬出扫描面。**为什么本次没做**：它是 schema 生命周期/隔离机制的
结构性重构，需要先证明合并后所有走该夹具的测试仍然隔离且不泄漏 schema（本机已残留
上千个 test schema），风险与工作量都远超「Phase D 限定范围」；且本次 D1 只是加门禁、
不改运行期语义。**第一步（具体、可独立提交）**：给 5 个副本做逐符号对照表
（`prepare_isolated_test_pool` / `prepare_shared_test_pool` / `init_template_schema` /
`drop-on-release` 登记表 / `PENDING_SCHEMA_RETURNS` / `SCHEMA_POOL` / `CLEANUP_RUNTIME`），
确认哪份是超集、哪份缺清理路径，并在 `tests/unit/` 加一个「副本漂移守卫」测试
（对同一职责的多个实现做函数签名/行为指纹比对），使「再出现第四份副本」立刻变红。
合并本身（删掉冗余副本、全部 crate 改走 `synapse-common` 唯一实现）作为后续 PR。

**D3 status（typed repository 层）——未做，仍立项。**
范围：为高频聚合引入少量 `query_as!` 函数（如 `EventRow::page`、`UserRow::by_id`、
`DeviceRow::list_for_user`），调用点改走它 —— 同时减少调用点数量与漂移面（铁律 2）。
**为什么本次没做**：它改变数据访问的**接口形状**（新增/迁移 repository API），需要设计
评审与逐域回归，属行为重构而非机械静态化；且 §3.3 的 15 处「名义 runtime」与 §3.1 的
41 处格式串拼装应当由这一层统一吸收，先立项再动。**第一步（具体、可独立提交）**：
选 `synapse-storage/src/event/pagination.rs` 作为试点（9 处格式串拼装，是最集中的单一
热点），把「列清单 + 游标 + 排序方向」收敛为一个 `EventPageQuery` 类型 + 2–3 条静态
`query_as!`，用现有 `event::db_tests` 的往返用例做等价性证明（含边界：空游标、
`origin_server_ts` 同毫秒并列，参考 C9 的决胜键教训），并把回收的 9 处写入
`sqlx_dynamic_ratio_baseline` 与 `sqlx_literal_production_baseline` 的同一提交。

> 注：本文 §3 把 D1 定为 typed repository、D3 定为「必须动态」白名单；本次执行把
> 门禁类工作（原 D3 方向的守卫）作为 D1 落地，故上文按任务书口径把 typed repository
> 记为 **D3 status**（对应本文 §3 的 D1）。「必须动态」白名单（DDL / `CREATE/DROP
> SCHEMA` / `set_config` / 故障注入 / `pg_*` 探针 / 动态标识符）已由 §3.1 的
> `maintenance.rs`、§3.2 的测试基建条目与 D1 基线的 `runtime` 分类部分覆盖，
> 但**尚未**固化成显式白名单文件 —— 保留为后续条目。
