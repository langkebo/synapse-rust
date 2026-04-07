#!/usr/bin/env python3
import argparse
import shutil
from pathlib import Path


def is_run_dir(path: Path) -> bool:
    if not path.is_dir():
        return False
    name = path.name
    if name in {"latest", "archive", "runs"}:
        return False
    return True


def resolve_keep_names(base: Path, keep: int) -> set[str]:
    run_dirs = [p for p in base.iterdir() if is_run_dir(p)]
    run_dirs.sort(key=lambda p: p.stat().st_mtime, reverse=True)
    keep_set = {p.name for p in run_dirs[: max(keep, 0)]}
    latest = base / "latest"
    if latest.exists():
        try:
            target = latest.resolve()
            if target.parent == base and target.is_dir():
                keep_set.add(target.name)
        except OSError:
            pass
    return keep_set


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--dir", default="test-results")
    parser.add_argument("--keep", type=int, default=5)
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()

    base = Path(args.dir)
    if not base.exists():
        return 0
    if not base.is_dir():
        raise SystemExit(f"Not a directory: {base}")

    keep_names = resolve_keep_names(base, args.keep)
    removed = 0
    kept = 0
    for entry in sorted(base.iterdir(), key=lambda p: p.name):
        if not is_run_dir(entry):
            continue
        if entry.name in keep_names:
            kept += 1
            continue
        if args.dry_run:
            print(f"Would remove {entry}")
        else:
            shutil.rmtree(entry)
            print(f"Removed {entry}")
        removed += 1

    print(f"Kept {kept} run dirs; removed {removed} run dirs")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

