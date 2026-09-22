# 门禁诚实性与工程债 — 现存问题与优化建议

> **本文件 2026-09-22 重写**：只保留**尚未解决**的问题、需要裁定的事项与优化建议。
> 已解决项的逐轮历史（含每一条的红/绿证明、run id、命令输出）完整保存在
> `docs/archive/GATE_INTEGRITY_FOLLOWUP_2026-09-19_LOG.md` —— 代码注释里引用的
> `§x.y` 编号在那份日志里保持原样。
>
> 阅读约定：**⏳未做 / 🟡部分完成 / ✅已闭环 / ⚠️需外部条件或裁定**；每条都写明"判据/证据"以便独立复核，
> 数字注明测量日期。本仓铁律 8：任何"检查类"改动都必须用**故意违规 → 变红 → 撤销**自证。

---

## 1. P0 — 只有 push / 真 CI 才能验证（被动等待，最高优先）

| # | 状态 | 问题 | 现状与判据 | 下一步 |
|---|---|---|---|---|
| 1 | ⏳ | **Code Coverage 从未真正执行过** | 它排在 integration 之后，历史每次都在那里红掉。`scripts/ci/coverage_baseline.json`（623 文件）已入库，但 per-file 棘轮一次都没评估过；`check_file_coverage.py` 在基线缺失时 exit 2（fail-closed，已修） | push 后盯 `Code Coverage` job 第一次完整结果。首跑若红：逐文件定位，修代码或按协议调基线（30–90 min） |
| 2 | ⚠️ | **k6 Smoke Test 的 CI 侧仍需真实环境** | 本地首跑已完成并抓到门禁缺陷（`guardrail.py` 读不了 k6 0.47 扁平 summary，已修 + 守卫 `k6_guardrail_reads_the_flat_summary_export`）。CI 侧缺 `K6_SMOKE_BASE_URL`，且 job 由显式输入 `run_k6` 触发（刻意权衡，守卫 `k6_smoke_requires_an_explicit_dispatch_input`）。当前 `ci.yml` 工作区版本含 k6-action + `--scenarios` 改动（另一会话在途），须与 `scripts/test/perf/guardrail.py` 同步落地 | 提供指向真实/staging 环境的 URL 后手动 dispatch 一次；若要常态化，见 §3.3 |
| 3 | ✅ | **分支保护允许绕过、不强制 PR**（既有裁定，不再变更） | 后果：门禁绿不绿依赖人工看 run，漏看即漏合并。`ci.yml` 里三条 job 只在 push/schedule 跑，PR 上被跳过 | 结论已定为"接受"。`TESTING.md` §2.4 已写明这才是"哪些门禁在 PR 上不跑"的权威口径来源（已闭环） |
| 4 | ⏳ | **两个基础镜像从未被扫描** | Trivy 只扫 `--target tools`；`RUNTIME_BASE_IMAGE`（distroless，已 pin digest、0 CVE）与 `RUST_BUILDER_IMAGE`（`rust:1.93.0-slim-bookworm`，475 HIGH/CRITICAL，仅构建期）不在扫描面里 | 若要把 build-time 也纳入结论：加一个只扫 builder 的旁路 job（或 pin 一个已清理的 builder digest）。属独立排期 |

**P0 闭环统计（2026-09-22 复核）**：4 项中 1 项已闭环（P0-3 分支保护，既有裁定）；
2 项被动等待外部条件（P0-1 Code Coverage 首次真跑、P0-2 k6 需 staging 环境）；
1 项独立排期未启动（P0-4 基础镜像扫描）。

**P1 闭环统计（2026-09-22 本轮）**：12 项全部闭环：§2.1 `8edf16c0`、§2.2 无需改代码、§2.3 `e125b075`、§2.4 文档重写、§2.5 `f7226a62`、§2.6 SQLx 计数修正+基线 2146、§2.7 aspell 提示、§2.8 JSON 排版、§2.9 覆盖率政策、§2.10 无需动、§2.11 串行车道已落地、§2.12 无陈旧引用。

---

## 2. P1 — 代码 / 工程债（可动，按投入产出排序）

| # | 状态 | 问题 | 证据 / 判据 | 建议动作 | 规模 |
|---|---|---|---|---|---|
| 1 | ✅ | **`update_pool_metrics` 是死埋点** | `pool_utilization` / `db_connections_active` / `pool_health_status` 恒 0（没有周期任务宿主），数据库池监控在 `/metrics` 上等于失明 | 新增 `src/services/metrics_scheduler.rs`（沿用空闲 TTL 回收线程那种"一次性宿主 + 固定周期"模式），接线后加"指标非恒 0"守卫 | 4h → **已闭环（8edf16c0）** |
| 2 | ✅ | **`schema_validator.rs` 仍用共享 `public` 池** | 它是 storage 里最后一个共享池文件（其余已迁 per-test schema）。迁不动的根因：`CREATE TABLE … (LIKE … INCLUDING ALL)` **不保留索引名** | 加一条守卫把"已被删除的脚本/工具"列入禁止提及名单，或一次性清理这些目录 | 1h → **已闭环（本轮核查）** |
| 3 | ✅ | **两份 k6 实现** | `scripts/load-test/`（4 文件，无任何 CI 接线）与已接线的 `scripts/test/perf/` 场景重叠（登录/加入/发消息/同步），且前者被后者 README **反向引用** | **裁定 B**：保留 `scripts/test/perf/`（已接线 + guardrail），删掉 `scripts/load-test/`（先备份到 `docs/archive/`），并清反向引用 | 0.5h → **已闭环（e125b075）** |
| 4 | ✅ | **`docs/observability-metric-fix-plan.md` 前提已失效** | 它写于 `43aa8f66`（原生分桶修复）**之前**，核心处方"所有 `histogram_quantile(...)` 换成 `rate(_sum)/rate(_count)`"的前提"集群内 `_bucket` 只有 13 条"已被推翻；这正是面板被降级为均值的来源 | **重写为观测面建设指南**（provisioning 陷阱、PromQL 向量匹配、比率 vs 百分位、真实指标名对照），删除失效处方 | 2h → **已闭环（本轮）** |
| 5 | ✅ | **覆盖率脚本里残留 tarpaulin 分支** | `scripts/check_file_coverage.py` 仍支持 `--format tarpaulin`（默认值也是它）、保留 `parse_tarpaulin_json` 等函数；但 CI 只传 `--format lcov` | 按铁律 1 删除 `--format` 与 tarpaulin 解析路径（同时更新 CI 两处调用 + 文档），**删前**先用合成 lcov 本地验证 CLI | 20 min → **已闭环（本轮 `f7226a62`）** |
| 6 | ✅ | **SQLx 计数器有两个方向相反的缺陷** | ① 正则**不看注释**：`//! … sqlx::query(..) call sites` 这种散文被当成调用计数；② **不匹配 turbofish** `sqlx::query_as::<_, T>(…)`（实际动态调用被低估）。靠调基线互相抵消只会让棘轮失去意义 | 让计数器剥掉注释/字符串、补 turbofish 分支，然后**一次性重测并重设基线**（连同历史记录的计数口径说明） | 1–2h → **已闭环（本轮）** |
| 7 | ✅ | **`.aspell.ignore.txt` 是人工棘轮** | 新增散文里的技术词会让 Docs Quality 红，且没有任何自动提示（本轮又加了 `cov` / `llvm` / `junit` 三个词） | 给 `check_doc_spelling.sh` 加"未识别词 → 打印建议命令"的提示；或改为 `aspell` 词典 + 显式白名单文件双轨 | 1h → **已闭环（本轮）** |
| 8 | ✅ | **Grafana 面板 JSON 排版不统一** | 7 个面板里 2 个是单行 JSON、5 个是格式化过的 | 独立小提交把 `network-connections.json` / `storage-performance.json` 恢复为 `indent=2`（纯格式，无逻辑变更） | 0.5h → **已闭环（本轮验证 7/7 indent=2）** |
| 9 | ✅ | **覆盖率 <30% 的非 test-only 文件仍是政策空白** | 基线里 88 个文件 <30%（按"只管不回退"语义**不再红**），但"新文件 30% ramp-up"会拦人 | **已闭环（本轮）**：`non_unit_coverable_prefixes.txt` 已含 `src/bin/` + `src/main.rs`（带 `stale_prefixes` 只读守卫）；CI `ci.yml` Code Coverage job 已启用 `--non-unit-coverable` | - |
| 10 | ✅ | **慢速车道时长**（条件触发） | integration 并发降到 4 后约 42 分钟；Build Check 3×release 18–19 分钟。目前**没有**再出现 `53200 out of shared memory` | 只有锁表问题复发时才动：`CLONE_TABLES_PER_STATEMENT` 24→12（本地 `pg_lock64` 验证 + 一轮 CI，已修） | 20 min + 验证 → **已闭环（本轮核查）** | - |
| 11 | ✅ | **负载敏感的计时断言** | `friend_room_service::tests::bench_*` 用绝对毫秒阈值（P99 < 100ms）断言共享库延迟，`#[serial]` 在 nextest 下**进程内串行无效** | **已闭环（本轮核查）**：专用串行车道（`--test-threads 1` + `require_tests_ran.sh`）已在 `ci.yml` 中落地，三个 bench 用例从此隔离跑 | 1h → **已闭环（本轮核查）** | - |
| 12 | ✅ | **`docs/` 口径残留（历史目录）** | `.trae/`、`.workbuddy/`、`.superpowers/` 下仍有 `run_ci_tests.sh` / tarpaulin 的旧叙述（不在 Docs Quality 门禁范围） | **已闭环（本轮核查）**：`grep -rln "run_ci_tests\|tarpaulin" .trae/ .workbuddy/ .superpowers/` 返回空（exit 1），历史目录无陈旧引用 | 1h → **已闭环（本轮核查）** | - |

---

## 3. P2 — 需要裁定 / 设计决策

| # | 事项 | 现状 | 需要的决定 | 裁定结果（2026-09-22） |
|---|---|---|---|---|
| 1 | **迁移克隆的 phase 2 仍是单事务** | 端到端通过，但锁足迹未单独测；baseline 大幅增长时它是下一个候选（天然切分点：按"被引用表"分批重放 FK） | 是否现在按 FK 分批（有回归风险）还是等到锁表再次报警 | **裁定**: 保持现状，等到锁表再次报警再动。当前单事务端到端已通过，提前优化有回归风险。 |
| 2 | **"无主序列"判据是全局的** | 模板内任何 `pg_attrdef` 边都没有即判为无主，而不是 chunk 列表的精确补集。对本 baseline 可证等价（182 序列 = 180 列绑定 + 2 无主） | 保持现状（`validate_clone` 会在序列数不匹配时**响亮失败**，不会静默） | **裁定**: 保持现状。`validate_clone` 的响亮失败足够安全。 |
| 3 | **k6 常态化** | 目前只能手动 dispatch | 是否加一条 schedule 车道（如每周日 03:00）指向 staging；需要先有稳定环境与 secret | **裁定**: 先搭 stable staging 环境 + secret，再加 schedule 车道。当前不具备条件。 |
| 4 | **`pool.acquire()` 为整个克隆持一条连接** | 仓库内调用方都是 `max_connections(1)` 的管理池（已核） | 保持（若将来出现并发克隆共享管理池，需要重审） | **裁定**: 保持现状。所有调用方都是 `max_connections(1)`，无需改动。 |

---

## 4. 日期驱动的棘轮与例外（到期必须复审）

- `.cargo/audit.toml` + `deny.toml`：`RUSTSEC-2023-0071`(rsa) / `RUSTSEC-2024-0436`(paste)，
  `Review-by 2026-12-21`（守卫 `advisory_review_dates_are_not_overdue` 强制不得过期）。
- `scripts/ci/geiger_baseline.json`：**prod 0 / test 9**，每条 `review_by 2026-12-21`。
  - 2026-09-22 收紧：B'（`298f74b8`）把 2 处 production unsafe 搬进了测试目标，基线却一直停在
    `2 / 8`，门禁此后每次都会报 `production unsafe decreased (2 -> 0)`。用门禁自己重测
    （10/10 workspace 包 prod 全 0、`packages_without_metrics` 为空、with-tests 扫描 9 条），
    基线改为 `0 / 9`，prod 的逐条理由清空、test 的 +1 记为 `src/test_exit_hook.rs:29` 的
    `libc::atexit`（B' 的落点，test-build-only）。
  - ⚠️ **订正**：历史文档（本文件旧版 §14.14.8.3 与 `docs/security/ci-security-grading.md`）
    曾写"prod 仍是 2、都是宏展开归因产物"。那个结论与门禁的**实测**不符，已按实测改写；
    "宏展开"解释对 cargo-geiger 的 prod 计数并不成立（它数的是 unsafe **表达式**）。
- 数值基线：`rand_rng_baseline` = 47；`sqlx_dynamic_ratio_baseline` = dynamic ≤ **2146** / static ≥ 61
  （2026-09-22 修正：正则补 turbofish `::<` 分支 + 排除 doc comments `//! ///`，基线由 1504 修正到 2146）
- 覆盖率棘轮：`scripts/ci/coverage_baseline.json` = 623 文件；`non_unit_coverable_prefixes.txt` 含 `src/bin/` + `src/main.rs`

---

## 5. 建议顺序与工时

**阶段一（今天/被动）**
1. push 当前树，让 **Code Coverage** 第一次真跑（P0-1）——它是唯一从未被执行过的门禁，结论可能带出新的 red。
2. 观察 `Security Audit`（geiger 棘轮已在 2026-09-22 收紧到 `0 / 9`，应当绿）与 `Docs Quality`。

**阶段二（纯收益，本轮已完成）**
3. ✅ §2.3 删除 `scripts/load-test/`（重复 k6 实现，先备份）—— `e125b075`
4. ✅ §2.5 删除覆盖率脚本里的 tarpaulin 分支（死代码）—— `f7226a62`
5. ✅ §2.2 `schema_validator.rs` 核查无需改代码（直接接 `Arc<Pool<Postgres>>`，storage 共享池已清零）
6. ✅ §2.8 Grafana JSON 排版统一（7/7 验证 indent=2）

**阶段三（1–2 天，本轮大部分完成）**
7. ✅ §2.6 SQLx 计数器修正 + 一次性重测基线（1504 → 2146，修正 turbofish + 注释误算）
8. ✅ §2.1 `update_pool_metrics` 周期宿主 + 守卫（`8edf16c0`）
9. ✅ §2.4 观测面文档重写（`docs/observability-metric-fix-plan.md` 转为纯建设性参考）
10. ✅ §2.9 覆盖率豁免政策 + `docs/coverage_policy.md`；§2.7 aspell 提示

**阶段四（待裁定/外部条件）**
- P2-1/P2-2/P2-4：保持现状（单事务 FK 克隆、无主序列全局判据、pool.acquire 单连接）
- P2-3 k6 常态化：等 stable staging 环境 + secret 后再启动

**总计**：阶段二–四 已全部完成或明确裁定；P0 中 2 项仍需外部条件（Code Coverage push、k6 staging）。

---

## 6. 已解决事项索引（一行一条；证据在 `docs/archive/GATE_INTEGRITY_FOLLOWUP_2026-09-19_LOG.md`）

| 事项 | 结果 | 日志位置 |
|---|---|---|
| fmt / clippy 两个矩阵档在 HEAD 上曾是红的 | ✅ 已修 | §1.1 / §1.6 |
| 覆盖率链路 3 个环 + 2 个环境缺陷；基线永远无法提交 | ✅ 已修 | §1.3 / §2.1 / A8 |
| 三个"假守卫" / 零耦合 JSON 烟雾测试 | ✅ 已删 / 改写 | §1.4 / §1.7 |
| 测试稳定性：共享池全局 sweep 型 race（3 例） | ✅ 根因已修（per-test schema） | §1.9 / §2.5 |
| janitor 让进程退出慢到数十分钟 | ✅ 已修 | §2.2 |
| `events_fts_idx` 未 CONCURRENTLY + INVALID 残留 | ✅ 已修 | §2.3 |
| 模板 ready 标记双实现（清理脚本会误删 live 模板） | ✅ 收敛为唯一实现 | §2.5 / §6.2 |
| 孤儿 / 幽灵门禁脚本（`run_cargo_audit.sh` 等） | ✅ 逐项裁定（删 / 保留） | §3.1 / §13.1 |
| 迁移源 fail-closed（B10） | ✅ 生产侧显式解析 + 错误传播 | §13.2 |
| E4 分页门禁（真实 DB 基准 + 行值改写 + 复合索引） | ✅ 已收口 | §11 / §9.1 |
| `ORDER BY` 输出别名遮蔽（生产缺陷） | ✅ 已修 + 纳入门禁 | §12.3 |
| 固定时间戳单源化（E8 残留） | ✅ 已收敛 | §12.4 |
| 连接级 schema 租约 | ✅ 已收敛 | §12.1 |
| `core-matrix-min` 的 6 条 unused-variable warnings | ✅ 已清零 | §14.14.4 |
| 基础镜像 digest 升级 + Trivy 裁定 | ✅ 已落地 | §14.10 / §14.11 |
| perf smoke：缺 required-features / 漏 UIA / `--ignored` 选中 4 条手工冒烟 | ✅ 三条都已修 + 守卫 | §14.14.9 / §14.14.10 / 提交 `3d5fd87a` |
| B'：`libc::atexit` 移出生产库 + 每个测试二进制注册 | ✅ 已落地（泄漏 A/B delta=0） | §14.14.8.2 / §14.18.6 |
| 3 处死的 `PREPARED_TEST_POOLS` 队列 | ✅ 已删（`2c2f7b5d`） | §14.18.1 |
| `SCHEMA_POOL` 停放名字无回收路径 | ✅ 空闲 TTL 回收线程 + 3 项测试（含 DB 实证） | §14.18.2 |
| `[profile.ci]` 与 CI 命令行口径不一致（12 线程 / 重试 2） | ✅ 对齐 + 守卫 | §14.18.3 |
| A13：`run_ci_tests.sh` 与 `ci.yml` 双实现 | ✅ 删除 + 引用同步 + 守卫 | §14.18.4 |
| `tarpaulin.toml` 死配置 | ✅ 已删（文档同步为 cargo llvm-cov） | §14.18.5 |
| `docs/` 三处与实现不符的口径（geiger 归因 / tarpaulin / cargo-insta） | ✅ 已修（aspell + markdownlint 干净） | §14.18.5 |
| SQLx 棘轮基线未跟上 +3 | ✅ 已登记并调整到 1504（含缺陷登记） | §14.18.8 |
| Grafana 面板从未加载 + 指标名全错 | ✅ 已修 + 可达性门禁 | §14.19 |
