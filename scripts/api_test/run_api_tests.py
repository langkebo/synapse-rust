#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
=============================================================================
Synapse-Rust 全量 API 路由健康测试执行器
=============================================================================

特性：
  1. 自动遍历全部已定义 API 路由（来自 RouteLedger 导出的路由清单 JSON，
     覆盖 client-server / admin / federation / vendor 等全部路由）
  2. 逐路由发送请求，记录 HTTP 状态码、响应耗时、响应体结构与内容
  3. 自动校验：
       - 状态码是否符合预期（自动学习端点是否需要认证）
       - 响应体 JSON 字段完整性 + 类型正确性（expectations.yaml 精确规则）
       - 关键业务异常值（如 /health 必须 healthy、versions 不能为空）
  4. 汇总报告：通过/失败统计、失败接口详细信息（URL/参数/实际vs预期差异）、
     整体接口健康度评分（JSON + Markdown + 自包含 HTML）
  5. 支持配置基础 URL、认证 Token、环境变量与命令行参数，
     便于在 dev / test / prod 多环境运行

用法示例：
    # 使用 config.yaml 默认配置（https://matrix.test）
    python3 run_api_tests.py

    # 指定环境与地址
    python3 run_api_tests.py --env prod --base-url https://example.com

    # 手动指定 token（跳过自动登录），并开启写操作探测
    python3 run_api_tests.py --token "syt_xxx" --allow-write

    # 导出最新路由清单后测试（首次运行建议：需要 cargo 编译，耗时较长）
    python3 run_api_tests.py --export-ledger

    # 只测某个模块（registered_by 过滤，如 admin）
    python3 run_api_tests.py --only-module admin

退出码：0 = 无 FAIL；1 = 存在 FAIL；2 = 运行错误
=============================================================================
"""

from __future__ import annotations

import argparse
import concurrent.futures
import json
import os
import re
import ssl
import subprocess
import sys
import time
import urllib.parse
from dataclasses import dataclass, field
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

import requests
import yaml

# ---------------------------------------------------------------------------
# 常量
# ---------------------------------------------------------------------------
SCRIPT_DIR = Path(__file__).resolve().parent
PROJECT_ROOT = SCRIPT_DIR.parent.parent
DEFAULT_CONFIG = SCRIPT_DIR / "config.yaml"
DEFAULT_EXPECTATIONS = SCRIPT_DIR / "expectations.yaml"
DEFAULT_LEDGER = PROJECT_ROOT / "tests" / "unit" / "fixtures" / "ledger_export" / "default.json"

VERDICT_PASS = "PASS"
VERDICT_WARN = "WARN"
VERDICT_FAIL = "FAIL"
VERDICT_SKIP = "SKIP"

# 需要认证的端点的典型响应码（自动学习也使用同样集合）
AUTH_REJECT_CODES = {401, 403}
# 路由存活但语义 4xx（占位符资源不存在 / 参数校验失败 / 方法不允许）→ 视为可用
ACCEPTABLE_4XX = {400, 404, 405, 406, 409, 410, 422}
# 写操作（默认只做匿名探测，防止数据副作用）
WRITE_METHODS = {"POST", "PUT", "PATCH", "DELETE"}
SAFE_METHODS = {"GET", "HEAD", "OPTIONS"}

ENV_OVERRIDES = {
    "env": "API_TEST_ENV",
    "base_url": "API_TEST_BASE_URL",
    "auth_token": "API_TEST_TOKEN",
    "verify_tls": "API_TEST_VERIFY_TLS",
    "timeout": "API_TEST_TIMEOUT",
    "concurrency": "API_TEST_CONCURRENCY",
    "allow_write": "API_TEST_ALLOW_WRITE",
}


# ---------------------------------------------------------------------------
# 数据结构
# ---------------------------------------------------------------------------
@dataclass
class ProbeSpec:
    """单个请求探测（匿名 / 认证）的定义。"""

    label: str                # anonymous | authed
    method: str
    url: str                  # 已实例化的完整 URL（不含 base_url）
    body: Optional[str] = None
    is_write: bool = False


@dataclass
class ProbeResult:
    """单个探测的执行结果。"""

    spec: ProbeSpec
    verdict: str = VERDICT_FAIL
    http_status: Optional[int] = None
    duration_ms: float = 0.0
    content_type: str = ""
    body: str = ""
    parsed_json: Optional[Any] = None
    checks: List[str] = field(default_factory=list)   # 人类可读的校验说明
    issues: List[str] = field(default_factory=list)   # 失败/警告原因
    error: Optional[str] = None                       # 连接错误等


@dataclass
class CaseResult:
    """一条路由的测试结果（含一个或多个探测）。"""

    method: str
    path: str
    registered_by: str
    probes: List[ProbeResult] = field(default_factory=list)

    @property
    def verdict(self) -> str:
        """端点级判定：最差探测决定。SKIP 优先，其次 FAIL，再 WARN，最后 PASS。"""
        if not self.probes:
            return VERDICT_SKIP
        order = {VERDICT_FAIL: 0, VERDICT_WARN: 1, VERDICT_PASS: 2, VERDICT_SKIP: 3}
        return min(self.probes, key=lambda p: order.get(p.verdict, 0)).verdict

    @property
    def max_duration_ms(self) -> float:
        return max((p.duration_ms for p in self.probes), default=0.0)

    def all_issues(self) -> List[str]:
        out: List[str] = []
        for p in self.probes:
            for i in p.issues:
                out.append(f"[{p.label} {p.spec.method}] {i}")
        return out


# ---------------------------------------------------------------------------
# 配置加载
# ---------------------------------------------------------------------------
def load_yaml(path: Path) -> dict:
    if not path.exists():
        return {}
    with open(path, "r", encoding="utf-8") as f:
        return yaml.safe_load(f) or {}


def _coerce_bool(v: Any) -> bool:
    if isinstance(v, bool):
        return v
    return str(v).strip().lower() in {"1", "true", "yes", "on"}


def load_config(cli: argparse.Namespace) -> dict:
    cfg = load_yaml(DEFAULT_CONFIG)
    # 环境变量覆盖
    for key, env in ENV_OVERRIDES.items():
        if os.environ.get(env) is not None:
            raw = os.environ[env]
            cfg[key] = _coerce_bool(raw) if key in {"verify_tls", "allow_write"} else raw
    # 命令行覆盖
    if cli.env:
        cfg["env"] = cli.env
    if cli.base_url:
        cfg["base_url"] = cli.base_url.rstrip("/")
    if cli.token:
        cfg["auth_token"] = cli.token
    if cli.verify_tls is not None:
        cfg["verify_tls"] = cli.verify_tls
    if cli.timeout:
        cfg["timeout"] = float(cli.timeout)
    if cli.concurrency:
        cfg["concurrency"] = int(cli.concurrency)
    if cli.allow_write:
        cfg["allow_write"] = True
    if cli.ledger:
        cfg["ledger_path"] = cli.ledger
    if cli.report_dir:
        cfg["report_dir"] = cli.report_dir
    # 归一化
    cfg.setdefault("verify_tls", True)
    cfg.setdefault("timeout", 10.0)
    cfg.setdefault("concurrency", 8)
    cfg.setdefault("request_delay", 0.0)
    cfg.setdefault("max_response_bytes", 4096)
    cfg.setdefault("allow_write", False)
    cfg.setdefault("path_params", {})
    # TLS 校验目标：True=系统信任库；也可为字符串（CA bundle 路径）
    # 自动探测 mkcert 根 CA（开发/测试环境常用 mkcert 签发受信证书）
    if cfg.get("verify_tls") is True:
        mkcert_ca = _find_mkcert_ca()
        if mkcert_ca:
            cfg["verify_tls"] = mkcert_ca
    # prod 环境强制禁止写探测
    if cfg.get("env") == "prod":
        cfg["allow_write"] = False
    return cfg


def _find_mkcert_ca() -> Optional[str]:
    """定位 mkcert 根 CA（rootCA.pem），供 requests 校验自签/本地 CA 证书。"""
    candidates: List[Path] = []
    caroot = os.environ.get("MKCERT_CAROOT")
    if caroot:
        candidates.append(Path(caroot) / "rootCA.pem")
    try:
        proc = subprocess.run(
            ["mkcert", "-CAROOT"], capture_output=True, text=True, timeout=10
        )
        if proc.returncode == 0:
            candidates.append(Path(proc.stdout.strip()) / "rootCA.pem")
    except (FileNotFoundError, subprocess.SubprocessError):
        pass
    # macOS 常见位置兜底
    candidates.append(Path.home() / "Library" / "Application Support" / "mkcert" / "rootCA.pem")
    for c in candidates:
        if c.exists():
            return str(c)
    return None


# ---------------------------------------------------------------------------
# 路由清单加载（RouteLedger 导出 JSON）
# ---------------------------------------------------------------------------
def export_ledger(profile: str = "default") -> Optional[Path]:
    """调用 cargo 编译并运行 synapse_ledger_export，返回导出 JSON 路径。

    使用与 Docker 镜像一致的 features（server,core-private-chat,widgets,
    external-services,voice-extended,cas-sso,saml-sso,friends）。
    """
    features = "server,core-private-chat,widgets,external-services,voice-extended,cas-sso,saml-sso,friends"
    out = SCRIPT_DIR / "reports" / f"ledger_{profile}_{datetime.now().strftime('%Y%m%d_%H%M%S')}.json"
    out.parent.mkdir(parents=True, exist_ok=True)
    cmd = [
        "cargo", "run", "--quiet", "--no-default-features",
        "--features", features,
        "--bin", "synapse_ledger_export",
        "--", f"--profile={profile}", f"--output={out}",
    ]
    print(f"[ledger] 编译并导出路由清单（首次较慢）：{' '.join(cmd)}")
    t0 = time.time()
    proc = subprocess.run(cmd, cwd=str(PROJECT_ROOT), capture_output=True, text=True, timeout=3600)
    if proc.returncode != 0:
        print(f"[ledger] 导出失败（exit={proc.returncode}）：\n{proc.stderr[-2000:]}", file=sys.stderr)
        return None
    print(f"[ledger] 导出完成（{time.time() - t0:.1f}s）：{out}")
    return out


def resolve_ledger(cfg: dict) -> Optional[Path]:
    raw = cfg.get("ledger_path") or str(DEFAULT_LEDGER)
    p = Path(raw)
    if not p.is_absolute():
        p = (SCRIPT_DIR / p).resolve()
    if not p.exists():
        # 回退到默认 fixture
        if DEFAULT_LEDGER.exists():
            print(f"[ledger] 指定清单不存在，回退默认：{DEFAULT_LEDGER}")
            return DEFAULT_LEDGER
        return None
    return p


def load_ledger(path: Path) -> List[dict]:
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)
    entries = data.get("entries") or data.get("routes") or []
    print(f"[ledger] 加载路由清单：{path.name}（{len(entries)} 条 (method, path)）")
    return entries


# ---------------------------------------------------------------------------
# 路径实例化
# ---------------------------------------------------------------------------
PLACEHOLDER_PATTERN = re.compile(r"\{([^}]+)\}")
GENERIC_VALUE = "apitest-placeholder"

# 参数名 → 值（与 config.yaml path_params 合并，这里提供启发式兜底）
DEFAULT_PARAM_VALUES: Dict[str, str] = {
    "user_id": "@testuser1:matrix.test",
    "user": "testuser1",
    "user_name": "testuser1",
    "room_id": "!apitest-nosuchroom:matrix.test",
    "room": "!apitest-nosuchroom:matrix.test",
    "room_alias": "#apitest-nosuchalias:matrix.test",
    "event_id": "$apitest-nosuchevent:matrix.test",
    "txn_id": "apitest-txn-00000000",
    "transaction_id": "apitest-txn-00000000",
    "device_id": "APITESTDEVICE0000",
    "filter_id": "apitest-filter-0000",
    "key_name": "com.example.apitest",
    "tag": "m.favourite",
    "scope": "global",
    "media_id": "doesnotexist",
    "server_name": "matrix.test",
    "sender": "@testuser1:matrix.test",
    "thread_id": "$apitest-nosuchevent:matrix.test",
    "token": "apitest-token-0000",
    "id": "apitest-0000",
    "name": "apitest-name",
    "network_id": "apitest-network",
    "appservice_id": "apitest-as",
}


def instantiate_path(path: str, param_values: Dict[str, str]) -> Tuple[str, Optional[str]]:
    """把 /rooms/{room_id}/messages 实例化为可请求路径。

    返回 (url, error)。无法实例化时 error 非空（如未知参数、参数值含非法字符）。
    """
    def repl(m: re.Match) -> str:
        name = m.group(1)
        val = param_values.get(name, DEFAULT_PARAM_VALUES.get(name, GENERIC_VALUE))
        # 参数值必须无空格/斜杠/花括号，避免污染路径结构
        if re.search(r"[\s/{}\\]", val):
            raise ValueError(f"参数 {{{name}}} 的值不安全：{val!r}")
        return val

    try:
        return PLACEHOLDER_PATTERN.sub(repl, path), None
    except ValueError as e:
        return path, str(e)


# ---------------------------------------------------------------------------
# 自动登录
# ---------------------------------------------------------------------------
def auto_login(cfg: dict, base_url: str, verify: Any, timeout: float) -> Tuple[Optional[str], Optional[str]]:
    """尝试用普通用户凭据登录获取 token。返回 (token, error)。"""
    username = (cfg.get("auth") or {}).get("username")
    password = (cfg.get("auth") or {}).get("password")
    if not username or not password:
        return None, "未配置 auth.username/password"
    payload = {"type": "m.login.password", "identifier": {"type": "m.id.user", "user": username}, "password": password}
    try:
        resp = requests.post(
            f"{base_url}/_matrix/client/v3/login", json=payload, timeout=timeout, verify=verify
        )
        if resp.status_code == 200:
            token = (resp.json() or {}).get("access_token")
            if token:
                return token, None
            return None, f"登录返回 200 但缺少 access_token：{resp.text[:200]}"
        return None, f"登录失败 HTTP {resp.status_code}：{resp.text[:200]}"
    except requests.RequestException as e:
        return None, f"登录请求异常：{e}"


def auto_login_admin(cfg: dict, base_url: str, verify: Any, timeout: float) -> Tuple[Optional[str], Optional[str]]:
    admin_cfg = cfg.get("admin") or {}
    username = admin_cfg.get("username")
    password = admin_cfg.get("password")
    if not username or not password:
        return None, "未配置 admin.username/password"
    payload = {"type": "m.login.password", "identifier": {"type": "m.id.user", "user": username}, "password": password}
    try:
        resp = requests.post(
            f"{base_url}/_matrix/client/v3/login", json=payload, timeout=timeout, verify=verify
        )
        if resp.status_code == 200:
            token = (resp.json() or {}).get("access_token")
            if token:
                return token, None
            return None, f"admin 登录返回 200 但缺少 access_token"
        return None, f"admin 登录失败 HTTP {resp.status_code}"
    except requests.RequestException as e:
        return None, f"admin 登录请求异常：{e}"


# ---------------------------------------------------------------------------
# 测试计划生成
# ---------------------------------------------------------------------------
# 认证方式不适用 bearer token 的端点前缀：
#   - federation 路由需联邦签名认证（X-Matrix 签名），非 bearer
#   - appservice 路由需 as_token（hs_token），非用户 token
#   - download_signed 需签名 URL（MSC3916）
# 这些端点只做匿名探测（验证路由存活 + 鉴权边界），不做 authed 探测。
AUTH_BEARER_INAPPLICABLE_PREFIXES = (
    "/_matrix/federation/",
    "/_synapse/federation/",
    "/_matrix/app/",
    "/_matrix/media/v3/download_signed",
)


def build_plan(entries: List[dict], cfg: dict) -> Tuple[List[CaseResult], List[str]]:
    """为每条路由生成探测计划。返回 (cases, skipped_reasons)。"""
    cases: List[CaseResult] = []
    skipped: List[str] = []
    param_values = cfg.get("path_params") or {}
    allow_write = bool(cfg.get("allow_write"))
    token = cfg.get("_token")
    admin_token = cfg.get("_admin_token")

    for e in entries:
        method = (e.get("method") or "GET").upper()
        path = e.get("path") or ""
        registered_by = e.get("registered_by") or "unknown"

        url, err = instantiate_path(path, param_values)
        if err:
            skipped.append(f"{method} {path} — {err}")
            continue

        probes: List[ProbeResult] = []
        is_write = method in WRITE_METHODS
        bearer_inapplicable = url.startswith(AUTH_BEARER_INAPPLICABLE_PREFIXES)

        # 1) 匿名探测（所有路由，验证「鉴权边界 + 路由存活 + 无 5xx」）
        probes.append(ProbeSpec(
            label="anonymous", method=method, url=url,
            body="{}" if is_write and method not in {"DELETE"} else None,
            is_write=False,
        ))

        # 2) 认证探测（无副作用方法：GET/HEAD；管理员端点使用 admin token）
        #    对 bearer 不适用的端点跳过（federation 签名 / appservice token）
        if method in SAFE_METHODS and token and not bearer_inapplicable:
            if "/_synapse/admin" in url or re.search(r"/_matrix/client/(?:r0|v1|v3)/admin", url):
                use_token = admin_token or token
            else:
                use_token = token
            probes.append(ProbeSpec(label="authed", method=method, url=url, body=None, is_write=False))

        # 3) 写操作认证探测（仅在 --allow-write 时，空 body 预期 4xx）
        if is_write and allow_write and token and not bearer_inapplicable:
            probes.append(ProbeSpec(
                label="authed-write", method=method, url=url,
                body="{}" if method != "DELETE" else None,
                is_write=True,
            ))

        case = CaseResult(method=method, path=path, registered_by=registered_by)
        for spec in probes:
            case.probes.append(ProbeResult(spec=spec))
        cases.append(case)

    return cases, skipped


# ---------------------------------------------------------------------------
# 请求执行
# ---------------------------------------------------------------------------
def _build_headers(probe: ProbeSpec, token: Optional[str]) -> Dict[str, str]:
    headers = {"User-Agent": "synapse-rust-api-test/1.0"}
    if probe.body is not None:
        headers["Content-Type"] = "application/json"
    if token:
        headers["Authorization"] = f"Bearer {token}"
    return headers


def execute_probe(
    probe: ProbeSpec,
    cfg: dict,
    token: Optional[str],
) -> ProbeResult:
    res = ProbeResult(spec=probe)
    base_url = cfg["base_url"]
    # verify_tls: True=系统信任库 | False=跳过校验 | str=CA bundle 路径
    verify = cfg.get("verify_tls", True)
    timeout = float(cfg.get("timeout"))
    max_bytes = int(cfg.get("max_response_bytes"))

    url = f"{base_url}{probe.url}"
    t0 = time.monotonic()
    try:
        resp = requests.request(
            probe.method,
            url,
            headers=_build_headers(probe, token),
            data=probe.body,
            timeout=timeout,
            verify=verify,
            allow_redirects=False,
        )
        res.duration_ms = (time.monotonic() - t0) * 1000
        res.http_status = resp.status_code
        res.content_type = resp.headers.get("Content-Type", "")
        raw = resp.content[: max_bytes * 2].decode("utf-8", errors="replace")
        res.body = raw[:max_bytes]
        # 尝试解析 JSON
        ct = res.content_type.lower()
        stripped = raw.lstrip()
        if "json" in ct or stripped.startswith(("{", "[")):
            try:
                res.parsed_json = json.loads(raw)
            except json.JSONDecodeError:
                res.parsed_json = None
                res.checks.append("响应非合法 JSON")
    except requests.exceptions.SSLError as e:
        res.error = f"TLS 校验失败（可用 --no-verify-tls 或配置 verify_tls=false 关闭）：{type(e).__name__}"
    except requests.exceptions.Timeout:
        res.error = f"请求超时（>{timeout}s）"
    except requests.exceptions.ConnectionError as e:
        res.error = f"连接失败：{type(e).__name__}"
    except requests.exceptions.RequestException as e:
        res.error = f"请求异常：{e}"
    return res


# ---------------------------------------------------------------------------
# 校验
# ---------------------------------------------------------------------------
def _status_is_success(status: int) -> bool:
    return 200 <= status < 300


def _status_is_5xx(status: Optional[int]) -> bool:
    return status is not None and 500 <= status < 600


def _json_type_name(v: Any) -> str:
    if v is None:
        return "null"
    if isinstance(v, bool):
        return "bool"
    if isinstance(v, int):
        return "integer"
    if isinstance(v, float):
        return "number"
    if isinstance(v, str):
        return "string"
    if isinstance(v, list):
        return "array"
    if isinstance(v, dict):
        return "object"
    return type(v).__name__


def validate_schema(result: ProbeResult, rule: dict, method: str, path: str) -> None:
    """按 expectations.yaml schemas 规则校验响应体。"""
    # 状态码
    exp_status = rule.get("status")
    if exp_status is not None and result.http_status != exp_status:
        result.issues.append(
            f"状态码不符合预期：实际 {result.http_status}，期望 {exp_status}"
        )
        return
    # 纯文本匹配
    if "expect_body_text" in rule:
        if result.body.strip() != rule["expect_body_text"]:
            result.issues.append(
                f"响应体文本不符：实际 {result.body.strip()[:120]!r}，期望 {rule['expect_body_text']!r}"
            )
        return
    # JSON 结构
    if result.parsed_json is None:
        if result.http_status is not None and _status_is_success(result.http_status):
            result.issues.append("200/2xx 响应但 body 不是合法 JSON")
        return
    exp_type = rule.get("type")
    if exp_type and _json_type_name(result.parsed_json) != exp_type:
        result.issues.append(
            f"顶层 JSON 类型不符：实际 {_json_type_name(result.parsed_json)}，期望 {exp_type}"
        )
        return
    # 字段存在 + 类型
    fields = rule.get("fields") or {}
    for fname, ftype in fields.items():
        if isinstance(result.parsed_json, dict) and fname in result.parsed_json:
            actual = _json_type_name(result.parsed_json[fname])
            if actual != ftype:
                result.issues.append(f"字段 {fname!r} 类型不符：实际 {actual}，期望 {ftype}")
        elif isinstance(result.parsed_json, dict):
            result.issues.append(f"缺少字段 {fname!r}（期望 {ftype}）")
        else:
            result.issues.append(f"顶层不是对象，无法校验字段 {fname!r}")
    # 非空约束
    for fname in rule.get("non_empty") or []:
        if isinstance(result.parsed_json, dict) and isinstance(result.parsed_json.get(fname), (list, dict, str)):
            if len(result.parsed_json[fname]) == 0:
                result.issues.append(f"字段 {fname!r} 不允许为空")


def apply_override_checks(
    result: ProbeResult, override: dict, method: str, path: str
) -> None:
    """应用 overrides 规则中的显式期望。"""
    if "expect_status" in override:
        exp = override["expect_status"]
        if result.http_status not in exp:
            result.issues.append(
                f"状态码不符合预期：实际 {result.http_status}，期望其中之一 {exp}"
            )
    if "expect_json_field" in override:
        for k, v in override["expect_json_field"].items():
            actual = (result.parsed_json or {}).get(k) if isinstance(result.parsed_json, dict) else None
            if actual != v:
                result.issues.append(f"业务字段 {k!r} 异常：实际 {actual!r}，期望 {v!r}")
    if "expect_body_text" in override:
        if result.body.strip() != override["expect_body_text"]:
            result.issues.append(
                f"响应体文本不符：实际 {result.body.strip()[:120]!r}，期望 {override['expect_body_text']!r}"
            )


def _extract_errcode(result: ProbeResult) -> Optional[str]:
    """从响应体中提取 Matrix errcode。"""
    if isinstance(result.parsed_json, dict):
        return result.parsed_json.get("errcode")
    return None


def classify_verdict(result: ProbeResult, override: dict, rule: dict) -> str:
    """根据探测结果判定 PASS/WARN/FAIL。

    自动学习规则：
      - 连接层错误 / 5xx                    → FAIL
      - 匿名探测 401/403                    → PASS（鉴权边界正确，端点需认证）
      - 匿名探测 2xx                        → PASS（公开端点可用）
      - 匿名探测其它 4xx                    → PASS（路由存活，语义错误由占位符引起）
      - 认证探测 2xx                        → PASS
      - 认证探测 401/403                    → FAIL（token 被拒 / 权限不足标记为疑点）
      - 认证探测其它 4xx                    → PASS（占位符资源不存在属预期）
      - 429                                → WARN（触发限流）
    """
    label = result.spec.label
    status = result.http_status

    # 显式规则优先
    if override and ("expect_status" in override or "expect_body_text" in override or "expect_json_field" in override):
        if result.issues:
            return VERDICT_FAIL
        return VERDICT_PASS
    if rule and "status" in rule:
        if result.issues:
            return VERDICT_FAIL
        return VERDICT_PASS

    # 连接错误
    if result.error is not None:
        return VERDICT_FAIL
    if status is None:
        return VERDICT_FAIL

    # 5xx
    if _status_is_5xx(status):
        result.issues.append(f"服务端错误 HTTP {status}（5xx 视为接口故障）")
        return VERDICT_FAIL

    # 429 限流
    if status == 429:
        result.checks.append("触发限流（429），可用性存疑")
        return VERDICT_WARN

    if label == "anonymous":
        # force_public：标注为公开的端点却拒绝匿名访问 → 疑点（WARN）
        if override.get("force_public") and status in AUTH_REJECT_CODES:
            result.issues.append("配置为公开端点，但匿名请求被 401/403 拒绝")
            return VERDICT_WARN
        if status in AUTH_REJECT_CODES:
            result.checks.append("鉴权边界正确（未带 token 被 401/403 拒绝）")
            return VERDICT_PASS
        if _status_is_success(status):
            result.checks.append("公开端点响应 2xx")
            return VERDICT_PASS
        if status in ACCEPTABLE_4XX:
            result.checks.append(f"路由存活（占位符资源/参数导致 {status}，属预期）")
            return VERDICT_PASS
        result.issues.append(f"匿名探测出现未预期状态码 {status}")
        return VERDICT_WARN

    if label in ("authed", "authed-write"):
        if _status_is_success(status):
            if label == "authed-write":
                result.checks.append("写操作返回 2xx（注意：真实写入可能发生）")
            else:
                result.checks.append("认证成功且响应 2xx")
            return VERDICT_PASS
        if status in AUTH_REJECT_CODES:
            errcode = _extract_errcode(result)
            if errcode == "M_FORBIDDEN":
                # 业务权限拒绝：占位符资源不存在 / 非成员访问房间数据等，属预期
                result.checks.append(f"业务权限拒绝（{errcode}，占位符资源/权限不足，属预期）")
                return VERDICT_PASS
            if errcode in ("M_UNKNOWN_TOKEN", "M_MISSING_TOKEN"):
                result.issues.append(f"认证探测被 {status} 拒绝（{errcode}：token 无效）")
                return VERDICT_FAIL
            # M_UNAUTHORIZED 及其它：无法确认是 token 问题还是权限语义 → 存疑
            result.checks.append(
                f"认证探测被 {status} 拒绝（errcode={errcode or '未知'}），"
                "可能为权限语义而非 token 问题，标记为存疑"
            )
            return VERDICT_WARN
        if status in ACCEPTABLE_4XX:
            result.checks.append(f"认证成功，占位符资源返回 {status}（属预期）")
            return VERDICT_PASS
        result.issues.append(f"认证探测出现未预期状态码 {status}")
        return VERDICT_WARN

    return VERDICT_WARN


# ---------------------------------------------------------------------------
# 评分
# ---------------------------------------------------------------------------
VERDICT_SCORE = {VERDICT_PASS: 1.0, VERDICT_WARN: 0.5, VERDICT_FAIL: 0.0}


def score_grade(score: float) -> str:
    if score >= 95:
        return "优秀"
    if score >= 90:
        return "良好"
    if score >= 80:
        return "一般"
    if score >= 60:
        return "较差"
    return "危险"


# ---------------------------------------------------------------------------
# 报告生成
# ---------------------------------------------------------------------------
def build_report(
    cases: List[CaseResult],
    cfg: dict,
    ledger_meta: dict,
    skipped: List[str],
    started_at: str,
    duration_total: float,
) -> dict:
    counted = [c for c in cases if c.verdict != VERDICT_SKIP]
    by_v = {
        VERDICT_PASS: sum(1 for c in cases if c.verdict == VERDICT_PASS),
        VERDICT_WARN: sum(1 for c in cases if c.verdict == VERDICT_WARN),
        VERDICT_FAIL: sum(1 for c in cases if c.verdict == VERDICT_FAIL),
        VERDICT_SKIP: sum(1 for c in cases if c.verdict == VERDICT_SKIP),
    }
    score = (
        sum(VERDICT_SCORE.get(c.verdict, 0.0) for c in counted) / len(counted) * 100
        if counted else 0.0
    )

    failures = []
    for c in cases:
        if c.verdict == VERDICT_FAIL:
            for p in c.probes:
                if p.verdict == VERDICT_FAIL:
                    failures.append({
                        "method": c.method,
                        "path": c.path,
                        "registered_by": c.registered_by,
                        "probe": p.spec.label,
                        "url": p.spec.url,
                        "request": {
                            "method": p.spec.method,
                            "body": p.spec.body,
                        },
                        "actual": {
                            "http_status": p.http_status,
                            "duration_ms": round(p.duration_ms, 1),
                            "content_type": p.content_type,
                            "body": p.body,
                            "error": p.error,
                        },
                        "expected": {
                            "http_status": (
                                "2xx 或占位符 4xx"
                                if p.spec.label != "anonymous"
                                else "2xx / 401-403（鉴权）/ 占位符 4xx"
                            ),
                            "diff": p.issues,
                        },
                    })

    warns = []
    for c in cases:
        if c.verdict == VERDICT_WARN:
            for p in c.probes:
                if p.verdict == VERDICT_WARN:
                    warns.append({
                        "method": c.method,
                        "path": c.path,
                        "registered_by": c.registered_by,
                        "http_status": p.http_status,
                        "issues": p.issues,
                    })

    # 按模块（registered_by）统计
    from collections import Counter, defaultdict
    by_module: Dict[str, Dict[str, int]] = defaultdict(lambda: {VERDICT_PASS: 0, VERDICT_WARN: 0, VERDICT_FAIL: 0, VERDICT_SKIP: 0})
    for c in cases:
        by_module[c.registered_by][c.verdict] += 1
    modules = [
        {"module": m, **counts}
        for m, counts in sorted(by_module.items(), key=lambda kv: -sum(kv[1].values()))
    ]

    # 耗时统计
    durations = [c.max_duration_ms for c in cases if c.probes]
    dur = {
        "avg_ms": round(sum(durations) / len(durations), 1) if durations else 0.0,
        "max_ms": round(max(durations), 1) if durations else 0.0,
        "p95_ms": round(sorted(durations)[int(len(durations) * 0.95)] , 1) if len(durations) >= 20 else None,
    }

    slowest = sorted(
        [{"method": c.method, "path": c.path, "duration_ms": round(c.max_duration_ms, 1)}
         for c in cases if c.probes and c.verdict != VERDICT_SKIP],
        key=lambda x: -x["duration_ms"],
    )[:10]

    return {
        "meta": {
            "tool": "run_api_tests.py",
            "generated_at": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
            "started_at": started_at,
            "duration_total_s": round(duration_total, 1),
            "env": cfg.get("env"),
            "base_url": cfg.get("base_url"),
            "ledger": ledger_meta,
            "verify_tls": cfg.get("verify_tls"),
            "timeout_s": cfg.get("timeout"),
            "concurrency": cfg.get("concurrency"),
            "allow_write": bool(cfg.get("allow_write")),
        },
        "summary": {
            "total": len(cases),
            "passed": by_v[VERDICT_PASS],
            "warned": by_v[VERDICT_WARN],
            "failed": by_v[VERDICT_FAIL],
            "skipped": by_v[VERDICT_SKIP],
            "pass_rate": round(by_v[VERDICT_PASS] / len(cases) * 100, 1) if cases else 0.0,
            "health_score": round(score, 1),
            "grade": score_grade(score),
            "probe_requests": sum(len(c.probes) for c in cases),
            "duration": dur,
        },
        "modules": modules,
        "failures": failures,
        "warnings": warns,
        "skipped_reasons": skipped,
        "slowest": slowest,
        "cases": [
            {
                "method": c.method,
                "path": c.path,
                "registered_by": c.registered_by,
                "verdict": c.verdict,
                "duration_ms": round(c.max_duration_ms, 1),
                "probes": [
                    {
                        "label": p.spec.label,
                        "verdict": p.verdict,
                        "http_status": p.http_status,
                        "duration_ms": round(p.duration_ms, 1),
                        "content_type": p.content_type,
                        "checks": p.checks,
                        "issues": p.issues,
                        "error": p.error,
                    }
                    for p in c.probes
                ],
            }
            for c in cases
        ],
    }


def render_markdown(report: dict) -> str:
    s = report["summary"]
    m = report["meta"]
    lines = [
        "# Synapse-Rust API 全量测试报告",
        "",
        f"- **环境**: `{m['env']}`  |  **目标**: `{m['base_url']}`",
        f"- **生成时间**: {m['generated_at']}  |  **总耗时**: {m['duration_total_s']}s",
        f"- **路由清单**: {m.get('ledger', {}).get('name', '-')}（{m.get('ledger', {}).get('entry_count', '-')} 条）",
        f"- **并发**: {m['concurrency']}  |  **超时**: {m['timeout_s']}s  |  **写探测**: {'开启' if m['allow_write'] else '关闭（仅匿名 401 探测）'}",
        "",
        "## 总体统计",
        "",
        "| 指标 | 值 |",
        "| --- | --- |",
        f"| 接口总数 | {s['total']} |",
        f"| ✅ 通过 | {s['passed']} |",
        f"| ⚠️ 警告 | {s['warned']} |",
        f"| ❌ 失败 | {s['failed']} |",
        f"| ⏭️ 跳过 | {s['skipped']} |",
        f"| 通过率 | {s['pass_rate']}% |",
        f"| **健康度评分** | **{s['health_score']} / 100（{s['grade']}）** |",
        f"| 请求总数 | {s['probe_requests']} 次 |",
        f"| 平均响应 | {s['duration']['avg_ms']}ms  |  最大 {s['duration']['max_ms']}ms |",
        "",
    ]
    if s["failed"]:
        lines += ["## 失败接口详情", "", "| 方法 | 路径 | 模块 | 探测 | 状态码 | 差异说明 |", "| --- | --- | --- | --- | --- | --- |"]
        for f in report["failures"]:
            diff = "; ".join(f["expected"]["diff"])[:200]
            lines.append(
                f"| {f['method']} | `{f['path']}` | {f['registered_by']} | {f['probe']} "
                f"| {f['actual']['http_status'] or f['actual']['error']} | {diff} |"
            )
        lines += [""]
    if s["warned"]:
        lines += ["## 警告接口", "", "| 方法 | 路径 | 模块 | 状态码 | 原因 |", "| --- | --- | --- | --- | --- |"]
        for w in report["warnings"]:
            lines.append(f"| {w['method']} | `{w['path']}` | {w['registered_by']} | {w['http_status']} | {'; '.join(w['issues'])[:150]} |")
        lines += [""]
    lines += ["## 按模块统计", "", "| 模块 | 总数 | 通过 | 警告 | 失败 | 跳过 |", "| --- | --- | --- | --- | --- | --- |"]
    for mod in report["modules"]:
        lines.append(
            f"| {mod['module']} | {mod[VERDICT_PASS] + mod[VERDICT_WARN] + mod[VERDICT_FAIL] + mod[VERDICT_SKIP]} "
            f"| {mod[VERDICT_PASS]} | {mod[VERDICT_WARN]} | {mod[VERDICT_FAIL]} | {mod[VERDICT_SKIP]} |"
        )
    if report["slowest"]:
        lines += ["", "## 最慢接口 TOP 10", "", "| 方法 | 路径 | 耗时(ms) |", "| --- | --- | --- |"]
        for sl in report["slowest"]:
            lines.append(f"| {sl['method']} | `{sl['path']}` | {sl['duration_ms']} |")
    lines += ["", "---", "由 `scripts/api_test/run_api_tests.py` 生成"]
    return "\n".join(lines) + "\n"


def render_html(report: dict) -> str:
    s = report["summary"]
    m = report["meta"]
    score = s["health_score"]
    grade = s["grade"]
    # 环形评分图：conic-gradient
    ring = f"conic-gradient(#22c55e 0% {score}%, #e5e7eb {score}% 100%)"
    fail_rows = ""
    for f in report["failures"]:
        diff = escape_html("; ".join(f["expected"]["diff"]))
        body = escape_html(f["actual"]["body"])[:300]
        fail_rows += f"""
        <tr>
          <td><span class="badge">{f["method"]}</span></td>
          <td><code>{escape_html(f["path"])}</code><div class="sub">{escape_html(f["registered_by"])}</div></td>
          <td>{f["probe"]}</td>
          <td class="st">{f["actual"]["http_status"] or "CONN"}</td>
          <td class="diff">{diff}<div class="sub">{body}</div></td>
        </tr>"""
    warn_rows = ""
    for w in report["warnings"][:20]:
        warn_rows += f"""
        <tr>
          <td><span class="badge">{w["method"]}</span></td>
          <td><code>{escape_html(w["path"])}</code><div class="sub">{escape_html(w["registered_by"])}</div></td>
          <td class="st">{w["http_status"]}</td>
          <td>{escape_html("; ".join(w["issues"]))}</td>
        </tr>"""
    mod_rows = ""
    for mod in report["modules"]:
        total = mod[VERDICT_PASS] + mod[VERDICT_WARN] + mod[VERDICT_FAIL] + mod[VERDICT_SKIP]
        rate = mod[VERDICT_PASS] / total * 100 if total else 0
        mod_rows += f"""
        <tr>
          <td>{escape_html(mod["module"])}</td>
          <td>{total}</td>
          <td class="ok">{mod[VERDICT_PASS]}</td>
          <td class="warn">{mod[VERDICT_WARN]}</td>
          <td class="bad">{mod[VERDICT_FAIL]}</td>
          <td><div class="bar"><div style="width:{rate:.0f}%"></div></div><span class="pct">{rate:.0f}%</span></td>
        </tr>"""
    slow_rows = ""
    for sl in report["slowest"]:
        slow_rows += f'<tr><td><span class="badge">{sl["method"]}</span></td><td><code>{escape_html(sl["path"])}</code></td><td>{sl["duration_ms"]}ms</td></tr>'

    return f"""<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Synapse-Rust API 测试报告</title>
<style>
  :root {{ --bg:#f6f7f9; --card:#fff; --line:#e5e7eb; --text:#111827; --muted:#6b7280;
          --ok:#16a34a; --warn:#d97706; --bad:#dc2626; --accent:#2563eb; }}
  * {{ box-sizing:border-box; margin:0; padding:0; }}
  body {{ background:var(--bg); color:var(--text); font-family:-apple-system,BlinkMacSystemFont,"Segoe UI",
         "PingFang SC","Hiragino Sans GB","Microsoft YaHei",sans-serif; padding:32px 16px; }}
  .wrap {{ max-width:1100px; margin:0 auto; }}
  h1 {{ font-size:22px; margin-bottom:4px; }}
  .meta {{ color:var(--muted); font-size:13px; margin-bottom:24px; }}
  .grid {{ display:grid; grid-template-columns:280px 1fr; gap:24px; margin-bottom:24px; }}
  @media (max-width:800px) {{ .grid {{ grid-template-columns:1fr; }} }}
  .card {{ background:var(--card); border:1px solid var(--line); border-radius:12px; padding:20px; }}
  .ring {{ width:150px; height:150px; border-radius:50%; background:var(--ring,{ring});
           display:flex; align-items:center; justify-content:center; margin:0 auto 12px; }}
  .ring-inner {{ width:110px; height:110px; border-radius:50%; background:#fff;
                 display:flex; flex-direction:column; align-items:center; justify-content:center; }}
  .score {{ font-size:34px; font-weight:800; }}
  .grade {{ font-size:14px; color:var(--muted); }}
  .stats {{ display:grid; grid-template-columns:repeat(4,1fr); gap:12px; }}
  .stat {{ text-align:center; padding:14px 6px; background:var(--card); border:1px solid var(--line); border-radius:12px; }}
  .stat .num {{ font-size:28px; font-weight:700; }}
  .stat .lbl {{ font-size:12px; color:var(--muted); margin-top:2px; }}
  .ok {{ color:var(--ok); }} .warn {{ color:var(--warn); }} .bad {{ color:var(--bad); }}
  table {{ width:100%; border-collapse:collapse; font-size:13px; }}
  th {{ text-align:left; color:var(--muted); font-weight:600; padding:8px 10px; border-bottom:2px solid var(--line); }}
  td {{ padding:8px 10px; border-bottom:1px solid var(--line); vertical-align:top; }}
  tr:hover td {{ background:#f9fafb; }}
  code {{ background:#f3f4f6; padding:1px 6px; border-radius:4px; font-size:12px; word-break:break-all; }}
  .badge {{ display:inline-block; min-width:52px; text-align:center; background:#eef2ff; color:#4338ca;
            border-radius:6px; padding:1px 6px; font-size:11px; font-weight:600; }}
  .sub {{ color:var(--muted); font-size:11px; margin-top:2px; word-break:break-all; }}
  .diff {{ color:var(--bad); }}
  .bar {{ display:inline-block; width:70px; height:8px; background:#e5e7eb; border-radius:4px; vertical-align:middle; }}
  .bar > div {{ height:100%; background:var(--ok); border-radius:4px; }}
  .pct {{ font-size:11px; color:var(--muted); margin-left:6px; }}
  h2 {{ font-size:16px; margin:24px 0 10px; }}
  .card + h2 {{ margin-top:32px; }}
  footer {{ margin-top:28px; color:var(--muted); font-size:12px; text-align:center; }}
</style>
</head>
<body><div class="wrap">
  <h1>Synapse-Rust API 全量测试报告</h1>
  <div class="meta">
    环境 <b>{m["env"]}</b> · 目标 <b>{m["base_url"]}</b> · 生成 {m["generated_at"]} ·
    总耗时 {m["duration_total_s"]}s · 并发 {m["concurrency"]} ·
    写探测 {'开启' if m["allow_write"] else '关闭'} · TLS 校验 {'开启' if m["verify_tls"] else '关闭'}
  </div>
  <div class="grid">
    <div class="card" style="text-align:center">
      <div class="ring"><div class="ring-inner"><div class="score">{score}</div><div class="grade">{grade}</div></div></div>
      <div style="font-size:12px;color:var(--muted)">接口健康度评分（满分 100）</div>
    </div>
    <div class="stats">
      <div class="stat"><div class="num">{s["total"]}</div><div class="lbl">接口总数</div></div>
      <div class="stat"><div class="num ok">{s["passed"]}</div><div class="lbl">✅ 通过（{(s["passed"]/s["total"]*100 if s["total"] else 0):.1f}%）</div></div>
      <div class="stat"><div class="num warn">{s["warned"]}</div><div class="lbl">⚠️ 警告</div></div>
      <div class="stat"><div class="num bad">{s["failed"]}</div><div class="lbl">❌ 失败</div></div>
    </div>
  </div>
  <div class="stats" style="grid-template-columns:repeat(4,1fr)">
    <div class="stat"><div class="num">{s["probe_requests"]}</div><div class="lbl">实际请求数</div></div>
    <div class="stat"><div class="num">{s["duration"]["avg_ms"]}ms</div><div class="lbl">平均响应时间</div></div>
    <div class="stat"><div class="num">{s["duration"]["max_ms"]}ms</div><div class="lbl">最大响应时间</div></div>
    <div class="stat"><div class="num">{s["skipped"]}</div><div class="lbl">跳过（无法构造 URL）</div></div>
  </div>

  <h2>❌ 失败接口（{s["failed"]}）</h2>
  <div class="card"><table>
    <tr><th>方法</th><th>路径 / 模块</th><th>探测</th><th>实际状态</th><th>差异说明 / 实际响应</th></tr>
    {fail_rows or '<tr><td colspan="5" style="color:var(--ok)">🎉 无失败接口</td></tr>'}
  </table></div>

  <h2>⚠️ 警告接口（{s["warned"]}，最多显示 20 条）</h2>
  <div class="card"><table>
    <tr><th>方法</th><th>路径 / 模块</th><th>状态</th><th>原因</th></tr>
    {warn_rows or '<tr><td colspan="4">无</td></tr>'}
  </table></div>

  <h2>按模块（registered_by）统计</h2>
  <div class="card"><table>
    <tr><th>模块</th><th>总数</th><th>通过</th><th>警告</th><th>失败</th><th>通过率</th></tr>
    {mod_rows}
  </table></div>

  <h2>最慢接口 TOP 10</h2>
  <div class="card"><table>
    <tr><th>方法</th><th>路径</th><th>耗时</th></tr>
    {slow_rows}
  </table></div>

  <footer>由 scripts/api_test/run_api_tests.py 生成 · RouteLedger 路由清单驱动</footer>
</div></body></html>
"""


def escape_html(s: str) -> str:
    if not s:
        return ""
    return (s.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
             .replace('"', "&quot;"))


# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------
def parse_args(argv: List[str]) -> argparse.Namespace:
    p = argparse.ArgumentParser(
        description="Synapse-Rust 全量 API 路由健康测试",
        formatter_class=argparse.ArgumentDefaultsHelpFormatter,
    )
    p.add_argument("--env", help="环境名 dev|test|prod（覆盖 config.yaml）")
    p.add_argument("--base-url", help="目标基础 URL，如 https://matrix.test")
    p.add_argument("--token", help="访问令牌（跳过自动登录）")
    p.add_argument("--ledger", help="路由清单 JSON 路径（覆盖默认）")
    p.add_argument("--export-ledger", action="store_true",
                   help="先编译并导出最新路由清单（需要 cargo，耗时较长）")
    p.add_argument("--verify-tls", dest="verify_tls", action="store_true", default=None,
                   help="强制开启 TLS 证书校验")
    p.add_argument("--no-verify-tls", dest="verify_tls", action="store_false",
                   help="关闭 TLS 证书校验（自签名证书环境）")
    p.add_argument("--timeout", type=float, help="单请求超时秒数")
    p.add_argument("--concurrency", type=int, help="并发请求数")
    p.add_argument("--allow-write", action="store_true",
                   help="开启写操作认证探测（空 body，预期 4xx；prod 环境自动忽略）")
    p.add_argument("--only-module", help="只测试 registered_by 匹配该子串的路由")
    p.add_argument("--report-dir", help="报告输出目录（默认 config.yaml report_dir）")
    p.add_argument("--json-out", help="JSON 报告输出路径")
    p.add_argument("--md-out", help="Markdown 报告输出路径")
    p.add_argument("--html-out", help="HTML 报告输出路径")
    p.add_argument("--quiet", action="store_true", help="减少日志输出")
    return p.parse_args(argv)


def main(argv: Optional[List[str]] = None) -> int:
    cli = parse_args(argv if argv is not None else sys.argv[1:])
    cfg = load_config(cli)
    started_at = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    t_start = time.monotonic()

    # ---- 1. 路由清单 ----
    ledger_path: Optional[Path] = None
    if cli.export_ledger:
        ledger_path = export_ledger()
        if ledger_path is None:
            print("⚠️ ledger 导出失败，回退默认清单", file=sys.stderr)
    if ledger_path is None:
        ledger_path = resolve_ledger(cfg)
    if ledger_path is None:
        print("❌ 未找到路由清单。请先运行 --export-ledger 或检查 ledger_path 配置。", file=sys.stderr)
        return 2
    ledger_data = json.loads(ledger_path.read_text(encoding="utf-8"))
    entries = load_ledger(ledger_path)
    ledger_meta = {
        "name": ledger_path.name,
        "path": str(ledger_path),
        "entry_count": ledger_data.get("entry_count", len(entries)),
        "profile": ledger_data.get("state_profile"),
        "generated_at": ledger_data.get("generated_at"),
        "schema_version": ledger_data.get("schema_version"),
    }

    if cli.only_module:
        entries = [e for e in entries if cli.only_module in (e.get("registered_by") or "")]
        print(f"[plan] 过滤模块 {cli.only_module!r}：{len(entries)} 条")

    # ---- 2. 认证准备 ----
    verify = cfg.get("verify_tls", True)   # True | False | CA bundle 路径
    timeout = float(cfg.get("timeout"))
    token = cfg.get("auth_token") or ""
    token_src = "配置"
    if not token:
        token, login_err = auto_login(cfg, cfg["base_url"], verify, timeout)
        if token:
            token_src = "自动登录"
        else:
            print(f"⚠️ 未获取到普通用户 token（{login_err}），认证探测将被跳过，仅执行匿名探测")
    cfg["_token"] = token

    admin_token, admin_err = auto_login_admin(cfg, cfg["base_url"], verify, timeout)
    if admin_token:
        print(f"[auth] admin token 获取成功（用于 /admin 端点认证探测）")
    else:
        print(f"[auth] 未获取到 admin token（{admin_err}），admin 端点仅做匿名探测")
    cfg["_admin_token"] = admin_token

    print(f"[auth] token 来源：{token_src}{'；admin：已获取' if admin_token else '；admin：未获取'}")

    # ---- 3. 构建计划并执行 ----
    cases, skipped = build_plan(entries, cfg)
    if not cases:
        print("❌ 测试计划为空（路由清单无有效条目）", file=sys.stderr)
        return 2

    expectations = load_yaml(DEFAULT_EXPECTATIONS)
    schemas = expectations.get("schemas") or {}
    overrides = expectations.get("overrides") or {}

    total_probes = sum(len(c.probes) for c in cases)
    print(f"[run] 共 {len(cases)} 条路由、{total_probes} 个请求探测，并发 {cfg.get('concurrency')} ...")

    done = 0
    fail_count = 0

    def work(item: Tuple[CaseResult, Dict[str, Any]]) -> Tuple[str, str]:
        case, ctx = item
        anonymous_status: Optional[int] = None
        for idx, probe in enumerate(case.probes):
            # 匿名探测不带 token；authed 探测中 admin 路径优先使用 admin token
            use_token: Optional[str] = None
            if probe.spec.label == "authed":
                if "/_synapse/admin" in probe.spec.url and ctx["admin_token"]:
                    use_token = ctx["admin_token"]
                else:
                    use_token = ctx["token"]
            res = execute_probe(probe.spec, cfg, use_token)
            # 校验规则准备
            key = f"{probe.spec.method} {case.path}"
            rule = schemas.get(key) or {}
            override = overrides.get(key) or {}
            # 判断端点是否需要认证：force_auth 显式标注，或匿名探测被 401/403 拒绝
            needs_auth = bool(override.get("force_auth"))
            if probe.spec.label == "anonymous":
                anonymous_status = res.http_status
                if res.http_status in AUTH_REJECT_CODES:
                    needs_auth = True
            is_authed = probe.spec.label != "anonymous"
            # schema 精确校验应用条件：
            #   - force_public 端点：匿名探测也校验
            #   - 需认证端点：仅对 authed 探测校验（匿名 401 属预期，不校验结构）
            #   - 公开端点：匿名探测校验
            if rule and (override.get("force_public") or not needs_auth or is_authed):
                validate_schema(res, rule, probe.spec.method, case.path)
            if override:
                apply_override_checks(res, override, probe.spec.method, case.path)
            # 判定：显式规则（expect_* / schema status）失败即 FAIL；
            # 否则走自动学习（含 force_auth / force_public 语义）
            has_explicit = bool(
                (override and ("expect_status" in override or "expect_body_text" in override
                               or "expect_json_field" in override))
                or (rule and "status" in rule)
            )
            if has_explicit:
                res.verdict = VERDICT_PASS if not res.issues else VERDICT_FAIL
            else:
                res.verdict = classify_verdict(res, override, rule)
            case.probes[idx] = res
        return case.method + " " + case.path, case.verdict

    # 注意：为避免共享可变结构问题，直接在列表上迭代修改（无跨线程共享写）
    with concurrent.futures.ThreadPoolExecutor(max_workers=int(cfg.get("concurrency"))) as ex:
        futures = {ex.submit(work, (c, {"token": token, "admin_token": admin_token})): c for c in cases}
        for fut in concurrent.futures.as_completed(futures):
            done += 1
            label, verdict = fut.result()
            if verdict == VERDICT_FAIL:
                fail_count += 1
            if not cli.quiet and (done % 100 == 0 or verdict == VERDICT_FAIL):
                print(f"[run] {done}/{len(cases)}  {label} → {verdict}")
            if cfg.get("request_delay"):
                time.sleep(float(cfg.get("request_delay")))

    duration_total = time.monotonic() - t_start
    print(f"[run] 完成：{done} 条，失败 {fail_count} 条（{duration_total:.1f}s）")

    # ---- 4. 报告 ----
    report = build_report(cases, cfg, ledger_meta, skipped, started_at, duration_total)
    report_dir = Path(cfg.get("report_dir") or "reports")
    if not report_dir.is_absolute():
        report_dir = (SCRIPT_DIR / report_dir).resolve()
    report_dir.mkdir(parents=True, exist_ok=True)

    json_out = Path(cli.json_out) if cli.json_out else report_dir / "api_test_report.json"
    md_out = Path(cli.md_out) if cli.md_out else report_dir / "api_test_report.md"
    html_out = Path(cli.html_out) if cli.html_out else report_dir / "api_test_report.html"

    json_out.write_text(json.dumps(report, ensure_ascii=False, indent=2), encoding="utf-8")
    md_out.write_text(render_markdown(report), encoding="utf-8")
    html_out.write_text(render_html(report), encoding="utf-8")

    s = report["summary"]
    print()
    print("=" * 64)
    print(f"  环境: {cfg.get('env')}  目标: {cfg['base_url']}")
    print(f"  接口总数 {s['total']} | 通过 {s['passed']} | 警告 {s['warned']} | "
          f"失败 {s['failed']} | 跳过 {s['skipped']}")
    print(f"  健康度评分: {s['health_score']} / 100（{s['grade']}）")
    print("=" * 64)
    print(f"  报告：{html_out}")
    print(f"        {md_out}")
    print(f"        {json_out}")
    return 0 if s["failed"] == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
