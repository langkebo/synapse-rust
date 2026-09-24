# SQLx 静态化优化方案（2026-09-23）

> **口径与实测**：本文所有数字由 `bash scripts/ci/check_sqlx_dynamic_ratio.sh`
> 与一次同正则的逐文件重测得出，命令与分布见 §1。**这不是"SQL 注入债"**——
> `sqlx::query("… WHERE id = $1").bind(x)` 仍是参数化查询；真正的代价是
> **编译器不再校验 SQL 文本、列名、列类型与可空性**。
>
> 本文是 backlog，不是已完成的结论；引用路径取自当前工作树。
>
> **执行优先级（2026-09-23 重排）**：C 批次（逐文件静态化）**暂停**，当前执行顺序以
> **§8 问题优先处理计划**为准——先修完 §7 登记的既有缺陷，再恢复静态化。§5 的阶段/顺序表
> 与「执行结果」的批次表保留为**历史记录**，不再是当前排期。

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
| C11 | `synapse-storage/src/friend_room/repository.rs`（**更正**，见 §7 D-16） | 26 | 从未进入 C1–C10 的生产模块；原写 `synapse-common/src/test_isolation.rs`（25）系误标，该模块属 D2 测试夹具收敛 |

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

> **历史记录（2026-09-23 重排）**：本节的阶段拆分与建议顺序是 A/B/C/D 规划期的依据；C 批次
> 现已暂停，**当前执行优先级见 §8**（先修 §7 的既有缺陷，再恢复静态化）。下表与其后的批次表
> 仅作历史记录保留 —— 不复用为当前排期。

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

> **历史记录**：本表记录 A–C18 的已执行批次，作为可复现的批次史保留；C19+ 在 §8 的
> 问题修复波次完成前不再新增（恢复条件见 §8.5）。

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

---

## 7. 优化过程发现的既有缺陷统一登记表（2026-09-23 汇总）

> **本节是这些发现的唯一汇总处。** 静态化（C1–C15）、worker S1–S4、Phase A/B/D 期间
> 挖出的既有缺陷 / 限制 / 覆盖缺口，此前散落在批次作者就地留下的 `NOTE(Cxx)` 注释、
> `scripts/ci/sqlx_dynamic_ratio_baseline` 的分批段落，以及本节之前的 §3.4 零散表格里。
> 自本节起，**后续每个批次（C16+ 及任何收尾工作）发现的既有问题一律追加到本节**，
> 不要在 baseline 里再开第二份清单。
>
> `scripts/ci/sqlx_dynamic_ratio_baseline` **仍然是棘轮口径的唯一记录**
> （`dynamic_production` / `static` 两个数字与每批的计数理由）；本节只登记**问题本身**，
> 不重复棘轮数字。
>
> **范围声明**：以下条目**都不在静态化范围内**——静态化是行为保持的重构，"把动态
> `query` 换成 `query!`"既不修这些缺陷、也不允许顺手改行为（改了就无法用编译期红证明
> 来验证等价性）。每条都需要**独立评审 + 独立提交**。棘轮（D1 守卫）与 baseline
> **不阻塞**这些条目的处理，反之亦然：处理它们时不要求同时改棘轮数字，除非确实回收了
> 动态站点。
>
> 计数口径：`dynamic_production=706`（C18 后）、`static=808`、`dynamic_test=704`、
> `query_builder=18`（`python3 scripts/ci/sqlx_query_census.py` 实测）。
> ⚠️ 本行此前写作 `dynamic_production=741` / `static=773` 并标注"C17 后实测"——
> 741/773 实为 **C16 后**的数值（C17 为 773→742 / 741→772，与 baseline 的
> `BASELINE_DYNAMIC_PRODUCTION=742` / `BASELINE_STATIC=772` 一致），两处各偏 1。
> C18 一并更正并登记为 D-35。

### 7.1 汇总表

| ID | 类别 | 位置 | 症状（一句话） | 状态 | 影响/可达性 | 建议处理 |
|---|---|---|---|---|---|---|
| D-01 | 产品缺陷 | `synapse-storage/src/room/mod.rs:1393` | `get_rooms_with_member_counts` 原 `WHERE … LEFT JOIN …` 语法非法（42601），查询完全无法执行；且零调用者 | 已修语法（`0e1716643`）；**未修**死函数 | 无（0 调用者，非 trait 方法） | 按铁律 1 删除整个函数 |
| D-02 | 产品缺陷 | `synapse-storage/src/saml/repository.rs:574` | 登出写 `processed_ts`，真列名 `processed_at`（42703），登出路径必然失败 | **已修**（`cbe718ff6`） | 有（`saml_service.rs:482`，saml-sso） | — |
| D-03 | 产品缺陷 | `synapse-storage/src/worker/repository.rs:757` | `get_statistics` 选了 15 个两张表都不存在的列（42703），端点从未返回过任何行 | **已修**（`0f6a76c13` + S1–S3 `483dfc045` / S4 `14eab2283`,`0e0af49d0`） | 有（`/_synapse/worker/v1/statistics`，`worker.rs:695`） | — |
| D-04 | 产品缺陷 | `synapse-e2ee/src/device_keys/storage.rs:233` | `create_tables()` DDL 缺 `fallback_used`，fallback 三分支都读写它（42703） | **未修**（潜伏） | 无（0 调用者；schema 由迁移拥有） | 按铁律 1 删除该方法 |
| D-05 | 数据一致性 | `synapse-e2ee/src/device_keys/models.rs:14`（结构体，字段已删） | `DeviceKey.id` 恒为 0（无任何查询投影 `id`，`into_device_key` 硬编码 `0`） | **已修**（W2 `cef006dd2`，按铁律 1 删字段） | 生产不读；全仓消费方只有同 crate 的 `test_mocks.rs` | 已修：删除 `DeviceKey.id`，7 处构造点的伪造 `0`/`1` 与 2 处断言一并删除；键由 `(user_id, device_id, algorithm, key_id)` 标识 |
| D-06 | 文档一致性 | `synapse-e2ee/src/device_keys/storage.rs:14-93` | `DeviceKeyRow` 每个字段前重复 "The `x` field." 行，注释错乱 | **未修**（cosmetic） | 无 | 一次性清理注释 |
| D-07 | 数据一致性 | `synapse-e2ee/src/device_keys/storage.rs:301`（impl）、`:151`（trait） | `record_device_list_change_best_effort` 完全吞错（`let Ok(..) else { return }` / `let _ =`），`stream_id` 插入失败对调用方不可见 | **已修**（W2 `cef006dd2`，取「错误向上传播」侧） | 有（设备密钥上传、设备删除、cross-signing 变更写路径） | 已修：改名 `record_device_list_change` 并返回 `Result<(), ApiError>`，两条语句都 `map_err(…)?`；上传/删除路径 `?`（fail-closed），`record_cross_signing_change` 同样改为可失败并让 3 个调用点 `?` |
| D-08 | 数据一致性 | `synapse-e2ee/src/device_keys/storage.rs:729`（`target`）、`:793`（`fb`） | `claim_one_time_key` 的 `target`/`fb` CTE 有 `LIMIT 1` 但无 `ORDER BY`，选取非确定 | **已修**（W2 `cef006dd2`） | 有（OTK claim 路径） | 已修：两条 CTE 各加 `ORDER BY added_ts, id`（先发最旧的）；集成用例以「最旧的最后插入」制造 heap 顺序与 added_ts 顺序相反 |
| D-09 | 产品缺陷 | `synapse-storage/src/space/repository.rs:719` | `suggested_only` 分支把 jsonb `via_servers` 解成 `Vec<String>`，真返回行时必然 `ColumnDecode` | **已修**（W2 `cef006dd2`） | 有（`/_matrix/federation/v1/hierarchy/{room_id}`，`suggested_only=true`） | 已修：改 `ARRAY(SELECT jsonb_array_elements_text(via_servers))`（与本文件其余 5 处一致）；`space::db_tests` 新增 `is_suggested = TRUE` 的用例 |
| D-10 | 产品缺陷 | `synapse-storage/src/module.rs:956`（INSERT；列清单 `:958`；请求结构体字段 `:361`；路由 `synapse-web/src/routes/module.rs:769`） | `create_media_callback` 从不写 `user_id`（NOT NULL DEFAULT `''`）⇒ 必然 23514 | **已修**（W1 `c128cdeab`） | 有（`POST /_synapse/admin/v1/media_callbacks`，`module.rs:851`） | 已修：请求结构体新增 `user_id`，INSERT 绑定，管理路由传认证管理员的 user_id（语义 = 注册者）；`module::db_tests` 新建（此前 0 DB 往返，D-15.1 同批关闭） |
| D-11 | 产品缺陷 | `migrations/00000000_unified_schema_v12.sql:563`（旧列族已删）；`synapse-storage/src/registration_token/repository.rs:362` | `create_room_invite` 漏写 NOT NULL 无默认的 `inviter`/`invitee` ⇒ 必然 23502 | **已修**（W1 `c128cdeab`，按铁律 1 删列） | 无 HTTP 路由调用方（service 层唯一，`registration_token_service.rs:243`） | 已修：删除 `room_invites` 的 6 个死列（`inviter`/`invitee`/`is_accepted`/`accepted_at`/`signature`/`signed_version`）+ 索引 `idx_room_invites_invitee` + 两条 legacy 注释（全仓零读写）；db 用例改走 `create_room_invite` 往返 |
| D-12 | 产品缺陷 | `synapse-storage/src/event_report/repository.rs:324`,`:359`,`:533` | `add_history` 只 `tracing::info!` 返回内存 `id:0`，`get_report_history`/`get_stats` 恒空；两张表不存在 | **未修** | 有（`event_report.rs:499/506`；审计写入 `event_report_service.rs:55/194/378`） | 建表 + 实现（独立功能批次） |
| D-13 | 结构性限制 | `synapse-storage/src/room_summary/repository.rs:326`,`:575`；`synapse-storage/src/presence/mod.rs:232` | `Vec<Option<T>>` 数组参数无 sqlx 映射，3 处无法宏化 | **结构性保留（有意）** | 已计入 `dynamic_production`（3 处 `literal`） | 改单个 `jsonb_to_recordset($n)` |
| D-14 | 结构性限制 | 见 §7.2 D-14 | 运行期拼装 SQL 无法静态化 + D1 守卫 14 处已知假阴性 | **结构性保留（有意）** | 见明细 | 见明细（逐文件回收方向） |
| D-15 | 覆盖缺口 | 见 §7.2 D-15 | 5 组已静态化代码无 DB 往返 / 无游标分支用例 | **覆盖缺口** | — | 见明细（逐项补测） |
| D-16 | 文档一致性 | 本文件 §5 批次表 / §1 分布表 | C11 目标写 `test_isolation.rs`，与 `friend_room` 的"从未迁移"记录矛盾 | **已修正**（本次 C11 行 + 本表） | — | 已在本节固化 |
| D-17 | 结构性限制 | 根 `.sqlx/`（680）与 `synapse-storage/.sqlx/`（53） | 同一职责两份离线缓存元数据 | **未修** | 并发会话曾误清空；棘轮/CI 口径不受影响 | 按铁律 2 收敛到一处 |
| D-18 | 结构性限制 | `synapse-storage/src/thread/storage.rs:864` | `search_relevance` 是仅排序用列，`ThreadSummary` 无字段，`query_as!` 按全列构造结构体 | **结构性保留（有意）** | `NOTE(C9)`；已用子查询包裹 | 保持；后续同类列沿用子查询写法 |
| D-19 | 结构性限制 | `synapse-storage/src/event_report/models.rs:29`、`synapse-storage/src/module.rs:255` | `query_as!` **不认** `#[sqlx(rename)]` / `#[sqlx(skip)]` | **结构性保留（有意）** | 迁移时须手写别名 / 合成 `NULL` 列 | 写入批次 checklist |
| D-20 | 结构性限制 | C6/C8/C9/C13 多处 | LEFT JOIN 外侧列被 PG 透传为 NOT NULL，sqlx 误推非空 → 运行期 `UnexpectedNullError` | **结构性保留（有意）** | 已用 `AS "col?"` 覆盖 | 写入批次 checklist |
| D-21 | 结构性限制 | `synapse-e2ee/src/device_keys/storage.rs:121` 等 | 宏 `ty_match` 拒绝 `&Option<T>` 绑定（旧 `.bind()` 接受） | **结构性保留（有意）** | 已用 `.as_deref()` 等替代 | 写入批次 checklist |
| D-22 | 结构性限制 | C12/C14/C15 等多处 | `query_as!` 不走 `FromRow`，`RETURNING *` 必须展开为显式列清单（多列 E0560 / 少列 E0063） | **结构性保留（有意）** | 迁移时机械展开 | 写入批次 checklist |
| D-23 | 文档一致性 | `synapse-storage/src/registration_token/repository.rs`（C14） | 普通字符串续行 `\` 改 raw string 后变成字面反斜杠，SQL 语法错 | **已绕过**（改真实换行） | 无遗留 | 作为陷阱登记 |
| D-24 | 产品缺陷 | `migrations/00000000_unified_schema_v12.sql:4984` | v11-10 清理 DO 块删除显式 `uq_*` UNIQUE INDEX，`ON CONFLICT (worker_id)` 曾会运行期失败 | **已修**（S1–S3 `483dfc045` 改 `ADD CONSTRAINT`） | worker 统计写路径 | — |
| D-25 | 覆盖缺口 | C6/C9/C11/C15 门控模块 | feature 未打开时模块不参与编译，`test(...)` 过滤器 0 命中 ⇒ "0 tests" 假绿 | **覆盖缺口** | 曾 4 次踩到 | 门禁/census 记录所需 feature 集 |
| D-26 | 文档一致性 | 本文件 §4 与旧 baseline | "DDL 不可用 `query!` 静态化"结论过宽；生产 DDL 可静态化，仅 `#[cfg(test)]` 内不行 | **已收窄**（§执行结果 2） | — | 已在 §执行结果 2 更正 |
| D-27 | 结构性限制 | `synapse-storage/src/search_index.rs` | 整模块无生产调用者（B3 已登记按铁律 1 删除），仍带 8 处生产动态 | **未修** | 全仓唯一引用是 `sync/mod.rs:10` 再导出，无消费者 | 删除整模块，一次回收 8 处 |
| D-28 | 产品缺陷 | `synapse-storage/src/event/batch.rs` 等 | 4 个 0 调用者死查询 | **已修**（B3 `2e9c3d11d`，直接删除） | 无 | — |
| D-29 | 结构性限制 | `synapse-storage/src/admin_federation.rs:186` | `get_server_admission_status` 声明 `Option<Option<String>>`、doc 称可返回 `Some(None)`，但 `status` 列 NOT NULL ⇒ 内层 None 与消费端 `Some(None)` 分支不可达 | **未修** | 有（`federation_auth.rs:214`，`admission_mode` 开时每个联邦请求） | 按铁律 1 收窄storage 返回类型并删消费端死分支 |
| D-30 | 结构性限制 | `synapse-storage/src/presence/mod.rs:452`,`:488`,`:522`,`:557` | `presence_subscriptions` 的 4 处 `is_undefined_column_error` 回退分支查 `user_id`/`friend_id`，合并后 schema 中从无此二列（42703）⇒ 分支既不可达又无法宏化 | **未修**（C17 保留动态） | 回退分支不可达；主分支正常 | 按铁律 1 删除 4 个回退分支与 `is_undefined_column_error` |
| D-31 | 产品缺陷 | `synapse-storage/src/background_update.rs:282`（INSERT；列清单 `:283`）；测试池 `:1037` | `create_update` 的 INSERT 从不写 `update_name`（NOT NULL UNIQUE 无默认）⇒ 真 schema 下必然 23502；模块 `db_tests` 自建简化表（`update_name` 可空、无 UNIQUE）掩盖了它 | **已修**（W1 `c128cdeab`） | 有（`POST /_synapse/admin/v1/background_updates`） | 已修：INSERT 写 `update_name = job_name`（同一 `$1`）；`get_bu_test_pool()` 切到 `isolated_test_pool()`、删自建表与手工补列补丁；新增 `test_create_update_roundtrip`（往返 + 重复名 23505） |
| D-32 | 产品缺陷 | `synapse-services/src/presence_service.rs:153` | C-3 批量 presence 写路径 `set_presence_batch`（storage + service + 内存替身 + db_tests 俱全）全仓**无任何调用者**，其 doc 宣称的"联邦 presence 同步 / 批量导入"从未接线 ⇒ 批量 upsert 与其内逐用户联邦广播是死代码 | **未修** | 无生产路径（仅 db_tests 覆盖） | 接线到联邦 EDU 批处理/批量导入，或按铁律 1 删除 batch API（连带回收 D-13 的该实例） |
| D-33 | 产品缺陷 | `synapse-storage/src/push_notification.rs:620`（INSERT）、`:731`（DELETE） | `push_notification_log.sent_at` **从未被任何语句写入**（全仓唯一生产 INSERT 的 11 列清单无此列；全仓 0 条 `UPDATE push_notification_log`），而保留期清理是 `DELETE … WHERE sent_at < $1` ⇒ 三值逻辑下 `NULL < $1` 恒为 NULL，**永远删 0 行**，该 append-only 表无界增长 | **已修**（W1 `c128cdeab`） | 有（`POST …/push_notification/cleanup`，`synapse-web/src/routes/push_notification.rs:206`；恒返回 `{"cleaned":0}`） | 已修（(a)+(b) 同时做）：INSERT 写 `sent_at = created_ts` 的同一 `now`；清理谓词改 `COALESCE(sent_at, created_ts) < $1`（对存量 NULL 行同样止血）；新建文件内 `db_tests`（此前 0）三条用例 |
| D-34 | 产品缺陷 | `synapse-storage/src/threepid.rs:253`（SELECT `get_pending_threepids`；谓词 `:268`）对 `:157`（INSERT `add_threepid`） | 谓词 `WHERE validated_at < added_ts` 与写入路径互相矛盾：`add_threepid` 的 INSERT **不写 `validated_at`**（列清单无此列）⇒ 真正的"待验证"行 `validated_at IS NULL`，`NULL < added_ts` 为 NULL ⇒ **永远不返回**；db 用例 `test_get_pending_threepids` 只能改用 `add_verified_threepid(…, validated_at=1, added_ts=1000)` 人为造行，并把"query 不过滤 is_verified"写进注释当成规格 | **已修**（W1 `c128cdeab`） | 无（唯一包装 `IdentityStorage::get_pending_three_pid_validations`，`synapse-services/src/identity/storage.rs:65`，全仓无调用者） | 已修：谓词改 `validated_at IS NULL OR validated_at < added_ts`；用例改回走 `add_threepid`，新增 `test_get_pending_threepids_excludes_validated_rows` 负例，删除"把缺陷当规格"的注释。**遗留**：包装方仍零调用者 —— 接线或按铁律 1 删除，属独立条目 |

| D-35 | 文档一致性 | 本文件 §7 导言（"计数口径"行） | 该行写 `dynamic_production=741` / `static=773` 并标注"C17 后实测"，但 741/773 是 **C16 后**的值：C17 为 773→742、741→772，与本仓 baseline（`BASELINE_DYNAMIC_PRODUCTION=742` / `BASELINE_STATIC=772`）矛盾，两处各偏 1 | **已修**（C18 提交一并更正为 706/808 并注明偏差来源） | — | 已在本节导言更正；计数一律以 `scripts/ci/sqlx_query_census.py` + `scripts/ci/sqlx_dynamic_ratio_baseline` 为唯一来源 |
| D-36 | 覆盖缺口 / 门禁 | `scripts/ci/test_ddl_allowlist`、`scripts/ci/insert_column_allowlist`、`tests/unit/test_ddl_guard_tests.rs`、`tests/integration/insert_column_coverage_tests.rs` | **系统性根因**：D-10/D-11/D-31/D-33/D-34 五条"写入端漏列"缺陷同源 —— DB 测试不跑迁移 schema，而用空 schema + 自建简化表，掩盖了 NOT NULL/CHECK/UNIQUE 约束与写入端漏列 | **已修**（守卫 A/B 落地 `7cd40a418`；W1 `c128cdeab` 已把 5 个夹具切到迁移模板） | — | §8.4 两条守卫均已实现并自证变红，见 §8.7 |
| D-37 | 冗余实现 + 吞错（**新登记**） | `synapse-storage/src/device/mod.rs:182`（实现）、`:220`（零调用者包装）、`:530`/`:557`/`:595`（三处 `let _ = …`） | `DeviceStorage::record_device_list_change` 是 `synapse-e2ee` 同名职责的**第二份实现**（铁律 2），三个调用点又都是 `let _ = …` 吞错（与 D-07 同型）；其 best-effort 包装 `:220` 全仓**零调用者**（铁律 1） | **未修**（2026-09-24 W2 顺带发现并登记） | 有（storage 层设备增删路径 `:530`/`:557`/`:595`） | 二选一：storage 层统一改为可失败并让三个调用点显式处理（与 D-07 的解法对齐），删掉零调用者的 best-effort 包装；同时评估与 `synapse-e2ee` 那份实现能否收敛成一份（铁律 2） |

**状态计数（2026-09-24 W2 + D-36 后）**：已修 **15**（D-02/D-03/D-24/D-28/D-35 + W1 的
D-10/D-11/D-31/D-33/D-34 + D-36 守卫 + W2 的 D-05/D-07/D-08/D-09）；未修 **10**
（D-01 语法已修但死函数待删、D-04、D-06、D-12、D-17、D-27、D-29、D-30、D-32、
**D-37** 新登记）；结构性保留（有意）**7**
（D-13/D-14/D-18…D-22）；覆盖缺口 **2**（D-15 含 D-15.6、D-25；D-36 虽同属覆盖缺口/门禁，
已计入上面的"未修 19"，此处不重复计数）；文档一致性 **3**
（D-16/D-23/D-26；D-35 已计入上面的"已修 5"，此处**不重复计数**——原文把 D-35 同时计入
两类，五类相加为 36 与"合计 35"矛盾，本次标注修正）。合计 **36** 条
（D-01…D-36；其中 **D-36 为本次重排新登记的系统性根因**，故由 35 增至 36）。
校验：5 + 19 + 7 + 2 + 3 = **36**。

### 7.2 逐条明细

#### D-01 `get_rooms_with_member_counts` 的 SQL 语法错误（C8）

- 位置：定义 `synapse-storage/src/room/mod.rs:1393`；修复后的 SQL
  `:1407-1411`（`FROM rooms r` → `LEFT JOIN room_memberships` → `LEFT JOIN
  room_summaries` → `WHERE r.room_id = ANY($1)`）。
- 证据（原始缺陷）：`git log -L` 显示 C8（`0e1716643`）之前的旧文本是
  `WHERE r.room_id = ANY($1)` 在前、`LEFT JOIN room_summaries rs …` 在后。psql 复现同构
  语法（`PREPARE bad_order AS SELECT 1 FROM rooms r WHERE r.room_id IS NOT NULL LEFT JOIN
  room_summaries rs ON rs.room_id = r.room_id;`）⇒
  `ERROR: 42601: syntax error at or near "LEFT"`。**该查询在 C8 之前完全无法执行**。
- 证据（零调用者）：`grep -rn 'get_rooms_with_member_counts' --include=*.rs .`（排除
  `/target/`）只命中定义 `:1392-1393`（其中 `:1392` 是自引用的 `/// See [...]`）——
  非 trait 方法，无动态分发路径。
- 可达性：**无**。没有任何路由 / service / 测试调用它。
- 状态：SQL 顺序**已修**（`0e1716643`，同时静态化为 `query_as!`）；函数本身**未删**。
- 建议处理：按铁律 1 删除整个函数（它现在能 prepare 了，但仍然是死代码）。删除属于
  独立的死代码清理，不在静态化批次内。

#### D-02 `saml/process_logout_request` 写错列名（C9）

- 位置：`synapse-storage/src/saml/repository.rs:574`（`UPDATE … SET status =
  'processed', processed_at = $2`）；读取路径 `:538`/`:560` 用
  `processed_at AS processed_ts` 对齐模型字段名。
- 证据：psql 对旧语句 `UPDATE saml_logout_requests SET status='processed',
  processed_ts = $2 WHERE request_id = $1` ⇒
  `ERROR: 42703: column "processed_ts" of relation "saml_logout_requests" does not exist`；
  新语句 `PREPARE` 成功。真列名 `processed_at`（`information_schema.columns` 实测
  `processed_at=1 / processed_ts=0`）。
- 可达性：**有**。调用方 `synapse-services/src/saml_service.rs:482`
  （`process_logout_response`，`#[cfg(feature = "saml-sso")]` 路径）。
- 状态：**已修**。提交 `cbe718ff6`（C9 静态化时按真实 schema 改回列名）。
- 备注：C9 段自述这是"顺手修复的既有缺陷（唯一一处）"。

#### D-03 `worker/get_statistics` 选了 15 个不存在的列（C9 发现，S1–S4 收口）

- 位置：`synapse-storage/src/worker/repository.rs:757`（`get_statistics`）。
- 证据（缺陷）：旧版本 SELECT 的 15 列在 `workers` 与 `worker_statistics` 中都不存在
  （`pending_commands` / `active_tasks` 两列**至今在任何迁移中都不存在**，其余 13 列
  分别属于 `workers` 或当时的 `worker_statistics`）；其 doc 引用的迁移
  `20260812120000_worker_statistics_load_metrics` 不在 `migrations/`
  （`ls migrations/ | grep 20260812120000` 无命中）。psql 实测首列即报
  `ERROR: 42703: column "worker_name" does not exist`。
- 证据（已修）：psql 对现行 SQL（`workers w LEFT JOIN worker_statistics s ON
  s.worker_id = w.worker_id`，22 列）执行 `PREPARE gs AS …` ⇒ `PREPARE` 成功；
  `worker_statistics` 的 6 个负载指标列 + `last_heartbeat_ts` 现由迁移
  `migrations/00000000_unified_schema_v12.sql:2062-2068` 提供（**这些 ALTER 正是 S1 加的**，
  `git log -S 'ADD COLUMN IF NOT EXISTS cpu_usage' -- migrations/` 命中 `483dfc045`），
  身份字段来自 `workers`（`:1986-1995`）。
- 生产者链接（首次有写入方，见任务书要求）：
  - S1–S3 `483dfc045`：心跳 `load_stats` 落库 + `upsert_statistics`
    （`worker/repository.rs:465`）+ 读取端契约对齐；`WorkerManager::get_statistics`
    消费（`synapse-services/src/worker/manager.rs:672`），生产装配
    `synapse-services/src/wiring/admin.rs:385`。
  - S4 `14eab2283`（worker 侧心跳发送器与负载采集）+ `0e0af49d0`（端到端 + 采集器单测）。
  - `.sqlx` 刷新 `80414ca6d`。
- 可达性：**有**。路由 `GET /_synapse/worker/v1/statistics`
  （`synapse-web/src/routes/worker.rs:695`）→ handler `:629` → manager `:672` → storage
  `:757`。
- 状态：**已修**。提交 `0f6a76c13`（契约修复 + 静态化，`42703` 消失）、`483dfc045`
  （S1–S3 补列并让计数列首次有生产者）。
- 备注：本文 §3.4 旧表曾把该端点标 `[未验证]`（未连库实跑）；本条给出的
  `PREPARE` 与路由链证明已完成运行期/静态双重核验。

#### D-04 `DeviceKeyStorage::create_tables()` DDL 缺 `fallback_used`（C10）

- 位置：`synapse-e2ee/src/device_keys/storage.rs:233-278`（手写 DDL），
  其中 `CREATE TABLE device_keys (…)` 无 `fallback_used` 列。
- 证据：权威 schema
  `migrations/00000000_unified_schema_v12.sql:673`（`fallback_used BOOLEAN NOT NULL
  DEFAULT FALSE`），且 `:3457` 的部分索引 `idx_device_keys_fallback` 依赖
  `fallback_used = FALSE`。psql 用该 DDL 建表后再执行 fallback 分支条件
  `SELECT user_id FROM device_keys WHERE is_fallback = TRUE AND fallback_used = FALSE`
  ⇒ `ERROR: 42703: column "fallback_used" does not exist`。
- 调用方：`grep -rn '\.create_tables(' --include=*.rs .` **无命中**（0 调用者）——
  建表真源是 `migrations/`，故这是**潜伏缺陷**而非当前故障。
- 可达性：**无**（无调用者）。
- 状态：**未修**。
- 建议处理：按铁律 1 删除该方法（不要给它补列——补列等于维护第二份 schema 真源，
  违反铁律 2/4）。

#### D-05 `DeviceKey.id` 恒为 0（C10）

> **已修（W2 `cef006dd2`）**：按铁律 1 **删除**了 `DeviceKey.id`，而不是投影真主键 ——
> 全仓唯一消费方是同 crate 的 `test_mocks.rs`（仅测试）。其下位置为修复前行号。
>
- 位置：`synapse-e2ee/src/device_keys/storage.rs:121-122`（`DeviceKey {` 在 `:121`，
  `id: 0,` 在 `:122`）。
- 证据：本文件内**没有任何** SQL 投影 `id`——`RETURNING`/`SELECT` 列清单为
  `user_id, device_id, algorithm, key_id, public_key, signatures, display_name,
  added_ts, ts_updated_ms, key_data, is_fallback`（`claim_one_time_key` 的两条 CTE
  `:735-745` / `:779-789` 只用 `id` 做 `DELETE … WHERE id IN (SELECT id FROM target)`，
  不返回它）。而 `device_keys.id` 是 `BIGSERIAL` 主键
  （psql `information_schema`：`bigint default=nextval('device_keys_id_seq')`）。
- 可达性：生产代码**不读** `DeviceKey.id`；全仓仅有的读取是
  `storage.rs:942`（手工构造的 `create_test_device_key()` 断言 `id == 1`）与
  `:1032`（`serde_json::from_value` 反序列化断言 `id == 10`）——两条都是纯单元测试，
  不经过 SQL。
- 状态：**未修**（C10 为保持行为未改列集合，故 id 仍恒 0）。
- 建议处理：二选一——把 `id` 加进投影并返回真实主键；或按铁律 1 删除 `DeviceKey.id`
  （既然无人消费）。需先确认 API 是否需要它。

#### D-06 `DeviceKeyRow` 文档注释错乱（C10）

- 位置：`synapse-e2ee/src/device_keys/storage.rs:13`（`pub struct DeviceKeyRow`）起，
  典型片段 `:14-24`。
- 证据：每个字段前被塞进了**全量**的 "The `x` field." 行（`user_id` 前是
  `The user_id field.`，`device_id` 前却是 `The user_id field. The device_id field.`，
  如此叠加）。`read` 该结构体即可见。
- 可达性：无（注释）。
- 状态：**未修**（纯 cosmetic）。
- 建议处理：一次性清理为每个字段一行（顺手检查同一次批量注释脚本影响的其他结构体）。

#### D-07 `record_device_list_change_best_effort` 完全吞错（C10）

> **已修（W2 `cef006dd2`）**，取「错误向上传播」侧：方法改名 `record_device_list_change`
> 并返回 `Result<(), ApiError>`，三处调用方各自显式决定（两处 `?` fail-closed，
> cross-signing 侧同样改为可失败）。其下位置为修复前行号。
>
- 位置：`synapse-e2ee/src/device_keys/storage.rs:292-328`。
- 证据：第一处插入 `:307-309` 是 `let Ok(stream_id) = row else { return; };`
  （失败即静默返回）；第二处 `:311-326` 是 `let _ = sqlx::query!(…).execute(…).await;`
  （错误被丢弃）。函数名自述 "best_effort"，但 `stream_id` 插入失败对调用方**完全不可见**。
- 可达性：**有**（设备列表变更写路径，由 E2EE 设备增删触发）。
- 状态：**未修**（语义待决策：best-effort 是否应至少记 warn/metric，或改 `?`）。
- 建议处理：先定语义——若 best-effort 是有意的，至少 `tracing::warn!` + 指标；
  若要保证一致，改 `?` 并把调用方改为可失败。属行为决策，独立提交。

#### D-08 `claim_one_time_key` 的 `target` CTE 无 `ORDER BY`（C10）

> **已修（W2 `cef006dd2`）**：`target` 与 `fb` 两条 CTE 各加 `ORDER BY added_ts, id`。
> 其下位置为修复前行号。
>
- 位置：`synapse-e2ee/src/device_keys/storage.rs:726-734`（OTK 的 `target` CTE）、
  `:769-777`（fallback 的 `fb` CTE）。
- 证据：两处都是 `SELECT id FROM device_keys WHERE … LIMIT 1`，**无 `ORDER BY`** ⇒
  当同一 `(user_id, device_id, algorithm)` 有多把未用密钥时，选取哪一把由 PG 决定
  （非确定性）。`f7569f5c8` 的"ORDER BY 决胜键"清理**没有**覆盖这里（该 CTE 连
  `ORDER BY` 都没有）。
- 可达性：**有**（OTK claim 路径）。
- 状态：**未修**。
- 建议处理：加 `ORDER BY added_ts, id`（确定性发放；`added_ts` 是现有列）。

#### D-09 `collect_hierarchy_recursive` 的 `suggested_only` 分支解码类型不符（C7）

> **已修（W2 `cef006dd2`）**：改用 `ARRAY(SELECT jsonb_array_elements_text(via_servers))`，
> 与本文件其余 5 处一致；`space::db_tests` 补了 `is_suggested = TRUE` 的用例
> （旧夹具全部 `is_suggested = FALSE`，`WHERE is_suggested = TRUE` 恒 0 行）。
> 其下位置为修复前行号。
>
- 位置：`synapse-storage/src/space/repository.rs:706-733`（分支），关键行 `:716`
  （`via_servers AS "via_servers!: Vec<String>"`）。
- 证据：`space_children.via_servers` 是 **jsonb**（psql
  `information_schema.columns.data_type = jsonb`；`jsonb` OID 3802 vs `text[]` OID 1009），
  而该 SELECT 把原始 jsonb 直接当作 `text[]` 解码 ⇒ 真返回行时 sqlx 类型检查失败。
  同族 **5** 处查询都用正确写法
  `ARRAY(SELECT jsonb_array_elements_text(via_servers))`：
  `:165`（`add_child` RETURNING）、`:203`（更新 RETURNING）、`:230`
  （`get_space_children`）、`:1017`/`:1045`（另两条同族读）。C7（`c8871fe76`）用
  `AS "via_servers!: Vec<String>"` 把静态形式保持成与旧动态形式**同样（坏）**，未改行为。
- 可达性：**有**。`GET /_matrix/federation/v1/hierarchy/{room_id}`
  （`synapse-web/src/routes/federation/mod.rs:335`）→ `events.rs:501`
  → `SpaceService::get_space_hierarchy_v1`（`synapse-services/src/room/space/children.rs:203`）
  → `repository.rs:770` → `:778` → `:706`；请求参数含 `suggested_only`。
  现有 `db_tests.rs:747` 虽传 `suggested_only=true`，但夹具
  `is_suggested=false` ⇒ 该查询返回 **0 行**，解码路径从未被触发，因此测试没红。
- 状态：**未修**。
- 建议处理：把 `:716` 改成
  `ARRAY(SELECT jsonb_array_elements_text(via_servers))`，并补一个
  `is_suggested=true` 的 DB 用例（否则同型回归无法被发现）。

#### D-10 `create_media_callback` 从不写 `user_id`（C12）

> **已修（W1 `c128cdeab`）**。下面的"位置"是修复前的行号；现状见 §7.1 D-10 与 §8.6。
>
> 位置：`synapse-storage/src/module.rs:949-952`（`sqlx::query_as!` 起于 `:946`，INSERT 在
> `:949`，列清单 `:950`，**不含 `user_id`**）。
- 证据：`media_callbacks.user_id` 是 `TEXT NOT NULL DEFAULT ''`
  （`migrations/00000000_unified_schema_v12.sql:2212`），而 v12 基线的 DO 循环会为**每张
  含 `text user_id` 列的表**自动加 `ck_%I_user_id_format`
  （`migrations/00000000_unified_schema_v12.sql:4786-4816`，`EXECUTE` 在 `:4811`）。
  psql 实测（事务内、已 `ROLLBACK`）：
  ```
  INSERT INTO media_callbacks (callback_name, callback_type, url, created_ts, updated_ts)
  VALUES ('__registry_probe__','test','http://x',1,1);
  ERROR:  23514: new row for relation "media_callbacks" violates check
          constraint "ck_media_callbacks_user_id_format"
  ```
  即该函数**对任何输入都失败**（缺省 `''` 不匹配 `^@…:…$`）。
- 既有问题证明：`git show HEAD~1:synapse-storage/src/module.rs` 的旧动态版本列清单与
  绑定完全一致（同样没有 `user_id`），与 C12 的 `RETURNING *` → 显式列之差无关。
- 可达性：**有**。`POST /_synapse/admin/v1/media_callbacks`
  （`synapse-web/src/routes/module.rs:851`，router 由 `assembly.rs:262` 合并）
  → `module_service.rs:683`。
- 状态：**未修**（C12 按规则仅登记）。
- 建议处理：把 `user_id` 纳入 `CreateMediaCallbackRequest` 并绑定（行为修复），
  或改绑 `NULL`（约束允许 NULL）——需产品确认该回调是否属于某用户。

#### D-11 `create_room_invite` 漏写 NOT NULL 列（C14）

> **已修（W1 `c128cdeab`）**：按铁律 1 **删除**了这两个死列而非补绑定 —— 见 §8.6。
> 下面的"位置"是修复前的行号。
>
> 位置：`synapse-storage/src/registration_token/repository.rs:362-387`，INSERT 列清单
> `:369-371`（只写 `invite_code, room_id, inviter_user_id, invitee_email, expires_at,
> created_ts`）。
- 证据：`room_invites.inviter` / `.invitee` 是 `TEXT NOT NULL` 且**无默认值**
  （`migrations/00000000_unified_schema_v12.sql:566-567`；psql
  `is_nullable=NO / column_default=<none>`），表上无触发器；psql 实测（已 ROLLBACK）：
  ```
  ERROR:  23502: null value in column "inviter" of relation "room_invites"
          violates not-null constraint
  ```
  即该方法**对任何输入都失败**。既有测试用裸 SQL 绕过并留了注释：
  `registration_token/db_tests.rs:828-829`（"create_room_invite is broken due to
  required inviter/invitee columns that it does not supply — pre-existing bug"）。
- 可达性：**无 HTTP 路由调用方**。`grep -rn 'create_room_invite' --include=*.rs .`
  仅命中 storage 定义、service 包装（`synapse-services/src/registration_token_service.rs:243`）
  与测试；`RegistrationTokenService::create_room_invite` 自身没有生产调用方 ⇒
  当前是**潜伏缺陷**（一旦接线路由即 100% 失败）。
- 状态：**未修**。
- 建议处理：产品决策——`inviter`/`invitee` 与 `inviter_user_id`/`invitee_email` 的映射
  （或删除冗余列）。属行为变更，独立提交；未决前不要接线到路由。

#### D-12 EventReport 的 history/stats 是空壳却已注册 HTTP 路由（C15）

- 位置：`synapse-storage/src/event_report/repository.rs:324`（`add_history`）、
  `:359`（`get_report_history`）、`:533`（`get_stats`）。
- 证据：`add_history` 只 `tracing::info!` 后返回内存构造的
  `EventReportHistory { id: 0, … }`（`:343-357`），从不落库；两个 getter 恒
  `Ok(vec![])`。psql 实测 `public` 中 `tablename LIKE 'event_report%'` 只有 **1** 张表
  （`event_reports`）——`event_report_history` / `event_report_stats` **不存在**。
- 可达性：**有**。路由
  `GET /_synapse/admin/v1/event_reports/{id}/history`（`synapse-web/src/routes/event_report.rs:499`）
  与 `GET /_synapse/admin/v1/event_reports/stats`（`:506`，router 由 `assembly.rs:259`
  合并）；写入侧 `synapse-services/src/event_report_service.rs:55/194/378`
  在 report/update/delete 时都调 `add_history` ⇒ **审核历史被静默丢弃**，
  两个 admin 端点**永远返回空**。
- 状态：**未修**（这三处是纯内存函数、不含 SQL，故不在 C15 的 40 个动态站点内）。
- 建议处理：独立功能批次——建 `event_report_history` / `event_report_stats` 表 +
  落地 `add_history`/`get_report_history`，`get_stats` 改按天聚合 SQL；在此之前需决定
  两个端点是否临时下线（当前返回空会被误读为"没有历史"）。

#### D-13 结构性限制：`Vec<Option<T>>` 数组参数无法静态化（C9，C17 新增第 3 处）

- 位置：`synapse-storage/src/room_summary/repository.rs:326`（`add_members_batch` 的
  `NOTE(C9)`，动态站点 `:332`）、`:575`（`set_states_batch` 的 `NOTE(C9)`，动态站点
  `:579`）；**C17 新增** `synapse-storage/src/presence/mod.rs:232`
  （`set_presence_batch` 的 `UNNEST($1::TEXT[], $2::TEXT[], $3::TEXT[], $4::BIGINT[])`，
  `$3` 为 `Vec<Option<&str>>`）。
- 证据：三处绑定**逐元素可空数组** `Vec<Option<String>>` / `Vec<Option<i64>>` /
  `Vec<Option<&str>>`；sqlx-postgres 只登记了非空元素数组（`Vec<String> | &[String]`
  等），无 `Vec<Option<T>>` 映射 ⇒ `query!` 以 E0308 拒绝
  （C17 实测原文：`expected &[String], found &[Option<String>]`，指向 `$3` 实参）。
  **这三处的 SQL 文本都是字面量**（故在 D1 基线里
  按 `literal` 登记），动态的只是绑定参数类型；SQL 里已是 `::TEXT[]`，加 SQL 转换无效。
  C17 已实测：把 `$1`/`$2`/`$4` 改成精确 `&[String]`/`&[i64]` 后，唯独 `$3` 仍被拒，
  故**整条语句**（不是单个参数）必须保持动态——这正是本条从 2 处扩到 3 处的原因。
- 可达性：room_summary 两处**有**（批量成员/状态写入路径）；C17 新增的
  `presence::set_presence_batch` 目前**无生产调用者**（见 §7 D-32），全仓引用只有
  storage trait/实现（`presence/api.rs:16`,`:70-71`）、service 包装
  （`synapse-services/src/presence_service.rs:153`）、内存替身
  （`test_mocks/presence.rs:43`）与两侧 db_tests；该语句当前只由测试往返覆盖。
- 状态：**结构性保留（有意）**，3 处已计入 `dynamic_production`
  （`python3 scripts/ci/sqlx_query_census.py --list-production`）。
- 建议处理：回收方向——把并行数组换成单个 `jsonb_to_recordset($n)`
  （JSON null ↔ SQL NULL 语义等价），属独立改造。

#### D-14 结构性限制：运行期拼装 SQL + D1 守卫的已知假阴性

- **A. 真正运行期拼装（当前 census 实测，`--list-production-dynamic`）**
  | 文件 | 站点 | 为何不能静态化 / 回收方向 |
  |---|---|---|
  | `synapse-storage/src/space/repository.rs` | 572, 626 | `search_spaces` 的 Trigram 相似度表达式运行期拼装；方向：固化为两条静态 SQL |
  | `synapse-storage/src/room_summary/repository.rs` | 332, 579 | 见 D-13（`Vec<Option<T>>`）；方向：`jsonb_to_recordset` |
  | `synapse-storage/src/sliding_sync/repository.rs` | 286, 316 | `QueryBuilder::<Postgres>`（`push_bind` 仍参数化）；计入 census 单列的 `query_builder=18`，两侧棘轮都不计；方向：filters 组合可枚举时固化为静态分支 |
  | `synapse-storage/src/friend_room/repository.rs` | **0** | C11（`502424004`）已全部静态化，不再有运行期站点 |
- **B. D1 守卫的已知假阴性：14 处（原 15 处，C15 关掉 1 处）**
  `let sql = "SELECT …"; sqlx::query(&sql)` 会被分类为 `runtime` 而绕过棘轮基线
  （守卫只看实参 token 形态）。当前实测：
  - `synapse-storage/src/event/create.rs` **12** 处（23, 36, 94, 112, 121, 139, 210,
    229, 237, 252, 271, 279）——实参是同文件的 `let … = r"…"` 局部字面量绑定
    （`:13`, `:76`, `:86`, `:182`, `:194`, `:202`）；回收方向：把 SQL 直接写进
    `query!` 调用点（宏要求字面量在调用点，`let` 绑定不满足），可回收 12 处。
  - `synapse-storage/src/presence/mod.rs` **2** 处（274, 305）——实参是
    `const PRESENCE_SELECT_BY_USER`（`:21` 字面量）；同型。
  - ~~`synapse-storage/src/event_report/repository.rs` 1 处（原 `:319`）~~ ——
    **已由 C15（`4fde96137`）关闭**：C15 把模块级
    `const REPORT_RATE_LIMIT_SELECT_FOR_UPDATE` 内联进宏并删除该常量
    （`grep 'REPORT_RATE_LIMIT_SELECT_FOR_UPDATE' synapse-storage/src/event_report/repository.rs`
    无命中）。故 15 → **14**。
  - 现状是**刻意不做**同文件 `const`/`let` 字面量绑定解析：守卫是棘轮（这 14 处已在
    D1 的 `runtime` 基线里锁住），把它记为收紧方向而不是隐藏。实现要点：在
    `iter_dynamic_sites` 里对 `path` 建 `name → 是否字面量绑定` 表，实参为裸标识符时
    查表即可（这 14 处会从 `runtime` 变 `literal`，基线相应 +14）。
- 状态：**结构性保留（有意）**；子项 B 同时是一个**已知的守卫覆盖缺口**。

#### D-15 覆盖缺口清单（汇总）

| 缺口 | 路径 | 现状（实测） | 建议补什么测试 |
|---|---|---|---|
| D-15.1 `module.rs` 25 处静态化转换无任何 DB 往返 | `synapse-storage/src/module.rs`（9 个 `test_` 全为纯构造/纯单元，文件内无 `require_test_pool`）；转换批次 C12 `5d42b590c` | 全仓（含 `tests/`、`synapse-services`）没有任何 `ModuleStorage` DB 往返用例；基线记录里的一次性 smoke（`c12_module_runtime_smoke`，逐站点跑 25 处，**10 passed**）只存在于隔离 worktree，**未提交**（提交会新增夹具动态 SQL） | 把该 smoke 整理后提交到 `module.rs` 的 db_tests（真实 DB、per-test schema），覆盖 keyset 游标两分支、`RETURNING` 展开列、`AS "col!"`、`NULL::BIGINT` 合成列 |
| D-15.2 `sliding_sync::list_room_token_sync` 游标分支 | `synapse-storage/src/sliding_sync/repository.rs:646`（游标分支 `:659` 起） | `db_tests.rs` 只有 `test_list_room_token_sync_without_cursor`（`:631`）与 `..._limit_truncates`（`:670`），**都传 `from=None`**；C13 基线据此登记为无覆盖。**但**集成用例 `tests/integration/sliding_sync_storage_tests_migrated.rs:1115`（`test_list_room_token_sync_with_cursor`，经 `tests/integration/mod.rs:89` 注册）**已覆盖游标分支**，且该用例自 2026-06-11（`e0c98397c`）就存在 ⇒ 任务书的"无测试"**不成立**（准确说法：`-p synapse-storage --lib` 口径内无覆盖） | 可选：在 `db_tests.rs` 补一个本地游标用例，把覆盖收进 storage crate 自己的 lib 口径 |
| D-15.3 `event_report::get_reports_by_room` 游标分支 | `synapse-storage/src/event_report/repository.rs:89`（游标分支 `:99` 起） | `db_tests.rs` 只有 `test_get_reports_by_room_basic`（`:196`）与 `..._limit`（`:239`），均 `since_ts/since_id=None`；同型游标在 by_reporter（`:304`）/by_status（`:396`）/all_reports（`:470`）都有专测，唯独 by_room 缺（C15 已登记） | 补 `test_get_reports_by_room_cursor_pagination`，夹具照 `test_get_reports_by_reporter_cursor_pagination` |
| D-15.4 `friend_room` 两个建议查询无任何测试 | `synapse-storage/src/friend_room/repository.rs:927`（`get_friend_suggestions_from_mutual_friends`）、`:986`（`..._from_shared_rooms`） | `grep -rn 'get_friend_suggestions_from' tests/ synapse-storage/src/friend_room/db_tests.rs` **无命中**；唯一调用方是 `synapse-services/src/friend_room_service/groups.rs:215,227`（C11 基线已登记） | 为这两个查询各补 DB 用例（含 `COUNT(DISTINCT …) AS "mutual_count!"` / `shared_rooms_count!` 与 LEFT JOIN `displayname?`/`avatar_url?` 覆盖） |
| D-15.5 C7 的 namespace 转换方法无直接 db_tests 调用方 | `synapse-storage/src/application_service/repository.rs` + `space/repository.rs`（C7 `c8871fe76`） | C7 基线列出的方法是 `get_statistics / update_last_seen / get_user_namespaces / get_room_alias_namespaces / get_room_namespaces / find_{user,room_alias,room}_namespace_conflict / is_{user,room_alias,room_id}_in_namespace / has_exclusive_user_namespace_match` = **12** 个（任务书写 13；实测清单只有 12 个名字）。抽查 `get_statistics` / `update_last_seen` / `get_user_namespaces` / `has_exclusive_user_namespace_match` 在 `space/db_tests.rs` 与 `application_service/db_tests.rs` 的调用数均为 0 | 为这 12 个方法补 namespace 冲突/命中与 `!` 覆盖的 DB 往返用例（编译期已校验，运行期风险低，优先级低于 D-15.1/D-15.4） |
| D-15.6 `push_notification` 18 处静态化转换**零** DB 往返 | `synapse-storage/src/push_notification.rs`（文件内只有 `mod tests` 的 15 个纯构造/序列化断言，**没有** `db_tests`）；转换批次 C18 `f1eb338d6` | `cargo nextest run -p synapse-storage --lib -E 'test(push_notification)'` 命中的 15 个用例全部不触 DB（C18 实测 31 tests = 15 个 push_notification 纯单测 + 14 个 threepid db_tests + 2 个 threepid 纯单测）。`cleanup_old_logs`（→ D-33）、`get_pending_notifications` 的 `FOR UPDATE SKIP LOCKED`、`register_device` 的 `ON CONFLICT … DO UPDATE` upsert、`create_notification_log` 的展开列 `RETURNING`、`mark_notification_failed` 两分支均**无运行期覆盖**（只有编译期按真实 schema 的 describe 校验）。D-33 能长期潜伏正是因为没有任何用例调用过 `cleanup_old_logs` | 在文件内补 `db_tests`（迁移模板 schema、per-test 隔离）：`register_device` upsert（含 `metadata` / `last_used_at`→`last_used_ts` 别名）/ `get_device` / `queue_notification` + `get_pending_notifications` / `mark_notification_sent` + `mark_notification_failed` 两分支 / `set_config` + `get_config` + `list_config` + `delete_config` / `create_notification_log` / `cleanup_old_logs`（该用例会立刻暴露 D-33） |

- 状态：**覆盖缺口**。
- 备注：任务书列的 5 条里有 1 条（D-15.2）经核实**不成立**（集成用例已覆盖），
  1 条（D-15.5）的方法数是 12 而非 13；其余 3 条成立。所有缺口都应在收尾批次里
  按"每条独立提交、先写用例（RED）再收口"处理。

#### D-16 文档一致性：C11 目标自相矛盾（本次修正）

- 位置：本文件 §5 批次表原第 168 行
  `| C11 | synapse-common/src/test_isolation.rs | 25 | 待 A1 分区后重估 |`，
  与 §1 分布表（`friend_room/repository.rs 26/26`）及 §执行结果 §3.4
  （"`friend_room` 属于 C11+ 的工作量"）矛盾。
- 证据：`scripts/ci/sqlx_dynamic_ratio_baseline` 的 C11 段（`:907-908`）已明确记录
  "方案文档…的 C11 行写的是 `synapse-common/src/test_isolation.rs`，而同一文档的分布表
  把 friend_room 列为 26/26 —— 文档自身不一致；本批按实际指派取
  `friend_room/repository.rs`，`test_isolation.rs` 仍在待办"，且实际执行 C11 =
  `502424004`（friend_room，-26）。而 `test_isolation.rs` 在 §3.2 被归入 **D2
  测试夹具收敛**（按 `#[cfg(any(test, feature = "test-utils"))]` 门控移出扫描面），
  根本不该作为 C11 的静态化目标。
- 状态：**已修正**（本次）：§5 的 C11 行已改为
  `synapse-storage/src/friend_room/repository.rs`（**更正**，见 §7 D-16），
  并注明 `test_isolation.rs` 属 D2。
- 附带约定：本节 §7.1 与 §7.2 是**唯一汇总处**；
  `scripts/ci/sqlx_dynamic_ratio_baseline` 仅保留棘轮口径（两个数字 + 计数理由），
  未来发现的问题一律追加到本节，不再在 baseline 里另开缺陷清单（双份记录即漂移源）。

#### D-17 `.sqlx` 双份离线缓存（结构性冗余）

- 位置：根 `.sqlx/` 与 `synapse-storage/.sqlx/`。
- 证据（**2026-09-23 重排时重测**；本条目撰写时的 680 已过时，见行内标注）：
  - 根 `.sqlx/` **782** 个 `query-*.json`（撰写时 **680**，C11–C18 期间增长），
    `synapse-storage/.sqlx/` **53** 个（未变）；
  - **两者都被 git 跟踪**：`git ls-files .sqlx | wc -l` = **782**（撰写时 680），
    `git ls-files synapse-storage/.sqlx | wc -l` = 53；
  - 逐文件 `cmp`：53 个里有 **34 个与根缓存逐字节相同**、**0 个内容冲突**、
    **19 个只存在于 `synapse-storage/.sqlx/`**（根缓存里没有同名文件）⇒ 根缓存
    **并未完全覆盖**子目录（"内容被根缓存覆盖"不成立）。34/0/19 的比例在重测中未变。
  - 解析规则（sqlx 按 `SQLX_OFFLINE_DIR` → `manifest_dir/.sqlx` → 根 `.sqlx` 逐文件
    回退）意味着编译 `synapse-storage` 时子目录会先被命中；`cargo sqlx prepare
    --workspace` 的 destination 是**根 `.sqlx/` 且先清后写**（C11–C15 段均记录该
    行为），所以那 19 条只可能是历史遗留或上一次 `-p synapse-storage` 刷新的产物。
    **这 19 条是否为陈旧条目**未逐条核对 ⇒ `[未验证]`（但 `deleted=0` 的历史刷新记录
    说明根缓存没有丢过仍在使用的条目）。
  - 事故记录：曾因并发会话误清空该缓存（baseline 与批次记录多次出现
    "destination 就是 `.sqlx/` 本身，`cargo sqlx prepare` 先清后写"的告警）。
- 可达性：影响 `SQLX_OFFLINE=true` 的编译与 CI 新鲜度门禁
  （`scripts/ci/check_sqlx_cache_fresh.sh`）。
- 状态：**未修**（登记为结构性冗余）。
- 建议处理：按铁律 2 收敛到一处——把 `synapse-storage/.sqlx/` 从 git 删除
  （`git rm -r --cached synapse-storage/.sqlx` 后清理），统一由根 `.sqlx/` 承担，
  并把"prepare 的 destination 只能是根 `.sqlx/`"写进批次 procedure；收敛前先跑一次
  `SQLX_OFFLINE=true cargo check --workspace --all-features` 证明确实不需要子目录。

#### D-18 结构性限制：排序专用列无对应结构体字段（`NOTE(C9)`）

- 位置：`synapse-storage/src/thread/storage.rs:864`（`NOTE(C9)`），查询 `:869` 起。
- 证据：`search_relevance` 只用于 `ORDER BY`，`ThreadSummary` 无该字段，而
  `query_as!` 按 describe 的**全部**列构造结构体字面量 ⇒ 多一列 E0560。C9 的解法是把
  查询包进子查询 `(…) AS q`，外层只投影结构体 16 列，
  `ORDER BY q.search_relevance DESC, q.sort_ts DESC NULLS LAST` 与原语义等价。
- 状态：**结构性保留（有意）**（解法已落地，登记为写批次时的 checklist 项）。
- 建议处理：保持现写法；后续遇到"仅排序/仅过滤列"沿用子查询包裹。

#### D-19 结构性限制：`query_as!` 不认 `#[sqlx(rename)]` / `#[sqlx(skip)]`

- 位置（现存受影响属性示例）：`synapse-storage/src/event_report/models.rs:29`
  （`#[sqlx(rename = "resolved_at")]`）、`synapse-storage/src/module.rs:255`
  （`#[sqlx(skip)]`）。同型属性还有 `room/models.rs:288,290,308,310`、
  `room_summary/models.rs:21`、`room_tag/mod.rs:21`、`module.rs:163`、
  `application_service/models.rs:20`、`push_notification.rs:39`。
- 证据：`query_as!` 不走 `FromRow`，而是按 describe 的列名逐列构造结构体字面量
  （`sqlx-macros-core-0.8.6/src/query/output.rs` 的 `quote_query_as`）⇒ rename 无效
  （C15 因此手写 `resolved_at AS "resolved_ts"`），skip 无效（C12 因此给
  `renewal_token_ts` 补 `NULL::BIGINT AS "renewal_token_ts"` 保持"DB 读取恒 None"）。
- 状态：**结构性保留（有意）**（库语义，非本仓缺陷）。
- 建议处理：写进静态化批次 checklist——迁移到 `query_as!` 时逐字段核对
  rename/skip，必须手写别名或合成列。

#### D-20 结构性限制：LEFT JOIN 外侧列被误推为非空

- 位置（已修复的实例）：`synapse-storage/src/worker/repository.rs:757` 起的
  `LEFT JOIN worker_statistics`（6 列 `AS "col?"`）、
  `synapse-storage/src/thread/storage.rs`（`get_thread_summary`/`search_threads` 各 4 列）、
  `synapse-storage/src/sliding_sync/repository.rs:659` 起的
  `LEFT JOIN sliding_sync_tokens`（`pos?`/`token_created_ts?`/`token_expires_at?`）。
- 证据：PG 把 `resorigtbl`/`resorigcol` 透传到底层列（catalog 中 NOT NULL），sqlx 据此
  推断非空，宏生成 `try_get_unchecked::<String>`，遇真实 NULL 即
  `ColumnDecode { source: UnexpectedNullError }`。运行期实证：
  `thread::db_tests::test_search_threads_finds_match` 首轮失败（`index: "7"` =
  `latest_event_id`），修复提交 `0d57b807d`（C9 加固）。
- 状态：**结构性保留（有意）**（已用 `AS "col?"` 系统性覆盖）。
- 建议处理：写进 checklist；C6/C8/C9/C13 已各自踩过一次。

#### D-21 结构性限制：宏 `ty_match` 拒绝 `&Option<T>` 绑定

- 位置（示例）：`synapse-e2ee/src/device_keys/storage.rs`（`display_name` 绑定）、
  C2/C3/C6/C9/C10/C13/C14 多处。
- 证据：`query!` 的 ty_match 比旧 `.bind()` 严格，`&Option<String>` 不被接受，
  必须 `.as_deref()`；`&String` 要 `.as_str()`；TEXT[] 要精确 `&[String]`。
- 状态：**结构性保留（有意）**。
- 建议处理：写进 checklist（与 D-13 的 `Vec<Option<T>>` 同族：宏对 Option 包裹的
  参数类型更严格）。

#### D-22 结构性限制：`RETURNING *` / `SELECT *` 必须展开为显式列清单

- 位置（示例）：C14 的 `registration_token`/`media_quota` 6 条 `RETURNING *`；
  C12 的 4 条（`modules`/`module_execution_logs`/`media_callbacks`）；
  C15 的 `event_reports` 两条。旧 `FromRow` 会**静默忽略**表里多出来的列，
  `query_as!` 则多列 E0560 / 少列 E0063。
- 证据：`sqlx-macros-core-0.8.6/src/query/output.rs` 的 `quote_query_as`
  （按 describe 列名构造 `T { <col>: <var>, … }`）。
- 状态：**结构性保留（有意）**。
- 建议处理：写进 checklist；`RETURNING *` 一律展开并逐列核对空值覆盖。

#### D-23 文档一致性 / 陷阱：raw string 里的 `\` 续行

- 位置：`synapse-storage/src/registration_token/repository.rs`（C14 的
  `get_all_tokens` 两条 SQL）。
- 证据：普通字符串字面量里 `\` + 换行是 Rust 续行转义；一旦为写 `AS "col!"` 改成
  raw string（`r#"…"#`），`\` 变成**字面反斜杠**原样发给 PG ⇒
  `error: error returned from database: syntax error at or near "\"`。C14 的修法是
  改用真实换行（raw string 内换行/缩进对 SQL 无害），两处必须同时改。
- 状态：**已绕过**（当前代码无遗留）。
- 建议处理：作为陷阱登记在本节；批次改 raw string 时全文件搜索 `\` 续行。

#### D-24 迁移陷阱：v11-10 清理块删除 `uq_*` 索引（S1–S3 修复）

- 位置：`migrations/00000000_unified_schema_v12.sql:4984`（清理 DO 块
  `RAISE NOTICE 'Dropped redundant UNIQUE INDEX: %.%'`）。
- 证据：S1 首轮实测该清理块会删除所有非 constraint 支撑的显式 `uq_*` UNIQUE INDEX
  （NOTICE: `Dropped redundant UNIQUE INDEX: …uq_worker_statistics_worker_id`），
  若唯一键写成 `CREATE UNIQUE INDEX uq_…`，则 `ON CONFLICT (worker_id)` 会在运行期报
  "no unique constraint matching"（新功能静默失效）。修法：改为
  `ALTER TABLE worker_statistics ADD CONSTRAINT uq_worker_statistics_worker_id
  UNIQUE (worker_id)`（包在 `pg_constraint` 判空 DO 块里，
  `migrations/…:2083-2085`），约束背书索引不在清理范围内。
- 可达性：有（心跳 `upsert_statistics` 写路径）。
- 状态：**已修**（`483dfc045`，随 S1–S3）。
- 建议处理：已在修复中固化；后续新增唯一键一律用 `ADD CONSTRAINT` 而非
  `CREATE UNIQUE INDEX uq_*`。

#### D-25 覆盖缺口 / 门禁陷阱：门控模块的 "0 tests" 假绿

- 位置：C6 `server_notification`（`#[cfg(feature = "server-notifications")]`）、
  C9 `saml`（`saml-sso`）、C11 `friend_room`（`friends`）、C15 `cas`（`cas-sso`）。
- 证据：feature 未打开时模块根本不参与编译，`cargo nextest list -E 'test(<mod>)'`
  匹配 **0** 个用例（nextest exit 4 "no tests to run"），离线 check 也直接 exit 0 ⇒
  "全绿"但一个宏都没被校验。C11 首轮实测踩到（不带 `friends` 时
  `-E 'test(friend_room)'` 匹配 0），C15 把 `cas-sso` 列为"本批的关键陷阱"。
- 状态：**覆盖缺口**（已记录，但尚无自动守卫保证每个批次带齐 feature）。
- 建议处理：在批次 procedure / CI 里为每个已迁移的门控模块记录所需 feature 集，
  并让"过滤器命中数为 0"直接失败（而不是当成功）；或让 census 按模块宣称的 feature
  集编译校验。`Cargo.toml` 的 `dynamic_production` 扫描面本身不受 feature 影响，
  所以这个缺口不会体现在棘轮数字里。

#### D-26 文档一致性：DDL 静态化结论已收窄

- 位置：本文件 §4 与旧 baseline 曾写"DDL 不可用 `query!` 静态化"。
- 证据：C10 实测把 `device_keys/storage.rs` 的 4 处 `CREATE TABLE`/`CREATE INDEX`
  转成 `query!` 并通过真实 DB 校验（生产 DDL **可以**静态化：sqlx 走扩展查询协议的
  Parse/Describe，utility statement 被接受、Describe 返回 `NoData`，宏在"输出列全为
  void"时退化为普通 `Query`；4 条缓存条目均为 `"columns": []`）。**只有
  `#[cfg(test)]` 内的宏**仍不可（`cargo sqlx prepare` 默认 target 集不收集，
  `--all-targets` 又缺 feature 报 E0432）。
- 状态：**已收窄**（本文件 §执行结果 2）。
- 建议处理：已在 §执行结果 2 更正；本节登记以防旧结论被再次引用。

#### D-27 结构性限制：`search_index` 死模块（B3 登记，未删）

- 位置：`synapse-storage/src/search_index.rs`（`SearchIndexStorage`
  `:94`/`:98`）。
- 证据：全仓唯一的模块引用是 `synapse-storage/src/sync/mod.rs:10` 的
  `pub use crate::search_index::{…}` **再导出**，而该再导出无人消费——
  `SearchIndexStorage` 在 `:94` 与 `:98`（自身）之外**没有任何**外部引用
  （其余命中全是文件内 `#[cfg(test)]` 的 `SearchIndexStorage::new`）。
  该模块带 **8** 处生产动态：6 处 `literal`（106, 216, 223, 233, 268, 271，已在 D1
  基线）+ 2 处 `runtime`（161, 179）。
- 状态：**未修**。
- 建议处理：按铁律 1 整体删除（含 `sync/mod.rs:10` 的再导出），一次回收 8 处动态；
  不要为它做静态化。

#### D-28 已修：B3 删除 4 个零调用者死查询

- 位置：`synapse-storage/src/event/batch.rs` 的 `get_latest_events_for_rooms`、
  `get_room_message_counts_batch`、`get_events_since_stream_ordering`、
  `get_room_events_by_stream_range`。
- 证据：Phase B3 判定全仓（含 `tests/`、`benches/`）无任何调用者，按铁律 1/6 直接删除
  而非计入基线（`dynamic_production 1455 → 1451`）。
- 状态：**已修**（B3，`2e9c3d11d`）。
- 备注：`get_room_message_counts_batch` 唯一的使用者是当时新增的 soft_failed 回归
  断言，已同步移除（删除死 API 优先于为它保留测试）。

#### D-29 结构性限制：`get_server_admission_status` 的内层 `None` 分支不可达（C16）

- 位置：`synapse-storage/src/admin_federation.rs:186`（`get_server_admission_status`，
  返回 `Result<Option<Option<String>>, sqlx::Error>`）；其 doc 注释
  `:176-181` 明确写 "Returns `Some(None)` when the row exists but `status` is NULL"。
  消费端死分支：`synapse-services/src/admin_federation_service.rs:486`
  （`Some(None) => Ok(Some("active".to_string()))`，注释自述"treat as active to
  preserve the middleware's historical behaviour"）。
- 证据（schema）：psql `\d federation_servers` 实测
  `status | text | | not null | 'active'::text`（`information_schema.columns`
  的 `is_nullable = NO`）⇒ 任何存在的行都不可能有 NULL status，
  **内层 `None` 不可达**。因此 storage 的 `Option<Option<String>>` 里第二层 Option
  是纯粹的兼容残留：它的唯一存在理由是"历史上该列可空"。
- 证据（消费端）：`synapse-web/src/middleware/federation_auth.rs:214` 只区分
  `Ok(Some(status)) if status != "active"`（拒绝）、`Ok(None)`（pending 拒绝）与
  `Ok(_)`（放行）；`Some(None)` 经 service 映射成 `Some("active")` 后落入 `Ok(_)`
  放行分支，与 `Some(Some("active"))` 完全同路。
  可达性：**有**——`federation.admission_mode` 打开时，每个联邦请求都会走到这里；
  但**该分支本身**无论如何都不会被触发。
- 证据（测试）：`admin_federation::db_tests` 只有
  `test_get_server_admission_status_unknown`（无行 ⇒ `None`）与
  `..._known`（有行、显式 status ⇒ `Some(Some(...))`）两例，**没有**也无法构造
  `Some(None)` 的用例。
- 状态：**未修**（本次静态化按行为保持原则只用 `SELECT status AS "status?"`
  把宏推断钉回声明类型，未改动 schema/签名/分支）。
- 建议处理：按铁律 1 把 `get_server_admission_status` 的返回类型收窄为
  `Option<String>`（无行 ⇒ `None`），删除 `admin_federation_service.rs:486` 的
  `Some(None)` 分支与 storage 的 `:176-181` doc 中"status 可为 NULL"的表述；
  属**行为契约变更**（虽无可达路径），需独立评审 + 独立提交，不夹带进静态化批次。

#### D-30 结构性限制：`presence_subscriptions` 的 `user_id`/`friend_id` 回退分支不可达且不可宏化（C17）

- 位置：`synapse-storage/src/presence/mod.rs` 的 4 处 `is_undefined_column_error`
  回退分支——`add_subscription`（`:452`，`INSERT INTO presence_subscriptions
  (user_id, friend_id, created_ts) … ON CONFLICT (user_id, friend_id)`）、
  `remove_subscription`（`:488`，`DELETE … WHERE user_id = $1 AND friend_id = $2`）、
  `get_subscriptions`（`:522`，`SELECT friend_id … WHERE user_id = $1`）、
  `get_subscribers`（`:557`，`SELECT user_id … WHERE friend_id = $1`）；判定函数
  `is_undefined_column_error` 在 `:16-18`（只认 SQLSTATE 42703）。
- 证据（schema）：权威迁移 `migrations/00000000_unified_schema_v12.sql:2912-2919`
  只定义 `subscriber_id` / `target_id` / `created_ts` 三列，主键
  `pk_presence_subscriptions (subscriber_id, target_id)`；`\d presence_subscriptions`
  实测无 `user_id` / `friend_id`。`migrations/` 是单一真相源，合并后不存在"另一种
  列名"的 schema；`friend_id` 全仓只作为 `friends` 表列（同文件 `:2952`）与 HTTP 层
  变量名出现（`synapse-web/src/routes/friend_room.rs` 等）。
- 证据（psql，`VERBOSITY=verbose`）：
  `PREPARE fb1 AS SELECT friend_id FROM presence_subscriptions WHERE user_id = $1;`
  ⇒ `ERROR: 42703: column "friend_id" does not exist`；
  `PREPARE fb2 AS SELECT user_id FROM presence_subscriptions WHERE friend_id = $1;`
  ⇒ `ERROR: 42703: column "user_id" does not exist`。
  同文件主分支（`SELECT target_id … WHERE subscriber_id = $1 LIMIT 5000`、
  `INSERT … ON CONFLICT (subscriber_id, target_id) DO NOTHING`、`DELETE … WHERE
  subscriber_id = $1 AND target_id = $2`）`PREPARE` 全部成功。
- 结论：回退的触发前提是主分支报 42703，而主分支使用的列全部存在于 catalog ⇒
  **分支不可达**；即便被触发，回退语句自身也是 42703 ⇒ **永远不可能成功**。它同时是
  （a）兼容残留死代码（其唯一存在理由是"曾经有过 user_id/friend_id 版 schema"，
  按铁律 1 应删）、（b）静态化的**结构性障碍**：`query!`/`query_scalar!` 必须按真实
  schema describe，这 4 条 SQL 会让 `cargo sqlx prepare` 直接失败（42703）。
- 状态：**未修**——按批次纪律"只修编译所必需、其余只登记不改行为"，C17 保留这 4 处
  动态调用（`sqlx::query` / `query_as::<_, (String,)>`），presence 生产区其余
  **14 处全部宏化**（`dynamic_production` 该文件 18 → 4）。
- 建议处理：按铁律 1 删除 4 个回退分支与 `is_undefined_column_error`
  （`:16-18`，删后该函数无使用者）——这会再回收 4 处动态站点（需同步下调
  `BASELINE_DYNAMIC_PRODUCTION`），但属**分支删除**（行为变更），需独立评审 + 独立提交。

#### D-31 产品缺陷：`create_update` 漏写 NOT NULL 的 `update_name`，测试自建简化表掩盖（C17）

> **已修（W1 `c128cdeab`）**。下面的"位置"是修复前的行号；现状见 §7.1 D-31 与 §8.6。
>
> 位置：`synapse-storage/src/background_update.rs:275-278`
  （`sqlx::query_as!` 起于 `:272`，INSERT 在 `:275`，列清单 `:276-277`，VALUES `:278`；
  列清单为 `job_name, job_type, description, table_name, column_name, total_items,
  batch_size, sleep_ms, depends_on, metadata, created_ts, status, max_retries`——
  **没有 `update_name`**）。
- 证据（schema）：权威迁移 `migrations/00000000_unified_schema_v12.sql:1927-1953`
  定义 `update_name TEXT NOT NULL` + `CONSTRAINT uq_background_updates_name UNIQUE
  (update_name)`，且**无 DEFAULT**（`\d background_updates` 的 Default 列实测为空）。
  psql 直接执行该 INSERT（`VERBOSITY=verbose`）⇒
  `ERROR: 23502: null value in column "update_name" of relation "background_updates"
  violates not-null constraint`。
- 证据（可达性）：**有**。`POST /_synapse/admin/v1/background_updates`
  （`synapse-web/src/routes/derived_route_table_always.inc.rs:4751`）→
  `synapse-services/src/background_update_service.rs:46`（先
  `get_update(&request.job_name)`，即 `WHERE update_name = $1`，必然查不到）
  → `:61` `create_update` ⇒ 每次请求都走到这条必然 23502 的 INSERT。
- 证据（覆盖缺口 / 为什么测试是绿的）：本模块 `db_tests` 全部走
  `crate::test_utils::prepare_empty_isolated_test_pool()`（`test_utils.rs:126`，
  创建**空 schema、不套迁移**），再由 `setup_background_update_db`
  （`background_update.rs:989` 起）自建同名简化表——其中
  `update_name TEXT,`（`:994`）**可空且无 UNIQUE**。于是测试里的 `create_update`
  成功、真 schema 下必然失败。测试代码自己把这个缺口写成了注释与补丁：
  `:1756-1758` 的 "create_update doesn't set update_name, so we need to manually set
  it for delete to work" 及其后的
  `UPDATE background_updates SET update_name = job_name WHERE update_name IS NULL`。
- 影响链（模型不一致）：`create_update` 之外的所有读写都按 `update_name` 定位
  （`get_update` / `update_status` / `update_progress` / `set_error` / `delete_update` /
  `retry_failed`），而生产创建路径只写 `job_name` ⇒ 即使 INSERT 被修好，该行也永远
  不会被这些方法命中（`BackgroundUpdate.job_name` 字段 ↔ 表里 `job_name` 可空 +
  `update_name` NOT NULL 的双列并存）。
- 状态：**未修**——C17 只把 `RETURNING *` 展开为显式列清单（`query_as!` 不走
  `FromRow`，多列 E0560），INSERT 的**列集合与绑定原样保留**，未借静态化改行为。
- 建议处理：产品决策——INSERT 补 `update_name = job_name`（并把冗余的 `job_name` 列
  收敛掉）或统一为单列；同时把 `background_update::db_tests` 从"自建简化表"改为迁移
  模板 schema（`prepare_isolated_test_pool`），否则同类 schema 漂移会持续被掩盖。
  属行为修复，需独立评审 + 独立提交。

#### D-32 产品缺陷：C-3 批量 presence 写路径 `set_presence_batch` 从未接线（C17）

- 位置：`synapse-services/src/presence_service.rs:153`（`PresenceService::set_presence_batch`，
  固有方法，非 trait 方法）；它转发到 storage 的
  `synapse-storage/src/presence/mod.rs:215`（`PresenceStorage::set_presence_batch`），
  并对每条 entry 调用 `:165` 的 `broadcast_presence_to_subscribers`。
- 证据（无调用者）：全仓 `grep -rn 'set_presence_batch' --include=*.rs .`（排除
  `target/`）只命中 4 个文件，全部是定义/转发/替身/测试，**没有任何调用点**：
  `synapse-storage/src/presence/api.rs`（trait 声明 + 转发，3 处）、
  `synapse-storage/src/presence/mod.rs`（实现 + db_tests，10 处）、
  `synapse-storage/src/test_mocks/presence.rs`（内存替身，1 处）、
  `synapse-services/src/presence_service.rs`（service 包装 + 3 个自身 db_tests，8 处）。
  没有任何 route / federation EDU handler / 后台任务调用它；以 `set_presence_batch(`
  为模式搜索调用点，命中同样只在这 4 个文件内。
- 证据（文档与实现不符）：storage 侧 doc（`presence/mod.rs:203-210`）自述
  "Uses `UNNEST` to batch the INSERT … eliminating N+1 SQL round-trips when updating
  presence for many users at once (e.g. federation presence sync, bulk presence
  import)"，service 侧 `:150-152` 亦标注 "C-3: Batch set presence…"——这两个 "e.g."
  调用方在树里都不存在 ⇒ **C-3 的批量优化与其中的批量联邦广播从未生效**。
  （单用户路径 `presence_service.rs:134` 仍在调用
  `broadcast_presence_to_subscribers`，故联邦广播功能本身不是死路，死的是批量入口。）
- 与静态化的关系：该语句 `UNNEST($1::TEXT[], $2::TEXT[], $3::TEXT[], $4::BIGINT[])`
  因可空元素数组参数无法宏化（§7 D-13，C17 保留为动态）。也就是说 C17 的 5 处保留动态
  中，有 1 处属于"新增的 D-13 实例"，且其整体可达性为"仅测试"。
- 状态：**未修**（静态化按行为保持原则原样保留，包括 `Vec<&str>` 绑定形态）。
- 建议处理：二选一——（a）把批量入口接到联邦 presence EDU 的批量处理 / 批量导入路径
  （需先确认真实批量场景存在），或（b）按铁律 1 删除 `set_presence_batch`（storage
  trait 方法 + service 方法 + 内存替身 + 双方 db_tests），连带回收该动态站点并让
  D-13 回到 2 处。两者都属行为/API 变更，需独立评审 + 独立提交。

#### D-33 产品缺陷：`cleanup_old_logs` 永远删 0 行，推送日志表无界增长（C18）

> **已修（W1 `c128cdeab`）**，§8.3.4 的 (a) 与 (b) 同时落地。下面的"位置"是修复前的行号。
>
> 位置：写入 `synapse-storage/src/push_notification.rs:614`（
  `PushNotificationStorage::create_notification_log` 的
  `INSERT INTO push_notification_log (…)`）；清理
  `synapse-storage/src/push_notification.rs:720`（`cleanup_old_logs` 的
  `DELETE FROM push_notification_log WHERE sent_at < $1`）。
- 证据（`sent_at` 无写入者）：
  1. 该 INSERT 的列清单共 11 列 ——
     `user_id, device_id, event_id, room_id, notification_type, push_type,
      is_success, error_message, provider_response, response_time_ms, created_ts` ——
     **没有 `sent_at`**；表定义（`migrations/00000000_unified_schema_v12.sql:1559`）
     里 `sent_at BIGINT` 既无 `NOT NULL` 也无 `DEFAULT`，也没有任何触发器。
  2. `grep -rn "UPDATE push_notification_log\|push_notification_log SET" --include=*.rs .`
     （排除 `target/`）命中 **0** 条；全仓对 `push_notification_log` 的语句只有
     这条 INSERT、这条 DELETE，以及 `synapse-services/src/push/service.rs:656`
     的一条 `SELECT provider_response …`。
  ⇒ 该表所有行的 `sent_at` 恒为 `NULL`。
- 证据（清理因此是 no-op）：SQL 三值逻辑下 `NULL < $1` 求值为 `NULL`，`WHERE` 不成立
  ⇒ DELETE 匹配 0 行。即无论 `days` 取何值（路由 clamp 到 1..200）、表里有多少历史，
  `cleanup_old_logs` 恒返回 `rows_affected() == 0`。
- 可达性：有真实调用链 ——
  `POST /_synapse/admin/v1/push_notification/cleanup`
  （`synapse-web/src/routes/push_notification.rs:206` 的 `cleanup_logs` handler）→
  `PushNotificationService::cleanup_old_logs`（`synapse-services/src/push/service.rs:568`）
  → storage 层。所以这不是死代码，而是一个**恒静默成功但什么也不做**的保留期端点：
  管理员看到 `{"cleaned":0}` 会以为"没有过期数据"，实际是谓词永不成立。该表因此是
  append-only 无界增长（与 D-31 同属"唯一写入方漏写列"家族）。
- 状态：**未修**——C18 只把 `RETURNING *` 展开为结构体的精确列清单（`query_as!`
  不走 `FromRow`），INSERT 的**列集合与绑定原样保留**，未借静态化改行为。
- 建议处理（产品决策，二选一）：
  （a）INSERT 补写 `sent_at`（沿用同一次调用里的 `created_ts` 时间戳，
  即 `sent_at` 表示"日志落库时刻"）——需先确认 `sent_at` 的语义是"推送发送时刻"
  还是"日志写入时刻"，若为前者则应在新列语义下重新设计；
  （b）把清理条件改为 `COALESCE(sent_at, created_ts) < $1`（对存量 NULL 行也能生效）。
  无论哪种，都应先补 D-15.6 的 `cleanup_old_logs` DB 用例（RED）再改实现。
  属行为修复，需独立评审 + 独立提交。

#### D-34 产品缺陷：`get_pending_threepids` 的谓词与自身写入路径互相矛盾（C18）

> **已修（W1 `c128cdeab`）**。下面的"位置"是修复前的行号；现状见 §7.1 D-34 与 §8.6。
>
> 位置：`synapse-storage/src/threepid.rs:247`
  （`ThreepidStorage::get_pending_threepids`，谓词实际在 `:262`
  `WHERE validated_at < added_ts`）；对照写入 `synapse-storage/src/threepid.rs:157`
  （`ThreepidStorage::add_threepid` 的 `INSERT INTO user_threepids
  (user_id, medium, address, added_ts, is_verified, verification_token,
  verification_expires_at)`；方法定义起于 `:149`）。
- 证据（写入路径不产生可被该谓词匹配的行）：`add_threepid` 的列清单**没有
  `validated_at`**，而 `user_threepids.validated_at` 可空无默认 ⇒ 这些行
  `validated_at IS NULL`。`NULL < added_ts` 求值为 `NULL`，`WHERE` 不成立 ⇒
  **永远不返回**。也就是说：由正常"新增待验证 3PID"路径写入的行，在
  "列出待验证 3PID"接口里一个都看不到；能进结果的只有 `validated_at` 有值且**早于**
  `added_ts` 的行，而 `add_verified_threepid`（`synapse-storage/src/threepid.rs:415`
  起）与 `verify_threepid`（`:339` 起）都是把 `validated_at` 设为"当前时刻"，
  正常调用下 `validated_at >= added_ts` ⇒ 也不匹配。
- 证据（测试把缺陷当规格）：`synapse-storage/src/threepid.rs:1094` 的
  `test_get_pending_threepids` 无法通过 `add_threepid` 造出目标行，只能改用
  `add_verified_threepid(&user_id, "email", &address, 1, 1000)`——把 `validated_at`
  手工设成 `1`、`added_ts` 设成 `1000` 来人为满足 `validated_at < added_ts`，
  并在注释里写明"Note: the query does not filter on is_verified, so a 'verified'
  threepid with validated_at < added_ts will appear in pending results"。
  即该用例证明的是"谓词按字面执行"，而不是"待验证 3PID 能被列出"——它锁定了错误语义。
- 可达性（限制影响面）：唯一包装方是
  `IdentityStorage::get_pending_three_pid_validations`
  （`synapse-services/src/identity/storage.rs:65`，调用
  `get_pending_threepids(100)`），而
  `grep -rn 'get_pending_three_pid_validations' --include=*.rs .`（排除 `target/`）
  只命中该方法自身的定义与 doc 注释，**没有任何调用者**，`synapse-web` 侧也无对应路由
  ⇒ 目前无生产路径受影响（与 D-32 同型：缺陷 + 未接线）。
- 与静态化的关系：C18 把该 SELECT 转为 `query_as!`，**只补了
  `is_verified AS "is_verified!"` 的可空性覆盖**，`WHERE` 谓词逐字保留，未改行为。
- 状态：**未修**。
- 建议处理：谓词改为 `validated_at IS NULL OR validated_at < added_ts`
  （"尚未验证 或 验证时刻早于写入时刻"；若 `is_verified = FALSE` 才是真正的语义，
  则应显式用它），并把 `test_get_pending_threepids` 改回走 `add_threepid`（RED）；
  同时决定 `get_pending_three_pid_validations` 是接线（identity server 的
  `requestToken`/待验证查询路径）还是按铁律 1 删除。属行为/API 变更，需独立评审 +
  独立提交。

#### D-35 文档一致性：§7 导言"计数口径"行落后一个批次（C18 发现并修正）

- 位置：本文件 §7 导言的第 422 行附近（C18 修正前的"计数口径"行）。
- 证据：该行原文为
  `dynamic_production=741（其中 literal 656 / runtime 85）、static=773、
  dynamic_test=704、query_builder=18（C17 后 python3 scripts/ci/sqlx_query_census.py 实测）`。
  但 `741 / 773` 是 **C16 完成时**的数值：C17 的批次史明确记录
  `dynamic_production 773 → 742`、`static 741 → 772`，且同批把
  `BASELINE_DYNAMIC_PRODUCTION` / `BASELINE_STATIC` 分别设为 `742` / `772`。
  即导言行与 baseline 在同一提交里互相矛盾，两处各偏 1（`dynamic_production` 少 1、
  `static` 多 1）。同型的"批次间计数器漂移"即 D-16 所述的漂移家族。
- 状态：**已修**——C18 把该行更正为 C18 后的实测值（`dynamic_production=706`、
  `static=808`、`dynamic_test=704`、`query_builder=18`），并就地注明
  "741/773 实为 C16 后数值"以免后人再按旧行反推。
- 备注：D-16 已确立"计数一律以脚本 + baseline 为唯一来源"的口径；本次更正与该口径
  一致，未引入第二份计数记录。

#### D-36 覆盖缺口 / 门禁：测试自建简化 schema 掩盖写入端约束（2026-09-23 重排**新登记**）

- 类别：覆盖缺口兼**系统性根因**。本条不是既有条目的重复，而是把 D-10 / D-11 / D-31 /
  D-33 / D-34 五条"写入端漏列 ⇒ 必然失败 / 永远无效"缺陷的**同一根因**显式立项，
  以便用一个守卫覆盖整个家族（守卫方案见 §8.4）。
- 症状：这些模块的 DB 测试**不跑迁移 schema**，而是从
  `crate::test_utils::prepare_empty_isolated_test_pool()`
  （`synapse-storage/src/test_utils.rs:126`：建**空 schema、不套任何迁移**）拿一个空库，
  再由测试自己 `CREATE TABLE` 一张同名简化表。简化表丢掉了 NOT NULL / CHECK / UNIQUE
  约束，于是"真 schema 下 100% 失败"的写入路径在这些测试里是绿的。
- 证据（逐条，均为当前工作树实测）：
  - **D-31**：`background_update::db_tests` 的 `get_bu_test_pool()`（`synapse-storage/src/background_update.rs:1109`）
    调 `prepare_empty_isolated_test_pool()`，随后 `setup_background_update_db()`
    （`:989`）自建同名表，其中 `update_name TEXT,`（`:994`）**可空且无 UNIQUE**；
    测试还用 `UPDATE background_updates SET update_name = job_name WHERE update_name IS NULL`
    （注释在 `:1756`）手工补列——测试自己把生产缺陷写成了补丁。
  - **D-11**：`synapse-storage/src/registration_token/db_tests.rs:828-829` 用裸 SQL 绕过
    `create_room_invite`，注释自述 "create_room_invite is broken due to required
    inviter/invitee columns that it does not supply — pre-existing bug"。
  - **D-33**：`synapse-storage/src/push_notification.rs` 文件内 `mod db_tests` 数 = **0**
    （`grep -c 'mod db_tests'` = 0），`cleanup_old_logs` 从未被任何用例调用（D-15.6）
    ⇒ 该 no-op 保留期端点能长期潜伏。
  - **D-34**：`test_get_pending_threepids`（`synapse-storage/src/threepid.rs:1094`）无法用
    `add_threepid` 造出目标行，改用 `add_verified_threepid(…, validated_at=1, added_ts=1000)`
    人为满足旧谓词，并把"query 不过滤 is_verified"写进注释当成规格——用例锁定的是错误语义。
  - 对照：`synapse-storage/src/test_isolation.rs:50` 的 `isolated_test_pool()`
    才是"从共享 v12 模板克隆 schema"的正确入口（"Every DB test in this crate should start
    here"），迁移模板由 `synapse-common/src/test_isolation.rs:375` 的
    `ensure_template_schema` 构建。
- 影响：这五条缺陷全部是**人工对照真 schema / 迁移**才发现的（C12/C14/C17/C18），
  没有任何一条由测试暴露；同类"写入端漏列"还会继续以同样方式潜伏。
- 状态：**未修**（本次重排新登记；作为独立条目跟踪，其守卫即 §8.4 的 8.4 项）。
- 建议处理：见 §8.4（模板 schema 断言 + INSERT 列覆盖 CATALOG 检查，两者都必须先用
  故意制造的违规证明会变红）。

### 7.x 处置约定

1. **不在静态化范围内。** 静态化是**行为保持**的机械重构；本节所有条目都涉及行为、
   契约、schema 或测试口径，必须各自独立评审、独立提交。禁止把它们的修复"夹带"进
   任何一个 C 批次——那会让"编译期红证明"失去意义（无法再证明改动等价）。
2. **不受棘轮/基线阻塞，也不阻塞棘轮。** `scripts/ci/sqlx_dynamic_ratio_baseline`
   只约束"生产动态不得增、静态不得减"。处理本节条目时：只有确实回收了动态站点
   （如 D-13 改 `jsonb_to_recordset`、D-27 删模块、D-14.B 的 14 处）才需要同步下调
   `BASELINE_DYNAMIC_PRODUCTION`；纯行为修复（D-02/D-05/D-07/D-08/D-10/D-11/D-12）
   不动棘轮数字。
3. **唯一登记处。** 本节是这些发现的唯一汇总；`scripts/ci/sqlx_dynamic_ratio_baseline`
   仅保留棘轮口径（数字 + 计数理由）。未来批次发现的新问题**追加到本节**，不要在
   baseline 或其他批次文档里再开第二份清单（双份记录已导致 D-16 型漂移）。
4. **状态纪律。** 每条必须能给出 `路径:行号` 或可复现命令；已修的必须给 commit
   （`git log -S` / `git log --oneline -- <path>`）；无法核验的标 `[未验证]`；
   行号漂移时以当前树为准更正（本节的 `路径:行号` 均为 2026-09-23 撰写时实测）。
5. **建议的处理顺序**（依据影响/可达性）：D-10 / D-12 / D-31 / D-33（已注册路由、100% 失败
   或静默丢数据/静默不清理）→ D-02 类回归防护（已修，补测试）→
   D-11 / D-04 / D-27 / D-30 / D-32 / D-34（潜伏或死代码清理）
   → D-05 / D-07 / D-08 / D-09（一致性/确定性）→ D-13 / D-14（结构性回收）→
   D-15（含 D-15.6）/ D-25（补测与门禁）→ D-17（缓存收敛）→ D-06 / D-16 / D-23 / D-26（文档/注释）。

---

#### D-37 `synapse-storage::device` 里的第二份 device-list-change 实现与三处吞错（2026-09-24 W2 顺带发现）

- 类别：**冗余实现（铁律 2）+ 吞错（与 D-07 同型）+ 零调用者包装（铁律 1）**。
- 位置：`synapse-storage/src/device/mod.rs`
  - `:182` `DeviceStorage::record_device_list_change`（两条语句都 `?`，本身是**正确**的那一份）；
  - `:220` `record_device_list_change_best_effort` —— 只是 `let _ = self.record_device_list_change(…)`
    的包装，全仓 `git grep` **零调用者**；
  - `:530` / `:557` / `:595`（设备注册 / 更新 / 删除路径）三处调用点都是
    `let _ = self.record_device_list_change(…).await;` —— 与 D-07 完全同型的静默吞错，
    只是发生在 storage 层而不是 `synapse-e2ee` 层。
- 证据：`git grep -n "record_device_list_change" -- '*.rs'` 的命中集合；
  `synapse-e2ee/src/device_keys/storage.rs` 里另有一份同职责实现（D-07 已修的那份），
  两份实现的 SQL 语句逐字相同（`device_lists_stream` + `device_lists_changes`）。
- 影响：storage 层设备增删若写不成 device-list change，调用方同样不可见 —— 与 D-07 相同的
  「对端设备列表静默过期」后果。
- 可达性：**有**（`register_device` / `update_device` / `delete_device` 三条 storage 路径）。
- 状态：**未修**（本次只登记；W2 的 D-07 只覆盖 `synapse-e2ee` 侧，未越界改 storage 侧）。
- 建议处理：① 三个调用点与 D-07 对齐给出显式决定（要么 `?`，要么就地 `tracing::warn!`）；
  ② 删除零调用者的 `:220` 包装（铁律 1）；③ 评估两份实现能否收敛成一份（铁律 2）——
  两份分属不同 crate 的不同类型，收敛需要一个共享位置（`synapse-common` 或让 storage 侧成为
  唯一实现），属独立设计事项。

## 8. 问题优先处理计划（2026-09-23 重排：先修问题，再继续静态化）

> **定位**：本节是**当前唯一执行排期**。§5 的阶段表与「执行结果」的批次表降级为**历史记录**。
> §7 仍是所有既有问题的**唯一登记处**；本节只做"排序 + 每条的修复/验收定义"，不重复登记证据。
> §7.x 处置约定第 5 条（"建议的处理顺序"）自本节起由 §8.2 的分波表取代，其余 4 条约定继续有效。

### 8.1 决策与理由

**决定**：**暂停 C 批次（逐文件静态化）**，先把 §7 登记的问题按"价值 × 改动量"分波修完，
再恢复静态化。这不是放弃静态化，而是把已投入的静态化**变现**。

为什么现在停：

1. **剩下的动态预算边际收益递减。** 当前计数（`python3 scripts/ci/sqlx_query_census.py`）
   为 `dynamic_production=706` / `static=808` / `dynamic_test=704`。总动态 ≈ 1410，其中
   **约一半（704 处）是 §4 / §7 D-13 / D-14 已证明原理上无法宏化的测试基建**（动态
   schema 名、`CREATE/DROP SCHEMA`、故障注入、`VACUUM/REINDEX` 标识符、`Vec<Option<T>>`
   参数等）；余下 706 处生产动态绝大多数是**尚未被任何 C 批次覆盖的生产模块**里的字面量站点
   （`--list-production-dynamic` 的 `literal` 类）。继续按文件扫，是在已证明"必须动态"
   的残差里找零头，**降计数不再等于降风险**。
2. **campaign 正在"发现"而不是"修复"。** D-31 / D-33 / D-34 全部是 C17 / C18 期间挖出的——
   即每继续一个批次，就再多登记几条"真 schema 下必然失败"的缺陷，而 §7.x 第 1 条明令
   禁止把这些修复夹带进静态化批次（夹带会让"编译期红证明"失效）。结果是缺陷越积越多、
   一条都没修。**先修完再继续，才能把 W1–W5 的 20 条可执行项真正变成已消除的风险**
   （另有 D-36 守卫作为防复发项，不计入这 20 条）。
3. **最高价值的动作现在是修复。** §8.2 的 W1–W4 共 18 条里有 8 条是"已注册路由 / 必然失败 /
   静默丢数据 / 静默不清理"，改动量多为单列 + 单绑定或整段删除；这是当前投入产出比最高的工作。

**不变的前提**（继续生效，不因暂停而放松）：

- **棘轮继续生效**：`bash scripts/ci/check_sqlx_dynamic_ratio.sh` 仍要求
  **生产动态不得增、静态不得减**。修复过程中即使只是改 SQL 文本（如 W1 的补列），也必须
  重跑 `cargo sqlx prepare --workspace` 并让 `check_sqlx_cache_fresh.sh` 保持绿。
- **D-13 / D-14 有意保持动态**（`Vec<Option<T>>` 数组参数；真正运行期拼装 SQL + D1 守卫的
  14 处已知假阴性）；D-18…D-22 的结构性限制同样继续按既有解法沿用，**本计划不为它们排期**。
- **C 批次是可恢复的暂停**，不是终止：恢复条件与不变式见 §8.5。

### 8.2 分波处置表

分波依据 **价值 × 改动量**：W1 是"写入端漏列 ⇒ 功能必然失败"（小改、高影响），W4 是
纯死代码/卫生（小改、低影响），W5 是补测。每条的内容都按 §7 的实际 `D-NN` 与 `路径:行号`
映射（提示名不作为编号依据）。**已处置的 8 条（D-02/D-03/D-16/D-23/D-24/D-26/D-28/D-35）
不进入任何波次，排除理由见本节末。**

**W1 —— 写入端漏列 ⇒ 功能必然失败（小改、高影响）**

| 波次 | 条目 | 类别 | 严重度/影响 | 可达性 | 改动量 | 验收判据 | 依赖 |
|---|---|---|---|---|---|---|---|
| W1 | D-31 | 产品缺陷（写入端漏列） | 高：INSERT 必然 23502，admin 端点 100% 失败 | 有：`POST /_synapse/admin/v1/background_updates` | 小：INSERT 补 1 列（值取 `job_name`，可复用 `$1`）；可选收敛 `job_name`/`update_name` 双列（迁移） | 迁移模板下 `create_update`→`get_update`→`delete_update` 往返通过；重复创建按 UNIQUE 报 23505；删掉测试里的手工补列 | 无 |
| W1 | D-10 | 产品缺陷（写入端漏列） | 高：必然 23514（CHECK `ck_media_callbacks_user_id_format`） | 有：`POST /_synapse/admin/v1/media_callbacks` | 小：请求结构体 + INSERT 补 `user_id` 绑定（或显式 `NULL`） | 迁移模板下 `create_media_callback` 成功、`get_media_callbacks` 读回同一行 | 产品确认 `user_id` 语义 |
| W1 | D-11 | 产品缺陷（写入端漏列） | 高（潜伏）：接线路由即 100% 23502 | 无 HTTP 路由（service 唯一包装，未接线） | 小：INSERT 补 `inviter`/`invitee` 绑定（或产品决定删冗余列 + 迁移） | `registration_token/db_tests` 删除裸 SQL 绕过（`:828-829`），改走 `create_room_invite` 并读回两列 | 产品决策：映射 vs 删列 |
| W1 | D-33 | 产品缺陷（写入端漏列） | 高：保留期端点恒 `{"cleaned":0}`，append-only 表无界增长 | 有：`POST /_synapse/admin/v1/push_notification/cleanup` | 小：INSERT 补 `sent_at`（同 `created_ts`）**或** DELETE 改 `COALESCE(sent_at, created_ts) < $1` | 先补 D-15.6 的 `cleanup_old_logs` DB 用例（RED：删 0 行 → GREEN：删 1 行） | 语义决策：`sent_at` = 发送时刻 or 落库时刻 |
| W1 | D-34 | 产品缺陷（谓词与写入矛盾） | 中（潜伏）：正常写入的待验证 3PID 永远列不出 | 无（唯一包装 `get_pending_three_pid_validations` 零调用者） | 小：谓词改 `validated_at IS NULL OR validated_at < added_ts` + 用例改走 `add_threepid` | 用 `add_threepid` 造行后 `get_pending_threepids` 必须返回该行（RED→GREEN）；并删除把缺陷当规格的注释 | 产品决策：包装方接线 or 删 |

**W2 —— 正确性 / 数据一致性**

| 波次 | 条目 | 类别 | 严重度/影响 | 可达性 | 改动量 | 验收判据 | 依赖 |
|---|---|---|---|---|---|---|---|
| W2 | D-09 | 产品缺陷（解码类型） | 中高：`suggested_only` 分支一旦真返回行即 `ColumnDecode` 失败 | 有：federation hierarchy（`suggested_only=true`） | 小：`:716` 改 `ARRAY(SELECT jsonb_array_elements_text(via_servers))`（同族 5 处已是此写法） | 新增 `is_suggested=true` 的 DB 用例（现有 `db_tests.rs:747` 夹具 0 行，未触发解码） | 无 |
| W2 | D-08 | 数据一致性 / 安全相邻 | 中：`LIMIT 1` 无 `ORDER BY`，OTK 选取非确定 | 有：OTK claim 路径 | 小：`target`（`:732`）与 `fb`（`:775`）两条 CTE 各加 `ORDER BY added_ts, id` | 同一 `(user, device, algorithm)` 多把未用密钥时，连续 claim 严格按 `added_ts, id` 顺序发放 | 无 |
| W2 | D-07 | 数据一致性 | 中：`stream_id` 插入失败被完全吞掉，不可观测 | 有：设备列表变更写路径 | 小：至少 `tracing::warn!` + 指标；或改 `?` 让调用方可失败 | 故障注入（令插入失败）下断言 warn/指标出现或错误向上传播，而非静默 `Ok` | 语义决策：best-effort 是否允许静默 |
| W2 | D-05 | 数据一致性 | 中低：`DeviceKey.id` 恒 0，而 `device_keys.id` 是 BIGSERIAL 主键（暴露给客户端） | 生产不读；仅 2 处纯单测断言 id | 小：投影 `id` 返回真主键，或按铁律 1 删字段 | 若保留：DB 往返断言 `id` 等于真实主键；若删除：编译期证明无消费方 | API 是否需要 `id` |

> **状态（2026-09-24）**：W2 四条已全部修复入库（`cef006dd2`）。证据与回归面见 **§8.8**；
> D-05 取「按铁律 1 删字段」、D-07 取「错误向上传播」，理由见 §8.8 与 §7 就地状态。

**W3 —— 契约说谎 / 静默丢弃**

| 波次 | 条目 | 类别 | 严重度/影响 | 可达性 | 改动量 | 验收判据 | 依赖 |
|---|---|---|---|---|---|---|---|
| W3 | D-29 | 契约说谎 | 中：doc 承诺的 `Some(None)` 不可达，消费端死分支仍在 | 有：`admission_mode` 下每个联邦请求经过；分支本身不可达 | 小：storage 返回类型收窄 `Option<String>` + 删 service `:486` 分支 + 改 doc | 编译期证明 `Some(None)` 分支消失；`db_tests` 未知/已存在两例仍绿 | 行为契约变更，需独立评审 |
| W3 | D-32 | 契约说谎 + 死代码 | 中低：批量 presence upsert 与其批量联邦广播从未接线 | 无（仅 db_tests） | 中：接线到联邦 presence EDU 批处理 / 批量导入，**或**按铁律 1 删除 batch API | 删除路径：全仓无 `set_presence_batch` 引用且棘轮 -1；接线路径：批量入口有集成用例 | 产品确认是否真有批量场景 |
| W3 | D-12 | 产品缺陷（**大**） | 高：审核历史静默丢弃，两个 admin 端点恒空 | 有：`GET …/{id}/history`、`GET …/stats` 路由已注册 | **大**：需建 `event_report_history`/`event_report_stats` 表 + 落地 `add_history`/`get_report_history` + `get_stats` 改聚合 SQL；**先决定两个端点是否临时下线**（当前返回空会被误读为"没有历史"） | 建表迁移 + `add_history` 落库；history/stats 端点在 DB 用例下返回非空且可断言 | 独立功能批次；表结构设计 —— **建议在本轮最后单独排期，不塞进 W1–W4 的快速修复** |

**W4 —— 死代码与卫生**

| 波次 | 条目 | 类别 | 严重度/影响 | 可达性 | 改动量 | 验收判据 | 依赖 |
|---|---|---|---|---|---|---|---|
| W4 | D-01 | 死代码 | 低：SQL 语法已修（`0e1716643`）但 0 调用者函数仍在 | 无 | 小：删整个函数 | 全仓无引用；棘轮 -1（同步下调基线） | 无 |
| W4 | D-04 | 死代码（潜伏） | 低：自建 DDL 缺 `fallback_used` 且 0 调用者 | 无 | 小：删 `create_tables()`（**不要**补列——那会造第二份 schema 真源，违反铁律 2/4） | 全仓无 `.create_tables(`；`migrations/` 仍是唯一 schema 真源 | 无 |
| W4 | D-27 | 死代码 | 低：整模块无消费者，却带 8 处生产动态 | 无（`sync/mod.rs:10` 再导出无人消费） | 小：删模块 + 再导出 | 全仓无 `SearchIndexStorage` 外部引用；棘轮 -8 | 无 |
| W4 | D-30 | 死代码（不可达分支） | 中低：4 处回退分支不可达，且阻塞宏化 | 回退分支不可达（主分支正常） | 小：删 4 个回退分支 + `is_undefined_column_error`（`:16`） | 删后 presence 生产动态 4→0；棘轮 -4 | 分支删除属行为变更，需独立评审 |
| W4 | D-06 | 卫生 | 无（cosmetic） | 无 | 小：清理 `DeviceKeyRow` 每字段重复的 doc 注释 | 每字段一行；无 `.sqlx`/棘轮影响 | 无 |
| W4 | D-17 | 卫生 / 冗余 | 低：同一职责两份离线缓存（根 **782** / 子 **53**；34 相同、0 冲突、19 仅存子目录） | 影响 `SQLX_OFFLINE` 编译与新鲜度门禁 | 小：`git rm -r --cached synapse-storage/.sqlx` 后清理，统一根缓存 | 先 `SQLX_OFFLINE=true cargo check --workspace --all-features` 证明不需要子目录；`check_sqlx_cache_fresh.sh` 绿 | 需先确认那 19 条非陈旧（§7 D-17 标 `[未验证]`） |

**W5 —— 覆盖缺口**

| 波次 | 条目 | 类别 | 严重度/影响 | 可达性 | 改动量 | 验收判据 | 依赖 |
|---|---|---|---|---|---|---|---|
| W5 | D-15（含 D-15.1–D-15.6） | 覆盖缺口 | 中：已静态化代码无 DB 往返/游标分支；D-15.6 正是 D-33 长期潜伏之因 | — | 中：逐子项补迁移模板下的 DB 用例 | 每子项先写 RED 用例再收口；D-15.2 经核实集成侧已覆盖（降级为可选）；D-15.5 方法数为 12 而非 13 | D-15.6 依赖 W1 的 D-33 语义决策 |
| W5 | D-25 | 覆盖缺口 / 门禁 | 中：门控模块 "0 tests" 假绿（曾 4 次踩到） | 不体现在棘轮数字里 | 小：批次 procedure / CI 记录每个门控模块所需 feature 集，过滤器 0 命中即失败 | 故意去掉某门控模块的 feature 跑守卫 → 必须变红（铁律 8） | 无 |

**守卫（8.4，本次新登记为 D-36）**

| 波次 | 条目 | 类别 | 严重度/影响 | 可达性 | 改动量 | 验收判据 | 依赖 |
|---|---|---|---|---|---|---|---|
| 守卫 | D-36 | 覆盖缺口 / 门禁（系统性根因） | 高：D-10/D-11/D-31/D-33/D-34 的共同根因 | — | 小–中：静态守卫 + 一个 CATALOG 检查 | 见 §8.4；两条守卫都必须先用违规探针证明会变红 | 以 W1 五条为 RED 样本 |

**不做（结构性，有意保留）**

| 条目 | 类别 | 为什么不做 |
|---|---|---|
| D-13 | 结构性限制 | `Vec<Option<T>>` 数组参数无 sqlx 映射（3 处 `literal` 已计入棘轮）；回收方向 `jsonb_to_recordset` 属独立改造，本计划不排期 |
| D-14 | 结构性限制 | 真正运行期拼装 SQL（41 处 / 12 文件 + `QueryBuilder`）+ D1 守卫 14 处已知假阴性；**有意保留**，逐文件回收方向见 §7 D-14 |
| D-18 / D-19 / D-20 / D-21 / D-22 | 结构性限制 | 仅排序列 / `query_as!` 不认 rename-skip / LEFT JOIN 外侧列空值 / `&Option<T>` 绑定 / `RETURNING *` 展开——解法均已落地并写成批次 checklist，沿用即可，本计划不新增工作 |

**已处置、不进入波次的 8 条（排除理由）**：D-02（已修 `cbe718ff6`）、D-03（已修
`0f6a76c13` + S1–S4）、D-24（已修 `483dfc045`）、D-28（B3 已删 4 个死查询 `2e9c3d11d`）、
D-35（C18 已更正计数）、D-16（已修正 C11 行）、D-23（已绕过，无遗留）、D-26（已收窄结论）。
其中 D-02/D-03/D-24/D-28/D-35 的"补测试/回归防护"若要做，归入 W5 的同型补测，不再单列条目。

### 8.3 第一波详情

W1 五条都是"写入端漏列（或谓词与写入矛盾）⇒ 操作必然失败 / 永远无效"。**统一原则**：
先让测试跑在**真迁移 schema** 上（RED），再改写入端或谓词（GREEN）；**不要**用"手工补列 /
裸 SQL 绕过 / 自建简化表"把 RED 抹平——那正是 D-36 的根因。

> **状态（2026-09-24）**：五条已全部修复入库（`c128cdeab`）。各条的 RED/GREEN 实证、
> 回归面与顺带关闭的条目见 **§8.6**；本节以下内容保留为修复前的现场记录（行号为修复前）。

#### 8.3.1 D-31 `create_update` 漏写 `update_name`

- **修复**：在 INSERT 列清单加入 `update_name`，并绑定到已有的 `job_name` 参数（`$1`），
  即 `update_name = job_name`；这是与其它所有读写（`get_update` / `update_status` /
  `update_progress` / `set_error` / `delete_update` / `retry_failed` 全按 `update_name`
  定位）一致的唯一选择。是否把冗余的 `job_name` 列按铁律 1 收敛掉（需迁移）是并行的产品决策，
  不阻塞本修复。
- **精确位置**：`synapse-storage/src/background_update.rs:275`（`INSERT INTO background_updates (`）、
  `:276-277`（列清单）、`:278`（VALUES）；改后 VALUES 需多加一个占位（可复用 `$1`，无需新增绑定）。
- **验收测试**：新增 `background_update::db_tests::test_create_update_roundtrip`：
  1. 把 `get_bu_test_pool()`（`:1109`）由 `prepare_empty_isolated_test_pool()` 改为
     `crate::test_isolation::isolated_test_pool()`（v12 模板克隆，
     `synapse-storage/src/test_isolation.rs:50`），并**删除** `setup_background_update_db`
     （`:989`）里的自建 `CREATE TABLE`；
  2. 断言 `create_update` 返回行、`get_update(job_name)` 命中、`delete_update` 生效；
  3. 断言重复 `create_update(同名)` 命中 UNIQUE 约束（23505，或映射后的领域错误）。
- **红/绿证明**：**RED** = 只做第 1 步（测试切到迁移模板）、不动实现 ⇒ `create_update`
  立刻 `23502`；**GREEN** = 补 `update_name` 列与绑定后，以上三条断言全绿；同时删除
  `:1756` 附近"手工 `UPDATE … SET update_name = job_name`"的测试补丁（它存在的唯一理由就是本缺陷）。

#### 8.3.2 D-10 `create_media_callback` 不写 `user_id`

- **修复**：INSERT 列清单补 `user_id`，绑定 `request.user_id`；若产品确认回调不归属任何用户，
  则显式绑定 `NULL`（`media_callbacks.user_id` 可空，约束允许 NULL）。**不能**继续依赖
  `DEFAULT ''`——`''` 违反 v12 的 `ck_media_callbacks_user_id_format`，这正是 23514 的来源。
- **精确位置**：`synapse-storage/src/module.rs:949`（INSERT）、`:950`（列清单）、
  `:952`（VALUES）；请求结构体 `CreateMediaCallbackRequest` 在 `synapse-storage/src/module.rs:338`。
- **验收测试**：在 `module.rs` 新建 `db_tests`（当前文件内 `test_` 全为纯单元，无
  `require_test_pool`，即 D-15.1 的"无任何 DB 往返"），用 `isolated_test_pool()`：
  1. `create_media_callback` 成功返回；
  2. `get_media_callbacks(Some(callback_type))` 读回同一行，且 `user_id` 等于所选语义的值。
- **红/绿证明**：**RED** = 迁移 schema 下当前实现 ⇒ `23514`
  （`ck_media_callbacks_user_id_format`）；**GREEN** = 补列绑定后两条断言通过。
  该用例同时关闭 D-15.1 的 module 覆盖缺口。

#### 8.3.3 D-11 `create_room_invite` 漏写 `inviter`/`invitee`

- **修复**：INSERT 列清单补 `inviter`、`invitee` 两列（都是 `TEXT NOT NULL` 且**无默认值**，
  `migrations/00000000_unified_schema_v12.sql:566-567`），按 §7 建议映射
  `inviter = request.inviter_user_id`、`invitee = request.invitee_email`；替代方案是产品
  决定删除这两个冗余列（需迁移）。**未决前不要接线到路由**（否则等于把潜伏缺陷变成在线故障）。
- **精确位置**：`synapse-storage/src/registration_token/repository.rs:369`（INSERT）、
  `:370`（列清单）、`:372`（VALUES）。
- **验收测试**：改写 `synapse-storage/src/registration_token/db_tests.rs:828-829` 的裸 SQL
  绕过，改为调用 `create_room_invite` 并用 `get_room_invite(invite_code)` 读回，断言
  `inviter`/`invitee` 等于请求值。
- **红/绿证明**：**RED** = 删掉裸 SQL 绕过、在迁移 schema 上直接调用现实现 ⇒ `23502`
  （null value in column "inviter"）；**GREEN** = 补两列绑定后往返通过。

#### 8.3.4 D-33 `push_notification_log.sent_at` 从未写入 ⇒ 清理永远删 0 行

- **修复**（二选一，先定语义）：
  - **(a)** 在 INSERT 列清单补 `sent_at`，绑定与同一调用中 `created_ts` 相同的 `now`
    时间戳 —— 语义 = "日志落库时刻"；需确认这不与 `sent_at` 的原始设计语义
    （"推送发送时刻"）冲突；若确为后者，应另设列并重新设计，而不是复用。
  - **(b)** 保留写入不变，把清理谓词改为 `WHERE COALESCE(sent_at, created_ts) < $1`
    —— 对**存量** `sent_at IS NULL` 行也立即生效，是能马上止血的选项。
  §7 倾向先做 (b)（无需回填历史数据），(a) 作为后续语义收敛。
- **精确位置**：写入 `synapse-storage/src/push_notification.rs:614`（INSERT）、
  `:615-616`（列清单）、`:617`（VALUES）；清理 `:720`（`DELETE … WHERE sent_at < $1`）。
- **验收测试**：先补 D-15.6 的 `db_tests`（该文件当前 `mod db_tests` 数 = **0**）：
  1. `test_create_notification_log_roundtrip` —— 断言落库成功；若选 (a) 另断言 `sent_at` 非空；
  2. `test_cleanup_old_logs_deletes_expired` —— 在迁移模板下写入一条 `created_ts` 远早于
     阈值（且 `sent_at` 为 NULL）的行，`cleanup_old_logs(days)` 必须删除 ≥1 行；
  3. 负例：`created_ts` 在阈值内的行**不得**被删（防止 (b) 误伤）。
- **红/绿证明**：**RED** = 迁移 schema + 当前实现，用例 2 断言 `rows_affected() == 1`
  但实际恒为 `0`（对应路由恒返回 `{"cleaned":0}`）；**GREEN** = 选 (a) 或 (b) 后
  用例 2 与负例同时通过。

#### 8.3.5 D-34 `get_pending_threepids` 谓词与 `add_threepid` 写入矛盾

- **修复（选谓词侧）**：把 `WHERE validated_at < added_ts` 改为
  `WHERE validated_at IS NULL OR validated_at < added_ts`。理由：文档意图是"列出**待验证**
  3PID"，而正常写入路径 `add_threepid` 产出的正是 `validated_at IS NULL` 的行；改写入端
  （硬塞一个 `validated_at`）会破坏"验证前为空"的语义，是错的一侧。若真正的语义是
  "未验证"，则应显式写 `COALESCE(is_verified, FALSE) = FALSE` 而不是依赖 `validated_at`
  的时间比较（`is_verified` 在 catalog 中可空、默认 false，需配 COALESCE）。
- **精确位置**：`synapse-storage/src/threepid.rs:247`（方法）、`:262`（谓词
  `WHERE validated_at < added_ts`）；写入对照 `:157`（`add_threepid` 的 INSERT，无
  `validated_at`）；用例 `:1094`。
- **验收测试**：把 `test_get_pending_threepids`（`:1094`）改回用 `add_threepid` 造行，
  断言返回该行；新增负例"`add_verified_threepid` 写入且 `validated_at >= added_ts` 的行
  不返回"；删除注释里"query 不过滤 is_verified，所以已验证的行也会出现在 pending 里"
  这类把缺陷当规格的表述。
- **红/绿证明**：**RED** = 把用例改走 `add_threepid` 后，当前谓词返回 **0** 行；
  **GREEN** = 改谓词后返回 1 行，且负例仍返回 0 行。
- **遗留决策**（不阻塞本修复）：唯一包装 `IdentityStorage::get_pending_three_pid_validations`
  （`synapse-services/src/identity/storage.rs:65`）全仓零调用者 —— 接线到 identity server 的
  requestToken / 待验证查询路径，或按铁律 1 删除；属独立条目。

### 8.4 系统性根因与守卫（D-36）

§7 的五条"写入端漏列"缺陷（D-10 / D-11 / D-31 / D-33 / D-34）有**同一根因**：这些模块的
DB 测试跑在**自建简化表**上（`prepare_empty_isolated_test_pool()` 给空 schema，测试自己
`CREATE TABLE`），于是 NOT NULL / CHECK / UNIQUE 约束被抹掉、"写入端漏列"在测试里永远绿。
D-36 已把该根因单独登记为条目；下面是**可落地且便宜**的守卫提案（两条，各自必须证明能变红）。

**守卫 A —— 模板 schema 断言（静态、无需 DB、可进 `tests/unit`）**
- 扩展已有词法扫描器 `scripts/ci/sqlx_query_census.py`（它已做注释/字符串剥离），新增
  `--list-test-ddl` 模式：对每个 `#[cfg(test)]` 模块 / `mod db_tests`，报告其中出现的
  `CREATE TABLE` / `ALTER TABLE` / `CREATE SCHEMA` 调用。
- 新增 `tests/unit/` 守卫测试：调用该模式，任何命中都必须出现在显式 allowlist
  `scripts/ci/test_ddl_allowlist` 中；allowlist 的键是 **`path::mod`（函数/模块级）**，
  **不是行号**（行号型 allowlist 会随 `cargo fmt` 漂移 —— 见铁律 8 与
  `scripts/shell_routes_allowlist.txt` 前车之鉴）。
- 语义：一个 db_test 若自建表，就不可能发现"写入端 vs 迁移 schema"的漂移（D-31 正是如此）。
  正统入口是 `crate::test_isolation::isolated_test_pool()`
  （`synapse-storage/src/test_isolation.rs:50`，从共享 v12 模板克隆；模板由
  `synapse-common/src/test_isolation.rs:375` 的 `ensure_template_schema` 构建）。
- **第一步（可独立提交，不需要守卫本体）**：把 `background_update::db_tests` 的
  `get_bu_test_pool()`（`synapse-storage/src/background_update.rs:1109`）从
  `prepare_empty_isolated_test_pool()` 切到 `isolated_test_pool()`，删掉
  `setup_background_update_db` 的自建表 —— 这一步单独就会让 D-31 变红（见 §8.3.1）。

**守卫 B —— INSERT 列覆盖 CATALOG 检查（一个 DB 连接，迁移 schema）**
- 用同一个扫描器静态抽出生产源码里所有**列清单为字面量**的 `INSERT INTO <table> (<cols>)`
  （当前生产 INSERT 已大面积宏化，抽取代价低）。
- 新增单个集成测试 `insert_column_coverage_tests`：对每个 `(table, cols)`，在**已迁移到最新
  schema 的测试库**上查 catalog，断言：
  1. `information_schema.columns` 中 `is_nullable = 'NO' AND column_default IS NULL`
     的每一列都在 `cols` 中；否则失败（或该 `table.column` 出现在
     `scripts/ci/insert_column_allowlist`，键同样用 `path::table`，不用行号）；
  2. 迁移生成的 `ck_<table>_user_id_format` 家族（v12 的 DO 循环，
     `migrations/00000000_unified_schema_v12.sql:4786-4816`）要求 `cols` 覆盖 `user_id`
     —— 这一条专抓 D-10（其 `user_id` 有 `DEFAULT ''`，只查 NOT NULL 会漏掉）。
- 为什么便宜：不读业务代码、不连多个库，只需一次 `information_schema` / `pg_constraint` 查询；
  也没有"评估任意 CHECK 谓词"的难题 —— 机器生成的约束族按**约束名**判定所需列。
- **已知边界**：列清单动态拼装的 INSERT（`format!`/`QueryBuilder`）抽不到，会被记为
  "未覆盖"而不是"通过"；这类站点数量少且已由 §7 D-14 登记，作为 allowlist 的显式条目处理。

**红证明（铁律 8：门禁必须自证能变红）**
- 守卫 A：临时加一个自建 `CREATE TABLE` 的探针 db_test（或临时把 allowlist 清空）⇒ 守卫必须
  失败；随后删除探针。
- 守卫 B：在 W1 修复**之前**先跑该检查 ⇒ 必须报出 `background_updates.update_name`（D-31）与
  `media_callbacks.user_id`（D-10）缺失（D-11 的 `room_invites.inviter/invitee` 亦然）；
  W1 修完后必须转绿。这两次红/绿就是守卫的验收证据，随守卫实现同批提交。

### 8.5 暂停期不变式（C 批次恢复的条件）

暂停期间以下不变式持续成立，用以保证 C 批次恢复时循环仍然可复现：

1. **棘轮继续生效，且只许单向收紧。** 生产静态化暂停，但
   `bash scripts/ci/check_sqlx_dynamic_ratio.sh` 仍必须绿：`dynamic_production` 不得**增**、
   `static` 不得**减**。修复波次中**确实回收**动态站点的条目（D-01 删 1、D-27 删 8、
   D-30 删 4、D-32 删 1、D-14.B 若做则 14）必须在**同一提交**里下调
   `BASELINE_DYNAMIC_PRODUCTION` 与相应 literal/runtime 基线；纯行为修复
   （D-05/D-07/D-08/D-09/D-10/D-11/D-12/D-31/D-33/D-34）不动棘轮数字。
2. **`.sqlx` 永不缩小，且随 SQL 变更刷新。** 任何**改动 SQL 文本**的修复
   （W1 的 D-10/D-11/D-31/D-33/D-34、W2 的 D-08/D-09）必须重跑
   `cargo sqlx prepare --workspace`，并让
   `bash scripts/ci/check_sqlx_cache_fresh.sh`（`git diff --exit-code -- .sqlx`）保持绿。
   D-17 的缓存收敛只做"合并到根 `.sqlx/`"，**不得**删除任何仍在使用的条目，收敛后
   `SQLX_OFFLINE=true cargo check --workspace --all-features` 必须通过。
3. **feature 集只增不减。** W1/W5 的用例必须带齐模块编译所需的 feature（D-25：曾 4 次
   "0 tests" 假绿）；恢复 C 批次时，批次 procedure 里记录的 feature 集是**只增**的集合，
   且"过滤器命中 0"必须判失败而不是通过。
4. **§7 是唯一登记处。** 修复过程中发现的新问题一律追加到 §7（D-36 已按此登记）；守卫与本
   计划的进展不另开清单。已完成条目的状态就地更新（`已修（<commit>）`），不删除条目。
5. **恢复 C 批次的前置条件。** 至少 W1–W4 完成（W5 可与 C 批次并行），且 §8.3 的五条各自的
   红/绿证据、§8.4 守卫 A/B 的红证明都已入库；恢复时从 C19 起，按"一个文件/模块一批、
   独立提交、独立降基线"的既有节奏继续，不改动 §7 条目。

### 8.6 W1 执行结果（2026-09-24，`c128cdeab`）

W1 五条全部修复并入库。**证据链**：先只改测试/夹具（`RED`）证明缺陷在真迁移 schema 下
确实成立，再改实现（`GREEN`）。所有用例跑在 `test_isolation::isolated_test_pool()`
克隆的 v12 模板上，**没有**任何"手工补列 / 裸 SQL 绕过 / 自建简化表"（那正是 D-36 的根因）。

| 条目 | 修复 | RED 证据（未改实现时） | GREEN 证据 |
|---|---|---|---|
| D-31 | INSERT 补 `update_name = job_name`（同一 `$1`）；测试池切到迁移模板、删自建表与 `:1756` 手工补列 | `23502 null value in column "update_name" of relation "background_updates"` | `test_create_update_roundtrip`：往返 + 重复名 `23505` + `delete_update` 后不可读 |
| D-10 | 请求结构体 + INSERT 补 `user_id`；管理路由传认证管理员 id | `23514 … violates check constraint "ck_media_callbacks_user_id_format"` | `test_create_media_callback_roundtrip_writes_user_id` + 约束存在性对照用例 |
| D-11 | 删 `room_invites` 6 个死列 + `idx_room_invites_invitee` + 2 条 legacy 注释（铁律 1） | HEAD 语句直跑：`23502 null value in column "inviter"` | `test_get_room_invite_found_and_not_found` 改走 `create_room_invite` → `get_room_invite` |
| D-33 | INSERT 写 `sent_at`（= `created_ts` 的同一 `now`）**且** 清理改 `COALESCE(sent_at, created_ts) < $1` | `sent_at` 恒 `None`；`cleanup_old_logs` 删 **0** 行（断言 1） | 落库用例 + 过期行被回收（=1）+ 窗口内行保留（=0）三条 |
| D-34 | 谓词改 `validated_at IS NULL OR validated_at < added_ts` | `get_pending_threepids` 返回 **0** 行（断言 1） | 主用例（`add_threepid` 造行后返回 1）+ 负例（`validated_at > added_ts` 不返回） |

**回归面**：`background_update` / `registration_token` / `push_notification` / `threepid` /
`module` 五个模块 **128 个用例全绿**（D-31 的池切换影响面最大）；`static` 棘轮
`706 / 808` 不变（纯行为修复）；`.sqlx` 由 `cargo sqlx prepare --workspace --
--features server-notifications,saml-sso,cas-sso,beacons` 刷新（-5/+5，总数 782）；
`check_sqlx_cache_fresh.sh`、`check_migration_consistency.py`、基线合并检查、
schema 表/合同覆盖（211/211、100%）全绿；两档 clippy 矩阵（`--features test-utils`
与 `--all-features`）均 `-D warnings` 通过。

**本波顺带关闭 / 更新的条目**

- **D-15.1**（`module.rs` 无任何 DB 往返）随 D-10 的新建 `module::db_tests` 关闭。
- **D-15.6**（`push_notification` 零 DB 覆盖）随 D-33 的新建 `push_notification::db_tests`
  **部分**关闭：本次只补 `create_notification_log` 与 `cleanup_old_logs` 两条主线；
  `register_device` upsert、`get_pending_notifications` 的 `FOR UPDATE SKIP LOCKED`、
  `mark_notification_failed` 两分支、`push_config` 四方法仍无运行期覆盖（留在 W5）。
- **D-17 边界**：本波只改 SQL 文本并重刷**根** `.sqlx/`；子目录 `synapse-storage/.sqlx/`
  （53 条）未动、仍待收敛（W4）。
- **D-11 遗留**：删列后 `create_room_invite` 已可直接工作，但唯一包装
  `registration_token_service.rs:243` 仍无 HTTP 路由调用方；是否接线由产品决定。
- **D-34 遗留**：`IdentityStorage::get_pending_three_pid_validations`
  （`synapse-services/src/identity/storage.rs:65`）仍零调用者 —— 接线或按铁律 1 删除。

**W1 未包含**：§8.4 的守卫 A/B（D-36）本体未做，两者都仍待实现并用违规探针自证变红；
W1 的 5 条 RED 样本（`23502 update_name` / `23514 media_callbacks.user_id` /
`23502 inviter`）可直接作为**守卫 B** 的验收样本。

### 8.7 D-36 守卫落地结果（2026-09-24，`7cd40a418`）

§8.4 的两条守卫都已实现，并各自用**违规探针**证明会变红（铁律 8）。扫描后端复用
`scripts/ci/sqlx_query_census.py`（新增 `--list-test-ddl` / `--emit-inserts` 两个模式），
没有第二份词法实现。

| 守卫 | 实现 | 覆盖口径 | 红证明 | 当前状态 |
|---|---|---|---|---|
| A | `tests/unit/test_ddl_guard_tests.rs` + `scripts/ci/test_ddl_allowlist` | test 区出现 `CREATE TABLE`/`ALTER TABLE`/`CREATE SCHEMA`/`DROP SCHEMA` 即需登记；键 `path::mod::fn`（无行号） | 违规探针先红后绿；另有三条辅证：扫描器必须命中已知站点、名单条目必须仍命中、注释散文不计而字符串夹具计 | 189 处命中 / 63 个键；5 条用例全绿 |
| B | `tests/integration/insert_column_coverage_tests.rs` + `scripts/ci/insert_column_allowlist` | R1 `NOT NULL` 无默认（且非 identity/generated）列必须出现在 INSERT 字面量列清单；R2 `ck_<table>_user_id_format` 且 `user_id NOT NULL` 的表必须含 `user_id`（专抓 `NOT NULL DEFAULT ''`，D-10 形态） | 探针表复刻 R1/R2 两种形态 → 两条规则都报违规，补全列清单与登记名单键后转绿；`dynamic` 列清单报「未覆盖」而非通过 | 生产 269 条 INSERT；当前仅 1 处命中（D-30 的不可达分支，已登记） |

**历史重放 RED 证据（守卫 B 的验收样本）**：`SYNAPSE_SQL_GUARD_ROOT=<2192a6d99 checkout>`
运行守卫 B，报出

```
synapse-storage/src/background_update.rs:274 INSERT INTO `background_updates` omits required column(s) update_name
synapse-storage/src/module.rs:948 INSERT INTO `media_callbacks` omits required column(s) user_id
```

即 W1 的 D-31 与 D-10；对当前树运行则转绿。这正是 §8.4 要求的"守卫 B 红证明"。

**守卫 A 里已登记的系统性根因（本波未修，属独立条目）**：
`synapse-services/src/test_utils.rs::ensure_test_schema_contract` 在 `DatabaseInitMode::Strict`
下自建 schema（31 处 DDL），是迁移 baseline 之外的第二份 schema 真源 —— 铁律 2 的问题，
已在 `test_ddl_allowlist` 就地注明，收敛时同批处理。名单里其余 D-36 类条目
（`refresh_token` / `device` / `oidc_session_storage` / `feature_flags` / `pruning` 等）
逐条迁移到 `isolated_test_pool()` 后必须同时删行（守卫 A 有"名单条目必须仍命中"的辅证钉住）。

### 8.8 W2 执行结果（2026-09-24，`cef006dd2`）

W2 四条全部修复。每条都是**先写出会红的用例、再改实现**。

| 条目 | 修复 | RED 实证（未改实现时） | GREEN 实证 |
|---|---|---|---|
| D-09 | `suggested_only` 分支改用 `ARRAY(SELECT jsonb_array_elements_text(via_servers))` | `ColumnDecode { index: "5", source: "encountered an array of 22749793 dimensions; only one-dimensional arrays are supported" }` | `space::db_tests::test_recursive_hierarchy_suggested_only_decodes_jsonb_via_servers`：只有 suggested child 返回，两个 via_server 按序往返 |
| D-08 | `target`/`fb` 两条 CTE 各加 `ORDER BY added_ts, id` | `left: ["NEWEST","MIDDLE","OLDEST"]`（heap 顺序）vs `right: ["OLDEST","MIDDLE","NEWEST"]` | 连续 3 次 claim 严格按 `added_ts` 升序发放 |
| D-07 | `record_device_list_change` 改为 `Result<(), ApiError>` + 两条语句 `map_err(…)?`；上传/删除路径 `?`，cross-signing 侧同样可失败 | 保留可失败签名、把 body 换回吞错版本：两条故障注入用例在 `expect_err` 处失败（实得 `Ok(())`） | 注入 `device_lists_stream`/`device_lists_changes` 的 CHECK 约束后分别返回 `Err`；并断言部分失败的数据形态（stream 行在、change 行不在） |
| D-05 | 删除 `DeviceKey.id`（铁律 1） | 字段恒为伪造常量 `0`；无 RED 可写之处在于**旧代码根本不可观测**（`id` 无投影来源） | 编译期证明：7 处构造点的 `id: 0/1` 与 2 处断言消失；`synapse-e2ee --lib` 456/456 |

**故障注入的一个陷阱（已写进用例注释）**：隔离池的 `search_path` 是 `"<schema>", public`，
所以"`DROP TABLE` 掉本测试的表"**不是**有效的失败注入 —— 未限定的 INSERT 会落到
`public` 里的同名表并成功（实测第一次就踩到）。改用"给本测试表加一条只拒绝本用例取值的
`CHECK` 约束"。

**回归面**：`space` 模块 61/61、`synapse-e2ee --lib` 456/456、services 相关 18/18、
新增 integration 4/4；棘轮 `706 / 808` 不变（新用例全在 `tests/` 或 test 区，且未新增动态
SQL）；`.sqlx` 刷新 −3/+3（D-09 投影 1 条 + 两条 CTE）；两档 clippy `-D warnings` 通过；
fmt 债务 0；migration 一致性检查 0 issue。

**W2 顺带发现（登记待办）**：
`synapse-storage/src/device/mod.rs` 里有**同一职责的第二份实现**（`DeviceStorage::record_device_list_change`
及其 `record_device_list_change_best_effort` 包装），且 `:530`/`:557`/`:595` 三处调用点同样是
`let _ = …` 吞错；其中 `record_device_list_change_best_effort` 全仓**零调用者**（铁律 1）。
本次未动该文件（超出 D-07 登记范围），作为新条目登记为 **D-37**。
