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

DATABASE_URL="${DATABASE_URL:-postgresql://synapse:synapse@localhost:5432/synapse_test}"
TEST_DATABASE_URL="${TEST_DATABASE_URL:-$DATABASE_URL}"
TEST_DB_TEMPLATE_SCHEMA="${TEST_DB_TEMPLATE_SCHEMA:-test_template_ci}"
OUTPUT_DIR="${OUTPUT_DIR:-coverage}"
# 显式钉并发：CI 的 4 vCPU 与本地 4 线程因此等价，覆盖率数字才可跨环境比较。
# 大于 4 需要先调高锁表上限（见 scripts/tune_test_db.sh），否则会 `53200 out of shared memory`。
TEST_THREADS="${TEST_THREADS:-4}"

# storage 步**刻意**多带两个 feature（external-services / builtin-oidc）：它们门控
# synapse-storage / synapse-common 的代码，且已提交的 per-file 基线就是在这一口径下产出的。
STORAGE_FEATURES="test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,cas-sso,saml-sso,external-services,builtin-oidc"
# 其余 crate + 集成测试的 feature 集必须与 route_ledger 快照一致（见文件头注释）。
REST_FEATURES="test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,cas-sso,saml-sso"

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

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
STORAGE_LCOV="$TMP_DIR/storage.lcov"
REST_LCOV="$TMP_DIR/rest.lcov"
mkdir -p "$OUTPUT_DIR"

log "DATABASE_URL            = $DATABASE_URL"
log "TEST_DB_TEMPLATE_SCHEMA = $TEST_DB_TEMPLATE_SCHEMA (${template_tables} tables)"
log "TEST_THREADS            = $TEST_THREADS"
log "OUTPUT_DIR              = $OUTPUT_DIR"

log "步骤 1/3: synapse-storage 单独跑（单线程，db_tests 直连 public）"
RUST_TEST_THREADS=1 cargo llvm-cov -p synapse-storage \
    --features "$STORAGE_FEATURES" --lib \
    --lcov --output-path "$STORAGE_LCOV"

log "步骤 2/3: 其余 crate + 集成测试（--exclude synapse-storage）"
RUST_TEST_THREADS="$TEST_THREADS" cargo llvm-cov --workspace --exclude synapse-storage \
    --features "$REST_FEATURES" \
    --lcov --output-path "$REST_LCOV" \
    -- --skip ledger_export_tests

log "步骤 3/3: 合并 lcov"
python3 scripts/merge_lcov.py "$STORAGE_LCOV" "$REST_LCOV" -o "$OUTPUT_DIR/lcov.info"

# 兜底清理累积的隔离 schema（若已改用 schema pool 复用则是 no-op）。
if [ "${SKIP_CLEANUP:-0}" != "1" ] && [ -f scripts/cleanup_test_schemas.sh ]; then
    log "兜底清理累积的隔离 schema"
    bash scripts/cleanup_test_schemas.sh
fi

log "覆盖率报告已写入 $OUTPUT_DIR/lcov.info"
log "棘轮：python3 scripts/check_file_coverage.py --report $OUTPUT_DIR/lcov.info --format lcov \\"
log "        --baseline scripts/ci/coverage_baseline.json --global-floor 40 --new-file-floor 30 \\"
log "        --core-files scripts/ci/core_file_coverage_prefixes.txt --core-threshold 70 \\"
log "        --non-unit-coverable scripts/ci/non_unit_coverable_prefixes.txt"
