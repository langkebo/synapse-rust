#!/usr/bin/env python3
"""文档度量一致性门禁 —— 规模数字只能由脚本从真源算出，文档不得手抄。

── 为什么存在 ──────────────────────────────────────────────────────────────
审计（`docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md` TEST-07 / DOC-02 / DOC-09 /
PERF-12 / DOC-07）的核心判据是「同一事实多份表示」：测试文件数在四处文档里各写一份，
索引数在 `migrations/INDEXES.md` 里手写一份。只要有人新增一个测试文件，四处就开始
说谎，而且**没有任何东西会红** —— 这是本仓最常见的"文档腐烂"形态。

数字漂移史（同一条事实，五个读数）：`342 → 347 → 349 → 351(裸 grep，含注释行)`，
真实值现在是 **343**（口径见 `baseline_index_names()`）。

审计 §0.4「数字真值」也由本脚本守住（39 个内联锚点里的 27 个）。那张表原先只写
「机械扫描」而没有命令，于是 4 组数字各自漂移：写「84 列」同时列出 43 + 42、静默忽略
837 处（真值 830 行）、sleep 90 处（棘轮 104）、门禁 47 个（元门禁 58）。现在 sleep /
`updated_ts` / 静默忽略 / clippy allow 全部从**已有基线或棘轮 `scan()`** 现算 ——
复用而不是另写一套扫描器，避免出现第三套口径。「依赖分裂」「门禁脚本数」「>2000 行明细」
三行仍需人工复核：前两个要跑 `cargo tree` / 元门禁，成本高于其余扫描；明细行的守卫是
「生产 0」这一条锚点。

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
import functools
import importlib.util
import json
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


def _json_int(rel: str, dotted: str) -> int:
    """读 JSON 里的整数字段（`dotted` 支持 `counts.routes` 这种一层的点号路径）。"""
    node: object = json.loads((REPO / rel).read_text(encoding="utf-8"))
    for part in dotted.split("."):
        if not isinstance(node, dict) or part not in node:
            raise SystemExit(
                f"FAIL {rel}: 缺少字段 {dotted!r}（度量真源变了，先修本脚本）"
            )
        node = node[part]
    if not isinstance(node, int):
        raise SystemExit(f"FAIL {rel}: {dotted} 不是整数而是 {type(node).__name__}")
    return node


# ── 审计 §0.4「数字真值」的其余真源 ─────────────────────────────────────────
# 那一版曾把 4 组数字写成「机械扫描」而没有命令，结果各自漂移：`updated_ts` 写
# 「84 列」却同时写 43 + 42（= 85）、静默忽略 837 处（真值 830 行）、sleep 90 处
# （棘轮 104）、门禁 47 个（元门禁 58）。下面的度量全部从**已有棘轮/基线文件**现算，
# 不让脚本再发明第三套口径。
SLEEP_BASELINE = "scripts/ci/sleep_baseline"
SCHEMA_TIMESTAMP_BASELINE = "scripts/ci/schema_timestamp_baseline"
CLIPPY_RATCHET = "scripts/ci/check_clippy_allow_ratchet.py"

# 与 §0.4 的生产口径逐字一致：剔除 `tests/`、`*/tests.rs`、`*_tests.rs`。
TEST_PATH = re.compile(r"(^|/)tests?/|/tests?\.rs$|_tests\.rs$|^tests/")

# §0.4 的静默忽略口径是 `grep -c` 的**行**口径（一行两处只算一次），不是出现次数 ——
# 口径差异正是旧数字对不上的原因，这里在注释里钉死。
SILENT_IGNORE = re.compile(r"let _ =|if let Ok\(|unwrap_or_default\(\)")

# 生产源码根：与 `check_file_size_ratchet.py` / `check_clippy_allow_ratchet.py` 的九个根一致，
# 但**去掉** `synapse-test-utils/` —— 它是测试基建，§0.4 把它单列（`test-utils/lib.rs` 2974 行）。
PRODUCTION_ROOTS = (
    "synapse-common/src/",
    "synapse-cache/src/",
    "synapse-storage/src/",
    "synapse-e2ee/src/",
    "synapse-federation/src/",
    "synapse-services/src/",
    "synapse-web/src/",
    "src/",
)

# 按名字就是测试/测试基建的文件（§0.4 的「测试基建 2 / 测试 3」桶），不算生产。
TEST_INFRA_FILE = re.compile(r"(^|/)test(_?[a-z_]+)?\.rs$")


@functools.lru_cache(maxsize=4)
def baseline_kv(rel: str) -> dict[str, int]:
    """读 `<KEY>=<int>` 形态的棘轮基线（sleep / schema 两个基线都用这种格式）。"""
    values: dict[str, int] = {}
    for line in (REPO / rel).read_text(encoding="utf-8").splitlines():
        m = re.match(r"([A-Z][A-Z_]*)=(-?\d+)$", line.strip())
        if m:
            values[m.group(1)] = int(m.group(2))
    return values


@functools.lru_cache(maxsize=1)
def silent_ignore_counts() -> tuple[int, int, int, int]:
    """(生产行, 生产文件, 全仓行, 全仓文件) —— 受版本控制的 `.rs`。"""
    prod_lines = prod_files = all_lines = all_files = 0
    for path in git_files("*.rs"):
        try:
            text = (REPO / path).read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue
        hits = sum(1 for line in text.splitlines() if SILENT_IGNORE.search(line))
        if not hits:
            continue
        all_lines += hits
        all_files += 1
        if not TEST_PATH.search(path):
            prod_lines += hits
            prod_files += 1
    return prod_lines, prod_files, all_lines, all_files


@functools.lru_cache(maxsize=1)
def clippy_scan_totals() -> tuple[int, int, int]:
    """(全部 allow 站点, 含 allow 的文件, 扫描的文件) —— 直接复用棘轮自己的 `scan()`。

    复用而不是重写扫描器，是为了让「文档里的 159 / 111 / 828」与棘轮输出永远同源。
    """
    spec = importlib.util.spec_from_file_location(
        "clippy_allow_ratchet", REPO / CLIPPY_RATCHET
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    sites, scanned = module.scan()
    return len(sites), len({site.path for site in sites}), scanned


def prod_files_over_2000() -> int:
    """生产源码里 >2000 行的文件数（ARCH-05 的不变量：必须是 0）。"""
    return sum(
        1
        for path in git_files("*.rs")
        if path.startswith(PRODUCTION_ROOTS)
        and not path.endswith(".inc.rs")
        and not TEST_PATH.search(path)
        and not TEST_INFRA_FILE.search(path)
        and len((REPO / path).read_text(encoding="utf-8").splitlines()) > 2000
    )


def derived_table_rows() -> int:
    """`derived_route_table_*.inc.rs` 里 `rows.push(DerivedRoute` 的行数（去重前）。

    这个数只出现在文档的"981 行去重为 979 条"这句解释里 —— 它是**生成物**的属性，
    不是路由面的属性，所以不能从 `ROUTE_HANDLERS.json` 推。多 profile 会把同一条
    `(method, path)` 推两次，差额就是文档里说的"孪生行"。
    """
    total = 0
    for path in sorted(
        (REPO / "synapse-web/src/routes").glob("derived_route_table_*.inc.rs")
    ):
        total += sum(
            1
            for line in path.read_text(encoding="utf-8").splitlines()
            if "rows.push(DerivedRoute" in line
        )
    return total


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
    # 路由面：`ROUTE_CONTRACT.md` 的附录是"生成时原样保留"的人工段，里面的条数没有任何
    # 门禁看着（R6 只比对 route-table-sdk 与 fixture，不比对文档文字）。
    # 实测过：正文写 975/968、fixture 三连写 1029/1040/1048，而真值早已是 979/899。
    "ROUTE_TABLE_GOLDEN": lambda: _json_int(
        "docs/openapi/route-table.json", "total_routes"
    ),
    "ROUTE_TABLE_SDK": lambda: _json_int(
        "docs/openapi/route-table-sdk.json", "total_routes"
    ),
    "LIVE_ROUTES": lambda: _json_int(
        "docs/synapse-rust/ROUTE_HANDLERS.json", "counts.routes"
    ),
    "DERIVED_TABLE_ROWS": derived_table_rows,
    # ── 审计 §0.4「数字真值」的其余行（真源见文件头注释）──
    "CLIPPY_ALLOW_ALL_SITES": lambda: clippy_scan_totals()[0],
    "CLIPPY_ALLOW_ALL_FILES": lambda: clippy_scan_totals()[1],
    "CLIPPY_ALLOW_SCAN_FILES": lambda: clippy_scan_totals()[2],
    "SLEEP_SITES_PROD": lambda: baseline_kv(SLEEP_BASELINE)["PROD"],
    "SLEEP_SITES_TEST_INLINE": lambda: baseline_kv(SLEEP_BASELINE)["TEST_INLINE"],
    "SLEEP_SITES_TEST_FILE": lambda: baseline_kv(SLEEP_BASELINE)["TEST_FILE"],
    "SLEEP_SITES_TOTAL": lambda: sum(
        baseline_kv(SLEEP_BASELINE)[key] for key in ("PROD", "TEST_INLINE", "TEST_FILE")
    ),
    "SCHEMA_UPDATED_TS_STRICT": lambda: baseline_kv(SCHEMA_TIMESTAMP_BASELINE)[
        "STRICT"
    ],
    "SCHEMA_UPDATED_TS_LOOSE": lambda: baseline_kv(SCHEMA_TIMESTAMP_BASELINE)["LOOSE"],
    "SCHEMA_UPDATED_TS_TOTAL": lambda: sum(
        baseline_kv(SCHEMA_TIMESTAMP_BASELINE).values()
    ),
    "SILENT_IGNORES_PROD_LINES": lambda: silent_ignore_counts()[0],
    "SILENT_IGNORES_PROD_FILES": lambda: silent_ignore_counts()[1],
    "SILENT_IGNORES_ALL_LINES": lambda: silent_ignore_counts()[2],
    "SILENT_IGNORES_ALL_FILES": lambda: silent_ignore_counts()[3],
    "PROD_FILES_OVER_2000": prod_files_over_2000,
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
    # `ROUTE_CONTRACT.md` 的附录是"生成时原样保留"的人工段 —— 下面的数字以前没人看，
    # 实测已经腐坏到 975/968（真值 979/899）。只锚定"当前值"的表述，历史增量表
    # （E.13/E.14）用 `⚠️ 历史…当时值` 标注，不做锚点。
    (
        "docs/synapse-rust/ROUTE_CONTRACT.md",
        "ROUTE_TABLE_GOLDEN",
        r"golden × default \| \*\*(?P<v>\d+)\*\*",
        "E.12 表：route-table.json（golden × default）条数",
    ),
    (
        "docs/synapse-rust/ROUTE_CONTRACT.md",
        "ROUTE_TABLE_SDK",
        r"sdk × all \| \*\*(?P<v>\d+)\*\*",
        "E.12 表：route-table-sdk.json（sdk × all）条数",
    ),
    (
        "docs/synapse-rust/ROUTE_CONTRACT.md",
        "LIVE_ROUTES",
        r"本文件路由面（\*\*(?P<v>\d+)\*\* 条",
        "附录 B「后端事实源」：当前路由面条数",
    ),
    (
        "docs/synapse-rust/ROUTE_CONTRACT.md",
        "DERIVED_TABLE_ROWS",
        r"(?P<v>\d+) 行去重为 \d+ 条",
        "总览：派生表行数（去重前）",
    ),
    # ── 审计 §0.4「数字真值」─────────────────────────────────────────────
    # 这张表原先既没写命令、也没有门禁，于是 4 组数字各自漂移（写「84 列」却列出
    # 43 + 42、静默忽略 837 处、sleep 90 处、门禁 47 个）。以下锚点逐行钉住文案，
    # 任何一个数字与真源对不上都会红。
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "TEST_FILES_TESTS_DIR",
        r"\| 测试文件 \| \*\*(?P<v>\d+)\*\*（",
        "§0.4：测试文件总数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "TEST_FILES_TESTS_DIR_RS",
        r"测试文件 \| \*\*\d+\*\*（`\.rs` \*\*(?P<v>\d+)\*\*",
        "§0.4：其中 .rs 数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "TEST_FILES_UNIT",
        r"`\.rs` \*\*\d+\*\*；unit \*\*(?P<v>\d+)\*\*",
        "§0.4：unit 文件数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "TEST_FILES_INTEGRATION",
        r"unit \*\*\d+\*\* / integration \*\*(?P<v>\d+)\*\*",
        "§0.4：integration 文件数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "TEST_FILES_E2E",
        r"integration \*\*\d+\*\* / e2e \*\*(?P<v>\d+)\*\*",
        "§0.4：e2e 文件数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "TEST_FILES_PERFORMANCE",
        r"e2e \*\*\d+\*\* / perf \*\*(?P<v>\d+)\*\*",
        "§0.4：perf 文件数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "BASELINE_INDEXES",
        r"\| 基线索引定义 \| \*\*(?P<v>\d+)\*\*",
        "§0.4：基线索引定义数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "LIVE_ROUTES",
        r"活路由 / 派生表行 / `INDEXES\.md` 人工表 \| \*\*(?P<v>\d+)\*\*",
        "§0.4：活路由条数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "DERIVED_TABLE_ROWS",
        r"人工表 \| \*\*\d+\*\* / \*\*(?P<v>\d+)\*\*",
        "§0.4：派生表行数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "INDEXES_DOC_ROWS",
        r"人工表 \| \*\*\d+\*\* / \*\*\d+\*\* / \*\*(?P<v>\d+)\*\*",
        "§0.4：INDEXES.md 人工表行数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "CLIPPY_ALLOW_ALL_SITES",
        r"\| clippy allow \| \*\*(?P<v>\d+) 处",
        "§0.4：clippy allow 全部站点",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "CLIPPY_ALLOW_ALL_FILES",
        r"clippy allow \| \*\*\d+ 处 / (?P<v>\d+) 文件\*\*",
        "§0.4：含 allow 的文件数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "CLIPPY_ALLOW_SCAN_FILES",
        r"（扫描 \*\*(?P<v>\d+)\*\* 个 `\.rs`）",
        "§0.4：clippy allow 扫描文件数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "CLIPPY_ALLOW_SITES",
        r"其中无理由 \*\*(?P<v>\d+) / \d+\*\*",
        "§0.4：无理由 allow 站点（棘轮基线）",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "CLIPPY_ALLOW_FILES",
        r"其中无理由 \*\*\d+ / (?P<v>\d+)\*\*",
        "§0.4：无理由 allow 文件（棘轮基线）",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "SILENT_IGNORES_PROD_LINES",
        r"生产 \*\*(?P<v>\d+)\*\* 行 / \*\*\d+\*\* 文件；",
        "§0.4：静默忽略 —— 生产行数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "SILENT_IGNORES_PROD_FILES",
        r"生产 \*\*\d+\*\* 行 / \*\*(?P<v>\d+)\*\* 文件；",
        "§0.4：静默忽略 —— 生产命中文件数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "SILENT_IGNORES_ALL_LINES",
        r"全仓 \*\*(?P<v>\d+)\*\* 行 / \*\*\d+\*\* 文件 \|",
        "§0.4：静默忽略 —— 全仓行数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "SILENT_IGNORES_ALL_FILES",
        r"全仓 \*\*\d+\*\* 行 / \*\*(?P<v>\d+)\*\* 文件 \|",
        "§0.4：静默忽略 —— 全仓命中文件数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "SLEEP_SITES_TOTAL",
        r"`tokio::time::sleep` \| \*\*(?P<v>\d+)\*\* 站点",
        "§0.4：sleep 站点总数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "SLEEP_SITES_PROD",
        r"站点（PROD \*\*(?P<v>\d+)\*\*",
        "§0.4：sleep —— PROD 桶",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "SLEEP_SITES_TEST_INLINE",
        r"TEST_INLINE \*\*(?P<v>\d+)\*\*",
        "§0.4：sleep —— TEST_INLINE 桶",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "SLEEP_SITES_TEST_FILE",
        r"TEST_FILE \*\*(?P<v>\d+)\*\*",
        "§0.4：sleep —— TEST_FILE 桶",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "PROD_FILES_OVER_2000",
        r"\| `>2000` 行 `\.rs` \| 生产 \*\*(?P<v>\d+)\*\*",
        "§0.4：生产 >2000 行文件数（ARCH-05 不变量 0）",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "SCHEMA_UPDATED_TS_TOTAL",
        r"基线 SQL `updated_ts` 列 \| \*\*(?P<v>\d+) 列",
        "§0.4：基线 SQL updated_ts 列数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "SCHEMA_UPDATED_TS_STRICT",
        r"`updated_ts` 列 \| \*\*\d+ 列\*\*：`NOT NULL` \*\*(?P<v>\d+)\*\*",
        "§0.4：update_ts NOT NULL 列数",
    ),
    (
        "docs/audit/COMPREHENSIVE_AUDIT_2026-10-22.md",
        "SCHEMA_UPDATED_TS_LOOSE",
        r"`NOT NULL` \*\*\d+\*\* / 可空 \*\*(?P<v>\d+)\*\* \|",
        "§0.4：updated_ts 可空列数",
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
