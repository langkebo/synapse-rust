#!/usr/bin/env python3
"""Verify TOML count assertions in Markdown audit documents."""

import sys
import re
from pathlib import Path

MARKDOWN_DIR = Path(__file__).parent.parent / "docs" / "audit"
TOML_DIR = Path(__file__).parent.parent / "synapse-services"

# Count patterns
patterns = {
    'ext_starts': r'^\s*\[ext\.',
    'synapse_web_routes': r'^\s*\[synapse-web\s',
    'synapse_web_structs': r'^\s*\[synapse-web\s+\S+\]\s*as\s+\w+',
    'synapse_web_routes_struct_annotated': r'^\s*\[synapse-web\s+\S+\]\s*-->\s*struct',
    'synapse_web_routes_enum_annotated': r'^\s*\[synapse-web\s+\S+\]\s*-->\s*enum',
    'synapse_web_routes_union_annotated': r'^\s*\[synapse-web\s+\S+\]\s*-->\s*union',
    'synapse_services_structs': r'^\s*\[synapse-services\s+\S+\]\s*-->\s*(struct|enum|union)',
}

def count_toml_patterns():
    """Count actual TOML markers in synapse-services."""
    counts = {}
    
    # Count ext starts
    ext_count = 0
    for toml_file in TOML_DIR.glob("**/*.toml"):
        try:
            content = toml_file.read_text(encoding='utf-8')
            matches = re.findall(patterns['ext_starts'], content, re.MULTILINE)
            ext_count += len(matches)
        except Exception:
            pass
    counts['ext_starts'] = ext_count
    
    # Count synapse-web routes entries
    route_count = 0
    struct_count = 0
    struct_annotated = 0
    enum_annotated = 0
    union_annotated = 0
    services_count = 0
    
    for toml_file in TOML_DIR.glob("**/*routes*.toml"):
        try:
            content = toml_file.read_text(encoding='utf-8')
            route_count += len(re.findall(patterns['synapse_web_routes'], content, re.MULTILINE))
            struct_annotated += len(re.findall(patterns['synapse_web_routes_struct_annotated'], content, re.MULTILINE))
            enum_annotated += len(re.findall(patterns['synapse_web_routes_enum_annotated'], content, re.MULTILINE))
            union_annotated += len(re.findall(patterns['synapse_web_routes_union_annotated'], content, re.MULTILINE))
        except Exception:
            pass
    
    counts['synapse_web_routes'] = route_count
    counts['synapse_web_routes_struct_annotated'] = struct_annotated
    counts['synapse_web_routes_enum_annotated'] = enum_annotated
    counts['synapse_web_routes_union_annotated'] = union_annotated
    counts['synapse_web_routes_struct_total'] = struct_annotated + enum_annotated + union_annotated
    
    # Count synapse-services struct annotations
    for toml_file in TOML_DIR.glob("**/*.toml"):
        try:
            if 'routes' in str(toml_file):
                continue  # already counted
            content = toml_file.read_text(encoding='utf-8')
            services_count += len(re.findall(patterns['synapse_services_structs'], content, re.MULTILINE))
        except Exception:
            pass
    
    counts['synapse_services_structs'] = services_count
    
    return counts

def verify_markdown_assertions():
    """Verify all TOML count assertions in Markdown files."""
    counts = count_toml_patterns()
    assertions = []
    
    for md_file in MARKDOWN_DIR.glob("*.md"):
        try:
            content = md_file.read_text(encoding='utf-8')
            # Find assertion lines like "文件里共有 202 条 [...]"
            for line in content.split('\n'):
                # Match Chinese count assertions
                match = re.search(r'文件里共有\s+(\d+)\s+条 \[ext\.', line)
                if match:
                    expected = int(match.group(1))
                    actual = counts['ext_starts']
                    assertions.append((md_file.name, line.strip(), expected, actual))
                
                match = re.search(r'共有\s+(\d+)\s+条\[synapse-web\s', line)
                if match:
                    expected = int(match.group(1))
                    actual = counts['synapse_web_routes']
                    assertions.append((md_file.name, line.strip(), expected, actual))
                
                match = re.search(r'(\d+)\s+struct注解', line)
                if match:
                    expected = int(match.group(1))
                    actual = counts['synapse_web_routes_struct_annotated']
                    assertions.append((md_file.name, line.strip(), expected, actual))
                
                match = re.search(r'(\d+)\s+enum注解', line)
                if match:
                    expected = int(match.group(1))
                    actual = counts['synapse_web_routes_enum_annotated']
                    assertions.append((md_file.name, line.strip(), expected, actual))
                
                match = re.search(r'(\d+)\s+union注解', line)
                if match:
                    expected = int(match.group(1))
                    actual = counts['synapse_web_routes_union_annotated']
                    assertions.append((md_file.name, line.strip(), expected, actual))
                
        except Exception as e:
            print(f"Error reading {md_file}: {e}", file=sys.stderr)
    
    # Report results
    errors = []
    for file_name, line, expected, actual in assertions:
        if expected != actual:
            errors.append({
                'file': file_name,
                'line': line,
                'expected': expected,
                'actual': actual
            })
    
    if errors:
        print("TOML count assertions FAILED:")
        for err in errors:
            print(f"  {err['file']}")
            print(f"    Line: {err['line']}")
            print(f"    Expected: {err['expected']}, Actual: {err['actual']}")
        return False
    else:
        print("All TOML count assertions PASSED")
        for file_name, line, expected, actual in assertions:
            print(f"  ✓ {file_name}: {expected} == {actual}")
        return True

if __name__ == '__main__':
    success = verify_markdown_assertions()
    sys.exit(0 if success else 1)
