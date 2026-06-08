#!/usr/bin/env python3
import re, os

STORAGE_DIR = '/Users/ljf/Desktop/hu_ts/synapse-rust/src/storage'
filepath = os.path.join(STORAGE_DIR, 'application_service.rs')
with open(filepath) as f:
    content = f.read()

# Find all sqlx::query_as! occurrences
print("Looking for query_as! in content...")
for m in re.finditer(r'sqlx::query_as!', content):
    print(f"  Found at {m.start()}: ...{content[m.start():m.start()+80]!r}...")

print("\nWith full pattern:")
query_as_pattern = re.compile(r'sqlx::query_as!\s*\(\s*\n?\s*(\w+)\s*,', re.DOTALL)
matches = list(query_as_pattern.finditer(content))
print(f"  Matches found: {len(matches)}")
for m in matches:
    print(f"  Struct: {m.group(1)}, pos: {m.start()}-{m.end()}")
    print(f"    Context: ...{content[m.start():m.end()+30]!r}...")