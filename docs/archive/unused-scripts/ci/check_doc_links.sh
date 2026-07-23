#!/usr/bin/env bash
# check_doc_links.sh — 检查 Markdown 文档中的本地文件链接是否存在
#
# 用法: bash scripts/ci/check_doc_links.sh [file_or_dir...]
# 默认: 检查 README.md 和 docs/
#
# 退出码:
#   0 — 所有链接有效
#   1 — 存在断链

set -euo pipefail

TARGETS=("${@:-README.md docs}")
BROKEN=0

check_file() {
    local file="$1"
    local dir
    dir="$(dirname "$file")"

    # 提取 markdown 链接中的本地路径（排除 http/https/ftp 链接和锚点）
    while IFS= read -r line; do
        # 跳过空行和注释
        [[ -z "$line" ]] && continue

        # 提取链接路径部分 [text](path) 中的 path
        # 排除: http://, https://, ftp://, #anchor, mailto:
        local path
        path=$(echo "$line" | grep -oE '\]\([^)]+\)' | sed 's/\](//;s/)//' |
            grep -vE '^https?://' |
            grep -vE '^ftp://' |
            grep -vE '^mailto:' |
            grep -vE '^#' |
            head -1 || true)

        [[ -z "$path" ]] && continue

        # 去除锚点部分
        local file_path="${path%%#*}"

        [[ -z "$file_path" ]] && continue

        # 解析相对路径
        local resolved
        if [[ "$file_path" = /* ]]; then
            resolved="$file_path"
        else
            resolved="$dir/$file_path"
        fi

        # 规范化路径
        resolved=$(cd "$dir" 2>/dev/null && realpath -m "$file_path" 2>/dev/null || echo "$resolved")

        if [[ ! -e "$resolved" ]]; then
            echo "BROKEN: $file -> $path (resolved: $resolved)"
            BROKEN=$((BROKEN + 1))
        fi
    done < <(grep -nE '\]\(' "$file" 2>/dev/null || true)
}

echo "Checking markdown links..."

for target in "${TARGETS[@]}"; do
    if [[ -f "$target" ]]; then
        check_file "$target"
    elif [[ -d "$target" ]]; then
        while IFS= read -r -d '' f; do
            check_file "$f"
        done < <(find "$target" -name '*.md' -print0 2>/dev/null)
    fi
done

if [ "$BROKEN" -gt 0 ]; then
    echo ""
    echo "FAIL: $BROKEN broken link(s) found"
    exit 1
fi

echo "OK: All markdown links are valid"
exit 0
