# 部署收尾一次性动作清单（2026-09-18）

适用对象：**已经在运行的 `docker/deploy/` 生产栈 VM**。
代码基准：`8efe7b77`（`cleanup/remaining-2026-09-18`）。本文所有 `file:line` 都以该提交为准。

本清单替代散落在聊天记录里的口头步骤。每个动作都给出：**必做/可选**、**为什么**、
**可直接粘贴的命令**、**期望输出**。若某条命令的输出与"期望"不符，按该步骤末尾的处置执行，
不要凭记忆改 SQL。

## 0. 动作总览

| # | 动作 | 级别 | 触发条件 |
|---|------|------|----------|
| A | 设置真实 `FEDERATION_MASTER_KEY` | **必做** | 始终 |
| B | 停 app + 删除旧 `federation_signing_keys` 行 | **必做**（A 改了主密钥时） | A 把空/占位值换成真实值时 |
| C | 用 migrator 应用基线、validate、启动 app | **必做** | 始终 |
| D | 验证两处 schema 修复真的落库 | **必做** | 始终（C 的 `validate` 证明不了） |
| E | 验证 8008/9090 只绑回环 | **必做** | 始终 |
| F | 删除 `.env` 里残留的 `JWT_SECRET=` | 可选（保洁） | 该行存在时 |
| G | 代理信任（`trust_forwarded` + `trusted_proxies`） | 可选 / **需裁定** | 需要按真实客户端 IP 限流与登录锁定时 |
| H | 跨机抓取 Prometheus 指标 | 可选 | 确有跨机抓取需求时 |

前置检查（任一条不满足就先解决，不要往下走）：

```bash
cd <部署仓库根目录>
git log --oneline -1          # 期望包含 8efe7b77（或其后继）
cd docker/deploy
docker compose config --quiet && echo "compose config OK"
```

`docker compose config` 在 `FEDERATION_MASTER_KEY` 缺失/为空时会直接报错退出
（`docker/deploy/docker-compose.yml:209` 的 `:?` 守卫）——这正是步骤 A 要修的状态。

---

## A.（必做）设置真实 `FEDERATION_MASTER_KEY`

**为什么**：compose 用 `${FEDERATION_MASTER_KEY:?...}` 强制非空
（`docker/deploy/docker-compose.yml:209`，引入于 `7a419aac`）。空值不会 fail-closed：
HKDF 接受空输入仍能派生出可用的 AES 密钥，于是联邦签名私钥以 `enc:` 前缀入库、
看起来加密实则拿到库导出即可解开，同时"未配置主密钥 → 拒绝持久化"的兜底分支永远走不到
（HKDF/`enc:` 见 `synapse-common/src/key_encryption.rs:10-33`；空白值归一化为 `None` 的生产
实现在 `synapse-common/src/config/loader.rs:95-107`，其输入来自 `docker/config/homeserver.yaml:109`
的 `signing_key_master_key: "${FEDERATION_MASTER_KEY:-}"`；长度下限在
`synapse-common/src/config/validation.rs:43-57`）。当前代码已经把空白值归一化为 `None`
并做长度校验，但**已经落库的旧 `enc:` 行不会因此变安全**——那要靠步骤 B 删掉。

**1) 看现状**（真实值应是 64 位十六进制；`length=0` 说明是空值）：

```bash
awk -F= '/^FEDERATION_MASTER_KEY=/{print "length=" length($2)}' .env
```

**2) 补齐/设置**（`generate-secrets.sh missing` 只补缺失或占位符，不覆盖已存在的真实值；
对应 `docker/deploy/scripts/generate-secrets.sh:74`，`deploy.sh` 也会自动调用它，
见 `docker/deploy/deploy.sh:827-828`）：

```bash
./scripts/generate-secrets.sh missing
grep '^FEDERATION_MASTER_KEY=' .env
```

或者手工写入等价的一行（32 字节十六进制 = 64 字符；**先删掉旧的空行**，
否则 `.env` 里会同时留两行）：

```bash
sed -i.bak '/^FEDERATION_MASTER_KEY=/d' .env
printf 'FEDERATION_MASTER_KEY=%s\n' "$(openssl rand -hex 32)" >> .env
```

**期望**：`grep` 打印 `FEDERATION_MASTER_KEY=<64 位十六进制>`，`length=64`。
随后 `docker compose config --quiet` 不再报错。缺值或空值时它的报错是（实测）：

```text
error while interpolating services.synapse.environment.FEDERATION_MASTER_KEY: required variable FEDERATION_MASTER_KEY is missing a value: FEDERATION_MASTER_KEY must be set (openssl rand -hex 32, or scripts/generate-secrets.sh missing)
```

---

## B.（必做，A 改了主密钥时）删除旧的联邦签名密钥行

**为什么**：签名私钥是用主密钥加密后存进 `federation_signing_keys.secret_key` 的。
换了主密钥后，旧行解不开：`load_or_create_key` 在解密失败时**直接返回错误、不会重新生成**
（`synapse-federation/src/key_rotation.rs:490-499`），启动路径只把它记成一条 error 日志
（`synapse-federation/src/key_rotation.rs:430-433`，由 `src/server/mod.rs:364` 触发），
结果是服务起来了但联邦签名一直不可用。删掉旧行后，下一次启动走
`Ok(None) → initialize()` 重新生成并用新主密钥加密（`key_rotation.rs:514-529`）、
经 `resolve_stored_secret_key` 加密入库（`key_rotation.rs:216-234`）。

未发布项目无兼容义务：旧公钥被联邦对端缓存这件事不需要兼顾，直接删干净。

**顺序很重要：先停 app，再删行；app 要等到步骤 C 把基线迁移完成后才重启。**

```bash
cd docker/deploy

# 1) 停掉 app（数据库保留）
docker compose stop synapse

# 2) 先列出库里实际存在的 server_name —— 不要拿 .env.example 的默认值猜
docker compose exec -T postgres sh -c 'psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -tA' <<'SQL'
SELECT DISTINCT server_name FROM federation_signing_keys ORDER BY 1;
SQL
```

按第 2 步的输出分流（**这一步是占位符的自证，不能跳过**）：

- 输出为空 → 库里确实没有任何签名密钥行，无行可删，跳过下面的 `DELETE`，直接做步骤 C。
- 输出里有你的 `SERVER_NAME`（`grep '^SERVER_NAME=' .env`）→ 用**完全一致**的字符串替换下面
  `'<SERVER_NAME>'` 后执行。
- 输出非空但**没有**你的 `SERVER_NAME` → **停下**：要么 `SERVER_NAME` 认错了，要么这个库
  属于另一套部署；在查清之前不要执行 `DELETE`。

```bash
cd docker/deploy
# 3) 删除本服务器的全部签名密钥行；把 '<SERVER_NAME>' 换成第 2 步列出的确切值
docker compose exec -T postgres sh -c 'psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -v ON_ERROR_STOP=1' <<'SQL'
DELETE FROM federation_signing_keys WHERE server_name = '<SERVER_NAME>';
SQL
# 期望：DELETE 1（该服务器通常只有一把当前密钥；若历史轮换留下多把 key_id，则是
# DELETE N 且仍要求 N >= 1）。若打印 DELETE 0：占位符没有匹配到任何行 —— 回到第 2 步
# 核对，不要当成"本来就没有行"放过；那个分支只会以"第 2 步输出为空"的形式出现。

# 4) 不要在这里启动 app —— 先做步骤 C（迁移），再启动（C 末尾给命令）
```

**验证（在步骤 C 完成、app 重新启动之后执行）**（期望：恰好 1 行、`secret_key` 以 `enc:` 开头；
`'<SERVER_NAME>'` 换成与第 3 步相同的值）：

```bash
docker compose exec -T postgres sh -c 'psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -tA' <<'SQL'
SELECT key_id, left(secret_key, 4) AS secret_prefix, created_ts
FROM federation_signing_keys
WHERE server_name = '<SERVER_NAME>';
SQL
```

同时确认 app 日志里没有 `Failed to initialize key rotation` / `Failed to decrypt signing key`：

```bash
docker logs synapse-app 2>&1 | grep -E "Failed to (initialize key rotation|decrypt signing key)" || echo "no key-rotation error"
```

**处置**：若日志仍解密失败，说明步骤 B 的 `DELETE` 没生效（例如 `server_name` 写错），
重查 `SELECT DISTINCT server_name FROM federation_signing_keys;` 后重删，不要改代码绕过。

---

## C.（必做）用 migrator 应用基线，再 validate

**为什么**：本仓库约定 schema 变更直接折进基线 `migrations/00000000_unified_schema_v12.sql`，
没有时间戳增量文件。迁移器现在按**内容校验和**判断基线是否被编辑过：内容变了就以容错模式重放整份基线
（`docker/db_migrate.sh:476-507`，比较在 `:497`，重放在 `:504-505`；由 `apply_pending_migrations`
`:531-534` 调用，`migrate` 子命令在 `:687-692`）。deploy 路径的 migrator 服务把这份实现
bind mount 进容器（`docker/deploy/docker-compose.yml:155`），wrapper 只是 `exec` 它
（`docker/deploy/scripts/container-migrate.sh:66`，收敛于 `8efe7b77`）。因此
**`e2ee_audit_log.device_id` 可空 与 `ck_room_memberships_valid` 允许 `'forget'`
这两处修复会随重放自动落库，不需要、也不应该手工 ALTER**。

**推荐做法**（`./deploy.sh` 内部严格按 迁移 → 校验 → 起服务 排序，
`docker/deploy/deploy.sh:1211-1235`）：

```bash
cd docker/deploy
./deploy.sh
```

**只想补跑迁移与校验**（等价于 `deploy.sh` 的 `run_migrations`，`--no-deps` 要求
`postgres`/`redis` 已在运行——已部署的 VM 上本来就在跑）：

```bash
cd docker/deploy
docker compose run -T --rm --no-deps migrator migrate
docker compose run -T --rm --no-deps migrator validate
docker compose up -d synapse    # 若步骤 B 停过 app；./deploy.sh 已自带这一步
```

**期望**：
- `migrate` 以 `[SUCCESS]` 结束。**在已经记录过基线的库上**，输出里还会有一行
  `[WARNING] 基线内容已变化，重放基线以应用变更: 00000000_unified_schema_v12.sql`
  （该警告只在 `schema_migrations` 里存在基线行、但记录校验和与当前文件不一致时才打印，
  见 `docker/db_migrate.sh:494-506`；首次重放后不再出现）。**没有这行不代表没重放**：
  一个有业务表但从未记录过基线行的库走的是另一分支，只会打印
  `[INFO] 检测到现有业务表，使用容错模式应用基线迁移`（`docker/db_migrate.sh:509-513`）。
  判别是否真的应用了变更，以 D1/D2 的查询为准，不要以这行警告为准。
- `validate` 以 `[SUCCESS] 数据库架构验证通过` 结束（`docker/db_migrate.sh:583-633`）。

**验证重放确实记了新校验和**（两条命令输出的**哈希部分**必须相同：`md5sum` 打印
`<hash>  <路径>`，SQL 只打印裸 `<hash>`；`schema_migrations.checksum` 存的是由
`md5sum | awk '{print $1}'` 算出的文件内容哈希，见 `docker/db_migrate.sh:373-381`）：

```bash
cd docker/deploy
md5sum ../../migrations/00000000_unified_schema_v12.sql
docker compose exec -T postgres sh -c 'psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -tA' <<'SQL'
SELECT checksum FROM schema_migrations WHERE version = '00000000_unified_schema_v12';
SQL
```

**三个"不要"**（都是已被代码/事故证伪的做法）：

1. **不要**裸跑 `bash docker/db_migrate.sh validate`。这是 H-14 的复现形态：宿主 `psql` +
   隐式 `localhost:5432` 打的是**宿主自己的** PostgreSQL，脚本会拒绝执行
   （`docker/db_migrate.sh:186-189`）。用上面的 migrator 容器形式。
2. **不要**以为 `docker compose up -d` 就等于迁移完成。`synapse` 只 `depends_on` postgres/redis
   （`docker/deploy/docker-compose.yml:178-182`），migrator 是 `restart: "no"` 的一次性服务
   （`:109-130`），两者**没有先后依赖**。
3. **不要**手工 `ALTER TABLE`。那会制造第二真相源，且下次重放会把它再覆盖一遍。

---

## D.（必做）验证两处 schema 修复真的落库

**为什么 C 的 `validate` 证明不了这两件事**：`validate_schema` 只检查一份固定的表名清单存在
（`docker/db_migrate.sh:587-614`）；而且基线的重放走**容错模式**——`psql_db <"$file" >/dev/null 2>&1 || true`
（`docker/db_migrate.sh:451-457`），**任何语句报错都会被吞掉**。所以必须独立查 `pg_catalog`。

### D1. `e2ee_audit_log.device_id` 可空

**为什么**：用户级审计操作（如 `verify_all_devices`）本来就没有单一设备，写库时传 `device_id: None`；
在 `NOT NULL` 下这条插入报 23502 并冒泡成整个"验证全部设备"调用失败
（`migrations/00000000_unified_schema_v12.sql:970-977`）。

```bash
cd docker/deploy
docker compose exec -T postgres sh -c 'psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -tA' <<'SQL'
SELECT is_nullable
FROM information_schema.columns
WHERE table_schema = 'public'   -- 固定 public，不依赖 search_path/current_schema()（同 validate_schema，docker/db_migrate.sh:617）
  AND table_name = 'e2ee_audit_log'
  AND column_name = 'device_id';
SQL
```

**期望**：`YES`。若为 `NO`，说明重放没生效 → 重跑步骤 C 并检查 migrator 日志。

### D2. `ck_room_memberships_valid` 允许 `'forget'`

**为什么**：MSC4267 的 `leave + forget` 会把 `membership` 置为 `'forget'`；旧约束只允许
`invite/join/knock/leave/ban`，导致每次 forget 都 SQLSTATE 23514 失败
（`migrations/00000000_unified_schema_v12.sql:5122-5130`）。

```bash
cd docker/deploy
docker compose exec -T postgres sh -c 'psql -U "$POSTGRES_USER" -d "$POSTGRES_DB" -tA' <<'SQL'
SELECT pg_get_constraintdef(oid)
FROM pg_constraint
WHERE conname = 'ck_room_memberships_valid'
  AND conrelid = 'public.room_memberships'::regclass;   -- 固定 public，不依赖 search_path
SQL
```

**期望**：输出里含有 `'forget'`，形如
`CHECK ((membership = ANY (ARRAY['invite'::text, 'join'::text, 'knock'::text, 'leave'::text, 'ban'::text, 'forget'::text])))`。
若没有 `'forget'`，同样重跑步骤 C。

> 功能级复核（可选，做完 D1/D2 之后）：用一个普通账号对某个房间执行 leave+forget，
> 确认返回 200 而不是 500；再对含多设备的账号执行"验证全部设备"，确认不再报
> `Failed to log key operation`。这两条是上面两处修复的最终用户可见面。

---

## E.（必做）验证 8008 / 9090 只绑回环

**为什么**：`synapse` 服务现在只把这两个端口发布到 `127.0.0.1`
（`docker/deploy/docker-compose.yml:272-273`，引入于 `b8fef799`）。原先发布到所有接口等于给
nginx 开了旁路：绕过它就绕过 TLS 与代理层。9090 更严重——`prometheus.enabled: true`
（`docker/config/homeserver.yaml:241-244`），而 metrics 路由**没有任何鉴权**：
`Router::new().route(&prometheus_path, get(render_prometheus_metrics))`
（`src/server/mod.rs:809-810`，监听器绑定 `server.host` 即 `0.0.0.0`，
见 `:926-941` 与 `docker/config/homeserver.yaml:3`）。

```bash
cd docker/deploy
docker compose port synapse 8008   # 期望：127.0.0.1:8008
docker compose port synapse 9090   # 期望：127.0.0.1:9090
```

**期望**：两行都以 `127.0.0.1:` 开头。若出现 `0.0.0.0:` 或 `:::`，说明 VM 上跑的还是旧 compose：
先 `git pull` 到含 `b8fef799` 的版本，再 `docker compose up -d synapse` 重建容器。

对外访问一律走 nginx（80/443/8448，`docker/deploy/docker-compose.yml:299-301`），不要改这两行。

---

## F.（可选，保洁）删除 `.env` 里残留的 `JWT_SECRET=`

**为什么**：`JWT_SECRET` 是惰性的——没有任何 Rust 代码读取它；示例/compose/`deploy.sh`/
`generate-secrets.sh` 里的条目已由 `b8fef799` 删除。只有 VM 上那份 git-ignored 的
`docker/deploy/.env` 可能还留着一行。留着无害，只是噪音。

```bash
cd docker/deploy
grep -n '^JWT_SECRET=' .env || echo "no JWT_SECRET line"
# 有输出才需要清理：
sed -i.bak '/^JWT_SECRET=/d' .env && echo "removed"
```

**期望**：`grep` 无输出，或打印 `no JWT_SECRET line`。无需重启（本来就不生效）。

---

## G.（可选 / 需裁定）代理信任：让限流与登录锁定按真实客户端 IP 计数

**现状（不是 bug，是默认保守）**：这里有两份**互不相通**的配置，改动必须分清——只改一个
文件，只有一半行为会变：

| 消费方 | 读取的配置 | 代码 |
|--------|-----------|------|
| 限流中间件（IP 限流桶） | `RATE_LIMIT_CONFIG_PATH` 指向的 `docker/config/rate_limit.yaml`（文件存在时**整体替换** `homeserver.yaml` 的 `rate_limit:` 段，不是字段级合并） | `synapse-web/src/middleware/rate_limit.rs:18`、`:50-54` |
| 登录失败锁定 `(ip, username)` 键 | `homeserver.yaml` 的 `rate_limit.trust_forwarded` / `trusted_proxies`（运行时视图 `Config::rate_limit`） | `synapse-web/src/routes/auth_compat.rs:503-510` |

限流那份文件管理器只挂在 `AppState` 上（`synapse-web/src/routes/state.rs:131` 的
`with_rate_limit_config`），**从不回写 `Config`**，所以登录锁定永远看不到 `rate_limit.yaml`
里的 `trust_forwarded`。`trust_forwarded` 默认 `false`
（`synapse-common/src/config/rate_limit.rs:73-79`，默认值在 `:108`），而实际生效的
`docker/config/rate_limit.yaml` 与 `docker/config/homeserver.yaml` 的 `rate_limit:` 段里
都**没有** `trust_forwarded` / `trusted_proxies`（`docker/config/homeserver.yaml:32-40`）。
于是现状是：**限流与登录锁定都按 nginx 的 peer 地址计数**。

**后果**：栈前面有 nginx，peer 地址是 nginx，于是登录失败锁定键 `(ip, username)`
退化成"按用户名"，无法隔离客户端，且任何客户端都能把某个名字锁死
（`synapse-web/src/routes/auth_compat.rs:495-502` 的运营注记）。

**为什么现在没开**：`trusted_proxies` 需要写死 nginx 所在网段，而 `synapse-network`
当前没有固定 `ipam` 子网（`docker/deploy/docker-compose.yml:328-331`），
Docker 动态分配网段 → 没有稳定值可写。开启必须先固定子网并**重建网络**（短暂停服）。

**若决定开启**（子网 + **两个文件都要改**：漏掉哪个文件，就有一半仍按 nginx 的 IP 计数）：

```yaml
# docker/deploy/docker-compose.yml —— 给 synapse-network 固定子网
networks:
  synapse-network:
    driver: bridge
    name: ${COMPOSE_PROJECT_NAME:-synapse}_network
    ipam:
      config:
        - subnet: 172.28.0.0/24   # 必须与本机/宿主其它网段不冲突
```

```yaml
# (1) docker/config/rate_limit.yaml —— 管限流中间件；该文件 deny_unknown_fields，键名不能拼错
#     （synapse-common/src/rate_limit_config.rs:97-103）
trust_forwarded: true
trusted_proxies:
  - "172.28.0.0/24"   # 只信任 nginx 用到的网段；更严格的做法是给 nginx 固定
                      # ipv4_address，然后这里只写 "<nginx-ip>/32"
```

```yaml
# (2) docker/config/homeserver.yaml —— 只管登录锁定；必须与 (1) 同值，否则两者按不同 IP 计数
rate_limit:
  # ... 原有键保持不变，追加下面两个 ...
  trust_forwarded: true
  trusted_proxies:
    - "172.28.0.0/24"
```

> 不写 (2) 也可以：在 `synapse` 容器的环境里设 `SYNAPSE__RATE_LIMIT__TRUST_FORWARDED=true`
> 即可覆盖 `homeserver.yaml` 的同名键（标准环境覆盖机制，
> `synapse-common/src/config/loader.rs:19-22` 的
> `Environment::with_prefix("SYNAPSE").separator("__")` 在文件源之后追加）。
> `trusted_proxies` 是一份 CIDR 列表，写进 `homeserver.yaml` 更直观。

```bash
cd docker/deploy
docker compose down          # 重建网络会短暂停服（本步的代价）
docker compose up -d
docker compose exec -T synapse sh -c 'grep -A3 trusted_proxies /app/config/rate_limit.yaml'
# 登录锁定读的是 homeserver.yaml —— 确认 (2) 也进了容器：
docker compose exec -T synapse sh -c 'grep -A6 "^rate_limit:" /app/config/homeserver.yaml'
```

**期望**：两条 `docker compose exec` 都能看到刚写入的 `trust_forwarded` / `trusted_proxies`
且 (1) 与 (2) 同值；nginx 转发后**限流与登录锁定**都按真实客户端 IP 计数，而非 nginx
容器 IP。**裁定点**：是否接受这次短暂停服。不接受就保持默认（现状），
这条不影响其它步骤的正确性。

---

## H.（可选）跨机抓取 Prometheus 指标

**不要**把 `${PROMETHEUS_PORT}` 发布到公网：`/metrics` **无鉴权**（`src/server/mod.rs:809-810`），
容器内绑定 `0.0.0.0:9090`，唯一的边界就是步骤 E 的回环发布。

推荐零暴露方案（SSH 隧道）：

```bash
ssh -N -L 9090:127.0.0.1:9090 <user>@<vm>
# 另开一个终端：
curl -s http://127.0.0.1:9090/metrics | head
```

若抓取端与 VM 同内网、必须直连，则把 `docker/deploy/docker-compose.yml:273` 的
`127.0.0.1` 改成该内网地址（例如 `10.0.0.5:${PROMETHEUS_PORT:-9090}:9090`），
并自行在抓取端做访问控制。**永远不要**改成 `0.0.0.0` / 不写地址。

---

## 附录：本清单明确不做的事

| 不做 | 依据 |
|------|------|
| 手工 `ALTER TABLE e2ee_audit_log ...` / 重建 `ck_room_memberships_valid` | 基线漂移重放会自动带上（`43c29105` + `8efe7b77`）；手工改是第二真相源 |
| 为"兼容旧库/旧密钥"保留旧 `enc:` 行或双写 | 未发布项目无兼容义务；删掉让服务重新生成（步骤 B） |
| 在宿主裸跑 `bash docker/db_migrate.sh validate` | H-14 护栏会拒绝（`docker/db_migrate.sh:186-189`），且目标实例是错的 |
| 把 8008/9090 重新发布到所有接口 | 等于给 nginx 开旁路 + 无鉴权 metrics 上公网（`b8fef799`） |
| 为兼容保留 `JWT_SECRET` 的读取路径 | 无 Rust 代码读取，相关条目已删（`b8fef799`） |

## 附录：主张 → 证据索引

| 主张 | 证据 |
|------|------|
| 基线变更靠内容校验和漂移重放自动应用 | `docker/db_migrate.sh:373-381`（内容校验和）、`:494-506`（已记录基线但校验和不一致 → 重放）、`:509-513`（有业务表但无基线行 → 容错应用）、`:531-534`、`:687-692` |
| deploy 的 migrator 用的是同一份实现 | `docker/deploy/scripts/container-migrate.sh:66`；`docker/deploy/docker-compose.yml:149-155` |
| 重放是容错模式，错误会被吞 | `docker/db_migrate.sh:451-457` |
| `validate` 只查固定表名清单 | `docker/db_migrate.sh:583-614` |
| `device_id` 可空修复 | `migrations/00000000_unified_schema_v12.sql:956-977` |
| `'forget'` 约束修复 | `migrations/00000000_unified_schema_v12.sql:5117-5130` |
| `FEDERATION_MASTER_KEY` 的 `:?` 守卫与空值危害 | `docker/deploy/docker-compose.yml:203-209`；`synapse-common/src/key_encryption.rs:10-33`；`synapse-common/src/config/loader.rs:95-107`；`docker/config/homeserver.yaml:109`；`synapse-common/src/config/validation.rs:43-57` |
| 旧密钥行换主密钥后无法解密、不会自动重生成 | `synapse-federation/src/key_rotation.rs:490-499`、`:514-529`；`src/server/mod.rs:364` |
| 8008/9090 只绑回环 | `docker/deploy/docker-compose.yml:265-273`；`b8fef799` |
| `/metrics` 无鉴权 | `src/server/mod.rs:809-810`、`:926-941`；`docker/config/homeserver.yaml:241-244` |
| 代理信任默认关闭；限流读 `rate_limit.yaml`，登录锁定读 `homeserver.yaml`（两者互不相通） | `synapse-common/src/config/rate_limit.rs:73-79`、`:108`；`synapse-web/src/middleware/rate_limit.rs:18`、`:50-54`；`synapse-web/src/routes/auth_compat.rs:503-510`；`synapse-web/src/routes/state.rs:131`；`synapse-common/src/config/loader.rs:19-22`；`docker/config/homeserver.yaml:32-40` |
| 网络未固定子网 | `docker/deploy/docker-compose.yml:328-331` |
| `JWT_SECRET` 惰性 | `b8fef799` |
