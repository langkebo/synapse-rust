#!/usr/bin/env python3
"""文档度量一致性门禁 —— 规模数字只能由脚本从真源算出，文档不得手抄。

── 为什么存在 ──────────────────────────────────────────────────────────────
审计（`docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md` TEST-07 / DOC-02 / DOC-09 /
PERF-12 / DOC-07）的核心判据是「同一事实多份表示」：测试文件数在四处文档里各写一份，
索引数在 `migrations/INDEXES.md` 里手写一份。只要有人新增一个测试文件，四处就开始
说谎，而且**没有任何东西会红** —— 这是本仓最常见的"文档腐烂"形态。

数字漂移史（同一条事实，五个读数）：`342 → 347 → 349 → 351(裸 grep，含注释行)`，
真实值现在是 **343**（口径见 `baseline_index_names()`）。

── 判据 ────────────────────────────────────────────────────────────────────
1. **块**（`<!-- doc-metrics:start --> … :end -->`）内每个 `KEY = VALUE`：
   KEY 必须是已知度量（防"文档自造一个数字"），VALUE 必须等于真源实算值。
2. **受管键**（`BLOCKS` 里列出的）在该块的文档里**必须出现**（防"新增度量后忘了写"）。
3. **内联锚点**（`INLINE_SITES`）：正则里的 `(?P<v>\\d+)` 必须等于度量值。
   正则**不匹配**也算 FAIL —— 说明那句话被改写了，需要同步更新本文件的 spec
   （这是有意的：让"改文案"这件事无法静默抛下数字）。
4. 内联锚点所在文件**不存在**（`.trae/` 被 gitignore，CI checkout 里就没有）⇒ 打印
   `SKIP` 而不失败。这与 `docs-quality-gate.yml` 对 `.trae/` 的既定处理一致：宁可
   在 CI 不检查，也不要为了「看起来覆盖了」写一条永远红的或永远绿的守卫。

── 用法 ────────────────────────────────────────────────────────────────────
    python3 scripts/ci/check_doc_metrics.py            # 检查（默认）
    python3 scripts/ci/check_doc_metrics.py --print    # 打印可直接粘贴的块
    python3 scripts/ci/check_doc_metrics.py -v         # 附带每个度量的算法说明

改完文档后**不要手改数字**：跑 `--print` 粘贴，再跑一次本脚本复验。
"""

from __future__ import annotations

import argparse
import re
import subprocess
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
BLOCK_BEGIN = "<!-- doc-metrics:start -->"
BLOCK_END = "<!-- doc-metrics:end -->"

# baseline SQL 的索引解析口径，**逐字镜像**
# `tests/unit/migration_consistency_tests.rs::baseline_declares_each_object_exactly_once`
# 的四种前缀。两处口径一旦分叉，守卫测试与本文档门禁会给出两个"权威"数字，
# 那正是本轮要消灭的东西（判据 1）。
INDEX_PREFIXES = (
    "CREATE UNIQUE INDEX CONCURRENTLY IF NOT EXISTS ",
    "CREATE INDEX CONCURRENTLY IF NOT EXISTS ",
    "CREATE UNIQUE INDEX IF NOT EXISTS ",
    "CREATE INDEX IF NOT EXISTS ",
)
BASELINE_SQL = "migrations/00000000_unified_schema_v12.sql"
INDEXES_DOC = "migrations/INDEXES.md"
CLIPPY_BASELINE = "scripts/ci/clippy_allow_baseline"


def git_files(*pathspecs: str) -> list[str]:
    """受版本控制的文件清单（域与审计口径一致：排除 `target/` 与未跟踪文件）。"""
    proc = subprocess.run(
        ["git", "-C", str(REPO), "ls-files", *pathspecs],
        capture_output=True,
        text=True,
        check=True,
    )
    return [line for line in proc.stdout.splitlines() if line.strip()]


def _rs_under(prefix: str) -> int:
    return sum(
        1 for p in git_files("tests") if p.startswith(prefix) and p.endswith(".rs")
    )


def baseline_index_names() -> list[str]:
    """按守卫测试口径解析基线 SQL 的索引名（跳过整行注释，只认四种前缀）。"""
    text = (REPO / BASELINE_SQL).read_text(encoding="utf-8")
    names: list[str] = []
    for line in text.splitlines():
        stripped = line.lstrip()
        if stripped.startswith("--"):
            continue
        for prefix in INDEX_PREFIXES:
            if stripped.startswith(prefix):
                rest = stripped[len(prefix) :]
                m = re.match(r"[^\s(;]+", rest)
                if m:
                    names.append(m.group(0))
                break
    return names


def indexes_doc_rows() -> int:
    """`INDEXES.md` 人工表里登记的索引行数（含划删除线的历史行，它们是文档事实）。"""
    text = (REPO / INDEXES_DOC).read_text(encoding="utf-8")
    return len(
        re.findall(
            r"^\|\s*[A-Za-z_][A-Za-z0-9_]*\s*\|\s*~{0,2}\s*idx_[A-Za-z0-9_]+\s*~{0,2}\s*\|",
            text,
            re.MULTILINE,
        )
    )


def clippy_allow_counts() -> tuple[int, int]:
    """(未带理由的 allow 站点数, 涉及的格文件数) —— 直接读棘轮基线，不重跑扫描器。"""
    sites = 0
    files = 0
    for line in (REPO / CLIPPY_BASELINE).read_text(encoding="utf-8").splitlines():
        s = line.strip()
        if not s or s.startswith("#"):
            continue
        m = re.match(r"(\d+)\s+(\S+)$", s)
        if m:
            sites += int(m.group(1))
            files += 1
    return sites, files


METRICS = {
    "TEST_FILES_TESTS_DIR": lambda: len(git_files("tests")),
    "TEST_FILES_TESTS_DIR_RS": lambda: len(
        [p for p in git_files("tests") if p.endswith(".rs")]
    ),
    "TEST_FILES_UNIT": lambda: _rs_under("tests/unit/"),
    "TEST_FILES_INTEGRATION": lambda: _rs_under("tests/integration/"),
    "TEST_FILES_E2E": lambda: _rs_under("tests/e2e/"),
    "TEST_FILES_PERFORMANCE": lambda: _rs_under("tests/performance/"),
    "BASELINE_INDEXES": lambda: len(baseline_index_names()),
    "INDEXES_DOC_ROWS": indexes_doc_rows,
    "CLIPPY_ALLOW_SITES": lambda: clippy_allow_counts()[0],
    "CLIPPY_ALLOW_FILES": lambda: clippy_allow_counts()[1],
}

# 块所在文档 → 该块必须出现的受管键。
BLOCKS: dict[str, tuple[str, ...]] = {
    "TESTING.md": (
        "TEST_FILES_TESTS_DIR",
        "TEST_FILES_TESTS_DIR_RS",
        "TEST_FILES_UNIT",
        "TEST_FILES_INTEGRATION",
        "TEST_FILES_E2E",
        "TEST_FILES_PERFORMANCE",
    ),
    INDEXES_DOC: (
        "BASELINE_INDEXES",
        "INDEXES_DOC_ROWS",
    ),
}

# (文档, 度量键, 正则, 说明) —— 正则必须含一个 `(?P<v>\d+)`。
INLINE_SITES: tuple[tuple[str, str, str, str], ...] = (
    (
        "docs/synapse-rust-vs-synapse-comparison.md",
        "TEST_FILES_TESTS_DIR",
        r"(?P<v>\d+) 个文件（`git ls-files tests",
        "对比文档「测试文件数」行：受管文件总数",
    ),
    (
        "docs/synapse-rust-vs-synapse-comparison.md",
        "TEST_FILES_TESTS_DIR_RS",
        r"其中 `\.rs` (?P<v>\d+) 个",
        "对比文档「测试文件数」行：其中 .rs 数",
    ),
    (
        "docs/synapse-rust-vs-synapse-comparison.md",
        "TEST_FILES_UNIT",
        r"`tests/unit`\((?P<v>\d+) 文件\)",
        "对比文档「测试分层」行：unit",
    ),
    (
        "docs/synapse-rust-vs-synapse-comparison.md",
        "TEST_FILES_INTEGRATION",
        r"`tests/integration`\((?P<v>\d+)\)",
        "对比文档「测试分层」行：integration",
    ),
    (
        "docs/synapse-rust-vs-synapse-comparison.md",
        "TEST_FILES_E2E",
        r"`tests/e2e`\((?P<v>\d+)\)",
        "对比文档「测试分层」行：e2e",
    ),
    (
        "docs/synapse-rust-vs-synapse-comparison.md",
        "TEST_FILES_PERFORMANCE",
        r"`tests/performance`\((?P<v>\d+)\)",
        "对比文档「测试分层」行：performance",
    ),
    (
        ".trae/rules/project_rules.md",
        "TEST_FILES_UNIT",
        r"unit/ +# 单元测试（约 (?P<v>\d+) 个文件）",
        "工作区规则 §9.1：unit 文件数",
    ),
    (
        ".trae/rules/project_rules.md",
        "TEST_FILES_INTEGRATION",
        r"integration/ +# 集成测试（约 (?P<v>\d+) 个文件）",
        "工作区规则 §9.1：integration 文件数",
    ),
)


def compute() -> dict[str, int]:
    return {name: fn() for name, fn in METRICS.items()}


def render_block(values: dict[str, int]) -> str:
    lines = [BLOCK_BEGIN]
    lines.append(
        "<!-- 本块由 scripts/ci/check_doc_metrics.py --print 生成；手改必红。 -->"
    )
    lines.append("")
    for name in sorted(values):
        lines.append(f"{name}={values[name]}")
    lines.append("")
    lines.append(BLOCK_END)
    return "\n".join(lines)


def check_blocks(values: dict[str, int], problems: list[str], notes: list[str]) -> int:
    checked = 0
    for rel, managed in BLOCKS.items():
        path = REPO / rel
        if not path.exists():
            problems.append(f"{rel}: 块声明在这里，但文件不存在")
            continue
        text = path.read_text(encoding="utf-8")
        start = text.find(BLOCK_BEGIN)
        end = text.find(BLOCK_END)
        if start < 0 or end < start:
            problems.append(f"{rel}: 缺少 {BLOCK_BEGIN} … {BLOCK_END} 块")
            continue
        body = text[start + len(BLOCK_BEGIN) : end]
        seen: dict[str, int] = {}
        for raw in body.splitlines():
            line = raw.strip()
            if not line or line.startswith("<!--"):
                continue
            m = re.match(r"([A-Z][A-Z0-9_]*)\s*=\s*(-?\d+)$", line)
            if not m:
                problems.append(
                    f"{rel}: 块内无法解析的行 {line!r}（格式必须是 KEY=VALUE）"
                )
                continue
            key, val = m.group(1), int(m.group(2))
            if key not in METRICS:
                problems.append(f"{rel}: 块里的 {key} 不是已知度量（不许自造数字）")
                continue
            if key in seen:
                problems.append(f"{rel}: {key} 在块里出现了两次")
                continue
            seen[key] = val
            checked += 1
            if val != values[key]:
                problems.append(f"{rel}: {key} 写着 {val}，实测 {values[key]}")
        for key in managed:
            if key not in seen:
                problems.append(f"{rel}: 块里缺受管键 {key}（新增度量后忘了写进文档）")
        notes.append(f"{rel}: 块内 {len(seen)} 个度量，受管 {len(managed)} 个")
    return checked


def check_inline(values: dict[str, int], problems: list[str], notes: list[str]) -> int:
    checked = 0
    for rel, key, pattern, desc in INLINE_SITES:
        path = REPO / rel
        if not path.exists():
            notes.append(f"SKIP {desc}: {rel} 不存在（gitignore 或不在此 checkout 中）")
            continue
        text = path.read_text(encoding="utf-8")
        m = re.search(pattern, text)
        if m is None:
            problems.append(
                f"{rel}: 锚点失效（{desc}）—— 文案被改写了，请同步更新 check_doc_metrics.py 的 spec"
            )
            continue
        checked += 1
        if int(m.group("v")) != values[key]:
            problems.append(f"{rel}: {desc} 写着 {m.group('v')}，实测 {values[key]}")
    return checked


def main() -> int:
    ap = argparse.ArgumentParser(description="文档度量一致性门禁")
    ap.add_argument(
        "--print", action="store_true", help="打印可直接粘贴的 doc-metrics 块"
    )
    ap.add_argument(
        "-v", "--verbose", action="store_true", help="列出参与校验的锚点与算法"
    )
    args = ap.parse_args()

    values = compute()

    if args.print:
        print(render_block(values))
        return 0

    problems: list[str] = []
    notes: list[str] = []
    n_block = check_blocks(values, problems, notes)
    n_inline = check_inline(values, problems, notes)

    if args.verbose:
        for note in notes:
            print(f"  {note}")
        print(f"  块内条目 {n_block} 个 / 内联锚点 {n_inline} 个")

    if problems:
        print(f"FAIL 文档度量与真源不一致（{len(problems)} 处）：", file=sys.stderr)
        for p in problems:
            print(f"  - {p}", file=sys.stderr)
        print(
            "\n修法：python3 scripts/ci/check_doc_metrics.py --print 取最新块并粘贴；\n"
            "      内联数字改成 --print 里的同名键值（不要手算）。",
            file=sys.stderr,
        )
        return 1

    print(
        f"OK 文档度量一致（块内 {n_block} 项 / 内联 {n_inline} 处；"
        f"测试文件 {values['TEST_FILES_TESTS_DIR_RS']} 个 = "
        f"{values['TEST_FILES_UNIT']}/{values['TEST_FILES_INTEGRATION']}/"
        f"{values['TEST_FILES_E2E']}/{values['TEST_FILES_PERFORMANCE']}；"
        f"基线索引 {values['BASELINE_INDEXES']} 条）"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
