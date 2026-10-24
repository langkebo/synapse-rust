#!/usr/bin/env python3
"""NEW-05：不许再有"名字声称覆盖、内容自己造字面量"的测试文件。

判定口径（已定稿，见 `shape_only_test_matrix` 头部）：① 接真 ⇒ 名字可继续叫 `*_route_tests`；
② 只锁形状 ⇒ 文件名带 `_shape_only_tests` + 文件头声明「只锁形状」+ 登记真覆盖文件或 `NONE`。

四条不变式：
* C1 登记：`tests/**/*_shape_only_tests.rs` 必须在表里，且文件头 20 行内有「只锁形状」声明；
* C2 真覆盖：表里填的覆盖文件必须存在，且**真的**发请求 / 碰 DB
  （`oneshot` / `Request::builder` / `create_router` / `setup_fresh_test_app` / `sqlx::query` …）；
* C3 名字承诺：`tests/**/*_{api,route}_tests.rs` 必须至少调用一个 `synapse_*::` 生产符号
  **或**含真实请求标记 —— 名字声称覆盖 API/路由却两样都没有 ⇒ 红；
* C4 过期条目：登记的文件不存在 / 改名没同步 / 声明被摘 ⇒ 红。

为什么不做"内容启发式"门禁：按"≥3 处本地字面量断言"判会**高估**（insta 快照看起来一样可疑，
但它断言真结构体序列化）。判定必须落在文件上由人写下来，本脚本只做机械校验 —— 而 C3 这条
名字级规则恰好抓到了启发式漏掉的 13 个同模板文件（它们 `.get(..).is_some()` 不足 3 处）。
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "scripts/ci/shape_only_test_matrix"
DECLARATION = "只锁形状"
REAL_MARKERS = (
    "oneshot",
    "ServiceExt",
    "Request::builder",
    "create_router",
    "setup_fresh_test_app",
    "sqlx::query",
    "with_local_connect_info",
    "connect_shared_test_pool",
    "get_test_pool_async",
)
PROD_CALL = re.compile(r"\bsynapse_[a-z_]+::")
ROW_RE = re.compile(r"^(tests/[\w./-]+\.rs)\|(NONE|tests/[\w./-]+\.rs)$")
SHAPE_NAME = re.compile(r"_shape_only_tests\.rs$")
CLAIM_NAME = re.compile(r"_(api|route)_tests\.rs$")


def tracked(prefix: str) -> list[str]:
    out = subprocess.run(
        ["git", "ls-files", prefix],
        cwd=ROOT,
        capture_output=True,
        text=True,
        check=True,
    )
    return [p for p in out.stdout.splitlines() if p.endswith(".rs")]


def has_real_marker(path: Path) -> bool:
    src = path.read_text(encoding="utf-8", errors="replace")
    return any(marker in src for marker in REAL_MARKERS)


def main() -> int:
    if not REGISTRY.exists():
        print(
            f"::error::{REGISTRY.relative_to(ROOT)} 不存在（只锁形状的测试文件没有登记处）"
        )
        return 1

    rows: dict[str, str] = {}
    bad: list[str] = []
    for raw in REGISTRY.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = ROW_RE.match(line)
        if not m:
            bad.append(
                f"登记行格式不合法（应为 `tests/…rs|NONE` 或 `tests/…rs|tests/…rs`）：{line}"
            )
            continue
        rows[m.group(1)] = m.group(2)

    shape_files = [p for p in tracked("tests") if SHAPE_NAME.search(p)]
    covered = gaps = 0
    print(f"登记 {len(rows)} 行；`*_shape_only_tests.rs` 实际 {len(shape_files)} 个")
    print()
    print(f"{'只锁形状的文件':56} {'真覆盖（HTTP/DB）':50} 状态")

    for path, covering in sorted(rows.items()):
        f = ROOT / path
        if not f.exists():
            bad.append(f"{path}：登记行指向的文件不存在（过期条目）")
            state = "RED 文件不存在"
        elif not SHAPE_NAME.search(path):
            bad.append(f"{path}：登记了但不是 `*_shape_only_tests.rs` 命名")
            state = "RED 命名不符"
        else:
            head = "\n".join(
                f.read_text(encoding="utf-8", errors="replace").splitlines()[:20]
            )
            if DECLARATION not in head:
                bad.append(
                    f"{path}：文件头 20 行内没有「{DECLARATION}」声明（只有名字不算说清楚）"
                )
                state = "RED 缺声明"
            elif covering == "NONE":
                gaps += 1
                state = "OK（已登记缺口）"
            else:
                cov = ROOT / covering
                if not cov.exists():
                    bad.append(f"{path}：真覆盖文件 {covering} 不存在")
                    state = "RED 覆盖缺失"
                elif not has_real_marker(cov):
                    bad.append(
                        f"{path}：真覆盖文件 {covering} 里没有任何真实依赖标记（它自己也不发请求）"
                    )
                    state = "RED 覆盖不真"
                else:
                    covered += 1
                    state = "OK"
        print(f"{path:56} {covering:50} {state}")

    for path in sorted(set(shape_files) - set(rows)):
        bad.append(
            f"{path}：叫 `_shape_only_tests` 但不在 {REGISTRY.relative_to(ROOT)} 里"
        )

    claims = [
        p for p in tracked("tests") if CLAIM_NAME.search(p) and not SHAPE_NAME.search(p)
    ]

    def touches_production(path: str) -> bool:
        src = (ROOT / path).read_text(encoding="utf-8", errors="replace")
        return bool(PROD_CALL.search(src)) or has_real_marker(ROOT / path)

    offenders = [p for p in claims if not touches_production(p)]
    for path in offenders:
        bad.append(
            f"{path}：文件名声称覆盖 API/路由，但既不调用 `synapse_*::` 生产符号、也不发请求 "
            f"⇒ 它测的是自己构造的字面量（接真，或改名为 `*_shape_only_tests.rs` 并登记）"
        )

    print()
    print(
        f"统计：只锁形状 {len(rows)} 个（有真覆盖 {covered} / 已登记缺口 {gaps}）；"
        f"`*_api/_route_tests.rs` 共 {len(claims)} 个，其中不碰生产代码 {len(offenders)} 个"
    )
    if bad:
        print()
        for item in bad:
            print(f"::error::{item}")
        print(
            f"\n结论：{len(bad)} 项违规（C1 登记 / C2 真覆盖 / C3 名字承诺 / C4 过期条目）"
        )
        return 1
    print("结论：OK —— 没有再冒充覆盖的测试文件；缺口已逐条登记为 NONE")
    return 0


if __name__ == "__main__":
    sys.exit(main())
