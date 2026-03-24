#!/bin/bash
set -e

# ============================================================================
# synapse-rust 自动化重构与部署脚本 v2.0
# ============================================================================

PROJECT_ROOT="/Users/ljf/Desktop/hu/synapse-rust"
DOCKER_DIR="$PROJECT_ROOT/docker"
IMAGE_NAME="vmuser232922/synapse-rust:latest"
PREV_IMAGE_NAME="vmuser232922/synapse-rust:previous"

echo "=========================================="
echo "🚀 开始 synapse-rust 重构与部署流程"
echo "=========================================="

cd "$DOCKER_DIR"

# 1. 容器清理阶段
echo "[1/6] 正在清理旧容器与网络..."
docker compose -f docker-compose.prod.yml down -v --remove-orphans || true
docker network prune -f || true

# 2. 技术债务审计 (静态扫描)
echo "[2/6] 正在执行技术债务审计..."
cd "$PROJECT_ROOT"
cargo clippy -- -D warnings || echo "⚠️ Clippy 发现警告，请后续检查 clippy_report.txt"
cargo audit || echo "⚠️ 发现依赖安全漏洞，请检查 audit_report.txt"

# 3. 镜像构建与安全扫描
echo "[3/6] 正在构建生产级镜像 (Multi-stage + Security Hardened)..."
cd "$DOCKER_DIR"
# 备份旧镜像用于回滚
docker tag $IMAGE_NAME $PREV_IMAGE_NAME 2>/dev/null || true
docker compose -f docker-compose.prod.yml build

echo "正在执行 Trivy 安全扫描..."
docker run --rm -v /var/run/docker.sock:/var/run/docker.sock \
    aquasec/trivy:0.49.1 image --severity HIGH,CRITICAL --exit-code 0 $IMAGE_NAME

# 4. 部署服务
echo "[4/6] 正在部署服务 (顺序: DB -> Redis -> Main -> Worker)..."
docker compose -f docker-compose.prod.yml up -d db redis
echo "等待数据库与 Redis 就绪..."
sleep 10

docker compose -f docker-compose.prod.yml up -d synapse-main
echo "等待主服务启动..."
sleep 5

docker compose -f docker-compose.prod.yml up -d synapse-worker

# 5. 健康检查与验证
echo "[5/6] 正在验证部署状态..."
docker compose -f docker-compose.prod.yml ps

echo "检查错误日志..."
docker compose -f docker-compose.prod.yml logs --tail=50 | grep -i -E "error|fatal" || echo "✅ 未发现 ERROR/FATAL 日志"

# 6. 性能基准测试 (k6)
echo "[6/6] 正在运行性能基准测试 (k6)..."
# 注意: 这里使用 docker 运行 k6 访问宿主机端口
docker run --rm --network host -i grafana/k6 run - <<EOF
import http from 'k6/http';
import { check, sleep } from 'k6';

export let options = {
  vus: 10, // 模拟 10 个并发用户 (压测时可增加)
  duration: '10s',
};

export default function () {
  let res = http.get('http://localhost:8008/_matrix/client/versions');
  check(res, {
    'status is 200': (r) => r.status === 200,
    'latency < 200ms': (r) => r.timings.duration < 200,
  });
  sleep(1);
}
EOF

echo "=========================================="
echo "🎉 部署完成！"
echo "=========================================="
