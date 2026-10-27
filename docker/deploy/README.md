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
9. [数据库迁移（基线重放）语义](#数据库迁移基线重放语义)
10. [备份与恢复](#备份与恢复)
11. [故障排查](#故障排查)

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
  --no-monitoring    跳过监控栈启动（prometheus/alertmanager/grafana/node-exporter/alert-handler）
  --image REF        使用指定的远程镜像（自动 docker pull，跳过本地构建）
  --keep-images      保留历史项目镜像（默认删除所有旧项目镜像以释放空间）
  --no-strict-warnings 未知 WARNING 仅提示、不阻断部署（默认阻断）
  --strict-warnings 未知 WARNING 阻断部署（默认行为）
  --no-rollback      失败时不自动回滚（默认自动回滚）
  --stop-timeout N   容器优雅停止(SIGTERM)等待秒数，默认 30
  --help             显示帮助信息
```

### 完整流程

```
环境依赖检查 → 安装缺失依赖(可选) → 配置文件检查 → 功能选择 → 功能摘要 →
目录准备 → SSL 证书准备 → 应用数据密钥准备 → hosts 检查 → 本地 TURN 检查 →
部署前备份 → 缓存清理 → 项目编译 → 优雅停止后端容器 → 移除旧部署资源 →
清理旧项目镜像 → 构建/拉取镜像 → 启动服务与迁移 → 数据库连接验证 →
数据库版本一致性校验 → 健康检查验证 → HTTPS 接口验证 → 日志告警分析 →
启动监控栈
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
├── docker-compose.yml            # 核心栈编排（postgres/redis/migrator/synapse/nginx）
├── docker-compose.monitoring.yml # 监控栈编排（prometheus/alertmanager/grafana/
│                                 #   node-exporter/alert-handler），需核心栈先起
├── .env                          # 环境变量（实际值，git 忽略）
├── .env.example                  # 环境变量模板
├── deploy.sh                     # 一键部署脚本（含回滚、日志、验证）
├── api-integration_test.sh       # API 集成测试套件
├── register_admin.py             # 注册管理员账户
├── update_admin_password.sh      # 更新管理员密码（user_id 由 SERVER_NAME 推导）
├── run_permission_matrix.sh      # 权限矩阵测试运行器
├── README.md                     # 本文档
├── nginx/
│   ├── nginx.conf                # Nginx 主配置（限流 zone / upstream）
│   └── conf.d/
│       ├── default.conf          # matrix.test HTTPS 站点
│       └── federation.conf       # 联邦 8448 端口
├── prometheus/
│   ├── prometheus.yml            # 抓取配置
│   ├── alerting-rules.yml        # 告警规则
│   ├── recording-rules.yml       # 记录规则
│   └── auth/                     # worker bearer token（git 忽略，deploy.sh 生成）
├── alertmanager/
│   └── alertmanager.yml          # 告警路由（投递到 alert-handler:8080）
├── grafana/
│   ├── grafana.ini
│   ├── dashboards/               # 预置面板 JSON
│   └── provisioning/             # 数据源与面板自动装载
├── alert-handler/                # 告警 webhook 接收器（Flask + gunicorn）
│   ├── Dockerfile
│   └── app.py
├── ssl/                          # TLS 证书（deploy.sh 自动生成）
│   ├── cert.pem
│   └── key.pem
├── synapse-data/                 # 应用数据 bind mount（首把密钥须宿主侧供给，见其 README）
│   └── README.md
├── scripts/
│   ├── container-migrate.sh      # 容器内迁移入口
│   ├── generate-secrets.sh       # 密钥生成
│   ├── backup.sh / restore.sh
│   └── init-db.sql
├── logs/                         # 部署日志
├── media/                        # 媒体文件持久化
└── backups/                      # 备份输出（deploy.sh 回滚用）
```

> **配置不再有副本（2026-09-15）**：`docker-compose.yml` 以 `../config` 直接挂载仓库根的
> canonical 目录 `docker/config/`（`postgres` 服务单独挂 `../config/postgres.conf`）。
> 此处**没有** `config/` 子目录 —— 也不要再创建（`deploy.sh` 会校验该路径，
> 并且不再在 deploy 目录下 `mkdir config`）。
>
> 历史上这里有一份手工 `cp` 同步的副本，它静默漂移：两侧 `rate_limit.yaml` 的
> `sync.enabled` 不一致（开发 `false` / 生产 `true`），导致 `/sync` **完全失去限流**
> （实测 120/120 请求全 200；详见 `docs/audit/S_series_verification_2026-09-11.md` §2）。
> 与迁移双副本（`2b16dc3c` 已根治）是同一类缺陷，现在用同样的方式根治：
> 单一真相源 + 按路径挂载。原先用于守护这份副本的
> `scripts/check_config_consistency.py` 及其 CI 步骤已随之删除 ——
> 副本本身不存在了，一致性检查不再有意义；防副本重现的守卫移到了
> `tests/unit/config_mount_tests.rs`。

> **迁移不再有副本（2026-09-11）**：`docker-compose.yml` 的 migrator 直接绑定挂载
> 仓库根的 canonical 目录 `../../migrations:/migrations:ro`。
> 此处**没有** `migrations/` 子目录 —— 也不要再创建。
>
> 历史上这里有一份手工同步的副本，它静默漂移：详见 `migrations/README.md`
> （副本独有 131 = 非 `archive/` 82［其中正向 42］+ `archive/` 49），
> 少了 13 个新迁移，导致全新部署的 schema 缺少这些修复。
> `scripts/check_migration_consistency.py`（CI 阻塞步骤）会拦截副本重新出现。
>
> ⚠️ 曾尝试用符号链接替代副本，**不可行**：BSD/macOS `find` 不跟随作为搜索根的符号链接，
> migrator 的基线探测会失败并报 "找不到统一基线脚本"。
> `deploy.sh` 本来就以 `PROJECT_ROOT=$SCRIPT_DIR/../..` 构建镜像，因此仓库根必然存在。

---

## 配置说明

核心变量（完整列表见 `.env.example`）：

| 变量 | 默认值 | 说明 |
|------|--------|------|
| SERVER_NAME | matrix.test | 服务器域名 |
| PUBLIC_BASEURL | https://matrix.test | 对外公开 URL |
| SYNAPSE_IMAGE | synapse-rust:local | 镜像引用 |
| POSTGRES_PASSWORD | (必填) | 数据库密码。**弱口令会被拒绝部署**（见下"数据库口令的三条约定"） |
| REDIS_PASSWORD | (必填) | Redis 密码 |
| SSL_CERT / SSL_KEY | cert.pem / key.pem | ssl/ 目录下证书文件名 |
| TURN_SHARED_SECRET | (必填，须替换占位符) | TURN 共享密钥（须与 coturn 一致） |
| TURN_HOST / TURN_PORT / TURNS_PORT | 127.0.0.1 / 3478 / 5349 | 本地 coturn 地址 |

**密钥生成**：`./scripts/generate-secrets.sh` 可自动补全缺失的随机密钥。

**数据库口令的三条约定**（2026-10-15 起）：

1. **部署时自动生成强口令**：`generate-secrets.sh missing` 会把"弱值"也当成"缺失"来补 ——
   判据是「命中弱口令名单（`synapse` / `postgres` / `admin` …）或长度 < 16」。
   此前它只认 `.env.example` 的 `CHANGE_ME`，于是 `POSTGRES_PASSWORD=synapse` 会通过
   所有检查一路留到生产（实测事故形态）。
2. **弱口令会挡住部署**：`deploy.sh` 在环境检查阶段调
   `./scripts/generate-secrets.sh check-password postgres`，弱值即 `exit 1`。
   确需保留自定义口令时置 `ALLOW_WEAK_DB_PASSWORD=1`（会打警告，不静默放行）。
3. **口令会收敛到已有数据卷**：`POSTGRES_PASSWORD` 只在**首次 initdb** 时被 entrypoint
   用来创建角色；数据卷已存在时，改 `.env` **不会**改变库里的口令。所以 `deploy.sh`
   在 postgres 就绪后、迁移之前执行 `converge_db_password`：先以 `.env` 的口令走
   **非 loopback** 认证探测（容器内 `127.0.0.1` 在 `pg_hba` 里是 `trust`，用它探等于没探），
   不一致就 `ALTER ROLE` 收敛，然后复验。`verify_database` 同样改用认证路径 ——
   原实现走 `trust`，"口令错了也报连接正常"。

本机脚本要连这个库时**别硬编码口令**，用单一来源 `scripts/lib/db_credentials.sh`：

```bash
bash scripts/lib/db_credentials.sh url synapse         # 完整 URL
eval "$(bash scripts/lib/db_credentials.sh exports)"   # 导出 DB_USER/DB_PASSWORD/... 
bash scripts/lib/db_credentials.sh source              # 打印口令来自哪个文件（排障用，不泄露口令）
```

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

项目复用本地 coturn 服务（独立 Docker 容器运行）。其源码/配置目录由
`COTURN_DIR` 指定，默认值见 `deploy.sh` 中的 `COTURN_DIR="${COTURN_DIR:-...}"`；
若你的 coturn 不在默认位置，部署前导出该变量（或 `--no-turn` 跳过）。

**部署脚本自动处理**：
1. 检查 coturn 容器/端口 `127.0.0.1:3478` 是否可达
2. 未运行则自动 `cd "$COTURN_DIR" && docker compose up -d`
3. 校验 coturn `static-auth-secret` 与 `.env` 的 `TURN_SHARED_SECRET`：未设置/仍为占位符时输出 ERROR（拒绝弱默认密钥），不一致时输出 WARNING 并提示修复

**端口**：3478 (STUN/TURN udp+tcp)、5349 (TURNS/DTLS)、49152-49351 (relay udp)

**手动管理**：

```bash
cd "$COTURN_DIR"            # 默认值见 deploy.sh；请按实际路径导出该变量
docker compose up -d        # 启动
docker compose logs -f coturn  # 日志
docker compose down         # 停止
```

**配置对应关系**：

| 位置 | 配置项 |
|------|--------|
| coturn turnserver.conf | `static-auth-secret=<强随机值>` |
| .env | `TURN_SHARED_SECRET=<与 coturn 相同的强随机值>` |
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

## 数据库迁移（基线重放）语义

`migrator` 只跑一份统一基线 `migrations/00000000_unified_schema_v12.sql`（**没有**时间戳增量文件），
所以"改 schema"＝"改基线"。运行时只有两条规则（`docker/db_migrate.sh`）：

| 记账 checksum vs 文件 checksum | 行为 |
|---|---|
| **相同** | `基线迁移已记录且内容未变` ⇒ **直接跳过**，什么都没执行 |
| **不同** | `基线内容已变化，重放基线以应用变更` ⇒ **整份基线重放**（容错模式；仍有语句失败则报错退出） |

⇒ "这次发布会不会碰数据库"取决于**基线文件内容**，与镜像 tag 无关：

```bash
git diff <上次发布的提交> -- migrations/00000000_unified_schema_v12.sql
```

### 重放代价：整份基线重跑，但绝大部分语句是幂等空操作

重放会再执行一遍全部 `CREATE TABLE IF NOT EXISTS` / `CREATE INDEX CONCURRENTLY IF NOT EXISTS`
等，正常情况下这些"已存在、跳过"。历史上**唯一随数据量线性增长**的是三张核心表上的
`CHECK` 约束重建 —— 已于 2026-10-15 修掉（方案 §13.9 / §13.11）：

- **B4.4**：`ADD CONSTRAINT … CHECK (…)` → `ADD … NOT VALID` + 独立 `VALIDATE CONSTRAINT`。
  `ADD`（两种写法都取 `AccessExclusiveLock`，差别只在**是否扫表**）不再扫表 ⇒
  **排他锁窗口从"全表扫描时长"降到近 0**；扫描改由 `VALIDATE` 在 `ShareUpdateExclusiveLock`
  下做（**不阻塞读写**）。实测（真实 `events` 约束、5,000,000 行）：AE 持有 **11,776 ms → 6 ms**、
  并发写 30 笔从 **全阻塞** 变成 **0 阻塞**。
- **B4.5**：这三条约束改成**只在定义变化时**才重建 ⇒ 定义没变的重放**不再重跑全表校验**。

### 发布前必查：本次基线变更是否触及热表约束

```bash
# 1) 是否动了三条热表 CHECK 的表达式
git diff <上次发布的提交> -- migrations/00000000_unified_schema_v12.sql \
  | grep -E '^[+-].*(ck_events_event_id_format|ck_rooms_room_id_format|ck_room_memberships_valid)'

# 2) 是否有新增的会扫表的语句（新增 CHECK 的 NOT VALID / VALIDATE 是否配对齐全）
grep -nE 'NOT VALID|VALIDATE CONSTRAINT' migrations/00000000_unified_schema_v12.sql
```

| 情况 | 影响 |
|---|---|
| 第 1 步**无输出** | 这三条不会被重建。**前提是它们都已 `convalidated=true`** —— 那时 `VALIDATE` 是空操作（实测：已 valid 的约束再 `VALIDATE` 不扫表）⇒ 重放**不会**在这三张表上做全表校验。⚠️ 若某条卡在 `NOT VALID`（上一次 `VALIDATE` 失败过），"P1-2b" 段会照常补校验 ⇒ **那时会扫表**；发布后核验那一步会把这个状态抓出来 |
| 第 1 步**有输出**（表达式真变了） | 该表跑一次 `VALIDATE`：**不阻塞读写**，但 CPU 密集。实测速率约 **2–4.5 µs/行**（与数据形态相关）⇒ 5M 行 ≈ 10–23 s、20M 行 ≈ 81 s |
| 新增了**其它**表上的 `ADD CONSTRAINT … CHECK (…)`（无 `NOT VALID`） | ⚠️ 那条**仍会**在 `ADD` 期间持有排他锁并扫表；只有上面三条热表约束装了"定义变化才重建"的守卫 |

⚠️ **`VALIDATE` 与 autovacuum 互相阻塞**：两者都取 `ShareUpdateExclusiveLock`。
**实测（一个会话持该锁时的行为）**：`VACUUM (ANALYZE)`（autovacuum 取同一把锁）报
`canceling statement due to lock timeout`，而无争用时同一命令成功；同一个窗口里 `SELECT` 与
`INSERT` **都成功**、`ALTER TABLE … ADD COLUMN` 被挡 ⇒ 一句话：**不挡读写，挡后台清理与 DDL**。
带表达式变更的发布建议放低峰，并在发布前确认没有正在跑的 autovacuum：

```bash
docker exec -it synapse-postgres psql -U synapse -d synapse -c \
  "SELECT relname, last_autovacuum FROM pg_stat_user_tables
    WHERE relname IN ('events','rooms','room_memberships');"

# 发布期间观察
docker exec -it synapse-postgres psql -U synapse -d synapse -c \
  "SELECT pid, state, wait_event_type, query FROM pg_stat_activity WHERE query ILIKE '%VALIDATE%';"
```

### 发布后核验

```bash
# 三条约束必须都是"已验证"；残留 NOT VALID ⇒ 说明 VALIDATE 没跑成功
docker exec -it synapse-postgres psql -U synapse -d synapse -c \
  "SELECT conname, convalidated FROM pg_constraint
    WHERE conname IN ('ck_rooms_room_id_format','ck_events_event_id_format','ck_room_memberships_valid');"

# 只读巡检：孤儿对象 / 未应用对象 / 未验证的 CHECK / VALIDATE 死引用
python3 scripts/report_orphan_db_objects.py --dsn-env DATABASE_URL
```

`VALIDATE` 失败会**如实报错**（`check constraint "…" of relation "…" is violated by some row`）——
那说明库里存在违反格式约束的历史数据，应作为**真实数据缺陷**处理，不要"改回 `NOT VALID` 了事"
（那只会把约束变成摆设，并被上面的巡检抓出来）。详见 `docs/runbook/DB_ORPHAN_OBJECT_AUDIT.md`。

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

## 连接预算（强制约束，由 `scripts/ci/check_connection_budget.py` 校验）

PostgreSQL 的 `max_connections` 是**全局**资源，而每个 synapse 进程都会开一个连接池
（上限 = 配置项 `database.max_size`，源码默认值见 `synapse-common/src/config/database.rs`
的 `default_database_max_size()`）。两侧都不设护栏时，多开一个进程就会让「连接耗尽」
变成运行期随机失败（测试 starvation、部署期 5xx）。

| 项 | 值 |
|---|---|
| 每进程池上限 | 50 |
| 最大并发进程数 | 4 |
| 测试/管理保留 | 20 |
| PostgreSQL max_connections | 250 |
| PostgreSQL pids_limit | 300 |
| 预期并发排序/哈希节点数（work_mem 预算） | 100 |

不变式：`每进程池上限 × 进程数 + 保留 ≤ max_connections` → `50 × 4 + 20 = 220 ≤ 250` ✅

**另两条同源不变式**（2026-10-15 起由同一门禁断言，见
`docs/audit/DB_REVIEW_2026-10-15.md` §4/§5）：

- **pids**：`pids_limit ≥ max_connections + 常驻进程数(7) + max_worker_processes(8)`
  → `300 ≥ 250 + 7 + 8 = 265` ✅
  （常驻进程实测：postmaster / logger / checkpointer / bgwriter / walwriter /
  autovacuum launcher / logical replication launcher = **7** 个；并行 worker 也占 pid。
  256 时 250+7=257 > 256 ⇒ 连接打满后段报 `could not fork new process`，客户端看到的是"连接失败"。）
- **内存**：`shared_buffers + work_mem × 预期并发 + maintenance_work_mem ≤ 0.8 × mem_limit`
  → `256MB + 4MB × 100 + 64MB = 720MB ≤ 0.8 × 1536MB = 1229MB` ✅
  （`work_mem` 是**每排序/哈希节点**上限，不是每连接上限；按 `max_connections` 算最坏情况会得出
  4GB 这种必然打穿的数，故这里用"预期并发"这一显式登记的预算量，而不是假装 250 个连接同时排序。）

**部署前必查**：PostgreSQL 默认 `max_connections = 100`，那种情况下最多只能跑
**1 个 worker**（`50 × 1 + 20 = 70`）；要跑 4 个 worker 必须显式把
`max_connections` 提到 **≥220（推荐 250）**。改了任何一项（池上限 / 进程数 /
max_connections）都必须同步更新本表，否则门禁会失败。

## 会话超时（实例级兜底，由 `scripts/ci/check_connection_budget.py` 校验）

`docker/config/postgres.conf` 显式设置了两项实例级超时（PERF-10）：

| 项 | 实例级值 | 应用侧默认 |
|---|---|---|
| `statement_timeout` | `5min` | 30s（每个连接由 `after_connect` 设置） |
| `idle_in_transaction_session_timeout` | `60s` | 60s（同上） |

**应用侧已兜底，这里防的是直连**：`src/server/database.rs` 在 `after_connect` 里对每个
连接执行 `set_config(...)`（值来自 `synapse-common/src/config/database.rs`，默认
30s / 10s / 60s），迁移器还会在每条语句前显式 `SET statement_timeout = '30s'`。
所以实例级取值**不服务于应用**，只用于挡住直连 psql 的运维查询无限挂起、把连接与锁一直占住。

两条约束（门禁断言）：

1. 两个键必须**显式存在** —— 删掉就回到"直连无上限"，等于没兜底（门禁 `exit 2`）；
2. 实例级**不得比应用默认更严**（`statement_timeout ≥ 30s`、`idle ≥ 60s`）。写成 10s 不会
   影响应用（会话级 `set_config` 会覆盖），只会提前掐断运维手工执行的
   `CREATE INDEX` / `VACUUM`。

长时间维护（重建索引、大批量 `VACUUM`）请在**同一会话**内自行放开：
`SET statement_timeout = 0;`。

## 媒体下载的匿名读语义（legacy r0/v1，SEC-12）

`/_matrix/media/r0/download/*` 与 `/_matrix/media/v1/download/*`（含 `/{filename}` 变体）
**不要求 access token**。这是 **Matrix legacy 语义**（媒体仓库在 v1.11 之前允许匿名读），
不是本仓漏挂鉴权 —— 老客户端依赖它，收紧会让它们所有历史图片加载失败。

当前各路径的实际口径（由 `tests/unit/doc_credibility_guard_tests.rs` 与代码双向校验）：

| 路径 | 鉴权 | 实现 |
|---|---|---|
| `/_matrix/media/r0/download/{server}/{media_id}[/{filename}]` | 匿名（token 可选） | `download_media_v1`（`OptionalAuthenticatedUser`） |
| `/_matrix/media/v1/download/{server}/{media_id}[/{filename}]` | 匿名（token 可选） | 同上 |
| `/_matrix/media/v3/download/...`、`/_matrix/media/v3/thumbnail/...` | 匿名（token 可选） | `download_media` / `get_thumbnail` |
| `/_matrix/media/v3/download_signed/...` | HMAC 签名（`?signature=`） | `download_media_signed` |
| `/_matrix/media/r1/download/...` | **必须** access token | 路由层挂 `auth_middleware`（VULN-01/VULN-02） |
| `/_matrix/client/v1/media/download/...`、`.../thumbnail/...` | **必须** access token | `download_media_authenticated` |

**如需关闭 r0/v1 的匿名读**：这不是配置项，要改代码 —— 给 r0/v1 的 legacy 下载 router
挂 `auth_middleware`（与 r1 完全相同的做法，`synapse-web/src/routes/media/mod.rs` 里
`create_media_r1_router` 即范例）。动手前先按**客户端版本分布**决定：客户端通常以
`/_matrix/client/versions` 是否声明 `v1.11` 作为切换依据，未升级的客户端仍会走 r0/v1。
在此之前保持现状；改了代码就必须同步改本节（守卫会因此变红）。
