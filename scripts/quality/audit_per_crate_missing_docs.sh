#!/usr/bin/env bash
# scripts/quality/audit_per_crate_missing_docs.sh
#
# B-3.1-b step 1: 摸底 6 个 crate 各自的 missing docs 数量。
# 临时把每个 crate 的 #![allow(missing_docs)] 改为 #![warn(missing_docs)]，
# 跑 cargo doc 统计，输出后立即回退。
#
# 输出写到 stdout：每个 crate 一行 "<crate>: <N> missing docs"
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

CRATES=(
    "synapse-common"
    "synapse-cache"
    "synapse-storage"
    "synapse-e2ee"
    "synapse-federation"
    "synapse-services"
    "synapse-rust"
)

for crate in "${CRATES[@]}"; do
    # 找 crate 的 lib.rs
    case "$crate" in
        synapse-rust)
            lib_rs="src/lib.rs"
            ;;
        *)
            lib_rs="${crate}/src/lib.rs"
            ;;
    esac

    if [[ ! -f "$lib_rs" ]]; then
        echo "$crate: SKIP (no lib.rs at $lib_rs)"
        continue
    fi

    # 备份 + 切换 allow → warn
    cp "$lib_rs" "${lib_rs}.bak"
    sed -i.bak 's/^#!\[allow(missing_docs)\]/#![warn(missing_docs)]/' "$lib_rs"

    # 跑 cargo doc
    count=$(cargo doc --no-deps -p "$crate" --all-features 2>&1 | grep -c "warning: missing documentation" || true)

    # 回退
    mv "${lib_rs}.bak" "$lib_rs"

    echo "$crate: $count missing docs"
done
