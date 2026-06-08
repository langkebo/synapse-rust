#!/usr/bin/env python3
"""Fix sqlx::query_as! macro type annotations in storage files."""

import re
import os

STORAGE_DIR = "/Users/ljf/Desktop/hu_ts/synapse-rust/src/storage"

FILES = [
    "application_service.rs",
    "device.rs",
    "openid_token.rs",
    "push_notification.rs",
    "refresh_token.rs",
    "sliding_sync.rs",
    "thread.rs",
    "threepid.rs",
    "token.rs",
    "user.rs",
]


def parse_struct_fields(file_content):
    """Parse all struct definitions and return mapping of struct_name -> {fields, renames}."""
    structs = {}

    # Match struct definitions - handle nested braces (e.g., HashMap<String, Vec<Device>>)
    struct_pattern = re.compile(
        r'pub\s+struct\s+(\w+)\s*\{',
        re.DOTALL
    )

    for match in struct_pattern.finditer(file_content):
        struct_name = match.group(1)
        start = match.end()

        # Find matching closing brace
        depth = 1
        pos = start
        while pos < len(file_content) and depth > 0:
            if file_content[pos] == '{':
                depth += 1
            elif file_content[pos] == '}':
                depth -= 1
            pos += 1

        body = file_content[start:pos - 1]

        fields = {}
        renames = {}

        # Parse field definitions line by line
        lines = body.split('\n')
        i = 0
        while i < len(lines):
            line = lines[i].strip()

            # Check for sqlx rename attribute on previous line(s)
            sqlx_rename = None
            j = i - 1
            while j >= 0 and (lines[j].strip().startswith('#[') or lines[j].strip().startswith('//')):
                rm = re.search(r'#\[sqlx\s*\(\s*rename\s*=\s*"([^"]+)"\s*\)', lines[j])
                if rm:
                    sqlx_rename = rm.group(1)
                    break
                j -= 1

            # Check for serde rename attribute
            serde_rename = None
            j = i - 1
            while j >= 0 and (lines[j].strip().startswith('#[') or lines[j].strip().startswith('//')):
                rm = re.search(r'#\[serde\s*\(\s*rename\s*=\s*"([^"]+)"\s*\)', lines[j])
                if rm:
                    serde_rename = rm.group(1)
                    break
                j -= 1

            # Match field definition: pub field_name: Type,
            m = re.match(r'pub\s+(\w+)\s*:\s*(.+?)(?:,|$)', line)
            if m:
                field_name = m.group(1)
                field_type = m.group(2).strip().rstrip(',')
                is_optional = field_type.startswith('Option<')
                fields[field_name] = is_optional

                if sqlx_rename:
                    renames[field_name] = sqlx_rename

            i += 1

        structs[struct_name] = {'fields': fields, 'renames': renames}

    return structs


def fix_column_aliases(sql_text, struct_info):
    """Replace AS col with AS "col!" or AS "col?" in the SQL string."""
    fields = struct_info['fields']
    renames = struct_info.get('renames', {})

    result = sql_text

    for field_name, is_optional in fields.items():
        sql_col = renames.get(field_name, field_name)
        suffix = '?' if is_optional else '!'

        # Match AS sql_col where sql_col is NOT already quoted with ! or ?
        pattern = re.compile(
            r'\bAS\s+(' + re.escape(sql_col) + r')\b(?!["!?])',
            re.DOTALL
        )

        def make_replacer(col, s):
            def replacer(m):
                return f'AS "{col}{s}"'
            return replacer

        result = pattern.sub(make_replacer(sql_col, suffix), result)

    return result


def fix_query_scalar_aliases(sql_text):
    """For query_scalar!, add AS "col!" annotations."""
    result = sql_text
    result = re.sub(r'\bAS\s+count\b(?!["!?])', 'AS "count!"', result)
    result = re.sub(r'\bAS\s+exists\b(?!["!?])', 'AS "exists!"', result)
    result = re.sub(r'\bAS\s+max_id\b(?!["!?])', 'AS "max_id!"', result)
    result = re.sub(r'\bAS\s+matches\b(?!["!?])', 'AS "matches!"', result)
    result = re.sub(r'\bAS\s+id\b(?!["!?])', 'AS "id!"', result)
    return result


def fix_query_aliases(sql_text):
    """For query!() calls, fix AS aliases."""
    result = sql_text
    result = re.sub(r'\bAS\s+content\b(?!["!?])', 'AS "content!"', result)
    result = re.sub(r'\bAS\s+data_type\b(?!["!?])', 'AS "data_type!"', result)
    result = re.sub(r'\bAS\s+stream_id\b(?!["!?])', 'AS "stream_id!"', result)
    return result


def find_sql_string(content, after_pos):
    """Find the raw/regular SQL string after a position. Returns (sql_start, sql_end) or None."""
    # Look for raw string: r", r#", r##", etc.
    raw_match = re.search(r'(r(#*))"', content[after_pos:after_pos + 50])
    if raw_match:
        hash_count = len(raw_match.group(2))
        open_quote = 'r' + '#' * hash_count + '"'
        close_quote = '"' + '#' * hash_count
    else:
        # Regular string
        quote_match = re.search(r'"', content[after_pos:after_pos + 2])
        if not quote_match:
            return None
        open_quote = '"'
        close_quote = '"'

    quote_pos = content.find(open_quote, after_pos)
    if quote_pos == -1:
        return None

    sql_start = quote_pos + len(open_quote)
    sql_end = content.find(close_quote, sql_start)
    if sql_end == -1:
        return None

    return (sql_start, sql_end)


def process_file(filepath):
    """Process a single file, fixing all sqlx macro type annotations."""
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()

    original_content = content

    # Parse struct definitions
    structs = parse_struct_fields(content)

    # Collect all replacements to make (in reverse order)
    replacements = []  # List of (start, end, new_text)

    # --- query_as! calls ---
    query_as_pattern = re.compile(
        r'sqlx::query_as!\s*\(\s*\n?\s*(\w+)\s*,',
        re.DOTALL
    )

    for match in query_as_pattern.finditer(content):
        struct_name = match.group(1)
        if struct_name not in structs:
            continue

        after_pos = match.end()
        range_result = find_sql_string(content, after_pos)
        if range_result is None:
            continue

        sql_start, sql_end = range_result
        sql_text = content[sql_start:sql_end]
        fixed_sql = fix_column_aliases(sql_text, structs[struct_name])

        if fixed_sql != sql_text:
            replacements.append((sql_start, sql_end, fixed_sql))

    # --- query_scalar! calls ---
    query_scalar_pattern = re.compile(
        r'sqlx::query_scalar!\s*\(\s*\n?\s*',
        re.DOTALL
    )

    for match in query_scalar_pattern.finditer(content):
        after_pos = match.end()
        range_result = find_sql_string(content, after_pos)
        if range_result is None:
            continue

        sql_start, sql_end = range_result
        sql_text = content[sql_start:sql_end]
        fixed_sql = fix_query_scalar_aliases(sql_text)

        if fixed_sql != sql_text:
            replacements.append((sql_start, sql_end, fixed_sql))

    # --- query! calls (not query_as! or query_scalar!) ---
    query_pattern = re.compile(
        r'(?<!_)sqlx::query!\s*\(\s*\n?\s*',
        re.DOTALL
    )

    for match in query_pattern.finditer(content):
        after_pos = match.end()
        range_result = find_sql_string(content, after_pos)
        if range_result is None:
            continue

        sql_start, sql_end = range_result
        sql_text = content[sql_start:sql_end]
        fixed_sql = fix_query_aliases(sql_text)

        if fixed_sql != sql_text:
            replacements.append((sql_start, sql_end, fixed_sql))

    # Sort replacements in reverse order (process from end to start)
    replacements.sort(key=lambda x: x[0], reverse=True)

    # Apply replacements
    for sql_start, sql_end, new_text in replacements:
        content = content[:sql_start] + new_text + content[sql_end:]

    if content != original_content:
        print(f"Modified: {filepath} ({len(replacements)} replacements)")
        with open(filepath, 'w', encoding='utf-8') as f:
            f.write(content)
    else:
        print(f"No changes: {filepath}")


def main():
    for filename in FILES:
        filepath = os.path.join(STORAGE_DIR, filename)
        if os.path.exists(filepath):
            process_file(filepath)
        else:
            print(f"File not found: {filepath}")


if __name__ == '__main__':
    main()