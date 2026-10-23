# synapse-rust 屎山诊断（2026-10-23，HEAD `ac10c540f`）

## 一句话结论

**最贵的一处不是任何一个大文件，而是「告警面有三套编码，其中两套是坏的」**：
连接池利用率过高在 Prometheus 两份规则、admin 遥测 API 的规则表里各写了一遍，指标名、表达式、
触发通道都不同，而没有任何东西校验它们一致。更进一步取证后（见「口径裁决」）发现：

- `monitoring/` 那整套监控栈的 **Alertmanager 配置被官方 `amtool` 判 FAILED**，且不在生产部署路径上
  ⇒ 那 16 条告警**一次都没发出过**；抽查它独有的 5 条告警，**4 条的指标是死的**
- deploy 栈那 34 条**能跑**，但出口是 `alert-handler` 的一行 `logger.info`，**没有任何真实通知渠道**

所以先动的不是"加一致性门禁"，是**删掉那套从未跑起来的栈、再给剩下这套补出口**。
删重复表示永远比给它加校验便宜。

## 方法与范围

- 基线：HEAD `ac10c540f`；工作区 1 个未提交文件（`docs/synapse-rust/ROUTE_CONTRACT.md`，仅头部日期行），
  结论如涉及工作区会单独标注
- 另一棵工作树：`.worktrees/gate-test`（分支 `test/exit-hook-single-source`）—— 所有检索已排除，
  否则计数翻倍
- 扫描：`scan_repo.py --root . --exclude .worktrees --exclude element-web-harness`，
  3077 个受版本控制文件，其中 1123 个 `.rs`（906 个计入生产）
- 规模参考：生产源文件行数 p50=294 / p90=1036 / p99=1973

**这次没覆盖什么**：
- 没有起服务、没有连 DB，所有结论都是静态的。运行期行为（锁泄漏多久发生一次、
  死告警在生产里挂了多少天）没有观测数据
- 只读了 6 个判据样本，没有逐模块通读；`synapse-storage` 的 5 个 2000+ 行文件只做了体检未做解剖
- 没有跑完整测试套件（全量 integration ~2h）；单条守卫与门禁是实跑的
- `telemetry_service.rs` 的规则表只看了池利用率一段，其余规则未逐条核对

---

## 判据 1：同一事实多个表示

| 事实 | 表示一 | 表示二 | 表示三 | 判定 |
|---|---|---|---|---|
| 连接池利用率过高 | `docker/deploy/prometheus/alerting-rules.yml:119` `pool_utilization >= 0.9` | `monitoring/alerting-rules.yml:128` `db_connections_active / (db_connections_active + db_connections_idle) > 0.9` | `synapse-services/src/telemetry_service.rs:396` `util >= 90.0` → 规则名 `db_pool_utilization` | **治理** |
| 服务不可用 | deploy `ServiceDown`：`up == 0` | monitoring `SynapseServiceDown`：`up{job="synapse-rust"} == 0` | — | 治理 |
| 告警规则整体 | deploy 版 **34 条** | monitoring 版 **16 条** | 两边**只有 1 条同名** | 需决策 |

取证命令：

```bash
for f in docker/deploy/prometheus/alerting-rules.yml monitoring/alerting-rules.yml; do
  printf "%-50s %s 条\n" "$f" "$(grep -cE '^[[:space:]]*- alert:' $f)"
done
# deploy 34 条 / monitoring 16 条；comm -12 求交 = 1 条

grep -rn "pool_utilization" --include='*.rs' . | grep -v '\.worktrees'
# synapse-common/src/server_metrics.rs:350  注册 gauge "pool_utilization"
# synapse-services/src/telemetry_service.rs:399  另一套字符串规则名 "db_pool_utilization"
```

**后果（按名字理解会做错什么）**：运维把池告警阈值从 90% 调到 85%，只改 `docker/deploy/` 那份——
dev 栈和 admin 遥测接口仍是 90%，**三条通道里两条在报旧标准**。而
`scripts/ci/check_prometheus_config.py:152-153` 只把两份文件都塞进 `RULE_FILES` 做
**promtool 语法校验**，不比对语义，所以门禁是绿的。

**两份告警规则是不是「按环境拆分的合理设计」？** 部分是：两个栈钉的 Prometheus 版本不同
（dev `v2.47.0` / deploy `v2.48.1`，见 `check_prometheus_config.py` 的注释），挂载点也不同
（`docker/docker-compose.monitoring.yml:31` vs `docker/deploy/docker-compose.monitoring.yml:44`）。
但**34 vs 16 且只 1 条同名**已经超出「环境差异」能解释的范围——它是两套独立演化出的规则集。

**建议动作**：不合并两份（合并会把环境差异藏进条件分支，见 `remediation-rules.md` 负面清单第 2 条），
而是**把「同一指标」的阈值抽成单一来源**：在 `docker/deploy/prometheus/` 下建一份
`thresholds.yml`（被两处 `- alert:` 用 `$THRESHOLD` 引用不可行——PromQL 无变量），
因此实际可行的方案是**加一条一致性门禁**：对两份规则里「同一指标、同一语义」的告警对
（当前 3 对：池利用率、池耗尽、服务不可用）断言阈值相同。验证方式：改任意一侧阈值 → 门禁红。

---

## 判据 2：失败被静默吸收

**口径修正（重要）**：`scan_repo.py` 原报的「373 处 `let _ =` / 96 文件」**虚高 2.8 倍**。
它按文件路径判测试，而 Rust 把测试写在生产文件的 `mod tests { … }` 里。不截断就会把测试清理
代码（`let _ = storage.remove_threepid(...)`）算成生产静默失败——`synapse-storage/src/threepid.rs`
的 8 处 `let _ =` **全在** `mod tests` 内。

三列口径（前两列已用修好的扫描器复现；第三列额外排除 `benches/` 与纯测试 crate）：

| 模式 | 原始报 | 扫描器修正后 | 再排除纯测试 crate | 虚高倍数 |
|---|---|---|---|---|
| `let _ =` | 373 处 / 96 文件 | **143 处 / 60 文件** | 135 处 / 57 文件 | 2.6× |
| `.ok();` 丢弃 | 36 处 / 19 文件 | **30 处 / 16 文件** | 31 处 / 17 文件 | 1.2× |
| `unreachable!/panic!` | 53 处 / 24 文件 | **4 处 / 4 文件** | — | **13×** |

`unreachable!/panic!` 那 13 倍最能说明问题：原来 49 处是测试里的断言 panic，**根本不是缺陷**。
按 53 去排治理项会得到一个完全假的优先级。

复现：

```bash
python3 - <<'PY'
# 见本次诊断脚本：git ls-files '*.rs' → 排除 tests/benches/test-utils
# → 正则 mod\s+\w*tests?\w*\s*\{ 起做花括号配对删除 → 数 `let _ =`
PY
```

**真正是缺陷的只有 2 类**（其余 130 处是 `Option` 缺省、best-effort 清理、`Drop` 里的最后一次尝试）：

### 缺陷 2-1：后台更新的锁释放与历史写入被吞，且无日志

`synapse-services/src/background_update_service.rs:214-216`（`complete_update`）、
`:238-240`（`fail_update`）：

```rust
let update = self.storage.update_status(job_name, "completed").await
    .map_err(|e| ApiError::internal_with_cause("Failed to complete update", e))?
    .ok_or_else(|| ApiError::not_found("Update not found"))?;   // ← 严格传播

self.storage.release_lock(job_name).await.ok();                  // ← 吞
self.storage.add_history(job_name, "completed", ...).await.ok(); // ← 吞
```

**为什么是缺陷**：主状态更新严格传播错误，但**锁释放失败被吞**。锁残留的后果是
「这个 job 之后再也取不到锁」——后台更新永久停摆，而日志里一个字都没有（`.ok()` 不记日志）。
`add_history` 被吞则让运维看不到该 job 的完成记录。**作者显然认为"主状态已成功，后面失败无所谓"，
但锁泄漏的代价比主状态更新失败更大**。

**建议动作**：`release_lock` 失败降级为 `warn!`（不能 `?`——主状态已提交，回滚会制造不一致），
`add_history` 同样。验证：构造一个 `release_lock` 返回 `Err` 的 mock，断言日志出现 warn
且函数仍返回 `Ok`（否则改成了新的行为回归）。

### 缺陷 2-2：一条告警引用从未被 emit 的指标

```bash
grep -rn "pickle_persist" . | grep -v '\.worktrees' | grep -v '^./target'
# docker/deploy/prometheus/alerting-rules.yml:331   expr: rate(megolm_vodozemac_pickle_persist_errors_total[5m]) > 0
# docker/deploy/prometheus/recording-rules.yml:105  同上
# docs/synapse-rust/archive/E2EE_VODOZEMAC_MIGRATION.md:445  （计划文档里列的「待加指标」）
# 没有任何 .rs 发射点
```

`scripts/ci/check_metric_instrumentation.py:132` 自己登记了 `record_megolm_vodozemac_pickle_persist`
为「未接」——但那个方法**在 `.rs` 里根本不存在**（`grep -rn "record_megolm_vodozemac_pickle_persist" --include='*.rs'`
零命中），说明 vodozemac 迁移没落地，而告警先加了。

**后果**：Prometheus 对不存在的指标，`rate(...)` 返回空向量 ⇒ **告警永不触发，也不报错**。
这是"报警器自己静默失效"，比普通静默失败严重一档——没有任何下游能发现它坏了。

**建议动作**：把这条告警和录制规则**删掉或注释并注明**（等迁移落地再加）。
验证：`promtool test rules` 用 `absent()` 断言该指标不存在时告警不触发——但删掉后应改成
"等指标出现才有意义"，更稳的做法是先在 `alerting-rules.test.yml` 里加一条断言把现状钉住。

---

## 判据 3：语义靠约定

| 现象 | 按名字理解会做错什么 | 能否变成结构 |
|---|---|---|
| `metric_instrumentation_baseline` 叫「基线」 | 以为「基线 = 可接受的正常状态」⇒ 新接一个埋点后不清理；实际语义是**已知缺陷清单，目标清零**（脚本注释原文：「基线里的每一条都是已知未接通的埋点，不是豁免」） | 能：改名为 `known_unwired_instrumentation`，或文件头写一句语义 |
| 17 份 `*_baseline` 语义不统一 | `file_size_baseline` = 已超标文件的**上限**（可以就这么大）；`coverage_baseline.json` = 覆盖率**下限**；`metric_instrumentation_baseline` = **待清零的缺陷**。三种相反方向共用「baseline」 | 部分：可加一个 `scripts/ci/README.md` 统一表说明每个文件的棘轮方向 |
| `ServiceDown` vs `SynapseServiceDown` | 以为前者是「synapse 服务挂了」（因为有后者）；实际前者是 `up == 0`，**任何 target** 掉线都触发，包含 postgres_exporter / node_exporter | 能：改名为 `TargetDown` |

**为什么这条值钱**：`check_metric_instrumentation.py` 特意写了「不是豁免」这句注释去澄清它自己的文件名——
**一次真实的误解已经发生过**，注释是补丁，不是修复。

---

## 判据 4：局部不可推理

挑一个真实需求（本次诊断真的做了）：**「给一个已有指标加一条告警」**。

> 1. `synapse-common/src/server_metrics.rs` —— 指标是否注册、名字是否精确（gauge 名就是
>    Prometheus 指标名，没有前缀映射层，`server_metrics.rs:350`）
> 2. `scripts/ci/check_metric_instrumentation.py` 的 `metric_instrumentation_baseline` ——
>    该埋点是否「已接通」。未接通的指标会注册但永不产生数据（脚本注释原文：
>    「指标会注册但永不产生数据 ⇒ 依赖它的告警永不触发」）
> 3. 选哪一份告警规则 —— `docker/deploy/prometheus/` 还是 `monitoring/`？两者无文档说明取舍
> 4. `scripts/ci/dashboard_metrics_allowlist` + `dashboard_metrics_baseline` —— 若同时加面板，
>    还要过 `check_dashboard_metrics.py` 的静态正则
> 5. `docker/deploy/prometheus/promtool/alerting-rules.test.yml` —— 加一条 `series` fixture，
>    否则 `promtool test rules` 覆盖不到新告警
> 6. `scripts/ci/check_prometheus_config.py` —— 校验规则文件语法 + 钉版一致

共 **6 跳**，其中 **3 跳必须读脚本源码**才能确定（第 2 跳的「接通」判定规则、第 4 跳的 allowlist
匹配方式、第 5 跳的 fixture 格式）。而第 3 跳**没有任何文档可依**——这正是判据 1 那两份规则
存在的代价。

**注意这不是"路由链"那种假成本**：路由派生表看似要改 3 张 `.inc.rs` + 7 个 fixture，实际
`gen_derived_routes.py` / `gen_route_handlers.py` / `gen_contract_doc.py` 全自动（见「看着像屎山其实不是」）。
告警这条链路没有对应的生成器，每一步都是手工。

---

## 判据 5：修复自己堆积

| 修复产物 | 规模 | 现在的人工成本 |
|---|---|---|
| `scripts/ci/` 门禁脚本 | **38 个**（.py/.sh） | 每次改动要知道哪些会被触发；`preflight.sh` 一次跑 12+ 项，实测 4m35s（含 cargo） |
| 手工维护的基线/白名单文件 | **17 份**（`*_baseline` / `*_allowlist` / `*.txt`） | 每份都是一条「要人工更新/收紧」的债；`clippy_allow_*` 与 `dashboard_metrics_*` 各有 baseline + allowlist **两份** |
| `ROUTE_CONTRACT.md` 的「自动生成于 <今天>」 | 1 行 | 生成器用 `datetime.date.today()`（`gen_contract_doc.py:269`）。**跨午夜必然出现仅含日期的 1 行 diff**，而它会让契约 gate 在之后每次提交时 SKIP ⇒ **链路根本没被校验**；丢弃没用（下次运行又生成）⇒ 只能把那一行提交掉 |
| `metric_instrumentation_baseline` 的 WHY 字典 | 14 条 | **4 条是孤儿**：`record_megolm_vodozemac_pickle_persist` / `_dual_write_promotion` / `_lazy_migration_batch`（方法在代码里不存在）+ `update_pool_metrics`（描述已被代码推翻，见下） |

**`update_pool_metrics` 那条是活的例子**：字典说「连接池状态回填未接 ⇒ `pool_utilization` /
`db_connections_active` / `db_connections_idle` / `pool_health_status` 恒为 0，
`DatabasePoolUtilizationHigh` 与 `DatabasePoolExhausted` 无法触发」。
实际 `src/tasks/mod.rs:158` 与 `src/server/mod.rs:451` **都在生产路径回填它**。
⇒ 注释过期，指标有数据，告警能触发。**若照抄注释写进报告，就会得出一条完全错误的「死告警」结论。**

**门禁自己的可达性**：`check_metric_instrumentation.py` 当前 EXIT=0（「未接通 12 个，基线 12」），
但它只比对「代码里定义了的方法」，字典里那 4 条孤儿**永远不会被比对**——手工清单的典型失效：
清单能防止新增，不能发现陈旧。

---

## 看着像屎山其实不是

| 现象 | 为什么不是 |
|---|---|
| 3 张 `derived_route_table_*.inc.rs`（5476 / 75 / 59 行）+ 7 个 ledger fixture + `ROUTE_CONTRACT.md` 都是「路由」 | **全是生成的**。`gen_derived_routes.py --check`、`gen_route_handlers.py --check`、`gen_contract_doc.py` 负责产出，人只改注解。改一条路由的真实成本是「改注解 + 跑 3 个脚本」，不是手改 11 个文件 |
| `migrations/00000000_unified_schema_v12.sql` 5830 行 | 单基线迁移文件，DDL 天然线性。改它要同步 `EXPECTED_BASELINE_FINGERPRINT` 是**契约**，不是重复 |
| `tests/unit/fixtures/ledger_export{,_sdk}/*` 三个 json 96.7% 相似 | 两个 lane（default / sdk）× 三个 profile 的**期望输出快照**。相似是应该的；合并反而会让「哪条 lane 差异」看不出来 |
| `tests/unit/ci_test_scope_tests.rs` 1406 行、`docker/deploy/api-integration_test.sh` 9075 行 | 测试/验证代码，不承担生产职责；且 600 行棘轮的 `ROOTS` 不含 `tests/` |
| `synapse-storage/src/room/mod.rs` 2531 行等 30 个 >800 行文件 | 行数不是判据。没读完全部就下「该拆」的结论，正是本 skill 反例表第一条。它们的真实成本要用判据 4 的链路去量，本次只做了体检 |
| 373 处 `let _ =` | 修正口径后 135 处，其中真正吞掉关键失败的只有 2 类（见判据 2）。按数量清理会把 130 处合法写法改坏 |

---

## 排序（治理项）

按「降成本幅度 / 爆炸半径」排序，不按 P0–P3。

| # | 动作 | 降的是哪条判据 | 爆炸半径 | 可失败的验证 |
|---|---|---|---|---|
| 1 | **删 `monitoring/` 整套监控栈**（16 条规则 + alertmanager + grafana + prometheus 配置），只把 `SpikeInAuthFailures` 搬进 `docker/deploy/prometheus/alerting-rules.yml` —— **已完成 `a348fbd9a`**（迁移在 `53945c9cd`） | 判据 1 | 中：删 9 个配置 + 1 处 compose 引用；**不动 deploy 栈** | 删后 `check_prometheus_config.py` → **PASS**（改前 C 项长期 FAIL）；`promtool test rules` SUCCESS |
| 2 | 给 `alertmanager.yml` 加 `amtool check-config` 门禁（现在**零校验**），**并入** `check_prometheus_config.py` | 判据 2 | 小：1 个函数 + 1 条 `RULE_FILES` 类比项 | 把 `target_match_anc` 塞回去 → 红 |
| 3 | 补 `alert-handler` 的真实出口（钉钉/企业微信，或接 log 采集） | 判据 2 | 小：1 个 `receivers` 段 + 可能需要凭据 | 灌一条假告警 → 群里收到 |
| 4 | 三条死告警取舍：`megolm pickle`（无指标）、`SlowSyncLatency`（注册但无 `.observe()`）、`HighFederationErrorRate`（`federation_operations_total` 零命中）—— 补埋点或删规则 | 判据 2 | 小：3 段规则 + 可能的埋点 | `promtool test rules` 用 `absent()` 钉住现状 |
| 5 | `background_update_service.rs` 的 `release_lock` / `add_history` 从 `.ok()` 改为 `warn!` —— **已完成 `2a693f751`** | 判据 2 | 小：3 个函数、5 个语句，不动控制流 | `.ok();` 由 5 → 0；`cargo check -p synapse-services --all-features` 通过 |
| 6 | 清理 `BASELINE_REASONS` 的 4 条孤儿 + 加「与基线一一对应」断言 —— **已完成 `ba03eda2a`** | 判据 1 + 5 | 极小：1 个 dict + 2 段断言 | 注入 `record_ghost_probe` → 点名报错；还原 → EXIT=0 |
| 7 | `gen_contract_doc.py` 的日期改用 HEAD 提交日期 —— **已完成 `cbd64b969`** | 判据 5 | 小：1 个函数 | 连跑两次 `md5` 相同；生成结果与 HEAD 逐字节一致（doc 无 diff） |
| 8 | `*_baseline` 语义分化：把「待清零的缺陷清单」改名，或在 `scripts/ci/README.md` 建一张表说明每个文件的棘轮方向 | 判据 3 | 中：17 份文件的命名，涉及多处引用 | 改名的那份要同步脚本常量；漏改 → 脚本因文件不存在而 exit 2 |
| 9 | ~~修 `scan_repo.py` 的 prod/test 分区~~ **本次已修** | 判据 2（取证工具） | 小：1 个函数（`prod_body`） | 已验：`let _ =` 373→143、`unreachable/panic` 53→4；`threepid.rs` 由 8 处归零 |

**#8 决定不做**：17 份清单里只有 3 种棘轮方向（上限 / 下限 / 待清零），而改名的收益是
"下一个人少困惑一次"，成本是动 17 个文件并同步 5 个脚本里的常量 —— 一次改错的代价
（脚本因文件不存在 exit 2，且它是 fail-closed）高于收益。按 skill 的立场，补一份 README
说明每个文件的方向属**化妆项**（降低的是可读性门槛，不是改动成本），登记不排期。

**为什么 #1 排第一**（口径裁决后已变更）：原先排第一的是「跨两份规则的一致性门禁」，
但裁决 1 判定 `monitoring/` 整套**从未跑起来过**（`amtool` FAILED + 不在部署路径 + 5 条抽查 4 条死）
⇒ 只剩一份规则时，**一致性门禁失去对象**。所以真正该先做的是把它删掉——
删掉重复表示永远比给它加校验便宜。同理 #2 的「补 alertmanager 校验」也不再是"跨两份对比"，
而是"这份配置有没有被任何门禁看过一次"（答案：没有）。
其余几条都是单点修复，降本幅度小于它。

---

## 不建议做

- **拆 30 个 >800 行的生产文件**。行数不是判据；本仓 `room/mod.rs` 2531 行但边界可能清楚。
  要做也应该先按判据 4 量出「哪条链路真的贵」，而不是按行数排队。这一步的收益远低于成本。
- **清理 135 处 `let _ =`**。其中 130 处是 `Option` 缺省、best-effort 清理、`Drop` 的最后尝试。
  按数量清理会把合法写法改成噪音（见 `remediation-rules.md` 负面清单第 1 条）。- **合并两份告警规则**。dev / deploy 两个栈的 Prometheus 钉版不同，合并会把版本差异藏进条件分支。
  应该做的是「阈值单一来源 + 一致性门禁」（#1），不是合并文件。
- **给 `preflight.sh` 再加门禁**。已经 38 个脚本、12+ 项、4m35s；继续加会让「跑一遍」变成
  需要计划的动作，人就开始跳过了。#1 加的那一条应该是**合并进已有的 `check_prometheus_config.py`**，
  而不是第 39 个脚本。

## 口径裁决（2026-10-23 追加，已回代码与官方工具取证）

### 裁决 1：deploy 版是唯一权威；`monitoring/` 整套删除，只搬一条告警

不是"各有受众"，也不是"dev 版需要补课"——**`monitoring/` 那套从来没有跑起来过**。

| # | 证据 | 复现 |
|---|---|---|
| 1 | `monitoring/alertmanager/alertmanager.yml` 官方校验 **FAILED** | `docker run --rm -v $PWD/monitoring/alertmanager:/cfg --entrypoint amtool prom/alertmanager:v0.27.0 check-config /cfg/alertmanager.yml` → `${SMTP_SMARTHOST:-localhost:25}: too many colons in address`。Alertmanager **不做环境变量替换**，`${...:-默认}` 被当字面地址 |
| 2 | 这份坏配置**确实被挂载** | `docker/docker-compose.monitoring.yml:76` 挂 `./monitoring/alertmanager/alertmanager.yml` ⇒ 容器起不来（配置解析失败即退出），16 条告警**一次都没发出去过** |
| 3 | 生产部署**根本不含它** | `docker/deploy/deploy.sh:17` 默认启动监控栈（组件：`prometheus/alertmanager/grafana/node-exporter/alert-handler`），`--no-monitoring` 才跳过；`monitoring/` 不在其中 |
| 4 | CI 对 `alertmanager.yml` **零校验** | `grep -rn alertmanager scripts/ci/ .github/workflows/` 只命中 prometheus.yml 的注释与面板 allowlist；`check_prometheus_config.py` 的 `RULE_FILES` 不含任何 alertmanager 配置 |

对照：deploy 版 `amtool check-config` → **SUCCESS（2 inhibit rules / 6 receivers）**。

**但 `monitoring/` 里有一条值得救**。抽查 5 条「deploy 没有、monitoring 有」的告警，看它们引用的指标是不是活的：

| 告警 | 指标 | 实测 | 结论 |
|---|---|---|---|
| `SpikeInAuthFailures` | `auth_failures_total` | 注册于 `server_metrics.rs:300`，`inc()` 于 `:474` | **活**，值得搬进 deploy |
| `SlowSyncLatency` | `sync_duration_ms_bucket` | 只注册（`:381`），**全仓无 `.observe()`** | 死：histogram 无样本 ⇒ `histogram_quantile` 空 |
| `HighFederationErrorRate` | `federation_operations_total` | `grep -rn federation_operations --include='*.rs'` **零命中** | 死：指标不存在 |
| `E2EECoverageDrop` | `messages_sent_total{encryption="true"}` | `messages_sent_total` 用 `register_counter` 注册（`:382`），**无标签** | 死：标签选择器永远匹配空 |
| `MessageQueueBacklog` | `message_queue_depth` | 注册 `:417`，`set` 只在 `src/bin/synapse_worker.rs:388`（worker 二进制） | 单体部署下恒 0 |

⇒ 5 条里 4 条是死的。**这套不是"另一套受众的规则集"，是没走通的设计稿。**

### 裁决 2：三套通道不是三选一，是两层分工 + 一个缺失的出口

| 层 | 承担者 | 机制 | 能力 | 现状 |
|---|---|---|---|---|
| **值班通知面**（主） | Prometheus rules → Alertmanager → receiver | push，持续评估 | 分组 / 抑制 / repeat / 静默 | deploy 链路完整，但**出口是 `alert-handler` 的 `logger.info`**（`docker/deploy/alert-handler/app.py:23-40`）：收到即记日志 + 返 200，不转发任何真实渠道；`alertmanager.yml` 里 `dingtalk` / `wecom` 两个 receiver 是 **空 `webhook_configs`** |
| **自助诊断面**（辅） | `/_synapse/admin/v1/telemetry/*` | pull，请求时 `sync_with_health()` 现算 | 有 **ack**（`/alerts/{id}/ack`），JSON | 活的（`synapse-web/src/routes/telemetry.rs:352-358`，`AdminUser` 鉴权）。但它**不是告警系统**：无推送、无去重、无抑制、无历史（`alerts: Arc<RwLock<HashMap>>` 内存，重启丢） |

**所以**：以 Prometheus/Alertmanager 为**唯一告警来源**，admin 遥测 API 明确定位为**排障辅助**
（"人已经在看这个问题时"用，它的 ack 能力 Prometheus 侧没有）。两者**不要互相替代**，
也不要再让任何第三处代码自己写阈值判断。

**真正缺的不是"谁是主"，是出口**：现在 34 条告警的终点是 `docker logs synapse-alert-handler`
里的一行 `logger.info`。没有 log 采集的话，等于没有通知。

### 落地顺序（按依赖，不按级别）

| 步 | 动作 | 状态 | 可失败的验证 |
|---|---|---|---|
| 1 | 删 `monitoring/` 整套 + 搬 `SpikeInAuthFailures` / `MessageQueueBacklog` | **已完成**：`53945c9cd`（迁移）、`a348fbd9a`（拆栈） | `promtool check rules` 36 rules / `promtool test rules` SUCCESS / `check_prometheus_config.py` **PASS**（改前 C 项长期 FAIL） |
| 2 | 给 `alertmanager.yml` 加 `amtool check-config` 门禁 | **已完成 `f6792a734`**：新增 section F，镜像版本从 compose 派生（`prom/alertmanager:v0.27.0`） | 变异自证：插入 `target_match_anc` → `line 95: field target_match_anc not found in type config.plain`；还原 → PASS |
| 3 | 补 `alert-handler` 的真实出口（钉钉/企业微信，或接日志采集） | **已完成 `ee939f9a2` + `a444f7fa8`**：可选 `ALERT_FORWARD_URL` + `ALERT_FORWARD_FORMAT`（raw/dingtalk/wecom），未配置时**启动即 warn**，daemon 线程转发且失败不影响返回 200（避免重试风暴）；格式适配层按字节截断（含标记预算）、单条最多 10 条并在头部标明。**仍缺**：真实 webhook URL 才能做端到端；钉钉「加签」模式未实现（IP 白名单/关键词校验的机器人不需要） | 八场景实测：raw 逐字段相等 / dingtalk `msgtype:text` / wecom `msgtype:markdown` / 未知格式回退 raw / 30 条长告警 3999 字节 / 真转发 / 未配置 / 下游不可达仍 200 |
| 4 | 三条死告警取舍 | **已完成**：`SlowSyncLatency` / `HighFederationErrorRate` 随 `monitoring/` 删除消失；`megolm_vodozemac_pickle_persist_errors_total` 在 `f6792a734` 显式删除（告警 + 录制规则，规则文件里留了"何时加回"的注释） | `promtool check rules` 35/17 SUCCESS；`promtool test rules` SUCCESS |

**顺带修掉的（同一门禁、同一条验证命令）**：`scripts/ci/prometheus_target_allowlist` 的 key 写成了
job 名 `coturn`，而 `check_dead_targets` 比对的是 target 的 **host** ⇒ 这份豁免从登记起从未生效，
C 项长期 FAIL、把真问题淹在已知问题里。key 改为 `host.docker.internal` 后 C 项转 OK。
**同一批还清了两个历史文件的既有格式债**：`tests/prometheus-implementation-{plan,summary}.md`
有 14 行 trailing spaces（MD009）与 5 个 aspell 未识别词——它们此前从未被改动，因此从未进过检查面；
给它们加"已废弃"标注时一并清掉。

### 仍未定的两项（需要你的输入）

| 决策 | 影响 | 需要的输入 |
|---|---|---|
| `message_queue_depth` 是否只在 worker 部署下有意义 | `MessageQueueBacklog` 搬不搬、搬了要不要加 `deploy_mode="worker"` 标签 | 生产实际是单体还是 worker 拓扑 |
| `SlowSyncLatency` / `HighFederationErrorRate` 是"该补埋点"还是"不需要这个信号" | 决定第 4 步是补还是删 | 运维值班时是否真的会看 sync p95 / 联邦错误率 |
