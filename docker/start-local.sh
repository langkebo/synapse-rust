#!/bin/bash

# Matrix Server Startup Script for cjystx.top
# 运行目录: /Users/ljf/Desktop/hu/synapse-rust/docker

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_DIR="$(dirname "$SCRIPT_DIR")/docker"

echo "========================================"
echo "Matrix Server Startup Script"
echo "Server Name: cjystx.top"
echo "========================================"
echo ""

# 检查 Docker 是否运行
if ! docker info > /dev/null 2>&1; then
    echo "Error: Docker is not running. Please start Docker first."
    exit 1
fi

# 检查 hosts 文件配置
echo "1. Checking hosts file configuration..."
if grep -q "cjystx.top" /etc/hosts 2>/dev/null; then
    echo "   ✓ cjystx.top found in /etc/hosts"
else
    echo "   ✗ cjystx.top NOT found in /etc/hosts"
    echo ""
    echo "   Please run the following command to add hosts entries:"
    echo "   sudo bash -c 'echo \"127.0.0.1 cjystx.top\" >> /etc/hosts && echo \"127.0.0.1 matrix.cjystx.top\" >> /etc/hosts'"
    echo ""
    read -p "   Continue anyway? (y/n): " -n 1 -r
    echo ""
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# 检查 SSL 证书
echo ""
echo "2. Checking SSL certificates..."
if [ -f "$DOCKER_DIR/ssl/fullchain.pem" ] && [ -f "$DOCKER_DIR/ssl/privkey.pem" ]; then
    echo "   ✓ SSL certificates found"
else
    echo "   ✗ SSL certificates not found"
    echo "   Generating certificates..."
    cd "$DOCKER_DIR/ssl" && ./generate_certs.sh
fi

# 停止现有容器
echo ""
echo "3. Stopping existing containers..."
cd "$DOCKER_DIR"
docker-compose -f docker-compose.local.yml down 2>/dev/null || true

# 构建并启动服务
echo ""
echo "4. Building and starting services..."
docker-compose -f docker-compose.local.yml up -d --build

# 等待服务启动
echo ""
echo "5. Waiting for services to start..."
sleep 10

# 检查服务状态
echo ""
echo "6. Checking service status..."
docker-compose -f docker-compose.local.yml ps

# 验证服务
echo ""
echo "7. Verifying services..."
echo ""

# 测试后端服务
echo "   Testing backend service (port 8008)..."
if curl -sf http://localhost:8008/health > /dev/null 2>&1; then
    echo "   ✓ Backend service is healthy"
else
    echo "   ✗ Backend service is not responding"
fi

# 测试 Nginx 服务
echo "   Testing Nginx service (port 80/443)..."
if curl -sf http://localhost:80/ > /dev/null 2>&1; then
    echo "   ✓ Nginx service is running"
else
    echo "   ✗ Nginx service is not responding"
fi

# 测试 .well-known 发现
echo "   Testing .well-known discovery..."
if curl -sf http://localhost:80/.well-known/matrix/server 2>/dev/null | grep -q "matrix.cjystx.top"; then
    echo "   ✓ .well-known/matrix/server is configured correctly"
else
    echo "   ✗ .well-known/matrix/server is not configured"
fi

echo ""
echo "========================================"
echo "Startup Complete!"
echo "========================================"
echo ""
echo "Services:"
echo "  - PostgreSQL: localhost:55432"
echo "  - Redis:      localhost:6379"
echo "  - Synapse:    localhost:8008"
echo "  - Nginx HTTP: localhost:80"
echo "  - Nginx HTTPS: localhost:443"
echo "  - Federation: localhost:8448"
echo ""
echo "Test URLs (after configuring /etc/hosts):"
echo "  - Health:      https://matrix.cjystx.top/health"
echo "  - Versions:    https://matrix.cjystx.top/_matrix/client/versions"
echo "  - Well-known:  https://cjystx.top/.well-known/matrix/server"
echo ""
echo "Logs:"
echo "  docker-compose -f docker-compose.local.yml logs -f synapse-rust"
echo ""
