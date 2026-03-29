#!/usr/bin/env python3
"""
Migration Script Compression Tool
Merges consecutive ALTER statements on the same table into single statements.
Reduces table rebuild次数 and improves migration performance.
"""

import re
import os
import sys
from pathlib import Path
from typing import List, Dict, Tuple, Optional
from dataclasses import dataclass
from datetime import datetime


@dataclass
class MigrationScript:
    filename: str
    version: str
    jira: str
    description: str
    content: str
    path: Path


@dataclass
class AlterStatement:
    table: str
    operation: str  # ADD, DROP, ALTER, etc.
    column: Optional[str] = None
    dtype: Optional[str] = None
    constraint: Optional[str] = None


def parse_migration_filename(filename: str) -> Tuple[str, str, str, str]:
    """Parse migration filename into version, jira, description."""
    pattern = r'^V(\d+)_\d+__([A-Z]+-\d+)__(.+)\.sql$'
    match = re.match(pattern, filename)
    if match:
        version = match.group(1)
        jira = match.group(2)
        description = match.group(3)
        return version, jira, description, filename

    pattern_old = r'^(\d{14})_(.+)\.sql$'
    match = re.match(pattern_old, filename)
    if match:
        version = match.group(1)
        description = match.group(2)
        return version, "LEGACY", description, filename

    return "", "", "", filename


def extract_table_name(sql: str) -> Optional[str]:
    """Extract table name from SQL statement."""
    patterns = [
        r'ALTER\s+TABLE\s+([^\s(]+)',
        r'CREATE\s+TABLE\s+(?:IF\s+NOT\s+EXISTS\s+)?([^\s(]+)',
        r'DROP\s+TABLE\s+(?:IF\s+EXISTS\s+)?([^\s(]+)',
        r'INSERT\s+INTO\s+([^\s(]+)',
        r'UPDATE\s+([^\s]+)\s+SET',
    ]
    for pattern in patterns:
        match = re.search(pattern, sql, re.IGNORECASE)
        if match:
            return match.group(1).strip()
    return None


def group_alter_statements(content: str) -> Dict[str, List[str]]:
    """Group consecutive ALTER statements by table."""
    lines = content.split('\n')
    groups: Dict[str, List[str]] = {}
    current_table = None
    current_group: List[str] = []
    pending_alter: List[str] = []

    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith('--'):
            if pending_alter:
                current_group.extend(pending_alter)
                pending_alter = []
            current_group.append(line)
            continue

        table = extract_table_name(stripped)
        if table:
            if pending_alter:
                if current_table not in groups:
                    groups[current_table] = []
                groups[current_table].extend(pending_alter)
                pending_alter = []
            current_table = table

            if re.match(r'ALTER\s+TABLE', stripped, re.IGNORECASE):
                pending_alter.append(stripped)
            else:
                if current_table not in groups:
                    groups[current_table] = []
                if pending_alter:
                    groups[current_table].extend(pending_alter)
                    pending_alter = []
                current_group.append(stripped)
        else:
            if pending_alter and re.match(r'\s+(DROP|ADD|ALTER|MODIFY)', stripped, re.IGNORECASE):
                pending_alter.append(stripped)
            else:
                if pending_alter:
                    current_group.extend(pending_alter)
                    pending_alter = []
                current_group.append(stripped)

    if pending_alter:
        if current_table not in groups:
            groups[current_table] = []
        groups[current_table].extend(pending_alter)

    return groups


def merge_alter_statements(alters: List[str], table: str) -> str:
    """Merge multiple ALTER statements on the same table."""
    if len(alters) <= 1:
        return '\n'.join(alters)

    add_columns = []
    drop_columns = []
    alter_columns = []
    other = []

    for alter in alters:
        upper = alter.upper()
        if 'ADD COLUMN' in upper or 'ADD' in upper and 'COLUMN' in upper:
            col_match = re.search(r'ADD\s+(?:COLUMN\s+)?([^\s]+)', alter, re.IGNORECASE)
            if col_match:
                add_columns.append(f"    {alter.strip()}")
        elif 'DROP COLUMN' in upper:
            col_match = re.search(r'DROP\s+(?:COLUMN\s+)?([^\s;]+)', alter, re.IGNORECASE)
            if col_match:
                drop_columns.append(f"    {alter.strip()}")
        elif 'ALTER' in upper:
            alter_columns.append(f"    {alter.strip()}")
        else:
            other.append(f"    {alter.strip()}")

    merged = []
    if add_columns:
        merged.append(f"ALTER TABLE {table}\n" + ",\n".join(add_columns) + ";")
    if drop_columns:
        merged.append(f"ALTER TABLE {table}\n" + ",\n".join(drop_columns) + ";")
    if alter_columns:
        merged.append(f"ALTER TABLE {table}\n" + ",\n".join(alter_columns) + ";")
    for o in other:
        merged.append(o)

    return '\n'.join(merged)


def compress_migration(content: str) -> str:
    """Compress a single migration script."""
    lines = content.split('\n')
    result = []
    i = 0

    while i < len(lines):
        line = lines[i]
        stripped = line.strip()

        if stripped.startswith('--') or not stripped:
            result.append(line)
            i += 1
            continue

        table = extract_table_name(stripped)
        if table and re.match(r'ALTER\s+TABLE', stripped, re.IGNORECASE):
            alter_group = [stripped]
            j = i + 1
            while j < len(lines):
                next_line = lines[j].strip()
                if not next_line or next_line.startswith('--'):
                    break
                next_table = extract_table_name(next_line)
                if next_table and next_table == table and re.match(r'\s+(DROP|ADD|ALTER|MODIFY)', next_line, re.IGNORECASE):
                    alter_group.append(next_line)
                    j += 1
                else:
                    break

            if len(alter_group) > 1:
                merged = merge_alter_statements(alter_group, table)
                result.append(merged)
                i = j
            else:
                result.append(line)
                i += 1
        else:
            result.append(line)
            i += 1

    return '\n'.join(result)


def generate_header(version: str, jira: str, description: str, checksum: str) -> str:
    """Generate migration script header."""
    date = datetime.now().strftime('%Y-%m-%d')
    return f"""-- +----------------------------------------------------------------------------+
-- | Migration: V{version}__{jira}__{description}
-- | Jira: {jira}
-- | Author: synapse-rust team
-- | Date: {date}
-- | Description: {description}
-- | Checksum: {checksum}
-- +----------------------------------------------------------------------------+

"""


def calculate_checksum(content: str) -> str:
    """Calculate SHA-256 checksum of content."""
    import hashlib
    return hashlib.sha256(content.encode()).hexdigest()[:16]


def scan_migrations(migrations_dir: Path) -> List[MigrationScript]:
    """Scan migrations directory and return list of migration scripts."""
    scripts = []

    for sql_file in migrations_dir.rglob('*.sql'):
        if 'undo' in sql_file.stem.lower() or 'rollback' in str(sql_file):
            continue

        try:
            content = sql_file.read_text()
            version, jira, description, filename = parse_migration_filename(sql_file.name)
            if version:
                scripts.append(MigrationScript(
                    filename=filename,
                    version=version,
                    jira=jira,
                    description=description,
                    content=content,
                    path=sql_file
                ))
        except Exception as e:
            print(f"Warning: Failed to parse {sql_file}: {e}", file=sys.stderr)

    scripts.sort(key=lambda s: s.version)
    return scripts


def find_mergeable_scripts(scripts: List[MigrationScript]) -> Dict[str, List[MigrationScript]]:
    """Find scripts that can be merged based on consecutive versions."""
    groups = {}

    for script in scripts:
        key = f"{script.jira}_{script.description[:20]}"
        if key not in groups:
            groups[key] = []
        groups[key].append(script)

    return {k: v for k, v in groups.items() if len(v) > 1}


def main():
    """Main entry point."""
    import argparse

    parser = argparse.ArgumentParser(description='Compress migration scripts')
    parser.add_argument('migrations_dir', type=Path, help='Path to migrations directory')
    parser.add_argument('--dry-run', action='store_true', help='Show what would be done')
    parser.add_argument('--output-dir', type=Path, help='Output directory for compressed scripts')

    args = parser.parse_args()

    if not args.migrations_dir.exists():
        print(f"Error: Directory {args.migrations_dir} does not exist", file=sys.stderr)
        sys.exit(1)

    print(f"Scanning {args.migrations_dir}...")
    scripts = scan_migrations(args.migrations_dir)
    print(f"Found {len(scripts)} migration scripts")

    mergeable = find_mergeable_scripts(scripts)
    if mergeable:
        print(f"\nFound {len(mergeable)} groups of potentially mergeable scripts:")
        for key, group in mergeable.items():
            print(f"  {key}: {len(group)} scripts")
    else:
        print("\nNo mergeable scripts found")

    output_dir = args.output_dir or args.migrations_dir
    total_savings = 0

    for script in scripts:
        original_lines = len(script.content.split('\n'))
        compressed = compress_migration(script.content)
        compressed_lines = len(compressed.split('\n'))
        savings = original_lines - compressed_lines

        if savings > 0:
            total_savings += savings
            print(f"\n{script.filename}:")
            print(f"  Original: {original_lines} lines")
            print(f"  Compressed: {compressed_lines} lines")
            print(f"  Savings: {savings} lines")

            if not args.dry_run:
                checksum = calculate_checksum(compressed)
                header = generate_header(script.version, script.jira, script.description, checksum)
                output = header + compressed
                output_file = output_dir / script.filename
                output_file.write_text(output)
                print(f"  Written to: {output_file}")

    print(f"\nTotal line savings: {total_savings}")

    if args.dry_run:
        print("\nDry run - no files written")


if __name__ == '__main__':
    main()
