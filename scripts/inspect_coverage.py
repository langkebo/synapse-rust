#!/usr/bin/env python3
"""Inspect tarpaulin JSON structure."""

import json

with open("coverage/tarpaulin-report.json") as f:
    data = json.load(f)

print("Top-level keys:", list(data.keys()))
print(f"covered={data['covered']}, coverable={data['coverable']}")
print(f"coverage type: {type(data['coverage']).__name__}")
print(f"files type: {type(data['files']).__name__}, len={len(data['files'])}")
print()

files = data["files"]
if isinstance(files, list) and files:
    print("First file entry:")
    entry = files[0]
    if isinstance(entry, dict):
        for k, v in entry.items():
            print(f"  {k}: {type(v).__name__} = {str(v)[:150]}")
    elif isinstance(entry, list):
        print(f"  list of {len(entry)} items, first: {str(entry[:3])[:150]}")
    elif isinstance(entry, str):
        print(f"  string: {entry[:200]}")
