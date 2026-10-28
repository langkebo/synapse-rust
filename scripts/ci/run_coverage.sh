#!/usr/bin/env bash
# 覆盖率测量的**唯一实现**（CI 与本地共用同一条命令）。
#
# 为什么要有这个脚本：此前同一件事有两份实现 —— `ci.yml` 的 `Run coverage` 步骤内联一段、
# `scripts/run_local_coverage.sh` 又一份 —— 两边的 storage 步 feature 集已经不同（8 vs 10）。
# 覆盖率数字只有在"同一口径"下才可比；而 `scripts/ci/coverage_baseline.json` 是 per-file
# 单向棘轮（`save_baseline` 取 `max(prev, cur)`，只降不升会一直红），口径分叉的代价是
# **永久红**。所以：命令只写在这里，CI 调用它，本地也调用它（铁律 2）。
#
# 用法：
#   bash scripts/ci/run_coverage.sh                 # 产出 coverage/lcov.info
#   TEST_THREADS=6 bash scripts/ci/run_coverage.sh
#   SKIP_CLEANUP=1 bash scripts/ci/run_coverage.sh
#
# 前置：
#   * `cargo-llvm-cov` 与 `llvm-tools-preview`（缺失时本脚本报错并给出安装命令）；
#   * 测试库已播种：`bash scripts/ci/prepare_test_db.sh`（public + `test_template_ci`）。
#
# 为什么是两步（历史根因，别再合并成一步）：
#   1. synapse-storage 的 db_tests 直连 **public** schema，与其它测试并发时相互 DROP/竞争；
#      因此它单独跑且单线程（`RUST_TEST_THREADS=1`）。
#   2. 其余 crate + 集成测试用与 route_ledger 快照一致的 feature 集（不是 --all-features：
#      friends/widgets 等非默认 feature 会在多核/有限内存下触发并发 clone 失败与 OOM）；
#      并跳过 `ledger_export_tests`（它的 fixture 反映导出二进制的默认 build）。
#   3. `scripts/merge_lcov.py` 合并两份 lcov。
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

# 口令的单一来源见 scripts/lib/db_credentials.sh（deploy 栈生成随机强口令）。
# shellcheck source=../lib/db_credentials.sh
source "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../lib/db_credentials.sh"
DATABASE_URL="${DATABASE_URL:-postgresql://synapse:$(db_password)@localhost:5432/synapse_test}"
TEST_DATABASE_URL="${TEST_DATABASE_URL:-$DATABASE_URL}"
TEST_DB_TEMPLATE_SCHEMA="${TEST_DB_TEMPLATE_SCHEMA:-test_template_ci}"
OUTPUT_DIR="${OUTPUT_DIR:-coverage}"
# 显式钉并发：CI 的 4 vCPU 与本地 4 线程因此等价，覆盖率数字才可跨环境比较。
# 大于 4 需要先调高锁表上限（见 scripts/tune_test_db.sh），否则会 `53200 out of shared memory`。
TEST_THREADS="${TEST_THREADS:-4}"

# storage 步**刻意**多带几个 feature：它们门控 synapse-storage / synapse-common 的代码，
# 且已提交的 per-file 基线就是在这一口径下产出的。
#
# 2026-10-27 补 `friends,widgets,burn-after-read`：这三个 feature 门控着
# `friend_room/repository.rs`(1390 行) / `widget.rs`(1332) / `burn_after_read.rs`(1066)
# —— 不含它们时**文件根本不编译、永远不进报告**，棘轮对近 3800 行代码零压力
# （基线里那三条 0.0 就是这么冻住的，见 TEST_COMPROMISE_AUDIT_2026-10-25 §7.5）。
# 实测（本机、单线程）：这三个模块的测试 **27 + 19 + 25 = 71 条全过**，storage 腿的耗时
# 边际增量 ~2 min（它本来就是单线程跑全量，见步骤 1 注释）。
STORAGE_FEATURES="test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,cas-sso,saml-sso,external-services,builtin-oidc,friends,widgets,burn-after-read"
# 其余 crate + 集成测试的 feature 集必须与 route_ledger 快照一致（见文件头注释）。
REST_FEATURES="test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,cas-sso,saml-sso"

# 步骤 2 的**报告范围**：`cargo llvm-cov report` 不支持 `--workspace`，必须逐个 `-p`。
# 这份名单 = 工作区成员 − synapse-storage（它由步骤 1 单独覆盖）。
# 漂移守卫：`tests/unit/coverage_leg_resilience_tests.rs::rest_report_scope_matches_the_workspace`
# 会拿 `cargo metadata` 的成员列表对账 —— 加了 crate 却忘了这里，报告就会静默少覆盖它。
REST_REPORT_ARGS=(
    -p synapse-cache
    -p synapse-common
    -p synapse-e2ee
    -p synapse-federation
    -p synapse-rust
    -p synapse-services
    -p synapse-test-utils
    -p synapse-web
)

log() { printf '[coverage] %s\n' "$*"; }

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
    log "ERROR: cargo-llvm-cov not found. Install the pinned version:"
    log "  rustup component add llvm-tools-preview"
    log "  cargo install cargo-llvm-cov --locked --version 0.8.7"
    exit 1
fi
if ! command -v psql >/dev/null 2>&1; then
    log "ERROR: psql not found（前置自检需要它；CI 的 ubuntu runner 自带 postgresql-client）"
    exit 1
fi

# 前置自检：模板 schema 必须在位。缺它时 `prepare_shared_test_pool` 会退化成"每测试跑迁移"，
# 覆盖率会跑到超时；更糟的是会把"环境没准备好"误报成测试失败。
template_tables="$(psql "$TEST_DATABASE_URL" -tAc \
    "SELECT count(*) FROM information_schema.tables WHERE table_schema='${TEST_DB_TEMPLATE_SCHEMA}'" 2>/dev/null || echo 0)"
if [ "${template_tables:-0}" -lt 100 ]; then
    log "ERROR: TEST_DB_TEMPLATE_SCHEMA='${TEST_DB_TEMPLATE_SCHEMA}' has ${template_tables} tables (expected >100)."
    log "  Seed both public and the template first:"
    log "    DATABASE_URL=$TEST_DATABASE_URL bash scripts/ci/prepare_test_db.sh"
    exit 1
fi

# 锁表上限是**环境前置条件**，而且它的缺失会伪装成"上百个测试失败"：
# 单个 `DROP SCHEMA … CASCADE` 在 v12 基线（227 表 / 707 索引 / 182 序列）上要约
# 1200 个锁条目，`max_locks_per_transaction=64`（镜像默认）时清理与克隆会以
# `53200 out of shared memory` 失败（`scripts/cleanup_test_schemas.sh` 头部有同一条结论）。
# 这里只**报告**、不改配置：CI 的 service container 无法传 `-c max_locks_per_transaction`
# （ci.yml 已在源码层确认 runner 把 `-c` 当 `--cpu-shares`）。
max_locks="$(psql "$TEST_DATABASE_URL" -tAc 'SHOW max_locks_per_transaction' 2>/dev/null || true)"
case "$max_locks" in
    '' | *[!0-9]*) max_locks='?' ;;
esac
log "max_locks_per_transaction = ${max_locks}"
if [ "$max_locks" != '?' ] && [ "$max_locks" -lt 128 ]; then
    log "WARNING: 锁表上限 ${max_locks} < 128。DROP/克隆密集路径可能报 53200 out of shared memory，"
    log "         而它会被表达成**测试失败**而不是环境问题。本地修法：bash scripts/tune_test_db.sh"
fi

# `cargo llvm-cov report` 只看 target 目录里的 profraw，**不区分是哪一轮写的**。
# 上一轮被 kill（超时/手动中断）时 profraw 会留在磁盘上，于是本轮报告混进旧数据
# ⇒ 覆盖率虚高 ⇒ 棘轮**假绿**（比无数据更糟，且事后不可见）。
# 直接删比 `cargo llvm-cov clean --workspace` 便宜得多：后者会清掉已编译产物、
# 触发全量重编（CI 上十分钟量级）。路径随 `CARGO_TARGET_DIR` 走，测试才能隔离。
LLVM_COV_TARGET_DIR="${CARGO_TARGET_DIR:-target}/llvm-cov-target"
if [ -d "$LLVM_COV_TARGET_DIR" ]; then
    stale_profraw="$(find "$LLVM_COV_TARGET_DIR" -name '*.profraw' -print -delete 2>/dev/null | wc -l | tr -d ' ')"
    log "清掉上一轮残留的 profraw: ${stale_profraw} 个"
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
STORAGE_LCOV="$TMP_DIR/storage.lcov"
REST_LCOV="$TMP_DIR/rest.lcov"
mkdir -p "$OUTPUT_DIR"

log "DATABASE_URL            = $DATABASE_URL"
log "TEST_DB_TEMPLATE_SCHEMA = $TEST_DB_TEMPLATE_SCHEMA (${template_tables} tables)"
log "TEST_THREADS            = $TEST_THREADS"
log "OUTPUT_DIR              = $OUTPUT_DIR"

# 步骤 2/3 跑的是 `--workspace --exclude synapse-storage` 的**全部**测试，其中包含
# `synapse-e2ee` 的 `vodozemac_interop_tests`。那个模块在 `CI` 下**故意 fail-closed**：
# 缺少 `E2EE_INTEROP=1` 时 `skip_message()` 直接 panic（sweep A1 残留：否则 lib 门禁会以
# "零断言"报绿）。CI 的 `Run library unit tests (--workspace --lib)` 早就设了这个变量，
# 而 Coverage 车道此前从未真正跑过 ⇒ 2026-09-30 首次执行时 18 个 interop 用例全 panic、
# `Run coverage` exit 101。这里与那条车道保持同一口径（本地跑覆盖率也一并覆盖这些用例）。
export E2EE_INTEROP=1

# ── 一条腿的执行：**测试失败也必须留下覆盖率数据** ────────────────────────────
#
# 旧实现直接把 `--lcov --output-path` 交给 cargo-llvm-cov，于是"先跑测试再写报告"
# 被绑成一次调用：**测试失败 ⇒ 报告根本不写**（cargo-llvm-cov 0.8.7 实测，见
# docs/audit/TEST_QUALITY_FINDINGS_2026-10-28.md §6.1）。在 CI 上的后果不是"少一个
# 数字"，而是整条车道的产出被抹掉：`Run coverage` 一红 ⇒ `Per-file coverage ratchet`
# 无数据可比 ⇒ per-file 棘轮**从未真正执行过**，而 §4 里"134 个文件未定性"正是这么来的。
#
# 拆成两步即可解耦：`--no-report` 只跑测试（失败也无妨，profraw 已经写到磁盘），
# 再由 `cargo llvm-cov report` 从 profraw 生成报告。`--no-fail-fast` 让一个测试
# binary 失败时不至于让后面的 binary 根本不跑（否则数据只覆盖前半截）。
#
# 退出码语义**不变**：任何一条腿有失败，脚本最后仍以非零退出（红还是红），
# 只是数据先落盘、可被下载与定性。
# ⚠️ 报告范围必须显式给（`-p …`，写在 `--` 分隔符之前）：`cargo llvm-cov report`
# **不接受 `--workspace`**，缺省时它按"当前包"过滤 —— 本仓根 crate 是 `synapse-rust`
# （只有 `src/` 与 `scripts/`），于是报告从 **627 个文件砍到 18 个**（2026-10-28 实测，
# run 40621674176：`files=18, coverage=36.46%`），棘轮会把其余 590 个文件读成
# `GONE-NOT-BUILT`。空范围在这里按失败处理，不让它静默退化成"只有根 crate"。
COVERAGE_LEG_FAILURES=0
LEG_ARGS=()

run_leg() {
    local label="$1" output="$2" arg inserted=0 test_rc=0
    shift 2
    log "$label"

    # `--` 之前是**报告范围**，之后是要执行的命令。
    local report_args=()
    while [ "$#" -gt 0 ] && [ "$1" != "--" ]; do
        report_args+=("$1")
        shift
    done
    if [ "$#" -gt 0 ]; then
        shift
    fi
    if [ "${#report_args[@]}" -eq 0 ]; then
        log "ERROR: $label 缺报告范围（-p <crate>）。缺省时 report 只覆盖当前包，"
        log "       会把报告砍成根 crate 的文件（实测 627 → 18 个）。"
        COVERAGE_LEG_FAILURES=$((COVERAGE_LEG_FAILURES + 1))
        return 0
    fi

    # `--no-report` / `--no-fail-fast` 是 **cargo** 的参数，必须落在测试侧的
    # `--` 之前；直接追加到末尾的话（步骤 2 带 `-- --skip …`）会被当成测试 harness
    # 参数，libtest 不认识 ⇒ 整条腿报 "unexpected argument"。
    LEG_ARGS=()
    for arg in "$@"; do
        if [ "$inserted" -eq 0 ] && [ "$arg" = "--" ]; then
            LEG_ARGS+=("--no-report" "--no-fail-fast")
            inserted=1
        fi
        LEG_ARGS+=("$arg")
    done
    if [ "$inserted" -eq 0 ]; then
        LEG_ARGS+=("--no-report" "--no-fail-fast")
    fi

    "${LEG_ARGS[@]}" || test_rc=$?

    if ! cargo llvm-cov report "${report_args[@]}" --lcov --output-path "$output"; then
        log "ERROR: $label 的报告生成失败（profraw 没写出来？编译阶段就挂了？）"
        COVERAGE_LEG_FAILURES=$((COVERAGE_LEG_FAILURES + 1))
        return 0
    fi
    if [ "$test_rc" -ne 0 ]; then
        log "WARNING: $label 有测试失败（rc=${test_rc}）；覆盖率数据仍已落盘，红由本步骤的退出码保留"
        COVERAGE_LEG_FAILURES=$((COVERAGE_LEG_FAILURES + 1))
    fi
}

run_leg "步骤 1/3: synapse-storage 单独跑（单线程，db_tests 直连 public）" "$STORAGE_LCOV" \
    -p synapse-storage -- \
    env RUST_TEST_THREADS=1 cargo llvm-cov -p synapse-storage \
    --features "$STORAGE_FEATURES" --lib

run_leg "步骤 2/3: 其余 crate + 集成测试（--exclude synapse-storage）" "$REST_LCOV" \
    "${REST_REPORT_ARGS[@]}" -- \
    env RUST_TEST_THREADS="$TEST_THREADS" cargo llvm-cov --workspace --exclude synapse-storage \
    --features "$REST_FEATURES" \
    -- --skip ledger_export_tests

log "步骤 3/3: 合并 lcov"
# 一条腿没产出 lcov 是**已发生过的现实**（run 40637601681：rsproxy 返回 504，
# storage 腿在下载依赖阶段就死了，profraw 一行没写）。旧实现在这里无条件调
# `merge_lcov.py`，于是 FileNotFoundError traceback 把**整轮**数据一起抹掉 ——
# 明明 `rest.lcov` 已经在磁盘上（覆盖 600+ 文件），结果 coverage/lcov.info 不存在，
# 棘轮跳过，覆盖率车道再次"跑了但没产出"。
#
# 取舍口径：**能合并就合并，缺腿要大声说**。少一条腿的报告仍然可用（缺的文件会被
# 棘轮读成 GONE-NOT-BUILT 并保留旧地板，不会把 0% 写进基线），而"零文件"比"少一条腿"
# 危险得多 —— 后者会被下游误读成"这些文件覆盖率掉了"。
existing_legs=()
if [ -f "$STORAGE_LCOV" ]; then
    existing_legs+=("$STORAGE_LCOV")
else
    log "WARNING: storage 腿没有产出 lcov（见上一条 ERROR）"
fi
if [ -f "$REST_LCOV" ]; then
    existing_legs+=("$REST_LCOV")
else
    log "WARNING: rest 腿没有产出 lcov（见上一条 ERROR）"
fi

if [ "${#existing_legs[@]}" -eq 0 ]; then
    log "ERROR: 两条腿都没有产出 lcov —— 不写空报告（空的 coverage/lcov.info 会让棘轮把"
    log "       608 个文件全部读成 GONE-NOT-BUILT，比没有报告更难归因）"
    exit 1
fi
if [ "${#existing_legs[@]}" -eq 1 ]; then
    log "WARNING: 只有 1 条腿有数据，做降级合并 —— 缺失腿覆盖的文件本轮不参与棘轮比较"
fi
python3 scripts/merge_lcov.py "${existing_legs[@]}" -o "$OUTPUT_DIR/lcov.info"

# 兜底清理累积的隔离 schema（若已改用 schema pool 复用则是 no-op）。
if [ "${SKIP_CLEANUP:-0}" != "1" ] && [ -f scripts/cleanup_test_schemas.sh ]; then
    log "兜底清理累积的隔离 schema"
    bash scripts/cleanup_test_schemas.sh
fi

log "覆盖率报告已写入 $OUTPUT_DIR/lcov.info"
log "棘轮：python3 scripts/check_file_coverage.py --report $OUTPUT_DIR/lcov.info \\"
log "        --baseline scripts/ci/coverage_baseline.json --global-floor 40 --new-file-floor 50 \\"
log "        --core-files scripts/ci/core_file_coverage_prefixes.txt --core-threshold 70 \\"
log "        --non-unit-coverable scripts/ci/non_unit_coverable_prefixes.txt"

if [ "$COVERAGE_LEG_FAILURES" -ne 0 ]; then
    log "ERROR: ${COVERAGE_LEG_FAILURES} 条腿有失败。报告已生成（供下载与定性），但本步骤按失败退出。"
    exit 1
fi
