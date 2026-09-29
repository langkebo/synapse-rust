# A5 - Live federation interop testing
# 手动执行指南

## 环境要求

- Docker Engine 20.10+
- Docker Compose 2.0+
- 至少 8GB 可用内存
- 约 5-10 GB 磁盘空间

## 快速开始

### 方案 A：在本地有 Docker 的环境中执行

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust

# 1. 清理并部署双实例
bash scripts/federation-test/cleanup_and_deploy.sh --all

# 2. 等待构建完成（约 5-10 分钟）

# 3. 运行联邦测试
bash scripts/federation-test/test_federation.sh

# 4. 查看结果
cat artifacts/federation-interop/federation_join_result.json
```

### 方案 B：在 Docker Desktop for Mac 中执行

如果当前环境没有 Docker，请在您的本地机器上执行以下步骤：

```bash
# 确保 Docker Desktop 正在运行
open -a Docker

# 检查 Docker 可用性
docker --version
docker compose version

# 执行清理和部署
cd /Users/ljf/Desktop/hu_ts/synapse-rust
bash scripts/federation-test/cleanup_and_deploy.sh --all

# 如果遇到代理问题
HTTPS_PROXY=http://127.0.0.1:7897 bash scripts/federation-test/cleanup_and_deploy.sh --all
```

### 方案 C：在有 Docker 的远程服务器上执行

```bash
# 1. SSH 到服务器
ssh user@remote-server

# 2. 上传配置文件
scp -r ~/Desktop/hu_ts/synapse-rust/docker/federation-test/ remote:/path/to/
scp -r ~/Desktop/hu_ts/synapse-rust/scripts/federation-test/ remote:/path/to/scripts/

# 3. 在服务器上执行
ssh user@remote-server
cd /path/to/synapse-rust
bash scripts/federation-test/cleanup_and_deploy.sh --all
bash scripts/federation-test/test_federation.sh

# 4. 下载结果
scp user@remote-server:/path/to/artifacts/federation-interop/ ~/Downloads/
```

## 分步手动执行

如果不使用自动化脚本，可以手动执行以下步骤：

### Step 1: 停止现有容器

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/federation-test

# 停止 Synapse-A
docker compose -f docker-compose-synapse-a.yml down -v

# 停止 Synapse-B
docker compose -f docker-compose-synapse-b.yml down -v

# 清理残留容器
docker ps -a --filter "name=synapse-federation" --format "{{.Names}}" | xargs -r docker rm -f
```

### Step 2: 清理旧镜像

```bash
# 删除旧镜像
docker rmi synapse-rust:latest --force || true

# 清理未使用的镜像
docker image prune -f --filter "until=24h"
```

### Step 3: 准备数据目录

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker

# 创建数据目录
mkdir -p federation-test/../data/a
mkdir -p federation-test/../data/b

# 设置权限
chmod 755 federation-test/../data/a
chmod 755 federation-test/../data/b
```

### Step 4: 生成密钥和配置

```bash
# 生成 Synapse-A 密钥
FED_KEY_A=$(openssl rand -base64 32)
MACAROON_A=$(openssl rand -hex 32)
SECRET_A=$(openssl rand -hex 64)

# 生成 Synapse-B 密钥
FED_KEY_B=$(openssl rand -base64 32)
MACAROON_B=$(openssl rand -hex 32)
SECRET_B=$(openssl rand -hex 64)

echo "Instance A Federation Key: $FED_KEY_A"
echo "Instance B Federation Key: $FED_KEY_B"
```

### Step 5: 构建镜像

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker

# 构建基础镜像
DOCKER_CARGO_FEATURE_ARGS="--features core-private-chat,widgets,external-services,voice-extended,cas-sso,saml-sso,friends --no-default-features" \
docker compose build --no-cache synapse-rust
```

**注意**: 这一步可能需要 5-10 分钟。

### Step 6: 启动 Synapse-A

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/federation-test

# 创建 .env.a 文件
cat > .env.a << EOF
COMPOSE_PROJECT_NAME=synapse-federation-test-a
SYNAPSE_IMAGE=synapse-rust
SYNAPSE_IMAGE_TAG=federation-a
SERVER_NAME=localhost
PUBLIC_BASEURL=http://localhost:18008
DB_USER=synapse
DB_PASSWORD=synapse_a_pwd
DB_NAME=synapse_a
REDIS_PASSWORD=redis_a_pwd
MACAROON_SECRET=$MACAROON_A
FORM_SECRET=$(openssl rand -hex 16)
REGISTRATION_SECRET=$(openssl rand -hex 64)
ADMIN_SECRET=$(openssl rand -hex 32)
SECRET_KEY=$SECRET_A
FEDERATION_SIGNING_KEY=$FED_KEY_A
FEDERATION_KEY_ID=ed25519:a
FEDERATION_MASTER_KEY=$(openssl rand -hex 32)
WORKER_REPLICATION_SECRET=$(openssl rand -hex 32)
TOKEN_HASH_SECRET=$(openssl rand -hex 32)
RUST_LOG=debug
TZ=UTC
EOF

# 启动容器
docker compose --env-file .env.a -f docker-compose-synapse-a.yml up -d

# 等待健康检查
sleep 30

# 检查状态
curl http://localhost:18008/_matrix/client/versions
```

### Step 7: 启动 Synapse-B

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/federation-test

# 创建 .env.b 文件
cat > .env.b << EOF
COMPOSE_PROJECT_NAME=synapse-federation-test-b
SYNAPSE_IMAGE=synapse-rust
SYNAPSE_IMAGE_TAG=federation-b
SERVER_NAME=localhost
PUBLIC_BASEURL=http://localhost:18009
DB_USER=synapse
DB_PASSWORD=synapse_b_pwd
DB_NAME=synapse_b
REDIS_PASSWORD=redis_b_pwd
MACAROON_SECRET=$MACAROON_B
FORM_SECRET=$(openssl rand -hex 16)
REGISTRATION_SECRET=$(openssl rand -hex 64)
ADMIN_SECRET=$(openssl rand -hex 32)
SECRET_KEY=$SECRET_B
FEDERATION_SIGNING_KEY=$FED_KEY_B
FEDERATION_KEY_ID=ed25519:b
FEDERATION_MASTER_KEY=$(openssl rand -hex 32)
WORKER_REPLICATION_SECRET=$(openssl rand -hex 32)
TOKEN_HASH_SECRET=$(openssl rand -hex 32)
RUST_LOG=debug
TZ=UTC
EOF

# 启动容器
docker compose --env-file .env.b -f docker-compose-synapse-b.yml up -d

# 等待健康检查
sleep 30

# 检查状态
curl http://localhost:18009/_matrix/client/versions
```

### Step 8: 验证联邦连接

```bash
# 检查两个实例的健康状态
echo "=== Synapse-A Health ==="
curl -s http://localhost:18008/_matrix/client/versions | jq .

echo "=== Synapse-B Health ==="
curl -s http://localhost:18009/_matrix/client/versions | jq .

# 查看日志
echo "=== Synapse-A Logs (last 50 lines) ==="
docker logs synapse-federation-a --tail 50

echo "=== Synapse-B Logs (last 50 lines) ==="
docker logs synapse-federation-b --tail 50
```

## 测试联邦功能

使用提供的测试脚本：

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust
bash scripts/federation-test/test_federation.sh
```

或者手动测试：

```bash
# 1. 在 Synapse-A 注册用户
curl -X POST http://localhost:18008/_synapse/admin/v1/register \
  -H "Content-Type: application/json" \
  -d '{"username": "user_a", "password": "pass123", "admin": true}'

# 2. 在 Synapse-B 注册用户  
curl -X POST http://localhost:18009/_synapse/admin/v1/register \
  -H "Content-Type: application/json" \
  -d '{"username": "user_b", "password": "pass456", "admin": true}'

# 3. 在 Synapse-A 创建房间
LOGIN_A=$(curl -s -X POST http://localhost:18008/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{"type": "m.login.password", "identifier": {"type": "m.id.user", "user": "user_a"}, "password": "pass123"}')

TOKEN_A=$(echo $LOGIN_A | jq -r '.access_token')

CREATE_ROOM=$(curl -s -X POST http://localhost:18008/_matrix/client/r0/createRoom \
  -H "Authorization: Bearer $TOKEN_A" \
  -H "Content-Type: application/json" \
  -d '{"visibility": "private", "name": "Federated Room"}')

ROOM_ID=$(echo $CREATE_ROOM | jq -r '.room_id')

echo "Created room: $ROOM_ID"

# 4. 从 Synapse-B 加入房��
LOGIN_B=$(curl -s -X POST http://localhost:18009/_matrix/client/r0/login \
  -H "Content-Type: application/json" \
  -d '{"type": "m.login.password", "identifier": {"type": "m.id.user", "user": "user_b"}, "password": "pass456"}')

TOKEN_B=$(echo $LOGIN_B | jq -r '.access_token')

JOIN_RESULT=$(curl -s -X POST "http://localhost:18009/_matrix/client/r0/join/$ROOM_ID" \
  -H "Authorization: Bearer $TOKEN_B" \
  -H "Content-Type: application/json" \
  -d '{}')

echo "Join result: $JOIN_RESULT"

# 5. 发送消息
curl -X POST "http://localhost:18008/_matrix/client/r0/rooms/$ROOM_ID/send/m.room.message" \
  -H "Authorization: Bearer $TOKEN_A" \
  -H "Content-Type: application/json" \
  -d '{"msgtype": "m.text", "body": "Hello from Synapse-A!"}'

# 6. 在 Synapse-B 获取事件
curl -s "http://localhost:18009/_matrix/client/r0/rooms/$ROOM_ID/messages" \
  -H "Authorization: Bearer $TOKEN_B" | jq '.chunk'
```

## 清理环境

```bash
# 停止并删除所有容器
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/federation-test

docker compose -f docker-compose-synapse-a.yml down -v
docker compose -f docker-compose-synapse-b.yml down -v

# 清理镜像（可选）
docker rmi synapse-rust:latest --force || true
docker image prune -f

# 清理数据目录
rm -rf /Users/ljf/Desktop/hu_ts/synapse-rust/docker/data/a
rm -rf /Users/ljf/Desktop/hu_ts/synapse-rust/docker/data/b
```

## 故障排除

### 问题 1: Docker 构建失败

**症状**: `configure: error: jemalloc version does not match`

**解决**:
```bash
# 绕过 WorkBuddy CLI 的 toybox grep 限制
PATH="/usr/bin:/bin:$PATH" docker compose build
```

### 问题 2: 容器启动后立即退出

**症状**: `docker ps` 看不到容器

**解决**:
```bash
# 查看日志
docker logs synapse-federation-a

# 常见原因:
# - 环境变量缺失
# - 端口冲突
# - 数据库连接失败
```

### 问题 3: 联邦连接失败

**症状**: `Cannot join room from remote server`

**可能原因**:
1. localhost:port 格式不被认可为有效的服务器标识
2. 签名密钥配置错误
3. 防火墙阻止了 18448/18449 端口

**检查**:
```bash
# 检查联邦端口
netstat -tlnp | grep -E "18448|18449"

# 查看联邦日志
docker logs synapse-federation-a | grep -i federation
docker logs synapse-federation-b | grep -i federation
```

### 问题 4: 代理问题

**症状**: `connection timeout` 或 `failed to fetch`

**解决**:
```bash
# 设置代理环境变量
export HTTPS_PROXY=http://127.0.0.1:7897
export HTTP_PROXY=http://127.0.0.1:7897

# 然后重新执行
bash scripts/federation-test/cleanup_and_deploy.sh --all
```

## 性能调优

如果资源有限，可以调整 Docker Compose 的资源限制：

编辑 `docker-compose-synapse-a.yml` 和 `docker-compose-synapse-b.yml`:

```yaml
services:
  synapse-rust:
    cpus: ${SYNAPSE_CPU_LIMIT:-1.0}  # 减少到 1.0
    mem_limit: ${SYNAPSE_MEMORY_LIMIT:-1024m}  # 减少到 1GB
```

## 监控

```bash
# 实时监控日志
docker compose -f docker-compose-synapse-a.yml logs -f
docker compose -f docker-compose-synapse-b.yml logs -f

# 监控资源使用
docker stats synapse-federation-a synapse-federation-b

# 检查 Prometheus 指标
curl http://localhost:18008/metrics | head -20
curl http://localhost:18009/metrics | head -20
```

---
**文档版本**: 1.0
**最后更新**: 2026-09-28
**维护者**: synapse-rust team
