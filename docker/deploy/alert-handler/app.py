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
import os
import threading
import urllib.error
import urllib.request

from flask import Flask, jsonify, request

app = Flask(__name__)
logging.basicConfig(level=logging.INFO, format="%(asctime)s %(levelname)s %(message)s")
logger = logging.getLogger("alert-handler")

# ── 可选的通知出口 ────────────────────────────────────────────────────────────
# 2026-10-23 之前本服务只 `logger.info` 就把 200 返回给 Alertmanager，等于"有链路、
# 无出口"：36 条告警评估得好好的，终点是容器日志。没有 log 采集就等于没有通知，
# 而**没有任何地方说过这件事**。
#
# 现在把出口变成一个显式配置：设了 `ALERT_FORWARD_URL` 就转发，没设就在**启动时**
# 打一条 warning。区别不是"多了个功能"，而是"缺失不再静默"。
FORWARD_URL = os.environ.get("ALERT_FORWARD_URL", "").strip()
FORWARD_TIMEOUT_SECONDS = float(os.environ.get("ALERT_FORWARD_TIMEOUT_SECONDS", "5"))

if FORWARD_URL:
    logger.info("alert-forward: enabled -> %s", FORWARD_URL)
else:
    logger.warning(
        "alert-forward: ALERT_FORWARD_URL 未配置 —— 告警只会写进本容器日志，"
        "不会推送到任何渠道。要真正收到通知，请设置 ALERT_FORWARD_URL"
        "（钉钉/企业微信/任意接收 JSON POST 的 webhook）并重启本服务。"
    )


def _forward(severity: str, payload: dict) -> None:
    """把原始 Alertmanager payload 转发到 `ALERT_FORWARD_URL`。

    **失败不影响对 Alertmanager 的返回值**：它只关心"我送到没有"，返回非 2xx 会让它
    重试整个告警组；而下游渠道挂掉时重试帮不上忙，只会把问题放大成投递风暴。
    转发失败只记 warn —— 让故障停在日志里，不扩散。
    """
    body = json.dumps(payload, ensure_ascii=False).encode("utf-8")
    req = urllib.request.Request(
        FORWARD_URL,
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(req, timeout=FORWARD_TIMEOUT_SECONDS) as resp:
            if not 200 <= resp.status < 300:
                logger.warning(
                    "alert-forward: 下游返回非 2xx severity=%s status=%s",
                    severity,
                    resp.status,
                )
    except (urllib.error.URLError, TimeoutError, OSError) as exc:
        logger.warning("alert-forward: 转发失败 severity=%s error=%s", severity, exc)


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
    if FORWARD_URL:
        # 异步转发：下游慢或挂掉不应该拖住对 Alertmanager 的响应（那会触发重试风暴）。
        # daemon 线程不阻止进程退出，gunicorn 回收 worker 时也不会卡住。
        threading.Thread(target=_forward, args=(severity, payload), daemon=True).start()
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
