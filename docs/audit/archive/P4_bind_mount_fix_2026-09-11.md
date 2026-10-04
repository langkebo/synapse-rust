# 单文件 bind mount 根因修复（P4 §5.6 / §8.2 #2）

> **日期**: 2026-09-11
> **基线提交**: `8ae3273a`
> **对应待办**: `docs/audit/P4_performance_baseline_2026-09-11.md` §8.2 第 2 项（中）

---

## 1. 缺陷

`docker/deploy/docker-compose.yml` 与 `docker/docker-compose.yml` 原先**逐个文件**绑定配置：

```yaml
- ./config/homeserver.yaml:/app/config/homeserver.yaml:ro
- ./config/rate_limit.yaml:/app/config/rate_limit.yaml:ro
```

单文件 bind mount 绑定的是宿主机 **inode**。对文件做原子替换
（写临时文件 + `rename` —— 编辑器、`sed -i`、Ansible/配置管理工具的常见实现）
会换掉 inode，而容器的绑定仍指向旧 inode：**容器内路径随之消失**，
但进程继续运行、继续按**旧配置**服务。

2026-09-11 实测（`docs/audit/P4_performance_baseline_2026-09-11.md` §5.6）：

```console
$ docker exec synapse-app head -2 /app/config/rate_limit.yaml
head: cannot open '/app/config/rate_limit.yaml' for reading: No such file or directory
$ docker logs synapse-app | grep 'Failed to reload'
WARN ... Failed to reload rate limit config: Failed to read config file:
     No such file or directory (os error 2)
```

宿主文件明明存在且内容正确，容器内却读不到；限流仍按旧规则生效；
且**必须 `docker compose restart`** 才能恢复。

> 上一轮的 `P4_rate_limit_observability_2026-09-11.md` 让这条降级路径**可见**
> （`degraded` + ERROR），本轮修的是**根因**。

---

## 2. 修复：挂载目录

挂载目录后，容器**按名字**解析文件，宿主机替换文件不再影响挂载。

### synapse 服务（两个 compose 文件一致）

```yaml
volumes:
  - synapse_data:/app/data
  - ./media:/app/data/media
  # ⚠️ 必须挂载**目录**，不要改回逐个文件挂载。…
  - ./config:/app/config:ro
```

**前提已验证**：镜像内 `/app/config` 本身为空（Dockerfile 把内置默认值放在
`/app/config_defaults/`，`docker/Dockerfile:96-97`），所以整目录挂载不会遮蔽任何
内置文件。`SYNAPSE_CONFIG_PATH` / `RATE_LIMIT_CONFIG_PATH` 仍指向该目录下的文件。

### postgres 服务：**刻意保持单文件挂载**

首次尝试把 `./config` 挂到 `/etc/postgresql` 导致 **postgres 崩溃重启循环**：

```console
$ docker logs synapse-postgres
postgres: could not access the server configuration file
          "/etc/postgresql/postgresql.conf": No such file or directory
```

原因：宿主机文件名是 **`postgres.conf`**，而 postgres 读的是
**`postgresql.conf`**。目录挂载后容器只看到 `postgres.conf`，名称不匹配。
已回退为单文件挂载，并**就地写下原因**（含实测报错），避免后人再试一次。

> 该单文件挂载的风险有限：postgres 只在启动时读一次配置。
> 若将来要统一，正确做法是先把宿主文件重命名为 `postgresql.conf`
> **并**同步 `deploy.sh:896` 的校验路径 —— 属独立改动，本次不做。

---

## 3. 回归证据

### 3.1 端到端（本地 dev 栈实测）

| 步骤 | 命令 | 结果 |
|---|---|---|
| 目录挂载生效 | `docker exec synapse-app ls -la /app/config/` | 三个配置文件可见，属主 `synapse` |
| 记录容器启动时间 | `docker inspect synapse-app --format '{{.State.StartedAt}}'` | `06:51:30Z` |
| **原子替换**配置 | Python `tempfile` + `os.replace()`，把 versions 规则 `10 → 999` | 写入成功 |
| **旧缺陷检查** | `docker exec synapse-app grep -A3 versions /app/config/rate_limit.yaml` | **读到 999** ✅（旧行为：No such file） |
| 热加载生效 | 等 30s watcher，再 60 次 `GET /versions` | **60×200**（999/s 规则生效）✅ |
| **无需重启** | 再次 `docker inspect ... StartedAt` | **仍是 `06:51:30Z`** ✅ |
| 恢复 | `cp` 回备份 + 等 30s + 60 次探测 | SHA256 一致；38×200 / 22×429（10/s 规则恢复）✅ |

关键对照：

```
修复前：原子替换 → 容器内 No such file → 旧配置继续生效 → 必须重启
修复后：原子替换 → 容器内读到新值   → 30s 内热加载生效 → 无需重启
```

### 3.2 新增守卫测试（5 个，`tests/unit/config_mount_tests.rs`）

```console
$ cargo nextest run --profile test --features test-utils --test unit -E 'test(/config_mount_tests/)'
    PASS config_mount_tests::app_config_is_mounted_as_a_directory
    PASS config_mount_tests::app_config_mount_is_read_only
    PASS config_mount_tests::postgres_config_stays_a_single_file_mount
    PASS config_mount_tests::directory_mount_is_documented_in_place
    PASS config_mount_tests::image_does_not_preload_files_into_the_mounted_app_config_dir
    Summary [0.015s] 5 tests run: 5 passed
```

**守卫有效性验证**（临时把 app 挂载改回单文件）：

```console
    FAIL config_mount_tests::app_config_mount_is_read_only
    FAIL config_mount_tests::app_config_is_mounted_as_a_directory
```

即测试确实能拦住回归，不是空转。已还原。

---

## 4. 门禁

```console
$ ./scripts/check_fmt_ratchet.sh
OK: fmt debt at baseline (0), no regression.

$ cargo clippy --workspace --all-targets --all-features --locked
CLIPPY_EXIT=0 errors=0 warnings=15      # 15 = 既有基线

$ cargo nextest run --profile test --features test-utils --lib --test unit
Summary [67.009s] 2488 tests run: 2488 passed (1 slow), 2 skipped
```

栈状态：`synapse-postgres` / `synapse-app` / `synapse-nginx` / `synapse-redis`
全部 healthy；`users` 表 0 行（测试期间未建任何用户）。

---

## 5. 复现方式

```bash
cd /Users/ljf/Desktop/hu_ts/synapse-rust/docker/deploy

# 1) 应用新的挂载（必须 recreate，restart 不会更新挂载）
docker compose up -d --no-deps postgres synapse
docker inspect synapse-app --format '{{range .Mounts}}{{.Source}} -> {{.Destination}}{{"\n"}}{{end}}'

# 2) 原子替换配置，验证容器仍能读到（旧缺陷会报 No such file）
cp config/rate_limit.yaml /tmp/rl.bak
python3 - <<'PY'
import os, tempfile
p='config/rate_limit.yaml'; s=open(p).read()
d=os.path.dirname(p); fd,tmp=tempfile.mkstemp(dir=d)
os.write(fd, s.replace('per_second: 10','per_second: 999',1).encode()); os.close(fd)
os.replace(tmp,p)
PY
docker exec synapse-app grep -A 3 versions /app/config/rate_limit.yaml

# 3) 等 30s 热加载，验证新规则生效且容器未重启
sleep 34
docker inspect synapse-app --format '{{.State.StartedAt}}'   # 应与步骤 1 相同
for i in $(seq 1 60); do curl -s -o /dev/null -w "%{http_code}\n" \
  http://localhost:8008/_matrix/client/versions; done | sort | uniq -c   # 60×200

# 4) 恢复
cp /tmp/rl.bak config/rate_limit.yaml

# 5) 守卫测试
cd ../.. && cargo nextest run --profile test --features test-utils --test unit \
  -E 'test(/config_mount_tests/)'
```

---

## 6. 本轮发现并记录的附带问题

| # | 现象 | 处置 |
|---|---|---|
| 1 | 宿主文件 `postgres.conf` 与容器读取路径 `postgresql.conf` **名称不一致**，使目录挂载不可用 | 保持单文件挂载并就地注明；若将来统一，需同时改 `deploy.sh:896` 的校验路径 |
| 2 | `docker compose restart` **不更新挂载**，只有 `up -d` 会 recreate | 已在脚本注释与本文档写明；这是"改了 compose 却不生效"的常见坑 |

---

## 7. P4 §8.2 进度

| # | 项 | 状态 |
|---|---|---|
| 1 | 限流降级 metric/health 信号 | ✅ `4111d9eb` |
| 2 | **单文件 bind mount 根因** | ✅ **本轮** |
| 3 | `TESTING.md` P95 阈值标定或删除 | ⬜ 待办（需决策：重标定 or 删除） |
| 4 | 同机同参数性能回归比对 | ⬜ 待办 |
| 5 | 采集 `performance_sliding_sync_benchmarks` | ⬜ 待办（低） |
| 6 | CI 上确认 `sliding-sync-perf-gate` 首跑 | ⬜ 待办 |
