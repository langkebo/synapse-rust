#!/usr/bin/env python3
"""上游 changelog 对齐方案的「章节棘轮」—— 章节被吞掉必须判红。

为什么存在
----------
`docs/audit/UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md` 是 24 条上游对齐
处置的**唯一权威记录**，但它反复被一份**陈旧且重排过表格的副本**覆盖
（2026-10-14 实测 5 次）。损伤是**静默**的：文件仍能解析、markdownlint 仍绿，
只是**整段章节消失**，要等到人重读才发现。

丢一节 = 丢掉一条结论。所以它必须和 `scripts/ci/check_file_size_ratchet.py`
一样有**基线 + 棘轮**。

两条判据（都要过）
------------------
1. **基线棘轮**：`scripts/ci/plan_doc_sections_baseline` 里的标题在文档里找不到 ⇒ FAIL。
   新增章节、调整顺序**都不会红** —— 只有**丢**才会。
2. **对照 HEAD**（`--no-head` 可关，`--doc` 自测模式自动关）：
   `git show HEAD:<doc>` 里的每个标题都必须仍在本文件里 ⇒ 否则 FAIL。
   判据 2 不依赖基线（不需要 `--update` 维护），因此**基线万一被误更新也拦得住**，
   且能抓到基线尚未收录的新章节。

判据 2 是 2026-10-14 加了第 5 次故障之后补的：根因是**宿主 app 进程**在回放它自己
捕获的那份旧副本（证据见文档 §11.9 / §11.12），从仓库侧无法阻止写入
—— 能做的是让"被吞"**不可能静默**。

用法
----
    python3 scripts/ci/check_plan_doc_sections.py
    python3 scripts/ci/check_plan_doc_sections.py --doc <md> --baseline <file>   # 自测/变异用
    python3 scripts/ci/check_plan_doc_sections.py --update
    python3 scripts/ci/check_plan_doc_sections.py --no-head                      # 只跑基线棘轮

为什么需要 `--doc`/`--baseline`
------------------------------
变异自证要能对着**副本**跑，而不是先把真文档改坏再赌自己记得还原。
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent.parent

DEFAULT_DOC = (
    ROOT / "docs" / "audit" / "UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md"
)
DEFAULT_BASELINE = ROOT / "scripts" / "ci" / "plan_doc_sections_baseline"

# `## …` / `### …` / `#### …` 都算章节（`####` 用于 §11.7.1 这类子小节 ——
# 第 4 次故障吞的就是一个 `####`）。忽略代码块里的 `#`（那些行不以 `#`+空格 开头）。
HEADING = re.compile(r"^#{2,4} .+$", re.MULTILINE)


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


def headings_in_head(rel_path: str) -> list[str] | None:
    """`HEAD:` 版本里的标题；取不到（无 git / 未跟踪 / 尚未提交）时返回 `None`。

    取不到就**跳过**判据 2，而不是判红 —— 一个在第一提交前必然失败的守卫只会被绕过。
    """
    try:
        out = subprocess.run(
            ["git", "show", f"HEAD:{rel_path}"],
            cwd=ROOT,
            capture_output=True,
            text=True,
            timeout=30,
            check=False,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    if out.returncode != 0:
        return None
    return headings_of(out.stdout)


def report(missing: list[str], total: int, present_count: int, why: str) -> None:
    print(f"FAIL {why}")
    for h in missing:
        print(f"  ✗ {h}")
    print()
    print(f"  {total} 条中缺 {len(missing)} 条 / 文档现有 {present_count} 条标题")
    print(
        "  该文档反复被『陈旧且重排过表格的副本』覆盖（根因：宿主 app 进程回放其缓存副本，"
    )
    print("  证据见文档 §11.9 / §11.12）。**内容以 git 为准**：")
    print(
        "  恢复：`git checkout -- docs/audit/UPSTREAM_CHANGELOG_ALIGNMENT_PLAN_2026-10-13.md`"
    )
    print("  确有意的删除：`python3 scripts/ci/check_plan_doc_sections.py --update`")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--doc", type=Path, default=None, help="自测用：改为检查这份文档"
    )
    parser.add_argument("--baseline", type=Path, default=DEFAULT_BASELINE)
    parser.add_argument("--update", action="store_true", help="按当前文档重写基线")
    parser.add_argument("--no-head", action="store_true", help="跳过『对照 HEAD』判据")
    args = parser.parse_args()

    doc = (args.doc or DEFAULT_DOC).resolve()

    if not doc.exists():
        print(f"FAIL 找不到文档：{doc}")
        return 1

    current = headings_of(doc.read_text(encoding="utf-8"))

    if args.update:
        args.baseline.write_text("", encoding="utf-8")  # 先清，避免把旧标题留在文件尾
        write_baseline(args.baseline, current)
        print(f"OK 基线已更新：{args.baseline}（{len(current)} 条标题）")
        return 0

    present = set(current)
    failed = False

    # ── 判据 1：基线棘轮 ───────────────────────────────────────────────
    baseline = read_baseline(args.baseline)
    if not baseline:
        print(f"FAIL 基线为空或不存在：{args.baseline}（先跑 --update）")
        return 1
    missing_baseline = [h for h in baseline if h not in present]
    if missing_baseline:
        failed = True
        report(
            missing_baseline,
            len(baseline),
            len(current),
            "基线里有、文档里没有的章节：",
        )

    # ── 判据 2：对照 HEAD（不依赖基线）─────────────────────────────────
    # 只在"检查的就是本仓那份文档"时跑：`--doc` 指向别处（变异副本）时无意义。
    head_checked = False
    head_note = ""
    if not args.no_head and args.doc is None:
        try:
            rel = doc.relative_to(ROOT).as_posix()
        except ValueError:
            rel = None
        if rel is not None:
            head = headings_in_head(rel)
            if head is None:
                head_note = "，对照 HEAD 已跳过（取不到 HEAD 版本）"
            else:
                head_checked = True
                missing_head = [h for h in head if h not in present]
                if missing_head:
                    failed = True
                    if missing_baseline:
                        print()
                    report(
                        missing_head,
                        len(head),
                        len(current),
                        "HEAD 里有、工作区副本里没有的章节（= 被陈旧副本覆盖的特征）：",
                    )

    if failed:
        return 1

    parts = [f"基线 {len(baseline)} 条"]
    if head_checked:
        parts.append("对照 HEAD 一致")
    print(
        f"OK 章节棘轮通过：{'、'.join(parts)}全部在位（文档共 {len(current)} 条标题）{head_note}"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
