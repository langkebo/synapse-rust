# 环境变量配置指南

本文档说明 Synapse-Rust 项目中所有需要配置的环境变量。

## 核心环境变量

### 必需变量

```bash
# 加密密钥（生成方法：openssl rand -hex 32）
OLM_PICKLE_KEY=<64位十六进制字符串>

# 数据库密码
SYNAPSE_DB_PASSWORD=<强密码>

# JWT 密钥（至少32字符）
SYNAPSE_JWT_SECRET=<至少32字符>

# Macaroon 密钥（至少32字符）
SYNAPSE_MACAROON_SECRET=<至少32字符>

# 表单密钥（至少32字符）
SYNAPSE_FORM_SECRET=<至少32字符>

# 注册密钥（至少32字符）
SYNAPSE_REGISTRATION_SECRET=<至少32字符>

# 安全密钥（至少32字符）
SYNAPSE_SECURITY_SECRET=<至少32字符>
```

### 服务器配置

```bash
# 服务器域名（默认：localhost）
SERVER_NAME=example.com

# 公开访问 URL（默认：http://localhost:28008）
PUBLIC_BASEURL=https://matrix.example.com

# 联邦端口（默认：8448）
FEDERATION_PORT=8448

# Homeserver 基础 URL（用于注册服务）
HOMESERVER_BASE_URL=https://matrix.example.com:8448
```

### 数据库配置

```bash
# 数据库连接 URL
DATABASE_URL=postgresql://user:password@host:5432/database

# 测试数据库 URL（用于运行测试）
TEST_DATABASE_URL=postgresql://synapse:synapse@localhost:5432/synapse_test
```

### Redis 配置

```bash
# Redis 连接 URL
REDIS_URL=redis://localhost:6379

# 测试 Redis URL
TEST_REDIS_URL=redis://localhost:6379
```

### Nginx 配置

```bash
# 域名（用于 nginx 配置模板）
DOMAIN_NAME=example.com

# Synapse 后端地址
SYNAPSE_UPSTREAM=synapse-rust:28008
```

## 测试环境变量

### 联邦测试

```bash
# 数据库密码
FEDERATION_TEST_DB_PASSWORD=test_password

# 共享密钥
FEDERATION_TEST_SHARED_SECRET_1=test_shared_secret_1
FEDERATION_TEST_SHARED_SECRET_2=test_shared_secret_2
```

## 可选变量

```bash
# 管理员密钥
SYNAPSE_ADMIN_SECRET=<密钥>

# 管理员共享密钥
SYNAPSE_ADMIN_SHARED_SECRET=<密钥>

# OIDC 客户端密钥
SYNAPSE_OIDC_CLIENT_SECRET=<密钥>

# TURN 服务器密码
SYNAPSE_TURN_PASSWORD=<密码>
```

## 配置文件示例

### 开发环境 (.env)

```bash
# 从模板复制
cp .env.example .env

# 生成安全密钥
export OLM_PICKLE_KEY=$(openssl rand -hex 32)
export SYNAPSE_JWT_SECRET=$(openssl rand -base64 32)
export SYNAPSE_MACAROON_SECRET=$(openssl rand -base64 32)
export SYNAPSE_FORM_SECRET=$(openssl rand -base64 32)
export SYNAPSE_REGISTRATION_SECRET=$(openssl rand -base64 32)
export SYNAPSE_SECURITY_SECRET=$(openssl rand -base64 32)

# 编辑 .env 文件填入生成的密钥
```

### 生产环境

**重要**: 生产环境不应使用 `.env` 文件，应通过以下方式配置：

1. **Docker Compose secrets**
2. **Kubernetes secrets**
3. **环境变量注入**
4. **密钥管理服务**（如 AWS Secrets Manager、HashiCorp Vault）

### CI/CD 环境

```bash
# GitHub Actions 示例
env:
  TEST_DATABASE_URL: postgresql://postgres:postgres@localhost:5432/test
  TEST_REDIS_URL: redis://localhost:6379
  RUST_LOG: debug
```

## 密钥生成工具

### 生成所有必需密钥

```bash
#!/bin/bash
echo "OLM_PICKLE_KEY=$(openssl rand -hex 32)"
echo "SYNAPSE_DB_PASSWORD=$(openssl rand -base64 24)"
echo "SYNAPSE_JWT_SECRET=$(openssl rand -base64 32)"
echo "SYNAPSE_MACAROON_SECRET=$(openssl rand -base64 32)"
echo "SYNAPSE_FORM_SECRET=$(openssl rand -base64 32)"
echo "SYNAPSE_REGISTRATION_SECRET=$(openssl rand -base64 32)"
echo "SYNAPSE_SECURITY_SECRET=$(openssl rand -base64 32)"
```

## 验证配置

### 检查必需变量

```bash
#!/bin/bash
required_vars=(
  "OLM_PICKLE_KEY"
  "SYNAPSE_DB_PASSWORD"
  "SYNAPSE_JWT_SECRET"
  "SYNAPSE_MACAROON_SECRET"
  "SYNAPSE_FORM_SECRET"
  "SYNAPSE_REGISTRATION_SECRET"
  "SYNAPSE_SECURITY_SECRET"
)

for var in "${required_vars[@]}"; do
  if [ -z "${!var}" ]; then
    echo "错误: $var 未设置"
    exit 1
  fi
done

echo "所有必需变量已设置"
```

## 安全最佳实践

1. **永远不要提交真实密钥到版本控制**
2. **使用强随机密钥**（至少 32 字节熵）
3. **定期轮换密钥**
4. **使用密钥管理服务**存储生产密钥
5. **限制密钥访问权限**
6. **审计密钥使用情况**

## 故障排查

### 密钥相关错误

```
Error: OLM_PICKLE_KEY is required
```
**解决**: 设置 `OLM_PICKLE_KEY` 环境变量

```
Error: Invalid OLM_PICKLE_KEY length
```
**解决**: 确保密钥是 64 位十六进制字符串（32 字节）

### 数据库连接错误

```
Error: Failed to connect to database
```
**解决**: 检查 `DATABASE_URL` 格式和数据库可访问性

### 测试失败

```
Error: TEST_DATABASE_URL not set
```
**解决**: 设置 `TEST_DATABASE_URL` 或使用默认值

## 迁移指南

### 从旧配置迁移

如果你之前使用硬编码配置，请按以下步骤迁移：

1. 备份现有配置
2. 创建 `.env` 文件
3. 设置所有必需环境变量
4. 更新 docker-compose.yml 引用环境变量
5. 测试新配置
6. 删除硬编码配置

---

**更新日期**: 2026-04-28  
**版本**: 1.0
