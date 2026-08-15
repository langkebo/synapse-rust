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
PROFILE="${1:-default}"
OUTPUT=""

# 简单解析 --key=value
for arg in "$@"; do
  case "$arg" in
    --profile=*) PROFILE="${arg#*=}" ;;
    --output=*)  OUTPUT="${arg#*=}" ;;
    *) echo "未知参数: $arg" >&2; exit 2 ;;
  esac
done

if [ -z "$OUTPUT" ]; then
  OUTPUT="$SCRIPT_DIR/reports/ledger_${PROFILE}.json"
fi
mkdir -p "$(dirname "$OUTPUT")"

FEATURES="server,core-private-chat,widgets,external-services,voice-extended,cas-sso,saml-sso,friends"

echo "[ledger] profile=$PROFILE features=$FEATURES"
echo "[ledger] 编译并导出（首次较慢，之后增量秒级）..."

cd "$PROJECT_ROOT"
cargo run --quiet --no-default-features \
  --features "$FEATURES" \
  --bin synapse_ledger_export \
  -- "--profile=$PROFILE" "--output=$OUTPUT"

echo "[ledger] 完成: $OUTPUT"
echo "[ledger] 条目数: $(python3 -c "import json;print(json.load(open('$OUTPUT'))['entry_count'])")"
