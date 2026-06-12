#!/bin/bash
# Layered isolation check script
# Ensures src/services/ and src/storage/ use shim re-exports,
# not full implementations that duplicate synapse-services/ and synapse-storage/
set -euo pipefail

MAX_SERVICE_SHIM_LINES=50
MAX_STORAGE_SHIM_LINES=30
EXIT_CODE=0

echo "=== Layer Isolation Check ==="

# 1. Check src/services/ for non-shim files that have synapse-services counterparts
#    C-class files (>100 lines, both sides have full impls) are INFO only (need manual merge)
#    Files 50-100 lines may have tests — show as INFO
echo ""
echo "--- Service Layer (known diversion for C-class files documented in LAYER_MIGRATION_OPTIMIZATION_PLAN) ---"
while IFS= read -r f; do
    filename=$(basename "$f")
    lines=$(wc -l < "$f" | tr -d ' ')
    # Skip mod.rs, container.rs, and files without synapse-services counterpart
    if [ "$filename" = "mod.rs" ] || [ "$filename" = "container.rs" ]; then
        continue
    fi
    ss_lines=0
    if [ -f "synapse-services/src/$filename" ]; then
        ss_lines=$(wc -l < "synapse-services/src/$filename" | tr -d ' ')
    fi
    if [ "$ss_lines" -gt 0 ] && [ "$lines" -gt "$MAX_SERVICE_SHIM_LINES" ]; then
        diff=$((lines - ss_lines))
        if [ $diff -lt 0 ]; then diff=$((-diff)); fi
        if [ $diff -gt 50 ]; then
            echo "INFO: $f ($lines lines vs synapse-services $ss_lines, diff=$diff) — C-class file, needs manual merge"
        elif [ "$ss_lines" -gt "$MAX_SERVICE_SHIM_LINES" ]; then
            echo "WARNING: $f ($lines lines vs synapse-services $ss_lines, diff=$diff) — both sides have full impls"
        else
            echo "INFO: $f ($lines lines, synapse-services is shim with $ss_lines lines) — may have tests"
        fi
    fi
done < <(find src/services -maxdepth 1 -name "*.rs" -type f)

# 2. Check src/storage/ for non-shim files that have synapse-storage counterparts
while IFS= read -r f; do
    filename=$(basename "$f")
    lines=$(wc -l < "$f" | tr -d ' ')
    # Skip mod.rs
    if [ "$filename" = "mod.rs" ]; then
        continue
    fi
    if [ -f "synapse-storage/src/$filename" ] && [ "$lines" -gt "$MAX_STORAGE_SHIM_LINES" ]; then
        echo "ERROR: $f has $lines lines (> $MAX_STORAGE_SHIM_LINES), should be a shim (synapse-storage/src/$filename exists)"
        EXIT_CODE=1
    fi
done < <(find src/storage -maxdepth 1 -name "*.rs" -type f)

# 3. Check for storage type leaks in service layer
if grep -rn "pub use crate::storage::" src/services/ --include="*.rs" | grep -qv "mod.rs"; then
    echo "ERROR: Service layer re-exports storage types directly:"
    grep -rn "pub use crate::storage::" src/services/ --include="*.rs"
    EXIT_CODE=1
fi

# 4. Check for direct SQL in service layer (not in storage layer)
if grep -rn "sqlx::query" src/services/ --include="*.rs" | grep -qv "database_initializer"; then
    echo "WARNING: Service layer contains direct SQL queries (should use storage layer):"
    grep -rn "sqlx::query" src/services/ --include="*.rs" | grep -v "database_initializer" | head -20
    echo "  (note: database_initializer is excluded as it manages DDL operations)"
fi

# 5. Check for error swallowing in storage operations
if grep -rn "let _ = .*_storage\." src/services/ --include="*.rs" | grep -qv "test"; then
    echo "WARNING: Potential error swallowing in storage operations:"
    grep -rn "let _ = .*_storage\." src/services/ --include="*.rs" | grep -v "test" | head -20
fi

# 6. Check for wildcard re-export of storage module in services
if grep -rn "pub use crate::storage::\*" src/services/ --include="*.rs" | grep -qv "^$"; then
    echo "ERROR: Wildcard re-export of storage module in services:"
    grep -rn "pub use crate::storage::\*" src/services/ --include="*.rs"
    EXIT_CODE=1
fi

# 7. Check for ambiguous_glob_reexports suppression
if grep -rn "allow(ambiguous_glob_reexports)" src/ --include="*.rs" | grep -qv "^$"; then
    echo "WARNING: ambiguous_glob_reexports suppression found:"
    grep -rn "allow(ambiguous_glob_reexports)" src/ --include="*.rs"
fi

if [ $EXIT_CODE -eq 0 ]; then
    echo "PASS: All layer isolation checks passed"
else
    echo "FAIL: Layer isolation violations found"
fi

exit $EXIT_CODE