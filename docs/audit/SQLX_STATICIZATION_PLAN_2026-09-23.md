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
