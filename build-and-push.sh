#!/bin/bash
set -e

echo "🚀 开始构建生产级 Docker 镜像..."

# 设置变量
IMAGE_NAME="vmuser232922/mysynapse"
VERSION="v6.0.5"
LATEST_TAG="latest"

# 切换到项目根目录
cd "$(dirname "$0")"

echo "📦 构建 amd64 架构的 Docker 镜像..."
docker buildx build \
  --platform linux/amd64 \
  --file docker/Dockerfile \
  --tag ${IMAGE_NAME}:${VERSION} \
  --tag ${IMAGE_NAME}:${LATEST_TAG} \
  --load \
  .

echo "✅ 镜像构建完成"

echo "📤 推送镜像到 Docker Hub..."
docker push ${IMAGE_NAME}:${VERSION}
docker push ${IMAGE_NAME}:${LATEST_TAG}

echo "✅ 镜像推送完成"
echo ""
echo "镜像信息:"
echo "  - ${IMAGE_NAME}:${VERSION}"
echo "  - ${IMAGE_NAME}:${LATEST_TAG}"
echo ""
docker images | grep mysynapse
