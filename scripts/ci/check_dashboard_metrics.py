#!/usr/bin/env python3
"""Grafana 面板指标名可达性门禁。

**为什么需要它**：本仓的面板长期"引用一套不存在的指标名"。实测（2026-09-22）
一次改动把 4 个面板里的 3 个 `histogram_quantile(...,_bucket[5m])` 降级成
`rate(_sum)/rate(_count)` 均值、并把指标名整批换成另一套同样不存在的名字
（`auth_requests_total` / `e2ee_session_count` / `persist_events_duration_ms_*` /
`turn_active_connections` …），而**没有任何门禁会因此变红** —— 面板只会在 Grafana
里安静地显示 "No data"（或者更糟：标题与表达式语义错位后显示一个自信的错数字）。

**判据（静态，无需活的 Prometheus，因此可在 CI 里跑）**：把每个面板表达式里出现的
**指标选择器**抽出来，逐个比对三处"名字的真实来源"：

  1. Rust 侧的注册点    `register_{counter,gauge,histogram}[...]("name")`
                        （直方图展开出 `name_bucket` / `name_count` / `name_sum`，
                          计数器兼容 `name_total`）
  2. Prometheus 录制规则 `- record: <name>`
  3. 显式外部白名单      `scripts/ci/dashboard_metrics_allowlist`
                        （node_exporter / prometheus 自监控 / alertmanager / coturn
                          exporter 等**不由本仓注册**的名字，逐条登记并写明来源）

三处都没有 ⇒ 违规。新增一个外部指标必须显式往白名单加一行（附来源说明），
不允许按前缀放行 —— 按前缀放行正是让 `turn_active_connections` 这类
"看着像但不是"的名字蒙混过关的原因。

**棘轮**：`scripts/ci/dashboard_metrics_baseline` 记录已登记的历史违规（只减不增）。
`--update` 重写基线。

退出码：0 = 通过；1 = 有新增违规；2 = 环境/解析失败（响亮失败，不静默放过）。
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
from pathlib import Path

DASHBOARDS_DIR = "docker/deploy/grafana/dashboards"
RULES_FILE = "docker/deploy/prometheus/recording-rules.yml"
ALLOWLIST_FILE = "scripts/ci/dashboard_metrics_allowlist"
BASELINE_FILE = "scripts/ci/dashboard_metrics_baseline"

# Rust 源码里指标注册/取用点的扫描范围（workspace crate 的 src 目录）。
RUST_SCAN_DIRS = [
    "src",
    "synapse-common/src",
    "synapse-web/src",
    "synapse-e2ee/src",
    "synapse-federation/src",
    "synapse-services/src",
    "synapse-storage/src",
    "synapse-cache/src",
]

REGISTER_RE = re.compile(
    r"""(?:register_|get_)(?:counter|gauge|histogram)[a-z_]*\s*\(\s*"([^"]+)"\s*"""
)
RECORD_RE = re.compile(r"^\s*-\s*record:\s*(\S+)\s*$", re.MULTILINE)
# 表达式里的字符串字面量与标签匹配器里的标识符都不是指标名。
STR_LITERAL_RE = re.compile(r'"[^"]*"')
LABEL_MATCHER_RE = re.compile(r"\{[^{}]*\}")
GROUPING_RE = re.compile(r"\b(?:by|without|on|ignoring)\s*\([^)]*\)")
RANGE_RE = re.compile(r"\[[^\]]*\]")
FUNC_CALL_RE = re.compile(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(")
IDENT_RE = re.compile(r"[A-Za-z_:][A-Za-z0-9_:]*")

KEYWORDS = {
    "and",
    "or",
    "unless",
    "offset",
    "bool",
    "inf",
    "nan",
    "group_left",
    "group_right",
    "le",
}


class GateError(RuntimeError):
    """环境或解析失败 —— 必须以非 0 退出，不能静默当成通过。"""


def run_git(args: list[str]) -> str:
    try:
        out = subprocess.run(
            ["git", *args], capture_output=True, text=True, check=True
        )
    except FileNotFoundError as exc:  # pragma: no cover - 环境问题
        raise GateError("git 不可用，无法枚举源码/规则文件") from exc
    except subprocess.CalledProcessError as exc:
        raise GateError(f"`git {' '.join(args)}` 失败：{exc.stderr.strip()}") from exc
    return out.stdout


def registered_names(repo: Path) -> set[str]:
    """Rust 侧注册的指标基名（含直方图/计数器的派生名展开）。"""
    base: set[str] = set()
    for d in RUST_SCAN_DIRS:
        target = repo / d
        if not target.exists():
            continue
        for p in target.rglob("*.rs"):
            try:
                text = p.read_text(encoding="utf-8", errors="replace")
            except OSError as exc:  # pragma: no cover
                raise GateError(f"读取 {p} 失败：{exc}") from exc
            base.update(REGISTER_RE.findall(text))
    if not base:
        raise GateError(
            "在 Rust 源码里一个 register_*(\"name\") 都没扫到 —— 扫描范围失效，"
            "不要把它当成'没有指标'"
        )
    out = set()
    for name in base:
        out |= {name, f"{name}_total", f"{name}_bucket", f"{name}_count", f"{name}_sum"}
    return out


def recorded_names(repo: Path) -> set[str]:
    path = repo / RULES_FILE
    if not path.exists():
        raise GateError(f"录制规则文件不存在：{RULES_FILE}")
    return set(RECORD_RE.findall(path.read_text(encoding="utf-8")))


def allowlisted(repo: Path) -> set[str]:
    path = repo / ALLOWLIST_FILE
    if not path.exists():
        raise GateError(f"外部白名单不存在：{ALLOWLIST_FILE}")
    names = set()
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        names.add(line.split()[0])
    return names


def selectors(expr: str) -> set[str]:
    """从一条 PromQL 里抽出候选指标选择器。"""
    e = STR_LITERAL_RE.sub('""', expr)
    e = LABEL_MATCHER_RE.sub("", e)
    e = GROUPING_RE.sub(" ", e)
    e = RANGE_RE.sub("", e)
    funcs = set(FUNC_CALL_RE.findall(e))
    e = FUNC_CALL_RE.sub(" ", e)
    return {t for t in IDENT_RE.findall(e) if t not in funcs and t not in KEYWORDS}


def dashboard_expressions(root: Path) -> tuple[list[tuple[str, str, str]], list[str]]:
    """返回 ([(文件, 面板标题, 表达式)], [用了 API 导出信封的文件名])。

    信封形态 `{"dashboard": {...}, "meta": {...}}` 是 Grafana **API 导出**格式，
    `type: file` 的 provisioning 读不了它 —— 实测日志是
    `error="Dashboard title cannot be empty"`，后果是**整个面板库一个都没加载**
    （比"指标名写错"更彻底）。所以这里不自动拆信封，而是把它当违规报出来，
    避免"静默拆掉后门禁通过、但 Grafana 依旧加载不了"的假绿。
    """
    rows: list[tuple[str, str, str]] = []
    enveloped: list[str] = []
    files = sorted(root.glob("*.json"))
    if not files:
        raise GateError(f"{root} 下没有 JSON 面板文件 —— 路径失效，不要当成通过")
    for path in files:
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except json.JSONDecodeError as exc:
            raise GateError(f"{path.name} 不是合法 JSON：{exc}") from exc
        if not isinstance(data, dict):
            raise GateError(f"{path.name} 顶层不是对象")
        if isinstance(data.get("dashboard"), dict):
            enveloped.append(path.name)
            data = data["dashboard"]
        for panel in data.get("panels", []):
            title = panel.get("title", "?")
            for target in panel.get("targets", []):
                expr = target.get("expr")
                if expr:
                    rows.append((path.name, title, expr))
    if not rows and not enveloped:
        raise GateError(
            f"{root} 下 {len(files)} 个面板文件里一条 expr 都没有 —— 结构变了，"
            "不要把它当成'没有违规'"
        )
    return rows, enveloped


def read_baseline(repo: Path) -> set[str]:
    path = repo / BASELINE_FILE
    if not path.exists():
        return set()
    return {
        line.split()[0]
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#")
    }


def write_baseline(repo: Path, names: set[str]) -> None:
    path = repo / BASELINE_FILE
    body = [
        "# 面板引用了、但三处名字来源都没有的指标。只减不增。",
        "# 生成：python3 scripts/ci/check_dashboard_metrics.py --update",
        "",
    ]
    body += sorted(names)
    path.write_text("\n".join(body) + "\n", encoding="utf-8")


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--repo-root", default=None)
    ap.add_argument("--dir", default=DASHBOARDS_DIR, help="面板目录（自证时指向 HEAD 副本）")
    ap.add_argument("--update", action="store_true", help="用当前结果重写基线")
    args = ap.parse_args()

    if args.repo_root:
        repo = Path(args.repo_root).resolve()
    else:
        repo = Path(run_git(["rev-parse", "--show-toplevel"]).strip())
    os.chdir(repo)

    known = registered_names(repo) | recorded_names(repo) | allowlisted(repo)
    dash_dir = (repo / args.dir).resolve()

    violations: dict[str, list[str]] = {}
    rows, enveloped = dashboard_expressions(dash_dir)
    for fname in enveloped:
        violations.setdefault("<API 导出信封：provisioning 会拒绝加载>", []).append(
            f"{fname} 顶层是 {{dashboard, meta}}，应为裸仪表板对象"
        )
    for fname, title, expr in rows:
        for name in sorted(selectors(expr)):
            if name not in known:
                violations.setdefault(name, []).append(f"{fname} :: {title}")

    baseline = read_baseline(repo)

    if args.update:
        write_baseline(repo, set(violations))
        print(f"基线已写入 {BASELINE_FILE}（{len(violations)} 条）")
        return 0

    new = {n: v for n, v in violations.items() if n not in baseline}
    stale = baseline - set(violations)

    print(
        f"面板表达式引用的指标名：{len(known)} 个已知名（Rust 注册 + 录制规则 + 外部白名单）"
    )
    print(f"违规（三处都查不到）：{len(violations)}；其中新增 {len(new)}、基线内 {len(violations) - len(new)}")

    if new:
        print("\n新增违规（这些面板会永远 No data，或更糟 —— 标题与语义错位）：")
        for name, wheres in sorted(new.items()):
            print(f"  ✗ {name}")
            for w in wheres[:3]:
                print(f"      出现在 {w}")
        print("\n修法：改成真实名字，或（仅当确为外部 exporter 指标时）")
        print(f"往 {ALLOWLIST_FILE} 加一行并写清来源。")
        return 1

    if stale:
        print(f"\n⚠️ 基线里有 {len(stale)} 条已不再违规，请 --update 收缩：{sorted(stale)}")

    print("\nOK：面板引用的指标名全部可达")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except GateError as exc:
        print(f"GATE ERROR: {exc}", file=sys.stderr)
        sys.exit(2)
