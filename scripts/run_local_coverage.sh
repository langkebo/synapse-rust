#!/usr/bin/env bash
# 方案 B：分两步生成完整 lcov.info（替代 tarpaulin）。
#
# 背景（三个根因，缺一不可）：
#   1. tarpaulin 0.35.2 的 `--implicit-test-threads` 有 bug —— 即使设置，仍向测试
#      argv 注入 `--test-threads=#CPU`，本地多核机器会以 8+ 并发跑 DB 集成测试，
#      触发共享 public schema 的数据竞争（RUST_TEST_THREADS 被 argv 覆盖失效）。
#   2. tarpaulin 的 LLVM 覆盖引擎在测试非零退出时不返回任何覆盖数据（官方文档明说），
#      只要有一个 flaky 测试失败，lcov 就全空。
#   3. storage 的 51 个 db_tests 文件直连 TEST_DATABASE_URL 的 public schema，而根
#      crate 集成测试会 DROP public（init_template_schema 清理残留）。两者在同一
#      workspace 跑时执行顺序不定，必然一方失败。
#
# cargo llvm-cov（已装 0.8.x + llvm-tools 组件）不受问题 1/2 影响：RUST_TEST_THREADS
# 真正生效；即使有测试失败，llvm-profdata 汇总 .profraw 后 `llvm-cov export` 仍能导出
# lcov（但为拿到完整数据仍应尽量全绿）。
#
# 步骤：
#   1. storage 单独跑（单线程，db_tests 直连 public —— 需预先初始化 public schema）
#   2. 其余 crate + 集成测试（--exclude synapse-storage，--all-features 与快照一致）
#   3. scripts/merge_lcov.py 合并两个 lcov
#
# 用法：
#   bash scripts/run_local_coverage.sh
#   TEST_THREADS=2 bash scripts/run_local_coverage.sh
#
# 前置：本地测试库需已初始化 public schema（255 表），见
#   scripts/init_test_public_schema.sh（或手动 psql 跳过 .undo.sql 跑 migrations/*.sql）。

set -euo pipefail

cd "$(dirname "$0")/.."

export DATABASE_URL="${DATABASE_URL:-postgresql://synapse:synapse@localhost:15432/synapse_test}"
export TEST_DATABASE_URL="${TEST_DATABASE_URL:-$DATABASE_URL}"
TEST_THREADS="${TEST_THREADS:-2}"
OUTPUT_DIR="coverage"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

STORAGE_LCOV="$TMP_DIR/storage.lcov"
REST_LCOV="$TMP_DIR/rest.lcov"

mkdir -p "$OUTPUT_DIR"

echo "==> 方案 B：分两步生成覆盖率"
echo "    DATABASE_URL  = $DATABASE_URL"
echo "    TEST_THREADS  = $TEST_THREADS"

echo
echo "==> 步骤 1/2: synapse-storage 单独跑（db_tests 直连 public，单线程避免并发竞争）"
RUST_TEST_THREADS=1 \
    cargo llvm-cov -p synapse-storage \
    --features test-utils \
    --lib \
    --lcov --output-path "$STORAGE_LCOV"

echo
echo "==> 步骤 2/2: 其余 crate + 集成测试（--exclude synapse-storage，--all-features 与快照一致）"
# ledger_export_tests 的 fixture 反映导出二进制的默认 build（无 voice-extended 等），
# 在 --all-features 下 live 会多出 voice/voip 等 feature 路由导致不匹配。跳过它以保持
# 其余测试（快照/同步等需要 --all-features）能全绿。
RUST_TEST_THREADS="$TEST_THREADS" \
    cargo llvm-cov --workspace --exclude synapse-storage \
    --all-features \
    --lcov --output-path "$REST_LCOV" \
    -- --skip ledger_export_tests

echo
echo "==> 合并 lcov"
python3 scripts/merge_lcov.py "$STORAGE_LCOV" "$REST_LCOV" -o "$OUTPUT_DIR/lcov.info"

echo
echo "==> 覆盖率报告已写入 $OUTPUT_DIR/lcov.info"
echo "    分析：python3 scripts/analyze_coverage.py"
