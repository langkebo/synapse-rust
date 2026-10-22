#!/usr/bin/env python3
"""A3：把 `/_matrix/client/v1` 的退役面钉成**只减不增**的棘轮。

为什么需要这个门禁
------------------
`docs/audit/A3_v1_deprecation_strategy.md` 的计划是「v3 双注册 → v1 返 410 → 删注册」。
但"删注册"这一步的真正前置条件**不在本仓**：要先确认客户端已经全部改调 v3
（阶段 3 写明是客户端团队职责，`check_sdk_route_coverage.py` 读的是仓库外的 SDK 根，
证明不了线上客户端用的是哪个前缀）。没有这层证据就批量删 v1，等于拿兼容性赌运气。

所以这里**不删任何路由**，只做三件事：

1. 冻结当前规模：`/_matrix/client/v1` 的 (method, path) 条数与去重路径数**只减不增**；
2. 逐条锁住**没有 v3 孪生**的那批：它们是"删了就真断"的端点（客户端没有替代品），
   基线里每消失一条都必须显式改基线 —— 让删除成为可见决策，而不是随手一带；
3. 报出**可以退的队列**：有 v3 孪生且 handler 相同的条数，即阶段 5 的候选集。

数据源
------
`docs/synapse-rust/ROUTE_HANDLERS.json`（由 `gen_route_handlers.py` 生成，
在 `check_route_contract.sh` 里排在本步之前）。**不要**另写一套路由扫描 ——
那会造出第二份真相源，本仓已因此吃过多次亏。

用法
----
    python3 scripts/contract/check_v1_deprecation.py            # 门禁（默认）
    python3 scripts/contract/check_v1_deprecation.py --update   # 刻意收缩后重写基线
"""

from __future__ import annotations

import argparse
import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[2]
ROUTE_HANDLERS = ROOT / "docs" / "synapse-rust" / "ROUTE_HANDLERS.json"
SURFACE_BASELINE = ROOT / "scripts" / "contract" / "v1_surface_baseline"
NO_TWIN_BASELINE = ROOT / "scripts" / "contract" / "v1_no_twin_baseline.txt"

CLIENT = "/_matrix/client/"
V1 = CLIENT + "v1/"
V3 = CLIENT + "v3/"


def load_routes(path: pathlib.Path) -> list[dict]:
    if not path.is_file():
        sys.exit(
            f"FAIL 找不到 {path}；先跑 python3 scripts/contract/gen_route_handlers.py",
        )
    return json.loads(path.read_text(encoding="utf-8"))["routes"]


def split_twin(routes: list[dict]) -> tuple[list[dict], list[dict]]:
    """把 client/v1 路由分成「有 v3 孪生」与「没有」两组。

    孪生判定用 (method, 同后缀的 v3 路径)：只比路径会漏掉"同名不同方法"的情况，
    而那种情况删掉 v1 之后客户端换个 method 就 405。
    """
    index = {(r["method"], r["path"]) for r in routes}
    v1 = [r for r in routes if r["path"].startswith(V1)]
    paired, lone = [], []
    for route in v1:
        suffix = route["path"][len(V1) :]
        if (route["method"], V3 + suffix) in index:
            paired.append(route)
        else:
            lone.append(route)
    return paired, lone


def read_surface_baseline() -> dict[str, int]:
    values: dict[str, int] = {}
    if not SURFACE_BASELINE.is_file():
        return values
    for line in SURFACE_BASELINE.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, _, raw = line.partition("=")
        try:
            values[key.strip()] = int(raw.strip())
        except ValueError:
            sys.exit(f"FAIL {SURFACE_BASELINE.name} 里 {key} 不是整数：{raw!r}")
    return values


def read_no_twin_baseline() -> list[str]:
    if not NO_TWIN_BASELINE.is_file():
        return []
    return [
        line.strip()
        for line in NO_TWIN_BASELINE.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.strip().startswith("#")
    ]


def write_baselines(paired: list[dict], lone: list[dict]) -> None:
    surface = {
        "V1_ROUTES_MAX": len(paired) + len(lone),
        "V1_PATHS_MAX": len({r["path"] for r in paired + lone}),
    }
    SURFACE_BASELINE.write_text(
        "# `/_matrix/client/v1` 退役面棘轮（A3）。只减不增。\n"
        "#\n"
        "# V1_ROUTES_MAX = (method, path) 条数；V1_PATHS_MAX = 去重路径数。\n"
        "# 刻意收缩（删掉一批 v1 注册）之后，用 --update 重写本文件。\n"
        "# 判据与用法见 scripts/contract/check_v1_deprecation.py 的 docstring。\n"
        + "".join(f"{k}={v}\n" for k, v in surface.items()),
        encoding="utf-8",
    )
    NO_TWIN_BASELINE.write_text(
        "# 没有 v3 孪生的 `/_matrix/client/v1` 端点 —— **删了就真断**（客户端没有替代品）。\n"
        "#\n"
        "# 每消失一行都必须是刻意决定：先把替代路径（v3 双注册 / vendor 前缀）做实，\n"
        "# 再在本文件里删掉该行并写清迁移依据。门禁两个方向都查：\n"
        "#   * 真实路由里已不存在的条目 → 红（防止只删代码、忘删基线）；\n"
        "#   * 真实路由里新出现、基线未登记的无孪生 v1 端点 → 红（防止退役面反弹）。\n"
        + "".join(
            f"{r['method']} {r['path']}\n"
            for r in sorted(lone, key=lambda r: (r["path"], r["method"]))
        ),
        encoding="utf-8",
    )


def main() -> int:
    parser = argparse.ArgumentParser(description="A3: client/v1 退役面棘轮")
    parser.add_argument(
        "--update", action="store_true", help="按实测重写基线（刻意收缩后使用）"
    )
    parser.add_argument(
        "--json",
        type=pathlib.Path,
        default=ROUTE_HANDLERS,
        help="路由真相源（默认 docs/synapse-rust/ROUTE_HANDLERS.json；变异自证时指向临时副本）",
    )
    args = parser.parse_args()

    routes = load_routes(args.json)
    paired, lone = split_twin(routes)

    if args.update:
        write_baselines(paired, lone)
        print(
            f"OK 基线已重写：v1 {len(paired) + len(lone)} 条 / {len({r['path'] for r in paired + lone})} 路径；"
            f"无孪生 {len(lone)} 条",
        )
        return 0

    baseline = read_surface_baseline()
    if not baseline:
        sys.exit(f"FAIL 缺少 {SURFACE_BASELINE.name}；先跑一次 --update 生成基线")

    routes_now = len(paired) + len(lone)
    paths_now = len({r["path"] for r in paired + lone})
    failures: list[str] = []

    if routes_now > baseline["V1_ROUTES_MAX"]:
        failures.append(
            f"client/v1 路由从 {baseline['V1_ROUTES_MAX']} 涨到 {routes_now} —— "
            "A3 的方向是退役：新增端点请注册 v3，或私有扩展走 /_matrix/vendor/v1。"
            "（不想看全量清单的话，`git diff docs/synapse-rust/ROUTE_HANDLERS.json` 就是新增项）",
        )
    if paths_now > baseline["V1_PATHS_MAX"]:
        failures.append(
            f"client/v1 去重路径从 {baseline['V1_PATHS_MAX']} 涨到 {paths_now}"
        )

    expected = set(read_no_twin_baseline())
    actual = {f"{r['method']} {r['path']}" for r in lone}
    for gone in sorted(expected - actual):
        failures.append(
            f"无 v3 孪生的 v1 端点从路由里消失了：{gone}\n"
            "    客户端没有替代路径 ⇒ 要么先补 v3 双注册，要么把删除理由写进基线再删这一行",
        )
    for added in sorted(actual - expected):
        failures.append(
            f"新出现一个**没有 v3 孪生**的 v1 端点：{added}\n"
            "    这类端点天生不可退役；请改注册到 v3 或 /_matrix/vendor/v1",
        )

    twin_index = {(r["method"], r["path"]): r for r in routes}
    paired_same_handler = sum(
        1
        for r in paired
        if twin_index[(r["method"], V3 + r["path"][len(V1) :])]["registered_by"]
        == r["registered_by"]
    )

    print(
        f"client/v1 现状：{routes_now} 条 / {paths_now} 路径"
        f"（基线 {baseline['V1_ROUTES_MAX']} / {baseline['V1_PATHS_MAX']}）",
    )
    print(f"  有 v3 孪生（阶段 5 的候选队列）: {paired_same_handler} 条")
    print(f"  无孪生（不可退役）            : {len(lone)} 条")

    if routes_now < baseline["V1_ROUTES_MAX"] or paths_now < baseline["V1_PATHS_MAX"]:
        print(
            f"提示：实测低于基线（{routes_now}<{baseline['V1_ROUTES_MAX']} 或 "
            f"{paths_now}<{baseline['V1_PATHS_MAX']}）⇒ 用 --update 收紧，别留静默额度",
        )

    if failures:
        print("\nFAIL A3 v1 退役面棘轮：", file=sys.stderr)
        for item in failures:
            print(f"  - {item}", file=sys.stderr)
        return 1

    print("OK A3 v1 退役面未见反弹（只减不增）")
    return 0


if __name__ == "__main__":
    sys.exit(main())
