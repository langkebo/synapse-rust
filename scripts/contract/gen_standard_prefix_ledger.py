#!/usr/bin/env python3
"""生成标准前缀私有端点台账 `standard_prefix_ledger.txt`。

用法
----
    python3 scripts/contract/extract_registered.py        # 先产出 artifacts/registered_routes.json
    python3 scripts/contract/gen_standard_prefix_ledger.py            # 打印到 stdout
    python3 scripts/contract/gen_standard_prefix_ledger.py --write    # 写回台账

为什么是"生成 + 人工评审"而不是纯生成
------------------------------------
台账的内容是**决策**（这条私有端点该删、该迁、还是本来就合法），不是可以从代码
推导出来的事实。本脚本只负责把「当前真实存在的 client 前缀路由」与「已知的
vendor 孪生关系」摆出来，并按规则给出**初判**；`policy=msc` 的条目与
`MIXED_MODULE_ROUTES` 里的逐条理由都需要人复核。因此：

* 生成保证**完整性**（不会漏掉任何一条真实路由，也不会留下不存在的条目）；
* 人工评审保证**正确性**（分类对不对）；
* `test_extract_registered.py::check_standard_prefix_bucket` 保证两者都**不会腐烂**。

判据来源
--------
分类依据是逐条对照 Matrix 规范/MSC 的**路径形状**，而不是模块名 ——
`room.rs` 有 97 条 client 前缀路由但其中 93 条是标准端点，
`handlers/thread.rs` 有 21 条但其中 18 条是 MSC3856；模块名分辨不出来。
详见 docs/前缀命名空间治理方案-2026-10-08.md §1。
"""

import argparse
import json
import os
import sys

SCRIPT_DIR = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, SCRIPT_DIR)
ROOT = os.path.abspath(os.path.join(SCRIPT_DIR, "..", ".."))
ARTIFACT = os.path.join(ROOT, "artifacts", "registered_routes.json")
LEDGER = os.path.join(SCRIPT_DIR, "standard_prefix_ledger.txt")

from standard_prefix_policy import (
    CLIENT_PREFIXES,
    MIXED_MODULE_ROUTES,
    VENDOR_PREFIX,
    WHOLESALE_PRIVATE_FILES,
    classify,
    normalize,
)

HEADER = """# 标准前缀私有端点台账（ISSUE-13 收口）
#
# 为什么有这份文件
# ----------------
# `/_matrix/client/{v1,v3}` 是 Matrix 稳定 CS 命名空间的领地，私有扩展应走
# `/_matrix/vendor/v1`。本项目目前仍有 <TOTAL> 条私有端点注册在 client 前缀下
# （其中 <WHOLESALE> 条落在"整模块私有"文件里，受 standard_prefix_policy.LEDGER_CEILING
# 的只减不增上限约束；余下 20 条在混合模块里，逐条登记但暂不做全覆盖断言）。
# 这份台账把「已知的私有面」逐条钉死，使它可以**只减不增**，并把每条的动作写清。
#
# 分类方法与依据见 docs/前缀命名空间治理方案-2026-10-08.md §1；
# 判据不是"模块名"，而是逐条对照规范/MSC 的路径形状
# （`room.rs` 97 条里 93 条标准、`handlers/thread.rs` 21 条里 18 条 MSC —— 模块名分辨不出来）。
#
# 双向棘轮（与 extract_unresolved_allowlist.txt 同款）
# --------------------------------------------------
#   * 新增一条 client 前缀私有端点而不登记 → 红；
#   * 台账里的条目在真实路由里已不存在 → 红（腐烂即红，防止台账只增不减）。
# 由 scripts/contract/test_extract_registered.py::check_standard_prefix_bucket 执行。
#
# 字段：<METHOD> <PATH>  # <policy> | <action> | <reason>  [<source file>]
#   policy  vendor        —— 私有面，唯一规范位置是 /_matrix/vendor/v1
#           mscNNNN       —— 有 MSC 归属，合法留在 client 前缀
#   action  delete-alias  —— vendor 孪生已存在，client 侧是死别名，按铁律 1 直接删
#           move-to-vendor—— 需要新增 vendor 挂载并移除 client 注册
#           keep          —— 有意保留（仅 mscNNNN）
#
# 另见 AGENTS.md 铁律 1（未发布项目无向后兼容义务）—— 因此本台账**不需要**
# retire_after / 兼容期一类字段：delete-alias 就是直接删。
#
# 生成：python3 scripts/contract/gen_standard_prefix_ledger.py --write
# 生成后需人工评审（初判来自路径形状规则，mscNNNN 与混合模块的逐条理由尤其需要复核）。
#
"""


def load_rows():
    if not os.path.exists(ARTIFACT):
        sys.exit(
            f"missing {ARTIFACT}\nrun: python3 scripts/contract/extract_registered.py"
        )
    with open(ARTIFACT) as fh:
        return json.load(fh)


def build_ledger(data) -> list:
    pairs = [(m, p, f) for f, ps in data["modules"].items() for m, p in ps]
    vendor_twins = {
        (m, normalize(p)) for m, p, _ in pairs if p.startswith(VENDOR_PREFIX)
    }

    rows = []

    for source in WHOLESALE_PRIVATE_FILES:
        for method, path in data["modules"].get(source, []):
            if not path.startswith(CLIENT_PREFIXES):
                continue
            policy, action, why = classify(method, path, vendor_twins)
            rows.append((method, path, policy, action, why, source))

    for source, known in MIXED_MODULE_ROUTES.items():
        for method, path in data["modules"].get(source, []):
            if not path.startswith(CLIENT_PREFIXES) or (method, path) not in known:
                continue
            policy, action, why = classify(method, path, vendor_twins)
            if action == "move-to-vendor":
                why = known[(method, path)]
            rows.append((method, path, policy, action, why, source))

    rows.sort(key=lambda r: (r[2], r[3], r[5], r[1], r[0]))
    return rows


def render(rows) -> str:
    # 用显式替换而不是 str.format：HEADER 里含 `{v1,v3}` 这类字面花括号，
    # format() 会把它当占位符（KeyError）。
    wholesale_total = sum(1 for r in rows if r[5] in WHOLESALE_PRIVATE_FILES)
    out = [
        HEADER.replace("<TOTAL>", str(len(rows))).replace(
            "<WHOLESALE>", str(wholesale_total)
        )
    ]
    by_action = {}
    for row in rows:
        by_action.setdefault((row[2], row[3]), []).append(row)
    for (policy, action), group in sorted(by_action.items()):
        out.append(f"\n## {policy} / {action}  （{len(group)} 条）\n")
        for method, path, _pol, _act, why, source in group:
            out.append(
                f"{method:6s} {path:64s} # {policy} | {action} | {why}  [{source}]\n"
            )
    return "".join(out)


def main() -> int:
    ap = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    ap.add_argument(
        "--write", action="store_true", help="写回 standard_prefix_ledger.txt"
    )
    args = ap.parse_args()

    text = render(build_ledger(load_rows()))
    if args.write:
        with open(LEDGER, "w") as fh:
            fh.write(text)
        print(f"wrote {os.path.relpath(LEDGER, ROOT)}")
    else:
        sys.stdout.write(text)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
