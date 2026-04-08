#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

COMPOSE_FILE="${COMPOSE_FILE:-docker-compose.prod.yml}"
ARTIFACTS_DIR="${ARTIFACTS_DIR:-$SCRIPT_DIR/artifacts}"

if [ ! -f .env ]; then
    cp config/.env.example .env
fi

set -a
. ./.env
set +a

IMAGE_REPO="${SYNAPSE_IMAGE:-synapse-rust}"
IMAGE_TAG="${SYNAPSE_IMAGE_TAG:-latest}"
TOOLS_IMAGE="${SYNAPSE_TOOLS_IMAGE:-synapse-rust-tools}"

compose() {
    docker compose -f "$COMPOSE_FILE" "$@"
}

wait_for_health() {
    local service_name="$1"
    local attempts="${2:-40}"
    local sleep_seconds="${3:-3}"
    local container_id
    container_id="$(compose ps -q "$service_name")"

    if [ -z "$container_id" ]; then
        echo "未找到服务容器: $service_name" >&2
        return 1
    fi

    for ((i=1; i<=attempts; i++)); do
        local health_status
        health_status="$(docker inspect -f '{{if .State.Health}}{{.State.Health.Status}}{{else}}running{{end}}' "$container_id")"
        if [ "$health_status" = "healthy" ] || [ "$health_status" = "running" ]; then
            return 0
        fi
        echo "等待 ${service_name} 就绪... (${i}/${attempts})"
        sleep "$sleep_seconds"
    done

    return 1
}

generate_sbom() {
    mkdir -p "$ARTIFACTS_DIR"
    docker buildx build \
        --file Dockerfile \
        --target sbom \
        --output "type=local,dest=$ARTIFACTS_DIR" \
        ..
}

sign_image() {
    if ! command -v cosign >/dev/null 2>&1; then
        echo "未安装 cosign，跳过镜像签名"
        return 0
    fi

    if [ -n "${COSIGN_KEY:-}" ]; then
        COSIGN_PASSWORD="${COSIGN_PASSWORD:-}" cosign sign --yes --key "$COSIGN_KEY" "${IMAGE_REPO}:${IMAGE_TAG}"
        return 0
    fi

    if [ "${COSIGN_EXPERIMENTAL:-false}" = "true" ]; then
        cosign sign --yes "${IMAGE_REPO}:${IMAGE_TAG}"
        return 0
    fi

    echo "未提供 COSIGN_KEY 且未启用 keyless 签名，跳过镜像签名"
}

echo "=========================================="
echo " synapse-rust Docker Deployment"
echo "=========================================="

mkdir -p data logs "$ARTIFACTS_DIR"

echo "[1/7] 停止旧容器..."
compose down --remove-orphans --volumes || true

echo "[2/7] 删除旧的 synapse-rust 镜像..."
old_images="$(docker image ls "$IMAGE_REPO" --format '{{.ID}}' | sort -u)"
if [ -n "$old_images" ]; then
    echo "$old_images" | xargs docker image rm -f
else
    echo "未发现旧镜像"
fi
old_tool_images="$(docker image ls "$TOOLS_IMAGE" --format '{{.ID}}' | sort -u)"
if [ -n "$old_tool_images" ]; then
    echo "$old_tool_images" | xargs docker image rm -f
fi

echo "[3/7] 重建 Docker 镜像..."
compose build --pull migrate synapse-rust

echo "[4/7] 生成 SBOM..."
generate_sbom

echo "[5/7] 启动数据库和 Redis..."
compose up -d db redis
wait_for_health db 40 3
wait_for_health redis 30 2

echo "[6/7] 执行数据库迁移与校验..."
compose rm -sf migrate >/dev/null 2>&1 || true
compose up --abort-on-container-exit --exit-code-from migrate migrate

echo "[7/7] 启动应用服务..."
compose up -d synapse-rust
wait_for_health synapse-rust 40 3

sign_image

echo
echo "部署完成"
echo "镜像: ${IMAGE_REPO}:${IMAGE_TAG}"
echo "SBOM: ${ARTIFACTS_DIR}/sbom.spdx.json"
echo "Client API: http://localhost:${SYNAPSE_PORT:-28008}"
echo "Federation: http://localhost:${FEDERATION_PORT:-28448}"
echo
compose ps
