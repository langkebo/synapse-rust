# 配置双副本一致性门禁（S 系列 P2 #4）

> **日期**: 2026-09-11
> **基线提交**: `a9901286`
> **对应待办**: `docs/audit/S_series_verification_2026-09-11.md` §6 P2 第 4 项

---

## 1. 缺陷：手工同步的配置双副本

仓库有两套配置树：

| 目录 | 使用者 |
|---|---|
| `docker/deploy/config/` | `docker/deploy/docker-compose.yml`（部署编排） |
| `docker/config/` | `docker/docker-compose.yml`（开发）**并**被打进镜像 |

它们靠**手工 `cp`** 保持同步。`docker/deploy/README.md` 文档化了这个约定，
但**没有任何东西强制它**：

* `deploy.sh` 不做拷贝（`grep -nE "cp .*config|rsync" docker/deploy/deploy.sh` → 无命中）；
* 没有任何 CI 步骤比较两侧。

这与**迁移双副本是同一类缺陷** —— 手工同步的副本静默漂移。
迁移已在 `2b16dc3c` 根治（单一真相源 + `check_migration_consistency.py` 阻塞 CI），
配置侧没有。

### 已实际造成的损害

2026-09-11 发现两侧 `rate_limit.yaml` 的 `sync.enabled` 不一致
（dev `false` / deploy `true`），而两份 `homeserver.yaml` 都写 `true`。
因为文件配置**整体替换** `rate_limit:` 段，文件赢 → `/sync` **完全没有限流**：

```console
$ for i in $(seq 1 120); do curl ... "/_matrix/client/v3/sync?timeout=0"; done | sort | uniq -c
    120 200                      # 零限流
```

已修复（提交 `14570209`），但**没有任何机制阻止它再次发生**。本次补上。

---

## 2. 修复：语义一致性门禁

新增 `scripts/check_config_consistency.py`，接入 `ci.yml` 的 `repo-sanity` job（阻塞）。

### 比较方式：语义，而非字节

比较前剥离注释与空行后按 YAML 叶路径展平为 `{dotted.path: value}`。
**纯注释差异不报** —— 两棵树本来就承载不同的说明注释
（例如 deploy 侧的 `homeserver.yaml` 注明 `rate_limit:` 段在运行时失效）。
若按字节比较，这个门禁会因注释反复误报，最终被人关掉。

### 有意的差异必须显式登记

```python
ALLOWED_DIFFERENCES: dict[tuple[str, str], str] = {
    ("rate_limit.yaml", "sync.enabled"): (
        "开发环境刻意关闭专用 /sync 限流，避免本地反复登录/压测时吃假阳性 429；"
        "生产（deploy）必须为 true —— /sync 在路由 ledger 中被标记 rate_limit_exempt，"
        "不受通用 IP 限流约束，本段是它唯一的限流来源。"
        "曾因两侧不一致导致 /sync 零限流（见 S_series_verification_2026-09-11.md §2）。"
    ),
}
```

* 键是**精确的叶路径**（`文件` + `section.key`），不是"整文件豁免" ——
  后者等于把门禁作废。
* 必须**写明理由**，且理由出现在 CI 失败信息与 `--explain` 输出里，
  这样"放行"这个动作在 review 中可见。

### 两侧文件集合也检查

只在单侧存在的配置文件会被记为 warning（新增文件忘同步是常见疏漏）。

---

## 3. 当前两侧的真实差异

剥离注释后：

| 文件 | 语义差异 |
|---|---|
| `homeserver.yaml` | **无**（纯注释差异） |
| `rate_limit.yaml` | **仅** `sync.enabled`（已登记，有意为之） |
| `postgres.conf` | **无**（纯注释差异） |

即：本次修复后，两侧的语义差异**恰好等于**白名单里的那一项。

---

## 4. 回归证据（7 个新测试）

`tests/unit/config_consistency_gate_tests.rs`：

```console
$ cargo nextest run --profile test --features test-utils --test unit -E 'test(/config_consistency_gate_tests/)'
    PASS config_consistency_gate_tests::config_consistency_checker_exists
    PASS config_consistency_gate_tests::checker_passes_on_current_tree
    PASS config_consistency_gate_tests::checker_supports_json_report
    PASS config_consistency_gate_tests::checker_ignores_comment_only_differences
    PASS config_consistency_gate_tests::intentional_sync_enabled_difference_is_allowlisted_and_documented
    PASS config_consistency_gate_tests::checker_is_wired_into_ci
    PASS config_consistency_gate_tests::rate_limit_config_is_compared
    Summary [0.100s] 7 tests run: 7 passed
```

**门禁有效性验证**（把 dev 侧 `default.per_second` 从 50 改成 77）：

```console
$ python3 scripts/check_config_consistency.py
  issue: rate_limit.yaml: default.per_second: dev='77' deploy='50'
check_config_consistency: FAIL
EXIT=1
```

即它确实拦得住真实漂移，不是空转。已还原（`git diff` 为空）。

---

## 5. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 正常应通过
python3 scripts/check_config_consistency.py
python3 scripts/check_config_consistency.py --explain      # 打印被放行的差异及理由

# 制造漂移 → 应失败
cp docker/config/rate_limit.yaml /tmp/rl.bak
sed -i '' 's/^  per_second: 50$/  per_second: 77/' docker/config/rate_limit.yaml
python3 scripts/check_config_consistency.py; echo "EXIT=$?"   # 1
cp /tmp/rl.bak docker/config/rate_limit.yaml

# JSON 报告（CI 归档用）
python3 scripts/check_config_consistency.py --json-report artifacts/config_consistency.json
```

---

## 6. 门禁

```console
$ ./scripts/check_fmt_ratchet.sh
OK: fmt debt at baseline (0), no regression.
$ python3 scripts/check_migration_consistency.py     # issues: []  warnings: []
$ bash scripts/ci/check_sqlx_dynamic_ratio.sh        # OK
$ python3 scripts/check_baseline_consolidation.py    # 已吸收全部 36 个增量迁移的对象
$ cargo clippy --workspace --all-targets --all-features --locked
CLIPPY_EXIT=0 errors=0 warnings=15                   # 15 = 既有基线
$ cargo nextest run --profile test --features test-utils --lib --test unit
Summary [104.481s] 2495 tests run: 2495 passed (1 slow), 2 skipped
```

---

## 7. 剩余待办

| # | 项 | 说明 |
|---|---|---|
| 1 | `TESTING.md` P95 阈值标定或删除 | **待决策**：文档写 500/1000/100ms，实测 1.95–6.4ms，代码里唯一阈值是 sliding-sync 的 5000ms 且从不执行 |
| 2 | 同机同参数性能回归比对（P4 §8.2 #4） | 需要稳定的采集环境 |
| 3 | 采集 `performance_sliding_sync_benchmarks`（P4 §8.2 #5） | 低优先级，需服务/DB |
| 4 | CI 上确认 `sliding-sync-perf-gate` 首跑（P4 §8.2 #6） | 本地无法完整复现 runner 环境 |
| 5 | presence stream 游标 / 联邦 knock / `get_raw` 弃用 / `RateLimitConfigAdapter` 死表面 | S 系列 P2/P3 |
