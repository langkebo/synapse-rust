# synapse-rust Docker 部署指南

> **版本**: v2.0.0
> **更新日期**: 2026-08-12
> **部署方式**: 本地构建镜像 `synapse-rust:local`，一键脚本部署
> **域名**: `matrix.test`（HTTPS + 联邦端口 8448）

---

## 目录

1. [概述](#概述)
2. [前置要求](#前置要求)
3. [一键部署](#一键部署)
4. [目录结构](#目录结构)
5. [配置说明](#配置说明)
6. [SSL/TLS 证书](#ssltls-证书)
7. [VoIP / TURN 集成](#voip--turn-集成)
8. [服务管理](#服务管理)
9. [备份与恢复](#备份与恢复)
10. [故障排查](#故障排查)

---

## 概述

本部署方案使用 Docker Compose 编排以下服务（全部在 `docker/deploy/` 下，单一部署目录，无重复配置）：

| 服务 | 镜像 | 说明 |
|------|------|------|
| synapse | synapse-rust:local | Matrix 主服务器（本地构建） |
| postgres | postgres:16-alpine | PostgreSQL 数据库 |
| redis | redis:7-alpine | Redis 缓存 |
| nginx | nginx:1.27-alpine | 反向代理 + HTTPS (443/8448) |
| migrator | postgres:16-alpine | 一次性数据库迁移/校验服务 |
| coturn | coturn/coturn:latest | 本地 TURN/STUN 服务（独立于本目录，见 [VoIP/TURN](#voip--turn-集成)） |

**HTTPS 访问**：`https://matrix.test`（需要 `/etc/hosts` 包含 `127.0.0.1 matrix.test`，证书由 mkcert 自动生成并受本机信任）。

---

## 前置要求

| 依赖 | 版本要求 | 说明 |
|------|---------|------|
| Docker | 24+ | 含 Docker Compose v2 插件 |
| curl / tar / awk / grep / openssl | 任意 | 部署脚本运行依赖 |
| mkcert | 推荐 | 可选；用于生成受系统信任的 HTTPS 证书，缺失时回退 openssl 自签名 |
| /etc/hosts | - | 需包含 `127.0.0.1 matrix.test` |

**自动安装依赖**：`./deploy.sh --install-deps` 会在 macOS (brew) / Debian (apt) / RHEL (yum) 上自动安装缺失的依赖。

---

## 一键部署

```bash
cd docker/deploy

# 1. 首次部署：完整流程（编译 + 构建镜像 + 迁移 + 启动 + HTTPS 验证）
./deploy.sh --all

# 2. 常规重启（镜像已构建，跳过编译构建）
./deploy.sh --all --skip-build

# 3. 仅启动已有服务（不迁移不构建）
docker compose up -d
```

### 脚本选项

```
./deploy.sh [选项]

  --all              部署所有功能（包含全部扩展，跳过交互选择）
  --core-private-chat 部署核心私密聊天能力（friends + burn-after-read）
  --core-only        仅部署核心 Matrix 功能（不含任何扩展）
  --features LIST    部署指定扩展功能（逗号分隔）
  --skip-build       跳过 cargo build 和 Docker 镜像构建
  --install-deps     自动安装缺失的依赖 (macOS: brew / Linux: apt/yum)
  --no-turn          跳过本地 coturn TURN 服务检查与启动
  --image REF        使用指定的远程镜像（自动 docker pull，跳过本地构建）
  --help             显示帮助信息
```

### 完整流程

```
环境检查 → 依赖安装(可选) → 配置检查 → SSL 证书自动生成 →
/etc/hosts 检查 → 本地 coturn 检查/启动 → 备份 → 缓存清理 →
镜像构建 → 数据库迁移 → 服务启动 → 健康/HTTPS 验证 → 日志检查
```

脚本特性：
- **日志**：每次部署写入 `logs/deploy_YYYYMMDD_HHMMSS.log`，终端同步输出
- **错误处理**：任意步骤失败自动触发回滚（`ROLLBACK_ON_FAILURE=true` 时），恢复旧镜像与数据库备份
- **健康检查**：等待每个容器 healthy 后才进入下一步
- **验证**：HTTP/HTTPS 健康端点、`/_matrix/client/versions`、日志 ERROR 扫描

---

## 目录结构

```text
docker/deploy/
├── docker-compose.yml      # Docker Compose 配置（唯一部署编排）
├── .env                    # 环境变量（实际值，git 忽略）
├── .env.example            # 环境变量模板
├── deploy.sh               # 一键部署脚本（含回滚、日志、验证）
├── README.md               # 本文档
├── config/
│   ├── homeserver.yaml     # Synapse 主配置（与 docker/config/ 同步）
│   ├── rate_limit.yaml     # 限流配置（与 docker/config/ 同步）
│   └── postgres.conf       # PostgreSQL 配置（与 docker/config/ 同步）
├── nginx/
│   ├── nginx.conf          # Nginx 主配置
│   └── conf.d/
│       ├── default.conf    # matrix.test HTTPS 站点
│       └── federation.conf # 联邦 8448 端口
├── ssl/                    # TLS 证书（deploy.sh 自动生成）
│   ├── cert.pem
│   └── key.pem
├── scripts/
│   ├── container-migrate.sh # 容器内迁移入口
│   ├── generate-secrets.sh  # 密钥生成
│   ├── backup.sh / restore.sh
│   └── init-db.sql
├── logs/                   # 部署日志
└── media/                  # 媒体文件持久化
```

> **迁移不再有副本（2026-09-11）**：`docker-compose.yml` 的 migrator 直接绑定挂载
> 仓库根的 canonical 目录 `../../migrations:/migrations:ro`。
> 此处**没有** `migrations/` 子目录 —— 也不要再创建。
>
> 历史上这里有一份手工同步的副本，它静默漂移：多出 42 个废弃 v7 文件、
> 少了 13 个新迁移，导致全新部署的 schema 缺少这些修复。
> `scripts/check_migration_consistency.py`（CI 阻塞步骤）会拦截副本重新出现。
>
> ⚠️ 曾尝试用符号链接替代副本，**不可行**：BSD/macOS `find` 不跟随作为搜索根的符号链接，
> migrator 的基线探测会失败并报 "找不到统一基线脚本"。
> `deploy.sh` 本来就以 `PROJECT_ROOT=$SCRIPT_DIR/../..` 构建镜像，因此仓库根必然存在。

> **配置同步约定**：`config/` 下的文件是运行版本；`docker/config/` 为镜像内建版本。修改 canonical 后需同步（`cp docker/config/<file> config/<file>`）。
>
> ⚠️ **该约定已由 CI 强制检查**（2026-09-11 起）：`scripts/check_config_consistency.py`
> 做**语义**比较（注释差异不报），并接入 `.github/workflows/ci.yml` 的 `repo-sanity` job。
> 有意的开发/生产差异必须登记在脚本的 `ALLOWED_DIFFERENCES` 白名单里并写明理由
> （目前仅 `rate_limit.yaml` 的 `sync.enabled`：开发 `false` / 生产 `true`）。
>
> 为什么需要：这两份配置靠手工 `cp` 同步，历史上**确实漂移过** —— 两侧
> `rate_limit.yaml` 的 `sync.enabled` 不一致，导致 `/sync` 完全失去限流
> （实测 120/120 请求全 200；详见 `docs/audit/S_series_verification_2026-09-11.md` §2）。
> 这与迁移双副本（`2b16dc3c` 已根治：单一真相源 + 阻塞检查）是**同一类**缺陷。

---

## 配置说明

核心变量（完整列表见 `.env.example`）：

| 变量 | 默认值 | 说明 |
|------|--------|------|
| SERVER_NAME | matrix.test | 服务器域名 |
| PUBLIC_BASEURL | https://matrix.test | 对外公开 URL |
| SYNAPSE_IMAGE | synapse-rust:local | 镜像引用 |
| POSTGRES_PASSWORD | (必填) | 数据库密码 |
| REDIS_PASSWORD | (必填) | Redis 密码 |
| SSL_CERT / SSL_KEY | cert.pem / key.pem | ssl/ 目录下证书文件名 |
| TURN_SHARED_SECRET | dev-turn-secret | TURN 共享密钥（须与 coturn 一致） |
| TURN_HOST / TURN_PORT / TURNS_PORT | 127.0.0.1 / 3478 / 5349 | 本地 coturn 地址 |

**密钥生成**：`./scripts/generate-secrets.sh` 可自动补全缺失的随机密钥。

---

## SSL/TLS 证书

`deploy.sh` 自动处理证书，无需手动干预：

1. **检查** `ssl/cert.pem` + `ssl/key.pem` 是否存在且 SAN 包含 `SERVER_NAME`
2. **缺失/不匹配** 时自动生成：
   - 优先 `mkcert`（本地 CA 受系统信任，浏览器无警告）
   - 回退 `openssl` 自签名（需手动信任，`curl -k` 可访问）
3. Nginx 在 443 (客户端) 与 8448 (联邦) 使用同一份证书

手动生成示例：

```bash
cd docker/deploy
mkcert -cert-file ssl/cert.pem -key-file ssl/key.pem matrix.test localhost 127.0.0.1
```

---

## VoIP / TURN 集成

项目复用本地 coturn 服务（源码与配置位于 `/Users/ljf/Desktop/hu_ts/coturn`，独立 Docker 容器运行）。

**部署脚本自动处理**：
1. 检查 coturn 容器/端口 `127.0.0.1:3478` 是否可达
2. 未运行则自动 `cd /Users/ljf/Desktop/hu_ts/coturn && docker compose up -d`
3. 校验 coturn `static-auth-secret` 与 `.env` 的 `TURN_SHARED_SECRET` 一致（不一致时输出 WARNING 并提示修复）

**端口**：3478 (STUN/TURN udp+tcp)、5349 (TURNS/DTLS)、49152-49351 (relay udp)

**手动管理**：

```bash
cd /Users/ljf/Desktop/hu_ts/coturn
docker compose up -d        # 启动
docker compose logs -f coturn  # 日志
docker compose down         # 停止
```

**配置对应关系**：

| 位置 | 配置项 |
|------|--------|
| coturn turnserver.conf | `static-auth-secret=dev-turn-secret` |
| .env | `TURN_SHARED_SECRET=dev-turn-secret` |
| homeserver.yaml `voip:` | `turn_shared_secret: ${TURN_SHARED_SECRET}` + `turn_uris` 指向 `matrix.test:3478/5349` |

> 若 coturn 密钥被修改，必须同步修改 `.env` 中 `TURN_SHARED_SECRET` 并重启 synapse。

---

## 服务管理

```bash
cd docker/deploy

docker compose ps            # 查看状态
docker compose logs -f synapse  # 跟踪应用日志
docker compose restart nginx # 重启某服务
docker compose down          # 停止全部（保留数据卷）
docker compose down -v       # 停止并删除数据卷（危险，数据丢失）
```

健康端点：
- `https://matrix.test/health`
- `http://localhost:8008/health`
- `https://matrix.test/_matrix/client/versions`

---

## 备份与恢复

```bash
cd docker/deploy
./scripts/backup.sh          # 备份数据库与配置（输出备份文件路径）
./scripts/restore.sh <备份文件>  # 恢复
```

部署脚本在升级前会自动执行备份（供失败回滚使用）。

---

## 故障排查

| 现象 | 可能原因 | 处理 |
|------|---------|------|
| `https://matrix.test` 无法访问 | /etc/hosts 缺映射 | `sudo sh -c 'echo "127.0.0.1 matrix.test" >> /etc/hosts'` |
| 浏览器证书警告 | 自签名证书未被信任 | 使用 mkcert 重新生成并 `mkcert -install` |
| `port 3478 already allocated` | 多个 coturn 实例 | `docker ps` 检查重复容器并清理 |
| TURN 无法分配中继 | coturn 密钥不一致 | 校验 `.env` 的 TURN_SHARED_SECRET |
| 迁移卡住 | 旧 migrator 容器残留 | `docker compose run --rm migrator migrate` |
| 镜像构建失败 | 网络/依赖问题 | 查看 `logs/deploy_*.log`，或 `docker build --no-cache` 重试 |
