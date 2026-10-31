#!/usr/bin/env python3
"""Prometheus 配置门禁：语法合法 + 规则可解析 + 目标真实存在 + 变体不漂移。

## 为什么需要这个门禁

2026-10-10 的运维巡检发现监控交付物存在一类**静默失效**：配置能提交、面板能加载，
但永远没有数据。逐项取证如下：

1. `monitoring/prometheus/prometheus.yml` 把 Alertmanager 的
   `repeat_interval` / `send_resolved` 写在 Prometheus 的 `alerting.alertmanagers[]`
   下 —— `promtool check config` 直接报
   `field not found in type config.AlertmanagerConfig`，**Prometheus 拒绝启动**。
   该文件此前从未被任何门禁校验过。

2. 同一文件的 `metric_relabel_configs` 写成 `action: keep` + 正则
   `(go_.*|process_.*|promhttp_.*)`，与注释（"过滤不需要的指标"）语义**相反**：
   keep 会把 `synapse_*` / `http_requests_total` / `room_*` 等**全部应用指标丢掉**
   → 所有业务面板恒 No data，且不报任何错。

3. `prometheus-with-mock.yml` 用 `static_configs.targets: ["file://...prom"]`
   + `file_sd_configs.files: ['.../*.prom']` 表达"MOCK 数据" —— 两者都非法
   （`static_configs.targets` 只能是 `host:port`；`file_sd` 提供的是**目标列表**
   而非指标样本），promtool 报 `path name ... is not valid for file discovery`。

4. 配置里指向**不存在的主机**（`db:9187` / `redis:9121` / `grafana:3000`）时
   Prometheus 不报错，只把该目标永久标 `down`，依赖它的面板静默变空 ——
   本仓 `coturn` 正是这样长期 down 而无人察觉（见 A6）。

因此本门禁做下面这些检查，且**任何一步失败都必须让 CI 变红**（不允许静默跳过）：

  A. `promtool check config` —— 抓取配置语法（含 rule_files 可达性）
  B. `promtool check rules`  —— 告警/录制规则语法
  C. **死目标守卫**：每个 `static_configs.targets` 的 host 必须是
     compose 里真实存在的 service 名 / container_name，
     或 `localhost` / `127.0.0.1`（自抓取），
     或登记在 `scripts/ci/prometheus_target_allowlist`（必须写明理由）。
  D. **漂移守卫**：`prometheus-with-mock.yml` 的 job 集合必须恰好等于
     `prometheus.yml` 的 job 集合 ∪ `{mock-*}`。
     两个文件内容几乎相同却各自演进，是本仓反复出现的"配置双副本"形态
     （参见 docker-compose.yml 头部对 `docker/config/` 的单源纪律）。
  E. `metric_relabel_configs` 的 keep/drop 语义守卫。
  F. `amtool check-config` 校验 Alertmanager 配置。
  G. **应用 `/metrics` 暴露面不变量**（问题清单 2026-10-31 §1.4 / §3.3）：
     `prometheus.public: true` 只能表达"匿名可抓"，表达不了"仅授权抓取"，
     于是"公网不可达"完全依赖两条**隐性事实**：compose 只把 9090 绑到回环、
     nginx 不转发它。任何人加一条 `ports: ["9090:9090"]` 或一条
     `proxy_pass http://synapse-app:9090` 就会静默把全部业务指标暴露出去，
     而没有门禁会红。G 把这两条事实变成可检查的不变量，并自带正反例自检。

## promtool 从哪来

优先用 Docker 镜像 `prom/prometheus:v2.47.0`（版本与
`docker/docker-compose.monitoring.yml` 中实际部署的 Prometheus 钉死一致 ——
用别的版本校验可能放过部署版本才有的字段错误）；没有 docker 时退回
PATH 上的 `promtool`。两者都不可用时**判失败**而不是跳过 ——
跳过等于门禁不存在。
"""

from __future__ import annotations

import argparse
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

try:
    import yaml
except ImportError:  # pragma: no cover - 环境缺依赖时给出可操作提示
    print("FATAL: 需要 pyyaml（pip install pyyaml）", file=sys.stderr)
    sys.exit(2)

REPO = Path(__file__).resolve().parents[2]

# promtool 容器镜像：**从各监控栈的 compose 派生**，取其中最**新**的版本。
#
# 为什么不硬编码（2026-10-15 实测）：
#   `promtool check rules` 自 v2.48 起会**校验 annotation 模板**，v2.47 **不校验**。
#   同一份 `monitoring/alerting-rules.yml`：
#     prom/prometheus:v2.47.0 → exit 0（对 6 处 `{{ $values.X }}` 视而不见）
#     prom/prometheus:v2.48.1 → exit 1（逐条报 undefined variable "$values"）
#   而两个栈的钉版并不相同（dev=v2.47.0、deploy=v2.48.1）。本门禁原先硬编码 v2.47.0，
#   于是"**触发时必炸**的告警模板在门禁下长期显示 OK" —— 又一个版本敏感型盲区。
#
# 规则：用**所有已部署版本里最新的那个**校验全部配置 = 用最严格的工具检查；
# 且任何一处 compose 升版都会被这条派生逻辑立刻吸收，不会静默落后。
MONITORING_COMPOSE_FILES = [
    "docker/deploy/docker-compose.monitoring.yml",
]
_PROM_IMAGE_RE = re.compile(r"image:\s*(prom/prometheus:(v[0-9][0-9.]*))")
_ALERTMANAGER_IMAGE_RE = re.compile(r"image:\s*(prom/alertmanager:(v[0-9][0-9.]*))")


def _version_key(image: str) -> tuple[int, ...]:
    version = image.rsplit(":", 1)[1]
    return tuple(int(part) for part in version.lstrip("v").split(".") if part.isdigit())


def derive_prometheus_image() -> str:
    """从各监控 compose 的钉版里取**最新**的 prom/prometheus 镜像名。"""
    pins: dict[str, list[str]] = {}
    for rel in MONITORING_COMPOSE_FILES:
        path = REPO / rel
        if not path.is_file():
            continue
        for match in _PROM_IMAGE_RE.finditer(path.read_text()):
            pins.setdefault(match.group(1), []).append(rel)
    if not pins:
        raise Failure(
            "无法从任何监控 compose 派生出 prom/prometheus 镜像版本（已尝试："
            + ", ".join(MONITORING_COMPOSE_FILES)
            + "）。\n"
            "      → 找不到版本就无法校验配置；**不允许**退回硬编码默认值"
            "（那正是本门禁 2026-10-15 之前的盲区根源）。"
        )
    newest = max(pins, key=_version_key)
    if len(pins) > 1:
        print(f"monitoring compose 存在多个 prom/prometheus 钉版：{sorted(pins)}")
        print(f"  → 用最新的 {newest} 校验全部配置（最严格口径）。")
    return newest


_prometheus_image_cache: str | None = None


def prometheus_image() -> str:
    global _prometheus_image_cache
    if _prometheus_image_cache is None:
        _prometheus_image_cache = derive_prometheus_image()
    return _prometheus_image_cache


def derive_alertmanager_image() -> str:
    """从监控 compose 的钉版里取**最新**的 prom/alertmanager 镜像名。

    与 prometheus 同理：不硬编码版本，任何一处 compose 升版都被这条派生逻辑立刻吸收。
    `amtool` 只随 `prom/alertmanager` 发布，所以镜像必须从 alertmanager 服务派生，
    不能借用 prometheus 镜像（那里没有 amtool）。
    """
    pins: dict[str, list[str]] = {}
    for rel in MONITORING_COMPOSE_FILES:
        path = REPO / rel
        if not path.is_file():
            continue
        for match in _ALERTMANAGER_IMAGE_RE.finditer(path.read_text()):
            pins.setdefault(match.group(1), []).append(rel)
    if not pins:
        raise Failure(
            "无法从任何监控 compose 派生出 prom/alertmanager 镜像版本（已尝试："
            + ", ".join(MONITORING_COMPOSE_FILES)
            + "）。\n"
            "      → 找不到版本就无法校验 alertmanager 配置；**不允许**硬编码默认值。"
        )
    newest = max(pins, key=_version_key)
    if len(pins) > 1:
        print(f"monitoring compose 存在多个 prom/alertmanager 钉版：{sorted(pins)}")
        print(f"  → 用最新的 {newest} 校验全部配置（最严格口径）。")
    return newest


_alertmanager_image_cache: str | None = None


def alertmanager_image() -> str:
    global _alertmanager_image_cache
    if _alertmanager_image_cache is None:
        _alertmanager_image_cache = derive_alertmanager_image()
    return _alertmanager_image_cache


ALLOWLIST = REPO / "scripts/ci/prometheus_target_allowlist"

# 自抓取目标（容器内 localhost 即 Prometheus 自身）
SELF_TARGETS = {"localhost", "127.0.0.1", "::1"}

# 每个 Prometheus 配置 → 它应当在哪些 compose 文件的服务集合里找目标。
# 键是配置路径（相对仓库根），值是 compose 路径列表。
#
# 2026-10-23：`monitoring/prometheus/*.yml` 与 `docker/docker-compose.monitoring.yml` 一并删除
# ——那套 dev 栈的 Alertmanager 配置被 `amtool check-config` 判为非法（`${VAR:-默认}` 是字面值），
# 且从未出现在 `docker/deploy/deploy.sh` 的部署路径上，`README_MONITORING.md` 也早已写明不要使用它。
CONFIG_COMPOSE_MAP: dict[str, list[str]] = {
    "docker/deploy/prometheus/prometheus.yml": [
        "docker/deploy/docker-compose.yml",
        "docker/deploy/docker-compose.monitoring.yml",
    ],
}

# 需要校验的规则文件（配置里的 rule_files 另行动态解析）
RULE_FILES = [
    "docker/deploy/prometheus/alerting-rules.yml",
    "docker/deploy/prometheus/recording-rules.yml",
]

# Alertmanager 配置文件 —— 用 `amtool check-config` 校验。
# `promtool` 只解析 Prometheus 配置，**完全不碰**这类文件，所以必须单列（section F）。
ALERTMANAGER_CONFIGS = [
    "docker/deploy/alertmanager/alertmanager.yml",
]

# 漂移守卫的基准对：(变体配置, 基准配置)
#
# **当前为空**：唯一的变体 `prometheus-with-mock.yml` 随 monitoring 栈删除。留空而非删掉整个
# 守卫，是因为"变体必须 = 基准 + 已知增量"这条不变量将来还会用上；`check_drift()` 对空列表
# 返回空（section D 显示 `不适用`，不是 `OK`）——见 `main()` 里对空 DRIFT_PAIRS 的处理。
DRIFT_PAIRS: list[tuple[str, str]] = []


class Failure(Exception):
    """一条可读的门禁失败。"""


def _run(cmd: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(cmd, capture_output=True, text=True, check=False)


def resolve_promtool() -> tuple[str, str]:
    """返回 (kind, prefix)。

    kind == "docker" 时 prefix 是镜像名，调用方负责把 `-v` 挂载放在镜像**之前**。
    kind == "local" 时 prefix 是 promtool 可执行文件路径。

    找不到就抛 Failure（不静默跳过）。
    """
    if shutil.which("docker"):
        return "docker", prometheus_image()
    local = shutil.which("promtool")
    if local:
        return "local", local
    raise Failure(
        "既没有 promtool 也没有 docker —— 无法校验 Prometheus 配置。\n"
        "  本门禁刻意不提供『跳过』分支：跳过等于门禁不存在（这正是被修的那类缺陷）。\n"
        "  修法：安装 promtool，或安装 Docker（CI 上二者必有其一）。"
    )


def resolve_amtool(prom_kind: str) -> tuple[str, str]:
    """返回校验 `alertmanager.yml` 用的 (kind, prefix)。

    docker 模式：用从 compose 派生的 **`prom/alertmanager`** 镜像 —— `amtool` 只随它发布，
    prometheus 镜像里没有。
    local 模式：找本机 `amtool`，找不到就 Failure（同样不提供跳过）。
    """
    if prom_kind == "docker":
        return "docker", alertmanager_image()
    local = shutil.which("amtool")
    if local:
        return "local", local
    raise Failure(
        "本机既没有 docker 也没有 amtool —— 无法校验 alertmanager 配置。\n"
        "  本门禁刻意不提供『跳过』分支：跳过等于门禁不存在。\n"
        "  修法：安装 Docker，或安装 amtool（随 alertmanager 一起发布）。"
    )


def _docker_cmd(
    image: str,
    mounts: list[tuple[Path, str]],
    args: list[str],
    entrypoint: str = "promtool",
) -> list[str]:
    """组装 `docker run --entrypoint <entrypoint> <mounts> <image> <args>`。

    挂载必须出现在镜像名**之前**，否则 docker 会把 `-v` 当成工具的参数
    （实测报 `unknown short flag '-v'`）。
    `entrypoint` 默认 `promtool`；校验 Alertmanager 配置时传 `amtool`。
    """
    cmd = ["docker", "run", "--rm", "--entrypoint", entrypoint]
    for host, container in mounts:
        cmd += ["-v", f"{host}:{container}:ro"]
    cmd += [image, *args]
    return cmd


def resolve_rule_file(config_rel: str, raw_path: str) -> Path:
    """把配置里声明的 rule_files 路径映射回仓库内的真实文件。"""
    base = os.path.basename(raw_path)
    candidates = [
        (REPO / config_rel).parent / base,
        REPO / "docker/deploy/prometheus" / base,
    ]
    host = next((c for c in candidates if c.is_file()), None)
    if host is None:
        raise Failure(
            f"{config_rel} 的 rule_files 指向 {raw_path}，但仓库里找不到对应文件\n"
            f"  （已尝试：{[str(c.relative_to(REPO)) for c in candidates]}）\n"
            "  这会让 Prometheus 启动时直接失败，必须修。"
        )
    return host


def check_config(kind: str, prefix: str, config_rel: str) -> list[str]:
    """A: promtool check config（含 rule_files 可达性）。"""
    config_abs = REPO / config_rel
    if not config_abs.is_file():
        return [f"{config_rel}: 文件不存在"]

    doc = yaml.safe_load(config_abs.read_text()) or {}
    rule_paths = [str(p) for p in (doc.get("rule_files") or [])]

    try:
        # 规则文件按**配置里声明的容器路径**回挂，这样 promtool 读到的路径
        # 与真实启动时一致，校验结果最贴近生产行为。
        rule_mounts = [(resolve_rule_file(config_rel, raw), raw) for raw in rule_paths]
    except Failure as e:
        return [str(e)]

    if kind == "docker":
        mounts = [(config_abs, "/tmp/prom-check.yml"), *rule_mounts]
        cmd = _docker_cmd(prefix, mounts, ["check", "config", "/tmp/prom-check.yml"])
        proc = _run(cmd)
    else:
        # 本地 promtool 读不到容器内绝对路径（/etc/prometheus/...），
        # 因此把 rule_files 重写成宿主真实路径后再校验（临时文件，不改动仓库）。
        import tempfile

        rewritten = dict(doc)
        rewritten["rule_files"] = [str(host) for host, _ in rule_mounts]
        with tempfile.NamedTemporaryFile("w", suffix=".yml", delete=False) as fh:
            yaml.safe_dump(rewritten, fh, allow_unicode=True, sort_keys=False)
            tmp = fh.name
        try:
            proc = _run([prefix, "check", "config", tmp])
        finally:
            os.unlink(tmp)

    if proc.returncode == 0 and "FAILED" not in proc.stdout:
        return []
    detail = (proc.stdout + proc.stderr).strip()
    return [f"{config_rel}:\n{_indent(detail)}"]


def check_rules(kind: str, prefix: str, rule_rel: str) -> list[str]:
    """B: promtool check rules。"""
    rule_abs = REPO / rule_rel
    if not rule_abs.is_file():
        return [f"{rule_rel}: 文件不存在"]

    if kind == "docker":
        cmd = _docker_cmd(
            prefix, [(rule_abs, str(rule_abs))], ["check", "rules", str(rule_abs)]
        )
    else:
        cmd = [prefix, "check", "rules", str(rule_abs)]

    proc = _run(cmd)
    if proc.returncode == 0 and "FAILED" not in proc.stdout:
        return []
    return [f"{rule_rel}:\n{_indent((proc.stdout + proc.stderr).strip())}"]


def check_alertmanager_configs(kind: str, prefix: str) -> list[str]:
    """F: amtool check-config —— Alertmanager 配置的语法与字段校验。

    为什么必须单独做：`promtool check config` 只解析 Prometheus 配置，**完全不碰**
    `alertmanager.yml`。2026-10-23 因此发现一份配置里同时存在三个致命问题而 CI 一直绿：

    * `smtp_smarthost: '${SMTP_SMARTHOST:-localhost:25}'` —— **Alertmanager 不做环境变量替换**，
      这是字面地址，`amtool` 报 `too many colons in address`；
    * `target_match_anc:` —— 不是 Alertmanager 的合法字段（合法的是
      `source_match` / `target_match` / `*_match_re` / `*_matchers`）；
    * 顶层 `silences:` —— 不是配置项（静默通过 API/UI 管理）。

    三条都会让 Alertmanager 启动即退出，而它被 compose 挂载 ⇒ 那套栈的告警一条都发不出去。
    """
    problems: list[str] = []
    for cfg_rel in ALERTMANAGER_CONFIGS:
        cfg_abs = REPO / cfg_rel
        if not cfg_abs.is_file():
            problems.append(f"{cfg_rel}: 文件不存在")
            continue
        if kind == "docker":
            cmd = _docker_cmd(
                prefix,
                [(cfg_abs.parent, "/cfg")],
                ["check-config", f"/cfg/{cfg_abs.name}"],
                entrypoint="amtool",
            )
        else:
            cmd = [prefix, "check-config", str(cfg_abs)]
        proc = _run(cmd)
        combined = (proc.stdout + proc.stderr).strip()
        if proc.returncode == 0 and "FAILED" not in combined:
            continue
        problems.append(f"{cfg_rel}:\n{_indent(combined)}")
    return problems


def _indent(text: str, prefix: str = "      ") -> str:
    return "\n".join(prefix + line for line in text.splitlines())


def load_allowlist() -> dict[str, str]:
    """读 allowlist：每行 `<host> <reason...>`；'#' 开头为注释。"""
    if not ALLOWLIST.is_file():
        return {}
    out: dict[str, str] = {}
    for lineno, raw in enumerate(ALLOWLIST.read_text().splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        parts = line.split(None, 1)
        if len(parts) != 2 or not parts[1].strip():
            raise Failure(
                f"{ALLOWLIST.relative_to(REPO)}:{lineno} 缺少理由（格式：`<host> <为什么允许>`）"
            )
        out[parts[0]] = parts[1].strip()
    return out


def _parse_declared_port(spec: str) -> str | None:
    """从 compose 端口写法里取出**容器侧**端口号。

    覆盖：`8008`、`8008:8008`、`127.0.0.1:9092:9090`、`3478:3478/udp`、
    `49152-49351:49152-49351/udp`。容器侧端口恒为「去掉协议、去掉主机映射后的最后一段」，
    端口区间取低端。
    """
    spec = spec.split("/", 1)[0].strip()
    if not spec:
        return None
    last = spec.split(":")[-1]
    if "-" in last:
        last = last.split("-", 1)[0]
    return last or None


def compose_service_ports(compose_rel: str) -> dict[str, set[str]]:
    """返回 {service 名 / container_name: 该服务声明的容器侧端口集合}。

    只检查「host 存在」是不够的：`db:9187` 里的 `db` 确实是 compose 服务，
    但 9187 上并没有 postgres_exporter 在监听 —— 这依然是永久 down 的目标
    （2026-10-10 巡检时 `monitoring/prometheus.yml` 就是这样写的）。
    所以必须连**端口**一起校验。
    """
    path = REPO / compose_rel
    if not path.is_file():
        return {}
    doc = yaml.safe_load(path.read_text()) or {}
    out: dict[str, set[str]] = {}
    for svc, body in (doc.get("services") or {}).items():
        ports: set[str] = set()
        if isinstance(body, dict):
            for raw in list(body.get("ports") or []) + list(body.get("expose") or []):
                p = _parse_declared_port(str(raw))
                if p:
                    ports.add(p)
        out[str(svc)] = ports
        if isinstance(body, dict) and body.get("container_name"):
            out[str(body["container_name"])] = ports
    return out


def _split_target(tgt: str) -> tuple[str, str | None]:
    """`host:port` → (host, port)。去掉 scheme。"""
    value = tgt.split("://", 1)[-1]
    if value.startswith("["):  # IPv6 字面量
        host, _, rest = value[1:].partition("]")
        port = rest.lstrip(":") or None
        return host, port
    if ":" in value:
        host, port = value.rsplit(":", 1)
        return host, port
    return value, None


def check_dead_targets() -> list[str]:
    """C: 死目标守卫（host 必须是 compose 服务，且端口必须被该服务声明）。"""
    allow = load_allowlist()
    problems: list[str] = []

    for config_rel, compose_rels in CONFIG_COMPOSE_MAP.items():
        config_abs = REPO / config_rel
        if not config_abs.is_file():
            continue
        doc = yaml.safe_load(config_abs.read_text()) or {}

        known: dict[str, set[str]] = {}
        for c in compose_rels:
            for name, ports in compose_service_ports(c).items():
                known.setdefault(name, set()).update(ports)

        for job in doc.get("scrape_configs") or []:
            job_name = job.get("job_name", "<unnamed>")
            targets: list[str] = []
            for sc in job.get("static_configs") or []:
                targets += [str(t) for t in (sc.get("targets") or [])]

            if (
                not targets
                and not job.get("file_sd_configs")
                and not job.get("dns_sd_configs")
            ):
                problems.append(
                    f"{config_rel} job `{job_name}` 没有任何抓取目标来源"
                    "（static_configs / file_sd_configs / dns_sd_configs 全为空）"
                )

            for tgt in targets:
                host, port = _split_target(tgt)
                if host in SELF_TARGETS:
                    continue
                if tgt in allow or host in allow:
                    continue

                if host not in known:
                    problems.append(
                        f"{config_rel} job `{job_name}` 的目标 `{tgt}` 指向 `{host}`，"
                        "但它在 "
                        + " / ".join(compose_rels)
                        + " 里都不是 service/container_name。\n"
                        "      → Prometheus 不会因此报错，只会把该目标永久标 `down`，\n"
                        "        依赖它的面板静默变空（coturn 就是这样长期 down）。\n"
                        "      修法：① 在 compose 中补该服务；或 ② 删掉这个 job；\n"
                        f"      或 ③ 若确为外部/独立栈目标，登记到 {ALLOWLIST.relative_to(REPO)}"
                        "（须写明理由）。"
                    )
                    continue

                # 端口级校验：服务存在 ≠ 端口有监听者。
                if port is not None and port not in known[host]:
                    declared = sorted(known[host]) or ["（未声明任何端口）"]
                    problems.append(
                        f"{config_rel} job `{job_name}` 的目标 `{tgt}`：服务 `{host}` 存在，"
                        f"但它并未声明端口 {port}（声明的只有 {declared}）。\n"
                        "      → 该端口上没有监听者（例如 postgres_exporter 不在 9187、\n"
                        "        redis_exporter 不在 9121），目标会永久 `down` 而无人察觉。\n"
                        "      修法：在 compose 中补对应 exporter 服务并声明该端口，或删掉这个 job。"
                    )
    return problems


def job_names(config_rel: str) -> list[str]:
    doc = yaml.safe_load((REPO / config_rel).read_text()) or {}
    return [str(j.get("job_name")) for j in (doc.get("scrape_configs") or [])]


def check_drift() -> list[str]:
    """D: 变体配置不得静默漂移。"""
    problems: list[str] = []
    for variant_rel, base_rel in DRIFT_PAIRS:
        if not (REPO / variant_rel).is_file() or not (REPO / base_rel).is_file():
            continue
        base = set(job_names(base_rel))
        variant = set(job_names(variant_rel))
        extra = {j for j in variant - base if not j.startswith("mock-")}
        missing = base - variant
        if extra:
            problems.append(
                f"{variant_rel} 相对 {base_rel} 多出非 mock-* 的 job：{sorted(extra)}\n"
                f"      → 变体只允许**新增 mock-\\* job**；其它差异会让两个文件各自漂移\n"
                f"        （本仓的『配置双副本』老毛病）。请把公共部分收敛到基准文件。"
            )
        if missing:
            problems.append(
                f"{variant_rel} 相对 {base_rel} 缺少 job：{sorted(missing)}\n"
                "      → 变体必须完整包含基准的 job，否则 dev 环境会静默少采集。"
            )
    return problems


def check_external_label_kind() -> list[str]:
    """E: 守住 `metric_relabel_configs` 的 keep/drop 语义陷阱。

    只做定点检查：`source_labels: [__name__]` 且 `action: keep` 时，
    若同时存在"过滤噪音指标"意图的注释，则几乎一定是写反了 —— 因为 keep +
    Go 运行时前缀会把应用指标全部丢掉。这里直接禁止该组合出现在本仓配置里。
    """
    problems: list[str] = []
    for config_rel in list(CONFIG_COMPOSE_MAP) + [
        "docker/deploy/prometheus/prometheus.yml"
    ]:
        path = REPO / config_rel
        if not path.is_file():
            continue
        doc = yaml.safe_load(path.read_text()) or {}
        for job in doc.get("scrape_configs") or []:
            for rc in job.get("metric_relabel_configs") or []:
                if not isinstance(rc, dict):
                    continue
                src = [str(s) for s in (rc.get("source_labels") or [])]
                if src != ["__name__"]:
                    continue
                if str(rc.get("action", "replace")) != "keep":
                    continue
                regex = str(rc.get("regex", ""))
                if re.search(r"go_\.\*|process_\.\*|promhttp_\.\*", regex):
                    problems.append(
                        f"{config_rel} job `{job.get('job_name')}` 用 "
                        f"`action: keep` + `regex: {regex}` 过滤 `__name__`。\n"
                        "      → 这会把**所有应用指标**（synapse_* / http_requests_total / …）\n"
                        "        全部丢掉，只留 Go 运行时指标 —— 与『过滤掉不需要的』意图相反，\n"
                        "        且面板恒 No data 而不报错。改成 `action: drop`。"
                    )
    return problems


# ——— G. 应用 /metrics 暴露面不变量 —————————————————————————————————————
#
# `docker/config/homeserver.yaml` 的 `prometheus.public: true` 表达的是"匿名可抓"，
# **表达不了"仅授权抓取"**（没有 bearer / mTLS / CIDR allowlist 这些手段）。
# 于是"公网不可达"只由两条隐性事实保证：核心 compose 只把应用的 metrics 端口绑到回环、
# nginx 不转发它。这两条事实写在注释里，谁都可能在某次改动中无意破坏，且没有任何门禁会红。
# 这里把它们变成可检查的不变量。
APP_COMPOSE = "docker/deploy/docker-compose.yml"
HOMESERVER_CONFIG = "docker/config/homeserver.yaml"
NGINX_ROOT = "docker/deploy/nginx"
LOOPBACK_HOSTS = {"127.0.0.1", "::1", "[::1]", "localhost"}

_UPSTREAM_RE = re.compile(r"upstream\s+([^\s{]+)\s*\{([^}]*)\}", re.DOTALL)
_SERVER_RE = re.compile(r"^\s*server\s+([^\s;]+)", re.MULTILINE)
_PROXY_PASS_RE = re.compile(r"proxy_pass\s+([^;]+);")


def app_metrics_port() -> str:
    """应用 metrics 端口**从 homeserver.yaml 派生**，不硬编码。"""
    doc = yaml.safe_load((REPO / HOMESERVER_CONFIG).read_text()) or {}
    prom = doc.get("prometheus")
    if not isinstance(prom, dict):
        raise Failure(
            f"{HOMESERVER_CONFIG} 里没有 `prometheus` 段 —— 无法确定应用 metrics 端口。\n"
            "      → 本门禁不允许硬编码默认值：端口改了而门禁没跟上，正是要防的那类静默失效。"
        )
    port = prom.get("port")
    if not isinstance(port, int):
        raise Failure(
            f"{HOMESERVER_CONFIG} 的 `prometheus.port` 不是整数（实际：{port!r}），"
            "无法用于暴露面检查。"
        )
    return str(port)


def app_hosts() -> set[str]:
    """应用在 compose / nginx 里可能被引用到的全部主机名。"""
    doc = yaml.safe_load((REPO / APP_COMPOSE).read_text()) or {}
    for svc, body in (doc.get("services") or {}).items():
        if (
            isinstance(body, dict)
            and str(body.get("container_name", "")) == "synapse-app"
        ):
            return {str(svc), "synapse-app"} | LOOPBACK_HOSTS
    raise Failure(
        f"{APP_COMPOSE} 里找不到 `container_name: synapse-app` 的服务 ——\n"
        "      → 没有它就无法判断哪些 nginx/代理目标指向应用；不允许退回空集合静默通过。"
    )


def _parse_port_binding(raw: object) -> tuple[str | None, str | None]:
    """解析 compose `ports` 的短/长语法 → (host_ip, 容器端口)。

    返回 `host_ip=None` 表示**未指定绑定地址**（Docker 默认绑所有接口），
    调用方必须把它当作"对外暴露"而不是"不知道"。
    """
    if isinstance(raw, dict):
        host_ip = raw.get("host_ip")
        target = raw.get("target")
        if target is None:
            return (str(host_ip) if host_ip else None, None)
        return (
            str(host_ip) if host_ip else None,
            str(target).split("-", 1)[0].strip(),
        )

    spec = str(raw).split("/", 1)[0].strip()
    if not spec:
        return (None, None)
    if spec.startswith("["):  # IPv6 字面量：`[::1]:9090:9090`
        host, _, rest = spec[1:].partition("]")
        tail = rest.lstrip(":").split(":") if rest.lstrip(":") else []
        return (f"[{host}]", tail[-1] if tail else None)
    parts = spec.split(":")
    if len(parts) == 1:
        return (None, parts[0])
    if len(parts) == 2:
        return (None, parts[1])
    # len >= 3：注意 `127.0.0.1:${PROMETHEUS_PORT:-9090}:9090` 里的 `:-` 也含冒号，
    # 所以只能取**首段作 host_ip、末段作容器端口**，不能按字段位置索引。
    return (parts[0], parts[-1])


def _exposed_bindings(doc: dict, port: str) -> list[str]:
    """列出把容器端口 `port` 发布到非回环地址的服务。"""
    offenders: list[str] = []
    for svc, body in (doc.get("services") or {}).items():
        if not isinstance(body, dict):
            continue
        for raw in body.get("ports") or []:
            host_ip, container = _parse_port_binding(raw)
            if container != port:
                continue
            if host_ip is not None and host_ip in LOOPBACK_HOSTS:
                continue
            where = host_ip or "0.0.0.0（未写 host_ip ⇒ Docker 绑定所有接口）"
            offenders.append(f"{svc} → {where}")
    return offenders


def _upstream_servers(text: str) -> dict[str, list[tuple[str, str | None]]]:
    out: dict[str, list[tuple[str, str | None]]] = {}
    for match in _UPSTREAM_RE.finditer(text):
        name = match.group(1)
        servers = [
            _split_proxy_target(m.group(1)) for m in _SERVER_RE.finditer(match.group(2))
        ]
        out[name] = servers
    return out


def _split_proxy_target(raw: str) -> tuple[str, str | None]:
    """`http://host:port/path` → (host, port)。无端口时 port=None。"""
    value = raw.strip().split("://", 1)[-1]
    hostport = value.split("/", 1)[0]
    if hostport.startswith("["):
        host, _, rest = hostport[1:].partition("]")
        return (host, rest.lstrip(":").split(":")[0] or None)
    if ":" in hostport:
        host, port = hostport.rsplit(":", 1)
        return (host, port or None)
    return (hostport, None)


def _nginx_metrics_exposures(text: str, port: str, hosts: set[str]) -> list[str]:
    """列出把应用 metrics 端口代理到公网的 `proxy_pass` 目标。

    `proxy_pass http://<裸 upstream 名>` 必须解析 upstream 块后再判定 ——
    本仓的 `synapse_backend` 指向 `synapse:8008`，与 metrics 无关；
    而 `prometheus.conf` 里指向 `synapse-prometheus:9090` 的**是监控栈自己的**
    Prometheus，也不能因为"端口等于 9090"就一刀切判违规。
    """
    upstreams = _upstream_servers(text)
    offenders: list[str] = []
    for match in _PROXY_PASS_RE.finditer(text):
        target = match.group(1).strip()
        host, target_port = _split_proxy_target(target)
        if target_port is not None:
            if host in hosts and target_port == port:
                offenders.append(target)
            continue
        for srv_host, srv_port in upstreams.get(host, []):
            if srv_host in hosts and srv_port == port:
                offenders.append(f"{target}（upstream 解析为 {srv_host}:{srv_port}）")
    return offenders


def _self_test_metrics_exposure(port: str, hosts: set[str]) -> list[str]:
    """正反例自检：判定逻辑本身失效时，本轮结论不可信，必须先红。"""
    probe = hosts | {"synapse-app"}
    failures: list[str] = []

    def expect_compose(label: str, ports: list[object], want: int) -> None:
        got = len(_exposed_bindings({"services": {"synapse": {"ports": ports}}}, port))
        if got != want:
            failures.append(f"compose 自检 `{label}` 期望 {want} 条违规，实得 {got} 条")

    def expect_nginx(label: str, text: str, want: int) -> None:
        got = len(_nginx_metrics_exposures(text, port, probe))
        if got != want:
            failures.append(f"nginx 自检 `{label}` 期望 {want} 条违规，实得 {got} 条")

    # 正例：这些写法都必须被识别为"对外暴露"
    expect_compose("裸端口映射", [f"{port}:{port}"], 1)
    expect_compose("显式 0.0.0.0", [f"0.0.0.0:{port}:{port}"], 1)
    expect_compose(
        "长语法无 host_ip", [{"target": int(port), "published": int(port)}], 1
    )
    expect_compose(
        "长语法内网 IP",
        [{"target": int(port), "published": int(port), "host_ip": "192.168.1.5"}],
        1,
    )
    expect_nginx(f"代理到应用主机:{port}", f"proxy_pass http://synapse-app:{port};", 1)
    expect_nginx(
        "裸 upstream 指向应用 metrics",
        f"upstream metrics_pool {{ server synapse-app:{port}; }}\n"
        "proxy_pass http://metrics_pool;",
        1,
    )

    # 反例：这些是当前仓里的**正确**写法，不得误报
    expect_compose("回环绑定", [f"127.0.0.1:{port}:{port}"], 0)
    expect_compose(
        "回环 + 环境变量插值", [f"127.0.0.1:${{PROMETHEUS_PORT:-{port}}}:{port}"], 0
    )
    expect_compose("其它端口", ["8008:8008"], 0)
    expect_nginx(
        "监控栈自己的 Prometheus",
        f"proxy_pass http://synapse-prometheus:{port}/api/v1/;",
        0,
    )
    expect_nginx(
        "upstream 指向 API 端口",
        "upstream synapse_backend { server synapse:8008; }\n"
        "proxy_pass http://synapse_backend;",
        0,
    )
    return failures


def check_metrics_exposure() -> list[str]:
    """G: 应用 /metrics 只能绑回环，且不得被 nginx 反向代理出去。"""
    try:
        port = app_metrics_port()
        hosts = app_hosts()
    except Failure as e:
        return [str(e)]

    problems = _self_test_metrics_exposure(port, hosts)
    if problems:
        return ["门禁自检失败 —— 正反例未按预期判定，本轮暴露面结论不可信："] + problems

    path = REPO / APP_COMPOSE
    if not path.is_file():
        problems.append(f"{APP_COMPOSE}: 文件不存在")
    else:
        doc = yaml.safe_load(path.read_text()) or {}
        for offender in _exposed_bindings(doc, port):
            problems.append(
                f"{APP_COMPOSE} 的 `{offender}` 把容器端口 {port}（应用 /metrics）"
                "发布到了非回环地址。\n"
                "      → homeserver.yaml 里 `prometheus.public: true` 表示该端点**无鉴权**，\n"
                "        发布即等于公网可读全部运维指标。只允许绑 127.0.0.1；\n"
                "        跨机抓取请让抓取端直连内网地址，而不是在这里发布端口。"
            )

    for conf in sorted((REPO / NGINX_ROOT).rglob("*.conf")):
        rel = conf.relative_to(REPO)
        for target in _nginx_metrics_exposures(conf.read_text(), port, hosts):
            problems.append(
                f"{rel} 里 `proxy_pass {target}` 指向应用 /metrics（端口 {port}）。\n"
                "      → nginx 是公网入口，这一条等于给无鉴权的 /metrics 开对外通道；\n"
                "        `synapse-prometheus` 的 9090 是监控栈自身，与应用 metrics 无关。"
            )
    return problems


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--json-report", help="把结果写一份 JSON 报告")
    args = parser.parse_args()

    all_problems: list[str] = []
    sections: list[tuple[str, list[str]]] = []

    try:
        kind, prefix = resolve_promtool()
    except Failure as e:
        print(f"FAIL: {e}", file=sys.stderr)
        return 1

    desc = f"docker 镜像 {prefix}" if kind == "docker" else f"本地 promtool ({prefix})"
    print(f"promtool: {desc}")
    try:
        am_kind, am_prefix = resolve_amtool(kind)
    except Failure as e:
        print(f"FAIL: {e}", file=sys.stderr)
        return 1
    am_desc = (
        f"docker 镜像 {am_prefix}"
        if am_kind == "docker"
        else f"本地 amtool ({am_prefix})"
    )
    print(f"amtool:   {am_desc}")
    print()

    for config_rel in CONFIG_COMPOSE_MAP:
        sections.append(
            (f"A. check config  {config_rel}", check_config(kind, prefix, config_rel))
        )
    for rule_rel in RULE_FILES:
        sections.append(
            (f"B. check rules   {rule_rel}", check_rules(kind, prefix, rule_rel))
        )
    sections.append(
        ("C. 死目标守卫（目标必须在 compose 中存在）", check_dead_targets())
    )
    # DRIFT_PAIRS 为空时不假装"检查过且通过"：标题写明不适用，免得下一个人以为这道守卫在保护什么。
    sections.append(
        (
            "D. 变体漂移守卫（无变体配置 ⇒ 不适用）"
            if not DRIFT_PAIRS
            else "D. 变体漂移守卫（变体 = 基准 + 已知增量）",
            check_drift(),
        )
    )
    sections.append(("E. keep/drop 语义守卫", check_external_label_kind()))
    sections.append(
        (
            "F. alertmanager 配置（amtool check-config）",
            check_alertmanager_configs(am_kind, am_prefix),
        )
    )
    sections.append(
        (
            "G. 应用 /metrics 暴露面不变量（只绑回环 + nginx 不转发）",
            check_metrics_exposure(),
        )
    )

    for title, problems in sections:
        status = "OK" if not problems else f"FAIL（{len(problems)} 项）"
        print(f"[{status}] {title}")
        for p in problems:
            print(f"  ✗ {p}")
        all_problems += problems

    print()
    if all_problems:
        print(
            f"Prometheus 配置门禁：FAIL（共 {len(all_problems)} 项）", file=sys.stderr
        )
    else:
        print("Prometheus 配置门禁：PASS")

    if args.json_report:
        report = {
            "status": "fail" if all_problems else "pass",
            "problems": all_problems,
            "sections": [{"title": t, "problems": p} for t, p in sections],
            "promtool": desc,
        }
        Path(args.json_report).write_text(
            json.dumps(report, ensure_ascii=False, indent=2)
        )
        print(f"报告：{args.json_report}")

    return 1 if all_problems else 0


if __name__ == "__main__":
    sys.exit(main())
