# Docker 配置审计与优化方案 — synapse-rust

> 审计日期：2026-09-19
> 最后更新：2026-09-19（实施完成批次 1 + 批次 2）
> 审计范围：`docker/Dockerfile`、`docker/complement/Dockerfile`、`docker/docker-compose.yml`、
> `docker/deploy/docker-compose.yml`、两个 `docker-compose.dev-host-access.yml`、`.dockerignore`、
> `docker/entrypoint.sh`、`docker/healthcheck.sh`、`docker/deploy/deploy.sh`、`Makefile` 的 docker 目标、
> `.github/workflows/*` 中的 docker 环节、`docker/deploy/nginx/*`。
> 方式：**只读审查**（批次 1）→ **已实施优化并验证**（批次 2）。
>
> 已完成批次（按提交序号）：
> - **Commit 1**（35a4186f）：P0 组全部 + P1×7 + P2×4
>   - O1-1: runtime-distroless ENTRYPOINT 修复为直接 exec
>   - O1-2: deploy.sh digest pin 覆盖移除
>   - O1-3: .dockerignore 增加 deploy/ssl/ 等排除
>   - O1-4: git rm --cached creds.env + server.crt（token 已过期）
>   - O2-1: Dockerfile 与 complement/Dockerfile target cache mount
>   - O2-2: deploy.sh prune 从 -af 改为有界清理
>   - O2-3: deploy compose synapse 等待 migrator
>   - O2-4: redis healthcheck 改用 REDISCLI_AUTH（两栈均已修复）
>   - O2-5: CI 新增 docker-security-scan.yml（hadolint + trivy）
>   - O2-6: Makefile docker-redeploy 移除不存在的 web.yml
>   - O2-8: dev 栈端口收紧为 127.0.0.1（两栈）+ deploy 端口注释
>   - O3-1: deploy synapse stop_grace_period 30s
>   - O3-4: dev 栈 postgres shm_size 256m
>   - O3-7: 移除 healthcheck.sh 死文件安装
>   - O3-11: .dockerignore 补齐遗漏项
> - **Commit 2**（a6f4f851）：P1×2 + P2×5
>   - O2-7: 两栈依赖镜像（postgres/redis/nginx）pin digest
>   - O2-8（补）: tools 阶段 EXPOSE 去掉 9090
>   - O3-2: nginx 日志路由到 stdout（Docker json-file 轮转）
>   - O3-3: .well-known CORS `*` 添加说明（Matrix 发现协议要求）
>   - O3-5: 所有服务的 memswap_limit
>   - O3-9: complement builder target cache mount
>   - O3-13: dev 栈移除无意义的 ./logs 挂载
>   - O3-14: 数据卷加 backup/data-type 标签
>
> `[实测需复核]` 的条目仍需真实构建验证。

---

## 0. 结论摘要

| 严重度 | 数量 | 一句话概括 |
|---|---|---|
| **P0（阻塞/高危）** | 4 | distroless 目标启动即崩；生产构建绕过 digest pin；TLS 私钥进构建上下文；真实 JWT 已入库 |
| **P1（高）** | 10 | 依赖零编译缓存导致构建极慢；编排缺迁移依赖；健康检查泄露密码；无镜像扫描；CI 无 buildx 缓存；prune 误伤全局 |
| **P2（中）** | 14 | 优雅停机、日志轮转、shm、镜像 tag 浮动、死文件、`.dockerignore` 漏项、源镜像硬编码等 |

**总体判断**：Dockerfile 本身的水准**高于平均**——多阶段构建、digest pin、非 root、HEALTHCHECK、
BuildKit cache mount、`.dockerignore` 精细裁剪都已到位，注释里还有大量踩坑记录，看得出是认真维护过的。
真正的问题集中在三处：

1. **`runtime-distroless` 这个目标是"写了但没跑通过"的死路径** —— 它和一个依赖 bash + `pg_isready` 的
   entrypoint 绑在一起，在 distroless 里必然失败。
2. **Dockerfile 里建立的安全标准，在 `deploy.sh` 和 CI 里被自己绕开了** —— digest pin 被 tag 覆盖、
   `.dockerignore` 漏了 `deploy/ssl/`、CI 没有镜像扫描。
3. **构建性能没有系统性投入** —— 没有 `target/` 的 cache mount，叠加 `CARGO_BUILD_JOBS=2`、
   `deploy.sh --no-cache`、`prune -af`，每次构建都在做全量重编。

---

## 1. P0 — 阻塞 / 高危

### P0-1 `runtime-distroless` 目标不可用（启动即失败）

**位置**：`docker/Dockerfile:135-165`

该阶段的 `ENTRYPOINT` 是 `["/app/entrypoint.sh"]`，而 `docker/entrypoint.sh` 第一行是 `#!/bin/bash`。
`gcr.io/distroless/cc-debian12` **不含任何 shell**（无 `/bin/bash`、无 `/bin/sh`），容器启动会直接报
`exec: /app/entrypoint.sh: no such file or directory`（内核找不到 shebang 指定的解释器）。

即便补上 shell，entrypoint 里还有三处 distroless 满足不了的依赖：

| entrypoint 依赖 | distroless 是否具备 |
|---|---|
| `#!/bin/bash` | ❌ 无 shell |
| `pg_isready`（`wait_for_db`） | ❌ postgresql-client 未装 |
| `timeout`（`run_migrations`） | ❌ coreutils 未装 |
| `tini`（PID 1 信号转发 / 僵尸进程回收） | ❌ tools 阶段有，distroless 没有 |

**现状影响**：该目标从未被任何 compose / CI / deploy.sh 引用（grep 全仓无 `--target runtime-distroless`），
所以是"看起来可用、实际一点就炸"的死路径。风险在于有人照着注释里的
`docker build --target runtime-distroless ...` 用它上生产。

---

### P0-2 生产构建用浮动 tag 覆盖了 digest pin

**位置**：`docker/deploy/deploy.sh:1168-1174`

```bash
# 覆盖基础镜像 digest pin，使用本地已拉取的 tag 版本，避免网络抖动导致 digest 拉取失败
docker build --no-cache \
    --build-arg "RUST_BUILDER_IMAGE=rust:1.93.0-slim-bookworm" \
    --build-arg "DEBIAN_BASE_IMAGE=debian:bookworm-slim" \
```

Dockerfile 顶部精心 pin 了三个 sha256 摘要（含 `RUNTIME_BASE_IMAGE`），但生产部署路径把它们替换成了
**浮动 tag**。这带来两个后果：

- **供应链防护失效**：`rust:1.93.0-slim-bookworm` / `debian:bookworm-slim` 的底层内容可以随上游重新构建而
  变化，构建不再可复现，Dockerfile 里的 pin 形同虚设。
- **标准不一致**：同一个项目里 Dockerfile 要求 pin 摘要、部署脚本要求浮动 tag，两份相反的规则。

注释里给出的理由是"避免网络抖动导致 digest 拉取失败"——这是**真实痛点**，但解法不该是放弃 pin，
见 §3 优化项 O1-2。

---

### P0-3 TLS 私钥在构建上下文内

**位置**：`.dockerignore:58,62` 与 `docker/deploy/ssl/key.pem`

`.dockerignore` 只排除了 `docker/ssl/` 和 `docker/nginx/ssl/`，**漏了 `docker/deploy/ssl/`**。
而构建上下文是仓库根（`docker build ... "$PROJECT_ROOT"`），`COPY . .` 会把这两个文件送进 daemon：

```
docker/deploy/ssl/cert.pem   （公开证书，影响小）
docker/deploy/ssl/key.pem    （-rw------- TLS 私钥）← 问题所在
```

后果链条：

1. 私钥进入构建上下文，被传给 docker daemon（本机或远程 builder）；
2. 留在 builder 阶段的中间层里。最终镜像只 `COPY --from=builder /out/app`，所以**不会进最终产物**，
   但中间层若被导出（`--cache-to type=local` / registry cache）或镜像被 push 含中间层，就会泄露；
3. 用远程 buildx builder 时，私钥会通过网络传到 builder 主机。

> 对照：`.dockerignore` 对 `.env` 的覆盖是好的（`.env*` 无分隔符，匹配任意层级的
> `docker/.env`、`docker/deploy/.env`），说明这是**遗漏**而非认知不足。

---

### P0-4 真实凭据与部署产物已进入 git 历史

**位置**：`git ls-files` 结果

```
scripts/test/fullstack-redo/results/creds.env   ← 4 个真实 JWT（含 admin）、4 个用户 ID
docker/nginx/ssl/server.crt                      ← 部署证书
```

`.gitignore` 第 8、18 行（`docker/nginx/ssl/`、`*.key`）已经写了规则，但这两个文件**在规则生效前就入库了**，
加 `.gitignore` 不会让已跟踪文件消失。`creds.env` 里的 token 虽带 `exp`（1785031174，约 2026-07-26 前后过期），
但：

- JWT 是**测试环境真实签发**的凭据，泄露后可用于对应 homeserver；
- 它们存在于 git 历史中，仅删除当前版本不够，需要 history rewrite 或至少确认该 token 已失效且服务端 secret 已轮换。

---

## 2. P1 — 高优先级

### P1-1 依赖编译没有任何缓存，构建必然全量重编

**位置**：`docker/Dockerfile:61-93`

骨架阶段（61-77 行）的注释写的是"先用空骨架**预编译**所有 crate，后续源码改动不会触发依赖重编"，
但实际执行的只有：

```dockerfile
cargo fetch --locked
```

`cargo fetch` **只下载不编译**。而真正的编译命令（91-93 行）只挂了两个 cache mount：

```dockerfile
--mount=type=cache,target=/usr/local/cargo/registry,sharing=locked
--mount=type=cache,target=/usr/local/cargo/git,sharing=locked
```

**没有 `/workspace/target` 的 cache mount**。叠加 `COPY . .` 一旦变化就 invalidate 后续所有层，
结果是：改一行源码 → 所有依赖 crate 重新编一遍。

再加上三重放大：

- `CARGO_BUILD_JOBS=2`（Dockerfile:13、compose 默认值），并行度极低；
- `deploy.sh` 用 `docker build --no-cache`，连层缓存也丢掉；
- CI `docker-smoke` job 给了 `timeout-minutes: 45`，侧面印证单次构建耗时已经很长。

`[估算]` 这个体量的 Rust workspace（9 个 crate）冷编译 20-40 分钟量级；加 target cache mount 后，
增量构建可降到 3-8 分钟。

---

### P1-2 deploy 栈的 synapse 不等 migrator

**位置**：`docker/deploy/docker-compose.yml:203-207`

```yaml
depends_on:
  postgres: { condition: service_healthy }
  redis:    { condition: service_healthy }
```

缺少 `migrator` 的完成条件。migrator 的 `restart: "on-failure:3"` 注释明确说它兜的是
"直接 `docker compose up` 起整个栈的用法"，但**在这个用法下 app 根本不等待迁移**：
可能以旧 schema 启动，或迁移中途 schema 处于中间态时 app 已经在服务流量。

标准部署走 `deploy.sh` 的 `compose run --rm migrator`（顺序执行）不受影响，所以这是**双路径行为不一致**。

---

### P1-3 健康检查把密码暴露在命令行

**位置**：两个 compose 的 `healthcheck.test`

```yaml
# redis
"redis-cli -a \"${REDIS_PASSWORD:?...}\" ping | grep PONG"
# postgres
"... PGPASSWORD=\"$${POSTGRES_PASSWORD}\" psql ... -tAc 'SELECT 1'"
```

`-a <password>` 与 `PGPASSWORD=...` 都会出现在：

- `docker inspect <container>` 输出的 `Healthcheck.Test` 字段；
- 容器内的进程列表（`ps auxww`）；
- 任何采集 `docker inspect` 的监控系统里。

redis 的官方解法是 `REDISCLI_AUTH` 环境变量（`redis-cli` 会读取），postgres 可用 `PGPASSFILE` 或
把 `PGPASSWORD` 放进 `environment:`（同样在 inspect 里，但至少不在 argv）。最干净的是用 Docker secrets。

---

### P1-4 没有镜像漏洞扫描 / Dockerfile lint

**位置**：`.github/workflows/`（全量 grep `trivy|grype|hadolint|docker scout` → 0 命中）

基础镜像 `debian:bookworm-slim`、`postgres:16-alpine`、`redis:7-alpine`、`nginx:1.27-alpine`
全部未经 CVE 扫描就进入部署。项目对依赖供应链明显敏感（digest pin、CI 有 `drift-detection.yml`），
唯独镜像这一环缺了门禁。

---

### P1-5 CI 没有 buildx，也没有 cache-from/cache-to

**位置**：`.github/workflows/backend-validation.yml`、`scripts/ci_backend_validation.sh:176`

CI 里是裸 `docker_compose build synapse-rust`（`docker/build-push-action`、`setup-buildx-action`
均无命中），每次 docker-smoke 冷启动。Dockerfile 里已经用了 BuildKit cache mount 语法，
但 CI 没有配套导出/导入缓存，cache mount 在 CI 的一次性 runner 上等于没有。

---

### P1-6 `deploy.sh` 清掉整机 Docker 构建缓存

**位置**：`docker/deploy/deploy.sh:991-992`

```bash
docker builder prune -af >/dev/null 2>&1 || true
docker buildx prune -af >/dev/null 2>&1 || true
```

`-a` 是"全部"、`-f` 是"不确认"。这会清掉**主机上所有项目**的 buildx 缓存，不只是本项目的。
在共享构建机上影响面很大，且与 P1-1 的"构建慢"直接冲突——刚清完缓存，下次构建又是全量。

---

### P1-7 `make docker-redeploy` 引用了不存在的 compose 文件

**位置**：`Makefile:291-293`

```make
docker-redeploy: docker-build
	@cd docker && docker compose -f docker-compose.yml -f docker-compose.web.yml \
	    up -d --no-deps --force-recreate synapse-rust
```

`docker/docker-compose.web.yml` **不存在**（`ls` 确认）。该 make 目标 100% 失败。
`docker-build` 本身能跑，所以损坏的是 redeploy 这一步。

---

### P1-8 compose 里的依赖镜像用浮动 tag

**位置**：两个 compose 文件

```yaml
image: postgres:${POSTGRES_VERSION:-16}      # dev
image: postgres:16-alpine                     # deploy
image: redis:7-alpine
image: nginx:1.27-alpine
```

- 大版本 tag（`16`、`7`、`1.27`）会静默升级 minor/patch，部署不可复现；
- 与 Dockerfile 的 digest pin 标准不一致；
- 没有 `digest:` 也没有 `pull_policy`（dev 栈）。

deploy 栈的 `synapse` 服务已经做了 `pull_policy: ${SYNAPSE_PULL_POLICY:-never}`（好实践），
说明团队知道这个问题，只是没推广到基础组件。

---

### P1-9 dev 栈把服务发布到 `0.0.0.0`

**位置**：`docker/docker-compose.yml:38-40`

```yaml
ports:
  - "${SYNAPSE_PORT:-8008}:8008"
  - "${FEDERATION_PORT:-28448}:8448"
```

deploy 栈已经改成 `127.0.0.1:...` 并写了长注释解释原因（"绕过 nginx 就绕过了 TLS"），
但 **dev 栈没有跟进**。在笔记本接入不可信网络时，8008 是明文 HTTP 的 Matrix 客户端 API。

---

### P1-10 metrics 端口无鉴权且在镜像层被 EXPOSE

**位置**：`docker/config/homeserver.yaml:247-251`、`docker/Dockerfile:162,212`

```yaml
prometheus: { enabled: true, port: 9090, path: "/metrics" }
```

deploy compose 的注释已经指出 metrics 路由**无鉴权**（`src/server/mod.rs` 直接挂 handler），
并用 `127.0.0.1:9090:9090` 缓解。但镜像层面 `EXPOSE 8008 8448 9090` 把 9090 也暴露了，
任何人 `docker run -P` 或换个 compose 就会把它放出去。

---

## 3. P2 — 中优先级

| # | 问题 | 位置 | 说明 |
|---|---|---|---|
| P2-1 | 无 `stop_grace_period` | deploy compose `synapse` | 默认 10s；`/sync` 长轮询、DB 事务会被硬杀。建议 30s |
| P2-2 | nginx 日志卷无轮转 | `nginx_logs:/var/log/nginx` | named volume 里的 access/error log 无 logrotate，只靠 json-file 的容器日志限制管不到 |
| P2-3 | CORS `*` | `nginx/conf.d/default.conf:60,66,72,78` | `.well-known` 与部分端点全开。Matrix 生态常见，但建议收敛为允许列表变量 |
| P2-4 | dev 栈 postgres 缺 `shm_size` | `docker/docker-compose.yml:92` | deploy 有 `shm_size: 256m`，dev 没有。docker 默认 64MB，并发排序/并行 worker 易报 "could not resize shared memory" |
| P2-5 | `mem_limit` 未配 `memswap_limit` | 两个 compose | 限制内存但不限 swap，容器可无界吃 swap |
| P2-6 | `container_name` 硬编码 | deploy 全部服务 / dev 由 `COMPOSE_PROJECT_NAME` 派生 | 无法并行起多套栈（CI 并发会撞名）、无法 `scale`、无法蓝绿 |
| P2-7 | `docker/healthcheck.sh` 是死文件 | Dockerfile:98 | 装进 `/app/scripts/healthcheck.sh`，但 Dockerfile 与两个 compose 用的都是 `/app/healthcheck`（Rust 二进制）。且它依赖 curl，在 distroless 里不可用 |
| P2-8 | ARG `RUST_VERSION` 声明未使用 | Dockerfile:1 | 实际用的是 `RUST_BUILDER_IMAGE`。`CACHE_BUST=1` 是常量，不传 `--build-arg` 时不会 invalidate 任何缓存，与注释"强制 invalidate COPY 缓存"不符 |
| P2-9 | complement/Dockerfile 大段复制 | `docker/complement/Dockerfile:19-60+` | builder 阶段与主 Dockerfile ~90% 重复（ENV、apt 列表、骨架 hack），且 apt 没用镜像源、没有 cache mount。双份维护必然漂移 |
| P2-10 | `docker/.env` 是弱口令 + 明文主密钥 | `docker/.env` | `SYNAPSE__FEDERATION__SIGNING_KEY_MASTER_KEY=3283...`、一堆 `dev_*_secret_*`。已被 gitignore，但 `make docker-build` 依赖 grep 该文件取值，文件缺失时 `IMAGE`/`TAG` 为空 → 静默产出错误 tag |
| P2-11 | `.dockerignore` 漏项 | — | 未排除 `.trae/`(708K)、`.trae-html-share-packages/`(1.1M)、`docker/deploy/test-results/`(308K)、`docker/deploy/synapse-data/`、`docker/element/`、`docker/complement/`、`docs/`(7.1M)、`scripts/`(6.7M)。`[估算]` 合计约 14-16MB 进上下文 |
| P2-12 | 第三方源硬编码 + 撤销检查关闭 | Dockerfile:35,110,187；`.cargo/config.toml` | `mirrors.ustc.edu.cn`、`rsproxy.cn` 写死；`CARGO_HTTP_CHECK_REVOKE=false` 降低了 TLS 证书撤销校验强度。海外 CI runner 上会明显变慢甚至失败 |
| P2-13 | `/app/logs` 挂载语义不一致 | dev 挂载、deploy 不挂载 | `homeserver.yaml:98-100` 的 logging 只配了 level/format，**没有 file 路径**（走 stdout），所以 dev 的 `./logs:/app/logs` 是空挂载，deploy 里 `/app/logs` 目录也用不上 |
| P2-14 | 数据卷无备份/标签策略 | `postgres_data` / `redis_data` / `synapse_data` | 裸 `driver: local`，无 label、无备份入口。`docker/deploy/scripts/backup.sh` 存在但未与编排挂钩（无 cron/Job/sidecar） |

---

## 4. 镜像体积估算

`[估算]` 未实际构建，按层组成推算：

| 目标 | 组成 | 估算体积 |
|---|---|---|
| `tools`（默认/生产在用） | `debian:bookworm-slim` (~75MB) + `postgresql-client` (~45MB 含 libpq) + `curl` (~5MB) + `tini` + `libssl3` + synapse-rust 二进制 | **~250-400MB**（二进制占比最大，取决于 feature 组合与 strip） |
| `runtime-distroless` | distroless/cc (~25MB) + 抽取的 libssl/libcrypto/libc + 二进制 | **~40-80MB**（但当前不可用，见 P0-1） |

**关键观察**：deploy 栈里 `RUN_MIGRATIONS: false`、迁移由独立 `migrator` 服务（`postgres:16-alpine` 镜像）做，
也就是说 **生产镜像里的 `postgresql-client` 是纯死重量**，它的存在只为了 `entrypoint.sh` 里那句 `pg_isready`。
修好 P0-1 之后，生产镜像可以直接切 distroless，省掉 ~120MB（debian-slim + psql client）。

---

## 5. 优化方案

### O1 — P0 组：先止血

#### O1-1 让 `runtime-distroless` 真的能跑（或明确废弃）

**目标**：消除"写了但跑不起来"的构建目标，同时为切换小镜像铺路。

**措施**（二选一，推荐 A）：

- **A. 修复**：把 distroless 从"跑 bash 脚本"改成"跑二进制"。
  1. `entrypoint.sh` 的等待/迁移逻辑**已有更好的归属**：deploy 栈有独立 migrator、
     `RUN_MIGRATIONS=false`。distroless 目标应直接
     `ENTRYPOINT ["/app/synapse-rust"]`，不走 entrypoint.sh；
  2. DB 等待交给 K8s initContainer / compose `depends_on: service_healthy`（已有），
     或让 Rust 二进制自带重试（`/health` 已经能反映 DB 状态）；
  3. PID 1：distroless 无 tini，需要静态编译一个 tini 或用 `docker run --init` /
     compose `init: true` 替代。
- **B. 废弃**：若短期不打算用 distroless，删掉 Stage 2/3（`runtime-libs`、`runtime-distroless`），
  避免误导。

**验证**：`docker build --target runtime-distroless -t test:distroless . && docker run --rm test:distroless`
必须能真正起来并过 HEALTHCHECK。**这条必须由一次真实构建来复核 `[实测需复核]`。**

**优先级**：P0

---

#### O1-2 恢复 digest pin，同时解决"digest 拉取失败"的真实痛点

**目标**：生产构建可复现，且不因网络抖动失败。

**措施**：

1. 从 `deploy.sh` 里**删掉**覆盖 `RUST_BUILDER_IMAGE` / `DEBIAN_BASE_IMAGE` 的两行 `--build-arg`，
   让 Dockerfile 的 sha256 默认值生效；
2. 针对注释里的真实痛点（网络抖动），改用正确的解法：
   - 提前 `docker pull <image>@sha256:...` 并保留在本地（digest 已拉取后构建不再联网）；
   - 或搭建内部 registry mirror（Harbor/pull-through cache），把 digest 拉取走内网；
   - `deploy.sh` 里加一步"预拉取并校验 digest"，失败时给出明确报错，而不是静默降级为 tag。

**验证**：`docker build` 后 `docker inspect --format '{{.Config.Labels}}'` + 比对
`docker history` 中基础层 digest 与 Dockerfile 声明一致。

**优先级**：P0

---

#### O1-3 把私钥移出构建上下文

**目标**：TLS 私钥不再进入任何 `docker build` 上下文。

**措施**：

1. `.dockerignore` 增加：
   ```
   docker/deploy/ssl/
   docker/deploy/test-results/
   docker/deploy/synapse-data/
   docker/element/
   docker/complement/
   .trae/
   .trae-html-share-packages/
   docs/
   ```
2. 更彻底：`docker/deploy/ssl/` 改为**运行期挂载**（挂载目录而非仓库内文件），
   仓库只保留 `ssl/.gitkeep` + README 说明如何生成证书；
3. 立即轮换 `docker/deploy/ssl/key.pem`——它已经以明文形式被送进过构建上下文。

**验证**：`docker build --no-cache -f- . <<<'FROM scratch
COPY . /ctx'` 后检查镜像内是否存在 `*.pem`；
或直接 `tar` 出上下文比对。简单门槛：CI 加一条 `.dockerignore` 覆盖检查。

**优先级**：P0

---

#### O1-4 清理 git 历史中的凭据

**目标**：仓库不再携带真实凭据与部署产物。

**措施**：

1. `git rm --cached scripts/test/fullstack-redo/results/creds.env docker/nginx/ssl/server.crt`
   （`.gitignore` 规则已存在，只是对已跟踪文件无效）；
2. 确认 `creds.env` 中 token 对应的服务端 `SECRET_KEY`/`TOKEN_HASH_SECRET` 已轮换或环境已废弃；
3. 若需要彻底清除历史，用 `git filter-repo`（注意：会改写历史，需团队协调 + 强制推送）；
4. CI 加一道 secret 扫描（gitleaks / trufflehog），防止复发。

**验证**：`git log --all --full-history -- '*creds.env'` 返回空（若执行了 filter-repo）；
至少 `git ls-files | grep creds.env` 返回空。

**优先级**：P0

---

### O2 — P1 组：构建性能与编排正确性

#### O2-1 给 `target/` 加 cache mount（**收益最大的一项**）

**目标**：源码改动不再触发依赖全量重编，增量构建从 20-40 分钟降到分钟级。

**措施**（`docker/Dockerfile`）：

```dockerfile
# 1) 骨架阶段：从 cargo fetch 升级为真正的预编译
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/workspace/target,sharing=locked \
    cargo build --release --locked ${CARGO_FEATURE_ARGS} --bin synapse-rust --bin healthcheck || true
#    骨架源码是 stub，编译会失败在最后链接，但依赖已全部编译进 target 缓存

# 2) 正式构建复用同一个 target 缓存
RUN --mount=type=cache,target=/usr/local/cargo/registry,sharing=locked \
    --mount=type=cache,target=/usr/local/cargo/git,sharing=locked \
    --mount=type=cache,target=/workspace/target,sharing=locked \
    cargo build --release --locked ${CARGO_FEATURE_ARGS} --bin synapse-rust --bin healthcheck
```

配套：

- `CARGO_BUILD_JOBS` 默认值从 `2` 提到 `4-8`（或按 `--build-arg` 传 `$(nproc)`）；
- CI 用 `docker/build-push-action` + `cache-from: type=gha` / `cache-to: type=gha,mode=max`
  （或 registry cache），否则 runner 上 cache mount 仍然冷；
- 考虑 `cargo-chef` 或 `cargo build --timings` 定位最重的 crate。

**验证**：改一行 `src/` 后重建，对比 `docker build` 耗时与
`cargo build` 阶段是否出现 "Compiling" 大量依赖。**需实测计时 `[实测需复核]`。**

**优先级**：P1（收益/成本比最高，建议与 O1 同批做）

---

#### O2-2 停止 `prune -af`，改为有界清理

**目标**：不再误伤其他项目的构建缓存。

**措施**（`deploy.sh:991-992`）：

```bash
# 只清理本项目相关，且保留可复用缓存
docker builder prune --filter 'until=168h' -f >/dev/null 2>&1 || true   # 只清 7 天前的
# 或彻底移除这两行，交给宿主机的定时清理策略
```

**验证**：在共享构建机上执行部署脚本，确认其他项目的 buildx 缓存仍在
（`docker buildx du` 对比前后）。

**优先级**：P1

---

#### O2-3 让 deploy 栈的 app 等待 migrator

**目标**：`docker compose up` 路径与 `deploy.sh` 路径行为一致。

**措施**：`docker/deploy/docker-compose.yml` 的 `synapse.depends_on` 增加：

```yaml
depends_on:
  migrator:
    condition: service_completed_successfully   # Compose v2.1+ / Docker Compose 2.x
  postgres: { condition: service_healthy }
  redis:    { condition: service_healthy }
```

注意：`service_completed_successfully` 需要较新的 Compose 版本；若版本不够，
退化为在 `entrypoint.sh` 里加"等待 schema 版本表就绪"的轮询（与 `DB_WAIT_ATTEMPTS` 同风格）。

**验证**：`docker compose up` 全新栈，观察 synapse 启动日志晚于 migrator 退出；
或直接 `docker compose logs --timestamps` 比对时间戳。

**优先级**：P1

---

#### O2-4 健康检查不再把密码放 argv

**目标**：密码不出现在 `docker inspect` 与进程列表。

**措施**：

- redis：`healthcheck.test` 改为 `redis-cli ping | grep PONG`，密码通过
  `environment: REDISCLI_AUTH: ${REDIS_PASSWORD}` 注入（`redis-cli` 原生读取该变量）；
- postgres：改用 `PGPASSFILE`（挂载一个 0600 的 `.pgpass`，或 entrypoint 生成到 tmpfs），
  或退一步把 `PGPASSWORD` 放进 `environment:`（仍在 inspect 的 Env 里，但至少不在 argv）；
- 长期：迁移到 Docker secrets（`secrets:` + `_FILE` 约定）。

**验证**：`docker inspect <c> --format '{{json .Config.Healthcheck}}'` 中不含密码明文；
容器内 `ps auxww` 不含密码。

**优先级**：P1

---

#### O2-5 接入镜像扫描与 Dockerfile lint

**目标**：基础镜像与产物镜像的 CVE 有门禁。

**措施**：新增 workflow（或并入 `ci.yml`）：

```yaml
- uses: hadolint/hadolint-action@v3          # Dockerfile lint
- uses: aquasecurity/trivy-action@master     # 镜像 CVE 扫描
  with:
    image-ref: 'synapse-rust:latest'
    severity: 'HIGH,CRITICAL'
    exit-code: '1'
    ignore-unfixed: true
```

**验证**：故意引入一个已知高危基础镜像，确认 CI 失败。

**优先级**：P1

---

#### O2-6 修 `make docker-redeploy`

**目标**：make 目标可用。

**措施**：`Makefile:292` 删除 `-f docker-compose.web.yml`（该服务已不存在），
或恢复 `docker/docker-compose.web.yml` 文件（若 web 栈确实需要）。

**验证**：`make -n docker-redeploy` 输出的 compose 命令引用的文件都存在。

**优先级**：P1

---

#### O2-7 依赖镜像 pin digest

**目标**：部署可复现，与 Dockerfile 的标准一致。

**措施**：把两个 compose 里的

```yaml
image: postgres:16-alpine            →  image: postgres:16-alpine@sha256:<digest>
image: redis:7-alpine                →  image: redis:7-alpine@sha256:<digest>
image: nginx:1.27-alpine             →  image: nginx:1.27-alpine@sha256:<digest>
```

digest 用 `docker buildx imagetools inspect <img> --format '{{.Manifest.Digest}}'` 获取；
同时加 `pull_policy: always`（或保留 `never` + 预拉取，取决于部署策略）。

**验证**：`docker compose config` 输出的 image 字段含 `@sha256:`。

**优先级**：P1

---

#### O2-8 dev 栈端口收紧 + metrics 不默认暴露

**目标**：dev 与 deploy 的网络暴露标准一致。

**措施**：

- `docker/docker-compose.yml:38-40` 改为
  `"127.0.0.1:${SYNAPSE_PORT:-8008}:8008"`、`"127.0.0.1:${FEDERATION_PORT:-28448}:8448"`；
- Dockerfile 的 `EXPOSE` 去掉 `9090`（改为在需要时由 compose 显式发布），
  或保留 `EXPOSE` 但在 compose 里一律 `127.0.0.1`；
- 中长期：给 `/metrics` 加鉴权（network policy 或 basic auth），
  这是 `src/server/mod.rs` 的改动，不在本次 Docker 范围，但需登记。

**验证**：`docker compose ps` 的 PORTS 列显示 `127.0.0.1:...`；
外部主机 `curl http://<host>:8008` 不通。

**优先级**：P1

---

### O3 — P2 组：稳健性与可维护性

| 编号 | 优化目标 | 改进措施 | 优先级 |
|---|---|---|---|
| O3-1 | 优雅停机 | deploy 栈 `synapse` 加 `stop_grace_period: 30s`（`/sync` 长轮询与 DB 事务需要） | P2 |
| O3-2 | 日志不撑爆磁盘 | nginx 日志卷加 logrotate（sidecar 或改用 stdout + json-file 限额）；或把 `/var/log/nginx` 软链到 stdout | P2 |
| O3-3 | CORS 收敛 | nginx 的 `Access-Control-Allow-Origin "*"` 改为从 `ALLOWED_ORIGINS` 变量生成的 map | P2 |
| O3-4 | dev/prod 行为一致 | dev 栈 postgres 补 `shm_size: 256m`（与 deploy 对齐） | P2 |
| O3-5 | 内存限制完整 | 所有 `mem_limit` 配套 `memswap_limit`（或统一改用 `deploy.resources.limits`） | P2 |
| O3-6 | 支持并行/蓝绿 | 移除 deploy 栈硬编码 `container_name`（改为依赖 compose 默认命名）；若 CI 依赖固定名，用 `COMPOSE_PROJECT_NAME` 隔离 | P2 |
| O3-7 | 去掉死文件 | 删除 `docker/healthcheck.sh`（未被引用）或改为 Dockerfile/compose 统一使用它（需保证镜像内有 curl） | P2 |
| O3-8 | 清理误导性代码 | 删除未使用的 `ARG RUST_VERSION`；修正 `CACHE_BUST` 的注释（或真正传入构建号） | P2 |
| O3-9 | 消除重复 | `complement/Dockerfile` 的 builder 阶段改为 `FROM ... AS builder` 复用主 Dockerfile 的 stage，或用 `COPY --from` 复用产物 | P2 |
| O3-10 | 构建不依赖本地 .env | `make docker-build` 改为 `IMAGE ?= synapse-rust` / `TAG ?= latest` 的 make 变量（可被覆盖），不再 grep `docker/.env` | P2 |
| O3-11 | 缩小构建上下文 | 按 P2-11 补全 `.dockerignore`（约省 14-16MB，更重要的是避免敏感/无关文件入上下文） | P2 |
| O3-12 | 源可切换 + 不降 TLS 强度 | 把 `mirrors.ustc.edu.cn` / `rsproxy.cn` 做成 `ARG`（默认官方源，国内 CI 才覆盖）；移除 `CARGO_HTTP_CHECK_REVOKE=false`，改为仅在明确需要时通过 build-arg 打开 | P2 |
| O3-13 | 日志挂载语义统一 | dev 移除 `./logs:/app/logs`（配置未启用文件日志），或给 `homeserver.yaml` 补文件日志配置并让 deploy 也挂载 | P2 |
| O3-14 | 数据可恢复 | 给三个 named volume 加 label；把 `backup.sh` 挂进编排（compose `profiles` 或宿主 cron + 明确的恢复演练步骤） | P2 |

---

## 6. 建议实施顺序

```
第 1 批（止血，可独立上线）
  O1-3 .dockerignore 补 deploy/ssl/  +  轮换私钥
  O1-4 清理 git 中的 creds.env / server.crt
  O1-2 deploy.sh 移除 digest 覆盖（配套：预拉取 + 内网 mirror）
  O1-1 distroless 修复或废弃

第 2 批（性能与编排，收益最大）
  O2-1 target cache mount + CARGO_BUILD_JOBS 提升 + CI buildx cache   ← 收益最高
  O2-2 停止 prune -af
  O2-3 synapse 依赖 migrator
  O2-4 健康检查去密码
  O2-6 修 make docker-redeploy
  O2-7 依赖镜像 pin digest

第 3 批（门禁与一致性）
  O2-5 trivy + hadolint 接入 CI
  O2-8 端口收紧 + metrics
  O3-* 稳健性项

第 4 批（可选，收益递减）
  切 distroless 生产镜像（~省 120MB）
  complement Dockerfile 复用
  备份自动化
```

---

## 7. 附录：审计覆盖的文件

| 文件 | 发现 |
|---|---|
| `docker/Dockerfile` | P0-1、P1-1、P1-10、P2-7、P2-8、P2-12 |
| `docker/complement/Dockerfile` | P2-9 |
| `docker/docker-compose.yml` | P1-3、P1-8、P1-9、P2-4、P2-13 |
| `docker/deploy/docker-compose.yml` | P1-2、P1-3、P1-8、P1-10、P2-1、P2-2、P2-5、P2-6、P2-14 |
| `docker/docker-compose.dev-host-access.yml` | 无问题 |
| `docker/deploy/docker-compose.dev-host-access.yml` | 无问题 |
| `.dockerignore` | P0-3、P2-11 |
| `docker/entrypoint.sh` | P0-1（bash/pg_isready/timeout 依赖） |
| `docker/healthcheck.sh` | P2-7（死文件） |
| `docker/deploy/deploy.sh` | P0-2、P1-6 |
| `Makefile` | P1-7、P2-10 |
| `.github/workflows/*.yml` | P1-4、P1-5 |
| `docker/deploy/nginx/conf.d/*.conf` | P2-2、P2-3 |
| `docker/config/homeserver.yaml` | P1-10、P2-13 |
| `docker/.env` | P2-10 |
| `.cargo/config.toml` | P2-12 |

**未发现问题**（值得一提的良好实践）：
基础镜像 digest pin ✅ · 多阶段构建 ✅ · 非 root 运行 ✅ · `no-new-privileges` + `cap_drop: ALL` ✅ ·
`read_only` + tmpfs ✅ · HEALTHCHECK 语义正确（走 DB 真实查询，并记录了 unix socket 假健康的踩坑）✅ ·
配置目录挂载而非单文件（记录了 inode 踩坑）✅ · 迁移唯一真相源（`../../migrations`）✅ ·
json-file 日志限额 ✅ · 两个 compose 栈职责边界有明确文档 ✅
