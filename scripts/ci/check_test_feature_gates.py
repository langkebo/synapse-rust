#!/usr/bin/env python3
"""TEST-06：整文件 `#![cfg(feature = "X")]` 的测试文件必须真的被某条车道执行。

为什么需要它
------------
`tests/**` 里有 5 个文件用**整文件** feature 门控（`#![cfg(feature = "…")]`）。门控一旦
不在测试车道的 feature 集里，文件**根本不参与编译**：`cargo nextest run --test unit`
不会报错、不会提示、连"0 个用例"都不打印 —— 它只是不存在。于是：

* 改坏了也绿（这是 TEST-06 的原始形态，比"只断言状态码"更隐蔽）；
* 文件被删/改名/门控被删，同样无声无息。

已有的两道守卫都盖不住这一面：`check_gated_module_tests.sh` 只看**生产 lib** 的
`#[cfg(feature)] pub mod m;`（它要求"过滤器命中 ≥1 用例"，前提是模块已被登记），
`check_feature_matrix.py` 只看 `[features]` 自身的一致性。

本脚本的三个不变式
------------------
* C1 扫描：`git ls-files 'tests/**/*.rs'` 里每个带整文件门控的文件都必须在
  `scripts/ci/feature_gated_test_matrix` 登记（一行一个，含门控 feature / 目标 / 过滤器）；
* C2 接线：登记行的 `(target, filter)` 必须能在 `.github/workflows/ci.yml` 的某条
  `nextest run` 命令行里同时找到，且该行**启用了这条 feature**（显式 `--features` 列出，
  或 `--all-features`）。用 `--all-features` 命中的行标记为 `ALL_FEATURES` 并在结论里
  单列 —— 卡片的判据要求"不得用 `--all-features` 掩盖某个文件其实没被编译"，所以
  两种命中必须可区分，而不是混成一个绿；
* C3 反向：登记行必须仍然对应"带该门控的真实文件"，否则报**过期条目**（文件删了、
  改名了、门控被摘掉了 —— 这三种都会让 C1 变成空转）。

判据（可变异）
--------------
* 新增一个整文件门控的测试文件而不登记 ⇒ C1 红；
* 把登记行的 feature 改错 / 把 ci.yml 里那条步骤的 `--features` 去掉 ⇒ C2 红；
* 把某个文件的 `#![cfg(feature = …)]` 删掉 ⇒ C3 红（登记行过期）。

退出码：0 = 全部通过；1 = 有违规（明细在 stdout）。
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REGISTRY = ROOT / "scripts/ci/feature_gated_test_matrix"
CI_YML = ROOT / ".github/workflows/ci.yml"

# 整文件门控：必须出现在**文件级**（行首 `#![cfg(...)]`）。
FILE_GATE_RE = re.compile(r'^#!\[cfg\(feature\s*=\s*"([^"]+)"\)\]')
# 登记行：`tests/unit/x.rs|feature|unit|filter_token`
#
# `filter_token` 写 `*` 表示"整条车道都跑这个 target"（车道没有 `-E` 过滤器，例如
# `--test unit` 的主车道）；写具名 token 表示车道必须用 `-E 'test(<token>)'` 把它选出来。
# 两种形态都要能证明，所以不对 `*` 做特殊照顾。
ROW_RE = re.compile(
    r"^(tests/[\w./-]+\.rs)\|([\w-]+)\|(unit|integration|e2e|performance)\|(\*|[\w:]+)$"
)


def git(args: list[str]) -> str:
    out = subprocess.run(
        ["git", *args], cwd=ROOT, capture_output=True, text=True, check=True
    )
    return out.stdout


def scan_gated_files() -> dict[str, str]:
    """`{path: gate_feature}`，只含**整文件**门控的测试文件。"""
    tracked = [p for p in git(["ls-files", "tests"]).splitlines() if p.endswith(".rs")]
    gated: dict[str, str] = {}
    for rel in tracked:
        for line in (
            (ROOT / rel).read_text(encoding="utf-8", errors="replace").splitlines()
        ):
            m = FILE_GATE_RE.match(line)
            if m:
                gated[rel] = m.group(1)
                break
    return gated


def join_continuations(text: str) -> list[str]:
    """把 YAML 里 `\\` 续行的命令拼成单行，避免"参数在下一行"导致漏判。"""
    logical: list[str] = []
    buf = ""
    for raw in text.splitlines():
        line = raw.rstrip()
        if line.endswith("\\"):
            buf += line[:-1] + " "
            continue
        buf += line
        logical.append(buf)
        buf = ""
    if buf:
        logical.append(buf)
    return logical


def lane_lines() -> list[str]:
    return [
        ln
        for ln in join_continuations(CI_YML.read_text(encoding="utf-8"))
        if "nextest run" in ln
    ]


def features_of(line: str) -> list[str]:
    m = re.search(r"--features[ =]\"?([\w,+-]+)\"?", line)
    return m.group(1).split(",") if m else []


def find_lane(
    lines: list[str], target: str, filter_token: str, feature: str
) -> tuple[str, str] | None:
    """返回 `(命中方式, 命中行)`；命中方式 ∈ {EXPLICIT, ALL_FEATURES}。"""
    explicit: tuple[str, str] | None = None
    via_all: tuple[str, str] | None = None
    for line in lines:
        if f"--test {target}" not in line:
            continue
        if filter_token != "*" and filter_token not in line:
            continue
        if feature in features_of(line):
            explicit = explicit or ("EXPLICIT", line.strip())
        elif "--all-features" in line:
            via_all = via_all or ("ALL_FEATURES", line.strip())
    return explicit or via_all


def main() -> int:
    if not REGISTRY.exists():
        print(
            f"::error::{REGISTRY.relative_to(ROOT)} 不存在；整文件门控的测试文件没有登记处"
        )
        return 1
    if not CI_YML.exists():
        print(f"::error::{CI_YML.relative_to(ROOT)} 不存在")
        return 1

    rows: dict[str, tuple[str, str, str]] = {}
    bad: list[str] = []
    for raw in REGISTRY.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        m = ROW_RE.match(line)
        if not m:
            bad.append(f"登记行格式不合法（应为 `path|feature|target|filter`）：{line}")
            continue
        path, feature, target, token = m.groups()
        if path in rows:
            bad.append(f"{path} 被登记了两次")
        rows[path] = (feature, target, token)

    gated = scan_gated_files()
    lines = lane_lines()

    print(
        f"扫描：`git ls-files tests` 中整文件门控的测试文件 {len(gated)} 个；登记 {len(rows)} 行"
    )
    print()
    print(f"{'文件':52} {'门控 feature':22} {'车道':12} {'命中方式':13} 状态")

    status_rows = 0
    for path, (feature, target, token) in sorted(rows.items()):
        status_rows += 1
        hit: tuple[str, str] | None = None
        if not (ROOT / path).exists():
            bad.append(f"{path}：登记行指向的文件不存在（过期条目）")
            state = "RED 文件不存在"
        elif path not in gated:
            bad.append(
                f"{path}：文件里已没有整文件 `#![cfg(feature = …)]` 门控（登记行过期）"
            )
            state = "RED 门控已消失"
        elif gated[path] != feature:
            bad.append(f"{path}：门控已变成 `{gated[path]}`，登记写的是 `{feature}`")
            state = "RED 门控不符"
        else:
            hit = find_lane(lines, target, token, feature)
            if hit is None:
                bad.append(
                    f"{path}：ci.yml 里没有一条 `nextest run --test {target}` 同时启用 `{feature}` "
                    f"并匹配过滤器 `{token}` ⇒ 该文件在 CI 里可能根本不参与编译"
                )
                state = "RED 无车道"
            else:
                state = "OK" if hit[0] == "EXPLICIT" else "OK（仅 ALL_FEATURES）"
        print(
            f"{path:52} {feature:22} {target:12} {(hit[0] if hit else '-'):13} {state}"
        )

    unregistered = sorted(set(gated) - set(rows))
    for path in unregistered:
        bad.append(
            f"{path}：整文件门控 `{gated[path]}`，但不在 {REGISTRY.relative_to(ROOT)} 里（改坏了也不会有任何车道发现）"
        )

    print()
    if unregistered:
        print(
            f"未登记的整文件门控文件 {len(unregistered)} 个：" + ", ".join(unregistered)
        )
    if bad:
        print()
        for item in bad:
            print(f"::error::{item}")
        print(f"\n结论：{len(bad)} 项违规（C1 扫描 / C2 接线 / C3 过期条目）")
        return 1
    print(
        f"结论：OK —— {status_rows} 个整文件门控的测试文件都登记且都落在一条启用其 feature 的车道上"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
