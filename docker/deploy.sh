#!/bin/bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

if [ ! -f .env ]; then
    cp config/.env.example .env
fi

set -a
. ./.env
set +a

IMAGE_REPO="${SYNAPSE_IMAGE:-synapse-rust}"
IMAGE_TAG="${SYNAPSE_IMAGE_TAG:-latest}"

wait_for_health() {
    local service_name="$1"
    local attempts="${2:-40}"
    local sleep_seconds="${3:-3}"
    local container_id
    container_id="$(docker compose ps -q "$service_name")"

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

echo "=========================================="
echo " synapse-rust Docker Deployment"
echo "=========================================="

mkdir -p data logs

echo "[1/6] 停止旧容器..."
docker compose down --remove-orphans
docker compose -p docker down --remove-orphans >/dev/null 2>&1 || true
docker rm -f docker-rust docker-postgres docker-redis >/dev/null 2>&1 || true

echo "[2/6] 删除旧的 synapse-rust 镜像..."
old_images="$(docker image ls "$IMAGE_REPO" --format '{{.ID}}' | sort -u)"
if [ -n "$old_images" ]; then
    echo "$old_images" | xargs docker image rm -f
else
    echo "未发现旧镜像"
fi

echo "[3/6] 重建 Docker 镜像..."
docker compose build synapse-rust

echo "[4/6] 启动数据库和 Redis..."
docker compose up -d db redis
wait_for_health db 40 3
wait_for_health redis 30 2

echo "[5/6] 执行数据库迁移与校验..."
docker compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust migrate
docker compose run --rm --no-deps --entrypoint /app/scripts/db_migrate.sh synapse-rust validate

echo "[6/6] 启动应用服务..."
docker compose up -d synapse-rust
wait_for_health synapse-rust 40 3

echo
echo "部署完成"
echo "镜像: ${IMAGE_REPO}:${IMAGE_TAG}"
echo "Client API: http://localhost:${SYNAPSE_PORT:-28008}"
echo "Federation: http://localhost:${FEDERATION_PORT:-28448}"
echo
docker compose ps
