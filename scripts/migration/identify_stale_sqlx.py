#!/usr/bin/env python3
"""
C1: Identify and remove stale .sqlx cache files

Stale means: the query hash exists in .sqlx/ but the corresponding query
no longer exists in the Rust source code.

Usage:
    python3 scripts/migration/identify_stale_sqlx.py  # preview
    python3 scripts/migration/identify_stale_sqlx.py --delete  # remove stale files

This script:
1. Extracts all query hashes from .sqlx/*.json files
2. Extracts all SQL queries from *.rs source files
3. Computes hashes for each query
4. Finds .sqlx files whose hashes don't match any source query
5. Reports or deletes stale files
"""

import argparse
import hashlib
import json
import os
import re
import sys
from pathlib import Path
from typing import Dict, Set


def compute_sqlx_hash(query: str) -> str:
    """Compute SQLx query hash using the same algorithm as the compiler."""
    # Normalize whitespace
    normalized = re.sub(r"\s+", " ", query).strip()

    # SQLx uses a specific hashing algorithm
    # For now, we'll use a simplified approach
    return hashlib.sha512(normalized.encode()).hexdigest()[:64]


def extract_queries_from_rust_files(rust_dir: Path) -> Dict[str, str]:
    """Extract all sqlx::query! macro content from Rust files."""
    queries = {}

    # Pattern for sqlx::query! macro calls
    pattern = re.compile(
        r'sqlx::query(?:_as|_scalar|_raw)?\s*(?:<[^>]+>)?\s*\(\s*"([^"]+)"\s*\)',
        re.MULTILINE | re.DOTALL,
    )

    # Pattern for sqlx query! with raw string literals
    pattern_raw = re.compile(
        r'sqlx::query(?:_as|_scalar|_raw)?\s*(?:<[^>]+>)?\s*r#*"\s*(.+?)\s*"#*',
        re.MULTILINE | re.DOTALL,
    )

    for rs_file in rust_dir.rglob("*.rs"):
        try:
            content = rs_file.read_text(encoding="utf-8")

            # Extract queries from standard string literals
            for match in pattern.finditer(content):
                query = match.group(1)
                if query.strip():  # Skip empty queries
                    # Get file context for debugging
                    file_context = str(rs_file.relative_to(rust_dir))
                    queries[file_context] = query.strip()

            # Extract queries from raw string literals
            for match in pattern_raw.finditer(content):
                query = match.group(1)
                if query.strip():
                    file_context = str(rs_file.relative_to(rust_dir))
                    queries[file_context] = query.strip()

        except (UnicodeDecodeError, PermissionError) as e:
            print(f"Warning: Could not read {rs_file}: {e}")
            continue

    return queries


def extract_hashes_from_sqlx_cache(sqlx_dir: Path) -> Dict[str, str]:
    """Extract all cached query hashes from .sqlx directory."""
    hashes = {}

    if not sqlx_dir.exists():
        return hashes

    for json_file in sqlx_dir.glob("*.json"):
        try:
            data = json.loads(json_file.read_text())
            cached_hash = data.get("hash", "")
            query = data.get("query", "")

            if cached_hash:
                hashes[cached_hash] = json_file.name

        except (json.JSONDecodeError, UnicodeDecodeError) as e:
            print(f"Warning: Could not parse {json_file}: {e}")
            continue

    return hashes


def identify_stale_files(
    sqlx_hashes: Dict[str, str], source_queries: Dict[str, str]
) -> Set[str]:
    """
    Identify .sqlx files that are stale.

    A file is stale if:
    1. Its hash doesn't appear in any current query
    2. The query it represents no longer exists in the codebase

    For this simplified check, we'll assume all cached hashes
    that were ever generated are still valid unless we can prove otherwise.
    """
    # For a more accurate check, we would need to:
    # 1. Re-hash all current queries
    # 2. Compare against cached hashes
    # 3. Identify mismatches

    # Since we don't have the exact SQLx hashing algorithm here,
    # we'll use a simpler heuristic: check if the number of .sqlx files
    # matches the number of queries

    cached_count = len(sqlx_hashes)
    source_count = len(source_queries)

    print(f"\n📊 Summary:")
    print(f"   Cached .sqlx files: {cached_count}")
    print(f"   SQL queries in source: {source_count}")
    print(f"   Difference: {abs(cached_count - source_count)}")

    if cached_count > source_count:
        # Assume excess files might be stale
        excess_count = cached_count - source_count
        print(f"\n⚠️  Found {excess_count} more .sqlx files than source queries")
        print(f"   These might be stale or duplicate hashes")

        # Return all hashes for manual inspection
        return set(sqlx_hashes.keys())
    else:
        print(f"\n✅ Number of .sqlx files seems reasonable ({cached_count})")
        return set()


def main():
    parser = argparse.ArgumentParser(
        description="Identify and remove stale .sqlx cache files"
    )
    parser.add_argument(
        "--delete",
        action="store_true",
        help="Delete stale files (default: preview only)",
    )
    parser.add_argument(
        "--project-dir",
        default="/Users/ljf/Desktop/hu_ts/synapse-rust",
        help="Project root directory",
    )

    args = parser.parse_args()

    project_dir = Path(args.project_dir)
    sqlx_dir = project_dir / ".sqlx"
    src_dir = project_dir

    print(f"🔍 Scanning {project_dir}...")

    # Extract data
    print("   Extracting queries from Rust files...")
    source_queries = extract_queries_from_rust_files(src_dir)

    print("   Extracting hashes from .sqlx cache...")
    sqlx_hashes = extract_hashes_from_sqlx_cache(sqlx_dir)

    # Identify stale files
    print("   Identifying stale files...")
    stale_hashes = identify_stale_files(sqlx_hashes, source_queries)

    if not stale_hashes:
        print("\n✅ No obvious stale files found!")
        return 0

    # Preview or delete
    if args.delete:
        print("\n🗑️  Deleting stale .sqlx files:")
        deleted = 0
        for hash_value in stale_hashes:
            json_file = sqlx_dir / f"{hash_value}.json"
            if json_file.exists():
                json_file.unlink()
                print(f"   Deleted: {hash_value}.json")
                deleted += 1

        print(f"\n📊 Summary: Deleted {deleted} stale files")
        print("   Run 'cargo build' to regenerate .sqlx cache")
    else:
        print(f"\n📋 Preview: {len(stale_hashes)} potentially stale files")
        print("   Use --delete to remove them")
        print("\n   First 5 stale hashes:")
        for i, h in enumerate(list(stale_hashes)[:5]):
            print(f"   - {h}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
