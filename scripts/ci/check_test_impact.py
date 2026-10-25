#!/usr/bin/env python3
"""改动面 → 必跑套件：按 `scripts/ci/test_impact_map` 把"改了哪些文件"翻成"必须跑什么"。

## 解决什么

提交时选测试范围的判据一直是"我改了哪个文件"，而不是"这次改动影响哪些**契约**"。
2026-10-24 的 NEW-17 把滑动同步的 `rooms` 分桶后，`sync_path_equivalence_tests` 里三条
跨路径等价用例按扁平 `rooms[room_id]` 读，**静默红了近两小时** —— 当次只跑了
`api_sliding_sync_routes_tests` + service 单测，而等价用例不在任何 routes 测试的视野里。

## 为什么是"提示"而不是门禁

全量 integration 是 42 分钟（`ci.yml` 的 `--test integration --all-features`，1424 条）。
把它塞进 pre-commit 的唯一后果是所有人都学会加 `--no-verify`，门禁整体失效。
所以本脚本**永远 exit 0**，只把"该跑什么"打在提交前的屏幕上。

## 用法

    python3 scripts/ci/check_test_impact.py                # 对比 HEAD + 未跟踪文件
    python3 scripts/ci/check_test_impact.py --files a.rs b.rs
    python3 scripts/ci/check_test_impact.py --quiet         # 无命中时不打印

命中项按"动作"去重；每条给出可直接粘贴的命令。
"""

from __future__ import annotations

import argparse
import fnmatch
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
MAP = ROOT / "scripts/ci/test_impact_map"

# 与 CI 的 integration 车道同一套 feature 集（`ci.yml` 的 `--test integration`）。
INTEGRATION_FEATURES = "test-utils,privacy-ext,voice-extended,voip-tracking,beacons,server-notifications,widgets"
VALID_ACTIONS = ("nextest:", "shell:", "note:")


def _git(*args: str) -> list[str]:
    result = subprocess.run(["git", *args], cwd=ROOT, capture_output=True, text=True)
    return [line.strip() for line in result.stdout.splitlines() if line.strip()] if result.returncode == 0 else []


def changed_files(whole_tree: bool = False) -> list[str]:
    """默认只取**已暂存**的文件 —— 那才是"本次提交"。

    多会话共用一个工作区时（本仓常态），用 `git diff HEAD` 会把别人的在飞改动也报进来，
    提示很快就变成噪声、然后被无视。暂存区为空（还没 `git add`，或用了
    `git commit -- <pathspec>`）时回退到"HEAD 差异 + 未跟踪"，宁可多报也不漏报。
    """
    if not whole_tree:
        staged = _git("diff", "--cached", "--name-only")
        if staged:
            return sorted(set(staged))
    return sorted(set(_git("diff", "--name-only", "HEAD")) | set(_git("ls-files", "--others", "--exclude-standard")))


def load_map() -> list[tuple[str, str, str]]:
    """`<glob> | <动作> | <理由>`；`note:` 动作可省第三段（它本身就是要人工确认的话）。"""
    entries: list[tuple[str, str, str]] = []
    for lineno, raw in enumerate(MAP.read_text().splitlines(), 1):
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        parts = raw.split("|", 2)
        if len(parts) == 2 and parts[1].strip().startswith("note:"):
            pattern, action, reason = parts[0].strip(), parts[1].strip(), ""
        elif len(parts) == 3:
            pattern, action, reason = (part.strip() for part in parts)
        else:
            print(f"FAIL {MAP.name}:{lineno} 格式应为 `<glob> | <动作> | <理由>`：{raw.strip()[:60]}")
            sys.exit(1)
        if not action.startswith(VALID_ACTIONS):
            print(f"FAIL {MAP.name}:{lineno} 动作必须以 {' / '.join(VALID_ACTIONS)} 开头：{action}")
            sys.exit(1)
        entries.append((pattern, action, reason))
    return entries


def render(action: str) -> str:
    kind, _, payload = action.partition(":")
    if kind == "nextest":
        return (
            f'cargo nextest run --test integration --features "{INTEGRATION_FEATURES}" '
            f"-E '{payload}'"
        )
    return payload


def main() -> int:
    parser = argparse.ArgumentParser(add_help=True)
    parser.add_argument("--files", nargs="*", default=None, help="显式给改动文件（默认看暂存区）")
    parser.add_argument("--all", action="store_true", help="改看整棵工作区（含别人未提交的改动）")
    parser.add_argument("--quiet", action="store_true", help="无命中时不打印")
    args = parser.parse_args()

    if args.files is not None:
        files = args.files
    else:
        files = changed_files(whole_tree=args.all)
    entries = load_map()

    hits: list[tuple[str, str, str, list[str]]] = []
    for pattern, action, reason in entries:
        matched = [f for f in files if fnmatch.fnmatch(f, pattern)]
        if matched:
            hits.append((pattern, action, reason, matched))

    if not hits:
        if not args.quiet:
            print(f"改动面 → 必跑套件：对比 {len(files)} 个文件，未命中任何映射（无需额外套件）")
        return 0

    print(f"改动面 → 必跑套件：对比 {len(files)} 个文件，命中 {len(hits)} 条映射")
    for pattern, action, reason, matched in hits:
        print(f"\n  命中 {pattern}（{len(matched)} 个文件，如 {matched[0]}）")
        print(f"    → {render(action)}")
        if reason:
            print(f"      理由：{reason}")

    unique: dict[str, list[str]] = {}
    for _, action, reason, _ in hits:
        unique.setdefault(render(action), []).append(reason or action.partition(":")[2])
    print("\n  汇总（去重，命令可直接粘贴）：")
    for command, reasons in unique.items():
        if command.startswith(("cargo", "bash", "python3")):
            print(f"    {command}")
        else:
            print(f"    ⚠️ 人工确认：{command}")
    print(
        "\n  ⚠️ 这是**提示不是门禁**：本脚本恒 exit 0。全量 integration 42 分钟，"
        "塞进 pre-commit 只会把人逼向 --no-verify。"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
