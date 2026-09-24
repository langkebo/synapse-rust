#!/usr/bin/env bash
#
# 门控模块「测试过滤器必须命中」检查（W5 / D-25 的门禁实现）。
#
# 背景：`#[cfg(feature = "X")] pub mod m;` 在 X 未打开时根本不参与编译，于是
# `cargo nextest run -E 'test(m)'` 匹配 0 个用例。libtest 打印 `running 0 tests`
# 并 exit 0、nextest exit 4，两者都会被当成"通过"⇒ 假绿（本仓已踩 4 次：
# C6 server_notification、C9 saml、C11 friend_room、C15 cas）。
#
# 本脚本**不重复实现**"0 个用例即失败"这段逻辑 —— 它调用既有的唯一实现
# `scripts/ci/require_tests_ran.sh`（铁律 2），并按
# `scripts/ci/gated_module_test_matrix` 登记的表逐行检查所需 feature。
#
# 用法：
#   bash scripts/ci/check_gated_module_tests.sh              # 全部行（CI 步骤）
#   bash scripts/ci/check_gated_module_tests.sh friend_room  # 只查匹配的行（本地）
#
# 环境：需要 DATABASE_URL / TEST_DATABASE_URL 与已建好的库（这些是 DB 往返用例）；
# 未设置时按 CI 约定用 synapse_test。
#
# 退出码：任一行匹配 0 个用例 → 1；被测命令失败 → 原样透传；全部命中 → 0。
set -uo pipefail

cd "$(dirname "$0")/../.." # repo root

MATRIX="scripts/ci/gated_module_test_matrix"
WRAPPER="scripts/ci/require_tests_ran.sh"

if [ ! -f "$MATRIX" ]; then
    echo "::error::${MATRIX} 不存在；门控模块的 feature 集没有登记处" >&2
    exit 1
fi
if [ ! -f "$WRAPPER" ]; then
    echo "::error::${WRAPPER} 不存在（0 个用例即失败的唯一实现）" >&2
    exit 1
fi

only=""
list_only=0
case "${1:-}" in
    --list) list_only=1 ;;
    "") ;;
    *) only="$1" ;;
esac
check_all_features="${GATED_MODULES_ALL_FEATURES:-1}"

# 组装 feature 参数：默认 `--all-features`。
#
# 为什么用 `--all-features` 而不是"每行只开自己那个 feature"：`--all-features` 正是
# CI 的 lib 批次已经在用的口径（`.github/workflows/ci.yml` 的
# `cargo nextest run --workspace --lib --all-features`），因此逐行检查**不额外产生任何
# 编译**，只是用不同过滤器重跑同一个已构建的二进制。逐行单独 feature 会为 N 行付出
# N 次全量构建，而它多证明的那一点（"这个 feature 的确是这个模块的开关"）已由
# `tests/unit/gated_module_test_gate_tests.rs` 静态断言 `lib.rs` 的
# `#[cfg(feature = "…")]` 锚点来覆盖。
feature_args=(--all-features)
if [ "$check_all_features" = "0" ]; then
    feature_args=()
fi

checked=0
failed=0
while IFS='|' read -r filter feature anchor; do
    case "$filter" in '' | '#'*) continue ;; esac
    if [ -n "$only" ] && [ "$filter" != "$only" ]; then
        continue
    fi
    if [ "$list_only" -eq 1 ]; then
        echo "${filter}|${feature}|${anchor}"
        continue
    fi
    checked=$((checked + 1))
    echo "==> 门控模块 '${filter}'（feature: ${feature}，声明于 ${anchor}）"
    if ! bash "$WRAPPER" cargo nextest run --workspace --lib "${feature_args[@]}" --locked --test-threads 4 \
        -E "test(/$filter/)"; then
        echo "::error::门控模块 '${filter}' 的过滤器匹配不到任何用例。若这一步是\"因为 feature 没打开\"，" >&2
        echo "  请在批次/CI 里补上 feature '${feature}'（登记于 ${MATRIX}）；若模块已删/改名，请更新该表。" >&2
        failed=$((failed + 1))
    fi
done <"$MATRIX"

if [ "$list_only" -eq 1 ]; then
    exit 0
fi

if [ "$checked" -eq 0 ]; then
    echo "::error::${MATRIX} 里没有匹配 '${only}' 的行 —— 空表/空选择也算门禁失效" >&2
    exit 1
fi

if [ "$failed" -ne 0 ]; then
    echo "::error::${failed}/${checked} 个门控模块的过滤器没有命中任何用例" >&2
    exit 1
fi

echo "OK: ${checked} 个门控模块的过滤器都命中了用例。"
