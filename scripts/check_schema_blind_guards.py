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

  【报 warning（需预算登记，否则升级为 error）】
  3. SQL/脚本里 `table_schema = 'public'` 字面量（可能是合法默认，人工复核）。
  4. PGOPTIONS search_path 含 public。

  【豁免机制】（旧脚本的致命伤：定义了 SAFE_* 却从不使用）
  - **逐规则**豁免：SAFE_FILES 每个条目写明 `reason` + `rules`，只降级它真实命中的那几条。
    （旧版是整文件豁免，导致"计数查询是有意的"顺便换来对 `DROP SCHEMA public CASCADE` 免疫，
     而后者恰是本守卫唯一想拦的破坏性 DDL —— 见 2026-10-27 CI 车道审计 §7。）
  - 每个条目必须附带理由，防止豁免清单腐烂；条目自身不合法时 startup 即失败。

退出码：任何 error → 1；warning 未登记/已过期/超预算 → 1；
只有"已登记且未过期且在预算内"的 warning → 0（打印汇总供人工复核）。
"""

from __future__ import annotations

import datetime
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

# ── 豁免清单（旧脚本定义了两份 SAFE_DROP_PATTERNS 且都从未被使用）──────────
# 每个条目必须带 `reason` 与 `rules`。**`rules` 是逐规则豁免，不是整文件豁免**：
#   "这个文件里的 public 计数查询是有意的" 并不意味着 "这个文件里的
#   `DROP SCHEMA public CASCADE` 也可以不报警"。旧的全文件豁免把后者一起吃了 ——
#   `.github/workflows/ci.yml` 因一条 SBG-03 被豁免，却连带对 SBG-01（真破坏性
#   DDL）免疫，而这恰好是本门禁唯一想拦的东西。新增条目请只列它**真实命中**的规则。
SAFE_FILES: dict[str, dict] = {
    # 这些脚本按设计就是要操作/重置 public schema（测试库初始化）
    "scripts/init_test_public_schema.sh": {
        "reason": "controlled public-schema reset by design",
        "rules": {"SBG-01", "SBG-04"},
    },
    "scripts/reset_database_v12.sh": {
        "reason": "DB initialization script, operates on public by design",
        "rules": {"SBG-01", "SBG-03"},
    },
    "scripts/cleanup_test_schemas.sh": {
        # 实际 DROP 的是变量 schema（`DROP SCHEMA "$s" CASCADE`），`public` 只出现在注释里
        "reason": "test-schema cleanup, public references are comments only",
        "rules": {"SBG-01"},
    },
    "scripts/ci/prepare_test_db.sh": {
        "reason": "CI seed script, seeds public + template schemas by design",
        "rules": {"SBG-03"},
    },
    # 测试库前置校验：显式只问 dev/test 容器里 `synapse_test.public` 的表数。
    # 把 public 换成 current_schema()/search_path 反而会让校验在错误的目标上通过
    # （它要证明的就是"迁移打进了那个固定库"），故不是 schema-blind 缺陷。
    "scripts/dev-test-setup.sh": {
        "reason": "dev-stack setup asserts synapse_test.public actually received migrations",
        "rules": {"SBG-03"},
    },
    "scripts/run_full_integration.sh": {
        "reason": "integration runner pre-checks the migrated public baseline (>=100 tables)",
        "rules": {"SBG-03"},
    },
    # 测试隔离框架：明确在受控的 test database 上重置 public schema
    "synapse-test-utils/src/lib.rs": {
        "reason": "test isolation framework, guarded DROP public with deploy detection",
        "rules": {"SBG-01", "SBG-03"},
    },
    # test_isolation.rs 里的 DROP 字面量是 SQL 注入防御的测试 payload（不是真实 DROP）
    "synapse-common/src/test_isolation.rs": {
        "reason": "test payloads for SQL injection defense — DROP strings are test inputs, not real operations",
        "rules": {"SBG-01"},
    },
    # CI 工作流：明确针对 synapse 生产库的迁移验证，非 schema-blind guard 缺陷
    ".github/workflows/ci.yml": {
        "reason": "CI migration validation against synapse production DB, public refs are intentional",
        "rules": {"SBG-03"},
    },
    ".github/workflows/db-tests-manual.yml": {
        "reason": "Manual db test script targeting synapse DB, public schema explicit by design",
        "rules": {"SBG-03"},
    },
    ".github/workflows/drift-detection.yml": {
        "reason": "Drift detection baseline apply to synapse DB, public schema is target",
        "rules": {"SBG-03"},
    },
    # database_initializer：生产代码迁移后完整性验证，非守卫缺陷
    "synapse-services/src/database_initializer/mod.rs": {
        "reason": "Production migration validation & normalization utility, not a schema-blind guard",
        "rules": {"SBG-03"},
    },
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
# (id, regex, message, severity)
# severity: 'error' → CI 失败；'warning' → 需在 WARNING_BUDGET 登记后才放行
PATTERNS: list[tuple[str, str, str, str]] = [
    # 1. 危险的 public DROP —— 非豁免文件中出现即 error
    (
        "SBG-01",
        r"DROP\s+SCHEMA\s+(?:IF\s+EXISTS\s+)?public\s+CASCADE",
        "DROP SCHEMA public CASCADE outside allowlisted reset scripts",
        "error",
    ),
    # 2. Rust 里的 schema 字面量比较（B3-3 类缺陷的直接特征）
    #    例如: current_schema() == Some("public") / schema != "public"
    (
        "SBG-02",
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
        "SBG-03",
        r"table_schema\s*=\s*['\"]public['\"]",
        "hardcoded table_schema='public' in catalog query — verify this is intentional "
        "(guards must cover the active schema, not just public)",
        "warning",
    ),
    # 4. PGOPTIONS 显式把 public 挂进 search_path
    (
        "SBG-04",
        r"search_path=[^'\"]*public",
        "search_path includes public schema — verify test isolation intent",
        "warning",
    ),
]

# ── 告警预算：warning 不再"打印即通过" ─────────────────────────────────────────
# 旧行为是 `warning` 永不挡 CI（`PASSED with warnings` + exit 0），观察到的唯一一条
# 因此可以长期无人处理（2026-10-13 审计 §2 B3 记录该形态）。现在改成：
#   ① 每条允许存在的 warning 必须在此**登记**（编号 → 理由 + 到期日 + 允许条数）；
#   ② 未登记的 warning、已过期的登记、超预算的条数 ⇒ **升级为 error**（exit 1）。
# 到期日的含义是"到这天必须二选一：修掉，或把理由/日期重新评估后再登记"。
WARNING_BUDGET: dict[str, dict] = {
    "SBG-03": {
        "reason": (
            "`scripts/ci/converge_public_schema.sh:245` 的 `table_schema='public'` 是**收敛脚本自身**"
            "在清点 public schema 剩余表数（该脚本的职责就是操作 public），语义上必须写死 public；"
            "已确认非 schema-blind 缺陷。保留告警是为了让它继续可见。"
            "（对照：`synapse-test-utils` / `synapse-common/test_isolation.rs` 等同类命中已在 SAFE_FILES 降级）"
        ),
        "expires": "2027-01-31",
        "allowed": 1,
    },
}


def validate_exemptions() -> list[str]:
    """SAFE_FILES 自检：条目不完整或规则编号写错，都会让豁免**静默失效**。

    豁免一旦对不上号，命中的那条会被当普通告警/错误报出来（假红，可见）；
    但"写了个不存在的规则编号"同样会让人以为"已经处理过"，所以这里直接拦。
    """
    known = {rule_id for rule_id, *_ in PATTERNS}
    problems: list[str] = []
    for path, entry in SAFE_FILES.items():
        if not isinstance(entry, dict):
            problems.append(f"{path}: SAFE_FILES 条目必须是 dict（reason + rules）")
            continue
        reason = entry.get("reason")
        if not isinstance(reason, str) or not reason.strip():
            problems.append(f"{path}: 缺 reason —— 豁免必须写明为什么这不是缺陷")
        rules = entry.get("rules")
        if not isinstance(rules, (set, frozenset)) or not rules:
            problems.append(
                f"{path}: 缺 rules（逐规则豁免，必须显式列出真实命中的规则）"
            )
            continue
        unknown = sorted(rules - known)
        if unknown:
            problems.append(
                f"{path}: rules 含未知规则编号 {unknown}（已知：{sorted(known)}）"
            )
    return problems


def iter_scan_files() -> list[Path]:
    files: list[Path] = []
    for pattern in SCAN_GLOBS:
        files.extend(REPO_ROOT.glob(pattern))
    # 去重 + 排序保证输出稳定；只留文件
    return sorted({f for f in files if f.is_file()})


def rel(p: Path) -> str:
    return str(p.relative_to(REPO_ROOT))


def is_comment_only(line: str, suffix: str) -> bool:
    """True when the line carries no executable content — only a comment.

    **Why this exists（2026-09-30 C78）**：本守卫原先逐行正则匹配、**连注释也算违规**。
    `scripts/ci/converge_public_schema.sh:13` 那句注释
    "`DROP SCHEMA public CASCADE` is **not** an option: …" 是在解释**为什么不做**这个危险操作，
    却被判成 `[ERROR] DROP SCHEMA public CASCADE outside allowlisted reset scripts`
    ⇒ `DB Migration Gate` 在 main 上常红（假阳性）。把整个文件加进 SAFE_FILES 是错的
    （会一并豁免该文件里**真实**的 DROP），所以按最小且安全的判据修：**只跳过纯注释行**。

    纯注释行 = 去掉前导空白后以该语言的注释标记开头（`.sh`/`.yml` 为 `#`，`.sql` 另有 `--`，
    `.rs` 为 `//`、`/*`、`*`）。行尾注释（代码后跟注释）**不跳过** —— 那种行确实有可执行内容。
    """
    stripped = line.lstrip()
    if suffix in (".sh", ".yml", ".yaml", ".sql"):
        markers = ("#",) if suffix != ".sql" else ("#", "--")
    else:  # .rs
        markers = ("//", "/*", "*")
    return stripped.startswith(markers)


def check_file(filepath: Path) -> list[dict]:
    issues: list[dict] = []
    relname = rel(filepath)
    exempt_rules = SAFE_FILES.get(relname, {}).get("rules", frozenset())
    try:
        text = filepath.read_text(encoding="utf-8", errors="replace")
    except OSError as e:
        print(f"WARN: cannot read {filepath}: {e}", file=sys.stderr)
        return issues
    for lineno, line in enumerate(text.splitlines(), 1):
        if is_comment_only(line, filepath.suffix):
            continue
        for rule_id, pattern, msg, severity in PATTERNS:
            if re.search(pattern, line, re.IGNORECASE):
                # 逐规则豁免：该文件对这一条规则已复核 ⇒ 降级 info 保留可见性。
                # 未列出的规则仍按原 severity 计 ⇒ 同一文件里的 `DROP SCHEMA ... CASCADE`
                # 不会因为"它的计数查询是有意的"而一起被放过。
                if rule_id in exempt_rules:
                    severity = "info"
                issues.append(
                    {
                        "id": rule_id,
                        "file": relname,
                        "line": lineno,
                        "msg": msg,
                        "severity": severity,
                        "content": line.strip()[:120],
                    }
                )
    return issues


def main() -> int:
    problems = validate_exemptions()
    if problems:
        print("SAFE_FILES 自检失败（豁免清单本身有问题，先修清单再谈扫描结果）：")
        for p in problems:
            print(f"  - {p}")
        return 1

    all_issues: list[dict] = []
    for f in iter_scan_files():
        all_issues.extend(check_file(f))

    errors = [i for i in all_issues if i["severity"] == "error"]
    warns = [i for i in all_issues if i["severity"] == "warning"]
    infos = [i for i in all_issues if i["severity"] == "info"]

    # 告警预算判定：未登记 / 已过期 / 超预算 的 warning 一律升级为 error。
    # 这样 "PASSED with warnings" 只可能出现在"预算内且未过期"的情形。
    today = datetime.date.today().isoformat()
    budget_violations: list[str] = []
    upgraded_ids: set[int] = set()
    counted: dict[str, int] = {}
    for issue in warns:
        rule_id = issue["id"]
        entry = WARNING_BUDGET.get(rule_id)
        where = f"{issue['file']}:{issue['line']}"
        if entry is None:
            budget_violations.append(
                f"{rule_id} 未登记：{where} —— 新增告警必须在 WARNING_BUDGET 登记"
                "（编号 + 理由 + 到期日 + 允许条数），或直接修掉"
            )
            upgraded_ids.add(id(issue))
            continue
        if entry["expires"] < today:
            budget_violations.append(
                f"{rule_id} 登记已到期（{entry['expires']}）：{where}"
            )
            upgraded_ids.add(id(issue))
            continue
        counted[rule_id] = counted.get(rule_id, 0) + 1
        allowed = int(entry.get("allowed", 1))
        if counted[rule_id] > allowed:
            budget_violations.append(
                f"{rule_id} 超预算（{counted[rule_id]} > {allowed}）：{where}"
            )
            upgraded_ids.add(id(issue))

    if all_issues:
        print("Schema-blind guard check results:")
        print("=" * 80)
        for issue in all_issues:
            tag = "error" if id(issue) in upgraded_ids else issue["severity"]
            print(f"[{tag.upper():7s}] {issue['id']}  {issue['file']}:{issue['line']}")
            print(f"         {issue['msg']}")
            print(f"         {issue['content']}")
        print("=" * 80)

    if budget_violations:
        print("WARNING BUDGET VIOLATIONS（下列告警已升级为 error）:")
        for v in budget_violations:
            print(f"  - {v}")
        print("=" * 80)

    print(
        f"Summary: {len(errors)} errors, {len(warns)} warnings "
        f"({len(upgraded_ids)} 条未登记/过期/超预算 ⇒ 计为 error), {len(infos)} exempted(info)"
    )
    if errors or budget_violations:
        print("\nFAILED: schema-blind guard violations (see above)")
        return 1
    if warns:
        print("\nPASSED with warnings — 全部已在 WARNING_BUDGET 登记且未过期：")
        for rule_id, n in sorted(counted.items()):
            print(
                f"  - {rule_id}: {n} 条（登记到期日 {WARNING_BUDGET[rule_id]['expires']}）"
            )
        return 0
    print("PASSED: no schema-blind patterns detected")
    return 0


if __name__ == "__main__":
    sys.exit(main())
