#!/usr/bin/env python3
import argparse
import json
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

import appservice_extended_soak as soak


DAY_SCENARIOS = {
    "D1": ("event-only", "transaction-only", "mixed"),
    "D2": ("mixed-backoff", "recovery", "continuous-ingress"),
    "D3": ("super-event-heavy",),
}

SCENARIO_CHOICES = [
    "event-only",
    "transaction-only",
    "mixed",
    "mixed-backoff",
    "recovery",
    "continuous-ingress",
    "super-event-heavy",
]


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run appservice long-window soak scenarios and generate a daily JSON/Markdown report."
    )
    parser.add_argument(
        "--day",
        default="D2",
        choices=["D1", "D2", "D3"],
        help="Daily report phase label.",
    )
    parser.add_argument(
        "--scenarios",
        nargs="+",
        choices=SCENARIO_CHOICES,
        help="Scenarios to execute in order. Defaults follow the selected day.",
    )
    parser.add_argument("--base-url", default="http://localhost:8008")
    parser.add_argument("--prometheus-url", default="http://localhost:9090/metrics")
    parser.add_argument("--token-file", default="/tmp/admin_token.txt")
    parser.add_argument(
        "--output-dir",
        required=True,
        help="Directory to write scenario outputs and the daily report.",
    )
    parser.add_argument("--event-only-duration", type=int, default=60)
    parser.add_argument("--transaction-only-duration", type=int, default=60)
    parser.add_argument("--mixed-duration", type=int, default=60)
    parser.add_argument("--mixed-backoff-duration", type=int, default=45)
    parser.add_argument("--continuous-ingress-duration", type=int, default=90)
    parser.add_argument("--recovery-wait", type=int, default=20)
    parser.add_argument("--super-event-heavy-duration", type=int, default=60)
    parser.add_argument(
        "--resource-summary",
        default="待人工补充 CPU/RSS/连接池/慢查询/主链路外溢情况",
        help="Manual resource/stability summary to embed in the report.",
    )
    parser.add_argument(
        "--today-goal",
        help="Daily goal line shown in the report. Defaults follow the selected day.",
    )
    parser.add_argument(
        "--next-plan",
        help="Next-step line shown in the report. Defaults follow the selected day.",
    )
    parser.add_argument(
        "--fail-on",
        choices=soak.FAIL_ON_CHOICES,
        default="never",
        help="Exit non-zero when the daily conclusion reaches the selected severity.",
    )
    args = parser.parse_args()
    if args.scenarios is None:
        args.scenarios = list(DAY_SCENARIOS[args.day])
    if args.today_goal is None:
        args.today_goal = default_today_goal(args.day)
    if args.next_plan is None:
        args.next_plan = default_next_plan(args.day)
    return args


def default_today_goal(day: str) -> str:
    mapping = {
        "D1": "验证默认阈值是否具备可测性和可解释性，拿到基础场景可信基线",
        "D2": "验证恢复路径、长时间 backlog 公平性，以及失败 AS 是否拖慢健康 AS",
        "D3": "验证极端 event-heavy 场景下默认阈值是否仍可作为生产默认值",
    }
    return mapping[day]


def default_next_plan(day: str) -> str:
    mapping = {
        "D1": "若三场均可信则进入 D2；若出现观测链异常或主链路外溢则先复跑/修环境",
        "D2": "根据 recovery 与 mixed-backoff 结果决定进入 D3，或先做故障复盘与补采样",
        "D3": "根据极端场景结果决定保持默认值、继续观察，或进入参数评审",
    }
    return mapping[day]


def load_client(args: argparse.Namespace) -> soak.AdminClient:
    token = soak.load_token(args.token_file)
    return soak.AdminClient(args.base_url, token, args.prometheus_url)


def scenario_display_name(name: str) -> str:
    mapping = {
        "event-only": "Baseline event-only",
        "transaction-only": "Transaction-only",
        "mixed": "Mixed steady-state",
        "mixed-backoff": "Mixed + backoff",
        "recovery": "Recovery burst",
        "continuous-ingress": "Continuous ingress",
        "super-event-heavy": "Super event-heavy",
    }
    return mapping[name]


def run_named_scenario(
    client: soak.AdminClient, args: argparse.Namespace, name: str
) -> dict:
    if name in {"event-only", "transaction-only", "mixed", "super-event-heavy"}:
        return run_stress_scenario(args, name)
    if name == "mixed-backoff":
        return soak.run_mixed_backoff(client, args.mixed_backoff_duration)
    if name == "recovery":
        return soak.run_recovery(client, args.recovery_wait)
    if name == "continuous-ingress":
        return soak.run_continuous_ingress(client, args.continuous_ingress_duration)
    raise ValueError(f"unsupported scenario: {name}")


def scenario_duration(args: argparse.Namespace, name: str) -> int:
    mapping = {
        "event-only": args.event_only_duration,
        "transaction-only": args.transaction_only_duration,
        "mixed": args.mixed_duration,
        "mixed-backoff": args.mixed_backoff_duration,
        "continuous-ingress": args.continuous_ingress_duration,
        "super-event-heavy": args.super_event_heavy_duration,
    }
    return mapping[name]


def run_stress_scenario(args: argparse.Namespace, name: str) -> dict:
    token = soak.load_token(args.token_file)
    results_dir = Path(args.output_dir) / f"_stress_{name}"
    results_dir.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.update(
        {
            "ADMIN_TOKEN": token,
            "BASE_URL": args.base_url,
            "PROMETHEUS_URL": args.prometheus_url,
            "RESULTS_DIR": str(results_dir),
        }
    )
    command = [
        "bash",
        str(Path(__file__).with_name("appservice_stress_test.sh")),
        name,
        str(scenario_duration(args, name)),
    ]
    subprocess.run(
        command,
        check=True,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )

    scenario_payload = json.loads(
        (results_dir / f"{name}.json").read_text(encoding="utf-8")
    )
    preflight_payload = json.loads(
        (results_dir / "outlet-consistency-preflight.json").read_text(encoding="utf-8")
    )
    postrun_payload = json.loads(
        (results_dir / "outlet-consistency-post-run.json").read_text(encoding="utf-8")
    )
    return {
        "scenario": name,
        "duration_seconds": scenario_payload["duration_seconds"],
        "scenario_metrics": scenario_payload["scenario_metrics"],
        "preflight": preflight_payload,
        "final": {
            "generated_at": scenario_payload["generated_at"],
            "outlets": scenario_payload["outlets"],
            "consistency": scenario_payload["consistency"],
        },
        "artifacts": {
            "stress_results_dir": str(results_dir),
        },
    }


def build_core_metric_line(result: dict, status: str) -> str:
    scenario = scenario_display_name(result["scenario"])
    stats = result["final"]["outlets"]["statistics"]
    consistency = result["final"]["consistency"]
    return (
        f"{scenario}: {status}；pending_events={stats['total_pending_events']}，"
        f"pending_txns={stats['total_pending_transactions']}，"
        f"backoff_services={stats['services_in_backoff']}，"
        f"capacity_limited={stats['services_capacity_limited']}，"
        f"max_delta={consistency['max_abs_delta']}"
    )


def build_service_sample_line(result: dict) -> str:
    scenario = result["scenario"]
    if scenario == "mixed-backoff":
        healthy = result["scenario_metrics"]["healthy"]
        unhealthy = result["scenario_metrics"]["unhealthy"]
        return (
            f"stress_as_1 success={healthy['total_success_count']} last={healthy['last_result']}; "
            f"stress_as_2 state={unhealthy['transaction_state']} backoff={unhealthy['total_backoff_count']}"
        )
    if scenario == "event-only":
        metrics = result["scenario_metrics"]
        return (
            f"event-only injected={metrics['events_injected']} "
            f"success={metrics['success']} capacity_limited={metrics['capacity_limited']}"
        )
    if scenario == "transaction-only":
        metrics = result["scenario_metrics"]
        return (
            f"transaction-only pending={metrics['pending_before']}->{metrics['pending_after']} "
            f"retry_services={metrics['retry_services']} backoff={metrics['backoff']}"
        )
    if scenario == "mixed":
        metrics = result["scenario_metrics"]
        return (
            f"mixed pending_events={metrics['pending_events']} "
            f"pending_txn={metrics['pending_txn']} success={metrics['success']}"
        )
    if scenario == "continuous-ingress":
        metrics = result["scenario_metrics"]
        return (
            f"stress_as_1 pending={metrics['pending_event_count']} "
            f"success={metrics['total_success_count']} last={metrics['last_result']}"
        )
    if scenario == "recovery":
        samples = []
        for service in result["scenario_metrics"]["services"][:3]:
            samples.append(
                f"{service['as_id']} state={service['transaction_state']} "
                f"pending={service['pending_event_count']}/{service['pending_transaction_count']}"
            )
        return "; ".join(samples)
    if scenario == "super-event-heavy":
        metrics = result["scenario_metrics"]
        return (
            f"heavy_success={metrics['heavy_success']} "
            f"light_success={metrics['light_success']}"
        )
    return scenario


def summarize_day(results: list[dict], args: argparse.Namespace) -> dict:
    scenario_summaries = []
    statuses: list[str] = []
    observability_states: list[bool] = []
    warnings: list[str] = []

    for result in results:
        status, reasons = soak.classify_scenario_result(result)
        statuses.append(status)
        observability_states.append(result["final"]["consistency"]["consistent"])
        scenario_summaries.append(
            {
                "scenario": result["scenario"],
                "display_name": scenario_display_name(result["scenario"]),
                "status": status,
                "reasons": reasons,
                "result_file": f"{result['scenario']}.json",
                "raw": result,
            }
        )
        if reasons:
            warnings.extend(
                f"{scenario_display_name(result['scenario'])}: {reason}"
                for reason in reasons
            )

    if all(observability_states):
        observability = "可信"
    elif any(observability_states):
        observability = "待确认"
    else:
        observability = "不可信"

    if "失败" in statuses or observability == "不可信":
        conclusion = "进入参数评审"
        light = "红灯"
    elif "预警" in statuses:
        conclusion = "继续观察"
        light = "黄灯"
    else:
        conclusion = "保持默认值"
        light = "绿灯"

    report_date = datetime.now(timezone.utc).astimezone().strftime("%Y-%m-%d")
    execution_result = "；".join(
        f"{summary['display_name']}: {summary['status']}"
        for summary in scenario_summaries
    )
    core_metrics = " | ".join(
        build_core_metric_line(summary["raw"], summary["status"])
        for summary in scenario_summaries
    )
    service_samples = " | ".join(
        build_service_sample_line(summary["raw"]) for summary in scenario_summaries
    )
    risk_and_blockers = (
        "；".join(warnings) if warnings else "无新增阻塞，保留观测样本继续累计"
    )
    report_exit_code = soak.exit_code_for_status(
        "失败"
        if conclusion == "进入参数评审"
        else "预警"
        if conclusion == "继续观察"
        else "通过",
        args.fail_on,
    )

    return {
        "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "day": args.day,
        "date_label": f"{args.day} / {report_date}",
        "today_goal": args.today_goal,
        "scenarios": [summary["display_name"] for summary in scenario_summaries],
        "execution_result": execution_result,
        "core_metrics_summary": core_metrics,
        "service_samples": service_samples,
        "resource_summary": args.resource_summary,
        "observability_conclusion": observability,
        "conclusion": conclusion,
        "daily_light": light,
        "risk_and_blockers": risk_and_blockers,
        "next_plan": args.next_plan,
        "gate": {
            "fail_on": args.fail_on,
            "exit_code": report_exit_code,
        },
        "scenario_summaries": scenario_summaries,
    }


def render_markdown(report: dict) -> str:
    scenario_lines = []
    for index, scenario in enumerate(report["scenarios"], start=1):
        scenario_lines.append(f"{index}. {scenario}")
    scenario_text = "  ".join(scenario_lines)

    samples = []
    for summary in report["scenario_summaries"]:
        reasons = "；".join(summary["reasons"]) if summary["reasons"] else "无"
        samples.append(
            f"| {summary['display_name']} | {summary['status']} | `{summary['result_file']}` | {reasons} |"
        )

    return "\n".join(
        [
            f"# AppService {report['day']} 压测日报",
            "",
            f"- 日期: {report['date_label']}",
            f"- 今日目标: {report['today_goal']}",
            f"- 今日场景: {scenario_text}",
            f"- 执行结果: {report['execution_result']}",
            f"- 核心指标摘要: {report['core_metrics_summary']}",
            f"- 单服务抽样: {report['service_samples']}",
            f"- 资源与稳定性: {report['resource_summary']}",
            f"- 观测链结论: {report['observability_conclusion']}",
            f"- 结论判断: {report['conclusion']}",
            f"- 日报结论档位: {report['daily_light']}",
            f"- 风险与阻塞: {report['risk_and_blockers']}",
            f"- 次日计划: {report['next_plan']}",
            "",
            "## 场景样本",
            "| 场景 | 状态 | 样本文件 | 备注 |",
            "|---|---|---|---|",
            *samples,
            "",
        ]
    )


def write_json(path: Path, payload: dict) -> None:
    with path.open("w", encoding="utf-8") as handle:
        json.dump(payload, handle, indent=2, ensure_ascii=False)
        handle.write("\n")


def write_text(path: Path, text: str) -> None:
    with path.open("w", encoding="utf-8") as handle:
        handle.write(text)


def main() -> int:
    args = parse_args()
    output_dir = Path(args.output_dir)
    output_dir.mkdir(parents=True, exist_ok=True)

    client = load_client(args)
    results: list[dict] = []
    for scenario in args.scenarios:
        result = run_named_scenario(client, args, scenario)
        results.append(result)
        write_json(output_dir / f"{scenario}.json", result)

    report = summarize_day(results, args)
    markdown = render_markdown(report)
    write_json(output_dir / "daily-report.json", report)
    write_text(output_dir / "daily-report.md", markdown)

    print(
        json.dumps(
            {
                "output_dir": str(output_dir),
                "report_json": "daily-report.json",
                "report_markdown": "daily-report.md",
                "conclusion": report["conclusion"],
                "fail_on": report["gate"]["fail_on"],
                "exit_code": report["gate"]["exit_code"],
            },
            ensure_ascii=False,
        )
    )
    return int(report["gate"]["exit_code"])


if __name__ == "__main__":
    sys.exit(main())
