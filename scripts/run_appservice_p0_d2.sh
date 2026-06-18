#!/usr/bin/env bash

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
    cat <<'EOF'
用法:
  ADMIN_TOKEN=... bash scripts/run_appservice_p0_d2.sh [baseline|after-change|custom-label]

可选环境变量:
  BASE_URL                      管理接口基址，默认 http://127.0.0.1:8008
  PROMETHEUS_URL                Prometheus 指标地址，默认 http://127.0.0.1:9090/metrics
  APPSERVICE_D2_DATE            归档日期，默认当天 YYYY-MM-DD
  APPSERVICE_D2_LABEL           归档标签；若提供位置参数则优先使用位置参数
  APPSERVICE_D2_FAIL_ON         门禁阈值，默认 warning
  APPSERVICE_D2_OUTPUT_ROOT     归档根目录，默认 artifacts/appservice
  APPSERVICE_D2_RESOURCE_SUMMARY
                                资源摘要文本；未提供时写入占位内容
  APPSERVICE_D2_RESOURCE_FILE   资源摘要文件路径；若提供则优先读取文件内容
  APPSERVICE_D2_NEXT_PLAN       写入 decision 模板的下一步计划

输出:
  <output_root>/<date>/<label>/
    daily-report.json
    daily-report.md
    continuous-ingress.json
    mixed-backoff.json
    recovery.json
    resource-summary.txt
    run-metadata.json
  <output_root>/<date>/decision.md
EOF
}

if [ "${1:-}" = "--help" ] || [ "${1:-}" = "-h" ]; then
    usage
    exit 0
fi

if [ -z "${ADMIN_TOKEN:-}" ]; then
    echo "ADMIN_TOKEN is required" >&2
    usage >&2
    exit 1
fi

BASE_URL="${BASE_URL:-http://127.0.0.1:8008}"
PROMETHEUS_URL="${PROMETHEUS_URL:-http://127.0.0.1:9090/metrics}"
RUN_DATE="${APPSERVICE_D2_DATE:-$(date +%F)}"
RUN_LABEL="${1:-${APPSERVICE_D2_LABEL:-baseline}}"
FAIL_ON="${APPSERVICE_D2_FAIL_ON:-warning}"
OUTPUT_ROOT="${APPSERVICE_D2_OUTPUT_ROOT:-$ROOT_DIR/artifacts/appservice}"
NEXT_PLAN="${APPSERVICE_D2_NEXT_PLAN:-根据日报结论决定保持默认值、继续观察或进入参数评审}"

DATE_DIR="$OUTPUT_ROOT/$RUN_DATE"
RUN_DIR="$DATE_DIR/$RUN_LABEL"
RESOURCE_FILE="$RUN_DIR/resource-summary.txt"
METADATA_FILE="$RUN_DIR/run-metadata.json"
DECISION_FILE="$DATE_DIR/decision.md"

mkdir -p "$RUN_DIR"

resolve_resource_summary() {
    if [ -n "${APPSERVICE_D2_RESOURCE_FILE:-}" ]; then
        cat "${APPSERVICE_D2_RESOURCE_FILE}"
        return 0
    fi

    if [ -n "${APPSERVICE_D2_RESOURCE_SUMMARY:-}" ]; then
        printf '%s\n' "${APPSERVICE_D2_RESOURCE_SUMMARY}"
        return 0
    fi

    printf '%s\n' "待补 CPU/RSS/连接池/慢查询/bridge 外部依赖摘要"
}

write_decision_template_if_missing() {
    if [ -f "$DECISION_FILE" ]; then
        return 0
    fi

    cat >"$DECISION_FILE" <<EOF
# AppService D2 决策记录

- 日期：$RUN_DATE
- 基线目录：\`$DATE_DIR/baseline\`
- 变更后目录：\`$DATE_DIR/after-change\`
- 当前推荐下一步：$NEXT_PLAN

## 当前默认值

- \`MAX_SERVICES_PER_TICK\`：待补
- \`HIGH_PENDING_TRANSACTION_THRESHOLD\`：待补
- \`HIGH_PENDING_EVENT_THRESHOLD\`：待补

## 本轮结论

- 结论：待补
- 负责人：待补
- 观察窗口：待补

## 样本路径

- 基线日报：\`$DATE_DIR/baseline/daily-report.md\`
- 变更后日报：\`$DATE_DIR/after-change/daily-report.md\`

## 回退条件

- 日报门禁退化为 \`预警\` 或 \`失败\`
- 资源占用较基线恶化超过 \`20%\`
- 健康 AS 成功推进下降
EOF
}

RESOURCE_SUMMARY="$(resolve_resource_summary)"
printf '%s\n' "$RESOURCE_SUMMARY" >"$RESOURCE_FILE"

echo "[appservice-d2] output dir: $RUN_DIR"
echo "[appservice-d2] running D2 gate (fail-on=$FAIL_ON)"

ADMIN_TOKEN="$ADMIN_TOKEN" \
BASE_URL="$BASE_URL" \
PROMETHEUS_URL="$PROMETHEUS_URL" \
python3 "$ROOT_DIR/scripts/appservice_daily_report.py" \
    --day D2 \
    --fail-on "$FAIL_ON" \
    --output-dir "$RUN_DIR" \
    --resource-summary "$RESOURCE_SUMMARY"

python3 - "$METADATA_FILE" <<'PY' "$RUN_DATE" "$RUN_LABEL" "$FAIL_ON" "$BASE_URL" "$PROMETHEUS_URL" "$RUN_DIR"
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

metadata_path = Path(sys.argv[1])
payload = {
    "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "day": "D2",
    "date": sys.argv[2],
    "label": sys.argv[3],
    "fail_on": sys.argv[4],
    "base_url": sys.argv[5],
    "prometheus_url": sys.argv[6],
    "output_dir": sys.argv[7],
}
metadata_path.write_text(json.dumps(payload, ensure_ascii=False, indent=2) + "\n")
PY

write_decision_template_if_missing

echo "[appservice-d2] completed"
echo "[appservice-d2] report: $RUN_DIR/daily-report.md"
echo "[appservice-d2] metadata: $METADATA_FILE"
echo "[appservice-d2] decision template: $DECISION_FILE"
