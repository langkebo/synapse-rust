#!/usr/bin/env python3
import argparse
import json
from datetime import datetime, timezone
from pathlib import Path


THRESHOLDS = {
    "smoke": {
        "login_duration": 500,
        "create_room_duration": 800,
        "send_message_duration": 600,
        "sync_duration": 1000,
        "room_summary_duration": 500,
        "errors": 0.01,
    },
    "baseline": {
        "login_duration": 500,
        "create_room_duration": 800,
        "send_message_duration": 600,
        "sync_duration": 1000,
        "room_summary_duration": 500,
        "errors": 0.01,
    },
    "stress": {
        "login_duration": 600,
        "create_room_duration": 1000,
        "send_message_duration": 800,
        "sync_duration": 1200,
        "room_summary_duration": 600,
        "errors": 0.02,
    },
    "peak": {
        "login_duration": 600,
        "create_room_duration": 1000,
        "send_message_duration": 800,
        "sync_duration": 1200,
        "room_summary_duration": 600,
        "errors": 0.02,
    },
    "friends": {
        "friend_search_duration": 400,
        "friend_list_duration": 300,
        "errors": 0.02,
    },
    "soak": {
        "login_duration": 700,
        "create_room_duration": 1200,
        "send_message_duration": 900,
        "sync_duration": 1500,
        "room_summary_duration": 800,
        "errors": 0.03,
    },
}

DISPLAY_NAMES = {
    "login_duration": "Login P95",
    "create_room_duration": "CreateRoom P95",
    "send_message_duration": "SendMessage P95",
    "sync_duration": "Sync P95",
    "room_summary_duration": "RoomSummary P95",
    "friend_search_duration": "FriendSearch P95",
    "friend_list_duration": "FriendList P95",
    "errors": "Error Rate",
}


def metric_value(metrics: dict, metric_name: str) -> float | None:
    """Read one metric's aggregate out of a k6 `--summary-export` document.

    Two shapes exist and both must be supported, because this gate is the only
    consumer and it had **never actually run** before 2026-09-21 (CI run
    `35571855133` only reaches it on a `workflow_dispatch` with `run_k6=true`):

      * k6 >= 0.47 (measured locally with the same v0.47.0 image CI installs) writes
        the aggregates **flat**:
            "login_duration": {"med":3,"avg":75.1,"p(90)":8,"p(95)":12,"thresholds":{…}}
            "errors":         {"passes":840,"fails":0,"value":1,"thresholds":{…}}
      * older/summary-handler output nests them under `values`:
            {"values": {"p(95)": 12, "rate": 0.0}}

    Reading only the nested form made every metric report `missing`, so the
    guardrail rendered "Actual: missing / Status: FAIL" for all seven rows even
    against a perfectly healthy target — a gate that can only ever fail.
    """
    metric = metrics.get(metric_name)
    if not isinstance(metric, dict):
        return None
    nested = metric.get("values")
    nested = nested if isinstance(nested, dict) else {}
    if metric_name == "errors":
        for candidate in (metric.get("value"), nested.get("rate"), metric.get("rate")):
            if candidate is not None:
                return candidate
        return None
    for candidate in (metric.get("p(95)"), nested.get("p(95)")):
        if candidate is not None:
            return candidate
    return None


def metric_unit(metric_name: str) -> str:
    return "%" if metric_name == "errors" else "ms"


def metric_actual_display(metric_name: str, value: float | None) -> str:
    if value is None:
        return "missing"
    if metric_name == "errors":
        return f"{value * 100:.2f}%"
    return f"{value:.2f}ms"


def metric_threshold_display(metric_name: str, threshold: float) -> str:
    if metric_name == "errors":
        return f"< {threshold * 100:.2f}%"
    return f"< {threshold:.0f}ms"


def evaluate_scenario(name: str, data: dict) -> dict:
    metrics = data.get("metrics", {})
    scenario_result = {"scenario": name, "passed": True, "metrics": []}
    for metric_name, threshold in THRESHOLDS[name].items():
        actual = metric_value(metrics, metric_name)
        passed = actual is not None and actual < threshold
        scenario_result["passed"] = scenario_result["passed"] and passed
        scenario_result["metrics"].append(
            {
                "name": metric_name,
                "display_name": DISPLAY_NAMES[metric_name],
                "threshold": threshold,
                "actual": actual,
                "passed": passed,
            }
        )
    return scenario_result


def render_markdown(results: list[dict], base_url: str) -> str:
    lines = [
        "# Performance Guardrail Report",
        "",
        f"- Base URL: {base_url}",
        f"- Generated At: {datetime.now(timezone.utc).isoformat()}",
        "",
    ]
    overall_passed = all(result["passed"] for result in results)
    lines.append(f"- Overall Status: {'PASS' if overall_passed else 'FAIL'}")
    lines.append("")

    for result in results:
        lines.extend(
            [
                f"## {result['scenario'].title()}",
                "",
                "| Metric | Target | Actual | Status |",
                "| --- | --- | --- | --- |",
            ]
        )
        for metric in result["metrics"]:
            lines.append(
                f"| {metric['display_name']} | "
                f"{metric_threshold_display(metric['name'], metric['threshold'])} | "
                f"{metric_actual_display(metric['name'], metric['actual'])} | "
                f"{'PASS' if metric['passed'] else 'FAIL'} |"
            )
        lines.append("")

    return "\n".join(lines) + "\n"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", required=True)
    parser.add_argument("--base-url", default="http://localhost:28008")
    parser.add_argument("--fail-on-breach", action="store_true")
    parser.add_argument(
        "--scenarios",
        nargs="+",
        default=["smoke", "baseline", "stress", "peak"],
    )
    args = parser.parse_args()

    results_dir = Path(args.results_dir)
    summary = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "base_url": args.base_url,
        "results": [],
    }

    for scenario in args.scenarios:
        if scenario not in THRESHOLDS:
            raise SystemExit(f"unsupported scenario: {scenario}")
        result_file = results_dir / f"{scenario}_results.json"
        if not result_file.exists():
            continue
        with result_file.open("r", encoding="utf-8") as handle:
            summary["results"].append(evaluate_scenario(scenario, json.load(handle)))

    if not summary["results"]:
        raise SystemExit("no k6 summary files were found")

    summary["overall_passed"] = all(result["passed"] for result in summary["results"])
    markdown = render_markdown(summary["results"], args.base_url)

    (results_dir / "performance_guardrail_report.md").write_text(
        markdown, encoding="utf-8"
    )
    (results_dir / "performance_guardrail_summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2),
        encoding="utf-8",
    )

    print(markdown)

    if args.fail_on_breach and not summary["overall_passed"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
