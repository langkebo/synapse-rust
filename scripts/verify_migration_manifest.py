#!/usr/bin/env python3
import argparse
import hashlib
import re
from pathlib import Path
import sys


REPO_ROOT = Path(__file__).resolve().parents[1]
MIGRATIONS_DIR = REPO_ROOT / "migrations"
TABLE_ROW = re.compile(r"^\|\s*(.+?)\s*\|\s*(.+?)\s*\|\s*([a-fA-F0-9]{64}|)\s*\|(?:\s*(.+?)\s*\|(?:\s*(.+?)\s*\|)?)?$")


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def parse_manifest(path: Path) -> list[tuple[str, int, str]]:
    entries: list[tuple[str, int, str]] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if not line.startswith("|"):
            continue
        if "Filename" in line or "---" in line or "(none)" in line:
            continue
        match = TABLE_ROW.match(line)
        if not match:
            continue
        filename, size, checksum = match.group(1), match.group(2), match.group(3)
        if not checksum:
            continue
        try:
            entries.append((filename, int(size), checksum.lower()))
        except ValueError:
            continue
    return entries


def resolve_path(filename: str) -> Path:
    candidate = MIGRATIONS_DIR / filename
    if candidate.exists():
        return candidate
    return REPO_ROOT / filename


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("manifest")
    args = parser.parse_args()

    manifest_path = Path(args.manifest)
    if not manifest_path.is_absolute():
        manifest_path = REPO_ROOT / manifest_path
    if not manifest_path.exists():
        print(f"Manifest not found: {manifest_path}", file=sys.stderr)
        return 2

    failures = []
    for filename, expected_size, expected_checksum in parse_manifest(manifest_path):
        target = resolve_path(filename)
        if not target.exists():
            failures.append(f"missing file: {filename}")
            continue
        actual_size = target.stat().st_size
        actual_checksum = sha256_file(target)
        if actual_size != expected_size:
            failures.append(
                f"size mismatch: {filename} expected={expected_size} actual={actual_size}"
            )
        if actual_checksum != expected_checksum:
            failures.append(
                f"checksum mismatch: {filename} expected={expected_checksum} actual={actual_checksum}"
            )

    if failures:
        print("Manifest verification failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print(f"Manifest verification passed: {manifest_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
