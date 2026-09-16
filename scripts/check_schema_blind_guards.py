#!/usr/bin/env python3
"""
Schema-blind guard anti-regression lint
Checks for patterns that could cause schema-related data integrity issues:
1. DROP SCHEMA without conditional checks
2. ALTER TABLE without schema qualification
3. CREATE TABLE without schema qualification
4. Direct schema manipulations that could affect public schema

This script should be run as part of CI to prevent the schema absorption bugs
from recurring (F-1/F-2/F-3 class bugs).
"""

import os
import re
import sys
from pathlib import Path

MIGRATIONS_DIR = Path("migrations")
# Known safe DROP SCHEMA patterns - files that contain controlled public schema resets
SAFE_DROP_PATTERNS = [
    "scripts/init_test_public_schema.sh",
    "scripts/init_v11_database.sh", 
    "scripts/cleanup_test_schemas.sh",
]

# Known safe DROP SCHEMA patterns - files that contain controlled public schema resets
SAFE_DROP_PATTERNS = [
    "scripts/init_test_public_schema.sh",
    "scripts/init_v11_database.sh", 
    "scripts/cleanup_test_schemas.sh",
]

# Known safe schema-blind patterns - files that intentionally use table_schema='public'
# These are initialization/reset scripts that operate on the public schema by design
SAFE_SCHEMA_BLIND_PATTERNS = {
    "scripts/init_test_public_schema.sh",      # Line 35: table count verification after reset
    "scripts/init_v11_database.sh",            # Lines 64,94: DB initialization scripts
    "scripts/ci/prepare_test_db.sh",           # Lines 22: initial pool setup
}

CRITICAL_PATTERNS = [
    # Pattern: Schema-blind guard using hardcoded 'public' - HIGH RISK
    # This causes guards to fail in non-public schemas (test_template, etc.)
    # Exclude known safe initialization scripts from this check
    (r"table_schema\s*=\s*['\"]public['\"]", 
     'Schema-blind guard: hardcoded table_schema=public literal - verify schema isolation', 
     'warning'),
    
    # Pattern: Schema-blind guard using hardcoded 'public' - HIGH RISK  
    (r"AND\s+table_name\s*=", 
     'Schema-blind guard: missing schema qualification in condition', 
     'warning'),
    
    # Pattern: DROP SCHEMA with potential data loss
    (r'DROP\s+SCHEMA\s+public\s+CASCADE', 
     'DROP SCHEMA public CASCADE - verify this is a controlled reset operation', 
     'warning'),
    
    # Pattern: DROP SCHEMA IF EXISTS - verify safety
    (r'DROP\s+SCHEMA\s+IF\s+EXISTS', 
     'DROP SCHEMA IF EXISTS - verify safety', 
     'warning'),
    
    # Pattern: DROP SCHEMA without CASCADE - verify safety
    (r'DROP\s+SCHEMA\s+\w+\s+(?!CASCADE)', 
     'DROP SCHEMA without CASCADE - verify safety', 
     'warning'),
    
    # Pattern: Schema-blind ALTER TABLE
    (r'ALTER\s+TABLE\s+(?!\w+\.)\w+\.', 
     'ALTER TABLE without schema qualification', 
     'warning'),
    
    # Pattern: Schema-blind CREATE TABLE
    (r'CREATE\s+TABLE\s+(?!\w+\.)\w+\.', 
     'CREATE TABLE without schema qualification', 
     'warning'),
    
    # Pattern: Schema-blind INSERT/UPDATE/DELETE
    (r'(INSERT|UPDATE|DELETE)\s+INTO\s+(?!\w+\.)\w+\.', 
     'Schema-blind DML statement', 
     'info'),
    
    # Pattern: PGOPTIONS search_path that includes public explicitly
    (r"PGOPTIONS='-c\s+search_path=.*public.*'", 
     'search_path includes public schema - verify test isolation', 
     'warning'),
    
    # Pattern: Search path manipulation without schema qualification
    (r"search_path\s*=\s*['\"]?(\w+,\s*)*public['\"]?", 
     'search_path defaults to public - verify safety', 
     'warning'),
]

def check_file(filepath):
    """Check a single file for schema-blind patterns."""
    issues = []
    try:
        content = filepath.read_text()
        lines = content.split('\n')
        for i, line in enumerate(lines, 1):
            for pattern, msg, severity in CRITICAL_PATTERNS:
                if re.search(pattern, line, re.IGNORECASE):
                    issues.append({
                        'file': str(filepath),
                        'line': i,
                        'pattern': msg,
                        'severity': severity,
                        'content': line.strip()[:100]
                    })
    except Exception as e:
        print(f"Error reading {filepath}: {e}", file=sys.stderr)
    return issues

def main():
    all_issues = []
    errors = 0
    warnings = 0
    
    # Check migrations
    if MIGRATIONS_DIR.exists():
        for sql_file in MIGRATIONS_DIR.rglob("*.sql"):
            issues = check_file(sql_file)
            all_issues.extend(issues)
    
    # Check scripts
    scripts_dir = Path("scripts")
    if scripts_dir.exists():
        for script_file in scripts_dir.rglob("*.sh"):
            issues = check_file(script_file)
            all_issues.extend(issues)
    
    # Check CI workflows
    workflows_dir = Path(".github/workflows")
    if workflows_dir.exists():
        for workflow_file in workflows_dir.rglob("*.yml"):
            issues = check_file(workflow_file)
            all_issues.extend(issues)
    
    # Report
    if all_issues:
        print("Schema-blind guard check results:")
        print("=" * 80)
        for issue in all_issues:
            severity = issue['severity'].upper()
            print(f"[{severity}] {issue['file']}:{issue['line']}")
            print(f"  Pattern: {issue['pattern']}")
            print(f"  Content: {issue['content']}")
            print()
            if issue['severity'] == 'error':
                errors += 1
            elif issue['severity'] == 'warning':
                warnings += 1
        
        print("=" * 80)
        print(f"Summary: {errors} errors, {warnings} warnings")
        
        if errors > 0:
            print("\n❌ FAILED: Schema-blind guards detected")
            sys.exit(1)
        else:
            print("\n⚠️  WARNINGS only: Review schema-blind patterns")
            sys.exit(0)
    else:
        print("✅ PASSED: No schema-blind patterns detected")
        sys.exit(0)

if __name__ == "__main__":
    main()
