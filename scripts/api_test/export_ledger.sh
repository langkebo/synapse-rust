#!/bin/bash
# =============================================================================
# 导出 Synapse-Rust 最新路由清单（RouteLedger → JSON）
#
# 使用与 Docker 镜像一致的 features 编译并运行 synapse_ledger_export，
# 输出全部 (method, path) 路由声明。测试执行器依赖该清单自动遍历所有路由。
#
# 用法：
#   ./export_ledger.sh [--profile=default|oidc|worker|saml|all] [--output=FILE]
# =============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
PROFILE="default"
OUTPUT=""

# 位置参数形式的 profile（`export_ledger.sh oidc`）。⚠️ 必须先判 `--` 前缀：
# 旧版直接写 `PROFILE="${1:-default}"`，于是 `--output=X` 被当成 profile，
# 最终以 `--profile=--output=X` 调用导出器而失败。
if [ $# -gt 0 ] && [ "${1#--}" = "$1" ]; then
    PROFILE="$1"
fi

# 简单解析 --key=value
for arg in "$@"; do
    case "$arg" in
        --profile=*) PROFILE="${arg#*=}" ;;
        --output=*) OUTPUT="${arg#*=}" ;;
        *)
            echo "未知参数: $arg" >&2
            exit 2
            ;;
    esac
done

if [ -z "$OUTPUT" ]; then
    OUTPUT="$SCRIPT_DIR/reports/ledger_${PROFILE}.json"
fi
mkdir -p "$(dirname "$OUTPUT")"

# ⚠️ `server` 特性已于 H-6 从根 crate 移除（零 `#[cfg(feature = "server")]` 门控、纯死标志），
# 继续传它会让 cargo 直接报「does not contain this feature」。该 bug 使本脚本自 H-6 起恒失败，
# `ledger.json` 因此冻结在 2026-08-12（见 `docs/audit/GATE_INTEGRITY_SWEEP_2026-09-19.md` E8）。
FEATURES="core-private-chat,widgets,external-services,voice-extended,cas-sso,saml-sso,friends"

echo "[ledger] profile=$PROFILE features=$FEATURES"
echo "[ledger] 编译并导出（首次较慢，之后增量秒级）..."

cd "$PROJECT_ROOT"
cargo run --quiet --no-default-features \
    --features "$FEATURES" \
    --bin synapse_ledger_export \
    -- "--profile=$PROFILE" "--output=$OUTPUT"

echo "[ledger] 完成: $OUTPUT"
echo "[ledger] 条目数: $(python3 -c "import json;print(json.load(open('$OUTPUT'))['entry_count'])")"
