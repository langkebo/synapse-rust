# 离线部署说明

本目录包含已导出的离线镜像包与部署说明，便于在无外网环境快速部署 Synapse Rust。

## 1. 导入镜像

```bash
docker load -i synapse-rust_release.tar
```

## 2. 启动依赖

请确保 PostgreSQL 15 与 Redis 7 可用，并将连接信息配置到服务配置中。

## 3. 服务配置

联邦功能依赖 `federation.signing_key` 配置，该字段为 **base64 编码的 32 字节 seed**。未配置时，`server_key` 相关接口将返回内部错误，联邦不可用。

示例配置（仅展示关键字段）：

```yaml
federation:
  signing_key: "BASE64_32_BYTES_SEED"
```

## 4. 启动服务

```bash
docker run -d \
  --name synapse_rust \
  -p 8008:8008 \
  -e DATABASE_URL="postgres://user:pass@host:5432/synapse" \
  -e REDIS_URL="redis://host:6379" \
  -e SERVER_NAME="example.com" \
  -e HOST="0.0.0.0" \
  -e PORT="8008" \
  -e MEDIA_PATH="/data/media" \
  -e JWT_SECRET="your-jwt-secret" \
  -v /path/to/config.yaml:/app/config.yaml \
  -v /path/to/media:/data/media \
  synapse-rust:release
```

## 5. 联邦可用性检查

确保配置已包含 `federation.signing_key`，并重启服务后再验证联邦端点。
