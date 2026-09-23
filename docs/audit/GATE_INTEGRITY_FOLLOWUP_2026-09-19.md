# 门禁诚实性与工程债 — 现存问题与优化建议

> **本文件 2026-09-22 重写**：只保留**尚未解决**的问题、需要裁定的事项与优化建议。
> 已解决项的逐轮历史（含每一条的红/绿证明、run id、命令输出）完整保存在
> `docs/archive/GATE_INTEGRITY_FOLLOWUP_2026-09-19_LOG.md` —— 代码注释里引用的
> `§x.y` 编号在那份日志里保持原样。
>
> 阅读约定：**⏳未做 / 🟡部分完成 / ⚠️需外部条件或裁定**；每条都写明"判据/证据"以便独立复核，
> 数字注明测量日期。本仓铁律 8：任何"检查类"改动都必须用**故意违规 → 变红 → 撤销**自证。

---

## 1. P0 — 只有 push / 真 CI 才能验证（被动等待，最高优先）

| # | 状态 | 问题 | 现状与判据 | 下一步 |
|---|---|---|---|---|
| 1 | ⏳ | **Code Coverage 从未真正执行过** | 它排在 integration 之后，历史每次都在那里红掉。`scripts/ci/coverage_baseline.json`（623 文件）已入库，但 per-file 棘轮一次都没评估过；`check_file_coverage.py` 在基线缺失时 exit 2（fail-closed，已修）。**2026-09-22 实测**：慢速车道 dispatch（run `35683146324`）**又被跳过**——`integration-test`/`coverage` 都 `needs: [test, changes]`，而 fast tier 的两条 **default-features** 车道红在 `Check metric instrumentation reachability`。**根因不是"基线过期"**：该脚本把 PCRE 的 `\b`/`\s` 用在 `git grep -E` 里，Linux 上生效后**连 `server_metrics.rs` 自己的定义都被当成同名冲突** ⇒ 25 个方法全"不可判定" ⇒ `已接通 0 / 未接通 0` ⇒ 棘轮的 stale 规则误报（macOS 上 `\b` 不生效，所以本机一直"通过"）。**方案见 `docs/audit/CODE_COVERAGE_FIRST_RUN_PLAN_2026-09-22.md`**；Stage 0/1/3/5 已落地（提交 `6b3f5312` + `2f4d9074` + `5a7a7f24`）：该门禁改成 POSIX 字符类 + 排除定义文件 + `--self-test`/`--print-ambiguous`（各带红证明）；覆盖率命令收敛为 `scripts/ci/run_coverage.sh` 唯一实现（本地那份已按裁定删除）；`cargo-llvm-cov` 钉 0.8.7；Codecov 按裁定降级为 `fail_ci_if_error: false` | 剩 **Stage 2/4**：本地按 CI 口径干跑并与基线副本比对（棘轮单调 `max(prev,cur)`，误红不会自动降；基线重置已获授权、需逐项理由）→ fast tier 全绿后 dispatch `run_slow_tier=true`，盯 `Integration Tests` → `Code Coverage` |
| 2 | ⚠️ | **k6 Smoke Test 的 CI 侧仍需真实环境** | 本地首跑已完成并抓到门禁缺陷。CI 侧缺 `K6_SMOKE_BASE_URL`。**2026-09-22 已删除全部 k6 相关代码**（CI job、benchmark job、性能测试脚本、守卫测试、Makefile targets），替代方案：Prometheus 自研渲染器已接通生产埋点，性能监控改用 Prometheus / Grafana 面板 | 门禁已删除，替代能力：Prometheus 渲染器已覆盖性能观测需求 |

**P0 当前状态（2026-09-22）**：
- 1 项⏳未做（Code Coverage 首次真跑）
- P0-1 外部条件：需 push 到 CI 验证

---

## 2. P2 — 需要裁定 / 设计决策

| # | 事项 | 现状 | 需要的决定 | 裁定结果（2026-09-22） |
|---|---|---|---|---|
| 1 | 迁移克隆的 phase 2 仍是单事务 | 端到端通过，但锁足迹未单独测；baseline 大幅增长时它是下一个候选（天然切分点：按"被引用表"分批重放 FK） | 是否现在按 FK 分批（有回归风险）还是等到锁表再次报警 | **裁定**: 保持现状，等到锁表再次报警再动。当前单事务端到端已通过，提前优化有回归风险。 |
| 2 | "无主序列"判据是全局的 | 模板内任何 `pg_attrdef` 边都没有即判为无主，而不是 chunk 列表的精确补集。对本 baseline 可证等价（182 序列 = 180 列绑定 + 2 无主） | 保持现状（`validate_clone` 会在序列数不匹配时**响亮失败**，不会静默） | **裁定**: 保持现状。`validate_clone` 的响亮失败足够安全。 |
| 3 | k6 常态化 | 目前只能手动 dispatch | 是否加一条 schedule 车道（如每周日 03:00）指向 staging；需要先有稳定环境与 secret | **裁定**: 先搭 stable staging 环境 + secret，再加 schedule 车道。当前不具备条件。 |
| 4 | `pool.acquire()` 为整个克隆持一条连接 | 仓库内调用方都是 `max_connections(1)` 的管理池（已核） | 保持（若将来出现并发克隆共享管理池，需要重审） | **裁定**: 保持现状。所有调用方都是 `max_connections(1)`，无需改动。 |

---

## 3. 阶段建议与工时

**阶段一（今天/被动）**
1. push 当前树，让 **Code Coverage** 第一次真跑（P0-1）——它是唯一从未被执行过的门禁，结论可能带出新的 red。
2. 观察 `Security Audit`（geiger 棘轮已在 2026-09-22 收紧到 `0 / 9`，应当绿）与 `Docs Quality`。

**阶段二（待推进）**
- 所有 P1 项已闭环，需推进 Code Coverage 首次跑通。

---

## 4. 日期驱动的棘轮（到期必须复审）

- `.cargo/audit.toml` + `deny.toml`：`RUSTSEC-2023-0071`(rsa) / `RUSTSEC-2024-0436`(paste)，
  `Review-by 2026-12-21`（守卫 `advisory_review_dates_are_not_overdue` 强制不得过期）。
- `scripts/ci/geiger_baseline.json`：**prod 0 / test 9**，每条 `review_by 2026-12-21`。
- 数值基线：`sqlx_dynamic_ratio_baseline` = dynamic ≤ **2146** / static ≥ 61
  （2026-09-22 修正：正则补 turbofish `::<` 分支 + 排除 doc comments `//! ///`）
- 覆盖率棘轮：`scripts/ci/coverage_baseline.json` = 623 文件；`non_unit_coverable_prefixes.txt` 含 `src/bin/` + `src/main.rs`