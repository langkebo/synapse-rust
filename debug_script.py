#!/usr/bin/env python3
import re, os

STORAGE_DIR = '/Users/ljf/Desktop/hu_ts/synapse-rust/src/storage'
filepath = os.path.join(STORAGE_DIR, 'application_service.rs')
with open(filepath) as f:
    content = f.read()

# Test struct parsing
struct_pattern = re.compile(r'pub\s+struct\s+(\w+)\s*\{', re.DOTALL)
for match in struct_pattern.finditer(content):
    struct_name = match.group(1)
    start = match.end()
    depth = 1
    pos = start
    while pos < len(content) and depth > 0:
        if content[pos] == '{': depth += 1
        elif content[pos] == '}': depth -= 1
        pos += 1
    body = content[start:pos-1]
    print(f'Struct: {struct_name}')
    for line in body.split('\n'):
        stripped = line.strip()
        m = re.match(r'pub\s+(\w+)\s*:\s*(.+?)(?:,|$)', stripped)
        if m:
            field_name = m.group(1)
            field_type = m.group(2).strip().rstrip(',')
            is_optional = field_type.startswith('Option<')
            print(f'  {field_name}: {field_type} (optional={is_optional})')
    print()

# Test query_as! matching
print("\n=== query_as! calls ===")
query_as_pattern = re.compile(r'sqlx::query_as!\s*\(\s*\n?\s*(\w+)\s*,', re.DOTALL)
for match in query_as_pattern.finditer(content):
    struct_name = match.group(1)
    snippet = content[match.start():match.start()+60]
    print(f'Found query_as!({struct_name}, ...) at pos {match.start()}')
    print(f'  Snippet: {snippet!r}')
    
    # Try to find SQL string
    after_pos = match.end()
    raw_match = re.search(r'(r(#*))"', content[after_pos:after_pos+50])
    if raw_match:
        hash_count = len(raw_match.group(2))
        open_quote = 'r' + '#' * hash_count + '"'
        close_quote = '"' + '#' * hash_count
        quote_pos = content.find(open_quote, after_pos)
        if quote_pos != -1:
            sql_start = quote_pos + len(open_quote)
            sql_end = content.find(close_quote, sql_start)
            if sql_end != -1:
                sql_text = content[sql_start:sql_end]
                print(f'  SQL ({len(sql_text)} chars): {sql_text[:200]!r}...')
            else:
                print(f'  Could not find close quote: {close_quote!r}')
        else:
            print(f'  Could not find open quote: {open_quote!r}')
    else:
        print(f'  No raw string found after pos {after_pos}')
        print(f'  Content: {content[after_pos:after_pos+50]!r}')
    print()