# SQLx 静态化：复核与优化方案（2026-10-15）

> **本文档是 `docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md` 的复核增量**，不改其正文；
> 冲突时以本文的**实测**为准（该文档最后一次口径是 2026-09-30，已漂移）。
> 复核命令全部可整行复制；所有数字都是本机 2026-10-15 真跑输出。

---

## 一、结论先行

1. **静态化本身是成功的**：`dynamic_production` 从 **1532 → 96**（−93.7%），**机械可转换的字面量在真·生产区已为 0**（今天仍为 0），
   登记缺陷 **111 条全部关闭或转结构性例外**。这条结论**没有**被本次复核推翻。
2. **但四道棘轮里有一道现在是红的**，而且**它红的数值是错的**：门禁报「测试区 758 → 763（+5）」，
   而真实是 **758 → 769（+11）** —— 有 **6 处**被门禁**算进了生产区**。
   ⇒ 这是"**棘轮少报违规**"，比假红危险：它不会拦住 6 处新增。
3. 根因是**工具的静态分析缺陷**（不是 SQL 写错）：`#[cfg(test)] mod x;` 若声明在**非模块根**文件
   （`foo.rs` 而非 `mod.rs`/`lib.rs`），其目标文件的动态 SQL 会被判成**生产区**。
4. 引入时点已定位到具体提交：**`db2ba7916`**（"refactor(size): 外置 4 个超限文件的测试模块"）。
5. 另有 3 道**绿道有余量**（生产名义只剩 1 处额度而真实有 7 处、static 余 4、query_builder 余 1），
   以及该方案文档的结论/口径已过时（其 §8.7 "十条判据全部满足" **今天第③条不成立**）。
6. ⇒ **优化方案 = 先修口径（批次 A），再收真实增量（批次 B），再收紧棘轮（批次 C），最后同步文档（批次 D）。**
   A 不做，B/C 都是在错的数字上做决定。

---

## 二、静态化方案的机制与它真正换来了什么

### 2.1 机制（读方案 + 实测确认）

| 机制 | 做法 | 本仓先例/位置 |
|---|---|---|
| 编译期校验 | `sqlx::query!` / `query_as!` / `query_scalar!`，SQL 实参必须是**调用点字面量**（R1） | 全仓 1353 处静态字面量 |
| 离线可构建 | `.sqlx/` 缓存（**1320** 条），CI 无需数据库即可编译 | `scripts/ci/sqlx_prepare.sh`（唯一写入口） |
| 分区棘轮 | 生产 / `#[cfg(test)]` 两分区，四道：`dynamic_production` / `dynamic_test` / `static` / `query_builder` | `scripts/ci/check_sqlx_dynamic_ratio.sh` |
| 逐文件棘轮 | literal（字面量）/ param（跨函数传参）按文件计数，白名单**只减不增**（R7） | `sqlx_literal_production_baseline`、（param 已清零） |
| 例外有登记 | D-13（`Vec<Option<T>>`）/ D-14（动态标识符）/ D-96 / D-105 | 方案 §7.3 |

### 2.2 换来的收益（**均已实测，不是自述**）

1. **挖出 15 条"真 schema 下必然失败"的硬缺陷**：列名写错、INSERT 漏 `NOT NULL` 列、绑定类型不符
   （例：`DateTime<Utc>` 绑进 BIGINT 列）、`WHERE $2 != '[]'` 在 prepare 阶段必败。这类缺陷**只在罕见路径炸**，
   静态化把它们变成了编译错误。
2. **顺带清掉整块死代码/第二份实现**：`transaction.rs` 整模块（285 行，同时是 advisory lock 的第二份实现）、
   `signature/` 整模块（`event_signatures` 的第二份实现且零消费者）、`room_state_events` 表与其唯一写入者。
3. **把"规则"变成长效资产**：R1–R13 + 四道自证过能变红的门禁。这是本战役唯一不会随时间衰减的东西
   （方案 §0.5 自己也是这么总结的）。
4. **离线构建能力**：CI 不需要数据库；`--compile` 档让"缓存缺条目"直接变成编译失败 ⇒ **SQL 与 schema 漂移在编译期暴露**。
5. **可观测的余量**：`dynamic_production` 成为可追踪指标，从 1532 降到 96 是**可度量**的工程收敛，而不是"重构完了感觉好多了"。

---

## 三、当前状态：实测（2026-10-15）

### 3.1 门禁现状：**1 红 3 绿**

```
$ python3 scripts/ci/sqlx_query_census.py
dynamic_production=102   dynamic_test=763   static_production=1353   static_test=0
dynamic=865   static=1353   query_builder=17   total=2218   ratio=0.39

$ bash scripts/ci/check_sqlx_dynamic_ratio.sh
棘轮基线 production<=103 test<=758 static>=1349 query_builder<=18
OK:   生产动态 SQL 未增加（102 <= 103）
FAIL: 测试基础设施动态 SQL 增加 5 处（758 → 763）      ← ← 唯一红
OK:   静态 SQL 未减少（1353 >= 1349）
OK:   QueryBuilder 组装未增加（17 <= 18）
```

### 3.2 生产区动态 SQL 的真实构成（**修正门禁口径之后**）

| 组成 | 处数 | 是否缺陷 |
|---|---|---|
| **真·生产（可机械转换）** | **0** | ✅ 已归零（2026-09-29 C63 达成，今天仍为 0） |
| D-14 结构性（`format!` 拼列清单/排序方向） | 26 | 否（R7 白名单，各有守卫） |
| 分页 `event/pagination.rs`（游标/排序方向） | 8 | 否（结构性） |
| D-13 `Vec<Option<T>>`（`presence/mod.rs` 1 + `room_summary/repository.rs` 2） | 3 | 否（sqlx 无映射） |
| D-96 `pg_stat_statements` / D-105 `_sqlx_migrations` | 2 | 否（关系不存在，宏无法 describe） |
| 测试基建（`test_isolation.rs` 25 + `synapse-test-utils` 28 + `test_schema_guard.rs` 4） | 57 | 否（有意，编译进生产构建） |
| **误判进生产区的测试模块** | **6** | ⚠️ **缺陷（工具）** |
| 合计（门禁读数） | **102** | 真实为 **96** |

literal 逐文件（42 处：36 真实 + 6 误判）：

```
synapse-test-utils/src/lib.rs                     14
synapse-common/src/test_isolation.rs               9
synapse-e2ee/src/megolm/storage/db_tests.rs        6   ← ← 误判（应为测试区）
synapse-storage/src/event/pagination.rs            6
synapse-common/src/test_schema_guard.rs            2
synapse-storage/src/room_summary/repository.rs     2
synapse-storage/src/monitoring.rs                  1   （D-96）
synapse-storage/src/migration_checks.rs            1   （D-105）
synapse-storage/src/presence/mod.rs                1   （D-13）
```

缓存两档均绿：

```
$ bash scripts/ci/check_sqlx_cache_fresh.sh --static
OK: .sqlx/ 含 1320 条查询元数据 / 已被 git 跟踪 / 源码 1353 处静态查询字面量全部命中
$ bash scripts/ci/check_sqlx_cache_fresh.sh --compile
OK: 离线构建通过，缓存覆盖全部已编译的 query! 调用点
```

### 3.3 与方案文档的口径差（文档已过时）

| 指标 | 方案文档（2026-09-30） | 基线提交 `090742266` 实测 | 今天实测 | 真实（修正后） |
|---|---|---|---|---|
| `dynamic_production` | 103 | **96** | 102 | **96** |
| `dynamic_test` | —（测试基建 57） | **758** | 763 | **769** |
| `static` | 1361 | 1349 | 1353 | 1353 |
| `query_builder` | 18 | 17 | 17 | 17 |
| `.sqlx` | 1327 / 1329 | — | **1320** | 1320 |

> `090742266` 的实测与它自己钉的基线（96/758/1349/17）**逐项吻合** ⇒ 该提交处 **门禁是绿的**。
> 所以现在的红是**之后**引入的，不是历史遗留。

---

## 四、问题与不足（按等级；每条都给判据）

### P1【红·最高】测试区棘轮少报违规（真实 +11，门禁报 +5）

- **现象**：四道棘轮唯一红道，报告口径错。
- **判据（实测）**：`--list-production-dynamic` 与基线提交逐文件 diff，生产区**只**多出
  `synapse-e2ee/src/megolm/storage/db_tests.rs :literal 6`；而该文件是 `#[cfg(test)]` 模块。
  ⇒ 这 6 处被算进生产区、并**从测试区扣掉**。
- **影响（为什么比假红严重）**：门禁的职责是"拦住新增"。这 6 处的错位使 **测试区新增被少计 6 处**
  （758→769 的真实增量被压成 758→763）。也就是**棘轮的眼睛有 6 处盲区**。
- **对照**：生产区名义只剩 `103-102=1` 处额度，真实是 `103-96=7` 处 ⇒ 同一缺陷让**生产区额度虚耗 6 处**。

### P2【缺陷·工具】分区判定不认"声明在非模块根的 `#[cfg(test)] mod x;`"

- **判据**：全仓这类声明点 **6 处**（今天只有 1 个目标文件含动态 SQL，影响面 **6 处**）：
  `megolm/storage.rs→db_tests`、`megolm/storage.rs→tests`、`key_rotation/service.rs→{db_tests,tests}`、
  `device_keys/service.rs→tests`、`tests/unit/sqlx_dynamic_literal_guard_tests.rs→db_tests`。
- **引入提交（决定性证据）**：`db2ba7916`（"refactor(size): 外置 4 个超限文件的测试模块…"）。
  该提交把 `synapse-e2ee/src/megolm/storage.rs` 的**内联** `#[cfg(test)]` 块（当时文件 701 行，
  3 处动态 SQL 位于 679/684/695，而 `#[cfg(test)]` 标记在 329/538 ⇒ 位于测试块内，当时被**正确**算作 test）
  外置成 `megolm/storage/db_tests.rs`（6 处）⇒ 立刻被算作 **production**。
  同批外置的 `synapse-services/src/media/mod.rs`（模块根）**没有**这个问题 —— 这正好构成对照。
- **二次伤害**：`sqlx_dynamic_literal_guard_tests` 把它当"生产区字面量动态 SQL"⇒ 该守卫长期红、
  且指向一个**测试文件**（我上一轮已实测并记在 `UPSTREAM_CHANGELOG_ALIGNMENT_PLAN` §11.15.5）。

### P3【中等】三道绿道有余量 ⇒ 棘轮允许静默回归

| 道 | 基线 | 实测 | 余量 |
|---|---|---|---|
| `production` | ≤103 | 102（真实 96） | 1（真实 7） |
| `static` | ≥1349 | 1353 | 4 |
| `query_builder` | ≤18 | 17 | 1 |

- **为什么是问题**：该仓自己的纪律是"各批**按实测值**直接收紧，不再留余量"（方案 §0.1）。
  有余量 = 允许静默新增而不判红。

### P4【中等】方案文档的结论与口径已过时

- §8.7「**结论：静态化轴的十条判据全部满足**」中第 ③ 条（四道棘轮全 OK）**今天不成立**。
- §0.1/§0.2/§8.4 的 `.sqlx` 1327/1329、`static` 1361/1362、`test ≤756` 均与实测不符。
- 这正是该方案自己登记的 **⑦ 文档级缺陷**（计数漂移/过时结论）的复发 ⇒ 说明**缺一道"文档数字随访"机制**。

### P5【低·但会误导】基线文件的口径注释举错了例子

- `sqlx_dynamic_ratio_baseline:81-82` 写：「计数**包含** crate 内 `#[cfg(test)]` 内联测试模块
  （例如 `synapse-storage/src/event/db_tests.rs`）」。
- 实测：`event/db_tests.rs` 由 `event/mod.rs`（**模块根**）声明 ⇒ **归 test 区是正确的**，这个例子举反了。
  真正需要写清的是 P2 那条（非模块根声明）。**误导性注释会把后来人引到错的排查方向**。

### P6【低·工具缺口】没有"列 test 区动态调用点"的模式

- `sqlx_query_census.py` 有 `--list-production-dynamic`，**没有** test 区对应物。
- 后果：本次定位 P1 只能靠"`git archive` 取各提交 + 自带脚本比对"（我已这么做，但这是**每次都要重造的轮子**）。

### P7【前瞻·非当前缺陷】PgBouncer 与 prepared statement（登记，不排期）

- 现状：全仓**未部署** PgBouncer（只有 `docs/PERFORMANCE_BASELINE.md:215` 写"考虑使用"）；
  sqlx 侧**没有**任何 `statement_cache_capacity` / `.prepare(false)` 配置。
- 风险：一旦按那份文档引入 **transaction 池模式**，sqlx 的命名 prepared statement 会跨连接复用 ⇒
  典型报错 `prepared statement "sqlx_s_1" already exists`。
- 处置：**引入 PgBouncer 那天必须同时**改连接选项（`PgConnectOptions::statement_cache_capacity(0)` 或
  `prepare(false)`），并把这条写进该性能文档的 PgBouncer 段落。现在不动代码。

### P8【结构性·不是缺陷，但要写清】

- `static_test = 0`：`#[cfg(test)]` 内的**宏**进不了 `cargo sqlx prepare`（`--all-targets` 会 E0432）
  ⇒ 测试区**结构上**只能动态 SQL。测试基建 57 处永远收不掉，直到上游支持。
- D-96/D-105：关系"只在部分环境存在"⇒ 宏无法 describe。用 `to_regclass` 探测**也换不来静态 SQL**（收益 0），维持现状。

---

## 五、可执行优化方案

> 顺序**不可调换**：A 不落地，B/C 都在错的数字上做决定。
> 每批独立提交；门禁见 §5.5；回退一律 `git checkout -- <该批文件>`（无 schema 变更、无需回滚数据）。

### 批次 A：修正分区判定口径（**必做，最高优先**）

| 项 | 内容 |
|---|---|
| 改什么 | `scripts/ci/sqlx_query_census.py`：把"`#[cfg(test)]` + `mod X;`"的判定从"声明文件是否是模块根"改成**解析目标文件**（`foo/mod.rs` 与 `foo.rs` 两种子模块布局都支持），目标文件属于 test 区 |
| 配套 | 同一提交内同步 `sqlx_dynamic_ratio_baseline`（`production` 103→**96**、`test` 758→**769**）、`sqlx_literal_production_baseline`（删掉 `megolm/storage/db_tests.rs` 那 6 行）。**并在基线文件里写明这是一次"口径变更"**（先例：2026-09-23 的"口径变更：生产 / `#[cfg(test)]` 分区"） |
| 验收（硬指标） | `dynamic_production` **必须 = 96**（与 `090742266` 实测一致）；`dynamic_test` **必须 = 769**；`sqlx_dynamic_literal_guard_tests` 的报错文件从 2 个降回 1 个（只剩 `megolm/storage/db_tests.rs` 不再出现，实际应为 0 个文件） |
| 门禁 | 四道棘轮 + literal/param 棘轮 + `check_sqlx_cache_fresh.sh --static/--compile` |
| **变异自证（R11，必做）** | 往 `synapse-e2ee/src/megolm/storage/tests.rs`（非模块根声明、当前 0 动态 SQL）加一处 `sqlx::query_scalar("SELECT 1")` ⇒ **必须计入 test 区**；还原后绿。不写这条自证就不知道修好没有 |
| 回退 | 只改 4 个脚本/基线文件，`git checkout --` 即回 |

### 批次 B：把测试区的真实增量收掉或登记（**已执行**：登记为 D-112）

- **实测**（用修好的 census 与基线提交 `090742266` 逐文件 diff）：`dynamic_test` 758 → **769 = +11**，
  逐文件归因如下。**注意：不是说好的"6 归位 + 5 新增"** —— 6 处漂移里 **3 处是既有**（基线时以
  `megolm/storage.rs` 内联块形式已算在 758 里）、**3 处是该次外置时新增的用例**；所以真实新增是 11。

| 文件 | Δ | 引入提交 | 这些动态 SQL 在做什么 | 判定 |
|---|---|---|---|---|
| `synapse-storage/src/sticky_event.rs` | +4 | `1697e030e` / `a4be3e7ac` | `force_expiry` 把 `expires_at` 写成**过去时间**（用例注释明说"要验的就是过期本身，不能被翻转 flag 满足"）；`SELECT expires_at` 直读**跟踪值**与上报值比对；`SELECT COUNT(*)` 断言行数 | **登记**：前两类是"API 不暴露的那一维"，结构上不可回收 |
| `synapse-storage/src/event/db_tests.rs` | +3 | `043b0bbe1` / `4c7f3e17b` | 取 `MAX(stream_ordering)` 做前置条件；`UPDATE room_memberships SET membership='leave'` 直接构造"曾是成员"的历史状态 | **登记**：`stream_ordering` 不在公开 API 上；历史成员态不能靠 leave 流程复现 |
| `synapse-e2ee/src/megolm/storage/db_tests.rs` | +3 | `db2ba7916` | 直读 `session_key` 断言列内是 **at-rest 密文（`v1:` 前缀）** 与 legacy 明文路径；`SELECT COUNT(*) FROM megolm_session_keys` | **登记**：at-rest 断言必须绕开会解封的 API |
| `synapse-storage/src/application_service/db_tests.rs` | +1 | `09c028a08` | `SELECT url IS NULL` —— 断言 **SQL NULL 与 JSON `null` 的区别**（`url: null` 支持的语义核心） | **登记**：公开读 API 把两者都映射成 `None`，只有直读列能验证 |

- **判据（为什么是"登记"而不是"回收"）**：`#[cfg(test)]` 里**用不了 sqlx 宏**
  （`cargo sqlx prepare` 收集不到，`--all-targets` 会 E0432 ⇒ P8）⇒ **回收这些动态 SQL
  换不来任何编译期检查**，收益只是"数字减几"，代价是改写那些**刻意直读原始列**的不变式夹具。
  按 §8.6 既有的取舍逻辑（"不要为压数字硬改"），**正确的处置是登记 + 按实测收紧**。
- **落地**：方案 §7.3 新增 **D-112**（`#[cfg(test)]` 区结构上不可能静态化 + 11 处逐条理由），
  基线 `test` 收到实测 **769**（零余量）。**可回收性复核**：`COUNT(*)` 两类理论上
  可用 list/`get_session_key` 之类 API 等价替换，但同样零编译期收益，列为可选后续（不排期）。
- **验收**：`dynamic_test` = 769 且棘轮绿 ✔（见 §八）。

### 批次 C：三道绿道按实测收紧（消除 P3）

| 道 | 从 | 到（实测值） |
|---|---|---|
| `BASELINE_DYNAMIC_PRODUCTION` | 103 | **96** |
| `BASELINE_STATIC` | 1349 | **1353** |
| `BASELINE_QUERY_BUILDER` | 18 | **17** |

- **变异自证**：生产区任一文件加 1 处动态 SQL ⇒ 立刻判红（`production 96 → 97`）；`QueryBuilder::new` 加一处 ⇒ 判红。
- 注意：`BASELINE_STATIC` 是"只禁减"，收紧到实测值会把"静态查询减少"也判红 ⇒ 若某批删代码是**好事**，
  该道会拦住 —— 这是有意设计（防"把静态换成动态"），收紧前确认这一点并写进基线注释。

### 批次 D：文档口径同步 + 加一道防复发机制（消除 P4/P5）

1. 更新 `SQLX_STATICIZATION_PLAN_2026-09-23.md` 的 §0.1/§0.2/§0.4/§8.4/§8.7 到**实测值**，
   并把"十条判据全部满足"改成**带日期的实测表**（今天第③条不成立这一事实要写进去，不能只改数字）。
2. 订正 §0.3/基线文件里那条举反了的例子（P5）。
3. **加机制（这是本批真正的价值）**：把这三组数字做成**可跑的核对**
   —— 新增 `scripts/ci/check_plan_numbers_fresh.sh`：从 census + 两个基线文件**现算**，
   与方案文档 §0.1/§0.2 表里的数字比对，不一致就判红（形状照抄已有的 `check_plan_doc_sections.py`：
   基线棘轮 + 对照 HEAD）。**没有这道，下次又会有"文档说全绿、实际红着"的结论。**

### 5.5 每批必跑门禁（沿用方案 §8.5，补两条）

```bash
python3 scripts/ci/sqlx_query_census.py                                    # 总览
bash    scripts/ci/check_sqlx_dynamic_ratio.sh                             # 四道棘轮
bash    scripts/ci/check_sqlx_cache_fresh.sh --static                      # 缓存静态核对
bash    scripts/ci/check_sqlx_cache_fresh.sh --compile                     # 离线构建（权威档）
B=<已迁移的库>; DATABASE_URL=…$B bash scripts/ci/sqlx_prepare.sh           # 仅在改过 SQL 文本时
cargo clippy --workspace --all-targets --all-features -- -D warnings       # 两档 clippy 之一
cargo nextest run --test unit --features test-utils
bash scripts/ci/check_sqlx_cache_fresh.sh --static  # 该模块真基线往返
```

⚠️ 两条禁令仍在（方案 §0.3，D-77）：**禁** `--full`（共享 `synapse_test.public` 会被并发会话清空 ⇒
1443 个误导性错误）、**禁**裸 `cargo sqlx prepare`（destination 就是 `.sqlx/` 且先清空再重写）。
要写缓存只有 `scripts/ci/sqlx_prepare.sh` 一个入口。

---

## 六、不做清单（沿用方案 §8.6，复核后仍成立）

1. **不把 26 处 D-14 的列清单/排序方向内联成字面量** —— 那是"用 14 份副本换编译期检查"，违反铁律 2，
   且与 `pagination` bench 复用同一常量的事实冲突。守卫（常量↔schema + 各自真基线往返）已经在位。
2. **不引入 `query_file!`** —— 只是把 14 份副本挪进文件，还多一套全仓无先例的机制。
3. **不给 `VACUUM`/`REINDEX` 硬编码表名** —— PG 协议层不允许标识符走占位符，硬编码=写死。
4. **不为压数字而动测试基建的 57 处** —— `#[cfg(test)]` 结构上不能用宏（P8），除非上游改。

---

## 七、残留风险

| 风险 | 等级 | 说明 |
|---|---|---|
| 批次 A 改动 census 会影响**三**个基线文件 | 中 | 必须同批提交；漏一个就会"数字对不上基线"，看起来像新违规。已列为验收硬指标 |
| 测试区真实增量尚未逐处定性 | ~~中~~ 已消除 | 批次 B 已完成：**+11 逐文件归因 + 逐处判定**，登记为 **D-112**；基线收到实测 769 |
| `.sqlx` 缩容会被脚本**默认拦住** | 低 | `sqlx_prepare.sh` 在条目减少时**回滚并要求 `ALLOW_CACHE_SHRINK=1`**（防误删，是有意设计）。删查询后重生成会"失败"，此时应人工核对条目内容再放行 |
| PgBouncer 引入时的 prepared statement 冲突 | 低（未来） | 见 P7；引入当天必须同时改连接选项 |
| 文档数字随访机制本身要维护 | 低 | 批次 D 的核对脚本同理需要一个基线文件；漏更新它会变成新的假红来源 |

---

## 八、执行记录（2026-10-15，四批全部落地）

| 批次 | 落地内容 | 实测验收 | 变异自证（R11） |
|---|---|---|---|
| **A** 修口径 | `sqlx_query_census.py::collect_test_gated_files` 补上 `foo.rs` + `foo/x.rs` 布局；新增 `--list-test-dynamic` 模式（补 P6 的工具缺口）；`sqlx_dynamic_ratio_baseline` 口径变更（`production` 103→**96**、`test` 758→**769**、`dynamic` 861→**865**）并写明理由 | `dynamic_production=96` / `dynamic_test=769` —— 与基线提交 `090742266` 的生产值**逐项一致**，6 处已出现在 test 区清单 | 往**非模块根**声明的测试模块（`megolm/storage/tests.rs`）加 1 处动态 SQL ⇒ 计入 **test**（96 不动、769→770）且棘轮在 **test** 道判红；旧口径会把它算进 production |
| **B** 定增量 | **+11 逐文件归因**（4 个文件，见 §5 批次 B 表）⇒ 登记为 **D-112**（方案 §7.3），基线收实测 769 | `dynamic_test=769 ≤ 769` | —（登记类，无需变异） |
| **C** 收紧 | `static` 1349→**1353**、`query_builder` 18→**17**（`production` 已在 A 收紧），取消全部静默余量 | 四道全绿且**零余量**：`96/769/1353/17` | 生产区加 1 处动态 SQL ⇒ `production 96→97` **FAIL** |
| **D** 文档 + 随访门禁 | 方案文档：§0.1 新增「0.1.1 2026-10-15 复核（口径变更 + 当前实测）」、§0.2 构成改 96 = 26+57+13、§7.3 加 **D-112**、§8.4 当前状态重写、§8.7 的"十条判据"补**带日期的更新**与机器可核对的数字块；订正 P5（举反了的注释）与 census docstring 里不存在的 `--list-uncertain`；**新增 `scripts/ci/check_plan_numbers_fresh.sh`** 并接入 `preflight.sh`（每次都跑） | `check_plan_numbers_fresh.sh` → `OK … 5 个键与真源一致` | ① 文档 `static` 数字改错 ⇒ FAIL（指名键）；② 删掉 `<!-- sqlx-plan-numbers:start -->` 锚点块 ⇒ FAIL（防门禁静默失效） |

**连带影响（已在批次 A 内处理，非新问题）**：
- `test_ddl_allowlist`：分区修正后，**我上一轮**写的纯文本断言（`ddl_block` 里含 `CREATE TABLE … (` 定位 needle）
  首次落进 test 区 ⇒ D-36 守卫 A 判红。按该清单**既有分组**「纯 SQL 词法解析测试：DDL 文本是字符串夹具、从不执行」
  登记 2 条（`…::ddl_block`、`…::key_rotation_log_stays_user_device_scoped_not_server_scoped`），
  并写明它**不属于**"该迁移到 `isolated_test_pool()`"那一组（该用例不建表、不连库）。守卫复跑 **9 passed**。
- `sqlx_literal_production_baseline` / `sqlx_param_production_baseline` / `insert_column_allowlist`：**无需改动**
  （前者口径是"未登记即基线 0"，分区修正后该文件自动退出生产清单；后两者本就不含该文件）—— 这一点是先核对再动手的，
  避免"改多了"。

**本次改动文件**：`scripts/ci/sqlx_query_census.py`、`scripts/ci/sqlx_dynamic_ratio_baseline`、
`scripts/ci/test_ddl_allowlist`、`scripts/ci/check_plan_numbers_fresh.sh`（新增）、
`scripts/quality/preflight.sh`、`docs/audit/SQLX_STATICIZATION_PLAN_2026-09-23.md`、本文件。
