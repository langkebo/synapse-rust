"""Alertmanager webhook receiver for the HuLa/Tjg monitoring stack.

The routes below are the ones `alertmanager.yml` actually posts to —
`/api/v1/alerts`, `/api/v1/alerts/critical` and `/api/v1/alerts/warning`.
They must match exactly: a mismatch returns 404 and Alertmanager silently
drops the notification.

Historical bug (2026-09-21): this file only served `/webhook`, while
`alertmanager.yml` posted to `/api/v1/alerts*`, so every notification was
discarded even when the receiver was reachable.
"""

import json
import logging

from flask import Flask, jsonify, request

app = Flask(__name__)
logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
logger = logging.getLogger("alert-handler")


def _record(severity: str):
    """Log every alert payload and acknowledge it to Alertmanager.

    Returning 2xx is what tells Alertmanager the notification was delivered;
    anything else makes it retry the whole group.
    """
    payload = request.get_json(silent=True) or {}
    alerts = payload.get("alerts", []) if isinstance(payload, dict) else []
    logger.info(
        "alert received severity=%s status=%s count=%s payload=%s",
        severity,
        payload.get("status") if isinstance(payload, dict) else None,
        len(alerts),
        json.dumps(payload, ensure_ascii=False),
    )
    return jsonify({"status": "ok", "received": len(alerts)})


@app.route("/api/v1/alerts", methods=["POST"])
def alerts():
    """Default receiver — all severities routed here."""
    return _record("default")


@app.route("/api/v1/alerts/critical", methods=["POST"])
def alerts_critical():
    """Critical + security receivers."""
    return _record("critical")


@app.route("/api/v1/alerts/warning", methods=["POST"])
def alerts_warning():
    """Warning receiver."""
    return _record("warning")


@app.route("/webhook", methods=["POST"])
def webhook():
    """Legacy alias, kept so older Alertmanager configs keep working."""
    return _record("legacy")


@app.route("/health", methods=["GET"])
def health():
    """Liveness probe."""
    return jsonify({"status": "healthy"})


if __name__ == "__main__":
    app.run(host="0.0.0.0", port=8080)
