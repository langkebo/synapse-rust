#!/usr/bin/env python3
"""
Performance Guardrail - k6 Smoke Test Results Evaluator

Evaluates k6 performance test results against defined thresholds.
Supports both k6 >= 0.47 (flat metrics) and older (nested values) formats.

Usage:
    python3 guardrail.py --results-dir ./results --scenarios smoke baseline --fail-on-breach
"""

import argparse
import json
import sys
from datetime import datetime, timezone
from pathlib import Path

# ============================================================================
# Threshold Definitions
# ============================================================================

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


# ============================================================================
# Metric Value Extraction
# ============================================================================


def metric_value(metrics: dict, metric_name: str) -> float | None:
    """Read one metric's aggregate out of a k6 `--summary-export` document.

    Supports two k6 output formats:
      * k6 >= 0.47 (flat):     {"login_duration": {"med":3,"p(95)":12,...}}
      * Older (nested values): {"login_duration": {"values": {"p(95)": 12}}}
    """
    metric = metrics.get(metric_name)
    if not isinstance(metric, dict):
        return None

    # Try flat format first (k6 >= 0.47)
    if metric_name == "errors":
        for candidate in (metric.get("value"), metric.get("rate")):
            if candidate is not None:
                return candidate
        nested = metric.get("values", {})
        if isinstance(nested, dict):
            for candidate in (nested.get("rate"),):
                if candidate is not None:
                    return candidate
        return None

    for candidate in (metric.get("p(95)"), metric.get("p(90)"), metric.get("median")):
        if candidate is not None:
            return candidate

    # Try nested format (older k6)
    nested = metric.get("values", {})
    if isinstance(nested, dict):
        for candidate in (nested.get("p(95)"), nested.get("p(90)")):
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


# ============================================================================
# Scenario Evaluation
# ============================================================================


def evaluate_scenario(name: str, data: dict) -> dict:
    """Evaluate a single scenario against its thresholds."""
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


# ============================================================================
# Report Rendering
# ============================================================================


def render_markdown(results: list[dict], base_url: str) -> str:
    """Render evaluation results as Markdown table."""
    lines = [
        "# Performance Guardrail Report",
        "",
        f"- Base URL: {base_url}",
        f"- Generated At: {datetime.now(timezone.utc).isoformat()}",
        "",
    ]

    overall_passed = all(result["passed"] for result in results)
    status_icon = "✅" if overall_passed else "❌"
    lines.append(
        f"- Overall Status: {status_icon} {'PASS' if overall_passed else 'FAIL'}"
    )
    lines.append("")

    for result in results:
        icon = "✅" if result["passed"] else "❌"
        lines.extend(
            [
                f"## {icon} {result['scenario'].title()}",
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
                f"{'✅' if metric['passed'] else '❌'} |"
            )
        lines.append("")

    return "\n".join(lines) + "\n"


def render_console_report(results: list[dict], base_url: str) -> str:
    """Render a concise console-friendly report."""
    lines = [
        "=" * 80,
        "PERFORMANCE GUARDRAIL REPORT",
        "=" * 80,
        f"Target: {base_url}",
        f"Time: {datetime.now(timezone.utc).isoformat()}",
        "-" * 80,
    ]

    overall_passed = all(result["passed"] for result in results)

    for result in results:
        icon = "✅" if result["passed"] else "❌"
        lines.append(f"\n{icon} {result['scenario'].title()}:")
        for metric in result["metrics"]:
            status = "PASS" if metric["passed"] else "FAIL"
            actual = metric_actual_display(metric["name"], metric["actual"])
            target = metric_threshold_display(metric["name"], metric["threshold"])
            lines.append(
                f"  [{status}] {metric['display_name']}: {actual} (target: {target})"
            )

    lines.append("")
    lines.append("=" * 80)
    lines.append(f"OVERALL: {'✅ PASS' if overall_passed else '❌ FAIL'}")
    lines.append("=" * 80)

    return "\n".join(lines)


# ============================================================================
# Main Entry Point
# ============================================================================


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Evaluate k6 smoke test results against performance thresholds."
    )
    parser.add_argument(
        "--results-dir",
        required=True,
        help="Directory containing k6 summary JSON files",
    )
    parser.add_argument(
        "--base-url", default="http://localhost:28008", help="Target server URL"
    )
    parser.add_argument(
        "--fail-on-breach",
        action="store_true",
        help="Exit with code 1 if any threshold breached",
    )
    parser.add_argument(
        "--scenarios",
        nargs="+",
        default=["smoke", "baseline", "stress", "peak"],
        help="Scenarios to evaluate (default: smoke baseline stress peak)",
    )
    args = parser.parse_args()

    results_dir = Path(args.results_dir)
    summary = {
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "base_url": args.base_url,
        "results": [],
    }

    missing_files = []

    for scenario in args.scenarios:
        if scenario not in THRESHOLDS:
            print(
                f"⚠️  Warning: unknown scenario '{scenario}', skipping", file=sys.stderr
            )
            continue

        result_file = results_dir / f"{scenario}_results.json"
        if not result_file.exists():
            missing_files.append(scenario)
            continue

        try:
            with result_file.open("r", encoding="utf-8") as handle:
                data = json.load(handle)
            summary["results"].append(evaluate_scenario(scenario, data))
        except (json.JSONDecodeError, KeyError) as e:
            print(f"⚠️  Warning: failed to parse {result_file}: {e}", file=sys.stderr)
            continue

    # Handle missing files
    if missing_files and not summary["results"]:
        print(
            f"❌ Error: no k6 summary files found for scenarios: {', '.join(missing_files)}",
            file=sys.stderr,
        )
        print(
            f"   Expected files: {', '.join(f'{s}_results.json' for s in missing_files)}",
            file=sys.stderr,
        )
        print(f"   Directory: {results_dir}", file=sys.stderr)
        print(f"\n💡 Hint: Run the test first, e.g.:", file=sys.stderr)
        print(
            f"        ./run_tests.sh {missing_files[0] if missing_files else 'smoke'}",
            file=sys.stderr,
        )
        return 2

    if missing_files:
        print(
            f"⚠️  Warning: missing result files for: {', '.join(missing_files)}",
            file=sys.stderr,
        )

    if not summary["results"]:
        print("❌ Error: no valid k6 summary files found", file=sys.stderr)
        return 2

    overall_passed = all(result["passed"] for result in summary["results"])

    # Generate reports
    markdown = render_markdown(summary["results"], args.base_url)
    console_report = render_console_report(summary["results"], args.base_url)

    # Write files
    (results_dir / "performance_guardrail_report.md").write_text(
        markdown, encoding="utf-8"
    )
    (results_dir / "performance_guardrail_summary.json").write_text(
        json.dumps(summary, ensure_ascii=False, indent=2), encoding="utf-8"
    )

    # Print console report
    print(console_report)

    # Return code
    if args.fail_on_breach and not overall_passed:
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
