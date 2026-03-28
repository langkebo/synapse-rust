#!/usr/bin/env python3
import pathlib
import re
import sys

ROOT = pathlib.Path(__file__).resolve().parents[1]
SOURCE_DIRS = [
    ROOT / "src",
    ROOT / "tests",
]
MIGRATIONS_DIR = ROOT / "migrations"
EXCEPTIONS_FILE = ROOT / "scripts" / "schema_table_coverage_exceptions.txt"

REF_PATTERNS = [
    re.compile(r"\bFROM\s+([a-z_][a-z0-9_]*)\b(?!\s*\()", re.IGNORECASE),
    re.compile(r"\bJOIN\s+([a-z_][a-z0-9_]*)\b(?!\s*\()", re.IGNORECASE),
    re.compile(r"\bINSERT\s+INTO\s+([a-z_][a-z0-9_]*)", re.IGNORECASE),
    re.compile(r"(?:^|[;(])\s*UPDATE\s+([a-z_][a-z0-9_]*)", re.IGNORECASE | re.MULTILINE),
    re.compile(r"\bDELETE\s+FROM\s+([a-z_][a-z0-9_]*)", re.IGNORECASE),
]
SQL_LITERAL_PATTERNS = [
    re.compile(
        r"sqlx::query(?:_as|_scalar)?!?(?:::<[^>]+>)?\(\s*r#\"(.*?)\"#\s*[,)]",
        re.DOTALL,
    ),
    re.compile(
        r"sqlx::query(?:_as|_scalar)?!?(?:::<[^>]+>)?\(\s*r\"(.*?)\"\s*[,)]",
        re.DOTALL,
    ),
    re.compile(
        r'sqlx::query(?:_as|_scalar)?!?(?:::<[^>]+>)?\(\s*"((?:\\.|[^"\\])*)"\s*[,)]',
        re.DOTALL,
    ),
]
SQL_TRIGGER_PATTERN = re.compile(r"\b(SELECT|INSERT|UPDATE|DELETE)\b", re.IGNORECASE)
CTE_PATTERN = re.compile(r"(?:WITH|,)\s*([a-z_][a-z0-9_]*)\s+AS\s*\(", re.IGNORECASE)
DEF_PATTERNS = [
    re.compile(
        r"\bCREATE\s+TABLE(?:\s+IF\s+NOT\s+EXISTS)?\s+([a-z_][a-z0-9_]*)",
        re.IGNORECASE,
    ),
    re.compile(
        r"\bCREATE\s+(?:OR\s+REPLACE\s+)?VIEW\s+([a-z_][a-z0-9_]*)",
        re.IGNORECASE,
    ),
    re.compile(
        r"\bCREATE\s+(?:OR\s+REPLACE\s+)?MATERIALIZED\s+VIEW\s+([a-z_][a-z0-9_]*)",
        re.IGNORECASE,
    ),
]
IGNORED_REFS = {
    "information_schema",
    "lateral",
    "pg_indexes",
    "pg_stat_activity",
    "pg_stat_database",
    "pg_stat_user_tables",
    "set",
    "values",
}


def read_exceptions() -> set[str]:
    if not EXCEPTIONS_FILE.exists():
        return set()
    return {
        line.strip()
        for line in EXCEPTIONS_FILE.read_text().splitlines()
        if line.strip() and not line.strip().startswith("#")
    }


def collect_references() -> dict[str, set[str]]:
    refs: dict[str, set[str]] = {}
    for source_dir in SOURCE_DIRS:
        if not source_dir.exists():
            continue
        for path in source_dir.rglob("*.rs"):
            text = path.read_text()
            sql_literals: list[str] = []
            for string_pattern in SQL_LITERAL_PATTERNS:
                sql_literals.extend(
                    literal
                    for literal in string_pattern.findall(text)
                    if SQL_TRIGGER_PATTERN.search(literal)
                )

            for literal in sql_literals:
                cte_names = {match.group(1).lower() for match in CTE_PATTERN.finditer(literal)}
                for pattern in REF_PATTERNS:
                    for match in pattern.finditer(literal):
                        table = match.group(1).lower()
                        if table in IGNORED_REFS or table in cte_names or table.startswith("pg_"):
                            continue
                        refs.setdefault(table, set()).add(str(path.relative_to(ROOT)))
    return refs


def collect_definitions() -> set[str]:
    defs: set[str] = set()
    for path in MIGRATIONS_DIR.rglob("*.sql"):
        text = path.read_text()
        for pattern in DEF_PATTERNS:
            defs.update(match.group(1).lower() for match in pattern.finditer(text))
    return defs


def main() -> int:
    refs = collect_references()
    defs = collect_definitions()
    exceptions = read_exceptions()
    missing = sorted(table for table in refs if table not in defs and table not in exceptions)

    if missing:
        print("Missing schema coverage for referenced tables:")
        for table in missing:
            locations = ", ".join(sorted(refs[table]))
            print(f"- {table}: {locations}")
        return 1

    ignored = sorted(table for table in refs if table in exceptions and table not in defs)
    if ignored:
        print("Known schema coverage exceptions:")
        for table in ignored:
            locations = ", ".join(sorted(refs[table]))
            print(f"- {table}: {locations}")

    print(f"Schema table coverage passed: {len(refs)} referenced tables checked, {len(defs)} schema tables found.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
