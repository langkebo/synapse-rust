#!/usr/bin/env python3
"""NEW-04：`scripts/...` 引用必须能解析到真实文件（只查"活文档"，不查归档）。

为什么要这条门禁而"朴素门禁"不行
--------------------------------
审计 NEW-04 原始记录是"30 处悬挂 `scripts/...` 引用"，并明确写了**不能**做"出现即要求存在"的
朴素检查：其中大量是

1. **相对路径**：`docker/deploy/README.md` 里写 `scripts/backup.sh`，真身是
   `docker/deploy/scripts/backup.sh`（Dockerfile 的 `WORKDIR` 就在 `docker/deploy`）；
2. **历史叙述**：`docs/archive/**` 里记的是"当时调用过 X，已删除"，要求它存在就是错的；
   还有 `.workbuddy/memory/**` 里的会话笔记。

所以本脚本只在**解析规则明确**的前提下判红：

* 域：受版本控制的活文件（`.rs/.sh/.py/.yml/.yaml/.md/.toml/.json/.mjs/.js`），
  **跳过** `docs/archive/`、`docs/audit/archive/`、`.workbuddy/`（历史与笔记，允许提消失的东西）；
* 解析顺序：① 仓库根相对；② `docker/deploy/` 相对（部署文档的默认工作目录）；
* 命中即通过；两条都不中 ⇒ 红。**唯一允许的例外**写在 `scripts/ci/script_ref_exceptions`：
  `<引用路径>|<为什么它不必存在>`（例如"文件名本身是反例，不是引用"）。

判据（可失败）
--------------
删掉任意一个被引用的脚本、或把引用写成不存在的名字 ⇒ 红；把 `docker/deploy/scripts/…`
错写成 `scripts/…` 而仓库根下也没有 ⇒ 红（这条正是本轮修掉的形态）。
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
EXCEPTIONS = ROOT / "scripts/ci/script_ref_exceptions"
BASELINE = ROOT / "scripts/ci/script_ref_baseline"

REF = re.compile(r"(?:\./)?(scripts/[A-Za-z0-9_./-]+\.(?:sh|py|mjs|js))")
SCANNED_SUFFIXES = (
    ".rs",
    ".sh",
    ".py",
    ".yml",
    ".yaml",
    ".md",
    ".toml",
    ".json",
    ".mjs",
    ".js",
)
SKIP_PREFIXES = (
    "docs/archive/",
    "docs/audit/archive/",
    ".workbuddy/",
    "target/",
    "vendor/",
)
RESOLVE_PREFIXES = ("", "docker/", "docker/deploy/")


def tracked() -> list[str]:
    out = subprocess.run(
        ["git", "ls-files"], cwd=ROOT, capture_output=True, text=True, check=True
    )
    return [p for p in out.stdout.splitlines() if p]


def load_exceptions() -> dict[str, str]:
    if not EXCEPTIONS.exists():
        return {}
    rows: dict[str, str] = {}
    for raw in EXCEPTIONS.read_text(encoding="utf-8").splitlines():
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        ref, _, reason = line.partition("|")
        rows[ref.strip()] = reason.strip()
    return rows


def resolves(ref: str) -> bool:
    return any((ROOT / prefix / ref).exists() for prefix in RESOLVE_PREFIXES)


def main() -> int:
    exceptions = load_exceptions()
    violations: list[str] = []
    total = 0
    for rel in tracked():
        if not rel.endswith(SCANNED_SUFFIXES) or rel.startswith(SKIP_PREFIXES):
            continue
        path = ROOT / rel
        if not path.exists():
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        for lineno, line in enumerate(text.splitlines(), 1):
            for match in REF.finditer(line):
                ref = match.group(1)
                total += 1
                if ref in exceptions or resolves(ref):
                    continue
                violations.append(
                    f"{rel}:{lineno}: `{ref}` 解析不到（试过仓库根与 docker/deploy/）"
                )

    print(
        f"扫描 {len(tracked())} 个受控文件，命中 `scripts/...` 引用 {total} 处；"
        f"例外登记 {len(exceptions)} 条"
    )

    def read_baseline() -> int | None:
        if not BASELINE.exists():
            return None
        for raw in BASELINE.read_text(encoding="utf-8").splitlines():
            line = raw.strip()
            if line.startswith("UNRESOLVED="):
                return int(line.split("=", 1)[1])
        return None

    if "--print" in sys.argv:
        for item in violations:
            print(item)
    elif "--update" in sys.argv:
        previous = read_baseline()
        if previous is not None and len(violations) > previous:
            print(
                f"FAIL --update 只能收紧：{previous} → {len(violations)}。"
                "新增的悬挂引用不是基线能批准的事（先改对路径，或登记 script_ref_exceptions）。",
                file=sys.stderr,
            )
            return 1
        BASELINE.write_text(
            "# NEW-04：活文档里解析不到的 `scripts/...` 引用数（只许下降）。\n"
            "# 明细用 `python3 scripts/ci/check_script_refs.py --print` 取。\n"
            f"UNRESOLVED={len(violations)}\n",
            encoding="utf-8",
        )
        print(f"OK 基线已收紧：UNRESOLVED={len(violations)}")
        return 0

    baseline = read_baseline()
    if baseline is not None and len(violations) <= baseline:
        print(f"OK 未解析引用 {len(violations)} ≤ 基线 {baseline}（只许下降）")
        return 0

    if violations:
        print(f"\n{len(violations)} 处解析不到（基线 {baseline}）：")
        for item in violations:
            print(f"  {item}")
        print(
            "\n修法：① 路径写错就改对（部署文档里的相对路径是相对 `docker/deploy/`）；"
            "② 脚本确实没了就把该句改成历史叙述并挪进 docs/archive/；"
            "③ 若引用本身是「反例/示例」而不是真实调用，登记到 scripts/ci/script_ref_exceptions。"
        )
        return 1
    print("结论：OK —— 活文档里的每一处 `scripts/...` 引用都能解析到真实文件")
    return 0


if __name__ == "__main__":
    sys.exit(main())
