#!/usr/bin/env python3
"""check_capability_ledger.py — 能力台账「每条都可追溯」门禁（MAINT-04 / FUNC-05）。

## 它解决的问题

`docs/synapse-rust/API_COVERAGE_REPORT.md` 的 §6.2 / §6.5 / §6.6 是**手写台账**。历史上
同一批能力在同一份报告的不同小节里出现过三种状态（§6.1 的"纠偏"表就是三次翻车的记录），
根因是"结论可以随手写，没人能证伪"。本门禁把每条台账行变成**可机检的断言**：

  C1  每条数据行必须恰好携带一个判定词：`已实现` / `设计决策` / `未实现`。
      —— 没有判定词的行无法被读者判断是"能力有"还是"能力没有"，等于没写。
  C2  声明 `已实现` 的行必须至少给出一个**可解析的代码引用**（`path:line` 或裸文件名）：
      文件必须存在、行号必须在文件范围内。写错路径 / 行号漂移 ⇒ 红。
  C3  声明 `设计决策` 的行必须引用一个**决策编号**（`RD-15`、`FUNC-05` …）或一份存在
      的文档，且该编号必须真的出现在 `docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md` 里。
      —— "我们决定不做"必须能追到决策记录，否则只是"忘了做"的委婉说法。
  C4  §6.6「设计决策台账」里列为"不注册"的路径，**不得**出现在路由台账
      （`scripts/contract/ledger_annotations.txt` 的 `unsupported=true` 或派生路由表）里：
      两条台账对同一路径给出相反结论 ⇒ 红。

`--mutation-check` 自证：注入一条"`已实现` + 不存在的路径"和一条"`设计决策` + 不存在的编号"，
要求 C2 / C3 各自变红。一个不会失败的门禁不是门禁。

用法:
  python3 scripts/contract/check_capability_ledger.py
  python3 scripts/contract/check_capability_ledger.py --mutation-check
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
REPORT = ROOT / "docs" / "synapse-rust" / "API_COVERAGE_REPORT.md"
AUDIT = ROOT / "docs" / "audit" / "COMPREHENSIVE_AUDIT_2026-10-22.md"
ANNOTATIONS = ROOT / "scripts" / "contract" / "ledger_annotations.txt"

VERDICTS = ("已实现", "设计决策", "未实现")
SECTIONS = ("### 6.2", "### 6.5", "### 6.6")

# 反引号里的引用：`path/to/file.rs:12-34`、`file.rs:12`、`file.rs:66,179`、`file.rs`
REF_RE = re.compile(r"`([^`]+?\.(?:rs|md|toml|py|sh|txt|json)(?::\d+(?:[,\-]\d+)*)?)`")
DECISION_RE = re.compile(
    r"\b(?:RD|FUNC|ARCH|PERF|SEC|MAINT|CODE|TEST|NEW|DOC)-\d+[a-z]?\b"
)
PATH_IN_ROW_RE = re.compile(r"`(/_matrix/[^`]+|/_synapse/[^`]+)`")


class RowError(Exception):
    pass


def _repo_lines(path: Path) -> int:
    try:
        return len(path.read_text(encoding="utf-8").splitlines())
    except OSError as exc:  # pragma: no cover - defensive
        raise RowError(f"无法读取 {path}: {exc}") from exc


def _resolve_ref(ref: str) -> Path:
    """把 `path:line` / `a/b/c.rs:12-34` / 裸 `c.rs` 解析成仓库内文件（唯一命中才算数）。"""
    file_part = ref.split(":", 1)[0]
    candidate = ROOT / file_part
    if candidate.is_file():
        return candidate
    matches = [
        p
        for pattern in (
            "synapse-*/src/**/" + file_part,
            "src/**/" + file_part,
            "tests/**/" + file_part,
            "scripts/**/" + file_part,
            "docs/**/" + file_part,
        )
        for p in ROOT.glob(pattern)
        if p.is_file()
    ]
    if len(matches) == 1:
        return matches[0]
    if len(matches) > 1:
        raise RowError(f"引用 `{ref}` 有 {len(matches)} 个同名候选，无法唯一定位")
    raise RowError(f"引用 `{ref}` 指向的文件不存在")


def _check_ref(ref: str) -> None:
    path = _resolve_ref(ref)
    if ":" in ref:
        line_spec = ref.split(":", 1)[1]
        start = int(re.split(r"[,\-]", line_spec)[0])
        total = _repo_lines(path)
        if start < 1 or start > total:
            raise RowError(
                f"引用 `{ref}` 的行号越界（{path.relative_to(ROOT)} 只有 {total} 行）"
            )


def check_row(row: str, audit_text: str) -> None:
    """对单条台账行做 C1–C3。`row` 是去掉首尾 `|` 后的原始 Markdown 行。"""
    present = [v for v in VERDICTS if v in row]
    if not present:
        raise RowError("缺少判定词（已实现 / 设计决策 / 未实现）")
    if len(present) > 1:
        raise RowError(f"同时出现多个判定词 {present}，读者无法判断结论")

    verdict = present[0]

    if verdict == "已实现":
        refs = REF_RE.findall(row)
        if not refs:
            raise RowError("声明「已实现」却没有任何 `path:line` / 文件名引用")
        for ref in refs:
            _check_ref(ref)

    elif verdict == "设计决策":
        decisions = DECISION_RE.findall(row)
        if not decisions:
            raise RowError("声明「设计决策」却没有决策编号（如 RD-15 / FUNC-05）")
        missing = [d for d in decisions if d not in audit_text]
        if missing:
            raise RowError(f"决策编号 {missing} 在审计文档里找不到，等于没有决策记录")


def parse_rows(text: str) -> list[tuple[str, int, str]]:
    """返回 (section, line_number, row) —— 只取 `|` 表格的数据行。"""
    rows: list[tuple[str, int, str]] = []
    section = None
    for lineno, line in enumerate(text.splitlines(), start=1):
        stripped = line.strip()
        for marker in SECTIONS:
            if stripped.startswith(marker):
                section = marker
                break
        else:
            if section and stripped.startswith("|"):
                cells = stripped.strip("|")
                if cells.strip() and set(cells.replace(" ", "").replace("|", "")) <= {
                    "-",
                    ":",
                }:
                    continue  # 分隔行
                first_cell = cells.split("|", 1)[0].strip()
                if first_cell in {"项", "能力"}:
                    continue  # 表头
                if stripped.count("|") >= 2:
                    rows.append((section, lineno, cells))
            elif section and stripped.startswith("#"):
                section = None
    return rows


def parse_no_route_claims(text: str) -> list[str]:
    """§6.6 里形如 `| ... | `路径` | 不注册 | ... |` 的行，返回被声明为"不注册"的路径。"""
    claims: list[str] = []
    in_section = False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("### 6.6"):
            in_section = True
            continue
        if in_section and stripped.startswith("#"):
            break
        if in_section and stripped.startswith("|"):
            cells = [c.strip() for c in stripped.strip("|").split("|")]
            if len(cells) >= 3 and "不注册" in cells[2]:
                claims.extend(PATH_IN_ROW_RE.findall(cells[1]))
    return claims


def registered_unsupported_paths() -> set[str]:
    paths: set[str] = set()
    for line in ANNOTATIONS.read_text(encoding="utf-8").splitlines():
        if "unsupported=true" not in line:
            continue
        for token in line.split():
            if token.startswith("/"):
                paths.add(token)
    return paths


def run_checks(
    report_text: str, audit_text: str, no_route_paths: list[str], registered: set[str]
) -> list[str]:
    problems: list[str] = []
    for section, lineno, row in parse_rows(report_text):
        try:
            check_row(row, audit_text)
        except RowError as exc:
            problems.append(f"API_COVERAGE_REPORT.md:{lineno} ({section}) {exc}")
    for path in no_route_paths:
        if path in registered:
            problems.append(
                f"§6.6 声明 `{path}` 不注册，但 ledger_annotations.txt 里它是 unsupported=true（两条台账互相矛盾）"
            )
    return problems


def mutation_check(report_text: str, audit_text: str) -> int:
    """注入两类缺陷，要求 C2 / C3 分别变红。"""
    failures = 0

    bad_ref = "| **示例** | 已实现（`synapse-common/src/does_not_exist.rs:1`） | — |"
    try:
        check_row(bad_ref, audit_text)
    except RowError:
        print("  ✓ C2 变异：`已实现` + 不存在的路径 ⇒ 红")
    else:
        print("  ✗ C2 变异未被捕获：不存在的路径被判成可追溯")
        failures += 1

    bad_decision = "| **示例** | 设计决策（`RD-9999`） | — |"
    try:
        check_row(bad_decision, audit_text)
    except RowError:
        print("  ✓ C3 变异：`设计决策` + 不存在的编号 ⇒ 红")
    else:
        print("  ✗ C3 变异未被捕获：不存在的决策编号被判成有记录")
        failures += 1

    # 另自证 C4：把一个"不注册"路径塞进已登记集合，必须红。
    claims = parse_no_route_claims(report_text)
    if claims:
        poisoned = run_checks(report_text, audit_text, claims, set(claims))
        if any("互相矛盾" in p for p in poisoned):
            print("  ✓ C4 变异：`不注册` 的路径出现在 ledger_annotations ⇒ 红")
        else:
            print("  ✗ C4 变异未被捕获")
            failures += 1
    return failures


def main() -> int:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument(
        "--mutation-check", action="store_true", help="注入缺陷并要求门禁变红"
    )
    args = parser.parse_args()

    report_text = REPORT.read_text(encoding="utf-8")
    audit_text = AUDIT.read_text(encoding="utf-8")

    problems = run_checks(
        report_text,
        audit_text,
        parse_no_route_claims(report_text),
        registered_unsupported_paths(),
    )

    print("==> 能力台账可追溯性（MAINT-04 / FUNC-05）")
    print(
        "    C1 判定词 / C2 已实现可解析 / C3 设计决策有编号 / C4 不注册声明与路由台账不矛盾"
    )
    print(
        f"    台账行数: {len(parse_rows(report_text))}，不注册声明: {len(parse_no_route_claims(report_text))}"
    )

    for problem in problems:
        print(f"  ✗ {problem}")
    if problems:
        print(f"\n能力台账门禁失败：{len(problems)} 处")
        return 1

    print("  ✓ 全部通过")
    if args.mutation_check:
        failures = mutation_check(report_text, audit_text)
        if failures:
            return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
