#!/bin/bash
# Synapse-Rust Cache Cleanup Script
# Usage: ./scripts/clean_cache.sh [--dry-run]

set -e

DRY_RUN=false
if [ "$1" == "--dry-run" ]; then
    DRY_RUN=true
    echo "[Dry-Run Mode] No actual deletion will occur."
fi

echo "========================================"
echo "  Synapse-Rust Cache Cleanup"
echo "========================================"

# Cargo cache
echo ""
echo "[1/3] Cleaning Cargo cache..."
if $DRY_RUN; then
    echo "  Would run: cargo clean"
    echo "  Would remove: ~/.cargo/bin/cargo-* (old versions)"
else
    cargo clean
    # Remove old cargo versions (optional)
    echo "  Cargo cache cleaned."
fi

# Docker builder cache
echo ""
echo "[2/3] Cleaning Docker builder cache..."
if $DRY_RUN; then
    echo "  Would run: docker builder prune -f"
else
    docker builder prune -f || echo "  Docker not available or no cache to prune"
    echo "  Docker builder cache cleaned."
fi

# Target directory (optional, aggressive cleanup)
echo ""
echo "[3/3] Additional cleanup..."
if $DRY_RUN; then
    echo "  Would remove: target/ directory"
    echo "  Would remove: .tarpaulin/ directory"
else
    # Only remove if explicitly requested
    if [ "$1" == "--full" ]; then
        rm -rf target/ .tarpaulin/ 2>/dev/null || true
        echo "  Full cleanup complete (removed target/ and .tarpaulin/)."
    else
        echo "  Use --full flag to also remove target/ and .tarpaulin/ directories."
    fi
fi

echo ""
echo "========================================"
if $DRY_RUN; then
    echo "  Dry run complete. No files deleted."
else
    echo "  Cache cleanup complete!"
fi
echo "========================================"
