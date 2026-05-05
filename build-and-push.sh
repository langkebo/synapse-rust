#!/bin/bash
# ----------------------------------------------------------------------------
# build-and-push.sh
#
# 构建生产级 (target=tools, linux/amd64) Docker 镜像，包含：
#   - synapse-rust 二进制 (release 优化, LTO thin, panic=abort, strip symbols)
#   - healthcheck 二进制
#   - migrations/  (全部 SQL 迁移脚本，由 docker/db_migrate.sh 在容器内执行)
#   - docker/db_migrate.sh + docker/deploy/scripts/container-migrate.sh
#   - 默认 rate_limit.yaml
#   - postgresql-client (容器内自治执行迁移与 schema 校验)
#
# 推送目标: vmuser232922/mysynapse 私有仓库 (Docker Hub)
#
# 用法:
#   ./build-and-push.sh                      # 构建并推送 0.1.6-amd64 + latest
#   VERSION=0.1.7 ./build-and-push.sh        # 自定义版本
#   SKIP_PUSH=1 ./build-and-push.sh          # 仅本地构建，不推送
# ----------------------------------------------------------------------------
set -euo pipefail

IMAGE_NAME="${IMAGE_NAME:-vmuser232922/mysynapse}"
VERSION="${VERSION:-0.1.6}"
PLATFORM="${PLATFORM:-linux/amd64}"
ARCH_SUFFIX="${ARCH_SUFFIX:-amd64}"
TARGET_STAGE="${TARGET_STAGE:-tools}"
BUILDER_NAME="${BUILDER_NAME:-amd64builder}"

VERSIONED_TAG="${VERSION}-${ARCH_SUFFIX}"
LATEST_TAG="latest"
BUILD_DATE="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
VCS_REF="$(git -C "$(dirname "$0")" rev-parse --short HEAD 2>/dev/null || echo unknown)"

cd "$(dirname "$0")"

echo "🚀 构建生产级 Docker 镜像"
echo "   IMAGE      : ${IMAGE_NAME}"
echo "   TAGS       : ${VERSIONED_TAG}, ${LATEST_TAG}"
echo "   PLATFORM   : ${PLATFORM}"
echo "   TARGET     : ${TARGET_STAGE}"
echo "   BUILD_DATE : ${BUILD_DATE}"
echo "   VCS_REF    : ${VCS_REF}"
echo ""

# 确保 buildx builder 可用 (跨平台构建在 Apple Silicon 上必须)
if ! docker buildx inspect "${BUILDER_NAME}" >/dev/null 2>&1; then
    echo "🔧 创建 buildx builder: ${BUILDER_NAME}"
    docker buildx create --name "${BUILDER_NAME}" --driver docker-container --use
else
    docker buildx use "${BUILDER_NAME}"
fi
docker buildx inspect --bootstrap >/dev/null

echo "📦 buildx 构建 (${PLATFORM}, target=${TARGET_STAGE})..."
docker buildx build \
    --builder "${BUILDER_NAME}" \
    --platform "${PLATFORM}" \
    --target "${TARGET_STAGE}" \
    --file docker/Dockerfile \
    --build-arg VERSION="${VERSION}" \
    --build-arg BUILD_DATE="${BUILD_DATE}" \
    --build-arg VCS_REF="${VCS_REF}" \
    --tag "${IMAGE_NAME}:${VERSIONED_TAG}" \
    --tag "${IMAGE_NAME}:${LATEST_TAG}" \
    --load \
    .

echo "✅ 构建完成"
docker images "${IMAGE_NAME}" | head -5

if [[ "${SKIP_PUSH:-0}" = "1" ]]; then
    echo "⏭  SKIP_PUSH=1, 跳过推送"
    exit 0
fi

echo ""
echo "📤 推送到 Docker Hub..."
docker push "${IMAGE_NAME}:${VERSIONED_TAG}"
docker push "${IMAGE_NAME}:${LATEST_TAG}"

echo "✅ 推送完成"
echo ""
echo "镜像清单:"
echo "  - ${IMAGE_NAME}:${VERSIONED_TAG}"
echo "  - ${IMAGE_NAME}:${LATEST_TAG}"
