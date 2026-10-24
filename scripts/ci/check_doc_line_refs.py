#!/usr/bin/env python3
"""DOC-04：文档里的 `path.rs:NNN` 引用必须还能指到"代码"，不能指向空行 / 纯注释 / 孤立的 `}`。

背景
----
审计 DOC-04 的原话是「三份文档共 9 组行号漂移，修法是改引用符号名而非行号」。实测远不止 9 组：
三份文档里有 145 处 `path:line`，而且**大部分在表格里** —— 那些行是
`scripts/contract/check_capability_ledger.py` 的 C2 判据（`已实现` 必须带可解析 `path:line`），
**不能**改成符号名，否则能力台账门禁立刻红。

所以本脚本按能力分层处理：
* **正文**里的行号引用已经全部改成符号名（见 `docs/synapse-rust/API_COVERAGE_REPORT.md`
  §8 的 `grep ... # 已实现：…` 块、`MSC_SEMANTICS.md` 的两处、`ROUTE_CONTRACT.md` 的一处）；
* **表格**里的 `path:line` 保留（机器要解析），改用本脚本守住"它至少还指着代码"：
  解析不到文件、行号越界、或该行是空行 / 纯注释 / 孤立的 `}` ⇒ 漂移信号，计入基线。

为什么只查这一档
----------------
"行号指对了但内容不是当初那句话"无法机器判定（没有 ground truth）。但**指向空行/注释/括号**
是确定的漂移 —— 它意味着这段代码被挪动过、引用已经失效。基线 `scripts/ci/doc_line_ref_baseline`
钉住当前值，只许下降。

判据（可失败）
--------------
在大括号闭合 / 注释 / 空行上新增一处 `path:line` 引用，或删掉被引用的行让已有引用越界 ⇒ 红。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BASELINE = ROOT / "scripts/ci/doc_line_ref_baseline"
DOCS = (
    "docs/synapse-rust/API_COVERAGE_REPORT.md",
    "docs/synapse-rust/MSC_SEMANTICS.md",
    "docs/synapse-rust/ROUTE_CONTRACT.md",
)
ROOTS = (
    "",
    "src/",
    "synapse-web/src/",
    "synapse-web/src/routes/",
    "synapse-common/src/",
    "synapse-services/src/",
    "synapse-storage/src/",
    "synapse-e2ee/src/",
    "synapse-cache/src/",
    "synapse-federation/src/",
    "synapse-test-utils/src/",
)
REF = re.compile(r"([\w./-]+\.rs):(\d+)(?:[-,](\d+))?")


def resolve(rel: str) -> Path | None:
    for prefix in ROOTS:
        candidate = ROOT / prefix / rel
        if candidate.exists():
            return candidate
    return None


def is_trivial(line: str) -> bool:
    stripped = line.strip()
    return stripped == "" or stripped == "}" or stripped.startswith(("//", "/*"))


def main() -> int:
    drift: list[str] = []
    total = 0
    for rel_doc in DOCS:
        doc = ROOT / rel_doc
        if not doc.exists():
            continue
        for lineno, line in enumerate(doc.read_text(encoding="utf-8").splitlines(), 1):
            for match in REF.finditer(line):
                target_rel, start = match.group(1), int(match.group(2))
                total += 1
                resolved = resolve(target_rel)
                if resolved is None:
                    drift.append(
                        f"{rel_doc}:{lineno}: `{match.group(0)}` 解析不到（试过 {len(ROOTS)} 个根）"
                    )
                    continue
                lines = resolved.read_text(
                    encoding="utf-8", errors="replace"
                ).splitlines()
                if start > len(lines):
                    drift.append(
                        f"{rel_doc}:{lineno}: `{match.group(0)}` 越界（{resolved.name} 只有 {len(lines)} 行）"
                    )
                elif is_trivial(lines[start - 1]):
                    drift.append(
                        f"{rel_doc}:{lineno}: `{match.group(0)}` 指到空行/注释/孤立括号 ⇒ 这段代码挪过位置"
                    )

    baseline = None
    if BASELINE.exists():
        for raw in BASELINE.read_text(encoding="utf-8").splitlines():
            if raw.strip().startswith("DRIFT="):
                baseline = int(raw.strip().split("=", 1)[1])

    print(
        f"三份文档共 {total} 处 `path.rs:line` 引用；指向空行/注释/括号或解析不到的 {len(drift)} 处"
        f"（基线 {baseline}）"
    )

    if "--print" in sys.argv:
        for item in drift:
            print("  " + item)
        return 0
    if "--update" in sys.argv:
        if baseline is not None and len(drift) > baseline:
            print(f"FAIL --update 只能收紧：{baseline} → {len(drift)}", file=sys.stderr)
            return 1
        BASELINE.write_text(
            "# DOC-04：三份文档里指向空行/注释/括号（或解析不到）的 `path.rs:line` 引用数，只许下降。\n"
            f"DRIFT={len(drift)}\n",
            encoding="utf-8",
        )
        print(f"OK 基线已收紧：DRIFT={len(drift)}")
        return 0

    if baseline is None:
        print("!! 缺少基线（跑 --update 建立）")
        return 1
    if len(drift) > baseline:
        for item in drift:
            print("  " + item)
        print(f"FAIL 漂移引用 {len(drift)} > 基线 {baseline}")
        return 1
    print(f"OK 漂移引用 {len(drift)} ≤ 基线 {baseline}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
