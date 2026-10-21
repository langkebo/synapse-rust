#!/usr/bin/env python3
"""`query_unknown` 策略登记表的门禁（覆盖 / 一致 / 陈旧 / 词表）。

背景
----
sidecar（`docs/synapse-rust/ROUTE_HANDLERS.json`）的 `query_unknown` 字段是从
源码里的 serde 属性**推导**出来的，它只能回答"现在是什么"，不能回答"应该是
什么"。这带来两个问题：

1. **度量会瞎**。2026-10-21 发现 `extract_registered._attrs_before` 从 `struct`
   关键字往后扫，遇到 `pub` 就放弃，导致所有 `pub struct` 的容器级
   `#[serde(...)]` 全部丢失 —— `parse_struct_deny_unknown` 恒为 False，
   203 条 typed 路由**全部**被报成 `ignore`，其中 55 条其实早已 deny。
   也就是说"reject=0"这个结论本身是度量缺陷的产物，不是代码现状。
2. **删属性不会红**。有人把 `deny_unknown_fields` 删掉、或者把 struct 挪给一个
   client 面路由共用，sidecar 只会安静地变一次，没有任何门禁会失败。

所以这里把**裁决**从推导结果里拆出来，落成一张静态表
（`scripts/contract/query_unknown_decisions.txt`），并强制它与 sidecar 双向一致：

* **覆盖**：sidecar 里每一条 typed 路由都必须在该表里恰好出现一次。
  新增一个 `Query<..>` 而不登记 ⇒ 红。
* **一致**：表里写 `reject` ⇔ sidecar 里就是 `reject`。给一个登记为
  `matrix-cs-ignore` 的 struct 加 deny ⇒ 红（必须先改裁决）。
* **陈旧**：表里的路由在 sidecar 里消失、或不再 typed ⇒ 红。
* **词表**：`decision` / `reason` 必须来自下面的封闭集合，且二者必须相容。

政策依据
--------
* `docs/audit/BACKEND_CONTRACT_SURFACE_PLAN_REVIEW_2026-10-19.md` §8.2：
  Query 面「保持开放，但必须逐条登记；收紧只带来 422」。
* `docs/audit/BACKEND_CONTRACT_SURFACE_VERIFICATION_2026-10-19.md` §3 N-1：
  「不能一刀切」，私有部署下 SDK 与服务端可共同发版才可以用 deny。
  落到本表就是：**项目私有、无外部客户端**的端点是唯一允许 `reject` 的增量。

用法
----
    python3 scripts/contract/check_query_unknown_decisions.py             # 门禁
    python3 scripts/contract/check_query_unknown_decisions.py --print      # 导出待编辑的表
    python3 scripts/contract/check_query_unknown_decisions.py --self-test  # 红/绿自证
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent
SIDECAR = ROOT / "docs" / "synapse-rust" / "ROUTE_HANDLERS.json"
REGISTRY = ROOT / "scripts" / "contract" / "query_unknown_decisions.txt"

# 裁决 → 允许的理由类。
REASONS_FOR = {
    "reject": {
        "already-deny",
        "internal-strict",
    },
    "ignore": {
        "matrix-cs-ignore",
        "vendor-open",
        "federation-forward-compat",
        "upstream-admin-parity",
        "external-idp",
        "shared-struct",
        "manual-deserialize",
        "compat-alias",
    },
}
ALL_REASONS = set().union(*REASONS_FOR.values())

COLUMNS = 5  # method, path, decision, reason, struct (+ 可选 note)


def load_sidecar(path=SIDECAR):
    with open(path, encoding="utf-8") as fh:
        doc = json.load(fh)
    typed = {}
    for route in doc["routes"]:
        unknown = route.get("query_unknown")
        if not unknown:
            continue
        key = (route["method"], route["path"])
        if key in typed:
            raise SystemExit(f"sidecar 里出现重复路由: {key}")
        typed[key] = {
            "decision": unknown,
            "struct": route.get("query_type") or "-",
            "file": route.get("file") or "-",
        }
    return typed


def parse_registry(path=REGISTRY):
    """返回 (rows, errors)。rows 是 key -> {decision, reason, struct, note, lineno}。"""
    rows = {}
    errors = []
    with open(path, encoding="utf-8") as fh:
        for lineno, raw in enumerate(fh, 1):
            line = raw.rstrip("\n")
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) < COLUMNS:
                errors.append(
                    f"{path.name}:{lineno}: 列数 {len(parts)} < {COLUMNS}（制表符分隔）"
                )
                continue
            method, route_path, decision, reason, struct = parts[:COLUMNS]
            note = parts[COLUMNS] if len(parts) > COLUMNS else ""
            key = (method, route_path)
            if key in rows:
                errors.append(
                    f"{path.name}:{lineno}: 重复登记 {method} {route_path}（上一次在第 {rows[key]['lineno']} 行）"
                )
                continue
            rows[key] = {
                "decision": decision,
                "reason": reason,
                "struct": struct,
                "note": note,
                "lineno": lineno,
            }
    return rows, errors


def evaluate(typed, rows):
    """纯函数：给定 sidecar 的 typed 路由与登记表，返回 (errors, warnings)。"""
    errors = []

    for key, row in rows.items():
        method, route_path = key
        where = f"{method} {route_path}"
        if row["decision"] not in REASONS_FOR:
            errors.append(
                f"第 {row['lineno']} 行 {where}: 未知裁决 {row['decision']!r}（只允许 reject / ignore）"
            )
        elif row["reason"] not in REASONS_FOR[row["decision"]]:
            if row["reason"] in ALL_REASONS:
                errors.append(
                    f"第 {row['lineno']} 行 {where}: 理由 {row['reason']!r} 与裁决 {row['decision']!r} 不相容"
                )
            else:
                errors.append(
                    f"第 {row['lineno']} 行 {where}: 未知理由 {row['reason']!r}"
                )

        if key not in typed:
            errors.append(
                f"第 {row['lineno']} 行 {where}: 陈旧登记（sidecar 里没有这条 typed 路由）"
            )
            continue
        actual = typed[key]["decision"]
        if row["decision"] != actual:
            errors.append(
                f"第 {row['lineno']} 行 {where}: 裁决 {row['decision']!r} 与 sidecar 实测 {actual!r} 不一致"
                f"（struct {typed[key]['struct']} @ {typed[key]['file']}）"
            )
        if row["struct"] != typed[key]["struct"]:
            errors.append(
                f"第 {row['lineno']} 行 {where}: 登记的 struct {row['struct']!r} 与 sidecar "
                f"{typed[key]['struct']!r} 不一致（结构体被换掉了？）"
            )

    for key in typed:
        if key not in rows:
            errors.append(
                f"{key[0]} {key[1]}: 未登记（struct {typed[key]['struct']} @ {typed[key]['file']}）"
                f" —— 新增 typed 路由必须先裁决"
            )

    warnings = []
    return errors, warnings


def print_skeleton(typed, rows):
    """导出全表：已有的行原样保留，新路由标 UNREVIEWED。"""
    print(
        "# 已有登记原样保留；新路由的 reason 一律是 UNREVIEWED，必须人工裁决后才能通过门禁。"
    )
    for key in sorted(typed, key=lambda k: (k[1], k[0])):
        method, route_path = key
        row = rows.get(key)
        if row:
            print(
                "\t".join(
                    [
                        method,
                        route_path,
                        row["decision"],
                        row["reason"],
                        row["struct"],
                        row["note"],
                    ]
                ).rstrip("\t")
            )
        else:
            print(
                "\t".join(
                    [
                        method,
                        route_path,
                        typed[key]["decision"],
                        "UNREVIEWED",
                        typed[key]["struct"],
                    ]
                )
            )


def self_test():
    typed = {
        ("GET", "/a"): {"decision": "reject", "struct": "QA", "file": "f.rs"},
        ("GET", "/b"): {"decision": "ignore", "struct": "QB", "file": "f.rs"},
    }

    def row(decision, reason, struct, lineno=1):
        return {
            "decision": decision,
            "reason": reason,
            "struct": struct,
            "note": "",
            "lineno": lineno,
        }

    cases = [
        (
            "全绿",
            {
                ("GET", "/a"): row("reject", "already-deny", "QA"),
                ("GET", "/b"): row("ignore", "matrix-cs-ignore", "QB"),
            },
            False,
        ),
        ("漏登记", {("GET", "/a"): row("reject", "already-deny", "QA")}, True),
        (
            "裁决与 sidecar 不一致",
            {
                ("GET", "/a"): row("ignore", "matrix-cs-ignore", "QA"),
                ("GET", "/b"): row("ignore", "matrix-cs-ignore", "QB"),
            },
            True,
        ),
        (
            "陈旧登记",
            {
                ("GET", "/a"): row("reject", "already-deny", "QA"),
                ("GET", "/b"): row("ignore", "matrix-cs-ignore", "QB"),
                ("GET", "/gone"): row("ignore", "matrix-cs-ignore", "QC", lineno=9),
            },
            True,
        ),
        (
            "未知理由",
            {
                ("GET", "/a"): row("reject", "because-i-said-so", "QA"),
                ("GET", "/b"): row("ignore", "matrix-cs-ignore", "QB"),
            },
            True,
        ),
        (
            "未知裁决",
            {
                ("GET", "/a"): row("maybe", "already-deny", "QA"),
                ("GET", "/b"): row("ignore", "matrix-cs-ignore", "QB"),
            },
            True,
        ),
        (
            "struct 被替换",
            {
                ("GET", "/a"): row("reject", "already-deny", "QA2"),
                ("GET", "/b"): row("ignore", "matrix-cs-ignore", "QB"),
            },
            True,
        ),
    ]
    failed = 0
    for name, rows, expect_error in cases:
        errors, _ = evaluate(typed, rows)
        ok = bool(errors) == expect_error
        print(f"  {'✅' if ok else '❌'} {name}: {len(errors)} error(s)")
        if errors:
            print(f"      first: {errors[0]}")
        failed += 0 if ok else 1
    if failed:
        print(f"❌ self-test 失败 {failed} 项")
        return 1
    print("✅ self-test 全部通过")
    return 0


def main(argv):
    if "--self-test" in argv:
        return self_test()

    typed = load_sidecar()
    rows, parse_errors = parse_registry()

    if "--print" in argv:
        for err in parse_errors:
            print(f"⚠️  {err}", file=sys.stderr)
        print_skeleton(typed, rows)
        return 0

    errors, warnings = evaluate(typed, rows)
    errors = parse_errors + errors

    for warning in warnings:
        print(f"⚠️  {warning}")
    if errors:
        for err in errors:
            print(f"❌ {err}")
        print(f"\n❌ query_unknown 登记表不合格：{len(errors)} 个问题")
        print(
            "   修复：编辑 scripts/contract/query_unknown_decisions.txt，不要在文档里手抄数字。"
        )
        return 1

    by_decision = {}
    for row in rows.values():
        by_decision[row["decision"]] = by_decision.get(row["decision"], 0) + 1
    print(
        f"✅ query_unknown 登记表一致：{len(rows)} 条 typed 路由"
        f"（reject {by_decision.get('reject', 0)} / ignore {by_decision.get('ignore', 0)}）"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
