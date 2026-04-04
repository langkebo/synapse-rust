#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

python3 - <<'PY'
import os
import re
import sys

root = os.path.join(os.getcwd(), "src")
excluded_prefixes = [
    os.path.join(root, "bin") + os.sep,
]

string_placeholder = re.compile(r'"[^"\n]*placeholder[^"\n]*"')
cfg_test = re.compile(r'^\s*#\s*\[\s*cfg\s*\(\s*test\s*\)\s*\]\s*$')
mod_decl = re.compile(r'^\s*mod\s+([A-Za-z_][A-Za-z0-9_]*)\b')

bad = 0

for dirpath, _, filenames in os.walk(root):
    for fn in filenames:
        if not fn.endswith(".rs"):
            continue
        path = os.path.join(dirpath, fn)
        if any(path.startswith(p) for p in excluded_prefixes):
            continue

        pending_test_mod = False
        in_test_mod = False
        depth = 0

        with open(path, "r", encoding="utf-8", errors="replace") as f:
            for i, line in enumerate(f, start=1):
                if in_test_mod:
                    depth += line.count("{") - line.count("}")
                    if depth <= 0:
                        in_test_mod = False
                        depth = 0
                    continue

                if cfg_test.match(line):
                    pending_test_mod = True
                    continue

                if pending_test_mod:
                    m = mod_decl.match(line)
                    if m:
                        in_test_mod = True
                        depth = line.count("{") - line.count("}")
                        if depth <= 0:
                            depth = 1
                        pending_test_mod = False
                        continue
                    if line.strip() and not line.lstrip().startswith("#["):
                        pending_test_mod = False

                if "placeholder" not in line:
                    continue
                if not string_placeholder.search(line):
                    continue

                rel = os.path.relpath(path, os.getcwd()).replace("\\", "/")
                print(f"{rel}:{i}: placeholder string literal is not allowed outside #[cfg(test)] modules")
                bad = 1

sys.exit(bad)
PY

