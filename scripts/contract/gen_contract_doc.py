#!/usr/bin/env python3
"""Generate docs/synapse-rust/ROUTE_CONTRACT.md from the extracted route surface + manifest coverage.

Source of truth for the route surface: artifacts/registered_routes.json (produced by
extract_registered.py from the real .route()/.nest() registrations in src/web/routes/**).

The generated doc embeds a volatile "自动生成于 <date>" line; CI drift gate
(scripts/contract/check_route_contract.sh) normalizes that line away before diffing, so the
gate only fails on *structural* drift, not on the timestamp.

Human-maintained content under "## 附录" (e.g. 附录 A) is preserved across regeneration so it
is never overwritten by the auto-generated body.
"""

import os, re, json, datetime

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
ROOT = os.environ.get("SYNAPSE_RUST_ROOT") or os.path.dirname(
    os.path.dirname(SCRIPT_DIR)
)
reg_raw = json.load(open(f"{ROOT}/artifacts/registered_routes.json"))
reg = reg_raw["modules"]
non_ns = reg_raw.get("non_namespace_routes", [])
OUT = f"{ROOT}/docs/synapse-rust/ROUTE_CONTRACT.md"

# manifest presence (full-file scan)
re_manifest = re.compile(
    r"fn\s+\w*(?:route_manifest|manifest_for|assembly_compat_manifest|top_level_inline_manifest)\w*\s*\("
)
mani = set()
for dp, _, fns in os.walk(f"{ROOT}/src/web/routes"):
    for f in fns:
        if not f.endswith(".rs"):
            continue
        fp = os.path.join(dp, f)
        rel = os.path.relpath(fp, f"{ROOT}/src/web/routes")
        if "tests" in rel.split(os.sep):
            continue
        if re_manifest.search(open(fp).read()):
            mani.add(rel)

CAT = {
    "federation": "联邦 (Federation)",
    "admin": "管理 (Admin)",
    "e2ee": "端到端加密 (E2EE)",
    "media": "媒体 (Media)",
    "sync": "同步 (Sync)",
    "sliding_sync": "滑动同步 (Sliding Sync)",
    "room": "房间 (Room)",
    "account": "账户 (Account)",
    "device": "设备 (Device)",
    "push": "推送 (Push)",
    "space": "空间 (Space)",
    "presence": "在线状态 (Presence)",
    "oidc": "OIDC",
    "voip": "通话 (VoIP)",
    "key_backup": "密钥备份 (Key Backup)",
    "key_rotation": "密钥轮转",
    "app_service": "应用服务 (AppService)",
    "search": "搜索 (Search)",
    "thirdparty": "第三方 (Third-party)",
    "dm": "私聊 (DM)",
    "friend": "好友 (Friends)",
    "widget": "小组件 (Widget)",
    "voice": "语音 (Voice)",
    "verification": "验证 (Verification)",
    "sticky": "粘性事件",
    "tags": "标签 (Tags)",
    "typing": "输入状态 (Typing)",
    "receipts": "回执 (Receipts)",
    "reactions": "反应 (Reactions)",
    "relations": "关联 (Relations)",
    "threads": "话题 (Threads)",
    "burn_after_read": "阅后即焚",
    "msc4108": "MSC4108",
    "rendezvous": "Rendezvous",
    "saml": "SAML",
    "cas": "CAS",
    "openclaw": "OpenClaw",
    "worker": "Worker",
    "push_notification": "推送通知",
    "ai_connection": "AI 连接",
    "ledger_export": "Ledger 导出",
    "delayed_events": "延迟事件",
    "ephemeral": "临时事件",
    "room_summary": "房间摘要",
    "moderation": "审核 (Moderation)",
    "feature_flags": "特性开关",
    "external_service": "外部服务",
    "guest": "访客 (Guest)",
    "captcha": "验证码 (Captcha)",
    "threepid": "3PID",
    "telemetry": "遥测 (Telemetry)",
    "account_compat": "账户兼容",
    "auth_compat": "认证兼容",
    "directory": "目录 (Directory)",
    "context": "上下文",
    "module": "模块",
    "assembly": "装配 (Assembly)",
    "background_update": "后台更新",
    "room_access": "房间访问",
    "pinned": "置顶",
    "event_report": "事件举报",
    "invite_blocklist": "邀请黑名单",
    "room_summary": "房间摘要",
    "sticky_event": "粘性事件",
    "pinned": "置顶",
}


def category(mod):
    base = os.path.basename(mod).replace(".rs", "")
    if mod in mani:
        pass
    # 优先按 basename 精确匹配（避免 friend_room.rs 被 'room' 子串误归到「房间」）
    for k, v in CAT.items():
        if base == k or base.startswith(k + "_") or base.startswith(k + "."):
            return v
    for k, v in CAT.items():
        if k in mod:
            return v
    return "其他 (Other)"


# group by category
bycat = {}
for mod, routes in reg.items():
    if not routes:
        continue
    c = category(mod)
    bycat.setdefault(c, []).append((mod, routes))

total = sum(len(v) for v in reg.values())
lines = []
lines.append("# synapse-rust 路由契约（Route Contract）")
lines.append("")
lines.append(
    f"> 自动生成于 {datetime.date.today().isoformat()}，源 = `src/web/routes/**` 真实 `.route()` 注册面 + 各模块 `*_route_manifest()` 覆盖情况。"
)
lines.append(">")
lines.append(
    "> 本文件是后端 HTTP 契约的**事实来源之一**（机器侧权威为 `src/web/routes/route_ledger.rs` 与各模块 manifest，启动时校验、集成测试 PATCH 探测）。人工文档（INDEX.md / API_COVERAGE_REPORT.md）须与之保持一致。"
)
lines.append(">")
lines.append(
    "> ⚠️ MSC 编号在本仓的**实际语义**以 [`MSC_SEMANTICS.md`](MSC_SEMANTICS.md) 为唯一真相源；"
    "若干编号（4155 / 4204 / 3967）被借用承载了与官方提案不同的功能，按编号推断语义前请先查表。"
)
lines.append("")
lines.append("## 总览")
lines.append("")
lines.append(f"- 注册路由条目（绝对 `(method, path)`，经 `.nest()` 前缀解析后去重）：**{total}**")
lines.append(f"- 含路由注册的模块文件：**{sum(1 for v in reg.values() if v)}**")
lines.append(f"- 含 `*_route_manifest` 函数的模块：**{len(mani)}**")
lines.append("")
lines.append("> **路径为何是绝对的**：本清单由 `extract_registered.py` 从真实 router 构造解析得到，")
lines.append("> 已递归应用 `.nest(\"/prefix\", ..)` 与 `expand_under_prefixes(..)` 的前缀。")
lines.append("> 因此每一条都是客户端可直接拼接的 serve 路径，而不是子 router 内的相对字面量。")
lines.append("")
lines.append("## 生成期自校验（不是自证）")
lines.append("")
lines.append("提取器在生成时对两份**独立**的事实来源做对账，任一项不达标即报错：")
lines.append("")
lines.append("| 对照源 | 含义 | 结果 |")
lines.append("|---|---|---|")
lines.append(
    "| 各模块 `*_route_manifest()` 声明集 | 手写的绝对路径声明，不经过本解析器的前缀推导 | **声明而未解析出 = 0** |"
)
lines.append(
    "| `tests/unit/fixtures/ledger_export/*.json` | 由真实 Rust 装配（`synapse_ledger_export`）导出、golden 测试守护 | **ledger 有而本清单缺 = 0** |"
)
lines.append("")
lines.append("第二条尤其关键：它保证本清单**不会漏掉任何一个真实对外服务的路由**。")
lines.append("反向差额（本清单多于 ledger）来自源码扫描会看到、而默认 feature 构建不注册的路由")
lines.append("（SAML / CAS / Voice / ExternalServices 等 gated 模块）以及 manifest 的漏声明。")
lines.append("")
lines.append("## 前缀之外 / 未装配的注册")
lines.append("")
lines.append(
    "以下注册不属于 `/_matrix/`、`/_synapse/`、`/.well-known/` 任一命名空间，只有两种成因："
)
lines.append("根级协议或探活端点（有意为之），或**定义了却从未 merge 进任何路由树的孤儿 router**。")
lines.append("")
if non_ns:
    lines.append("| 模块 | Method | Path |")
    lines.append("|---|---|---|")
    for mod, meth, path in non_ns:
        lines.append(f"| `{mod}` | `{meth}` | `{path}` |")
else:
    lines.append("_（无）_")
lines.append("")
lines.append("## 契约覆盖（manifest 一致性）")
lines.append("")
lines.append(
    "`route_ledger` 在启动时校验所有 manifest 内 `(method,path)` 不重复，集成测试 `api_route_ledger_tests.rs` 对每个声明做 PATCH 探测（断言 405）。"
)
lines.append("**已知缺口 / 漂移**：")
lines.append("")
lines.append(
    "- `src/web/routes/threepid.rs`：定义 `create_threepid_router()`（`/requestToken`、`/submitToken`）但**从未 merge 进任何路由树**（仅 `mod.rs` re-export），且自身无 manifest 函数 → 属于孤儿/死代码；实际 3PID 端点位于 `account_compat.rs`（`/account/3pid/...`）。"
)
lines.append(
    "  **机器证据**：这两个路径在「前缀之外 / 未装配的注册」表中——解析器沿 `create_router` 的整条装配链递归后，它们仍未获得任何前缀，与 CAS 根级端点并列，可直接区分「有意根级」与「从未装配」。"
)
lines.append(
    "- `space/children_hierarchy.rs`、`space/membership_state.rs`、`space/summary.rs`：无独立 manifest 函数，但其路由由 `space.rs` 的 `space_route_manifest()` 统一声明（已覆盖）。"
)
lines.append("")
lines.append("## 模块级路由清单（逐模块）")
lines.append("")
for c in sorted(bycat):
    mods = sorted(bycat[c], key=lambda x: (-len(x[1]), x[0]))
    cat_total = sum(len(r) for _, r in mods)
    lines.append(f"### {c} （{cat_total} 条）")
    lines.append("")
    for mod, routes in mods:
        has = "✅manifest" if mod in mani else "⚠️无manifest"
        lines.append(f"#### `{mod}` — {len(routes)} 条 {has}")
        lines.append("")
        for meth, path in routes:
            lines.append(f"- `{meth}` `{path}`")
        lines.append("")
lines.append("---")
lines.append(
    "> 本文件由 `scripts/contract/extract_registered.py` + `gen_contract_doc.py` 生成。路由面随代码变化，请定期重新生成（或运行 `make route-contract-check` / CI `route-contract-gate` 门禁）。"
)

# 保留旧文件 `## 附录` 之后的人工维护内容（如「附录 A」），防止重新生成覆盖
manual_tail = ""
try:
    with open(OUT) as _f:
        _old = _f.read()
    if "## 附录" in _old:
        manual_tail = _old[_old.index("## 附录") :].rstrip()
except FileNotFoundError:
    pass

content = "\n".join(lines)
if manual_tail:
    content += "\n---\n" + manual_tail + "\n"

with open(OUT, "w") as f:
    f.write(content)
print(
    f"wrote {OUT}: {total} routes, {len(bycat)} categories"
    + (" (appendix preserved)" if manual_tail else "")
)
