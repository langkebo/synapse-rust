#!/usr/bin/env python3
import json
import os
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from threading import Lock


HOST = os.environ.get("MOCK_APPSERVICE_HOST", "127.0.0.1")
PORT = int(os.environ.get("MOCK_APPSERVICE_PORT", "9100"))
_FAILURE_LOCK = Lock()


def _parse_failure_budget(raw: str) -> dict[str, int]:
    budget: dict[str, int] = {}
    for entry in raw.split(","):
        entry = entry.strip()
        if not entry:
            continue
        token, sep, count = entry.partition(":")
        if not sep:
            continue
        try:
            budget[token.strip()] = max(0, int(count.strip()))
        except ValueError:
            continue
    return budget


FAILURE_BUDGET = _parse_failure_budget(os.environ.get("MOCK_FAIL_COUNTS", ""))
ALWAYS_FAIL_TOKENS = {
    token.strip()
    for token in os.environ.get("MOCK_ALWAYS_FAIL_TOKENS", "").split(",")
    if token.strip()
}


class Handler(BaseHTTPRequestHandler):
    server_version = "mock-appservice-bridge/1.0"

    def _write_json(self, status_code: int, payload: dict) -> None:
        body = json.dumps(payload).encode("utf-8")
        self.send_response(status_code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, fmt: str, *args) -> None:
        # Keep output readable during long stress runs.
        return

    def do_POST(self) -> None:
        if self.path == "/_matrix/app/v1/ping":
            self._write_json(200, {"ok": True})
            return
        self._write_json(404, {"error": "not found"})

    def do_PUT(self) -> None:
        length = int(self.headers.get("Content-Length", "0"))
        if length:
            self.rfile.read(length)
        auth_header = self.headers.get("Authorization", "")
        token = auth_header.removeprefix("Bearer ").strip()
        if token in ALWAYS_FAIL_TOKENS:
            self._write_json(503, {"error": "configured persistent failure"})
            return

        with _FAILURE_LOCK:
            remaining_failures = FAILURE_BUDGET.get(token, 0)
            if remaining_failures > 0:
                FAILURE_BUDGET[token] = remaining_failures - 1
                self._write_json(
                    503,
                    {
                        "error": "configured transient failure",
                        "remaining_failures": FAILURE_BUDGET[token],
                    },
                )
                return

        self._write_json(200, {"ok": True})


def main() -> None:
    httpd = ThreadingHTTPServer((HOST, PORT), Handler)
    print(f"mock appservice bridge listening on http://{HOST}:{PORT}", flush=True)
    httpd.serve_forever()


if __name__ == "__main__":
    main()
