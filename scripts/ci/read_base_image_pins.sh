#!/usr/bin/env bash
# 读出 `docker/Dockerfile` 里三个基础镜像 ARG 的**唯一真相源**值（已 digest-pin）。
#
# 为什么需要这个脚本
# ------------------
# `docker-security-scan.yml` 的 `Digest Pin Integrity` job 曾经把三个 digest
# **抄了一份**写死在 workflow 里。于是 Dockerfile 升级 pin 之后，那个 job 仍然在
# 验证**旧** digest —— 它自证不了"当前使用的 pin 有效"，正是本仓反复踩到的
# "门禁不会红"形态。digest 现在只写在 `docker/Dockerfile` 一处，两个 job 都从这里读。
#
# 用法
# ----
#   bash scripts/ci/read_base_image_pins.sh                     # 输出 KEY=REF，每行一条
#   bash scripts/ci/read_base_image_pins.sh RUNTIME_BASE_IMAGE  # 只输出该 ARG 的 REF
#
# 退出码
# ------
#   0  正常
#   2  ARG 缺失、或值没有 `@sha256:` pin（响亮失败；绝不静默当成"没有基础镜像"）
set -euo pipefail

DOCKERFILE="${DOCKERFILE:-docker/Dockerfile}"
ARGS=(RUNTIME_BASE_IMAGE DEBIAN_BASE_IMAGE RUST_BUILDER_IMAGE)

read_pin() {
    local arg="$1" ref
    # 去行尾空白与 `#` 行内注释；digest 里不含 `#`，因此这个剥离是安全的。
    ref="$(sed -n "s/^ARG ${arg}=//p" "$DOCKERFILE" | head -1 | tr -d '\r' | sed 's/[[:space:]]*#.*$//; s/[[:space:]]*$//')"
    if [ -z "$ref" ]; then
        echo "ERROR: ${DOCKERFILE} does not define ARG ${arg}" >&2
        exit 2
    fi
    case "$ref" in
        *@sha256:[0-9a-f][0-9a-f]*) ;;
        *)
            echo "ERROR: ARG ${arg} is not digest-pinned: ${ref}" >&2
            exit 2
            ;;
    esac
    printf '%s\n' "$ref"
}

if [ "$#" -eq 1 ]; then
    read_pin "$1"
elif [ "$#" -eq 0 ]; then
    for arg in "${ARGS[@]}"; do
        # `|| exit 2` 不可省：`read_pin` 在 `$(…)` 子壳里失败时，命令替换的退出码
        # 不会自动传播，脚本会打印空值并以 0 退出 —— 那正是"门禁不会红"的形态。
        ref="$(read_pin "$arg")" || exit 2
        printf '%s=%s\n' "$arg" "$ref"
    done
else
    echo "usage: $0 [RUNTIME_BASE_IMAGE|DEBIAN_BASE_IMAGE|RUST_BUILDER_IMAGE]" >&2
    exit 2
fi
