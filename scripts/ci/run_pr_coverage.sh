#!/usr/bin/env bash
# PR 车道的覆盖率采集：**只跑指定 crate 的 lib 腿**，产出 `coverage/lcov.info`。
#
# 为什么不直接复用 `run_coverage.sh`：那条是主车道的全量口径（storage 单跑 + 其余
# workspace，67 分钟、含 1424 条集成测试），PR 上等不起。这里把范围收窄到
# "改动落到的那几个 crate"，用 `cargo llvm-cov -p <crate> --lib` 换时间。
#
# ⚠️ 口径差异必须写在脸上：**单腿读数是下界**，与 `scripts/ci/coverage_baseline.json`
# 的合并基线不可比（实测 `src/bin/healthcheck.rs` 合并 10.91% / 单腿 0.00%）。
# 所以下游 `check_changed_line_coverage.py` 只判"变更行有没有被跑到"，不比对基线。
#
# 用法：
#   bash scripts/ci/run_pr_coverage.sh synapse-common synapse-web
#   OUTPUT_DIR=coverage PR_COVERAGE_FEATURES=test-utils bash scripts/ci/run_pr_coverage.sh synapse-storage
#
# 前置：
#   * `cargo-llvm-cov` + `llvm-tools-preview`；
#   * 测试库已播种（`bash scripts/ci/prepare_test_db.sh`）—— **只有** 参数里含
#     `synapse-storage` 时才需要，脚本会先自检，缺了就给出可直接粘贴的修法。
#
# 退出码：0 = 每条腿都成功；1 = 有腿失败（数据仍然写出，红由退出码保留）；
#         2 = 参数/前置不可用（fail-closed）。
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$REPO_ROOT"

OUTPUT_DIR="${OUTPUT_DIR:-coverage}"
# 与主车道同一套"给测试用的 feature"口径（`test-utils` 门控着 mock 与契约测试）。
# 只在该 crate 真的定义了这个 feature 时才加 —— `synapse-cache` / `synapse-test-utils`
# 没有它，硬传会让 cargo 直接报错（实测于 2026-10-28）。
PR_COVERAGE_FEATURES="${PR_COVERAGE_FEATURES:-test-utils}"

log() { printf '[pr-coverage] %s\n' "$*"; }

if [ "$#" -eq 0 ]; then
    log "ERROR: 至少要给一个 crate 名（由 scripts/ci/affected_crates.py 产出）"
    log "       例：bash scripts/ci/run_pr_coverage.sh synapse-common"
    exit 2
fi

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
    log "ERROR: cargo-llvm-cov 缺失。安装："
    log "  rustup component add llvm-tools-preview"
    log "  cargo install cargo-llvm-cov --locked --version 0.8.7"
    exit 2
fi

# `synapse-storage` 的 lib 测试里有一批直连 **public** schema 的 db_tests：没有播种好的
# 模板库时它们会以"表不存在"失败，而那种红看起来像代码问题。前置自检放在这里，
# 让失败信息直接指向修法。
for crate in "$@"; do
    if [ "$crate" != "synapse-storage" ]; then
        continue
    fi
    template="${TEST_DB_TEMPLATE_SCHEMA:-test_template_ci}"
    url="${TEST_DATABASE_URL:-${DATABASE_URL:-postgresql://synapse:synapse@localhost:5432/synapse_test}}"
    tables="$(psql "$url" -tAc "SELECT count(*) FROM information_schema.tables WHERE table_schema='${template}'" 2>/dev/null || echo 0)"
    case "$tables" in
        '' | *[!0-9]*) tables=0 ;;
    esac
    if [ "$tables" -lt 100 ]; then
        log "ERROR: synapse-storage 需要播种好的测试库，但 '${template}' 只有 ${tables} 张表。"
        log "  修法：DATABASE_URL=$url bash scripts/ci/prepare_test_db.sh"
        exit 2
    fi
    log "synapse-storage 前置 OK（${template}: ${tables} 张表）"
done

# `cargo llvm-cov report` 不区分 profraw 是哪一轮写的：上一轮被 kill 留下的数据会
# 混进本轮 ⇒ 覆盖率虚高 ⇒ 假绿。与主车道同一条理由、同一个做法。
LLVM_COV_TARGET_DIR="${CARGO_TARGET_DIR:-target}/llvm-cov-target"
if [ -d "$LLVM_COV_TARGET_DIR" ]; then
    stale="$(find "$LLVM_COV_TARGET_DIR" -name '*.profraw' -print -delete 2>/dev/null | wc -l | tr -d ' ')"
    log "清掉上一轮残留的 profraw: ${stale} 个"
fi

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
mkdir -p "$OUTPUT_DIR"

FAILURES=0
LEGS=()

for crate in "$@"; do
    # 让 python 决定要不要加 feature：它是唯一读 `cargo metadata` 的地方。
    # 拼成一个字符串并按需做**有意**的词分割（`--features test-utils` 是两个词）。
    # 不用空数组 + `"${arr[@]}"`：macOS 自带 bash 3.2 在 `set -u` 下会报 unbound variable。
    features_arg="$(python3 scripts/ci/affected_crates.py --features-flag "$crate" || true)"

    leg="$TMP_DIR/$crate.lcov"
    log "crate ${crate}：跑 lib 腿（feature: ${features_arg:-none}）"

    # 先跑测试（失败也留下 profraw），再从 profraw 生成报告 —— 与主车道同构：
    # 绑成一次调用时，测试一红就什么都不写（这正是主车道"跑了但没数据"的第一代成因）。
    rc=0
    # shellcheck disable=SC2086  # $features_arg 故意不加引号（见上）
    if [ "$crate" = "synapse-storage" ]; then
        # db_tests 直连 public schema，与其它测试并发时会相互 DROP/竞争。
        env RUST_TEST_THREADS=1 cargo llvm-cov -p "$crate" $features_arg --lib \
            --no-report --no-fail-fast || rc=$?
    else
        cargo llvm-cov -p "$crate" $features_arg --lib \
            --no-report --no-fail-fast || rc=$?
    fi

    if ! cargo llvm-cov report -p "$crate" --lcov --output-path "$leg"; then
        log "ERROR: ${crate} 的报告生成失败（编译挂了 / 没写出 profraw）"
        FAILURES=$((FAILURES + 1))
        continue
    fi
    if [ "$rc" -ne 0 ]; then
        log "WARNING: ${crate} 有测试失败（rc=${rc}）；覆盖率数据已落盘，红由退出码保留"
        FAILURES=$((FAILURES + 1))
    fi
    LEGS+=("$leg")
done

if [ "${#LEGS[@]}" -eq 0 ]; then
    log "ERROR: 没有任何一条腿产出 lcov —— 不写空报告"
    log "       （空的 coverage/lcov.info 会让下游把『没测到』读成『0% 覆盖』）"
    exit 1
fi
if [ "${#LEGS[@]}" -lt "$#" ]; then
    log "WARNING: ${#LEGS[@]}/$# 条腿有数据，做降级合并 —— 缺腿覆盖的文件本轮不参与判定"
fi

python3 scripts/merge_lcov.py "${LEGS[@]}" -o "$OUTPUT_DIR/lcov.info"
log "报告写入 $OUTPUT_DIR/lcov.info"

if [ "$FAILURES" -ne 0 ]; then
    log "ERROR: ${FAILURES} 条腿有失败（数据已写出，供下载与归因）"
    exit 1
fi
log "全部 ${#LEGS[@]} 条腿成功"
