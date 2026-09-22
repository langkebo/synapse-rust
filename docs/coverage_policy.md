# Coverage Policy — synapse-rust

> 权威门禁：`scripts/check_file_coverage.py` + `scripts/ci/coverage_baseline.json`。
> 本文档是政策的**唯一**来源，CI 配置变更时须同步更新。

---

## 1. 棘轮语义

- **已记录在 baseline 中的文件**：只检查**不回退**（值不得低于 baseline 记录值），
  不再重放绝对阈值。
- **新文件**（baseline 中没有）：
  - 核心路径（`core_file_coverage_prefixes.txt` 前缀）：≥ 70%
  - 其余：≥ 30%（ramp-up 底线）
  - **豁免前缀**（`non_unit_coverable_prefixes.txt`）：30% ramp-up 不生效
- **test-only 支持代码**（`test_mocks/`、`synapse-test-utils/`、`*test_utils.rs`、
  `*test_isolation.rs`、`*test_schema_guard.rs`、`scripts/bench_harness.rs`）：
  直接跳过（既非生产、也无法被覆盖率腿测量，记录 0% 无意义）

---

## 2. 覆盖率腿的盲区

CI 用 `cargo llvm-cov --workspace --lib`（+ `-p synapse-storage --lib`）。
`--lib` **只编译并执行 library 单元测试**：

| 路径 | 是否被腿执行 | 记录值 | 政策 |
|---|---|---|---|
| `src/main.rs` / `src/bin/*.rs` | 否（binary 入口不链接） | 0.0% | `non_unit_coverable_prefixes.txt` 豁免，ramp-up 不生效；baseline 已知仍检查不回退 |
| `tests/**/*.rs` | 是（但独立于 `--lib` 腿） | 不记录 | 不在扫描范围，由 integration 腿覆盖 |
| `#[cfg(test)]` 内联模块 | 是（随 crate 编译） | 记录 | 计入（同样存在 schema 漂移风险） |
| `*test_utils.rs` 等 test-support | 否（`cfg(test)` 消费） | 0.0% | test-only 直接跳过 |

**已知 111 个文件 <30%**（2026-09-22 测量）：
- 6 个 `src/bin/*` + `src/main.rs`：按上表豁免
- 105 个生产代码：按"只管不回退"语义不再红；**重命名/移动会触发 70% core 地板或 30% ramp-up**（这是有意的棘轮压力）

---

## 3. 豁免前缀的守卫

`non_unit_coverable_prefixes.txt` 受三重守卫保护（`check_file_coverage.py`）：

1. **缺失守卫**：`--non-unit-coverable` 文件不存在 ⇒ exit 2（fail-closed）
2. **空文件守卫**：文件存在但 0 行 ⇒ exit 2
3. **陈旧前缀守卫**：`stale_prefixes()` 扫描磁盘 `*.rs`，
   前缀匹配不到任何文件 ⇒ exit 2（防止"豁免"静默失效）

CI 调用 `ci.yml` Code Coverage job 的两处：
```
python3 scripts/check_file_coverage.py \
  --report coverage/lcov.info \
  --format lcov \
  --baseline scripts/ci/coverage_baseline.json \
  --global-floor 40 --new-file-floor 30 \
  --core-files scripts/ci/core_file_coverage_prefixes.txt \
  --core-threshold 70 \
  --non-unit-coverable scripts/ci/non_unit_coverable_prefixes.txt
```

---

## 4. Baseline 自动提交

- `ci.yml` "Commit coverage baseline (auto-ratchet)" step：
  main push 时把更新后的 `coverage_baseline.json` 自动提交
  （`chore(coverage): auto-update per-file baseline [skip ci]`）
- PR / schedule / dispatch **不**自动提交
- 本地手动重测基线：
  ```bash
  python3 scripts/check_file_coverage.py \
    --report coverage/lcov.info \
    --baseline scripts/ci/coverage_baseline.json \
    --save-baseline scripts/ci/coverage_baseline.json \
    --global-floor 0 --new-file-floor 0 --core-threshold 0
  ```

---

## 5. 相关棘轮

- SQLx 动态/静态查询棘轮：`scripts/ci/check_sqlx_dynamic_ratio.sh`
  （动态 ≤ 2146 / 静态 ≥ 61，2026-09-22 修正注释误算 + turbofish 计数后重测）
- 覆盖率 <30% 文件：本文档 §2 已说明政策，不再单独设门禁
- 新文件 ramp-up 拦截：`check_file_coverage.py` 的 new-file-floor 参数

---

## 6. 文档索引

| 文件 | 用途 |
|---|---|
| `scripts/ci/coverage_baseline.json` | 每文件覆盖率快照（623 文件） |
| `scripts/ci/core_file_coverage_prefixes.txt` | 核心路径前缀（≥70%） |
| `scripts/ci/non_unit_coverable_prefixes.txt` | 覆盖率腿盲区前缀（ramp-up 豁免） |
| `scripts/check_file_coverage.py` | 棘轮执行脚本 |
| `docs/audit/GATE_INTEGRITY_FOLLOWUP_2026-09-19.md` | 门禁诚实性与工程债（现存问题） |
