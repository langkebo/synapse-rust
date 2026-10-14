#!/usr/bin/env python3
"""上游 changelog 对齐方案的「章节棘轮」—— 章节被吞掉必须判红。

为什么存在
----------
`docs/audit/UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md` 是 24 条上游对齐
处置的**唯一权威记录**，但它反复被一份**陈旧且重排过表格的副本**覆盖
（2026-10-14 之前 ≥4 次，当天又 2 次）。损伤是**静默**的：文件仍能解析、
markdownlint 仍绿，只是**整段章节消失**，要等到人重读才发现。

丢一节 = 丢掉一条结论。所以它必须和 `scripts/ci/check_file_size_ratchet.py`
一样有**基线 + 棘轮**：

* 基线：`scripts/ci/plan_doc_sections_baseline`（一行一个 `## `/`### ` 标题，按文档顺序）。
* **基线里有的标题在文档里找不到 ⇒ FAIL。**
* 新增章节、调整顺序**都不会红** —— 只有**丢**才会。
* `--update` 按当前文档重写基线（仅供**有意且已审阅**的改动，纪律同 file_size 基线）。

用法
----
    python3 scripts/ci/check_plan_doc_sections.py
    python3 scripts/ci/check_plan_doc_sections.py --doc <md> --baseline <file>   # 自测/变异用
    python3 scripts/ci/check_plan_doc_sections.py --update

为什么需要 `--doc`/`--baseline`
------------------------------
变异自证要能对着**副本**跑，而不是先把真文档改坏再赌自己记得还原。
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent

DEFAULT_DOC = (
    ROOT / "docs" / "audit" / "UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md"
)
DEFAULT_BASELINE = ROOT / "scripts" / "ci" / "plan_doc_sections_baseline"

# `## …` 与 `### …` 都算章节。只取标题行，忽略代码块里的 `#`（那些行不以 `#` 开头后接空格）。
HEADING = re.compile(r"^#{2,3} .+$", re.MULTILINE)


def headings_of(text: str) -> list[str]:
    """按文档顺序取出所有标题行（去掉行尾空白，避免硬换行造成假差异）。"""
    return [line.rstrip() for line in HEADING.findall(text)]


def read_baseline(path: Path) -> list[str]:
    """读基线：只把以 `##` 开头的行当标题，其余（含 `#` 注释与空行）忽略。

    这条规则让基线文件可以自带注释，而不会把注释误判成"缺失的章节"。
    """
    if not path.exists():
        return []
    out: list[str] = []
    for raw in path.read_text(encoding="utf-8").splitlines():
        line = raw.rstrip()
        if line.startswith("##"):
            out.append(line)
    return out


def write_baseline(path: Path, headings: list[str]) -> None:
    header = (
        "# 上游 changelog 对齐方案 —— 章节基线（棘轮）\n"
        "#\n"
        "# 由 `python3 scripts/ci/check_plan_doc_sections.py --update` 生成。\n"
        "# 任何一行在文档里消失 ⇒ `check_plan_doc_sections.py` 判红。\n"
        "# 见 `docs/audit/UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md` §11.9。\n"
        "#\n"
        "# 注意：本文件里以 `#`（单个）开头的是注释；以 `##` 开头的是被追踪的标题。\n"
    )
    path.write_text(header + "\n".join(headings) + "\n", encoding="utf-8")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--doc", type=Path, default=DEFAULT_DOC)
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--update", action="store_true", help="按当前文档重写基线")
    args = parser.parse_args()

    if not args.doc.exists():
        print(f"FAIL 找不到文档：{args.doc}")
        return 1

    current = headings_of(args.doc.read_text(encoding="utf-8"))

    if args.update:
        args.baseline.write_text("", encoding="utf-8")  # 先清，避免把旧标题留在文件尾
        write_baseline(args.baseline, current)
        print(f"OK 基线已更新：{args.baseline}（{len(current)} 条标题）")
        return 0

    baseline = read_baseline(args.baseline)
    if not baseline:
        print(f"FAIL 基线为空或不存在：{args.baseline}（先跑 --update）")
        return 1

    present = set(current)
    missing = [h for h in baseline if h not in present]

    if missing:
        print("FAIL 有章节被吞掉了（基线里有、文档里没有）：")
        for h in missing:
            print(f"  ✗ {h}")
        print()
        print(f"  基线 {len(baseline)} 条 / 文档 {len(current)} 条")
        print("  该文档反复被『陈旧且重排过表格的副本』覆盖（见文档 §11.9）。")
        print(
            "  恢复：`git checkout -- docs/audit/UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md`"
        )
        print(
            "  确有意的删除：`python3 scripts/ci/check_plan_doc_sections.py --update`"
        )
        return 1

    print(
        f"OK 章节棘轮通过：基线 {len(baseline)} 条全部在位（文档共 {len(current)} 条标题）"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
