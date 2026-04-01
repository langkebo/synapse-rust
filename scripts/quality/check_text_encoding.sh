#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

python3 - <<'PY'
import os
import re
import sys

root = os.path.join(os.getcwd(), "src", "web")
patterns = [
    (re.compile(r'CONTENT_TYPE,\s*"text/html"\s*\)'), "text/html; charset=utf-8"),
    (re.compile(r'CONTENT_TYPE,\s*"application/xml"\s*\)'), "application/xml; charset=utf-8"),
]

bad = 0
for dirpath, _, filenames in os.walk(root):
    for fn in filenames:
        if not fn.endswith(".rs"):
            continue
        path = os.path.join(dirpath, fn)
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            for i, line in enumerate(f, start=1):
                for pat, expected in patterns:
                    if pat.search(line):
                        rel = os.path.relpath(path, os.getcwd()).replace("\\", "/")
                        print(f"{rel}:{i}: missing {expected}")
                        bad = 1
sys.exit(bad)
PY
