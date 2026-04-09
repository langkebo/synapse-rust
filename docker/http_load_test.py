#!/usr/bin/env python3
import argparse
import http.client
import ssl
import threading
import time
import urllib.parse
import urllib.error
from dataclasses import dataclass


@dataclass
class Result:
    ok: int = 0
    fail: int = 0
    ok_2xx_3xx: int = 0
    http_4xx: int = 0
    http_429: int = 0
    http_5xx: int = 0
    timeout: int = 0
    conn_error: int = 0
    other_error: int = 0
    samples_ms: list[float] = None

    def __post_init__(self) -> None:
        if self.samples_ms is None:
            self.samples_ms = []


def _make_connection(parsed: urllib.parse.SplitResult, timeout: float):
    host = parsed.hostname or ""
    port = parsed.port
    if parsed.scheme == "https":
        ctx = ssl.create_default_context()
        return http.client.HTTPSConnection(host, port=port, timeout=timeout, context=ctx)
    return http.client.HTTPConnection(host, port=port, timeout=timeout)


def _request_once(conn, method: str, path: str, headers: dict[str, str]):
    conn.request(method, path, headers=headers)
    resp = conn.getresponse()
    resp.read()
    return resp.status


def _percentile(sorted_values: list[float], p: float) -> float | None:
    if not sorted_values:
        return None
    if p <= 0:
        return sorted_values[0]
    if p >= 100:
        return sorted_values[-1]
    k = (len(sorted_values) - 1) * (p / 100.0)
    f = int(k)
    c = min(f + 1, len(sorted_values) - 1)
    if f == c:
        return sorted_values[f]
    d = k - f
    return sorted_values[f] * (1.0 - d) + sorted_values[c] * d


def run_test(
    url: str,
    target_rps: int,
    duration_s: int,
    concurrency: int,
    timeout: float,
    max_samples: int,
    block_size: int,
    extra_headers: dict[str, str] | None = None,
) -> int:
    parsed = urllib.parse.urlsplit(url)
    if parsed.scheme not in ("http", "https"):
        raise SystemExit("only http/https are supported")
    path = parsed.path or "/"
    if parsed.query:
        path = f"{path}?{parsed.query}"

    start_mono = time.monotonic()
    end_mono = start_mono + float(duration_s)
    ticket_lock = threading.Lock()
    next_ticket = 0
    per_thread: list[Result] = []

    headers = {
        "Host": parsed.netloc,
        "User-Agent": "synapse-rust-http-load-test/2",
        "Accept": "application/json",
        "Connection": "keep-alive",
    }
    if extra_headers:
        headers.update(extra_headers)

    def thread_main() -> None:
        nonlocal next_ticket
        res = Result()
        conn = None
        ticket = 0
        local_samples = 0
        while True:
            with ticket_lock:
                ticket = next_ticket
                next_ticket += block_size
            for t in range(ticket, ticket + block_size):
                scheduled = start_mono + (t / max(1, target_rps))
                if scheduled >= end_mono:
                    per_thread.append(res)
                    if conn is not None:
                        try:
                            conn.close()
                        except Exception:
                            pass
                    return
                now = time.monotonic()
                if scheduled > now:
                    time.sleep(scheduled - now)

                if conn is None:
                    try:
                        conn = _make_connection(parsed, timeout)
                    except Exception:
                        conn = None
                        res.fail += 1
                        res.conn_error += 1
                        continue

                t0 = time.monotonic()
                try:
                    status = _request_once(conn, "GET", path, headers)
                    ms = (time.monotonic() - t0) * 1000.0
                    if local_samples < max_samples:
                        res.samples_ms.append(ms)
                        local_samples += 1
                    if 200 <= status < 400:
                        res.ok_2xx_3xx += 1
                        res.ok += 1
                    elif 400 <= status < 500:
                        res.http_4xx += 1
                        if status == 429:
                            res.http_429 += 1
                        res.ok += 1
                    else:
                        res.http_5xx += 1
                        res.fail += 1
                except TimeoutError:
                    res.fail += 1
                    res.timeout += 1
                    try:
                        conn.close()
                    except Exception:
                        pass
                    conn = None
                except (OSError, http.client.HTTPException, urllib.error.URLError):
                    res.fail += 1
                    res.conn_error += 1
                    try:
                        conn.close()
                    except Exception:
                        pass
                    conn = None
                except Exception:
                    res.fail += 1
                    res.other_error += 1
                    try:
                        conn.close()
                    except Exception:
                        pass
                    conn = None

    threads = [threading.Thread(target=thread_main) for _ in range(concurrency)]
    for th in threads:
        th.daemon = True
        th.start()
    for th in threads:
        th.join()

    total_ok = sum(r.ok for r in per_thread)
    total_fail = sum(r.fail for r in per_thread)
    total = total_ok + total_fail
    actual_duration = max(1e-9, time.monotonic() - start_mono)
    actual_rps = total / actual_duration
    error_rate = (total_fail / total * 100.0) if total else 100.0

    samples: list[float] = []
    for r in per_thread:
        samples.extend(r.samples_ms)
    samples.sort()

    p50 = _percentile(samples, 50.0)
    p95 = _percentile(samples, 95.0)
    p99 = _percentile(samples, 99.0)

    print("=== load test summary ===")
    print(f"url={url}")
    print(f"duration_s={duration_s}")
    print(f"target_rps={target_rps}")
    print(f"concurrency={concurrency}")
    print(f"timeout_s={timeout}")
    print(f"total_requests={total}")
    print(f"ok={total_ok}")
    print(f"fail={total_fail}")
    print(f"actual_rps={actual_rps:.2f}")
    print(f"error_rate_pct={error_rate:.2f}")
    print("=== breakdown ===")
    print(f"ok_2xx_3xx={sum(r.ok_2xx_3xx for r in per_thread)}")
    print(f"http_4xx={sum(r.http_4xx for r in per_thread)}")
    print(f"http_429={sum(r.http_429 for r in per_thread)}")
    print(f"http_5xx={sum(r.http_5xx for r in per_thread)}")
    print(f"timeout={sum(r.timeout for r in per_thread)}")
    print(f"conn_error={sum(r.conn_error for r in per_thread)}")
    print(f"other_error={sum(r.other_error for r in per_thread)}")
    print("=== latency_ms_samples ===")
    print(f"samples={len(samples)}")
    if p50 is not None:
        print(f"p50={p50:.2f}")
    if p95 is not None:
        print(f"p95={p95:.2f}")
    if p99 is not None:
        print(f"p99={p99:.2f}")
    if samples:
        print(f"max={samples[-1]:.2f}")
    return 0 if total_fail == 0 else 1


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default="http://localhost:28008/_matrix/client/versions")
    parser.add_argument("--rps", type=int, default=2000)
    parser.add_argument("--duration", type=int, default=1800)
    parser.add_argument("--concurrency", type=int, default=256)
    parser.add_argument("--timeout", type=float, default=2.0)
    parser.add_argument("--max-samples", type=int, default=200000)
    parser.add_argument("--block-size", type=int, default=256)
    parser.add_argument(
        "--bearer",
        default="",
        help="If set, sends Authorization: Bearer <token> header",
    )
    parser.add_argument(
        "--header",
        action="append",
        default=[],
        help="Extra header in 'Key: Value' form. Can be specified multiple times.",
    )
    args = parser.parse_args()

    extra_headers: dict[str, str] = {}
    if args.bearer:
        extra_headers["Authorization"] = f"Bearer {args.bearer}"
    for raw in args.header:
        if ":" not in raw:
            raise SystemExit("--header must be in 'Key: Value' form")
        k, v = raw.split(":", 1)
        k = k.strip()
        v = v.strip()
        if not k:
            raise SystemExit("--header has empty key")
        extra_headers[k] = v

    return run_test(
        args.url,
        args.rps,
        args.duration,
        args.concurrency,
        args.timeout,
        args.max_samples,
        args.block_size,
        extra_headers if extra_headers else None,
    )


if __name__ == "__main__":
    raise SystemExit(main())
