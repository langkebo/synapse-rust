#!/usr/bin/env python3
"""路由覆盖度三口径的唯一计算实现（度量单一化）。

背景：`docs/synapse-rust/API_COVERAGE_REPORT.md` 里的「注册条目 / 唯一路径 / 逻辑端点」
长期靠人工递增抄写，v1.15 / v1.16 / v1.17 三个版本各自写下互相矛盾的数字，而机器侧
sidecar 实测是 1048 条。本脚本是这些数字的**唯一实现**：报告只允许引用它生成的块，
人工手改会被 `--check` 判红。

数据源（唯一）：`docs/synapse-rust/ROUTE_HANDLERS.json`
  —— 由 `scripts/contract/gen_route_handlers.py` 从真实 `.route()` 注册面生成并入库，
     每次改路由都由契约链重生成。本脚本不自己解析 Rust 源码，避免第二份抽取实现。

三种口径（不可相加、不可互换）：
  注册条目   唯一 `(method, absolute_path)` 对
  唯一路径   去掉方法后的唯一 `absolute_path`
  逻辑端点   在唯一路径基础上折叠版本前缀（v1/v3/r0/unstable/<seg> → vX）

用法：
  python3 scripts/contract/route_metrics.py              # 人类可读
  python3 scripts/contract/route_metrics.py --json       # 机器可读
  python3 scripts/contract/route_metrics.py --write      # 重写报告里五个生成块
  python3 scripts/contract/route_metrics.py --check      # 漂移门禁（CI）
  python3 scripts/contract/route_metrics.py --self-test  # 证明 --check 能失败
"""

import argparse
import json
import os
import re
import sys
from collections import Counter

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT = os.environ.get("SYNAPSE_RUST_ROOT") or os.path.dirname(
    os.path.dirname(SCRIPT_DIR)
)
SIDECAR = os.path.join(ROOT, "docs/synapse-rust/ROUTE_HANDLERS.json")
REPORT = os.path.join(ROOT, "docs/synapse-rust/API_COVERAGE_REPORT.md")
CONTRACT = os.path.join(ROOT, "docs/synapse-rust/ROUTE_CONTRACT.md")

CLIENT_PREFIX = "/_matrix/client"
ADMIN_PREFIX = "/_synapse/admin"


# ---- 线程安全的口径定义 ------------------------------------------------------


def logical_path(p):
    """折叠版本前缀。规则与报告 §8 曾手工粘贴的配方逐字一致。"""
    p = re.sub(
        r"^/_matrix/client/(v3|v1|r0|v[0-9]+|versions)/", "/_matrix/client/vX/", p
    )
    p = re.sub(
        r"^/_matrix/client/(unstable|org\.matrix\.[a-z0-9._]*)/",
        "/_matrix/client/vX/",
        p,
    )
    p = re.sub(r"^/_matrix/client/vX/org\.[a-z0-9._]+/", "/_matrix/client/vX/", p)
    p = re.sub(r"^/_synapse/admin/v[0-9]+/", "/_synapse/admin/vX/", p)
    return p


def bucket(p):
    if p.startswith(CLIENT_PREFIX):
        return "client"
    if p.startswith(ADMIN_PREFIX):
        return "admin"
    return "other"


def other_breakdown(paths):
    """「其他命名空间」的四个来源，报告 §1.1 需要它们自洽。"""
    c = Counter()
    for p in paths:
        if p.startswith(CLIENT_PREFIX) or p.startswith(ADMIN_PREFIX):
            continue
        if p.startswith("/_matrix/"):
            c["matrix_non_client"] += 1
        elif p.startswith("/_synapse/"):
            c["synapse_non_admin"] += 1
        elif p.startswith("/.well-known/"):
            c["well_known"] += 1
        else:
            c["root"] += 1
    return dict(c)


# ---- 分类规则（原报告 §8.1 内联脚本，现为唯一实现） ---------------------------

CLIENT_RULES = [
    (
        "认证",
        r"/(login|logout|register|refresh|oidc|saml|cas|rendezvous)"
        r"|/account/(password|deactivate|3pid)|msc2965|msc4108|msc3882|/organizations",
    ),
    (
        "同步",
        r"/sync|/notifications|msc3575|/to_device|/pushrules|/pushers|/push$|/push/",
    ),
    (
        "设备与密钥",
        r"/devices|/keys|/room_keys"
        r"|/cross_signing|/dehydrated_device|msc3814",
    ),
    ("搜索", r"/search"),
    ("媒体", r"/media|/upload|/thumbnail|/preview_url"),
    (
        "用户",
        r"/profile|/presence|/user_directory|/thirdparty|/users|/capabilities|/account_data",
    ),
    (
        "消息",
        r"/rooms/[^/]+/(send|messages|receipt|typing|redact|report|read_markers)"
        r"|/sendToDevice|msc4140|/rooms/[^/]+/event/",
    ),
    ("房间", r"."),
]

ADMIN_RULES = [
    (
        "用户管理",
        r"/users|/user_sessions|/registration_tokens|/register|/account_validity"
        r"|/whois|/whoami|/account|/password_auth_providers|/user_stats|/invite",
    ),
    ("媒体", r"/media|/quarantine_media|/purge_media_cache|/media_callbacks"),
    ("联邦", r"/federation|/destinations|/server|/version|/rate-limit-status"),
    (
        "安全",
        r"/event_reports|/reports|/policy|/audit|/feature-flags|/experimental_features"
        r"|/background_updates",
    ),
    (
        "房间管理",
        r"/rooms|/retention|/purge_room|/purge_history|/shutdown_room|/spaces"
        r"|/room_stats|/stats|/statistics|/server_notices|/send_server_notice"
        r"|/jitsi|/cleanup|/config|/captcha",
    ),
    ("服务器", r"."),
]


def classify(path, rules):
    for name, pat in rules:
        if re.search(pat, path):
            return name
    return rules[-1][0]


# ---- 上游人工口径（唯一的"人工常量"存放点） ---------------------------------
#
# §四 的「上游 Synapse」列来自官方文档章节端点索引的人工清点，**不是**机器抽取，
# 因此只能作为常量表放在这里，不能在报告里另抄一份。本仓侧数字全部由 compute()
# 产出；两者只做「差额」对照，不做百分比 —— 两个口径不可除（见报告 §四警告）。

UPSTREAM_MANUAL = {
    "client": [
        ("认证", 35),
        ("房间", 50),
        ("消息", 40),
        ("媒体", 20),
        ("用户", 25),
        ("设备与密钥", 18),
        ("同步", 15),
        ("搜索", 10),
    ],
    "admin": [
        ("用户管理", 27),
        ("房间管理", 33),
        ("服务器", 17),
        ("媒体", 18),
        ("联邦", 14),
        ("安全", 10),
    ],
}


# ---- 计算 ------------------------------------------------------------------


def load_paths():
    with open(SIDECAR, encoding="utf-8") as f:
        sidecar = json.load(f)
    entries = [(r["method"], r["path"]) for r in sidecar["routes"]]
    if len(set(entries)) != len(entries):
        raise SystemExit(
            "route_metrics: sidecar 含重复 (method, path) —— 生成器去重失效，拒绝出数。"
        )
    declared = sidecar.get("counts", {}).get("routes")
    if declared is not None and declared != len(entries):
        raise SystemExit(
            f"route_metrics: sidecar counts.routes={declared} 与实际 {len(entries)} 条不符。"
        )
    return entries


def compute():
    entries = load_paths()
    paths = [p for _, p in entries]
    unique_paths = sorted(set(paths))
    logical = sorted({logical_path(p) for p in unique_paths})

    def split(lst):
        c = Counter(bucket(p) for p in lst)
        return {
            "total": len(lst),
            "client": c["client"],
            "admin": c["admin"],
            "other": c["other"],
        }

    # 分类：注册条目按条目计、唯一路径按路径计、逻辑端点按折叠后路径计
    def by_category(lst, prefix, rules):
        c = Counter(classify(p, rules) for p in lst if p.startswith(prefix))
        return dict(c)

    return {
        "source": os.path.relpath(SIDECAR, ROOT),
        "calibers": {
            "registered_entries": split(paths),
            "unique_paths": split(unique_paths),
            "logical_endpoints": split(logical),
        },
        "other_breakdown": {
            "unique_paths": other_breakdown(unique_paths),
            "registered_entries": other_breakdown(paths),
            "logical_endpoints": other_breakdown(logical),
        },
        "client_categories": {
            "registered_entries": by_category(paths, CLIENT_PREFIX, CLIENT_RULES),
            "unique_paths": by_category(unique_paths, CLIENT_PREFIX, CLIENT_RULES),
            "logical_endpoints": by_category(logical, CLIENT_PREFIX, CLIENT_RULES),
        },
        "admin_categories": {
            "registered_entries": by_category(paths, ADMIN_PREFIX, ADMIN_RULES),
            "unique_paths": by_category(unique_paths, ADMIN_PREFIX, ADMIN_RULES),
            "logical_endpoints": by_category(logical, ADMIN_PREFIX, ADMIN_RULES),
        },
    }


# ---- 渲染 ------------------------------------------------------------------


def _cat_rows(cat, order=None):
    names = order or sorted(
        cat["registered_entries"], key=lambda n: -cat["registered_entries"][n]
    )
    rows = []
    for n in names:
        rows.append(
            f"| {n} | {cat['logical_endpoints'].get(n, 0)} "
            f"| {cat['unique_paths'].get(n, 0)} | {cat['registered_entries'].get(n, 0)} |"
        )
    return rows


def render_totals(m):
    C = m["calibers"]
    return "\n".join(
        [
            "| 口径 | 含义 | 全部 | `/_matrix/client` | `/_synapse/admin` | 其他命名空间 |",
            "|---|---|---:|---:|---:|---:|",
            f"| **注册条目** | 唯一 `(method, absolute_path)` 对 "
            f"| **{C['registered_entries']['total']}** | {C['registered_entries']['client']} "
            f"| {C['registered_entries']['admin']} | {C['registered_entries']['other']} |",
            f"| **唯一路径** | 去掉方法后的唯一 `absolute_path` "
            f"| **{C['unique_paths']['total']}** | {C['unique_paths']['client']} "
            f"| {C['unique_paths']['admin']} | {C['unique_paths']['other']} |",
            f"| **逻辑端点** | 在唯一路径基础上折叠版本前缀（`v1`/`v3`/`r0`/`unstable/<seg>` → `vX`） "
            f"| **{C['logical_endpoints']['total']}** | {C['logical_endpoints']['client']} "
            f"| {C['logical_endpoints']['admin']} | {C['logical_endpoints']['other']} |",
        ]
    )


def render_categories(title, cat):
    lines = [
        "| 类别 | 逻辑端点 | 唯一路径 | 注册条目 |",
        "|------|---------:|--------:|--------:|",
    ]
    lines.extend(_cat_rows(cat))
    totals = (
        sum(cat["logical_endpoints"].values()),
        sum(cat["unique_paths"].values()),
        sum(cat["registered_entries"].values()),
    )
    lines.append(f"| **合计** | **{totals[0]}** | **{totals[1]}** | **{totals[2]}** |")
    return "\n".join(lines)


def render_upstream(key, cat):
    """§四 对照表：本仓列由 compute() 出，上游列取 UPSTREAM_MANUAL。"""
    lines = [
        "| 类别 | synapse-rust（逻辑端点，机器） | 上游 Synapse `[人工口径]` | 差额 |",
        "|------|------------------------------:|-------------------------:|-----:|",
    ]
    seen = set()
    rows = UPSTREAM_MANUAL[key]
    for name, upstream in rows:
        seen.add(name)
        ours = cat["logical_endpoints"].get(name, 0)
        diff = f"{ours - upstream:+d}" if ours != upstream else "0"
        lines.append(f"| {name} | {ours} | {upstream} | {diff} |")
    for name in sorted(
        cat["logical_endpoints"], key=lambda n: -cat["logical_endpoints"][n]
    ):
        if name not in seen:
            lines.append(f"| {name} | {cat['logical_endpoints'][name]} | — | — |")
    total = sum(cat["logical_endpoints"].values())
    upstream_total = sum(v for _, v in rows)
    lines.append(
        f"| **合计** | **{total}** | **{upstream_total}** | **{total - upstream_total:+d}** |"
    )
    return "\n".join(lines)


FENCE = "<!-- ROUTE_METRICS:{name} BEGIN — 由 scripts/contract/route_metrics.py --write 生成，勿手改 -->"
END = "<!-- ROUTE_METRICS:{name} END -->"
BLOCKS = ("TOTALS", "CLIENT", "ADMIN", "UPSTREAM_CLIENT", "UPSTREAM_ADMIN")


def replace_block(text, name, body):
    start_marker = FENCE.format(name=name)
    end_marker = END.format(name=name)
    if start_marker not in text or end_marker not in text:
        raise SystemExit(
            f"route_metrics: {os.path.relpath(REPORT, ROOT)} 缺少标记 {name}；"
            "先在文档里放好 BEGIN/END 标记再 --write。"
        )
    head, rest = text.split(start_marker, 1)
    _, tail = rest.split(end_marker, 1)
    return f"{head}{start_marker}\n{body}\n{end_marker}{tail}"


def check_contract_total(m):
    """sidecar 与 ROUTE_CONTRACT.md 的总览必须同数（两份独立产物）。"""
    if not os.path.exists(CONTRACT):
        return None
    with open(CONTRACT, encoding="utf-8") as f:
        txt = f.read()
    hit = re.search(r"注册路由条目[^\n]*?\*\*(\d+)\*\*", txt)
    if not hit:
        return None
    declared = int(hit.group(1))
    if declared != m["calibers"]["registered_entries"]["total"]:
        raise SystemExit(
            f"route_metrics: ROUTE_CONTRACT.md 总览 {declared} ≠ sidecar "
            f"{m['calibers']['registered_entries']['total']}；两份机器产物已分叉，先修契约链。"
        )
    return declared


def human(m):
    out = [
        f"数据源: {m['source']}",
        "",
        "三种口径（不可相加/互换）:",
        render_totals(m),
        "",
        "其他命名空间拆解: "
        + json.dumps(m["other_breakdown"]["unique_paths"], ensure_ascii=False),
        "",
        "Client 分类:",
        render_categories("Client", m["client_categories"]),
        "",
        "Admin 分类:",
        render_categories("Admin", m["admin_categories"]),
    ]
    return "\n".join(out)


# ---- 入口 ------------------------------------------------------------------


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--json", action="store_true", help="输出 JSON")
    ap.add_argument("--write", action="store_true", help="重写报告中的生成块")
    ap.add_argument("--check", action="store_true", help="检测报告生成块与实测是否漂移")
    ap.add_argument(
        "--self-test", action="store_true", help="证明 --check 会因为手改而失败"
    )
    sub = ap.parse_args()

    m = compute()

    if sub.self_test:
        # 造一份「手改」的报告，要求 check 判红；再造一份正确的，要求判绿。
        # 门禁如果永远绿，就等于没有门禁（铁律 8）。
        with open(REPORT, encoding="utf-8") as f:
            original = f.read()
        good = render_blocks(m)
        baseline = text_with_blocks(original, good)
        if block_drift(baseline, m):
            raise SystemExit(
                "route_metrics --self-test: 正确内容未判绿 —— 报告标记缺失或渲染不稳定"
            )
        tampered_body = good[0].replace(
            str(m["calibers"]["registered_entries"]["total"]), "9999", 1
        )
        tampered = text_with_blocks(original, (tampered_body, good[1], good[2]))
        if not block_drift(tampered, m):
            raise SystemExit(
                "route_metrics --self-test: 手改一个数字后 --check 仍然通过 ⇒ 门禁无效"
            )
        print("✅ self-test: 篡改被检出，正确内容判绿")
        return 0

    if sub.json:
        print(json.dumps(m, ensure_ascii=False, indent=2))
        return 0

    if sub.write:
        blocks = render_blocks(m)
        with open(REPORT, encoding="utf-8") as f:
            text = f.read()
        for name, body in zip(BLOCKS, blocks):
            text = replace_block(text, name, body)
        with open(REPORT, "w", encoding="utf-8") as f:
            f.write(text)
        print(f"✅ 已重写 {os.path.relpath(REPORT, ROOT)} 中 {len(BLOCKS)} 个生成块")
        return 0

    if sub.check:
        check_contract_total(m)
        with open(REPORT, encoding="utf-8") as f:
            text = f.read()
        drift = block_drift(text, m)
        if drift:
            print("❌ API_COVERAGE_REPORT.md 的覆盖度生成块已漂移：")
            for d in drift:
                print(f"   - {d}")
            print("   修法: python3 scripts/contract/route_metrics.py --write")
            return 1
        print("✅ 覆盖度三口径与 sidecar 一致（无手抄漂移）")
        return 0

    print(human(m))
    return 0


def render_blocks(m):
    return (
        render_totals(m),
        render_categories("Client", m["client_categories"]),
        render_categories("Admin", m["admin_categories"]),
        render_upstream("client", m["client_categories"]),
        render_upstream("admin", m["admin_categories"]),
    )


def text_with_blocks(text, blocks):
    for name, body in zip(BLOCKS, blocks):
        text = replace_block(text, name, body)
    return text


def block_drift(text, m):
    """返回漂移描述列表；空即一致。"""
    drift = []
    for name, body in zip(BLOCKS, render_blocks(m)):
        start_marker = FENCE.format(name=name)
        end_marker = END.format(name=name)
        if start_marker not in text or end_marker not in text:
            drift.append(f"{name}: 缺少生成块标记")
            continue
        got = text.split(start_marker, 1)[1].split(end_marker, 1)[0].strip()
        if got != body.strip():
            drift.append(f"{name}: 生成块内容与实测不一致")
    return drift


if __name__ == "__main__":
    sys.exit(main())
