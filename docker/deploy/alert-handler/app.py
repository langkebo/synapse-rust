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

# 下游的消息格式。默认 `raw` 原样透传 Alertmanager payload —— 那只对"自己写的接收器"有用，
# 钉钉/企业微信都不认（它们要 `msgtype` 包裹）。所以格式必须是显式选项，而不是"填个 URL 就行"。
FORWARD_FORMAT = os.environ.get("ALERT_FORWARD_FORMAT", "raw").strip().lower()

# 单条消息的体积上限（字节）。钉钉 text 上限 20000、企业微信 markdown 上限 4096，
# 取小的那个 ⇒ 两种格式都不会被下游拒收。
MAX_BODY_BYTES = 4000
# 一条消息里最多列几条告警：超出只报总数。告警风暴时把 200 条糊进一条消息，
# 人在群里根本读不了，不如给"共 N 条" + 前几条样本。
MAX_ALERTS_IN_MESSAGE = 10

_KNOWN_FORMATS = ("raw", "dingtalk", "wecom")
if FORWARD_FORMAT not in _KNOWN_FORMATS:
    logger.warning(
        "alert-forward: ALERT_FORWARD_FORMAT=%r 不认识，回退成 raw（原样透传）。"
        "可选值：%s",
        FORWARD_FORMAT,
        " / ".join(_KNOWN_FORMATS),
    )
    FORWARD_FORMAT = "raw"

if FORWARD_URL:
    logger.info("alert-forward: enabled -> %s (format=%s)", FORWARD_URL, FORWARD_FORMAT)
else:
    logger.warning(
        "alert-forward: ALERT_FORWARD_URL 未配置 —— 告警只会写进本容器日志，"
        "不会推送到任何渠道。要真正收到通知，请设置 ALERT_FORWARD_URL"
        "（钉钉/企业微信/任意接收 JSON POST 的 webhook）并重启本服务。"
    )


def _truncate(text: str, limit: int = MAX_BODY_BYTES) -> str:
    """按**字节**截断且不切断多字节字符。

    两点容易错：① `text[:limit]` 数的是字符不是字节，而下游的 4096/20000 都是字节；
    中文一字 3 字节，按字符截会把消息撑到三倍上限。② 截断标记本身也占字节，必须从
    预算里扣掉，否则 `MAX_BODY_BYTES=4000` 会发出 4043 字节 —— 名字和行为对不上。
    """
    encoded = text.encode("utf-8")
    if len(encoded) <= limit:
        return text
    marker = f"\n…（已截断，原消息 {len(encoded)} 字节）"
    budget = max(0, limit - len(marker.encode("utf-8")))
    head = encoded[:budget].decode("utf-8", "ignore")
    return f"{head}{marker}"


def _summarize(severity: str, payload: dict) -> str:
    """把 Alertmanager payload 压成人能读的几行文本。

    只取 `labels.alertname`、`annotations.summary`、`status` 三个字段 —— 其余
    （`startsAt`、`generatorURL`、`fingerprint`）对值班的人没有即时价值，塞进群里
    只会把关键信息挤到屏幕外。
    """
    alerts = payload.get("alerts") or []
    status = str(payload.get("status") or "unknown")
    shown_limit = min(len(alerts), MAX_ALERTS_IN_MESSAGE)
    head = f"[{severity.upper()}] {len(alerts)} 条告警 · {status}"
    # "只列了前 N 条"必须写在**头部**：写在尾部会被 `_truncate` 砍掉，
    # 于是群里看到 10 条会以为总共就 10 条 —— 一个只报总数不报警的摘要。
    if len(alerts) > shown_limit:
        head += f"（仅列前 {shown_limit} 条）"
    lines = [head]
    for item in alerts[:shown_limit]:
        if not isinstance(item, dict):
            continue
        labels = item.get("labels") or {}
        ann = item.get("annotations") or {}
        name = labels.get("alertname") or "（无 alertname）"
        summary = ann.get("summary") or ann.get("description") or ""
        one_line = " ".join(str(summary).split())
        lines.append(f"- {name}: {one_line}" if one_line else f"- {name}")
    return _truncate("\n".join(lines))


def _render(severity: str, payload: dict) -> dict:
    """按 `ALERT_FORWARD_FORMAT` 渲染成下游要的 JSON 结构。"""
    if FORWARD_FORMAT == "dingtalk":
        return {"msgtype": "text", "text": {"content": _summarize(severity, payload)}}
    if FORWARD_FORMAT == "wecom":
        # 企微 markdown 支持 `**加粗**`；只把**首行的** `[severity]` 加粗，
        # 不动正文里的方括号（告警 summary 里出现 `[` 时整体 replace 会把格式搞坏）。
        first, sep, rest = _summarize(severity, payload).partition("\n")
        first = first.replace("[", "**[").replace("]", "]**", 1)
        content = f"{first}{sep}{rest}" if sep else first
        return {"msgtype": "markdown", "markdown": {"content": content}}
    return payload


def _forward(severity: str, payload: dict) -> None:
    """把渲染后的 payload 转发到 `ALERT_FORWARD_URL`。

    **失败不影响对 Alertmanager 的返回值**：它只关心"我送到没有"，返回非 2xx 会让它
    重试整个告警组；而下游渠道挂掉时重试帮不上忙，只会把问题放大成投递风暴。
    转发失败只记 warn —— 让故障停在日志里，不扩散。
    """
    body = json.dumps(_render(severity, payload), ensure_ascii=False).encode("utf-8")
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
