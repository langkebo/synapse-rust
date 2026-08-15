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
#   TEST_THREADS=6 bash scripts/run_local_coverage.sh
#
# 前置：本地测试库需已初始化 public schema（255 表），见
#   scripts/init_test_public_schema.sh（或手动 psql 跳过 .undo.sql 跑 migrations/*.sql）。
# 如需 TEST_THREADS >= 6 跑集成测试，先调高锁表上限（避免 clone 并发 "out of
# shared memory"）：bash scripts/tune_test_db.sh（一次性，ALTER SYSTEM 持久化）。

set -euo pipefail

cd "$(dirname "$0")/.."

export DATABASE_URL="${DATABASE_URL:-postgresql://synapse:synapse@localhost:15432/synapse_test}"
export TEST_DATABASE_URL="${TEST_DATABASE_URL:-$DATABASE_URL}"
TEST_THREADS="${TEST_THREADS:-4}"
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
# storage 用与 rest 步骤一致的扩展 feature 集（test-utils + 扩展 feature），
# 使 feature 门控的 db_tests（server_notification/saml/cas/beacon 等）也被编译并
# 测量，而非停留在 test_mocks 间接编译的 0% 状态。
RUST_TEST_THREADS=1 \
    cargo llvm-cov -p synapse-storage \
    --features "test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,cas-sso,saml-sso" \
    --lib \
    --lcov --output-path "$STORAGE_LCOV"

echo
echo "==> 步骤 2/2: 其余 crate + 集成测试（--exclude synapse-storage，特定 feature 集与快照一致）"
# 用特定 feature 集（CI 覆盖率 job 的 minimal features + cas-sso/saml-sso），
# 而非 --all-features：--all-features 会启用 openclaw/friends/widgets 等死代码
# feature，编译更多测试，在本地多核/有限内存下触发 prepare_shared_test_pool
# 并发 clone schema 失败（646 个）与 OOM（exit 137）。route_ledger 快照已用
# 同一 feature 集重新生成，故此处必须与快照保持一致。
# ledger_export_tests 的 fixture 反映导出二进制的默认 build（无 voice-extended 等），
# 在多 feature 下 live 会多出 voice/voip 等路由导致不匹配，跳过它。
REST_FEATURES="test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,cas-sso,saml-sso"
RUST_TEST_THREADS="$TEST_THREADS" \
    cargo llvm-cov --workspace --exclude synapse-storage \
    --features "$REST_FEATURES" \
    --lcov --output-path "$REST_LCOV" \
    -- --skip ledger_export_tests

echo
echo "==> 合并 lcov"
python3 scripts/merge_lcov.py "$STORAGE_LCOV" "$REST_LCOV" -o "$OUTPUT_DIR/lcov.info"

# 集成测试通过 prepare_shared_test_pool 每次 clone 一个 111 张表的隔离 schema，用完
# 不 DROP，会累积到数千个（catalog 膨胀拖慢后续 clone）。跑完后兜底清理（若已改用
# schema pool 复用则此步骤是 no-op）。设 SKIP_CLEANUP=1 可跳过。
if [ "${SKIP_CLEANUP:-0}" != "1" ]; then
    echo
    echo "==> 兜底清理累积的隔离 schema"
    if [ -f scripts/cleanup_test_schemas.sh ]; then
        bash scripts/cleanup_test_schemas.sh
    else
        echo "    （cleanup_test_schemas.sh 不存在，跳过）"
    fi
fi

echo
echo "==> 覆盖率报告已写入 $OUTPUT_DIR/lcov.info"
echo "    分析：python3 scripts/analyze_coverage.py"
