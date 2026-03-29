#!/usr/bin/env python3
"""
Migration Script Lifecycle Management Tool
Manages lifecycle tags for migration scripts: deprecated, unused, test-only
Provides tools for identifying and cleaning up redundant scripts.
"""

import os
import re
import sys
import json
import argparse
from pathlib import Path
from typing import List, Dict, Optional, Set
from dataclasses import dataclass, asdict
from datetime import datetime, timedelta
from enum import Enum


class LifecycleTag(Enum):
    ACTIVE = "active"
    DEPRECATED = "deprecated"
    UNUSED = "unused"
    TEST_ONLY = "test-only"
    ARCHIVED = "archived"


@dataclass
class MigrationScript:
    filename: str
    version: str
    jira: str
    description: str
    path: Path
    tag: LifecycleTag
    last_modified: datetime
    last_executed: Optional[datetime]
    execution_count: int
    reason: Optional[str] = None


@dataclass
class LifecyclePolicy:
    unused_threshold_days: int = 180
    deprecated_retention_days: int = 14
    requires_dual_review: bool = True
    requires_changelog: bool = True
    requires_git_archive: bool = True


LIFECYCLE_HEADER_TEMPLATE = """-- +----------------------------------------------------------------------------+
-- | Migration Lifecycle Tag: {tag}
-- | Reason: {reason}
-- | Tagged at: {tagged_at}
-- | Tagged by: {tagged_by}
-- | Scheduled removal: {scheduled_removal}
-- +----------------------------------------------------------------------------+

"""


def parse_lifecycle_tag(content: str) -> Optional[Dict]:
    """Parse lifecycle tag from migration script header."""
    patterns = [
        r'Migration Lifecycle Tag:\s*(\w+)',
        r'Reason:\s*(.+)',
        r'Tagged at:\s*(.+)',
        r'Tagged by:\s*(.+)',
        r'Scheduled removal:\s*(.+)',
    ]

    tag_info = {}
    for pattern in patterns:
        match = re.search(pattern, content, re.MULTILINE)
        if match:
            key = pattern.split(':')[0].strip().lower().replace(' ', '_')
            tag_info[key] = match.group(1).strip()

    return tag_info if 'migration_lifecycle_tag' in tag_info else None


def add_lifecycle_tag(
    script_path: Path,
    tag: LifecycleTag,
    reason: str,
    tagged_by: str,
    policy: LifecyclePolicy
) -> bool:
    """Add lifecycle tag to a migration script."""
    try:
        content = script_path.read_text()

        if parse_lifecycle_tag(content):
            print(f"Script {script_path.name} already has a lifecycle tag")
            return False

        tagged_at = datetime.now()
        if tag == LifecycleTag.DEPRECATED:
            scheduled_removal = tagged_at + timedelta(days=policy.deprecated_retention_days)
        elif tag == LifecycleTag.UNUSED:
            scheduled_removal = tagged_at + timedelta(days=policy.unused_threshold_days)
        else:
            scheduled_removal = tagged_at + timedelta(days=365)

        header = LIFECYCLE_HEADER_TEMPLATE.format(
            tag=tag.value,
            reason=reason,
            tagged_at=tagged_at.isoformat(),
            tagged_by=tagged_by,
            scheduled_removal=scheduled_removal.isoformat()
        )

        new_content = header + content
        script_path.write_text(new_content)

        print(f"Added {tag.value} tag to {script_path.name}")
        return True

    except Exception as e:
        print(f"Error tagging {script_path.name}: {e}", file=sys.stderr)
        return False


def remove_lifecycle_tag(script_path: Path) -> bool:
    """Remove lifecycle tag from a migration script."""
    try:
        content = script_path.read_text()

        header_match = re.search(
            r'-- \+\-{7}\+[\s\S]+?-- \+\-{7}\+\n\n',
            content
        )

        if header_match:
            new_content = content.replace(header_match.group(0), '')
            script_path.write_text(new_content)
            print(f"Removed lifecycle tag from {script_path.name}")
            return True
        else:
            print(f"No lifecycle tag found in {script_path.name}")
            return False

    except Exception as e:
        print(f"Error removing tag from {script_path.name}: {e}", file=sys.stderr)
        return False


def scan_migrations(
    migrations_dir: Path,
    schema_migrations_query: Optional[str] = None
) -> List[MigrationScript]:
    """Scan migrations directory and determine lifecycle status."""
    scripts = []

    for sql_file in migrations_dir.rglob('*.sql'):
        if 'undo' in sql_file.stem.lower() or 'rollback' in str(sql_file):
            continue

        try:
            content = sql_file.read_text()
            tag_info = parse_lifecycle_tag(content)

            version, jira, description, _ = parse_filename(sql_file.name)

            stat = sql_file.stat()
            last_modified = datetime.fromtimestamp(stat.st_mtime)

            tag = LifecycleTag.ACTIVE
            if tag_info:
                try:
                    tag = LifecycleTag(tag_info['migration_lifecycle_tag'])
                except ValueError:
                    tag = LifecycleTag.ACTIVE

            scripts.append(MigrationScript(
                filename=sql_file.name,
                version=version,
                jira=jira,
                description=description,
                path=sql_file,
                tag=tag,
                last_modified=last_modified,
                last_executed=None,
                execution_count=0,
                reason=tag_info.get('reason') if tag_info else None
            ))

        except Exception as e:
            print(f"Warning: Failed to parse {sql_file}: {e}", file=sys.stderr)

    return scripts


def parse_filename(filename: str) -> tuple:
    """Parse migration filename into components."""
    pattern = r'^V(\d+)_\d+__([A-Z]+-\d+)__(.+)\.sql$'
    match = re.match(pattern, filename)
    if match:
        return match.group(1), match.group(2), match.group(3), filename

    pattern_old = r'^(\d{14})_(.+)\.sql$'
    match = re.match(pattern_old, filename)
    if match:
        return match.group(1), "LEGACY", match.group(2), filename

    return "", "", "", filename


def identify_candidates_for_deprecation(
    scripts: List[MigrationScript],
    threshold_days: int = 180
) -> List[MigrationScript]:
    """Identify scripts that should be deprecated."""
    candidates = []
    cutoff = datetime.now() - timedelta(days=threshold_days)

    for script in scripts:
        if script.tag != LifecycleTag.ACTIVE:
            continue

        if script.last_modified < cutoff:
            candidates.append(script)

    return candidates


def identify_candidates_for_deletion(
    scripts: List[MigrationScript],
    policy: LifecyclePolicy
) -> List[MigrationScript]:
    """Identify scripts ready for deletion."""
    candidates = []
    now = datetime.now()

    for script in scripts:
        if script.tag == LifecycleTag.ACTIVE:
            continue

        if script.tag == LifecycleTag.DEPRECATED:
            scheduled_date = None
            if script.reason:
                match = re.search(r'Scheduled removal:\s*(.+)', script.reason)
                if match:
                    try:
                        scheduled_date = datetime.fromisoformat(match.group(1).strip())
                    except ValueError:
                        pass

            if scheduled_date and now >= scheduled_date:
                candidates.append(script)

        elif script.tag == LifecycleTag.UNUSED:
            candidates.append(script)

    return candidates


def generate_changelog_entry(
    script: MigrationScript,
    action: str,
    approved_by: List[str]
) -> str:
    """Generate changelog entry for script deletion."""
    now = datetime.now().isoformat()
    return f"""
## {now} - {action}: {script.filename}

- **Version**: {script.version}
- **Jira**: {script.jira}
- **Description**: {script.description}
- **Action**: {action}
- **Approved by**: {', '.join(approved_by)}
- **Git Archive**: archive/{script.version}__{script.jira}
"""


def create_git_archive(script: MigrationScript, repo_path: Path) -> bool:
    """Create git archive for a migration script."""
    try:
        import subprocess

        archive_name = f"{script.version}__{script.jira}"
        tag_name = f"archive/{archive_name}"

        result = subprocess.run(
            ['git', 'tag', '-a', tag_name, '-m', f"Migration archived: {script.filename}"],
            cwd=repo_path,
            capture_output=True,
            text=True
        )

        if result.returncode != 0:
            print(f"Failed to create git tag: {result.stderr}", file=sys.stderr)
            return False

        archive_file = repo_path / f"{archive_name}.tar.gz"
        subprocess.run(
            ['git', 'archive', '--format=tar', '--prefix', f"{archive_name}/", tag_name],
            stdout=open(archive_file, 'wb'),
            cwd=repo_path
        )

        import gzip
        with gzip.open(archive_file.with_suffix('.tar.gz'), 'rb') as f_in:
            with open(archive_file, 'wb') as f_out:
                f_out.write(f_in.read())

        print(f"Created git archive: {archive_file}")
        return True

    except Exception as e:
        print(f"Error creating archive for {script.filename}: {e}", file=sys.stderr)
        return False


def main():
    """Main entry point."""
    parser = argparse.ArgumentParser(description='Manage migration script lifecycle')
    parser.add_argument('migrations_dir', type=Path, help='Path to migrations directory')
    parser.add_argument('--repo-path', type=Path, help='Git repository path')
    parser.add_argument('--tag', choices=[t.value for t in LifecycleTag], help='Tag to apply')
    parser.add_argument('--filename', help='Specific file to tag')
    parser.add_argument('--reason', help='Reason for tagging')
    parser.add_argument('--tagged-by', default='system', help='User tagging the script')
    parser.add_argument('--remove-tag', action='store_true', help='Remove tag from script')
    parser.add_argument('--list', action='store_true', help='List all migration scripts')
    parser.add_argument('--candidates', action='store_true', help='List candidates for deprecation')
    parser.add_argument('--deletable', action='store_true', help='List scripts ready for deletion')
    parser.add_argument('--dry-run', action='store_true', help='Show what would be done')

    args = parser.parse_args()

    if not args.migrations_dir.exists():
        print(f"Error: Directory {args.migrations_dir} does not exist", file=sys.stderr)
        sys.exit(1)

    policy = LifecyclePolicy()

    if args.list:
        scripts = scan_migrations(args.migrations_dir)
        print(f"Found {len(scripts)} migration scripts:")
        for script in scripts:
            status = script.tag.value
            if script.tag != LifecycleTag.ACTIVE:
                status = f"{status} ({script.reason})"
            print(f"  [{status}] {script.filename}")

    elif args.candidates:
        scripts = scan_migrations(args.migrations_dir)
        candidates = identify_candidates_for_deprecation(scripts, policy.unused_threshold_days)
        print(f"Found {len(candidates)} candidates for deprecation:")
        for script in candidates:
            print(f"  {script.filename} (last modified: {script.last_modified})")

    elif args.deletable:
        scripts = scan_migrations(args.migrations_dir)
        deletable = identify_candidates_for_deletion(scripts, policy)
        print(f"Found {len(deletable)} scripts ready for deletion:")
        for script in deletable:
            print(f"  [{script.tag.value}] {script.filename}")

    elif args.tag and args.filename:
        script_path = args.migrations_dir / args.filename
        if not script_path.exists():
            print(f"Error: File {script_path} not found", file=sys.stderr)
            sys.exit(1)

        if args.remove_tag:
            if not args.dry_run:
                remove_lifecycle_tag(script_path)
            else:
                print(f"Would remove tag from {args.filename}")

        else:
            tag = LifecycleTag(args.tag)
            if not args.dry_run:
                add_lifecycle_tag(
                    script_path,
                    tag,
                    args.reason or "No reason provided",
                    args.tagged_by,
                    policy
                )
            else:
                print(f"Would add {tag.value} tag to {args.filename}")

    else:
        parser.print_help()


if __name__ == '__main__':
    main()
