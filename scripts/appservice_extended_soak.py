#!/usr/bin/env python3
import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request


SUMMARY_KEYS = [
    "total_services",
    "scheduler_available_services",
    "services_in_backoff",
    "services_capacity_limited",
    "services_with_pending_transactions",
    "total_pending_events",
    "total_pending_transactions",
    "total_success_count",
    "total_failure_count",
    "total_backoff_count",
    "total_capacity_limited_count",
    "total_in_flight_count",
]

FAIL_ON_CHOICES = ("never", "warning", "failure")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Run extended live appservice soak scenarios."
    )
    parser.add_argument(
        "scenario",
        choices=["continuous-ingress", "mixed-backoff", "recovery"],
        help="Scenario to execute.",
    )
    parser.add_argument(
        "--base-url", default=os.environ.get("BASE_URL", "http://localhost:8008")
    )
    parser.add_argument(
        "--prometheus-url",
        default=os.environ.get("PROMETHEUS_URL", "http://localhost:9090/metrics"),
    )
    parser.add_argument(
        "--token-file",
        default=os.environ.get("ADMIN_TOKEN_FILE", "/tmp/admin_token.txt"),
    )
    parser.add_argument("--output", help="Optional path to write the JSON result.")
    parser.add_argument(
        "--duration",
        type=int,
        default=60,
        help="Loop duration in seconds for sustained scenarios.",
    )
    parser.add_argument(
        "--recovery-wait",
        type=int,
        default=15,
        help="Seconds to wait after recovery injections before reading final state.",
    )
    parser.add_argument(
        "--fail-on",
        choices=FAIL_ON_CHOICES,
        default="never",
        help="Exit non-zero when the evaluated scenario status reaches the selected severity.",
    )
    return parser.parse_args()


def load_token(token_file: str) -> str:
    if "ADMIN_TOKEN" in os.environ:
        return os.environ["ADMIN_TOKEN"].strip()
    try:
        with open(token_file, "r", encoding="utf-8") as handle:
            return handle.read().strip()
    except OSError as exc:
        raise SystemExit(
            f"failed to read admin token from {token_file}: {exc}"
        ) from exc


class AdminClient:
    def __init__(self, base_url: str, token: str, prometheus_url: str) -> None:
        self.base_url = base_url.rstrip("/")
        self.token = token
        self.prometheus_url = prometheus_url

    def _request(
        self,
        path: str,
        method: str = "GET",
        payload: dict | None = None,
        retries: int = 3,
        retry_delay: float = 1.0,
    ):
        headers = {"Authorization": f"Bearer {self.token}"}
        data = None
        if payload is not None:
            headers["Content-Type"] = "application/json"
            data = json.dumps(payload).encode("utf-8")
        last_error: Exception | None = None
        for attempt in range(retries):
            req = urllib.request.Request(
                f"{self.base_url}{path}",
                headers=headers,
                data=data,
                method=method,
            )
            try:
                with urllib.request.urlopen(req, timeout=15) as response:
                    body = response.read()
                    if not body:
                        return None
                    return json.loads(body.decode("utf-8"))
            except (urllib.error.HTTPError, urllib.error.URLError) as exc:
                last_error = exc
                if attempt + 1 >= retries:
                    raise
                time.sleep(retry_delay)
        raise (
            last_error
            if last_error is not None
            else RuntimeError("request failed without an error")
        )

    def post_event(self, as_id: str, room_id: str, body: str) -> None:
        payload = {
            "room_id": room_id,
            "event_type": "m.room.message",
            "sender": "@stress:localhost",
            "content": {"msgtype": "m.text", "body": body},
        }
        try:
            self._request(
                f"/_synapse/admin/v1/appservices/{as_id}/events",
                method="POST",
                payload=payload,
            )
        except urllib.error.URLError:
            # Keep long-running soak loops moving if the local instance hiccups.
            return

    def statistics(self) -> list[dict]:
        data = self._request("/_synapse/admin/v1/appservices/statistics")
        return data if isinstance(data, list) else []

    def telemetry(self) -> dict:
        data = self._request("/_synapse/admin/v1/telemetry/metrics")
        if isinstance(data, dict):
            return data.get("appservice_scheduler", {})
        return {}

    def prometheus(self) -> str:
        request = urllib.request.Request(self.prometheus_url, method="GET")
        with urllib.request.urlopen(request, timeout=15) as response:
            return response.read().decode("utf-8")


def get_service(stats: list[dict], as_id: str) -> dict:
    for item in stats:
        if item.get("as_id") == as_id:
            return item
    return {}


def print_json(payload: dict) -> None:
    print(json.dumps(payload, indent=2, sort_keys=True))


def parse_metric_value(raw: str) -> int | float:
    value = float(raw)
    if value.is_integer():
        return int(value)
    return value


def aggregate_statistics_summary(stats: list[dict]) -> dict:
    return {
        "total_services": len(stats),
        "scheduler_available_services": sum(
            1 for item in stats if item.get("scheduler", {}).get("available") is True
        ),
        "services_in_backoff": sum(
            1
            for item in stats
            if item.get("scheduler", {}).get("transaction_state") == "retry_backoff"
        ),
        "services_capacity_limited": sum(
            1
            for item in stats
            if item.get("scheduler", {}).get("last_result") == "capacity_limited"
            or item.get("scheduler", {}).get("transaction_state") == "capacity_limited"
        ),
        "services_with_pending_transactions": sum(
            1
            for item in stats
            if (
                item.get("pending_transaction_count", 0)
                or item.get("scheduler", {}).get("pending_transaction_count", 0)
            )
            > 0
        ),
        "total_pending_events": sum(
            item.get("pending_event_count", 0) for item in stats
        ),
        "total_pending_transactions": sum(
            item.get("pending_transaction_count", 0) for item in stats
        ),
        "total_success_count": sum(
            item.get("scheduler", {}).get("total_success_count", 0) for item in stats
        ),
        "total_failure_count": sum(
            item.get("scheduler", {}).get("total_failure_count", 0) for item in stats
        ),
        "total_backoff_count": sum(
            item.get("scheduler", {}).get("total_backoff_count", 0) for item in stats
        ),
        "total_capacity_limited_count": sum(
            item.get("scheduler", {}).get("total_capacity_limited_count", 0)
            for item in stats
        ),
        "total_in_flight_count": sum(
            item.get("scheduler", {}).get("total_in_flight_count", 0) for item in stats
        ),
    }


def normalize_telemetry_summary(telemetry: dict) -> dict:
    return {key: telemetry.get(key, 0) for key in SUMMARY_KEYS}


def build_prometheus_summary(metrics_text: str) -> dict:
    metric_map: dict[str, int | float] = {}
    for line in metrics_text.splitlines():
        if not line or line.startswith("#"):
            continue
        parts = line.split()
        if len(parts) != 2:
            continue
        metric_map[parts[0]] = parse_metric_value(parts[1])

    metric_names = {
        "total_services": "synapse_appservice_scheduler_total_services",
        "scheduler_available_services": "synapse_appservice_scheduler_available_services",
        "services_in_backoff": "synapse_appservice_scheduler_backoff_services",
        "services_capacity_limited": "synapse_appservice_scheduler_capacity_limited_services",
        "services_with_pending_transactions": "synapse_appservice_scheduler_services_with_pending_transactions",
        "total_pending_events": "synapse_appservice_scheduler_pending_events",
        "total_pending_transactions": "synapse_appservice_scheduler_pending_transactions",
        "total_success_count": "synapse_appservice_scheduler_success_count",
        "total_failure_count": "synapse_appservice_scheduler_failure_count",
        "total_backoff_count": "synapse_appservice_scheduler_backoff_count",
        "total_capacity_limited_count": "synapse_appservice_scheduler_capacity_limited_count",
        "total_in_flight_count": "synapse_appservice_scheduler_in_flight_count",
    }
    return {
        key: metric_map.get(metric_name, 0) for key, metric_name in metric_names.items()
    }


def compare_outlets(statistics: dict, telemetry: dict, prometheus: dict) -> dict:
    comparisons = []
    mismatched_keys = []
    max_abs_delta = 0.0
    for key in SUMMARY_KEYS:
        stat_value = statistics.get(key, 0)
        telemetry_value = telemetry.get(key, 0)
        prometheus_value = prometheus.get(key, 0)
        consistent = stat_value == telemetry_value == prometheus_value
        if not consistent:
            mismatched_keys.append(key)
        values = [float(stat_value), float(telemetry_value), float(prometheus_value)]
        max_abs_delta = max(max_abs_delta, max(values) - min(values))
        comparisons.append(
            {
                "key": key,
                "statistics": stat_value,
                "telemetry": telemetry_value,
                "prometheus": prometheus_value,
                "consistent": consistent,
            }
        )
    return {
        "consistent": not mismatched_keys,
        "mismatched_keys": mismatched_keys,
        "max_abs_delta": int(max_abs_delta)
        if max_abs_delta.is_integer()
        else max_abs_delta,
        "comparisons": comparisons,
    }


def collect_outlet_snapshot(client: AdminClient) -> dict:
    stats = client.statistics()
    telemetry = client.telemetry()
    prometheus = client.prometheus()
    statistics_summary = aggregate_statistics_summary(stats)
    telemetry_summary = normalize_telemetry_summary(telemetry)
    prometheus_summary = build_prometheus_summary(prometheus)
    consistency = compare_outlets(
        statistics_summary, telemetry_summary, prometheus_summary
    )
    return {
        "generated_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
        "raw_statistics": stats,
        "outlets": {
            "statistics": statistics_summary,
            "telemetry": telemetry_summary,
            "prometheus": prometheus_summary,
        },
        "consistency": consistency,
    }


def classify_scenario_result(result: dict) -> tuple[str, list[str]]:
    scenario = result["scenario"]
    final_snapshot = result["final"]
    consistency = final_snapshot["consistency"]
    stats = final_snapshot["outlets"]["statistics"]
    reasons: list[str] = []

    if not consistency["consistent"]:
        reasons.append(
            f"三出口关键聚合不一致: {', '.join(consistency['mismatched_keys']) or 'unknown'}"
        )
        return "失败", reasons

    if scenario == "recovery":
        unrecovered = [
            service["as_id"]
            for service in result["scenario_metrics"]["services"]
            if service.get("pending_event_count", 0) > 0
            or service.get("pending_transaction_count", 0) > 0
            or service.get("transaction_state") not in (None, "", "idle")
        ]
        if unrecovered:
            reasons.append(
                f"恢复窗口结束后仍有服务未回到 idle: {', '.join(unrecovered)}"
            )
            return "预警", reasons
        return "通过", reasons

    if scenario == "continuous-ingress":
        injected = result["scenario_metrics"]["injected"]
        pending = result["scenario_metrics"]["pending_event_count"]
        if pending >= injected:
            reasons.append(f"积压未下降: pending={pending}, injected={injected}")
            return "失败", reasons
        if stats["services_capacity_limited"] > 0 or pending > 50:
            reasons.append(
                f"持续写入结束后仍有较高积压或限流: pending={pending}, capacity_limited={stats['services_capacity_limited']}"
            )
            return "预警", reasons
        return "通过", reasons

    if scenario == "event-only":
        success = result["scenario_metrics"]["success"]
        capacity_limited = result["scenario_metrics"]["capacity_limited"]
        if success <= 0:
            reasons.append("未观察到成功投递")
            return "失败", reasons
        if capacity_limited > 0:
            reasons.append(f"基础事件场景已触发容量限流: {capacity_limited}")
            return "预警", reasons
        return "通过", reasons

    if scenario == "transaction-only":
        retry_services = result["scenario_metrics"]["retry_services"]
        pending_after = result["scenario_metrics"]["pending_after"]
        if retry_services <= 0:
            reasons.append("未观察到 retry_backoff 服务")
            return "预警", reasons
        if pending_after <= result["scenario_metrics"]["pending_before"]:
            reasons.append("transaction-only 场景未观察到明确积压抬升")
            return "预警", reasons
        return "通过", reasons

    if scenario == "mixed":
        success = result["scenario_metrics"]["success"]
        pending_events = result["scenario_metrics"]["pending_events"]
        if success <= 0:
            reasons.append("mixed 场景未观察到成功推进")
            return "失败", reasons
        if pending_events > 100:
            reasons.append(f"mixed 场景结束时 pending_events 偏高: {pending_events}")
            return "预警", reasons
        return "通过", reasons

    if scenario == "mixed-backoff":
        healthy_success = result["scenario_metrics"]["healthy"]["total_success_count"]
        unhealthy_backoff = result["scenario_metrics"]["unhealthy"][
            "total_backoff_count"
        ]
        pending_events = stats["total_pending_events"]
        if healthy_success <= 0:
            reasons.append("健康 AS 未观察到成功推进")
            return "失败", reasons
        if unhealthy_backoff <= 0:
            reasons.append("失败 AS 未呈现可解释的 backoff 计数")
            return "预警", reasons
        if pending_events > 100:
            reasons.append(f"长时间窗口下 pending_events 仍偏高: {pending_events}")
            return "预警", reasons
        return "通过", reasons

    if scenario == "super-event-heavy":
        light_success = result["scenario_metrics"]["light_success"]
        if light_success <= 0:
            reasons.append("轻量服务未获得 dispatch，疑似出现饿死")
            return "失败", reasons
        if stats["services_capacity_limited"] > 0:
            reasons.append(
                f"极端场景命中 capacity_limited: {stats['services_capacity_limited']}"
            )
            return "预警", reasons
        return "通过", reasons

    return "通过", reasons


def exit_code_for_status(status: str, fail_on: str) -> int:
    if fail_on == "never":
        return 0
    if fail_on == "warning":
        return 1 if status in {"预警", "失败"} else 0
    if fail_on == "failure":
        return 1 if status == "失败" else 0
    raise ValueError(f"unsupported fail_on policy: {fail_on}")


def emit_result(payload: dict, output_path: str | None) -> None:
    if output_path:
        output_dir = os.path.dirname(output_path)
        if output_dir:
            os.makedirs(output_dir, exist_ok=True)
        with open(output_path, "w", encoding="utf-8") as handle:
            json.dump(payload, handle, indent=2, sort_keys=True)
            handle.write("\n")
    print_json(payload)


def run_continuous_ingress(client: AdminClient, duration: int) -> dict:
    preflight = collect_outlet_snapshot(client)
    client.post_event("stress_as_1", "!stress_room_1:localhost", "probe")
    end = time.time() + duration
    injected = 0
    while time.time() < end:
        client.post_event("stress_as_1", "!continuous:localhost", "continuous")
        injected += 1
        time.sleep(0.05)

    final_snapshot = collect_outlet_snapshot(client)
    service = get_service(final_snapshot["raw_statistics"], "stress_as_1")
    return {
        "scenario": "continuous-ingress",
        "duration_seconds": duration,
        "scenario_metrics": {
            "injected": injected,
            "pending_event_count": service.get("pending_event_count", 0),
            "pending_transaction_count": service.get("pending_transaction_count", 0),
            "total_success_count": service.get("scheduler", {}).get(
                "total_success_count", 0
            ),
            "last_result": service.get("scheduler", {}).get("last_result"),
        },
        "preflight": preflight,
        "final": final_snapshot,
    }


def run_mixed_backoff(client: AdminClient, duration: int) -> dict:
    preflight = collect_outlet_snapshot(client)
    client.post_event("stress_as_1", "!stress_room_1:localhost", "probe")
    end = time.time() + duration
    injected = 0
    while time.time() < end:
        client.post_event("stress_as_1", "!healthy:localhost", "healthy")
        client.post_event("stress_as_2", "!unhealthy:localhost", "unhealthy")
        injected += 2
        time.sleep(0.2)

    final_snapshot = collect_outlet_snapshot(client)
    healthy = get_service(final_snapshot["raw_statistics"], "stress_as_1")
    unhealthy = get_service(final_snapshot["raw_statistics"], "stress_as_2")
    return {
        "scenario": "mixed-backoff",
        "duration_seconds": duration,
        "scenario_metrics": {
            "injected": injected,
            "healthy": {
                "pending_event_count": healthy.get("pending_event_count", 0),
                "pending_transaction_count": healthy.get(
                    "pending_transaction_count", 0
                ),
                "total_success_count": healthy.get("scheduler", {}).get(
                    "total_success_count", 0
                ),
                "last_result": healthy.get("scheduler", {}).get("last_result"),
            },
            "unhealthy": {
                "pending_event_count": unhealthy.get("pending_event_count", 0),
                "pending_transaction_count": unhealthy.get(
                    "pending_transaction_count", 0
                ),
                "transaction_state": unhealthy.get("scheduler", {}).get(
                    "transaction_state"
                ),
                "last_result": unhealthy.get("scheduler", {}).get("last_result"),
                "total_backoff_count": unhealthy.get("scheduler", {}).get(
                    "total_backoff_count", 0
                ),
                "total_failure_count": unhealthy.get("scheduler", {}).get(
                    "total_failure_count", 0
                ),
            },
        },
        "preflight": preflight,
        "final": final_snapshot,
    }


def run_recovery(client: AdminClient, recovery_wait: int) -> dict:
    preflight = collect_outlet_snapshot(client)
    client.post_event("stress_as_1", "!stress_room_1:localhost", "probe")
    for index in range(1, 6):
        as_id = f"stress_as_{index}"
        room_id = f"!recovery_{index}:localhost"
        for event_index in range(20):
            client.post_event(as_id, room_id, f"recovery-{index}-{event_index}")

    time.sleep(recovery_wait)

    final_snapshot = collect_outlet_snapshot(client)
    services = []
    for index in range(1, 6):
        service = get_service(final_snapshot["raw_statistics"], f"stress_as_{index}")
        services.append(
            {
                "as_id": service.get("as_id"),
                "pending_event_count": service.get("pending_event_count", 0),
                "pending_transaction_count": service.get(
                    "pending_transaction_count", 0
                ),
                "transaction_state": service.get("scheduler", {}).get(
                    "transaction_state"
                ),
                "last_result": service.get("scheduler", {}).get("last_result"),
                "total_backoff_count": service.get("scheduler", {}).get(
                    "total_backoff_count", 0
                ),
                "total_failure_count": service.get("scheduler", {}).get(
                    "total_failure_count", 0
                ),
                "total_success_count": service.get("scheduler", {}).get(
                    "total_success_count", 0
                ),
            }
        )

    return {
        "scenario": "recovery",
        "recovery_wait_seconds": recovery_wait,
        "scenario_metrics": {
            "services": services,
        },
        "preflight": preflight,
        "final": final_snapshot,
    }


def main() -> int:
    args = parse_args()
    token = load_token(args.token_file)
    client = AdminClient(args.base_url, token, args.prometheus_url)

    if args.scenario == "continuous-ingress":
        result = run_continuous_ingress(client, args.duration)
    elif args.scenario == "mixed-backoff":
        result = run_mixed_backoff(client, args.duration)
    elif args.scenario == "recovery":
        result = run_recovery(client, args.recovery_wait)
    else:
        raise SystemExit(f"unsupported scenario: {args.scenario}")
    status, reasons = classify_scenario_result(result)
    exit_code = exit_code_for_status(status, args.fail_on)
    result["evaluation"] = {
        "status": status,
        "reasons": reasons,
        "fail_on": args.fail_on,
        "exit_code": exit_code,
    }
    emit_result(result, args.output)
    return exit_code


if __name__ == "__main__":
    sys.exit(main())
