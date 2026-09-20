#!/usr/bin/env python3
"""
Schema-blind guard anti-regression lint (rewritten 2026-09-17, M-3).

目的：抓住"hardcoded schema 字面量（通常是 'public'）导致守卫在非 public
schema 上失效"这一类缺陷（B3-3 / F 系列问题的根源模式）。

判据（吸取 2026-09-17 第三轮复核教训，旧的判据双向错误）：

  【报 error】
  1. Rust 代码里 `current_schema()` 之后的字符串字面量比较用了硬编码
     'public'（守卫盲区：在 test_template_ci 等隔离 schema 上失效）。
     —— 但白名单：显式操作 public 的守卫文件（已知合法）。
  2. `DROP SCHEMA public CASCADE` 出现在非豁免文件中。

  【报 warning（不挡 CI）】
  3. SQL/脚本里 `table_schema = 'public'` 字面量（可能是合法默认，人工复核）。
  4. PGOPTIONS search_path 含 public。

  【豁免机制】（旧脚本的致命伤：定义了 SAFE_* 却从不使用）
  - 文件级豁免：SAFE_FILES 列出的初始化/重置脚本按设计操作 public。
  - 每个 SafeFile 条目必须附带理由注释，防止豁免清单腐烂。

退出码：任何 error → 1；只有 warning → 0（打印汇总供人工复核）。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# ── 豁免清单（旧脚本定义了两份 SAFE_DROP_PATTERNS 且都从未被使用）──────────
# 每个条目必须带理由。豁免的是"整个文件对该 pattern 的命中"。
SAFE_FILES: dict[str, str] = {
    # 这些脚本按设计就是要操作/重置 public schema（测试库初始化）
    "scripts/init_test_public_schema.sh": "controlled public-schema reset by design",
    "scripts/reset_database_v12.sh": "DB initialization script, operates on public by design",
    "scripts/cleanup_test_schemas.sh": "test-schema cleanup, public references are intentional",
    "scripts/ci/prepare_test_db.sh": "CI seed script, seeds public + template schemas by design",
    # 测试隔离框架：明确在受控的 test database 上重置 public schema
    "synapse-test-utils/src/lib.rs": "test isolation framework, guarded DROP public with deploy detection",
    # test_isolation.rs 里的 DROP 字面量是 SQL 注入防御的测试 payload（不是真实 DROP）
    "synapse-common/src/test_isolation.rs": "test payloads for SQL injection defense — DROP strings are test inputs, not real operations",
    # CI 工作流：明确针对 synapse 生产库的迁移验证，非 schema-blind guard 缺陷
    ".github/workflows/ci.yml": "CI migration validation against synapse production DB, public refs are intentional",
    ".github/workflows/db-tests-manual.yml": "Manual db test script targeting synapse DB, public schema explicit by design",
    ".github/workflows/drift-detection.yml": "Drift detection baseline apply to synapse DB, public schema is target",
    # database_initializer：生产代码迁移后完整性验证，非守卫缺陷
    "synapse-services/src/database_initializer/mod.rs": "Production migration validation & normalization utility, not a schema-blind guard",
}

# ── 检查目标 ──────────────────────────────────────────────────────────────────
# 重点：Rust 代码（旧脚本只扫 sql/sh/yml，恰好漏掉了 schema_health_check.rs
# 这类真正出过 B3-3 问题的 Rust 守卫文件 —— 这是"假阴性"的主因）。
SCAN_GLOBS = [
    "migrations/**/*.sql",
    "scripts/**/*.sh",
    ".github/workflows/*.yml",
    "src/**/*.rs",
    "synapse-*/src/**/*.rs",
    "src/bin/*.rs",
]

# ── patterns ─────────────────────────────────────────────────────────────────
# (regex, message, severity)
# severity: 'error' → CI 失败；'warning' → 打印但通过
PATTERNS: list[tuple[str, str, str]] = [
    # 1. 危险的 public DROP —— 非豁免文件中出现即 error
    (
        r"DROP\s+SCHEMA\s+(?:IF\s+EXISTS\s+)?public\s+CASCADE",
        "DROP SCHEMA public CASCADE outside allowlisted reset scripts",
        "error",
    ),
    # 2. Rust 里的 schema 字面量比较（B3-3 类缺陷的直接特征）
    #    例如: current_schema() == Some("public") / schema != "public"
    (
        r"""current_schema\(\)\s*[=!]=\s*(?:Some\()?["']public["']""",
        "hardcoded 'public' comparison against current_schema() — guard is schema-blind "
        "(fails on isolated test schemas); use the configured schema name instead",
        "error",
    ),
    # 3. SQL catalog 查询硬编码 table_schema='public'（守卫只看 public → 盲区）
    #    注：以下文件中的 table_schema='public' 已确认为合法用途（SAFE_FILES 中有理由），
    #    但仍报 warning 保留可见性，便于后续审计人员核查上下文。
    #    真正危险的模式是 Rust current_schema() 字面量比较（pattern 2，error 级）。
    (
        r"table_schema\s*=\s*['\"]public['\"]",
        "hardcoded table_schema='public' in catalog query — verify this is intentional "
        "(guards must cover the active schema, not just public)",
        "warning",
    ),
    # 4. PGOPTIONS 显式把 public 挂进 search_path
    (
        r"search_path=[^'\"]*public",
        "search_path includes public schema — verify test isolation intent",
        "warning",
    ),
]


def iter_scan_files() -> list[Path]:
    files: list[Path] = []
    for pattern in SCAN_GLOBS:
        files.extend(REPO_ROOT.glob(pattern))
    # 去重 + 排序保证输出稳定；只留文件
    return sorted({f for f in files if f.is_file()})


def rel(p: Path) -> str:
    return str(p.relative_to(REPO_ROOT))


def check_file(filepath: Path) -> list[dict]:
    issues: list[dict] = []
    relname = rel(filepath)
    is_exempt = relname in SAFE_FILES
    try:
        text = filepath.read_text(encoding="utf-8", errors="replace")
    except OSError as e:
        print(f"WARN: cannot read {filepath}: {e}", file=sys.stderr)
        return issues
    for lineno, line in enumerate(text.splitlines(), 1):
        for pattern, msg, severity in PATTERNS:
            if re.search(pattern, line, re.IGNORECASE):
                # 豁免文件：降级为 info 且不计入汇总（保留可见性）
                if is_exempt:
                    severity = "info"
                issues.append(
                    {
                        "file": relname,
                        "line": lineno,
                        "msg": msg,
                        "severity": severity,
                        "content": line.strip()[:120],
                    }
                )
    return issues


def main() -> int:
    all_issues: list[dict] = []
    for f in iter_scan_files():
        all_issues.extend(check_file(f))

    errors = [i for i in all_issues if i["severity"] == "error"]
    warns = [i for i in all_issues if i["severity"] == "warning"]
    infos = [i for i in all_issues if i["severity"] == "info"]

    if all_issues:
        print("Schema-blind guard check results:")
        print("=" * 80)
        for issue in all_issues:
            print(f"[{issue['severity'].upper():7s}] {issue['file']}:{issue['line']}")
            print(f"         {issue['msg']}")
            print(f"         {issue['content']}")
        print("=" * 80)

    print(
        f"Summary: {len(errors)} errors, {len(warns)} warnings, {len(infos)} exempted(info)"
    )
    if errors:
        print("\nFAILED: schema-blind guard violations (see above)")
        return 1
    if warns:
        print("\nPASSED with warnings — review the flagged lines above.")
        return 0
    print("PASSED: no schema-blind patterns detected")
    return 0


if __name__ == "__main__":
    sys.exit(main())
