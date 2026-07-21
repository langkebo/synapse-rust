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
  <output_root>/<date>/decision.autofill.md
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
DECISION_AUTOFILL_FILE="$DATE_DIR/decision.autofill.md"

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

write_decision_autofill() {
    python3 - "$DATE_DIR" "$DECISION_AUTOFILL_FILE" "$RUN_DATE" "$NEXT_PLAN" <<'PY'
import json
import sys
from pathlib import Path

date_dir = Path(sys.argv[1])
output_file = Path(sys.argv[2])
run_date = sys.argv[3]
fallback_next_plan = sys.argv[4]


def load_report(label: str):
    path = date_dir / label / "daily-report.json"
    if not path.exists():
        return None
    return json.loads(path.read_text(encoding="utf-8"))


def scenario_line(report, display_name: str) -> str:
    for item in report.get("scenario_summaries", []):
        if item.get("display_name") == display_name:
            reasons = "；".join(item.get("reasons", [])) if item.get("reasons") else "无"
            return f"{item.get('status', '待补')}；{reasons}"
    return "待补"


def scenario_status(report, display_name: str) -> str:
    for item in report.get("scenario_summaries", []):
        if item.get("display_name") == display_name:
            return item.get("status", "待补")
    return "待补"


def report_present(label: str, report) -> str:
    return "是" if report and (date_dir / label / "daily-report.md").exists() else "否"


def raw_samples_present(label: str) -> str:
    required = ["continuous-ingress.json", "mixed-backoff.json", "recovery.json"]
    return "是" if all((date_dir / label / name).exists() for name in required) else "否"


baseline = load_report("baseline")
after_change = load_report("after-change")

primary = after_change or baseline or {}
conclusion = primary.get("conclusion", "待补")
resource_summary = primary.get("resource_summary", "待补")
observability = primary.get("observability_conclusion", "待补")
core_metrics = primary.get("core_metrics_summary", "待补")
risk_and_blockers = primary.get("risk_and_blockers", "待补")
service_samples = primary.get("service_samples", "待补")
next_plan = primary.get("next_plan", fallback_next_plan)

allow_next_round = "否" if conclusion == "进入参数评审" else "是"
change_flag = "是" if after_change else "否"

rank = {"保持默认值": 3, "继续观察": 2, "进入参数评审": 1}
if baseline and after_change:
    baseline_conclusion = baseline.get("conclusion", "待补")
    after_conclusion = after_change.get("conclusion", "待补")
    if rank.get(after_conclusion, 0) > rank.get(baseline_conclusion, 0):
        overall_change = "改善"
    elif rank.get(after_conclusion, 0) < rank.get(baseline_conclusion, 0):
        overall_change = "变差"
    else:
        overall_change = "持平"

    compare_lines = []
    for display_name in ("Continuous ingress", "Mixed + backoff", "Recovery burst"):
        base_status = scenario_status(baseline, display_name)
        after_status = scenario_status(after_change, display_name)
        if rank.get(after_status, 0) > rank.get(base_status, 0):
            trend = "改善"
        elif rank.get(after_status, 0) < rank.get(base_status, 0):
            trend = "变差"
        else:
            trend = "持平"
        compare_lines.append(
            f"- `{display_name}`：baseline={base_status}，after-change={after_status}，趋势={trend}"
        )

    compare_block = "\n".join(
        [
            "## baseline / after-change 对比",
            "",
            f"- baseline 结论：{baseline_conclusion}",
            f"- after-change 结论：{after_conclusion}",
            f"- 总体变化：{overall_change}",
            f"- baseline 风险与阻塞：{baseline.get('risk_and_blockers', '待补')}",
            f"- after-change 风险与阻塞：{after_change.get('risk_and_blockers', '待补')}",
            f"- baseline 核心指标摘要：{baseline.get('core_metrics_summary', '待补')}",
            f"- after-change 核心指标摘要：{after_change.get('core_metrics_summary', '待补')}",
            "",
            "### 场景逐项对比",
            "",
            *compare_lines,
            "",
        ]
    )
else:
    compare_block = "\n".join(
        [
            "## baseline / after-change 对比",
            "",
            "- 当前仅检测到单侧样本，待补另一侧结果后再生成自动对比摘要",
            "",
        ]
    )

content = f"""# AppService D2 决策记录（自动预填）

> 说明: 本文件由 `scripts/run_appservice_p0_d2.sh` 自动生成，用于给人工决策提供初稿。
> 人工可编辑版本建议写入: `decision.md`

- 日期：{run_date}
- 基线目录：`{date_dir / 'baseline'}`
- 变更后目录：`{date_dir / 'after-change'}`
- 当前推荐下一步：{next_plan}

## 当前默认值

- `MAX_SERVICES_PER_TICK`：待补
- `HIGH_PENDING_TRANSACTION_THRESHOLD`：待补
- `HIGH_PENDING_EVENT_THRESHOLD`：待补

## 本轮变更

- 是否改参数：{change_flag}
- 若改参数，本轮只改了哪个参数：待补
- 改动前值：待补
- 改动后值：待补
- 改动理由：待补

## 样本完整性检查

- `baseline` 已生成 `daily-report.md`：{report_present('baseline', baseline)}
- `baseline` 已生成 3 个 `D2` 场景原始样本：{raw_samples_present('baseline')}
- `after-change` 是否已生成：{"是" if after_change else "否"}
- 三出口一致性是否可信：{"是" if observability == "可信" else "否" if observability else "待补"}
- 资源摘要是否已补齐：{"否" if resource_summary.startswith("待补") else "是"}

## 关键观测

- `continuous-ingress`：{scenario_line(primary, 'Continuous ingress')}
- `mixed-backoff`：{scenario_line(primary, 'Mixed + backoff')}
- `recovery`：{scenario_line(primary, 'Recovery burst')}
- 核心指标摘要：{core_metrics}
- 单服务抽样：{service_samples}
- 资源与稳定性：{resource_summary}
- 风险与阻塞：{risk_and_blockers}

{compare_block}

## 结论

- 最终结论：{conclusion}
- 结论理由：待补
- 是否允许继续下一轮参数调整：{allow_next_round}

## 回退条件

- 日报门禁从 `通过` 退化为 `预警` 或 `失败`
- 资源占用较基线恶化超过 `20%`
- 健康 AS 成功推进下降
- 恢复窗口结束后仍无法回到 `idle`

## 下一步

- 下一步计划：{next_plan}
- 预计再次复测时间：待补
- 需要谁参与评审：待补
"""

output_file.write_text(content + "\n", encoding="utf-8")
PY
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
write_decision_autofill

echo "[appservice-d2] completed"
echo "[appservice-d2] report: $RUN_DIR/daily-report.md"
echo "[appservice-d2] metadata: $METADATA_FILE"
echo "[appservice-d2] decision template: $DECISION_FILE"
echo "[appservice-d2] decision autofill: $DECISION_AUTOFILL_FILE"
